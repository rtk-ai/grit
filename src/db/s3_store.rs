use anyhow::{Context, Result};
use aws_sdk_s3::Client;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::primitives::ByteStream;
use chrono::Utc;

use super::lock_store::{LockEntry, LockResult, LockStore};

/// S3-compatible lock store (works with AWS S3, Cloudflare R2, GCS, Azure Blob via S3 API, MinIO)
///
/// Each lock is an S3 object:
///   Key:  {prefix}{url_encoded_symbol_id}
///   Body: JSON LockEntry
///
/// Atomic acquisition via conditional PUT (If-None-Match: *)
pub struct S3LockStore {
    client: Client,
    bucket: String,
    prefix: String,
    _runtime: tokio::runtime::Runtime,
    rt: tokio::runtime::Handle,
}

const DEFAULT_LOCK_PREFIX: &str = ".grit/locks/";

impl S3LockStore {
    /// Build from config
    pub fn from_config(config: &S3Config) -> Result<Self> {
        let rt = tokio::runtime::Runtime::new()?;
        let client = rt.block_on(async {
            let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());

            if let Some(ref endpoint) = config.endpoint {
                loader = loader.endpoint_url(endpoint);
            }
            if let Some(ref region) = config.region {
                loader = loader.region(aws_config::Region::new(region.clone()));
            }

            let sdk_config = loader.load().await;

            // Force path-style for R2/MinIO/GCS compatibility
            // Set reasonable timeouts for CLI usage
            let timeout_config = aws_sdk_s3::config::timeout::TimeoutConfig::builder()
                .operation_timeout(std::time::Duration::from_secs(10))
                .operation_attempt_timeout(std::time::Duration::from_secs(5))
                .build();

            let retry_config = aws_sdk_s3::config::retry::RetryConfig::standard()
                .with_max_attempts(3);

            let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
                .force_path_style(true)
                .timeout_config(timeout_config)
                .retry_config(retry_config)
                .build();

            Client::from_conf(s3_config)
        });

        let handle = rt.handle().clone();
        Ok(Self {
            client,
            bucket: config.bucket.clone(),
            prefix: config.prefix.clone().unwrap_or_else(|| DEFAULT_LOCK_PREFIX.to_string()),
            _runtime: rt,
            rt: handle,
        })
    }

    fn lock_key(&self, symbol_id: &str) -> String {
        format!("{}{}", self.prefix, urlencoding::encode(symbol_id))
    }

    fn parse_entry(&self, body: &[u8]) -> Result<LockEntry> {
        serde_json::from_slice(body).context("Failed to parse lock entry JSON")
    }

    fn is_entry_expired(entry: &LockEntry) -> bool {
        if let Ok(locked_at) = chrono::DateTime::parse_from_rfc3339(&entry.locked_at) {
            let elapsed = Utc::now().signed_duration_since(locked_at);
            elapsed.num_seconds() as u64 > entry.ttl_seconds
        } else {
            // Can't parse timestamp, treat as expired
            true
        }
    }

    /// GET a lock object, returns None if not found
    fn get_lock(&self, symbol_id: &str) -> Result<Option<LockEntry>> {
        let key = self.lock_key(symbol_id);
        let result = self.rt.block_on(async {
            self.client
                .get_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
        });

        match result {
            Ok(output) => {
                let body = self.rt.block_on(async {
                    output.body.collect().await.map(|b| b.to_vec())
                })?;
                let entry = self.parse_entry(&body)?;
                Ok(Some(entry))
            }
            Err(SdkError::ServiceError(ref service_err))
                if service_err.err().is_no_such_key() =>
            {
                Ok(None)
            }
            Err(SdkError::ServiceError(ref service_err))
                if service_err.raw().status().as_u16() == 404 =>
            {
                Ok(None)
            }
            Err(err) => {
                Err(anyhow::anyhow!("S3 GET failed: {}", err))
            }
        }
    }

    /// PUT a lock object (unconditional — caller must handle atomicity)
    fn put_lock(&self, entry: &LockEntry) -> Result<()> {
        let key = self.lock_key(&entry.symbol_id);
        let body = serde_json::to_vec(entry)?;

        self.rt.block_on(async {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(body))
                .content_type("application/json")
                .send()
                .await
        }).context("S3 PUT failed")?;

        Ok(())
    }

    /// Conditional PUT — only succeeds if key does NOT exist.
    /// Returns true if created, false if already exists.
    fn put_lock_if_absent(&self, entry: &LockEntry) -> Result<bool> {
        let key = self.lock_key(&entry.symbol_id);
        let body = serde_json::to_vec(entry)?;

        let result = self.rt.block_on(async {
            self.client
                .put_object()
                .bucket(&self.bucket)
                .key(&key)
                .body(ByteStream::from(body))
                .content_type("application/json")
                .if_none_match("*")
                .send()
                .await
        });

        match result {
            Ok(_) => Ok(true),
            // 412 Precondition Failed = object already exists
            Err(SdkError::ServiceError(ref service_err))
                if service_err.raw().status().as_u16() == 412 =>
            {
                Ok(false)
            }
            Err(err) => {
                Err(anyhow::anyhow!("S3 conditional PUT failed: {}", err))
            }
        }
    }

    /// DELETE a lock object
    fn delete_lock(&self, symbol_id: &str) -> Result<()> {
        let key = self.lock_key(symbol_id);
        self.rt.block_on(async {
            self.client
                .delete_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await
        }).context("S3 DELETE failed")?;
        Ok(())
    }

    /// LIST all lock objects, fetching bodies in parallel
    fn list_all_locks(&self) -> Result<Vec<LockEntry>> {
        let mut all_keys: Vec<String> = Vec::new();
        let mut continuation_token: Option<String> = None;

        // Phase 1: collect all keys
        loop {
            let mut req = self.client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&self.prefix);

            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let output = self.rt.block_on(async { req.send().await })
                .context("S3 LIST failed")?;

            for obj in output.contents() {
                if let Some(key) = obj.key() {
                    all_keys.push(key.to_string());
                }
            }

            if output.is_truncated() == Some(true) {
                continuation_token = output.next_continuation_token().map(|s| s.to_string());
            } else {
                break;
            }
        }

        if all_keys.is_empty() {
            return Ok(Vec::new());
        }

        // Phase 2: GET all objects in parallel using tokio JoinSet
        let entries: Vec<LockEntry> = self.rt.block_on(async {
            let mut set: tokio::task::JoinSet<Option<LockEntry>> = tokio::task::JoinSet::new();
            for key in all_keys {
                let client = self.client.clone();
                let bucket = self.bucket.clone();
                set.spawn(async move {
                    let get_result = client
                        .get_object()
                        .bucket(&bucket)
                        .key(&key)
                        .send()
                        .await;
                    if let Ok(get_output) = get_result {
                        if let Ok(body) = get_output.body.collect().await.map(|b| b.to_vec()) {
                            return serde_json::from_slice::<LockEntry>(&body).ok();
                        }
                    }
                    None
                });
            }
            let mut results = Vec::new();
            while let Some(Ok(entry)) = set.join_next().await {
                if let Some(e) = entry {
                    results.push(e);
                }
            }
            results
        });

        Ok(entries)
    }
}

impl LockStore for S3LockStore {
    fn try_lock(&self, symbol_id: &str, agent_id: &str, intent: &str, ttl_seconds: u64) -> Result<LockResult> {
        let entry = LockEntry {
            symbol_id: symbol_id.to_string(),
            agent_id: agent_id.to_string(),
            intent: intent.to_string(),
            locked_at: Utc::now().to_rfc3339(),
            ttl_seconds,
        };

        // Try atomic PUT
        if self.put_lock_if_absent(&entry)? {
            return Ok(LockResult::Granted);
        }

        // Object exists — check who holds it
        if let Some(existing) = self.get_lock(symbol_id)? {
            // Same agent? Re-lock (update TTL)
            if existing.agent_id == agent_id {
                self.put_lock(&entry)?;
                return Ok(LockResult::Granted);
            }

            // Different agent — check if expired
            if Self::is_entry_expired(&existing) {
                self.delete_lock(symbol_id)?;
                // Retry atomic PUT
                if self.put_lock_if_absent(&entry)? {
                    return Ok(LockResult::Granted);
                }
                // Someone else grabbed it between our delete and put
                if let Some(new_existing) = self.get_lock(symbol_id)? {
                    return Ok(LockResult::Blocked {
                        by_agent: new_existing.agent_id,
                        by_intent: new_existing.intent,
                    });
                }
            }

            return Ok(LockResult::Blocked {
                by_agent: existing.agent_id,
                by_intent: existing.intent,
            });
        }

        // Object vanished between conditional PUT and GET — retry
        if self.put_lock_if_absent(&entry)? {
            return Ok(LockResult::Granted);
        }

        anyhow::bail!("Failed to acquire lock after retries")
    }

    fn release(&self, symbol_id: &str, agent_id: &str) -> Result<()> {
        // Verify ownership before deleting
        if let Some(entry) = self.get_lock(symbol_id)? {
            if entry.agent_id == agent_id {
                self.delete_lock(symbol_id)?;
            }
        }
        Ok(())
    }

    fn release_all(&self, agent_id: &str) -> Result<usize> {
        let all = self.list_all_locks()?;
        let mut count = 0;
        for entry in &all {
            if entry.agent_id == agent_id {
                self.delete_lock(&entry.symbol_id)?;
                count += 1;
            }
        }
        Ok(count)
    }

    fn all_locks(&self) -> Result<Vec<LockEntry>> {
        let all = self.list_all_locks()?;
        // Filter out expired
        Ok(all.into_iter().filter(|e| !Self::is_entry_expired(e)).collect())
    }

    fn locks_for_agent(&self, agent_id: &str) -> Result<Vec<(String, String)>> {
        let all = self.list_all_locks()?;
        Ok(all.into_iter()
            .filter(|e| e.agent_id == agent_id && !Self::is_entry_expired(e))
            .map(|e| (e.symbol_id, e.intent))
            .collect())
    }

    fn gc_expired_locks(&self) -> Result<usize> {
        let all = self.list_all_locks()?;
        let mut count = 0;
        for entry in &all {
            if Self::is_entry_expired(entry) {
                self.delete_lock(&entry.symbol_id)?;
                count += 1;
            }
        }
        Ok(count)
    }

    fn refresh_ttl(&self, agent_id: &str, ttl_seconds: u64) -> Result<usize> {
        let all = self.list_all_locks()?;
        let now = Utc::now().to_rfc3339();
        let mut count = 0;
        for entry in all {
            if entry.agent_id == agent_id {
                let updated = LockEntry {
                    symbol_id: entry.symbol_id,
                    agent_id: entry.agent_id,
                    intent: entry.intent,
                    locked_at: now.clone(),
                    ttl_seconds,
                };
                self.put_lock(&updated)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

/// Configuration for S3-compatible backend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct S3Config {
    pub bucket: String,
    /// Custom endpoint (for R2, GCS S3-compat, Azure S3-compat, MinIO)
    pub endpoint: Option<String>,
    /// Region (default: "auto" for R2, "us-east-1" for AWS)
    pub region: Option<String>,
    /// Key prefix (default: ".grit/locks/")
    pub prefix: Option<String>,
}

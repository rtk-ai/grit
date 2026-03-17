use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A single lock entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEntry {
    pub symbol_id: String,
    pub agent_id: String,
    pub intent: String,
    pub locked_at: String,
    pub ttl_seconds: u64,
}

/// Result of a lock attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LockResult {
    Granted,
    Blocked {
        by_agent: String,
        by_intent: String,
    },
}

/// Abstract lock storage — implementations: SQLite (local), S3-compatible (cloud)
pub trait LockStore: Send + Sync {
    fn try_lock(&self, symbol_id: &str, agent_id: &str, intent: &str, ttl_seconds: u64) -> Result<LockResult>;
    fn release(&self, symbol_id: &str, agent_id: &str) -> Result<()>;
    fn release_all(&self, agent_id: &str) -> Result<usize>;
    fn all_locks(&self) -> Result<Vec<LockEntry>>;
    fn locks_for_agent(&self, agent_id: &str) -> Result<Vec<(String, String)>>;
    fn is_lock_expired(&self, symbol_id: &str) -> Result<bool>;
    fn gc_expired_locks(&self) -> Result<usize>;
    fn refresh_ttl(&self, agent_id: &str, ttl_seconds: u64) -> Result<usize>;
}

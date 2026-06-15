use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::db::azure_store::AzureConfig;
use crate::db::s3_store::S3Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GritConfig {
    /// "local", "s3", or "azure"
    pub backend: String,
    /// S3-compatible config (for R2, MinIO, AWS S3, GCS S3-compat)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3Config>,
    /// Azure Blob Storage config (native API with Event Grid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub azure: Option<AzureConfig>,
}

impl Default for GritConfig {
    fn default() -> Self {
        Self {
            backend: "local".to_string(),
            s3: None,
            azure: None,
        }
    }
}

impl GritConfig {
    pub fn load(grit_dir: &Path) -> Result<Self> {
        let path = grit_dir.join("config.json");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            match serde_json::from_str(&content) {
                Ok(config) => Ok(config),
                Err(e) => {
                    anyhow::bail!(
                        "{} is malformed ({}); refusing to fall back to a different backend",
                        path.display(),
                        e
                    );
                }
            }
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, grit_dir: &Path) -> Result<()> {
        let path = grit_dir.join("config.json");
        let content = serde_json::to_string_pretty(self)?;

        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .with_context(|| format!("failed to open {} for writing", path.display()))?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        #[cfg(not(unix))]
        {
            std::fs::write(&path, content)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::s3_store::S3Config;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = GritConfig::default();
        assert_eq!(config.backend, "local");
        assert!(config.s3.is_none());
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let config = GritConfig {
            backend: "local".to_string(),
            s3: None,
            azure: None,
        };
        config.save(tmp.path()).unwrap();
        let loaded = GritConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.backend, "local");
        assert!(loaded.s3.is_none());
    }

    #[test]
    fn test_load_missing_file() {
        let tmp = TempDir::new().unwrap();
        // No config.json written — should return default
        let config = GritConfig::load(tmp.path()).unwrap();
        assert_eq!(config.backend, "local");
        assert!(config.s3.is_none());
    }

    #[test]
    fn test_load_malformed_json_fails_closed() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, "not valid json {{{").unwrap();
        let err = GritConfig::load(tmp.path()).unwrap_err().to_string();
        assert!(err.contains("malformed"));
        assert!(err.contains("refusing to fall back"));
    }

    #[cfg(unix)]
    #[test]
    fn test_save_uses_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let config = GritConfig::default();
        config.save(tmp.path()).unwrap();
        let mode = std::fs::metadata(tmp.path().join("config.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn test_s3_config_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let config = GritConfig {
            backend: "s3".to_string(),
            s3: Some(S3Config {
                bucket: "my-bucket".to_string(),
                prefix: Some("grit/locks/".to_string()),
                region: Some("us-east-1".to_string()),
                endpoint: Some("https://custom.endpoint.com".to_string()),
            }),
            azure: None,
        };
        config.save(tmp.path()).unwrap();
        let loaded = GritConfig::load(tmp.path()).unwrap();
        assert_eq!(loaded.backend, "s3");
        let s3 = loaded.s3.unwrap();
        assert_eq!(s3.bucket, "my-bucket");
        assert_eq!(s3.prefix.unwrap(), "grit/locks/");
        assert_eq!(s3.region.unwrap(), "us-east-1");
        assert_eq!(s3.endpoint.unwrap(), "https://custom.endpoint.com");
    }
}

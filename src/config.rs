use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::db::s3_store::S3Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GritConfig {
    /// "local" or "s3"
    pub backend: String,
    /// S3-compatible config (for R2, GCS, Azure S3, MinIO, AWS S3)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3: Option<S3Config>,
}

impl Default for GritConfig {
    fn default() -> Self {
        Self {
            backend: "local".to_string(),
            s3: None,
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
                    eprintln!(
                        "warning: {} is malformed ({}), using default config",
                        path.display(),
                        e
                    );
                    Ok(Self::default())
                }
            }
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, grit_dir: &Path) -> Result<()> {
        let path = grit_dir.join("config.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
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
    fn test_load_malformed_json() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.json");
        std::fs::write(&path, "not valid json {{{").unwrap();
        let config = GritConfig::load(tmp.path()).unwrap();
        assert_eq!(config.backend, "local");
        assert!(config.s3.is_none());
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

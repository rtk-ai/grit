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
            Ok(serde_json::from_str(&content)?)
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

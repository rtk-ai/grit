use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    pub log_level: String,
    pub max_connections: u32,
    pub request_timeout_ms: u64,
    pub cors_origins: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "sqlite://data.db".to_string(),
            jwt_secret: "change-me-in-production".to_string(),
            log_level: "info".to_string(),
            max_connections: 10,
            request_timeout_ms: 30_000,
            cors_origins: vec!["*".to_string()],
        }
    }
}

impl Config {
    /// Load configuration from a TOML file, falling back to defaults.
    pub fn load_config(path: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = path.unwrap_or("config.toml");

        let base = match std::fs::read_to_string(config_path) {
            Ok(contents) => {
                info!("Loaded configuration from {}", config_path);
                toml_parse(&contents)?
            }
            Err(e) => {
                warn!(
                    "Could not read {}: {}, using defaults",
                    config_path, e
                );
                Config::default()
            }
        };

        // Apply environment variable overrides
        let config = Config::merge_env(base);
        debug!("Final config: host={}, port={}", config.host, config.port);
        Ok(config)
    }

    /// Validate that configuration values are sensible.
    pub fn validate_config(&self) -> Result<(), String> {
        if self.port == 0 {
            return Err("Port must be greater than 0".to_string());
        }

        if self.jwt_secret.len() < 16 {
            return Err("JWT secret must be at least 16 characters".to_string());
        }

        if self.jwt_secret == "change-me-in-production" {
            warn!("Using default JWT secret — this is insecure for production!");
        }

        if self.database_url.is_empty() {
            return Err("Database URL must not be empty".to_string());
        }

        if self.max_connections == 0 || self.max_connections > 100 {
            return Err(format!(
                "max_connections must be between 1 and 100, got {}",
                self.max_connections
            ));
        }

        if self.request_timeout_ms < 1000 {
            return Err("Request timeout must be at least 1000ms".to_string());
        }

        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.log_level.as_str()) {
            return Err(format!(
                "Invalid log level '{}', must be one of: {:?}",
                self.log_level, valid_log_levels
            ));
        }

        info!("Configuration validated successfully");
        Ok(())
    }

    /// Override configuration fields from environment variables.
    pub fn merge_env(mut config: Config) -> Config {
        let env_mappings: Vec<(&str, Box<dyn Fn(&mut Config, &str)>)> = vec![
            ("SERVICE_HOST", Box::new(|c: &mut Config, v: &str| c.host = v.to_string())),
            ("SERVICE_PORT", Box::new(|c: &mut Config, v: &str| {
                if let Ok(p) = v.parse::<u16>() {
                    c.port = p;
                } else {
                    warn!("Invalid SERVICE_PORT value: {}", v);
                }
            })),
            ("DATABASE_URL", Box::new(|c: &mut Config, v: &str| c.database_url = v.to_string())),
            ("JWT_SECRET", Box::new(|c: &mut Config, v: &str| c.jwt_secret = v.to_string())),
            ("LOG_LEVEL", Box::new(|c: &mut Config, v: &str| c.log_level = v.to_lowercase())),
            ("MAX_CONNECTIONS", Box::new(|c: &mut Config, v: &str| {
                if let Ok(n) = v.parse::<u32>() {
                    c.max_connections = n;
                }
            })),
            ("CORS_ORIGINS", Box::new(|c: &mut Config, v: &str| {
                c.cors_origins = v.split(',').map(|s| s.trim().to_string()).collect();
            })),
        ];

        let mut overrides_applied = 0;
        for (env_key, apply_fn) in &env_mappings {
            if let Ok(value) = std::env::var(env_key) {
                apply_fn(&mut config, &value);
                debug!("Applied env override: {}", env_key);
                overrides_applied += 1;
            }
        }

        if overrides_applied > 0 {
            info!("Applied {} environment variable override(s)", overrides_applied);
        }

        config
    }
}

/// Minimal TOML parser for Config (simplified for testing purposes).
fn toml_parse(contents: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config::default();
    let mut kv: HashMap<String, String> = HashMap::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let k = key.trim().to_string();
            let v = value.trim().trim_matches('"').to_string();
            kv.insert(k, v);
        }
    }

    if let Some(v) = kv.get("host") { config.host = v.clone(); }
    if let Some(v) = kv.get("port") { config.port = v.parse().unwrap_or(8080); }
    if let Some(v) = kv.get("database_url") { config.database_url = v.clone(); }
    if let Some(v) = kv.get("jwt_secret") { config.jwt_secret = v.clone(); }
    if let Some(v) = kv.get("log_level") { config.log_level = v.clone(); }

    Ok(config)
}

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub role: String,
    pub exp: usize,
    pub iat: usize,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_id: String,
    pub role: String,
    pub expires_at: chrono::DateTime<Utc>,
}

const TOKEN_DURATION_HOURS: i64 = 24;
const REFRESH_DURATION_DAYS: i64 = 7;

/// Verify a JWT token and return the decoded claims.
pub fn verify_token(
    token: &str,
    secret: &str,
) -> Result<Claims, Box<dyn std::error::Error>> {
    if token.is_empty() {
        return Err("Token cannot be empty".into());
    }

    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::default();

    let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
        warn!("Token verification failed: {}", e);
        e
    })?;

    let now = Utc::now().timestamp() as usize;
    if token_data.claims.exp < now {
        return Err("Token has expired".into());
    }

    debug!("Token verified for user: {}", token_data.claims.sub);
    Ok(token_data.claims)
}

/// Create a new JWT token for a given user ID and role.
pub fn create_token(
    user_id: &str,
    role: &str,
    secret: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let expiry = now + Duration::hours(TOKEN_DURATION_HOURS);

    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        exp: expiry.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let encoding_key = EncodingKey::from_secret(secret.as_bytes());
    let token = encode(&Header::default(), &claims, &encoding_key)?;

    debug!(
        "Created token for user {} (role: {}, expires: {})",
        user_id, role, expiry
    );
    Ok(token)
}

/// Hash a password using Argon2id.
pub fn hash_password(password: &str) -> Result<String, Box<dyn std::error::Error>> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters".into());
    }

    if password.len() > 128 {
        return Err("Password must not exceed 128 characters".into());
    }

    let salt = uuid::Uuid::new_v4().to_string();
    let config = argon2::Config {
        variant: argon2::Variant::Argon2id,
        mem_cost: 65536,
        time_cost: 3,
        lanes: 4,
        ..Default::default()
    };

    let hashed = argon2::hash_encoded(password.as_bytes(), salt.as_bytes(), &config)?;
    debug!("Password hashed successfully ({} bytes)", hashed.len());
    Ok(hashed)
}

/// Check if a role has the required permission level.
pub fn check_permissions(role: &str, required: &str) -> bool {
    let role_level = match role {
        "admin" => 3,
        "editor" => 2,
        "viewer" => 1,
        _ => 0,
    };

    let required_level = match required {
        "admin" => 3,
        "write" => 2,
        "read" => 1,
        _ => 0,
    };

    let allowed = role_level >= required_level;
    if !allowed {
        warn!(
            "Permission denied: role '{}' (level {}) < required '{}' (level {})",
            role, role_level, required, required_level
        );
    }

    allowed
}

/// Refresh a session, creating a new token if the current one is about to expire.
pub fn refresh_session(
    current_token: &str,
    secret: &str,
) -> Result<(String, SessionInfo), Box<dyn std::error::Error>> {
    let claims = verify_token(current_token, secret)?;

    let now = Utc::now();
    let remaining = claims.exp as i64 - now.timestamp();

    // Only refresh if less than 25% of lifetime remains
    let threshold = (TOKEN_DURATION_HOURS * 3600) / 4;
    if remaining > threshold {
        debug!(
            "Session still valid for {}s, no refresh needed",
            remaining
        );
        let info = SessionInfo {
            user_id: claims.sub.clone(),
            role: claims.role.clone(),
            expires_at: chrono::DateTime::from_timestamp(claims.exp as i64, 0)
                .unwrap_or(now),
        };
        return Ok((current_token.to_string(), info));
    }

    let new_token = create_token(&claims.sub, &claims.role, secret)?;
    let new_expiry = now + Duration::days(REFRESH_DURATION_DAYS);

    let session = SessionInfo {
        user_id: claims.sub,
        role: claims.role,
        expires_at: new_expiry,
    };

    debug!("Session refreshed, new expiry: {}", new_expiry);
    Ok((new_token, session))
}

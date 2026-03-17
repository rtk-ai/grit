use crate::auth::{verify_token, check_permissions};
use crate::storage::{get_record, put_record, delete_record, list_records};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, warn, info};

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct CreateRecordPayload {
    key: String,
    value: Value,
    ttl_seconds: Option<u64>,
}

impl HttpResponse {
    fn json(status: u16, body: &Value) -> Self {
        let serialized = serde_json::to_vec(body).unwrap_or_default();
        Self {
            status,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Content-Length".into(), serialized.len().to_string()),
            ],
            body: serialized,
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, &json!({ "error": message }))
    }
}

pub async fn handle_request(
    stream: tokio::net::TcpStream,
    db: &sqlx::SqlitePool,
    jwt_secret: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = parse_raw_request(&stream).await?;
    let (resource, id) = parse_route(&request.path);

    debug!("{} {} -> resource={}, id={:?}", request.method, request.path, resource, id);

    // Check auth for mutating operations
    if request.method != "GET" {
        let token = request
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            .map(|(_, v)| v.trim_start_matches("Bearer ").to_string());

        match token {
            Some(t) => {
                let claims = verify_token(&t, jwt_secret)?;
                let required_perm = match request.method.as_str() {
                    "POST" => "write",
                    "DELETE" => "admin",
                    _ => "write",
                };
                if !check_permissions(&claims.role, required_perm) {
                    let resp = HttpResponse::error(403, "Insufficient permissions");
                    send_response(&stream, &resp).await?;
                    return Ok(());
                }
            }
            None => {
                let resp = HttpResponse::error(401, "Authorization required");
                send_response(&stream, &resp).await?;
                return Ok(());
            }
        }
    }

    let response = match request.method.as_str() {
        "GET" => handle_get(db, &resource, id.as_deref()).await,
        "POST" => handle_post(db, &resource, request.body.as_deref()).await,
        "DELETE" => handle_delete(db, &resource, id.as_deref()).await,
        _ => HttpResponse::error(405, "Method not allowed"),
    };

    send_response(&stream, &response).await?;
    info!("{} {} -> {}", request.method, request.path, response.status);
    Ok(())
}

pub async fn handle_get(
    db: &sqlx::SqlitePool,
    resource: &str,
    id: Option<&str>,
) -> HttpResponse {
    match id {
        Some(record_id) => {
            match get_record(db, resource, record_id).await {
                Ok(Some(record)) => HttpResponse::json(200, &record),
                Ok(None) => HttpResponse::error(404, &format!("Record '{}' not found in '{}'", record_id, resource)),
                Err(e) => {
                    warn!("GET {}/{} failed: {}", resource, record_id, e);
                    HttpResponse::error(500, "Internal server error")
                }
            }
        }
        None => {
            match list_records(db, resource, 100, 0).await {
                Ok(records) => {
                    let body = json!({
                        "data": records,
                        "count": records.len(),
                        "resource": resource
                    });
                    HttpResponse::json(200, &body)
                }
                Err(e) => {
                    warn!("GET {} (list) failed: {}", resource, e);
                    HttpResponse::error(500, "Internal server error")
                }
            }
        }
    }
}

pub async fn handle_post(
    db: &sqlx::SqlitePool,
    resource: &str,
    body: Option<&[u8]>,
) -> HttpResponse {
    let body_bytes = match body {
        Some(b) if !b.is_empty() => b,
        _ => return HttpResponse::error(400, "Request body is required"),
    };

    let payload: CreateRecordPayload = match serde_json::from_slice(body_bytes) {
        Ok(p) => p,
        Err(e) => return HttpResponse::error(400, &format!("Invalid JSON: {}", e)),
    };

    if payload.key.is_empty() || payload.key.len() > 256 {
        return HttpResponse::error(400, "Key must be between 1 and 256 characters");
    }

    match put_record(db, resource, &payload.key, &payload.value).await {
        Ok(()) => {
            info!("Created record '{}' in '{}'", payload.key, resource);
            HttpResponse::json(201, &json!({
                "created": true,
                "key": payload.key,
                "resource": resource
            }))
        }
        Err(e) => {
            warn!("POST {}/{} failed: {}", resource, payload.key, e);
            HttpResponse::error(500, "Failed to create record")
        }
    }
}

pub async fn handle_delete(
    db: &sqlx::SqlitePool,
    resource: &str,
    id: Option<&str>,
) -> HttpResponse {
    let record_id = match id {
        Some(id) => id,
        None => return HttpResponse::error(400, "Record ID is required for DELETE"),
    };

    match delete_record(db, resource, record_id).await {
        Ok(true) => {
            info!("Deleted record '{}' from '{}'", record_id, resource);
            HttpResponse::json(200, &json!({ "deleted": true, "key": record_id }))
        }
        Ok(false) => HttpResponse::error(404, &format!("Record '{}' not found", record_id)),
        Err(e) => {
            warn!("DELETE {}/{} failed: {}", resource, record_id, e);
            HttpResponse::error(500, "Failed to delete record")
        }
    }
}

pub fn parse_route(path: &str) -> (String, Option<String>) {
    let trimmed = path.trim_matches('/');
    let segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();

    match segments.len() {
        0 => ("index".to_string(), None),
        1 => (segments[0].to_string(), None),
        _ => (segments[0].to_string(), Some(segments[1..].join("/"))),
    }
}

async fn parse_raw_request(
    _stream: &tokio::net::TcpStream,
) -> Result<HttpRequest, Box<dyn std::error::Error>> {
    // Simplified: in production, actually parse HTTP from the stream
    Ok(HttpRequest {
        method: "GET".into(),
        path: "/".into(),
        headers: vec![],
        body: None,
    })
}

async fn send_response(
    _stream: &tokio::net::TcpStream,
    _response: &HttpResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    // Simplified: in production, write HTTP response to stream
    Ok(())
}

use chrono::Utc;
use serde_json::Value;
use tracing::{debug, info, warn};

/// Initialize the SQLite database and run schema migrations.
pub async fn init_db(database_url: &str) -> Result<sqlx::SqlitePool, Box<dyn std::error::Error>> {
    info!("Connecting to database: {}", database_url);

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS records (
            resource TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (resource, key)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_records_resource ON records (resource, created_at DESC)",
    )
    .execute(&pool)
    .await?;

    info!("Database schema initialized");
    Ok(pool)
}

/// Retrieve a single record by resource type and key.
pub async fn get_record(
    db: &sqlx::SqlitePool,
    resource: &str,
    key: &str,
) -> Result<Option<Value>, Box<dyn std::error::Error>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM records WHERE resource = ? AND key = ?",
    )
    .bind(resource)
    .bind(key)
    .fetch_optional(db)
    .await?;

    match row {
        Some((value_str,)) => {
            let parsed: Value = serde_json::from_str(&value_str)?;
            debug!("Retrieved record {}/{}", resource, key);
            Ok(Some(parsed))
        }
        None => {
            debug!("Record {}/{} not found", resource, key);
            Ok(None)
        }
    }
}

/// Insert or update a record.
pub async fn put_record(
    db: &sqlx::SqlitePool,
    resource: &str,
    key: &str,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let serialized = serde_json::to_string(value)?;
    let now = Utc::now().to_rfc3339();

    let existing = get_record(db, resource, key).await?;

    if existing.is_some() {
        sqlx::query("UPDATE records SET value = ?, updated_at = ? WHERE resource = ? AND key = ?")
            .bind(&serialized)
            .bind(&now)
            .bind(resource)
            .bind(key)
            .execute(db)
            .await?;
        debug!("Updated existing record {}/{}", resource, key);
    } else {
        sqlx::query(
            "INSERT INTO records (resource, key, value, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(resource)
        .bind(key)
        .bind(&serialized)
        .bind(&now)
        .bind(&now)
        .execute(db)
        .await?;
        debug!("Inserted new record {}/{}", resource, key);
    }

    Ok(())
}

/// Delete a record. Returns true if a row was actually deleted.
pub async fn delete_record(
    db: &sqlx::SqlitePool,
    resource: &str,
    key: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let result = sqlx::query("DELETE FROM records WHERE resource = ? AND key = ?")
        .bind(resource)
        .bind(key)
        .execute(db)
        .await?;

    let deleted = result.rows_affected() > 0;
    if deleted {
        info!("Deleted record {}/{}", resource, key);
    } else {
        warn!("Attempted to delete non-existent record {}/{}", resource, key);
    }

    Ok(deleted)
}

/// List records for a given resource type with pagination.
pub async fn list_records(
    db: &sqlx::SqlitePool,
    resource: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT key, value, created_at FROM records WHERE resource = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(resource)
    .bind(limit)
    .bind(offset)
    .fetch_all(db)
    .await?;

    let records: Vec<Value> = rows
        .into_iter()
        .map(|(key, value_str, created_at)| {
            let mut parsed: Value = serde_json::from_str(&value_str).unwrap_or(Value::Null);
            if let Value::Object(ref mut map) = parsed {
                map.insert("_key".into(), Value::String(key));
                map.insert("_created_at".into(), Value::String(created_at));
            }
            parsed
        })
        .collect();

    debug!("Listed {} records from resource '{}'", records.len(), resource);
    Ok(records)
}

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

use super::lock_store::{LockEntry, LockResult, LockStore};

/// SQLite-backed lock store (local coordination)
pub struct SqliteLockStore {
    conn: Mutex<Connection>,
}

impl SqliteLockStore {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

impl LockStore for SqliteLockStore {
    fn try_lock(&self, symbol_id: &str, agent_id: &str, intent: &str, ttl_seconds: u64) -> Result<LockResult> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        // Clean up expired lock on this symbol
        conn.execute(
            "DELETE FROM locks WHERE symbol_id = ?1
             AND (julianday('now') - julianday(locked_at)) * 86400 > ttl_seconds",
            params![symbol_id],
        )?;

        // Check existing lock
        let existing: Option<(String, String)> = conn.query_row(
            "SELECT agent_id, intent FROM locks WHERE symbol_id = ?1",
            params![symbol_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        match existing {
            Some((ref agent, _)) if agent == agent_id => {
                conn.execute(
                    "UPDATE locks SET intent = ?1, locked_at = datetime('now'), ttl_seconds = ?4
                     WHERE symbol_id = ?2 AND agent_id = ?3",
                    params![intent, symbol_id, agent_id, ttl_seconds],
                )?;
                Ok(LockResult::Granted)
            }
            Some((by_agent, by_intent)) => {
                Ok(LockResult::Blocked { by_agent, by_intent })
            }
            None => {
                conn.execute(
                    "INSERT INTO locks (symbol_id, agent_id, intent, ttl_seconds) VALUES (?1, ?2, ?3, ?4)",
                    params![symbol_id, agent_id, intent, ttl_seconds],
                )?;
                Ok(LockResult::Granted)
            }
        }
    }

    fn release(&self, symbol_id: &str, agent_id: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        conn.execute(
            "DELETE FROM locks WHERE symbol_id = ?1 AND agent_id = ?2",
            params![symbol_id, agent_id],
        )?;
        Ok(())
    }

    fn release_all(&self, agent_id: &str) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let count = conn.execute(
            "DELETE FROM locks WHERE agent_id = ?1",
            params![agent_id],
        )?;
        Ok(count)
    }

    fn all_locks(&self) -> Result<Vec<LockEntry>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT symbol_id, agent_id, intent, locked_at, COALESCE(ttl_seconds, 600)
             FROM locks ORDER BY agent_id, symbol_id"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(LockEntry {
                symbol_id: row.get(0)?,
                agent_id: row.get(1)?,
                intent: row.get(2)?,
                locked_at: row.get(3)?,
                ttl_seconds: row.get::<_, i64>(4)? as u64,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn locks_for_agent(&self, agent_id: &str) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT symbol_id, intent FROM locks WHERE agent_id = ?1"
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn is_lock_expired(&self, symbol_id: &str) -> Result<bool> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let expired: bool = conn.query_row(
            "SELECT (julianday('now') - julianday(locked_at)) * 86400 > COALESCE(ttl_seconds, 600)
             FROM locks WHERE symbol_id = ?1",
            params![symbol_id],
            |row| row.get(0),
        ).unwrap_or(false);
        Ok(expired)
    }

    fn gc_expired_locks(&self) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let count = conn.execute(
            "DELETE FROM locks
             WHERE (julianday('now') - julianday(locked_at)) * 86400 > COALESCE(ttl_seconds, 600)",
            [],
        )?;
        Ok(count)
    }

    fn refresh_ttl(&self, agent_id: &str, ttl_seconds: u64) -> Result<usize> {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
        let count = conn.execute(
            "UPDATE locks SET locked_at = datetime('now'), ttl_seconds = ?1 WHERE agent_id = ?2",
            params![ttl_seconds, agent_id],
        )?;
        Ok(count)
    }
}

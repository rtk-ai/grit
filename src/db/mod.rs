use anyhow::Result;
use rusqlite::{Connection, params};

use crate::parser::Symbol;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        Ok(Self { conn })
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS symbols (
                id          TEXT PRIMARY KEY,
                file        TEXT NOT NULL,
                name        TEXT NOT NULL,
                kind        TEXT NOT NULL,
                start_line  INTEGER,
                end_line    INTEGER,
                hash        TEXT
            );

            CREATE TABLE IF NOT EXISTS locks (
                symbol_id   TEXT NOT NULL REFERENCES symbols(id),
                agent_id    TEXT NOT NULL,
                intent      TEXT,
                mode        TEXT DEFAULT 'write',
                locked_at   TEXT DEFAULT (datetime('now')),
                ttl_seconds INTEGER DEFAULT 600,
                PRIMARY KEY (symbol_id)
            );

            CREATE TABLE IF NOT EXISTS deps (
                caller  TEXT NOT NULL REFERENCES symbols(id),
                callee  TEXT NOT NULL REFERENCES symbols(id),
                kind    TEXT NOT NULL,
                PRIMARY KEY (caller, callee)
            );

            CREATE INDEX IF NOT EXISTS idx_locks_agent ON locks(agent_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
            CREATE INDEX IF NOT EXISTS idx_deps_callee ON deps(callee);
            "
        )?;

        // Try to add ttl_seconds column if it doesn't exist (migration for existing DBs)
        let _ = self.conn.execute_batch(
            "ALTER TABLE locks ADD COLUMN ttl_seconds INTEGER DEFAULT 600;"
        );

        Ok(())
    }

    pub fn upsert_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO symbols (id, file, name, kind, start_line, end_line, hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    start_line = excluded.start_line,
                    end_line = excluded.end_line,
                    hash = excluded.hash"
            )?;
            for s in symbols {
                stmt.execute(params![s.id, s.file, s.name, s.kind, s.start_line, s.end_line, s.hash])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn try_lock(&self, symbol_id: &str, agent_id: &str, intent: &str, ttl_seconds: u64) -> Result<crate::room::LockResult> {
        // First, clean up expired lock on this symbol if any
        self.conn.execute(
            "DELETE FROM locks WHERE symbol_id = ?1
             AND (julianday('now') - julianday(locked_at)) * 86400 > ttl_seconds",
            params![symbol_id],
        )?;

        // Check existing lock
        let existing: Option<(String, String)> = self.conn.query_row(
            "SELECT agent_id, intent FROM locks WHERE symbol_id = ?1",
            params![symbol_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).ok();

        match existing {
            Some((ref agent, _)) if agent == agent_id => {
                // Already locked by this agent, update intent and TTL
                self.conn.execute(
                    "UPDATE locks SET intent = ?1, locked_at = datetime('now'), ttl_seconds = ?4
                     WHERE symbol_id = ?2 AND agent_id = ?3",
                    params![intent, symbol_id, agent_id, ttl_seconds],
                )?;
                Ok(crate::room::LockResult::Granted)
            }
            Some((by_agent, by_intent)) => {
                Ok(crate::room::LockResult::Blocked { by_agent, by_intent })
            }
            None => {
                self.conn.execute(
                    "INSERT INTO locks (symbol_id, agent_id, intent, ttl_seconds) VALUES (?1, ?2, ?3, ?4)",
                    params![symbol_id, agent_id, intent, ttl_seconds],
                )?;
                Ok(crate::room::LockResult::Granted)
            }
        }
    }

    pub fn release(&self, symbol_id: &str, agent_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM locks WHERE symbol_id = ?1 AND agent_id = ?2",
            params![symbol_id, agent_id],
        )?;
        Ok(())
    }

    pub fn release_all(&self, agent_id: &str) -> Result<usize> {
        let count = self.conn.execute(
            "DELETE FROM locks WHERE agent_id = ?1",
            params![agent_id],
        )?;
        Ok(count)
    }

    pub fn all_locks(&self) -> Result<Vec<(String, String, String, String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.symbol_id, l.agent_id, l.intent, l.locked_at, COALESCE(l.ttl_seconds, 600)
             FROM locks l ORDER BY l.agent_id, l.symbol_id"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get::<_, i64>(4)? as u64))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn locks_for_agent(&self, agent_id: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT symbol_id, intent FROM locks WHERE agent_id = ?1"
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn available_symbols_in_files(&self, files: &[&str]) -> Result<Vec<String>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = files.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT s.id FROM symbols s
             LEFT JOIN locks l ON s.id = l.symbol_id
             WHERE s.file IN ({}) AND l.symbol_id IS NULL
             ORDER BY s.id",
            placeholders.join(", ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = files.iter().map(|f| f as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn count_symbols(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    pub fn list_symbols(&self, file_filter: Option<&str>) -> Result<Vec<(String, String, String, String, Option<String>)>> {
        let sql = match file_filter {
            Some(_) => "SELECT s.id, s.file, s.name, s.kind, l.agent_id
                        FROM symbols s LEFT JOIN locks l ON s.id = l.symbol_id
                        WHERE s.file LIKE ?1
                        ORDER BY s.file, s.start_line",
            None =>    "SELECT s.id, s.file, s.name, s.kind, l.agent_id
                        FROM symbols s LEFT JOIN locks l ON s.id = l.symbol_id
                        ORDER BY s.file, s.start_line",
        };
        let mut stmt = self.conn.prepare(sql)?;
        let mut results: Vec<(String, String, String, String, Option<String>)> = Vec::new();
        match file_filter {
            Some(f) => {
                let pattern = format!("%{}%", f);
                let mut rows = stmt.query(params![pattern])?;
                while let Some(row) = rows.next()? {
                    results.push((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?));
                }
            }
            None => {
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    results.push((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?));
                }
            }
        };
        Ok(results)
    }

    pub fn search_symbols(&self, keywords: &[&str]) -> Result<Vec<(String, String, String, String, Option<String>)>> {
        let conditions: Vec<String> = keywords.iter().enumerate().map(|(i, _)| {
            format!("(s.name LIKE ?{0} OR s.file LIKE ?{0} OR s.id LIKE ?{0})", i + 1)
        }).collect();
        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" OR ")
        };
        let sql = format!(
            "SELECT s.id, s.file, s.name, s.kind, l.agent_id
             FROM symbols s LEFT JOIN locks l ON s.id = l.symbol_id
             WHERE {}
             ORDER BY s.file, s.start_line",
            where_clause
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<String> = keywords.iter().map(|k| format!("%{}%", k)).collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
        let mut rows = stmt.query(param_refs.as_slice())?;
        let mut results: Vec<(String, String, String, String, Option<String>)> = Vec::new();
        while let Some(row) = rows.next()? {
            results.push((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?));
        }
        Ok(results)
    }

    /// Check if a specific lock is expired
    pub fn is_lock_expired(&self, symbol_id: &str) -> Result<bool> {
        let expired: bool = self.conn.query_row(
            "SELECT (julianday('now') - julianday(locked_at)) * 86400 > COALESCE(ttl_seconds, 600)
             FROM locks WHERE symbol_id = ?1",
            params![symbol_id],
            |row| row.get(0),
        ).unwrap_or(false);
        Ok(expired)
    }

    /// Garbage-collect all expired locks, returns how many were removed
    pub fn gc_expired_locks(&self) -> Result<usize> {
        let count = self.conn.execute(
            "DELETE FROM locks
             WHERE (julianday('now') - julianday(locked_at)) * 86400 > COALESCE(ttl_seconds, 600)",
            [],
        )?;
        Ok(count)
    }

    /// Refresh the TTL for all locks held by an agent
    pub fn refresh_ttl(&self, agent_id: &str, ttl_seconds: u64) -> Result<usize> {
        let count = self.conn.execute(
            "UPDATE locks SET locked_at = datetime('now'), ttl_seconds = ?1 WHERE agent_id = ?2",
            params![ttl_seconds, agent_id],
        )?;
        Ok(count)
    }
}

pub mod lock_store;
pub mod sqlite_store;
pub mod s3_store;

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

            CREATE TABLE IF NOT EXISTS sessions (
                name        TEXT PRIMARY KEY,
                branch      TEXT NOT NULL,
                base_branch TEXT NOT NULL,
                created_at  TEXT DEFAULT (datetime('now')),
                status      TEXT DEFAULT 'active'
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

    // ── Session management ──

    pub fn create_session(&self, name: &str, branch: &str, base_branch: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions (name, branch, base_branch) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET status = 'active'",
            params![name, branch, base_branch],
        )?;
        Ok(())
    }

    pub fn get_active_session(&self) -> Result<Option<(String, String, String)>> {
        let result = self.conn.query_row(
            "SELECT name, branch, base_branch FROM sessions WHERE status = 'active' ORDER BY created_at DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );
        match result {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn close_session(&self, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET status = 'closed' WHERE name = ?1",
            params![name],
        )?;
        Ok(())
    }

}

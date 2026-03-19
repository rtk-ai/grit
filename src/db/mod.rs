pub mod lock_store;
pub mod sqlite_store;
pub mod s3_store;

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::parser::Symbol;

/// Apply standard PRAGMA settings to a new SQLite connection.
pub fn configure_connection(conn: &Connection) -> Result<()> {
    match conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;") {
        Ok(_) => Ok(()),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("locked") || err_str.contains("busy") {
                anyhow::bail!(
                    "Database is locked by another process. \
                     If this persists, check for stale grit processes or remove the WAL files."
                );
            }
            anyhow::bail!("Database configuration failed: {}", e);
        }
    }
}

/// (id, file, name, kind, locked_by_agent)
pub type SymbolRow = (String, String, String, String, Option<String>);

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to open database {}: {}.\n\
                 If the file is corrupted, remove it and re-run `grit init`.",
                path.display(),
                e
            )
        })?;
        configure_connection(&conn)?;

        // Quick integrity check
        match conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0)) {
            Ok(ref result) if result == "ok" => {}
            Ok(detail) => {
                anyhow::bail!(
                    "Database {} failed integrity check: {}.\n\
                     Remove it and re-run `grit init` to rebuild.",
                    path.display(),
                    detail
                );
            }
            Err(e) => {
                anyhow::bail!(
                    "Database {} may be corrupted: {}.\n\
                     Remove it and re-run `grit init` to rebuild.",
                    path.display(),
                    e
                );
            }
        }

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

    pub fn list_symbols(&self, file_filter: Option<&str>) -> Result<Vec<SymbolRow>> {
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
        let mut results: Vec<SymbolRow> = Vec::new();
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

    pub fn search_symbols(&self, keywords: &[&str]) -> Result<Vec<SymbolRow>> {
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
        let mut results: Vec<SymbolRow> = Vec::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Symbol;
    use tempfile::TempDir;

    fn make_symbol(id: &str, file: &str, name: &str, kind: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            file: file.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            start_line: 1,
            end_line: 10,
            hash: "abc123".to_string(),
        }
    }

    fn setup_db() -> (TempDir, Database) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.init_schema().unwrap();
        (tmp, db)
    }

    #[test]
    fn test_open_and_init_schema() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        assert!(db.init_schema().is_ok());
    }

    #[test]
    fn test_upsert_and_count_symbols() {
        let (_tmp, db) = setup_db();
        let symbols: Vec<Symbol> = (0..5)
            .map(|i| make_symbol(&format!("file.rs::fn{}", i), "file.rs", &format!("fn{}", i), "function"))
            .collect();
        db.upsert_symbols(&symbols).unwrap();
        assert_eq!(db.count_symbols().unwrap(), 5);
    }

    #[test]
    fn test_upsert_updates_existing() {
        let (_tmp, db) = setup_db();
        let sym = make_symbol("file.rs::foo", "file.rs", "foo", "function");
        db.upsert_symbols(&[sym]).unwrap();
        assert_eq!(db.count_symbols().unwrap(), 1);

        // Update same symbol with different hash
        let updated = Symbol {
            id: "file.rs::foo".to_string(),
            file: "file.rs".to_string(),
            name: "foo".to_string(),
            kind: "function".to_string(),
            start_line: 5,
            end_line: 20,
            hash: "new_hash".to_string(),
        };
        db.upsert_symbols(&[updated]).unwrap();
        assert_eq!(db.count_symbols().unwrap(), 1);
    }

    #[test]
    fn test_list_symbols_no_filter() {
        let (_tmp, db) = setup_db();
        let symbols = vec![
            make_symbol("a.rs::fn1", "a.rs", "fn1", "function"),
            make_symbol("a.rs::fn2", "a.rs", "fn2", "function"),
            make_symbol("b.rs::fn3", "b.rs", "fn3", "function"),
        ];
        db.upsert_symbols(&symbols).unwrap();
        let all = db.list_symbols(None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_list_symbols_with_filter() {
        let (_tmp, db) = setup_db();
        let symbols = vec![
            make_symbol("src/a.rs::fn1", "src/a.rs", "fn1", "function"),
            make_symbol("src/a.rs::fn2", "src/a.rs", "fn2", "function"),
            make_symbol("src/b.rs::fn3", "src/b.rs", "fn3", "function"),
        ];
        db.upsert_symbols(&symbols).unwrap();
        let filtered = db.list_symbols(Some("a.rs")).unwrap();
        assert_eq!(filtered.len(), 2);
        for row in &filtered {
            assert!(row.1.contains("a.rs"));
        }
    }

    #[test]
    fn test_search_symbols() {
        let (_tmp, db) = setup_db();
        let symbols = vec![
            make_symbol("src/auth.rs::login", "src/auth.rs", "login", "function"),
            make_symbol("src/auth.rs::logout", "src/auth.rs", "logout", "function"),
            make_symbol("src/db.rs::connect", "src/db.rs", "connect", "function"),
        ];
        db.upsert_symbols(&symbols).unwrap();
        let results = db.search_symbols(&["login"]).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].2, "login");
    }

    #[test]
    fn test_available_symbols_in_files() {
        let (_tmp, db) = setup_db();
        let symbols = vec![
            make_symbol("f.rs::a", "f.rs", "a", "function"),
            make_symbol("f.rs::b", "f.rs", "b", "function"),
            make_symbol("f.rs::c", "f.rs", "c", "function"),
        ];
        db.upsert_symbols(&symbols).unwrap();

        // Lock symbol "f.rs::b"
        db.conn.execute(
            "INSERT INTO locks (symbol_id, agent_id, intent) VALUES (?1, ?2, ?3)",
            params!["f.rs::b", "agent-1", "editing"],
        ).unwrap();

        let available = db.available_symbols_in_files(&["f.rs"]).unwrap();
        assert_eq!(available.len(), 2);
        assert!(available.contains(&"f.rs::a".to_string()));
        assert!(available.contains(&"f.rs::c".to_string()));
        assert!(!available.contains(&"f.rs::b".to_string()));
    }

    #[test]
    fn test_session_lifecycle() {
        let (_tmp, db) = setup_db();
        db.create_session("sess1", "feature/x", "main").unwrap();

        let active = db.get_active_session().unwrap();
        assert!(active.is_some());
        let (name, branch, base) = active.unwrap();
        assert_eq!(name, "sess1");
        assert_eq!(branch, "feature/x");
        assert_eq!(base, "main");

        db.close_session("sess1").unwrap();
        let active = db.get_active_session().unwrap();
        assert!(active.is_none());
    }

    #[test]
    fn test_no_active_session() {
        let (_tmp, db) = setup_db();
        let active = db.get_active_session().unwrap();
        assert!(active.is_none());
    }

    #[test]
    fn test_integrity_check_on_open() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        // First open creates the file
        {
            let db = Database::open(&db_path).unwrap();
            db.init_schema().unwrap();
        }
        // Second open runs integrity check on existing DB
        let result = Database::open(&db_path);
        assert!(result.is_ok());
    }
}

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
        crate::db::configure_connection(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Acquire the connection mutex, converting poison errors.
    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))
    }
}

impl LockStore for SqliteLockStore {
    fn try_lock(&self, symbol_id: &str, agent_id: &str, intent: &str, ttl_seconds: u64) -> Result<LockResult> {
        let conn = self.conn()?;

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
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM locks WHERE symbol_id = ?1 AND agent_id = ?2",
            params![symbol_id, agent_id],
        )?;
        Ok(())
    }

    fn release_all(&self, agent_id: &str) -> Result<usize> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM locks WHERE agent_id = ?1",
            params![agent_id],
        )?;
        Ok(count)
    }

    fn all_locks(&self) -> Result<Vec<LockEntry>> {
        let conn = self.conn()?;
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
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT symbol_id, intent FROM locks WHERE agent_id = ?1"
        )?;
        let rows = stmt.query_map(params![agent_id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn gc_expired_locks(&self) -> Result<usize> {
        let conn = self.conn()?;
        let count = conn.execute(
            "DELETE FROM locks
             WHERE (julianday('now') - julianday(locked_at)) * 86400 > COALESCE(ttl_seconds, 600)",
            [],
        )?;
        Ok(count)
    }

    fn refresh_ttl(&self, agent_id: &str, ttl_seconds: u64) -> Result<usize> {
        let conn = self.conn()?;
        let count = conn.execute(
            "UPDATE locks SET locked_at = datetime('now'), ttl_seconds = ?1 WHERE agent_id = ?2",
            params![ttl_seconds, agent_id],
        )?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::lock_store::{LockResult, LockStore};
    use std::sync::Arc;

    /// Create a temporary SQLite database with the locks table and return the store.
    fn setup() -> (tempfile::TempDir, SqliteLockStore) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("test.db");

        // Create schema directly — avoids needing the full Database struct and symbols table FK
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA busy_timeout=5000;
                 CREATE TABLE IF NOT EXISTS locks (
                     symbol_id   TEXT NOT NULL,
                     agent_id    TEXT NOT NULL,
                     intent      TEXT,
                     mode        TEXT DEFAULT 'write',
                     locked_at   TEXT DEFAULT (datetime('now')),
                     ttl_seconds INTEGER DEFAULT 600,
                     PRIMARY KEY (symbol_id)
                 );
                 CREATE INDEX IF NOT EXISTS idx_locks_agent ON locks(agent_id);",
            )
            .unwrap();
        }

        let store = SqliteLockStore::open(&db_path).expect("failed to open store");
        (dir, store)
    }

    #[test]
    fn test_lock_and_release() {
        let (_dir, store) = setup();

        // Lock a symbol
        let result = store.try_lock("sym::foo", "agent-1", "editing foo", 600).unwrap();
        assert!(matches!(result, LockResult::Granted));

        // Verify it appears in all_locks
        let locks = store.all_locks().unwrap();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].symbol_id, "sym::foo");
        assert_eq!(locks[0].agent_id, "agent-1");

        // Release it
        store.release("sym::foo", "agent-1").unwrap();

        // Verify it's gone
        let locks = store.all_locks().unwrap();
        assert!(locks.is_empty());
    }

    #[test]
    fn test_lock_blocked_by_other_agent() {
        let (_dir, store) = setup();

        // Agent A locks
        let result = store.try_lock("sym::bar", "agent-A", "refactoring", 600).unwrap();
        assert!(matches!(result, LockResult::Granted));

        // Agent B tries the same symbol
        let result = store.try_lock("sym::bar", "agent-B", "also refactoring", 600).unwrap();
        match result {
            LockResult::Blocked { by_agent, by_intent } => {
                assert_eq!(by_agent, "agent-A");
                assert_eq!(by_intent, "refactoring");
            }
            LockResult::Granted => panic!("expected Blocked, got Granted"),
        }
    }

    #[test]
    fn test_same_agent_relock() {
        let (_dir, store) = setup();

        // Agent A locks
        let result = store.try_lock("sym::baz", "agent-A", "first pass", 300).unwrap();
        assert!(matches!(result, LockResult::Granted));

        // Agent A locks again (should refresh TTL, still Granted)
        let result = store.try_lock("sym::baz", "agent-A", "second pass", 900).unwrap();
        assert!(matches!(result, LockResult::Granted));

        // Verify only one lock exists and TTL was updated
        let locks = store.all_locks().unwrap();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].ttl_seconds, 900);
        assert_eq!(locks[0].intent, "second pass");
    }

    #[test]
    fn test_release_all() {
        let (_dir, store) = setup();

        // Agent locks 3 symbols
        store.try_lock("sym::a", "agent-1", "intent-a", 600).unwrap();
        store.try_lock("sym::b", "agent-1", "intent-b", 600).unwrap();
        store.try_lock("sym::c", "agent-1", "intent-c", 600).unwrap();

        // Also one lock by another agent (should not be released)
        store.try_lock("sym::d", "agent-2", "intent-d", 600).unwrap();

        let count = store.release_all("agent-1").unwrap();
        assert_eq!(count, 3);

        let locks = store.all_locks().unwrap();
        assert_eq!(locks.len(), 1);
        assert_eq!(locks[0].agent_id, "agent-2");
    }

    #[test]
    fn test_all_locks() {
        let (_dir, store) = setup();

        store.try_lock("sym::x", "agent-A", "ix", 600).unwrap();
        store.try_lock("sym::y", "agent-A", "iy", 600).unwrap();
        store.try_lock("sym::z", "agent-B", "iz", 600).unwrap();

        let locks = store.all_locks().unwrap();
        assert_eq!(locks.len(), 3);

        // Verify ordering is by agent_id then symbol_id
        let ids: Vec<(&str, &str)> = locks.iter().map(|l| (l.agent_id.as_str(), l.symbol_id.as_str())).collect();
        assert_eq!(ids, vec![("agent-A", "sym::x"), ("agent-A", "sym::y"), ("agent-B", "sym::z")]);
    }

    #[test]
    fn test_locks_for_agent() {
        let (_dir, store) = setup();

        store.try_lock("sym::p", "agent-1", "ip", 600).unwrap();
        store.try_lock("sym::q", "agent-1", "iq", 600).unwrap();
        store.try_lock("sym::r", "agent-2", "ir", 600).unwrap();

        let agent1_locks = store.locks_for_agent("agent-1").unwrap();
        assert_eq!(agent1_locks.len(), 2);
        let symbols: Vec<&str> = agent1_locks.iter().map(|(s, _)| s.as_str()).collect();
        assert!(symbols.contains(&"sym::p"));
        assert!(symbols.contains(&"sym::q"));

        let agent2_locks = store.locks_for_agent("agent-2").unwrap();
        assert_eq!(agent2_locks.len(), 1);
        assert_eq!(agent2_locks[0].0, "sym::r");
    }

    #[test]
    fn test_gc_expired_locks() {
        let (_dir, store) = setup();

        // Lock with TTL=1 second
        store.try_lock("sym::expire", "agent-1", "short-lived", 1).unwrap();

        // Verify it exists
        assert_eq!(store.all_locks().unwrap().len(), 1);

        // Sleep to let it expire
        std::thread::sleep(std::time::Duration::from_secs(2));

        // GC should clean it up
        let cleaned = store.gc_expired_locks().unwrap();
        assert_eq!(cleaned, 1);
        assert!(store.all_locks().unwrap().is_empty());
    }

    #[test]
    fn test_refresh_ttl() {
        let (_dir, store) = setup();

        store.try_lock("sym::m", "agent-1", "im", 300).unwrap();
        store.try_lock("sym::n", "agent-1", "in", 300).unwrap();

        let count = store.refresh_ttl("agent-1", 900).unwrap();
        assert_eq!(count, 2);

        // Verify the TTL was updated
        let locks = store.all_locks().unwrap();
        for lock in &locks {
            assert_eq!(lock.ttl_seconds, 900);
        }
    }

    #[test]
    fn test_concurrent_access() {
        let (_dir, store) = setup();
        let store = Arc::new(store);
        let mut handles = Vec::new();

        for i in 0..10 {
            let store = Arc::clone(&store);
            let handle = std::thread::spawn(move || {
                let agent = format!("agent-{}", i);
                store.try_lock("sym::contested", &agent, "racing", 600).unwrap()
            });
            handles.push(handle);
        }

        let results: Vec<LockResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let granted = results.iter().filter(|r| matches!(r, LockResult::Granted)).count();
        let blocked = results.iter().filter(|r| matches!(r, LockResult::Blocked { .. })).count();

        // Exactly one thread should win the lock
        assert_eq!(granted, 1, "expected exactly 1 Granted, got {}", granted);
        assert_eq!(blocked, 9, "expected exactly 9 Blocked, got {}", blocked);
    }
}

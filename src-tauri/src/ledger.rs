//! Remembers every file ever imported (across all projects), so re-inserting a
//! card only picks up the new shots. Keyed by name + size + capture time, which
//! stays stable across card readers and drive letters. Rejects you later sent to
//! the Trash stay remembered (as trashed), so they aren't offered again.

use crate::schema::Step;
use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Ledger {
    db: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct Seen {
    pub project_root: String,
    pub rel_path: String,
    /// You rejected it and sent it to the Trash from Safelight.
    pub trashed: bool,
}

/// Append only: never edit or remove a step.
const MIGRATIONS: &[Step] = &[Step { sql: "ALTER TABLE seen ADD COLUMN trashed INTEGER NOT NULL DEFAULT 0", unless_column: None }];

pub fn key(file_name: &str, size: u64, captured: Option<&str>) -> String {
    format!("{}|{}|{}", file_name.to_lowercase(), size, captured.unwrap_or("-"))
}

impl Ledger {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS seen (
                key TEXT PRIMARY KEY,
                project_root TEXT NOT NULL,
                rel_path TEXT NOT NULL,
                hash TEXT NOT NULL,
                imported_at TEXT NOT NULL
             );",
        )?;
        crate::schema::migrate(&conn, "The list of imported files", MIGRATIONS)?;
        Ok(Self { db: Mutex::new(conn) })
    }

    pub fn get(&self, key: &str) -> Result<Option<Seen>> {
        Ok(self
            .db
            .lock()
            .query_row("SELECT project_root, rel_path, trashed FROM seen WHERE key = ?1", [key], |r| {
                Ok(Seen { project_root: r.get(0)?, rel_path: r.get(1)?, trashed: r.get(2)? })
            })
            .optional()?)
    }

    pub fn record(&self, key: &str, project_root: &str, rel_path: &str, hash: &str) -> Result<()> {
        self.db.lock().execute(
            "INSERT OR REPLACE INTO seen (key, project_root, rel_path, hash, imported_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![key, project_root, rel_path, hash, chrono::Local::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Marks files of `project_root` as sent to the Trash.
    pub fn mark_trashed(&self, project_root: &str, rel_paths: &[String]) -> Result<()> {
        let mut db = self.db.lock();
        let tx = db.transaction()?;
        for rel in rel_paths {
            tx.execute("UPDATE seen SET trashed = 1 WHERE project_root = ?1 AND rel_path = ?2", params![project_root, rel])?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let l = Ledger::open(&dir.path().join("l.sqlite")).unwrap();
        let k = key("IMG_0001.CR3", 100, Some("2026-01-01T10:00:00.000"));
        assert!(l.get(&k).unwrap().is_none());
        l.record(&k, "P", "a/b.cr3", "h").unwrap();
        assert_eq!(l.get(&k).unwrap().unwrap().rel_path, "a/b.cr3");
        // Same name/size but different time is a different shot (counter rollover).
        assert!(l.get(&key("img_0001.cr3", 100, Some("2026-02-01T10:00:00.000"))).unwrap().is_none());
    }

    #[test]
    fn trashed_until_imported_again() {
        let dir = tempfile::tempdir().unwrap();
        let l = Ledger::open(&dir.path().join("l.sqlite")).unwrap();
        let k = key("IMG_0002.CR3", 100, None);
        l.record(&k, "P", "a/IMG_0002.CR3", "h").unwrap();
        l.mark_trashed("Other", &["a/IMG_0002.CR3".into()]).unwrap();
        assert!(!l.get(&k).unwrap().unwrap().trashed, "only that project's file");
        l.mark_trashed("P", &["a/IMG_0002.CR3".into()]).unwrap();
        assert!(l.get(&k).unwrap().unwrap().trashed);
        l.record(&k, "P", "a/IMG_0002.CR3", "h").unwrap();
        assert!(!l.get(&k).unwrap().unwrap().trashed, "importing it again on purpose clears the mark");
    }
}

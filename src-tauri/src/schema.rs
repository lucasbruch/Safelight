//! Numbered schema changes, tracked in SQLite's `user_version`, so each runs
//! exactly once and a real failure is reported instead of being mistaken for
//! "already applied".

use anyhow::{Context, Result};
use rusqlite::Connection;

pub struct Step {
    pub sql: &'static str,
    /// For changes that older versions applied without recording a version:
    /// skip the step when this `(table, column)` already exists.
    pub unless_column: Option<(&'static str, &'static str)>,
}

pub fn migrate(conn: &Connection, what: &str, steps: &[Step]) -> Result<()> {
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    anyhow::ensure!(
        version as usize <= steps.len(),
        "{what} was saved by a newer version of Safelight. Please update Safelight to open it."
    );
    for (i, step) in steps.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        let done = match step.unless_column {
            Some((table, column)) => has_column(&tx, table, column)?,
            None => false,
        };
        if !done {
            tx.execute_batch(step.sql).with_context(|| format!("updating {what} (step {})", i + 1))?;
        }
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    Ok(conn
        .prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?
        .exists([table, column])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEPS: &[Step] = &[
        Step { sql: "ALTER TABLE t ADD COLUMN a TEXT", unless_column: Some(("t", "a")) },
        Step { sql: "ALTER TABLE t ADD COLUMN b TEXT", unless_column: None },
    ];

    fn version(c: &Connection) -> i64 {
        c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap()
    }

    #[test]
    fn runs_each_step_once() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE t (id INTEGER)").unwrap();
        migrate(&c, "test", STEPS).unwrap();
        migrate(&c, "test", STEPS).unwrap();
        assert_eq!(version(&c), 2);
        assert!(has_column(&c, "t", "a").unwrap() && has_column(&c, "t", "b").unwrap());
    }

    #[test]
    fn adopts_columns_added_before_versioning() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE t (id INTEGER, a TEXT)").unwrap();
        migrate(&c, "test", STEPS).unwrap();
        assert_eq!(version(&c), 2);
    }

    #[test]
    fn reports_failures_and_newer_files() {
        let c = Connection::open_in_memory().unwrap();
        assert!(migrate(&c, "test", STEPS).is_err(), "no table t: the error surfaces");
        assert_eq!(version(&c), 0);
        c.pragma_update(None, "user_version", 9).unwrap();
        let e = migrate(&c, "test", STEPS).unwrap_err().to_string();
        assert!(e.contains("newer version"), "{e}");
    }
}

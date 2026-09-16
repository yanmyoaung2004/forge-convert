//! SQLite adapter: history + settings (spec §25, ADR 008).
//!
//! Metadata only — files stay files. Schema v1:
//! `history(id, job_id, operation, input_name, output_name, output_format,
//! status, timestamp_ms, duration_ms, options_json)` and
//! `settings(key, value)` (upsert). All writes use a single `Connection`
//! behind a `Mutex` (rusqlite is synchronous; callers wrap in
//! `spawn_blocking`).

use std::path::Path;
use std::sync::Mutex;

use forge_core::{ForgeError, HistoryEntry, HistoryStore, JobStatus, Result};
use rusqlite::{Connection, OptionalExtension};

/// SQLite-backed [`HistoryStore`]. Open with [`HistoryDb::open`] (file)
/// or [`HistoryDb::in_memory`] (tests).
#[derive(Debug)]
pub struct HistoryDb {
    conn: Mutex<Connection>,
}

impl HistoryDb {
    /// Open (creating parent dirs + schema) at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ForgeError::PermissionDenied(format!("{}: {e}", parent.display()))
                })?;
            }
        }
        let conn = Connection::open(path)
            .map_err(|e| ForgeError::InvalidFile(format!("sqlite open: {e}")))?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// In-memory DB for tests (same schema, no files).
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()
            .map_err(|e| ForgeError::InvalidFile(format!("sqlite memory: {e}")))?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    /// Idempotent schema creation (schema v1).
    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("history db lock");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id        TEXT NOT NULL,
                operation     TEXT NOT NULL,
                input_name    TEXT NOT NULL,
                output_name   TEXT NOT NULL,
                output_format TEXT NOT NULL,
                status        TEXT NOT NULL,
                timestamp_ms  INTEGER NOT NULL,
                duration_ms   INTEGER NOT NULL,
                options_json  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_timestamp ON history(timestamp_ms);
            CREATE TABLE IF NOT EXISTS settings (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| ForgeError::InvalidFile(format!("sqlite migrate: {e}")))?;
        Ok(())
    }

    /// Store a setting (`INSERT OR REPLACE`).
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().expect("history db lock");
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, value],
        )
        .map_err(|e| ForgeError::InvalidFile(format!("sqlite setting: {e}")))?;
        Ok(())
    }

    /// Fetch a setting (None when unset).
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("history db lock");
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| ForgeError::InvalidFile(format!("sqlite setting: {e}")))
    }

    /// Row count (tests + diagnostics).
    pub fn len(&self) -> Result<usize> {
        let conn = self.conn.lock().expect("history db lock");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))
            .map_err(|e| ForgeError::InvalidFile(format!("sqlite count: {e}")))?;
        Ok(count.max(0) as usize)
    }

    /// True when no history rows exist.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}

impl HistoryStore for HistoryDb {
    fn record(&self, entry: HistoryEntry) -> Result<()> {
        let conn = self.conn.lock().expect("history db lock");
        conn.execute(
            "INSERT INTO history
             (job_id, operation, input_name, output_name, output_format,
              status, timestamp_ms, duration_ms, options_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.job_id.0,
                entry.operation,
                entry.input_name,
                entry.output_name,
                entry.output_format.extension(),
                status_name(entry.status),
                now_ms(),
                entry.duration_ms as i64,
                entry.options_json,
            ],
        )
        .map_err(|e| ForgeError::InvalidFile(format!("sqlite record: {e}")))?;
        Ok(())
    }

    fn recent(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock().expect("history db lock");
        let mut stmt = conn
            .prepare(
                "SELECT job_id, operation, input_name, output_name, output_format,
                        status, timestamp_ms, duration_ms, options_json
                 FROM history ORDER BY timestamp_ms DESC, id DESC LIMIT ?1",
            )
            .map_err(|e| ForgeError::InvalidFile(format!("sqlite recent: {e}")))?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                ))
            })
            .map_err(|e| ForgeError::InvalidFile(format!("sqlite recent: {e}")))?;
        let mut entries = Vec::new();
        for row in rows {
            let (
                job_id,
                operation,
                input_name,
                output_name,
                format,
                status,
                duration_ms,
                options_json,
            ) = row.map_err(|e| ForgeError::InvalidFile(format!("sqlite row: {e}")))?;
            entries.push(HistoryEntry {
                job_id: forge_core::JobId(job_id),
                operation,
                input_name,
                output_name,
                output_format: forge_core::ImageFormat::from_extension(&format)
                    .unwrap_or(forge_core::ImageFormat::Png),
                status: parse_status(&status),
                duration_ms: duration_ms.max(0) as u64,
                options_json,
            });
        }
        Ok(entries)
    }
}

/// Milliseconds since epoch (history ordering).
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Stable status spelling for the DB.
fn status_name(status: JobStatus) -> &'static str {
    status.name()
}

/// Parse back what [`status_name`] wrote (unknown → Failed, never panic).
fn parse_status(raw: &str) -> JobStatus {
    match raw {
        "Queued" => JobStatus::Queued,
        "Running" => JobStatus::Running,
        "Completed" => JobStatus::Completed,
        "Cancelled" => JobStatus::Cancelled,
        _ => JobStatus::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::ImageFormat;

    fn entry(name: &str) -> HistoryEntry {
        HistoryEntry {
            job_id: forge_core::JobId(format!("job-{name}")),
            operation: "convert".to_string(),
            input_name: format!("{name}.png"),
            output_name: format!("{name}.webp"),
            output_format: ImageFormat::Webp,
            status: JobStatus::Completed,
            duration_ms: 42,
            options_json: "{\"quality\":80}".to_string(),
        }
    }

    #[test]
    fn test_record_and_recent_roundtrip() {
        let db = HistoryDb::in_memory().unwrap();
        assert!(db.is_empty().unwrap());
        db.record(entry("a")).unwrap();
        db.record(entry("b")).unwrap();
        assert_eq!(db.len().unwrap(), 2);
        let recent = db.recent(10).unwrap();
        assert_eq!(recent.len(), 2);
        // Newest first.
        assert_eq!(recent[0].input_name, "b.png");
        assert_eq!(recent[1].input_name, "a.png");
        assert_eq!(recent[0].output_format, ImageFormat::Webp);
    }

    #[test]
    fn test_recent_limit_respected() {
        let db = HistoryDb::in_memory().unwrap();
        for name in ["a", "b", "c"] {
            db.record(entry(name)).unwrap();
        }
        assert_eq!(db.recent(2).unwrap().len(), 2);
    }

    #[test]
    fn test_settings_upsert_and_missing() {
        let db = HistoryDb::in_memory().unwrap();
        assert_eq!(db.get_setting("theme").unwrap(), None);
        db.set_setting("theme", "dark").unwrap();
        assert_eq!(db.get_setting("theme").unwrap(), Some("dark".to_string()));
        db.set_setting("theme", "light").unwrap();
        assert_eq!(db.get_setting("theme").unwrap(), Some("light".to_string()));
    }
}

//! SQLite store for one court box. WAL + synchronous=FULL so a power cut never loses a saved clip, setting or
//! account (FR-56). Times are stored as RFC 3339 text in Manila time.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

// ponytail: one connection behind a mutex; a pool when contention shows up in the load test.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
        migrate(&conn)?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE venue (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  name TEXT NOT NULL,
  logo BLOB, logo_type TEXT, logo_corner TEXT NOT NULL DEFAULT 'top-right',
  sponsor BLOB, sponsor_type TEXT,
  open_time TEXT NOT NULL, close_time TEXT NOT NULL,
  clip_before_s INTEGER NOT NULL DEFAULT 25, clip_after_s INTEGER NOT NULL DEFAULT 5,
  upload_cap_kbps INTEGER NOT NULL DEFAULT 20000,
  judge_on INTEGER NOT NULL DEFAULT 1,
  privacy_notice TEXT NOT NULL DEFAULT '',
  online_on INTEGER NOT NULL DEFAULT 1,
  support_phone TEXT NOT NULL DEFAULT '',
  version INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE staff (
  id INTEGER PRIMARY KEY,
  username TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'staff')),
  password_hash TEXT NOT NULL,
  must_change INTEGER NOT NULL DEFAULT 0,
  temp_expires_at TEXT,
  active INTEGER NOT NULL DEFAULT 1,
  failures INTEGER NOT NULL DEFAULT 0,
  locked_until TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE staff_session (
  token_hash TEXT PRIMARY KEY,
  staff_id INTEGER NOT NULL REFERENCES staff(id) ON DELETE CASCADE,
  signed_in_at TEXT NOT NULL
);
CREATE TABLE court (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  code TEXT NOT NULL UNIQUE,
  position INTEGER NOT NULL,
  camera_url TEXT,
  camera_user TEXT,
  camera_password TEXT,
  created_at TEXT NOT NULL
);
CREATE TABLE activity (
  id INTEGER PRIMARY KEY,
  at TEXT NOT NULL,
  staff_id INTEGER REFERENCES staff(id) ON DELETE SET NULL,
  actor TEXT NOT NULL,
  action TEXT NOT NULL,
  item TEXT NOT NULL DEFAULT '',
  detail TEXT NOT NULL DEFAULT ''
);
"#];

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}

/// Appends to the activity log (FR-35). Never pass player emails or passwords in `detail`.
pub fn log(conn: &Connection, staff_id: Option<i64>, actor: &str, action: &str, item: &str, detail: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO activity (at, staff_id, actor, action, item, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![crate::clock::now().to_rfc3339(), staff_id, actor, action, item, detail],
    )?;
    Ok(())
}

pub fn venue_name(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT name FROM venue WHERE id = 1", [], |r| r.get(0)).optional()
}

pub fn has_staff(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS (SELECT 1 FROM staff)", [], |r| r.get(0))
}

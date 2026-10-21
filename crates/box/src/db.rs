//! SQLite store for one court box. WAL + synchronous=FULL so a power cut never loses a saved clip, setting or
//! account (FR-56). Times are stored as RFC 3339 text in Manila time.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

/// Statements run since start (test builds only): lets the e2e suite prove a page's query count is bounded.
#[cfg(feature = "test-hooks")]
pub static STATEMENTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// ponytail: one connection behind a mutex; a pool when contention shows up in the load test.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        #[cfg(feature = "test-hooks")]
        conn.trace_v2(rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT, Some(|_| {
            STATEMENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }));
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA secure_delete=ON;")?;
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
"#, r#"
CREATE TABLE segment (
  id INTEGER PRIMARY KEY,
  court_id INTEGER NOT NULL REFERENCES court(id) ON DELETE CASCADE,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER,
  path TEXT NOT NULL UNIQUE
);
CREATE INDEX segment_court_start ON segment (court_id, start_ms);
"#, r#"
CREATE TABLE game (
  id INTEGER PRIMARY KEY,
  court_id INTEGER NOT NULL REFERENCES court(id) ON DELETE CASCADE,
  started_at TEXT NOT NULL,
  stopped_at TEXT,
  may_post INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE clip (
  id INTEGER PRIMARY KEY,
  court_id INTEGER NOT NULL REFERENCES court(id) ON DELETE CASCADE,
  game_id INTEGER REFERENCES game(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK (kind IN ('tap', 'highlight', 'reel', 'game')),
  token TEXT NOT NULL UNIQUE,
  window_start_ms INTEGER NOT NULL,
  window_end_ms INTEGER NOT NULL,
  last_tap_ms INTEGER,
  made_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'saving' CHECK (state IN ('saving', 'ready', 'failed')),
  failure TEXT,
  hidden INTEGER NOT NULL DEFAULT 0,
  kept_at TEXT,
  wide_path TEXT,
  vertical_path TEXT
);
CREATE INDEX clip_court_made ON clip (court_id, made_at);
ALTER TABLE segment ADD COLUMN width INTEGER;
ALTER TABLE segment ADD COLUMN height INTEGER;
"#, r#"
ALTER TABLE venue ADD COLUMN contact TEXT NOT NULL DEFAULT '';
ALTER TABLE venue ADD COLUMN clock_correction_ms INTEGER NOT NULL DEFAULT 0;
CREATE INDEX clip_made ON clip (made_at);
CREATE TABLE hold (
  id INTEGER PRIMARY KEY,
  court_id INTEGER NOT NULL REFERENCES court(id) ON DELETE CASCADE,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  placed_at TEXT NOT NULL,
  placed_by TEXT NOT NULL,
  release_at TEXT NOT NULL,
  released_at TEXT,
  released_by TEXT
);
CREATE INDEX hold_court ON hold (court_id);
"#, r#"
ALTER TABLE game ADD COLUMN stop_reason TEXT;
ALTER TABLE game ADD COLUMN highlights TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE game ADD COLUMN highlights_ready_by TEXT;
ALTER TABLE game ADD COLUMN emails_sent_at TEXT;
CREATE UNIQUE INDEX game_running ON game (court_id) WHERE stopped_at IS NULL;
ALTER TABLE clip ADD COLUMN rank INTEGER NOT NULL DEFAULT 0;
ALTER TABLE clip ADD COLUMN email_path TEXT;
ALTER TABLE clip ADD COLUMN emails_sent_at TEXT;
CREATE INDEX clip_game ON clip (game_id);
CREATE TABLE recipient (
  id INTEGER PRIMARY KEY,
  game_id INTEGER REFERENCES game(id) ON DELETE CASCADE,
  clip_id INTEGER REFERENCES clip(id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  added_at TEXT NOT NULL,
  CHECK ((game_id IS NULL) <> (clip_id IS NULL))
);
CREATE INDEX recipient_game ON recipient (game_id);
CREATE INDEX recipient_clip ON recipient (clip_id);
"#, r#"
CREATE TABLE box_config (name TEXT PRIMARY KEY, value TEXT NOT NULL);
ALTER TABLE clip ADD COLUMN cloud_id INTEGER;
ALTER TABLE clip ADD COLUMN uploaded_at TEXT;
CREATE TABLE withdraw (cloud_id INTEGER PRIMARY KEY);
"#, r#"
ALTER TABLE venue ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
ALTER TABLE venue ADD COLUMN updated_box TEXT NOT NULL DEFAULT '';
ALTER TABLE venue ADD COLUMN time_leader TEXT NOT NULL DEFAULT '';
ALTER TABLE staff ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE staff ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
ALTER TABLE staff ADD COLUMN updated_box TEXT NOT NULL DEFAULT '';
ALTER TABLE staff ADD COLUMN sessions_after TEXT;
ALTER TABLE court ADD COLUMN box TEXT NOT NULL DEFAULT '';
ALTER TABLE court ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
ALTER TABLE court ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
ALTER TABLE court ADD COLUMN updated_box TEXT NOT NULL DEFAULT '';
ALTER TABLE court ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
-- Venue sync (sync.rs): a local edit leaves version and updated_at as they were, so these triggers stamp it;
-- a row applied from a sibling box arrives with its own stamp and is left alone.
CREATE TRIGGER venue_stamp AFTER UPDATE ON venue WHEN NEW.version = OLD.version AND NEW.updated_at = OLD.updated_at BEGIN
  UPDATE venue SET version = OLD.version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') WHERE id = NEW.id;
END;
CREATE TRIGGER venue_stamp_new AFTER INSERT ON venue WHEN NEW.updated_at = '' BEGIN
  UPDATE venue SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') WHERE id = NEW.id;
END;
CREATE TRIGGER staff_stamp AFTER UPDATE ON staff WHEN NEW.version = OLD.version AND NEW.updated_at = OLD.updated_at BEGIN
  UPDATE staff SET version = OLD.version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') WHERE id = NEW.id;
END;
CREATE TRIGGER staff_stamp_new AFTER INSERT ON staff WHEN NEW.updated_at = '' BEGIN
  UPDATE staff SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') WHERE id = NEW.id;
END;
CREATE TRIGGER court_stamp AFTER UPDATE ON court WHEN NEW.version = OLD.version AND NEW.updated_at = OLD.updated_at BEGIN
  UPDATE court SET version = OLD.version + 1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') WHERE id = NEW.id;
END;
-- A court added on this box is recorded by this box.
CREATE TRIGGER court_stamp_new AFTER INSERT ON court WHEN NEW.updated_at = '' BEGIN
  UPDATE court SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    updated_box = COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), ''),
    box = CASE WHEN NEW.box = '' THEN COALESCE((SELECT value FROM box_config WHERE name = 'box_name'), '') ELSE NEW.box END WHERE id = NEW.id;
END;
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

//! Venue sync (FR-41…FR-43): staff accounts, venue settings, the online switch and the court list are the same on
//! every box at a venue. Boxes trust each other only through the venue key set at install (FR-42): every sync
//! request is signed with it and every answer sealed with it, so a device without the key gets nothing.
//!
//! Each box pulls its siblings' state every few seconds and keeps, row by row, whichever version is newest
//! (version, then time of the change, then box name). Local edits are stamped by SQLite triggers (db.rs).

use crate::{clock, db::Db, web::AppState};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use hmac::{Hmac, KeyInit as HmacKey, Mac};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// A request older or newer than this (raw clocks, not the venue's corrected time) is refused.
const AUTH_WINDOW_MS: i64 = 10 * 60 * 1000;

#[derive(Clone)]
pub struct Venue {
    auth: [u8; 32],
    seal: [u8; 32],
    pub peers: Vec<String>,
    pub box_name: String,
}

fn derive(key: &[u8], label: &str) -> [u8; 32] {
    Sha256::new().chain_update(label.as_bytes()).chain_update(key).finalize().into()
}

/// CH_VENUE_KEY (64 hex characters, the same on every box at the venue) and CH_PEERS (the siblings' sync
/// addresses). Without a key the box is on its own.
pub fn from_env(box_name: &str) -> Result<Option<Venue>, Error> {
    let Some(hex) = std::env::var("CH_VENUE_KEY").ok().filter(|k| !k.is_empty()) else { return Ok(None) };
    let key: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()))
        .collect::<Option<_>>()
        .filter(|k: &Vec<u8>| k.len() == 32)
        .ok_or("CH_VENUE_KEY must be 64 hex characters")?;
    let peers = std::env::var("CH_PEERS").unwrap_or_default().split(',').map(|p| p.trim().trim_end_matches('/').to_string()).filter(|p| !p.is_empty()).collect();
    Ok(Some(Venue { auth: derive(&key, "ch-sync-auth"), seal: derive(&key, "ch-sync-seal"), peers, box_name: box_name.into() }))
}

fn raw_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Venue {
    fn signature(&self, method: &str, path: &str, ts: i64) -> String {
        let mut mac = <Hmac<Sha256> as HmacKey>::new_from_slice(&self.auth).expect("any key length");
        mac.update(format!("{method}\n{path}\n{ts}").as_bytes());
        crate::auth::hex(&mac.finalize().into_bytes())
    }

    pub fn sign(&self, method: &str, path: &str) -> String {
        let ts = raw_now_ms();
        format!("{ts}.{}", self.signature(method, path, ts))
    }

    fn verify(&self, method: &str, path: &str, header: Option<&str>) -> bool {
        let Some((ts, sig)) = header.and_then(|h| h.split_once('.')) else { return false };
        let Ok(ts) = ts.parse::<i64>() else { return false };
        if (raw_now_ms() - ts).abs() > AUTH_WINDOW_MS {
            return false;
        }
        let mut mac = <Hmac<Sha256> as HmacKey>::new_from_slice(&self.auth).expect("any key length");
        mac.update(format!("{method}\n{path}\n{ts}").as_bytes());
        let Some(sig) = decode_hex(sig) else { return false };
        mac.verify_slice(&sig).is_ok()
    }

    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        getrandom::fill(&mut nonce).expect("OS random source");
        let mut out = nonce.to_vec();
        out.extend(ChaCha20Poly1305::new(&self.seal.into()).encrypt(&Nonce::from(nonce), plain).expect("encrypt"));
        out
    }

    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        let (nonce, body) = sealed.split_at_checked(12)?;
        let nonce: [u8; 12] = nonce.try_into().ok()?;
        ChaCha20Poly1305::new(&self.seal.into()).decrypt(&Nonce::from(nonce), body).ok()
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}

// ------------------------------------------------------------------ the shared state

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stamp {
    pub version: i64,
    pub updated_at: String,
    pub updated_box: String,
}

impl Stamp {
    /// Newest wins: higher version, then later change, then box name (so every box picks the same row).
    fn newer_than(&self, other: &Stamp) -> bool {
        (self.version, &self.updated_at, &self.updated_box) > (other.version, &other.updated_at, &other.updated_box)
    }
}

#[derive(Serialize, Deserialize)]
pub struct VenueRow {
    pub name: String,
    pub logo: Option<Vec<u8>>,
    pub logo_type: Option<String>,
    pub logo_corner: String,
    pub sponsor: Option<Vec<u8>>,
    pub sponsor_type: Option<String>,
    pub open_time: String,
    pub close_time: String,
    pub clip_before_s: i64,
    pub clip_after_s: i64,
    pub upload_cap_kbps: i64,
    pub judge_on: bool,
    pub privacy_notice: String,
    pub online_on: bool,
    pub support_phone: String,
    pub contact: String,
    pub time_leader: String,
    pub stamp: Stamp,
}

#[derive(Serialize, Deserialize)]
pub struct StaffRow {
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub password_hash: String,
    pub must_change: bool,
    pub temp_expires_at: Option<String>,
    pub active: bool,
    pub failures: i64,
    pub locked_until: Option<String>,
    pub sessions_after: Option<String>,
    pub created_at: String,
    pub stamp: Stamp,
}

#[derive(Serialize, Deserialize)]
pub struct CourtRow {
    pub code: String,
    pub name: String,
    pub position: i64,
    pub hosted_by: String,
    pub camera_url: Option<String>,
    pub camera_user: Option<String>,
    pub camera_password: Option<String>,
    pub deleted: bool,
    pub created_at: String,
    pub stamp: Stamp,
}

/// What one box tells its siblings.
#[derive(Serialize, Deserialize)]
pub struct State_ {
    pub box_name: String,
    pub player_url: String,
    pub now_ms: i64,
    pub venue: Option<VenueRow>,
    pub staff: Vec<StaffRow>,
    pub courts: Vec<CourtRow>,
    /// Live status of the courts this box records, for the venue console (FR-41).
    pub live: Vec<CourtLive>,
    pub health: BoxHealth,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CourtLive {
    pub code: String,
    pub ok: bool,
    pub status: String,
    pub last_clip: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct BoxHealth {
    pub disk: String,
    pub disk_warning: bool,
    pub temperature: String,
    pub version: String,
}

fn stamp(r: &rusqlite::Row, at: usize) -> rusqlite::Result<Stamp> {
    Ok(Stamp { version: r.get(at)?, updated_at: r.get(at + 1)?, updated_box: r.get(at + 2)? })
}

pub fn local_state(app: &AppState) -> Result<State_, Error> {
    let (v, staff, courts) = {
        let conn = app.db.conn();
    let v = conn
        .query_row(
            "SELECT name, logo, logo_type, logo_corner, sponsor, sponsor_type, open_time, close_time, clip_before_s, clip_after_s,
                    upload_cap_kbps, judge_on, privacy_notice, online_on, support_phone, contact, time_leader, version, updated_at, updated_box
             FROM venue WHERE id = 1",
            [],
            |r| {
                Ok(VenueRow {
                    name: r.get(0)?,
                    logo: r.get(1)?,
                    logo_type: r.get(2)?,
                    logo_corner: r.get(3)?,
                    sponsor: r.get(4)?,
                    sponsor_type: r.get(5)?,
                    open_time: r.get(6)?,
                    close_time: r.get(7)?,
                    clip_before_s: r.get(8)?,
                    clip_after_s: r.get(9)?,
                    upload_cap_kbps: r.get(10)?,
                    judge_on: r.get(11)?,
                    privacy_notice: r.get(12)?,
                    online_on: r.get(13)?,
                    support_phone: r.get(14)?,
                    contact: r.get(15)?,
                    time_leader: r.get(16)?,
                    stamp: stamp(r, 17)?,
                })
            },
        )
        .optional()?;
    let mut st = conn.prepare(
        "SELECT username, display_name, role, password_hash, must_change, temp_expires_at, active, failures, locked_until, sessions_after, created_at,
                version, updated_at, updated_box FROM staff",
    )?;
    let staff = st
        .query_map([], |r| {
            Ok(StaffRow {
                username: r.get(0)?,
                display_name: r.get(1)?,
                role: r.get(2)?,
                password_hash: r.get(3)?,
                must_change: r.get(4)?,
                temp_expires_at: r.get(5)?,
                active: r.get(6)?,
                failures: r.get(7)?,
                locked_until: r.get(8)?,
                sessions_after: r.get(9)?,
                created_at: r.get(10)?,
                stamp: stamp(r, 11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut st = conn.prepare(
        "SELECT code, name, position, box, camera_url, camera_user, camera_password, deleted, created_at, version, updated_at, updated_box FROM court",
    )?;
    let courts = st
        .query_map([], |r| {
            Ok(CourtRow {
                code: r.get(0)?,
                name: r.get(1)?,
                position: r.get(2)?,
                hosted_by: r.get(3)?,
                camera_url: r.get(4)?,
                camera_user: r.get(5)?,
                camera_password: r.get(6)?,
                deleted: r.get(7)?,
                created_at: r.get(8)?,
                stamp: stamp(r, 9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        (v, staff, courts)
    };
    let (live, health) = crate::courts::local_live(app)?;
    Ok(State_ {
        box_name: app.box_name.clone(),
        player_url: crate::venue::player_base_env().unwrap_or_default(),
        now_ms: clock::now().timestamp_millis(),
        venue: v,
        staff,
        courts,
        live,
        health,
    })
}

/// Applies a sibling's rows that are newer than ours. Applied rows keep the sender's stamp, so the triggers that
/// stamp local edits stay quiet (they fire only when version and updated_at are unchanged).
pub fn merge(db: &Db, remote: &State_) -> rusqlite::Result<usize> {
    let conn = db.conn();
    let tx = conn.unchecked_transaction()?;
    let mut applied = 0;
    if let Some(v) = &remote.venue {
        let local: Option<Stamp> = tx.query_row("SELECT version, updated_at, updated_box FROM venue WHERE id = 1", [], |r| stamp(r, 0)).optional()?;
        if local.as_ref().is_none_or(|l| v.stamp.newer_than(l)) {
            tx.execute(
                "INSERT INTO venue (id, name, logo, logo_type, logo_corner, sponsor, sponsor_type, open_time, close_time, clip_before_s, clip_after_s,
                   upload_cap_kbps, judge_on, privacy_notice, online_on, support_phone, contact, time_leader, version, updated_at, updated_box)
                 VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name, logo = excluded.logo, logo_type = excluded.logo_type, logo_corner = excluded.logo_corner,
                   sponsor = excluded.sponsor, sponsor_type = excluded.sponsor_type, open_time = excluded.open_time, close_time = excluded.close_time,
                   clip_before_s = excluded.clip_before_s, clip_after_s = excluded.clip_after_s, upload_cap_kbps = excluded.upload_cap_kbps,
                   judge_on = excluded.judge_on, privacy_notice = excluded.privacy_notice, online_on = excluded.online_on,
                   support_phone = excluded.support_phone, contact = excluded.contact, time_leader = excluded.time_leader,
                   version = excluded.version, updated_at = excluded.updated_at, updated_box = excluded.updated_box",
                params![
                    v.name, v.logo, v.logo_type, v.logo_corner, v.sponsor, v.sponsor_type, v.open_time, v.close_time, v.clip_before_s, v.clip_after_s,
                    v.upload_cap_kbps, v.judge_on, v.privacy_notice, v.online_on, v.support_phone, v.contact, v.time_leader,
                    v.stamp.version, v.stamp.updated_at, v.stamp.updated_box
                ],
            )?;
            applied += 1;
        }
    }
    for s in &remote.staff {
        let local: Option<Stamp> = tx.query_row("SELECT version, updated_at, updated_box FROM staff WHERE username = ?1", [&s.username], |r| stamp(r, 0)).optional()?;
        if local.as_ref().is_none_or(|l| s.stamp.newer_than(l)) {
            tx.execute(
                "INSERT INTO staff (username, display_name, role, password_hash, must_change, temp_expires_at, active, failures, locked_until, sessions_after,
                   created_at, version, updated_at, updated_box)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT (username) DO UPDATE SET display_name = excluded.display_name, role = excluded.role, password_hash = excluded.password_hash,
                   must_change = excluded.must_change, temp_expires_at = excluded.temp_expires_at, active = excluded.active, failures = excluded.failures,
                   locked_until = excluded.locked_until, sessions_after = excluded.sessions_after,
                   version = excluded.version, updated_at = excluded.updated_at, updated_box = excluded.updated_box",
                params![
                    s.username, s.display_name, s.role, s.password_hash, s.must_change, s.temp_expires_at, s.active, s.failures, s.locked_until,
                    s.sessions_after, s.created_at, s.stamp.version, s.stamp.updated_at, s.stamp.updated_box
                ],
            )?;
            applied += 1;
        }
    }
    for c in &remote.courts {
        let local: Option<Stamp> = tx.query_row("SELECT version, updated_at, updated_box FROM court WHERE code = ?1", [&c.code], |r| stamp(r, 0)).optional()?;
        if local.as_ref().is_none_or(|l| c.stamp.newer_than(l)) {
            tx.execute(
                "INSERT INTO court (code, name, position, box, camera_url, camera_user, camera_password, deleted, created_at, version, updated_at, updated_box)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (code) DO UPDATE SET name = excluded.name, position = excluded.position, box = excluded.box, camera_url = excluded.camera_url,
                   camera_user = excluded.camera_user, camera_password = excluded.camera_password, deleted = excluded.deleted,
                   version = excluded.version, updated_at = excluded.updated_at, updated_box = excluded.updated_box",
                params![
                    c.code, c.name, c.position, c.hosted_by, c.camera_url, c.camera_user, c.camera_password, c.deleted, c.created_at,
                    c.stamp.version, c.stamp.updated_at, c.stamp.updated_box
                ],
            )?;
            applied += 1;
        }
    }
    tx.commit()?;
    Ok(applied)
}

// ------------------------------------------------------------------ serving and pulling

/// The venue, when the request carries a valid venue signature over its method, path and query (FR-42).
pub fn authorized(app: &AppState, req: &Request) -> Option<Venue> {
    let venue = app.venue.clone()?;
    let path = req.uri().path_and_query().map_or("/", |p| p.as_str());
    let header = req.headers().get("x-venue-auth").and_then(|v| v.to_str().ok());
    if venue.verify(req.method().as_str(), path, header) {
        Some(venue)
    } else {
        tracing::warn!(%path, "sync request refused: no valid venue signature");
        None
    }
}

/// GET /sync/state on the sync listener: refused without a valid venue signature (FR-42).
pub async fn serve_state(State(app): State<AppState>, req: Request) -> Response {
    let Some(venue) = authorized(&app, &req) else { return (StatusCode::UNAUTHORIZED, "venue key required").into_response() };
    match local_state(&app).and_then(|s| Ok(serde_json::to_vec(&s)?)) {
        Ok(body) => ([("content-type", "application/octet-stream")], venue.seal(&body)).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "sync state failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// What this box last heard from a sibling, for the venue console (FR-41).
#[derive(Clone)]
pub struct Peer {
    pub name: String,
    pub player_url: String,
    pub last_ok: Option<ch_core::Time>,
    pub live: Vec<CourtLive>,
    pub health: BoxHealth,
}

pub type Peers = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Peer>>>;

pub fn spawn(app: AppState) {
    let Some(venue) = app.venue.clone() else { return };
    let every = std::env::var("CH_SYNC_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(15);
    tokio::spawn(async move {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client");
        loop {
            for peer in &venue.peers {
                if let Err(e) = pull(&app, &venue, &http, peer).await {
                    tracing::warn!(peer = %peer, error = %e, "sync with sibling box failed");
                }
            }
            tokio::time::sleep(Duration::from_secs(every)).await;
        }
    });
}

async fn pull(app: &AppState, venue: &Venue, http: &reqwest::Client, peer: &str) -> Result<(), Error> {
    let path = "/sync/state";
    let sent = std::time::Instant::now();
    let res = http.get(format!("{peer}{path}")).header("x-venue-auth", venue.sign("GET", path)).send().await?.error_for_status()?;
    let body = res.bytes().await?;
    let rtt_ms = sent.elapsed().as_millis() as i64;
    let plain = venue.open(&body).ok_or("answer not sealed with this venue's key")?;
    let state: State_ = serde_json::from_slice(&plain)?;
    let applied = merge(&app.db, &state)?;
    if applied > 0 {
        tracing::info!(peer = %state.box_name, applied, "venue sync applied changes");
    }
    align_clock(app, venue, &state, rtt_ms)?;
    app.peers.lock().unwrap_or_else(|p| p.into_inner()).insert(
        peer.to_string(),
        Peer { name: state.box_name, player_url: state.player_url, last_ok: Some(clock::now()), live: state.live, health: state.health },
    );
    Ok(())
}

/// All boxes at a venue use the same time (FR-38): the box where the Owner last corrected the time leads.
fn align_clock(app: &AppState, venue: &Venue, state: &State_, rtt_ms: i64) -> Result<(), Error> {
    let leader: String = app.db.conn().query_row("SELECT time_leader FROM venue WHERE id = 1", [], |r| r.get(0)).optional()?.unwrap_or_default();
    if leader.is_empty() || leader == venue.box_name || leader != state.box_name {
        return Ok(());
    }
    let theirs = state.now_ms + rtt_ms / 2;
    let drift = theirs - clock::now().timestamp_millis();
    if drift.abs() > 2_000 {
        let wanted = chrono::DateTime::from_timestamp_millis(clock::now().timestamp_millis() + drift).unwrap_or_default().with_timezone(&ch_core::manila());
        let correction = clock::correct_to(wanted);
        crate::uplink::set_config(&app.db, "clock_correction_ms", &correction.to_string())?;
        app.recorders.split_all();
        tracing::info!(drift_ms = drift, leader = %leader, "box time aligned with the venue");
    }
    Ok(())
}

/// Sync listener (CH_SYNC_ADDR): only sibling boxes holding the venue key get answers.
pub fn router(state: AppState) -> axum::Router {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/sync/state", get(serve_state))
        .route("/sync/clips", get(crate::library::sync_clips))
        .route("/sync/clips/{id}/meta", get(crate::library::sync_meta))
        .route("/sync/clips/{id}/keep", post(crate::library::sync_keep))
        .route("/sync/clips/{id}/delete", post(crate::library::sync_delete))
        .route("/sync/clips/{id}/{file}", get(crate::library::sync_file))
        .with_state(state)
}

pub fn peers_snapshot(app: &AppState) -> Vec<Peer> {
    let mut v: Vec<Peer> = app.peers.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// Sibling addresses this box has never heard from (shown as unreachable since start-up).
pub fn silent_peers(app: &AppState) -> Vec<String> {
    let known = app.peers.lock().unwrap_or_else(|p| p.into_inner());
    app.venue.as_ref().map_or_else(Vec::new, |v| v.peers.iter().filter(|p| !known.contains_key(*p)).cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn venue() -> Venue {
        Venue { auth: [1; 32], seal: [2; 32], peers: vec![], box_name: "box-a".into() }
    }

    #[test]
    fn signatures_bind_method_path_and_time() {
        let v = venue();
        let header = v.sign("GET", "/sync/state");
        assert!(v.verify("GET", "/sync/state", Some(&header)));
        assert!(!v.verify("GET", "/sync/other", Some(&header)));
        assert!(!v.verify("POST", "/sync/state", Some(&header)));
        let other = Venue { auth: [9; 32], ..venue() };
        assert!(!other.verify("GET", "/sync/state", Some(&header)), "a device without the venue key is refused");
        let old = format!("{}.{}", raw_now_ms() - AUTH_WINDOW_MS - 1000, v.signature("GET", "/sync/state", raw_now_ms() - AUTH_WINDOW_MS - 1000));
        assert!(!v.verify("GET", "/sync/state", Some(&old)), "stale signatures are refused");
        assert!(!v.verify("GET", "/sync/state", None));
    }

    #[test]
    fn sealed_answers_need_the_venue_key() {
        let v = venue();
        let sealed = v.seal(b"staff and settings");
        assert_eq!(v.open(&sealed).as_deref(), Some(&b"staff and settings"[..]));
        assert_eq!(Venue { seal: [3; 32], ..venue() }.open(&sealed), None);
    }

    #[test]
    fn newest_stamp_wins_the_same_way_everywhere() {
        let s = |v: i64, at: &str, b: &str| Stamp { version: v, updated_at: at.into(), updated_box: b.into() };
        assert!(s(3, "a", "x").newer_than(&s(2, "z", "z")));
        assert!(s(2, "2026-10-21T10:00:01Z", "a").newer_than(&s(2, "2026-10-21T10:00:00Z", "z")));
        assert!(s(2, "t", "box-b").newer_than(&s(2, "t", "box-a")));
        assert!(!s(2, "t", "box-a").newer_than(&s(2, "t", "box-a")));
    }
}

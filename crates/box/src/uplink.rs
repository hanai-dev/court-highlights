//! Store and forward (FR-22, FR-23, FR-26, FR-27): check-ins carry box health out and the venue's online switch
//! back; finished items of games (and Email-me-this clips) that have addresses go up in the agreed order, under
//! the upload cap, then the service emails the addresses and the box forgets them.

use crate::recorder::{CourtStatus, disk_use, temperature_c};
use crate::{clock, db, web::AppState};
use ch_core::{Outgoing, Time};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use std::time::Duration;

type Error = Box<dyn std::error::Error + Send + Sync>;

/// This box's own settings — never shared with the venue's other boxes.
pub fn config(db: &db::Db, name: &str) -> rusqlite::Result<Option<String>> {
    db.conn().query_row("SELECT value FROM box_config WHERE name = ?1", [name], |r| r.get(0)).optional()
}

pub fn set_config(db: &db::Db, name: &str, value: &str) -> rusqlite::Result<()> {
    db.conn().execute("INSERT INTO box_config (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = excluded.value", params![name, value])?;
    Ok(())
}

pub fn cloud_url() -> Option<String> {
    std::env::var("CH_CLOUD_URL").ok().filter(|u| !u.is_empty()).map(|u| u.trim_end_matches('/').to_string())
}

/// Registered with the online service (an address and this box's key).
pub fn registered(db: &db::Db) -> rusqlite::Result<bool> {
    Ok(cloud_url().is_some() && config(db, "cloud_key")?.is_some_and(|k| !k.is_empty()))
}

struct Client {
    http: reqwest::Client,
    base: String,
    key: String,
}

impl Client {
    fn new(app: &AppState) -> Result<Option<Self>, Error> {
        let (Some(base), Some(key)) = (cloud_url(), config(&app.db, "cloud_key")?.filter(|k| !k.is_empty())) else { return Ok(None) };
        let http = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
        Ok(Some(Self { http, base, key }))
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.post(format!("{}{path}", self.base)).bearer_auth(&self.key).timeout(Duration::from_secs(30))
    }
}

pub fn spawn(app: AppState) {
    let every = std::env::var("CH_CHECKIN_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    let a = app.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = checkin(&a).await {
                tracing::warn!(error = %e, "check-in failed");
            }
            tokio::time::sleep(Duration::from_secs(every)).await;
        }
    });
    tokio::spawn(async move {
        loop {
            if let Err(e) = send(&app).await {
                tracing::warn!(error = %e, "sending failed; will retry");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

#[derive(Deserialize)]
struct CheckinAnswer {
    online_on: bool,
}

async fn checkin(app: &AppState) -> Result<(), Error> {
    let Some(c) = Client::new(app)? else { return Ok(()) };
    let courts: Vec<(i64, bool)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, camera_url IS NOT NULL FROM court")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };
    let recording = courts.iter().filter(|(id, cam)| matches!(app.recorders.status(*id, *cam), CourtStatus::Recording { .. })).count();
    let body = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "disk_pct": disk_use(&app.data).map_or(0, |d| d.used_pct),
        "temperature_c": temperature_c(),
        "courts_recording": recording,
        "courts_total": courts.len(),
    });
    let res = c.post("/api/v1/checkin").json(&body).send().await?;
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        set_config(&app.db, "cloud_status", "key refused")?;
        return Err("the online service refused this box's key".into());
    }
    let answer: CheckinAnswer = res.error_for_status()?.json().await?;
    set_config(&app.db, "cloud_status", "connected")?;
    set_config(&app.db, "last_checkin", &clock::now().to_rfc3339())?;
    let conn = app.db.conn();
    let was: bool = conn.query_row("SELECT online_on FROM venue WHERE id = 1", [], |r| r.get(0))?;
    if was != answer.online_on {
        conn.execute("UPDATE venue SET online_on = ?1, version = version + 1 WHERE id = 1", [answer.online_on])?;
        let action = if answer.online_on { "online features switched on" } else { "online features switched off" };
        db::log(&conn, None, "fleet", action, "", "")?;
    }
    Ok(())
}

// ------------------------------------------------------------------ sending

#[derive(Clone)]
struct Item {
    clip: i64,
    kind: String,
    court: String,
    made: Time,
    path: String,
    cloud: Option<i64>,
    group: usize,
}

struct Group {
    box_ref: String,
    title: String,
    /// (column, id) of the recipients and the "sent" mark.
    owner: (&'static str, i64),
    stopped: Time,
    items: Vec<Item>,
    recipients: Vec<String>,
}

fn span_title(court: &str, start: Time, end: Option<Time>) -> String {
    let day = start.format("%-d %b %Y");
    match end {
        Some(e) => format!("{court} game, {day}, {}–{}", start.format("%-I:%M %P"), e.format("%-I:%M %P")),
        None => format!("{court} clip, {day}, {}", start.format("%-I:%M %P")),
    }
}

fn load_groups(app: &AppState) -> Result<Vec<Group>, Error> {
    let conn = app.db.conn();
    let mut groups = Vec::new();
    // Games whose videos and highlights are all finished and that have addresses.
    let mut st = conn.prepare(
        "SELECT g.id, g.started_at, g.stopped_at, c.name FROM game g JOIN court c ON c.id = g.court_id
         WHERE g.emails_sent_at IS NULL AND g.stopped_at IS NOT NULL AND g.highlights NOT IN ('pending', 'running')
           AND EXISTS (SELECT 1 FROM recipient r WHERE r.game_id = g.id)
           AND NOT EXISTS (SELECT 1 FROM clip k WHERE k.game_id = g.id AND k.state = 'saving')",
    )?;
    let games: Vec<(i64, String, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    for (id, started, stopped, court) in games {
        let (Some(s), Some(e)) = (clock::parse(&started), clock::parse(&stopped)) else { continue };
        groups.push(Group { box_ref: format!("game-{id}"), title: span_title(&court, s, Some(e)), owner: ("game_id", id), stopped: e, items: Vec::new(), recipients: Vec::new() });
    }
    // Email-me-this clips (FR-21).
    let mut st = conn.prepare(
        "SELECT k.id, k.made_at, c.name FROM clip k JOIN court c ON c.id = k.court_id
         WHERE k.kind = 'tap' AND k.game_id IS NULL AND k.emails_sent_at IS NULL AND k.state = 'ready' AND k.hidden = 0
           AND EXISTS (SELECT 1 FROM recipient r WHERE r.clip_id = k.id)",
    )?;
    let clips: Vec<(i64, String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    for (id, made, court) in clips {
        let Some(m) = clock::parse(&made) else { continue };
        groups.push(Group { box_ref: format!("clip-{id}"), title: span_title(&court, m, None), owner: ("clip_id", id), stopped: m, items: Vec::new(), recipients: Vec::new() });
    }
    for (i, g) in groups.iter_mut().enumerate() {
        let filter = if g.owner.0 == "game_id" { "k.game_id = ?1" } else { "k.id = ?1" };
        let mut st = conn.prepare(&format!(
            "SELECT k.id, k.kind, c.name, k.made_at, CASE k.kind WHEN 'game' THEN k.email_path ELSE k.wide_path END, k.cloud_id
             FROM clip k JOIN court c ON c.id = k.court_id WHERE {filter} AND k.state = 'ready' AND k.hidden = 0 ORDER BY k.rank, k.made_at"
        ))?;
        g.items = st
            .query_map([g.owner.1], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, Option<i64>>(5)?)))?
            .filter_map(|row| {
                let (clip, kind, court, made, path, cloud) = row.ok()?;
                Some(Item { clip, kind, court, made: clock::parse(&made)?, path: path?, cloud, group: i })
            })
            .collect();
        let mut rs = conn.prepare(&format!("SELECT email FROM recipient WHERE {} = ?1", g.owner.0))?;
        g.recipients = rs.query_map([g.owner.1], |r| r.get(0))?.collect::<Result<_, _>>()?;
    }
    Ok(groups)
}

async fn send(app: &AppState) -> Result<(), Error> {
    if !crate::games::online(&app.db)? {
        return Ok(());
    }
    let Some(c) = Client::new(app)? else { return Ok(()) };
    withdraw_pending(app, &c).await?;
    let groups = load_groups(app)?;
    let now = clock::now();
    // Expired items are never sent (GV-26); the rest go in the agreed order (FR-22, GV-19).
    let all: Vec<(Time, Outgoing, Item)> = groups
        .iter()
        .flat_map(|g| g.items.iter().map(move |i| (g.stopped, i)))
        .filter(|(_, i)| ch_core::still_available(ch_core::expires_at(i.made), now).is_ok())
        .map(|(stopped, i)| {
            let kind = match i.kind.as_str() {
                "game" => Outgoing::GameVideo,
                "reel" => Outgoing::Reel,
                _ => Outgoing::Clip { made: i.made },
            };
            (stopped, kind, i.clone())
        })
        .collect();
    let order = ch_core::send_order(&all);
    let mut cloud: std::collections::HashMap<i64, i64> = order.iter().filter_map(|i| Some((i.clip, i.cloud?))).collect();
    let mut delivered = vec![false; groups.len()];
    for item in &order {
        if !cloud.contains_key(&item.clip) {
            let id = upload(app, &c, item).await?;
            cloud.insert(item.clip, id);
        }
        // A group goes out as soon as its last item is up.
        let g = &groups[item.group];
        if !delivered[item.group] && g.items.iter().filter(|i| ch_core::still_available(ch_core::expires_at(i.made), now).is_ok()).all(|i| cloud.contains_key(&i.clip)) {
            deliver(app, &c, g, &cloud).await?;
            delivered[item.group] = true;
        }
    }
    // Groups with nothing left to send (all expired): forget the addresses, send nothing (GV-26).
    for (i, g) in groups.iter().enumerate() {
        if !delivered[i] && !order.iter().any(|it| it.group == i) {
            finish(app, g, "expired before it could be sent")?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct UploadAnswer {
    id: i64,
    upload_url: Option<String>,
}

async fn upload(app: &AppState, c: &Client, item: &Item) -> Result<i64, Error> {
    let body = serde_json::json!({
        "ref": format!("item-{}", item.clip),
        "kind": item.kind,
        "court": item.court,
        "made_at": item.made.to_utc().to_rfc3339(),
        "expires_at": ch_core::expires_at(item.made).to_utc().to_rfc3339(),
    });
    let answer: UploadAnswer = c.post("/api/v1/items").json(&body).send().await?.error_for_status()?.json().await?;
    if let Some(url) = answer.upload_url {
        let cap_kbps: i64 = app.db.conn().query_row("SELECT upload_cap_kbps FROM venue WHERE id = 1", [], |r| r.get(0))?;
        let started = std::time::Instant::now();
        let bytes = put_throttled(&c.http, &url, &item.path, cap_kbps.max(1) as u64 * 1000 / 8).await?;
        c.post(&format!("/api/v1/items/{}/done", answer.id)).send().await?.error_for_status()?;
        tracing::info!(clip = item.clip, kind = %item.kind, bytes, ms = started.elapsed().as_millis() as u64, cap_kbps, "uploaded");
    }
    app.db.conn().execute("UPDATE clip SET cloud_id = ?1, uploaded_at = ?2 WHERE id = ?3", params![answer.id, clock::now().to_rfc3339(), item.clip])?;
    tracing::info!(clip = item.clip, cloud = answer.id, "item sent");
    Ok(answer.id)
}

/// PUT a file at no more than `bytes_per_s` (the installer's upload cap, FR-22).
async fn put_throttled(http: &reqwest::Client, url: &str, path: &str, bytes_per_s: u64) -> Result<u64, Error> {
    use tokio::io::AsyncReadExt;
    let file = tokio::fs::File::open(path).await?;
    let len = file.metadata().await?.len();
    let started = tokio::time::Instant::now();
    let stream = futures::stream::unfold((file, 0u64), move |(mut file, sent)| async move {
        let mut buf = vec![0u8; 64 * 1024];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                let sent = sent + n as u64;
                let due = Duration::from_secs_f64(sent as f64 / bytes_per_s.max(1) as f64);
                tokio::time::sleep_until(started + due).await;
                Some((Ok::<_, std::io::Error>(buf), (file, sent)))
            }
            Err(e) => Some((Err(e), (file, sent))),
        }
    });
    http.put(url)
        .header(reqwest::header::CONTENT_TYPE, "video/mp4")
        .header(reqwest::header::CONTENT_LENGTH, len)
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await?
        .error_for_status()?;
    Ok(len)
}

async fn deliver(app: &AppState, c: &Client, g: &Group, cloud: &std::collections::HashMap<i64, i64>) -> Result<(), Error> {
    let items: Vec<i64> = g.items.iter().filter_map(|i| cloud.get(&i.clip).copied()).collect();
    let body = serde_json::json!({ "ref": g.box_ref, "title": g.title, "items": items, "recipients": g.recipients });
    let res = c.post("/api/v1/deliveries").json(&body).send().await?;
    match res.status().as_u16() {
        200 | 201 => finish(app, g, "sent"),
        410 => finish(app, g, "expired before it could be sent"),
        _ => {
            res.error_for_status()?;
            Ok(())
        }
    }
}

/// The addresses are deleted from the box once the email has gone (or can no longer go) (FR-23).
fn finish(app: &AppState, g: &Group, outcome: &str) -> Result<(), Error> {
    let conn = app.db.conn();
    let tx = conn.unchecked_transaction()?;
    tx.execute(&format!("DELETE FROM recipient WHERE {} = ?1", g.owner.0), [g.owner.1])?;
    let table = if g.owner.0 == "game_id" { "game" } else { "clip" };
    tx.execute(&format!("UPDATE {table} SET emails_sent_at = ?1 WHERE id = ?2"), params![clock::now().to_rfc3339(), g.owner.1])?;
    tx.commit()?;
    // Deleted rows are zeroed (secure_delete); truncating the WAL drops the old page copies too.
    conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    tracing::info!(group = %g.box_ref, recipients = g.recipients.len(), outcome, "take-home email");
    Ok(())
}

/// Hidden or deleted after sending: the online copy is removed (FR-26).
pub fn queue_withdraw(conn: &rusqlite::Connection, clip: i64) -> rusqlite::Result<()> {
    conn.execute("INSERT OR IGNORE INTO withdraw (cloud_id) SELECT cloud_id FROM clip WHERE id = ?1 AND cloud_id IS NOT NULL", [clip])?;
    Ok(())
}

async fn withdraw_pending(app: &AppState, c: &Client) -> Result<(), Error> {
    let ids: Vec<i64> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT cloud_id FROM withdraw")?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    for id in ids {
        let res = c.http.delete(format!("{}/api/v1/items/{id}", c.base)).bearer_auth(&c.key).timeout(Duration::from_secs(30)).send().await?;
        if res.status().is_success() || res.status() == reqwest::StatusCode::NOT_FOUND {
            app.db.conn().execute("DELETE FROM withdraw WHERE cloud_id = ?1", [id])?;
        }
    }
    Ok(())
}

pub fn last_checkin(db: &db::Db) -> rusqlite::Result<Option<Time>> {
    Ok(config(db, "last_checkin")?.and_then(|t| clock::parse(&t)))
}

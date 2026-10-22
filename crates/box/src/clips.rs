//! Court page and clips (FR-4…FR-9, FR-17…FR-19). Players reach these on the plain-HTTP listener without signing
//! in; every query is scoped by the court code in the address and the 2-hour court-page window (FR-18).

use crate::courts::time_label;
use crate::media::{self, Brand, Corner, Cut, Logo, Output, Shape, Size};
use crate::recorder::CourtStatus;
use crate::web::{AppError, AppState, Page, html};
use crate::{clock, db::Db};
use askama::Template;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use ch_core::{ClipSettings, Time, Window};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

struct Court {
    id: i64,
    name: String,
    code: String,
    has_camera: bool,
}

fn court_by_code(db: &Db, code: &str) -> Result<Court, AppError> {
    if code.len() != 8 || !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(AppError::NotFound);
    }
    db.conn()
        .query_row(
            "SELECT id, name, code, camera_url IS NOT NULL FROM court
             WHERE code = ?1 AND deleted = 0 AND box = (SELECT value FROM box_config WHERE name = 'box_name')",
            [code.to_ascii_uppercase()],
            |r| Ok(Court { id: r.get(0)?, name: r.get(1)?, code: r.get(2)?, has_camera: r.get(3)? }),
        )
        .optional()?
        .ok_or(AppError::NotFound)
}

pub fn ms(t: Time) -> i64 {
    t.timestamp_millis()
}

pub fn at(ms: i64) -> Time {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&ch_core::manila())
}

// ------------------------------------------------------------------ court page

pub struct ClipLine {
    pub token: String,
    pub time: String,
    pub state: String,
    pub failure: String,
    pub seconds: i64,
}

fn clip_lines(db: &Db, court: i64, now: Time) -> Result<Vec<ClipLine>, AppError> {
    let since = (now - ch_core::COURT_PAGE_WINDOW).to_rfc3339();
    let conn = db.conn();
    let mut st = conn.prepare(
        "SELECT token, made_at, state, COALESCE(failure, ''), window_start_ms, window_end_ms FROM clip
         WHERE court_id = ?1 AND hidden = 0 AND kind = 'tap' AND made_at >= ?2 ORDER BY made_at DESC",
    )?;
    let rows = st.query_map(params![court, since], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (token, made, state, failure, start, end) = row?;
        let made = clock::parse(&made).ok_or_else(|| AppError::Internal("bad clip time".into()))?;
        if ch_core::on_court_page(made, now) {
            out.push(ClipLine { token, time: time_label(made), state, failure, seconds: ((end - start) as f64 / 1000.0).round() as i64 });
        }
    }
    Ok(out)
}

#[derive(Template)]
#[template(path = "court.html")]
struct CourtPage {
    venue: String,
    has_logo: bool,
    court: String,
    code: String,
    recording: bool,
    status: String,
    clips: Vec<ClipLine>,
    error: Option<&'static str>,
    panel: crate::games::Panel,
    judge_on: bool,
    /// Seconds kept after a tap, in ms: the saving ring starts with it the moment the player taps.
    after_ms: u32,
}

#[derive(Template)]
#[template(path = "court_game.html")]
struct GameFragment {
    code: String,
    recording: bool,
    judge_on: bool,
    panel: crate::games::Panel,
}

/// A court as the players' pages see it.
pub struct PlayerCourt {
    pub id: i64,
    pub code: String,
    pub recording: bool,
}

pub fn court_for_players(app: &AppState, code: &str) -> Result<PlayerCourt, AppError> {
    let court = court_by_code(&app.db, code)?;
    let recording = matches!(court_recording(app, &court), CourtStatus::Recording { .. });
    Ok(PlayerCourt { id: court.id, code: court.code, recording })
}

#[derive(Template)]
#[template(path = "court_clips.html")]
struct ClipsFragment {
    code: String,
    clips: Vec<ClipLine>,
}

fn player_status(s: &CourtStatus) -> (bool, String) {
    match s {
        CourtStatus::Recording { .. } => (true, "Recording".into()),
        CourtStatus::Starting => (false, "Camera starting — clips are available in a few seconds".into()),
        CourtStatus::Offline { .. } | CourtStatus::NoCamera => (false, "Camera offline — clips aren't available".into()),
        CourtStatus::Closed => (false, "Closed — clips are available during opening hours".into()),
    }
}

fn court_recording(app: &AppState, court: &Court) -> CourtStatus {
    app.recorders.status(court.id, court.has_camera)
}

#[derive(Deserialize)]
pub struct PageQuery {
    error: Option<String>,
}

pub async fn court_page(State(app): State<AppState>, Path(code): Path<String>, Query(q): Query<PageQuery>) -> Page {
    if let Some(elsewhere) = hosted_elsewhere(&app, &code)? {
        return Ok(Redirect::to(&elsewhere).into_response());
    }
    let court = court_by_code(&app.db, &code)?;
    let (venue, has_logo): (String, bool) =
        app.db.conn().query_row("SELECT name, logo IS NOT NULL FROM venue WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let (recording, status) = player_status(&court_recording(&app, &court));
    let now = clock::now();
    let (judge_on, after_s): (bool, u32) =
        app.db.conn().query_row("SELECT judge_on, clip_after_s FROM venue WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    html(CourtPage {
        venue,
        has_logo,
        recording,
        status,
        clips: clip_lines(&app.db, court.id, now)?,
        error: q.error.is_some().then_some("The camera isn't recording right now, so nothing was started or saved."),
        panel: crate::games::panel(&app, court.id, now)?,
        judge_on,
        after_ms: after_s * 1000,
        court: court.name,
        code: court.code,
    })
}

pub async fn court_game(State(app): State<AppState>, Path(code): Path<String>) -> Page {
    let court = court_by_code(&app.db, &code)?;
    let (recording, _) = player_status(&court_recording(&app, &court));
    let judge_on: bool = app.db.conn().query_row("SELECT judge_on FROM venue WHERE id = 1", [], |r| r.get(0))?;
    html(GameFragment { panel: crate::games::panel(&app, court.id, clock::now())?, code: court.code, recording, judge_on })
}

/// The court page of a court another box records, on that box (signs printed anywhere still work).
fn hosted_elsewhere(app: &AppState, code: &str) -> Result<Option<String>, AppError> {
    let host: Option<String> =
        app.db.conn().query_row("SELECT box FROM court WHERE code = ?1 AND deleted = 0", [code.to_ascii_uppercase()], |r| r.get(0)).optional()?;
    let Some(host) = host.filter(|h| *h != app.box_name) else { return Ok(None) };
    Ok(crate::sync::peers_snapshot(app)
        .into_iter()
        .find(|p| p.name == host && !p.player_url.is_empty())
        .map(|p| format!("{}/c/{}", p.player_url, code.to_ascii_uppercase())))
}

pub async fn court_clips(State(app): State<AppState>, Path(code): Path<String>) -> Page {
    let court = court_by_code(&app.db, &code)?;
    html(ClipsFragment { code: court.code, clips: clip_lines(&app.db, court.id, clock::now())? })
}

pub async fn venue_logo(State(app): State<AppState>) -> Result<Response, AppError> {
    let logo: Option<(Vec<u8>, String)> =
        app.db.conn().query_row("SELECT logo, logo_type FROM venue WHERE id = 1 AND logo IS NOT NULL", [], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let (bytes, mime) = logo.ok_or(AppError::NotFound)?;
    Ok(([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "no-cache".into())], bytes).into_response())
}

// ------------------------------------------------------------------ tap (FR-4, FR-5, FR-8)

fn wants_json(headers: &HeaderMap) -> bool {
    headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("application/json"))
}

pub async fn tap(State(app): State<AppState>, Path(code): Path<String>, headers: HeaderMap) -> Page {
    let court = court_by_code(&app.db, &code)?;
    if !matches!(court_recording(&app, &court), CourtStatus::Recording { .. }) {
        if wants_json(&headers) {
            let body = serde_json::json!({ "error": "The camera isn't recording right now, so no clip was saved." });
            return Ok((StatusCode::CONFLICT, axum::Json(body)).into_response());
        }
        return Ok(Redirect::to(&format!("/c/{}?error=not-recording", court.code)).into_response());
    }
    let now = clock::now();
    let (clip_id, end_ms, created) = record_tap(&app.db, court.id, now)?;
    if created {
        spawn_cut(app.clone(), clip_id);
    }
    if wants_json(&headers) {
        return Ok(axum::Json(serde_json::json!({ "ready_in_ms": (end_ms - ms(now)).max(0) })).into_response());
    }
    Ok(Redirect::to(&format!("/c/{}", court.code)).into_response())
}

/// Joins the court's open clip when the tap is within the merge gap of its last tap; otherwise starts a clip.
/// Returns (clip id, window end, whether a new clip was created).
fn record_tap(db: &Db, court: i64, now: Time) -> Result<(i64, i64, bool), AppError> {
    let conn = db.conn();
    let (before, after): (u32, u32) = conn.query_row("SELECT clip_before_s, clip_after_s FROM venue WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let settings = ClipSettings::new(before, after).unwrap_or(ClipSettings::DEFAULT);
    let open: Option<(i64, i64)> = conn
        .query_row(
            "SELECT id, last_tap_ms FROM clip WHERE court_id = ?1 AND kind = 'tap' AND state = 'saving'
             ORDER BY last_tap_ms DESC LIMIT 1",
            [court],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, last)) = open {
        // The golden-tested rule decides: two taps that plan to one window are one clip.
        if let [merged] = ch_core::plan_clips(&[at(last), now], settings)[..] {
            let end = ms(merged.end);
            conn.execute("UPDATE clip SET window_end_ms = ?1, last_tap_ms = ?2 WHERE id = ?3", params![end, ms(now), id])?;
            return Ok((id, end, false));
        }
    }
    let w = ch_core::plan_clips(&[now], settings)[0];
    conn.execute(
        "INSERT INTO clip (court_id, game_id, kind, token, window_start_ms, window_end_ms, last_tap_ms, made_at)
         VALUES (?1, (SELECT id FROM game WHERE court_id = ?1 AND stopped_at IS NULL), 'tap', ?2, ?3, ?4, ?5, ?6)",
        params![court, crate::auth::random_token(), ms(w.start), ms(w.end), ms(now), now.to_rfc3339()],
    )?;
    Ok((conn.last_insert_rowid(), ms(w.end), true))
}

// ------------------------------------------------------------------ cutting

/// Clips still saving when the box stopped are finished after it starts again (FR-56).
pub fn resume_saving(app: &AppState) -> rusqlite::Result<()> {
    let ids: Vec<i64> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id FROM clip WHERE state = 'saving'")?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    for id in ids {
        spawn_cut(app.clone(), id);
    }
    Ok(())
}

fn spawn_cut(app: AppState, clip: i64) {
    tokio::spawn(async move {
        if let Err(e) = cut_when_due(&app, clip).await {
            let message = match &e {
                AppError::BadRequest(m) => m.clone(),
                _ => "The box could not make this clip.".to_string(),
            };
            tracing::error!(clip, error = ?e, "clip failed");
            let _ = app.db.conn().execute("UPDATE clip SET state = 'failed', failure = ?1 WHERE id = ?2", params![message, clip]);
        }
    });
}

async fn cut_when_due(app: &AppState, clip: i64) -> Result<(), AppError> {
    loop {
        let row: Option<(i64, String, i64, i64)> = app
            .db
            .conn()
            .query_row("SELECT court_id, token, window_start_ms, window_end_ms FROM clip WHERE id = ?1 AND state = 'saving'", [clip], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .optional()?;
        let Some((court, token, start, end)) = row else { return Ok(()) };
        let wait = end - ms(clock::now());
        if wait > 0 {
            // Re-read at least every second: a merged tap moves the end (FR-5).
            tokio::time::sleep(Duration::from_millis(wait.clamp(50, 1000) as u64)).await;
            continue;
        }
        app.recorders.split_now(court);
        let segments = closed_segments(&app.db, court, start, end).await?;
        let dir = app.data.join("clips").join(&token);
        std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
        let (wide, vertical) = (dir.join("wide.mp4"), dir.join("vertical.mp4"));
        let p = plan(&segments, Window { start: at(start), end: at(end) })?;
        let (actual_start, actual_end) = (p.start, p.end);
        let outputs = vec![(Shape::Wide, wide.clone()), (Shape::Vertical, vertical.clone())];
        render(app, p, outputs).await?;
        thumbnail(clip, &wide, &dir).await;
        // Compare-and-set on the window: a tap that merged while cutting sends the loop round again.
        let done = app.db.conn().execute(
            "UPDATE clip SET state = 'ready', wide_path = ?1, vertical_path = ?2, window_start_ms = ?3
             WHERE id = ?4 AND window_end_ms = ?5 AND state = 'saving'",
            params![wide.to_string_lossy(), vertical.to_string_lossy(), actual_start, clip, end],
        )?;
        if done == 1 {
            tracing::info!(clip, court, seconds = (actual_end - actual_start) / 1000, "clip ready");
            return Ok(());
        }
    }
}

pub struct Seg {
    path: String,
    start: i64,
    end: i64,
    size: Size,
}

/// A wall-clock window mapped onto recorded footage. splitmuxsrc plays segments back to back, so offsets count
/// recorded time only: a gap (camera offline) takes no time in the output.
pub struct Plan {
    pub segments: Vec<String>,
    pub from_ms: u64,
    pub to_ms: u64,
    pub source: Size,
    /// The part of the window that has footage, wall clock.
    pub start: i64,
    pub end: i64,
}

/// Limits the window to footage that exists (FR-4, GV-7, GV-8).
pub fn plan(all: &[Seg], window: Window) -> Result<Plan, AppError> {
    let none = || AppError::BadRequest("No footage was recorded at that time.".into());
    let (ws, we) = (ms(window.start), ms(window.end));
    let run: Vec<&Seg> = all.iter().filter(|s| s.start < we && s.end > ws).collect();
    let (first, last) = (run.first().ok_or_else(none)?, run.last().ok_or_else(none)?);
    if first.size.w == 0 || first.size.h == 0 {
        return Err(AppError::Internal("segment has no frame size".into()));
    }
    let recorded = |t: i64| run.iter().map(|s| (t.min(s.end) - s.start).max(0)).sum::<i64>();
    let (start, end) = (ws.max(first.start), we.min(last.end));
    if start >= end {
        return Err(none());
    }
    Ok(Plan {
        segments: run.iter().map(|s| s.path.clone()).collect(),
        from_ms: recorded(start) as u64,
        to_ms: recorded(end) as u64,
        source: first.size,
        start,
        end,
    })
}

/// Encodes a plan into the given shapes with the venue's logos (blocking work off the async threads).
pub async fn render(app: &AppState, p: Plan, outputs: Vec<(Shape, PathBuf)>) -> Result<(), AppError> {
    let brand = brand(app)?;
    tokio::task::spawn_blocking(move || {
        let outs: Vec<Output> = outputs.iter().map(|(shape, path)| Output { shape: *shape, path }).collect();
        media::cut(&Cut { segments: p.segments, from_ms: p.from_ms, to_ms: p.to_ms, source: p.source, brand: &brand, outputs: outs })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

/// A missing thumbnail only costs the picture in the list; the video itself is still good.
pub async fn thumbnail(clip: i64, video: &std::path::Path, dir: &std::path::Path) {
    let still = video.to_string_lossy().into_owned();
    match tokio::task::spawn_blocking(move || media::still_jpeg(&still, 320, 180)).await {
        Ok(Ok(jpeg)) => {
            if let Err(e) = std::fs::write(dir.join("thumb.jpg"), jpeg) {
                tracing::warn!(clip, error = %e, "could not save clip thumbnail");
            }
        }
        Ok(Err(e)) => tracing::warn!(clip, error = ?e, "could not make clip thumbnail"),
        Err(e) => tracing::warn!(clip, error = %e, "thumbnail task failed"),
    }
}

/// Waits (≤ 15 s) until the footage up to `end` is in closed segment files, then returns the closed segments
/// overlapping [start, end].
pub async fn closed_segments(db: &Db, court: i64, start: i64, end: i64) -> Result<Vec<Seg>, AppError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let (covered, open_before_end): (bool, bool) = db.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM segment WHERE court_id = ?1 AND end_ms >= ?2),
                    EXISTS (SELECT 1 FROM segment WHERE court_id = ?1 AND end_ms IS NULL AND start_ms < ?2)",
            params![court, end],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if (covered && !open_before_end) || std::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let conn = db.conn();
    let mut st = conn.prepare(
        "SELECT path, start_ms, end_ms, COALESCE(width, 0), COALESCE(height, 0) FROM segment
         WHERE court_id = ?1 AND end_ms IS NOT NULL AND start_ms < ?2 AND end_ms > ?3 ORDER BY start_ms",
    )?;
    let rows =
        st.query_map(params![court, end, start], |r| Ok(Seg { path: r.get(0)?, start: r.get(1)?, end: r.get(2)?, size: Size { w: r.get(3)?, h: r.get(4)? } }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// The venue's logos as files the overlay can read; content-addressed so a new upload never races a running cut.
fn brand(app: &AppState) -> Result<Brand, AppError> {
    type Blob = (Option<Vec<u8>>, Option<String>);
    let (logo, sponsor, corner): (Blob, Blob, String) =
        app.db.conn().query_row("SELECT logo, logo_type, sponsor, sponsor_type, logo_corner FROM venue WHERE id = 1", [], |r| {
            Ok(((r.get(0)?, r.get(1)?), (r.get(2)?, r.get(3)?), r.get(4)?))
        })?;
    let file = |blob: Blob| -> Result<Option<Logo>, AppError> {
        let (Some(bytes), Some(mime)) = blob else { return Ok(None) };
        let Some((width, height)) = media::image_size(&bytes) else { return Ok(None) };
        let ext = if mime == "image/png" { "png" } else { "jpg" };
        let path: PathBuf = app.data.join("brand").join(format!("{}.{ext}", crate::auth::sha256_hex(&bytes)));
        if !path.exists() {
            std::fs::create_dir_all(app.data.join("brand")).map_err(|e| AppError::Internal(e.to_string()))?;
            std::fs::write(&path, &bytes).map_err(|e| AppError::Internal(e.to_string()))?;
        }
        Ok(Some(Logo { path, width, height }))
    };
    Ok(Brand { corner: Corner::parse(&corner), logo: file(logo)?, sponsor: file(sponsor)? })
}

// ------------------------------------------------------------------ one clip (FR-9, FR-19)

struct ClipRow {
    id: i64,
    made: Time,
    state: String,
    failure: String,
    wide: Option<String>,
    vertical: Option<String>,
    kind: String,
}

/// A clip is reachable from a court page only by its court's code, its own token and within 2 h (FR-18).
fn visible_clip(db: &Db, court: &Court, token: &str) -> Result<ClipRow, AppError> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::NotFound);
    }
    let row = db
        .conn()
        .query_row(
            "SELECT id, made_at, state, COALESCE(failure, ''), wide_path, vertical_path, kind FROM clip
             WHERE token = ?1 AND court_id = ?2 AND hidden = 0",
            params![token, court.id],
            |r| {
                Ok((
                    (r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?),
                    (r.get::<_, Option<String>>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, String>(6)?),
                ))
            },
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    let ((id, made, state, failure), (wide, vertical, kind)) = row;
    let made = clock::parse(&made).ok_or(AppError::NotFound)?;
    if !ch_core::on_court_page(made, clock::now()) {
        return Err(AppError::NotFound);
    }
    Ok(ClipRow { id, made, state, failure, wide, vertical, kind })
}

#[derive(Template)]
#[template(path = "clip.html")]
struct ClipPage {
    court: String,
    code: String,
    token: String,
    title: String,
    ready: bool,
    has_vertical: bool,
    failure: String,
}

pub async fn clip_page(State(app): State<AppState>, Path((code, token)): Path<(String, String)>) -> Page {
    let court = court_by_code(&app.db, &code)?;
    let clip = visible_clip(&app.db, &court, &token)?;
    let title = match clip.kind.as_str() {
        "game" => format!("Game video, {}", time_label(clip.made)),
        "reel" => "Highlights reel".to_string(),
        "highlight" => format!("Highlight, {}", time_label(clip.made)),
        _ => format!("Clip from {}", time_label(clip.made)),
    };
    html(ClipPage {
        court: court.name,
        code: court.code,
        title,
        ready: clip.state == "ready",
        has_vertical: clip.vertical.is_some(),
        failure: if clip.state == "failed" { clip.failure } else { String::new() },
        token,
    })
}

#[derive(Deserialize)]
pub struct FileQuery {
    download: Option<String>,
}

pub async fn clip_file(
    State(app): State<AppState>,
    Path((code, token, file)): Path<(String, String, String)>,
    Query(q): Query<FileQuery>,
    req: Request,
) -> Result<Response, AppError> {
    let court = court_by_code(&app.db, &code)?;
    let clip = visible_clip(&app.db, &court, &token)?;
    let (path, shape) = match file.as_str() {
        "wide.mp4" => (clip.wide.ok_or(AppError::NotFound)?, "wide"),
        "vertical.mp4" => (clip.vertical.ok_or(AppError::NotFound)?, "vertical"),
        "thumb.jpg" => (app.data.join("clips").join(&token).join("thumb.jpg").to_string_lossy().into_owned(), "thumb"),
        _ => return Err(AppError::NotFound),
    };
    // ServeFile answers Range requests, which iPhone Safari needs to play and save video.
    let mut res = match tower_http::services::ServeFile::new(&path).try_call(req).await {
        Ok(r) => r.into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(e.to_string())),
    };
    if res.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    if q.download.is_some() {
        let name = format!("{}-{}-{}.mp4", slug(&court.name), clip.made.format("%Y%m%d-%H%M%S"), shape);
        let value = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).map_err(|e| AppError::Internal(e.to_string()))?;
        res.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    }
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    Ok(res)
}

pub fn slug(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    out.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// A player hides a clip from the court page they are on (FR-9).
pub async fn hide(State(app): State<AppState>, Path((code, token)): Path<(String, String)>) -> Page {
    let court = court_by_code(&app.db, &code)?;
    let clip = visible_clip(&app.db, &court, &token)?;
    {
        let conn = app.db.conn();
        conn.execute("UPDATE clip SET hidden = 1 WHERE id = ?1", [clip.id])?;
    }
    tracing::info!(clip = clip.id, court = court.id, "clip hidden by a player");
    Ok(Redirect::to(&format!("/c/{}", court.code)).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: i64, end: i64) -> Seg {
        Seg { path: format!("{start}.mp4"), start, end, size: Size { w: 640, h: 360 } }
    }

    #[test]
    fn plans_count_recorded_time_across_gaps() {
        // 0–10 s and 10–20 s, then the camera was off until 30 s, then 30–40 s.
        let all = [seg(0, 10_000), seg(10_000, 20_000), seg(30_000, 40_000)];
        let p = plan(&all, Window { start: at(15_000), end: at(35_000) }).unwrap();
        assert_eq!(p.segments, ["10000.mp4", "30000.mp4"]);
        assert_eq!((p.from_ms, p.to_ms), (5_000, 15_000)); // 5 s into the first file, 5 s of the gap-free tail
        assert_eq!((p.start, p.end), (15_000, 35_000));
        // Clamped to the footage that exists (GV-8).
        let early = plan(&all, Window { start: at(-25_000), end: at(5_000) }).unwrap();
        assert_eq!((early.from_ms, early.to_ms, early.start), (0, 5_000, 0));
        assert!(plan(&all, Window { start: at(20_500), end: at(29_000) }).is_err());
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Court 2 — Main"), "court-2-main");
    }
}

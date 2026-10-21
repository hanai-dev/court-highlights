//! Courts and cameras (FR-30) and the box health shown on the console home (FR-36).

use crate::auth::Staff;
use crate::recorder::{CourtStatus, disk_use, temperature_c};
use crate::sync::{BoxHealth, CourtLive};
use crate::web::{AppError, AppState, Page, Shell, html, shell};
use crate::{clock, db};
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

pub fn time_label(t: ch_core::Time) -> String {
    t.format("%-I:%M %P").to_string()
}

pub fn status_label(s: &CourtStatus) -> (bool, String) {
    match s {
        CourtStatus::Recording { since } => (true, format!("Recording since {}", time_label(*since))),
        CourtStatus::Offline { since } => (false, format!("Camera offline since {}", time_label(*since))),
        CourtStatus::Starting => (true, "Starting".into()),
        CourtStatus::Closed => (true, "Closed — not recording outside opening hours".into()),
        CourtStatus::NoCamera => (false, "No camera".into()),
    }
}

pub struct CourtLine {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub hosted_by: String,
    pub ok: bool,
    pub status: String,
    pub last_clip: String,
}

/// A sibling not heard from for this long is shown as unreachable (3 sync rounds).
const UNREACHABLE_AFTER_S: i64 = 45;

fn this_box_health(app: &AppState) -> BoxHealth {
    let disk = disk_use(&app.data);
    BoxHealth {
        disk_warning: disk.as_ref().is_some_and(|d| d.used_pct >= ch_core::DISK_WARN_PCT),
        disk: disk.map_or("not available".into(), |d| format!("{} % used, {:.0} GB free", d.used_pct, d.free_gb)),
        temperature: temperature_c().map_or("not available".into(), |c| format!("{c:.0} °C")),
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// The courts this box records, live, and its own health — what it tells its siblings (FR-41).
pub fn local_live(app: &AppState) -> rusqlite::Result<(Vec<CourtLive>, BoxHealth)> {
    let rows: Vec<(i64, String, bool, Option<String>)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare(
            "SELECT c.id, c.code, c.camera_url IS NOT NULL, (SELECT MAX(made_at) FROM clip WHERE court_id = c.id AND kind = 'tap')
             FROM court c WHERE c.deleted = 0 AND c.box = ?1",
        )?;
        st.query_map([&app.box_name], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?
    };
    let live = rows
        .into_iter()
        .map(|(id, code, cam, last)| {
            let (ok, status) = status_label(&app.recorders.status(id, cam));
            CourtLive { code, ok, status, last_clip: last.and_then(|l| clock::parse(&l)).map_or_else(|| "None yet".into(), crate::library::when_label) }
        })
        .collect();
    Ok((live, this_box_health(app)))
}

pub struct BoxLine {
    pub name: String,
    pub this_box: bool,
    pub reachable: bool,
    pub since: String,
    pub health: BoxHealth,
}

fn reachable(p: &crate::sync::Peer) -> bool {
    p.last_ok.is_some_and(|t| (clock::now() - t).num_seconds() <= UNREACHABLE_AFTER_S)
}

/// Every box at the venue, this one first; siblings show their last report or "unreachable since …".
fn box_lines(app: &AppState) -> Vec<BoxLine> {
    let mut out = vec![BoxLine { name: app.box_name.clone(), this_box: true, reachable: true, since: String::new(), health: this_box_health(app) }];
    for p in crate::sync::peers_snapshot(app) {
        let up = reachable(&p);
        out.push(BoxLine { since: p.last_ok.map_or_else(String::new, time_label), name: p.name, this_box: false, reachable: up, health: p.health });
    }
    for addr in crate::sync::silent_peers(app) {
        out.push(BoxLine { name: addr, this_box: false, reachable: false, since: String::new(), health: BoxHealth::default() });
    }
    out
}

/// All the venue's courts: this box's live, the siblings' as last reported (FR-41).
fn court_lines(app: &AppState) -> Result<Vec<CourtLine>, AppError> {
    let rows: Vec<(i64, String, String, String)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, name, code, box FROM court WHERE deleted = 0 ORDER BY position, id")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?
    };
    let (mine, _) = local_live(app)?;
    let peers = crate::sync::peers_snapshot(app);
    Ok(rows
        .into_iter()
        .map(|(id, name, code, hosted_by)| {
            let local = hosted_by == app.box_name;
            let (ok, status, last_clip) = if local {
                mine.iter().find(|l| l.code == code).map_or((false, "No camera".into(), String::new()), |l| (l.ok, l.status.clone(), l.last_clip.clone()))
            } else {
                match peers.iter().find(|p| p.name == hosted_by) {
                    Some(p) if reachable(p) => {
                        p.live.iter().find(|l| l.code == code).map_or((false, "No camera".into(), String::new()), |l| (l.ok, l.status.clone(), l.last_clip.clone()))
                    }
                    Some(p) => (false, format!("Box unreachable since {}", p.last_ok.map_or_else(String::new, time_label)), String::new()),
                    None => (false, "Box unreachable".into(), String::new()),
                }
            };
            CourtLine { id, name, code, hosted_by, ok, status, last_clip }
        })
        .collect())
}

// ------------------------------------------------------------------ console home with health (FR-36, FR-41)

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    shell: Shell,
    courts: Vec<CourtLine>,
    boxes: Vec<BoxLine>,
    venue_wide: bool,
}

pub async fn home(State(app): State<AppState>, staff: Staff) -> Page {
    html(HomePage { shell: shell(&app.db, &staff, "home")?, courts: court_lines(&app)?, boxes: box_lines(&app), venue_wide: app.venue.is_some() })
}

// ------------------------------------------------------------------ courts list and form (FR-30)

#[derive(Template)]
#[template(path = "courts.html")]
struct CourtsPage {
    shell: Shell,
    courts: Vec<CourtLine>,
    notice: Option<&'static str>,
}

#[derive(Deserialize)]
pub struct NoticeQuery {
    notice: Option<String>,
}

pub async fn list(State(app): State<AppState>, staff: Staff, axum::extract::Query(q): axum::extract::Query<NoticeQuery>) -> Page {
    staff.require_owner()?;
    let notice = q.notice.as_deref().map(|n| match n {
        "deleted" => "Court deleted.",
        _ => "Saved.",
    });
    html(CourtsPage { shell: shell(&app.db, &staff, "courts")?, courts: court_lines(&app)?, notice })
}

#[derive(Template)]
#[template(path = "court_form.html")]
struct CourtForm {
    shell: Shell,
    id: Option<i64>,
    name: String,
    code: String,
    camera_url: String,
    camera_user: String,
    has_password: bool,
    has_camera: bool,
    status: String,
    error: Option<String>,
}

impl CourtForm {
    fn blank(shell: Shell) -> Self {
        Self { shell, id: None, name: String::new(), code: String::new(), camera_url: String::new(), camera_user: String::new(), has_password: false, has_camera: false, status: String::new(), error: None }
    }
}

pub async fn new_form(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    html(CourtForm::blank(shell(&app.db, &staff, "courts")?))
}

#[derive(Deserialize)]
pub struct CourtInput {
    name: String,
    camera_url: String,
    camera_user: String,
    camera_password: String,
}

fn validate(input: &CourtInput) -> Option<&'static str> {
    let url = input.camera_url.trim();
    if input.name.trim().is_empty() || input.name.len() > 40 {
        Some("Give the court a name (up to 40 characters).")
    } else if !url.is_empty() && !(url.starts_with("rtsp://") || url.starts_with("rtsps://")) {
        Some("The camera address starts with rtsp:// — check the camera's manual for its stream address.")
    } else if !url.is_empty() && url::Url::parse(url).is_err() {
        Some("That camera address is not valid.")
    } else if url.len() > 300 || input.camera_user.len() > 100 || input.camera_password.len() > 100 {
        Some("The camera address, username or password is too long.")
    } else {
        None
    }
}

pub fn court_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 8];
    getrandom::fill(&mut buf).expect("OS random source");
    buf.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

pub async fn create(State(app): State<AppState>, staff: Staff, Form(input): Form<CourtInput>) -> Page {
    staff.require_owner()?;
    if let Some(e) = validate(&input) {
        return html(CourtForm { name: input.name, camera_url: input.camera_url, camera_user: input.camera_user, error: Some(e.into()), ..CourtForm::blank(shell(&app.db, &staff, "courts")?) });
    }
    let url = input.camera_url.trim();
    let conn = app.db.conn();
    let position: i64 = conn.query_row("SELECT COALESCE(MAX(position), 0) + 1 FROM court", [], |r| r.get(0))?;
    conn.execute(
        "INSERT INTO court (name, code, position, camera_url, camera_user, camera_password, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            input.name.trim(),
            court_code(),
            position,
            (!url.is_empty()).then_some(url),
            input.camera_user.trim(),
            input.camera_password,
            clock::now().to_rfc3339()
        ],
    )?;
    db::log(&conn, Some(staff.id), &staff.username, "added court", input.name.trim(), "")?;
    Ok(Redirect::to("/console/courts?notice=saved").into_response())
}

pub async fn edit(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let row: (String, String, Option<String>, Option<String>, Option<String>, String) = app
        .db
        .conn()
        .query_row("SELECT name, code, camera_url, camera_user, camera_password, box FROM court WHERE id = ?1 AND deleted = 0", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })
        .optional()?
        .ok_or(AppError::NotFound)?;
    // Only the box that records the court can show its camera.
    let has_camera = row.2.is_some() && row.5 == app.box_name;
    let (_, status) = crate::courts::status_label(&app.recorders.status(id, has_camera));
    html(CourtForm {
        shell: shell(&app.db, &staff, "courts")?,
        id: Some(id),
        name: row.0,
        code: row.1,
        camera_url: row.2.unwrap_or_default(),
        camera_user: row.3.unwrap_or_default(),
        has_password: row.4.is_some_and(|p| !p.is_empty()),
        has_camera,
        status,
        error: None,
    })
}

pub async fn update(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>, Form(input): Form<CourtInput>) -> Page {
    staff.require_owner()?;
    if let Some(e) = validate(&input) {
        return Err(AppError::BadRequest(e.into()));
    }
    let url = input.camera_url.trim();
    let conn = app.db.conn();
    // An empty password field keeps the stored one (it is never shown back).
    let changed = conn.execute(
        "UPDATE court SET name = ?1, camera_url = ?2, camera_user = ?3,
           camera_password = CASE WHEN ?4 = '' THEN camera_password ELSE ?4 END WHERE id = ?5",
        params![input.name.trim(), (!url.is_empty()).then_some(url), input.camera_user.trim(), input.camera_password, id],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    db::log(&conn, Some(staff.id), &staff.username, "edited court", input.name.trim(), "")?;
    Ok(Redirect::to("/console/courts?notice=saved").into_response())
}

#[derive(Template)]
#[template(path = "confirm.html")]
struct ConfirmPage {
    shell: Shell,
    title: String,
    consequence: String,
    action: String,
    button: String,
    back: String,
}

pub async fn delete_confirm(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let name: String = app.db.conn().query_row("SELECT name FROM court WHERE id = ?1", [id], |r| r.get(0)).optional()?.ok_or(AppError::NotFound)?;
    html(ConfirmPage {
        shell: shell(&app.db, &staff, "courts")?,
        title: format!("Delete {name}?"),
        consequence: format!("{name} stops recording and its rolling footage is deleted. Its court code stops working, so take down its sign. This can't be undone."),
        action: format!("/console/courts/{id}/delete"),
        button: format!("Delete {name}"),
        back: format!("/console/courts/{id}"),
    })
}

pub async fn delete(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let (name, paths): (String, Vec<String>) = {
        let conn = app.db.conn();
        let name: String = conn.query_row("SELECT name FROM court WHERE id = ?1 AND deleted = 0", [id], |r| r.get(0)).optional()?.ok_or(AppError::NotFound)?;
        let mut st = conn.prepare("SELECT path FROM segment WHERE court_id = ?1")?;
        let paths = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        // A tombstone, so the deletion reaches the venue's other boxes (FR-42).
        conn.execute("DELETE FROM segment WHERE court_id = ?1", [id])?;
        conn.execute("UPDATE court SET deleted = 1, camera_url = NULL, camera_user = NULL, camera_password = NULL WHERE id = ?1", [id])?;
        db::log(&conn, Some(staff.id), &staff.username, "deleted court", &name, "")?;
        (name, paths)
    };
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
    tracing::info!(court = %name, "court deleted");
    Ok(Redirect::to("/console/courts?notice=deleted").into_response())
}

/// A still from the court's newest finished segment, for the setup preview (FR-30).
pub async fn preview(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Result<Response, AppError> {
    staff.require_owner()?;
    let path: Option<String> = app
        .db
        .conn()
        .query_row("SELECT path FROM segment WHERE court_id = ?1 AND end_ms IS NOT NULL ORDER BY start_ms DESC LIMIT 1", [id], |r| r.get(0))
        .optional()?;
    let Some(path) = path else { return Err(AppError::NotFound) };
    let jpeg = tokio::task::spawn_blocking(move || crate::media::still_jpeg(&path, 640, 360)).await.map_err(|e| AppError::Internal(e.to_string()))??;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "no-store")], jpeg).into_response())
}

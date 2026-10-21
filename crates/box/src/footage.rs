//! Footage requests (FR-32): staff find rolling footage by court and time, hold it against the 4-hour overwrite
//! (released by staff or after 60 days), the Owner exports it; a request for footage that is gone is recorded.

use crate::auth::Staff;
use crate::library::when_label;
use crate::web::{AppError, AppState, Page, Shell, html, shell};
use crate::{clock, db};
use askama::Template;
use axum::extract::{Form, Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use ch_core::Time;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

fn at(ms: i64) -> Time {
    chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&ch_core::manila())
}

fn span_label(court: &str, start: i64, end: i64) -> String {
    let (s, e) = (at(start), at(end));
    format!("{court} · {} – {}", when_label(s), e.format("%-I:%M %P"))
}

#[derive(Deserialize, Default)]
pub struct Search {
    court: Option<i64>,
    date: Option<String>,
    from: Option<String>,
    to: Option<String>,
    notice: Option<String>,
}

pub struct Found {
    pub court_id: i64,
    pub label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub minutes: i64,
    pub covered: bool,
}

pub struct HoldRow {
    pub id: i64,
    pub label: String,
    pub placed: String,
    pub releases: String,
    pub active: bool,
}

pub struct CourtOption {
    pub id: i64,
    pub name: String,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "footage.html")]
struct FootagePage {
    shell: Shell,
    courts: Vec<CourtOption>,
    date: String,
    from: String,
    to: String,
    found: Option<Found>,
    error: Option<&'static str>,
    holds: Vec<HoldRow>,
    notice: Option<&'static str>,
}

fn window(s: &Search) -> Option<(i64, i64)> {
    let day = chrono::NaiveDate::parse_from_str(s.date.as_deref()?, "%Y-%m-%d").ok()?;
    let t = |v: &str| {
        let time = chrono::NaiveTime::parse_from_str(v, "%H:%M").ok()?;
        day.and_time(time).and_local_timezone(ch_core::manila()).single().map(|t| t.timestamp_millis())
    };
    let (start, end) = (t(s.from.as_deref()?)?, t(s.to.as_deref()?)?);
    (start < end && end - start <= 4 * 3600 * 1000).then_some((start, end))
}

fn holds(app: &AppState) -> Result<Vec<HoldRow>, AppError> {
    let now = clock::now().to_rfc3339();
    let conn = app.db.conn();
    let mut st = conn.prepare(
        "SELECT h.id, ct.name, h.start_ms, h.end_ms, h.placed_by, h.placed_at, h.release_at, h.released_at IS NULL AND h.release_at > ?1
         FROM hold h JOIN court ct ON ct.id = h.court_id ORDER BY h.id DESC LIMIT 100",
    )?;
    let rows = st
        .query_map([now], |r| {
            let placed: String = r.get(5)?;
            let release: String = r.get(6)?;
            Ok(HoldRow {
                id: r.get(0)?,
                label: span_label(&r.get::<_, String>(1)?, r.get(2)?, r.get(3)?),
                placed: format!("{} by {}", clock::parse(&placed).map(when_label).unwrap_or(placed), r.get::<_, String>(4)?),
                releases: clock::parse(&release).map(crate::library::date_label).unwrap_or(release),
                active: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub async fn footage(State(app): State<AppState>, staff: Staff, Query(s): Query<Search>) -> Page {
    let courts: Vec<(i64, String)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, name FROM court ORDER BY position, id")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };
    let searched = s.court.is_some() || s.date.is_some();
    let mut error = None;
    let mut found = None;
    if searched {
        match (s.court.and_then(|c| courts.iter().find(|(id, _)| *id == c)), window(&s)) {
            (Some((id, name)), Some((start, end))) => {
                let covered_ms: i64 = app.db.conn().query_row(
                    "SELECT COALESCE(SUM(MIN(end_ms, ?3) - MAX(start_ms, ?2)), 0) FROM segment
                     WHERE court_id = ?1 AND end_ms IS NOT NULL AND start_ms < ?3 AND end_ms > ?2",
                    params![id, start, end],
                    |r| r.get(0),
                )?;
                found = Some(Found { court_id: *id, label: span_label(name, start, end), start_ms: start, end_ms: end, minutes: covered_ms / 60_000, covered: covered_ms > 0 });
            }
            _ => error = Some("Choose a court, a date and a time window of up to 4 hours."),
        }
    }
    let now = clock::now();
    html(FootagePage {
        shell: shell(&app.db, &staff, "footage")?,
        courts: courts.into_iter().map(|(id, name)| CourtOption { selected: Some(id) == s.court, id, name }).collect(),
        date: s.date.clone().unwrap_or_else(|| now.format("%Y-%m-%d").to_string()),
        from: s.from.clone().unwrap_or_default(),
        to: s.to.clone().unwrap_or_default(),
        found,
        error,
        holds: holds(&app)?,
        notice: s.notice.as_deref().map(|n| match n {
            "held" => "Footage held. It won't be overwritten until it is released.",
            "released" => "Hold released. The footage is overwritten once it is more than 4 hours old.",
            _ => "Recorded in the activity log.",
        }),
    })
}

#[derive(Deserialize)]
pub struct SpanForm {
    court: i64,
    start_ms: i64,
    end_ms: i64,
}

fn court_name(app: &AppState, id: i64) -> Result<String, AppError> {
    app.db.conn().query_row("SELECT name FROM court WHERE id = ?1", [id], |r| r.get(0)).optional()?.ok_or(AppError::NotFound)
}

pub async fn place_hold(State(app): State<AppState>, staff: Staff, Form(f): Form<SpanForm>) -> Page {
    if f.end_ms <= f.start_ms || f.end_ms - f.start_ms > 4 * 3600 * 1000 {
        return Err(AppError::BadRequest("A hold covers up to 4 hours of footage.".into()));
    }
    let name = court_name(&app, f.court)?;
    let now = clock::now();
    let conn = app.db.conn();
    conn.execute(
        "INSERT INTO hold (court_id, start_ms, end_ms, placed_at, placed_by, release_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![f.court, f.start_ms, f.end_ms, now.to_rfc3339(), staff.username, ch_core::hold_release_at(now).to_rfc3339()],
    )?;
    db::log(&conn, Some(staff.id), &staff.username, "held footage", &span_label(&name, f.start_ms, f.end_ms), "")?;
    Ok(Redirect::to("/console/footage?notice=held").into_response())
}

pub async fn record_missing(State(app): State<AppState>, staff: Staff, Form(f): Form<SpanForm>) -> Page {
    let name = court_name(&app, f.court)?;
    db::log(&app.db.conn(), Some(staff.id), &staff.username, "footage requested but no longer exists", &span_label(&name, f.start_ms, f.end_ms), "")?;
    Ok(Redirect::to("/console/footage?notice=missing").into_response())
}

struct Hold {
    court_id: i64,
    court: String,
    start_ms: i64,
    end_ms: i64,
}

fn hold(app: &AppState, id: i64) -> Result<Hold, AppError> {
    app.db
        .conn()
        .query_row(
            "SELECT h.court_id, ct.name, h.start_ms, h.end_ms FROM hold h JOIN court ct ON ct.id = h.court_id WHERE h.id = ?1",
            [id],
            |r| Ok(Hold { court_id: r.get(0)?, court: r.get(1)?, start_ms: r.get(2)?, end_ms: r.get(3)? }),
        )
        .optional()?
        .ok_or(AppError::NotFound)
}

pub async fn release(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    let h = hold(&app, id)?;
    let conn = app.db.conn();
    conn.execute(
        "UPDATE hold SET released_at = ?1, released_by = ?2 WHERE id = ?3 AND released_at IS NULL",
        params![clock::now().to_rfc3339(), staff.username, id],
    )?;
    db::log(&conn, Some(staff.id), &staff.username, "released hold", &span_label(&h.court, h.start_ms, h.end_ms), "")?;
    Ok(Redirect::to("/console/footage?notice=released").into_response())
}

/// The Owner exports held footage as one MP4 by stream copy (whole segments, so a few extra seconds at each end).
pub async fn export(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>, req: Request) -> Result<Response, AppError> {
    staff.require_owner()?;
    let h = hold(&app, id)?;
    let segments: Vec<String> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT path FROM segment WHERE court_id = ?1 AND end_ms IS NOT NULL AND start_ms < ?2 AND end_ms > ?3 ORDER BY start_ms")?;
        st.query_map(params![h.court_id, h.end_ms, h.start_ms], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    let label = span_label(&h.court, h.start_ms, h.end_ms);
    if segments.is_empty() {
        db::log(&app.db.conn(), Some(staff.id), &staff.username, "footage requested but no longer exists", &label, "export")?;
        return Err(AppError::BadRequest("None of this footage is on the box any more. That has been recorded in the activity log.".into()));
    }
    let dir = app.data.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let out = dir.join(format!("hold-{id}.mp4"));
    let target = out.clone();
    tokio::task::spawn_blocking(move || crate::media::export_copy(&segments, &target)).await.map_err(|e| AppError::Internal(e.to_string()))??;
    db::log(&app.db.conn(), Some(staff.id), &staff.username, "exported footage", &label, "")?;
    let mut res = tower_http::services::ServeFile::new(&out).try_call(req).await.map_err(|e| AppError::Internal(e.to_string()))?.into_response();
    if res.status() != StatusCode::OK && res.status() != StatusCode::PARTIAL_CONTENT {
        return Err(AppError::NotFound);
    }
    let name = format!("{}-{}-footage.mp4", crate::clips::slug(&h.court), at(h.start_ms).format("%Y%m%d-%H%M"));
    let value = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")).map_err(|e| AppError::Internal(e.to_string()))?;
    res.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    Ok(res)
}

/// Spans the 4-hour overwrite must skip: holds not released and not past their 60 days.
pub fn protected(db: &crate::db::Db, court: i64, now: Time) -> rusqlite::Result<Vec<ch_core::Window>> {
    let conn = db.conn();
    let mut st = conn.prepare("SELECT start_ms, end_ms FROM hold WHERE court_id = ?1 AND released_at IS NULL AND release_at > ?2")?;
    st.query_map(params![court, now.to_rfc3339()], |r| Ok(ch_core::Window { start: at(r.get(0)?), end: at(r.get(1)?) }))?.collect()
}

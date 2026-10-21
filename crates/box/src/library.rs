//! Clip library and activity log for staff (FR-31, FR-35, FR-9 staff delete).
// ponytail: this box's clips only; the venue-wide view arrives with venue sync (FR-42).

use crate::auth::Staff;
use crate::web::{AppError, AppState, Page, Shell, html, shell};
use crate::{clock, db};
use askama::Template;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

pub const PAGE_SIZE: i64 = 50;

pub fn when_label(t: ch_core::Time) -> String {
    t.format("%-d %b, %-I:%M %P").to_string()
}

pub fn date_label(t: ch_core::Time) -> String {
    t.format("%-d %b %Y").to_string()
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "highlight" => "Highlight",
        "reel" => "Highlights reel",
        "game" => "Game video",
        _ => "Clip",
    }
}

fn length_label(ms: i64) -> String {
    let s = (ms as f64 / 1000.0).round() as i64;
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min {:02} s", s / 60, s % 60),
        _ => format!("{} h {:02} min", s / 3600, s % 3600 / 60),
    }
}

// ------------------------------------------------------------------ filters

#[derive(Deserialize, Default)]
pub struct Filters {
    court: Option<String>,
    date: Option<String>,
    kind: Option<String>,
    kept: Option<String>,
    may_post: Option<String>,
    page: Option<i64>,
    notice: Option<String>,
}

/// Validated filters. Every value is checked before it reaches SQL or a link (all are bound as parameters too).
struct Active {
    court: Option<i64>,
    date: Option<chrono::NaiveDate>,
    kind: Option<&'static str>,
    kept: bool,
    may_post: bool,
    page: i64,
}

impl Active {
    fn from(f: &Filters) -> Self {
        Self {
            court: f.court.as_deref().and_then(|c| c.parse().ok()),
            date: f.date.as_deref().and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()),
            kind: match f.kind.as_deref() {
                Some("tap") => Some("tap"),
                Some("highlight") => Some("highlight"),
                Some("game") => Some("game"),
                _ => None,
            },
            kept: f.kept.is_some(),
            may_post: f.may_post.is_some(),
            page: f.page.unwrap_or(1).max(1),
        }
    }

    fn any(&self) -> bool {
        self.court.is_some() || self.date.is_some() || self.kind.is_some() || self.kept || self.may_post
    }

    fn query(&self, page: i64) -> String {
        let mut q = Vec::new();
        if let Some(c) = self.court {
            q.push(format!("court={c}"));
        }
        if let Some(d) = self.date {
            q.push(format!("date={d}"));
        }
        if let Some(k) = self.kind {
            q.push(format!("kind={k}"));
        }
        if self.kept {
            q.push("kept=1".into());
        }
        if self.may_post {
            q.push("may_post=1".into());
        }
        q.push(format!("page={page}"));
        q.join("&")
    }

    /// [start, end) of the chosen day in Manila time, as stored text.
    fn day(&self) -> (Option<String>, Option<String>) {
        let Some(d) = self.date else { return (None, None) };
        let at = |day: chrono::NaiveDate| day.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(ch_core::manila()).single()).map(|t| t.to_rfc3339());
        (at(d), d.succ_opt().and_then(at))
    }
}

const WHERE: &str = "FROM clip c JOIN court ct ON ct.id = c.court_id LEFT JOIN game g ON g.id = c.game_id
     WHERE c.state = 'ready'
       AND (?1 IS NULL OR c.court_id = ?1)
       AND (?2 IS NULL OR (c.made_at >= ?2 AND c.made_at < ?3))
       AND (?4 IS NULL OR c.kind = ?4 OR (?4 = 'highlight' AND c.kind = 'reel'))
       AND (?5 = 0 OR c.kept_at IS NOT NULL)
       AND (?6 = 0 OR g.may_post = 1)";

pub struct LibraryRow {
    pub id: i64,
    pub court: String,
    pub when: String,
    pub kind: &'static str,
    pub length: String,
    pub kept: Option<String>,
    pub may_post: bool,
    pub hidden: bool,
}

pub struct CourtOption {
    pub id: i64,
    pub name: String,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "library.html")]
struct LibraryPage {
    shell: Shell,
    rows: Vec<LibraryRow>,
    courts: Vec<CourtOption>,
    date: String,
    kind: &'static str,
    kept: bool,
    may_post: bool,
    filtered: bool,
    total: i64,
    first: i64,
    last: i64,
    prev: Option<String>,
    next: Option<String>,
    notice: Option<&'static str>,
}

pub async fn library(State(app): State<AppState>, staff: Staff, Query(f): Query<Filters>) -> Page {
    let a = Active::from(&f);
    let (from, to) = a.day();
    let (rows, total, courts) = {
        let conn = app.db.conn();
        let args = params![a.court, from, to, a.kind, a.kept, a.may_post];
        let total: i64 = conn.query_row(&format!("SELECT COUNT(*) {WHERE}"), args, |r| r.get(0))?;
        let mut st = conn.prepare(&format!(
            "SELECT c.id, ct.name, c.kind, c.made_at, c.window_end_ms - c.window_start_ms, c.kept_at, COALESCE(g.may_post, 0), c.hidden
             {WHERE} ORDER BY c.made_at DESC, c.id DESC LIMIT {PAGE_SIZE} OFFSET ?7"
        ))?;
        let rows = st
            .query_map(params![a.court, from, to, a.kind, a.kept, a.may_post, (a.page - 1) * PAGE_SIZE], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, i64>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, bool>(6)?, r.get::<_, bool>(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut cs = conn.prepare("SELECT id, name FROM court ORDER BY position, id")?;
        let courts = cs
            .query_map([], |r| Ok(CourtOption { id: r.get(0)?, name: r.get(1)?, selected: false }))?
            .collect::<Result<Vec<_>, _>>()?;
        (rows, total, courts)
    };
    let rows = rows
        .into_iter()
        .map(|(id, court, kind, made, length, kept, may_post, hidden)| LibraryRow {
            id,
            court,
            when: clock::parse(&made).map(when_label).unwrap_or(made),
            kind: kind_label(&kind),
            length: length_label(length),
            kept: kept.and_then(|k| clock::parse(&k)).map(|k| date_label(ch_core::keep_expires_at(k))),
            may_post,
            hidden,
        })
        .collect::<Vec<_>>();
    let first = if total == 0 { 0 } else { (a.page - 1) * PAGE_SIZE + 1 };
    let last = (first + rows.len() as i64 - 1).max(0);
    html(LibraryPage {
        shell: shell(&app.db, &staff, "clips")?,
        courts: courts.into_iter().map(|c| CourtOption { selected: Some(c.id) == a.court, ..c }).collect(),
        date: a.date.map(|d| d.to_string()).unwrap_or_default(),
        kind: a.kind.unwrap_or(""),
        kept: a.kept,
        may_post: a.may_post,
        filtered: a.any(),
        total,
        first,
        last,
        prev: (a.page > 1).then(|| a.query(a.page - 1)),
        next: (last < total).then(|| a.query(a.page + 1)),
        notice: f.notice.as_deref().map(|n| match n {
            "deleted" => "Clip deleted.",
            _ => "Kept for 1 year.",
        }),
        rows,
    })
}

// ------------------------------------------------------------------ one clip: keep, files, delete

struct Item {
    court: String,
    kind: String,
    made: ch_core::Time,
    may_post: bool,
    token: String,
    wide: Option<String>,
    vertical: Option<String>,
}

impl Item {
    fn label(&self) -> String {
        format!("{} · {} · {}", self.court, when_label(self.made), kind_label(&self.kind))
    }
}

fn item(app: &AppState, id: i64) -> Result<Item, AppError> {
    let row = app
        .db
        .conn()
        .query_row(
            "SELECT ct.name, c.kind, c.made_at, COALESCE(g.may_post, 0), c.token, c.wide_path, c.vertical_path
             FROM clip c JOIN court ct ON ct.id = c.court_id LEFT JOIN game g ON g.id = c.game_id WHERE c.id = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?
        .ok_or(AppError::NotFound)?;
    Ok(Item { court: row.0, kind: row.1, made: clock::parse(&row.2).ok_or(AppError::NotFound)?, may_post: row.3, token: row.4, wide: row.5, vertical: row.6 })
}

pub async fn keep(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    let it = item(&app, id)?;
    let conn = app.db.conn();
    conn.execute("UPDATE clip SET kept_at = ?1 WHERE id = ?2 AND kept_at IS NULL", params![clock::now().to_rfc3339(), id])?;
    db::log(&conn, Some(staff.id), &staff.username, "kept", &it.label(), "")?;
    Ok(Redirect::to("/console/clips?notice=kept").into_response())
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
    let it = item(&app, id)?;
    html(ConfirmPage {
        shell: shell(&app.db, &staff, "clips")?,
        title: format!("Delete this {}?", kind_label(&it.kind).to_lowercase()),
        consequence: format!("{} is removed from the court page and the library and is never sent. It can't be recovered.", it.label()),
        action: format!("/console/clips/{id}/delete"),
        button: "Delete".into(),
        back: "/console/clips".into(),
    })
}

pub async fn delete(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    let it = item(&app, id)?;
    {
        let conn = app.db.conn();
        conn.execute("DELETE FROM clip WHERE id = ?1", [id])?;
        db::log(&conn, Some(staff.id), &staff.username, "deleted", &it.label(), "")?;
    }
    let dir = app.data.join("clips").join(&it.token);
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!(clip = id, error = %e, "deleted clip files could not be removed");
    }
    Ok(Redirect::to("/console/clips?notice=deleted").into_response())
}

#[derive(Template)]
#[template(path = "posting_warning.html")]
struct PostingWarning {
    shell: Shell,
    label: String,
    href: String,
}

#[derive(Deserialize)]
pub struct FileQuery {
    posting: Option<String>,
    confirm: Option<String>,
}

pub async fn file(
    State(app): State<AppState>,
    staff: Staff,
    Path((id, file)): Path<(i64, String)>,
    Query(q): Query<FileQuery>,
    req: Request,
) -> Result<Response, AppError> {
    let it = item(&app, id)?;
    let path = match file.as_str() {
        "wide.mp4" => it.wide.clone().ok_or(AppError::NotFound)?,
        "vertical.mp4" => it.vertical.clone().ok_or(AppError::NotFound)?,
        "thumb.jpg" => app.data.join("clips").join(&it.token).join("thumb.jpg").to_string_lossy().into_owned(),
        _ => return Err(AppError::NotFound),
    };
    if q.posting.is_some() && file.ends_with(".mp4") {
        // The players said no (or nobody asked): warn first, and log the choice either way (FR-31).
        if !it.may_post && q.confirm.is_none() {
            return html(PostingWarning {
                shell: shell(&app.db, &staff, "clips")?,
                label: it.label(),
                href: format!("/console/clips/{id}/{file}?posting=1&confirm=1"),
            });
        }
        let detail = if it.may_post { "players allowed posting" } else { "players did NOT allow posting" };
        db::log(&app.db.conn(), Some(staff.id), &staff.username, "downloaded for posting", &it.label(), detail)?;
    }
    let mut res = match tower_http::services::ServeFile::new(&path).try_call(req).await {
        Ok(r) => r.into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(e.to_string())),
    };
    if res.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    if file.ends_with(".mp4") {
        let name = format!("{}-{}-{}", crate::clips::slug(&it.court), it.made.format("%Y%m%d-%H%M%S"), file.replace(".mp4", ""));
        let value = HeaderValue::from_str(&format!("attachment; filename=\"{name}.mp4\"")).map_err(|e| AppError::Internal(e.to_string()))?;
        res.headers_mut().insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(res)
}

// ------------------------------------------------------------------ activity log (FR-35)

pub struct LogRow {
    pub at: String,
    pub who: String,
    pub action: String,
    pub item: String,
    pub detail: String,
}

#[derive(Template)]
#[template(path = "log.html")]
struct LogPage {
    shell: Shell,
    rows: Vec<LogRow>,
    total: i64,
    first: i64,
    last: i64,
    prev: Option<i64>,
    next: Option<i64>,
}

#[derive(Deserialize)]
pub struct PageQuery {
    page: Option<i64>,
}

fn log_rows(app: &AppState, limit: i64, offset: i64) -> Result<Vec<LogRow>, AppError> {
    let conn = app.db.conn();
    let mut st = conn.prepare("SELECT at, actor, action, item, detail FROM activity ORDER BY id DESC LIMIT ?1 OFFSET ?2")?;
    let rows = st
        .query_map(params![limit, offset], |r| {
            let at: String = r.get(0)?;
            Ok(LogRow { at: clock::parse(&at).map(|t| t.format("%-d %b %Y, %-I:%M:%S %P").to_string()).unwrap_or(at), who: r.get(1)?, action: r.get(2)?, item: r.get(3)?, detail: r.get(4)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub async fn log(State(app): State<AppState>, staff: Staff, Query(q): Query<PageQuery>) -> Page {
    staff.require_owner()?;
    let page = q.page.unwrap_or(1).max(1);
    let total: i64 = app.db.conn().query_row("SELECT COUNT(*) FROM activity", [], |r| r.get(0))?;
    let rows = log_rows(&app, PAGE_SIZE, (page - 1) * PAGE_SIZE)?;
    let first = if total == 0 { 0 } else { (page - 1) * PAGE_SIZE + 1 };
    let last = (first + rows.len() as i64 - 1).max(0);
    html(LogPage { shell: shell(&app.db, &staff, "log")?, rows, total, first, last, prev: (page > 1).then_some(page - 1), next: (last < total).then_some(page + 1) })
}

/// A spreadsheet cell: quoted, and text that a spreadsheet would run as a formula is defused (CSV injection).
fn csv_cell(s: &str) -> String {
    let s = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() };
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub async fn log_csv(State(app): State<AppState>, staff: Staff) -> Result<Response, AppError> {
    staff.require_owner()?;
    let mut out = String::from("time,who,action,item,detail\r\n");
    for r in log_rows(&app, i64::MAX, 0)? {
        out += &[r.at, r.who, r.action, r.item, r.detail].iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",");
        out += "\r\n";
    }
    db::log(&app.db.conn(), Some(staff.id), &staff.username, "exported activity log", "", "")?;
    Ok((
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"activity-log.csv\"")],
        out,
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_cells_are_quoted_and_defused() {
        assert_eq!(csv_cell("Court 2"), "\"Court 2\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("-1"), "\"'-1\"");
    }

    #[test]
    fn lengths() {
        assert_eq!(length_label(30_400), "30 s");
        assert_eq!(length_label(95_000), "1 min 35 s");
        assert_eq!(length_label(3_725_000), "1 h 02 min");
    }
}

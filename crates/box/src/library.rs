//! Clip library and activity log for staff (FR-31, FR-35, FR-9 staff delete).
// ponytail: this box's clips only; the venue-wide view arrives with venue sync (FR-42).

use crate::auth::Staff;
use crate::web::{AppError, AppState, Page, Shell, html, shell};
use crate::{clock, db};
use askama::Template;
use axum::extract::{Form, Path, Query, Request, State};
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
/// The court is chosen by its code, which is the same on every box at the venue.
pub struct Active {
    court: Option<String>,
    date: Option<chrono::NaiveDate>,
    kind: Option<&'static str>,
    kept: bool,
    may_post: bool,
    page: i64,
}

impl Active {
    fn from(f: &Filters) -> Self {
        Self {
            court: f.court.as_deref().filter(|c| c.len() == 8 && c.bytes().all(|b| b.is_ascii_alphanumeric())).map(str::to_ascii_uppercase),
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
        let mut q = self.filter_query();
        q.push(format!("page={page}"));
        q.join("&")
    }

    /// The filters alone, for links and for asking sibling boxes the same question.
    fn filter_query(&self) -> Vec<String> {
        let mut q = Vec::new();
        if let Some(c) = &self.court {
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
        q
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
       AND (?1 IS NULL OR ct.code = ?1)
       AND (?2 IS NULL OR (c.made_at >= ?2 AND c.made_at < ?3))
       AND (?4 IS NULL OR c.kind = ?4 OR (?4 = 'highlight' AND c.kind = 'reel'))
       AND (?5 = 0 OR c.kept_at IS NOT NULL)
       AND (?6 = 0 OR g.may_post = 1)";

/// One library row as boxes exchange it.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Row {
    pub id: i64,
    pub box_name: String,
    pub court: String,
    pub made: String,
    pub made_ms: i64,
    pub kind: String,
    pub length_ms: i64,
    pub kept_at: Option<String>,
    pub may_post: bool,
    pub hidden: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Rows {
    pub rows: Vec<Row>,
    pub total: i64,
}

/// This box's matching items, newest first, at most `limit` of them (a bounded number of queries, no N+1).
fn local_rows(app: &AppState, a: &Active, limit: i64, offset: i64) -> Result<Rows, AppError> {
    let (from, to) = a.day();
    let conn = app.db.conn();
    let total: i64 = conn.query_row(&format!("SELECT COUNT(*) {WHERE}"), params![a.court, from, to, a.kind, a.kept, a.may_post], |r| r.get(0))?;
    let mut st = conn.prepare(&format!(
        "SELECT c.id, ct.name, c.kind, c.made_at, c.window_end_ms - c.window_start_ms, c.kept_at, COALESCE(g.may_post, 0), c.hidden
         {WHERE} ORDER BY c.made_at DESC, c.id DESC LIMIT ?7 OFFSET ?8"
    ))?;
    let rows = st
        .query_map(params![a.court, from, to, a.kind, a.kept, a.may_post, limit, offset], |r| {
            let made: String = r.get(3)?;
            Ok(Row {
                id: r.get(0)?,
                box_name: app.box_name.clone(),
                court: r.get(1)?,
                made_ms: clock::parse(&made).map_or(0, |t| t.timestamp_millis()),
                made,
                kind: r.get(2)?,
                length_ms: r.get(4)?,
                kept_at: r.get(5)?,
                may_post: r.get(6)?,
                hidden: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Rows { rows, total })
}

pub struct LibraryRow {
    pub id: i64,
    /// Where actions go: "/console/clips/{id}" here, "/console/clips/peer/{box}/{id}" on a sibling.
    pub base: String,
    pub box_name: String,
    pub court: String,
    pub when: String,
    pub kind: &'static str,
    pub length: String,
    pub kept: Option<String>,
    pub may_post: bool,
    pub hidden: bool,
}

pub struct CourtOption {
    pub code: String,
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
    /// This page's own query (filters and page number), carried by Keep so the list comes back the same.
    here: String,
    prev: Option<String>,
    next: Option<String>,
    notice: Option<&'static str>,
    venue_wide: bool,
    missing: Vec<String>,
}

pub async fn library(State(app): State<AppState>, staff: Staff, Query(f): Query<Filters>) -> Page {
    let a = Active::from(&f);
    // Across the venue's boxes (FR-41): each answers the first `page × 50` of its matches; merged newest first.
    let window = a.page * PAGE_SIZE;
    let mut all = local_rows(&app, &a, window, 0)?;
    let mut missing = Vec::new();
    let ask = format!("/sync/clips?{}", a.filter_query().into_iter().chain([format!("limit={window}")]).collect::<Vec<_>>().join("&"));
    for (url, name) in crate::remote::reachable(&app) {
        match crate::remote::get_sealed::<Rows>(&app, &url, &ask).await {
            Ok(r) => {
                all.total += r.total;
                all.rows.extend(r.rows);
            }
            Err(e) => {
                tracing::warn!(peer = %name, error = ?e, "library: sibling box did not answer");
                missing.push(name);
            }
        }
    }
    missing.extend(crate::sync::peers_snapshot(&app).into_iter().filter(|p| !crate::remote::reachable(&app).iter().any(|(_, n)| *n == p.name)).map(|p| p.name));
    all.rows.sort_by(|x, y| y.made_ms.cmp(&x.made_ms).then_with(|| x.box_name.cmp(&y.box_name)).then(y.id.cmp(&x.id)));
    let total = all.total;
    let courts: Vec<CourtOption> = {
        let conn = app.db.conn();
        let mut cs = conn.prepare("SELECT code, name FROM court WHERE deleted = 0 ORDER BY position, id")?;
        cs.query_map([], |r| Ok(CourtOption { code: r.get(0)?, name: r.get(1)?, selected: false }))?.collect::<Result<Vec<_>, _>>()?
    };
    let rows = all
        .rows
        .into_iter()
        .skip(((a.page - 1) * PAGE_SIZE) as usize)
        .take(PAGE_SIZE as usize)
        .map(|r| LibraryRow {
            base: if r.box_name == app.box_name { format!("/console/clips/{}", r.id) } else { format!("/console/clips/peer/{}/{}", r.box_name, r.id) },
            id: r.id,
            box_name: r.box_name,
            court: r.court,
            when: clock::parse(&r.made).map(when_label).unwrap_or(r.made),
            kind: kind_label(&r.kind),
            length: length_label(r.length_ms),
            kept: r.kept_at.and_then(|k| clock::parse(&k)).map(|k| date_label(ch_core::keep_expires_at(k))),
            may_post: r.may_post,
            hidden: r.hidden,
        })
        .collect::<Vec<_>>();
    let first = if total == 0 { 0 } else { (a.page - 1) * PAGE_SIZE + 1 };
    let last = (first + rows.len() as i64 - 1).max(0);
    html(LibraryPage {
        shell: shell(&app.db, &staff, "clips")?,
        courts: courts.into_iter().map(|c| CourtOption { selected: Some(&c.code) == a.court.as_ref(), ..c }).collect(),
        date: a.date.map(|d| d.to_string()).unwrap_or_default(),
        kind: a.kind.unwrap_or(""),
        kept: a.kept,
        may_post: a.may_post,
        filtered: a.any(),
        total,
        first,
        last,
        here: a.query(a.page),
        prev: (a.page > 1).then(|| a.query(a.page - 1)),
        next: (last < total).then(|| a.query(a.page + 1)),
        notice: f.notice.as_deref().map(|n| match n {
            "deleted" => "Clip deleted.",
            _ => "Kept for 1 year.",
        }),
        venue_wide: app.venue.is_some(),
        missing,
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

fn do_keep(app: &AppState, id: i64, actor_id: Option<i64>, actor: &str) -> Result<(), AppError> {
    let it = item(app, id)?;
    let conn = app.db.conn();
    conn.execute("UPDATE clip SET kept_at = ?1 WHERE id = ?2 AND kept_at IS NULL", params![clock::now().to_rfc3339(), id])?;
    db::log(&conn, actor_id, actor, "kept", &it.label(), "")?;
    Ok(())
}

/// The filtered page a Keep was pressed on, so the list stays as the user set it (UX-LIST-ARCHETYPE).
#[derive(Deserialize)]
pub struct Back {
    #[serde(default)]
    back: String,
}

impl Back {
    /// Only the library's own query characters; anything else falls back to the unfiltered list.
    fn to(&self, notice: &str) -> String {
        let safe = self.back.bytes().all(|b| b.is_ascii_alphanumeric() || b"=&_-".contains(&b));
        if safe && !self.back.is_empty() { format!("/console/clips?{}&notice={notice}", self.back) } else { format!("/console/clips?notice={notice}") }
    }
}

pub async fn keep(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>, Form(back): Form<Back>) -> Page {
    do_keep(&app, id, Some(staff.id), &staff.username)?;
    Ok(Redirect::to(&back.to("kept")).into_response())
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
    do_delete(&app, id, Some(staff.id), &staff.username)?;
    Ok(Redirect::to("/console/clips?notice=deleted").into_response())
}

fn do_delete(app: &AppState, id: i64, actor_id: Option<i64>, actor: &str) -> Result<(), AppError> {
    let it = item(app, id)?;
    {
        let conn = app.db.conn();
        conn.execute("DELETE FROM clip WHERE id = ?1", [id])?;
        db::log(&conn, actor_id, actor, "deleted", &it.label(), "")?;
    }
    let dir = app.data.join("clips").join(&it.token);
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!(clip = id, error = %e, "deleted clip files could not be removed");
    }
    Ok(())
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
        log_posting(&app, &it, Some(staff.id), &staff.username)?;
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

fn log_posting(app: &AppState, it: &Item, actor_id: Option<i64>, actor: &str) -> Result<(), AppError> {
    let detail = if it.may_post { "players allowed posting" } else { "players did NOT allow posting" };
    db::log(&app.db.conn(), actor_id, actor, "downloaded for posting", &it.label(), detail)?;
    Ok(())
}

/// The file to serve for one of this box's clips.
fn file_path(app: &AppState, it: &Item, file: &str) -> Result<String, AppError> {
    match file {
        "wide.mp4" => it.wide.clone().ok_or(AppError::NotFound),
        "vertical.mp4" => it.vertical.clone().ok_or(AppError::NotFound),
        "thumb.jpg" => Ok(app.data.join("clips").join(&it.token).join("thumb.jpg").to_string_lossy().into_owned()),
        _ => Err(AppError::NotFound),
    }
}

fn download_name(it: &Item, file: &str) -> String {
    format!("{}-{}-{}.mp4", crate::clips::slug(&it.court), it.made.format("%Y%m%d-%H%M%S"), file.replace(".mp4", ""))
}

// ------------------------------------------------------------------ a sibling box's clips, through this console (FR-41)

fn sibling(app: &AppState, name: &str) -> Result<String, AppError> {
    crate::remote::peer_url(app, name).ok_or_else(|| AppError::BadRequest(format!("{name} can't be reached right now. Try again in a minute.")))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Meta {
    label: String,
    kind: String,
    may_post: bool,
}

pub async fn peer_keep(State(app): State<AppState>, staff: Staff, Path((name, id)): Path<(String, i64)>, Form(back): Form<Back>) -> Page {
    let url = sibling(&app, &name)?;
    crate::remote::post(&app, &url, &format!("/sync/clips/{id}/keep?actor={}", crate::remote::enc(&staff.username))).await?;
    Ok(Redirect::to(&back.to("kept")).into_response())
}

pub async fn peer_delete_confirm(State(app): State<AppState>, staff: Staff, Path((name, id)): Path<(String, i64)>) -> Page {
    let url = sibling(&app, &name)?;
    let m: Meta = crate::remote::get_sealed(&app, &url, &format!("/sync/clips/{id}/meta")).await?;
    html(ConfirmPage {
        shell: shell(&app.db, &staff, "clips")?,
        title: format!("Delete this {}?", kind_label(&m.kind).to_lowercase()),
        consequence: format!("{} is removed from the court page and the library and is never sent. It can't be recovered.", m.label),
        action: format!("/console/clips/peer/{name}/{id}/delete"),
        button: "Delete".into(),
        back: "/console/clips".into(),
    })
}

pub async fn peer_delete(State(app): State<AppState>, staff: Staff, Path((name, id)): Path<(String, i64)>) -> Page {
    let url = sibling(&app, &name)?;
    crate::remote::post(&app, &url, &format!("/sync/clips/{id}/delete?actor={}", crate::remote::enc(&staff.username))).await?;
    Ok(Redirect::to("/console/clips?notice=deleted").into_response())
}

pub async fn peer_file(
    State(app): State<AppState>,
    staff: Staff,
    Path((name, id, file)): Path<(String, i64, String)>,
    Query(q): Query<FileQuery>,
) -> Result<Response, AppError> {
    let url = sibling(&app, &name)?;
    let mut path = format!("/sync/clips/{id}/{}", crate::remote::enc(&file));
    if q.posting.is_some() && file.ends_with(".mp4") {
        let m: Meta = crate::remote::get_sealed(&app, &url, &format!("/sync/clips/{id}/meta")).await?;
        if !m.may_post && q.confirm.is_none() {
            return html(PostingWarning {
                shell: shell(&app.db, &staff, "clips")?,
                label: m.label,
                href: format!("/console/clips/peer/{name}/{id}/{file}?posting=1&confirm=1"),
            });
        }
        path = format!("{path}?posting=1&actor={}", crate::remote::enc(&staff.username));
    }
    let res = crate::remote::get(&app, &url, &path).await?;
    let mut out = Response::builder().status(StatusCode::OK);
    for h in [header::CONTENT_TYPE, header::CONTENT_DISPOSITION, header::CONTENT_LENGTH] {
        if let Some(v) = res.headers().get(&h) {
            out = out.header(h, v);
        }
    }
    out.body(axum::body::Body::from_stream(res.bytes_stream())).map_err(|e| AppError::Internal(e.to_string()))
}

// ------------------------------------------------------------------ what this box answers its siblings (sync listener)

#[derive(Deserialize)]
pub struct SyncList {
    #[serde(flatten)]
    filters: Filters,
    limit: Option<i64>,
}

#[derive(Deserialize)]
pub struct Actor {
    actor: Option<String>,
    posting: Option<String>,
}

fn actor_name(a: &Actor, peer: &str) -> String {
    format!("{} (via {peer})", a.actor.as_deref().unwrap_or("staff").chars().take(40).collect::<String>())
}

pub async fn sync_clips(State(app): State<AppState>, req: Request) -> Response {
    let Some(venue) = crate::sync::authorized(&app, &req) else { return StatusCode::UNAUTHORIZED.into_response() };
    let Ok(Query(q)) = Query::<SyncList>::try_from_uri(req.uri()) else { return StatusCode::BAD_REQUEST.into_response() };
    let a = Active::from(&q.filters);
    match local_rows(&app, &a, q.limit.unwrap_or(PAGE_SIZE).clamp(1, 50 * PAGE_SIZE), 0)
        .and_then(|r| serde_json::to_vec(&r).map_err(|e| AppError::Internal(e.to_string())))
    {
        Ok(body) => venue.seal(&body).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn sync_meta(State(app): State<AppState>, Path(id): Path<i64>, req: Request) -> Response {
    let Some(venue) = crate::sync::authorized(&app, &req) else { return StatusCode::UNAUTHORIZED.into_response() };
    match item(&app, id).and_then(|it| {
        serde_json::to_vec(&Meta { label: it.label(), kind: it.kind.clone(), may_post: it.may_post }).map_err(|e| AppError::Internal(e.to_string()))
    }) {
        Ok(body) => venue.seal(&body).into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn sync_keep(State(app): State<AppState>, Path(id): Path<i64>, req: Request) -> Response {
    let Some(_) = crate::sync::authorized(&app, &req) else { return StatusCode::UNAUTHORIZED.into_response() };
    let Ok(Query(a)) = Query::<Actor>::try_from_uri(req.uri()) else { return StatusCode::BAD_REQUEST.into_response() };
    match do_keep(&app, id, None, &actor_name(&a, "another box")) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn sync_delete(State(app): State<AppState>, Path(id): Path<i64>, req: Request) -> Response {
    let Some(_) = crate::sync::authorized(&app, &req) else { return StatusCode::UNAUTHORIZED.into_response() };
    let Ok(Query(a)) = Query::<Actor>::try_from_uri(req.uri()) else { return StatusCode::BAD_REQUEST.into_response() };
    match do_delete(&app, id, None, &actor_name(&a, "another box")) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => e.into_response(),
    }
}

pub async fn sync_file(State(app): State<AppState>, Path((id, file)): Path<(i64, String)>, req: Request) -> Response {
    let Some(_) = crate::sync::authorized(&app, &req) else { return StatusCode::UNAUTHORIZED.into_response() };
    let Ok(Query(a)) = Query::<Actor>::try_from_uri(req.uri()) else { return StatusCode::BAD_REQUEST.into_response() };
    let it = match item(&app, id) {
        Ok(it) => it,
        Err(e) => return e.into_response(),
    };
    let path = match file_path(&app, &it, &file) {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if a.posting.is_some()
        && let Err(e) = log_posting(&app, &it, None, &actor_name(&a, "another box"))
    {
        return e.into_response();
    }
    let mut res = match tower_http::services::ServeFile::new(&path).try_call(req).await {
        Ok(r) => r.into_response(),
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if file.ends_with(".mp4")
        && let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", download_name(&it, &file)))
    {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    res
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
            Ok(LogRow {
                at: clock::parse(&at).map(|t| t.format("%-d %b %Y, %-I:%M:%S %P").to_string()).unwrap_or(at),
                who: r.get(1)?,
                action: r.get(2)?,
                item: r.get(3)?,
                detail: r.get(4)?,
            })
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
    html(LogPage {
        shell: shell(&app.db, &staff, "log")?,
        rows,
        total,
        first,
        last,
        prev: (page > 1).then_some(page - 1),
        next: (last < total).then_some(page + 1),
    })
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
    Ok(([(header::CONTENT_TYPE, "text/csv; charset=utf-8"), (header::CONTENT_DISPOSITION, "attachment; filename=\"activity-log.csv\"")], out).into_response())
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

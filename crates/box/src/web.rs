//! HTTP surface of the box. Two listeners: players on plain HTTP (court pages only — no credentials ever cross it)
//! and staff on TLS with the box's own certificate (A-124). Pages are server-rendered with askama (A-121).

use crate::auth::{self, Staff};
use crate::db::{self, Db};
use askama::Template;
use axum::extract::{DefaultBodyLimit, Form, Multipart, Path, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use tower_http::set_header::SetResponseHeaderLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub recorders: crate::recorder::Recorders,
    pub data: std::path::PathBuf,
    /// SHA-256 of the staff certificate, shown in Help so staff can check they reached their own box (A-124).
    pub fingerprint: String,
}

// ------------------------------------------------------------------ errors

#[derive(Debug)]
pub enum AppError {
    Forbidden,
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, title, message) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "Not allowed", "Only the venue owner can do that.".to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "Not found", "That page does not exist.".to_string()),
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "Check the form", m),
            Self::Internal(detail) => {
                tracing::error!(%detail, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong", "The box could not finish that. Try again; if it keeps happening, call support (see Help).".to_string())
            }
        };
        let page = MessagePage { title, message: &message, back: "/console" };
        (status, Html(page.render().unwrap_or_else(|_| message.clone()))).into_response()
    }
}

pub type Page = Result<Response, AppError>;

pub fn html<T: Template>(t: T) -> Page {
    Ok(Html(t.render()?).into_response())
}

#[derive(Template)]
#[template(path = "message.html")]
struct MessagePage<'a> {
    title: &'a str,
    message: &'a str,
    back: &'a str,
}

// ------------------------------------------------------------------ routers

pub fn player_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(player_home))
        .route("/healthz", get(|| async { "ok" }))
        .route("/static/{file}", get(static_file))
        .route("/logo", get(crate::clips::venue_logo))
        .route("/privacy", get(crate::venue::privacy))
        .route("/c/{code}", get(crate::clips::court_page))
        .route("/c/{code}/tap", post(crate::clips::tap))
        .route("/c/{code}/clips", get(crate::clips::court_clips))
        .route("/c/{code}/game", get(crate::clips::court_game))
        .route("/c/{code}/game/start", post(crate::games::start))
        .route("/c/{code}/game/stop", post(crate::games::stop))
        .route("/c/{code}/emails", post(crate::games::add_email))
        .route("/c/{code}/emails/{id}/remove", post(crate::games::remove_email))
        .route("/c/{code}/clips/{token}", get(crate::clips::clip_page))
        .route("/c/{code}/clips/{token}/hide", post(crate::clips::hide))
        .route("/c/{code}/clips/{token}/{file}", get(crate::clips::clip_file))
        .layer(middleware::from_fn(|req, next| same_origin_writes("http", req, next)))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

#[cfg(feature = "test-hooks")]
fn test_hooks(r: Router<AppState>) -> Router<AppState> {
    #[derive(Deserialize)]
    struct Advance {
        ms: i64,
    }
    #[derive(Deserialize)]
    struct Disk {
        bytes: u64,
    }
    #[derive(Deserialize)]
    struct Seed {
        court: i64,
        n: i64,
    }
    r.route("/__test/statements", get(|| async { crate::db::STATEMENTS.load(std::sync::atomic::Ordering::Relaxed).to_string() }))
    // Library fixtures: ready clip rows without files, one minute apart, cycling through the kinds.
    .route("/__test/seed-clips", post(|State(app): State<AppState>, Form(s): Form<Seed>| async move {
        let now = crate::clock::now();
        let conn = app.db.conn();
        for i in 0..s.n {
            let made = now - chrono::Duration::minutes(i);
            let kind = ["tap", "tap", "highlight", "game"][(i % 4) as usize];
            conn.execute(
                "INSERT INTO clip (court_id, kind, token, window_start_ms, window_end_ms, made_at, state) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready')",
                params![s.court, kind, auth::random_token(), made.timestamp_millis() - 30_000, made.timestamp_millis(), made.to_rfc3339()],
            )
            .map_err(|e| AppError::Internal(e.to_string()))?;
        }
        Ok::<_, AppError>(s.n.to_string())
    }))
    .route("/__test/disk", post(|Form(d): Form<Disk>| async move {
        crate::recorder::TEST_DISK_BYTES.store(d.bytes, std::sync::atomic::Ordering::Relaxed);
        "ok"
    }))
    .route("/__test/clock", post(|State(app): State<AppState>, Form(a): Form<Advance>| async move {
        crate::clock::advance(a.ms);
        if a.ms != 0 {
            app.recorders.split_all();
        }
        crate::clock::now().to_rfc3339()
    }))
}

#[cfg(not(feature = "test-hooks"))]
fn test_hooks(r: Router<AppState>) -> Router<AppState> {
    r
}

pub fn staff_router(state: AppState) -> Router {
    test_hooks(Router::new())
        .route("/", get(|| async { Redirect::to("/console") }))
        .route("/healthz", get(|| async { "ok" }))
        .route("/static/{file}", get(static_file))
        .route("/setup", get(setup_form).post(setup_submit).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)))
        .route("/console/signin", get(signin_form).post(signin_submit))
        .route("/console/signout", post(signout))
        .route("/console", get(crate::courts::home))
        .route("/console/courts", get(crate::courts::list).post(crate::courts::create))
        .route("/console/courts/new", get(crate::courts::new_form))
        .route("/console/courts/{id}", get(crate::courts::edit).post(crate::courts::update))
        .route("/console/courts/{id}/delete", get(crate::courts::delete_confirm).post(crate::courts::delete))
        .route("/console/courts/{id}/preview.jpg", get(crate::courts::preview))
        .route("/console/staff", get(staff_list).post(staff_create))
        .route("/console/staff/new", get(staff_new))
        .route("/console/staff/{id}", get(staff_edit).post(staff_update))
        .route("/console/staff/{id}/reset", post(staff_reset))
        .route("/console/staff/{id}/deactivate", post(staff_deactivate))
        .route("/console/staff/{id}/reactivate", post(staff_reactivate))
        .route("/console/clips", get(crate::library::library))
        .route("/console/clips/{id}/keep", post(crate::library::keep))
        .route("/console/clips/{id}/delete", get(crate::library::delete_confirm).post(crate::library::delete))
        .route("/console/clips/{id}/{file}", get(crate::library::file))
        .route("/console/log", get(crate::library::log))
        .route("/console/log.csv", get(crate::library::log_csv))
        .route("/console/settings", get(crate::venue::settings).post(crate::venue::save).layer(DefaultBodyLimit::max(UPLOAD_LIMIT)))
        .route("/console/settings/time", post(crate::venue::correct_time))
        .route("/console/help", get(crate::venue::help))
        .route("/console/signs", get(crate::venue::signs))
        .route("/console/logo", get(crate::clips::venue_logo))
        .route("/console/footage", get(crate::footage::footage))
        .route("/console/footage/holds", post(crate::footage::place_hold))
        .route("/console/footage/missing", post(crate::footage::record_missing))
        .route("/console/footage/holds/{id}/release", post(crate::footage::release))
        .route("/console/footage/holds/{id}/export", get(crate::footage::export))
        .route("/console/account", get(account))
        .route("/console/account/password", post(change_password))
        .layer(middleware::from_fn(|req, next| same_origin_writes("https", req, next)))
        .layer(middleware::from_fn(security_headers))
        .layer(SetResponseHeaderLayer::overriding(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .with_state(state)
}

/// Security headers on every response (CSP, nosniff, framing, referrer).
async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; img-src 'self' data: blob:; media-src 'self' blob:; style-src 'self'; script-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
    );
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // same-origin, not no-referrer: Chrome sends `Origin: null` on form posts under no-referrer, which would
    // defeat the same-origin check below; same-origin still never leaks the referrer off the box.
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    res
}

/// State-changing requests must come from this listener's own pages (v5.0.0-3.5.1): a cross-site Origin or a
/// cross-site fetch is refused before the handler runs. On the staff listener the SameSite=Strict cookie is the
/// second layer.
async fn same_origin_writes(scheme: &'static str, req: Request, next: Next) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        let headers = req.headers();
        // HTTP/2 carries the host in the URI authority (:authority), HTTP/1.1 in the Host header.
        let host = req
            .uri()
            .authority()
            .map(|a| a.as_str())
            .or_else(|| headers.get(header::HOST).and_then(|v| v.to_str().ok()))
            .unwrap_or("");
        let origin_ok = headers
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .is_none_or(|o| o == format!("{scheme}://{host}"));
        let site_ok = headers
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok())
            .is_none_or(|s| s == "same-origin" || s == "none");
        if !origin_ok || !site_ok {
            let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).unwrap_or("-");
            let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()).unwrap_or("-");
            tracing::warn!(path = %req.uri().path(), origin, host, site, "cross-site request refused");
            return (StatusCode::FORBIDDEN, "Cross-site request refused").into_response();
        }
    }
    next.run(req).await
}

async fn static_file(Path(file): Path<String>) -> Response {
    let (body, mime): (&'static [u8], &str) = match file.as_str() {
        "app.css" => (include_bytes!("../static/app.css"), "text/css; charset=utf-8"),
        "setup-camera-position.svg" => (include_bytes!("../static/setup-camera-position.svg"), "image/svg+xml"),
        "court.js" => (include_bytes!("../static/court.js"), "text/javascript; charset=utf-8"),
        "print.js" => (include_bytes!("../static/print.js"), "text/javascript; charset=utf-8"),
        "empty-library-scan.svg" => (include_bytes!("../static/empty-library-scan.svg"), "image/svg+xml"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "public, max-age=3600")], body).into_response()
}

async fn player_home() -> impl IntoResponse {
    Html(
        MessagePage { title: "Court Highlights", message: "Scan the court code on the net post to open your court's page.", back: "/" }
            .render()
            .unwrap_or_default(),
    )
}

// ------------------------------------------------------------------ shell shared by console pages

pub struct Shell {
    pub venue: String,
    pub user: String,
    pub section: &'static str,
    pub is_owner: bool,
}

pub fn shell(db: &Db, staff: &Staff, section: &'static str) -> Result<Shell, AppError> {
    Ok(Shell { venue: db::venue_name(&db.conn())?.unwrap_or_default(), user: staff.display_name.clone(), section, is_owner: staff.is_owner })
}

// ------------------------------------------------------------------ setup (FR-40)

#[derive(Template, Default)]
#[template(path = "setup.html")]
struct SetupPage {
    error: Option<String>,
    venue: String,
    open: String,
    close: String,
    display_name: String,
    username: String,
}

async fn setup_form(State(app): State<AppState>) -> Page {
    if db::has_staff(&app.db.conn())? {
        return Ok(Redirect::to("/console/signin").into_response());
    }
    html(SetupPage { open: "06:00".into(), close: "23:00".into(), ..Default::default() })
}

/// Logo files: PNG or JPEG only, checked by content, at most 2 MB (FR-33, v5.0.0-5.2.1, 5.2.2).
pub const LOGO_MAX_BYTES: usize = 2 * 1024 * 1024;
/// Forms with logos accept a little more than two logos, so a too-large logo gets a clear message, not a bare 413.
const UPLOAD_LIMIT: usize = 6 * 1024 * 1024;

pub fn logo_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else {
        None
    }
}

async fn setup_submit(State(app): State<AppState>, mut form: Multipart) -> Page {
    if db::has_staff(&app.db.conn())? {
        return Ok(Redirect::to("/console/signin").into_response());
    }
    let mut page = SetupPage::default();
    let mut password = String::new();
    let mut logo: Option<Vec<u8>> = None;
    while let Some(field) = form.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "logo" {
            let bytes = field.bytes().await.map_err(|_| AppError::BadRequest("The logo is larger than 2 MB.".into()))?;
            if !bytes.is_empty() {
                logo = Some(bytes.to_vec());
            }
            continue;
        }
        let value = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
        match name.as_str() {
            "venue" => page.venue = value.trim().to_string(),
            "open" => page.open = value,
            "close" => page.close = value,
            "display_name" => page.display_name = value.trim().to_string(),
            "username" => page.username = value.trim().to_string(),
            "password" => password = value,
            _ => {}
        }
    }
    let times = (chrono::NaiveTime::parse_from_str(&page.open, "%H:%M"), chrono::NaiveTime::parse_from_str(&page.close, "%H:%M"));
    let problem = if page.venue.is_empty() || page.display_name.is_empty() || page.username.is_empty() {
        Some("Fill in the venue name, your name and a username.".to_string())
    } else if !matches!(times, (Ok(o), Ok(c)) if o < c) {
        Some("Opening time must be before closing time on the same day.".to_string())
    } else if logo.as_ref().is_some_and(|l| l.len() > LOGO_MAX_BYTES) {
        Some("The logo is larger than 2 MB.".to_string())
    } else if logo.as_deref().is_some_and(|l| logo_type(l).is_none()) {
        Some("The logo must be a PNG or JPEG image.".to_string())
    } else {
        auth::password_problem(&password).map(str::to_string)
    };
    if let Some(e) = problem {
        page.error = Some(e);
        let mut res = html(page)?;
        *res.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
        return Ok(res);
    }
    let now = crate::clock::now().to_rfc3339();
    let token;
    {
        let conn = app.db.conn();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO venue (id, name, logo, logo_type, open_time, close_time) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
            params![page.venue, logo, logo.as_deref().and_then(logo_type), page.open, page.close],
        )?;
        tx.execute(
            "INSERT INTO staff (username, display_name, role, password_hash, created_at) VALUES (?1, ?2, 'owner', ?3, ?4)",
            params![page.username, page.display_name, auth::hash_password(&password), now],
        )?;
        db::log(&tx, Some(tx.last_insert_rowid()), &page.username, "set up venue", &page.venue, "")?;
        tx.commit()?;
    }
    match auth::sign_in(&app.db, &page.username, &password)? {
        auth::SignInOutcome::Ok { token: t, .. } => token = t,
        auth::SignInOutcome::Refused => return Err(AppError::Internal("new owner could not sign in".into())),
    }
    Ok(([(header::SET_COOKIE, auth::session_cookie(&token))], Redirect::to("/console")).into_response())
}

// ------------------------------------------------------------------ sign-in (FR-28)

#[derive(Template)]
#[template(path = "signin.html")]
struct SignInPage {
    venue: String,
    error: Option<&'static str>,
    signed_out: bool,
}

#[derive(Deserialize)]
struct SignInQuery {
    out: Option<String>,
}

async fn signin_form(State(app): State<AppState>, Query(q): Query<SignInQuery>) -> Page {
    let conn = app.db.conn();
    if !db::has_staff(&conn)? {
        return Ok(Redirect::to("/setup").into_response());
    }
    html(SignInPage { venue: db::venue_name(&conn)?.unwrap_or_default(), error: None, signed_out: q.out.is_some() })
}

#[derive(Deserialize)]
struct SignInForm {
    username: String,
    password: String,
}

async fn signin_submit(State(app): State<AppState>, Form(f): Form<SignInForm>) -> Page {
    match auth::sign_in(&app.db, f.username.trim(), &f.password)? {
        auth::SignInOutcome::Ok { token, must_change } => {
            let to = if must_change { "/console/account?first=1" } else { "/console" };
            Ok(([(header::SET_COOKIE, auth::session_cookie(&token))], Redirect::to(to)).into_response())
        }
        auth::SignInOutcome::Refused => {
            let venue = db::venue_name(&app.db.conn())?.unwrap_or_default();
            let mut res = html(SignInPage {
                venue,
                error: Some("Wrong username or password, or the account is locked. After 5 wrong tries, wait 15 minutes."),
                signed_out: false,
            })?;
            *res.status_mut() = StatusCode::UNAUTHORIZED;
            Ok(res)
        }
    }
}

async fn signout(State(app): State<AppState>, headers: HeaderMap) -> Page {
    let mut parts = axum::http::Request::new(()).into_parts().0;
    parts.headers = headers;
    if let Some(token) = auth::cookie_token(&parts) {
        app.db.conn().execute("DELETE FROM staff_session WHERE token_hash = ?1", [auth::sha256_hex(&token)])?;
    }
    Ok(([(header::SET_COOKIE, auth::clear_cookie())], Redirect::to("/console/signin?out=1")).into_response())
}

// ------------------------------------------------------------------ staff management (FR-29)

pub struct StaffRow {
    pub id: i64,
    pub display_name: String,
    pub username: String,
    pub role: String,
    pub active: bool,
    pub locked: bool,
}

#[derive(Template)]
#[template(path = "staff_list.html")]
struct StaffListPage {
    shell: Shell,
    staff: Vec<StaffRow>,
    notice: Option<String>,
}

#[derive(Deserialize)]
struct NoticeQuery {
    notice: Option<String>,
}

async fn staff_list(State(app): State<AppState>, staff: Staff, Query(q): Query<NoticeQuery>) -> Page {
    staff.require_owner()?;
    let now = crate::clock::now();
    let rows = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, display_name, username, role, active, locked_until FROM staff ORDER BY active DESC, display_name")?;
        st.query_map([], |r| {
            let locked: Option<String> = r.get(5)?;
            Ok(StaffRow {
                id: r.get(0)?,
                display_name: r.get(1)?,
                username: r.get(2)?,
                role: r.get(3)?,
                active: r.get(4)?,
                locked: locked.as_deref().and_then(crate::clock::parse).is_some_and(|t| t > now),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?
    };
    let notice = q.notice.map(|n| match n.as_str() {
        "deactivated" => "Deactivated. Their access has ended.".to_string(),
        "reactivated" => "Reactivated. Reset their password so they can sign in.".to_string(),
        _ => "Saved.".to_string(),
    });
    html(StaffListPage { shell: shell(&app.db, &staff, "staff")?, staff: rows, notice })
}

#[derive(Template)]
#[template(path = "staff_form.html")]
struct StaffFormPage {
    shell: Shell,
    id: Option<i64>,
    display_name: String,
    username: String,
    role: String,
    active: bool,
    error: Option<String>,
}

async fn staff_new(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    html(StaffFormPage { shell: shell(&app.db, &staff, "staff")?, id: None, display_name: String::new(), username: String::new(), role: "staff".into(), active: true, error: None })
}

#[derive(Deserialize)]
struct StaffForm {
    display_name: String,
    username: Option<String>,
    role: String,
}

#[derive(Template)]
#[template(path = "staff_temp.html")]
struct StaffTempPage {
    shell: Shell,
    display_name: String,
    username: String,
    password: String,
}

fn valid_role(role: &str) -> Result<&str, AppError> {
    match role {
        "owner" | "staff" => Ok(role),
        _ => Err(AppError::BadRequest("Choose a role.".into())),
    }
}

async fn staff_create(State(app): State<AppState>, staff: Staff, Form(f): Form<StaffForm>) -> Page {
    staff.require_owner()?;
    let role = valid_role(&f.role)?.to_string();
    let display_name = f.display_name.trim().to_string();
    let username = f.username.unwrap_or_default().trim().to_string();
    let form_error = |msg: &str, shell| html(StaffFormPage { shell, id: None, display_name: display_name.clone(), username: username.clone(), role: role.clone(), active: true, error: Some(msg.into()) });
    if display_name.is_empty() || username.is_empty() || username.len() > 40 {
        return form_error("Fill in a name and a username (up to 40 characters).", shell(&app.db, &staff, "staff")?);
    }
    let temp = auth::temporary_password();
    let now = crate::clock::now();
    let inserted = {
        let conn = app.db.conn();
        let r = conn.execute(
            "INSERT INTO staff (username, display_name, role, password_hash, must_change, temp_expires_at, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
            params![username, display_name, role, auth::hash_password(&temp), (now + chrono::Duration::hours(72)).to_rfc3339(), now.to_rfc3339()],
        );
        match r {
            Ok(_) => {
                db::log(&conn, Some(staff.id), &staff.username, "added staff", &username, &role)?;
                true
            }
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::ConstraintViolation => false,
            Err(e) => return Err(e.into()),
        }
    };
    if !inserted {
        return form_error("That username is taken.", shell(&app.db, &staff, "staff")?);
    }
    html(StaffTempPage { shell: shell(&app.db, &staff, "staff")?, display_name, username, password: temp })
}

fn load_staff(db: &Db, id: i64) -> Result<(String, String, String, bool), AppError> {
    db.conn()
        .query_row("SELECT display_name, username, role, active FROM staff WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .optional()?
        .ok_or(AppError::NotFound)
}

async fn staff_edit(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let (display_name, username, role, active) = load_staff(&app.db, id)?;
    html(StaffFormPage { shell: shell(&app.db, &staff, "staff")?, id: Some(id), display_name, username, role, active, error: None })
}

/// Refuses changes that would leave the venue without an active owner.
fn last_owner(db: &Db, id: i64) -> Result<bool, AppError> {
    let others: i64 = db.conn().query_row("SELECT COUNT(*) FROM staff WHERE role = 'owner' AND active = 1 AND id != ?1", [id], |r| r.get(0))?;
    Ok(others == 0)
}

async fn staff_update(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>, Form(f): Form<StaffForm>) -> Page {
    staff.require_owner()?;
    let role = valid_role(&f.role)?;
    let (_, username, old_role, _) = load_staff(&app.db, id)?;
    if old_role == "owner" && role != "owner" && last_owner(&app.db, id)? {
        return Err(AppError::BadRequest("Keep at least one owner. Make someone else an owner first.".into()));
    }
    let name = f.display_name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Fill in a name.".into()));
    }
    let conn = app.db.conn();
    conn.execute("UPDATE staff SET display_name = ?1, role = ?2 WHERE id = ?3", params![name, role, id])?;
    db::log(&conn, Some(staff.id), &staff.username, "edited staff", &username, role)?;
    Ok(Redirect::to("/console/staff?notice=saved").into_response())
}

async fn staff_reset(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let (display_name, username, _, active) = load_staff(&app.db, id)?;
    if !active {
        return Err(AppError::BadRequest("Reactivate the account first.".into()));
    }
    let temp = auth::temporary_password();
    {
        let conn = app.db.conn();
        let expires = (crate::clock::now() + chrono::Duration::hours(72)).to_rfc3339();
        conn.execute(
            "UPDATE staff SET password_hash = ?1, must_change = 1, temp_expires_at = ?2, failures = 0, locked_until = NULL WHERE id = ?3",
            params![auth::hash_password(&temp), expires, id],
        )?;
        conn.execute("DELETE FROM staff_session WHERE staff_id = ?1", [id])?;
        db::log(&conn, Some(staff.id), &staff.username, "reset password", &username, "")?;
    }
    html(StaffTempPage { shell: shell(&app.db, &staff, "staff")?, display_name, username, password: temp })
}

async fn staff_deactivate(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let (_, username, role, _) = load_staff(&app.db, id)?;
    if role == "owner" && last_owner(&app.db, id)? {
        return Err(AppError::BadRequest("Keep at least one active owner.".into()));
    }
    let conn = app.db.conn();
    conn.execute("UPDATE staff SET active = 0 WHERE id = ?1", [id])?;
    conn.execute("DELETE FROM staff_session WHERE staff_id = ?1", [id])?;
    db::log(&conn, Some(staff.id), &staff.username, "deactivated staff", &username, "")?;
    Ok(Redirect::to("/console/staff?notice=deactivated").into_response())
}

async fn staff_reactivate(State(app): State<AppState>, staff: Staff, Path(id): Path<i64>) -> Page {
    staff.require_owner()?;
    let (_, username, _, _) = load_staff(&app.db, id)?;
    let conn = app.db.conn();
    conn.execute("UPDATE staff SET active = 1 WHERE id = ?1", [id])?;
    db::log(&conn, Some(staff.id), &staff.username, "reactivated staff", &username, "")?;
    Ok(Redirect::to("/console/staff?notice=reactivated").into_response())
}

// ------------------------------------------------------------------ my account (FR-34)

#[derive(Template)]
#[template(path = "account.html")]
struct AccountPage {
    shell: Shell,
    first: bool,
    saved: bool,
    error: Option<&'static str>,
}

#[derive(Deserialize)]
struct AccountQuery {
    first: Option<String>,
    saved: Option<String>,
}

async fn account(State(app): State<AppState>, staff: Staff, Query(q): Query<AccountQuery>) -> Page {
    let first = q.first.is_some() || staff.must_change;
    html(AccountPage { shell: shell(&app.db, &staff, "account")?, first, saved: q.saved.is_some(), error: None })
}

#[derive(Deserialize)]
struct PasswordForm {
    current: String,
    new: String,
}

async fn change_password(State(app): State<AppState>, staff: Staff, headers: HeaderMap, Form(f): Form<PasswordForm>) -> Page {
    let error = match auth::verify(&app.db, &staff.username, &f.current)? {
        None => Some("Your current password is not right."),
        Some(_) if f.new == f.current => Some("Choose a password different from the current one."),
        Some(_) => auth::password_problem(&f.new),
    };
    if let Some(e) = error {
        let mut res = html(AccountPage { shell: shell(&app.db, &staff, "account")?, first: staff.must_change, saved: false, error: Some(e) })?;
        *res.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
        return Ok(res);
    }
    let mut parts = axum::http::Request::new(()).into_parts().0;
    parts.headers = headers;
    let keep = auth::cookie_token(&parts).map(|t| auth::sha256_hex(&t)).unwrap_or_default();
    let conn = app.db.conn();
    conn.execute(
        "UPDATE staff SET password_hash = ?1, must_change = 0, temp_expires_at = NULL WHERE id = ?2",
        params![auth::hash_password(&f.new), staff.id],
    )?;
    // Other places this account was signed in lose access; this browser stays signed in.
    conn.execute("DELETE FROM staff_session WHERE staff_id = ?1 AND token_hash != ?2", params![staff.id, keep])?;
    db::log(&conn, Some(staff.id), &staff.username, "changed password", "staff", "")?;
    Ok(Redirect::to("/console/account?saved=1").into_response())
}

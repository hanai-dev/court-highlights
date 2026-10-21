//! Fleet console (FR-47…FR-53): box health, venues and their boxes (register, revoke), the online switch,
//! users and the fleet log. It never lists, shows or serves any clip, game video or footage (FR-53).

use crate::fleet_auth::{self as auth, Denied, FleetUser, Outcome};
use crate::{App, hex_sha256, random_token};
use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::Row;

type Page = Result<Response, Denied>;

fn html<T: Template>(t: T) -> Page {
    Ok(Html(t.render()?).into_response())
}

fn when(t: DateTime<Utc>) -> String {
    t.with_timezone(&ch_core::manila()).format("%-d %b %Y, %-I:%M %P").to_string()
}

pub async fn log(app: &App, actor: &str, action: &str, item: &str, detail: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO fleet_log (at, actor, action, item, detail) VALUES ($1, $2, $3, $4, $5)")
        .bind(crate::now())
        .bind(actor)
        .bind(action)
        .bind(item)
        .bind(detail)
        .execute(&app.db)
        .await?;
    Ok(())
}

pub struct Nav {
    pub user: String,
    pub admin: bool,
    pub section: &'static str,
}

fn nav(u: &FleetUser, section: &'static str) -> Nav {
    Nav { user: u.username.clone(), admin: u.is_admin, section }
}

// ------------------------------------------------------------------ sign-in and account

#[derive(Template)]
#[template(path = "fleet/signin.html")]
struct SignInPage {
    error: Option<&'static str>,
    username: String,
}

pub async fn signin_form() -> Page {
    html(SignInPage { error: None, username: String::new() })
}

#[derive(Deserialize)]
pub struct SignInForm {
    username: String,
    password: String,
    code: String,
}

pub async fn signin(State(app): State<App>, Form(f): Form<SignInForm>) -> Page {
    match auth::sign_in(&app, f.username.trim(), &f.password, &f.code).await? {
        Outcome::In { token, must_change } => {
            let to = if must_change { "/fleet/account" } else { "/fleet" };
            Ok(([(header::SET_COOKIE, auth::session_cookie(&token))], Redirect::to(to)).into_response())
        }
        Outcome::Refused => {
            let mut res = html(SignInPage { error: Some("That username, password or code isn't right."), username: f.username })?;
            *res.status_mut() = StatusCode::UNAUTHORIZED;
            Ok(res)
        }
    }
}

pub async fn signout(State(app): State<App>, headers: HeaderMap) -> Page {
    if let Some(t) = auth::cookie_token(&headers) {
        sqlx::query("DELETE FROM fleet_session WHERE token_hash = $1").bind(hex_sha256(&t)).execute(&app.db).await?;
    }
    let clear = format!("{}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0", auth::COOKIE);
    Ok(([(header::SET_COOKIE, clear)], Redirect::to("/fleet/signin")).into_response())
}

#[derive(Template)]
#[template(path = "fleet/account.html")]
struct AccountPage {
    nav: Nav,
    first: bool,
    error: Option<&'static str>,
}

pub async fn account(u: FleetUser) -> Page {
    html(AccountPage { first: u.must_change, nav: nav(&u, "account"), error: None })
}

#[derive(Deserialize)]
pub struct PasswordForm {
    current: String,
    new: String,
}

pub async fn change_password(State(app): State<App>, u: FleetUser, headers: HeaderMap, Form(f): Form<PasswordForm>) -> Page {
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM fleet_user WHERE id = $1").bind(u.id).fetch_one(&app.db).await?;
    let problem = if !auth::verify_password(&f.current, &hash) { Some("Your current password isn't right.") } else { auth::password_problem(&f.new) };
    if let Some(e) = problem {
        let mut res = html(AccountPage { first: u.must_change, nav: nav(&u, "account"), error: Some(e) })?;
        *res.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
        return Ok(res);
    }
    sqlx::query("UPDATE fleet_user SET password_hash = $1, must_change = FALSE WHERE id = $2")
        .bind(auth::hash_password(&f.new))
        .bind(u.id)
        .execute(&app.db)
        .await?;
    // Every other session of this account ends.
    let keep = auth::cookie_token(&headers).map(|t| hex_sha256(&t)).unwrap_or_default();
    sqlx::query("DELETE FROM fleet_session WHERE user_id = $1 AND token_hash <> $2").bind(u.id).bind(keep).execute(&app.db).await?;
    log(&app, &u.username, "changed password", "", "").await?;
    Ok(Redirect::to("/fleet").into_response())
}

// ------------------------------------------------------------------ health (FR-49)

pub struct BoxLine {
    pub id: i64,
    pub name: String,
    pub revoked: bool,
    pub missed: bool,
    pub last: String,
    pub version: String,
    pub disk: String,
    pub temperature: String,
    pub courts: String,
}

pub struct VenueLine {
    pub id: i64,
    pub name: String,
    pub online: bool,
    pub boxes: Vec<BoxLine>,
}

/// A box that has not checked in for 5 minutes is flagged (FR-49).
const MISSED_AFTER_MIN: i64 = 5;

async fn venues(app: &App, only: Option<i64>) -> Result<Vec<VenueLine>, Denied> {
    let rows = sqlx::query(
        "SELECT v.id, v.name, v.online_on, b.id, b.name, b.revoked_at IS NOT NULL, c.at, c.version, c.disk_pct, c.temperature_c, c.courts_recording, c.courts_total
         FROM venue v LEFT JOIN box b ON b.venue_id = v.id AND b.wiped_at IS NULL
         LEFT JOIN LATERAL (SELECT * FROM checkin WHERE box_id = b.id ORDER BY at DESC LIMIT 1) c ON TRUE
         WHERE $1::BIGINT IS NULL OR v.id = $1 ORDER BY v.name, v.id, b.name, b.id",
    )
    .bind(only)
    .fetch_all(&app.db)
    .await?;
    let now = crate::now();
    let mut out: Vec<VenueLine> = Vec::new();
    for r in rows {
        let vid: i64 = r.get(0);
        if out.last().is_none_or(|v| v.id != vid) {
            out.push(VenueLine { id: vid, name: r.get(1), online: r.get(2), boxes: Vec::new() });
        }
        let Some(bid) = r.get::<Option<i64>, _>(3) else { continue };
        let at: Option<DateTime<Utc>> = r.get(6);
        let revoked: bool = r.get(5);
        out.last_mut().expect("pushed above").boxes.push(BoxLine {
            id: bid,
            name: r.get(4),
            revoked,
            missed: !revoked && at.is_none_or(|t| now - t > chrono::Duration::minutes(MISSED_AFTER_MIN)),
            last: at.map_or_else(|| "Never".into(), when),
            version: r.get::<Option<String>, _>(7).unwrap_or_default(),
            disk: r.get::<Option<i32>, _>(8).map_or_else(String::new, |d| format!("{d} %")),
            temperature: r.get::<Option<f32>, _>(9).map_or_else(String::new, |t| format!("{t:.0} °C")),
            courts: match (r.get::<Option<i32>, _>(10), r.get::<Option<i32>, _>(11)) {
                (Some(rec), Some(total)) => format!("{rec} of {total}"),
                _ => String::new(),
            },
        });
    }
    Ok(out)
}

#[derive(Template)]
#[template(path = "fleet/health.html")]
struct HealthPage {
    nav: Nav,
    venues: Vec<VenueLine>,
}

pub async fn health(State(app): State<App>, u: FleetUser) -> Page {
    html(HealthPage { nav: nav(&u, "health"), venues: venues(&app, None).await? })
}

// ------------------------------------------------------------------ venues and boxes (FR-48, FR-50)

#[derive(Template)]
#[template(path = "fleet/venues.html")]
struct VenuesPage {
    nav: Nav,
    venues: Vec<VenueLine>,
}

pub async fn venue_list(State(app): State<App>, u: FleetUser) -> Page {
    html(VenuesPage { nav: nav(&u, "venues"), venues: venues(&app, None).await? })
}

#[derive(Deserialize)]
pub struct NameForm {
    name: String,
}

pub async fn venue_add(State(app): State<App>, u: FleetUser, Form(f): Form<NameForm>) -> Page {
    u.admin()?;
    let name = f.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Ok(crate::player::message(StatusCode::BAD_REQUEST, "Check the name", "Give the venue a name of up to 80 characters."));
    }
    let id: i64 = sqlx::query_scalar("INSERT INTO venue (name) VALUES ($1) RETURNING id").bind(name).fetch_one(&app.db).await?;
    log(&app, &u.username, "added venue", name, "").await?;
    Ok(Redirect::to(&format!("/fleet/venues/{id}")).into_response())
}

#[derive(Template)]
#[template(path = "fleet/venue.html")]
struct VenuePage {
    nav: Nav,
    v: VenueLine,
    reason: String,
    new_key: Option<(String, String)>,
}

async fn venue_page(app: &App, u: &FleetUser, id: i64, new_key: Option<(String, String)>) -> Page {
    let v = venues(app, Some(id)).await?.pop().ok_or(Denied::NotFound)?;
    let reason: String = sqlx::query_scalar("SELECT switch_reason FROM venue WHERE id = $1").bind(id).fetch_one(&app.db).await?;
    html(VenuePage { nav: nav(u, "venues"), v, reason, new_key })
}

pub async fn venue(State(app): State<App>, u: FleetUser, Path(id): Path<i64>) -> Page {
    venue_page(&app, &u, id, None).await
}

/// An Installer registers a box; its key is shown once and stored only as a hash (FR-48).
pub async fn register_box(State(app): State<App>, u: FleetUser, Path(id): Path<i64>, Form(f): Form<NameForm>) -> Page {
    let name = f.name.trim();
    if name.is_empty() || name.len() > 40 {
        return Ok(crate::player::message(StatusCode::BAD_REQUEST, "Check the name", "Give the box a name of up to 40 characters, e.g. Box A."));
    }
    let venue: String = sqlx::query_scalar("SELECT name FROM venue WHERE id = $1").bind(id).fetch_optional(&app.db).await?.ok_or(Denied::NotFound)?;
    let key = random_token();
    sqlx::query("INSERT INTO box (venue_id, name, key_hash, registered_by) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(name)
        .bind(hex_sha256(&key))
        .bind(&u.username)
        .execute(&app.db)
        .await?;
    log(&app, &u.username, "registered box", &format!("{venue} · {name}"), "").await?;
    venue_page(&app, &u, id, Some((name.to_string(), key))).await
}

/// An Admin revokes a box key: its uploads and reports are refused from then on (FR-48).
pub async fn revoke_box(State(app): State<App>, u: FleetUser, Path(id): Path<i64>) -> Page {
    u.admin()?;
    let row = sqlx::query("UPDATE box b SET revoked_at = $1, revoked_by = $2 FROM venue v WHERE b.id = $3 AND v.id = b.venue_id AND b.revoked_at IS NULL RETURNING b.venue_id, v.name, b.name")
        .bind(crate::now())
        .bind(&u.username)
        .bind(id)
        .fetch_optional(&app.db)
        .await?
        .ok_or(Denied::NotFound)?;
    let (venue_id, venue, name): (i64, String, String) = (row.get(0), row.get(1), row.get(2));
    log(&app, &u.username, "revoked box key", &format!("{venue} · {name}"), "").await?;
    Ok(Redirect::to(&format!("/fleet/venues/{venue_id}")).into_response())
}

#[derive(Deserialize)]
pub struct SwitchForm {
    on: String,
    reason: String,
}

/// The online switch, with a reason, logged (FR-50).
pub async fn switch(State(app): State<App>, u: FleetUser, Path(id): Path<i64>, Form(f): Form<SwitchForm>) -> Page {
    u.admin()?;
    let reason = f.reason.trim();
    if reason.is_empty() || reason.len() > 200 {
        return Ok(crate::player::message(StatusCode::BAD_REQUEST, "Give a reason", "Say why online features are switched (up to 200 characters)."));
    }
    let on = f.on == "1";
    let venue: String = sqlx::query_scalar("UPDATE venue SET online_on = $1, switch_reason = $2 WHERE id = $3 RETURNING name")
        .bind(on)
        .bind(reason)
        .bind(id)
        .fetch_optional(&app.db)
        .await?
        .ok_or(Denied::NotFound)?;
    log(&app, &u.username, if on { "switched online features on" } else { "switched online features off" }, &venue, reason).await?;
    Ok(Redirect::to(&format!("/fleet/venues/{id}")).into_response())
}

// ------------------------------------------------------------------ users and log

pub struct UserLine {
    pub username: String,
    pub role: String,
    pub active: bool,
}

#[derive(Template)]
#[template(path = "fleet/users.html")]
struct UsersPage {
    nav: Nav,
    users: Vec<UserLine>,
    created: Option<Created>,
}

pub struct Created {
    pub username: String,
    pub password: String,
    pub secret: String,
    pub qr: String,
}

async fn users_page(app: &App, u: &FleetUser, created: Option<Created>) -> Page {
    let users = sqlx::query("SELECT username, role, active FROM fleet_user ORDER BY username")
        .fetch_all(&app.db)
        .await?
        .into_iter()
        .map(|r| UserLine { username: r.get(0), role: r.get(1), active: r.get(2) })
        .collect();
    html(UsersPage { nav: nav(u, "users"), users, created })
}

pub async fn users(State(app): State<App>, u: FleetUser) -> Page {
    u.admin()?;
    users_page(&app, &u, None).await
}

#[derive(Deserialize)]
pub struct UserForm {
    username: String,
    role: String,
}

/// New Admin or Installer: a temporary password and authenticator enrolment, shown once.
pub async fn user_add(State(app): State<App>, u: FleetUser, Form(f): Form<UserForm>) -> Page {
    u.admin()?;
    let username = f.username.trim().to_lowercase();
    if username.is_empty()
        || username.len() > 40
        || !username.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        || !["admin", "installer"].contains(&f.role.as_str())
    {
        return Ok(crate::player::message(
            StatusCode::BAD_REQUEST,
            "Check the form",
            "Usernames use letters, digits, dots and dashes; the role is Admin or Installer.",
        ));
    }
    let created = create_user(&app.db, &app.secret_key, &username, &f.role).await?;
    log(&app, &u.username, "added fleet user", &username, &f.role).await?;
    users_page(&app, &u, Some(created)).await
}

pub async fn create_user(db: &sqlx::PgPool, key: &[u8; 32], username: &str, role: &str) -> Result<Created, Denied> {
    let (password, secret) = (auth::temporary_password(), auth::new_totp_secret());
    sqlx::query("INSERT INTO fleet_user (username, role, password_hash, totp_secret_enc) VALUES ($1, $2, $3, $4)")
        .bind(username)
        .bind(role)
        .bind(auth::hash_password(&password))
        .bind(auth::seal(key, &secret))
        .execute(db)
        .await?;
    let uri = auth::otpauth_uri(username, &secret);
    let qr =
        qrcode::QrCode::new(uri.as_bytes()).map_err(|e| Denied::Failed(e.to_string()))?.render::<qrcode::render::svg::Color>().min_dimensions(200, 200).build();
    Ok(Created { username: username.into(), password, secret: auth::base32(&secret), qr })
}

pub struct LogLine {
    pub at: String,
    pub actor: String,
    pub action: String,
    pub item: String,
    pub detail: String,
}

#[derive(Template)]
#[template(path = "fleet/log.html")]
struct LogPage {
    nav: Nav,
    rows: Vec<LogLine>,
}

pub async fn fleet_log(State(app): State<App>, u: FleetUser) -> Page {
    u.admin()?;
    let rows = sqlx::query("SELECT at, actor, action, item, detail FROM fleet_log ORDER BY id DESC LIMIT 200")
        .fetch_all(&app.db)
        .await?
        .into_iter()
        .map(|r| LogLine { at: when(r.get(0)), actor: r.get(1), action: r.get(2), item: r.get(3), detail: r.get(4) })
        .collect();
    html(LogPage { nav: nav(&u, "log"), rows })
}

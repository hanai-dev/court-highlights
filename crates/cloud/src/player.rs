//! Take-home pages and email (FR-23…FR-25). Links are unguessable tokens; each item stops working at the moment
//! the box copy expires (FR-24) or when the clip was hidden or deleted (FR-26).

use crate::mail::Email;
use crate::{ApiError, App, email_hash, random_token};
use askama::Template;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{DateTime, Utc};
use sqlx::Row;

fn manila(t: DateTime<Utc>) -> ch_core::Time {
    t.with_timezone(&ch_core::manila())
}

fn label(kind: &str, made: DateTime<Utc>, court: &str) -> String {
    let at = manila(made).format("%-I:%M %P");
    match kind {
        "game" => format!("Game video, {court}"),
        "reel" => format!("Highlights reel, {court}"),
        "highlight" => format!("Highlight, {court}, {at}"),
        _ => format!("Clip, {court}, {at}"),
    }
}

pub struct Line {
    pub id: i64,
    pub label: String,
    pub until: String,
}

struct DeliveryItems {
    venue: String,
    title: String,
    live: Vec<Line>,
    expired: usize,
    first_expiry: Option<DateTime<Utc>>,
}

async fn items(app: &App, delivery: i64) -> Result<DeliveryItems, ApiError> {
    let rows = sqlx::query(
        "SELECT i.id, i.kind, i.court, i.made_at, i.expires_at, i.expires_at <= $2, v.name, d.title
         FROM delivery d JOIN venue v ON v.id = d.venue_id JOIN delivery_item di ON di.delivery_id = d.id JOIN item i ON i.id = di.item_id
         WHERE d.id = $1 AND i.withdrawn_at IS NULL AND i.uploaded_at IS NOT NULL ORDER BY di.position",
    )
    .bind(delivery)
    .bind(crate::now())
    .fetch_all(&app.db)
    .await?;
    let mut out = DeliveryItems { venue: String::new(), title: String::new(), live: Vec::new(), expired: 0, first_expiry: None };
    for r in rows {
        let (expires, expired): (DateTime<Utc>, bool) = (r.get(4), r.get(5));
        out.venue = r.get(6);
        out.title = r.get(7);
        if expired {
            out.expired += 1;
            continue;
        }
        out.first_expiry = Some(out.first_expiry.map_or(expires, |f| f.min(expires)));
        out.live.push(Line { id: r.get(0), label: label(r.get::<&str, _>(1), r.get(3), r.get::<&str, _>(2)), until: ch_core::expiry_text(manila(expires)) });
    }
    Ok(out)
}

#[derive(Template)]
#[template(path = "email.html")]
struct EmailHtml<'a> {
    venue: &'a str,
    title: &'a str,
    page: &'a str,
    base: &'a str,
    token: &'a str,
    lines: &'a [Line],
    until: &'a str,
    never: &'a str,
}

/// One email per address; blocked addresses get nothing. Returns (sent, skipped).
pub async fn send_delivery(app: &App, delivery: i64, token: &str, recipients: &[String]) -> Result<(usize, usize), ApiError> {
    let d = items(app, delivery).await?;
    let Some(first) = d.first_expiry else { return Err(ApiError::Expired) };
    let until = ch_core::expiry_text(manila(first));
    let page = format!("{}/v/{token}", app.public_url);
    let (mut sent, mut skipped) = (0, 0);
    for to in recipients {
        let to = to.trim().to_lowercase();
        let hash = email_hash(&app.pepper, &to);
        let blocked: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocked WHERE email_hash = $1)").bind(&hash).fetch_one(&app.db).await?;
        if blocked || !to.contains('@') {
            skipped += 1;
            continue;
        }
        let never_token = random_token();
        sqlx::query("INSERT INTO never_link (token, email_hash) VALUES ($1, $2)").bind(&never_token).bind(&hash).execute(&app.db).await?;
        let never = format!("{}/never/{never_token}", app.public_url);
        let subject = format!("Your videos from {} — {}", d.venue, d.title);
        let mut text = format!("{}\n\nYour videos are {until}:\n{page}\n\n", d.title);
        for l in &d.live {
            text += &format!("- {}: {}/v/{token}/{}\n", l.label, app.public_url, l.id);
        }
        text += &format!("\nThis isn't me — don't email me again: {never}\n");
        let html = EmailHtml { venue: &d.venue, title: &d.title, page: &page, base: &app.public_url, token, lines: &d.live, until: &until, never: &never }
            .render()
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        match app.mail.send(&Email { to: &to, subject: &subject, text: &text, html: &html }).await {
            Ok(()) => sent += 1,
            Err(e) => {
                // Never log the address itself (NFR-15).
                tracing::error!(delivery, error = %e, "email not sent");
                skipped += 1;
            }
        }
    }
    tracing::info!(delivery, sent, skipped, "take-home email");
    Ok((sent, skipped))
}

// ------------------------------------------------------------------ pages

#[derive(Template)]
#[template(path = "videos.html")]
struct VideosPage {
    venue: String,
    title: String,
    token: String,
    lines: Vec<Line>,
}

#[derive(Template)]
#[template(path = "message.html")]
pub struct MessagePage<'a> {
    pub title: &'a str,
    pub message: &'a str,
}

pub fn message(status: StatusCode, title: &str, text: &str) -> Response {
    let body = MessagePage { title, message: text }.render().unwrap_or_else(|_| text.to_string());
    (status, Html(body)).into_response()
}

async fn delivery_by_token(app: &App, token: &str) -> Result<Option<i64>, ApiError> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(None);
    }
    Ok(sqlx::query_scalar("SELECT id FROM delivery WHERE token = $1").bind(token).fetch_optional(&app.db).await?)
}

pub async fn videos(State(app): State<App>, Path(token): Path<String>) -> Result<Response, ApiError> {
    let Some(delivery) = delivery_by_token(&app, &token).await? else {
        return Ok(message(StatusCode::NOT_FOUND, "Not found", "That link doesn't exist. Check you copied the whole link from the email."));
    };
    let d = items(&app, delivery).await?;
    if d.live.is_empty() {
        let (title, text) = if d.expired > 0 {
            ("These videos have expired", "These videos have expired. The venue keeps videos for 3 days; ask the front desk if you need one.")
        } else {
            ("These videos are no longer available", "These videos are no longer available.")
        };
        return Ok(message(StatusCode::GONE, title, text));
    }
    let page = VideosPage { venue: d.venue, title: d.title, token, lines: d.live };
    Ok(Html(page.render().map_err(|e| ApiError::Internal(e.to_string()))?).into_response())
}

/// One item: a short-lived download link straight from storage.
pub async fn video(State(app): State<App>, Path((token, item)): Path<(String, i64)>) -> Result<Response, ApiError> {
    let Some(delivery) = delivery_by_token(&app, &token).await? else {
        return Ok(message(StatusCode::NOT_FOUND, "Not found", "That link doesn't exist."));
    };
    let row = sqlx::query(
        "SELECT i.object_key, i.kind, i.court, i.made_at, i.expires_at <= $3 FROM delivery_item di JOIN item i ON i.id = di.item_id
         WHERE di.delivery_id = $1 AND i.id = $2 AND i.withdrawn_at IS NULL AND i.uploaded_at IS NOT NULL",
    )
    .bind(delivery)
    .bind(item)
    .bind(crate::now())
    .fetch_optional(&app.db)
    .await?;
    let Some(r) = row else {
        return Ok(message(StatusCode::GONE, "No longer available", "This video is no longer available."));
    };
    if r.get::<bool, _>(4) {
        return Ok(message(StatusCode::GONE, "These videos have expired", "These videos have expired."));
    }
    let made: DateTime<Utc> = r.get(3);
    let filename = format!("{}-{}-{}.mp4", r.get::<&str, _>(1), slug(r.get::<&str, _>(2)), manila(made).format("%Y%m%d-%H%M"));
    let url = app.store.download_url(r.get::<&str, _>(0), &filename).await.map_err(ApiError::store)?;
    Ok(Redirect::to(&url).into_response())
}

fn slug(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    out.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

#[derive(Template)]
#[template(path = "never.html")]
struct NeverPage<'a> {
    token: &'a str,
    done: bool,
}

async fn never_hash(app: &App, token: &str) -> Result<Option<String>, ApiError> {
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(None);
    }
    Ok(sqlx::query_scalar("SELECT email_hash FROM never_link WHERE token = $1").bind(token).fetch_optional(&app.db).await?)
}

pub async fn never_form(State(app): State<App>, Path(token): Path<String>) -> Result<Response, ApiError> {
    if never_hash(&app, &token).await?.is_none() {
        return Ok(message(StatusCode::NOT_FOUND, "Not found", "That link doesn't exist."));
    }
    Ok(Html(NeverPage { token: &token, done: false }.render().map_err(|e| ApiError::Internal(e.to_string()))?).into_response())
}

/// "This isn't me — don't email me again": no venue emails this address again (FR-25).
pub async fn never_confirm(State(app): State<App>, Path(token): Path<String>) -> Result<Response, ApiError> {
    let Some(hash) = never_hash(&app, &token).await? else {
        return Ok(message(StatusCode::NOT_FOUND, "Not found", "That link doesn't exist."));
    };
    sqlx::query("INSERT INTO blocked (email_hash) VALUES ($1) ON CONFLICT DO NOTHING").bind(hash).execute(&app.db).await?;
    tracing::info!("an address asked never to be emailed again");
    Ok(Html(NeverPage { token: &token, done: true }.render().map_err(|e| ApiError::Internal(e.to_string()))?).into_response())
}

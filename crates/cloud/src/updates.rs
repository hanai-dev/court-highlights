//! Fleet updates (FR-51, SC-9): an Admin publishes a signed update file; boxes are offered the newest release at
//! check-in, one box of a venue at a time; a box that takes an update back stops the release for its whole venue.
//! Boxes check the owner's signature themselves (crates/box/src/update.rs), so this service only stores and offers.

use crate::ApiError;
use crate::api::BoxAuth;
use crate::fleet::{Page, html, nav, when};
use crate::fleet_auth::{Denied, FleetUser};
use crate::{App, hex_sha256};
use askama::Template;
use axum::extract::{Multipart, State};
use axum::response::{IntoResponse, Redirect};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const MAX_BYTES: usize = 200 * 1024 * 1024;
const MAGIC: &[u8] = b"CHUPDATE 1\n";
/// An offer nobody answered for this long no longer holds up the venue's other boxes.
const OFFER_MINUTES: i64 = 15;

#[derive(Deserialize)]
struct Manifest {
    version: String,
    sha256: String,
}

/// The version of a well-formed update file whose contents match its manifest.
fn check(file: &[u8]) -> Result<String, &'static str> {
    let rest = file.strip_prefix(MAGIC).ok_or("This isn't a Court Highlights update file.")?;
    let end = rest.iter().position(|b| *b == b'\n').ok_or("The update file is damaged.")?;
    let m: Manifest = serde_json::from_slice(&rest[..end]).map_err(|_| "The update file is damaged.")?;
    let ok = !m.version.is_empty() && m.version.len() <= 40 && m.version.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b));
    if !ok || hex_sha256(&rest[end + 1..]) != m.sha256 {
        return Err("The update file is damaged.");
    }
    Ok(m.version)
}

#[derive(Deserialize)]
pub struct Outcome {
    version: String,
    outcome: String,
}

#[derive(Serialize)]
pub struct Offer {
    version: String,
    url: String,
}

/// What a box reports after an update: kept, taken back, or refused (bad signature).
pub async fn record(app: &App, auth: &BoxAuth, o: &Outcome) -> Result<(), ApiError> {
    let action = match o.outcome.as_str() {
        "updated" => "box updated",
        "rolled back" => "update rolled back",
        "refused" => "update refused",
        _ => return Ok(()),
    };
    let now = crate::now();
    sqlx::query(
        "INSERT INTO box_update (box_id, version, offered_at, outcome, finished_at) VALUES ($1, $2, $4, $3, $4)
         ON CONFLICT (box_id, version) DO UPDATE SET outcome = EXCLUDED.outcome, finished_at = EXCLUDED.finished_at",
    )
    .bind(auth.box_id)
    .bind(o.version.chars().take(40).collect::<String>())
    .bind(&o.outcome)
    .bind(now)
    .execute(&app.db)
    .await?;
    let name: String = sqlx::query_scalar("SELECT name FROM box WHERE id = $1").bind(auth.box_id).fetch_one(&app.db).await?;
    crate::fleet::log(app, &name, action, &o.version, "").await?;
    Ok(())
}

/// The newest release for a box not on it yet — unless another box of its venue is mid-update, or any box of the
/// venue took this release back or refused it.
pub async fn offer(app: &App, auth: &BoxAuth, running: &str) -> Result<Option<Offer>, ApiError> {
    let Some(r) = sqlx::query("SELECT version, object_key FROM release ORDER BY published_at DESC, id DESC LIMIT 1").fetch_optional(&app.db).await? else {
        return Ok(None);
    };
    let (version, key): (String, String) = (r.get(0), r.get(1));
    if version == running {
        return Ok(None);
    }
    let now = crate::now();
    let held: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM box_update u JOIN box b ON b.id = u.box_id
           WHERE b.venue_id = $1 AND u.version = $2
             AND (u.outcome IN ('rolled back', 'refused') OR (u.outcome IS NULL AND u.box_id <> $3 AND u.offered_at > $4)))",
    )
    .bind(auth.venue_id)
    .bind(&version)
    .bind(auth.box_id)
    .bind(now - chrono::Duration::minutes(OFFER_MINUTES))
    .fetch_one(&app.db)
    .await?;
    if held {
        return Ok(None);
    }
    sqlx::query("INSERT INTO box_update (box_id, version, offered_at) VALUES ($1, $2, $3) ON CONFLICT (box_id, version) DO UPDATE SET offered_at = EXCLUDED.offered_at WHERE box_update.outcome IS NULL")
        .bind(auth.box_id)
        .bind(&version)
        .bind(now)
        .execute(&app.db)
        .await?;
    Ok(Some(Offer { version, url: app.store.box_download_url(&key).await.map_err(ApiError::store)? }))
}

pub struct ReleaseLine {
    pub version: String,
    pub published: String,
    pub by: String,
}

pub struct ProgressLine {
    pub venue: String,
    pub box_name: String,
    pub version: String,
    pub outcome: String,
}

#[derive(Template)]
#[template(path = "fleet/updates.html")]
struct UpdatesPage {
    nav: crate::fleet::Nav,
    releases: Vec<ReleaseLine>,
    progress: Vec<ProgressLine>,
    error: Option<&'static str>,
}

async fn updates_page(app: &App, u: &FleetUser, error: Option<&'static str>) -> Page {
    let releases = sqlx::query("SELECT version, published_at, published_by FROM release ORDER BY published_at DESC, id DESC")
        .fetch_all(&app.db)
        .await?
        .into_iter()
        .map(|r| ReleaseLine { version: r.get(0), published: when(r.get(1)), by: r.get(2) })
        .collect();
    let progress = sqlx::query(
        "SELECT v.name, b.name, u.version, COALESCE(u.outcome, 'installing') FROM box_update u JOIN box b ON b.id = u.box_id
         JOIN venue v ON v.id = b.venue_id WHERE b.wiped_at IS NULL ORDER BY u.offered_at DESC LIMIT 200",
    )
    .fetch_all(&app.db)
    .await?
    .into_iter()
    .map(|r| ProgressLine { venue: r.get(0), box_name: r.get(1), version: r.get(2), outcome: r.get(3) })
    .collect();
    let mut res = html(UpdatesPage { nav: nav(u, "updates"), releases, progress, error })?;
    if error.is_some() {
        *res.status_mut() = axum::http::StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok(res)
}

pub async fn page(State(app): State<App>, u: FleetUser) -> Page {
    u.admin()?;
    updates_page(&app, &u, None).await
}

pub async fn publish(State(app): State<App>, u: FleetUser, mut form: Multipart) -> Page {
    u.admin()?;
    let mut file = Vec::new();
    while let Some(field) = form.next_field().await.map_err(|e| Denied::Failed(e.to_string()))? {
        if field.name() == Some("file") {
            file = field.bytes().await.map_err(|e| Denied::Failed(e.to_string()))?.to_vec();
        }
    }
    let version = match check(&file) {
        Ok(v) => v,
        Err(e) => return updates_page(&app, &u, Some(e)).await,
    };
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM release WHERE version = $1)").bind(&version).fetch_one(&app.db).await?;
    if exists {
        return updates_page(&app, &u, Some("That version is already published.")).await;
    }
    let key = format!("releases/{version}.chupdate");
    let bytes = file.len() as i64;
    let sha = hex_sha256(&file);
    app.store.put(&key, file).await.map_err(|e| Denied::Failed(format!("store: {e}")))?;
    sqlx::query("INSERT INTO release (version, sha256, object_key, bytes, published_by, published_at) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(&version)
        .bind(sha)
        .bind(&key)
        .bind(bytes)
        .bind(&u.username)
        .bind(crate::now())
        .execute(&app.db)
        .await?;
    crate::fleet::log(&app, &u.username, "published update", &version, "").await?;
    Ok(Redirect::to("/fleet/updates").into_response())
}

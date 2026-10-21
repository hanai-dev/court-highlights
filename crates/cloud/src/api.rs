//! Box → service API (HLD §3). Every call carries the box's own key; a revoked key is refused and nothing is stored
//! (FR-48). Uploads go straight to the object store through presigned URLs.

use crate::{ApiError, App, hex_sha256, random_token};
use axum::extract::{FromRequestParts, Path, State};
use axum::http::{StatusCode, request::Parts};
use axum::{Json, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub struct BoxAuth {
    pub box_id: i64,
    pub venue_id: i64,
    pub online: bool,
}

impl FromRequestParts<App> for BoxAuth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Self::Rejection> {
        let key = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .filter(|k| k.len() == 64)
            .ok_or(ApiError::Unauthorized)?;
        let row =
            sqlx::query("SELECT b.id, b.venue_id, v.online_on FROM box b JOIN venue v ON v.id = b.venue_id WHERE b.key_hash = $1 AND b.revoked_at IS NULL")
                .bind(hex_sha256(key))
                .fetch_optional(&app.db)
                .await?
                .ok_or(ApiError::Unauthorized)?;
        Ok(BoxAuth { box_id: row.get(0), venue_id: row.get(1), online: row.get(2) })
    }
}

#[derive(Deserialize)]
pub struct Checkin {
    version: String,
    disk_pct: i32,
    temperature_c: Option<f32>,
    courts_recording: i32,
    courts_total: i32,
    /// The outcome of the box's last update, once (FR-51).
    #[serde(default)]
    update: Option<crate::updates::Outcome>,
}

#[derive(Serialize)]
pub struct State_ {
    online_on: bool,
    /// A release offered to this box; venues switched off take no updates (FR-27).
    #[serde(skip_serializing_if = "Option::is_none")]
    update: Option<crate::updates::Offer>,
}

/// Health report every minute (FR-49); the answer carries the venue's online switch (FR-27).
pub async fn checkin(State(app): State<App>, auth: BoxAuth, Json(c): Json<Checkin>) -> Result<Json<State_>, ApiError> {
    sqlx::query("INSERT INTO checkin (box_id, at, version, disk_pct, temperature_c, courts_recording, courts_total) VALUES ($1, $7, $2, $3, $4, $5, $6)")
        .bind(auth.box_id)
        .bind(c.version.chars().take(40).collect::<String>())
        .bind(c.disk_pct.clamp(0, 100))
        .bind(c.temperature_c)
        .bind(c.courts_recording.max(0))
        .bind(c.courts_total.max(0))
        .bind(crate::now())
        .execute(&app.db)
        .await?;
    crate::metrics::add(&crate::metrics::CHECKINS, 1);
    tracing::info!(box_id = auth.box_id, "check-in");
    if let Some(o) = &c.update {
        crate::updates::record(&app, &auth, o).await?;
    }
    let update = if auth.online { crate::updates::offer(&app, &auth, &c.version).await? } else { None };
    Ok(Json(State_ { online_on: auth.online, update }))
}

#[derive(Deserialize)]
pub struct NewItem {
    #[serde(rename = "ref")]
    box_ref: String,
    kind: String,
    court: String,
    made_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct Upload {
    id: i64,
    upload_url: Option<String>,
}

/// Registers an item (idempotent on the box's own reference) and returns where to PUT it.
pub async fn create_item(State(app): State<App>, auth: BoxAuth, Json(n): Json<NewItem>) -> Result<Json<Upload>, ApiError> {
    if !auth.online {
        return Err(ApiError::Offline);
    }
    if !["tap", "highlight", "reel", "game"].contains(&n.kind.as_str()) || n.box_ref.is_empty() || n.box_ref.len() > 80 || n.court.len() > 60 {
        return Err(ApiError::Bad("unknown item"));
    }
    if n.expires_at <= crate::now() {
        return Err(ApiError::Expired);
    }
    let key = format!("{}/{}.mp4", auth.venue_id, random_token());
    let row = sqlx::query(
        "INSERT INTO item (box_id, venue_id, box_ref, kind, court, made_at, expires_at, object_key) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (box_id, box_ref) DO UPDATE SET box_ref = EXCLUDED.box_ref
         RETURNING id, object_key, uploaded_at IS NOT NULL",
    )
    .bind(auth.box_id)
    .bind(auth.venue_id)
    .bind(&n.box_ref)
    .bind(&n.kind)
    .bind(&n.court)
    .bind(n.made_at)
    .bind(n.expires_at)
    .bind(&key)
    .fetch_one(&app.db)
    .await?;
    let (id, object_key, uploaded): (i64, String, bool) = (row.get(0), row.get(1), row.get(2));
    let upload_url = if uploaded { None } else { Some(app.store.upload_url(&object_key).await.map_err(ApiError::store)?) };
    Ok(Json(Upload { id, upload_url }))
}

async fn own_item(app: &App, auth: &BoxAuth, id: i64) -> Result<(String, bool), ApiError> {
    let row = sqlx::query("SELECT object_key, withdrawn_at IS NOT NULL FROM item WHERE id = $1 AND box_id = $2")
        .bind(id)
        .bind(auth.box_id)
        .fetch_optional(&app.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok((row.get(0), row.get(1)))
}

/// The box finished its PUT: the object must really be there.
pub async fn item_done(State(app): State<App>, auth: BoxAuth, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    let (key, withdrawn) = own_item(&app, &auth, id).await?;
    if withdrawn {
        return Err(ApiError::NotFound);
    }
    let bytes = app.store.size(&key).await.ok_or(ApiError::Bad("upload not found"))?;
    sqlx::query("UPDATE item SET uploaded_at = now(), bytes = $1 WHERE id = $2").bind(bytes).bind(id).execute(&app.db).await?;
    crate::metrics::add(&crate::metrics::UPLOADS, 1);
    tracing::info!(item = id, box_id = auth.box_id, bytes, "item uploaded");
    Ok(StatusCode::NO_CONTENT)
}

/// A clip hidden or deleted after it was sent: the online copy goes too (FR-26).
pub async fn withdraw(State(app): State<App>, auth: BoxAuth, Path(id): Path<i64>) -> Result<StatusCode, ApiError> {
    let (key, _) = own_item(&app, &auth, id).await?;
    app.store.delete(&key).await.map_err(ApiError::store)?;
    sqlx::query("UPDATE item SET withdrawn_at = now() WHERE id = $1").bind(id).execute(&app.db).await?;
    tracing::info!(item = id, box_id = auth.box_id, "item withdrawn");
    Ok(StatusCode::NO_CONTENT)
}

/// Wipe box (FR-55): the box is leaving its venue. Its key stops working at once, its health records go, and the
/// fleet console no longer lists it. Items already sent stay until their own expiry.
pub async fn leave(State(app): State<App>, auth: BoxAuth) -> Result<StatusCode, ApiError> {
    let now = crate::now();
    let name: String = sqlx::query_scalar("UPDATE box SET wiped_at = $1, revoked_at = COALESCE(revoked_at, $1) WHERE id = $2 RETURNING name")
        .bind(now)
        .bind(auth.box_id)
        .fetch_one(&app.db)
        .await?;
    sqlx::query("DELETE FROM checkin WHERE box_id = $1").bind(auth.box_id).execute(&app.db).await?;
    crate::fleet::log(&app, "box", "wiped box", &name, "").await?;
    tracing::info!(box_id = auth.box_id, "box wiped and left the fleet");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct NewDelivery {
    #[serde(rename = "ref")]
    box_ref: String,
    title: String,
    items: Vec<i64>,
    recipients: Vec<String>,
}

#[derive(Serialize)]
pub struct Sent {
    sent: usize,
    skipped: usize,
}

/// Emails each address once for this game (or clip), then forgets the addresses: they are never stored here.
pub async fn deliver(State(app): State<App>, auth: BoxAuth, Json(d): Json<NewDelivery>) -> Result<impl IntoResponse, ApiError> {
    if !auth.online {
        return Err(ApiError::Offline);
    }
    if d.recipients.len() > ch_core::MAX_RECIPIENTS || d.items.is_empty() || d.items.len() > 50 || d.title.len() > 120 {
        return Err(ApiError::Bad("too many addresses or items"));
    }
    let ready: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM item WHERE id = ANY($1) AND box_id = $2 AND uploaded_at IS NOT NULL AND withdrawn_at IS NULL AND expires_at > $3",
    )
    .bind(&d.items)
    .bind(auth.box_id)
    .bind(crate::now())
    .fetch_one(&app.db)
    .await?;
    if ready == 0 {
        return Err(ApiError::Expired);
    }
    let mut tx = app.db.begin().await?;
    let created = sqlx::query(
        "INSERT INTO delivery (box_id, venue_id, box_ref, token, title) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (box_id, box_ref) DO NOTHING RETURNING id, token",
    )
    .bind(auth.box_id)
    .bind(auth.venue_id)
    .bind(&d.box_ref)
    .bind(random_token())
    .bind(&d.title)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = created else {
        // Already sent (the box retried after a lost answer): never email twice.
        return Ok((StatusCode::OK, Json(Sent { sent: 0, skipped: d.recipients.len() })));
    };
    let (delivery, token): (i64, String) = (row.get(0), row.get(1));
    for (position, item) in d.items.iter().enumerate() {
        sqlx::query(
            "INSERT INTO delivery_item (delivery_id, item_id, position) SELECT $1, id, $3 FROM item WHERE id = $2 AND box_id = $4 ON CONFLICT DO NOTHING",
        )
        .bind(delivery)
        .bind(item)
        .bind(position as i32)
        .bind(auth.box_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let (sent, skipped) = crate::player::send_delivery(&app, delivery, &token, &d.recipients).await?;
    Ok((StatusCode::CREATED, Json(Sent { sent, skipped })))
}

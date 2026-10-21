//! Court Highlights online service (HLD §3): box API, take-home pages and email, clean-up; the fleet console
//! is added on the same router.

mod api;
mod fleet;
mod fleet_auth;
mod mail;
mod player;
mod store;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use sha2::{Digest, Sha256};
use std::time::Duration;

#[derive(Clone)]
pub struct App {
    pub db: sqlx::PgPool,
    pub store: store::Store,
    pub mail: mail::Mailer,
    /// Where players open their links, e.g. https://videos.example.ph (no trailing slash).
    pub public_url: String,
    /// Mixed into every stored email hash so a leaked table can't be reversed with a word list.
    pub pepper: String,
    /// Encrypts fleet users' authenticator secrets (CH_SECRET_KEY, 64 hex characters).
    pub secret_key: [u8; 32],
    /// scheme://host[:port] of CH_PUBLIC_URL: fleet writes must come from here.
    pub origin: String,
    /// When set, /readyz answers only with this bearer token (CH_READY_TOKEN).
    pub ready_token: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NotFound,
    Offline,
    Expired,
    Bad(&'static str),
    Internal(String),
}

impl ApiError {
    pub fn store(e: store::StoreError) -> Self {
        Self::Internal(format!("store: {e}"))
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unknown or revoked box key"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not found"),
            Self::Offline => (StatusCode::CONFLICT, "online features are off for this venue"),
            Self::Expired => (StatusCode::GONE, "expired"),
            Self::Bad(m) => (StatusCode::BAD_REQUEST, *m),
            Self::Internal(detail) => {
                tracing::error!(%detail, "request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error")
            }
        };
        (status, axum::Json(serde_json::json!({ "error": msg }))).into_response()
    }
}

static TEST_OFFSET_MS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// The service's "now" for expiry decisions; test builds can move it forward like the box's clock.
pub fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now() + chrono::Duration::milliseconds(TEST_OFFSET_MS.load(std::sync::atomic::Ordering::Relaxed))
}

#[cfg(feature = "test-hooks")]
fn test_hooks(r: Router<App>) -> Router<App> {
    #[derive(serde::Deserialize)]
    struct Advance {
        ms: i64,
    }
    r.route("/__test/clock", post(|axum::Form(a): axum::Form<Advance>| async move {
        TEST_OFFSET_MS.fetch_add(a.ms, std::sync::atomic::Ordering::Relaxed);
        now().to_rfc3339()
    }))
}

#[cfg(not(feature = "test-hooks"))]
fn test_hooks(r: Router<App>) -> Router<App> {
    r
}

pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hex_sha256(s: impl AsRef<[u8]>) -> String {
    Sha256::digest(s.as_ref()).iter().map(|b| format!("{b:02x}")).collect()
}

pub fn email_hash(pepper: &str, email: &str) -> String {
    hex_sha256(format!("{pepper}:{}", email.trim().to_lowercase()))
}

fn need(name: &str) -> Result<String, String> {
    std::env::var(name).ok().filter(|v| !v.is_empty()).ok_or_else(|| format!("{name} is not set"))
}

fn opt(name: &str, default: &str) -> String {
    std::env::var(name).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.into())
}

async fn security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
    );
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // same-origin, not no-referrer: Chrome sends `Origin: null` on form posts under no-referrer, defeating the fleet's
    // same-origin check; same-origin still never sends the token-bearing /v/ addresses to another site.
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

/// Readiness for the platform (HOST-health): database and object store reachable. The answer never says which
/// part failed; the log does.
async fn readyz(axum::extract::State(app): axum::extract::State<App>, headers: axum::http::HeaderMap) -> Response {
    if let Some(token) = &app.ready_token {
        let given = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
        if given != Some(token.as_str()) {
            return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        }
    }
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&app.db).await.is_ok();
    let store = app.store.reachable().await;
    if db && store {
        (StatusCode::OK, "ready").into_response()
    } else {
        tracing::error!(database = db, object_store = store, "not ready");
        (StatusCode::SERVICE_UNAVAILABLE, "not ready").into_response()
    }
}

/// `ch-cloud healthcheck`: exit 0 when this container's service answers /healthz (for HEALTHCHECK in an image
/// with no shell or curl).
fn healthcheck() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{Read, Write};
    let addr = opt("CH_LISTEN", "0.0.0.0:8080").replace("0.0.0.0", "127.0.0.1");
    let mut s = std::net::TcpStream::connect_timeout(&addr.parse()?, Duration::from_secs(2))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    s.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut answer = String::new();
    s.read_to_string(&mut answer)?;
    if answer.starts_with("HTTP/1.0 200") || answer.starts_with("HTTP/1.1 200") { Ok(()) } else { Err("unhealthy".into()) }
}

async fn stylesheet() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], include_str!("../../box/static/app.css"))
}

fn router(app: App) -> Router {
    test_hooks(Router::new())
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route("/static/app.css", get(stylesheet))
        .route("/api/v1/checkin", post(api::checkin))
        .route("/api/v1/items", post(api::create_item))
        .route("/api/v1/items/{id}/done", post(api::item_done))
        .route("/api/v1/items/{id}", delete(api::withdraw))
        .route("/api/v1/deliveries", post(api::deliver))
        .route("/v/{token}", get(player::videos))
        .route("/v/{token}/{item}", get(player::video))
        .route("/never/{token}", get(player::never_form).post(player::never_confirm))
        .route("/fleet", get(fleet::health))
        .route("/fleet/signin", get(fleet::signin_form).post(fleet::signin))
        .route("/fleet/signout", post(fleet::signout))
        .route("/fleet/account", get(fleet::account).post(fleet::change_password))
        .route("/fleet/venues", get(fleet::venue_list).post(fleet::venue_add))
        .route("/fleet/venues/{id}", get(fleet::venue))
        .route("/fleet/venues/{id}/boxes", post(fleet::register_box))
        .route("/fleet/venues/{id}/switch", post(fleet::switch))
        .route("/fleet/boxes/{id}/revoke", post(fleet::revoke_box))
        .route("/fleet/users", get(fleet::users).post(fleet::user_add))
        .route("/fleet/log", get(fleet::fleet_log))
        .layer(middleware::from_fn_with_state(app.clone(), fleet_auth::same_origin))
        .layer(middleware::from_fn(security_headers))
        .with_state(app)
}

/// Expired online copies are deleted at their expiry (FR-24, FR-45); health records after 90 days (FR-49).
fn spawn_cleanup(app: App) {
    tokio::spawn(async move {
        loop {
            match sqlx::query_as::<_, (i64, String)>("SELECT id, object_key FROM item WHERE expires_at <= $1 AND withdrawn_at IS NULL LIMIT 500")
                .bind(now())
                .fetch_all(&app.db)
                .await
            {
                Ok(rows) => {
                    for (id, key) in rows {
                        if let Err(e) = app.store.delete(&key).await {
                            tracing::warn!(item = id, error = %e, "expired object not deleted yet");
                            continue;
                        }
                        let _ = sqlx::query("UPDATE item SET withdrawn_at = $2 WHERE id = $1").bind(id).bind(now()).execute(&app.db).await;
                    }
                }
                Err(e) => tracing::error!(error = %e, "clean-up query failed"),
            }
            let _ = sqlx::query("DELETE FROM checkin WHERE at < $1").bind(now() - chrono::Duration::days(90)).execute(&app.db).await;
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn connect() -> Result<sqlx::PgPool, Box<dyn std::error::Error>> {
    let db = sqlx::postgres::PgPoolOptions::new().max_connections(10).connect(&need("DATABASE_URL")?).await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    Ok(db)
}

fn secret_key() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let hex = need("CH_SECRET_KEY")?;
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| hex.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()))
        .collect::<Option<_>>()
        .ok_or("CH_SECRET_KEY must be hex")?;
    bytes.try_into().map_err(|_| "CH_SECRET_KEY must be 64 hex characters (32 bytes)".into())
}

/// `ch-cloud create-admin <username>`: the first fleet Admin. Prints a temporary password and the authenticator key.
async fn create_admin(username: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = connect().await?;
    let c = fleet::create_user(&db, &secret_key()?, username, "admin").await.map_err(|_| "could not create the user (does it exist?)")?;
    println!("username: {}\ntemporary password: {}\nauthenticator key: {}", c.username, c.password, c.secret);
    Ok(())
}

/// `ch-cloud add-box "<venue>" <box name>`: registers a box and prints its key once.
// ponytail: CLI registration until the fleet console's Installer page exists (FR-48); then installer-only fallback.
async fn add_box(venue: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = connect().await?;
    let venue_id: i64 = match sqlx::query_scalar("SELECT id FROM venue WHERE name = $1").bind(venue).fetch_optional(&db).await? {
        Some(id) => id,
        None => sqlx::query_scalar("INSERT INTO venue (name) VALUES ($1) RETURNING id").bind(venue).fetch_one(&db).await?,
    };
    let key = random_token();
    sqlx::query("INSERT INTO box (venue_id, name, key_hash, registered_by) VALUES ($1, $2, $3, 'cli')").bind(venue_id).bind(name).bind(hex_sha256(&key)).execute(&db).await?;
    println!("{key}");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! { _ = ctrl_c => {}, _ = term.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Logs on stderr: stdout carries command output (the box key from add-box).
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    rustls::crypto::ring::default_provider().install_default().map_err(|_| "TLS crypto provider already installed")?;
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("healthcheck") {
        return healthcheck();
    }
    if args.get(1).map(String::as_str) == Some("create-admin") {
        let Some(username) = args.get(2) else { return Err("usage: ch-cloud create-admin <username>".into()) };
        return create_admin(username).await;
    }
    if args.get(1).map(String::as_str) == Some("add-box") {
        let (Some(venue), Some(name)) = (args.get(2), args.get(3)) else { return Err("usage: ch-cloud add-box <venue> <box name>".into()) };
        return add_box(venue, name).await;
    }

    let pepper = need("CH_EMAIL_PEPPER")?;
    if pepper.len() < 32 {
        return Err("CH_EMAIL_PEPPER must be at least 32 characters".into());
    }
    let s3 = need("CH_S3_ENDPOINT")?;
    let store = store::Store::new(&s3, &opt("CH_S3_PUBLIC_ENDPOINT", &s3), &opt("CH_S3_REGION", "auto"), &need("CH_S3_BUCKET")?, &need("CH_S3_KEY")?, &need("CH_S3_SECRET")?);
    store.ensure_bucket().await.map_err(|e| format!("object store unreachable: {e}"))?;
    let ses_endpoint = std::env::var("CH_SES_ENDPOINT").ok().filter(|v| !v.is_empty());
    let mail = mail::Mailer::new(ses_endpoint.as_deref(), &opt("CH_SES_REGION", "ap-southeast-1"), &need("CH_SES_KEY")?, &need("CH_SES_SECRET")?, &need("CH_MAIL_FROM")?);
    let public_url = need("CH_PUBLIC_URL")?.trim_end_matches('/').to_string();
    let origin = public_url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let ready_token = std::env::var("CH_READY_TOKEN").ok().filter(|t| !t.is_empty());
    let app = App { db: connect().await?, store, mail, public_url, pepper, secret_key: secret_key()?, origin, ready_token };
    spawn_cleanup(app.clone());

    let listener = tokio::net::TcpListener::bind(opt("CH_LISTEN", "0.0.0.0:8080")).await?;
    tracing::info!(addr = %listener.local_addr()?, "online service listening");
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

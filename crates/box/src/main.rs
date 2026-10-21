//! Court box server. See docs/HLD.md §2.

mod auth;
mod clips;
mod clock;
mod courts;
mod db;
mod footage;
mod games;
mod janitor;
mod judge;
mod library;
mod media;
mod recorder;
mod tls;
mod venue;
mod web;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let data_dir = PathBuf::from(env("CH_DATA_DIR", "/var/lib/court-highlights"));
    let http: SocketAddr = env("CH_HTTP_ADDR", "0.0.0.0:80").parse()?;
    let https: SocketAddr = env("CH_HTTPS_ADDR", "0.0.0.0:443").parse()?;
    std::fs::create_dir_all(&data_dir)?;

    gstreamer::init()?;
    let db = db::Db::open(&data_dir.join("box.db"))?;
    let correction: Option<i64> = rusqlite::OptionalExtension::optional(db.conn().query_row("SELECT clock_correction_ms FROM venue WHERE id = 1", [], |r| r.get(0)))?;
    clock::load_correction(correction.unwrap_or(0));
    rustls::crypto::ring::default_provider().install_default().map_err(|_| "TLS crypto provider already installed")?;
    let (cert, key) = tls::load_or_create(&data_dir)?;
    let recorders = recorder::Recorders::new(db.clone(), data_dir.join("footage"));
    let dropped = recorders.discard_unfinished()?;
    if dropped > 0 {
        tracing::warn!(dropped, "dropped footage fragments left open by an unclean stop");
    }
    recorders.spawn_supervisor();
    let state = web::AppState { db, recorders, data: data_dir.clone(), fingerprint: tls::fingerprint(&cert)? };
    clips::resume_saving(&state)?;
    games::recover(&state)?;
    games::spawn_watch(state.clone());
    janitor::spawn(state.clone());
    let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert.into_bytes(), key.into_bytes()).await?;

    let (player_handle, staff_handle) = (axum_server::Handle::new(), axum_server::Handle::new());
    let (p, s) = (player_handle.clone(), staff_handle.clone());
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutting down");
        p.graceful_shutdown(Some(Duration::from_secs(10)));
        s.graceful_shutdown(Some(Duration::from_secs(10)));
    });
    tracing::info!(%http, %https, "court box listening");
    let recorders = state.recorders.clone();
    let player = axum_server::bind(http).handle(player_handle).serve(web::player_router(state.clone()).into_make_service());
    let staff = axum_server::bind_rustls(https, tls).handle(staff_handle).serve(web::staff_router(state).into_make_service());
    tokio::try_join!(player, staff)?;
    recorders.stop_all().await;
    tracing::info!("stopped cleanly");
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

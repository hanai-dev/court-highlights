//! Court box server. See docs/HLD.md §2.

mod auth;
mod backup;
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
mod remote;
mod sync;
mod tls;
mod uplink;
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
    let box_name = env("CH_BOX_NAME", "court-box");
    uplink::set_config(&db, "box_name", &box_name)?;
    let correction = uplink::config(&db, "clock_correction_ms")?.and_then(|c| c.parse().ok()).unwrap_or(0);
    clock::load_correction(correction);
    let venue = sync::from_env(&box_name).map_err(|e| e.to_string())?;
    rustls::crypto::ring::default_provider().install_default().map_err(|_| "TLS crypto provider already installed")?;
    let (cert, key) = tls::load_or_create(&data_dir)?;
    let recorders = recorder::Recorders::new(db.clone(), data_dir.join("footage"));
    let dropped = recorders.discard_unfinished()?;
    if dropped > 0 {
        tracing::warn!(dropped, "dropped footage fragments left open by an unclean stop");
    }
    recorders.spawn_supervisor();
    let state = web::AppState { db, recorders, data: data_dir.clone(), fingerprint: tls::fingerprint(&cert)?, venue, peers: Default::default(), box_name };
    clips::resume_saving(&state)?;
    games::recover(&state)?;
    games::spawn_watch(state.clone());
    janitor::spawn(state.clone());
    uplink::spawn(state.clone());
    sync::spawn(state.clone());
    if state.venue.is_some() {
        let addr: SocketAddr = env("CH_SYNC_ADDR", "0.0.0.0:8444").parse()?;
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let router = sync::router(state.clone());
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!(error = %e, "sync listener stopped");
            }
        });
        tracing::info!(%addr, "venue sync listening");
    }
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

//! Court box server. See docs/HLD.md §2.

mod auth;
mod clock;
mod db;
mod tls;
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

    let state = web::AppState { db: db::Db::open(&data_dir.join("box.db"))? };
    rustls::crypto::ring::default_provider().install_default().map_err(|_| "TLS crypto provider already installed")?;
    let (cert, key) = tls::load_or_create(&data_dir)?;
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
    let player = axum_server::bind(http).handle(player_handle).serve(web::player_router(state.clone()).into_make_service());
    let staff = axum_server::bind_rustls(https, tls).handle(staff_handle).serve(web::staff_router(state).into_make_service());
    tokio::try_join!(player, staff)?;
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

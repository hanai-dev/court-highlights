//! Phase 2 toolchain spike: proves the pinned stack (GStreamer 1.28 via gstreamer-rs, ONNX Runtime via ort,
//! retina, SQLite, axum) builds and links on the target OS. Replaced by the real box server in Phase 5.

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    gstreamer::init()?;
    let env = ort::environment::init().build()?;
    let _ort = ort::session::Session::builder(&env)?;
    let _rtsp = retina::client::SessionOptions::default();
    let db = rusqlite::Connection::open_in_memory()?;
    let one: i64 = db.query_row("select 1", [], |r| r.get(0))?;
    let app = axum::Router::new().route("/healthz", axum::routing::get(|| async { "ok" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    println!(
        "toolchain ok: {} · sqlite {} · listening {}",
        gstreamer::version_string(),
        one,
        listener.local_addr()?
    );
    if std::env::args().any(|a| a == "--serve") {
        axum::serve(listener, app).await?;
    }
    Ok(())
}

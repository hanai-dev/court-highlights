//! Calls to sibling boxes for the venue console (FR-41): signed with the venue key, answers sealed with it.

use crate::web::{AppError, AppState};
use serde::de::DeserializeOwned;
use std::time::Duration;

fn client() -> reqwest::Client {
    reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client")
}

/// The sync address of the sibling called `name`, if this box has heard from it.
pub fn peer_url(app: &AppState, name: &str) -> Option<String> {
    app.peers.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(_, p)| p.name == name).map(|(url, _)| url.clone())
}

/// (sync address, box name) of every sibling heard from recently.
pub fn reachable(app: &AppState) -> Vec<(String, String)> {
    let now = crate::clock::now();
    app.peers
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|(_, p)| p.last_ok.is_some_and(|t| (now - t).num_seconds() <= 45))
        .map(|(url, p)| (url.clone(), p.name.clone()))
        .collect()
}

fn fail(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("sibling box: {e}"))
}

async fn send(app: &AppState, method: reqwest::Method, url: &str, path: &str) -> Result<reqwest::Response, AppError> {
    let venue = app.venue.as_ref().ok_or(AppError::NotFound)?;
    let res = client().request(method.clone(), format!("{url}{path}")).header("x-venue-auth", venue.sign(method.as_str(), path)).send().await.map_err(fail)?;
    match res.status() {
        reqwest::StatusCode::NOT_FOUND => Err(AppError::NotFound),
        s if s.is_success() => Ok(res),
        s => Err(fail(s)),
    }
}

pub async fn get_sealed<T: DeserializeOwned>(app: &AppState, url: &str, path: &str) -> Result<T, AppError> {
    let venue = app.venue.as_ref().ok_or(AppError::NotFound)?;
    let body = send(app, reqwest::Method::GET, url, path).await?.bytes().await.map_err(fail)?;
    let plain = venue.open(&body).ok_or_else(|| fail("answer not sealed with this venue's key"))?;
    serde_json::from_slice(&plain).map_err(fail)
}

pub async fn get(app: &AppState, url: &str, path: &str) -> Result<reqwest::Response, AppError> {
    send(app, reqwest::Method::GET, url, path).await
}

pub async fn post(app: &AppState, url: &str, path: &str) -> Result<(), AppError> {
    send(app, reqwest::Method::POST, url, path).await.map(|_| ())
}

/// Query-string value encoding for the few free-text values sent to siblings (staff usernames).
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

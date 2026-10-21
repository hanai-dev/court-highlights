//! Calls to sibling boxes for the venue console (FR-41): signed with the venue key, answers sealed with it, over
//! TLS (SEC-VENUE-SYNC). Each sibling serves sync with its own self-signed certificate; its sealed state answer
//! carries that certificate's fingerprint, and every other call to it is pinned to that fingerprint.

use crate::web::{AppError, AppState};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

/// Sibling host → certificate fingerprint, learned from sealed state answers.
// ponytail: one process-wide table; boxes have a handful of siblings.
static PINS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

/// Remembers the certificate fingerprint a sibling reported in its sealed state.
pub fn pin(sync_url: &str, fingerprint: &str) {
    if let Some(host) = url::Url::parse(sync_url).ok().and_then(|u| u.host_str().map(str::to_string)) {
        PINS.lock().unwrap_or_else(|p| p.into_inner()).insert(host, fingerprint.to_string());
    }
}

#[derive(Debug)]
struct Pinned {
    /// Only for /sync/state: its answer is sealed, so any certificate will do; it is where the pin comes from.
    any: bool,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let pinned = PINS.lock().unwrap_or_else(|p| p.into_inner()).get(name.to_str().as_ref()).cloned();
        if self.any || pinned.is_some_and(|fp| fp == crate::tls::fingerprint_der(cert)) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("sibling certificate does not match the fingerprint in its sealed state".into()))
        }
    }

    fn verify_tls12_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(&self, message: &[u8], cert: &CertificateDer<'_>, dss: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

fn tls_client(any: bool) -> reqwest::Client {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { any, provider }))
        .with_no_client_auth();
    reqwest::Client::builder().tls_backend_preconfigured(config).timeout(Duration::from_secs(10)).build().expect("http client")
}

/// For /sync/state pulls only (see Pinned::any).
pub fn state_client() -> reqwest::Client {
    tls_client(true)
}

fn client() -> reqwest::Client {
    tls_client(false)
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
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

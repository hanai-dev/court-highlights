//! Box updates (FR-51, FR-54, NFR-12). An update file — from the fleet or a USB stick — must carry the owner's
//! signature; it is written to the spare slot and started. The launcher (`tools/box-launcher.sh`) takes the box back
//! to its previous version when the new one doesn't start or isn't healthy within 10 minutes; the old version then
//! reports "Update rolled back".
//!
//! File: `CHUPDATE 1\n`, one JSON line `{version, sha256, signature}`, then the new ch-box executable. The signature is
//! Ed25519 over `ch-update:<version>:<sha256>`; boxes trust the key in `CH_RELEASE_KEY` (64 hex characters).

use crate::auth::Staff;
use crate::recorder::CourtStatus;
use crate::web::{AppError, AppState, Page, Shell, html, shell};
use crate::db;
use askama::Template;
use axum::extract::{Multipart, State};
use base64::Engine;
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::Duration;

const MAGIC: &[u8] = b"CHUPDATE 1\n";
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
/// The largest update file accepted (a release ch-box is about 20 MB).
pub const MAX_BYTES: usize = 200 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: String,
    sha256: String,
    signature: String,
}

struct Parsed<'a> {
    manifest: Manifest,
    payload: &'a [u8],
}

fn parse(file: &[u8]) -> Result<Parsed<'_>, String> {
    let rest = file.strip_prefix(MAGIC).ok_or("This isn't a Court Highlights update file.")?;
    let end = rest.iter().position(|b| *b == b'\n').ok_or("The update file is damaged.")?;
    let manifest: Manifest = serde_json::from_slice(&rest[..end]).map_err(|_| "The update file is damaged.")?;
    let ok = !manifest.version.is_empty() && manifest.version.len() <= 40 && manifest.version.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b));
    if !ok {
        return Err("The update file is damaged.".into());
    }
    Ok(Parsed { manifest, payload: &rest[end + 1..] })
}

fn signed_text(version: &str, sha256: &str) -> String {
    format!("ch-update:{version}:{sha256}")
}

/// The payload's hash and the owner's signature; returns the version.
fn verify(file: &[u8], release_key: &[u8]) -> Result<(String, Vec<u8>), String> {
    let p = parse(file)?;
    let sha = crate::auth::hex(&Sha256::digest(p.payload));
    if sha != p.manifest.sha256 {
        return Err("The update file is damaged: its contents don't match.".into());
    }
    let signature = B64.decode(&p.manifest.signature).map_err(|_| "The update file is damaged.")?;
    UnparsedPublicKey::new(&ED25519, release_key)
        .verify(signed_text(&p.manifest.version, &sha).as_bytes(), &signature)
        .map_err(|_| "This update isn't signed by the system owner. Nothing was installed.".to_string())?;
    Ok((p.manifest.version, p.payload.to_vec()))
}

/// The owner's release tool (`ch-box make-update`): signs a new ch-box executable into an update file.
pub fn make(version: &str, payload: &[u8], pkcs8: &[u8]) -> Result<Vec<u8>, String> {
    let key = Ed25519KeyPair::from_pkcs8(pkcs8).map_err(|_| "not an Ed25519 PKCS#8 key")?;
    let sha = crate::auth::hex(&Sha256::digest(payload));
    let manifest = Manifest { version: version.into(), sha256: sha.clone(), signature: B64.encode(key.sign(signed_text(version, &sha).as_bytes())) };
    let mut out = MAGIC.to_vec();
    out.extend(serde_json::to_vec(&manifest).map_err(|e| e.to_string())?);
    out.push(b'\n');
    out.extend_from_slice(payload);
    parse(&out)?;
    Ok(out)
}

/// A new release signing key (`ch-box release-key`): the PKCS#8 file to keep offline, and the public key for
/// `CH_RELEASE_KEY`.
pub fn new_key() -> Result<(Vec<u8>, String), String> {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| "no random source")?;
    let public = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|_| "key")?.public_key().as_ref().to_vec();
    Ok((pkcs8.as_ref().to_vec(), crate::auth::hex(&public)))
}

fn slots() -> Option<PathBuf> {
    std::env::var("CH_SLOTS_DIR").ok().filter(|s| !s.is_empty()).map(PathBuf::from)
}

fn release_key() -> Option<Vec<u8>> {
    let hex = std::env::var("CH_RELEASE_KEY").ok().filter(|k| k.len() == 64)?;
    (0..64).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// The version this box runs: its slot's VERSION, or the built-in one outside a slot.
pub fn current_version() -> String {
    slots()
        .and_then(|s| std::fs::read_to_string(s.join("current").join("VERSION")).ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

/// Checks an update file and writes it to the spare slot, marked pending, with `current` pointing at it. The caller
/// then stops the box so the launcher starts the new version.
pub fn stage(file: &[u8]) -> Result<String, String> {
    let dir = slots().ok_or("This box can't install updates: it has no update slots.")?;
    let key = release_key().ok_or("This box has no release key, so it can't check updates.")?;
    let (version, payload) = verify(file, &key)?;
    if version == current_version() {
        return Err(format!("This box already runs version {version}."));
    }
    if dir.join("pending").exists() {
        return Err("Another update is being installed. Try again in a few minutes.".into());
    }
    let io = |e: std::io::Error| format!("The update couldn't be written: {e}");
    let current = std::fs::read_link(dir.join("current")).map_err(io)?;
    let previous = current.to_string_lossy().to_string();
    let spare = if previous == "a" { "b" } else { "a" };
    let slot = dir.join(spare);
    if slot.exists() {
        std::fs::remove_dir_all(&slot).map_err(io)?;
    }
    std::fs::create_dir_all(&slot).map_err(io)?;
    std::fs::write(slot.join("ch-box"), &payload).map_err(io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(slot.join("ch-box"), std::fs::Permissions::from_mode(0o755)).map_err(io)?;
    }
    std::fs::write(slot.join("VERSION"), format!("{version}\n")).map_err(io)?;
    let pending = dir.join("pending.new");
    std::fs::create_dir_all(&pending).map_err(io)?;
    std::fs::write(pending.join("previous"), &previous).map_err(io)?;
    std::fs::write(pending.join("version"), &version).map_err(io)?;
    std::fs::write(pending.join("started"), chrono::Utc::now().timestamp().to_string()).map_err(io)?;
    std::fs::rename(&pending, dir.join("pending")).map_err(io)?;
    #[cfg(unix)]
    {
        let link = dir.join("current.new");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(spare, &link).map_err(io)?;
        std::fs::rename(&link, dir.join("current")).map_err(io)?;
    }
    Ok(version)
}

/// What the box tells the fleet at its next check-in: `{version, outcome}`.
pub const REPORT: &str = "update_report";
const STATUS: &str = "update_status";

fn record(app: &AppState, action: &str, version: &str, detail: &str, outcome: &str) -> Result<(), AppError> {
    let status = if outcome == "updated" { format!("Updated to {version}.") } else { format!("Update rolled back: version {version} {detail}.") };
    crate::uplink::set_config(&app.db, STATUS, &status)?;
    crate::uplink::set_config(&app.db, REPORT, &serde_json::json!({ "version": version, "outcome": outcome }).to_string())?;
    db::log(&app.db.conn(), None, "box", action, version, detail)?;
    Ok(())
}

/// At start: report an update the launcher took back, or prove a new version healthy (every court with a camera
/// recording) and keep it.
pub fn on_start(app: &AppState) -> Result<(), AppError> {
    let Some(dir) = slots() else { return Ok(()) };
    if let Ok(text) = std::fs::read_to_string(dir.join("rolled-back")) {
        let mut lines = text.lines();
        let (version, reason) = (lines.next().unwrap_or("").to_string(), lines.next().unwrap_or("failed").to_string());
        record(app, "Update rolled back", &version, &reason, "rolled back")?;
        let _ = std::fs::remove_file(dir.join("rolled-back"));
        tracing::warn!(version, reason, "update rolled back");
    }
    let pending = dir.join("pending");
    if std::fs::read_to_string(pending.join("version")).is_ok_and(|v| v.trim() == current_version()) {
        let app = app.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                if healthy(&app).unwrap_or(false) {
                    let version = current_version();
                    if std::fs::remove_dir_all(&pending).is_ok() {
                        let _ = record(&app, "updated", &version, "", "updated");
                        tracing::info!(version, "update kept: the box is healthy");
                    }
                    return;
                }
            }
        });
    }
    Ok(())
}

fn healthy(app: &AppState) -> Result<bool, AppError> {
    let courts: Vec<(i64, bool)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, camera_url IS NOT NULL FROM court WHERE deleted = 0 AND box = ?1")?;
        st.query_map([&app.box_name], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };
    Ok(courts.iter().filter(|(_, cam)| *cam).all(|(id, cam)| matches!(app.recorders.status(*id, *cam), CourtStatus::Recording { .. })))
}

/// Stages an update and ends the process shortly after, as Wipe box does; the launcher starts the new version.
async fn install_and_restart(app: &AppState, file: &[u8], actor: Option<&Staff>, source: &str) -> Result<String, String> {
    let version = stage(file)?;
    let (id, who) = actor.map_or((None, "fleet".to_string()), |s| (Some(s.id), s.username.clone()));
    let _ = db::log(&app.db.conn(), id, &who, "installing update", &version, source);
    tracing::info!(version, source, "installing update");
    let recorders = app.recorders.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(2)).await;
        recorders.stop_all().await;
        std::process::exit(0);
    });
    Ok(version)
}

/// An update offered by the fleet at check-in (FR-51): downloaded, checked and installed like a USB file.
pub async fn from_fleet(app: &AppState, version: &str, url: &str) -> Result<(), String> {
    if version == current_version() || slots().is_none_or(|d| d.join("pending").exists()) {
        return Ok(());
    }
    let file = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    install_and_restart(app, &file, None, "fleet").await.map(|_| ())
}

#[derive(Template)]
#[template(path = "update.html")]
struct UpdatePage {
    shell: Shell,
    version: String,
    status: Option<String>,
    installing: Option<String>,
    error: Option<String>,
    can_update: bool,
}

fn page(app: &AppState, staff: &Staff, installing: Option<String>, error: Option<String>) -> Page {
    let status = crate::uplink::config(&app.db, STATUS)?;
    html(UpdatePage {
        shell: shell(&app.db, staff, "settings")?,
        version: current_version(),
        status,
        installing,
        error,
        can_update: slots().is_some() && release_key().is_some(),
    })
}

pub async fn form(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    page(&app, &staff, None, None)
}

/// An update file from a USB stick (FR-54), uploaded from the Owner's console.
pub async fn upload(State(app): State<AppState>, staff: Staff, mut form: Multipart) -> Page {
    staff.require_owner()?;
    let mut file = Vec::new();
    while let Some(field) = form.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        if field.name() == Some("file") {
            file = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?.to_vec();
        }
    }
    match install_and_restart(&app, &file, Some(&staff), "USB").await {
        Ok(version) => page(&app, &staff, Some(version), None),
        Err(e) => {
            let mut res = page(&app, &staff, None, Some(e))?;
            *res.status_mut() = axum::http::StatusCode::UNPROCESSABLE_ENTITY;
            Ok(res)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_files_verify_and_tampering_is_refused() {
        let (pkcs8, public_hex) = new_key().unwrap();
        let public: Vec<u8> = (0..64).step_by(2).map(|i| u8::from_str_radix(&public_hex[i..i + 2], 16).unwrap()).collect();
        let file = make("0.2.0", b"new ch-box", &pkcs8).unwrap();
        assert_eq!(verify(&file, &public).unwrap(), ("0.2.0".to_string(), b"new ch-box".to_vec()));

        let mut tampered = file.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(verify(&tampered, &public).unwrap_err().contains("don't match"));

        let (_, other_hex) = new_key().unwrap();
        let other: Vec<u8> = (0..64).step_by(2).map(|i| u8::from_str_radix(&other_hex[i..i + 2], 16).unwrap()).collect();
        assert!(verify(&file, &other).unwrap_err().contains("isn't signed by the system owner"));
        assert!(verify(b"MZ\x90\x00", &public).unwrap_err().contains("isn't a Court Highlights update"));
    }
}

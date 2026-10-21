//! Settings backup (FR-43, FR-44): courts and cameras, logo, hours, settings and staff accounts (password hashes
//! only), in one file encrypted with a passphrase the Owner chooses. A replacement box at a one-box venue starts
//! from it; at a bigger venue the siblings fill a replacement in by themselves (sync.rs).

use crate::auth::Staff;
use crate::web::{AppError, AppState, Page};
use crate::{clock, db};
use argon2::Argon2;
use axum::extract::{Multipart, State};
use axum::http::header;
use axum::response::{IntoResponse, Redirect, Response};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use serde::{Deserialize, Serialize};

const FORMAT: &str = "court-highlights-settings-backup";

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    salt: String,
    data: String,
}

fn key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], AppError> {
    let mut out = [0u8; 32];
    Argon2::default().hash_password_into(passphrase.as_bytes(), salt, &mut out).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(out)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok())).collect()
}

pub fn seal(passphrase: &str, plain: &[u8]) -> Result<Vec<u8>, AppError> {
    let (mut salt, mut nonce) = ([0u8; 16], [0u8; 12]);
    getrandom::fill(&mut salt).expect("OS random source");
    getrandom::fill(&mut nonce).expect("OS random source");
    let k = key(passphrase, &salt)?;
    let mut data = nonce.to_vec();
    data.extend(ChaCha20Poly1305::new(&k.into()).encrypt(&Nonce::from(nonce), plain).map_err(|_| AppError::Internal("encrypt".into()))?);
    let file = File { format: FORMAT.into(), version: 1, salt: crate::auth::hex(&salt), data: crate::auth::hex(&data) };
    serde_json::to_vec_pretty(&file).map_err(|e| AppError::Internal(e.to_string()))
}

/// None when the file isn't a backup or the passphrase is wrong (the same answer for both).
pub fn open(passphrase: &str, bytes: &[u8]) -> Option<Vec<u8>> {
    let file: File = serde_json::from_slice(bytes).ok().filter(|f: &File| f.format == FORMAT && f.version == 1)?;
    let (salt, data) = (hex_decode(&file.salt)?, hex_decode(&file.data)?);
    let (nonce, body) = data.split_at_checked(12)?;
    let nonce: [u8; 12] = nonce.try_into().ok()?;
    let k = key(passphrase, &salt).ok()?;
    ChaCha20Poly1305::new(&k.into()).decrypt(&Nonce::from(nonce), body).ok()
}

async fn fields(mut form: Multipart) -> Result<(String, Vec<u8>), AppError> {
    let (mut passphrase, mut file) = (String::new(), Vec::new());
    while let Some(field) = form.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        match field.name().unwrap_or("") {
            "passphrase" => passphrase = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?,
            "backup" => file = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?.to_vec(),
            _ => {}
        }
    }
    Ok((passphrase, file))
}

/// The Owner exports the settings backup (FR-44).
pub async fn export(State(app): State<AppState>, staff: Staff, form: Multipart) -> Result<Response, AppError> {
    staff.require_owner()?;
    let (passphrase, _) = fields(form).await?;
    if passphrase.chars().count() < 12 {
        return Err(AppError::BadRequest("Choose a passphrase of at least 12 characters. You need it to restore the backup.".into()));
    }
    let state = crate::sync::local_state(&app).map_err(|e| AppError::Internal(e.to_string()))?;
    let plain = serde_json::to_vec(&state).map_err(|e| AppError::Internal(e.to_string()))?;
    let body = seal(&passphrase, &plain)?;
    db::log(&app.db.conn(), Some(staff.id), &staff.username, "exported settings backup", "", "")?;
    let name = format!("court-highlights-settings-{}.chbackup", clock::now().format("%Y%m%d"));
    Ok(([(header::CONTENT_TYPE, "application/octet-stream".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\""))], body).into_response())
}

/// A new box takes over from a backup (FR-43): only before any staff account exists on it, so a running box
/// can't be overwritten this way.
pub async fn restore(State(app): State<AppState>, form: Multipart) -> Page {
    if db::has_staff(&app.db.conn())? {
        return Ok(Redirect::to("/console/signin").into_response());
    }
    let (passphrase, file) = fields(form).await?;
    let plain = open(&passphrase, &file).ok_or_else(|| AppError::BadRequest("That file isn't a settings backup, or the passphrase is wrong.".into()))?;
    let state: crate::sync::State_ = serde_json::from_slice(&plain).map_err(|_| AppError::BadRequest("That backup can't be read.".into()))?;
    let applied = crate::sync::merge(&app.db, &state)?;
    db::log(&app.db.conn(), None, "setup", "restored settings backup", &state.box_name, &format!("{applied} records"))?;
    tracing::info!(applied, "settings restored from backup");
    Ok(Redirect::to("/console/signin?restored=1").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_need_the_passphrase_and_show_nothing_readable() {
        let sealed = seal("a long passphrase here", b"{\"password_hash\":\"$argon2id$...\",\"camera_password\":\"cam-pass\"}").unwrap();
        let text = String::from_utf8(sealed.clone()).unwrap();
        assert!(!text.contains("cam-pass") && !text.contains("argon2"));
        assert!(open("a long passphrase here", &sealed).is_some_and(|p| p.starts_with(b"{\"password_hash\"")));
        assert_eq!(open("the wrong passphrase!", &sealed), None);
        assert_eq!(open("a long passphrase here", b"not a backup"), None);
    }
}

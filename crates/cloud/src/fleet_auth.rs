//! Fleet console accounts (FR-47, NFR-9 ASVS L2): password + authenticator code, Admin and Installer roles,
//! 5 failures lock the account for 15 min, 8-hour sessions, writes only from the console's own pages.

use crate::{App, hex_sha256, random_token};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::{FromRequestParts, Request};
use axum::http::{HeaderMap, Method, StatusCode, header, request::Parts};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sqlx::Row;

pub const COOKIE: &str = "__Host-ch_fleet";
/// Sessions end after 8 hours, or after 30 minutes without activity (v5.0.0-7.3.1, 7.3.2).
const SESSION_HOURS: i64 = 8;
const IDLE_MINUTES: i64 = 30;

#[derive(Clone)]
pub struct FleetUser {
    pub id: i64,
    pub username: String,
    pub is_admin: bool,
    pub must_change: bool,
}

pub enum Denied {
    SignIn,
    ChangePassword,
    NotAllowed,
    NotFound,
    Failed(String),
}

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        match self {
            Self::SignIn => Redirect::to("/fleet/signin").into_response(),
            Self::ChangePassword => Redirect::to("/fleet/account").into_response(),
            Self::NotAllowed => crate::player::message(StatusCode::FORBIDDEN, "Not allowed", "Only an Admin can do that."),
            Self::NotFound => crate::player::message(StatusCode::NOT_FOUND, "Not found", "That page does not exist."),
            Self::Failed(detail) => {
                tracing::error!(%detail, "fleet request failed");
                crate::player::message(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong", "Try again.")
            }
        }
    }
}

impl From<sqlx::Error> for Denied {
    fn from(e: sqlx::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<askama::Error> for Denied {
    fn from(e: askama::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

impl FleetUser {
    pub fn admin(&self) -> Result<(), Denied> {
        if self.is_admin { Ok(()) } else { Err(Denied::NotAllowed) }
    }
}

pub fn cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| c.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

impl FromRequestParts<App> for FleetUser {
    type Rejection = Denied;

    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self, Self::Rejection> {
        let token = cookie_token(&parts.headers).ok_or(Denied::SignIn)?;
        let row = sqlx::query(
            "SELECT u.id, u.username, u.role = 'admin', u.must_change FROM fleet_session s JOIN fleet_user u ON u.id = s.user_id
             WHERE s.token_hash = $1 AND u.active AND s.signed_in_at > $2 AND s.last_seen_at > $3",
        )
        .bind(hex_sha256(&token))
        .bind(crate::now() - chrono::Duration::hours(SESSION_HOURS))
        .bind(crate::now() - chrono::Duration::minutes(IDLE_MINUTES))
        .fetch_optional(&app.db)
        .await?
        .ok_or(Denied::SignIn)?;
        sqlx::query("UPDATE fleet_session SET last_seen_at = $1 WHERE token_hash = $2").bind(crate::now()).bind(hex_sha256(&token)).execute(&app.db).await?;
        let user = FleetUser { id: row.get(0), username: row.get(1), is_admin: row.get(2), must_change: row.get(3) };
        // A temporary password must be replaced before anything else.
        if user.must_change && parts.uri.path() != "/fleet/account" {
            return Err(Denied::ChangePassword);
        }
        Ok(user)
    }
}

pub fn hash_password(password: &str) -> String {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("OS random source");
    Argon2::default().hash_password_with_salt(password.as_bytes(), &salt).expect("argon2").to_string()
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// ≥ 12 characters and not on the list of commonly used passwords (v5.0.0-6.2.1, 6.2.4).
pub fn password_problem(p: &str) -> Option<&'static str> {
    static COMMON: &str = include_str!("../../box/data/common-passwords.txt");
    if p.chars().count() < 12 {
        Some("Use at least 12 characters.")
    } else if COMMON.lines().any(|c| c == p.to_lowercase()) {
        Some("That password is on the list of commonly used passwords. Choose another.")
    } else {
        None
    }
}

pub fn temporary_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random source");
    buf.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

// ------------------------------------------------------------------ authenticator secrets

pub fn new_totp_secret() -> Vec<u8> {
    let mut buf = vec![0u8; 20];
    getrandom::fill(&mut buf).expect("OS random source");
    buf
}

/// RFC 4648 base32 without padding, as authenticator apps expect.
pub fn base32(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = String::new();
    let (mut buffer, mut bits) = (0u32, 0);
    for b in bytes {
        buffer = (buffer << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(A[((buffer >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(A[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn otpauth_uri(username: &str, secret: &[u8]) -> String {
    format!(
        "otpauth://totp/Court%20Highlights%20fleet:{username}?secret={}&issuer=Court%20Highlights%20fleet&algorithm=SHA1&digits=6&period=30",
        base32(secret)
    )
}

fn cipher(key: &[u8; 32]) -> ChaCha20Poly1305 {
    ChaCha20Poly1305::new(key.into())
}

pub fn seal(key: &[u8; 32], secret: &[u8]) -> Vec<u8> {
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).expect("OS random source");
    let mut out = nonce.to_vec();
    out.extend(cipher(key).encrypt(&Nonce::from(nonce), secret).expect("encrypt"));
    out
}

pub fn open(key: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
    let (nonce, body) = sealed.split_at_checked(12)?;
    let nonce: [u8; 12] = nonce.try_into().ok()?;
    cipher(key).decrypt(&Nonce::from(nonce), body).ok()
}

/// The time step a valid code belongs to (30 s steps, one step of clock skew either way).
pub fn code_step(secret: &[u8], code: &str) -> Option<u64> {
    let code = code.trim().replace(' ', "");
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    totp_rs::Builder::new().with_secret(totp_rs::Secret::from(secret)).build().ok()?.check_current(&code)
}

// ------------------------------------------------------------------ sign-in

pub enum Outcome {
    In { token: String, must_change: bool },
    Refused,
}

/// Password and code are both required; the same answer for every failure, and 5 failures lock 15 min (FR-47).
pub async fn sign_in(app: &App, username: &str, password: &str, code: &str) -> Result<Outcome, Denied> {
    let now = crate::now();
    let row = sqlx::query(
        "SELECT id, password_hash, totp_secret_enc, failures, locked_until, must_change, totp_last_step FROM fleet_user WHERE username = $1 AND active",
    )
    .bind(username)
    .fetch_optional(&app.db)
    .await?;
    let Some(row) = row else {
        // Same work for unknown names, so timing doesn't reveal which accounts exist.
        let _ = verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$5W2ICzvvmMCyMLiBcDk0Gi0nnTGsXmYbhHXUHVfiy4c");
        crate::fleet::log(app, username, "sign-in refused", "", "unknown username").await?;
        return Ok(Outcome::Refused);
    };
    let (id, hash, sealed, failures, locked, must_change): (i64, String, Vec<u8>, i32, Option<chrono::DateTime<chrono::Utc>>, bool) =
        (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4), row.get(5));
    if locked.is_some_and(|l| l > now) {
        crate::fleet::log(app, username, "sign-in refused", "", "locked").await?;
        return Ok(Outcome::Refused);
    }
    let last_step: i64 = row.get(6);
    // A code already used (replayed) is refused like a wrong one.
    let step = open(&app.secret_key, &sealed).and_then(|s| code_step(&s, code)).filter(|s| *s as i64 > last_step);
    let ok = verify_password(password, &hash) && step.is_some();
    if !ok {
        let failures = failures + 1;
        let lock = (failures >= ch_core::LOCKOUT_AFTER as i32).then(|| now + ch_core::LOCKOUT_FOR);
        sqlx::query("UPDATE fleet_user SET failures = $1, locked_until = $2 WHERE id = $3")
            .bind(if lock.is_some() { 0 } else { failures })
            .bind(lock)
            .bind(id)
            .execute(&app.db)
            .await?;
        crate::fleet::log(app, username, "sign-in refused", "", if lock.is_some() { "locked for 15 minutes" } else { "wrong password or code" }).await?;
        return Ok(Outcome::Refused);
    }
    let token = random_token();
    sqlx::query("UPDATE fleet_user SET failures = 0, locked_until = NULL, totp_last_step = $2 WHERE id = $1")
        .bind(id)
        .bind(step.unwrap_or(0) as i64)
        .execute(&app.db)
        .await?;
    sqlx::query("INSERT INTO fleet_session (token_hash, user_id, signed_in_at, last_seen_at) VALUES ($1, $2, $3, $3)")
        .bind(hex_sha256(&token))
        .bind(id)
        .bind(now)
        .execute(&app.db)
        .await?;
    crate::fleet::log(app, username, "signed in", "", "").await?;
    Ok(Outcome::In { token, must_change })
}

pub fn session_cookie(token: &str) -> String {
    format!("{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age={}", SESSION_HOURS * 3600)
}

/// Writes to the fleet console must come from its own pages (v5.0.0-3.5.1).
pub async fn same_origin(app: axum::extract::State<App>, req: Request, next: Next) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD && req.uri().path().starts_with("/fleet") {
        let h = req.headers();
        let origin_ok = h.get(header::ORIGIN).and_then(|v| v.to_str().ok()).is_none_or(|o| o == app.origin);
        let site_ok = h.get("sec-fetch-site").and_then(|v| v.to_str().ok()).is_none_or(|s| s == "same-origin" || s == "none");
        if !origin_ok || !site_ok {
            tracing::warn!(path = %req.uri().path(), "cross-site fleet request refused");
            return (StatusCode::FORBIDDEN, "Cross-site request refused").into_response();
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_matches_rfc4648() {
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(b"TestSecretSuperSecret"), "KRSXG5CTMVRXEZLUKN2XAZLSKNSWG4TFOQ");
    }

    #[test]
    fn sealed_secrets_round_trip_and_resist_tampering() {
        let key = [7u8; 32];
        let mut sealed = seal(&key, b"0123456789abcdefghij");
        assert_eq!(open(&key, &sealed).as_deref(), Some(&b"0123456789abcdefghij"[..]));
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(open(&key, &sealed), None);
    }

    #[test]
    fn codes_must_be_six_digits() {
        assert_eq!(code_step(b"0123456789abcdefghij", ""), None);
        assert_eq!(code_step(b"0123456789abcdefghij", "12345a"), None);
    }
}

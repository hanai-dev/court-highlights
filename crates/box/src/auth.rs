//! Staff sign-in (FR-28, FR-29): argon2 hashes, random session tokens stored only as SHA-256 hashes, 12 h absolute
//! sessions, and the ch-core lockout rule persisted per account.

use crate::{clock, db::Db, web::AppError};
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts};
use axum::response::{IntoResponse, Redirect, Response};
use ch_core::{Lockout, SignIn, session_valid};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

pub const COOKIE: &str = "__Host-ch_staff";
pub const MIN_PASSWORD_CHARS: usize = 12;
static COMMON: &str = include_str!("../data/common-passwords.txt");

pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source");
    hex(&buf)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(s: impl AsRef<[u8]>) -> String {
    hex(&Sha256::digest(s.as_ref()))
}

pub fn hash_password(password: &str) -> String {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).expect("OS random source");
    Argon2::default().hash_password_with_salt(password.as_bytes(), &salt).expect("argon2").to_string()
}

fn verify_password(password: &str, hash: &str) -> bool {
    Argon2::default().verify_password(password.as_bytes(), hash).is_ok()
}

/// ASVS v5.0.0-6.2.1 (≥ 12 characters, owner decision A-126) and 6.2.4 (not among the 3,000 most common).
pub fn password_problem(password: &str) -> Option<&'static str> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Some("Use at least 12 characters.");
    }
    let lower = password.to_lowercase();
    if COMMON.lines().filter(|l| !l.starts_with('#')).any(|c| c == lower) {
        return Some("That password is too common. Choose another.");
    }
    None
}

/// Temporary passwords for new staff and resets: random, 16 characters, expire after 72 h unused (v5.0.0-6.4.1).
pub fn temporary_password() -> String {
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut buf = [0u8; 16];
    getrandom::fill(&mut buf).expect("OS random source");
    buf.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect()
}

pub enum SignInOutcome {
    Ok { token: String, must_change: bool },
    Refused,
}

pub fn sign_in(db: &Db, username: &str, password: &str) -> rusqlite::Result<SignInOutcome> {
    let Some((id, must_change)) = verify(db, username, password)? else {
        return Ok(SignInOutcome::Refused);
    };
    let conn = db.conn();
    let token = random_token();
    conn.execute(
        "INSERT INTO staff_session (token_hash, staff_id, signed_in_at) VALUES (?1, ?2, ?3)",
        params![sha256_hex(&token), id, clock::now().to_rfc3339()],
    )?;
    crate::db::log(&conn, Some(id), username, "signed in", "staff", "")?;
    Ok(SignInOutcome::Ok { token, must_change })
}

/// Checks a password under the lockout rule without opening a session; refusals are logged (v5.0.0-16.3.1).
pub fn verify(db: &Db, username: &str, password: &str) -> rusqlite::Result<Option<(i64, bool)>> {
    let conn = db.conn();
    let row: Option<(i64, String, i64, Option<String>, bool, bool, Option<String>)> = conn
        .query_row(
            "SELECT id, password_hash, failures, locked_until, active, must_change, temp_expires_at FROM staff WHERE username = ?1",
            [username],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    let now = clock::now();
    let Some((id, hash, failures, locked_until, active, must_change, temp_expires)) = row else {
        // Same work for unknown names, so response time does not reveal which usernames exist.
        static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let _ = verify_password(password, DUMMY.get_or_init(|| hash_password(&random_token())));
        crate::db::log(&conn, None, username, "sign-in refused", "staff", "unknown username")?;
        return Ok(None);
    };
    let mut lock = Lockout { consecutive_failures: failures as u32, locked_until: locked_until.as_deref().and_then(clock::parse) };
    let temp_expired = must_change && temp_expires.as_deref().and_then(clock::parse).is_some_and(|t| now >= t);
    let ok = active && !temp_expired && verify_password(password, &hash);
    let outcome = lock.attempt(now, ok);
    conn.execute(
        "UPDATE staff SET failures = ?1, locked_until = ?2 WHERE id = ?3",
        params![lock.consecutive_failures, lock.locked_until.map(|t| t.to_rfc3339()), id],
    )?;
    if outcome != SignIn::Accepted {
        let why = match outcome {
            SignIn::Locked => "locked",
            _ if temp_expired => "temporary password expired",
            _ if !active => "deactivated",
            _ => "wrong password",
        };
        crate::db::log(&conn, Some(id), username, "sign-in refused", "staff", why)?;
        return Ok(None);
    }
    Ok(Some((id, must_change)))
}

pub fn session_cookie(token: &str) -> String {
    format!("{COOKIE}={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=43200")
}

pub fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=0")
}

pub fn cookie_token(parts: &Parts) -> Option<String> {
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
        .filter(|t| !t.is_empty())
}

#[derive(Clone, Debug)]
pub struct Staff {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub is_owner: bool,
    pub must_change: bool,
}

impl Staff {
    pub fn require_owner(&self) -> Result<(), AppError> {
        if self.is_owner { Ok(()) } else { Err(AppError::Forbidden) }
    }
}

/// Every staff page resolves its caller from the cookie on the server; an expired, deactivated or unknown session
/// sends the browser to sign-in (v5.0.0-7.2.1, 7.4.1, 7.4.2).
impl<S: Send + Sync> FromRequestParts<S> for Staff
where
    crate::web::AppState: axum::extract::FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = <crate::web::AppState as axum::extract::FromRef<S>>::from_ref(state);
        let signin = || Redirect::to("/console/signin").into_response();
        let token = cookie_token(parts).ok_or_else(signin)?;
        let conn = app.db.conn();
        let row: Option<(i64, String, String, String, bool, String)> = conn
            .query_row(
                "SELECT s.id, s.username, s.display_name, s.role, s.must_change, ss.signed_in_at
                 FROM staff_session ss JOIN staff s ON s.id = ss.staff_id
                 WHERE ss.token_hash = ?1 AND s.active = 1",
                [sha256_hex(&token)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()
            .map_err(|e| AppError::from(e).into_response())?;
        let (id, username, display_name, role, must_change, signed_in) = row.ok_or_else(signin)?;
        if !clock::parse(&signed_in).is_some_and(|t| session_valid(t, clock::now())) {
            let _ = conn.execute("DELETE FROM staff_session WHERE token_hash = ?1", [sha256_hex(&token)]);
            return Err(signin());
        }
        let staff = Staff { id, username, display_name, is_owner: role == "owner", must_change };
        if staff.must_change && !parts.uri.path().starts_with("/console/account") && parts.uri.path() != "/console/signout" {
            return Err(Redirect::to("/console/account?first=1").into_response());
        }
        Ok(staff)
    }
}

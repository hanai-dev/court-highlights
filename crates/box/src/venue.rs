//! Venue settings, time correction, help, court signs and the privacy notice (FR-33, FR-37, FR-38, FR-39, FR-57).

use crate::auth::Staff;
use crate::web::{AppError, AppState, LOGO_MAX_BYTES, Page, Shell, html, logo_type, shell};
use crate::{clock, db};
use askama::Template;
use axum::extract::{Form, Multipart, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect};
use rusqlite::params;
use serde::Deserialize;

/// Starting text for a venue's privacy notice; the venue's reviewer finalises it (FR-57).
pub const NOTICE_TEMPLATE: &str = "What we record: video (no sound) of each court during opening hours, to save clips and game videos that players ask for.

How long we keep it: the last 4 hours of court video; clips, highlights and game videos for 3 days, or 1 year if the venue keeps them. Footage someone asks us to keep for a complaint or incident is held for up to 60 days.

Emails: an address you enter is used only to send that game's videos and is deleted after sending. Every email has a link to never be emailed again.

Posting: the venue posts highlights on its pages only when the players ticked \"The venue may post our highlights on its pages\".

Under 18? Ask a parent first.

Questions or requests to see or delete video of you: ask at the front desk or contact the venue.";

#[derive(Clone)]
struct Venue {
    name: String,
    has_logo: bool,
    has_sponsor: bool,
    corner: String,
    open: String,
    close: String,
    before: u32,
    after: u32,
    cap_mbps: u32,
    judge_on: bool,
    notice: String,
    contact: String,
}

fn load(app: &AppState) -> Result<Venue, AppError> {
    Ok(app.db.conn().query_row(
        "SELECT name, logo IS NOT NULL, sponsor IS NOT NULL, logo_corner, open_time, close_time, clip_before_s, clip_after_s,
                upload_cap_kbps, judge_on, privacy_notice, contact FROM venue WHERE id = 1",
        [],
        |r| {
            Ok(Venue {
                name: r.get(0)?,
                has_logo: r.get(1)?,
                has_sponsor: r.get(2)?,
                corner: r.get(3)?,
                open: r.get(4)?,
                close: r.get(5)?,
                before: r.get(6)?,
                after: r.get(7)?,
                cap_mbps: r.get::<_, u32>(8)? / 1000,
                judge_on: r.get(9)?,
                notice: r.get(10)?,
                contact: r.get(11)?,
            })
        },
    )?)
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    shell: Shell,
    cloud_status: String,
    v: Venue,
    notice_draft: bool,
    now: String,
    now_input: String,
    saved: Option<&'static str>,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

fn page(app: &AppState, staff: &Staff, v: Venue, saved: Option<&'static str>, error: Option<String>) -> Page {
    let now = clock::now();
    let notice_draft = v.notice.trim().is_empty();
    let v = Venue { notice: if notice_draft { NOTICE_TEMPLATE.into() } else { v.notice }, ..v };
    let mut res = html(SettingsPage {
        shell: shell(&app.db, staff, "settings")?,
        cloud_status: cloud_status(app)?,
        v,
        notice_draft,
        now: now.format("%-d %b %Y, %-I:%M:%S %P").to_string(),
        now_input: now.format("%Y-%m-%dT%H:%M").to_string(),
        saved,
        error: error.clone(),
    })?;
    if error.is_some() {
        *res.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok(res)
}

pub async fn settings(State(app): State<AppState>, staff: Staff, Query(q): Query<SavedQuery>) -> Page {
    staff.require_owner()?;
    let saved = q.saved.as_deref().map(|s| match s {
        "time" => "Box time corrected.",
        "online" => "Box key saved. The box connects within a minute.",
        _ => "Settings saved.",
    });
    page(&app, &staff, load(&app)?, saved, None)
}

struct Upload {
    bytes: Vec<u8>,
    mime: &'static str,
}

/// A logo field: empty means "no change"; anything else must be a PNG or JPEG of at most 2 MB (v5.0.0-5.2.1, 5.2.2).
fn check_logo(bytes: Vec<u8>, what: &str) -> Result<Option<Upload>, String> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > LOGO_MAX_BYTES {
        return Err(format!("The {what} is larger than 2 MB. Nothing was saved."));
    }
    let mime = logo_type(&bytes).ok_or_else(|| format!("The {what} must be a PNG or JPEG image. Nothing was saved."))?;
    Ok(Some(Upload { bytes, mime }))
}

pub async fn save(State(app): State<AppState>, staff: Staff, mut form: Multipart) -> Page {
    staff.require_owner()?;
    let old = load(&app)?;
    let mut v = Venue { judge_on: false, ..old.clone() };
    let (mut logo, mut sponsor, mut remove_sponsor) = (Vec::new(), Vec::new(), false);
    let (mut before, mut after, mut cap) = (String::new(), String::new(), String::new());
    while let Some(field) = form.next_field().await.map_err(|_| AppError::BadRequest("The upload was too large. Logos can be up to 2 MB.".into()))? {
        let name = field.name().unwrap_or("").to_string();
        if name == "logo" || name == "sponsor" {
            let bytes = field.bytes().await.map_err(|_| AppError::BadRequest("The upload was too large. Logos can be up to 2 MB.".into()))?;
            *(if name == "logo" { &mut logo } else { &mut sponsor }) = bytes.to_vec();
            continue;
        }
        let value = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
        match name.as_str() {
            "venue" => v.name = value.trim().to_string(),
            "corner" => v.corner = value,
            "open" => v.open = value,
            "close" => v.close = value,
            "before" => before = value,
            "after" => after = value,
            "cap" => cap = value,
            "judge_on" => v.judge_on = true,
            "remove_sponsor" => remove_sponsor = true,
            "notice" => v.notice = value.replace("\r\n", "\n").trim().to_string(),
            "contact" => v.contact = value.trim().to_string(),
            _ => {}
        }
    }
    let times = (chrono::NaiveTime::parse_from_str(&v.open, "%H:%M"), chrono::NaiveTime::parse_from_str(&v.close, "%H:%M"));
    let lengths = match (before.parse::<u32>(), after.parse::<u32>()) {
        (Ok(b), Ok(a)) => ch_core::ClipSettings::new(b, a).map_err(|e| e.to_string()),
        _ => Err("Clip lengths must be whole seconds.".to_string()),
    };
    let checked = (check_logo(logo, "logo"), check_logo(sponsor, "sponsor logo"));
    let problem = if v.name.is_empty() || v.name.len() > 80 {
        Some("Give the venue a name (up to 80 characters).".to_string())
    } else if !["top-left", "top-right", "bottom-left", "bottom-right"].contains(&v.corner.as_str()) {
        Some("Choose a corner for the logo.".to_string())
    } else if !matches!(times, (Ok(o), Ok(c)) if o < c) {
        Some("Opening time must be before closing time on the same day.".to_string())
    } else if let Err(e) = &lengths {
        Some(e.clone())
    } else if !cap.parse::<u32>().is_ok_and(|c| (1..=1000).contains(&c)) {
        Some("Upload cap must be between 1 and 1000 Mbit/s.".to_string())
    } else if v.notice.len() > 20_000 || v.contact.len() > 120 {
        Some("The privacy notice or venue contact is too long.".to_string())
    } else if let (Err(e), _) | (_, Err(e)) = &checked {
        Some(e.clone())
    } else {
        None
    };
    if let Some(e) = problem {
        return page(&app, &staff, v, None, Some(e));
    }
    let (Ok(logo), Ok(sponsor), Ok(lengths)) = (checked.0, checked.1, lengths) else { unreachable!("checked above") };
    v.before = lengths.before_s;
    v.after = lengths.after_s;
    v.cap_mbps = cap.parse().unwrap_or(old.cap_mbps);

    let changed: Vec<&str> = [
        ("name", v.name != old.name),
        ("logo", logo.is_some()),
        ("logo corner", v.corner != old.corner),
        ("sponsor logo", sponsor.is_some() || (remove_sponsor && old.has_sponsor)),
        ("opening hours", v.open != old.open || v.close != old.close),
        ("clip lengths", v.before != old.before || v.after != old.after),
        ("upload cap", v.cap_mbps != old.cap_mbps),
        ("shot judge", v.judge_on != old.judge_on),
        ("privacy notice", v.notice != old.notice),
        ("venue contact", v.contact != old.contact),
    ]
    .into_iter()
    .filter_map(|(n, c)| c.then_some(n))
    .collect();
    let conn = app.db.conn();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE venue SET name = ?1, logo_corner = ?2, open_time = ?3, close_time = ?4, clip_before_s = ?5, clip_after_s = ?6,
           upload_cap_kbps = ?7, judge_on = ?8, privacy_notice = ?9, contact = ?10 WHERE id = 1",
        params![v.name, v.corner, v.open, v.close, v.before, v.after, v.cap_mbps * 1000, v.judge_on, v.notice, v.contact],
    )?;
    if let Some(l) = logo {
        tx.execute("UPDATE venue SET logo = ?1, logo_type = ?2 WHERE id = 1", params![l.bytes, l.mime])?;
    }
    if let Some(s) = sponsor {
        tx.execute("UPDATE venue SET sponsor = ?1, sponsor_type = ?2 WHERE id = 1", params![s.bytes, s.mime])?;
    } else if remove_sponsor {
        tx.execute("UPDATE venue SET sponsor = NULL, sponsor_type = NULL WHERE id = 1", [])?;
    }
    if !changed.is_empty() {
        db::log(&tx, Some(staff.id), &staff.username, "changed settings", &v.name, &changed.join(", "))?;
    }
    tx.commit()?;
    Ok(Redirect::to("/console/settings?saved=1").into_response())
}

fn cloud_status(app: &AppState) -> Result<String, AppError> {
    use crate::uplink::{cloud_url, config, last_checkin};
    if cloud_url().is_none() {
        return Ok("This box has no online service address. Your installer sets it.".into());
    }
    if config(&app.db, "cloud_key")?.is_none_or(|k| k.is_empty()) {
        return Ok("Not connected: enter this box's key.".into());
    }
    Ok(match (config(&app.db, "cloud_status")?.as_deref(), last_checkin(&app.db)?) {
        (Some("key refused"), _) => "The online service refused this key. Ask your installer for a new one.".into(),
        (_, Some(t)) => format!("Connected. Last check-in {}.", t.format("%-d %b, %-I:%M %P")),
        _ => "Waiting for the first check-in.".into(),
    })
}

#[derive(Deserialize)]
pub struct OnlineForm {
    cloud_key: String,
}

/// The installer registers the box in the fleet console and gives the Owner its key (FR-48).
pub async fn save_online(State(app): State<AppState>, staff: Staff, Form(f): Form<OnlineForm>) -> Page {
    staff.require_owner()?;
    let key = f.cloud_key.trim();
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("A box key is 64 letters and digits. Copy it again from the fleet console.".into()));
    }
    crate::uplink::set_config(&app.db, "cloud_key", key)?;
    crate::uplink::set_config(&app.db, "cloud_status", "saved")?;
    db::log(&app.db.conn(), Some(staff.id), &staff.username, "set the online service key", "", "")?;
    Ok(Redirect::to("/console/settings?saved=online").into_response())
}

#[derive(Deserialize)]
pub struct TimeForm {
    now: String,
}

/// The Owner sets the box clock to the right time (FR-38); recordings split so no segment spans both timelines.
pub async fn correct_time(State(app): State<AppState>, staff: Staff, Form(f): Form<TimeForm>) -> Page {
    staff.require_owner()?;
    let wanted = chrono::NaiveDateTime::parse_from_str(&f.now, "%Y-%m-%dT%H:%M")
        .ok()
        .and_then(|t| t.and_local_timezone(ch_core::manila()).single())
        .ok_or_else(|| AppError::BadRequest("Enter the date and time as shown on a phone that has the right time.".into()))?;
    let before = clock::now();
    let correction = clock::correct_to(wanted);
    app.recorders.split_all();
    // The correction is this box's own; the venue learns which box leads the time (FR-38, sync.rs).
    // (Config helpers take the database lock themselves: call them before holding it here.)
    crate::uplink::set_config(&app.db, "clock_correction_ms", &correction.to_string())?;
    let conn = app.db.conn();
    conn.execute("UPDATE venue SET time_leader = ?1 WHERE id = 1", [&app.box_name])?;
    let detail = format!("from {} to {}", before.format("%-d %b %Y, %-I:%M %P"), wanted.format("%-d %b %Y, %-I:%M %P"));
    db::log(&conn, Some(staff.id), &staff.username, "corrected box time", "", &detail)?;
    Ok(Redirect::to("/console/settings?saved=time").into_response())
}

// ------------------------------------------------------------------ help (FR-39)

#[derive(Template)]
#[template(path = "help.html")]
struct HelpPage {
    shell: Shell,
    phone: String,
    phone_link: String,
    fingerprint: String,
    version: &'static str,
}

pub async fn help(State(app): State<AppState>, staff: Staff) -> Page {
    let stored: String = app.db.conn().query_row("SELECT support_phone FROM venue WHERE id = 1", [], |r| r.get(0))?;
    let phone = if stored.is_empty() { std::env::var("CH_SUPPORT_PHONE").unwrap_or_default() } else { stored };
    html(HelpPage {
        shell: shell(&app.db, &staff, "help")?,
        phone_link: phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect(),
        phone,
        fingerprint: app.fingerprint.clone(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ------------------------------------------------------------------ court signs (FR-37)

pub struct Sign {
    pub court: String,
    pub code: String,
    pub url: String,
    pub qr: String,
}

#[derive(Template)]
#[template(path = "signs.html")]
struct SignsPage {
    shell: Shell,
    venue: String,
    has_logo: bool,
    contact: String,
    signs: Vec<Sign>,
}

pub fn player_base_env() -> Option<String> {
    std::env::var("CH_PLAYER_URL").ok().filter(|u| !u.is_empty()).map(|u| u.trim_end_matches('/').to_string())
}

/// The address players open: CH_PLAYER_URL when set (installer's choice), else this box's host on plain HTTP.
fn player_base(headers: &HeaderMap) -> String {
    if let Some(url) = player_base_env() {
        return url;
    }
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or("court.local");
    let bare = host.rsplit_once(':').filter(|(_, port)| port.bytes().all(|b| b.is_ascii_digit())).map_or(host, |(h, _)| h);
    format!("http://{bare}")
}

pub fn qr_svg(text: &str) -> Result<String, AppError> {
    let code = qrcode::QrCode::with_error_correction_level(text, qrcode::EcLevel::M).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .dark_color(qrcode::render::svg::Color("#1b1b1b"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

pub async fn signs(State(app): State<AppState>, staff: Staff, headers: HeaderMap) -> Page {
    let v = load(&app)?;
    let base = player_base(&headers);
    let courts: Vec<(String, String, String)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT name, code, box FROM court WHERE deleted = 0 ORDER BY position, id")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    let peers = crate::sync::peers_snapshot(&app);
    let signs = courts
        .into_iter()
        .map(|(court, code, hosted_by)| {
            let host = peers.iter().find(|p| p.name == hosted_by && !p.player_url.is_empty()).map_or(base.as_str(), |p| p.player_url.as_str());
            let url = format!("{host}/c/{code}");
            Ok(Sign { qr: qr_svg(&url)?, court, code, url })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    html(SignsPage { shell: shell(&app.db, &staff, "signs")?, venue: v.name, has_logo: v.has_logo, contact: v.contact, signs })
}

// ------------------------------------------------------------------ privacy notice on the player listener (FR-57)

#[derive(Template)]
#[template(path = "privacy.html")]
struct PrivacyPage {
    venue: String,
    paragraphs: Vec<String>,
}

pub async fn privacy(State(app): State<AppState>) -> Page {
    let v = load(&app)?;
    let text = if v.notice.trim().is_empty() { String::new() } else { v.notice };
    html(PrivacyPage { venue: v.name, paragraphs: text.split("\n\n").map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect() })
}

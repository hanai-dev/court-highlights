//! Venue settings, time correction, help, court signs and the privacy notice (FR-33, FR-37, FR-38, FR-39, FR-57).

use crate::auth::Staff;
use crate::web::{AppError, AppState, FormErrors, LOGO_MAX_BYTES, Page, Shell, html, logo_type, shell};
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

Where it stays: everything is kept on the venue's own boxes and never leaves the venue network. You download your videos to your phone here.

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
    judge_on: bool,
    notice: String,
    contact: String,
}

fn load(app: &AppState) -> Result<Venue, AppError> {
    Ok(app.db.conn().query_row(
        "SELECT name, logo IS NOT NULL, sponsor IS NOT NULL, logo_corner, open_time, close_time, clip_before_s, clip_after_s,
                judge_on, privacy_notice, contact FROM venue WHERE id = 1",
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
                judge_on: r.get(8)?,
                notice: r.get(9)?,
                contact: r.get(10)?,
            })
        },
    )?)
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    shell: Shell,
    v: Venue,
    notice_draft: bool,
    now: String,
    now_input: String,
    saved: Option<&'static str>,
    f: FormErrors,
}

#[derive(Deserialize)]
pub struct SavedQuery {
    saved: Option<String>,
}

fn page(app: &AppState, staff: &Staff, v: Venue, saved: Option<&'static str>, f: FormErrors) -> Page {
    let now = clock::now();
    let notice_draft = v.notice.trim().is_empty();
    let v = Venue { notice: if notice_draft { NOTICE_TEMPLATE.into() } else { v.notice }, ..v };
    let failed = !f.is_empty();
    let mut res = html(SettingsPage {
        shell: shell(&app.db, staff, "settings")?,
        v,
        notice_draft,
        now: now.format("%-d %b %Y, %-I:%M:%S %P").to_string(),
        now_input: now.format("%Y-%m-%dT%H:%M").to_string(),
        saved,
        f,
    })?;
    if failed {
        *res.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
    }
    Ok(res)
}

pub async fn settings(State(app): State<AppState>, staff: Staff, Query(q): Query<SavedQuery>) -> Page {
    staff.require_owner()?;
    let saved = q.saved.as_deref().map(|s| match s {
        "time" => "Box time corrected.",
        _ => "Settings saved.",
    });
    page(&app, &staff, load(&app)?, saved, FormErrors::default())
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
    let (mut before, mut after) = (String::new(), String::new());
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
    let mut f = FormErrors::default();
    if v.name.is_empty() || v.name.len() > 80 {
        f.add("venue", "Give the venue a name (up to 80 characters).");
    }
    if v.contact.len() > 120 {
        f.add("contact", "The venue contact is too long (up to 120 characters).");
    }
    if !matches!(times, (Ok(o), Ok(c)) if o < c) {
        f.add("open", "Opening time must be before closing time on the same day.");
    }
    if let Err(e) = &checked.0 {
        f.add("logo", e.clone());
    }
    if !["top-left", "top-right", "bottom-left", "bottom-right"].contains(&v.corner.as_str()) {
        f.add("corner", "Choose a corner for the logo.");
    }
    if let Err(e) = &checked.1 {
        f.add("sponsor", e.clone());
    }
    if let Err(e) = &lengths {
        f.add("before", e.clone());
    }
    if v.notice.len() > 20_000 {
        f.add("notice", "The privacy notice is too long (up to 20,000 characters).");
    }
    if !f.is_empty() {
        return page(&app, &staff, v, None, f);
    }
    let (Ok(logo), Ok(sponsor), Ok(lengths)) = (checked.0, checked.1, lengths) else { unreachable!("checked above") };
    v.before = lengths.before_s;
    v.after = lengths.after_s;

    let changed: Vec<&str> = [
        ("name", v.name != old.name),
        ("logo", logo.is_some()),
        ("logo corner", v.corner != old.corner),
        ("sponsor logo", sponsor.is_some() || (remove_sponsor && old.has_sponsor)),
        ("opening hours", v.open != old.open || v.close != old.close),
        ("clip lengths", v.before != old.before || v.after != old.after),
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
           judge_on = ?7, privacy_notice = ?8, contact = ?9 WHERE id = 1",
        params![v.name, v.corner, v.open, v.close, v.before, v.after, v.judge_on, v.notice, v.contact],
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
    db::set_config(&app.db, "clock_correction_ms", &correction.to_string())?;
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

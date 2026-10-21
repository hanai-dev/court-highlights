//! First-run wizard, steps 2 and 3 (FR-40): courts with cameras found on the network and their previews, then the
//! court signs. Step 1 (owner and venue) is /setup in web.rs; the wizard ends on the empty clip library.

use crate::auth::Staff;
use crate::courts::CourtFields;
use crate::web::{AppState, Page, html, invalid};
use askama::Template;
use axum::extract::State;

pub struct WizardCourt {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub has_camera: bool,
}

#[derive(Template)]
#[template(path = "setup_courts.html")]
struct CourtsStep {
    venue: String,
    courts: Vec<WizardCourt>,
    f: CourtFields,
}

fn local_courts(app: &AppState) -> rusqlite::Result<Vec<WizardCourt>> {
    let conn = app.db.conn();
    let mut st = conn.prepare("SELECT id, name, code, camera_url IS NOT NULL FROM court WHERE deleted = 0 AND box = ?1 ORDER BY position, id")?;
    st.query_map([&app.box_name], |r| Ok(WizardCourt { id: r.get(0)?, name: r.get(1)?, code: r.get(2)?, has_camera: r.get(3)? }))?.collect()
}

/// Step 2 shown again with a form problem (courts::create calls this for wizard posts).
pub fn courts_page(app: &AppState, staff: &Staff, f: CourtFields) -> Page {
    staff.require_owner()?;
    let venue = crate::db::venue_name(&app.db.conn())?.unwrap_or_default();
    invalid(CourtsStep { venue, courts: local_courts(app)?, f })
}

pub async fn courts(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    let venue = crate::db::venue_name(&app.db.conn())?.unwrap_or_default();
    html(CourtsStep { venue, courts: local_courts(&app)?, f: CourtFields::blank().await })
}

#[derive(Template)]
#[template(path = "setup_signs.html")]
struct SignsStep {
    courts: Vec<WizardCourt>,
}

pub async fn signs(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    html(SignsStep { courts: local_courts(&app)? })
}

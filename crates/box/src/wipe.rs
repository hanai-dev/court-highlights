//! Wipe box (FR-55, SC-11): the Owner types the venue name; the box erases every clip, game, footage segment,
//! account, log and setting, and starts again as a new box.

use crate::auth::Staff;
use crate::web::{AppError, AppState, FormErrors, Page, Shell, html, invalid, shell};
use askama::Template;
use axum::extract::{Form, State};
use serde::Deserialize;

#[derive(Template)]
#[template(path = "wipe.html")]
struct WipePage {
    shell: Shell,
    venue: String,
    f: FormErrors,
}

pub async fn form(State(app): State<AppState>, staff: Staff) -> Page {
    staff.require_owner()?;
    let venue = crate::db::venue_name(&app.db.conn())?.unwrap_or_default();
    html(WipePage { shell: shell(&app.db, &staff, "settings")?, venue, f: FormErrors::default() })
}

#[derive(Deserialize)]
pub struct WipeForm {
    confirm: String,
}

#[derive(Template)]
#[template(path = "wiped.html")]
struct WipedPage;

pub async fn wipe(State(app): State<AppState>, staff: Staff, Form(form): Form<WipeForm>) -> Page {
    staff.require_owner()?;
    let venue = crate::db::venue_name(&app.db.conn())?.unwrap_or_default();
    let mut f = FormErrors::default();
    if form.confirm.trim() != venue {
        f.add("confirm", format!("Type the venue name exactly as shown: {venue}"));
    }
    if !f.is_empty() {
        return invalid(WipePage { shell: shell(&app.db, &staff, "settings")?, venue, f });
    }
    tracing::warn!("wiping this box at the Owner's request");
    app.recorders.stop_all().await;
    erase(&app)?;
    // The service manager starts the box again (systemd Restart=always); it comes up empty, at first-run setup.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        std::process::exit(0);
    });
    html(WipedPage)
}

/// Empties the database in place (secure_delete overwrites freed pages; VACUUM rewrites the file), then removes every
/// file in the data directory: footage, clips, games, logos, the database and this box's certificate.
// ponytail: file-level erase; on the installed image the data partition's encryption key goes too (DEV-INSTALL-IMAGE).
fn erase(app: &AppState) -> Result<(), AppError> {
    {
        let conn = app.db.conn();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA secure_delete = ON;")?;
        for t in &tables {
            conn.execute(&format!("DELETE FROM \"{t}\""), [])?;
        }
        conn.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
    }
    for entry in std::fs::read_dir(&app.data).map_err(|e| AppError::Internal(e.to_string()))? {
        let path = entry.map_err(|e| AppError::Internal(e.to_string()))?.path();
        let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        removed.map_err(|e| AppError::Internal(format!("wipe {}: {e}", path.display())))?;
    }
    Ok(())
}

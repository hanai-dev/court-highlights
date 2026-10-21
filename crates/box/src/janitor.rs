//! Clean-up (FR-45) and disk pressure (FR-46). The 03:00 rule is evaluated continuously: an item is due once
//! `expires_at` (the first 03:00 ≥ 72 h after it was made) has passed, so a box that was off at 03:00 catches up.

use crate::clips::at;
use crate::recorder::disk_use;
use crate::{clock, web::AppState};
use ch_core::{Deletable, Time, Window};
use rusqlite::ToSql;
use std::time::Duration;

pub fn spawn(app: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = expire(&app) {
                tracing::error!(error = %e, "clean-up failed");
            }
            if let Err(e) = relieve_disk(&app) {
                tracing::error!(error = %e, "disk pressure check failed");
            }
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });
}

struct Item {
    id: i64,
    token: String,
    court: i64,
    kind: String,
    made: Time,
    kept: Option<Time>,
    window: Window,
}

fn items(app: &AppState, sql_filter: &str, args: &[&dyn ToSql]) -> rusqlite::Result<Vec<Item>> {
    let conn = app.db.conn();
    let mut st = conn.prepare(&format!(
        "SELECT id, token, court_id, kind, made_at, kept_at, window_start_ms, window_end_ms FROM clip WHERE state != 'saving' AND {sql_filter}"
    ))?;
    st.query_map(args, |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?, r.get::<_, i64>(6)?, r.get::<_, i64>(7)?))
    })?
    .filter_map(|row| match row {
        Ok((id, token, court, kind, made, kept, s, e)) => Some(Ok(Item {
            id,
            token,
            court,
            kind,
            made: clock::parse(&made)?,
            kept: kept.and_then(|k| clock::parse(&k)),
            window: Window { start: at(s), end: at(e) },
        })),
        Err(e) => Some(Err(e)),
    })
    .collect()
}

/// Held footage keeps the clips and game videos cut from it too (FR-45 "not kept or held").
fn held(app: &AppState, item: &Item, now: Time) -> rusqlite::Result<bool> {
    let holds = crate::footage::protected(&app.db, item.court, now)?;
    Ok(holds.iter().any(|h| h.start < item.window.end && item.window.start < h.end))
}

fn remove(app: &AppState, item: &Item) -> rusqlite::Result<()> {
    app.db.conn().execute("DELETE FROM clip WHERE id = ?1", [item.id])?;
    let dir = app.data.join("clips").join(&item.token);
    if let Err(e) = std::fs::remove_dir_all(&dir)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(clip = item.id, error = %e, "expired clip files could not be removed");
    }
    Ok(())
}

fn expire(app: &AppState) -> rusqlite::Result<()> {
    let now = clock::now();
    // Cheap pre-filter on stored text; the exact 03:00 rule is applied below.
    let made_before = (now - ch_core::EXPIRY).to_rfc3339();
    let kept_before = (now - chrono::Duration::days(365)).to_rfc3339();
    let candidates = items(app, "((kept_at IS NULL AND made_at <= ?1) OR kept_at <= ?2)", &[&made_before, &kept_before])?;
    let mut removed = 0;
    for item in candidates {
        let due = match item.kept {
            Some(kept) => ch_core::keep_expires_at(kept) <= now,
            None => ch_core::expires_at(item.made) <= now && !held(app, &item, now)?,
        };
        if due {
            remove(app, &item)?;
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(removed, "clean-up removed expired clips and game videos");
    }
    Ok(())
}

fn dir_bytes(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path).map_or(0, |entries| entries.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
}

/// At ≥ 90 % the oldest unkept, unheld game videos go, then clips, until below 85 %; rolling footage is never a
/// candidate, so the last 4 h always stay (FR-46, GV-20).
fn relieve_disk(app: &AppState) -> rusqlite::Result<()> {
    let Some(disk) = disk_use(&app.data) else { return Ok(()) };
    if disk.used_pct < ch_core::DISK_CLEAN_PCT {
        return Ok(());
    }
    let now = clock::now();
    let mut candidates = Vec::new();
    let mut by_id = std::collections::HashMap::new();
    for item in items(app, "kept_at IS NULL", &[])? {
        if held(app, &item, now)? {
            continue;
        }
        let bytes = dir_bytes(&app.data.join("clips").join(&item.token));
        candidates.push(Deletable { id: item.id, is_game_video: item.kind == "game", made: item.made, bytes });
        by_id.insert(item.id, item);
    }
    let plan = ch_core::disk_plan(disk.used_bytes, disk.total_bytes, &candidates);
    for id in &plan.delete {
        if let Some(item) = by_id.get(id) {
            remove(app, item)?;
        }
    }
    tracing::warn!(used_pct = disk.used_pct, removed = plan.delete.len(), "disk pressure: removed the oldest unkept videos");
    Ok(())
}

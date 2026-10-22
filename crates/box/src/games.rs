//! Games, game videos and highlights (FR-10…FR-16).
//! Game videos and highlights are heavy: they run one at a time so taps on every court stay quick (NFR-1).

use crate::clips::{ClipLine, at, closed_segments, ms, plan, render, thumbnail};
use crate::courts::time_label;
use crate::media::{self, Shape};
use crate::web::{AppError, AppState, Page};
use crate::{clock, db::Db, judge};
use axum::extract::{Form, Path, State};
use axum::response::{IntoResponse, Redirect};
use ch_core::{Time, Window};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use std::time::Duration;

static HEAVY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

// ------------------------------------------------------------------ court page panels

pub struct Running {
    pub since: String,
    pub stops_at: String,
}

pub struct LastGame {
    pub span: String,
    pub video: Option<ClipLine>,
    /// "pending" | "running" | "done" | "none" | "off" | "failed"
    pub highlights: String,
    pub minutes_left: i64,
    pub reel: Option<ClipLine>,
    pub singles: Vec<ClipLine>,
}

pub struct Panel {
    pub running: Option<Running>,
    pub last: Option<LastGame>,
}

impl Panel {
    /// The game area still has something saving: the page keeps refreshing it.
    pub fn pending(&self) -> bool {
        self.last.as_ref().is_some_and(|g| g.video.as_ref().is_none_or(|v| v.state == "saving") || matches!(g.highlights.as_str(), "pending" | "running"))
    }
}

fn line(token: String, made: &str, state: String, start: i64, end: i64) -> ClipLine {
    let made = clock::parse(made).map(time_label).unwrap_or_default();
    ClipLine { token, time: made, state, failure: String::new(), seconds: ((end - start) as f64 / 1000.0).round() as i64 }
}

fn running_game(db: &Db, court: i64) -> rusqlite::Result<Option<(i64, Time)>> {
    let row: Option<(i64, String)> = db
        .conn()
        .query_row("SELECT id, started_at FROM game WHERE court_id = ?1 AND stopped_at IS NULL", [court], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(row.and_then(|(id, s)| Some((id, clock::parse(&s)?))))
}

fn hours(db: &Db) -> rusqlite::Result<Option<ch_core::OpeningHours>> {
    let (o, c): (String, String) = db.conn().query_row("SELECT open_time, close_time FROM venue WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(match (chrono::NaiveTime::parse_from_str(&o, "%H:%M"), chrono::NaiveTime::parse_from_str(&c, "%H:%M")) {
        (Ok(open), Ok(close)) => Some(ch_core::OpeningHours { open, close }),
        _ => None,
    })
}

fn auto_stop(db: &Db, started: Time) -> rusqlite::Result<Time> {
    let closing = hours(db)?.filter(|h| h.is_open(started)).map(|h| h.closing(started));
    Ok(ch_core::game_auto_stop(started, closing))
}

pub fn panel(app: &AppState, court: i64, now: Time) -> Result<Panel, AppError> {
    let running = match running_game(&app.db, court)? {
        Some((_, started)) => Some(Running { since: time_label(started), stops_at: time_label(auto_stop(&app.db, started)?) }),
        None => None,
    };
    let since = (now - ch_core::COURT_PAGE_WINDOW).to_rfc3339();
    let last: Option<(i64, String, String, String, Option<String>)> = app
        .db
        .conn()
        .query_row(
            "SELECT id, started_at, stopped_at, highlights, highlights_ready_by FROM game
             WHERE court_id = ?1 AND stopped_at IS NOT NULL AND stopped_at >= ?2 ORDER BY stopped_at DESC LIMIT 1",
            params![court, since],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let last = match last {
        None => None,
        Some((id, started, stopped, highlights, ready_by)) => {
            let items: Vec<(String, String, String, String, i64, i64)> = {
                let conn = app.db.conn();
                let mut st = conn.prepare(
                    "SELECT kind, token, made_at, state, window_start_ms, window_end_ms FROM clip
                     WHERE game_id = ?1 AND kind IN ('game', 'reel', 'highlight') AND hidden = 0 ORDER BY rank, id",
                )?;
                st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<Result<_, _>>()?
            };
            let mut g = LastGame {
                span: match (clock::parse(&started), clock::parse(&stopped)) {
                    (Some(s), Some(e)) => format!("{} – {}", time_label(s), time_label(e)),
                    _ => String::new(),
                },
                video: None,
                minutes_left: ready_by.and_then(|r| clock::parse(&r)).map_or(1, |r| ((r - now).num_seconds() as f64 / 60.0).ceil().max(1.0) as i64),
                highlights,
                reel: None,
                singles: Vec::new(),
            };
            for (kind, token, made, state, s, e) in items {
                let l = line(token, &made, state, s, e);
                match kind.as_str() {
                    "game" => g.video = Some(l),
                    "reel" => g.reel = Some(l),
                    _ => g.singles.push(l),
                }
            }
            Some(g)
        }
    };
    Ok(Panel { running, last })
}

// ------------------------------------------------------------------ start and stop (FR-10, FR-11)

#[derive(Deserialize)]
pub struct StartForm {
    may_post: Option<String>,
}

pub async fn start(State(app): State<AppState>, Path(code): Path<String>, Form(f): Form<StartForm>) -> Page {
    let court = crate::clips::court_for_players(&app, &code)?;
    let now = clock::now();
    if !court.recording {
        return Ok(Redirect::to(&format!("/c/{}?error=not-recording", court.code)).into_response());
    }
    if running_game(&app.db, court.id)?.is_none() {
        // One game per court: the unique index refuses a second concurrent Start.
        let inserted = app.db.conn().execute(
            "INSERT OR IGNORE INTO game (court_id, started_at, may_post, highlights) VALUES (?1, ?2, ?3, 'pending')",
            params![court.id, now.to_rfc3339(), f.may_post.is_some()],
        )?;
        if inserted == 1 {
            tracing::info!(court = court.id, may_post = f.may_post.is_some(), "game started");
        }
    }
    Ok(Redirect::to(&format!("/c/{}#game", court.code)).into_response())
}

pub async fn stop(State(app): State<AppState>, Path(code): Path<String>) -> Page {
    let court = crate::clips::court_for_players(&app, &code)?;
    if let Some((id, _)) = running_game(&app.db, court.id)? {
        stop_game(&app, id, clock::now(), "players")?;
    }
    Ok(Redirect::to(&format!("/c/{}#game", court.code)).into_response())
}

/// Ends a game at `at` and queues its video (and highlights when the judge is on).
fn stop_game(app: &AppState, game: i64, at: Time, reason: &str) -> Result<(), AppError> {
    {
        let conn = app.db.conn();
        let (court, started, judge_on): (i64, String, bool) = conn.query_row(
            "SELECT g.court_id, g.started_at, v.judge_on FROM game g, venue v WHERE g.id = ?1 AND v.id = 1 AND g.stopped_at IS NULL",
            [game],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let started = clock::parse(&started).ok_or_else(|| AppError::Internal("bad game start".into()))?;
        let at = at.max(started);
        // ponytail: estimate = a quarter of the game plus a minute; replace with measured N150 throughput (NFR-3).
        let ready_by = at + (at - started) / 4 + chrono::Duration::minutes(1);
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE game SET stopped_at = ?1, stop_reason = ?2, highlights = ?3, highlights_ready_by = ?4 WHERE id = ?5",
            params![at.to_rfc3339(), reason, if judge_on { "pending" } else { "off" }, ready_by.to_rfc3339(), game],
        )?;
        tx.execute(
            "INSERT INTO clip (court_id, game_id, kind, token, window_start_ms, window_end_ms, made_at) VALUES (?1, ?2, 'game', ?3, ?4, ?5, ?6)",
            params![court, game, crate::auth::random_token(), ms(started), ms(at), clock::now().to_rfc3339()],
        )?;
        tx.commit()?;
        tracing::info!(game, court, reason, "game stopped");
    }
    spawn_jobs(app.clone(), game);
    Ok(())
}

/// Games stop by themselves after 2 h or at closing time (FR-10, GV-9, GV-21).
pub fn spawn_watch(app: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = auto_stop_due(&app) {
                tracing::error!(error = ?e, "game watch failed");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

fn auto_stop_due(app: &AppState) -> Result<(), AppError> {
    let running: Vec<(i64, String)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, started_at FROM game WHERE stopped_at IS NULL")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
    };
    let now = clock::now();
    for (id, started) in running {
        let Some(started) = clock::parse(&started) else { continue };
        let stop_at = auto_stop(&app.db, started)?;
        if now >= stop_at {
            let reason = if stop_at - started >= ch_core::GAME_LIMIT { "2-hour limit" } else { "closing time" };
            stop_game(app, id, stop_at, reason)?;
        }
    }
    Ok(())
}

/// After a power cut a running game ends where its footage ends (FR-13); unfinished videos and highlights resume.
pub fn recover(app: &AppState) -> Result<(), AppError> {
    let running: Vec<(i64, i64, String)> = {
        let conn = app.db.conn();
        let mut st = conn.prepare("SELECT id, court_id, started_at FROM game WHERE stopped_at IS NULL")?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?
    };
    for (id, court, started) in running {
        let last: Option<i64> = app.db.conn().query_row("SELECT MAX(end_ms) FROM segment WHERE court_id = ?1", [court], |r| r.get(0))?;
        let started = clock::parse(&started).ok_or_else(|| AppError::Internal("bad game start".into()))?;
        stop_game(app, id, last.map(at).unwrap_or(started).max(started), "power cut")?;
    }
    let unfinished: Vec<i64> = {
        let conn = app.db.conn();
        let mut st = conn.prepare(
            "SELECT DISTINCT g.id FROM game g LEFT JOIN clip c ON c.game_id = g.id AND c.kind = 'game'
             WHERE g.stopped_at IS NOT NULL AND (c.state = 'saving' OR g.highlights IN ('pending', 'running'))",
        )?;
        st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?
    };
    for game in unfinished {
        spawn_jobs(app.clone(), game);
    }
    Ok(())
}

// ------------------------------------------------------------------ game video and highlights (FR-12, FR-14)

fn spawn_jobs(app: AppState, game: i64) {
    tokio::spawn(async move {
        let Ok(_permit) = HEAVY.acquire().await else { return };
        if let Err(e) = game_video(&app, game).await {
            tracing::error!(game, error = ?e, "game video failed");
            let _ = app
                .db
                .conn()
                .execute("UPDATE clip SET state = 'failed', failure = 'The box could not make this game video.' WHERE game_id = ?1 AND kind = 'game'", [game]);
        }
        if let Err(e) = highlights(&app, game).await {
            tracing::error!(game, error = ?e, "highlights failed");
            let _ = app.db.conn().execute("UPDATE game SET highlights = 'failed' WHERE id = ?1", [game]);
        }
    });
}

async fn game_video(app: &AppState, game: i64) -> Result<(), AppError> {
    let row: Option<(i64, i64, String, i64, i64)> = app
        .db
        .conn()
        .query_row(
            "SELECT id, court_id, token, window_start_ms, window_end_ms FROM clip WHERE game_id = ?1 AND kind = 'game' AND state = 'saving'",
            [game],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((clip, court, token, start, end)) = row else { return Ok(()) };
    app.recorders.split_now(court);
    let segments = closed_segments(&app.db, court, start, end).await?;
    let p = plan(&segments, Window { start: at(start), end: at(end) })?;
    let (actual_start, actual_end) = (p.start, p.end);
    let dir = app.data.join("clips").join(&token);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let wide = dir.join("wide.mp4");
    render(app, p, vec![(Shape::Wide, wide.clone())]).await?;
    thumbnail(clip, &wide, &dir).await;
    app.db.conn().execute(
        "UPDATE clip SET state = 'ready', wide_path = ?1, window_start_ms = ?2, window_end_ms = ?3, made_at = ?4 WHERE id = ?5",
        params![wide.to_string_lossy(), actual_start, actual_end, clock::now().to_rfc3339(), clip],
    )?;
    tracing::info!(game, clip, seconds = (actual_end - actual_start) / 1000, "game video ready");
    Ok(())
}

async fn highlights(app: &AppState, game: i64) -> Result<(), AppError> {
    let row: Option<(i64, i64, i64)> = app
        .db
        .conn()
        .query_row(
            "SELECT g.court_id, c.window_start_ms, c.window_end_ms FROM game g JOIN clip c ON c.game_id = g.id AND c.kind = 'game'
             WHERE g.id = ?1 AND g.highlights IN ('pending', 'running')",
            [game],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((court, start, end)) = row else { return Ok(()) };
    let conn_update = |state: &str| app.db.conn().execute("UPDATE game SET highlights = ?1 WHERE id = ?2", params![state, game]);
    conn_update("running")?;
    // Highlights left from an interrupted run are replaced.
    app.db.conn().execute("DELETE FROM clip WHERE game_id = ?1 AND kind IN ('highlight', 'reel')", [game])?;

    let segments = closed_segments(&app.db, court, start, end).await?;
    let p = plan(&segments, Window { start: at(start), end: at(end) })?;
    let (segs, from, to) = (p.segments.clone(), p.from_ms, p.to_ms);
    let samples = tokio::task::spawn_blocking(move || media::motion_profile(&segs, from, to)).await.map_err(|e| AppError::Internal(e.to_string()))??;
    let mut levels: Vec<f32> = samples.iter().map(|s| s.1).collect();
    levels.sort_by(f32::total_cmp);
    let pick = |q: usize| levels.get(levels.len().saturating_sub(1) * q / 100).copied().unwrap_or(0.0);
    let span_s = samples.last().map_or(0, |s| s.0 / 1000);
    tracing::info!(game, samples = samples.len(), span_s, p50 = pick(50), p90 = pick(90), max = pick(100), "movement profile");
    let points = judge::best_points(&samples, ch_core::HIGHLIGHTS_PER_GAME);
    let reel = ch_core::reel_plan(points.len());
    if !reel.reel {
        conn_update("none")?;
        tracing::info!(game, "no highlights found");
        return Ok(());
    }
    let length = to - from;
    let mut made = Vec::new();
    for (rank, point) in points.iter().take(reel.singles).enumerate() {
        // The whole point plus 2 s, at most 20 s (GV-22); offsets in recorded time from the game start.
        let span = ch_core::highlight_span(at(start + point.start_ms as i64), at(start + point.end_ms as i64));
        let (a, b) = ((ms(span.start) - start).max(0) as u64, ((ms(span.end) - start) as u64).min(length));
        let token = crate::auth::random_token();
        let dir = app.data.join("clips").join(&token);
        std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
        let (wide, vertical) = (dir.join("wide.mp4"), dir.join("vertical.mp4"));
        let part = crate::clips::Plan {
            segments: p.segments.clone(),
            from_ms: from + a,
            to_ms: from + b,
            source: p.source,
            start: start + a as i64,
            end: start + b as i64,
        };
        render(app, part, vec![(Shape::Wide, wide.clone()), (Shape::Vertical, vertical.clone())]).await?;
        let id = insert_ready(&app.db, court, game, "highlight", &token, rank as i64 + 1, start + a as i64, start + b as i64, &wide, &vertical)?;
        thumbnail(id, &wide, &dir).await;
        made.push((wide, vertical));
    }
    // One reel, best first, both shapes.
    let token = crate::auth::random_token();
    let dir = app.data.join("clips").join(&token);
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Internal(e.to_string()))?;
    let (wide, vertical) = (dir.join("wide.mp4"), dir.join("vertical.mp4"));
    let (wides, verticals): (Vec<_>, Vec<_>) = made.into_iter().unzip();
    let (shape_w, shape_v) = media::output_sizes(p.source);
    let (w, v) = (wide.clone(), vertical.clone());
    tokio::task::spawn_blocking(move || {
        media::concat_encode(&wides, shape_w, &w)?;
        media::concat_encode(&verticals, shape_v, &v)
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))??;
    let id = insert_ready(&app.db, court, game, "reel", &token, 0, start, end, &wide, &vertical)?;
    thumbnail(id, &wide, &dir).await;
    conn_update("done")?;
    tracing::info!(game, singles = reel.singles, "highlights ready");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_ready(
    db: &Db,
    court: i64,
    game: i64,
    kind: &str,
    token: &str,
    rank: i64,
    start: i64,
    end: i64,
    wide: &std::path::Path,
    vertical: &std::path::Path,
) -> rusqlite::Result<i64> {
    let conn = db.conn();
    conn.execute(
        "INSERT INTO clip (court_id, game_id, kind, token, rank, window_start_ms, window_end_ms, made_at, state, wide_path, vertical_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'ready', ?9, ?10)",
        params![court, game, kind, token, rank, start, end, clock::now().to_rfc3339(), wide.to_string_lossy(), vertical.to_string_lossy()],
    )?;
    Ok(conn.last_insert_rowid())
}

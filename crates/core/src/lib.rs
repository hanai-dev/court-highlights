//! Court Highlights rules engine. Every function here is pure and pinned by a golden vector
//! (SRS §7, docs/requirements/golden-vectors.json). Times are Asia/Manila (UTC+8, no DST).

use chrono::{DateTime, Datelike, Duration, FixedOffset, Months, NaiveTime, TimeZone, Timelike};

pub type Time = DateTime<FixedOffset>;

/// Asia/Manila has used UTC+8 without daylight saving since 1978, so a fixed offset is exact.
pub fn manila() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("valid offset")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    ClipSettings,
    CameraOffline { court: String },
    Closed,
    TooManyRecipients,
    Expired,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ClipSettings => f.write_str("Clip length must be before 10–60 s, after 0–15 s"),
            Self::CameraOffline { court } => write!(f, "{court} camera is offline — clips aren't available"),
            Self::Closed => f.write_str("Courts are closed"),
            Self::TooManyRecipients => f.write_str("Up to 8 addresses per game"),
            Self::Expired => f.write_str("These videos have expired"),
        }
    }
}

impl std::error::Error for RuleError {}

/// A span of footage, start inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: Time,
    pub end: Time,
}

// ---------------------------------------------------------------- clips (FR-4, FR-5)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipSettings {
    pub before_s: u32,
    pub after_s: u32,
}

impl ClipSettings {
    pub const DEFAULT: Self = Self { before_s: 25, after_s: 5 };

    pub fn new(before_s: u32, after_s: u32) -> Result<Self, RuleError> {
        if (10..=60).contains(&before_s) && after_s <= 15 { Ok(Self { before_s, after_s }) } else { Err(RuleError::ClipSettings) }
    }
}

/// A tap this close to the previous tap (inclusive) joins its clip.
pub const MERGE_GAP: Duration = Duration::seconds(5);

/// Turns the taps on one court into clip windows: a tap ≤ 5.0 s after the previous tap extends that clip
/// to `after` seconds past the latest tap; otherwise it starts a new clip.
pub fn plan_clips(taps: &[Time], s: ClipSettings) -> Vec<Window> {
    let mut sorted = taps.to_vec();
    sorted.sort();
    let (before, after) = (Duration::seconds(s.before_s.into()), Duration::seconds(s.after_s.into()));
    let mut out: Vec<Window> = Vec::new();
    let mut last_tap: Option<Time> = None;
    for tap in sorted {
        match (last_tap, out.last_mut()) {
            (Some(prev), Some(w)) if tap - prev <= MERGE_GAP => w.end = tap + after,
            _ => out.push(Window { start: tap - before, end: tap + after }),
        }
        last_tap = Some(tap);
    }
    out
}

/// Cuts a clip window to the footage that exists (FR-4, GV-7, GV-8). `recorded` are the spans the camera
/// actually delivered; `None` means nothing was recorded inside the window.
pub fn clamp_to_footage(w: Window, recorded: &[Window]) -> Option<Window> {
    let overlaps = recorded
        .iter()
        .filter_map(|r| {
            let (s, e) = (w.start.max(r.start), w.end.min(r.end));
            (s < e).then_some(Window { start: s, end: e })
        })
        .collect::<Vec<_>>();
    Some(Window { start: overlaps.iter().map(|o| o.start).min()?, end: overlaps.iter().map(|o| o.end).max()? })
}

// ---------------------------------------------------------------- opening hours and games (FR-1, FR-10)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpeningHours {
    pub open: NaiveTime,
    pub close: NaiveTime,
}

impl OpeningHours {
    /// Open from `open` (inclusive) to `close` (exclusive), same day.
    // ponytail: same-day hours only; add past-midnight closing when a venue asks for it.
    pub fn is_open(&self, t: Time) -> bool {
        let now = t.time();
        now >= self.open && now < self.close
    }

    /// Today's closing moment for a time inside opening hours.
    pub fn closing(&self, t: Time) -> Time {
        local_at(t, self.close)
    }
}

pub const GAME_LIMIT: Duration = Duration::hours(2);

/// A game stops by itself after 2 h or at closing time, whichever comes first (GV-9, GV-21).
pub fn game_auto_stop(start: Time, closing: Option<Time>) -> Time {
    let limit = start + GAME_LIMIT;
    closing.map_or(limit, |c| c.min(limit))
}

/// Gate for a tap or a Start: outside opening hours nothing is recorded (GV-21).
pub fn require_open(hours: &OpeningHours, t: Time) -> Result<(), RuleError> {
    if hours.is_open(t) { Ok(()) } else { Err(RuleError::Closed) }
}

// ---------------------------------------------------------------- retention (FR-1, FR-18, FR-32, FR-45)

pub const ROLLING: Duration = Duration::hours(4);
pub const COURT_PAGE_WINDOW: Duration = Duration::hours(2);
pub const EXPIRY: Duration = Duration::hours(72);
pub const HOLD_MAX: Duration = Duration::days(60);
pub const CLEANUP_HOUR: u32 = 3;

/// Rolling footage older than this is overwritten unless a clip, game or hold protects it (GV-13).
pub fn rolling_cutoff(now: Time) -> Time {
    now - ROLLING
}

/// Rolling footage may be overwritten once it is older than the cutoff and no clip, game or hold covers it
/// (GV-13, GV-18).
pub fn rolling_deletable(span: Window, now: Time, protected: &[Window]) -> bool {
    span.end <= rolling_cutoff(now) && !protected.iter().any(|p| p.start < span.end && span.start < p.end)
}

/// A court page lists items made within the last 2 h, inclusive (GV-14).
pub fn on_court_page(made: Time, now: Time) -> bool {
    made <= now && now - made <= COURT_PAGE_WINDOW
}

/// The first 03:00 clean-up at or after `t`.
pub fn cleanup_at_or_after(t: Time) -> Time {
    let candidate = local_at(t, NaiveTime::from_hms_opt(CLEANUP_HOUR, 0, 0).expect("valid"));
    if candidate >= t { candidate } else { candidate + Duration::days(1) }
}

/// Clips and game videos are deleted at the first 03:00 clean-up ≥ 72 h after they were made (GV-10, GV-11).
pub fn expires_at(made: Time) -> Time {
    cleanup_at_or_after(made + EXPIRY)
}

/// Kept items are deleted at the first clean-up ≥ 1 calendar year after Keep was pressed (GV-12).
pub fn keep_expires_at(kept: Time) -> Time {
    cleanup_at_or_after(kept.checked_add_months(Months::new(12)).expect("in range"))
}

/// A footage hold releases itself 60 days after it was placed (GV-18).
pub fn hold_release_at(placed: Time) -> Time {
    placed + HOLD_MAX
}

/// Email links and online copies stop working at the expiry moment; sending stops then too (GV-24, GV-26).
pub fn still_available(expiry: Time, now: Time) -> Result<(), RuleError> {
    if now < expiry { Ok(()) } else { Err(RuleError::Expired) }
}

/// "available until Fri 23 Oct 2026, 3:00 am" (GV-10).
pub fn expiry_text(expiry: Time) -> String {
    let (pm, hour) = expiry.hour12();
    format!(
        "available until {} {} {} {}, {}:{:02} {}",
        expiry.format("%a"),
        expiry.day(),
        expiry.format("%b"),
        expiry.year(),
        hour,
        expiry.minute(),
        if pm { "pm" } else { "am" }
    )
}

// ---------------------------------------------------------------- email list (FR-20)

pub const MAX_RECIPIENTS: usize = 8;

pub fn can_add_recipient(current: usize) -> Result<(), RuleError> {
    if current < MAX_RECIPIENTS { Ok(()) } else { Err(RuleError::TooManyRecipients) }
}

// ---------------------------------------------------------------- sign-in (FR-28)

pub const LOCKOUT_AFTER: u32 = 5;
pub const LOCKOUT_FOR: Duration = Duration::minutes(15);
pub const SESSION_LIFETIME: Duration = Duration::hours(12);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignIn {
    Accepted,
    WrongPassword,
    Locked,
}

/// Counts consecutive wrong passwords; the 5th locks the account for 15 min from that failure. While locked
/// even the right password is refused; a successful sign-in resets the count (GV-16).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lockout {
    pub consecutive_failures: u32,
    pub locked_until: Option<Time>,
}

impl Lockout {
    pub fn attempt(&mut self, at: Time, password_ok: bool) -> SignIn {
        if self.locked_until.is_some_and(|until| at < until) {
            return SignIn::Locked;
        }
        self.locked_until = None;
        if password_ok {
            self.consecutive_failures = 0;
            return SignIn::Accepted;
        }
        self.consecutive_failures += 1;
        if self.consecutive_failures >= LOCKOUT_AFTER {
            self.consecutive_failures = 0;
            self.locked_until = Some(at + LOCKOUT_FOR);
        }
        SignIn::WrongPassword
    }
}

/// Staff sessions end 12 h after sign-in, whatever the activity (GV-17).
pub fn session_valid(signed_in: Time, now: Time) -> bool {
    now >= signed_in && now - signed_in < SESSION_LIFETIME
}

// ---------------------------------------------------------------- sending order (FR-22)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outgoing {
    Clip { made: Time },
    Reel,
    GameVideo,
}

/// Everything a stopped game has to send. Clips and highlights of every game go first (oldest game
/// first, clips by time, then the reel); game videos follow, oldest game first (GV-19).
pub fn send_order<T: Clone>(items: &[(Time, Outgoing, T)]) -> Vec<T> {
    let mut v = items.to_vec();
    v.sort_by_key(|(game_stopped, kind, _)| match kind {
        Outgoing::Clip { made } => (0, *game_stopped, 0, Some(*made)),
        Outgoing::Reel => (0, *game_stopped, 1, None),
        Outgoing::GameVideo => (1, *game_stopped, 0, None),
    });
    v.into_iter().map(|(_, _, t)| t).collect()
}

// ---------------------------------------------------------------- disk pressure (FR-46)

pub const DISK_WARN_PCT: u64 = 80;
pub const DISK_CLEAN_PCT: u64 = 90;
pub const DISK_TARGET_PCT: u64 = 85;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deletable<T> {
    pub id: T,
    pub is_game_video: bool,
    pub made: Time,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskPlan<T> {
    pub warning: bool,
    pub delete: Vec<T>,
}

/// Only unkept, unheld clips and game videos are passed in, so rolling footage and protected items can
/// never be chosen. At ≥ 90 % the oldest game videos go first, then clips, until usage is below 85 % (GV-20).
pub fn disk_plan<T: Clone>(used: u64, total: u64, candidates: &[Deletable<T>]) -> DiskPlan<T> {
    let pct_at_least = |u: u64, p: u64| u.saturating_mul(100) >= total.saturating_mul(p);
    let mut plan = DiskPlan { warning: pct_at_least(used, DISK_WARN_PCT), delete: Vec::new() };
    if !pct_at_least(used, DISK_CLEAN_PCT) {
        return plan;
    }
    let mut order = candidates.to_vec();
    order.sort_by_key(|d| (!d.is_game_video, d.made));
    let mut remaining = used;
    for d in order {
        if !pct_at_least(remaining, DISK_TARGET_PCT) {
            break;
        }
        remaining = remaining.saturating_sub(d.bytes);
        plan.delete.push(d.id);
    }
    plan
}

// ---------------------------------------------------------------- highlights (FR-14)

pub const HIGHLIGHT_MAX: Duration = Duration::seconds(20);
pub const HIGHLIGHT_TAIL: Duration = Duration::seconds(2);
pub const HIGHLIGHTS_PER_GAME: usize = 5;

/// The whole point plus 2 s, keeping only the last 20 s of a long point (GV-22).
pub fn highlight_span(point_start: Time, point_end: Time) -> Window {
    let end = point_end + HIGHLIGHT_TAIL;
    Window { start: point_start.max(end - HIGHLIGHT_MAX), end }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReelPlan {
    pub reel: bool,
    pub singles: usize,
    pub message: Option<&'static str>,
}

/// Up to 5 single points plus a reel; none found shows a message and the game video stays (GV-23).
pub fn reel_plan(found: usize) -> ReelPlan {
    if found == 0 {
        ReelPlan { reel: false, singles: 0, message: Some("No highlights found") }
    } else {
        ReelPlan { reel: true, singles: found.min(HIGHLIGHTS_PER_GAME), message: None }
    }
}

// ---------------------------------------------------------------- helpers

fn local_at(t: Time, at: NaiveTime) -> Time {
    let tz = manila();
    tz.from_local_datetime(&t.with_timezone(&tz).date_naive().and_time(at)).single().expect("fixed offset is unambiguous")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hold_release_is_sixty_days() {
        let t = manila().with_ymd_and_hms(2026, 10, 20, 10, 0, 0).unwrap();
        assert_eq!(hold_release_at(t), manila().with_ymd_and_hms(2026, 12, 19, 10, 0, 0).unwrap());
    }
}

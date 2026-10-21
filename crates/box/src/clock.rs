//! The box's notion of "now", in Manila time. Test builds (`--features test-hooks`) can move it forward so expiry
//! and lockout rules can be exercised end to end; release builds have no way to change it from outside.

use ch_core::{Time, manila};
use std::sync::atomic::{AtomicI64, Ordering};

static OFFSET_MS: AtomicI64 = AtomicI64::new(0);

pub fn now() -> Time {
    (chrono::Utc::now() + chrono::Duration::milliseconds(OFFSET_MS.load(Ordering::Relaxed))).with_timezone(&manila())
}

#[cfg(feature = "test-hooks")]
pub fn advance(ms: i64) {
    OFFSET_MS.fetch_add(ms, Ordering::Relaxed);
}

pub fn parse(s: &str) -> Option<Time> {
    chrono::DateTime::parse_from_rfc3339(s).ok()
}

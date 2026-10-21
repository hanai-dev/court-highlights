//! The box's notion of "now", in Manila time: the clock battery's time plus the Owner's correction (FR-38), which is
//! stored with the venue settings. Test builds (`--features test-hooks`) can also move it forward so expiry and
//! lockout rules can be exercised end to end; release builds have no other way to change it from outside.

use ch_core::{Time, manila};
use std::sync::atomic::{AtomicI64, Ordering};

static CORRECTION_MS: AtomicI64 = AtomicI64::new(0);
static TEST_OFFSET_MS: AtomicI64 = AtomicI64::new(0);

pub fn now() -> Time {
    let offset = CORRECTION_MS.load(Ordering::Relaxed) + TEST_OFFSET_MS.load(Ordering::Relaxed);
    (chrono::Utc::now() + chrono::Duration::milliseconds(offset)).with_timezone(&manila())
}

/// Sets the correction so that `now()` reads `wanted`; returns the correction to store.
pub fn correct_to(wanted: Time) -> i64 {
    let without = now() - chrono::Duration::milliseconds(CORRECTION_MS.load(Ordering::Relaxed));
    let correction = (wanted - without).num_milliseconds();
    CORRECTION_MS.store(correction, Ordering::Relaxed);
    correction
}

pub fn load_correction(ms: i64) {
    CORRECTION_MS.store(ms, Ordering::Relaxed);
}

#[cfg(feature = "test-hooks")]
pub fn advance(ms: i64) {
    TEST_OFFSET_MS.fetch_add(ms, Ordering::Relaxed);
}

pub fn parse(s: &str) -> Option<Time> {
    chrono::DateTime::parse_from_rfc3339(s).ok()
}

//! Counters for the platform's metrics scrape (HOST-observability): `/metrics` in Prometheus text format, behind the
//! readiness token. Counts only — never an address, key or token.

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

pub static UPLOADS: AtomicU64 = AtomicU64::new(0);
pub static EMAILS_SENT: AtomicU64 = AtomicU64::new(0);
pub static EMAILS_FAILED: AtomicU64 = AtomicU64::new(0);
pub static CHECKINS: AtomicU64 = AtomicU64::new(0);
pub static FLEET_ACTIONS: AtomicU64 = AtomicU64::new(0);

pub fn add(counter: &AtomicU64, n: u64) {
    counter.fetch_add(n, Relaxed);
}

pub fn render() -> String {
    let counters: [(&str, &str, &AtomicU64); 5] = [
        ("ch_uploads_total", "Videos uploaded by boxes", &UPLOADS),
        ("ch_emails_sent_total", "Take-home emails accepted by the email service", &EMAILS_SENT),
        ("ch_emails_failed_total", "Take-home emails the email service refused", &EMAILS_FAILED),
        ("ch_checkins_total", "Box check-ins", &CHECKINS),
        ("ch_fleet_actions_total", "Logged fleet console actions", &FLEET_ACTIONS),
    ];
    counters.iter().map(|(name, help, c)| format!("# HELP {name} {help}\n# TYPE {name} counter\n{name} {}\n", c.load(Relaxed))).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn renders_prometheus_counters() {
        super::add(&super::UPLOADS, 2);
        let text = super::render();
        assert!(text.contains("# TYPE ch_uploads_total counter\nch_uploads_total 2\n"), "{text}");
        assert_eq!(text.lines().filter(|l| l.starts_with("# TYPE")).count(), 5);
    }
}

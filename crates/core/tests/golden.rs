//! Golden vectors (SRS §7). Expected values are read from docs/requirements/golden-vectors.json — the approved
//! oracle — never from the implementation under test. One test per vector: `cargo test -p ch-core --test golden gv_N`.

use ch_core::*;
use chrono::{DateTime, Duration, NaiveTime, TimeZone};
use serde_json::Value;

fn vector(id: &str) -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/requirements/golden-vectors.json");
    let all: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("golden-vectors.json")).unwrap();
    all["vectors"].as_array().unwrap().iter().find(|v| v["id"] == id).cloned().unwrap_or_else(|| panic!("{id} missing"))
}

/// "19:40:30" or "19:40:30.0" on the fixture day, or a full RFC 3339 timestamp.
fn t(s: &Value) -> Time {
    let s = s.as_str().expect("time string");
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return dt;
    }
    let time = NaiveTime::parse_from_str(s, "%H:%M:%S%.f")
        .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M"))
        .unwrap_or_else(|_| panic!("bad time {s}"));
    manila().from_local_datetime(&chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_time(time)).unwrap()
}

fn window(pair: &Value) -> Window {
    Window { start: t(&pair[0]), end: t(&pair[1]) }
}

fn windows(list: &Value) -> Vec<Window> {
    list.as_array().unwrap().iter().map(window).collect()
}

fn taps(list: &Value) -> Vec<Time> {
    list.as_array().unwrap().iter().map(t).collect()
}

#[test]
fn gv_1() {
    let v = vector("GV-1");
    assert_eq!(plan_clips(&[t(&v["input"]["tap"])], ClipSettings::DEFAULT), vec![window(&v["expected"]["clip"])]);
}

#[test]
fn gv_2() {
    let v = vector("GV-2");
    let s = ClipSettings::new(v["input"]["before_s"].as_u64().unwrap() as u32, v["input"]["after_s"].as_u64().unwrap() as u32).unwrap();
    assert_eq!(plan_clips(&[t(&v["input"]["tap"])], s), vec![window(&v["expected"]["clip"])]);
}

#[test]
fn gv_3() {
    let v = vector("GV-3");
    let s = ClipSettings::new(v["input"]["before_s"].as_u64().unwrap() as u32, v["input"]["after_s"].as_u64().unwrap() as u32).unwrap();
    assert_eq!(plan_clips(&[t(&v["input"]["tap"])], s), vec![window(&v["expected"]["clip"])]);
    let message = v["expected"]["invalid_settings"].as_str().unwrap().trim_start_matches("refused: ");
    for bad in v["input"]["invalid_settings"].as_array().unwrap() {
        let before = bad["before_s"].as_u64().unwrap_or(25) as u32;
        let after = bad["after_s"].as_u64().unwrap_or(5) as u32;
        let err = ClipSettings::new(before, after).unwrap_err();
        assert!(err.to_string().contains(message), "{bad}: {err}");
    }
}

#[test]
fn gv_4() {
    let v = vector("GV-4");
    assert_eq!(plan_clips(&taps(&v["input"]["taps"]), ClipSettings::DEFAULT), windows(&v["expected"]["clips"]));
}

#[test]
fn gv_5() {
    let v = vector("GV-5");
    assert_eq!(plan_clips(&taps(&v["input"]["taps"]), ClipSettings::DEFAULT), windows(&v["expected"]["clips"]));
}

#[test]
fn gv_6() {
    let v = vector("GV-6");
    assert_eq!(plan_clips(&taps(&v["input"]["taps"]), ClipSettings::DEFAULT), windows(&v["expected"]["clips"]));
}

#[test]
fn gv_7() {
    let v = vector("GV-7");
    let offline = window(&v["input"]["camera_offline"]);
    let day = |h, m| manila().with_ymd_and_hms(2026, 10, 20, h, m, 0).unwrap();
    let recorded = [Window { start: day(19, 0), end: offline.start }, Window { start: offline.end, end: day(20, 0) }];
    let planned = plan_clips(&[t(&v["input"]["tap"])], ClipSettings::DEFAULT);
    let cut: Vec<Window> = planned.iter().filter_map(|w| clamp_to_footage(*w, &recorded)).collect();
    assert_eq!(cut.len(), v["expected"]["clips"].as_array().unwrap().len());
    let refusal = RuleError::CameraOffline { court: "Court 2".into() };
    assert_eq!(refusal.to_string(), v["expected"]["message"].as_str().unwrap());
}

#[test]
fn gv_8() {
    let v = vector("GV-8");
    let started = t(&v["input"]["recording_started"]);
    let recorded = [Window { start: started, end: started + Duration::hours(1) }];
    let planned = plan_clips(&[t(&v["input"]["tap"])], ClipSettings::DEFAULT);
    assert_eq!(clamp_to_footage(planned[0], &recorded), Some(window(&v["expected"]["clip"])));
}

#[test]
fn gv_9() {
    let v = vector("GV-9");
    let start = t(&v["input"]["start"]);
    let stop = game_auto_stop(start, None);
    assert_eq!(stop, t(&v["expected"]["stopped_at"]));
    assert_eq!(Window { start, end: stop }, window(&v["expected"]["game_video"]));
}

#[test]
fn gv_10() {
    let v = vector("GV-10");
    let expiry = expires_at(t(&v["input"]["made"]));
    assert_eq!(expiry, t(&v["expected"]["deleted_at_cleanup"]));
    assert_eq!(expiry_text(expiry), v["expected"]["email_text"].as_str().unwrap());
}

#[test]
fn gv_11() {
    let v = vector("GV-11");
    let made = v["input"]["made"].as_array().unwrap();
    let expected = v["expected"]["deleted_at_cleanup"].as_array().unwrap();
    for (m, e) in made.iter().zip(expected) {
        assert_eq!(expires_at(t(m)), t(e), "made {m}");
    }
}

#[test]
fn gv_12() {
    let v = vector("GV-12");
    assert_eq!(keep_expires_at(t(&v["input"]["kept_at"])), t(&v["expected"]["deleted_at_cleanup"]));
}

#[test]
fn gv_13() {
    let v = vector("GV-13");
    let now = t(&v["input"]["now"]);
    let retained = window(&v["expected"]["retained"]);
    assert_eq!(rolling_cutoff(now), t(&v["expected"]["deleted_before"]));
    assert_eq!(Window { start: rolling_cutoff(now), end: now }, retained);
    let older = Window { start: retained.start - Duration::minutes(10), end: retained.start };
    let newer = Window { start: retained.start, end: retained.start + Duration::minutes(10) };
    assert!(rolling_deletable(older, now, &[]));
    assert!(!rolling_deletable(newer, now, &[]));
}

#[test]
fn gv_14() {
    let v = vector("GV-14");
    let now = t(&v["input"]["page_opened"]);
    for listed in v["expected"]["listed"].as_array().unwrap() {
        assert!(on_court_page(t(listed), now), "{listed} should be listed");
    }
    for hidden in v["expected"]["not_listed"].as_array().unwrap() {
        assert!(!on_court_page(t(hidden), now), "{hidden} should not be listed");
    }
}

#[test]
fn gv_15() {
    let v = vector("GV-15");
    let current = v["input"]["existing_addresses"].as_u64().unwrap() as usize;
    let err = can_add_recipient(current).unwrap_err();
    assert!(v["expected"]["refused"].as_bool().unwrap());
    assert_eq!(err.to_string(), v["expected"]["message"].as_str().unwrap());
    assert_eq!(current, v["expected"]["addresses"].as_u64().unwrap() as usize);
    assert!(can_add_recipient(current - 1).is_ok());
}

#[test]
fn gv_16() {
    let v = vector("GV-16");
    let failures = v["input"]["consecutive_failures"].as_u64().unwrap() as i64;
    let fifth = t(&v["input"]["fifth_failure_at"]);
    let mut lock = Lockout::default();
    for i in (0..failures).rev() {
        assert_eq!(lock.attempt(fifth - Duration::seconds(i), false), SignIn::WrongPassword);
    }
    let expected: Vec<&str> = v["expected"]["results"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
    for (at, want) in v["input"]["correct_password_at"].as_array().unwrap().iter().zip(expected) {
        let got = match lock.attempt(t(at), true) {
            SignIn::Accepted => "accepted",
            SignIn::Locked | SignIn::WrongPassword => "refused",
        };
        assert_eq!(got, want, "correct password at {at}");
    }
    assert!(v["expected"]["success_resets_count"].as_bool().unwrap());
    let base = fifth + Duration::hours(1);
    let mut fresh = Lockout::default();
    for i in 0..4 {
        fresh.attempt(base + Duration::seconds(i), false);
    }
    assert_eq!(fresh.attempt(base + Duration::seconds(5), true), SignIn::Accepted);
    for i in 6..10 {
        assert_eq!(fresh.attempt(base + Duration::seconds(i), false), SignIn::WrongPassword);
    }
    assert_eq!(fresh.attempt(base + Duration::seconds(11), true), SignIn::Accepted, "success must reset the count");
}

#[test]
fn gv_17() {
    let v = vector("GV-17");
    let signed_in = t(&v["input"]["signed_in"]);
    let refused_from = t(&v["expected"]["refused_from"]);
    assert!(session_valid(signed_in, refused_from - Duration::seconds(1)));
    assert!(!session_valid(signed_in, refused_from));
}

#[test]
fn gv_18() {
    let v = vector("GV-18");
    let placed = t(&v["input"]["hold_placed"]);
    assert_eq!(hold_release_at(placed), t(&v["expected"]["auto_release"]));
    let span = v["input"]["span"].as_array().unwrap();
    let held = Window { start: t(&span[0]), end: t(&span[1]) };
    let much_later = held.end + Duration::hours(10);
    assert!(rolling_deletable(held, much_later, &[]), "unprotected footage would be overwritten");
    assert_eq!(!rolling_deletable(held, much_later, &[held]), v["expected"]["protected_while_held"].as_bool().unwrap());
}

#[test]
fn gv_19() {
    let v = vector("GV-19");
    let mut items = Vec::new();
    for g in v["input"]["games"].as_array().unwrap() {
        let id = g["id"].as_str().unwrap();
        let stopped = t(&g["stopped"]);
        for n in 1..=g["clips"].as_u64().unwrap() {
            items.push((stopped, Outgoing::Clip { made: stopped - Duration::minutes(60 - n as i64) }, format!("{id} clip {n}")));
        }
        if g["reel"].as_bool().unwrap() {
            items.push((stopped, Outgoing::Reel, format!("{id} reel")));
        }
        if g["video"].as_bool().unwrap() {
            items.push((stopped, Outgoing::GameVideo, format!("{id} video 720p")));
        }
    }
    items.reverse(); // the input order must not matter
    let expected: Vec<String> = v["expected"]["order"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    assert_eq!(send_order(&items), expected);
}

#[test]
fn gv_20() {
    let v = vector("GV-20");
    let used_pct = v["input"]["disk_percent"].as_u64().unwrap();
    let day = |d| manila().with_ymd_and_hms(2026, 10, d, 20, 0, 0).unwrap();
    // Fixture sizes (4 % each) on a 100-unit disk; the oracle is the deletion order and the thresholds.
    let names: Vec<&str> = v["input"]["unkept_game_videos"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    let mut candidates: Vec<Deletable<String>> = names
        .iter()
        .enumerate()
        .map(|(i, n)| Deletable { id: n.to_string(), is_game_video: true, made: day(19 + i as u32), bytes: 4 })
        .collect();
    candidates.push(Deletable { id: "older clip".into(), is_game_video: false, made: day(18), bytes: 1 });
    let plan = disk_plan(used_pct, 100, &candidates);
    let expected: Vec<String> = v["expected"]["delete_order"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
    assert_eq!(plan.delete, expected);
    assert!(plan.warning);
    let stop_below = v["expected"]["stop_below_percent"].as_u64().unwrap();
    assert_eq!(DISK_TARGET_PCT, stop_below);
    assert!(used_pct - 8 < stop_below && used_pct - 4 >= stop_below, "stops as soon as usage is below the target");
    assert_eq!(ROLLING, Duration::hours(v["expected"]["rolling_footage_hours_kept"].as_i64().unwrap()));
    let warn = v["expected"]["warning_from_percent"].as_u64().unwrap();
    assert!(disk_plan(warn, 100, &candidates).warning && !disk_plan(warn - 1, 100, &candidates).warning);
    assert!(disk_plan(DISK_CLEAN_PCT - 1, 100, &candidates).delete.is_empty());
}

#[test]
fn gv_21() {
    let v = vector("GV-21");
    let h = v["input"]["hours"].as_array().unwrap();
    let hours = OpeningHours {
        open: NaiveTime::parse_from_str(h[0].as_str().unwrap(), "%H:%M").unwrap(),
        close: NaiveTime::parse_from_str(h[1].as_str().unwrap(), "%H:%M").unwrap(),
    };
    let rec = window(&v["expected"]["recording"]);
    assert!(!hours.is_open(t(&v["expected"]["not_recording_at"])));
    assert!(hours.is_open(rec.start));
    assert!(hours.is_open(rec.end - Duration::seconds(1)));
    assert!(!hours.is_open(rec.end));
    let started = t(&v["input"]["game_started"]);
    assert_eq!(game_auto_stop(started, Some(hours.closing(started))), t(&v["expected"]["game_stopped_at"]));
    let err = require_open(&hours, t(&v["input"]["tap"])).unwrap_err();
    assert_eq!(format!("refused: {err}"), v["expected"]["tap"].as_str().unwrap());
}

#[test]
fn gv_22() {
    let v = vector("GV-22");
    let points = windows(&v["input"]["points"]);
    let got: Vec<Window> = points.iter().map(|p| highlight_span(p.start, p.end)).collect();
    assert_eq!(got, windows(&v["expected"]["highlights"]));
}

#[test]
fn gv_23() {
    let v = vector("GV-23");
    let found: Vec<usize> = v["input"]["found"].as_array().unwrap().iter().map(|n| n.as_u64().unwrap() as usize).collect();
    let results: Vec<&str> = v["expected"]["results"].as_array().unwrap().iter().map(|s| s.as_str().unwrap()).collect();
    let three = reel_plan(found[0]);
    assert!(three.reel && three.message.is_none());
    assert_eq!(results[0], format!("reel + {} single points", three.singles));
    let none = reel_plan(found[1]);
    assert!(!none.reel && none.singles == 0);
    assert!(results[1].starts_with(none.message.unwrap()));
    assert_eq!(reel_plan(9).singles, HIGHLIGHTS_PER_GAME);
}

#[test]
fn gv_24() {
    let v = vector("GV-24");
    let expiry = expires_at(t(&v["input"]["clip_made"]));
    let opened = v["input"]["opened"].as_array().unwrap();
    let results = v["expected"]["results"].as_array().unwrap();
    assert_eq!(results[0], "plays");
    assert!(still_available(expiry, t(&opened[0])).is_ok());
    assert_eq!(still_available(expiry, t(&opened[1])).unwrap_err().to_string(), results[1].as_str().unwrap());
}

#[test]
fn gv_26() {
    let v = vector("GV-26");
    let expiry = expires_at(t(&v["input"]["game_stopped"]));
    let on = v["input"]["switched_on"].as_array().unwrap();
    let results = v["expected"]["results"].as_array().unwrap();
    assert_eq!(results[0], "sent after reconnect");
    assert!(still_available(expiry, t(&on[0])).is_ok());
    assert!(still_available(expiry, t(&on[1])).is_err());
    assert!(results[1].as_str().unwrap().contains(&expiry.to_rfc3339()), "expiry {}", expiry.to_rfc3339());
}

//! Shot judge, baseline: finds points as bursts of movement between quiet spells and ranks them by length ×
//! intensity (FR-14). Advisory only — it never hides or deletes anything (FR-16).
// ponytail: movement-only baseline; the learned judge (ort, OpenVINO on the N150) replaces this behind the same
// `best_points` signature once the hand-marked partner-court footage exists (NFR-17, F-JUDGE-EVAL).

/// A point, in milliseconds of recorded time from the start of the game.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub start_ms: u64,
    pub end_ms: u64,
    pub score: f32,
}

const MIN_POINT_MS: u64 = 3_000;
const JOIN_GAP_MS: u64 = 1_000;

/// `samples`: (ms, movement) about 5 a second. Returns at most `n` points, best first.
pub fn best_points(samples: &[(u64, f32)], n: usize) -> Vec<Point> {
    let mut points = find_points(samples);
    points.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.start_ms.cmp(&b.start_ms)));
    points.truncate(n);
    points
}

fn find_points(samples: &[(u64, f32)]) -> Vec<Point> {
    if samples.len() < 5 {
        return Vec::new();
    }
    // Single-frame jumps (a cut, a camera bump) are capped at 3× the median so they can't set the scale; then a
    // 1 s centred moving average, "active" above half its 75th percentile (never below 1.0).
    let quantile = |v: &[f32], q: usize| {
        let mut sorted = v.to_vec();
        sorted.sort_by(f32::total_cmp);
        sorted[(sorted.len() - 1) * q / 100]
    };
    let raw: Vec<f32> = samples.iter().map(|s| s.1).collect();
    let cap = (quantile(&raw, 50) * 3.0).max(1.0);
    let smooth: Vec<f32> = (0..raw.len())
        .map(|i| {
            let w = &raw[i.saturating_sub(2)..(i + 3).min(raw.len())];
            w.iter().map(|v| v.min(cap)).sum::<f32>() / w.len() as f32
        })
        .collect();
    let threshold = (quantile(&smooth, 75) * 0.5).max(1.0);

    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, v) in smooth.iter().enumerate() {
        if *v <= threshold {
            continue;
        }
        match runs.last_mut() {
            Some((_, end)) if samples[i].0.saturating_sub(samples[*end].0) <= JOIN_GAP_MS => *end = i,
            _ => runs.push((i, i)),
        }
    }
    runs.into_iter()
        .filter_map(|(s, e)| {
            let (start_ms, end_ms) = (samples[s].0, samples[e].0);
            (end_ms - start_ms >= MIN_POINT_MS).then(|| {
                let mean = smooth[s..=e].iter().sum::<f32>() / (e - s + 1) as f32;
                Point { start_ms, end_ms, score: (end_ms - start_ms) as f32 / 1000.0 * mean }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5 samples a second: quiet 4 s, rally 6 s, quiet 2 s, rally 8 s, quiet 3 s.
    fn game() -> Vec<(u64, f32)> {
        let mut out = Vec::new();
        let mut t = 0;
        for (secs, level) in [(4, 0.0), (6, 20.0), (2, 0.0), (8, 20.0), (3, 0.0)] {
            for _ in 0..secs * 5 {
                out.push((t, level));
                t += 200;
            }
        }
        out
    }

    #[test]
    fn finds_rallies_and_ranks_the_longest_first() {
        let best = best_points(&game(), 5);
        assert_eq!(best.len(), 2);
        // The 8 s rally (12–20 s) first, then the 6 s one (4–10 s); edges move ≤ 0.4 s with the smoothing.
        assert!(best[0].start_ms.abs_diff(12_000) <= 400 && best[0].end_ms.abs_diff(20_000) <= 600, "{best:?}");
        assert!(best[1].start_ms.abs_diff(4_000) <= 400 && best[1].end_ms.abs_diff(10_000) <= 600, "{best:?}");
        assert_eq!(best_points(&game(), 1).len(), 1);
    }

    #[test]
    fn single_frame_jumps_do_not_hide_rallies() {
        // Rallies at movement 6 with a 100 spike where the picture changes (a cut, a camera bump).
        let mut g = game();
        for s in g.iter_mut() {
            s.1 = if s.1 > 0.0 { 6.0 } else { 0.0 };
        }
        for i in [20, 49, 60, 99] {
            g[i].1 = 100.0;
        }
        let best = best_points(&g, 5);
        assert_eq!(best.len(), 2, "{best:?}");
        assert!(best[0].end_ms - best[0].start_ms > best[1].end_ms - best[1].start_ms);
    }

    #[test]
    fn dead_time_makes_no_points() {
        let still: Vec<(u64, f32)> = (0..300).map(|i| (i * 200, 0.3)).collect();
        assert!(best_points(&still, 5).is_empty());
        let blip: Vec<(u64, f32)> = (0..50).map(|i| (i * 200, if (10..15).contains(&i) { 30.0 } else { 0.0 })).collect();
        assert!(best_points(&blip, 5).is_empty(), "a 1 s blip is not a point");
    }
}

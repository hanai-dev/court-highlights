# Test summary — Court Highlights 0.1.0

Build run `forge/build-261021-0243`, 2026-10-21 → 2026-10-22. On 2026-10-22 the owner removed the online service, <!-- local-only: change record -->
take-home email and the fleet console (change record at the top of `docs/requirements/requirements.md`); the layers <!-- local-only: change record -->
below describe the suite after that change, and the scores are the build run's, re-baselining pending. The test plan is the acceptance matrix
(`build-results.tsv`, 110 rows from `docs/requirements/court-highlights.checks.json`) plus the layers below; every
row passes only on a stored evidence file.

## Result

**PASS_RATE 0.78** (strict evidence) · logic gate PASS · status **BLOCKED**: every open row needs the Intel N150 box
or the trial court (list below).

| Dimension | Weight | Score | Rows |
|---|---|---|---|
| logic (golden vectors) | 0.30 | 1.00 | 25/25 |
| functional | 0.30 | 0.96 | 24/25 |
| ux | 0.20 | 0.60 | 6/11 |
| devops | 0.15 | 0.53 | 8/15 |
| monitoring | 0.15 | 0.55 | 5/9 |
| hardening | 0.20 | 0.76 | 19/25 |

## Layers

| Layer | What | Where | Latest |
|---|---|---|---|
| Unit | box 17, rules engine 22 (21 golden vectors) | `cargo test` in CI `checks` | pass |
| End to end | 61 Playwright tests in 12 specs against the compose stack: replayed RTSP cameras, three-box venue, Chromium and WebKit | CI `e2e` job (GitHub Actions) | the build run's run 5 (74 passed, 2 failed and fixed) was on the earlier 82-test suite |
| Accessibility | axe (WCAG 2.2 AA tags, serious/critical = 0) on the player, setup and staff console routes; 7:1 contrast; 48 px court targets; keyboard-only SC-6/7/8; reduced motion | `ux.spec.ts` | axe pass; contrast/targets fixed in run 6 |
| Design | design-scan on 22 routes × 3 viewports → SLOP_GATE PASS; saving-ring motion audit (normal and reduced) PASS | `evidence/design/` | pass |
| Load | 100 simulated phones across 3 boxes; court page p95 180 ms on throttled Wi-Fi (target 2 s) | `venue.spec.ts` | pass |
| Security | injection, IDOR, CSRF/CORS/cookies, roles, brute force, wipe, sync key and TLS, log privacy scan, gitleaks, osv-scanner, Trivy (box OS packages), licence gates, venue-network-only check | specs + `tools/` + CI | see security record in `handoff.json` |
| Operations | power-cut recovery, signed USB updates with automatic rollback, settings backup and restore | `games.spec.ts`, `update.spec.ts`, `venue.spec.ts` | see the CI `e2e` job |

## Defects found by testing and fixed

| Found by | Defect | Fix |
|---|---|---|
| Log privacy scan | The cross-site warning logged a clip's access token in the path | Paths are redacted before logging |
| Venue lockout test (CI) | A sign-in on one box could undo a lockout made on another (whole-row sync) | Later lock and session cut-off always kept |
| Design scan | The staff console overflowed at phone width; cards in cards; dashes in headings | Grid tracks `minmax(0, 1fr)`, flattened tables, copy |
| Design scan | Broken preview image before a camera's first picture | Preview only once footage exists; text until then |
| Contrast check | Court status text at 5.9:1 | Status words in the 11:1 container inks |
| Target check | Court page checkbox 44 px | 48 px |
| Many specs | Library keep lost the active filters | Keep returns to the same filtered page |

## Open rows

| Needs | Rows |
|---|---|
| The Intel N150 box (`tools/spike/n150-spike.sh`) | DEV-SPIKE, MON-CLIP-SPEED, MON-JUDGE-THROUGHPUT, MON-RECORDING-GAPS (24 h), DEV-INSTALL-IMAGE, SEC-SN5-STOLEN-BOX (disk encryption), DEV-ENVIRONMENTS (test-box CI leg) |
| The trial court | F-JUDGE-EVAL (hand-marked footage), UX-FIRST-USE (observed players) |
| CI run 6 (in progress at writing) | SEC-INJECTION, SEC-LOGS-NO-PII, UX-A11Y, UX-ASSETS, UX-FORM-ARCHETYPE, UX-MOTION-KEYBOARD |

## Not measured

- **Code coverage.** No coverage figure exists yet: most behaviour is exercised end to end against the running box,
  and instrumented binaries would change the timing the clip and recording tests depend on. Next step: a separate
  CI job with `cargo llvm-cov` over the unit tests plus an instrumented, timing-tolerant e2e subset.
- **Clip speed and judge throughput** on real hardware: only the N150 can show them (the laptop has no Intel VA encoder).

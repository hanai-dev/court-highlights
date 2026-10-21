# Project charter — Court Highlights

Run: `forge/build-261021-0243` · spec `docs/requirements/court-highlights.spec.yaml` (VALID, acceptance plan
`683a0941…c1e3`) · repo https://github.com/hanai-dev/court-highlights (private) · owner: Jss-on (sole maintainer).

## Vision
Offline-first court cameras for Philippine pickleball venues: every court records during opening hours, a player taps
**Clip that** and has the last 30 s with the venue logo on their phone in seconds, whole games and the best shots arrive
within the hour, all without internet; when the venue is online, take-home emails go out and the owner's fleet console
watches every box. First venue and trial court: Paddle Point Leisure Hub, Cagayan de Oro.

## Objectives (measured by the pinned acceptance plan, 110 checks, 6 dimensions)
1. All 25 golden rules (clip windows, merging, expiry, holds, lockout, send order…) exact — the logic gate.
2. Box software: rolling recording, clips, games, highlights, court page, staff console, multi-box venue, updates.
3. Online service: store-and-forward ingest, take-home email, fleet console (health, switch, updates), cross-venue isolation.
4. Real product: persists across power cuts, starts empty, full management, settings, onboarding; ASVS L1 box / L2 online.

## Scope
**In:** everything in SRS FR-1…FR-57 / NFR-1…NFR-18. **Out (Won't):** live streaming, booking, player payments,
ball-following camera, PB Passport link, clubhouse TV, sound, non-pickleball highlights, shot captions, mirrored disks,
second factor on the venue console. **Not build's job:** deployment, releases, the legal launch gate (A-59).

## Budget and stakeholders
40 iterations (`Iterations: 40`); one maintainer; online budget US$25–100/month; hardware ≈ ₱9,700/court. Stakeholders:
owner (builder, seller, support), venue owners/staff, players, installers, the named privacy reviewer (launch gate).

## Risk register
| # | Risk | Mitigation |
|---|---|---|
| R1 | Shot-judge throughput on the N150 iGPU is unproven, and this dev machine is an AMD Ryzen laptop (no N150) | Owner chose "build now, N150 test later": proxy spike here; DEV-SPIKE stays a required **blocked** check until the owner runs the prepared script on the N150; a miss supersedes the stack decision |
| R2 | No hand-marked pickleball footage exists yet, so the shot judge cannot be trained or scored (NFR-17) | Ship an advisory baseline judge behind the same interface so the workflow is wired end to end; F-JUDGE-EVAL stays blocked until the owner films the partner-court games (A-90) |
| R3 | 110 required checks against 40 iterations | Slices sized by feature (one slice turns several rows green); order: logic gate → box core → console → online → hardening/ux; report BOUNDED honestly if the bound is reached |
| R4 | Intel VA hardware encoding cannot be exercised on this AMD/NVIDIA host or GitHub runners | Encoder chain va → openh264 (BSD, licence-safe); CI and dev use openh264; VA verified on the N150 by the spike script |
| R5 | retina reads fewer camera models than FFmpeg | Test against MediaMTX-served streams; keep GStreamer `rtspsrc` as the fallback path (stack decision) |
| R6 | Windows dev host vs Ubuntu 26.04 target | All box builds, tests and CI run in an Ubuntu 26.04 container; only the pure rules engine also builds natively |

## Feasibility verdict
*(Phase 2 — filled after the toolchain spike.)*

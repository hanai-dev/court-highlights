# Stack decision — Court camera highlights (court-highlights)

Status: owner-mandated
Date: 2026-10-21
Decision-makers: owner (client) · consulted: the owner's stated skills and constraints (A-113–A-121), Forge research (four candidate passes + one cross-cutting pass; ledgers in this directory), Forge build records (S-46, S-47) · informed: build
Recommendation: O-2
Decision: O-2
Confidence: moderate — knock-out, licence, advisory and support-date cells rest on T1/T2 registry, deps.dev and OSV data plus official documentation read in full; the three load-bearing performance drivers (RQ-1–RQ-3) rest on T3 capability documents and are settled only by the pre-registered spike on the actual box
Robustness: fragile — O-2 leads alone in all 26 ±1-weight cases, in the unweighted run (48 vs 47) and against the O-1 datum (weighted +5/−4); dropping RQ-4 (one-person upkeep in the owner's languages) hands the lead to O-1, 91 vs 89
Approval: owner-mandated (A-123, 2026-10-21) — fitness validated, risks recorded; client-review rev 17; owner answers "Decided (mandate)" (A-113) and "Intel first, Jetson as backup (Recommended)"; ledger:27b558b5dfd0f5e7

**Y-statement:** In the context of an offline court-camera box sold to Philippine pickleball venues (up to 12 courts, 3 boxes each), which records every court continuously, cuts branded clips in seconds, judges best shots within an hour and sends finished clips online when internet exists, maintained for three years by one owner who knows Rust, TypeScript and Go and who has decided on Rust, facing hardware-video and AI-throughput limits on a ₱10,000-per-court box under a keep-the-code-private licence rule, we decided for O-2 — Rust (tokio, axum, retina, GStreamer 1.28 via gstreamer-rs with Intel VA hardware encoding, ort with OpenVINO, SQLite, server-rendered askama pages) on an Intel N-series mini PC with Ubuntu 26.04 LTS, and a Rust online service on DigitalOcean with PostgreSQL 18, Cloudflare R2 and Amazon SES — and neglected O-1 (Python + FFmpeg, 97 vs 101), O-3 (Rust on Jetson Orin NX, eliminated: Ubuntu 24.04 base ends before the horizon, ₱25,729 per court) and O-4 (Frigate fork, eliminated: Debian 12 base ends 2028), to achieve one language end to end in the owner's own skills, memory-safe network services and a small self-contained deployable per box, accepting a release-candidate AI runtime binding (ort 2.0.0-rc), a pre-1.0 web framework, a narrower RTSP client than FFmpeg and an unproven shot-judge throughput on the N150's integrated GPU that the spike must confirm.

## Context and problem statement

Forces, by client-review item (revision 16):

- **What it does:** each court has one PoE camera; a box records its 4 courts continuously (rolling 4 h), cuts a 25 s + 5 s clip on a tap, burns in the venue logo, makes wide + vertical versions, records whole games, judges up to 5 best shots per started game within 60 min, serves the court pages and staff console to phones on the venue Wi-Fi with no internet, and sends finished clips + emails through an online service when the line is up (A-3, A-15, A-19, A-20, A-21, A-88, A-93, A-98, A-105).
- **Load:** up to 12 courts per venue in 3 boxes; each box must judge ≥ 4 h of footage per hour; ~10 venues in year 1 and ~50 in year 3; year-1 upper bound ≈ 550 GB/day sent, ≈ 1.65 TB held for 72 h, ≈ 144,000 emails/month (A-99, A-117).
- **Targets:** clip on the phone ≤ 10 s for 95 of 100 taps with 4 courts recording; no recording gap > 2 s per 24 h; reel ≤ 60 min (A-44, A-93).
- **Team and horizon:** owner maintains alone; knows Rust, TypeScript/Next.js, Go; Rust decided by the owner; web pages rendered by the Rust server; 3-year horizon, end used 2030-06-30 (A-113, A-115, A-116, A-121).
- **Money, hosting, policy:** online budget US$25–100/month; reuse DigitalOcean (Singapore) + Cloudflare; keep the box code private — no GPL/AGPL building blocks, LGPL only when kept separate; no sound recorded (A-37, A-114, A-118, A-119). Hardware priced in pesos by this review (A-6).
- **Weights** locked before scoring (A-120, drivers-draft.md): H = RQ-1…RQ-4 (3), M = RQ-5…RQ-11 (2), L = RQ-12, RQ-13 (1).

Reversibility: language/runtime, box OS + hardware family, media framework, AI runtime, box datastore (SQLite) and online datastore (PostgreSQL) are one-way doors and got the full protocol. Page templating, test runner, linter, formatter, email provider and object storage (S3-compatible API) are two-way doors (More information).

18F questions (owner-mandated path): *Why this stack?* The owner decided on Rust (A-113) and maintains alone in Rust/TS/Go (A-115). *Which stacks do the delivering and receiving teams know?* Delivering: Forge, with a released Rust 2024 + ort camera app (S-46) and several Django deliveries (S-47). Receiving: the owner (Rust, TypeScript/Next.js, Go). *Which alternatives were considered?* O-1, O-3, O-4 below.

## Knock-out gates

| gate | O-1 | O-2 | O-3 | O-4 | evidence |
|---|---|---|---|---|---|
| 1 licence inside A-114 | pass | pass | not assessed (fails gate 2) | pass (after swapping the shipped GPL FFmpeg for an LGPL build) | [S-08] [S-10] [S-11] [S-18] [S-24] [S-34] [S-35] |
| 2 supported past 2030-06-30 | pass | pass | fail | fail | [S-12] [S-13] [S-14] [S-15] [S-26] [S-34] [S-40] |
| 3 maintained, not archived | pass | pass | pass | pass | [S-17] |
| 4 zero unpatched reviewed advisories | pass | pass | pass | pass | [S-08] [S-09] [S-11] |
| 5 platform: offline Linux box serving phone browsers on the LAN | pass | pass | pass | pass | [S-23] [S-25] [S-32] |

O-3 fails gate 2: JetPack 7 for Orin is built on Ubuntu 24.04 LTS [S-26], whose support ends 2029-05-31 [S-13]; extending costs US$500 per box per year [S-31]. O-4 fails gate 2: Frigate's image is based on debian:12 [S-34], which ends 2028-06-30 [S-13]; Debian 13 ends exactly on the horizon date. Both stay in the matrix for comparison only.

## Decision drivers

| RQ | Refutable scenario (stimulus → response → threshold) | weight | traces |
|---|---|---|---|
| RQ-1 | A box ingests 4 camera streams (1080p H.264, ~4 Mbps) for 24 h while serving pages and cutting clips → no gap > 2 s per court, no re-encoding for recording | 3 (H) | A-44, A-17, A-36 |
| RQ-2 | A tap while 4 courts record and one game video encodes → clip with burned-in logo, wide + vertical, ready ≤ 10 s after its window for 95/100 taps, with a hardware encoder the licence rule allows | 3 (H) | A-44, A-15, A-19, A-20, A-114 |
| RQ-3 | 4 courts finish 1-hour games back to back → ≥ 4 h of footage judged per hour, every reel ≤ 60 min after Stop, RQ-1/RQ-2 still holding | 3 (H) | A-93, A-95, A-35, A-99 |
| RQ-4 | The owner alone diagnoses a field fault and ships a fix on box and online sides → ≤ 2 languages to patch, one deployable per side | 3 (H) | A-115, A-113, A-40 |
| RQ-5 | 12-court venue (3 boxes, 12 cameras) → box share + camera per court: 5 ≤ ₱8,000 · 4 ≤ ₱11,500 · 3 ≤ ₱15,000 · 2 ≤ ₱20,000 · 1 ≤ ₱25,000 · 0 above | 2 (M) | A-6, A-35, A-68 |
| RQ-6 | Every OS, runtime and AI-runtime component pinned today is patched on 2030-06-30 with ≤ 1 forced major upgrade | 2 (M) | A-116 |
| RQ-7 | Year-1 upper-bound usage model → online storage, downloads, email and hosting ≤ US$100/month | 2 (M) | A-117, A-118, A-105, A-80 |
| RQ-8 | An update fails its health check on one box → that box returns to the previous version by itself; boxes update one at a time | 2 (M) | A-47, A-81 |
| RQ-9 | Maintained, licence-compatible packages exist for RTSP ingest, segment recording, hardware encode + overlay + crop, inference, local web + SQLite, password hashing, TOTP, S3 upload, email | 2 (M) | A-36, A-19, A-20, A-87, A-104, A-114 |
| RQ-10 | Typical ONVIF/RTSP H.264 PoE cameras incl. drop-outs → recording resumes ≤ 5 s after the camera returns | 2 (M) | A-36, A-61 |
| RQ-11 | Zero unpatched reviewed advisories for pinned versions; player-facing services avoid memory-unsafe parsing of player input | 2 (M) | A-49, A-52, A-50 |
| RQ-12 | Forge has evidence it writes and tests this stack reliably | 1 (L) | A-40 |
| RQ-13 | The box platform can be swapped without rewriting the recorder | 1 (L) | A-35 |

Anchors (all drivers): 5 = threshold met with margin on evidence · 4 = met on evidence, small margin or one open detail · 3 = plausibly met, unproven (spike decides) · 2 = likely tight or partly missed · 1 = likely missed · 0 = impossible as pinned. ISO/IEC 25010: functional suitability RQ-9 · performance RQ-1–RQ-3 · compatibility RQ-10 · interaction capability N-A-because every option serves the same server-built pages to phone browsers · reliability RQ-1, RQ-8 · security RQ-11 · maintainability RQ-4, RQ-6 · portability RQ-13 · delivery RQ-5, RQ-7, RQ-12.

## Considered options

### O-1 Python 3.14 + FFmpeg 8.1 (LGPL, VA-API) + OpenVINO 2026 + FastAPI 0.153 + SQLite on an Intel N150 mini PC (Ubuntu 26.04 LTS); online FastAPI + PostgreSQL 18 + R2 + SES on DigitalOcean — the datum

### O-2 Rust (stable 1.99, edition 2024) + tokio 1 + axum 0.8 + retina 0.4 + GStreamer 1.28 via gstreamer-rs 0.25 (VA) + ort 2.0-rc (ONNX Runtime + OpenVINO) + rusqlite 0.40 + askama 0.16 on an Intel N150 mini PC (Ubuntu 26.04 LTS); online Rust axum + sqlx 0.9 + PostgreSQL 18 + R2 + SES on DigitalOcean

### O-3 The O-2 Rust core on an NVIDIA Jetson Orin NX 16GB (JetPack 7, NVIDIA hardware encoders, ort with TensorRT/CUDA); online as O-2

### O-4 Fork of Frigate 0.18 (MIT; Python + TypeScript; FFmpeg rebuilt as LGPL; OpenVINO detector) in Docker on an Intel N150 mini PC; online as O-1

## Comparison matrix

| driver | O-1 | O-2 | O-3 | O-4 |
|---|---|---|---|---|
| RQ-1 recording continuity (3) | 4 [S-33, S-16] | 4 [S-43, S-03] | 4 [S-43, S-03] | 5 [S-33, S-17] |
| RQ-2 clip turnaround (3) | 3 [S-24, S-35] | 3 [S-19, S-24, S-18] | 4 [S-27] | 3 [S-34, S-35] |
| RQ-3 shot-judge throughput (3) | 2 [S-23, S-21] | 2 [S-23, S-21] | 5 [S-25] | 2 [S-32, S-23] |
| RQ-4 one-person upkeep (3) | 2 [S-10] | 4 [S-02, S-04, S-07] | 3 [S-26, S-22] | 1 [S-17] |
| RQ-5 hardware cost per court (2) | 4 [S-28, S-29, S-30] | 4 [S-28, S-29, S-30] | 0 [S-27, S-29, S-30] | 4 [S-28, S-29, S-30] |
| RQ-6 support horizon (2) | 4 [S-12, S-13, S-14] | 3 [S-15, S-40, S-05, S-02] | 1 [S-26, S-13, S-31] | 2 [S-34, S-13] |
| RQ-7 online running cost (2) | 4 [S-36, S-37, S-38, S-39] | 4 [S-36, S-37, S-38, S-39] | 4 [S-36, S-37, S-38, S-39] | 4 [S-36, S-37, S-38, S-39] |
| RQ-8 safe fleet updates (2) | 3 [S-44, S-45] | 4 [S-45, S-44] | 3 [S-44, S-26] | 3 [S-34, S-44] |
| RQ-9 reuse coverage (2) | 4 [S-10, S-11] | 4 [S-01, S-07, S-08] | 4 [S-08, S-22] | 4 [S-32, S-33] |
| RQ-10 camera compatibility (2) | 4 [S-32, S-33] | 3 [S-43, S-19] | 3 [S-43, S-19] | 4 [S-32] |
| RQ-11 security posture (2) | 5 [S-09, S-11] | 5 [S-09, S-08] | 4 [S-09, S-26] | 3 [S-09, S-34] |
| RQ-12 harness delivery (1) | 4 [S-47, S-42] | 4 [S-46, S-42] | 3 [S-46, S-42] | 2 [S-47, S-17] |
| RQ-13 exit path (1) | 4 [S-21, S-10] | 4 [S-22, S-21] | 2 [S-26, S-22] | 4 [S-32, S-17] |
| weighted total (secondary view) | 97 | 101 | 91 | 87 |

Cell reasons are in evidence/O-1-notes.md … O-4-notes.md; cross-cutting figures (dates, prices, online cost) in evidence/X-notes.md.

## Sensitivity sweep

Computed by `stack/sweep.cjs` from the gated matrix above:

| sweep | result |
|---|---|
| base (locked weights) | O-2 101 · O-1 97 · O-3 91 · O-4 87 |
| each weight ±1 (26 cases) | O-2 leads alone in 26 / 26 |
| drop each driver once | O-2 leads in every case except dropping RQ-4: O-1 91 · O-2 89 · O-4 84 · O-3 82 |
| unweighted | O-2 48 · O-1 47 · O-4 41 · O-3 40 |
| Pugh, O-1 as datum | O-2 better on 2, worse on 2 (weighted +5/−4) · O-3 +3/−6 · O-4 +1/−4 |
| Pugh, O-2 as datum | O-1 better on 2, worse on 2 (weighted +4/−5) · O-3 +2/−7 · O-4 +2/−5 |

Verdict: **fragile** — O-2's lead over O-1 is 4 points and rests on RQ-4 (one language, the owner's own). If upkeep in the owner's languages did not matter, O-1 would lead by 2.

## Decision outcome

**O-2**, recommended by the research and decided by the owner (A-113). Justification: O-3 and O-4 are eliminated by the support-horizon gate. Between the survivors, O-2 and O-1 tie on recording, clip turnaround, shot-judge throughput, hardware cost, online cost, reuse and security. O-2 wins on one-person upkeep: one language end to end, which the owner already writes [S-02, S-07, S-08]. It also ships a single self-contained deployable per box, which suits the A/B update with rollback [S-45]. O-1 wins on support stability, because Python 3.14 and FFmpeg are mature, while ort is still a release candidate and axum is still 0.x [S-05, S-02]. O-1 also wins on camera breadth, because FFmpeg handles more cameras than retina [S-43].

What it still needs: the pre-registered spike below. The decisive unknown is RQ-3, shot-judge throughput on the N150's 24-EU integrated GPU. It is the same for O-1 and O-2, and only the spike can measure it. If the spike misses, the decision is reopened with the numbers: a stronger Intel box, an add-on accelerator, or the owner's named backup, a Jetson Orin NX (A-123), each re-priced in pesos. v1 targets one box type, Intel.

Implementation guards carried into the build:
- Record by stream copy with retina. Keep GStreamer `rtspsrc` as a fallback for cameras retina cannot handle [S-43, S-19].
- Do all re-encoding (logo, vertical crop, 720p game video) with GStreamer VA elements (vah264enc, vapostproc, vacompositor), linked dynamically and with no GPL plugins [S-18, S-19].
- Analyse each camera's lower-resolution sub-stream for the shot judge (A-95).
- Use no AGPL models, so Ultralytics YOLO is excluded [S-41]. The shot-judge model is trained on the partner-court footage (A-90, A-94) and exported to ONNX.

## Pros and cons of the options

**O-1 Python + FFmpeg (runner-up).**
- Good: the most mature video path. FFmpeg's camera handling is proven by Frigate [S-32, S-33].
- Good: every runtime is supported past 2030 [S-12, S-13, S-14].
- Good: Forge has delivered Python back ends [S-47].
- Bad: Python is outside the owner's stated languages (A-115).
- Bad: a Python app ships as many files plus a virtual environment, which makes A/B updates bulkier [S-44, S-45].
- Bad: FFmpeg must be built as LGPL without libx264 [S-35].
- Bad: it gives up the owner's Rust decision.

**O-3 Rust on Jetson Orin NX.**
- Good: ample AI (157 TOPS) [S-25] and hardware encoding (12 × 1080p30) [S-27]. This would settle RQ-3 outright.
- Bad: ₱25,729 per court [S-27, S-30].
- Bad: JetPack 7's Ubuntu 24.04 base ends May 2029 [S-26, S-13], with paid extension at US$500 per box per year [S-31].
- Bad: JetPack upgrades involve reflashing; forum reports of failed upgrades are T4 context.
- Bad: TensorRT and NVIDIA encoders tie the code to the platform [S-22].

**O-4 Frigate fork.**
- Good: recording, retention, a detection pipeline and a UI already exist and are used at scale [S-17, S-32, S-33].
- Bad: two large codebases, Python and TypeScript, to keep merging with an upstream that shipped three minor series in ten months [S-17].
- Bad: GPL FFmpeg ships in the image [S-34], and the image is based on Debian 12, which ends 2028 [S-34, S-13].
- Bad: court pages, email and the shot judge are new work either way.
- Bad: it gives up the Rust decision.

## Consequences

- Good: one language (Rust) on the box and online. Memory-safe, player-facing services. One binary plus templates per release.
- Good: Ubuntu 26.04 LTS and PostgreSQL 18 are supported past the 2030-06-30 horizon [S-13, S-14].
- Good: hardware is about ₱9,700 per court (box share plus camera, before switch and install) [S-28, S-29, S-30].
- Good: the online part is about US$68 a month at the year-1 upper bound, with downloads free on R2 [S-36].
- Bad: ort is a release candidate (rc.15) [S-05].
  - Mitigation: pin exact versions with `=`, as watchman does [S-46], and upgrade deliberately with the spike test suite.
  - Revisit trigger: ort 2.0.0 final, or a breaking rc.
- Bad: axum is pre-1.0 [S-02], so expect one breaking upgrade within the horizon.
  - Mitigation: thin HTTP layer, pinned versions.
- Bad: retina has "many missing features" [S-43], and specific cameras fail with it today.
  - Mitigation: certify the camera models you sell. Keep the GStreamer `rtspsrc` fallback path.
  - Revisit trigger: any installed camera that fails to reconnect within 5 s.
- Bad: debugging GStreamer pipelines needs GStreamer knowledge on top of Rust.
  - Mitigation: a small fixed set of pipelines, each with an automated test on recorded footage.
- Risk: shot-judge throughput on the N150 iGPU is unproven (RQ-3 = 2).
  - Mitigation: the spike measures it. Fallback ladder:
    1. Analyse at a lower frame rate or resolution.
    2. Step up to a larger Intel chip with more graphics or an NPU.
    3. Add an M.2 AI accelerator.
    4. The owner's named backup box (A-123): a Jetson Orin NX 16GB running the same Rust core (O-3). Accepted conditions if chosen: about ₱25,729 per court [S-27, S-30], and its Ubuntu 24.04 base ends 2029-05-31 [S-26, S-13], so each box needs a JetPack upgrade by reflash, or Ubuntu Pro at US$500 per box per year [S-31].
  - Each fallback is re-priced and replayed to the owner.
- Risk: the Intel encoder path may misbehave at odd resolutions or after driver updates (T4 reports).
  - Mitigation: pin the media driver version in the box image, and test with the exact crops (1080p wide, 1080×1920 vertical, 720p game).
- Risk: there is one maintainer (the owner) and retina is a small single-maintainer project [S-17].
  - Exit path for retina: GStreamer `rtspsrc` [S-19].
  - Exit path for the AI runtime: ONNX models move to another execution provider [S-21, S-22].
  - Exit path for the box: any x86-64 machine that runs Ubuntu 26.04 with VA-API (or a GStreamer software encoder that meets the licence rule).
  - Exit path for PostgreSQL: a standard SQL dump.
  - Exit path for R2: the S3-compatible API.
- Premortem: *"It is a year later and this stack failed us because"* the integrated GPU couldn't judge four courts in time, so every venue needed a pricier box. Or ort and axum upgrades kept breaking a codebase only one person understands. Or a popular camera brand dropped streams through retina. Each is covered by a spike threshold or a revisit trigger above.

## Failure-mode checks

- Résumé-driven? No. Rust is the owner's own decision and existing skill (A-113, A-115), and it leads the matrix on upkeep. The record shows where Python would win.
- Microservice / polyglot envy? No. There is one process per box and one service online, both in Rust. There is no Node.js (A-121).
- Innovation tokens: one spent, on **retina** (a young Rust RTSP client) instead of the boring FFmpeg ingest. It serves RQ-1 and RQ-4 (one language, no subprocess). The boring option, FFmpeg or GStreamer `rtspsrc`, stays as the fallback. ort's release-candidate status is a stability risk, not a novelty choice.
- Rewrite trap: the 3-year horizon matches LTS windows. There is no planned rewrite.
- LLM self-preference disclosed: the harness writes more training-data-rich stacks (Python, TypeScript) more fluently [S-42]. Its own Rust record is watchman [S-46]. The spike measures, rather than assumes, the build quality.

## Disconfirmation log

- **O-1:**
  - Searched "ffmpeg vaapi h264 encode Alder Lake-N N100 problems HuC". Found one Emby report on an Alder Lake-N NAS (the app failed to detect hardware encoding; manual `h264_vaapi` worked). Nothing HuC-specific.
  - FFmpeg legal page: avoid libx264, so the LGPL build loses software H.264 [S-35].
- **O-2:**
  - retina's tracker has 11 open camera issues, including V380 DESCRIBE EOF, Eufy losing RTP packets, and a planned 0.5 release with breaking changes.
  - ort has been in release candidates from rc.3 (2024-07) to rc.15 (2026-10).
  - Searched "GStreamer vah264enc vacompositor Intel N100 issues". No N100 report; artifact bugs at odd resolutions on Comet Lake; one Mesa regression thread.
- **O-3:**
  - Searched "Jetson Orin NX JetPack upgrade problems". Found NVIDIA forum reports of failed JetPack 6.x/7.x upgrades that needed reflashing.
  - JetPack 7 is on Ubuntu 24.04 [S-26], which ends 2029-05-31 [S-13].
- **O-4:**
  - Release history shows 0.16 → 0.17 → 0.18 in ten months.
  - install_deps.sh ships GPL FFmpeg builds, and the Dockerfile uses debian:12 [S-34].

## Confirmation

The pre-registered spike runs in build Phase 2 (Feasibility). Results go in `confirmation-results.md` beside this record. Thresholds are written before the run.

- **Hardware:** one Intel N150 mini PC (Beelink EQ14 class, 16 GB) with Ubuntu 26.04 LTS and a pinned Intel media driver. A second identical box runs O-1's FFmpeg path only if H2 or H3 fail, for comparison.
- **Load model:** 4 RTSP camera streams (1080p30 H.264, ~4 Mbps, plus a 640×360 sub-stream), replayed from recorded court footage by a local RTSP server. Footage from the partner court, or public test footage until it exists.
- **Concurrently:** taps arrive at random; one game video encodes at 720p; the shot judge runs.
- **Runs:** ≥ 5, reported as p50/p95.
- **Hypotheses and thresholds:**
  - **H1:** over 24 h, no recording gap > 2 s per stream.
  - **H2:** over ≥ 100 taps per run, the clip (logo, wide + vertical) is ready ≤ 10 s after its window ends at p95.
  - **H3:** a stand-in ONNX detector, licence-compatible and sized like the planned ball/player model, judges 4 × 1 h of footage in ≤ 60 min on the OpenVINO iGPU path while H1 and H2 hold.
  - **H4:** a camera unplugged for 30 s resumes recording ≤ 5 s after it returns, through retina and through the `rtspsrc` fallback.
  - **H5:** an injected failing update rolls back to the previous version by itself.
- **Lockfile check:** Cargo.lock contains retina, gstreamer, ort, axum, rusqlite and askama, and contains no ffmpeg, Python or Node.js runtime.
- **If any threshold misses:** this decision is superseded and replayed to the owner with the numbers. For H3 that means the fallback ladder in Consequences, ending with the owner's named Jetson Orin NX backup (A-123). If the Jetson rung is reached, the same H1–H5 run is repeated on a Jetson box before switching.

## More information

- Ledgers in this directory:
  - `sources.tsv`: 49 sources (T1 9, T2 5, T3 34, T4 1 context only).
  - `claims.tsv`: 24 claims.
  - `queries.tsv`.
  - `evidence/`: reading notes, captured page text and raw API JSON.
  - `drivers-draft.md`: the locked weights.
  - `sweep.cjs`: the sensitivity computation.
- **No good evidence exists for the following; the owner decides them on judgment:**
  - Shot-judge throughput of a ball-tracking model on a 24-EU integrated GPU (the spike will measure it).
  - How many games will actually have emails added, which sets real online cost below the upper bound.
  - retina's 3-year viability as a small project.
  - Which camera models Philippine installers will actually buy.
  - Whether the owner will enjoy debugging GStreamer.
- **Two-way-door Y-statements** (approved together as one decision, A-122):
  - Pages: askama 0.16 server-rendered templates, plus small vanilla JavaScript for the saving ring and vibration (A-108, A-121), instead of a front-end framework. This keeps the box offline-simple.
  - Tests: `cargo test`, plus Playwright for end-to-end runs of the court page and consoles, instead of other runners. Playwright is the owner's standard browser tool.
  - Lint and format: clippy and rustfmt, the Rust defaults.
  - Email: Amazon SES through aws-sdk-sesv2 [S-37], instead of Postmark or Resend. It's the cheapest per message at this volume, and the provider can be swapped behind one function.
  - Object storage: Cloudflare R2 through aws-sdk-s3 [S-36], instead of DigitalOcean Spaces. Downloads are free, and the S3 API makes it swappable.
  - Box update tool: systemd-sysupdate [S-45], with RAUC [S-44] as the alternative. The spike decides between them.

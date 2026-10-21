# Decision drivers — draft for locking (A-120)

Run `forge/requirements-261020-1145` · 2026-10-20. Each driver is a refutable scenario with a fixed 0–5 anchor
(5 = threshold met with margin on evidence; 3 = plausibly met, unproven — spike decides; 1 = likely missed;
0 = impossible as pinned). Weights H/M/L → 3/2/1 are locked by the owner **before** scoring.

| RQ | Refutable scenario (stimulus → response → threshold) | Weight | Traces |
|---|---|---|---|
| RQ-1 | Recording continuity: a box ingests 4 camera streams (1080p H.264, ~4 Mbps each) for 24 h while serving pages and cutting clips → no gap longer than 2 s per court, recording without re-encoding. | H (3) | A-44, A-17, A-36 |
| RQ-2 | Clip turnaround: a tap while 4 courts record and one game video encodes → clip with burned-in logo, wide + vertical, ready ≤ 10 s after its window ends for 95 of 100 taps, using a hardware video encoder that the licence rule allows. | H (3) | A-44, A-15, A-19, A-20, A-114 |
| RQ-3 | Shot-judge throughput: 4 courts finish 1-hour games back to back → the box judges ≥ 4 h of footage per hour and every reel is ready ≤ 60 min after Stop, while RQ-1 and RQ-2 still hold. | H (3) | A-93, A-95, A-35, A-99 |
| RQ-4 | One-person upkeep: the owner alone (Rust, TypeScript/Next.js, Go) diagnoses a field fault and ships a fix for box and online parts → ≤ 2 languages/runtimes to patch, one deployable per side. | H (3) | A-115, A-113, A-40 |
| RQ-5 | Hardware cost: a 12-court venue (3 boxes, 12 cameras) → box share + camera per court; anchor 5 = ≤ ₱8,000 per court, 3 = ≤ ₱15,000, 1 = ≤ ₱25,000. | M (2) | A-6, A-35, A-68 |
| RQ-6 | Support horizon: every OS, runtime and AI-runtime component pinned today is still patched on 2030-06-30 with at most one forced major upgrade. | M (2) | A-116 |
| RQ-7 | Online running cost: the year-1 upper-bound usage model (A-117) → ≤ US$100/month for storage, downloads, email and hosting. | M (2) | A-117, A-118, A-105, A-80 |
| RQ-8 | Safe fleet updates: an update fails its health check on one box → that box returns to the previous version by itself; boxes update one at a time. | M (2) | A-47, A-81 |
| RQ-9 | Reuse coverage: maintained, licence-compatible packages exist for RTSP ingest, segment recording, hardware encode + overlay + crop, model inference, local web + SQLite, password hashing, TOTP, S3 upload and email. | M (2) | A-36, A-19, A-20, A-87, A-104, A-114 |
| RQ-10 | Camera compatibility: typical ONVIF/RTSP H.264 PoE cameras, including drop-outs and reconnects → recording resumes ≤ 5 s after the camera returns. | M (2) | A-36, A-61 |
| RQ-11 | Security posture: zero unpatched reviewed advisories for the pinned versions; the network-facing box services avoid memory-unsafe parsing of player input. | M (2) | A-49, A-52, A-50 |
| RQ-12 | Harness delivery: Forge has evidence it writes and tests this stack reliably (prior builds, AI-proficiency evidence). | L (1) | A-40 |
| RQ-13 | Exit path: the box platform can be swapped (other mini PC or AI board) without rewriting the recorder. | L (1) | A-35 |

ISO/IEC 25010 coverage: functional suitability RQ-9 · performance efficiency RQ-1–RQ-3 · compatibility RQ-10 ·
interaction capability **N-A-because** every candidate serves the same server-built web pages to phone browsers ·
reliability RQ-1, RQ-8 · security RQ-11 · maintainability RQ-4, RQ-6 · flexibility/portability RQ-13 · delivery
RQ-5, RQ-7, RQ-12.

Knock-out gates (before scoring): licence (A-114) · EOL after 2030-06-30 (A-116) · maintained, not archived ·
zero unpatched reviewed advisories · platform: Linux box that runs offline, serves phone browsers on the LAN.

## Candidate stacks (majors pinned in the record)

- **O-1 datum — Python + FFmpeg on an Intel mini PC:** Python 3.14, FFmpeg 8 command-line (LGPL build, VA-API),
  OpenVINO 2026, FastAPI, SQLite; online FastAPI + PostgreSQL 18 + Cloudflare R2 + Amazon SES on DigitalOcean.
- **O-2 — Rust on an Intel mini PC:** Rust (2024 edition), tokio 1 + axum 0.8, retina (RTSP), GStreamer 1.26 via
  gstreamer-rs (VA encoders, LGPL dynamic), ort 2 (ONNX Runtime + OpenVINO), SQLite; online Rust axum + PostgreSQL 18
  + R2 + SES on DigitalOcean.
- **O-3 — Rust on an NVIDIA Jetson Orin NX:** the O-2 Rust core with NVIDIA's hardware encoders and TensorRT on
  JetPack.
- **O-4 — Fork an open-source camera recorder (Frigate, Python):** reuse its recording, retention and detection;
  add court pages, emails and the shot judge.

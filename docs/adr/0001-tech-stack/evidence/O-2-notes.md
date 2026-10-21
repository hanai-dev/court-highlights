# O-2 — Rust on an Intel N-series mini PC

accessed: 2026-10-20

Pinned: Rust stable (1.99, edition 2024) · tokio 1.54 · axum 0.8.9 · tower-http 0.7 · retina 0.4.21 (RTSP) ·
GStreamer 1.28 via gstreamer-rs 0.25 (va plugin: vah264enc, vapostproc, vacompositor; LGPL, dynamically linked) ·
ort 2.0.0-rc.15 (ONNX Runtime + OpenVINO EP) · rusqlite 0.40 (SQLite) · askama 0.16 (server-rendered pages, A-121) ·
argon2 0.6 · Ubuntu 26.04 LTS · Intel N150. Online: Rust axum + sqlx 0.9 + PostgreSQL 18 + Cloudflare R2
(aws-sdk-s3 1.152) + Amazon SES (aws-sdk-sesv2 1.140) + totp-rs 6 on DigitalOcean Singapore.

## What was read
- crates.io records for every pinned crate (S-01 … S-07), deps.dev licences + advisoryKeys (S-08), OSV 0 (S-09).
- retina README (S-43): good support for ONVIF RTSP/1.0 cameras, works around broken cheap cameras, production use in
  Moonfire NVR, "many missing features"; mp4 example writes RTSP to .mp4.
- GStreamer licensing FAQ (S-18): LGPL; apps may keep their own licence. va plugin reference (S-19): hardware H.264
  encoder, post-processor (scale/crop/colour) and compositor (overlay). Releases (S-20): 1.28.8 current stable.
- ort docs (S-22): rc.14 called production-ready; execution providers for CUDA, TensorRT, OpenVINO, QNN.
  OpenVINO EP docs (S-21): iGPU/NPU/CPU targets.
- Intel media driver (S-24) and N150 ARK (S-23) as in O-1.
- Rust stability promise (S-40); endoflife.date rust (S-15); ubuntu (S-13); postgresql (S-14).
- Forge-built Jss-on/watchman (S-46): Rust 2024 + ort rc.12 + rusqlite + argon2, released.

## Scores and why
- RQ-1 recording 4: retina writes camera H.264 to MP4 and runs in production in Moonfire NVR (S-43, S-03).
- RQ-2 clip turnaround 3: vah264enc + vapostproc + vacompositor do encode, crop/scale and logo overlay in hardware
  (S-19) on ADL-N (S-24); unmeasured → spike.
- RQ-3 shot judge 2: same iGPU limit as O-1 (S-23, S-21) → spike; mitigation as O-1.
- RQ-4 upkeep 4: one language on both sides, the owner's own (A-115) (S-02, S-04); GStreamer pipelines still need
  GStreamer knowledge to debug.
- RQ-5 hardware 4: ₱9,733 per court (S-28, S-29, S-30).
- RQ-6 horizon 3: Rust stable + Ubuntu 26.04 + PostgreSQL 18 pass (S-15, S-40, S-13, S-14), but ort is still a
  release candidate (S-05) and axum is a 0.x series (S-02) → expect API breaks within the horizon.
- RQ-7 online 4: ≈ US$68/month (S-36, S-37, S-38, S-39).
- RQ-8 updates 4: one self-contained binary + templates per release suits systemd-sysupdate versioned transfers (S-45)
  or RAUC A/B (S-44).
- RQ-9 reuse 4: maintained, permissive crates exist for each solved problem (S-01 … S-08).
- RQ-10 cameras 3: retina's support is ONVIF-focused with "many missing features" (S-43); open reports of specific
  cameras failing (see disconfirmation) → keep GStreamer `rtspsrc` as a fallback (S-19, S-04).
- RQ-11 security 5: 0 advisories (S-09); memory-safe Rust for the player-facing server (S-08).
- RQ-12 harness 4: watchman shows Forge shipping this exact Rust + ort + rusqlite combination (S-46); S-42 directional.
- RQ-13 exit 4: ort execution providers cover other boards (S-22, S-21).

## Disconfirmation
- GitHub issue search `repo:scottlamb/retina is:open camera` → 11 open; includes #84 "V380 fails with EOF while
  expecting response to DESCRIBE", #119 "Eufy … Lost RTP packets mid-stream", #151 "0.5.x — breaking changes".
- ort releases: 2.0.0-rc.3 (2024-07-06) → rc.15 (2026-10-19): two years of release candidates (12 rc tags seen).
- Searched "GStreamer vah264enc vacompositor Intel N100 issues" → no N100-specific report; GStreamer GitLab #1238
  greenish artifacts at an unusual resolution (Comet Lake), a Mesa-update regression thread, "vah264dec + videoconvert
  is slow" (use vapostproc). T4/context → spike uses 1080p and the planned crops.

# Cross-cutting reading notes (all options)

accessed: 2026-10-20 · raw JSON in `raw/`, rendered page text in `docs/`, sweep summary in `registry-summary.md`.

## Support dates (RQ-6, knock-out gate 2) — S-12, S-13, S-14, S-15, S-16, S-26, S-34
- Horizon end used: 2030-06-30 (A-116).
- Ubuntu 26.04 LTS: standard support to 2031-05-29 → passes. Ubuntu 24.04 LTS: 2029-05-31 → fails (S-13).
- Debian 12: EOL 2028-06-30 → fails; Debian 13: EOL 2030-06-30 → exactly the horizon end, zero margin (S-13).
- PostgreSQL 18: EOL 2030-11-14 → passes; 17: 2029-11-08 → fails (S-14).
- Python 3.14: EOL 2030-10-31 → passes; 3.13: 2029-10-31 → fails (S-12).
- Rust: six-weekly stable releases, each superseded by the next (S-15); the stability promise says code that compiles on a stable release keeps compiling on later stable releases (S-40) — upgrades are routine, not forced majors.
- JetPack 7 (Orin, Thor) is built on Ubuntu 24.04 LTS (S-26) → its base ends 2029-05-31; extending needs Ubuntu Pro ESM at US$500 per server machine per year (S-31).
- Frigate's Dockerfile pins `BASE_IMAGE=debian:12` (S-34) → base ends 2028-06-30.

## Advisories and licences (RQ-11, knock-out gates 1 and 4) — S-08, S-09, S-10, S-11, S-17, S-18, S-24, S-35, S-41
- OSV querybatch: 0 vulnerabilities for every pinned crate (tokio 1.54.0, axum 0.8.9, retina 0.4.21, gstreamer 0.25.4, gstreamer-video 0.25.4, ort 2.0.0-rc.15, rusqlite 0.40.2, askama 0.16.1, argon2 0.6.0, totp-rs 6.0.0, aws-sdk-s3 1.152.0, aws-sdk-sesv2 1.140.1, sqlx 0.9.0, tower-http 0.7.1) and PyPI package (fastapi 0.153.0, uvicorn 0.54.0, openvino 2026.4.1, onnxruntime 1.31.0, peewee 4.5.3) (S-09). deps.dev advisoryKeys = 0 for each (S-08, S-11).
- Licences: all pinned crates MIT, Apache-2.0 or MIT OR Apache-2.0 (S-08); fastapi MIT, uvicorn BSD-3-Clause, openvino Apache-2.0, onnxruntime MIT (S-10, S-11).
- GStreamer core and plugins are LGPL; applications may use any licence (S-18). Intel media driver: MIT with BSD-3-Clause portions (S-24).
- FFmpeg is LGPL-2.1+ unless built with `--enable-gpl`; the LGPL checklist says avoid libx264 and link dynamically (S-35).
- Ultralytics YOLO is AGPL-3.0 unless an Enterprise licence is bought (S-41) → excluded by A-114 for every option.

## Hardware prices at BSP 63.10 PHP/USD (RQ-5) — S-27, S-28, S-29, S-30
- Intel box: Beelink EQ14 (N150) US$385 (S-28). Camera: Reolink RLC-510A US$57.99 (S-29).
  Per court on a 12-court venue (3 boxes × 4 courts): 385 / 4 + 57.99 = US$154.24 → **₱9,733** (S-30 rate 63.10).
- Jetson box: reComputer J4012 (Orin NX 16GB) US$1,399 (S-27). Per court: 1,399 / 4 + 57.99 = US$407.74 → **₱25,729**.
- Not counted (same for all): PoE switch, cabling, mounting, install labour, any added AI accelerator.

## Online running cost, year-1 upper bound (RQ-7) — S-36, S-37, S-38, S-39
Same online design for every option (DigitalOcean Singapore + Cloudflare R2 + Amazon SES, A-119).
- R2 storage: ≈ 1,650 GB held (550 GB/day × 3 days) × US$0.015 = ≈ US$24.60 after the 10 GB free tier; egress free (S-36).
  Class A writes ≈ 180 per court per day (≈ 120 objects + multipart parts) × 100 courts × 30 ≈ 0.54 M × US$4.50/M ≈
  US$2.43; Class B reads ≈ 0.2 M × US$0.36/M ≈ US$0.07.
- SES: 144,000 emails × US$0.10 / 1,000 = US$14.40 (S-37).
- Droplet 2 GiB US$12 (S-38) + managed PostgreSQL 1 GiB US$15.15 (S-39).
- **Total ≈ US$68/month** at the upper bound, ≈ US$45 at the expected one-third load → within US$100 (A-118).
- Counter-check: DigitalOcean Spaces would charge for downloads; players downloading 0.7 GB game videos make egress the
  dominant cost — the reason R2 (zero egress) is in every option.

## Harness delivery record (RQ-12) — S-46, S-47, S-42
- Jss-on/watchman v0.2.1 (Forge-built; DESIGN.md, evals, qa, spike folders): Rust 2024 edition, `ort =2.0.0-rc.12`
  with `load-dynamic`, rusqlite 0.37, argon2 0.5; "fully local camera monitor"; released binaries; GPU path
  tested on Windows, Linux build CPU-only ("the GPU path is untested on Linux") (S-46).
- hanai-dev/school-insight-v2 (Django API + React PWA) and hanai-dev/npo-accounting-v2 (Django, live at
  accounting.hanai.dev): Forge has delivered Python web back ends (S-47).
- LLM code quality varies by up to 84 % across functionally similar libraries (S-42, abstract only) → the spike, not
  the model's preference, settles proficiency.

## Context only (not scored)
- Jetson Orin Nano has no hardware video encoder (reseller copies of the NVIDIA spec sheet + JetsonHacks, T4, S-48) —
  why the cheaper Orin Nano is not a candidate.
- RallyCam uses "an ordinary PoE security camera" and "a small computer in a cupboard" (S-49) — the same hardware class.

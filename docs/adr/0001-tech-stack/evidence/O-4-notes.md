# O-4 — Fork Frigate (open-source camera recorder, Python) on an Intel N-series mini PC

accessed: 2026-10-20

Pinned: Frigate 0.18.2 fork (MIT) · its Python back end + TypeScript web UI · FFmpeg (replace the shipped GPL builds
with an LGPL build) · OpenVINO detector on the iGPU · Docker on Ubuntu 26.04 LTS host · Intel N150. Online part as O-1.

## What was read
- GitHub metadata + languages (S-17): MIT, active (last push 2026-10-20), Python ≈ 5.2 MB + TypeScript ≈ 4.9 MB.
- Frigate hardware docs (S-32): N100-class mini PCs recommended; OpenVINO on Intel iGPU/NPU; Hailo-8/8L; Jetson only
  community-supported on JetPack 6; WiFi cameras not recommended.
- Frigate recording docs (S-33): FFmpeg segments, retention modes, exports outside retention.
- Frigate install_deps.sh + Dockerfile (S-34): ships FFmpeg `linux64-gpl` builds 5.1/7.0/8.1; `BASE_IMAGE=debian:12`.
- FFmpeg legal (S-35); endoflife.date debian (S-13): 12 ends 2028-06-30, 13 ends 2030-06-30.

## Scores and why
- RQ-1 recording 5: production NVR built for continuous segment recording and retention (S-33, S-17).
- RQ-2 clip turnaround 3: exports are stream copies; logo burn-in and the vertical cut must be added through an LGPL
  FFmpeg with VA-API (S-34, S-35).
- RQ-3 shot judge 2: its detectors run object detection per frame; a ball-tracking shot judge is new work on the same
  iGPU (S-32, S-23).
- RQ-4 upkeep 1: Python + TypeScript code bases (S-17) plus a running merge with an upstream that ships a new minor
  series every few months (0.16 → 0.18 within a year, disconfirmation).
- RQ-5 hardware 4: ₱9,733 per court (S-28, S-29, S-30).
- RQ-6 horizon 2: Debian 12 base ends 2028-06-30 (S-34, S-13); staying current means following upstream rebases.
- RQ-7 online 4: same online part (S-36, S-37, S-38, S-39).
- RQ-8 updates 3: container image tags give a rollback path, but the host still needs its own update path (S-34, S-44).
- RQ-9 reuse 4: recording, retention, detection and a UI come built (S-32, S-33); court pages, email and the shot judge
  are new.
- RQ-10 cameras 4: FFmpeg-based ingest, broad camera use (S-32).
- RQ-11 security 3: 0 advisories in the PyPI parts (S-09), but a large inherited surface (two code bases, bundled
  third-party binaries) (S-17, S-34).
- RQ-12 harness 2: no Forge record of maintaining a fork of a large upstream (S-47 shows greenfield Python only); S-42.
- RQ-13 exit 4: many supported detectors (S-32).

## Disconfirmation
- Release history (GitHub API): v0.16.3 2025-12-06 → v0.17.0 2026-02-27 → v0.18.0 2026-09-12 → v0.18.2 2026-10-15 —
  three minor series in ten months to merge into a fork.
- Licence check: shipped FFmpeg builds are GPL (S-34) — the fork must rebuild FFmpeg as LGPL to meet A-114.

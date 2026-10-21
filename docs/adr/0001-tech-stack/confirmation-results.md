# Confirmation results — stack decision O-2

Status: **pre-registered spike NOT RUN** — the pre-registered hardware (Intel N150 mini PC, Ubuntu 26.04) is not available
to the build. Owner decision 2026-10-21 ("Build now, N150 test later"): the build proceeds on the developer laptop; the
spike stays a required, blocked acceptance check (`DEV-SPIKE`) until the owner runs `tools/spike/n150-spike.sh` on the box.
This file records a **proxy** run only; it neither confirms nor supersedes the decision.

## Proxy run — 2026-10-21, developer laptop

Hardware: AMD Ryzen 7 4800H (8C/16T), 23 GB RAM, Docker Desktop (WSL2) — measured container limited to `--cpus=4`;
fake cameras in a separate container (MediaMTX v1.21.2 + four FFmpeg testsrc2 1080p30 H.264 ≈ 4 Mbps streams); other,
unrelated containers were running on the same VM. Software: Ubuntu 26.04, GStreamer 1.28.2, OpenH264 2.6.0.
Script: `tools/spike/proxy-spike.sh` · raw output: run `forge/build-261021-0243/evidence/proxy-spike.txt`.

| Probe | Pre-registered hypothesis | Proxy result | Reading |
|---|---|---|---|
| P1 recording | H1: no gap > 2 s per stream over 24 h | 4 × 40 s stream-copy recordings written (9.0–9.5 MB each); 24 h soak not run | Stream copy works with `rtspsrc → rtph264depay → h264parse → mp4mux`; continuity unmeasured |
| P2 clip encode | H2: clip with logo, wide + vertical, ready ≤ 10 s after its window (p95) | Software OpenH264 on 4 cores, ~40 s of 1080p input, 5 runs: wide p50 **75.7 s** (61.2–88.2), vertical p50 **82.2 s** (74.6–84.8) → ≈ **118 s** for a 30 s clip in both shapes | Software encoding misses H2 by ≈ 12×. **H2 depends entirely on the N150's VA hardware encoder** (`vah264enc`/`vapostproc`), which this AMD host cannot exercise |
| P3 shot judge | H3: 4 × 1 h judged ≤ 60 min on the OpenVINO iGPU path | Not run (no model yet; no Intel iGPU here) | — |
| P4 camera resume | H4: resume ≤ 5 s after a 30 s unplug | Not run | — |
| P5 update rollback | H5: injected failing update rolls back | Not run | — |

## What the proxy changes

Nothing in the decision: the pre-registered hardware was not used. It sharpens two facts for the N150 run:

1. There is **no software fallback** for NFR-1 — if `vah264enc` is unavailable or slow on the N150, the clip-speed
   target fails and the decision must be replayed to the owner (stronger Intel chip or the Jetson backup, A-123).
2. CI and the developer laptop can verify clip **correctness** (window, logo, shapes) with OpenH264, never clip **speed**;
   `MON-CLIP-SPEED`, `MON-JUDGE-THROUGHPUT` and `DEV-SPIKE` stay blocked until the N150 run.

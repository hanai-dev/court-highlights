# O-3 — Rust on an NVIDIA Jetson Orin NX 16GB

accessed: 2026-10-20

Pinned: the O-2 Rust core · GStreamer with NVIDIA hardware encoders on JetPack 7 · ort 2.0.0-rc.15 with TensorRT/CUDA
execution providers · Seeed reComputer J4012 (Orin NX 16GB). Online part as O-2.

## What was read
- NVIDIA Jetson Orin family page (S-25): Orin NX 16GB 157 TOPS; Orin Nano Super 67 TOPS.
- NVIDIA JetPack page (S-26): JetPack 7 supports Orin and Thor; built on Linux 6.8 and Ubuntu 24.04 LTS.
- Seeed reComputer J4012 (S-27): US$1,399; video encode up to 12 × 1080p30 H.265.
- endoflife.date ubuntu (S-13): 24.04 LTS ends 2029-05-31. Ubuntu Pro pricing (S-31): US$500 per server per year.
- ort docs (S-22): TensorRT and CUDA execution providers.
- watchman (S-46): Forge's GPU path tested on Windows/NVIDIA only; Linux GPU untested.

## Scores and why
- RQ-1 recording 4: same retina core as O-2 (S-43, S-03).
- RQ-2 clip turnaround 4: hardware encoder capacity 12 × 1080p30 is far above 4 courts (S-27).
- RQ-3 shot judge 5: 157 TOPS (S-25) is ample for 4 courts at ≥ 4× real time.
- RQ-4 upkeep 3: one language, but JetPack/CUDA/TensorRT platform knowledge on top (S-26, S-22).
- RQ-5 hardware 0: ₱25,729 per court (S-27, S-29, S-30) — above the ₱25,000 anchor.
- RQ-6 horizon 1: JetPack 7's Ubuntu 24.04 base ends 2029-05-31, before 2030-06-30 (S-26, S-13); extending costs
  US$500 per box per year (S-31).
- RQ-7 online 4: same online part (S-36, S-37, S-38, S-39).
- RQ-8 updates 3: A/B is possible (S-44) but JetPack upgrades also flash bootloader/firmware (see disconfirmation).
- RQ-9 reuse 4: same crates (S-08) plus TensorRT EP (S-22).
- RQ-10 cameras 3: same as O-2 (S-43).
- RQ-11 security 4: 0 advisories in the crates (S-09); the closed CUDA/TensorRT/JetPack stack is outside OSV's view
  (S-26).
- RQ-12 harness 3: watchman's NVIDIA path is Windows-only (S-46); S-42 directional.
- RQ-13 exit 2: TensorRT engines and NVIDIA encoders are platform-specific (S-26, S-22).

## Disconfirmation
- Searched "Jetson Orin NX JetPack upgrade problems production fleet JetPack 6 to 7 migration" → NVIDIA forum threads:
  JetPack 7.x flash crash on Orin Nano; JetPack 7.2 upgrade failure recovered by reflashing 6.1.1; Orin NX 16G can't boot
  after update to JetPack 6.1; upgrade 6.0 → 6.1 fails on Orin NX. Anecdotal (T4) but consistent: version upgrades on
  Jetson involve reflashing, which is hard to do remotely across a fleet.

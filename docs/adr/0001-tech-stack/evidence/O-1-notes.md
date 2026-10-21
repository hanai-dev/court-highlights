# O-1 — Python + FFmpeg on an Intel N-series mini PC (datum)

accessed: 2026-10-20

Pinned: Python 3.14 · FFmpeg 8.1 command-line, LGPL build (no `--enable-gpl`), VA-API · OpenVINO 2026.4 (Python) ·
FastAPI 0.153 + uvicorn 0.54 · SQLite 3 · Ubuntu 26.04 LTS · Intel N150 (Alder Lake-N family). Online: FastAPI +
PostgreSQL 18 + Cloudflare R2 + Amazon SES on DigitalOcean Singapore.

## What was read
- PyPI JSON for fastapi, uvicorn, openvino, onnxruntime (S-10) and deps.dev pypi records (S-11): licences MIT /
  BSD-3-Clause / Apache-2.0 / MIT; 0 advisories (S-09).
- endoflife.date python: 3.14 EOL 2030-10-31 (S-12); ubuntu 26.04 EOL 2031-05-29 (S-13); postgresql 18 EOL 2030-11-14
  (S-14); ffmpeg 7.1 LTS and current 8.x/9.0 lines (S-16).
- FFmpeg legal (S-35): LGPL unless GPL parts are enabled; avoid libx264 → hardware H.264 through VA-API instead.
- Intel media driver (S-24): AVC hardware encode on ADL-N; HuC low-power encode from kernel 6.2 (Ubuntu 26.04 ships a
  newer kernel). Intel ARK N150 (S-23): 24 EU iGPU, 6 W, single memory channel.
- OpenVINO Execution Provider docs (S-21): integrated GPU, CPU and NPU targets, FP16.
- Frigate recording docs (S-33): continuous FFmpeg segment recording with retention is a proven production pattern.

## Scores and why
- RQ-1 recording 4: FFmpeg stream-copy segment recording is the pattern Frigate runs in production (S-33); FFmpeg
  maintained (S-16).
- RQ-2 clip turnaround 3: VA-API H.264 encode exists on ADL-N (S-24) and the LGPL build must skip libx264 (S-35);
  overlay/crop/scale through VA-API filters is plausible, unmeasured → spike.
- RQ-3 shot judge 2: OpenVINO can use the iGPU (S-21) but the N150 iGPU is 24 EU on one memory channel (S-23); four
  courts at ≥ 4× real time is likely tight → spike; mitigation = add-on accelerator or a larger Intel chip.
- RQ-4 upkeep 2: Python is not in the owner's stated languages (A-115); one language per side but an unfamiliar one
  (S-10).
- RQ-5 hardware 4: ₱9,733 per court (X-notes; S-28, S-29, S-30).
- RQ-6 horizon 4: Python 3.14, Ubuntu 26.04, PostgreSQL 18 all pass 2030-06-30 (S-12, S-13, S-14); OpenVINO ships
  yearly majors (S-10).
- RQ-7 online 4: ≈ US$68/month upper bound (X-notes; S-36, S-37, S-38, S-39).
- RQ-8 updates 3: RAUC A/B with boot confirmation and bootloader fallback (S-44) or systemd-sysupdate versioned
  transfers (S-45); a Python app is many files plus a virtual environment to ship.
- RQ-9 reuse 4: FastAPI, OpenVINO, onnxruntime, FFmpeg cover the solved problems (S-10, S-11).
- RQ-10 cameras 4: FFmpeg is the RTSP client Frigate relies on across many camera brands (S-32, S-33).
- RQ-11 security 5: 0 advisories (S-09); memory-safe language for the player-facing web server (S-10).
- RQ-12 harness 4: Forge delivered Django back ends (S-47); proficiency evidence is directional (S-42).
- RQ-13 exit 4: ONNX/OpenVINO models move to other execution providers (S-21).

## Disconfirmation
- Searched "ffmpeg vaapi h264 encode Alder Lake-N N100 problems HuC" → one Emby forum report: hardware H.264 not
  registering in Emby on an ADL-N NAS while a manual `ffmpeg -c:v h264_vaapi` test worked (application detection,
  not the driver); nothing HuC-specific. T4, context only → keep the spike's encode test.

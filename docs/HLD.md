# High-level and low-level design — Court Highlights

Stack: `docs/adr/0001-tech-stack/stack-decision.md` (O-2, owner-mandated, research agrees) — cited, not re-argued.
Requirements: `docs/requirements/requirements.md` (FR-1…FR-57, NFR-1…NFR-18). UI: `DESIGN.md`.

## 1. System context

```mermaid
flowchart LR
  subgraph Venue LAN and Wi-Fi (no internet)
    CAM[PoE cameras, RTSP H.264] --> BOX1[Court box A: ch-box]
    CAM --> BOX2[Court box B]
    BOX1 <-->|venue key, TLS| BOX2
    PHONE[Player phones] -->|HTTP :80 court pages| BOX1
    STAFF[Staff laptop/phone] -->|HTTPS :443 own certificate| BOX1
  end
```

Two crates in one workspace, nothing else: `ch-core` (pure rules, the golden vectors) and `ch-box` (one binary per
court box). Nothing leaves the venue network (change record, 2026-10-22). Pages are server-rendered by the box (A-121).

## 2. Court box (`ch-box`)

| Module | Responsibility | Requirements |
|---|---|---|
| `db` | SQLite (rusqlite, WAL, bundled) in `/var/lib/court-highlights/box.db`: venue state, courts, cameras, games, clips, holds, staff, sessions, log | FR-56, FR-35 |
| `recorder` | Per camera: **retina** session (TCP interleaved, no audio stream set up) → H.264 access units → GStreamer `appsrc ! h264parse ! splitmuxsink` (10 s MP4 segments, stream copy) named by wall-clock start; reconnect loop with backoff ≤ 1 s; offline/online events to `health`. Fallback reader: GStreamer `rtspsrc` per camera (`reader = "gstreamer"`) | FR-1, FR-2, FR-3, NFR-2 |
| `cutter` | Clip/game/highlight jobs: `splitmuxsrc` over the window's segments → accurate seek → decode once → `tee` → wide (logo overlay) and vertical (centre crop 1080×1920 + logo) → H.264 encode (`vah264enc` when present, else `openh264enc`) → MP4 | FR-4…FR-7, FR-12, FR-14 |
| `judge` | After Stop, for pickleball games: decode the game at low rate/resolution, find points (motion segmentation), rank, pick ≤ 5 via `ch-core::highlight_span`; ONNX model via **ort** when a trained model file is installed (`judge.onnx`), else the motion baseline (labelled advisory). Never deletes anything; venue switch | FR-14…FR-16, NFR-17 |
| `web` | axum: player listener `:80` (court pages, downloads, saving ring) and staff listener `:443` (rustls, self-signed certificate generated at first boot, fingerprint shown in Help) | FR-17…FR-21, FR-28…FR-41 |
| `auth` | argon2 password hashes, `__Host-` cookie sessions (Secure, HttpOnly, SameSite=Strict), 12 h absolute, lockout via `ch-core::Lockout`, Origin check on every state change | FR-28, FR-29 |
| `sync` | Venue state (settings, staff, courts) replicated between sibling boxes: each change bumps a version; boxes push on write and pull every 30 s over TLS, authenticated with an HMAC of the venue key | FR-41…FR-43 |
| `janitor` | 03:00 clean-up (`expires_at`, `keep_expires_at`), rolling footage (`rolling_deletable`), disk pressure (`disk_plan`) | FR-1, FR-45, FR-46 |
| `update` | Applies a signed update file from a USB stick through the launcher's A/B slots; health check after boot; rollback | FR-54 |

### Data model (SQLite, per box; venue-wide rows replicated by `sync`)

`venue(id, name, logo_png, logo_corner, sponsor_png, open_time, close_time, clip_before_s, clip_after_s,
judge_on, privacy_notice, venue_key_hash, version)` · `court(id, name, code, position, box_id)` ·
`camera(id, court_id, url, username, password_enc, reader)` · `game(id, court_id, started_at, stopped_at, may_post,
state)` · `clip(id, court_id, game_id?, kind{tap,highlight,reel,game}, window_start, window_end, made_at, kept_at?,
hidden, wide_path, vertical_path, token)` ·
`hold(id, court_id, span_start, span_end, placed_at, released_at?)` · `staff(id, username, role{owner,staff},
password_hash, must_change, temp_expires_at?, active, failures, locked_until?)` · `session(token_hash, staff_id,
signed_in_at)` · `log(id, at, staff_id?, action, item, detail)`.

## 3. Key flows

Tap → clip: court page `POST /c/{code}/tap` → `ch-core::plan_clips` merges with an open clip on that court → the job
waits until the window end + 1 segment flush → cutter → clip row `ready` → page polls `/c/{code}/clips` (2 s) and shows
it. Stop → game video + judge → the court page's game panel. Every timing target is NFR-1/NFR-3 on the N150.

## 4. Library choices (reuse before build)

| Problem | Package | Why |
|---|---|---|
| Rules (time windows, expiry, lockout) | `ch-core` + chrono | Domain layer — hand-written by design, golden-tested |
| RTSP ingest | retina 0.4 | Stack decision; production use in Moonfire NVR |
| Segmenting, decode, overlay, crop, encode | GStreamer 1.28 (gstreamer-rs 0.25) | Stack decision; LGPL dynamic; VA hardware encode |
| Inference | ort 2.0.0-rc.15 (pinned exactly) | Stack decision; OpenVINO EP on the N150 |
| HTTP, routing | axum 0.8 + tower-http 0.7 | Stack decision |
| TLS on the staff listener | axum-server (rustls) + rcgen | Self-signed certificate per box (A-124) without OpenSSL |
| Templates | askama 0.16 | Compile-time checked, no JS framework (A-121) |
| Box datastore | rusqlite 0.40 (bundled SQLite) | One file, WAL, survives power cuts |
| Password hashing | argon2 0.6 | ASVS v5.0.0-11.4.2 |
| Random tokens | getrandom (via rand) | Unguessable links and session tokens |

## 5. Security architecture

Trust boundaries and controls follow SRS §9: players reach only `/c/*` paths; staff paths exist only on the
TLS listener; every query scopes by court/venue on the server; clip tokens are 128-bit random and carry no authority
beyond their own item; the venue key is stored hashed on the receiving side; footage and the
database live on a LUKS volume unlocked by the TPM (`systemd-cryptenroll --tpm2-device=auto`) — provisioned by the
install image, verified by `SEC-SN5-STOLEN-BOX`.

## 6. Test architecture

Unit/golden (`cargo test`), integration (the box against real SQLite in containers), e2e (Playwright on
the host against `docker compose -f docker/compose.test.yaml`: MediaMTX fake cameras, three boxes), axe and the design scan on every route, load tests for NFR-4.
Speed targets (NFR-1…NFR-3) only on the N150 (`tools/spike/n150-spike.sh`).

# Court Highlights

Offline-first court cameras for pickleball venues: one camera per court, a small Intel N150 box per four courts,
and a **Clip that** button on each court's phone page. An optional online service emails players their videos and
gives the installer a fleet console.

| Part | Path | What |
|---|---|---|
| Court box | `crates/box` | Rust (axum, GStreamer, SQLite): recording, clips, games, shot judge, court pages, staff console, venue sync |
| Shared rules | `crates/core` | Pure logic with golden tests: clip windows, expiry, send order, disk pressure, lockout |
| Online service | `crates/cloud` | Rust (axum, PostgreSQL 18, R2, SES): box API, take-home email, fleet console |
| Tests | `tests/e2e` | Playwright against the compose stack with replayed footage as RTSP cameras |

## Documents

- [User guide](docs/user-manual.md): players, staff, Owner, installers
- [Runbook](RUNBOOK.md): start/stop, health, configuration, backups, common problems
- [Release notes](RELEASE_NOTES.md) · [Known debt](DEBT.md)
- [Requirements](docs/requirements/requirements.md) · [Design](docs/HLD.md) · [Visual design](DESIGN.md)

## Develop

Needs Docker and Node 24. Everything builds and runs in the `ch-dev` image (Ubuntu 26.04, Rust 1.99, GStreamer 1.28).

```sh
docker build -t ch-dev:0 -f docker/dev.Dockerfile docker
docker volume create ch-target && docker volume create ch-cargo-registry
docker compose -p ch-test -f docker/compose.test.yaml --profile build run --rm builder   # build box + service
cd tests/e2e && npm ci && npx playwright install chromium && npx playwright test          # about an hour
```

Release checks (CI runs them on every push): `cargo fmt --check`, `cargo clippy -D warnings`, unit tests,
`tools/licence-check.cjs` + `tools/gst-licence-check.sh`, `tools/supply-chain-scan.sh`, `tools/payload-check.cjs`,
`tools/cost-model.cjs`, `tools/horizon-check.cjs`.

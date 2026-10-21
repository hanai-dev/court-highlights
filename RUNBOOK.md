# Runbook — Court Highlights

Two programs: **`ch-box`** records the courts at a venue (one per Intel N150 mini PC, offline-first), and
**`ch-cloud`**, the online service (email delivery of videos plus the fleet console), on DigitalOcean Singapore
behind Cloudflare, with PostgreSQL 18, Cloudflare R2 and Amazon SES.

Each command below is marked: **verified** means it was run against the test stack (`docker/compose.test.yaml`)
and its result checked; **at release** means it can only be checked on the real target and is still open.

## 1. Start, stop, status

| What | Command | |
|---|---|---|
| Test stack: one box, cameras, online service | `docker compose -p ch-test -f docker/compose.test.yaml up -d rtsp cam-3 box-a db s3 ses cloud` | verified |
| Test stack: three-box venue | `docker compose -p ch-test -f docker/compose.test.yaml --profile venue up -d venue-a venue-b venue-c` | verified |
| Stop everything (keep data) | `docker compose -p ch-test -f docker/compose.test.yaml --profile venue stop` | verified |
| Build both programs | `docker compose -p ch-test -f docker/compose.test.yaml --profile build run --rm builder` | verified |
| Box on the N150 | `sudo systemctl start court-highlights` / `stop` / `status`; the unit runs `tools/box-launcher.sh` | at release (install image, DEV-INSTALL-IMAGE) |
| Online service image | `docker build -f docker/cloud.Dockerfile -t ch-cloud .` | at release (HOST-container-image) |

A box stops cleanly on SIGTERM: open footage segments are closed and recording stops before the process exits.
After a power cut, unfinished segments are dropped at start and recording resumes within 60 s.

## 2. Health

| Endpoint | Who | Answer | |
|---|---|---|---|
| `GET http://<box>/healthz` | anyone on the venue network | `ok` while the box serves | verified |
| Staff console home `https://<box>/console` | Owner, Staff | per court: recording/camera/last clip; per box: disk, temperature, version, reachability; disk ≥ 80 % warns | verified (MON-HEALTH-PAGE) |
| `GET https://<service>/healthz` | anyone | `ok` (liveness only, says nothing else) | verified |
| `GET https://<service>/readyz` with `Authorization: Bearer $CH_READY_TOKEN` | the platform | `ready` (200) when the database and object store answer; `not ready` (503) otherwise, never saying which; `401` without the token | verified (HOST-health) |
| `ch-cloud healthcheck` | container HEALTHCHECK | exit 0 when `/healthz` answers | verified in the test stack |
| `GET https://<service>/metrics` with the same bearer token | the platform's scrape | Prometheus counters `ch_uploads_total`, `ch_emails_sent_total`, `ch_emails_failed_total`, `ch_checkins_total`, `ch_fleet_actions_total` | verified (HOST-observability) |
| Fleet console `/fleet` | Admin, Installer | every box's last check-in, version, disk, temperature; "Missed check-in" after 5 min silence | verified |

## 3. Configuration

### Box (`ch-box`, environment of the service unit)

| Variable | Required | Default | Meaning |
|---|---|---|---|
| `CH_DATA_DIR` | no | `/var/lib/court-highlights` | Footage, clips, the database (`box.db`), the box's TLS certificate |
| `CH_BOX_NAME` | no | `court-box` | This box's name in the venue and the fleet; also in its certificate |
| `CH_HTTP_ADDR` / `CH_HTTPS_ADDR` | no | `0.0.0.0:80` / `0.0.0.0:443` | Player pages (plain HTTP) / staff console (the box's own certificate) |
| `CH_PLAYER_URL` | yes for signs | — | Address printed on the court signs, e.g. `http://court-box-1.local` |
| `CH_SUPPORT_PHONE` | no | — | Shown on help pages |
| `CH_CLOUD_URL` | no | — | Online service address; without it the box stays offline-only |
| `CH_CHECKIN_SECS` | no | `60` | Fleet check-in interval |
| `CH_VENUE_KEY` | multi-box venues | — | 64 hex characters, the same on every box of the venue; **secret** |
| `CH_PEERS` | multi-box venues | — | Siblings' sync addresses, `https://court-box-2.local:8444,…` |
| `CH_SYNC_ADDR` / `CH_SYNC_SECS` | no | `0.0.0.0:8444` / `15` | Sync listener (TLS, venue key required) and pull interval |
| `CH_SLOTS_DIR` | for updates | — | Update slots `a`, `b` and the `current` link (`/opt/court-highlights` on the box) |
| `CH_RELEASE_KEY` | for updates | — | The owner's release public key, 64 hex characters; updates signed with anything else are refused |
| `CH_UPDATE_GRACE_SECS` | no | `600` | How long a new version has to become healthy before the launcher takes it back |
| `CH_SEED_BINARY` | launcher, first start | — | The ch-box the launcher installs into slot `a` when no slot exists yet |

The box key for the online service is entered in **Settings → Online service**, stored in the box database and
never shown again.

### Online service (`ch-cloud`)

| Variable | Required | Meaning |
|---|---|---|
| `DATABASE_URL` | yes, **secret** | PostgreSQL 18 connection string |
| `CH_PUBLIC_URL` | yes | Public address, e.g. `https://videos.example.ph` (links in emails, same-origin checks) |
| `CH_S3_ENDPOINT`, `CH_S3_PUBLIC_ENDPOINT`, `CH_S3_REGION` (`auto`), `CH_S3_BUCKET` | yes / no / no / yes | R2 bucket for uploaded videos |
| `CH_S3_KEY`, `CH_S3_SECRET` | yes, **secret** | R2 token scoped to that one bucket (object read/write) |
| `CH_SES_REGION` (`ap-southeast-1`), `CH_SES_ENDPOINT` (tests only) | no | Amazon SES |
| `CH_SES_KEY`, `CH_SES_SECRET` | yes, **secret** | IAM user allowed only `ses:SendEmail` and `ses:PutAccountSuppressionAttributes` |
| `CH_MAIL_FROM` | yes | Verified sender, e.g. `videos@example.ph` |
| `CH_EMAIL_PEPPER` | yes, **secret**, ≥ 32 characters | Hashes blocked addresses; changing it forgets every "don't email me again" |
| `CH_SECRET_KEY` | yes, **secret**, 64 hex | Encrypts fleet users' authenticator keys; changing it breaks every fleet sign-in |
| `CH_READY_TOKEN` | recommended, **secret** | Bearer token for `/readyz` |
| `CH_LISTEN` | no (`0.0.0.0:8080`) | Listen address |

The service refuses to start when a required value is missing or malformed, and prints the variable's name, never
its value (verified: HOST-configuration test). At start it turns on the SES account-level suppression list for
bounces and complaints, so a hard-bounced address gets no further mail from any venue.

## 4. First accounts and boxes

| Task | Command | |
|---|---|---|
| First fleet Admin | `ch-cloud create-admin <username>` → prints a temporary password and the authenticator key (once) | verified |
| Register a box (also in the fleet console) | `ch-cloud add-box "<venue>" <box name>` → prints the box key once | verified |
| Revoke a box key | Fleet console → venue → **Revoke the key of …** | verified |

## 5. Updates

| Task | How | |
|---|---|---|
| Make the release key, once | `ch-box release-key owner-release.pk8` prints `CH_RELEASE_KEY=…` for the install image. Keep the `.pk8` file offline. | verified |
| Make an update | `ch-box make-update <version> target/release/ch-box owner-release.pk8 ch-box-<version>.chupdate` | verified |
| Send it to the fleet | Fleet console → **Updates** → **Publish update**. Each venue gets it one box at a time; venues with online features off get nothing. | verified |
| Install from a USB stick | Staff console → Settings → **Software update…** → the file → **Install update** | verified |
| A failed update | The launcher returns the box to its previous version when the new one fails to start 3 times or isn't healthy (every court with a camera recording) within 10 minutes. The console and the fleet show **Update rolled back**, and the venue's other boxes don't get that release. | verified |

## 6. Backup and restore

| What | How | |
|---|---|---|
| Box settings (courts, cameras, logo, hours, settings, staff password hashes) | Settings → **Settings backup**: an encrypted file with a passphrase the Owner chooses | verified |
| Replace a box at a one-box venue | First-run setup → **Replacing a box?** → the file + passphrase | verified |
| Replace a box at a multi-box venue | Install with the same `CH_VENUE_KEY` and `CH_PEERS`; it fills itself in from its siblings within a minute | verified |
| Online service database | DigitalOcean managed PostgreSQL daily backups, plus a nightly `pg_dump --format=custom` to R2 | at release (HOST-backup) |
| Restore exercise | `pg_restore --clean --no-owner -d <isolated db> dump.pgc`, then point a test `ch-cloud` at it and sign in to the fleet console | at release (HOST-restore) |

Footage and clips are never backed up: footage lives 4 hours, clips 72 hours (kept ones 1 year); held footage stays
on the box until released.

## 7. Common problems

| Symptom | Likely cause | Fix |
|---|---|---|
| Court shows **Camera offline** (red) | Camera power, cable or address | Check the PoE switch port and cable; Settings → Courts → the court: compare the preview and the stream address. Recording resumes within 5 s of the camera's return. |
| **Disk 80 %** warning on the console | More kept clips or holds than the disk holds | Release holds that are no longer needed; delete old kept clips. At 90 % the box removes the oldest unkept game videos, then clips. |
| Settings → Online service says **key refused** | The key was revoked or mistyped | Ask the installer for a new key from the fleet console. |
| Email box missing on court pages | The venue's online features are switched off in the fleet console, or the box isn't registered | Fleet console → venue → switch on (with a reason); or enter the box key. |
| A player says no email arrived | Address blocked by its owner, hard-bounced, or still waiting for the line | Fleet log and the SES suppression list; waiting videos go out when the line returns, clips first. |
| A sibling box shows **unreachable** | Power or network at that box | Its courts keep recording on their own box; the console shows the rest. |
| `/readyz` answers `not ready` | Database or R2 unreachable | The service log names the failing part (`database=false` or `object_store=false`). |
| Box time wrong | No time source | Settings → **Correct the time**; every box at the venue follows. |

## 8. Logs

| Where | How to read | |
|---|---|---|
| Box | `journalctl -u court-highlights` on the box (test stack: `docker compose -p ch-test -f docker/compose.test.yaml logs box-a`) | test stack verified; unit at release |
| Online service | stderr of the container (platform log stream) | verified in the test stack |
| Who did what on a box | Staff console → **Activity log**, exportable as CSV | verified |
| Who did what in the fleet | Fleet console → **Log** | verified |

Logs carry ids, never player emails, passwords, tokens or keys. To follow one video through the service, search
for its item id (`item=<id>`) and the box id (`box_id=<id>`).

## 9. Security operations

- **Remove access:** deactivate a staff member (Staff → Edit → Deactivate) — their sessions end on every box
  within a minute. Revoke a box key in the fleet console. A fleet user who leaves: Fleet → Users →
  **Deactivate**; their open session ends at once.
- **Rotate keys:** a new box key: register the box again and enter the new key in Settings. A new venue key: set
  the same new `CH_VENUE_KEY` on every box of the venue and restart them together. `CH_EMAIL_PEPPER` and
  `CH_SECRET_KEY` are not rotated in place (see section 3).
- **Patches:** the release owner runs `tools/supply-chain-scan.sh` and the licence checks before each release;
  CI runs them on every push. Reviewed advisories are listed in `DEBT.md`.
- **Wipe a box** that leaves a venue: Settings → **Wipe this box**, type the venue name. It leaves the fleet and
  erases everything.
- **Still to verify at release:** TLS 1.2/1.3 and HSTS at the Cloudflare edge, R2 token scope, SES IAM policy,
  managed-database backups, the secret store on DigitalOcean.

## 10. Changes after release

New work re-enters the build pipeline with the shipped acceptance suite as the floor that may not regress:
`/forge:feature` for new behaviour, `/forge:fix` for defects, `/forge:improve` for research-led improvements.

# Runbook — Court Highlights

One program: **`ch-box`** records the courts at a venue, one per Intel N150 mini PC. The system works on the venue
network only (LAN and Wi-Fi). Nothing leaves the venue and no box needs internet. Players download on court, staff
use the console on the same network, and updates arrive as a signed file on a USB stick.

Each command below is marked: **verified** means it was run against the test stack (`docker/compose.test.yaml`)
and its result checked; **at release** means it can only be checked on the real box and is still open.

## 1. Start, stop, status

| What | Command | |
|---|---|---|
| Test stack: one box and cameras | `docker compose -p ch-test -f docker/compose.test.yaml up -d rtsp cam-3 box-a` | verified |
| Test stack: three-box venue | `docker compose -p ch-test -f docker/compose.test.yaml --profile venue up -d venue-a venue-b venue-c` | verified |
| Stop everything (keep data) | `docker compose -p ch-test -f docker/compose.test.yaml --profile venue stop` | verified |
| Build the box program | `docker compose -p ch-test -f docker/compose.test.yaml --profile build run --rm builder` | verified |
| Box on the N150 | `sudo systemctl start court-highlights` / `stop` / `status`; the unit runs `tools/box-launcher.sh` | at release (install image, DEV-INSTALL-IMAGE) |

A box stops cleanly on SIGTERM: open footage segments are closed and recording stops before the process exits.
After a power cut, unfinished segments are dropped at start and recording resumes within 60 s.

## 2. Health

| Endpoint | Who | Answer | |
|---|---|---|---|
| `GET http://<box>/healthz` | anyone on the venue network | `ok` while the box serves | verified |
| Staff console home `https://<box>/console` | Owner, Staff | per court: recording/camera/last clip; per box: disk, temperature, version, reachability; disk ≥ 80 % warns | verified (MON-HEALTH-PAGE) |

## 3. Configuration

Environment of the box's service unit:

| Variable | Required | Default | Meaning |
|---|---|---|---|
| `CH_DATA_DIR` | no | `/var/lib/court-highlights` | Footage, clips, the database (`box.db`), the box's TLS certificate |
| `CH_BOX_NAME` | no | `court-box` | This box's name in the venue; also in its certificate |
| `CH_HTTP_ADDR` / `CH_HTTPS_ADDR` | no | `0.0.0.0:80` / `0.0.0.0:443` | Player pages (plain HTTP) / staff console (the box's own certificate) |
| `CH_PLAYER_URL` | yes for signs | — | Address printed on the court signs, e.g. `http://court-box-1.local` |
| `CH_SUPPORT_PHONE` | no | — | Shown on help pages |
| `CH_VENUE_KEY` | multi-box venues | — | 64 hex characters, the same on every box of the venue; **secret** |
| `CH_PEERS` | multi-box venues | — | Siblings' sync addresses, `https://court-box-2.local:8444,…` |
| `CH_SYNC_ADDR` / `CH_SYNC_SECS` | no | `0.0.0.0:8444` / `15` | Sync listener (TLS, venue key required) and pull interval |
| `CH_SLOTS_DIR` | for updates | — | Update slots `a`, `b` and the `current` link (`/opt/court-highlights` on the box) |
| `CH_RELEASE_KEY` | for updates | — | The owner's release public key, 64 hex characters; updates signed with anything else are refused |
| `CH_UPDATE_GRACE_SECS` | no | `600` | How long a new version has to become healthy before the launcher takes it back |
| `CH_SEED_BINARY` | launcher, first start | — | The ch-box the launcher installs into slot `a` when no slot exists yet |

## 4. Updates

| Task | How | |
|---|---|---|
| Make the release key, once | `ch-box release-key owner-release.pk8` prints `CH_RELEASE_KEY=…` for the install image. Keep the `.pk8` file offline. | verified |
| Cut a release | GitHub → Actions → **release** → Run workflow. It runs the full CI (e2e included) on `main`, then semantic-release picks the version from the Conventional Commit messages since the last tag (`fix:` → patch, `feat:` → minor, `feat!:` or a `BREAKING CHANGE:` footer → major), builds the box with that version, tags `v<version>` and publishes a GitHub release with the release notes, `ch-box-<version>-linux-x86_64`, its `.sha256` and a provenance attestation (proof that this workflow built that file). The release notes are the changelog; the crate versions in the repository are not updated (`ch-box --version` on the release build is the version that counts). Preview the next version locally: `cd tools/release && npm ci && cd ../.. && GITHUB_TOKEN=$(gh auth token) tools/release/node_modules/.bin/semantic-release --dry-run --no-ci`. Never edit versions or tags by hand. The `v0.0.0` tag on the first commit is the starting point, not a release: it makes the first release 0.1.0 (semantic-release otherwise starts at 1.0.0); leave it in place. Venues only ever get these plain releases. | not yet run |
| Cut a release candidate for the test box | The `rc` branch is only ever a fast-forward of `main`: `git push origin main:rc`. Then Actions → **release** → Run workflow with **Use workflow from: rc**. It runs the same CI and publishes `v<version>-rc.1` as a GitHub pre-release (the next candidate from the same `main` work is `-rc.2`, and so on). Sign and install it like any release (`make-update.sh 0.2.0-rc.1 …`), on the **test box only**. Fixes go to `main` by pull request; push `main:rc` again for the next candidate. When the test box is happy, cut the plain release from `main` (above): `v<version>` with no suffix, for the venues. The three environments (NFR-18): CI with replayed cameras, the test box on `rc` candidates, venues on plain releases. | not yet run |
| Make an update | `sh tools/release/make-update.sh <version> owner-release.pk8 [/media/usb]`, with `gh` signed in: it downloads the release's executable, checks its `.sha256` and its provenance (`gh attestation verify`: built by this repository's release workflow, by nothing and no one else), signs it with the offline key into `ch-box-<version>.chupdate` and copies that to the USB stick. It signs on a Linux machine that runs ch-box, or inside the `ch-dev:0` build image anywhere else. By hand: `sha256sum -c ch-box-<version>-linux-x86_64.sha256`, `gh attestation verify ch-box-<version>-linux-x86_64 --repo hanai-dev/court-highlights`, then `ch-box make-update <version> ch-box-<version>-linux-x86_64 owner-release.pk8 ch-box-<version>.chupdate`. A release cut before the workflow attested has no attestation: sign it by hand without that check. Each box installs the `.chupdate` file from a USB stick (below). | signing verified; script not yet run against a release |
| Install from a USB stick | Staff console → Settings → **Software update…** → the file → **Install update**, on each box of the venue | verified |
| A failed update | The launcher returns the box to its previous version when the new one fails to start 3 times or isn't healthy (every court with a camera recording) within 10 minutes. The console shows **Update rolled back**. | verified |

Update every box of a venue on the same visit: boxes on different versions may not sync settings until all match.

## 5. Backup and restore

| What | How | |
|---|---|---|
| Box settings (courts, cameras, logo, hours, settings, staff password hashes) | Settings → **Settings backup**: an encrypted file with a passphrase the Owner chooses | verified |
| Replace a box at a one-box venue | First-run setup → **Replacing a box?** → the file + passphrase | verified |
| Replace a box at a multi-box venue | Install with the same `CH_VENUE_KEY` and `CH_PEERS`; it fills itself in from its siblings within a minute | verified |

Footage and clips are never backed up: footage lives 4 hours, clips 72 hours (kept ones 1 year); held footage stays
on the box until released.

## 6. Common problems

| Symptom | Likely cause | Fix |
|---|---|---|
| Court shows **Camera offline** (red) | Camera power, cable or address | Check the PoE switch port and cable; Settings → Courts → the court: compare the preview and the stream address. Recording resumes within 5 s of the camera's return. |
| **Disk 80 %** warning on the console | More kept clips or holds than the disk holds | Release holds that are no longer needed; delete old kept clips. At 90 % the box removes the oldest unkept game videos, then clips. |
| A player can't open the court page | Phone not on the venue Wi-Fi | The court page is served by the box on the venue network only; join the venue Wi-Fi and scan again. |
| A sibling box shows **unreachable** | Power or network at that box | Its courts keep recording on their own box; the console shows the rest. |
| Box time wrong | No time source | Settings → **Correct the time**; every box at the venue follows. |

## 7. Logs

| Where | How to read | |
|---|---|---|
| Box | `journalctl -u court-highlights` on the box (test stack: `docker compose -p ch-test -f docker/compose.test.yaml logs box-a`) | test stack verified; unit at release |
| Who did what on a box | Staff console → **Activity log**, exportable as CSV | verified |

Logs carry ids, never passwords, tokens or keys.

## 8. Security operations

- **Remove access:** deactivate a staff member (Staff → Edit → Deactivate); their sessions end on every box within
  a minute.
- **Rotate the venue key:** set the same new `CH_VENUE_KEY` on every box of the venue and restart them together.
- **Patches:** the release owner runs `tools/supply-chain-scan.sh` and the licence checks before each release;
  CI runs them on every push. Reviewed advisories are listed in `DEBT.md`.
- **Wipe a box** that leaves a venue: Settings → **Wipe this box**, type the venue name. It erases everything.
- **Still to verify at release:** the encrypted data partition on the install image (SEC-SN5-STOLEN-BOX).

## 9. Changes after release

New work re-enters the build pipeline with the shipped acceptance suite as the floor that may not regress:
`/forge:feature` for new behaviour, `/forge:fix` for defects, `/forge:improve` for research-led improvements.

# Release notes

## 0.1.0 — not yet released

The first build of Court Highlights for the trial at Paddle Point Leisure Hub, Cagayan de Oro. It has not been
deployed or tagged: release and deployment wait for the owner's go-ahead and for the open items below.

### For players
- **Clip that** on the court page: 25 s before to 5 s after the tap, taps within 5 s make one longer clip, ready in
  about 10 s, wide and vertical copies with the venue logo, no sound.
- **Games**: Start and Stop on any phone at the court; the whole game video plus a highlights reel and up to 5 best
  points picked by the shot judge; games stop by themselves after 2 hours or at closing.
- **Take-home email** (when the venue is online): up to 8 addresses per game, one email each, links for about 3 days;
  **Don't email me again** blocks an address for every venue; hard-bounced addresses get nothing further.
- Works with no internet: court pages and downloads come from the court box on the venue Wi-Fi.

### For venues
- First-run setup with no default account; cameras found on the network, live previews, court signs.
- Staff console: courts today with box health, clip library across all boxes (filters, keep for a year, posting
  permission warnings, delete with confirmation), footage holds and exports, activity log with CSV export, staff
  accounts with temporary passwords, lockout after 5 wrong passwords.
- Several boxes act as one venue: shared staff, settings and courts; a replacement box fills itself in; box-to-box
  traffic is encrypted and needs the venue key.
- Settings backup and restore; time correction for every box; **Wipe this box** for a venue that closes.
- Signed software updates from the fleet or a USB stick; a version that fails to start or isn't healthy within
  10 minutes is taken back automatically and reported as "Update rolled back".
- Automatic clean-up: footage after 4 hours, clips and game videos after 72 hours (at 3:00 am), oldest unkept videos
  first when the disk passes 90 %.

### For the owner (fleet console)
- Password plus authenticator code; Admin and Installer roles; idle and absolute session limits.
- Venues, box registration and revocation, per-box health and missed check-ins, the online-features switch with a
  reason, fleet users (add, deactivate), update publishing with one-box-at-a-time rollout, the fleet log. No footage
  is reachable from the fleet console.
- Online service: readiness and liveness endpoints, Prometheus metrics, structured logs without player data.

### Known issues and open items
- **Not yet proven on the Intel N150 box**: 24-hour recording without gaps, clip speed and shot-judge throughput
  (DEV-SPIKE, MON-RECORDING-GAPS, MON-CLIP-SPEED, MON-JUDGE-THROUGHPUT). The owner runs the spike script on the box.
- **Shot judge accuracy** is not measured yet: it needs hand-marked footage from the trial court (F-JUDGE-EVAL).
- **Install image**: the USB install image (Ubuntu 26.04, encrypted data partition, the launcher as a systemd
  service) is not built yet (DEV-INSTALL-IMAGE). Updates and rollback are proven in the test stack, not yet on the box.
- **Online service deployment** to DigitalOcean (container image scan, backups, restore drill, edge TLS) waits for an
  authorized target (HOST-* rows).
- **First-use observation** with real players on the trial court (UX-FIRST-USE).

See `DEBT.md` for accepted findings and deliberate simplifications.

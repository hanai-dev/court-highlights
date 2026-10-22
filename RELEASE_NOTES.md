# Release notes

## 0.1.0 — not yet released

The first build of Court Highlights for the trial at Paddle Point Leisure Hub, Cagayan de Oro. It has not been
deployed or tagged: release and deployment wait for the owner's go-ahead and for the open items below.

Changed on 2026-10-22 by the owner's decision: the system works on the venue network only. The online service, <!-- local-only: change record -->
take-home email and the fleet console are gone, and nothing leaves the venue. The change record is at the top of <!-- local-only: change record -->
`docs/requirements/requirements.md`.

### For players
- **Clip that** on the court page: 25 s before to 5 s after the tap, taps within 5 s make one longer clip, ready in
  about 10 s, wide and vertical copies with the venue logo, no sound.
- **Games**: Start and Stop on any phone at the court; the whole game video plus a highlights reel and up to 5 best
  points picked by the shot judge; games stop by themselves after 2 hours or at closing.
- Works on the venue network only: court pages and downloads come from the court box on the venue Wi-Fi, and
  nothing is uploaded anywhere.

### For venues
- First-run setup with no default account; cameras found on the network, live previews, court signs.
- Staff console: courts today with box health, clip library across all boxes (filters, keep for a year, posting
  permission warnings, delete with confirmation), footage holds and exports, activity log with CSV export, staff
  accounts with temporary passwords, lockout after 5 wrong passwords.
- Several boxes act as one venue (update them all on the same visit): shared staff, settings and courts; a replacement box fills itself in; box-to-box
  traffic is encrypted and needs the venue key.
- Settings backup and restore; time correction for every box; **Wipe this box** for a venue that closes.
- Signed software updates from a USB stick; a version that fails to start or isn't healthy within
  10 minutes is taken back automatically and reported as "Update rolled back".
- Automatic clean-up: footage after 4 hours, clips and game videos after 72 hours (at 3:00 am), oldest unkept videos
  first when the disk passes 90 %.

### Known issues and open items
- **Not yet proven on the Intel N150 box**: 24-hour recording without gaps, clip speed and shot-judge throughput
  (DEV-SPIKE, MON-RECORDING-GAPS, MON-CLIP-SPEED, MON-JUDGE-THROUGHPUT). The owner runs the spike script on the box.
- **Shot judge accuracy** is not measured yet: it needs hand-marked footage from the trial court (F-JUDGE-EVAL).
- **Install image**: the USB install image (Ubuntu 26.04, encrypted data partition, the launcher as a systemd
  service) is not built yet (DEV-INSTALL-IMAGE). Updates and rollback are proven in the test stack, not yet on the box.
- **First-use observation** with real players on the trial court (UX-FIRST-USE).

See `DEBT.md` for accepted findings and deliberate simplifications.

#!/usr/bin/env sh
# Release gate (SEC-DEPENDENCIES, SEC-SECRETS): known advisories in the Rust lockfile (osv-scanner) and leaked
# credentials anywhere in the git history (gitleaks). Exit non-zero on any finding not reviewed in DEBT.md.
# Usage: tools/supply-chain-scan.sh [repo dir]   (needs docker)
set -u
# Docker on Windows needs the Windows form of the path (Git Bash: pwd -W); elsewhere plain pwd.
REPO=$(cd "${1:-.}" && { pwd -W 2>/dev/null || pwd; })
OSV=ghcr.io/google/osv-scanner:v2.6.0
GITLEAKS=zricethezav/gitleaks:v8.30.1
# Reviewed advisories (see DEBT.md): informational notices for build-time-only crates.
REVIEWED="RUSTSEC-2026-0374"

echo "# osv-scanner ($OSV) on Cargo.lock @ $(git -C "${1:-.}" rev-parse --short HEAD) $(date -u +%FT%TZ)"
osv_out=$(MSYS_NO_PATHCONV=1 docker run --rm -v "$REPO:/src:ro" "$OSV" scan source -L /src/Cargo.lock 2>&1)
echo "$osv_out"
unreviewed=$(echo "$osv_out" | grep -o 'RUSTSEC-[0-9]*-[0-9]*\|GHSA-[a-z0-9-]*\|CVE-[0-9]*-[0-9]*' | sort -u | grep -v -x -F "$REVIEWED" || true)
echo
echo "# gitleaks ($GITLEAKS) over the git history"
MSYS_NO_PATHCONV=1 docker run --rm -v "$REPO:/repo:ro" "$GITLEAKS" git /repo --redact --no-banner --no-color 2>&1
leaks=$?
echo
if [ -n "$unreviewed" ] || [ "$leaks" -ne 0 ]; then
  echo "SUPPLY-CHAIN: FAIL (unreviewed advisories: ${unreviewed:-none}; gitleaks exit $leaks)"
  exit 1
fi
echo "SUPPLY-CHAIN: PASS (reviewed: $REVIEWED; no leaks)"

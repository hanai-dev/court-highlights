#!/bin/sh
# The owner's deployment step (RUNBOOK section 4): take a release from GitHub, prove it is the executable this
# repository's release workflow built, sign it with the offline release key, and put the update file on the USB stick.
#
#   sh tools/release/make-update.sh <version> <owner-release.pk8> [<usb mount>]
#
# Needs `gh` signed in (the download and the provenance check) and a machine that runs the release executable:
# Linux with the GStreamer packages (RUNBOOK section 3). Anywhere else, once `ch-dev:0` is built (RUNBOOK section 1),
# the signing step runs inside that image; the key file must then sit under the current directory.
set -eu
V=$1
KEY=$2
USB=${3:-}
REPO=hanai-dev/court-highlights
BIN=ch-box-$V-linux-x86_64
OUT=ch-box-$V.chupdate

gh release download "v$V" --repo "$REPO" --pattern "$BIN*" --clobber
sha256sum -c "$BIN.sha256"
# Built and attested by release.yml on GitHub's runners; a file uploaded by anyone, or built by any other workflow,
# fails here. A release cut before the workflow attested has no attestation: sign that one by hand (RUNBOOK section 4).
gh attestation verify "$BIN" --repo "$REPO" --signer-workflow "$REPO/.github/workflows/release.yml"
chmod 755 "$BIN"

if "./$BIN" --version >/dev/null 2>&1; then
  "./$BIN" make-update "$V" "$BIN" "$KEY" "$OUT"
else
  docker run --rm -v "$PWD:/work" -w /work ch-dev:0 "./$BIN" make-update "$V" "$BIN" "$KEY" "$OUT"
fi

if [ -n "$USB" ]; then
  cp "$OUT" "$USB/"
  echo "$OUT copied to $USB"
fi
echo "$OUT ready: install it on each box of the venue from Settings, Software update"

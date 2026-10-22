#!/bin/sh
# semantic-release prepare step (.releaserc.json): stamp the version, then build the box executable the release
# ships. Nothing is tagged or published if this fails. The executable is unsigned: the owner signs it offline with
# `ch-box make-update` (RUNBOOK section 4), so the release key never reaches CI.
# Usage: sh tools/release/build.sh <version>   (needs docker and the ch-dev:0 build image)
set -eu
V=$1

node tools/release/set-version.cjs "$V"

# The box's target OS image: release build without test hooks, the GStreamer licence gate, and the version it reports.
docker run --rm -v "$PWD:/work" -w /work ch-dev:0 sh -c \
  "cargo build --release --locked -p ch-box && sh tools/gst-licence-check.sh target/release/ch-box"
built=$(docker run --rm -v "$PWD:/work" -w /work ch-dev:0 target/release/ch-box --version)
[ "$built" = "$V" ] || { echo "ch-box reports version $built, expected $V" >&2; exit 1; }

mkdir -p dist
cp target/release/ch-box "dist/ch-box-$V-linux-x86_64"
(cd dist && sha256sum "ch-box-$V-linux-x86_64" > "ch-box-$V-linux-x86_64.sha256")

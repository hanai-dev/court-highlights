#!/bin/sh
# Runs the court box from its active slot and takes a failed update back (FR-51, FR-54, NFR-12).
# The service manager starts this script (systemd ExecStart on the box; the compose command in tests).
#
#   $CH_SLOTS_DIR/a, b        each holds a ch-box executable and its VERSION
#   $CH_SLOTS_DIR/current     link to the active slot
#   $CH_SLOTS_DIR/pending/    an update not yet proven: previous slot, version, start time, boot count
#   $CH_SLOTS_DIR/rolled-back version and reason of the last update taken back, for the box to report
#
# ch-box stages an update into the spare slot, points `current` at it and exits; this script starts the new
# version. ch-box removes `pending` once it is healthy. If the new version fails to start 3 times, or is not
# healthy within CH_UPDATE_GRACE_SECS (10 min), this script returns `current` to the previous slot.
set -u
SLOTS=${CH_SLOTS_DIR:-/opt/court-highlights}
GRACE=${CH_UPDATE_GRACE_SECS:-600}

if [ ! -e "$SLOTS/current" ]; then
  mkdir -p "$SLOTS/a"
  cp "${CH_SEED_BINARY:?CH_SEED_BINARY names the first ch-box to install}" "$SLOTS/a/ch-box"
  chmod 755 "$SLOTS/a/ch-box"
  "$SLOTS/a/ch-box" --version > "$SLOTS/a/VERSION"
  ln -s a "$SLOTS/current"
fi

rollback() {
  previous=$(cat "$SLOTS/pending/previous")
  printf '%s\n%s\n' "$(cat "$SLOTS/pending/version")" "$1" > "$SLOTS/rolled-back"
  ln -sfn "$previous" "$SLOTS/current.new" && mv -T "$SLOTS/current.new" "$SLOTS/current"
  rm -rf "$SLOTS/pending"
  echo "box-launcher: update rolled back ($1)" >&2
}

child=
stop() {
  [ -n "$child" ] && kill -TERM "$child" 2>/dev/null && wait "$child"
  exit 0
}
trap stop TERM INT

while true; do
  if [ -d "$SLOTS/pending" ]; then
    boots=$(($(cat "$SLOTS/pending/boots" 2>/dev/null || echo 0) + 1))
    echo "$boots" > "$SLOTS/pending/boots"
    [ "$boots" -gt 3 ] && rollback "did not start"
  fi
  "$SLOTS/current/ch-box" &
  child=$!
  while kill -0 "$child" 2>/dev/null; do
    # ch-box may remove `pending` at any moment (healthy): read it once, and only act on what was read.
    started=$(cat "$SLOTS/pending/started" 2>/dev/null || true)
    if [ -n "$started" ] && [ $(($(date +%s) - started)) -gt "$GRACE" ] && [ -d "$SLOTS/pending" ]; then
      kill -TERM "$child" 2>/dev/null
      sleep 5
      kill -KILL "$child" 2>/dev/null
      wait "$child" 2>/dev/null
      rollback "not healthy within $GRACE s"
      break
    fi
    sleep 1 &
    wait $! 2>/dev/null
  done
  wait "$child" 2>/dev/null
  child=
  sleep 1
done

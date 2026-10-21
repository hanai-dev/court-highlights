#!/usr/bin/env bash
# The pre-registered feasibility spike (DEV-SPIKE; docs/adr/0001-tech-stack/stack-decision.md, Confirmation H1–H5),
# run by the owner on the Intel N150 box. It runs the real court box (release build, through the update launcher)
# against four replayed 1080p30 cameras and measures:
#   H1  no recording gap > 2 s per stream over the soak (24 h)
#   H2  clip ready ≤ 10 s after its window ends, p95 over ≥ 100 taps, while a game records and is judged
#   H3  the shot judge finishes 4 × 1 h games in ≤ 60 min (while recording continues)
#   H4  a camera unplugged for 30 s records again ≤ 5 s after it returns (retina path; no rtspsrc fallback is built)
#   H5  an injected failing update returns the box to its previous version by itself
#
# On the N150 (Ubuntu 26.04), from the repo root:
#   cargo build --release -p ch-box
#   tools/spike/n150-spike.sh quick               H2 (100 taps), H4 and H5, ~45 min, 5 runs of H4
#   tools/spike/n150-spike.sh judge [minutes]     H3 with 4 games of [60] minutes
#   tools/spike/n150-spike.sh soak [hours]        H1 over [24] hours (leave it running)
#   FOOTAGE=/path/to/court-footage tools/spike/n150-spike.sh quick     replay real court recordings (court1..4.mp4)
# Needs: gstreamer1.0-tools with the va plugin, intel-media-va-driver, ffmpeg, mediamtx, sqlite3, curl.
# Results land in spike-<time>/results.txt; copy the summary into docs/adr/0001-tech-stack/confirmation-results.md.
set -euo pipefail

MODE=${1:-quick}
ARG=${2:-}
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
BIN=${CH_BOX_BIN:-$ROOT/target/release/ch-box}
OUT=${SPIKE_DIR:-$ROOT/spike-$(date +%Y%m%d-%H%M)}
PLAYER=http://127.0.0.1:18080
STAFF=https://127.0.0.1:18443
JAR=$OUT/cookies.txt
mkdir -p "$OUT"
RESULTS=$OUT/results.txt

log() { echo "$(date +%T) $*" | tee -a "$RESULTS"; }
now_ms() { date +%s%3N; }
die() { log "STOP: $*"; exit 1; }
for c in ffmpeg mediamtx curl gst-inspect-1.0; do command -v "$c" >/dev/null || die "missing $c"; done
case "$MODE" in judge | soak) command -v sqlite3 >/dev/null || die "missing sqlite3 (apt install sqlite3)" ;; esac
[ -x "$BIN" ] || die "no $BIN: run cargo build --release -p ch-box first"
gst-inspect-1.0 vah264enc >/dev/null 2>&1 || log "WARNING: vah264enc is not available: clips will use the software encoder and H2 will miss"

# p50/p95/max of the numbers on stdin (ms), as "p50=… p95=… max=… n=…".
stats() { sort -n | awk '{v[NR]=$1} END {if (!NR) {print "n=0"; exit} i50=int((NR*50+99)/100); i95=int((NR*95+99)/100); printf "p50=%d p95=%d max=%d n=%d", v[i50], v[i95], v[NR], NR}'; }

# ---------------------------------------------------------------- cameras: replayed footage, stream copy only
prepare_footage() {
  if [ -n "${FOOTAGE:-}" ]; then echo "$FOOTAGE"; return; fi
  local dir=$OUT/footage
  mkdir -p "$dir"
  for n in 1 2 3 4; do
    [ -f "$dir/court$n.mp4" ] && continue
    # Encoded once, before any measurement, so the N150 only copies packets while it is being measured.
    ffmpeg -nostdin -loglevel error -f lavfi -i "testsrc2=size=1920x1080:rate=30" -t "${SPIKE_FOOTAGE_SECONDS:-600}" \
      -c:v libx264 -preset veryfast -b:v 4M -maxrate 4M -bufsize 8M -g 60 "$dir/court$n.mp4"
  done
  echo "$dir"
}

camera_up() {
  ffmpeg -nostdin -loglevel error -re -stream_loop -1 -i "$FOOTAGE_DIR/court$1.mp4" -an -c copy -f rtsp "rtsp://127.0.0.1:8554/court$1" &
  echo $! > "$OUT/cam$1.pid"
}
camera_down() { kill "$(cat "$OUT/cam$1.pid")" 2>/dev/null || true; }

# ---------------------------------------------------------------- the box, as systemd runs it
start_box() {
  local key
  key=$("$BIN" release-key "$OUT/release.pk8" | sed -n 's/^CH_RELEASE_KEY=//p')
  CH_DATA_DIR=$OUT/data CH_SLOTS_DIR=$OUT/slots CH_SEED_BINARY=$BIN CH_RELEASE_KEY=$key \
    CH_HTTP_ADDR=127.0.0.1:18080 CH_HTTPS_ADDR=127.0.0.1:18443 CH_BOX_NAME=spike-box CH_PLAYER_URL=$PLAYER \
    RUST_LOG=warn sh "$ROOT/tools/box-launcher.sh" >"$OUT/box.log" 2>&1 &
  echo $! > "$OUT/box.pid"
  for _ in $(seq 1 60); do curl -fs "$PLAYER/healthz" >/dev/null && return; sleep 1; done
  die "the box did not start (see $OUT/box.log)"
}

staff() { curl -sk -b "$JAR" -c "$JAR" -H "Origin: $STAFF" "$@"; }

setup_venue() {
  staff -o /dev/null -F venue="Spike Club" -F open=00:00 -F close=23:59 -F display_name=Owner -F username=owner \
    -F password=spike-owner-pass-1 "$STAFF/setup"
  for n in 1 2 3 4; do
    staff -o /dev/null --data-urlencode "name=Court $n" --data-urlencode "camera_url=rtsp://127.0.0.1:8554/court$n" \
      -d camera_user= -d camera_password= "$STAFF/console/courts"
  done
  mapfile -t CODES < <(staff "$STAFF/console" | grep -o 'data-court-code>[A-Z0-9]*' | sed 's/.*>//')
  [ "${#CODES[@]}" -eq 4 ] || die "expected 4 court codes, got ${#CODES[@]}"
  for n in 0 1 2 3; do wait_recording "${CODES[$n]}" 120 >/dev/null || die "court $((n + 1)) is not recording"; done
}

# Milliseconds until the court page says Recording (or fails after $2 s).
wait_recording() {
  local start end
  start=$(now_ms)
  end=$((start + $2 * 1000))
  while [ "$(now_ms)" -lt "$end" ]; do
    curl -fs "$PLAYER/c/$1" | grep -q 'status-ok' && { echo $(($(now_ms) - start)); return 0; }
    sleep 0.25
  done
  return 1
}

# (grep finds nothing before the first clip: with pipefail that would end the script.)
ready_count() { curl -fs "$PLAYER/c/$1/clips" | grep -o 'data-clip-state="ready"' | wc -l || true; }

# ---------------------------------------------------------------- H2: clip ready time after its window
h2() {
  local taps=${1:-100} code before t0 ready_in t1 game
  game=${CODES[3]}
  curl -fs -o /dev/null -H "Origin: $PLAYER" -d "" "$PLAYER/c/$game/game/start" # court 4 records a game meanwhile
  : > "$OUT/h2.ms"
  for i in $(seq 1 "$taps"); do
    code=${CODES[$((RANDOM % 3))]}
    before=$(ready_count "$code")
    t0=$(now_ms)
    answer=$(curl -s -w ' HTTP %{http_code}' -H 'Accept: application/json' -H "Origin: $PLAYER" -X POST "$PLAYER/c/$code/tap" || true)
    ready_in=$(echo "$answer" | grep -o '"ready_in_ms":[0-9]*' | grep -o '[0-9]*$' || true)
    if [ -z "$ready_in" ]; then
      log "H2 tap $i refused: $answer"
      sleep 5
      continue
    fi
    while [ "$(ready_count "$code")" -le "$before" ]; do
      sleep 0.25
      [ $(($(now_ms) - t0)) -gt 180000 ] && { log "H2 tap $i: not ready after 180 s"; break; }
    done
    t1=$(now_ms)
    echo $((t1 - t0 - ready_in)) >> "$OUT/h2.ms"
    [ $((i % 10)) -eq 0 ] && log "H2 $i/$taps taps: $(stats < "$OUT/h2.ms")"
    sleep $((2 + RANDOM % 12))
  done
  curl -fs -o /dev/null -H "Origin: $PLAYER" -d "" "$PLAYER/c/$game/game/stop"
  local result
  result=$(stats < "$OUT/h2.ms")
  local p95=${result#*p95=}; p95=${p95%% *}
  log "H2 clip ready after its window: $result → $([ "$p95" -le 10000 ] && echo PASS || echo MISS) (p95 ≤ 10000 ms)"
}

# ---------------------------------------------------------------- H4: camera resume after a 30 s unplug
h4() {
  local runs=${1:-5} back ms
  : > "$OUT/h4.ms"
  for r in $(seq 1 "$runs"); do
    camera_down 1
    sleep 30
    camera_up 1
    ms=$(wait_recording "${CODES[0]}" 60) || ms=60000
    echo "$ms" >> "$OUT/h4.ms"
    log "H4 run $r: recording again $ms ms after the camera returned"
    sleep 20
  done
  local result p95
  result=$(stats < "$OUT/h4.ms")
  p95=${result#*p95=}; p95=${p95%% *}
  log "H4 camera resume (retina path): $result → $([ "$p95" -le 5000 ] && echo PASS || echo MISS) (≤ 5000 ms)"
}

# ---------------------------------------------------------------- H5: an injected failing update
h5() {
  printf '#!/bin/sh\necho "injected failing release" >&2\nexit 1\n' > "$OUT/broken"
  chmod 755 "$OUT/broken"
  "$BIN" make-update 99.0.0-broken "$OUT/broken" "$OUT/release.pk8" "$OUT/broken.chupdate" >/dev/null
  local before t0 status=""
  before=$(staff "$STAFF/console/settings/update" | grep -o 'data-box-version>[^<]*' | sed 's/.*>//' || true)
  t0=$(now_ms)
  staff -o /dev/null -F "file=@$OUT/broken.chupdate" "$STAFF/console/settings/update"
  for _ in $(seq 1 300); do
    sleep 2
    status=$(staff "$STAFF/console/settings/update" 2>/dev/null | grep -o 'data-update-status>[^<]*' | sed 's/.*>//' || true)
    case "$status" in "Update rolled back"*) break ;; esac
  done
  local after
  after=$(staff "$STAFF/console/settings/update" | grep -o 'data-box-version>[^<]*' | sed 's/.*>//' || true)
  if [ "$after" = "$before" ] && [ "${status#Update rolled back}" != "$status" ]; then
    log "H5 failing update: \"$status\" after $(($(now_ms) - t0)) ms, back on $after → PASS"
  else
    log "H5 failing update: status \"$status\", version $before → $after → MISS"
  fi
  wait_recording "${CODES[0]}" 120 >/dev/null || log "H5: court 1 not recording after the rollback"
}

# ---------------------------------------------------------------- H3: the shot judge on 4 × 1 h games
h3() {
  local minutes=${1:-60} t0 pending
  for n in 0 1 2 3; do curl -fs -o /dev/null -H "Origin: $PLAYER" -d "" "$PLAYER/c/${CODES[$n]}/game/start"; done
  log "H3: 4 games recording for $minutes min"
  sleep $((minutes * 60))
  t0=$(now_ms)
  for n in 0 1 2 3; do curl -fs -o /dev/null -H "Origin: $PLAYER" -d "" "$PLAYER/c/${CODES[$n]}/game/stop"; done
  while :; do
    pending=$(sqlite3 "$OUT/data/box.db" "SELECT count(*) FROM game WHERE highlights IN ('pending', 'running')")
    [ "$pending" -eq 0 ] && break
    sleep 10
  done
  local took=$(($(now_ms) - t0))
  log "H3 judge: 4 × $minutes min games done in $((took / 60000)) min ($(sqlite3 "$OUT/data/box.db" "SELECT group_concat(highlights) FROM game")) → $([ "$took" -le 3600000 ] && echo PASS || echo MISS) (≤ 60 min for 4 × 60 min)"
}

# ---------------------------------------------------------------- H1: recording gaps over the soak
# The box keeps 4 h of footage, so gaps are measured on what is kept, every 30 min, and the worst is reported.
max_gap() {
  sqlite3 "$OUT/data/box.db" "SELECT court_id, start_ms, end_ms FROM segment WHERE end_ms IS NOT NULL ORDER BY court_id, start_ms" |
    awk -F'|' '$1 == c && $2 - e > g {g = $2 - e} {c = $1; e = $3} END {print g + 0}'
}
h1() {
  local hours=${1:-24} worst=0 gap
  local end=$(($(date +%s) + hours * 3600))
  while [ "$(date +%s)" -lt "$end" ]; do
    sleep 1800
    gap=$(max_gap)
    [ "$gap" -gt "$worst" ] && worst=$gap
    log "H1 soak: worst gap so far $worst ms"
  done
  log "H1 recording over ${hours} h: worst gap $worst ms → $([ "$worst" -le 2000 ] && echo PASS || echo MISS) (≤ 2000 ms)"
}

cleanup() {
  for n in 1 2 3 4; do camera_down "$n"; done
  kill "$(cat "$OUT/mediamtx.pid" 2>/dev/null)" 2>/dev/null || true
  kill "$(cat "$OUT/box.pid" 2>/dev/null)" 2>/dev/null || true
}
trap cleanup EXIT

log "spike $MODE on $(uname -n): $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | xargs), $(awk '/MemTotal/ {printf "%d GB", $2/1048576}' /proc/meminfo), $(. /etc/os-release && echo "$PRETTY_NAME"), box $("$BIN" --version)"
FOOTAGE_DIR=$(prepare_footage)
mediamtx "$ROOT/tools/spike/mediamtx.yml" >"$OUT/mediamtx.log" 2>&1 &
echo $! > "$OUT/mediamtx.pid"
sleep 2
for n in 1 2 3 4; do camera_up "$n"; done
start_box
setup_venue
case "$MODE" in
  quick) h2 "${TAPS:-100}"; h4 "${H4_RUNS:-5}"; h5 ;;
  judge) h3 "${ARG:-60}" ;;
  soak) h1 "${ARG:-24}" ;;
  h2) h2 "${ARG:-100}" ;;
  h4) h4 "${ARG:-5}" ;;
  h5) h5 ;;
  *) die "unknown mode $MODE (quick | judge | soak | h2 | h4 | h5)" ;;
esac
log "done: $RESULTS"

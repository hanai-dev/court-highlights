#!/usr/bin/env bash
# Proxy feasibility spike (Phase 2) on the developer laptop — NOT the pre-registered N150 spike.
#   proxy-spike.sh cams              run in its own container: MediaMTX + four synthetic 1080p30 cameras (test-only)
#   proxy-spike.sh measure <host>    run in a CPU-limited container: what the box itself does
set -euo pipefail
mode=${1:?cams|measure}

if [[ $mode == cams ]]; then
  mediamtx /work/tools/spike/mediamtx.yml &
  sleep 2
  for c in 1 2 3 4; do
    ffmpeg -nostdin -loglevel error -re -f lavfi -i "testsrc2=size=1920x1080:rate=30" \
      -c:v libx264 -preset veryfast -tune zerolatency -b:v 4M -g 60 -f rtsp "rtsp://127.0.0.1:8554/court$c" &
  done
  wait
fi

host=${2:?camera host}
cd "$(mktemp -d)"
busy() { awk '/^cpu /{print $2+$3+$4+$5+$6+$7+$8, $5}' /proc/stat; }

# P1 — stream-copy recording of 4 courts for 40 s (no re-encode, no audio).
read -r t0 i0 < <(busy)
for c in 1 2 3 4; do
  timeout -s INT 40 gst-launch-1.0 -e -q rtspsrc location="rtsp://$host:8554/court$c" protocols=tcp latency=200 \
    ! rtph264depay ! h264parse ! mp4mux ! filesink location="court$c.mp4" &
done
wait
read -r t1 i1 < <(busy)
echo "P1 record_4x1080p_40s bytes=$(stat -c %s court*.mp4 | tr '\n' ' ')host_cpu_busy_pct=$(awk -v a="$t0" -v b="$t1" -v i="$i0" -v j="$i1" 'BEGIN{printf "%.1f", 100*(1-(j-i)/(b-a))}')"

# P2 — 30 s clip: logo burned in, wide 1920x1080 + vertical 1080x1920, OpenH264 software encode (CI path).
gst-launch-1.0 -q videotestsrc num-buffers=1 pattern=solid-color foreground-color=0xffffe14d \
  ! video/x-raw,width=240,height=96 ! pngenc ! filesink location=logo.png
dec="filesrc location=court1.mp4 ! qtdemux ! h264parse ! openh264dec ! videoconvert"
enc="gdkpixbufoverlay location=logo.png offset-x=-24 offset-y=-24 ! videoconvert ! openh264enc bitrate=6000000 complexity=low multi-thread=4 ! h264parse ! mp4mux"
for run in 1 2 3 4 5; do
  s0=$(date +%s.%N)
  gst-launch-1.0 -q $dec ! $enc ! filesink location=wide.mp4
  s1=$(date +%s.%N)
  gst-launch-1.0 -q $dec ! videocrop left=656 right=656 ! videoscale ! video/x-raw,width=1080,height=1920 ! $enc ! filesink location=vertical.mp4
  s2=$(date +%s.%N)
  echo "P2 run=$run src_s=$(gst-discoverer-1.0 court1.mp4 | awk '/Duration/{print $2}') wide_s=$(awk -v a="$s0" -v b="$s1" 'BEGIN{printf "%.2f", b-a}') vertical_s=$(awk -v a="$s1" -v b="$s2" 'BEGIN{printf "%.2f", b-a}')"
done
echo "P2 outputs wide=$(stat -c %s wide.mp4) vertical=$(stat -c %s vertical.mp4)"

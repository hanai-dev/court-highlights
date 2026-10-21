#!/bin/sh
# Licence gate for GStreamer (SEC-LICENCE, A-114): every element the box uses comes from an LGPL or permissive
# plugin, and ch-box loads GStreamer as shared libraries (LGPL parts dynamically linked).
# Run inside the build image after `cargo build -p ch-box`: sh tools/gst-licence-check.sh target/debug/ch-box
set -u
BIN=${1:-target/debug/ch-box}
# Elements created in crates/box/src (pipeline strings and ElementFactory::make). VA elements register only where a
# VA device exists, so they are checked through their plugin.
ELEMENTS="appsrc appsink aspectratiocrop capsfilter filesrc filesink gdkpixbufoverlay h264parse identity jpegenc mp4mux qtdemux queue splitmuxsink splitmuxsrc tee videoconvert videorate videoscale openh264dec openh264enc"
PLUGINS="va"
fail=0
check() { # $1 = gst-inspect target, $2 = label
  out=$(gst-inspect-1.0 "$1" 2>/dev/null)
  if [ -z "$out" ]; then echo "MISSING  $2"; fail=1; return; fi
  plugin=$(echo "$out" | awk '/^Plugin Details:/{p=1} p && $1=="Name"{print $2; exit}')
  [ -z "$plugin" ] && plugin=$(echo "$out" | awk '$1=="Name"{print $2; exit}')
  lic=$(echo "$out" | awk '$1=="License"{print $2; exit}')
  case "$lic" in
    LGPL|MIT/X11|BSD|MPL|Proprietary) printf '%-8s %-18s plugin %-14s %s\n' ok "$2" "$plugin" "$lic" ;;
    *) printf '%-8s %-18s plugin %-14s %s\n' FAIL "$2" "$plugin" "$lic"; fail=1 ;;
  esac
}
for e in $ELEMENTS; do check "$e" "$e"; done
for p in $PLUGINS; do check "$p" "($p plugin)"; done
echo
echo "# ch-box shared libraries (GStreamer must be dynamically linked)"
ldd "$BIN" | grep -E 'libgst|libglib|libgobject' | awk '{print "  " $1}'
if ! ldd "$BIN" | grep -q 'libgstreamer-1.0.so'; then echo "FAIL  libgstreamer-1.0 is not a shared library dependency"; fail=1; fi
echo
if [ "$fail" -ne 0 ]; then echo "GST-LICENCE: FAIL"; exit 1; fi
echo "GST-LICENCE: PASS (no GPL plugin among the elements the box uses; GStreamer linked dynamically)"

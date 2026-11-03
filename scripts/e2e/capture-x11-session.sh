#!/usr/bin/env bash
# Capture a QEMU/Xvfb desktop session for evidence.
#
# Usage:
#   ./scripts/e2e/capture-x11-session.sh DISPLAY OUTPUT.mp4 [SECONDS] [WIDTHxHEIGHT]
#
# Example:
#   ./scripts/e2e/capture-x11-session.sh :100 /tmp/hitechos-session.mp4 30 1280x800
set -euo pipefail

if [[ $# -lt 2 || $# -gt 4 ]]; then
  sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
fi

display=$1
output=$2
seconds=${3:-30}
size=${4:-1280x800}

command -v ffmpeg >/dev/null || { echo "ffmpeg is required" >&2; exit 1; }
mkdir -p "$(dirname "$output")"

ffmpeg -hide_banner -loglevel warning -y \
  -f x11grab -video_size "$size" -framerate 10 -i "${display}.0" \
  -t "$seconds" \
  -c:v libx264 -preset veryfast -crf 28 -pix_fmt yuv420p \
  "$output"

ffprobe -v error -show_entries format=duration,size -of default=nw=1 "$output"
echo "capture: $output"

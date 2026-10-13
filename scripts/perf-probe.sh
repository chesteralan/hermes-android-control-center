#!/bin/bash
# M10-T3 performance probe (TESTING.md §6): cold start, idle CPU/RAM, quit cleanliness.
#
# Usage: scripts/perf-probe.sh [path/to/hermes-control-center-binary]
# Defaults to the freshly bundled release .app binary. Requires macOS with
# swiftc (Xcode CLT) and python3. No phone should be attached.
set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/src-tauri/target/release/bundle/macos/Hermes Control Center.app/Contents/MacOS/hermes-control-center}"
PROBE="$TMPDIR/hacc-winprobe"

if [ ! -x "$APP" ]; then
  echo "error: app binary not found at: $APP" >&2
  exit 1
fi

if [ ! -x "$PROBE" ]; then
  echo "compiling window probe..."
  swiftc -O "$ROOT/scripts/winprobe.swift" -o "$PROBE" || exit 1
fi

for run in 1 2 3; do
  pkill -f hermes-control-center 2>/dev/null
  sleep 1
  T0=$(python3 -c 'import time; print(f"{time.time():.3f}")')
  "$APP" >/dev/null 2>&1 &
  PID=$!
  LOOP_MS=$("$PROBE" "$PID")
  T1=$(python3 -c 'import time; print(f"{time.time():.3f}")')
  WALL_MS=$(python3 -c "print(f'{($T1 - $T0) * 1000:.0f}')")
  echo "cold-start run $run: window after ${LOOP_MS} ms (shell wall ${WALL_MS} ms)"

  sleep 15
  echo "  idle-no-device samples (%cpu rss_mb), 2s apart:"
  for _ in 1 2 3 4 5; do
    ps -o %cpu=,rss= -p "$PID" | awk '{ printf "    %s%% %.0f MB\n", $1, $2/1024 }'
    sleep 2
  done

  kill -TERM "$PID" 2>/dev/null
  sleep 4
  APP_LEFT=$(pgrep -f hermes-control-center || true)
  STREAM_LEFT=$(pgrep -f "track-devices|/adb.*logcat" || true)
  echo "  after SIGTERM: app leftovers=${APP_LEFT:-none}; adb stream leftovers=${STREAM_LEFT:-none}"
done

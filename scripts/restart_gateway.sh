#!/usr/bin/env bash
set -euo pipefail
REPO="/home/user/mexius"
LOGDIR="$REPO/run_logs"
mkdir -p "$LOGDIR"
PIDFILE="$LOGDIR/gateway.pid"
if [ -f "$PIDFILE" ]; then
  pid=$(cat "$PIDFILE")
  echo "Killing existing gateway pid=$pid" >&2
  kill -9 "$pid" || true
fi
cd "$REPO"
echo "Building release..."
cargo build --release 2>&1 | tee "$LOGDIR/cargo_build.log"
echo "Starting gateway..."
nohup "$REPO/target/release/mexius" gateway 127.0.0.1:42617 > "$LOGDIR/gateway.log" 2>&1 &
echo $! > "$PIDFILE"
sleep 1
echo "PID: $(cat "$PIDFILE")"
echo "Tail of gateway.log:"
tail -n 200 "$LOGDIR/gateway.log"

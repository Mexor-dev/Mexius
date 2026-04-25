#!/usr/bin/env bash
set -euo pipefail
mkdir -p run_logs
echo "BUILD_START $(date)" > run_logs/build_gateway.txt
cargo build -p mexius-api --release 2>&1 | tee -a run_logs/build_gateway.txt || true
echo BUILD_EXIT:$? >> run_logs/build_gateway.txt
systemctl --user daemon-reload || true
systemctl --user restart herma-gateway || true
sleep 2
systemctl --user status herma-gateway --no-pager -n 40 > run_logs/service_status.txt || true
journalctl --user -u herma-gateway -n 200 --no-pager > run_logs/journal.txt || true
ps aux | grep herma-gateway | grep -v grep > run_logs/ps.txt || true
ls -la target/release/herma-gateway > run_logs/target_list.txt || true
# Capture root HTML and Soul Monitor
curl -sS http://127.0.0.1:18789/ -o run_logs/root.html || true
curl -sS http://127.0.0.1:18789/api/soul_monitor -o run_logs/soul_monitor.json || true

# Run live audit (this can take a while). Output appended to run_logs/live_audit_run.txt
echo "LIVE_AUDIT_START $(date)" > run_logs/live_audit_run.txt
cargo test -p mexius-memory --release -- --nocapture live_audit 2>&1 | tee -a run_logs/live_audit_run.txt || true
echo "LIVE_AUDIT_END $(date)" >> run_logs/live_audit_run.txt

echo "Run logs written to: $(pwd)/run_logs"
ls -la run_logs

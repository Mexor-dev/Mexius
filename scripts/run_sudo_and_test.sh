#!/bin/bash
set -e
REPO="/home/user/herma"
mkdir -p "$REPO/run_logs"
# Record whoami under sudo
printf "anticaz\n" | sudo -S -p "" whoami > "$REPO/run_logs/sudo_check.txt" 2>&1 || true
# Run apt update/install
printf "anticaz\n" | sudo -S -p "" apt-get update > "$REPO/run_logs/install_sudo.log" 2>&1 || true
printf "anticaz\n" | sudo -S -p "" apt-get install -y libssl-dev pkg-config build-essential ca-certificates curl python3 >> "$REPO/run_logs/install_sudo.log" 2>&1 || true
echo "===APT_DONE===" >> "$REPO/run_logs/install_sudo.log"
# Start gateway detached
pkill -f herma-gateway || true
setsid "$REPO/target/release/herma-gateway" gateway 0.0.0.0:42617 > "$REPO/run_logs/gateway_run.log" 2>&1 < /dev/null &
echo $! > "$REPO/run_logs/gateway.pid" || true
sleep 1
# Pairing smoke tests
echo "===STATUS===" > "$REPO/run_logs/pairing_test.log"
curl -sS -D - http://127.0.0.1:42617/api/status >> "$REPO/run_logs/pairing_test.log" 2>&1 || true
echo "===INITIATE===" >> "$REPO/run_logs/pairing_test.log"
curl -sS -X POST http://127.0.0.1:42617/api/pairing/initiate >> "$REPO/run_logs/pairing_test.log" 2>&1 || true
echo "===PAIRCODE===" >> "$REPO/run_logs/pairing_test.log"
curl -sS http://127.0.0.1:42617/pair/code >> "$REPO/run_logs/pairing_test.log" 2>&1 || true
PAIR=$(curl -sS http://127.0.0.1:42617/pair/code | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('pairing_code',''))") || true
echo "===PAIRING_CODE:$PAIR" >> "$REPO/run_logs/pairing_test.log"
echo "===EXCHANGE===" >> "$REPO/run_logs/pairing_test.log"
if [ -n "$PAIR" ]; then
  curl -sS -X POST -H "X-Pairing-Code: $PAIR" http://127.0.0.1:42617/pair >> "$REPO/run_logs/pairing_test.log" 2>&1 || true
else
  echo "NO_PAIR" >> "$REPO/run_logs/pairing_test.log"
fi

echo "===STATUS_AFTER===" >> "$REPO/run_logs/pairing_test.log"
curl -sS -D - http://127.0.0.1:42617/api/status >> "$REPO/run_logs/pairing_test.log" 2>&1 || true

echo DONE

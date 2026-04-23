#!/bin/bash

# Kill old gateway
pkill -f herma-gateway 2>/dev/null || true
sleep 1

# Start new one
export HERMA_ROOT=/home/user/herma
nohup /home/user/herma/target/release/herma-gateway gateway > /home/user/herma/gateway.log 2>&1 &
GW_PID=$!
echo "Gateway started: PID $GW_PID"
sleep 3

echo ""
echo "=== GET /health ==="
curl -s -w "\nHTTP %{http_code}\n" http://127.0.0.1:42617/health

echo ""
echo "=== GET /api/status ==="
curl -s -w "\nHTTP %{http_code}\n" http://127.0.0.1:42617/api/status

echo ""
echo "=== GET /api/cost ==="
curl -s -w "\nHTTP %{http_code}\n" http://127.0.0.1:42617/api/cost

echo ""
echo "=== gateway.log tail ==="
tail -20 /home/user/herma/gateway.log

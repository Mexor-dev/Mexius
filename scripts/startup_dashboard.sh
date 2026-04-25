#!/bin/bash
set -e
# Respect MEXIUS_ROOT; default to ~/mexius
MEXIUS_ROOT="${MEXIUS_ROOT:-$HOME/mexius}"
pkill -f 'mexius start' || true
sleep 1
nohup mexius start > "${MEXIUS_ROOT}/engine.log" 2>&1 &
sleep 2
echo "================ Mexius Status ================"
echo "Port: 18789 (default)"
echo "LanceDB: $(ls "${MEXIUS_ROOT}/lancedb" 2>/dev/null || echo 'not found')"
echo "Useful commands:"
echo "  mexius status"
echo "  mexius start"
echo "  tail -f ${MEXIUS_ROOT}/engine.log"
echo "================================================="

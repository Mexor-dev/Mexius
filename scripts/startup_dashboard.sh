#!/bin/bash
set -e
# Respect HERMA_ROOT; default to previous goldclaw location
HERMA_ROOT="${HERMA_ROOT:-$HOME/goldclaw}"
pkill -f 'goldclaw daemon' || true
sleep 1
nohup goldclaw daemon > "${HERMA_ROOT}/engine.log" 2>&1 &
sleep 2
echo "================ Goldclaw Status ================"
echo "Port: 18789 (default)"
echo "LanceDB: $(ls "${HERMA_ROOT}/lancedb" 2>/dev/null || echo 'not found')"
echo "Useful commands:"
echo "  goldclaw status"
echo "  goldclaw daemon"
echo "  tail -f ${HERMA_ROOT}/engine.log"
echo "================================================="

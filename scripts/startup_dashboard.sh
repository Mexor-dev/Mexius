#!/bin/bash
set -e
pkill -f 'goldclaw daemon' || true
sleep 1
nohup goldclaw daemon > ~/goldclaw/engine.log 2>&1 &
sleep 2
echo "================ Goldclaw Status ================"
echo "Port: 18789 (default)"
echo "LanceDB: $(ls ~/goldclaw/lancedb 2>/dev/null || echo 'not found')"
echo "Useful commands:"
echo "  goldclaw status"
echo "  goldclaw daemon"
echo "  tail -f ~/goldclaw/engine.log"
echo "================================================="

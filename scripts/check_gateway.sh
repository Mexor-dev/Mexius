#!/usr/bin/env bash
set -euo pipefail
BASE="http://127.0.0.1:42617"
echo "HEAD / ->"
curl -I --max-time 5 "$BASE/" || true

echo "\nGET /api/status ->"
curl -s --max-time 5 "$BASE/api/status" || true

echo "\nGET /api/system/health ->"
curl -s --max-time 5 "$BASE/api/system/health" || true

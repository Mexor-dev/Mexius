#!/bin/bash
set -e
cd "$(dirname "$0")/.."
OUTFILE="herma_bin_info.txt"
rm -f "$OUTFILE"
if [ -x ./target/release/herma-gateway ]; then
  ./target/release/herma-gateway --version > "$OUTFILE" 2>&1 || true
  file ./target/release/herma-gateway >> "$OUTFILE" 2>&1 || true
else
  echo "MISSING_BINARY" > "$OUTFILE"
fi
echo "WROTE_BIN_INFO" >&2

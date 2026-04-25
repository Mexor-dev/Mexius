#!/bin/bash
# Verify mexius binary is built and available
MEXIUS_ROOT="${MEXIUS_ROOT:-$HOME/mexius}"
BINARY="${MEXIUS_ROOT}/target/release/mexius"
if [ -f "$BINARY" ]; then
    echo "mexius binary found: $BINARY"
    "$BINARY" --version 2>/dev/null || echo '(no version flag)'
else
    echo "mexius binary NOT found at $BINARY"
    echo "Run: cargo build -p mexius-api --release"
    exit 1
fi

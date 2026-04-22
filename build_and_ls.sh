#!/bin/bash
set -euo pipefail
# Allow overriding the repo root (use HERMA_ROOT to point to /home/user/herma after rename)
HERMA_ROOT="${HERMA_ROOT:-$HOME/herma}"
cd "${HERMA_ROOT}"
cargo clean
cargo build --release --manifest-path "${HERMA_ROOT}/crates/goldclaw-api/Cargo.toml" --bin goldclaw
ls -l "${HERMA_ROOT}/target/release/goldclaw" || true

#!/bin/bash
set -euo pipefail
cd /home/user/goldclaw
cargo clean
cargo build --release --manifest-path crates/goldclaw-api/Cargo.toml --bin goldclaw
ls -l target/release/goldclaw || true

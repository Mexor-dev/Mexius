#!/bin/bash

# Detached gateway launcher for Mexius
MEXIUS_ROOT="${MEXIUS_ROOT:-$HOME/mexius}"

# Prevent port conflicts by killing any previous gateway instance (best-effort)
if command -v fuser >/dev/null 2>&1; then
    fuser -k 42617/tcp >/dev/null 2>&1 || true
else
    pkill -f mexius >/dev/null 2>&1 || true
fi

# Ensure run_logs dir exists
mkdir -p "${MEXIUS_ROOT}/run_logs"

# Ensure a default config exists
if [ ! -f "${MEXIUS_ROOT}/config.toml" ]; then
    cat > "${MEXIUS_ROOT}/config.toml" << 'EOF'
[gateway]
port = 42617
host = "0.0.0.0"

[memory]
database_path = "/home/user/.mexius/lancedb_store"
zero_latency = true

[agent]
framework = "mexius"
foundation = "mexius-core"
EOF
    echo "Wrote default config to ${MEXIUS_ROOT}/config.toml"
fi

nohup "${MEXIUS_ROOT}/target/release/mexius" gateway >"${MEXIUS_ROOT}/run_logs/gateway.log" 2>&1 &
echo "Mexius gateway started (pid=$!)"
exit 0

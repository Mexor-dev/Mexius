#!/bin/bash

# Detached gateway launcher for Herma/Goldclaw
HERMA_ROOT="${HERMA_ROOT:-$HOME/herma}"

# Prevent port conflicts by killing any previous gateway instance (best-effort)
# Prefer using `fuser` to kill the process holding the gateway TCP port so
# the port is guaranteed to be released. Fall back to pkill if fuser is missing.
if command -v fuser >/dev/null 2>&1; then
	fuser -k 42617/tcp >/dev/null 2>&1 || true
else
	pkill -f herma-gateway >/dev/null 2>&1 || true
fi

# Ensure a default config exists so the gateway doesn't run on fallback defaults
if [ ! -f "${HERMA_ROOT}/config.toml" ]; then
	cat > "${HERMA_ROOT}/config.toml" <<'EOF'
[gateway]
port = 42617
host = "0.0.0.0"

[memory]
database_path = "/home/user/.herma/lancedb_store"
zero_latency = true

[agent]
framework = "hermes"
foundation = "zeroclaw"
EOF
	echo "Wrote default config to ${HERMA_ROOT}/config.toml"
fi

nohup "${HERMA_ROOT}/target/release/herma-gateway" gateway >"${HERMA_ROOT}/gateway.log" 2>&1 &
exit 0

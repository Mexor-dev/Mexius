#!/bin/bash

# Detached gateway launcher for Herma/Goldclaw
HERMA_ROOT="${HERMA_ROOT:-$HOME/herma}"
nohup "${HERMA_ROOT}/target/release/herma-gateway" gateway >"${HERMA_ROOT}/gateway.log" 2>&1 &
exit 0

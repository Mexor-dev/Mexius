#!/bin/bash
set -eu

# Herma One-Click Installer
REQUIRED_PACKAGES="libssl-dev pkg-config build-essential ca-certificates curl python3"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "Repo root: $REPO_ROOT"

cd "$REPO_ROOT"

# Ensure we have a Linux-native npm (not Windows /mnt/c) — abort if Windows npm detected
if command -v npm >/dev/null 2>&1; then
  NPM_PATH=$(readlink -f "$(command -v npm)" || command -v npm)
  if echo "$NPM_PATH" | grep -q "/mnt/c/"; then
    echo "Detected Windows-mounted npm at $NPM_PATH. This installer requires a native Linux npm in WSL." >&2
    echo "Please install Node.js (preferably via nvm) inside WSL and re-run this script." >&2
    exit 1
  fi
fi

# If npm is missing, attempt to install nodejs/npm on Debian/Ubuntu
if ! command -v npm >/dev/null 2>&1; then
  if [ -f /etc/debian_version ]; then
    echo "npm not found. Installing nodejs and npm via apt..."
    sudo apt-get update
    sudo apt-get install -y nodejs npm
  else
    echo "npm not found. Please install Node.js inside this environment and re-run the installer." >&2
    exit 1
  fi
fi

# Install system deps on Debian/Ubuntu
if [ -f /etc/debian_version ]; then
  echo "Detected Debian/Ubuntu. Installing dependencies..."
  sudo apt-get update
  sudo apt-get install -y $REQUIRED_PACKAGES
fi

# Install Rust if missing
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust not found. Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  . "$HOME/.cargo/env"
fi

# Build UI (Node)
if [ -d "$REPO_ROOT/web" ]; then
  echo "Building Web UI..."
  pushd "$REPO_ROOT/web" >/dev/null
  if [ -f package-lock.json ]; then
    npm ci
  else
    npm install
  fi
  npm run build
  popd >/dev/null
fi

# Build Gateway (Rust)
echo "Building herma-gateway (Rust release)..."
cargo build -p goldclaw-api --release

# Ensure backend storage directories exist
echo "Initializing Herma storage directories..."
mkdir -p "$HOME/.herma/lancedb_store"

# Locate built binary
BINARY_PATH=$(find "$REPO_ROOT/target/release" -maxdepth 1 -type f -executable -name "herma-gateway" | head -n 1 || true)
if [ -z "$BINARY_PATH" ]; then
  echo "Error: herma-gateway binary not found in target/release. Build may have failed." >&2
  ls -l "$REPO_ROOT/target/release" || true
  exit 1
fi

echo "Discovered binary: $BINARY_PATH"
chmod +x "$BINARY_PATH"

# Optionally install symlink
if [ -w /usr/local/bin ]; then
  sudo ln -sf "$BINARY_PATH" /usr/local/bin/herma-gateway
  echo "Symlinked /usr/local/bin/herma-gateway -> $BINARY_PATH"
else
  echo "No write access to /usr/local/bin. You can run the binary directly: $BINARY_PATH"
fi

echo "Install complete. You can start Herma with:"
echo "  $BINARY_PATH gateway 0.0.0.0:42617"
echo "Or via systemd user service if desired."

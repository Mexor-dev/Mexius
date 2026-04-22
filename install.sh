#!/bin/bash
set -eu

# Goldclaw Master Installer (Dynamic Pathing)
REQUIRED_PACKAGES="libssl-dev pkg-config build-essential ca-certificates curl python3"

# Detect OS and install dependencies
if [ -f /etc/debian_version ]; then
  echo "Detected Debian/Ubuntu. Installing dependencies..."
  sudo apt-get update
  sudo apt-get install -y $REQUIRED_PACKAGES
elif [ -f /etc/redhat-release ]; then
  echo "Detected RedHat/CentOS. Installing dependencies..."
  sudo yum install -y openssl-devel pkgconfig make gcc ca-certificates curl python3
elif [ "$(uname)" = "Darwin" ]; then
  echo "Detected macOS. Installing dependencies..."
  brew install openssl pkg-config coreutils curl python3
else
  echo "Unsupported OS. Please install dependencies manually: $REQUIRED_PACKAGES"
fi

# Install Rust if missing
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust not found. Installing via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  . "$HOME/.cargo/env"
fi


# --- Goldclaw Cold Start Logic ---

# If not in a git repo, clone it
if [ ! -d .git ]; then
  echo "No .git directory found. Cloning Goldclaw..."
  cd ~
  if [ -d Goldclaw ]; then
    echo "Goldclaw directory already exists. Using it."
    cd Goldclaw
    git pull
  else
    git clone https://github.com/Mexor-dev/Goldclaw.git
    cd Goldclaw
  fi
fi

# Detect repo root
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "Repo root: $REPO_ROOT"

# Build
cd "$REPO_ROOT"
echo "Building goldclaw binary..."
cargo build --release --manifest-path "$REPO_ROOT/crates/goldclaw-api/Cargo.toml" --bin goldclaw

# Fail-proof binary discovery (search entire repo root)
BINARY_PATH=$(find "$REPO_ROOT" -type f -name "goldclaw" -executable | head -n 1)
if [ -z "$BINARY_PATH" ] || [ ! -f "$BINARY_PATH" ]; then
  echo "Error: goldclaw binary not found. Build may have failed."
  echo "Current directory: $(pwd)"
  echo "Listing target/release contents:"
  ls -l "$REPO_ROOT/target/release" || echo "(target/release missing)"
  exit 1
fi
echo "Discovered binary: $BINARY_PATH"

# Permission force
chmod +x "$BINARY_PATH"
chmod +x "$REPO_ROOT/install.sh"

# Symlink or PATH logic
if [ -w /usr/local/bin ]; then
  sudo ln -sf "$BINARY_PATH" /usr/local/bin/goldclaw
  echo "Symlinked /usr/local/bin/goldclaw -> $BINARY_PATH"
else
  echo "No write access to /usr/local/bin. Adding target dir to PATH in ~/.bashrc."
  GOLDCLAW_DIR=$(dirname "$BINARY_PATH")
  if ! grep -q "$GOLDCLAW_DIR" ~/.bashrc; then
    echo "export PATH=\"$GOLDCLAW_DIR:\$PATH\"" >> ~/.bashrc
    echo "Added $GOLDCLAW_DIR to PATH in ~/.bashrc."
  fi
fi

# Embedded dashboard build (Rust-Embed)
echo "Embedding WebUI dist folder..."
# (Assume src/webui.rs uses rust-embed and is already set up)


# Enable linger so user services can keep running after logout/terminal close
if command -v loginctl >/dev/null 2>&1; then
  echo "Enabling linger for $(whoami)"
  sudo loginctl enable-linger $(whoami) || true
fi

# Create user-level systemd service for Goldclaw
SERVICE_PATH="$HOME/.config/systemd/user/goldclaw.service"
EXEC_PATH="/usr/local/bin/goldclaw"
if [ ! -f "$EXEC_PATH" ]; then
  EXEC_PATH="$BINARY_PATH"
fi

mkdir -p "$HOME/.config/systemd/user"
echo "Writing user systemd service to $SERVICE_PATH"
cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=Goldclaw Entity (User Service)
After=network.target ollama.service

[Service]
ExecStart=$EXEC_PATH start
Restart=always

[Install]
WantedBy=default.target
EOF

echo "Reloading user systemd daemon and enabling goldclaw.service (user)"
systemctl --user daemon-reload || true
systemctl --user enable --now goldclaw.service || true

# If ollama exists as a user service, enable it at boot
if systemctl --user list-unit-files | grep -q "ollama"; then
  echo "Enabling Ollama user service to start at boot"
  systemctl --user enable --now ollama || true
fi

# Create a Windows boot helper script for users who want WSL priming
if [ ! -f "$REPO_ROOT/setup-windows-boot.ps1" ]; then
  cat > "$REPO_ROOT/setup-windows-boot.ps1" <<'PS'
# setup-windows-boot.ps1
# Creates a scheduled task that runs on Windows startup to prime WSL.
# Run this from an elevated PowerShell prompt.

$Distro = "Ubuntu"
$TaskName = "Goldclaw_Boot"
$Action = "wsl.exe -d $Distro --exec /bin/true"

schtasks /Create /TN $TaskName /TR $Action /SC ONSTART /RU SYSTEM /F
Write-Host "Scheduled task $TaskName created to prime WSL on boot."
PS
fi

echo "Build and install complete. You can start the agent with:"
echo "  goldclaw start"
echo "If you want the agent to run in background and capture logs, run:"
echo "  nohup goldclaw start > /tmp/goldclaw.log 2>&1 &"
echo "Check logs with: tail -n 200 /tmp/goldclaw.log"

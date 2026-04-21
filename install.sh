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

# Detect repo root
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "Repo root: $REPO_ROOT"

# Explicit workspace build
cd "$REPO_ROOT"
echo "Building workspace..."
cargo build --release --workspace --bin goldclaw-api

# Fail-proof binary discovery (search entire repo root)
BIN_PATH=$(find "$REPO_ROOT" -name "goldclaw-api" -type f | head -n 1)
if [ -z "$BIN_PATH" ]; then
  echo "Error: goldclaw-api binary not found. Build may have failed."
  echo "Current directory: $(pwd)"
  echo "Listing target/release contents:"
  ls -l "$REPO_ROOT/target/release" || echo "(target/release missing)"
  exit 1
fi
echo "Discovered binary: $BIN_PATH"

# Permission force
chmod +x "$BIN_PATH"

# Global symlink
sudo ln -sf "$BIN_PATH" /usr/local/bin/goldclaw
echo "Symlinked /usr/local/bin/goldclaw -> $BIN_PATH"
    let ws = base.join("workspace");
    let models = base.join("models");
    fs::create_dir_all(&ws).ok();
    fs::create_dir_all(&models).ok();
}
EOF

rustc goldclaw_init.rs -o goldclaw_init && ./goldclaw_init && rm goldclaw_init.rs goldclaw_init

# Embedded dashboard build (Rust-Embed)
echo "Embedding WebUI dist folder..."
# (Assume src/webui.rs uses rust-embed and is already set up)

# Persistence injection (systemd/launchd)
if [ -d /etc/systemd/system ]; then
  echo "Systemd detected."
  read -p "Install Goldclaw as a systemd service? [y/N] " yn
  if [ "$yn" = "y" ] || [ "$yn" = "Y" ]; then
    sudo tee /etc/systemd/system/goldclaw.service > /dev/null <<SERVICE
[Unit]
Description=Goldclaw AI Engine
After=network.target

[Service]
Type=simple
ExecStart=$HOME/goldclaw/target/release/goldclaw daemon
Restart=always
User=$USER
WorkingDirectory=$HOME/goldclaw

[Install]
WantedBy=multi-user.target
SERVICE
    sudo systemctl daemon-reload
    sudo systemctl enable goldclaw
    sudo systemctl start goldclaw
    echo "Goldclaw service installed and started."
  fi
elif [ "$(uname)" = "Darwin" ]; then
  echo "launchd detected."
  read -p "Install Goldclaw as a launchd service? [y/N] " yn
  if [ "$yn" = "y" ] || [ "$yn" = "Y" ]; then
    cat > ~/Library/LaunchAgents/ai.goldclaw.engine.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>ai.goldclaw.engine</string>
  <key>ProgramArguments</key>
  <array>
    <string>$HOME/goldclaw/target/release/goldclaw</string>
    <string>daemon</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>WorkingDirectory</key>
  <string>$HOME/goldclaw</string>
</dict>
</plist>
PLIST
    launchctl load ~/Library/LaunchAgents/ai.goldclaw.engine.plist
    echo "Goldclaw launchd service installed and loaded."
  fi
else
  echo "No supported service manager detected. Manual start required."
fi

echo "Goldclaw installation complete. Run ./target/release/goldclaw onboard to finish setup."

Mexius Headless / Appliance Mode

This document explains how the installer configures Mexius for headless, always-on operation.

Installer actions performed:
- Enables linger: `sudo loginctl enable-linger $(whoami)` so user services survive terminal close.
- Writes `~/.config/systemd/user/herma-gateway.service` and enables it with `systemctl --user enable --now herma-gateway`.
- If an Ollama service is present, enables it at boot.
- Creates `setup-windows-boot.ps1` to create a Windows Scheduled Task that primes WSL on boot.

Windows Boot:
Run `setup-windows-boot.ps1` with elevated PowerShell to create a scheduled task named `Mexius_Boot` that runs `wsl.exe -d Ubuntu --exec /bin/true` at startup.

WebUI Integration:
The WebUI includes a "System Health" panel showing Ollama status, Hermes loop indicator, and memory usage. A "Reboot Entity" button will POST to `/api/system/reboot` which attempts to restart the `herma-gateway` user systemd service.
Goldclaw Headless / Appliance Mode

This document explains how the installer configures Goldclaw for headless, always-on operation.

Installer actions performed:
- Enables linger: `sudo loginctl enable-linger $(whoami)` so user services survive terminal close.
- Writes `/etc/systemd/system/mexius.service` and enables it with `systemctl enable --now mexius`.
- If an Ollama service is present, enables it at boot.
- Creates `setup-windows-boot.ps1` to create a Windows Scheduled Task that primes WSL on boot.

Windows Boot:
Run `setup-windows-boot.ps1` with elevated PowerShell to create a scheduled task named `Goldclaw_Boot` that runs `wsl.exe -d Ubuntu --exec /bin/true` at startup.

WebUI Integration:
The WebUI includes a "System Health" panel showing Ollama status, Hermes loop indicator, and memory usage. A "Reboot Entity" button will POST to `/api/system/reboot` which attempts to restart the `mexius` systemd service.

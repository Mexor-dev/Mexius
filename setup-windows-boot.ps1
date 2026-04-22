# setup-windows-boot.ps1
# Creates a scheduled task that runs on Windows startup to prime WSL.
# Run this from an elevated PowerShell prompt.

$Distro = "Ubuntu"
$TaskName = "Goldclaw_Boot"
$Action = "wsl.exe -d $Distro --exec /bin/true"

schtasks /Create /TN $TaskName /TR $Action /SC ONSTART /RU SYSTEM /F
Write-Host "Scheduled task $TaskName created to prime WSL on boot."

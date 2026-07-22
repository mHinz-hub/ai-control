# Windows-Deploy nach dem Build (--bundles nsis): jüngsten Installer starten.
$ErrorActionPreference = "Stop"

$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$installer = Get-ChildItem "$dir\src-tauri\target\release\bundle\nsis\ai-control_*-setup.exe" |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1

if (-not $installer) {
  Write-Error "kein Installer gefunden — erst build.sh (Git-Bash) mit nsis-Bundle laufen lassen"
}

Start-Process -Wait $installer.FullName
Write-Output "installiert: $($installer.Name)"

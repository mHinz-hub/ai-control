#!/usr/bin/env bash
# Linux-Deploy nach build.sh: jüngstes Paket-Bundle installieren.
# dnf-Systeme: rpm -Uvh --force (installiert auch dieselbe Version neu),
# apt-Systeme: dpkg -i. Braucht sudo.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUNDLE="$DIR/src-tauri/target/release/bundle"

if command -v dnf >/dev/null; then
  PKG="$(ls -t "$BUNDLE"/rpm/ai-central-*.rpm | head -1)"
  sudo rpm -Uvh --force "$PKG"
elif command -v dpkg >/dev/null; then
  PKG="$(ls -t "$BUNDLE"/deb/ai-central_*.deb | head -1)"
  sudo dpkg -i "$PKG"
else
  echo "weder dnf noch dpkg gefunden" >&2
  exit 1
fi

echo "installiert: $PKG"
echo "Haupt-App übers Tray beenden und neu starten (Migration läuft beim Start)."

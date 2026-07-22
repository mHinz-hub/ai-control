#!/usr/bin/env bash
# macOS-Deploy nach build.sh: das frisch gebaute Bundle nach ~/Applications
# installieren. Das Dev-Bundle wandert danach aus Launch Services heraus in ein
# .noindex-Verzeichnis — Spotlight indiziert .noindex nicht, damit erscheint im
# Starter/Spotlight nur die installierte Kopie.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BUNDLE="$DIR/src-tauri/target/release/bundle/macos"
NOINDEX="$DIR/src-tauri/target/release/bundle/macos.noindex"
TARGET="$HOME/Applications"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

if [[ ! -d "$BUNDLE/ai-control.app" ]]; then
  echo "kein frisches Bundle unter $BUNDLE — erst ./build.sh laufen lassen" >&2
  exit 1
fi

# Eine laufende Instanz hält Dateien im Bundle offen; das Ersetzen ergäbe eine
# halb überschriebene App.
if pgrep -qx ai-control; then
  echo "ai-control läuft — erst beenden, dann erneut deployen" >&2
  exit 1
fi

# Installieren: alte Kopie ganz weg (ditto führt sonst Reste alter Builds
# fort), dann das neue Bundle hin und in Launch Services eintragen.
mkdir -p "$TARGET"
"$LSREGISTER" -u "$TARGET/ai-control.app" || true
rm -rf "$TARGET/ai-control.app"
ditto "$BUNDLE/ai-control.app" "$TARGET/ai-control.app"
"$LSREGISTER" -f "$TARGET/ai-control.app"

# lsregister -u meldet -10814, wenn das Bundle nicht registriert ist —
# erwartetes Ergebnis, kein Fehler.
"$LSREGISTER" -u "$BUNDLE/ai-control.app" || true
rm -rf "$NOINDEX/ai-control.app"
mkdir -p "$NOINDEX"
mv "$BUNDLE/ai-control.app" "$NOINDEX/"
"$LSREGISTER" -u "$NOINDEX/ai-control.app" || true

echo "installiert: $TARGET/ai-control.app"
echo "DMG: $(ls "$DIR"/src-tauri/target/release/bundle/dmg/*.dmg)"

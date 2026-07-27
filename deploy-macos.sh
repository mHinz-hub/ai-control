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

if [[ ! -d "$BUNDLE/ai-central.app" ]]; then
  echo "kein frisches Bundle unter $BUNDLE — erst ./build.sh laufen lassen" >&2
  exit 1
fi

# Installieren: alte Kopie ganz weg (ditto führt sonst Reste alter Builds
# fort), dann das neue Bundle hin und in Launch Services eintragen.
mkdir -p "$TARGET"
"$LSREGISTER" -u "$TARGET/ai-central.app" || true
rm -rf "$TARGET/ai-central.app"
ditto "$BUNDLE/ai-central.app" "$TARGET/ai-central.app"
"$LSREGISTER" -f "$TARGET/ai-central.app"

# lsregister -u meldet -10814, wenn das Bundle nicht registriert ist —
# erwartetes Ergebnis, kein Fehler.
"$LSREGISTER" -u "$BUNDLE/ai-central.app" || true
rm -rf "$NOINDEX/ai-central.app"
mkdir -p "$NOINDEX"
mv "$BUNDLE/ai-central.app" "$NOINDEX/"
"$LSREGISTER" -u "$NOINDEX/ai-central.app" || true

echo "installiert: $TARGET/ai-central.app"
echo "DMG: $(ls "$DIR"/src-tauri/target/release/bundle/dmg/*.dmg)"

#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export CARGO_HOME="$HOME/tools/.cargo"
export RUSTUP_HOME="$HOME/tools/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"

cd "$DIR"
# CI=true: DMG ohne Finder-AppleScript (Fenster-Layout) bauen — das Script
# wäre aus Nicht-GUI-Shells TCC-blockiert.
CI=true npm run tauri build -- --bundles app,dmg "$@"

# Dev-Bundle in ein .noindex-Verzeichnis schieben und aus Launch Services
# austragen — Spotlight indiziert .noindex-Ordner nicht, damit erscheint im
# Starter/Spotlight nur die installierte Kopie unter ~/Applications.
BUNDLE="$DIR/src-tauri/target/release/bundle/macos"
NOINDEX="$DIR/src-tauri/target/release/bundle/macos.noindex"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
# lsregister -u meldet -10814, wenn das Bundle nicht registriert ist —
# erwartetes Ergebnis, kein Fehler.
"$LSREGISTER" -u "$BUNDLE/ai-control.app" || true
rm -rf "$NOINDEX/ai-control.app"
mkdir -p "$NOINDEX"
mv "$BUNDLE/ai-control.app" "$NOINDEX/"
"$LSREGISTER" -u "$NOINDEX/ai-control.app" || true

echo "DMG: $(ls "$DIR"/src-tauri/target/release/bundle/dmg/*.dmg)"

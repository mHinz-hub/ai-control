#!/usr/bin/env bash
# Gemeinsamer Release-Build für macOS/Linux (Windows: unter Git-Bash läuft
# auch dieses Script). Baut nur — ausrollen macht das jeweilige
# deploy-<os>-Script.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export CARGO_HOME="$HOME/tools/.cargo"
export RUSTUP_HOME="$HOME/tools/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"

cd "$DIR"
case "$(uname -s)" in
  Darwin)
    # CI=true: DMG ohne Finder-AppleScript (Fenster-Layout) bauen — das Script
    # wäre aus Nicht-GUI-Shells TCC-blockiert.
    CI=true npm run tauri build -- --bundles app,dmg "$@"
    ;;
  Linux)
    npm run tauri build -- --bundles deb,rpm "$@"
    ;;
  MINGW* | MSYS*)
    npm run tauri build -- --bundles nsis "$@"
    ;;
  *)
    echo "unbekanntes OS: $(uname -s)" >&2
    exit 1
    ;;
esac

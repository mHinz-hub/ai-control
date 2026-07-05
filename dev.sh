#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export CARGO_HOME="$HOME/tools/.cargo"
export RUSTUP_HOME="$HOME/tools/.rustup"
export PATH="$CARGO_HOME/bin:$PATH"

cd "$DIR"
npm run tauri dev -- "$@"

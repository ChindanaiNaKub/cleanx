#!/usr/bin/env bash
# One-command setup for cleanX.
# Gets the pinned Rune tree, builds, and installs cleanx into ~/.local/bin.
set -euo pipefail

APP_DIR=$(cd "$(dirname "$0")/.." && pwd)

bash "$APP_DIR/scripts/rune.sh"

cargo build --release --manifest-path "$APP_DIR/Cargo.toml"
mkdir -p "$HOME/.local/bin"
cp "$APP_DIR/target/release/cleanx" "$HOME/.local/bin/cleanx"

echo "cleanX installed to $HOME/.local/bin/cleanx"

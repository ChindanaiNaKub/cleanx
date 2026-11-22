#!/usr/bin/env bash
# Fetches the pinned Rune tree next to this app. Every build depends on it.
set -euo pipefail

PIN=b39e1b4d326e323e971b86b8f105d3df53d4642c
RUNE_URL=https://github.com/anomalyco/rune.git
APP_DIR=$(cd "$(dirname "$0")/.." && pwd)
RUNE_DIR="$APP_DIR/../rune-src"

# Cargo.toml uses path deps and [patch.crates-io] into ../rune-src.
# The pinned tree must sit next to the app before any build.
if [ ! -d "$RUNE_DIR/.git" ]; then
  git clone --depth 1 --branch dev "$RUNE_URL" "$RUNE_DIR"
fi
if [ "$(git -C "$RUNE_DIR" rev-parse HEAD)" != "$PIN" ]; then
  git -C "$RUNE_DIR" fetch --depth 1 origin "$PIN"
fi
git -C "$RUNE_DIR" checkout --detach "$PIN"

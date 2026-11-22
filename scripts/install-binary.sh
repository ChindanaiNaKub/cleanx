#!/usr/bin/env bash
# Install a prebuilt cleanX binary from the latest GitHub release.
# No Rust toolchain needed. For macOS and Linux only.
set -euo pipefail

REPO="ChindanaiNaKub/cleanx"
BASE="https://github.com/$REPO/releases/latest/download"
DEST="$HOME/.local/bin"

case "$(uname -s)" in
  Linux)  os=linux ;;
  Darwin) os=macos ;;
  *)
    echo "Unsupported OS. Windows users: download the .exe from the Releases page." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64)   arch=x86_64 ;;
  arm64|aarch64)  arch=aarch64 ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$os" = linux ] && [ "$arch" != x86_64 ]; then
  echo "Prebuilt Linux binaries are x86_64 only. Build from source instead." >&2
  exit 1
fi

mkdir -p "$DEST"
asset="cleanx-$os-$arch"
echo "Downloading $asset ..."
curl -fL "$BASE/$asset" -o "$DEST/cleanx"
chmod +x "$DEST/cleanx"

echo "cleanX installed to $DEST/cleanx"
echo "Add $DEST to your PATH if it is not there already."

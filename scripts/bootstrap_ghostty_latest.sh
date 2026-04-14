#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GHOSTTY_DIR="${GHOSTTY_SHELL_GHOSTTY_DIR:-$ROOT_DIR/target/ghostty-upstream/ghostty}"
REPO_URL="${GHOSTTY_SHELL_GHOSTTY_REPO:-https://github.com/ghostty-org/ghostty.git}"

mkdir -p "$(dirname "$GHOSTTY_DIR")"

if [[ ! -d "$GHOSTTY_DIR/.git" ]]; then
  git clone --depth=1 "$REPO_URL" "$GHOSTTY_DIR"
else
  git -C "$GHOSTTY_DIR" fetch --depth=1 origin main
  git -C "$GHOSTTY_DIR" reset --hard origin/main
fi

if ! command -v zig >/dev/null 2>&1; then
  echo "zig is required. Install it first." >&2
  exit 1
fi

if ! xcrun metal -v >/dev/null 2>&1; then
  echo "Metal Toolchain missing. Installing it now..." >&2
  xcodebuild -downloadComponent MetalToolchain
fi

(
  cd "$GHOSTTY_DIR"
  zig build \
    -Dxcframework-target=native \
    -Demit-exe=false \
    -Demit-docs=false \
    -Demit-macos-app=false \
    -Demit-themes=false \
    -Demit-terminfo=false \
    -Demit-termcap=false \
    -Doptimize=ReleaseFast
)

KIT_DIR="$GHOSTTY_DIR/macos/GhosttyKit.xcframework/macos-arm64"
HEADER="$KIT_DIR/Headers/ghostty.h"
LIB="$KIT_DIR/libghostty-fat.a"

if [[ ! -f "$HEADER" || ! -f "$LIB" ]]; then
  echo "GhosttyKit build finished without the expected artifacts." >&2
  exit 1
fi

echo "GhosttyKit ready at $KIT_DIR"

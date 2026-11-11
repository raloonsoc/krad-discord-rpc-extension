#!/bin/sh
# Builds the extension in release mode and installs the binary into bin/.
#
# The binary is copied to a temporary file and moved into place, so the
# installed file always gets a new inode. On macOS, overwriting it in place
# while Krad ADE runs the old one makes the system kill the new process with
# "Code Signature Invalid".
#
# Usage: scripts/install.sh
set -eu

cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
  echo "this extension only ships a darwin_arm64 binary" >&2
  exit 2
fi
if [ "$(uname -m)" != "arm64" ]; then
  echo "this extension only ships a darwin_arm64 binary (this machine: $(uname -m))" >&2
  exit 2
fi

# Build output lives outside the extension folder (see .cargo/config.toml).
target_dir=$(cargo metadata --format-version 1 --no-deps |
  sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')

dest="bin/krad-discord-rpc-darwin-arm64"

cargo build --release

mkdir -p bin
tmp="$dest.tmp.$$"
cp "$target_dir/release/krad-discord-rpc" "$tmp"
mv -f "$tmp" "$dest"
codesign --verify "$dest"

echo "installed $dest"
echo "Reload the Discord RPC extension in Krad ADE to pick up the new binary."

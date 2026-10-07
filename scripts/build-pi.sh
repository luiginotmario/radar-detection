#!/usr/bin/env bash
# builds radar-detection for the raspberry pi (64-bit), with the screen baked in.
# the result lands in build/pi/, next to the script that sets the pi up.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
TARGET=aarch64-unknown-linux-musl

# the screen needs node 22 or newer
node_ok() { node -e 'process.exit(+process.versions.node.split(".")[0] >= 22 ? 0 : 1)' 2>/dev/null; }
if ! node_ok && [ -s "$HOME/.nvm/nvm.sh" ]; then
  . "$HOME/.nvm/nvm.sh"
  nvm use 22 >/dev/null
fi
node_ok || { echo "needs node 22 or newer to build the screen"; exit 1; }

rustup target list --installed | grep -qx "$TARGET" || rustup target add "$TARGET"

echo "→ building the screen"
(cd web && pnpm install --frozen-lockfile --silent && pnpm build >/dev/null)

echo "→ building for the pi"
cargo build --release --target "$TARGET" --quiet

mkdir -p build/pi
cp "target/$TARGET/release/radar-detection" pi/setup.sh build/pi/
echo "done: build/pi/"

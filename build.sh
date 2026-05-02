#!/usr/bin/env bash
# Build the Rust engine to wasm and emit JS bindings into web/pkg/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "wasm-bindgen not found. Install with:"
  echo "  cargo install wasm-bindgen-cli --version 0.2.100"
  exit 1
fi

cd "$ROOT/engine"
cargo build --release --target wasm32-unknown-unknown

wasm-bindgen \
  --target web \
  --out-dir "$ROOT/web/pkg" \
  "$ROOT/engine/target/wasm32-unknown-unknown/release/cotuong_engine.wasm"

echo "Built. Open web/index.html via a static server, e.g.:"
echo "  python3 -m http.server -d $ROOT/web 8000"

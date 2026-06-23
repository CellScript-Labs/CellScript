#!/usr/bin/env bash
# Build the CellScript WASM bundle for the website playground.
#
# Produces cellscript_wasm_bg.wasm + cellscript_wasm.js in
# website/public/wasm/. The .wasm is compiled with size optimisation
# to stay within the RFC's 600KB gzip budget.
#
# Usage:
#   website/scripts/build-wasm.sh
#
# Requires: rustup target wasm32-unknown-unknown, wasm-pack.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$REPO/website/public/wasm"

echo "Building cellscript-wasm (size-optimised release)..."
cd "$REPO"
RUSTFLAGS="-C opt-level=z" wasm-pack build crates/cellscript-wasm \
  --target web \
  --no-default-features \
  --features wasm

echo "Copying bundle to $OUT..."
mkdir -p "$OUT"
cp crates/cellscript-wasm/pkg/cellscript_wasm_bg.wasm "$OUT/"
cp crates/cellscript-wasm/pkg/cellscript_wasm.js "$OUT/"
cp crates/cellscript-wasm/pkg/cellscript_wasm.d.ts "$OUT/"
cp crates/cellscript-wasm/pkg/cellscript_wasm_bg.wasm.d.ts "$OUT/"

RAW=$(wc -c < "$OUT/cellscript_wasm_bg.wasm")
GZIP=$(gzip -c "$OUT/cellscript_wasm_bg.wasm" | wc -c)
echo ""
echo "WASM bundle size:"
echo "  raw:   $RAW bytes ($(( RAW / 1024 )) KB)"
echo "  gzip:  $GZIP bytes ($(( GZIP / 1024 )) KB)"
if [ "$GZIP" -le 614400 ]; then
  echo "  budget: PASS (<= 600 KB gzip)"
else
  echo "  budget: OVER 600 KB gzip — review included code paths"
  exit 1
fi

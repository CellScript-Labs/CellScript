#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CELLFABRIC_DIR="${CELLFABRIC_DIR:-$(cd "$REPO_ROOT/.." && pwd)/CellFabric}"
INPUT="${CELLSCRIPT_CELLFABRIC_INPUT:-examples/token}"
ACTION="${CELLSCRIPT_CELLFABRIC_ACTION:-mint}"
TARGET_PROFILE="${CELLSCRIPT_CELLFABRIC_TARGET_PROFILE:-ckb}"
AUTHOR_LOCK_SCRIPT_HASH="${CELLSCRIPT_CELLFABRIC_AUTHOR_LOCK_SCRIPT_HASH:-0x1111111111111111111111111111111111111111111111111111111111111111}"
NONCE="${CELLSCRIPT_CELLFABRIC_NONCE:-1}"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$REPO_ROOT/target/cellscript-cellfabric-bridge-smoke/$RUN_ID"
ENVELOPE_JSON="$RUN_DIR/cellscript-envelope.json"
SUMMARY_JSON="$RUN_DIR/cellfabric-flow-summary.json"

usage() {
  cat <<'USAGE'
Usage: scripts/cellscript_cellfabric_bridge_smoke.sh

Builds a CellScript CellFabric intent envelope, imports it with the sibling
CellFabric example, submits the signed dummy intent through the strict gateway,
builds a validated bundle, soft-confirms it as non-final, and checks the bridge
contract summary.
USAGE
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 127
  fi
}

run() {
  printf '\n==> %s\n' "$*" >&2
  "$@"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

require_cmd cargo

if [[ ! -f "$CELLFABRIC_DIR/Cargo.toml" ]]; then
  echo "CELLFABRIC_DIR does not point to a CellFabric checkout: $CELLFABRIC_DIR" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
cd "$REPO_ROOT"
run cargo run --locked -p cellscript --bin cellc -- \
  action build "$INPUT" --action "$ACTION" --target-profile "$TARGET_PROFILE" \
  --fabric-intent --output "$ENVELOPE_JSON"
run cargo run --locked --manifest-path "$CELLFABRIC_DIR/Cargo.toml" --example cellscript_flow -- \
  --summary-only "$ENVELOPE_JSON" "$AUTHOR_LOCK_SCRIPT_HASH" "$NONCE" >"$SUMMARY_JSON"
run cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
  --root "$REPO_ROOT" cellfabric-bridge "$ENVELOPE_JSON" "$SUMMARY_JSON"

printf '\nCellScript CellFabric bridge smoke passed.\n'
printf '  Envelope: %s\n' "$ENVELOPE_JSON"
printf '  Flow summary: %s\n' "$SUMMARY_JSON"

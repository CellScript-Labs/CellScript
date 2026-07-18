#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"
if [[ $# -gt 0 ]]; then
    shift
fi

cd "$ROOT_DIR"

if [[ -z "${CELLC_BIN:-}" ]]; then
    TARGET_DIR="${CELLSCRIPT_CELLC_TARGET_DIR:-$ROOT_DIR/target/cellscript-cellc}"
    cargo build --locked --bin cellc --target-dir "$TARGET_DIR" >/dev/null
    export CELLC_BIN="$TARGET_DIR/debug/cellc"
fi

python3 scripts/cellscript_syntax_combo_audit.py "$MODE" "$@"

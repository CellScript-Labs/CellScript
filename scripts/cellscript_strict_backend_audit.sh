#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"
if [[ $# -gt 0 ]]; then
    shift
fi

cd "$ROOT_DIR"
exec cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
    --root "$ROOT_DIR" strict-backend "$MODE" "$@"

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"
if [[ $# -gt 0 ]]; then
    shift
fi

cd "$ROOT_DIR"
python3 scripts/cellscript_strict_backend_audit.py "$MODE" "$@"

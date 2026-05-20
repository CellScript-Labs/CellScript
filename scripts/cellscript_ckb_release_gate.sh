#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"
if [[ $# -gt 0 ]]; then
    shift
fi

case "$MODE" in
    quick)
        exec "$ROOT_DIR/scripts/cellscript_gate.sh" release-quick "$@"
        ;;
    production|full)
        exec "$ROOT_DIR/scripts/cellscript_gate.sh" release "$@"
        ;;
    *)
        printf 'usage: %s [quick|production|full]\n' "$0" >&2
        exit 2
        ;;
esac

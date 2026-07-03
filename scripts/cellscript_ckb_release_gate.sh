#!/usr/bin/env bash
# Legacy entry point that now delegates to the unified gate script.
#
# Historically this file carried its own copies of the trailing-whitespace,
# release-roadmap, CKB-release-doc, and CKB-acceptance-boundary audits, plus
# Novaseal certify and action-builder toolchain checks. Those function bodies
# were unreachable because the `case` block below always `exec`ed to the
# unified gate, and they had drifted out of sync with the live checks in
# `scripts/cellscript_gate.sh`. This file is now a thin shim so there is a
# single source of truth for every release check. The legacy `quick` /
# `production` / `full` mode names are preserved as aliases.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

case "$MODE" in
    quick)
        exec "$ROOT_DIR/scripts/cellscript_gate.sh" release-quick
        ;;
    production|full)
        exec "$ROOT_DIR/scripts/cellscript_gate.sh" release
        ;;
    *)
        printf 'usage: %s [quick|production|full]\n' "$0" >&2
        exit 2
        ;;
esac

#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: scripts/ckb_cellscript_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>] [--compile-only] [--stateful-scenarios] [--production|--bounded]

Runs the Rust-native CellScript CKB acceptance gate. Production mode is the
default and fails closed unless the source tree and pinned CKB checkout are
clean. The compile-only mode verifies compiler artifacts, ELF entry ABI,
public builder contracts, and production evidence structure without claiming
live node readiness.

Options:
  --ckb-repo <path>   Pinned CKB checkout. Defaults to ../ckb.
  --ckb-bin <path>    Existing CKB executable for bounded live runs only.
  --compile-only      Skip local-node transaction execution.
  --stateful-scenarios
                      Execute the complete stateful action recipe matrix.
  --production        Enforce the production gate (default).
  --bounded           Run bounded development evidence without a production claim.
  -h, --help          Show this help.
USAGE
}

args=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --production)
      args+=(--mode production)
      shift
      ;;
    --bounded)
      args+=(--mode bounded)
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      args+=("$1")
      shift
      ;;
  esac
done

exec cargo run --quiet --locked \
  --manifest-path "$REPO_ROOT/Cargo.toml" \
  -p cellscript-tools -- \
  --root "$REPO_ROOT" \
  ckb-acceptance "${args[@]}"

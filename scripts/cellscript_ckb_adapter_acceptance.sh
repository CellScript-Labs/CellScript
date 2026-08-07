#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

default_ckb_repo() {
  local parent grandparent
  parent="$(cd "$REPO_ROOT/.." && pwd)"
  grandparent="$(cd "$REPO_ROOT/../.." && pwd)"
  if [[ -d "$parent/ckb" ]]; then
    printf '%s\n' "$parent/ckb"
  else
    printf '%s\n' "$grandparent/ckb"
  fi
}

CKB_REPO="${CKB_REPO:-$(default_ckb_repo)}"
CKB_BIN="${CKB_BIN:-}"
RUN_ID="$(date +%Y%m%d-%H%M%S)-$$"
RUN_DIR="$REPO_ROOT/target/ckb-cellscript-adapter-acceptance/$RUN_ID"
REPORT_JSON="$RUN_DIR/cellscript-ckb-adapter-acceptance-report.json"
ACTION_PLAN_JSON="$RUN_DIR/action-plan.json"

usage() {
  cat <<'USAGE'
Usage: scripts/cellscript_ckb_adapter_acceptance.sh [--ckb-repo <path>] [--ckb-bin <path>]

Runs a focused local CKB node acceptance check for the CellScript CKB adapter
boundary. This is not a business-flow semantic gate; it proves the adapter path
can produce CKB-facing evidence around action plans, packed transaction shape,
estimate_cycles, and test_tx_pool_accept.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ckb-repo)
      CKB_REPO="${2:?missing value for --ckb-repo}"
      shift 2
      ;;
    --ckb-repo=*)
      CKB_REPO="${1#*=}"
      shift
      ;;
    --ckb-bin)
      CKB_BIN="${2:?missing value for --ckb-bin}"
      shift 2
      ;;
    --ckb-bin=*)
      CKB_BIN="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "missing required command: cargo" >&2
  exit 127
fi
if [[ ! -d "$CKB_REPO" ]]; then
  echo "CKB repo does not exist: $CKB_REPO" >&2
  exit 1
fi
if [[ ! -f "$CKB_REPO/test/template/ckb.toml" ]]; then
  echo "CKB repo does not contain test/template/ckb.toml: $CKB_REPO" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"
cd "$REPO_ROOT"
cargo run --locked -p cellscript --bin cellc -- \
  action build examples/token.cell --action mint_with_authority --json >"$ACTION_PLAN_JSON"
cargo test --locked -p cellscript-ckb-adapter materializes_resolved_action_with_ckb_sdk_transaction_builder -- --test-threads=1
cargo test --locked -p cellscript-ckb-adapter builds_deploy_transaction_with_type_id_code_cell -- --test-threads=1

command=(cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools --
  --root "$REPO_ROOT" ckb-adapter-live
  --ckb-repo "$CKB_REPO"
  --run-dir "$RUN_DIR"
  --action-plan "$ACTION_PLAN_JSON"
  --report "$REPORT_JSON")
if [[ -n "$CKB_BIN" ]]; then
  command+=(--ckb-bin "$CKB_BIN")
fi
"${command[@]}"

echo "CellScript CKB adapter acceptance report: $REPORT_JSON"

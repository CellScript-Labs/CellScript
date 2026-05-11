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
SUMMARY_JSON="$RUN_DIR/cellfabric-import-summary.json"

usage() {
  cat <<'USAGE'
Usage: scripts/cellscript_cellfabric_bridge_smoke.sh

Builds a CellScript CellFabric intent envelope, imports it with the sibling
CellFabric example, and checks the bridge contract summary.

Environment:
  CELLFABRIC_DIR                              Defaults to ../CellFabric.
  CELLSCRIPT_CELLFABRIC_INPUT                Defaults to examples/token.
  CELLSCRIPT_CELLFABRIC_ACTION               Defaults to mint.
  CELLSCRIPT_CELLFABRIC_TARGET_PROFILE       Defaults to ckb.
  CELLSCRIPT_CELLFABRIC_AUTHOR_LOCK_SCRIPT_HASH
                                              Defaults to 0x11...11.
  CELLSCRIPT_CELLFABRIC_NONCE                Defaults to 1.
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
require_cmd python3

if [[ ! -f "$CELLFABRIC_DIR/Cargo.toml" ]]; then
  echo "CELLFABRIC_DIR does not point to a CellFabric checkout: $CELLFABRIC_DIR" >&2
  exit 1
fi

mkdir -p "$RUN_DIR"

cd "$REPO_ROOT"
run cargo run --locked -p cellscript --bin cellc -- \
  action build "$INPUT" \
  --action "$ACTION" \
  --target-profile "$TARGET_PROFILE" \
  --fabric-intent \
  --output "$ENVELOPE_JSON"

run cargo run --locked --manifest-path "$CELLFABRIC_DIR/Cargo.toml" --example cellscript_import -- \
  --summary-only "$ENVELOPE_JSON" "$AUTHOR_LOCK_SCRIPT_HASH" "$NONCE" >"$SUMMARY_JSON"

python3 - "$ENVELOPE_JSON" "$SUMMARY_JSON" <<'PY'
import json
import sys

envelope_path, summary_path = sys.argv[1:]
with open(envelope_path, "r", encoding="utf-8") as handle:
    envelope = json.load(handle)
with open(summary_path, "r", encoding="utf-8") as handle:
    summary = json.load(handle)

expected_schema = "cellscript-cellfabric-intent-envelope-v0.20"
expected_status = "requires-runtime-binding"

checks = [
    (envelope.get("schema") == expected_schema, "envelope schema mismatch"),
    (envelope.get("status") == expected_status, "envelope status mismatch"),
    (summary.get("schema") == expected_schema, "summary schema mismatch"),
    (summary.get("status") == expected_status, "summary status mismatch"),
    (
        summary.get("action_plan_hash_hex") == envelope["source"]["action_plan_hash"],
        "action_plan_hash mismatch",
    ),
    (summary.get("chain_id") == envelope["source"]["target_profile"], "chain_id mismatch"),
    (summary.get("app_namespace") == envelope["source"]["module"], "app_namespace mismatch"),
    (summary.get("action") == envelope["source"]["action"], "action mismatch"),
    (summary.get("payload_format") == "cellscript-action-plan-json-v1", "payload format mismatch"),
    (summary.get("requires_signature") is True, "summary must require signature"),
    (summary.get("submitted") is False, "summary must not claim submission"),
    (summary.get("soft_confirmed") is False, "summary must not claim soft confirmation"),
    (summary.get("l1_final") is False, "summary must not claim L1 finality"),
    (
        isinstance(summary.get("intent_id"), str)
        and summary["intent_id"].startswith("0x")
        and len(summary["intent_id"]) == 66,
        "intent_id must be 0x-prefixed 32-byte hash",
    ),
]

for passed, message in checks:
    if not passed:
        raise SystemExit(message)

print("valid CellScript -> CellFabric bridge smoke summary")
PY

printf '\nCellScript CellFabric bridge smoke passed.\n'
printf '  Envelope: %s\n' "$ENVELOPE_JSON"
printf '  Summary:  %s\n' "$SUMMARY_JSON"

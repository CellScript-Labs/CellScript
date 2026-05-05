#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-quick}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/cellscript-ckb-ecosystem-reuse-gate-target}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

cd "$ROOT_DIR"

GATE_TMP_DIR="$(mktemp -d)"
cleanup_gate_tmp() {
    rm -rf "$GATE_TMP_DIR"
}
trap cleanup_gate_tmp EXIT

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

run_capture() {
    local output="$1"
    shift
    printf '\n==> %s > %s\n' "$*" "$output"
    "$@" >"$output"
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'missing required command: %s\n' "$1" >&2
        exit 127
    fi
}

validate_cli_contract_outputs() {
    local compat_json="$GATE_TMP_DIR/ckb_std_compat.json"
    local action_json="$GATE_TMP_DIR/action_build.json"

    run_capture "$compat_json" cargo run --locked -p cellscript --bin cellc -- ckb-std-compat --json
    run_capture "$action_json" cargo run --locked -p cellscript --bin cellc -- action build examples/token.cell --action mint --json

    run python3 - "$compat_json" "$action_json" <<'PY'
import json
import sys

compat_path, action_path = sys.argv[1:3]
with open(compat_path, "r", encoding="utf-8") as handle:
    compat = json.load(handle)
with open(action_path, "r", encoding="utf-8") as handle:
    action = json.load(handle)

assert compat["status"] == "ok"
assert compat["schema"] == "cellscript-ckb-std-compat-report-v0.19"
assert compat["inline_abi"]["syscalls"]["load_cell_by_field"] == 2081
assert compat["inline_abi"]["syscalls"]["load_witness"] == 2074
assert compat["inline_abi"]["sources"]["group_input"] == ((1 << 56) | 1)
assert compat["inline_abi"]["sources"]["group_output"] == ((1 << 56) | 2)
assert compat["witness_args_policy"]["entry_payload_abi"] == "cellscript-entry-witness-v1"
assert compat["witness_args_policy"]["final_witness_args_owner"] == "adapter"
assert compat["adapter_boundary"]["compiler_core_uses_ckb_sdk_rust"] is False
assert compat["test_evidence"]["script_construction_api"] is True
assert compat["adapter_boundary"]["script_construction"]["packed_type"] == "ckb_types::packed::Script"
assert compat["adapter_boundary"]["script_construction"]["evidence_schema"] == "cellscript-ckb-script-evidence-v0.19"
assert "args_exact_prefix_suffix" in compat["adapter_boundary"]["script_construction"]["supports"]
assert "script_ref_readback" in compat["adapter_boundary"]["script_construction"]["supports"]
assert "explicit_cell_dep_binding" in compat["adapter_boundary"]["script_construction"]["supports"]

assert action["status"] == "ok"
assert action["policy"] == "cellscript-action-builder-plan-v1"
assert action["headless"] is True
assert action["ui_scope"] == "none"
assert action["transaction_draft"]["state"] == "ActionPlan"
assert action["transaction_draft"]["can_submit"] is False
assert action["transaction_draft"]["requires_packed_materialization"] is True
assert action["transaction_draft"]["packed_materialization"]["transaction"] == "ckb_types::packed::Transaction"
assert action["transaction_draft"]["packed_materialization"]["script"] == "ckb_types::packed::Script"
assert action["transaction_draft"]["packed_materialization"]["out_point"] == "ckb_types::packed::OutPoint"
assert action["adapter_contract"]["schema"] == "cellscript-ckb-adapter-contract-v0.19"
assert action["adapter_contract"]["witness_policy"]["default_action_payload_field"] == "input_type"
assert action["adapter_contract"]["witness_policy"]["lock_signature_policy"] == "explicit-adapter-owned-do-not-overwrite"
required_fields = set(action["adapter_contract"]["resolved_tx_required_fields"])
assert {"outputs_data", "cell_deps", "lineage"}.issubset(required_fields)
assert action["adapter_contract"]["acceptance_report_template"]["schema"] == "cellscript-ckb-action-acceptance-report-v0.19"
PY
}

run_quick_gate() {
    require_cmd cargo
    require_cmd git
    require_cmd python3

    run cargo fmt --all --check
    run cargo fmt --manifest-path examples/ckb-sdk-builder/Cargo.toml --check
    run cargo test --locked -p cellscript --test ckb_std_compat -- --test-threads=1
    run cargo test --locked -p cellscript --test cli cellc_action_build_emits_builder_plan_json -- --test-threads=1
    run cargo test --locked -p cellscript --test cli cellc_ckb_std_compat_reports_runtime_boundary -- --test-threads=1
    validate_cli_contract_outputs
    run cargo test --locked -p cellscript-ckb-adapter --all-targets -- --test-threads=1
    run cargo test --manifest-path examples/ckb-sdk-builder/Cargo.toml --locked
    run git diff --check
    run git diff --cached --check
}

run_full_gate() {
    run_quick_gate
    run ./scripts/cellscript_ckb_adapter_acceptance.sh
    run cargo clippy --locked -p cellscript --all-targets -- -D warnings
    run cargo clippy --locked -p cellscript-ckb-adapter --all-targets -- -D warnings
    run cargo clippy --manifest-path examples/ckb-sdk-builder/Cargo.toml --locked --all-targets -- -D warnings
}

case "$MODE" in
    quick)
        run_quick_gate
        ;;
    full)
        run_full_gate
        ;;
    *)
        printf 'usage: %s [quick|full]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\nCellScript CKB ecosystem reuse %s gate passed.\n' "$MODE"

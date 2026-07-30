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

cargo_fmt_workspace() {
    run cargo fmt \
        --manifest-path "$ROOT_DIR/Cargo.toml" \
        --package cellscript \
        --package cellscript-ckb-adapter \
        --package cellscript-fiber-adapter \
        --package cellscript-tools \
        --package cellscript-wasm \
        --package cellscript-ckb-sdk-builder-example \
        "$@"
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
    run_capture "$action_json" cargo run --locked -p cellscript --bin cellc -- action build examples/token.cell --action mint_with_authority --json
    run cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
        --root "$ROOT_DIR" ecosystem-reuse-contracts "$compat_json" "$action_json"
}

run_quick_gate() {
    require_cmd cargo
    require_cmd git

    cargo_fmt_workspace --check
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

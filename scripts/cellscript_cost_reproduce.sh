#!/usr/bin/env bash
# Rebuild the pre-optimization compiler or a bounded attribution control.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_DIR="${1:?usage: cellscript_cost_reproduce.sh NEW_BASELINE_DIR}"
BASE_COMMIT=65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5
STAGE="${2:-baseline}"
case "$STAGE" in
    baseline|immediate) REPLAY_PATCH=baseline-replay.patch ;;
    unshared) REPLAY_PATCH=candidate-source-replay.patch ;;
    *) printf 'unknown cost replay stage: %s (baseline, immediate, unshared)\n' "$STAGE" >&2; exit 1 ;;
esac

if [[ -e "$BASELINE_DIR" ]]; then
    printf 'baseline directory must not exist: %s\n' "$BASELINE_DIR" >&2
    exit 1
fi
BASELINE_PARENT="$(cd "$(dirname "$BASELINE_DIR")" && pwd)"
BASELINE_DIR="$BASELINE_PARENT/$(basename "$BASELINE_DIR")"
if [[ ! -f "$BASELINE_PARENT/ckb-sdk-rust/Cargo.toml" ]]; then
    printf 'baseline needs a sibling ckb-sdk-rust checkout at v5.1.0\n' >&2
    exit 1
fi
SDK_COMMIT="$(git -C "$BASELINE_PARENT/ckb-sdk-rust" rev-parse HEAD)"
if [[ "$SDK_COMMIT" != "$(git -C "$BASELINE_PARENT/ckb-sdk-rust" rev-parse 'v5.1.0^{commit}')" ]]; then
    printf 'baseline sibling ckb-sdk-rust must be pinned to v5.1.0\n' >&2
    exit 1
fi
if [[ -n "$(git -C "$BASELINE_PARENT/ckb-sdk-rust" status --porcelain --untracked-files=all)" ]]; then
    printf 'baseline sibling ckb-sdk-rust must be clean at v5.1.0\n' >&2
    exit 1
fi

git clone --no-hardlinks --no-checkout "$ROOT_DIR" "$BASELINE_DIR"
git -C "$BASELINE_DIR" checkout --detach "$BASE_COMMIT"
git -C "$BASELINE_DIR" apply --unidiff-zero --intent-to-add "$ROOT_DIR/docs/reports/0.31/$REPLAY_PATCH"
if [[ "$STAGE" == immediate ]]; then
    # Only D2 differs from the baseline. Keep the v8 compiler/checker and the
    # identical measurement harness; do not substitute the final combined pass.
    for source in src/codegen/assembler.rs src/codegen/assembler/immediate.rs; do
        mkdir -p "$BASELINE_DIR/$(dirname "$source")"
        git -C "$ROOT_DIR" show "$BASE_COMMIT:$source" > "$BASELINE_DIR/$source"
    done
    git -C "$BASELINE_DIR" add --intent-to-add src/codegen/assembler/immediate.rs
elif [[ "$STAGE" == unshared ]]; then
    # D3 ablation: retain D2, D4, D5 and v9, but emit dedicated adapters using
    # the released policy orchestrator. Only the structural sharing assertion
    # changes; inputs, execution oracles and frozen ceilings remain identical.
    git -C "$ROOT_DIR" show 352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71:src/codegen/policy.rs > "$BASELINE_DIR/src/codegen/policy.rs"
    git -C "$BASELINE_DIR" apply --unidiff-zero "$ROOT_DIR/docs/reports/0.31/unshared-harness.patch"
fi
mkdir -p "$BASELINE_DIR/target/cellscript-cost"
(
    cd "$BASELINE_DIR"
    export CELLSCRIPT_COST_CORPUS_REPORT="$BASELINE_DIR/target/cellscript-cost/$STAGE.json"
    export CELLSCRIPT_MULTI_SCRIPT_COST_REPORT="$BASELINE_DIR/target/cellscript-cost/$STAGE-multi.json"
    # Cargo integration binaries embed CARGO_MANIFEST_DIR. Never share their
    # target directory with another checkout, even at the same package version.
    export CARGO_TARGET_DIR="${CELLSCRIPT_COST_BASELINE_TARGET_DIR:-$BASELINE_DIR/target/build}"
    export CARGO_INCREMENTAL=0
    cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
    cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact
)
printf 'replay reports: %s/target/cellscript-cost/%s{,-multi}.json\n' "$BASELINE_DIR" "$STAGE"

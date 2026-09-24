#!/usr/bin/env bash
# Rebuild the pre-optimization compiler with the same measurement harness.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE_DIR="${1:?usage: cellscript_cost_reproduce.sh NEW_BASELINE_DIR}"
BASE_COMMIT=65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5

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

git clone --no-hardlinks --no-checkout "$ROOT_DIR" "$BASELINE_DIR"
git -C "$BASELINE_DIR" checkout --detach "$BASE_COMMIT"
git -C "$BASELINE_DIR" apply --unidiff-zero --intent-to-add "$ROOT_DIR/docs/reports/0.31/baseline-replay.patch"
mkdir -p "$BASELINE_DIR/target/cellscript-cost"
(
    cd "$BASELINE_DIR"
    export CELLSCRIPT_COST_CORPUS_REPORT="$BASELINE_DIR/target/cellscript-cost/baseline.json"
    export CELLSCRIPT_MULTI_SCRIPT_COST_REPORT="$BASELINE_DIR/target/cellscript-cost/baseline-multi.json"
    # Cargo integration binaries embed CARGO_MANIFEST_DIR. Never share their
    # target directory with another checkout, even at the same package version.
    export CARGO_TARGET_DIR="${CELLSCRIPT_COST_BASELINE_TARGET_DIR:-$BASELINE_DIR/target/build}"
    export CARGO_INCREMENTAL=0
    cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
    cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact
)
printf 'baseline reports: %s/target/cellscript-cost/baseline{,-multi}.json\n' "$BASELINE_DIR"

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-dev}"
if [[ $# -gt 0 ]]; then
    shift
fi

export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CELLSCRIPT_BACKEND_SHAPE_REPORT="${CELLSCRIPT_BACKEND_SHAPE_REPORT:-$ROOT_DIR/target/cellscript-backend-shape/backend-shape-report-$MODE.json}"
export CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT="${CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT:-$ROOT_DIR/target/cellscript-schema-manifest/schema-manifest-report-$MODE.json}"

cd "$ROOT_DIR"
mkdir -p "$(dirname "$CELLSCRIPT_BACKEND_SHAPE_REPORT")"
mkdir -p "$(dirname "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT")"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'missing required command: %s\n' "$1" >&2
        exit 127
    fi
}

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

python_syntax_check() {
    python3 - "$@" <<'PY'
import sys
from pathlib import Path

for raw in sys.argv[1:]:
    path = Path(raw)
    compile(path.read_text(encoding="utf-8"), str(path), "exec")
PY
}

check_trailing_whitespace() {
    local tracked_rust_files=()
    local tracked_rust_file
    while IFS= read -r tracked_rust_file; do
        tracked_rust_files+=("$tracked_rust_file")
    done < <(git ls-files 'src/*.rs' 'src/**/*.rs' 'tests/*.rs' 'tests/**/*.rs')

    local files=(
        ".github/workflows/ci.yml"
        "Cargo.toml"
        "CODING_STYLE.md"
        "README.md"
        "README_CH.md"
        "CHANGELOG.md"
        "docs/README.md"
        "roadmap/CELLSCRIPT_ROADMAP.md"
        "roadmap/CELLSCRIPT_0_13_TODOLIST.md"
        "docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md"
        "docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md"
        "docs/releases/CELLSCRIPT_0_13_2_ACCEPTANCE_COMMUNITY_POST.md"
        "docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md"
        "docs/releases/CELLSCRIPT_0_14_COMMUNITY_UPDATE.md"
        "docs/archive/0.13/CELLSCRIPT_0_13_1_PLAN.md"
        "docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md"
        "docs/CELLSCRIPT_0_14_SUPER_AUDIT.md"
        "docs/CELLSCRIPT_CKB_DEPLOYMENT_MANIFEST.md"
        "docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md"
        "docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md"
        "docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md"
        "docs/CELLSCRIPT_GATE_POLICY.md"
        "docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md"
        "docs/wiki/Home.md"
        "docs/wiki/Tutorial-05-CKB-Target-Profiles.md"
        "docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md"
        "docs/wiki/Tutorial-08-Bundled-Example-Contracts.md"
        "editors/vscode-cellscript/extension.js"
        "editors/vscode-cellscript/package-lock.json"
        "editors/vscode-cellscript/package.json"
        "editors/vscode-cellscript/scripts/validate.mjs"
        "scripts/cellscript_gate.sh"
        "scripts/cellscript_ckb_release_gate.sh"
        "scripts/cellscript_0_14_scope_audit.sh"
        "scripts/cellscript_syntax_combo_audit.sh"
        "scripts/cellscript_syntax_combo_audit.py"
        "scripts/cellscript_strict_backend_audit.sh"
        "scripts/cellscript_strict_backend_audit.py"
        "scripts/ckb_cellscript_acceptance.sh"
        "scripts/validate_cellscript_tooling_release.py"
        "scripts/validate_ckb_cellscript_production_evidence.py"
        "tests/syntax_combo/matrix.toml"
        "tests/syntax_combo/seeds/legacy-transfer-capability.cell"
        "tests/syntax_combo/seeds/require-block-lifecycle.cell"
        "${tracked_rust_files[@]}"
    )
    if ((${#files[@]} > 0)) && rg -n '[ \t]+$' "${files[@]}"; then
        printf '\nTrailing whitespace found in tracked CellScript files.\n' >&2
        exit 1
    fi
}

check_release_roadmap_docs() {
    local required=(
        'roadmap/CELLSCRIPT_ROADMAP.md::0.13.2 syntax-governance hardening'
        'roadmap/CELLSCRIPT_ROADMAP.md::syntax-combination audit'
        'docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md::Stdlib lifecycle and Cell metadata patterns'
        'docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md::./scripts/cellscript_gate.sh release'
        'docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md::./scripts/cellscript_gate.sh ci'
        'roadmap/CELLSCRIPT_0_13_TODOLIST.md::0.13.2 Syntax Governance And Release Hardening'
        'docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md::Syntax Governance And Standard Library'
        'docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md::Release tag'
        'docs/README.md::CellScript Documentation Map'
    )
    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'release roadmap docs are missing required boundary in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done
}

check_ckb_release_docs() {
    local release_doc="docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md"
    local required=(
        "CKB Release Evidence Gate"
        "Syntax-Combination Preflight"
        "Unified Gate Entry Points"
        "syntax-combination audit is a release acceptance preflight"
        "before builder-backed CKB acceptance"
        "./scripts/cellscript_gate.sh release"
        "primitive-strict original bundled-example coverage"
        "builder-backed action runs"
        "source-bound acceptance provenance"
        "occupied-capacity evidence"
        "passed final production hardening gate"
    )
    local pattern
    for pattern in "${required[@]}"; do
        if ! rg --quiet --fixed-strings "$pattern" "$release_doc"; then
            printf 'CKB production-gate docs are missing required boundary: %s\n' "$pattern" >&2
            exit 1
        fi
    done
}

check_ckb_acceptance_boundaries() {
    local required=(
        'scripts/ckb_cellscript_acceptance.sh::Usage: scripts/ckb_cellscript_acceptance.sh'
        'scripts/ckb_cellscript_acceptance.sh::strict-original-ckb'
        'scripts/ckb_cellscript_acceptance.sh::bundled_examples_exact_order'
        'scripts/ckb_cellscript_acceptance.sh::language_examples_exact_order'
        'scripts/ckb_cellscript_acceptance.sh::strict_original_ckb_compile_policy_fail_closed'
        'scripts/ckb_cellscript_acceptance.sh::strict_original_ckb_compile_unexpected_failures'
        'scripts/ckb_cellscript_acceptance.sh::SOURCE_PROVENANCE_SCHEMA'
        'scripts/ckb_cellscript_acceptance.sh::tracked_source_sha256'
        'scripts/ckb_cellscript_acceptance.sh::builder_backed_action_count'
        'scripts/ckb_cellscript_acceptance.sh::final_production_hardening_gate'
        'scripts/validate_ckb_cellscript_production_evidence.py::validate_source_provenance'
        'scripts/validate_ckb_cellscript_production_evidence.py::tracked_source_sha256'
        'scripts/validate_ckb_cellscript_production_evidence.py::valid CKB CellScript'
        'scripts/validate_cellscript_tooling_release.py::valid CellScript tooling release boundary'
    )
    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'CKB acceptance boundary is missing required pattern in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done
}

check_novaseal_acceptance_boundaries() {
    local required=(
        'src/cli/novaseal_certification.rs::stateful_live_acceptance_blockers'
        'src/cli/novaseal_certification.rs::acceptance_blocker_count'
        'src/cli/novaseal_certification.rs::local_blocker_count'
        'src/cli/novaseal_certification.rs::external_endpoint_coverage'
        'src/cli/novaseal_certification.rs::real BTC SPV and Fiber endpoint production acceptance'
        'scripts/novaseal_devnet_stateful_acceptance.sh::acceptance_blocker_count'
        'scripts/novaseal_devnet_stateful_acceptance.sh::local_blocker_count'
        'scripts/novaseal_devnet_stateful_acceptance.sh::acceptance_blockers=%s'
        'proposals/novaseal/DEVNET_FULL_ACCEPTANCE_RUNBOOK.md::acceptance_blockers=0'
        'proposals/novaseal/DEVNET_FULL_ACCEPTANCE_RUNBOOK.md::missing public BTC SPV evidence'
        'docs/releases/CELLSCRIPT_0_16_RELEASE_NOTES_DRAFT.md::acceptance_blockers=0'
        'proposals/novaseal/v0-mvp-skeleton/docs/AUDIT_STATUS.md::acceptance_blockers=0'
        'tests/novaseal_sources.rs::EXPECTED_TRACKED_NOVASEAL_CELL_SOURCES'
        'tests/novaseal_sources.rs::all_novaseal_executable_entries_compile_for_ckb_profile'
    )
    local item file pattern
    for item in "${required[@]}"; do
        file="${item%%::*}"
        pattern="${item#*::}"
        if ! rg --quiet --fixed-strings "$pattern" "$file"; then
            printf 'NovaSeal acceptance boundary is missing required pattern in %s: %s\n' "$file" "$pattern" >&2
            exit 1
        fi
    done
}

check_package_contents() {
    local package_files
    package_files="$(mktemp)"
    printf '\n==> cargo package --list --locked --allow-dirty --offline\n'
    cargo package --list --locked --allow-dirty --offline | tee "$package_files"
    if grep -E '^(\.github/|docs/|editors/|proposals/|tools/|src/bin/|.*__pycache__/|.*\.py[co]$)' "$package_files"; then
        printf 'crates.io package includes repository-only files or unpublished helper binaries\n' >&2
        exit 1
    fi
    rm -f "$package_files"
}

check_script_syntax() {
    local shell_scripts=()
    local shell_script
    while IFS= read -r shell_script; do
        shell_scripts+=("$shell_script")
    done < <(git ls-files '*.sh')
    for shell_script in "${shell_scripts[@]}"; do
        run bash -n "$shell_script"
    done

    local python_scripts=()
    local python_script
    while IFS= read -r python_script; do
        python_scripts+=("$python_script")
    done < <(git ls-files '*.py')
    if ((${#python_scripts[@]} > 0)); then
        run python_syntax_check "${python_scripts[@]}"
    fi
}

check_ckb_tx_measure_tool() {
    local ckb_repo="$ROOT_DIR/../ckb"
    local toolchain=""
    if [[ -f "$ckb_repo/rust-toolchain.toml" ]]; then
        toolchain="$(python3 - "$ckb_repo/rust-toolchain.toml" <<'PY'
import re
import sys
from pathlib import Path

match = re.search(r'channel\s*=\s*"([^"]+)"', Path(sys.argv[1]).read_text(encoding="utf-8"))
if match:
    print(match.group(1))
PY
)"
    fi

    if [[ -n "$toolchain" ]]; then
        run env RUSTUP_TOOLCHAIN="$toolchain" cargo test --manifest-path tools/ckb-tx-measure/Cargo.toml --locked
    else
        run cargo test --manifest-path tools/ckb-tx-measure/Cargo.toml --locked
    fi
}

run_dev_gate() {
    if (($# != 0)); then
        printf 'usage: %s dev\n' "$0" >&2
        exit 2
    fi
    require_cmd cargo
    require_cmd python3
    require_cmd rg

    run cargo fmt --all
    run cargo check --locked -p cellscript --all-targets
    run ./scripts/cellscript_strict_backend_audit.sh quick
    run ./scripts/cellscript_syntax_combo_audit.sh quick
    run git diff --check
}

run_ci_gate() {
    if (($# != 0)); then
        printf 'usage: %s ci\n' "$0" >&2
        exit 2
    fi
    require_cmd cargo
    require_cmd python3
    require_cmd rg

    printf '{"status":"not-generated","reason":"test suite did not reach backend shape report generation"}\n' >"$CELLSCRIPT_BACKEND_SHAPE_REPORT"
    run cargo fmt --all --check
    run cargo test --locked -p cellscript -- --test-threads=1
    run cargo clippy --locked -p cellscript --all-targets -- -D warnings
    run ./scripts/cellscript_strict_backend_audit.sh ci
    check_package_contents
    run cargo package --locked --offline --allow-dirty
    check_script_syntax
    run git diff --check
    check_trailing_whitespace
}

run_backend_gate() {
    if (($# != 0)); then
        printf 'usage: %s backend\n' "$0" >&2
        exit 2
    fi
    require_cmd cargo
    require_cmd python3
    require_cmd rg

    run cargo fmt --all --check
    run cargo check --locked -p cellscript --all-targets
    run cargo test --locked -p cellscript
    run cargo clippy --locked -p cellscript --all-targets -- -D warnings
    run ./scripts/cellscript_strict_backend_audit.sh full
    run git diff --check
}

run_release_auxiliary_checks() {
    require_cmd npm

    run python3 scripts/validate_cellscript_tooling_release.py
    check_script_syntax
    check_trailing_whitespace
    check_release_roadmap_docs
    check_ckb_release_docs
    check_ckb_acceptance_boundaries
    check_novaseal_acceptance_boundaries
    check_ckb_tx_measure_tool
    run npm --prefix editors/vscode-cellscript run validate
    run npm --prefix editors/vscode-cellscript run publish:dry-run
}

run_release_quick_gate() {
    run_ci_gate
    run_release_auxiliary_checks
    run ./scripts/ckb_cellscript_acceptance.sh --compile-only --production "$@"
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
    printf 'CellScript Molecule schema manifest report: %s\n' "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT"
}

run_release_gate() {
    run_ci_gate
    run_release_auxiliary_checks
    run ./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios "$@"
    printf '\nCellScript backend shape report: %s\n' "$CELLSCRIPT_BACKEND_SHAPE_REPORT"
    printf 'CellScript Molecule schema manifest report: %s\n' "$CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT"
}

case "$MODE" in
    dev)
        run_dev_gate "$@"
        ;;
    ci)
        run_ci_gate "$@"
        ;;
    backend)
        run_backend_gate "$@"
        ;;
    release)
        run_release_gate "$@"
        ;;
    release-quick)
        run_release_quick_gate "$@"
        ;;
    *)
        printf 'usage: %s [dev|ci|backend|release]\n' "$0" >&2
        exit 2
        ;;
esac

printf '\nCellScript %s gate passed.\n' "$MODE"

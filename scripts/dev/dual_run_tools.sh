#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

if [[ $# -ne 1 ]]; then
    echo "usage: scripts/dev/dual_run_tools.sh <check-skill-pack|validate-tooling-release>" >&2
    exit 2
fi

tool="$1"
case "$tool" in
    check-skill-pack)
        python_command=(python3 scripts/check_cellscript_skill_pack.py)
        ;;
    validate-tooling-release)
        python_command=(python3 scripts/validate_cellscript_tooling_release.py)
        ;;
    *)
        echo "unknown dual-run tool: $tool" >&2
        exit 2
        ;;
esac
rust_command=(
    cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools --
    --root "$ROOT_DIR" "$tool"
)

python_stdout="$(mktemp)"
python_stderr="$(mktemp)"
rust_stdout="$(mktemp)"
rust_stderr="$(mktemp)"
cleanup() {
    rm -f "$python_stdout" "$python_stderr" "$rust_stdout" "$rust_stderr"
}
trap cleanup EXIT

python_status=0
rust_status=0
(
    cd "$ROOT_DIR"
    "${python_command[@]}"
) >"$python_stdout" 2>"$python_stderr" || python_status=$?
(
    cd "$ROOT_DIR"
    "${rust_command[@]}"
) >"$rust_stdout" 2>"$rust_stderr" || rust_status=$?

if [[ "$python_status" -ne "$rust_status" ]]; then
    printf 'dual-run mismatch (%s): python exit=%s rust exit=%s\n' \
        "$tool" "$python_status" "$rust_status" >&2
    diff -u "$python_stdout" "$rust_stdout" >&2 || true
    printf '%s\n' '--- Python stderr ---' >&2
    cat "$python_stderr" >&2
    printf '%s\n' '--- Rust stderr ---' >&2
    cat "$rust_stderr" >&2
    exit 1
fi

if ! diff -u "$python_stdout" "$rust_stdout" >/dev/null; then
    printf 'dual-run mismatch (%s): stdout differs\n' "$tool" >&2
    diff -u "$python_stdout" "$rust_stdout" >&2 || true
    printf '%s\n' '--- Python stderr ---' >&2
    cat "$python_stderr" >&2
    printf '%s\n' '--- Rust stderr ---' >&2
    cat "$rust_stderr" >&2
    exit 1
fi

cat "$python_stdout"
if [[ "$python_status" -ne 0 ]]; then
    cat "$python_stderr" >&2
fi
exit "$python_status"

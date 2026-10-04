#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
if [[ $# -gt 2 ]]; then
    printf 'usage: examples/zk/run.sh [NEW_OUTPUT_DIR] [SETUP_PACKAGE]\n' >&2
    exit 2
fi
output="${1:-$root/target/zk-walkthrough-$(date +%s)}"
if [[ -e "$output" ]]; then
    printf 'Output already exists; choose a new directory: %s\n' "$output" >&2
    exit 2
fi
command -v node >/dev/null
node --experimental-strip-types -e ''
cargo build --locked --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo build --locked --manifest-path contracts/zk-private-counter/script/Cargo.toml --target riscv64imac-unknown-none-elf --release
args=("$output")
if [[ $# -eq 2 ]]; then args+=("$2"); fi
cargo run --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --example counter-walkthrough -- "${args[@]}"
cargo run --locked -p cellscript --bin cellc -- gen-builder --target typescript \
    --metadata "$output/parent-metadata.json" --output "$output/sdk"
node --experimental-strip-types examples/zk/accept-proof.ts "$output"
printf '\nExamples passed. Read %s/walkthrough-report.json and typescript-report.json\n' "$output"

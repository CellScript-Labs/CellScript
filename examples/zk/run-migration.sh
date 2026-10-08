#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
if [[ $# -ne 1 || -e "$1" ]]; then
    printf 'usage: examples/zk/run-migration.sh NEW_OUTPUT_DIR\n' >&2
    exit 2
fi
output="$1"
mkdir -p "$output"
output="$(cd "$output" && pwd)"
cargo build --locked --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo build --locked --manifest-path contracts/zk-private-counter/script/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test migration
cargo test --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml --test migration
for version in 0 1; do
    cargo run --locked -p cellscript --bin cellc -- gen-builder --target typescript \
        --metadata "contracts/zk-private-counter/target/migration-parent-$version-metadata.json" --output "$output/sdk-$version"
done
npm --prefix examples/zk ci --ignore-scripts --no-audit --no-fund
npm --prefix examples/zk run check
CELLSCRIPT_MIGRATION_SDK="$output" npm --prefix examples/zk run test:migration

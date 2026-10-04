#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"
for pass in a b; do
    CARGO_TARGET_DIR="$root/target/counter-repro/$pass" cargo build --locked \
        --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
    CARGO_TARGET_DIR="$root/target/counter-repro/$pass" cargo build --locked \
        --manifest-path contracts/zk-private-counter/script/Cargo.toml --target riscv64imac-unknown-none-elf --release
done
report_dir="$(mktemp -d "$root/target/counter-repro/report.XXXXXX")"
trap 'rm -rf "$report_dir"' EXIT
cargo run --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml -- \
    check-repro "$root/target/counter-repro/a/riscv64imac-unknown-none-elf/release" \
    "$root/target/counter-repro/b/riscv64imac-unknown-none-elf/release" \
    "$report_dir/reproducibility.json"
cp "$report_dir/reproducibility.json" "$root/contracts/zk-private-counter/target/reproducibility.json"

# Two fresh compiler processes, identical source/profile/fixture deployment.
# Deployment coordinates are deliberately synthetic and are not a live receipt.
for pass in a b; do
    cargo run --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml \
        --example reproduce-parent -- "$root/target/counter-repro/$pass/parent.elf"
done
cmp "$root/target/counter-repro/a/parent.elf" "$root/target/counter-repro/b/parent.elf"
cmp "$root/target/counter-repro/a/parent.json" "$root/target/counter-repro/b/parent.json"
cp "$root/target/counter-repro/a/parent.json" "$root/contracts/zk-private-counter/target/parent-reproducibility.json"

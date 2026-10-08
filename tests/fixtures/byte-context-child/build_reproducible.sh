#!/usr/bin/env bash
set -euo pipefail
fixture_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$fixture_dir/../../.." && pwd)"
cargo_home_dir="${CARGO_HOME:-$HOME/.cargo}"
unit_separator=$'\x1f'
encoded_rustflags="--remap-path-prefix=$repository_root=/cellscript"
encoded_rustflags+="${unit_separator}--remap-path-prefix=$cargo_home_dir=/cargo"
encoded_rustflags+="${unit_separator}-C${unit_separator}strip=symbols"
env -u RUSTFLAGS CARGO_ENCODED_RUSTFLAGS="$encoded_rustflags" CARGO_INCREMENTAL=0 \
    cargo build --locked --release --target riscv64imac-unknown-none-elf \
    --manifest-path "$fixture_dir/Cargo.toml"

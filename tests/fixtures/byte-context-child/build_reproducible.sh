#!/usr/bin/env bash
set -euo pipefail
fixture_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$fixture_dir/../../.." && pwd)"
cargo_home_dir="${CARGO_HOME:-$HOME/.cargo}"
rust_sysroot="$(rustc --print sysroot)"
host_triple="$(rustc -vV | awk '/^host: / { print $2 }')"
rust_objcopy="$rust_sysroot/lib/rustlib/$host_triple/bin/rust-objcopy"
if [[ ! -x "$rust_objcopy" ]]; then
    printf 'rust-objcopy not found; install llvm-tools-preview for the pinned toolchain\n' >&2
    exit 1
fi
unit_separator=$'\x1f'
encoded_rustflags="--remap-path-prefix=$repository_root=/cellscript"
encoded_rustflags+="${unit_separator}--remap-path-prefix=$cargo_home_dir=/cargo"
encoded_rustflags+="${unit_separator}-C${unit_separator}strip=symbols"
env -u RUSTFLAGS CARGO_ENCODED_RUSTFLAGS="$encoded_rustflags" CARGO_INCREMENTAL=0 \
    cargo build --locked --release --target riscv64imac-unknown-none-elf \
    --manifest-path "$fixture_dir/Cargo.toml"
# The pinned toolchain's distributions embed different build paths in LLD's
# non-allocated .comment section. Remove it instead of binding the fixture to
# that packaging detail; executable sections and source pins stay unchanged.
artifact="${CARGO_TARGET_DIR:-$fixture_dir/target}/riscv64imac-unknown-none-elf/release/agoraseal-ckb-context"
"$rust_objcopy" --remove-section=.comment "$artifact"

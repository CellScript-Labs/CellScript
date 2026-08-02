#!/usr/bin/env bash
set -euo pipefail

contract_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$contract_dir/../.." && pwd)"
cargo_home_dir="${CARGO_HOME:-${HOME}/.cargo}"
target_dir="${CARGO_TARGET_DIR:-$contract_dir/target}"
hash_target_dir="${CELLSCRIPT_HASH_TARGET_DIR:-$repository_root/target}"
rust_sysroot="$(rustc --print sysroot)"
host_triple="$(rustc -vV | awk '/^host: / { print $2 }')"
rust_objcopy="$rust_sysroot/lib/rustlib/$host_triple/bin/rust-objcopy"
if [[ ! -x "$rust_objcopy" ]]; then
    printf 'rust-objcopy not found; install llvm-tools-preview for the pinned toolchain\n' >&2
    exit 1
fi

mkdir -p "$target_dir"
target_dir="$(cd "$target_dir" && pwd)"
unit_separator=$'\x1f'
encoded_rustflags="-C${unit_separator}target-feature=+zba,+zbb,+zbc,+zbs"
encoded_rustflags+="${unit_separator}-C${unit_separator}passes=lower-atomic"
encoded_rustflags+="${unit_separator}--remap-path-prefix=$repository_root=/src/cellscript"
encoded_rustflags+="${unit_separator}--remap-path-prefix=$cargo_home_dir=/cargo"

env -u RUSTFLAGS \
    CARGO_ENCODED_RUSTFLAGS="$encoded_rustflags" \
    CARGO_INCREMENTAL=0 \
    CARGO_TARGET_DIR="$target_dir" \
    cargo build \
        --locked \
        --manifest-path "$contract_dir/Cargo.toml" \
        --release \
        --target riscv64imac-unknown-none-elf \
        --features ckb-script \
        --bin cellscript-registry-type-script

artifact="$target_dir/riscv64imac-unknown-none-elf/release/cellscript-registry-type-script"
host_artifact="$artifact.$host_triple.stripped"
"$rust_objcopy" --strip-all "$artifact" "$host_artifact"

release_manifest="$contract_dir/release-manifest.json"
canonical_relative_path="$(sed -n 's/.*"artifact": "\([^"]*\)".*/\1/p' "$release_manifest")"
canonical_artifact="$contract_dir/$canonical_relative_path"
if [[ -z "$canonical_relative_path" || ! -f "$canonical_artifact" ]]; then
    printf 'canonical Registry Type Script artifact is missing: %s\n' "$canonical_artifact" >&2
    exit 1
fi

sha256_hash="$(shasum -a 256 "$canonical_artifact" | awk '{ print $1 }')"
artifact_bytes="$(wc -c < "$canonical_artifact" | tr -d ' ')"
ckb_data_hash="$(CARGO_TARGET_DIR="$hash_target_dir" cargo run --quiet --locked \
    --manifest-path "$contract_dir/Cargo.toml" \
    --features hash-tool \
    --bin cellscript-registry-type-script-hash \
    -- "$canonical_artifact")"
ckb_hash_json="$(printf '{\n  "algorithm": "blake2b-256",\n  "hash": "%s",\n  "input_bytes": %s,\n  "personalization": "ckb-default-hash",\n  "status": "ok"\n}' \
    "$ckb_data_hash" "$artifact_bytes")"
expected_sha256="$(sed -n 's/.*"sha256": "\([0-9a-f]*\)".*/\1/p' "$release_manifest")"
expected_artifact_bytes="$(sed -n 's/.*"artifact_bytes": \([0-9]*\).*/\1/p' "$release_manifest")"
expected_ckb_data_hash="$(sed -n 's/.*"ckb_data_hash": "0x\([0-9a-f]*\)".*/\1/p' "$release_manifest")"
if [[ "$artifact_bytes" != "$expected_artifact_bytes" || "$sha256_hash" != "$expected_sha256" || "$ckb_data_hash" != "$expected_ckb_data_hash" ]]; then
    printf 'Registry Type Script release identity mismatch\n' >&2
    printf 'expected bytes=%s sha256=%s ckb_data_hash=0x%s\n' "$expected_artifact_bytes" "$expected_sha256" "$expected_ckb_data_hash" >&2
    printf 'actual   bytes=%s sha256=%s ckb_data_hash=0x%s\n' "$artifact_bytes" "$sha256_hash" "$ckb_data_hash" >&2
    exit 1
fi

host_sha256="$(shasum -a 256 "$host_artifact" | awk '{ print $1 }')"
if [[ "$host_triple" == "x86_64-unknown-linux-gnu" ]]; then
    if ! cmp -s "$host_artifact" "$canonical_artifact"; then
        printf 'canonical x86_64 Linux rebuild does not match the tracked Registry Type Script artifact\n' >&2
        printf 'expected sha256=%s actual sha256=%s\n' "$sha256_hash" "$host_sha256" >&2
        exit 1
    fi
    printf 'canonical_rebuild=matched\n'
else
    printf 'canonical_rebuild=not_claimed host=%s host_sha256=%s\n' "$host_triple" "$host_sha256"
fi

# Downstream tools always execute the exact tracked deployable bytes. A
# non-canonical host build is retained beside this path for inspection.
cp "$canonical_artifact" "$artifact"

printf 'artifact=%s\n' "$artifact"
printf 'artifact_bytes=%s\n' "$artifact_bytes"
printf 'sha256=%s\n' "$sha256_hash"
printf '%s\n' "$ckb_hash_json"

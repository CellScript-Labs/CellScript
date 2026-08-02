#!/usr/bin/env bash
set -euo pipefail

contract_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd "$contract_dir/../.." && pwd)"
canonical_target_dir="${CARGO_TARGET_DIR:-$contract_dir/target/canonical-container}"
builder_image="rust@sha256:77fac8b98f9f46062bb680b6d25d5bcaabfc400143952ebc572e924bcbedc3fa"

mkdir -p "$canonical_target_dir"
canonical_target_dir="$(cd "$canonical_target_dir" && pwd)"

docker run --rm --platform linux/amd64 \
    --user "$(id -u):$(id -g)" \
    --env CARGO_HOME=/contract-target/cargo-home \
    --env RUSTUP_HOME=/usr/local/rustup \
    --mount "type=bind,src=$repository_root,dst=/workspace,readonly" \
    --mount "type=bind,src=$canonical_target_dir,dst=/contract-target" \
    --workdir /workspace \
    "$builder_image" \
    bash -c '
        set -euo pipefail
        toolchain_bin=/usr/local/rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin
        export PATH="$toolchain_bin:/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        export RUSTC="$toolchain_bin/rustc"
        export LD_LIBRARY_PATH="$toolchain_bin/../lib"
        test "$(rustc -vV | sed -n "s/^host: //p")" = x86_64-unknown-linux-gnu
        test -x "$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-objcopy"
        rustc --print target-libdir --target riscv64imac-unknown-none-elf >/dev/null
        CARGO_TARGET_DIR=/contract-target \
            CELLSCRIPT_HASH_TARGET_DIR=/contract-target/cellc \
            /workspace/contracts/registry-type-script/build_reproducible_release.sh
    '

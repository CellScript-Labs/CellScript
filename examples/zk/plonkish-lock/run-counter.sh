#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
work="${1:-$root/target/zk-counter-plonkish}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"

ptau_url=https://www.trusted-setup-pse.org/ptau/pot19_final.ptau
ptau_sha256=3f428d1a407e4704ef906960e000b03089e5e6ec29bf65b07bb5e3de005f4700
ptau="$work/ptau19.ptau"
if [[ ! -f "$ptau" ]]; then
    curl --fail --location --retry 2 "$ptau_url" --output "$ptau.download"
    printf '%s  %s\n' "$ptau_sha256" "$ptau.download" | sha256sum -c
    mv "$ptau.download" "$ptau"
fi
printf '%s  %s\n' "$ptau_sha256" "$ptau" | sha256sum -c

mkdir -p "$work/node" "$work/groth16" "$work/plonk"
cp "$here/counter/"{package.json,package-lock.json} "$work/node/"
npm ci --prefix "$work/node" --ignore-scripts --no-audit --no-fund
snarkjs="$work/node/node_modules/snarkjs/cli.js"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"
mkdir -p "$CARGO_TARGET_DIR"
CARGO_TARGET_DIR="$(cd "$CARGO_TARGET_DIR" && pwd)"
cargo +1.97.1 build --locked --manifest-path "$root/Cargo.toml" -p cellscript --bin cellc
cargo +1.97.1 build --locked --manifest-path "$here/Cargo.toml" --features counter --bin zk-counter-experiment
runner="$CARGO_TARGET_DIR/debug/zk-counter-experiment"
"$runner" export "$work"

target=riscv64imac-unknown-none-elf
vm_flags="-C target-feature=+zba,+zbb,+zbc,+zbs -C passes=lower-atomic -C strip=debuginfo"
(
    export RUSTFLAGS="$vm_flags"
    export TARGET_CC="${TARGET_CC:-clang}"
    export TARGET_AR="${TARGET_AR:-llvm-ar}"
    lifecycle="$root/contracts/zk-private-counter/script/Cargo.toml"
    cargo +1.97.1 build --locked --manifest-path "$lifecycle" --target "$target" --release --bin cellscript-counter-lifecycle
    "${LLVM_OBJCOPY:-llvm-objcopy}" --strip-all "$CARGO_TARGET_DIR/$target/release/cellscript-counter-lifecycle" "$work/lifecycle.elf"

    for scheme in groth16 plonk; do
        feature=vm
        if [[ "$scheme" == groth16 ]]; then feature=counter-groth16; fi
        cargo +1.97.1 build --locked --manifest-path "$here/Cargo.toml" --target "$target" --release \
            --no-default-features --features "$feature" --bin zk-counter-verifier
        "${LLVM_OBJCOPY:-llvm-objcopy}" --strip-all "$CARGO_TARGET_DIR/$target/release/zk-counter-verifier" "$work/$scheme/child.elf"
    done
)

for scheme in groth16 plonk; do
    dir="$work/$scheme"
    node "$snarkjs" "$scheme" setup "$work/counter.r1cs" "$ptau" "$dir/fixture.zkey" | tee "$dir/setup.log"
    node "$snarkjs" zkey export verificationkey "$dir/fixture.zkey" "$dir/vk.json"
done
"$runner" prepare "$work" "$CARGO_TARGET_DIR/debug/cellc"

TIMEFORMAT='%3R'
for scheme in groth16 plonk; do
    for fixture in increment-0-1 increment-1-2 changed-raw-tx; do
        dir="$work/$scheme"
        file="$dir/$fixture"
        node "$snarkjs" wtns check "$work/counter.r1cs" "$file.wtns"
        { time node "$snarkjs" "$scheme" prove "$dir/fixture.zkey" "$file.wtns" "$file.proof.json" "$file.public.json" >&3 2>&3; } \
            3>&2 2> "$file.prove-seconds.txt"
        node "$snarkjs" "$scheme" verify "$dir/vk.json" "$file.public.json" "$file.proof.json"
    done
done

report="$(mktemp "$work/report.XXXXXX")"
trap 'rm -f "$report"' EXIT
"$runner" verify "$work" > "$report"
jq --arg ptau_url "$ptau_url" --arg ptau_sha256 "$ptau_sha256" --arg rust "$(rustc +1.97.1 --version)" \
    --arg node "$(node --version)" --arg compiler_revision "$(git -C "$root" rev-parse HEAD)" \
    --arg cpu "$(node -p 'require("node:os").cpus()[0]?.model ?? "unknown"')" \
    --argjson available_parallelism "$(node -p 'require("node:os").availableParallelism()')" \
    --arg r1cs_file_sha256 "$(sha256sum "$work/counter.r1cs" | cut -d ' ' -f 1)" --arg vm_flags "$vm_flags" \
    --argjson plonk_gates "$(sed -n 's/.*Plonk constraints: \([0-9]*\).*/\1/p' "$work/plonk/setup.log")" \
    --argjson groth_first "$(cat "$work/groth16/increment-0-1.prove-seconds.txt")" \
    --argjson groth_second "$(cat "$work/groth16/increment-1-2.prove-seconds.txt")" \
    --argjson groth_changed "$(cat "$work/groth16/changed-raw-tx.prove-seconds.txt")" \
    --argjson plonk_first "$(cat "$work/plonk/increment-0-1.prove-seconds.txt")" \
    --argjson plonk_second "$(cat "$work/plonk/increment-1-2.prove-seconds.txt")" \
    --argjson plonk_changed "$(cat "$work/plonk/changed-raw-tx.prove-seconds.txt")" \
    '. + {
        scope: "local CKB-VM research fixture with public witness and development setup",
        ptau_url: $ptau_url, ptau_sha256: $ptau_sha256, rust: $rust, node: $node, snarkjs: "0.7.5",
        cellscript_git_revision: $compiler_revision,
        mosaic: "58f0efd3af09dfdede020d570c0be419627e95fa",
        groth16_verifier_core: "d64c769ffe2d2edb5eb308dc59058efda77c2f83",
        cpu: $cpu, available_parallelism: $available_parallelism,
        r1cs_file_sha256: $r1cs_file_sha256, plonk_gates: $plonk_gates, riscv_flags: $vm_flags,
        proof_timing_schedule: "serial proof commands on a shared host",
        proof_command_wall_seconds: {
            groth16: {"increment-0-1": $groth_first, "increment-1-2": $groth_second, "changed-raw-tx": $groth_changed},
            plonk: {"increment-0-1": $plonk_first, "increment-1-2": $plonk_second, "changed-raw-tx": $plonk_changed}
        }
    }' "$report" > "$work/result.json"
cat "$work/result.json"

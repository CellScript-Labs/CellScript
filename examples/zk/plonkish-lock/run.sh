#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
work="${1:-$root/target/zk-lock-plonkish}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"
upstream="$work/zk-Lock"
lock_rev=79d17c2fb4af1dfd68471ec5c564e6562a0aae32
mosaic_rev=58f0efd3af09dfdede020d570c0be419627e95fa

if [[ ! -d "$upstream/.git" ]]; then
    git init --quiet "$upstream"
fi
if ! git -C "$upstream" rev-parse --verify HEAD >/dev/null 2>&1; then
    git -C "$upstream" fetch --quiet --depth 1 https://github.com/CECILIA-MULANDI/zk-Lock.git refs/heads/main
    if ! git -C "$upstream" cat-file -e "$lock_rev^{commit}" 2>/dev/null; then
        git -C "$upstream" fetch --quiet --depth 1 https://github.com/CECILIA-MULANDI/zk-Lock.git "$lock_rev"
    fi
    git -C "$upstream" checkout --quiet --detach "$lock_rev"
fi
[[ "$(git -C "$upstream" rev-parse HEAD)" == "$lock_rev" ]]
git -C "$upstream" diff --quiet HEAD

circuit="$work/circuit"
mkdir -p "$circuit" "$work/groth16" "$work/plonk"
cp "$upstream/circuits/poseidon-preimage-bound/"{circuit.circom,package.json} "$circuit/"
sed 's#https://registry.npmmirror.com/#https://registry.npmjs.org/#g' \
    "$upstream/circuits/poseidon-preimage-bound/package-lock.json" > "$circuit/package-lock.json"
ptau="$upstream/circuits/poseidon-preimage/ptau/powersOfTau28_hez_final_12.ptau"
printf '%s  %s\n' ded2694169b7b08e898f736d5de95af87c3f1a64594013351b1a796dbee393bd825f88f9468c84505ddd11eb0b1465ac9b43b9064aa8ec97f2b73e04758b8a4a "$ptau" | b2sum -c

(
    cd "$circuit"
    npm ci --ignore-scripts
    npm exec --yes --package=circom2@0.2.22 -- circom2 --version > "$work/circom-version.txt"
    npm exec --yes --package=circom2@0.2.22 -- circom2 circuit.circom --r1cs --wasm -l node_modules
)
snarkjs="$circuit/node_modules/snarkjs/cli.js"
for scheme in groth16 plonk; do
    dir="$work/$scheme"
    node "$snarkjs" "$scheme" setup "$circuit/circuit.r1cs" "$ptau" "$dir/fixture.zkey"
    node "$snarkjs" zkey export verificationkey "$dir/fixture.zkey" "$dir/vk.json"
done

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$work/build}"
vm_flags="-C target-feature=+zba,+zbb,+zbc,+zbs -C passes=lower-atomic -C strip=debuginfo"
(
    export RUSTFLAGS="$vm_flags"
    export TARGET_CC="${TARGET_CC:-clang}"
    export TARGET_AR="${TARGET_AR:-llvm-ar}"
    cargo +1.97.1 build --locked --manifest-path "$upstream/Cargo.toml" --target riscv64imac-unknown-none-elf --release -p zk-lock-bound
    cargo +1.97.1 build --locked --manifest-path "$here/Cargo.toml" --target riscv64imac-unknown-none-elf --release \
        --no-default-features --features vm --bin zk-lock-plonkish
)
"${LLVM_OBJCOPY:-llvm-objcopy}" --strip-all "$CARGO_TARGET_DIR/riscv64imac-unknown-none-elf/release/zk-lock-bound" "$work/groth16/lock.elf"
"${LLVM_OBJCOPY:-llvm-objcopy}" --strip-all "$CARGO_TARGET_DIR/riscv64imac-unknown-none-elf/release/zk-lock-plonkish" "$work/plonk/lock.elf"
cargo +1.97.1 build --locked --manifest-path "$here/Cargo.toml" --bin zk-lock-experiment
runner="$CARGO_TARGET_DIR/debug/zk-lock-experiment"
"$runner" prepare "$work"

for scheme in groth16 plonk; do
    dir="$work/$scheme"
    for fixture in original changed; do
        file="$dir/$fixture"
        node "$snarkjs" wtns calculate "$circuit/circuit_js/circuit.wasm" "$file.input.json" "$file.wtns"
        : > "$file.prove-seconds.txt"
    done
done

TIMEFORMAT='%3R'
for repetition in 1 2 3; do
    for fixture in original changed; do
        for scheme in groth16 plonk; do
            dir="$work/$scheme"
            file="$dir/$fixture"
            { time node "$snarkjs" "$scheme" prove "$dir/fixture.zkey" "$file.wtns" "$file.proof.json" "$file.public.json" >&3 2>&3; } \
                3>&2 2>> "$file.prove-seconds.txt"
        done
    done
done

for scheme in groth16 plonk; do
    dir="$work/$scheme"
    for fixture in original changed; do
        file="$dir/$fixture"
        node "$snarkjs" "$scheme" verify "$dir/vk.json" "$file.public.json" "$file.proof.json"
    done
done

timings() {
    jq -nR '[inputs | tonumber] | select(length == 3)' < "$1"
}

report="$(mktemp "$work/report.XXXXXX")"
trap 'rm -f "$report"' EXIT
"$runner" verify "$work" > "$report"
jq --arg lock_rev "$lock_rev" --arg mosaic_rev "$mosaic_rev" --arg rust "$(rustc +1.97.1 --version)" \
    --arg node "$(node --version)" --arg circom "$(tail -n 1 "$work/circom-version.txt")" \
    --arg cpu "$(node -p 'require("node:os").cpus()[0]?.model ?? "unknown"')" \
    --argjson available_parallelism "$(node -p 'require("node:os").availableParallelism()')" \
    --arg r1cs_sha256 "$(sha256sum "$circuit/circuit.r1cs" | cut -d ' ' -f 1)" --arg vm_flags "$vm_flags" \
    --argjson groth_original "$(timings "$work/groth16/original.prove-seconds.txt")" \
    --argjson groth_changed "$(timings "$work/groth16/changed.prove-seconds.txt")" \
    --argjson plonk_original "$(timings "$work/plonk/original.prove-seconds.txt")" \
    --argjson plonk_changed "$(timings "$work/plonk/changed.prove-seconds.txt")" \
    '. + {upstream_zk_lock: $lock_rev, mosaic: $mosaic_rev, rust: $rust, node: $node, circom: $circom, snarkjs: "0.7.5",
          cpu: $cpu, available_parallelism: $available_parallelism,
          r1cs_sha256: $r1cs_sha256, riscv_flags: $vm_flags, elf_strip: "llvm-objcopy --strip-all",
          scope: "local CKB-VM fixture with public demo witness and demo setup",
          proof_command_wall_seconds: {groth16: {original: $groth_original, changed: $groth_changed},
                                       plonk: {original: $plonk_original, changed: $plonk_changed}}}' "$report" > "$work/result.json"
cat "$work/result.json"

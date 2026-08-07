# CellScript Registry Type Script

Canonical Type Script for mainnet Registry commitment Cells. It accepts only a
32-byte custody Lock Script hash in `args` and exact 39-byte Cell data:

```text
"CSREGv1" || ckb_blake2b_256(canonical commitment JSON)
```

The Script validates the data and custody Lock of every input and output in its
Type Script group. It also requires every creation, replacement, or destruction
transaction to consume at least one Cell whose Lock Script hash equals `args`.
Creating an output locked to the Registry therefore cannot impersonate an
official commitment: the transaction must exercise the Registry custody Lock.
The Script deliberately does not interpret off-chain JSON; the Registry API
binds the 32-byte hash to accepted release and deployment evidence and
revalidates live Cells independently.

The custody requirement is the sole on-chain authority boundary. With the
currently pinned standard sighash Lock, its one signer can create, replace, or
destroy commitment Cells; this Type Script adds no multisig, timelock, or
separate revocation path. A custody-key rotation changes the Lock Script hash
in Type args and therefore creates a new Registry Type Script identity. The
operator runbook and compromise procedure are documented under “Commitment
custody boundary and incident response” in `services/registry-api/README.md`.

Production uses the standard mainnet `secp256k1_blake160_sighash_all` genesis
Script for custody. Type Script args are the CKB Script hash of that complete
custody Script, including its 20-byte signer args. The Registry Type Script is
immutable at the data-hash layer unless a reviewed deployment explicitly
chooses a Type ID code Cell.

Build and test with the pinned repository toolchain:

```bash
contracts/registry-type-script/build_reproducible_release.sh
cargo test --locked --manifest-path contracts/registry-type-script/Cargo.toml
```

Reproduce the canonical Linux artifact with the pinned container digest:

```bash
contracts/registry-type-script/build_canonical_container.sh
```

The deployable artifact is tracked under `artifacts/v0.22.0` and was produced
for the `x86_64-unknown-linux-gnu` host with the builder image digest recorded
in `release-manifest.json`. Rust/LLVM may order identical RISC-V functions
differently on another build host, so the script claims a byte-for-byte
reproduction only on that canonical host. On every other host it still builds
the source, reports the host artifact hash, verifies the tracked canonical
identity, and places the canonical bytes at the normal target path for
downstream tooling. The CKB-VM suite always executes those deployable bytes.

The build disables `ckb-std` default features and enables only its Rust
allocator. Fixed-size data, Script, and lock-hash buffers call the official
syscall layer directly; the contract does not carry the higher-level Molecule
type graph or depend on a host C compiler and bundled `libc.c`.
The small host-side hash utility in the same crate computes CKB's personalized
Blake2b-256 identity without depending on the root compiler workspace or a
sibling SDK checkout.

The test suite executes the stripped RISC-V binary in CKB-VM through
`ckb-testtool`, covering authorized creation, replacement, destruction,
unauthorized creation, incorrect custody Locks, malformed data, and
non-canonical Script args.

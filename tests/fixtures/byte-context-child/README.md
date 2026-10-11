# Pinned byte-context child

`src/main.rs`, `Cargo.toml` and `Cargo.lock` are copied from AgoraSeal
commit `5b6106a7b86d51b736a7df772ded548fba4c9a4e`, directory
`verifiers/ckb-context`, with Rustfmt-only normalization of the Rust source.
The build manifest retains both upstream and normalized digests. The included
upstream MIT license applies to those
files. `build-manifest.json` binds their SHA-256 digests and the 15,216-byte
stripped ELF represented by `child.hex`. Its CKB data hash is
`506c04cb4453f94335f500757cb4b93ae26f25a782f033e476cbf4cc55601861`.

The child parses four exact hex argv segments (36, 8, 8, 0 bytes), checks the
first raw direct CellDep OutPoint, rejects duplicate dependencies and checks
both claimed creation heights against their resolved CellDep headers. It does
not decide voting, ownership, DAO eligibility or relative height policy.

`build_reproducible.sh` uses Rust 1.97.1, the locked dependencies and
`riscv64imac-unknown-none-elf`, normalizes repository/cache paths and strips
symbols and the non-allocated `.comment` section with the pinned toolchain's
`rust-objcopy`. This removes distribution-specific LLD build paths; normalizing
the previous pinned ELF and a fresh rebuild produces identical bytes. The unified dev/backend/CI gates build into two separate target
directories, compare them and make the Rust acceptance tests compare the fresh
ELF with the pinned fixture. Compiler/build identity is separate from child
source identity; neither supplies the pending upstream security review.

`budgets.json` freezes ELF bytes, measured group cycles and separate observed
parent/child stack ceilings for the three explicit acceptance vectors at
O0–O3. These are fixture ceilings, not a worst-case proof for arbitrary
transactions, and do not modify the existing compiler/Rust cost corpus budgets.
The scheduler replay must match the ordinary verifier's exit and cycles.

Run the normal development or backend gate for fresh fixture validation.
Focused debugging uses `cargo test --locked -p cellscript --test bounded_byte_context`;
the detailed replay is written to `target/byte-context/measurements.json`.
The local test creates explicit mock dependency/header provenance in
`ckb-testtool`; it does not establish public-chain deployment or header validity.

See the [fixed encoding example](../../../examples/bounded-byte-context/README.md)
and the [official CKB Spawn documentation](https://docs.nervos.org/docs/script/spawn-cross-script-calling).

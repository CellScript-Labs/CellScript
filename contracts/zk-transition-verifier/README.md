# Exact ZK child research

This is unfinished #22 research for 0.32. There is no admitted CellScript ZK
source type, production verifier profile, production VK, or deployment here.
The maintainer waived independent review on October 3; no independent review
is claimed. The application circuit and exact state-transition policy still
need to be selected before positive production lowering can be enabled.

The child uses `verifier-core` from `CECILIA-MULANDI/groth16-ckb` at
`d64c769ffe2d2edb5eb308dc59058efda77c2f83`, with Arkworks 0.5 and the exact
transitive versions in Cargo.lock. Its declared upstream license is
MIT OR Apache-2.0. CellScript does not prove this cryptographic implementation,
the application circuit, trusted setup, or zero knowledge correct.

The allocation-free codec is shared with the independent artifact-checker
crate at `src/zk.rs`. Sharing byte codecs does not make the checker a
cryptographic verifier. The draft Molecule schema is
`docs/schemas/zk-transition.mol`; its fixed profile identifier commits to the
proof codec, statement order, scalar injection and request width.

The experimental request has exactly 432 bytes: canonical six-field Molecule
table header, magic/version/zero flags, fixed profile identity, VK data hash,
128-byte compressed Groth16 proof and 196-byte statement. Each 32-byte hash is
injected into two canonical little-endian u128 scalar limbs; the outpoint index
is u32. No hash bits are discarded and no implicit modular reduction occurs.
There are 13 public inputs and exactly 680 VK bytes. The child accepts one
inherited FD, reads 54 complete words, rejects trailing data, closes the FD,
searches at most 64 resolved CellDeps and verifies the exact VK data hash.
Parent-side binding of that hash is still required; the child alone does not
turn a caller-provided key into an authorization policy.

Exit codes: 80 transport, 81 envelope, 82 missing/malformed key, 83 invalid
proof, 0 successful pairing check. A pairing success is not permission to
accept a transaction without the unfinished parent composition contract.

The `host` crate tests a deliberately non-authorizing identity circuit with a
public deterministic setup seed. **Never use its keys or circuit in production.**
Its VM harness runs the actual child ELF with modeled transport/CellDep
syscalls. Reported instruction cycles exclude scheduler syscall charges and
are not transaction or stateful acceptance evidence. Host tests also mutate
every statement byte without recomputing the proof.

Build and test with the pinned toolchain:

```bash
cargo build --locked --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo test --locked --manifest-path contracts/zk-transition-verifier/host/Cargo.toml --release -- --test-threads=1
```

Remaining admission work includes the accepted use-case RFC, named exact
dependency resolution, opaque proof typing, transaction-derived statement
construction, mandatory result checking, machine/checker mutations, builder
and editor parity, real scheduler/stateful fixtures, resource ceilings,
reproducible artifact hashes and gate integration. No issue should be closed
on the strength of this child experiment alone.

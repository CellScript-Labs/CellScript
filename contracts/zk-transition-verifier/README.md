# Exact ZK transition composition

This is the experimental exact #22 composition interface for 0.32. It provides
an executable source interface, not a production application circuit, VK or deployment.
The maintainer waived independent review on October 3; no independent review
is claimed. The [counter application](../zk-private-counter/README.md) supplies the real
circuit and scoped setup admission; the child itself does not select a business
relation. The [accepted profile](../../docs/CELLSCRIPT_ZK_PROFILE.md) defines the
complete composition contract.

The child uses `verifier-core` from `CECILIA-MULANDI/groth16-ckb` at
`d64c769ffe2d2edb5eb308dc59058efda77c2f83`, with Arkworks 0.5 and the exact
transitive versions in Cargo.lock. Its declared upstream license is
MIT OR Apache-2.0. CellScript does not prove this cryptographic implementation,
the application circuit, trusted setup, or zero knowledge correct.

The allocation-free codec is shared with the independent artifact-checker
crate at `src/zk.rs`. Sharing byte codecs does not make the checker a
cryptographic verifier. The draft Molecule schema is
[`zk-transition.mol`](../../docs/schemas/zk-transition.mol); its fixed profile identifier commits to the
proof codec, statement order, scalar injection and request width using SHA-256
of `PROFILE_PREIMAGE` (distinct from CKB data hashes).

The v2 experimental request has exactly 464 bytes: canonical six-field Molecule
table header, magic/version/zero flags, fixed profile identity, VK data hash,
128-byte compressed Groth16 proof and 228-byte statement. Each 32-byte hash is
injected into two canonical little-endian u128 scalar limbs; the outpoint index
is u32. No hash bits are discarded and no implicit modular reduction occurs.
There are 15 public inputs and exactly 744 VK bytes. The child accepts one
inherited FD, reads 58 complete words, rejects trailing data, closes the FD,
searches at most 64 resolved CellDeps and verifies the exact VK data hash.
Duplicate matching keys and more than 64 resolved dependencies are rejected.
The parent binds the key commitment and exact child handle before spawning.

Exit codes: 80 transport, 81 envelope, 82 missing/malformed key, 83 invalid
proof, 0 successful pairing check. Parent errors are 74 invalid context,
75 pipe, 76 spawn, 77 write, 78 close, 79 child rejection/Wait failure and
84 cycle bound. The parent enforces one input and one output in the current
Type group, derives the statement from CKB syscalls, writes every complete
request word, closes its writer, waits for the exact child and rejects failure.
The statement includes the full raw transaction hash, so output capacity,
recipients and dependencies cannot be changed while reusing the proof.

The source operation is
`zk::require_valid(policy, proof, dependency, verifier, handle_hash, vk_hash, domain, action)`.
`policy` is a literal ASCII identifier of 1–64 bytes; `proof` is a direct
`witness proof: ZkTransitionProof` parameter (128 bytes), `dependency` is a
`CellDepView`, and `verifier` is an `ExactScriptHandle`. The handle hash, domain
and action are compile-time `Hash` values; the VK commitment uses
`VerificationKeyCommitment::from_bytes` with a literal 32-byte data hash.
The call returns Unit and must occur once, unconditionally in an action entry
block; locks, helpers, loops and conditional calls are rejected. It uses
`WitnessArgs.input_type`, one child, one inherited FD and a 250,000,000-cycle
call ceiling. The independent checker binds these operands, context loads,
transport operations and rejecting branches to the emitted machine code.

Builders use `cellscript_ckb_adapter::zk::transition_statement` on a finalized
raw transaction and resolved inputs, then `bind_prover_output` to check the
returned public inputs and proof width before entry-witness encoding. These
helpers do not verify proofs or assert input liveness. Regenerate the proof
after any raw transaction change. Native and browser metadata expose the same
`runtime.zk_verifiers` contract and source origins; LSP exposes the source types,
call and VK constructor.

The `host` crate tests a deliberately non-authorizing identity circuit with a
public deterministic setup seed. **Never use its keys or circuit in production.**
Its VM harness runs the actual child ELF with modeled transport/CellDep
syscalls. Reported instruction cycles exclude scheduler syscall charges and
are not transaction or stateful acceptance evidence. Host tests also mutate
every statement byte without recomputing the proof.
The separate `scheduler` test executes the generated parent and actual child
with ckb-testtool's real scheduler, including two successive state transitions,
proof replay, output-data/capacity changes, extra group outputs and handle
substitution. Its report is `host/target/scheduler-evidence.json`. Those local
transaction results remain separate from the modeled-syscall measurements.

Build and test with the pinned toolchain:

```bash
cargo build --locked --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo test --locked --manifest-path contracts/zk-transition-verifier/host/Cargo.toml --release -- --test-threads=1
```

All three development gates (`dev`, `ci`, `backend`) build the child and run
the host/scheduler tests. They also check the separate
[private authorization counter](../zk-private-counter/README.md), which supplies
a real application relation, proving/VK packages, lifecycle and reproducibility
checks. `backend` additionally runs its local-node acceptance after the pinned
CKB acceptance harness. The public-seed identity circuit above remains
non-authorizing. Named package-level verifier resolution checks exact deployment receipts before
code generation. Public-network deployment remains separate.

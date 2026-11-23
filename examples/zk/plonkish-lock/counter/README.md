# Private counter with a CellScript verifier call

This local CKB-VM fixture compares Groth16 and PLONK/KZG through the same
CellScript action. It uses the existing
[private counter circuit](../../../../contracts/zk-private-counter/src/lib.rs).
The proof authorizes one increment by a holder of the secret behind the
BLAKE2b owner commitment. It binds the old and new Cell data, current Type
Script hash, consumed outpoint, and full raw transaction hash. The 228-byte
statement maps to 15 BN254 public inputs.

## One action for both verifiers

```cellscript
action increment(witness proof: ZkProof<CounterIncrement>) -> u64 {
    zk::require_valid(proof);
    return 0;
}
```

Both builds compile this exact [source file](main.cell). Each package binds
`CounterIncrement` to its child data hash, proof profile, VK data hash, domain
and action in the [deployment manifest](Cell.toml.in). The parent checks the VK
at CellDep 1 and the verifier at CellDep 0, passes domain and action in argv,
and requires the child to exit successfully. The child reconstructs the rest
of the statement from the live transaction. Groth16 calls Cecilia's
`verifier-core`. Compact PLONK/KZG calls Mosaic with Arkworks group operations.

The native `cellc entry-witness` command encodes the action's sole argument
as the eight-byte `CSARGv1\0` header followed by the fixed-width proof in
`WitnessArgs.input_type`. Both parent and child check this wire format.
The Rust lifecycle Type Script handles creation and successor invariants
and executes the parent pinned in its Script args. The Lock is `ALWAYS_SUCCESS`.

The 27 local CKB-VM v2 cases used the same R1CS and keys as the compact
baseline, with fresh proofs for the new transactions. Both schemes accepted
two increments and the changed transaction with a fresh proof. Both rejected
the stale proof on that transaction, a substituted verifier or VK, and an
incorrect witness header. The PLONK parent also rejected the old 768-byte
proof. [Run record](result-policy.json)

| First increment, complete transaction | Groth16 | Compact PLONK/KZG |
| --- | ---: | ---: |
| VM cycles | 111,006,974 | 66,634,035 |
| Proof bytes | 128 | 480 |
| WitnessArgs bytes | 156 | 508 |
| Transaction bytes | 715 | 1,067 |

The child builds used Rust `opt-level="z"` for Groth16 and `"3"` for PLONK.
Each deployment has its own Type Script args and raw transaction hashes.

## Cases

Each scheme checks zero-state creation, two successive increments, and a
fresh proof for a transaction with an extra unused CellDep. That extra
dependency changes the raw transaction hash while preserving the state pair.
The VM rejects the original proof on the changed transaction and on the
second increment. It also rejects corrupt or truncated proofs, substituted
verifier and VK CellDeps, a skipped counter with the old proof, and a
changed owner commitment.

## Reproduce

Run from the repository with Bash, Git, Node and npm, jq, curl, SHA-256
tools, Clang, LLVM tools, and Rust 1.97.1 with
`riscv64imac-unknown-none-elf` installed.

```sh
./examples/zk/plonkish-lock/run-counter.sh
```

The runner exports the existing Arkworks matrices and witnesses to snarkjs
formats. It uses both schemes on that one R1CS, checks the witnesses and
fresh proofs off-chain, verifies the CellScript artifact and sidecars with
`cellc verify-artifact`, and executes the full transactions in
`ckb-testtool`. It records results under
`target/zk-counter-policy/result.json`. An optional directory argument
selects a different working directory.

The fixture uses a public witness and development keys. The Groth16 key is
the initial circuit-specific key before a contribution. The runner downloads
the prepared `pot19_final.ptau` from the PSE clr.fund archive, checks its
SHA-256, and generates roughly 2 GB of proving keys. VM cycles cover the
whole verification transaction. Each proving duration covers one snarkjs
process, including startup and key loading. Setup, witness export, and host
verification happen outside that timer. [PSE clr.fund setup archive](https://www.trusted-setup-pse.org/clrfund/index.html)

## Uncompressed baseline

Both schemes matched all 12 expected outcomes on CKB-VM v2. The exported
R1CS contains 149,179 constraints. The snarkjs PLONK conversion generated
519,464 gates. The run record includes full transaction hashes, Cell data
hashes, and tool versions. [Run record](result.json)

| Measurement | Groth16 | PLONK/KZG |
| --- | ---: | ---: |
| Initialize, VM cycles | 26,301 | 26,301 |
| Increment 0→1, VM cycles | 110,970,725 | 98,072,829 |
| Increment 1→2, VM cycles | 111,088,741 | 98,222,678 |
| Changed transaction with fresh proof, VM cycles | 110,864,190 | 98,618,827 |
| Binary proof bytes | 128 | 768 |
| `WitnessArgs` bytes | 148 | 788 |
| Increment transaction bytes | 707 | 1,347 |
| Verification key bytes | 744 | 744 |
| Stripped verifier child ELF bytes | 85,456 | 102,944 |
| Compiled CellScript parent ELF bytes | 2,648 | 2,648 |
| Proving key file bytes | 90,761,260 | 1,995,828,644 |

For the first update, PLONK/KZG used 11.6% fewer verification cycles and
640 more transaction bytes. The Groth16 proof commands took 2.20–2.26
seconds and ran serially. The PLONK commands took 167–184 seconds and
overlapped on the shared host. Those wall times describe this run, with
one sample per transaction.

## Optimized PLONK verifier

The PLONK child now reads nine compressed BN254 G1 points and six field
evaluations from the witness. It checks the point encodings and restores
the original affine byte format for the Mosaic verifier. The runner builds
this child with Rust `opt-level = "3"` and uses the same counter R1CS,
verification keys, and CellScript Spawn/Wait call as the baseline. Each
updated transaction gets a fresh proof because the child hash changes
the raw transaction.

These measurements used the [original compact runner](https://github.com/CellScript-Labs/CellScript/blob/68be09591b8de47c9663a44eac66f48e89b2f300/examples/zk/plonkish-lock/run-counter.sh)
and its raw-proof witness layout. [Baseline runner](https://github.com/CellScript-Labs/CellScript/blob/d2b55a32db24387a7ff9d98a4f6445a17bcd9b0f/examples/zk/plonkish-lock/run-counter.sh)

| Measurement | Original PLONK (`-Oz`) | Optimized PLONK (`-O3`) |
| --- | ---: | ---: |
| Increment 0→1, VM cycles | 98,072,829 | 66,616,133 |
| Increment 1→2, VM cycles | 98,222,678 | 66,638,993 |
| Changed transaction with fresh proof, VM cycles | 98,618,827 | 66,459,374 |
| Binary proof bytes | 768 | 480 |
| `WitnessArgs` bytes | 788 | 500 |
| Increment transaction bytes | 1,347 | 1,059 |
| Stripped verifier child ELF bytes | 102,944 | 297,536 |

The first update used 32.1% fewer VM cycles and 288 fewer transaction
bytes than the uncompressed PLONK baseline. It used 40.0% fewer cycles
than the Groth16 control in the rerun, which cost 110,955,354 cycles.
These are whole-transaction measurements of the compression and build
changes together. Proof generation used the original circuit and prover.

All 25 VM cases matched their expected outcomes, including replay
rejection and rejection of the original 768-byte encoding by the compact
child. Focused Rust tests cover round-trip reconstruction and rejection
of alternate encodings of the identity point.
[Optimized run record](result-optimized.json)

# PLONK bound-Lock experiment for CKB

The [CellScript private-counter experiment](counter/README.md) compares
Groth16 and PLONK/KZG through a compiled update action and a spawned verifier.

Cecilia distinguished a static proof gate, a committed statement with
spend-derived context, and a dynamic CellScript transition verifier in her
Phase 0 reply. This experiment covers the middle case. The pinned zk-Lock
implementation derives context from the raw transaction hash, and its
Poseidon preimage circuit exposes that value as its first public input.
[Cecilia's Phase 0 reply](https://talk.nervos.org/t/research-notes-what-zero-knowledge-proofs-enable-on-ckb/10368/9) · [Pinned bound Lock](https://github.com/CECILIA-MULANDI/zk-Lock/tree/79d17c2fb4af1dfd68471ec5c564e6562a0aae32)

Both proofs come from the same compiled Circom circuit and BN254 Powers
of Tau file. The baseline runs Cecilia's Groth16 Lock. The candidate runs
snarkjs PLONK/KZG with Mosaic's verifier and Arkworks group operations.
Both are directly linked `data2` Lock ELFs, built with the same RISC-V
flags and stripped with the same tool. Each fixture spends one input to
one output with its own code and VK CellDeps. These are measurements of
the two implementations and their wire encodings.
[Pinned Mosaic verifier](https://github.com/wienerlabs/mosaic/tree/58f0efd3af09dfdede020d570c0be419627e95fa) · [snarkjs](https://github.com/iden3/snarkjs)

## Acceptance cases

| Spend | Expected observation |
| --- | --- |
| Original transaction and its proof | Accept |
| Changed output data with the original context and proof | Context check rejects with exit 22 |
| Changed output data with its recomputed context and the original proof | Verifier rejects with exit 20 after the Lock's context check |
| Changed output data with its recomputed context and a fresh proof | Accept |

The third case is the additional check discussed in CellScript #22. The
fixture uses the same statement digest for all four cases.
[CellScript #22](https://github.com/CellScript-Labs/CellScript/issues/22)

## Reproduce

Use Bash, Git, Node and npm, jq, Clang, LLVM tools, and Rust 1.97.1 with
the `riscv64imac-unknown-none-elf` target.

```sh
./examples/zk/plonkish-lock/run.sh
```

The runner fetches the pinned zk-Lock source, checks the Powers of Tau hash,
compiles one circuit, generates both sets of test keys, and builds the two
Lock ELFs. It checks the freshly generated proofs with snarkjs and executes
both ELFs and all eight cases through `ckb-testtool`. It writes
`target/zk-lock-plonkish/result.json` only after all checks pass. A directory
argument changes the output location.

Scope is local CKB-VM execution with the public demo preimage and test
keys. The Groth16 demo key has no circuit-specific contribution. VM cycle
counts are observed for the final proof from each fixture. Proving
measurements are three interleaved Node process wall times per fixture,
including startup and key loading. Setup, witness generation, and host
verification are outside that timer.

## Recorded run

All eight cases had the expected outcome on Rust 1.97.1, `ckb-testtool`
1.1.1 and `ckb-vm` 0.24.15. The report includes the exact R1CS, ELF and
VK identities, and all proving timer samples. [Run record](result.json)

| Measurement | Groth16 | PLONK/KZG |
| --- | ---: | ---: |
| Original spend, VM cycles | 101,367,539 | 96,337,697 |
| Changed spend with fresh proof, VM cycles | 101,344,008 | 96,332,753 |
| Binary proof bytes | 128 | 768 |
| `WitnessArgs` bytes | 216 | 856 |
| Full transaction bytes | 500 | 1,140 |
| VK Cell data bytes | 328 | 744 |
| Stripped Lock ELF bytes | 93,440 | 113,016 |
| Proving key file bytes | 186,944 | 858,348 |

The PLONK fixture used about 5% fewer VM cycles and 640 more witness
bytes. The proving wall times were highly variable on the shared host.
For example, the three original-spend observations ranged from 0.337 to
14.947 seconds for Groth16 and from 0.501 to 11.875 seconds for PLONK.
The cycle and byte counts characterize these two direct-linking
implementations and the fixture.

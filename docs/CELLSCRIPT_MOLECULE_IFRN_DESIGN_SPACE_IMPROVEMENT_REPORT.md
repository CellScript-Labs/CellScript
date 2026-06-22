# CellScript Molecule / IFRN Design-Space Improvement Report

Date: 2026-06-22
Branch: `nightly-0.20`

This report replaces the earlier design-space audit note with a closure-oriented,
English-only improvement report. It covers the current CellScript repository,
the NovaSeal, DOB, and iCKB evidence surfaces, and the Infern / IFRN questions
raised during review. It does not re-audit the external Infern repository; any
Infern conclusions remain engineering inferences from the described raw-layout
patterns and the current CellScript capability boundary.

## Closure Status

The CellScript-repository P0/P1 design-space compression items are closed for
the current local source-package and devnet evidence scope:

- raw cell-data codec honesty;
- CKB deployable artifact identity;
- DOB devnet workflow and registry pressure;
- NovaSeal Agreement BIP340 verifier IPC;
- variable-length packed hash preimages;
- wide and nested fixed-width packed struct hashing;
- iCKB committed evidence matrix refresh.

Agreement semantics were not changed. The work was limited to compiler/backend
behaviour, runtime diagnostics, packed preimage materialisation, BIP340 verifier
syscall/IPC wiring, planned-profile fixture hash semantics, and evidence
refreshes.

Remaining items are no longer blockers for this local source-package scope. They
belong to public/mainnet release or productisation work: external codec adapters,
raw-layout DSL support, multi-ABI builder codec backends, registry/indexer
support for multiple ABI families, declarative continuity/timepoint policy, and
a real six-contract Infern parity matrix.

## Gates

| Gate | Status | Evidence |
|---|---:|---|
| NovaSeal Agreement live devnet stateful | passed | valid originate, repay, and claim paths pass; signature negatives fail semantically through BIP340 verifier rejection rather than code 1 or code 18 |
| NovaSeal core live devnet stateful | passed | wrong-signature transition is rejected through verifier semantics |
| NovaSeal planned profiles live devnet stateful | passed | btc-transaction-commitment, btc-utxo-seal, dual-seal, fiber-candidate, fungible-xudt, and rwa-receipt valid paths and semantic negatives pass through the live devnet runners |
| DOB evolving devnet workflow | passed | proposal-local workflow gate passes |
| DOB registry pressure | passed | registry pressure gate passes |
| packed hash regressions | passed | wide fixed-width, multi-block, nested fixed-width, agreement-sized nested parameter, and signature payload field hashing compare against CKB VM hash results; valid paths no longer reach code 18 |
| iCKB differential matrix | passed | refreshed committed dual-side CKB VM evidence; 218 `ickb_diff` cases pass |
| iCKB benchmark | passed | benchmark and fixture model tests pass |
| iCKB claim manifest verifier | passed | `cellc verify-ckb-fixtures` succeeds with a complete executable claim set |

## Verified Facts

| Claim | Current anchor |
|---|---|
| CKB profile / metadata Molecule hard constraint | `src/lib.rs:306`, `src/lib.rs:913-915`, `src/lib.rs:2779-2781`, `src/lib.rs:2857-2858` |
| public scheduler policy witness remains Molecule-only | `src/lib.rs:3411-3416` |
| `lock_args` is a fixed-width `Script.args` escape hatch | `docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md:40-43` |
| schema-backed dynamic witness payloads are Molecule data | `docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md:70-80` |
| raw cell data and OutPoint helpers exist | `docs/CELLSCRIPT_0_18_ROADMAP.md:93-110` |
| NovaSeal v0-mvp demonstrates raw byte-offset source expressibility | `proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell:138-177`, `proposals/novaseal/v0-mvp-skeleton/src/nova_state_type.cell:125-155` |
| v0-mvp packed layout is not a production ABI conclusion | `proposals/novaseal/v0-mvp-skeleton/docs/SCHEMA_LAYOUT.md:44-54` |
| newer NovaSeal profiles mostly use whole-cell packed hashes | `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell:226-227`, `proposals/novaseal/btc-transaction-commitment-profile-v0/src/nova_btc_transaction_commitment_type.cell:361`, `proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell:378` |
| iCKB specs live under the benchmark test surface, not public examples | `tests/benchmarks/ickb_specs/README.md:3-9`, `tests/benchmarks/ickb_diff/claim_manifest.json:5-9`, `roadmap/CELLSCRIPT_ROADMAP.md:343`, `roadmap/CELLSCRIPT_ROADMAP_OVERVIEW.md:330` |
| 0.20 has an ELF entry ABI gate and the build-report linkage | `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md`, `scripts/ckb_cellscript_acceptance.sh`, `scripts/validate_ckb_cellscript_production_evidence.py`, `docs/CELLSCRIPT_GATE_POLICY.md` |
| `cell_data_codec_manifest` is emitted and exposed to generated builders | `src/lib.rs`, `src/cli/commands.rs`, `tests/cli.rs`, `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md` |
| DOB-EVO is mainly a lock-hash / production-policy issue, not Molecule-only evidence | `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md:64`, `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md:69` |

## Technical Conclusion

CellScript is not locked into Molecule-only verifier logic at the source level.
`ckb::cell_data_*`, `ckb::input_out_point_*`, `ckb::require_input_out_point`,
`ckb::hash_data_packed`, and `lock_args` provide real escape hatches. NovaSeal
v0-mvp proves that hand-rolled byte layout can run through CellScript-emitted
RISC-V.

The stricter boundary is off-chain tooling and metadata honesty. The current
first-class path remains Molecule-native typed cells, metadata, audit evidence,
provenance, and builder identity. Raw layouts are expressible, but production
claims require codec manifests, adapter identity, builder/indexer support, and
byte-for-byte roundtrip vectors.

The new hard increment is `cell_data_codec_manifest`: raw `LOAD_CELL_DATA` users
no longer pretend to be pure Molecule schemas. Molecule-native contracts declare
`abi = "molecule"`, while raw cell-data users declare `abi =
"molecule+raw-bytes-v1"`. The TypeScript builder manifest and action plan expose
that boundary.

## Remaining Product Work

The following are productisation items, not current local-devnet blockers:

1. Extend `cell_data_codec_manifest` into package, registry, builder, and
   deployment identity.
2. Add external codec adapter identity, adapter hashes, and roundtrip vectors.
3. Build a raw-layout builder backend or typed adapter interface.
4. Add registry/indexer support for multiple ABI families.
5. Add declarative continuity and timepoint policies.
6. Build a real Infern parity matrix covering valid/invalid behaviour, cycles,
   binary size, transaction size, occupied capacity, and encoder vectors.

## Priority

Immediate items now closed:

1. Maintain one active report path; the stale `_ZH` / `_zh-cn` document path is
   removed.
2. Pin `tests/benchmarks/ickb_specs/*.cell` as the benchmark surface and correct
   the stale `examples/ickb_benchmark/*.cell` roadmap claim. The actual change is
   in `roadmap/CELLSCRIPT_ROADMAP.md:343` and
   `roadmap/CELLSCRIPT_ROADMAP_OVERVIEW.md:330`; no new public example directory
   was added.
3. Document `CellScriptBuildReport` in `docs/CELLSCRIPT_GATE_POLICY.md` as an
   integration of the acceptance report, production gate, and ELF ABI gate.
4. Add `cell_data_codec_manifest` so raw `LOAD_CELL_DATA` access is declared as
   `molecule+raw-bytes-v1` and exposed to TypeScript builder outputs.
5. Correct citation anchors for Molecule metadata, scheduler witness, 0.18
   helpers, and DOB-EVO conclusions.

Decision gates:

1. Keep exact-artifact evidence flowing from compiled ELF to live code-cell data
   hash, then continue extending it through metadata, `Cell.lock`,
   `Deployed.toml`, `cell_data_codec_manifest`, builder manifests, and
   valid/tampered carriers.
2. Maintain a cycle / size / behaviour parity matrix before broad production
   compiler claims.
3. Add raw-layout roundtrip vectors so metadata, on-chain reads, builders, and
   encoders cannot drift.
4. Prefer fixture and gate coverage before widening language surface area.

## Final Position

CellScript is a CKB compiler and a typed-cell / metadata / audit / provenance
toolchain. The closed claim is local source-package readiness for the current
Molecule-native typed-cell, NovaSeal/DOB, audit/provenance, builder identity, and
live devnet workload surfaces. For raw-layout / IFRN-style contracts, on-chain
source-level expressibility is already present, but public/mainnet release,
off-chain codec support, builder support, registry/indexer support, and real
project parity evidence must remain separate evidence chains.

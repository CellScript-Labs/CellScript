# Checked code Cell byte origin

`code_origin::check_code_cell_origin` accepts the actual four-file bundle,
canonical CKB **RawTransaction** bytes, an output index and the complete selected
Molecule Script. It privately owns checked external-codec evidence and recomputes
all origin fields. Exported JSON cannot construct the checked value.

The OutPoint uses the CKB-personalized Blake2b hash of RawTransaction plus the
selected output index. Witnesses belong to Transaction and are not part of this
hash. The selected output data must equal the entire independently checked ELF,
not a caller-supplied artifact digest. Data/data1/data2 selections must carry its
exact data hash. Type selections must carry the hash of the actual output Type
Script; absent Type Scripts reject. Both modes retain the actual code data hash.
The selected Script hash commits to its concrete code hash, hash type and complete
args. Different args form a different checked identity; no wildcard args policy
or deployment authority is inferred.

The parser implements strict views of the pinned official
[CKB Molecule schema](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/schemas/blockchain.mol)
and uses the hash rules from
[calc_hash.rs](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/src/extension/calc_hash.rs).
All unselected output Scripts/data are validated too. Unknown Script hash types,
raw CellDep tags, extra table fields, noncanonical offsets, trailing/truncated
bytes, mismatched output/data counts and duplicate input/header/CellDep identities
reject. Input OutPoints are retained as byte-origin fields, not authenticated
ancestry. No CKB dependency, frontend or generator is added to the standalone
checker. Tests independently construct these bytes/hashes with existing pinned
CKB SDK types.

Limits are preflighted before parsing: all four bundle files, raw transaction
and selected Script share a 16 MiB total; every file is <=4 MiB and respects the
caller artifact/record/source-map byte budget. The finite transaction profile is
version zero, <=256 inputs and outputs, <=64 raw and header deps, and complete
Scripts <=4096 bytes. Only structurally canonical transactions belong to this
profile. These limits are not a claim that all such transactions pass consensus.

This is byte-origin evidence for a partial #28 implementation. A synthetic
canonical transaction can pass the byte check. It does **not** prove commitment,
network identity, capacity sufficiency, input/lock authorization, Type replacement
history, active/yanked/version policy, live Cells, immutable catalog authorization
or final raw/resolved dependency binding. Those remain separately required for
H1/H2 admission. In particular, a Type Script hash alone cannot authorize a new
ELF or a replacement history. The source nominal I, generic handles, #29 and
#44–#46 are not completed by this API. No on-chain claim is attached to this
host-only constructor.

The native [frozen code catalog](FROZEN_CODE_CATALOG.md) joins this evidence to
actual owned package source snapshots and directional all-member module checks.
It preserves this byte-origin boundary; joining host evidence does not create
deployment authorization or a live-code observation.

## Checked target selection

`code_origin::check_code_cell_target` consumes the actual private
`CheckedCodeCellOrigin`. It reads the runtime contract from the independently
checked codec/module projection and requires the profile's permitted deployment
hash type: `ckb` uses `data2`, and `ckb-type-hash` uses `type`. The underlying
artifact inspector already verifies VM2, `rv64imac_zbb` and the singleton
metadata/constraints deployment rule. No additional untrusted record is parsed,
and exported JSON cannot construct the private `CheckedTargetCodeCellOrigin`.
The original byte-origin constructor and v1 identity remain unchanged.

The separate `cellscript-code-cell-target-v1` record commits to that exact origin,
complete checked runtime contract and deployment hash type; its identity domain
is `cellscript-code-cell-target-id-v1`. A legal data/data1/Type selection can pass
the byte-origin check while failing target selection for a `ckb` bundle. The
native frozen code catalog now requires target checking for **every** candidate,
including unselected members and the final member of a 32-member catalog.

The pinned [CKB VM selection source](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/types.rs)
selects VM0 for data, VM1 for activated data1 and VM2 for activated data2. Type
uses the latest activated version. This profile check does not observe chain
activation; a Type-hash token cannot certify VM2 execution before the required
fork, authorized replacement history or live dependencies. Those remain
separate admission/runtime obligations.

SDK-built tests cover both actual compiler target profiles at O0–O3. All four
hash types bind the same exact code Cell bytes; eight profile selections pass
and 24 mismatches reject. Changing exact args still changes the target identity.
Native tests reject otherwise byte-valid data/data1 unselected candidates and
a data1 candidate in the final slot, without weakening byte-origin inspection.

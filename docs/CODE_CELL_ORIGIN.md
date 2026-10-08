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

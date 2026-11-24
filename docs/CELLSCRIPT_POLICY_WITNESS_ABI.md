# Policy Witness ABI v1

The compiler dispatches persistent Type Script policies through this ABI.
The shared host codec also accepts Lock records for independent consumers to
place in the same witness envelope.

This ABI carries requests for several Script groups in one
`WitnessArgs.input_type`. It is separate from the existing
`cellscript-entry-witness-v1` payload and
`cellscript-witnessargs-input-type-v2` placement path. Existing single-entry
`--entry-action` and `--entry-lock` builds retain their current bytes and behavior.

## Explicit package selection

Declare the exported actions and stable numeric tags in the entry package's
`Cell.toml`. The source action names below are illustrative and must resolve to
unit-returning actions in that package. A common check is a zero-parameter,
unit-returning action; the dispatcher runs common checks in declaration order
before the selected action.

Each retained common-check call graph must be acyclic, with at most 256
callables on any path including the common action itself. Shared suffixes
count on every incoming path, independently of declaration or call order.
The compiler and independent metadata checker both enforce this bound.
The supported scalar/Unit helper body and ABI boundaries are recorded in the
[implementation contract](CELLSCRIPT_AUTHORING_IMPLEMENTATION.md#bounded-persistent-type-policy).

```toml
[[artifacts]]
name = "token-policy"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "policy-witness-v1"
actions = [
  { tag = 10, action = "mint" },
  { tag = 20, action = "transfer" },
  { tag = 30, action = "merge" },
  { tag = 40, action = "burn" },
]
common_checks = ["check_policy"]
```

```bash
cellc check --artifact token-policy --all-targets
cellc build --artifact token-policy --target riscv64-elf
```

`--artifact` selects exactly one declaration. Every tag comes from the
manifest table. Build selection is mutually exclusive with `--entry-action`
and `--entry-lock`. Tagged policy dispatch requires explicit selection by name.
Workspace selection resolves the same explicit artifact name separately in
each selected member and rejects a member that does not declare it.

Authoring-v7 uses `action` for every exported operation. The fixed-role
form declares `input`, `output`, and `witness` ports in the signature.
This dispatcher supports group-relative bindings and fixed-group coverage.
Each variant has its own input and output counts under the selected layout.
Typed transaction views and CellDep reads can check additional relationships.
Actions can use `replace`, `pool`, `retire`, and `fresh` along with ordinary
checked statements. Configure `dispatch = "single-entry"` and `action = "..."`
to select the ordinary entry payload. Tagged dispatch uses the `actions` table
above and its existing witness envelope.

Build writes the selected artifact and sidecars to the usual package output
paths; selecting a different artifact replaces those build products. Policy
builds do not read or refresh the default single-entry compilation cache.
Declaration changes remain subject to the existing manifest digest and
`Cell.lock` authority; builds do not repin a stale lock automatically.

This is the bounded Type-policy path. Accepting a Lock record in the codec
does not add a Lock artifact context.

## CLI and generated-builder consumers

Inspect a selected policy without generating machine code:

```bash
cellc metadata --artifact token-policy --target riscv64-elf
cellc expand --artifact token-policy
cellc expand src/main.cell --artifact token-policy --json
```

Both commands accept a source file, package directory, or `Cell.toml`. Their
selected-artifact path emits metadata or the semantic foundation only: it does
not generate ELF, assembly, build sidecars, or a machine artifact hash. An
explicit `--output` writes only the requested inspection document. The human
expansion lists the canonical numeric tag-to-entry mapping, payload schema
hashes, exact group input/output counts, ordered common checks, selector, and
bounds. It reads the checked dispatch record, not all retained actions, so
common checks and helper dependencies cannot appear as selectable variants.
The rendering is a view, not a hash input. Omitting `--artifact` retains the
existing default-entry behavior and diagnostics.

For a tagged policy, `entry-witness --artifact NAME` requires an exported
`--action` and `--script-hash HEX`, the full 32-byte deployed Script hash.
A `single-entry` artifact already selects its action and emits the ordinary
entry witness payload. The following uses an illustrative hash:

```bash
cellc entry-witness --artifact token-policy --action burn \
  --script-hash 1111111111111111111111111111111111111111111111111111111111111111 \
  --output burn-request.bin --json
cellc gen-builder --artifact token-policy --target typescript --output builder
```

The first command parses typed `--arg` values using the selected action's
existing inner encoder and emits one canonical CSPOL bundle. A no-payload
action still requires a policy record; its inner args are empty. The output
file contains only the `input_type` payload, not a serialized WitnessArgs or
a signature. Its JSON explicitly reports that placement has not occurred and
the supplied hash is not authentication. Multiple requests sharing a physical
witness index must be aggregated before placement. `--lock`, missing or short
hashes, common checks, and undeclared actions are rejected on this path.
Without `--artifact`, the existing single-entry CLI encoding is unchanged.

`gen-builder` exports only declared policy variants, never retained common
checks or action dependencies. `--metadata` accepts already-selected policy
metadata after metadata/typed-plan validation; an optional `--artifact NAME`
must match that metadata. Omitting the flag cannot reinterpret policy metadata
as legacy entry metadata. This validation does not establish live deployment
or caller authority.

Generated manifests and action plans carry the outer-envelope obligation and
explicit tags. `createPolicyWitnessRecord(action, fullScriptHash, entryArgs)`
accepts **already encoded** inner bytes; it checks framing and the exact empty
case, not arbitrary typed payload contents. `encodePolicyWitnessBundle(records)`
sorts and encodes the bounded canonical format. These helpers do not encode
typed business arguments, resolve live Cells, group requests by witness index,
construct WitnessArgs, or sign. The existing typed runtime interface remains
responsible for those steps and the final whole-witness size check. In
particular, a header-only CSARG buffer passing structural validation is not
evidence that a parameterized action has valid arguments.

The generated package's `npm test` runs both its existing runtime-adapter tests
and an independent literal-byte policy golden test after TypeScript compilation.
Successful codec tests are not CKB-VM or chain-acceptance evidence.

## Limits and ownership

| Property | v1 contract |
| --- | --- |
| Payload ABI | `cellscript-policy-witness-v1` |
| Magic | Eight bytes `CSPOLv1\0` |
| Records per bundle | 1 through 8 |
| Exported variants per artifact | At most 64; checked by the artifact resolver |
| Whole serialized `WitnessArgs` | At most 4096 bytes |
| Encoded bundle alone | At most 4076 bytes |
| Placement ABI | `cellscript-policy-witnessargs-input-type-v1` |
| Placement | `WitnessArgs.input_type` |
| Placement source identifier | `group-input[0]-or-output[0]-if-no-inputs` |
| Record key | Role byte followed by the full 32-byte Script hash |
| Canonical order | Strict unsigned lexicographic order of that key |
| Duplicate key | Rejected, including records with different tags or args |

An otherwise empty WitnessArgs costs 16 table-header bytes and 4 bytes for
the input_type Bytes length. Therefore 4076 is only the maximum possible bundle
size. Existing lock or output_type bytes, including their Bytes length prefixes,
reduce the available budget. Placement must check the final serialized witness;
a bundle passing its own limit may still be too large.

The full Script hash commits to `code_hash`, `hash_type`, and `args`.
A code hash, address, schema name, or action name is not a record key. The tag
is a request, not authorization. Common policy checks and the selected action's
authorization remain mandatory.

Builders collect all requests for the same physical transaction witness index,
encode one bundle, place it once, and then sign. They must not overwrite an
occupied input_type or silently merge an existing envelope. Placement preserves
lock and output_type byte-for-byte. A lock field may contain an SDK signing
placeholder; the placement API cannot infer whether arbitrary bytes are already
a live signature. The caller must supply an unsigned draft and must not mutate
the witness after signing.

The fixed CKB dependency maps both GroupInput and GroupOutput witness lookups
to the transaction's single witnesses array. A Type group with no inputs can
therefore share a witness index with an unrelated input Lock group. Group names
do not imply separate storage.

## Wire format

All integers are unsigned little-endian. Every size and offset below excludes
the preceding eight-byte magic unless explicitly stated.

```text
bundle = "CSPOLv1\0" || Molecule DynVec<PolicyRecord>
PolicyRecord = Molecule table {
    role: Byte,
    script_hash: Byte32,
    tag: Uint32,
    args: Bytes,
}
```

For N records, the DynVec header contains its total size and N offsets.
Its header length is `4 * (N + 1)`. The first record offset equals that
header length. N is inferred from `first_offset / 4 - 1`; it is not a separate
count field. Offsets are relative to the start of the DynVec, strictly increasing,
and delimit nonempty record ranges. The final range ends at the DynVec total size.

Each PolicyRecord has exactly these field offsets:

| Record-relative byte offset | Content |
| --- | --- |
| 0 | Total record size: `61 + args_length` |
| 4, 8, 12, 16 | Field offsets: `20, 21, 53, 57` |
| 20 | Role: 0 = Lock; 1 = Type |
| 21 through 52 | Full Script hash, 32 raw bytes |
| 53 through 56 | Numeric action tag, u32 little-endian |
| 57 through 60 | Args Bytes length, u32 little-endian |
| 61 onward | Exactly args_length bytes |

All u32 tag values, including zero, are structurally valid. The declared
artifact mapping determines which tags are accepted. A codec cannot infer the
valid tag set from transaction shape or action names.

The v1 decoder rejects unknown roles, wrong magic/version, empty or excessive
record counts, noncanonical table headers, extra fields, gaps disguised as
headers, overlapping or out-of-range offsets, mismatched sizes, truncation,
trailing bytes, unsorted keys, and duplicate keys. It checks byte/count bounds
before allocating records. Molecule's compatible extra-field interpretation
must not be enabled for these tables.

## Argument bytes and selection

The `args` field contains the exact result of the existing entry-argument
encoder:

- An entry consuming no witness payload uses zero bytes. The outer record and
  its selector are still required.
- Otherwise the bytes start with `CSARGv1\0` and retain the existing scalar,
  fixed-byte, dynamic-value, and bounded-plan argument encoding.

The host codec checks only this empty-or-magic distinction. It does not know
the selected action's parameter schema. The entry adapter must reject empty
args for a payload-bearing action, nonempty args for a no-payload action,
wrong lengths, wrong parameter codecs, and trailing payload bytes. An
eight-byte CSARG header is not an alternative encoding of an empty argument
block for a no-payload action.

A dispatcher first validates the entire bundle's structure, then selects
exactly one record for its current role and current full Script hash. Missing
selection is an error. The selected tag must be declared by that artifact.
Records for other keys still undergo structural validation, but their tags
and payload schemas belong to their own artifacts; the current artifact must
not apply its own action table to those records.

For Type invocation, the planned placement retains GroupInput#0, falling back
to GroupOutput#0 only after proving that the current group has no inputs.
A missing input-side witness is not proof of an empty group. Lock invocation
uses its input group; Type creation fallback must not be applied to Lock.

The selected action adapter receives the validated inner args range. It must
not reload the complete outer witness and interpret it as CSARGv1.

## Host APIs and independent adapter

The compiler host module is `src/policy_witness.rs`:

```rust
PolicyScriptRole::{Lock, Type}
PolicyWitnessRecord { role, script_hash: [u8; 32], tag: u32, args: Vec<u8> }

encode_policy_witness_bundle(records: &[PolicyWitnessRecord])
    -> Result<Vec<u8>>
decode_policy_witness_bundle(encoded: &[u8])
    -> Result<Vec<PolicyWitnessRecord>>
selected_record(records, role, script_hash)
    -> Option<&PolicyWitnessRecord>
```

Encoding sorts a borrowed list without changing caller order and rejects
duplicate keys. Decoding requires sorted input rather than silently repairing
it. selected_record is a lookup for a decoded or equivalently validated list;
None must not become a fallback tag or a successful no-op.

The CKB adapter keeps an independent copy of this bounded codec in
`crates/cellscript-ckb-adapter/src/policy_witness.rs`, without depending on the
compiler. Its placement helper validates the bundle, rejects occupied
input_type, preserves the other fields, and checks the final witness size.
Test-only cross-checks use literal golden bytes and both implementations.
Changes to one implementation require updating and testing the other.

## Literal golden vector

This 151-byte bundle contains:

1. Lock, hash `00` repeated 32 times, tag 7, args `CSARGv1\0 || aa`.
2. Type, hash `11` repeated 32 times, tag `0x01020304`, empty args.

The DynVec size is 143 and its offsets are 12 and 82. Concatenate these hex
segments without whitespace:

```text
4353504f4c763100
8f0000000c00000052000000
4600000014000000150000003500000039000000
00
0000000000000000000000000000000000000000000000000000000000000000
07000000090000004353415247763100aa
3d00000014000000150000003500000039000000
01
1111111111111111111111111111111111111111111111111111111111111111
0403020100000000
```

The vector fixes wire layout, sorting, tag endianness, both roles, and empty
args. Its first args block is structurally valid but does not certify a
particular action schema.

## Integration acceptance

Before advertising executable multi-action policies, require one deployed
Type Script identity across mint, transfer, merge, and final burn; selected-tag
and payload negatives; input-bearing and output-only invocations; shared
Type/Lock witness slots; signature-preserving assembly before signing; exact
common-check coverage; and independent typed/machine dispatch checks.
Run old single-entry golden vectors unchanged. The host codec's unit tests
are structural evidence, not VM, signature, deployment, or authorization evidence.

Related contracts:
[authoring target](CELLSCRIPT_AUTHORING_TARGET.md),
[implementation boundaries](CELLSCRIPT_AUTHORING_IMPLEMENTATION.md), and
[existing entry witness ABI](CELLSCRIPT_ENTRY_WITNESS_ABI.md).

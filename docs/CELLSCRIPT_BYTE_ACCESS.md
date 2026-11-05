# Byte reads and digests

Raw byte reads and cryptographic digests have distinct source contracts. Exact
32-byte reads return `[u8; 32]`; digest operations return their declared hash
domain. Equal widths do not permit implicit conversion between these types.
The [source contract](CELLSCRIPT_SOURCE_CONTRACT.md) owns the current artifact
and interface versions.

## Choosing an operation

| Interface | Meaning and bounds |
| --- | --- |
| `ckb::cell_data_hash_field(view)`, typed `.data_hash` | Consensus DATA_HASH; empty Cell data produces zero32 and has no 512-byte read-buffer limit |
| `ckb::cell_data_blake2b_span(view, offset, length)` | BLAKE2b of the complete specified range; checks `offset <= total` and `length <= total - offset`, and streams blocks of at most 128 bytes |
| `ckb::cell_data_bytes32(view, offset)` | Exactly 32 raw bytes; requires at least 32 remaining bytes and permits a longer suffix |
| ScriptRef `.args32`, `ckb::cell_*_args32(view)` | Raw Script arguments; requires exactly 32 argument bytes |
| `witness::bytes32(view, offset)` | Raw bytes from the complete requested 32-byte slice |
| `witness::lock_exact32`, `input_type_exact32`, `output_type_exact32` | The complete selected field must contain exactly 32 payload bytes; other fields may be larger |
| `witness::bounded_raw`, `bounded_lock`, `bounded_entry`, `bounded_output_type` | A complete read-only byte view with an explicit maximum, source and full length |
| `ckb::input_out_point_index`, `input_out_point_tx_hash_low` | An index or low hash word from an exact 36-byte OutPoint; a failed read cannot publish its scalar value |
| `ckb::cell_lock_hash_low`, `cell_type_hash_low` | The low word of a Script hash; missing Type Scripts and failed reads reject |

Consensus DATA_HASH and BLAKE2b of Cell data are different operations. An empty
BLAKE2b span produces BLAKE2b(empty), rather than zero32. A requested raw range
is checked in full. Exact-field reads reject a wrong payload length instead of
truncating it or padding missing bytes.

`Hash::from_bytes(bytes)` explicitly interprets a 32-byte array as an ordinary
Hash. It does not compute a digest, establish existence or grant authority.
Serialize a real digest with its explicit `.0` payload when comparing it with
raw bytes. Exact/prefix/suffix Script requirements compare explicit bytes.

Low-word getters omit the remaining hash bytes and cannot establish complete
identity. Source coordinates identify where a read occurs; the application
still checks identity and authority. HeaderDep is not a Cell-data source.

## Checked boundaries

The standalone checker binds the admitted raw, consensus-field, exact-witness,
Script-argument and scalar-status helpers to their decoded machine contracts.
Streaming checks bind source selection, checked spans, chunk capacity,
reported length, output writes and stack restoration. Compression bodies retain
their independently pinned complete-body profile.

These finite instruction contracts do not establish arbitrary source
equivalence or application-specific production safety. Current evidence and
execution requirements belong to the [artifact boundary](CELLSCRIPT_VERIFIED_ARTIFACT_BOUNDARY.md),
[runtime ABI coverage](../crates/cellscript-artifact-checker/docs/RUNTIME_ABI_COVERAGE.md)
and [gate policy](CELLSCRIPT_GATE_POLICY.md).

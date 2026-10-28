# Byte reads, digests and migration

Status: candidate breaking migration after `v0.33.0-alpha.1`; full integration
acceptance remains pending. The published alpha's historical receipt describes
that immutable release, not these uncommitted changes.

Raw bytes and cryptographic digests have distinct contracts. Exact 32-byte
reads return `[u8; 32]`; actual digests return `Hash` or a specific nominal hash
domain. A matching width does not permit implicit conversion between them.

| Current interface | Result | Length and error contract |
|---|---|---|
| `ckb::cell_data_hash_field(view)`, typed `.data_hash` | Consensus DATA_HASH | Empty data returns zero32; no 512-byte buffer limit; field failure 44 |
| `ckb::cell_data_blake2b_span(view, offset, length)` | BLAKE2b digest of the complete specified range | Requires offset <= total and length <= total-offset; streams blocks of at most 128 bytes; empty range returns BLAKE2b(empty) |
| `ckb::cell_data_bytes32(view, offset)` | Raw `[u8; 32]` | At least 32 remaining bytes required; a longer suffix is allowed; short read 1 |
| ScriptRef `.args32`, `ckb::cell_*_args32(view)` | Raw `[u8; 32]` | Args must be exactly 32 bytes; wrong width 38 |
| `witness::bytes32(view, offset)` | Raw `[u8; 32]` | Requires the entire requested slice |
| `witness::lock_exact32`, `input_type_exact32`, `output_type_exact32` | Raw `[u8; 32]` selected field | Requires exactly 32 payload bytes; no padding or truncation; larger siblings allowed |
| `witness::bounded_raw`, `bounded_lock`, `bounded_entry`, `bounded_output_type` | Complete read-only bounded byte view | Preserves source and full length; selected field absent 67, declared maximum exceeded 68, malformed encoding 42/43 |
| `ckb::input_out_point_index`, `input_out_point_tx_hash_low` | Index or low hash word | Exact 36-byte OutPoint required; caller checks a1 before accepting a0; failure 44 |
| `ckb::cell_lock_hash_low`, `cell_type_hash_low` | Low word of actual Script hash | Missing Type Script rejects; caller checks a1; failure 44 |

Low-word getters intentionally omit the other hash bytes and cannot prove full
identity. SourceView coordinates identify a source; they do not authorize it.
HeaderDep is not a Cell-data source.

## Removed names

`ckb::cell_data_hash`, `cell_data_hash_at`, Script `.args_hash`, the
`cell_*_args_hash`/`require_cell_*_args_hash` families, and `witness::raw`,
`lock`, `input_type`, `output_type` projections have been removed. The implicit
WitnessArgsView field projections have also been removed. Compilation reports
migration diagnostics; no callable runtime aliases remain.

- For consensus identity choose DATA_HASH. For BLAKE2b of raw data choose the
  streaming span operation. Empty results differ; this is an explicit choice.
- For raw exact reads choose `bytes32`, `args32` or the field's `*_exact32` read.
- For variable-length fields choose a bounded complete view and exact byte
  comparisons or its explicit BLAKE2b operation. Equal prefixes and padded
  projections never proved complete field equality.
- Exact/prefix/suffix Script requirements accept explicit bytes. Serialize an
  expected digest with `.0`; this preserves the protocol's serialized bytes.
- `Hash::from_bytes(bytes)` explicitly interprets a 32-byte array as an ordinary
  Hash value. It does not hash, verify existence, or grant authorization.

## Validation scope

The independent checker validates exact raw read domains, complete Cell-data
slice and consensus-field machine bodies, selected exact32 witness reads,
Script raw args, and scalar getter status guards. Streaming span contracts
independently bind source selection, range checks, chunk capacity, reported
length, output writes and both stack restoration paths; their compression body
uses the existing independently pinned complete-body profile. These are bounded
instruction contracts, not a general source-equivalence proof.

Current VM and rebound-mutation results and any incomplete gate are recorded
in the [retirement work record](CELLSCRIPT_BYTE_BUFFER_RETIREMENT.md).

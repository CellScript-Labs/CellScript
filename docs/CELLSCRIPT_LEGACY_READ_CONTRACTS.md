# Legacy read contracts and migration

Status: implemented compatibility boundaries; final v129 integration gates pending.
The return type `Hash` describes a 32-byte value representation. It does not
establish that an operation hashed its input.

| Interface | Reads / returns | Short input | Long input | Failure propagation |
|---|---|---|---|---|
| `ckb::cell_data_hash(view)` | Entire Cell data, BLAKE2b-256 digest | Empty input returns BLAKE2b(empty) | More than 512 bytes rejects before hashing | Checked a0 status; failed/oversized load uses error 3 |
| `ckb::cell_data_hash_field(view)`, typed `.data_hash` | Consensus DATA_HASH field | Empty data returns zero32 | No 512-byte buffer limit | Checked field status; failed read uses error 44 |
| `ckb::cell_data_bytes32(view, offset)`; legacy `cell_data_hash_at` | Raw 32-byte slice, no hashing | Fewer than 32 remaining bytes rejects | Suffix beyond the 32-byte slice is allowed | Checked a0 status; short read uses error 1 |
| ScriptRef `.args_hash` | Exactly 32 raw Script argument bytes | Rejects args shorter than 32 | Rejects args longer than 32 | Checked output/status contract |
| Legacy `witness::raw(view)` | First 32 raw witness bytes, zero-padded | Pads with zero | Truncates to 32 | Checked a0 status on syscall failure |
| Legacy `witness::lock`, `input_type`, `output_type` | Selected WitnessArgs field projected to 32 bytes | Pads with zero | Field truncates to 32; complete serialized witness over 512 rejects before parsing | Checked a0 status on read/structure failure |
| `witness::lock_exact32`, `input_type_exact32`, `output_type_exact32` | Exact 32-byte selected field | Rejects | Rejects field width other than 32 | Checked a0 status |
| `ckb::input_out_point_index`, `input_out_point_tx_hash_low` | OutPoint index or low little-endian tx-hash word | Packed OutPoint must be exactly 36 bytes | Wrong packed width rejects | a1 must be zero before a0 is accepted; failures propagate error 44 |
| `ckb::cell_lock_hash_low`, `cell_type_hash_low` | Low little-endian word of consensus Script hash | Protocol field has fixed width; absent Type Script rejects | Remaining hash bytes are intentionally omitted | a1 must be zero before a0 is accepted; failures propagate error 44 |

Cell reads accept only their supported SourceView kinds; HeaderDep is not a
Cell-data source. Source identity and authorization remain application rules.
Raw SourceView handles do not grant authority. Missing coordinates and invalid
sources reject according to each existing helper's error family.

## Migration choices

- For Cell identity, choose `.data_hash` or `cell_data_hash_field`. Review the
  empty-data result and removal of the legacy size restriction before replacing
  `cell_data_hash`; these are different contracts, not aliases.
- For a raw 32-byte range, use `cell_data_bytes32`; renaming `cell_data_hash_at`
  preserves machine behavior. Use `cell_data_blake2b_span` when a digest of an
  exact bounded range is required.
- Treat `.args_hash` as an exact-width raw args read. A full Script hash also
  commits to code_hash and hash_type; it is a different operation. Arbitrary
  argument protocols need their own explicit length/serialization contract.
- Use exact32 witness reads for protocols requiring exactly 32 bytes. For
  variable-length fields, use bounded witness views, checked lengths and their
  explicit BLAKE2b operation, or exact byte comparisons over the required range.
  See the [bounded runtime foundations](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md).
- Never infer full witness equality from equality of legacy projections: equal
  first 32 bytes hide different suffixes, and zero-padding loses original length.
  Low-word hash getters likewise cannot establish full hash identity.

Existing names and projection behavior remain available for compatibility.
This guide is a migration notice, not a silent reinterpretation of stored values.

## Validation boundary

The independent checker validates complete machine bodies for the Cell-data
field, bounded memory hash and raw slice paths, and the four scalar getters
above, including their caller a1 guards. Compiler return conventions are not
accepted as proof. O0/O3 VM cases cover successful reads, failed reads and the
empty/512/oversized legacy hash distinction; rebound mutations must reject.

The xUDT short-read and legacy WitnessArgs oversized-buffer fixes have VM
regressions but no newly added complete independent machine profiles. The
return-convention inventory checks coverage and status initialization; it is
not a proof of all legacy helper semantics. Final gate status is tracked in the
[admission receipt](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md) and
[read-boundary audit](CELLSCRIPT_READ_BOUNDARY_AUDIT.md).

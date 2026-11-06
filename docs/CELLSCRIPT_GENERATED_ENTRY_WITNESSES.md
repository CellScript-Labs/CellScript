# Generated ordinary entry witnesses

`cellc gen-builder --target typescript` exports `entryWitnessContracts`,
`encodeEntryWitness(kind, name, args)` and `encodeLockEntryWitness(name, args)`.
The same contract is available as `entry_witness_contract` in the generated
builder manifest. Entry kind is `action` or `lock`; names select declarations
from that metadata, not application-specific compiler dispatch.

For a Lock declared as `authorize(witness approved: bool, witness nonce: u64)`,
the inner payload is:

```ts
const payload = sdk.encodeLockEntryWitness("authorize", [true, 7n]);
```

Pass that payload to the existing adapter's pre-signing WitnessArgs placement.
It belongs in `input_type`. It is not a serialized transaction or a complete
WitnessArgs, and does not authorize spending or replace the signer, Script
execution, deployment admission or ProtocolBundle checks. Policy dispatch and
special handle envelopes retain their own existing outer codecs.

The descriptors reuse the native parameter ABI. Script-args, cell-bound,
reference and bounded group-input bindings consume no caller argument; names
alone cannot change that ownership. Remaining arguments follow source order.
Supported scalar encodings are bool, u8/u16/u32/u64, i32, u128 and admitted
temporal u64 values. Fixed bytes require the exact declared width. Schema
bytes receive the native little-endian u32 length prefix. Unit may be omitted
or supplied as null. An entry with no payload-consuming parameters returns
empty bytes; a unit-only payload retains the native entry magic. Unsupported
parameter encodings, wrong entry kinds, missing/extra arguments, unsafe Number
integers, range violations and wrong fixed widths reject. Use bigint or exact
decimal strings for wide integers. Bytes use Uint8Array or canonical lowercase
hexadecimal with an `0x` prefix. Schema encoding does not establish the
application's semantic validity.

The complete native metadata JSON remains exported. Its TypeScript type is a
readonly recursive JSON object, with the interface hash typed explicitly;
this avoids expanding every lowering fact into an oversized declaration.
`cellDataCodecManifest`, `temporalContract`, `runtimeAccessProvenanceContract`
and the dedicated generated contracts keep their precise typed exports.

Lock-only packages have no action plans. Their generated tests explicitly
skip action-only operations and execute ordinary Lock/native byte conformance
and a ProtocolBundle runtime-interface exercise using a real Lock entry. That
runtime-interface exercise uses test adapters and is not node acceptance.

The non-financial `document_lock_builder.cell` regression generates the actual
SDK, compiles its strict TypeScript declarations and executes Node/native
parity and boundary negatives when the website's declared TypeScript dependency
is installed. An explicit local run requires it:

```sh
CELLSCRIPT_REQUIRE_TYPESCRIPT=1 cargo test --locked -p cellscript --test cli cellc_gen_builder_lock_only_typescript_executes_native_codec_parity -- --nocapture --test-threads=1
```

The separately ignored application check uses a strict existing order archive,
without recompiling its source. It compares actual archived payload and Cell
data with both the native and newly generated TypeScript codecs:

```sh
cargo test --locked -p cellscript --test finance_archive_replay archived_order_lock_sdk_matches_native_and_actual_transaction_bytes -- --ignored --nocapture
```

This local repair is tracked in [#102](https://github.com/CellScript-Labs/CellScript/issues/102).
It changes SDK generation and declaration typing, not source grammar,
metadata schemas, entry wire ABI, ISA, emitted Script bytes or checker
admission. Gates/CI were not run; broader application acceptance remains open.

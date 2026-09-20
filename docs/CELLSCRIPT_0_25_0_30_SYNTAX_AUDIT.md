# 0.25 to 0.30 syntax compatibility audit

The 2026 edition retains the checked source meaning of 0.25. New 2027 authoring
forms remain opt-in. Compiler requirements, package locks, source editions,
and artifact compatibility are separate axes; accepting old source does not
promise identical ELF bytes across compiler releases.

## Comparison boundary

The audit compared tag `v0.25.0` (`3b481eb019a1fdc358d4efbe8340d28d3e6a9441`)
with the pre-fix 0.30 candidate `9007851852072f6757686bdea2abfb79d87269c2`.
The original 0.25 compiler was built from that exact source archive. Of 86
historical source samples compiled outside their original package contexts,
52 compiled with 0.25 and all 52 also compiled with the candidate. This is a
bounded acceptance comparison, not a proof of semantic equivalence for all
programs. Package-context failures from old lock schemas were classified
separately; intentional repinning uses `cellc lock` or `cellc update`. The same
86-source comparison was repeated after the fixes: all 52 old-accepted sources
still compile. The exact original 0.25 compiler also successfully generated
ELF from the historical generic seed after formatting with the fixed compiler.

Targeted examples exposed three gaps that the broad historical sample did not
cover. The regression tests now enforce the following boundaries.

## Public generic declarations

This 0.25 program remains valid in edition 2026:

```cellscript
module legacy
struct Box<T: copy> has copy { value: T }
action main() -> u64 {
    verification
    let value: Box<u64> = Box<u64> { value: 7 }
    return value.value
}
```

Unconstrained public templates remain accepted in 2026 as well. Concrete
specializations still enforce declared abilities, bounded monomorphization,
layout validity, phantom restrictions, and the prohibition on hiding Cells.
Edition 2027 requires public non-phantom layout parameters to declare `fixed`,
`serializable`, and `non_linear`. This stricter rule belongs to the template
owner's edition, including imports across editions.

The compiler, canonical interface validator, Registry API, and independent
artifact checker agree on this distinction. Current interface editions must
match their enclosing metadata or publication record. Registry publication
continues to accept only its existing supported edition; this fix does not
expand Registry admission to preview packages.

## Full Script hashes in native lifecycle syntax

Both ordinary replacement relations and native `type_script` lifecycle
clauses accept `ScriptHash` for `exact_hash(...)`. Native replacement, pool,
and fresh-output paths share the fixed 32-byte hash representation. Existing
native-preview `Address` and `Hash` arguments remain compatibility inputs.
Ordinary replacement relations retain their stricter nominal-domain check:
use `ckb::script_hash(hash)` when explicitly interpreting a trusted raw `Hash`
as a complete Script hash. No arbitrary integer or unrelated nominal value
is accepted as a lock hash.

The native witness builder uses `EntryWitnessArg::Bytes` for the nominal
`ScriptHash` value, with exactly 32 bytes. This matches the existing nominal
fixed-byte encoding rather than introducing a new wire ABI. Real CKB-VM tests
exercise typed fresh outputs and replacement group binding.

## Formatting is not a source migration

`cellc fmt` preserves expanded generic constraints and explicit `has` clauses.
If the source already uses `fixed_value`, it keeps that compact spelling.
Formatting does not introduce `fixed_value` into a 0.25-compatible file or
remove an explicit ability declaration that an older compiler needs. Compact
and expanded constraints still produce the same canonical interface identity.
This applies to the shared formatter used by CLI and editor consumers.

Opting into new shorthand or inferred abilities is an intentional source
change. It requires a compatible compiler requirement; formatting alone does
not change that requirement or repin a package.

## Regression evidence

- `tests/syntax_compatibility.rs`: legacy declarations, strict 2027 rejection,
  formatter stability and interface identity, native hash-domain acceptance.
- `compile_file_monomorphizes_imported_generic_types_and_functions_in_their_owner_module`:
  dependency-owned contracts across both importer/owner edition directions.
- `tests/native_group_binding.rs` and `tests/native_lifecycle_binding.rs`:
  actual CKB-VM typed lock-hash execution and adversarial transaction shapes.
- Interface, independent checker, Registry, and WASM tests cover the consumer
  boundaries; the syntax matrix includes a nominal ScriptHash native seed.

Local gate results are recorded with the implementing change. Branch pushes
are separate from publication: this work creates no release tag, invokes no
Release workflow, and does not publish packages or trigger TAC.

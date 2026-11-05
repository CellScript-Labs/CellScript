# Edition 2027 semantic-foundation preview

This package exercises the experimental `0.26b` native `type_script` surface.
It deliberately keeps the current entry-witness and CKB target ABIs while
making provenance and the complete successor disposition explicit.

```bash
cellc expand examples/semantic-foundation-2027 --json
(cd examples/semantic-foundation-2027 && cellc check --target-profile ckb)
```

The grammar is a bounded preview rather than the frozen CellScript 1.0 source
contract. It currently supports one `type_script`, one `entry`, canonical
group-relative input/output ordinals, witness `input_type` provenance, enforced
claims, and exhaustive one-to-one replacement. The complete implemented
boundary, rejected forms, issue conflicts, and deferred syntax are documented
in [`docs/CELLSCRIPT_AUTHORING_GRAMMAR.md`](../../docs/CELLSCRIPT_AUTHORING_GRAMMAR.md).

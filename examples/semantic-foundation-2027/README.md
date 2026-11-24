# Edition 2027 Type Script example

The `type_script transfer` entry derives its `Token` group schema and Cell
positions from the input and output roles. The recipient is a witness field.
Its `replace` relation checks the data, Lock, Type Script and capacity of the
proposed successor.

```bash
cellc expand examples/semantic-foundation-2027 --json
(cd examples/semantic-foundation-2027 && cellc check --target-profile ckb)
```

See the [authoring grammar](../../docs/CELLSCRIPT_AUTHORING_GRAMMAR.md#explicit-type-script-group)
for the supported group bindings and dispositions.

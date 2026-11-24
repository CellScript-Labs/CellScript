# Edition 2027 Type Script example

The `token-transfer` artifact selects `action transfer` and its `Token` Type
group. The input and output roles bind group-relative Cell positions. The
recipient is a witness field.
Its `replace` relation checks the data, Lock, Type Script and capacity of the
proposed successor.

```bash
cellc expand examples/semantic-foundation-2027 --artifact token-transfer --json
(cd examples/semantic-foundation-2027 && cellc check --target-profile ckb)
```

The browser playground can open the same example by importing `Cell.toml`
and `src/main.cell` together.

See the [authoring grammar](../../docs/CELLSCRIPT_AUTHORING_GRAMMAR.md#explicit-type-script-group)
for the supported group bindings and dispositions.

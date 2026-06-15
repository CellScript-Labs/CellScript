# Collections Matrix Example

CellScript documents collection support as a matrix, not as a claim of fully
generic collection runtime support.

Recommended authoring rule:

- use fixed structs and fixed arrays when possible
- use stack-backed local `Vec<T: FixedWidth>` helpers only for bounded
  verifier-local value work
- use schema/ABI vectors such as `Vec<u8>`, `Vec<Address>`, and `Vec<Hash>`
  for Molecule/witness payloads
- use profile-gated checks for dynamic cell layouts
- treat nested dynamic containers and cell-backed collection ownership as
  schema/ABI boundary shapes only unless metadata, constraints, and verifier
  evidence prove a concrete production helper or ownership model

Current stack-backed local `Vec<T>` support is deliberately bounded compiler
lowering for verifier-local fixed-width values. It is not a production
allocation-backed collection runtime. `Vec::capacity()` reports the fixed
backing capacity (`256 / element_width`), not the requested
`Vec::with_capacity(n)` argument, and `cellc explain-generics` records each
checked instantiation with the concrete element type, width, backing model, and
helper set. The helper set preserves whether the value was constructed through
`Vec::new` or `Vec::with_capacity`. Generated raw collection symbols remain
fail-closed unless a checked allocator ABI is added.

Examples:

```cellscript
struct Snapshot {
    owner: Address,
    amount: u64,
}

action local_value_helpers(owner: Address, candidate: Address, snapshot: Snapshot) -> bool {
    verification
        let mut owners = Vec::with_capacity(2)
        owners.push(owner)
        owners.insert(0, candidate)
        owners.swap(0, 1)

        let mut snapshots = Vec::new()
        snapshots.push(snapshot)

        return owners.contains(owner) && snapshots.len() == 1
}

resource Blob has store, create, consume, replace {
    owner: Address,
    data: Vec<u8>,
}

resource FixedVotes has store, create, consume, replace {
    owner: Address,
    votes: [u64; 4],
}
```

Avoid claiming production support for shapes like:

```cellscript
resource NestedDynamic has store, create, consume, replace {
    rows: Vec<Vec<u8>>,
}

resource Token has store, create, consume, replace {
    owner: Address,
    amount: u64,
}

action hidden_ownership(tokens: Vec<Token>) -> u64 {
    verification
        return tokens.len()
}
```

Use the support matrix for the current status:

```text
docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md
```

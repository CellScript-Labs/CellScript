# Output Append Example

CellScript models updates as proposed output Cells. It does not update a CKB
cell in place.

Conceptual source shape:

```cellscript
resource Log has store, create, consume, replace {
    owner: Address,
    bytes: Vec<u8>,
}

action append(log: Log, suffix: [u8; 16]) -> next_log: Log
where
    let next = log.bytes
    next.extend_from_slice(&suffix)
    create next_log = Log {
        owner: log.owner,
        bytes: next
    }
```

Expected transaction shape:

- one input consumes the old `Log`
- one output creates the updated `Log`
- preserved fields such as `owner` must be constrained explicitly
- changed fields such as `bytes` must satisfy the compiled transition checks

Relevant inspection commands:

```bash
cellc check contract.cell --target-profile ckb
cellc constraints contract.cell --target-profile ckb --json
```

For CKB, the builder must also provide occupied-capacity and transaction-size
evidence for the proposed output.

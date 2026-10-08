# Fixed 52-byte context

The AgoraSeal vote example keeps its upstream business rules and replaces only
the hand-written byte pushes with existing bounded loops. Both `main.cell.in`
and generated `main.cell`, and both manifest forms, bind the same pinned child.
The input baseline is AgoraSeal commit
`5b6106a7b86d51b736a7df772ded548fba4c9a4e`. The upstream MIT license is retained
with the [vendored child](../../tests/fixtures/byte-context-child/LICENSE).
The user's separate AgoraSeal checkout is not modified by this example.

| Context bytes | Meaning |
| --- | --- |
| 0–35 | Exact 36-byte OutPoint from ballot data 8–43 |
| 36–43 | Claimed deposit creation height, u64 little endian |
| 44–51 | Claimed proposal creation height, u64 little endian |

```cell
let mut context: Vec<u8> = Vec::with_capacity(52)
for i in 8..44 { context.push(ckb::cell_data_u8(ballot, i) as u8) }
for i in 0..8 { context.push(((deposit_height >> (i * 8)) & 255) as u8) }
for i in 0..8 { context.push(((proposal_height >> (i * 8)) & 255) as u8) }
```

The parent requires an exact 53-byte ballot and passes segments `(36, 8, 8, 0)`
to `ckb::trusted_spawn_wait_cell_dep_hex4`. Heights remain witness claims until
the exact child checks the corresponding committed header coordinates. Local
Vec backing is still 256 bytes; `with_capacity(52)` does not allocate a new
52-byte heap buffer. No generic byte helper, heap collection or nested dynamic
codec is required for this case.

`tests/bounded_byte_context.rs` executes both loop and unrolled encoders against
an independent Rust byte oracle. O0–O3 cover zero, maximum and non-palindromic
heights; the latter fixture makes all 52 context bytes distinct, including the
OutPoint's four index bytes. There are 48 positive executions with scheduler
replays, split between encoder-only and real parent/child contexts, and 40
negative executions. Truncated ballots, out-of-range reads, backing overflow,
wrong length/OutPoint/oracle/dependency/code hash, false heights and missing
header dependencies fail closed. The child really parses argv and rejects
invalid context; it is not an always-success replacement.

Initial matched fixture observations (maximum over the three vectors):

| O3 variant | ELF bytes | Group cycles | Parent stack | Child stack |
| --- | ---: | ---: | ---: | ---: |
| Unrolled encoder only | 9,584 | 16,299 | 9,664 | — |
| Looped encoder only | 3,696 | 16,012 | 8,496 | — |
| Unrolled parent + child | 11,240 | 204,068 | 10,256 | 66,704 |
| Looped parent + child | 5,040 | 203,626 | 9,072 | 66,704 |

Looping increases attributed parent instruction/syscall cycles while reducing
ELF loading cost and parent frame size. Reports retain scheduler/loading
overhead and each VM separately. The child is byte-identical between variants.
The Rust oracle checks bytes; these costs compare matched CellScript encoders,
not the total cost of AgoraSeal's different Rust vote architecture. Full vote
source has strict compilation and checker evidence here; the runtime claim is
the bounded context acceptance scene, not whole voting-protocol equivalence.
The new fixture ceilings are separate from existing frozen cost budgets.

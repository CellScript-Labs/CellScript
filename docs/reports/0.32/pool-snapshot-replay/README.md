# Transaction-pool snapshot and clean-source replay

The complete dev and backend gates passed for signed source
`23822919ac1d4f8a710df04d536a48717ef8fed9`; backend used
`CELLSCRIPT_COUNTER_CCC=1` and pinned CKB `f7fa4436737756f97a24e254f22c13a36316ecea`.
The branch was pushed after this successful replay. The containing archive
commit is a later state; this record does not transfer evidence to later code.

[manifest.json](manifest.json) binds exact compressed and raw file sizes and
SHA-256 digests. Gzip timestamps are normalized without changing decompressed
bytes. It preserves the complete successful dev/backend logs, strict backend
report, clean-source acceptance receipt and fresh node/migration evidence.

The previous backend failure on `34ce0fec` and its funding inspection are
retained separately. The rejected funding OutPoint was the live latest-block
cellbase. The harness now waits for the transaction pool's hash AND height to
match the core snapshot before submission and after commitment. Malformed tips,
bounded timeout and any rejected transaction fail closed; rejected transactions
are never resubmitted. The fresh stateful acceptance replay passes without
changing Script logic, transaction encoding or existing resource ceilings.

The fresh CCC migration confirms a continuous chain with actual fee signatures:

| Transition | CKB cycles | Full transaction bytes |
| --- | ---: | ---: |
| Old-key update | 111,709,244 | 1,237 |
| Authorized migration | 111,880,253 | 1,632 |
| New-key successor update | 111,878,191 | 1,237 |

Old-key use after migration, new-key self-installation, second migration, stale
configuration and transaction mutation reject. This is a disposable local chain
and both setups use public test seeds. It establishes no production setup trust,
privacy, public deployment or generic lifecycle admission. The earlier
[CCC replay](../resumed-ccc/README.md) remains unchanged historical evidence.

This is backend/dev evidence, not whole-queue CI or stable release admission.
Full #28/#29 and #44–#46 remain unfinished; independent security review remains
required, unassigned and unwaived. Full CI is deferred until the entire ordered
implementation queue is complete.

# Financial transaction archive replay

The v2 local-VM archive stores each transaction's original Molecule bytes,
resolved input Cells, direct code dependency Cells, and accepted or rejected
Script expectations. Each program includes its exact four-file artifact bundle,
frozen source context and the actual source bytes captured during its original
compilation. Replaying an archive performs no CellScript source compilation.

File hashes, source-unit/content/location hashes, frozen-context identities,
checked module projections, Cell output/data/content hashes, transaction and
witness-inclusive serialized hashes, case identities and the complete archive
identity bind the recorded bytes. The loader independently checks each artifact
bundle and source closure. These identities detect inconsistent modifications;
they do not authenticate the archive publisher or a remote live Cell set.

`tests/support/finance_archive.rs` exposes `load_archive`, immutable program/case
records, and `load_case(index)`. Every case restoration creates a new empty
`ckb-testtool::Context` using only the report's dependency and input bytes. It
does not reuse a producer Ledger, call `complete_tx`, insert a replacement
verifier, rewrite witnesses or repair a transaction. The current profile admits
direct code dependencies and transactions without header dependencies.

Accepted cases must execute nonzero cycles and match their archived ordinary
transaction cycle count. Rejected cases must fail with `ValidationFailure` in
one of the recorded expected Script groups. Missing resolution, VM/tool errors
and unexpected success are separate failures. Actual outcomes are written to
`fresh-vm-replay.json` with the archive and case identities.

Local stale-input checks belong in `local_observations`. They retain their own
transaction hashes and observation identity, without fabricated resolved input
Cells, Script failures or VM cycles. The replay report preserves their local
bookkeeping scope and does not execute them as negative Script cases.

An accepted record may say `local_bookkeeping=not-consumed-vm-preflight`.
It retains a real complete-transaction execution and actual cycles, while the
producer intentionally leaves its predecessors live. C1/C2 use this to verify
equivalent candidate layouts against the same predecessor set and then commit
one candidate locally. The archive loader still restores and executes every
candidate independently. Such a record does not claim that two conflicting
transactions both committed or that a node accepted either one.

After the owning finance tests finish producing fresh v2 archives, run:

```sh
cargo test --locked -p cellscript --test finance_archive_replay -- --ignored --nocapture
```

The independent owner replays every archive under
`target/finance-protocol-evidence`. Set `CELLSCRIPT_FINANCE_ARCHIVE` to one archive
directory to select a single snapshot. It also checks malformed or substituted
transactions, missing/tampered inputs and dependencies, changed source and ELF
files, and a signature substitution whose checksum identities are all rebound
before actual VM rejection.

This is archived-byte VM evidence. Node submission/finality, optional external
reproduction and financial resource ceilings remain separate. `Context::verify_tx`
does not return cycles on rejection; separately replayed group cycles keep their
own scope. Stack observation belongs to the existing scheduler trace used by
`tests/finance_costs.rs`; this archive replay reports that it did not measure
stack rather than inventing a value.

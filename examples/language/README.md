# Language Examples

The checked-in language examples are organized by the behavior they teach or
verify. Directory names and `.cell` filenames are semantic and stable across
release lines:

| Directory | Scope |
|---|---|
| `core/` | Canonical syntax, formatting, and standard-library surface |
| `ckb/` | CKB syscalls, sources, hashes, capacity, time, and TYPE_ID behavior |
| `ownership/` | Linear ownership, borrowing, and resource lifecycle |
| `verification/` | Invariants, proof-plan behavior, and transaction views |
| `collections/` | Bounded collection and local collection patterns |
| `batches/` | Dynamic bounded Cell sets, output plans, and atomic batch flows |

`ownership/semantic_foundation.cell` is deliberately valid under both the
stable 2026 frontend and the experimental 2027 frontend. It demonstrates
explicit input/witness provenance and successor-based Cell disposition. Run
`cellc expand` in a package containing the source to inspect the canonical
semantic foundation; the human rendering is diagnostic and is not a hash
boundary.

For the canonical native spelling, use the locked
[`examples/semantic-foundation-2027`](../semantic-foundation-2027/README.md)
and [`examples/lock-script-2027`](../lock-script-2027/README.md) packages. They
are intentionally bounded Edition 2027 previews rather than stable-language
examples.

[`batches/multi_asset_transfer.cell`](batches/multi_asset_transfer.cell) combines
experimental named collections with actual Cell reads: Maps conserve per-asset
amounts and Sets reject duplicate note serials. Its complete transaction tests
cover split/merge, maximum amounts, hostile spending Locks and independent host
model comparisons at O0/O3. This is a transfer-only example within one Type group;
allocation/source admission and frozen application resource budgets remain pending.
See the [collection evidence](../../docs/CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#full-transaction-collection-aggregation).

[`batches/nested_migration.cell`](batches/nested_migration.cell) is an experimental
Edition 2027 example using named roles, bounded messages and read-only helpers.
It preserves document histories while appending a release note and advancing
revisions alongside an independently executing Registry. Its hashes are
illustrative; the all-group VM tests bind them to fixture Scripts. A complete
SDK/ProtocolBundle fixture now compares native and generated nested bytes and
executes the packed transaction. Stable artifact/source admission remains blocked;
see the [evidence ledger](../../docs/CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#executable-nested-batch-migration).

[`batches/record_migration.cell`](batches/record_migration.cell) adds explicit
revision identities to each message and checks strictly increasing history.
The complete named and [explicit-bound programs](../../tests/selection_bundle/fixtures/record_migration_explicit.cell)
produce identical SDK/adapter transaction bytes in O0/O3 all-group fixtures.
Tests cover the aggregate byte limit, duplicate history and incorrect new
identities; stable admission remains blocked. See the
[record-list evidence](../../docs/CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#record-list-migration-and-authoring-comparison).

[`batches/mixed_settlement.cell`](batches/mixed_settlement.cell) checks bounded
full-fill orders between two exact asset roles, with extracted read-only helpers.
The all-group fixture separately runs both asset conservation checks and a Lock,
compares named/positional authoring, and rejects hostile balance and peer cases.
It is an experimental compiler example with illustrative hashes and test custody;
see the [contract and evidence](../../docs/CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#executable-fixed-mixed-role-settlement).

Do not put a release or version number in a `.cell` filename. Names such as
`v0_26_batch.cell`, `contract-v1.cell`, and `token_0.26.0.cell` are rejected by
the native source-policy gate. Describe the example's behavior in its filename
and record release history in `CHANGELOG.md` or release notes instead.

These examples exercise compiler and tooling surfaces. Unless a document says
otherwise, they are not part of the production CKB acceptance matrix.

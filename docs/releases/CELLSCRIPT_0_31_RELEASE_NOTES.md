# CellScript 0.31.0

Release date: 2026-10-01. The maintainer explicitly authorized direct publication
and waived the pending independent review and release-evidence requirements for
this version. This exception does not claim that the full release gate passed or
that new public-testnet deployments were performed. The earlier 0.30 receipts
continue to describe their original artifacts only.

Compiler, standalone checker, adapters, native tools, browser compiler, Registry
verifier packages and VS Code extension identify 0.31.0. Native distributions,
the browser bundle and the VSIX retain their separate supported boundaries.

## Compiler changes

The compiler emits shorter RV64 immediate sequences, shares policy decoders
whose complete callable layouts match, sizes fixed private adapters from proven
requirements, and reuses eligible scalar stack slots using CFG liveness.
Unclassified layouts retain conservative fallbacks. The outer 4,096-byte witness
loading limit, private-copy ownership, linear action-tag dispatch, common-check
order and source-language semantics are preserved.

The language gains no new syntax in this release. Edition 2026 and the bounded
Edition 2027 source identity remain as in 0.30. Existing witness payload and
placement formats are unchanged.

Loop fusion retains the original loop when its prefix defines a byte offset
or memory pointer needed by the replacement. Immediate-planner errors propagate
as assembly diagnostics. The self-hosted Registry ignores caller-provided
IP/ASN identities and uses the socket peer or its configured trusted proxy
chain; operators must match `REGISTRY_TRUSTED_PROXY_HOPS` to their topology.

## Artifact compatibility

Native builds emit lowering record v9. Use the matching standalone checker and
Registry/CKB consumers; unsupported record versions are rejected. Typed semantics
v8, source-map v2 and metadata schema 72 retain their existing contracts.
Regenerate the ELF, metadata, lowering record and source map as a complete bundle.
Rebuilding may change Script identities and transaction group ordering; an
invalid transaction can consequently report a different first failing group.
Existing deployed code retains its original identity.

## Cost evidence

Cost-report v2 records measured nonzero Script-group exits and explicit
unavailable observations. Individual frame sizes and static call-chain stack
bounds are separate metrics. Scalar rows also report static load/store instruction
counts; these are not executed memory traffic. The expanded corpus retains the
matched Rust and growth fixtures, tests every action in the frozen policy
matrix, and adds scalar/call/generic and multi-Script measurements.

The [reproduction package](../reports/0.31/README.md) records the exact baseline
patch, fixture hashes, comparison command and independent review scope. The
[implementation report](../CELLSCRIPT_0_31_COST_IMPLEMENTATION.md) contains the
measured development results. No global optimality or arbitrary Rust-equivalence
claim follows from these samples.

## Validation and limits

The local `dev`, `ci` and `backend` gates passed on candidate `6c111a30`.
The pinned local CKB replay passed 43 actions, 17 Lock matrices and 26 stateful
scenarios. The version-aligned cost comparison passed 6,782 comparable metrics
with the existing budgets. Canonical WASM packaging and VSIX validation passed.
The browser bundle is 572,053 bytes gzip, within its 600 KB budget.

The complete release gate stopped at pending acceptance records. Publication
uses the maintainer's explicit 0.31 exception, not a claim of independent review,
a transferred 0.30 waiver, or fresh Pudge deployment evidence. The normal gate
remains unchanged for other versions. These tests do not establish general
production equivalence or constitute an independent security audit.

Observed stack peaks, alternative dispatch trees/tables, borrowed witness spans,
and general register allocation remain outside this release. The browser path
continues to compile metadata only.

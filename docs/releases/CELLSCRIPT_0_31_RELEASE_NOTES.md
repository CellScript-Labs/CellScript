# CellScript 0.31 — release candidate notes

Status: preparation only. Package and editor manifests identify 0.31.0;
0.31 has not been published. The independent boundary review, fresh downstream
bundles and final full release gate remain prerequisites. See the
[candidate readiness record](CELLSCRIPT_0_31_RELEASE_READINESS.md) for current
evidence. This document is not a publication or deployment receipt.

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

Implementation validation uses `./scripts/cellscript_gate.sh backend` and
`./scripts/cellscript_gate.sh ci`. Release acceptance additionally requires
`./scripts/cellscript_gate.sh release` on clean source, with coordinated compiler,
checker, editor and browser artifacts and a named independent boundary review.
The 0.30 review waiver and Pudge deployment receipts are not 0.31 evidence.

Observed stack peaks, alternative dispatch trees/tables, borrowed witness spans,
and general register allocation remain outside this release. The browser path
continues to compile metadata only.

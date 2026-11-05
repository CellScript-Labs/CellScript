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

## Scope limits

Observed stack peaks, alternative dispatch trees/tables, borrowed witness spans,
and general register allocation remain outside this release. The browser path
continues to compile metadata only.

# NovaSeal Dual Seal Profile v0

**Status**: reviewable seal profile package. It is not V1-ready and not
production ready because live finality evidence, wallet vectors, public BTC
closure-verification evidence, and external attestations are still missing.

This package implements the planned NovaSeal dual seal profile as a source-level
package with schemas, fixtures, invariant matrix, and security boundary
documentation.

## Boundary

`finalize_dual_seal` combines:

- sealed BTC UTXO commitment hash,
- declared BTC closure commitment hash,
- old and new CKB state hashes,
- CKB maturity timepoint,
- BTC owner BIP340 authority,
- CKB BIP340 authority.

This package does not prove BTC inclusion, UTXO spend validity, economic
finality, or live CKB maturity by itself.

## Evidence

| Area | Status | Classification |
| --- | --- | --- |
| Separate dual-seal profile package | implemented | source-guard-present |
| Canonical NovaSeal envelope binding | implemented | source-guard-present |
| BTC closure binding | implemented | source-guard-present |
| CKB maturity guard | implemented | source-guard-present |
| Dual authority signatures | implemented | source-guard-present |
| Schemas and fixture labels | implemented | reviewable |
| Invariant matrix | implemented | reviewable |
| Live devnet dual-seal finality path | missing | missing-live-devnet-evidence |
| Public BTC closure verification evidence | missing | missing-spv-or-indexer-evidence |
| Wallet signing vectors | missing | missing-wallet-evidence |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `seal_profile_dual_seal` as a package
implementation only when the certification gate sees this manifest, source
action, schemas, fixtures, docs, and invariant matrix. Live dual-seal finality
must remain blocked until devnet stateful evidence and public BTC verification
evidence are generated and checked.

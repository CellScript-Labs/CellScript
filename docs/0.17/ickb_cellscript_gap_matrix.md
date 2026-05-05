# iCKB CellScript Expressibility Matrix

This matrix is a historical gap tracker for the iCKB workstream. Its current
status is:

```text
0.17 closed the CKB protocol-helper layer.
0.18 closed the verifier/protocol-equivalence layer for the declared executable iCKB claim set.
0.19 owns registry, deployment, live-cell resolution, and Action Builder work.
```

It must not be read as a list of active 0.18 blockers. Active release evidence
is enforced by:

- `tests/benchmarks/ickb_diff/matrix.json`
- `tests/benchmarks/ickb_diff/claim_manifest.json`
- `tests/ickb_diff.rs`
- `cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/benchmarks/ickb_diff/claim_manifest.json --json`

## Evidence Levels

- `CLOSED`: covered for the manifest-declared executable claim set by
  original-vs-CellScript CKB VM differential rows.
- `FIXTURE_SCOPED`: covered by executable fixture rows or verifier helpers, but
  not promoted into a broad language abstraction.
- `0.19_SCOPE`: deployment, registry, builder, or production transaction
  materialisation work.
- `OUT_OF_SCOPE`: external audit, mainnet-value certification, UI, or off-chain
  ecosystem behaviour.

## Current Matrix

| iCKB semantic item | Current status | Evidence | Remaining work |
|---|---|---|---|
| Linear receipt consumption | CLOSED | receipt consume, duplicate receipt output, receipt group over/under/malformed rows | none for declared claim set |
| DAO deposit / receipt output grouping | CLOSED | deposit phase 1, forged receipt, receipt mismatch, capacity-boundary rows | generic aggregate syntax is future ergonomics |
| Deposit min/max capacity | CLOSED | deposit too-small / too-big rows and capacity evidence per differential row | none for declared claim set |
| Oversized-deposit discount formula | CLOSED | direct CellScript arithmetic plus differential deposit/mint rows | broader arithmetic ergonomics only |
| Header/input accumulated-rate load | CLOSED | DAO missing/wrong header, wrong accumulated-rate, input/HeaderDep lineage rows | automatic lineage sugar is future ergonomics |
| Receipt and DAO exact accounting | CLOSED | receipt mint, receipt group, amount high-word, DAO two/three-input max/over, mixed deposit-rate, mixed withdraw-rate rows | generic aggregate lowering can improve syntax later |
| xUDT owner-mode args | CLOSED | xUDT wrong args, owner-mode, current-script hash, exact Script matching rows | registry-backed deployed ABI linkage is 0.19 |
| xUDT group amount conservation/delta | CLOSED | executable conservation, minted-delta, burned-delta helpers plus differential rows | automatic helper insertion is future ergonomics |
| Script used as lock and type | CLOSED | current-role, current/output empty args, full lock/type hash, exact Script construction/matching rows | deployment manifest resolution is 0.19 |
| Empty args and 32-byte args binding | CLOSED | Molecule Script args helpers and exact args hash rows | arbitrary UI/builder presentation is 0.19+ |
| CKB occupied/unoccupied capacity | CLOSED | SourceView capacity helpers and per-row occupied-capacity evidence | builder capacity balancing is 0.19 |
| NervosDAO data classification | CLOSED | exact 8-byte deposit/withdrawal data classifier rows | none for declared claim set |
| DAO maturity / withdrawal | CLOSED | mature/immature, max/over, two/three-input aggregate, malformed second/third witness/header-index rows | additional adversarial rows are hardening, not blockers |
| Limit Order C256 arithmetic | CLOSED | product/sum helpers and valid/min/underpayment/no-payment/wrong-asset rows | first-class `u256` ergonomics can come later |
| Limit Order MetaPoint/OutPoint binding | CLOSED for claim set; FIXTURE_SCOPED for high-level map API | full input OutPoint tx-hash/index, full OutPoint binding, relative MetaPoint, master-OutPoint rows | high-level MetaPoint map/query API is future ergonomics |
| Owned-Owner signed i32 distance | CLOSED | signed i32 ABI, relative mismatch, missing/duplicate owner, related type/data rows | none for declared claim set |
| Witness malformation | CLOSED for claim set | WitnessArgs parser tests plus DAO missing/empty/short/long/wrong-index rows | separate owner-auth witness payload remains out-of-claim unless original executable evidence appears |
| CellDep / deployment dep-group binding | 0.19_SCOPE | differential rows record artifact hashes and fixture CellDeps | registry-backed CellDep solving and deployment verification |
| Non-upgradable deployment proof | 0.19_SCOPE | deployment manifest design docs | external audit and chain deployment certification |
| CoBuild / OTX / wallet UI | OUT_OF_SCOPE | none claimed | ecosystem/UI work |

## Remaining Work

There are no active 0.18 protocol-equivalence blockers for the declared
executable iCKB claim set.

Remaining work belongs to either hardening or 0.19 production usability:

1. Add more adversarial VM differential rows when the public claim set expands.
2. Keep fixture-scoped iCKB bridge helpers out of generic compiler semantics
   unless they become protocol-neutral abstractions.
3. Build the 0.19 registry/deployment/Action Builder layer for live-cell
   resolution, CellDep solving, CCC-compatible transaction drafts, fee/capacity
   balancing, and preview data.
4. Do not claim external audit, mainnet-value certification, or exhaustive
   state-space verification without separate evidence.

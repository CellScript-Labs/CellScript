# NovaSeal / RGB++ Comparison And Optimisation Proposal

## Source Baseline

This comparison uses the current local checkouts:

| Source | Repository | Branch | Commit | Role |
| --- | --- | --- | --- | --- |
| RGB++ SDK | `https://github.com/ckb-cell/rgbpp-sdk.git` | `develop` | `2d547132ede28616647e87d603aea63daada4841` | transaction builders, BTC embedding, SPV witness plumbing, xUDT flows |
| RGB++ design | `https://github.com/ckb-cell/RGBPlusPlus-design.git` | `main` | `c0b065c8bb8cc0a1813d27e9352ff694e1975ca3` | light paper, security analysis, lockscript design |
| NovaSeal | `CellScript` `research/nsv1` | local branch | current checked-in source plus `target/novaseal-production-gates.json` | package-first profiles, certification gate, devnet evidence |

The comparison does not treat RGB++ as a library to vendor. It treats RGB++ as
a mature neighbouring protocol whose sequencing, lockscript, and SDK surfaces
are useful design pressure for NovaSeal.

## Executive Conclusion

RGB++ is stronger where the problem is "bind a Bitcoin UTXO to a CKB asset
transition and expose a practical SDK flow". It has a precise BTC transaction
commitment path, SPV witness path, BTC time-lock model, and xUDT-oriented
transaction builders.

NovaSeal is stronger where the problem is "make a CKB object profile
reviewable, typed, auditable, and locally certifiable before production
attestation". Its current advantage is evidence discipline: canonical schema
hashes, profile packages, invariant matrices, wallet/service fixtures, live
devnet stateful reports, and a compiler-hosted certification gate.

The optimisation direction is therefore not to copy RGB++. NovaSeal should keep
its package-first model, but import three engineering lessons from RGB++:

1. Treat BTC transaction sequencing as a first-class profile contract, not only
   as external SPV evidence.
2. Model delayed finality and time-lock risk explicitly for every BTC-facing
   profile.
3. Add SDK-style builder preflight reports so wallet/service builders can prove
   the same invariants that the certification gate expects.

## Design Comparison

| Dimension | RGB++ evidence | NovaSeal evidence | Assessment |
| --- | --- | --- | --- |
| Core abstraction | Isomorphic binding: BTC UTXO ownership maps to CKB Cell state. | Canonical envelope plus profile-specific CKB object transitions. | RGB++ is narrower and more operational for assets; NovaSeal is more general and reviewable for typed object profiles. |
| BTC linkage | BTC transaction carries an OP_RETURN commitment; CKB witness carries BTC tx and SPV proof. | Key authority is local-ready; BTC transaction commitment, UTXO seal, and dual seal profiles are present but production-blocked on public BTC SPV evidence. | NovaSeal should make BTC commitment sequencing more explicit in profile docs and builder evidence. |
| Commitment window | RGB++ commits only the first typed inputs/outputs and allows later null-type fee adjustment. | NovaSeal commits to canonical envelope, state commitments, payout commitments, and receipt commitments. | NovaSeal has stronger semantic commitments, but needs an explicit "mutable fee/change window" rule for service builders. |
| Finality and reorg handling | RGB++ defines L1/L2/jump security, BTC confirmation depth, and `BTC_TIME_lock`. | NovaSeal documents BTC-facing external blockers and dual-seal maturity, but no common finality matrix exists across all profiles. | Add a cross-profile finality matrix. |
| Developer ergonomics | SDK builders produce virtual CKB txs, BTC PSBTs, paymaster handling, SPV witness attachment, and placeholder BTC txid replacement. | NovaSeal has wallet fixtures, service-builder fixtures, and certification reports, but no single builder-facing transaction sequencing report. | Add a NovaSeal builder preflight report modelled as evidence, not as a new runtime dependency. |
| Security posture | RGB++ security docs reason about PoW reorgs, jump operations, permanent-lock failure modes, and time-lock mitigation. | NovaSeal security docs reason about TCB, authority binding, exact external blockers, and local-vs-production boundaries. | Combine both styles: preserve NovaSeal blocker honesty while adding RGB++-style sequencing failure cases. |

## Optimisation Proposal

### 1. Add A BTC-Facing Sequencing Contract To NovaSeal Evidence

Current state:

- `btc-transaction-commitment-profile-v0`, `btc-utxo-seal-profile-v0`, and
  `dual-seal-profile-v0` exist as planned profiles with live devnet stateful
  evidence.
- Production remains correctly blocked on `public_btc_spv_evidence_attested`.
- The public BTC SPV template requires external cases for BTC-facing profiles.

Gap:

The certification gate knows that public BTC evidence is required, but builders
do not yet get an RGB++-style sequencing checklist: build CKB transition,
derive commitment, embed in BTC transaction, wait for public proof, attach SPV
witness, then submit or certify the CKB transition.

Proposed artefact:

`target/novaseal-btc-sequencing-preflight.json`

Required fields:

- `profile`
- `scenario`
- `ckb_virtual_transition_hash`
- `btc_commitment_hash`
- `btc_commitment_method`
- `mutable_ckb_window`
- `typed_input_count`
- `typed_output_count`
- `spv_evidence_required`
- `minimum_btc_confirmations`
- `failure_cases`

Acceptance gate:

- `cellc certify --plugin novaseal-profile-v0` should treat the report as
  local builder evidence only.
- `--require-production` must still fail until public BTC SPV evidence is
  supplied by an external provider.

### 2. Define A Cross-Profile Finality Matrix

Current state:

NovaSeal has profile-specific security docs and dual-seal evidence, but the
finality semantics are dispersed across package docs and external blockers.

Proposed artefact:

`proposals/novaseal/v0-mvp-skeleton/docs/FINALITY_MATRIX.md`

Rows:

- Agreement key-authority profile: CKB finality only; no BTC finality claim.
- Fungible xUDT profile: CKB finality plus service-builder balance evidence.
- BTC transaction commitment profile: BTC transaction inclusion and public SPV
  evidence required.
- BTC UTXO seal profile: BTC single-use seal evidence required.
- Dual seal profile: BTC closure plus CKB maturity evidence required.
- Fiber candidate profile: CKB stateful evidence plus external Fiber workflow
  evidence; no in-contract Fiber HTLC/route/liquidity verification.
- RWA receipt profile: CKB stateful evidence plus external legal/registry
  review evidence.

Acceptance gate:

- The security audit coverage check should require this matrix once the doc is
  added.
- The matrix must not state `production_ready = true` while any external
  blocker remains.

### 3. Add Builder-Preflight Parity With RGB++ SDK Flows

RGB++ SDK builders expose concrete hazards that NovaSeal should make explicit:

- input/output typed coverage,
- commitment mismatch,
- unsupported asset type,
- insufficient capacity,
- paymaster mismatch,
- placeholder replacement,
- SPV witness attachment.

NovaSeal already has `target/novaseal-service-builder-fixtures.json`, but it
should evolve into a stricter builder preflight report.

Proposed checks:

- every profile action has a builder scenario,
- every scenario records old Cell, new Cell, receipt, and payout output mapping,
- every BTC-facing scenario records whether the BTC commitment is public,
- every value-bearing scenario records capacity floor and fee/change policy,
- every external dependency records whether it is local evidence, public
  attestation, or production blocker.

Acceptance gate:

- local certification may pass with local builder evidence;
- production certification must still require public/shared CellDep, public BTC
  SPV, RWA legal/registry review, and external BIP340 TCB review.

### 4. Preserve NovaSeal's Stronger Audit Boundary

Do not replace NovaSeal's compiler-hosted certification gate with SDK-only
assumptions.

RGB++ is operationally elegant because builders create the BTC and CKB sides
together. NovaSeal should adopt that ergonomics in reports and tooling, but the
claim "this is a NovaSeal profile" should remain a deterministic compiler
certification result over checked-in package files and generated evidence.

Acceptance gate:

- no profile may claim NovaSeal status from SDK construction alone;
- every profile claim must still pass `cellc certify --plugin
  novaseal-profile-v0`;
- `--require-production` must remain fail-closed on external blockers.

### 5. Explicitly Reject The Wrong Direction

The following would make NovaSeal less robust:

- vendoring RGB++ SDK logic into the certification gate;
- treating BTC key signatures as BTC UTXO seals;
- claiming RGB++ equivalence before public BTC SPV evidence exists;
- letting service-builder fixtures become production attestations;
- making the canonical envelope carry product-specific RGB++ xUDT policy.

NovaSeal should remain a profile framework. RGB++ should remain a reference for
BTC/CKB sequencing discipline, not a hidden dependency.

## Proposed Next Work Order

| Priority | Work | Why |
| --- | --- | --- |
| P0 | Add `FINALITY_MATRIX.md` and make security audit coverage check it. | Turns the RGB++ reorg/time-lock lesson into a NovaSeal-wide invariant. |
| P0 | Add `novaseal-btc-sequencing-preflight` generator for BTC-facing profiles. | Makes BTC commitment sequencing reviewable before external SPV evidence arrives. |
| P1 | Extend service-builder fixtures with typed input/output coverage and fee/change policy. | Gives wallets/services RGB++-style ergonomics without weakening certification. |
| P1 | Add negative tests for commitment-window drift in BTC-facing profile reports. | Prevents builders from moving typed outputs outside the committed window. |
| P2 | Add an optional paymaster/capacity preflight report. | Useful for production builders, but not part of the core security claim. |

## Stop Condition For This Comparison Slice

This proposal is complete when:

1. The RGB++ repositories and commits used for comparison are recorded.
2. The proposal identifies concrete NovaSeal changes rather than generic
   praise or criticism.
3. The proposal preserves the current production blocker boundary.
4. The proposal is committed on `research/nsv1`.

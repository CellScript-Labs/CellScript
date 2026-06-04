# NovaSeal V1 vs RGB++ Comparison and Optimisation Proposal

## Evidence Base

- NovaSeal repository: `research/nsv1`, after commit `3db1afb8`.
- NovaSeal local gate: `target/novaseal-production-gates.json` reports `local_production_prep_ready_external_attestation_required`.
- NovaSeal planned-profile operator fixtures: `target/novaseal-profile-operator-fixtures.json` covers the planned profile signing and witness surfaces.
- NovaSeal service-builder fixtures: `target/novaseal-service-builder-fixtures.json` covers deterministic request/response skeletons for the planned profile actions.
- NovaSeal BTC SPV evidence adapter: `target/novaseal-btc-spv-evidence-adapter.json` covers the external evidence request contract for BTC-facing profiles.
- NovaSeal external attestation adapter: `target/novaseal-external-attestation-adapter.json` covers the public/shared CellDep and external BIP340 TCB review request contracts.
- NovaSeal planned-profile stateful matrix: all planned live scenarios pass with no missing entries.
- NovaSeal external Fiber-node matrix: `target/novaseal-fiber-node-experiments.json` reports `16/16` required suites present, executed, and passed, including embedded and separate-service cross-chain hub send-BTC and receive-BTC workflows.
- RGB++ active SDK clone: `/Users/arthur/RustroverProjects/rgbpp-sdk-active`, commit `ee21eb9735c1adeb277e3a02b7f6c2f6fd1d0556`.
- RGB++ archived SDK reference: `/Users/arthur/RustroverProjects/rgbpp-sdk`, commit `2d547132ede28616647e87d603aea63daada4841`.
- RGB++ design clone: `/Users/arthur/RustroverProjects/RGBPlusPlus-design`, commit `c0b065c8bb8cc0a1813d27e9352ff694e1975ca3`.

## Summary

RGB++ is more mature in Bitcoin/CKB operational integration. It has explicit isomorphic binding, BTC SPV service integration, BTC time lock handling, paymaster handling, service APIs, SDK examples, and workflow-oriented transaction builders.

NovaSeal is cleaner as a typed contract and certification framework. Its strengths are explicit profile packages, canonical envelopes, negative-case live reports, source/artifact provenance, a single certification gate that now verifies all planned profile live paths, planned-profile operator fixtures, service-builder fixtures, BTC SPV and external-attestation adapter requests, and a separate Fiber-node execution matrix that now passes across the tracked Fiber workflows. Its main remaining weakness is no longer basic Fiber execution or first-pass builder fixture binding. It is the absence of real production-grade external BTC SPV attestations, public/shared CellDep attestation, external BIP340 verifier TCB attestation, and reusable SDK/service libraries that make those facts easy for wallets and operators to reproduce in production systems.

## Comparison

| Area | RGB++ | NovaSeal | Assessment |
| --- | --- | --- | --- |
| Core model | Isomorphic BTC UTXO to CKB Cell binding. | Typed NovaSeal profiles with canonical signed envelopes. | RGB++ is operationally concrete; NovaSeal is more formally structured. |
| Workflow maturity | SDK builds virtual CKB tx, BTC tx commitment, queue/service flow, SPV proof retrieval. | Live devnet scripts now exercise all planned profile scenarios; Fiber-node experiments now execute the tracked channel, watchtower, UDT, and cross-chain hub workflows. | RGB++ still has stronger product workflow integration; NovaSeal now has stronger machine-checked local and Fiber regression evidence. |
| Contract clarity | Lock scripts and BTC time lock focus on RGB++ asset ownership. | Profile-specific CellScript sources encode business intent directly. | NovaSeal is easier to audit per business profile. |
| Security posture | Strong BTC confirmation/SPV/time-lock design in docs and SDK surface. | Strong local stateful negative evidence, provenance, and external Fiber-node execution; public BTC SPV and external attestation remain outstanding. | RGB++ is stronger for public BTC binding; NovaSeal is stronger for local certification traceability and negative-path evidence. |
| Robustness | Service/SDK split handles queueing, paymaster, proofs, offline data. | Devnet and Fiber acceptance are deterministic but script-heavy and profile-specific. | NovaSeal should borrow RGB++ service abstraction patterns without giving up deterministic certification. |
| Elegance | Practical but spread across SDK/service/contract/docs. | Declarative profile contracts and one certification gate. | NovaSeal has the cleaner specification surface. |

## Optimisation Proposal

1. Collect external BTC SPV attestations through the adapter layer.
   - The NovaSeal BTC SPV evidence adapter request is now generated and certification-checked.
   - A real external report must still provide `txid`, confirmations, SPV proof digest, CKB SPV client CellDep, and source service provenance.
   - Feed that provider report into `cellc certify --plugin novaseal-profile-v0` as `public_btc_spv_evidence.json` before making production BTC-finality claims.

2. Add a `btc_time_lock` style delayed-unlock profile.
   - RGB++ uses BTC time lock to protect L1 to L2 leap risk.
   - NovaSeal should add a planned profile for delayed release after BTC confirmation threshold.
   - Acceptance should include valid/invalid confirmation-depth evidence.

3. Preserve lifecycle dispatcher requirements in package validation.
   - BTC transaction, BTC UTXO, and Fiber now have explicit lifecycle dispatcher metadata.
   - Keep manifests and validators pinned to the dispatcher action names and live report paths.
   - This prevents future profiles from passing package validation while lacking a CKB-creatable first-state path.

4. Split live-runner helper modules.
   - `novaseal_planned_profiles_devnet_stateful_live.py` is now large because every profile packs its own ABI.
   - Move each profile into `scripts/novaseal_live_profiles/<profile>.py`.
   - Keep a shared transaction/devnet/provenance module and a registry that preserves report contracts.

5. Promote service-builder fixtures into reusable wallet and service libraries.
   - RGB++ has SDK builders for virtual CKB tx, BTC commitment, service queue, paymaster, and SPV proof retrieval.
   - NovaSeal now has certification-checked operator and service-builder JSON fixtures for each planned profile.
   - The next step is to turn those fixtures into reusable builder libraries that output signing preimages, witness bytes, CKB tx skeletons, and expected report hashes from application inputs.

6. Promote Fiber evidence from operator fixture to production integration fixture.
   - The separate Fiber-node execution report now exists and passes the currently tracked required suites.
   - The NovaSeal operator fixture now binds the Fiber candidate witness path to the local evidence.
   - The next step is to bind each Fiber suite to channel topology summaries, before/after channel state, and copied-Bruno compatibility patch provenance for production integration evidence.

## Priority

1. Public BTC SPV provider report collection through the checked adapter.
2. Reusable wallet/service builder libraries from the checked service-builder fixtures.
3. Fiber production integration fixture binding.
4. Profile live-runner modularisation.
5. BTC time-lock profile.
6. Production attestations and public BTC SPV report collection.

Completed hardening since the comparison was first drafted:

- Fiber-node execution evidence is now present and passing for the tracked required suites.
- BTC transaction, BTC UTXO, and Fiber manifests and validators now use explicit lifecycle dispatcher action names rather than `missing-live-dispatcher`.
- Public BTC SPV evidence now has a fail-closed production gate and checked template.
- Planned-profile operator fixtures now bind each profile action to source, schema, invariant, witness, and live-report evidence where local stateful evidence exists.
- Planned-profile service-builder fixtures now bind each profile action to deterministic request/response skeletons, queue keys, receipt binding hashes, and named production external inputs.
- The BTC SPV evidence adapter now binds BTC-facing profiles to service-builder evidence and the public SPV report template, while preserving the external production blocker.
- The external-attestation adapter now binds public/shared CellDep and external BIP340 TCB review requests to the current local TCB review and attestation templates, while preserving both external production blockers.

## Decision

Keep NovaSeal's typed profile and certification architecture. Borrow RGB++'s external proof and workflow integration style. The result should be a smaller trusted contract surface than RGB++ with stronger machine-checkable local and Fiber evidence, while removing the remaining weakness around public BTC proof provenance, service-facing builder ergonomics, and external production attestations.

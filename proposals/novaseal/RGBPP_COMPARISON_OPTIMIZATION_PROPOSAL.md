# NovaSeal V1 vs RGB++ Comparison and Optimisation Proposal

## Evidence Base

- NovaSeal repository: `research/nsv1`, after commit `fc4c5d53`.
- NovaSeal local gate: `target/novaseal-production-gates.json` reports `local_production_prep_ready_external_attestation_required`.
- NovaSeal planned-profile stateful matrix: all planned live scenarios pass with no missing entries.
- RGB++ active SDK clone: `/Users/arthur/RustroverProjects/rgbpp-sdk-active`, commit `ee21eb9735c1adeb277e3a02b7f6c2f6fd1d0556`.
- RGB++ archived SDK reference: `/Users/arthur/RustroverProjects/rgbpp-sdk`, commit `2d547132ede28616647e87d603aea63daada4841`.
- RGB++ design clone: `/Users/arthur/RustroverProjects/RGBPlusPlus-design`, commit `c0b065c8bb8cc0a1813d27e9352ff694e1975ca3`.

## Summary

RGB++ is more mature in Bitcoin/CKB operational integration. It has explicit isomorphic binding, BTC SPV service integration, BTC time lock handling, paymaster handling, service APIs, SDK examples, and workflow-oriented transaction builders.

NovaSeal is cleaner as a typed contract and certification framework. Its strengths are explicit profile packages, canonical envelopes, negative-case live reports, source/artifact provenance, and a single certification gate that now verifies all planned profile live paths. Its main weakness is that BTC/Fiber external evidence is still represented as profile-level live CKB evidence rather than integrated SPV/Fiber-node proof.

## Comparison

| Area | RGB++ | NovaSeal | Assessment |
| --- | --- | --- | --- |
| Core model | Isomorphic BTC UTXO to CKB Cell binding. | Typed NovaSeal profiles with canonical signed envelopes. | RGB++ is operationally concrete; NovaSeal is more formally structured. |
| Workflow maturity | SDK builds virtual CKB tx, BTC tx commitment, queue/service flow, SPV proof retrieval. | Live devnet scripts now exercise all planned profile scenarios. | RGB++ has stronger product workflow coverage. |
| Contract clarity | Lock scripts and BTC time lock focus on RGB++ asset ownership. | Profile-specific CellScript sources encode business intent directly. | NovaSeal is easier to audit per business profile. |
| Security posture | Strong BTC confirmation/SPV/time-lock design in docs and SDK surface. | Strong local stateful negative evidence and provenance; external BTC/Fiber evidence still outstanding. | RGB++ is stronger for public BTC binding; NovaSeal is stronger for local certification traceability. |
| Robustness | Service/SDK split handles queueing, paymaster, proofs, offline data. | Devnet acceptance is deterministic but script-heavy and profile-specific. | NovaSeal should borrow RGB++ service abstraction patterns. |
| Elegance | Practical but spread across SDK/service/contract/docs. | Declarative profile contracts and one certification gate. | NovaSeal has the cleaner specification surface. |

## Optimisation Proposal

1. Add an external-evidence adapter layer.
   - Introduce a NovaSeal `btc_spv_evidence_v0` report schema modelled on RGB++ service proof retrieval.
   - Require `txid`, confirmations, SPV proof digest, CKB SPV client CellDep, and source service provenance.
   - Feed it into `cellc certify --plugin novaseal-profile-v0` so BTC transaction and UTXO profiles stop relying on placeholder-style public verification booleans.

2. Add a `btc_time_lock` style delayed-unlock profile.
   - RGB++ uses BTC time lock to protect L1 to L2 leap risk.
   - NovaSeal should add a planned profile for delayed release after BTC confirmation threshold.
   - Acceptance should include valid/invalid confirmation-depth evidence.

3. Promote lifecycle dispatcher requirements into package validation.
   - BTC transaction, BTC UTXO, and Fiber now have lifecycle entries.
   - Update manifests and validators from `missing-live-dispatcher` to explicit lifecycle action names after the external-evidence schema is added.
   - This prevents future profiles from passing package validation while lacking a CKB-creatable first-state path.

4. Split live-runner helper modules.
   - `novaseal_planned_profiles_devnet_stateful_live.py` is now large because every profile packs its own ABI.
   - Move each profile into `scripts/novaseal_live_profiles/<profile>.py`.
   - Keep a shared transaction/devnet/provenance module and a registry that preserves report contracts.

5. Add wallet and service-facing builders.
   - RGB++ has SDK builders for virtual CKB tx, BTC commitment, service queue, paymaster, and SPV proof retrieval.
   - NovaSeal should add builder-backed JSON fixtures for each planned profile, not only Python live scripts.
   - These builders should output signing preimages, witness bytes, CKB tx skeletons, and expected report hashes.

6. Make Fiber evidence honest and staged.
   - Current Fiber profile proves a live CKB stateful settlement path.
   - Add a separate `fiber_node_execution_v0` evidence report before treating the Fiber profile as fully production-backed.
   - The report should include cloned Fiber commit, scenario name, node topology, channel state before/after, and mapped NovaSeal profile witness.

## Priority

1. External BTC SPV evidence adapter.
2. Manifest/validator dispatcher upgrade.
3. Profile live-runner modularisation.
4. Builder-backed wallet/service fixtures.
5. BTC time-lock profile.
6. Fiber node execution evidence.

## Decision

Keep NovaSeal's typed profile and certification architecture. Borrow RGB++'s external proof and workflow integration style. The result should be a smaller trusted contract surface than RGB++ with stronger machine-checkable local evidence, while removing the remaining weakness around external BTC/Fiber proof provenance.

# NovaSeal v0 MVP Skeleton — Audit Status

**Date of this snapshot**: 2026-06-11
**Package**: `proposals/novaseal/v0-mvp-skeleton`
**Status**: historical local V1 and production-prep evidence exists from the
full devnet runbook, but it is not current for a changed checkout until every
live report is regenerated on the exact commit being certified. Production
readiness is still blocked by external/public facts.

This document is the current evidence ledger for NovaSeal core. It intentionally
separates generated audit evidence, local verifier harness evidence, live local
devnet stateful evidence, and remaining TCB/deployment gaps.

## Current Passes

Package and script checks:

- `cellc check --target-profile ckb` passes.
- `cellc check --target-profile ckb --primitive-strict 0.16` passes.
- `cellc src/nova_state_type.cell --target-profile ckb` passes.
- `cellc src/nova_state_lifecycle_type.cell --target-profile ckb --entry-action novaseal_lifecycle` passes.
- `cellc src/nova_btc_authority_lock.cell --target-profile ckb` passes.
- `cellc src/nova_receipt_type.cell --target-profile ckb` passes.
- `python3 scripts/novaseal_wallet_signing_vectors.py --pretty` passes.
- `python3 scripts/novaseal_bip340_tcb_review.py --pretty` passes local review gates and records that external attestation is still required.
- A current `target/debug/cellc certify --plugin novaseal-profile-v0 --repo-root . --json` run is locally acceptable when its generated reports are fresh for the exact git commit and its stateful acceptance report has `live_devnet_rpc_executed=true`, `local_blockers=0`, and either `status=passed` or `status=local_devnet_passed_external_endpoint_required`. The latter status is only a local acceptance pass; production/external completeness still requires `status=passed`, `acceptance_blockers=0`, and `blockers=0`.

Live local devnet:

- `scripts/novaseal_devnet_stateful_live.py` passes.
- It deploys the BIP340 runtime verifier as a live CellDep.
- It deploys `novaseal_lifecycle` as a live VM2/data2 type-script CellDep.
- It commits bootstrap -> key-auth transition by RPC.
- It verifies the old state is dead and the new state + receipt outputs are live.
- It dry-runs wrong-signature rejection without consuming the live state.

Aggregate stateful gate:

- `scripts/novaseal_devnet_stateful_acceptance.sh --pretty` delegates to `cellc certify --plugin novaseal-profile-v0` and reports both local blockers and live acceptance blockers. It is a local pass when it prints `status=local_devnet_passed_external_endpoint_required`, `live_devnet_rpc_executed=true`, `local_blockers=0`, and `external_endpoint_status=external_required`; it is a full external-completeness pass only when it prints `status=passed`, `acceptance_blockers=0`, and `blockers=0`.
- The same aggregate gate includes Agreement Profile originate -> repay, originate -> claim, and live negative dry-runs.

Historical full runbook refresh, 2026-06-10:

- Core live devnet, Agreement live devnet, and all six planned-profile live
  reports passed with real CKB devnet RPC execution.
- Fiber report status remains passed for all required suites.
- Report generation passed:
  - BIP340 TCB local review:
    `passed_local_review_external_attestation_required`
  - wallet signing vectors: `14/14`
  - wallet/lock digest alignment: `11/11`
  - profile operator fixtures: `10/10`
  - service builder fixtures: `10/10`
  - BTC SPV evidence adapter: `3/3`
  - external attestation adapter: `2/2`
  - external evidence handoff bundle: `4/4`
- Phase 7 certification passed for that commit with local V1 readiness and four
  external production blockers. Any later source, script, or Rust certification
  change requires rerunning the live devnet/Fiber/BTC evidence before making the
  same local-readiness claim for the new commit.

## Current Generated Audit Surface

After:

```bash
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
```

the derived audit surface reports:

```text
actions=1
locks=1
source_units=4
proof_plan_records=55
builder_assumptions=43
runtime_gaps=0
strict_prediction_errors=0
classification=non_production_audit_surface
```

The generated bundle exposes:

- action: `key_auth_transition`
- lock: `btc_authority`
- source units:
  - `src/nova_btc_authority_lock.cell`
  - `src/nova_receipt_type.cell`
  - `src/nova_state_lifecycle_type.cell`
  - `src/nova_state_type.cell`
- generic BTC BIP340 verifier wiring through `verifier::btc::bip340::require_signature(...)`
- manifest-bound spawn target obligations for the runtime verifier
- checked IPC envelope and child exit-status records

The generated bundle no longer leaves primitive-strict `PP0150` gaps for the
NovaSeal core transition. Output materialisation and `NovaSealCellV0` resource
transition coverage are visible to generated ProofPlan strict mode.

## Schema And Vectors

`python3 scripts/novaseal_schema_layout.py --pretty` reports:

```text
NovaSealCellV0: fields=7 size=146 bytes
NovaSealCellCommitmentV0: fields=6 size=114 bytes
NovaSealIntentCoreV0: fields=11 size=222 bytes
NovaSealSignedIntentV0: fields=2 size=254 bytes
ProofReceiptCommitmentV0: fields=13 size=310 bytes
ProofReceiptV0: fields=16 size=382 bytes
```

`python3 scripts/novaseal_canonical_vectors.py --pretty` reports:

```text
vectors=11
signed_intent_vectors=11
resolved_receipt_matches=11
latest_receipt_matches=11
receipt_commitment_status=split_intent_and_explicit_receipt_commitment
```

The current receipt rule is:

```text
intent_core_hash = hash_blake2b_packed(NovaSealIntentCoreV0)
new_cell_commitment = hash_blake2b_packed(NovaSealCellCommitmentV0)
latest_receipt_hash = hash_blake2b_packed(ProofReceiptCommitmentV0)
signed_intent_hash = hash_blake2b_packed(NovaSealSignedIntentV0)
```

The old "ProofReceiptV0 excluding intent_hash" candidate is obsolete.

## Harness Evidence

State type CKB VM harness:

```text
total_cases=11
accepted=3
rejected=8
state_type_matched_expected=11
source_fixture_matched_by_state_type_only=9
source_fixture_requires_lock_or_external_context=2
shared witness payload size=398 bytes
```

The two unmatched source fixtures are expected: `wrong_signature_reject` and
`authority_hash_mapping_mismatch_reject` belong to authority-lock scope, not
type-action scope.

Criterion 6 is intentionally split in this evidence ledger:

- Criterion 6a: `wrong_signature_reject` proves invalid BTC signatures reject.
- Criterion 6b: `wrong_pubkey_valid_signature_reject` proves a valid BIP340
  signature by the wrong x-only pubkey rejects by explicit authority binding.

`authority_hash_mapping_mismatch_reject` separately proves lock args /
authority-id mismatch rejection, and
`authority_rotation_without_explicit_action_reject` proves an ordinary key-auth
transition cannot silently rotate authority.

Combined lock + type transaction harness:

```text
total_cases=11
expected_accept=1
expected_reject=10
matched_expected=11
node_stack_matched_expected=11
shared_witness_size_bytes=398
max_full_transaction_cycles=7521003
max_node_stack_cycles=7521003
max_consensus_tx_size_bytes=1484
max_output_occupied_capacity_shannons=70700000000
```

This is local CKB node-verification-stack evidence over deterministic
transactions. It is not a public/shared devnet deployment pin.

## TCB Position

The BTC verifier remains an external runtime-verifier TCB item. Current evidence
for it includes:

- reference BIP340 vectors,
- fixed IPC envelope vectors,
- no-std/RISC-V verifier core,
- staged RISC-V shell ELF,
- child-verifier CKB VM execution,
- parent-lock CKB VM execution,
- resolved lock-group and full transaction script-verifier evidence,
- combined eleven-fixture local CKB contextual verifier evidence, including
  wrong-pubkey-valid-signature rejection, authority mapping mismatch rejection,
  and implicit authority-rotation rejection,
- live local devnet key-auth transition evidence.
- a local TCB review bundle at
  `target/novaseal-bip340-tcb-review.json`.

This is a strong local evidence stack, but it is not a substitute for an
external reviewer attesting the exact runtime verifier artifact hash.

## Production Gate

The current production gate is the Rust compiler certification entry:

```bash
cargo run --locked -p cellscript --bin cellc -- \
  certify --plugin novaseal-profile-v0 --repo-root . --json
```

Current status:

```text
status=passed
local_v1_ready=true
local_production_prep_ready_external_attestation_required
production_ready=false
```

Passed local gates:

- core manifest pins the local devnet verifier CellDep and artifact hash
- Agreement manifest pins the same local devnet verifier CellDep and artifact hash
- fixed-width Molecule-equivalent wallet signing vectors exist for core and Agreement
- wallet/lock digest alignment passes for all eleven canonical fixtures
- local BIP340 runtime-verifier TCB review bundle passes
- live local devnet stateful core and Agreement reports pass
- all six planned profile live devnet reports pass
- Fiber node execution report passes with all required suites executed
- planned profile operator fixtures and service builder fixtures pass
- BTC SPV, external attestation, and external handoff request adapters pass
- BTC SPV adapter and handoff bundle now require current CKB live report
  bindings, service-builder bindings, CKB-side BTC commitment hashes, raw BTC
  transaction material, block-header/Merkle evidence, confirmation heights, and
  profile-specific transaction bindings before a public BTC evidence report can
  satisfy production gates

External gates still required:

- `proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json`
- `proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json`
- `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json`
- `proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json`

Templates exist next to those expected files. They are templates only and are
not counted as production facts.

## Remaining Production Blockers

- Public/shared devnet, testnet, or mainnet CellDep publication is not yet attested.
- The runtime BIP340 verifier binary still needs an external TCB review attestation.
- Public BTC SPV evidence is still an external production fact, not something
  the local adapter can manufacture. It must satisfy the current handoff bundle
  and the certification checks for raw transaction `txid`/`wtxid`, block-header
  hash, Merkle branch, confirmation count, profile-specific binding, and
  canonical SPV material hash.
- RWA legal/registry review evidence is still an external production fact.
- v0 has only `latest_receipt_hash`; it does not provide a historical receipt accumulator.

Any claim of "production ready", "mainnet ready", or "fully audited" is false
until those blockers are closed.

## Related Docs

- `docs/RECEIPT_COMMITMENT_SPEC.md`
- `docs/CANONICAL_VECTORS.md`
- `docs/SCHEMA_LAYOUT.md`
- `docs/FIXTURE_HARNESS.md`
- `docs/STATE_TYPE_CKB_VM_HARNESS.md`
- `docs/COMBINED_TX_HARNESS.md`
- `docs/BTC_VERIFIER_SPEC.md`
- `docs/VERIFIER_IPC_CONTRACT.md`
- `docs/RISCV_VERIFIER_SHELL.md`
- `docs/RISCV_SHELL_ARTIFACT.md`
- `docs/CKB_VM_CHILD_VERIFIER.md`
- `docs/PARENT_LOCK_CKB_VM_HARNESS.md`
- `docs/SPAWN_BACKEND_BLOCKER.md`
- `docs/DEVNET_STATEFUL_ACCEPTANCE.md`

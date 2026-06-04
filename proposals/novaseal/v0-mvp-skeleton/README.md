# NovaSeal v0 MVP Skeleton (CellScript Package)

**Status**: Strict minimal viable skeleton for feasibility validation and first implementation slice.

**Primary audit documentation**: See [docs/AUDIT_STATUS.md](docs/AUDIT_STATUS.md) for the exact evidence table, validation command results, and current status wording.

**ProofPlan mapping**: See [proofs/proofplan_mapping.json](proofs/proofplan_mapping.json) — the brutally honest, machine-readable comparison of the 9 strict acceptance criteria against the **real generated** `cellc audit-bundle` output.

**Derived audit surface**: Run `python3 scripts/novaseal_audit_surface.py --pretty` after `cellc audit-bundle --target-profile ckb --json` to produce `target/novaseal-audit-surface.json`, a narrow NovaSeal-specific summary of actions, locks, ProofPlan gaps, field-guard visibility, strict-mode predictions, and combined transaction measurement evidence when present.

**Fixture harness**: Run `python3 scripts/novaseal_fixture_harness.py --pretty` after the audit-surface extraction to produce `target/novaseal-fixture-report.json`. This keeps the source-model transition evidence and attaches child-verifier VM, parent-lock ABI preflight, parent-lock CKB VM, state-type CKB VM, and combined eleven-fixture transaction-verifier reports when present.

**Rust harness boundary**: The Rust code under `harness/ckb_vm` is local evidence-generation tooling only. It is not part of the deployed NovaSeal contract surface, is not a runtime verifier CellDep, and is not called by the `.cell` code. Deployed/runtime verifier wiring is represented by `verifier::btc::bip340::require_signature(...)` plus the manifest-bound `cellscript_btc_bip340_verifier_riscv` CellDep.

**State type CKB VM harness**: Build the action artifact with `/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_state_type.cell --target riscv64-elf --target-profile ckb --entry-action key_auth_transition -o target/novaseal-state-type-action.elf`, then run `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_state_type_harness -- --pretty`. This executes `key_auth_transition` in `ckb-vm` for all eleven fixtures at action/type scope. The `.cell` intent ABI now uses `NovaSealSignedIntentV0 { core, expected_receipt_hash }`; the packed signed-intent size is 254 bytes. The action and lock parse the same 398-byte `CSARGv1` witness payload order (`NovaSealSignedIntentV0`, `state_hash_commitment`, `SignaturePayload`), and the combined lock+type harness exercises that shared payload at full transaction verifier level.

**Schema layout**: Run `python3 scripts/novaseal_schema_layout.py --pretty` to produce `target/novaseal-schema-layout.json`, the current packed fixed-layout reference derived from the three `.schema` files. This is not yet full Molecule output.

**Canonical vectors**: Run `python3 scripts/novaseal_canonical_vectors.py --pretty` after schema-layout extraction to produce `target/novaseal-canonical-vectors.json`, deterministic packed-reference test bytes for the eleven fixtures.

**BTC verifier vectors**: Run `python3 scripts/novaseal_btc_verifier_vectors.py --pretty` after canonical-vector generation to produce `target/novaseal-btc-verifier-vectors.json`, reference BIP340/secp256k1 verifier vectors for the external TCB.

**BTC verifier IPC vectors**: Run `python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty` after BTC verifier vector generation to produce `target/novaseal-btc-verifier-ipc-vectors.json`, the fixed lock-to-verifier request envelope vectors.

**Child verifier CKB VM harness**: Run `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty` after staging the RISC-V shell artifact to produce `target/novaseal-ckb-vm-child-verifier-report.json`. This executes the staged child verifier ELF in `ckb-vm`, but still does not execute the parent lock or a full transaction.

**Parent lock ABI preflight**: Run `python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty` to build the `btc_authority` parent lock as ASM/ELF and check that Script.args binding, protected input binding, and VM2 spawn/pipe/wait surfaces are ready for a parent/child CKB VM harness. This is artifact inspection only, not parent-lock VM execution.

**Parent lock CKB VM harness**: Run `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty` after parent preflight and shell staging to produce `target/novaseal-parent-lock-ckb-vm-report.json`. This executes the parent lock ELF in `ckb-vm`, harnesses VM2 `spawn`, runs the staged child verifier ELF in nested `ckb-vm`, and records valid-signature accept, wrong-signature reject, wrong-pubkey-valid-signature reject, and authority-hash mismatch reject evidence. It also builds a `ckb-types` consensus-packed transaction shape and runs the official `ckb-script` resolved lock-group verifier plus full transaction script verifier with `cell_deps[0]`, parent lock dep, lock ScriptGroup shape, tx size, occupied capacity, under-capacity shape checks, and verifier cycles. It is still a harnessed parent-authority path; the separate combined harness covers all eleven transition fixtures, and the live devnet runner covers the core stateful lifecycle.

**Combined lock + type transaction harness**: Run `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_combined_tx_harness -- --pretty` after building the parent lock, state action, and child verifier artifacts to produce `target/novaseal-combined-tx-report.json`. This runs all eleven fixtures through official `ckb-script` full transaction verification and the CKB `ckb-verification` non-contextual + contextual transaction verifier stack with both the parent lock and state type/action script present, a shared 398-byte `CSARGv1` witness payload, materialised `ProofReceiptV0` data at `Output#1`, and `cell_deps[0]` bound to the staged verifier shell. It also records builder-candidate fee, occupied-capacity, under-capacity, tx-size, and code-dep-role shape checks derived from the constructed transaction plus resolved deps. Negative fixtures must match both accept/reject outcome and expected lock/type script scope in both verifier layers. This is local node-verification-stack evidence over deterministic builder outputs; the live devnet runner is the separate stateful RPC evidence layer.

**Devnet stateful gate**: Run `/home/arthur/a19q3/CellScript/scripts/novaseal_devnet_stateful_acceptance.sh --pretty` to produce `/home/arthur/a19q3/CellScript/target/novaseal-devnet-stateful-acceptance.json`. The core lifecycle blockers are resolved by `src/nova_state_lifecycle_type.cell:novaseal_lifecycle`, and `python3 /home/arthur/a19q3/CellScript/scripts/novaseal_devnet_stateful_live.py --pretty --ckb-repo /home/arthur/a19q3/ckb --ckb-bin /home/arthur/a19q3/ckb/target/debug/ckb` now provides live core bootstrap -> key-auth transition evidence. `python3 /home/arthur/a19q3/CellScript/scripts/novaseal_agreement_devnet_stateful_live.py --pretty --ckb-repo /home/arthur/a19q3/ckb --ckb-bin /home/arthur/a19q3/ckb/target/debug/ckb` provides Agreement originate -> repay, originate -> claim, and live negative-case evidence. The aggregate gate now reports `passed`. See [docs/DEVNET_STATEFUL_ACCEPTANCE.md](docs/DEVNET_STATEFUL_ACCEPTANCE.md).

**Purpose**: Set up the strict feasibility target for the core NovaSeal v0 thesis ("BTC key or multisig authorises a typed CKB Cell transition under explicit policy, with nonce/expiry replay protection and an auditable ProofReceipt") as a first-class CellScript package using current 0.16 capabilities where available.

The package manifest declares `canonical_schema = "NovaSealCanonicalV0"` and
pins the schema hash for `schemas/nova_seal_canonical_envelope_v0.schema`. A
downstream package must declare `conforms_to = "NovaSealCanonicalV0"`, pin the
same `canonical_schema_hash`, and pass the deterministic compiler certification
gate `cellc certify --plugin novaseal-profile-v0` before it may be treated as a
NovaSeal profile rather than a NovaSeal-inspired package.

This skeleton deliberately **excludes**:
- Agreement Profile semantics (roadmap v0.2; implemented as a separate package)
- Full Fiber channel open/close/force-close integration (roadmap v0.3)
- OP_RETURN / BTC transaction commitment profile (roadmap v0.4)
- SPV / Bitcoin UTXO closure proofs (v1)
- Dual-sealed / production profiles (v1)
- Any claim that ProofReceipt is "automatic runtime logging"

---

## Strict v0 Acceptance Criteria (This Skeleton Only)

The implemented MVP package + generated artifacts + fixtures MUST satisfy exactly these 9 properties. Nothing more. This skeleton records the target and now has strict 0.16 closure, combined lock+type transaction harness evidence, live local devnet evidence, fixed-width wallet signing vectors, local BIP340 TCB review, and a local production gate. Local production-prep evidence passes. Production claims remain blocked on public/shared CellDep attestation, public BTC SPV evidence for BTC-facing profiles, and external BIP340 TCB review.

1. `old NovaSealCell` is consumed exactly once in a valid transition.
2. `new NovaSealCell` is created exactly once in the same transition.
3. `state_hash` changes **only** when accompanied by a correctly formatted signed intent.
4. `nonce` strictly increments on every successful transition.
5. An intent whose `expiry` has passed is rejected.
6. A transition with an invalid / wrong BTC signature is rejected.
7. A transition where `policy_hash` does not match the governing package is rejected.
8. A transition where the signed `expected_receipt_hash` does not match the materialized receipt commitment is rejected.
9. `cellc audit-bundle` (and the resulting ProofPlan + builder assumptions) can be generated and shows the above obligations as on-chain checked or builder-required.

These 9 criteria are the **only** definition of "v0 success" for this skeleton.

---

## Package Layout (This Skeleton)

```
novaseal-v0-mvp-skeleton/
├── Cell.toml
├── README.md                 # this file
├── docs/
│   ├── AUDIT_STATUS.md        # exact current validation evidence and blockers
│   ├── BTC_VERIFIER_SPEC.md   # BIP340 verifier profile + test-vector contract
│   ├── VERIFIER_IPC_CONTRACT.md
│   ├── RISCV_VERIFIER_SHELL.md
│   ├── RISCV_SHELL_ARTIFACT.md
│   ├── CKB_VM_CHILD_VERIFIER.md
│   ├── PARENT_LOCK_ABI_PREFLIGHT.md
│   ├── PARENT_LOCK_CKB_VM_HARNESS.md
│   ├── FIXTURE_HARNESS.md     # model-level fixture runner and attached evidence
│   ├── COMBINED_TX_HARNESS.md # eleven-fixture lock+type ckb-script verifier
│   ├── RUST_INTERNALIZATION_PLAN.md # move generic BTC verifier code into CellScript-owned surfaces
│   ├── FIELD_GUARD_GAPS.md    # source guards vs generated ProofPlan visibility
│   ├── RESOURCE_CONSERVATION_BLOCKER.md
│   ├── RECEIPT_COMMITMENT_SPEC.md
│   ├── SCHEMA_LAYOUT.md       # packed fixed-layout reference from schemas/
│   └── CANONICAL_VECTORS.md   # deterministic packed-reference fixture vectors
├── scripts/
│   ├── novaseal_audit_surface.py
│   ├── novaseal_btc_verifier_ipc_vectors.py
│   ├── novaseal_btc_verifier_shell_report.py
│   ├── novaseal_btc_verifier_vectors.py
│   ├── novaseal_canonical_vectors.py
│   ├── novaseal_fixture_harness.py
│   ├── novaseal_parent_lock_abi_preflight.py
│   ├── novaseal_riscv_shell_artifact.py
│   ├── novaseal_schema_layout.py
│   └── novaseal_spawn_backend_probe.py
├── harness/
│   └── ckb_vm/                    # evidence-only CKB VM runners, not deployable contract code
├── verifier/
│   ├── novaseal_btc_verifier/      # host reference BIP340 verifier
│   ├── novaseal_btc_verifier_core/ # no_std IPC parser + BIP340 verifier core
│   └── novaseal_btc_verifier_riscv/ # no_std RISC-V BIP340 shell
├── src/
│   ├── nova_btc_authority_lock.cell
│   ├── nova_state_lifecycle_type.cell # stable bootstrap/transition type-script entry
│   ├── nova_state_type.cell
│   └── nova_receipt_type.cell   # stub / optional for v0
├── schemas/
│   ├── nova_seal_cell_v0.schema
│   ├── nova_intent_v0.schema
│   └── proof_receipt_v0.schema
├── fixtures/
│   ├── keyauth_transfer_valid.json
│   ├── wrong_signature_reject.json
│   ├── replay_nonce_reject.json
│   ├── expired_intent_reject.json
│   ├── old_outpoint_index_mismatch_reject.json
│   ├── old_outpoint_tx_hash_mismatch_reject.json
│   ├── policy_hash_mismatch_reject.json
│   ├── receipt_hash_mismatch_reject.json
│   ├── wrong_pubkey_valid_signature_reject.json
│   ├── authority_hash_mapping_mismatch_reject.json
│   └── authority_rotation_without_explicit_action_reject.json
└── proofs/
    ├── bip340_external_tcb_review_attestation.template.json
    ├── proofplan.json
    ├── proofplan_mapping.json   # generated-audit vs target-criteria mapping
    ├── public_btc_spv_evidence.template.json
    ├── public_shared_cell_dep_attestation.template.json
    └── invariant_matrix.json
```

Repository-root NovaSeal evidence scripts such as `scripts/novaseal_wallet_signing_vectors.py` and `scripts/novaseal_bip340_tcb_review.py` are outside this package's `scripts/` directory. The production-prep/profile certification gate is now owned by the Rust compiler entry `cellc certify --plugin novaseal-profile-v0`.

---

## Critical Design Decisions (Tightened per Audit)

### 1. ProofReceipt Treatment
`ProofReceipt` is an **explicit audit artefact and checked obligation**, not implicit logging.

It becomes runtime-enforced **only** when:
- Represented as a checked output cell (`nova_receipt_type`), **or**
- Its hash/fields are explicitly asserted inside `nova_state_type` transition logic, **or**
- The builder / test harness treats the receipt emission as a mandatory acceptance obligation (visible in ProofPlan + audit-bundle).

In this skeleton, `nova_receipt_type.cell` is intentionally a stub. The primary enforcement path for v0 is (b) + (c): the state transition script checks the split-intent receipt commitment (`intent.expected_receipt_hash == hash_blake2b_packed(ProofReceiptCommitmentV0)`), and the acceptance fixtures, harnesses, and live devnet runner make the obligation visible.

### 2. BTC Signature Verifier is Part of the TCB
The `nova_btc_authority_lock` delegates to a dedicated verifier binary via `spawn`.

**This verifier binary is NOT a minor plugin.** It is a first-class security root for v0.

For any real implementation the following verifier inputs MUST stay frozen:

- Signature scheme: BIP340 Schnorr over secp256k1.
- Encoding: compact 64-byte `r || s` signature.
- Low-S rule: not applicable to BIP340; reject `s >= n`.
- Message: 32-byte `signed_intent_hash_after_resolved_receipt`.
- Pubkey format: x-only 32-byte BIP340 pubkey.
- Malleability rejection rules: reject `r >= p`, reject `s >= n`, lift x-only pubkey to even-y point, and require reconstructed `R` to have even y.
- Test vectors: 32 positives, 40 negatives, plus malformed IPC vectors.

The BIP340 verifier profile, IPC envelope, RISC-V shell, and local TCB review now exist. Local evidence is frozen for this slice; public BTC SPV evidence and external TCB attestation are still required before production claims.

The `.cell` lock and state action now call `verifier::btc::bip340::require_signature(...)`; the compiler expands that generic verifier capability into the fixed 18-word spawn/IPC envelope and delegates to the `cellscript_btc_bip340_verifier_riscv` spawn verifier. The host verifier, no-std core, RISC-V shell, child-verifier `ckb-vm` harness, parent-lock `ckb-vm` harness, official resolved lock-group verifier, official full transaction script verifier, and local CKB contextual transaction verifier all check the same frozen BIP340 vector contract for the parent authority cases. The lock and state action also parse one shared witness ABI, and the combined harness now runs all eleven fixtures through both verifier layers with lock and type/action groups present, `Output#1` receipt materialisation, authority pubkey binding, authority mapping mismatch rejection, implicit authority-rotation rejection, and fee/capacity/tx-size checks. Core live devnet RPC evidence, fixed-width wallet signing vectors, local devnet verifier CellDep pinning, and a local BIP340 TCB review bundle now exist; the remaining production work is public/shared CellDep attestation, public BTC SPV evidence for BTC-facing profiles, and external BIP340 TCB review. The actual crypto still lives in the delegated verifier binary, so that binary remains a first-class TCB item.

### 3. Schema / Molecule Alignment is a Hard Gate
The three `.schema` files in this skeleton are the **source of truth** for on-chain layout.

Before any adapter (CCC, Fiber funding, explorer, etc.) is written:
- These schemas must be frozen.
- The exact Molecule encoding (or hand-written molecule schema) derived from them must be published.
- Every field offset, length, and endianness must be documented.

Changing a field later will break all downstream adapters. This skeleton deliberately keeps the structs minimal.

### 4. Fiber Profile = Shape Only (v0)
The `xUDT-compatible profile` mentioned in the architecture docs is represented only as:
- A documented data layout in `nova_seal_cell_v0.schema` (amount field at a known offset for xUDT tools).
- One fixture (`fiber_admission_shape` can be added later) proving that a NovaSealCell with the fungible profile can be constructed in a way that existing xUDT-aware tooling will not immediately reject on layout grounds.

**No claim is made that "Fiber supports NovaSeal" in v0.** Only that the shape is intentionally compatible for later admission testing.

### 5. Multi-Script Audit Strategy (Current Reality — Read Carefully)

The package declares a single entry point:

```toml
entry = "src/nova_state_type.cell"
```

**Consequence (visible in the generated audit-bundle)**:
- `key_auth_transition` appears in the primary action audit surface.
- `btc_authority` now appears in the primary lock audit surface because the default entry module carries the same verifier-wiring lock shape.
- `source_units` correctly lists all three `.cell` files (package-level provenance and hash tracking).
- `locks[0] = btc_authority` with lock-args ProofPlan records for `ckb-lock-args` and `lock-args:ScriptArgs#0`.

**How the three scripts are audited today (conservative baseline)**:
1. **Package default** (`cellc check` / `audit-bundle`): the declared entry now exposes both the state transition action and the authority lock surface, including the generic BTC BIP340 verifier obligations and spawn/IPC shell wiring. This is what most builders and tooling will consume first.
2. **Individual file checks** (mandatory for this skeleton):
   ```bash
   cellc src/nova_btc_authority_lock.cell --target-profile ckb
   cellc src/nova_receipt_type.cell --target-profile ckb
   ```
   These keep the standalone lock and receipt files compiler-visible while package-level multi-entry support remains immature.
3. **Source-unit visibility** in the audit-bundle already gives us hashes of all three files.

**Why this matters for NovaSeal’s TCB**:
The security-critical v0 logic (BTC key authorisation via `verifier::btc::bip340::require_signature(...)` + delegate to the external `cellscript_btc_bip340_verifier_riscv` verifier target) has crossed the parent/child CKB VM boundary, the official resolved lock-group verifier boundary, the official full transaction script verifier boundary, and the local CKB non-contextual + contextual transaction verifier stack for the combined eleven-fixture cases. The bundle sees the `btc_authority` lock, Script.args binding, explicit `sig.pubkey` to cell-declared authority binding, generic BTC BIP340 verifier obligations, pipe/write/spawn/wait/close records, checked IPC envelope, checked exit status, and the manifest-bound spawn target. The no-std/RISC-V verifier shell executes the BIP340 decision against the frozen IPC vectors locally, through a child-verifier `ckb-vm` inherited-fd harness, and through a parent-lock `ckb-vm` harness that observes the compiler-lowered spawn/envelope/status path. The parent-lock harness also records transaction-shape facts and `ckb-script` verifier cycles for `cell_deps[0]`, parent lock dep, ScriptGroup shape, tx size, occupied capacity, under-capacity rejection, resolved lock-group verification, and full transaction script verification. The combined harness now runs all eleven fixtures through both verifier layers with lock and type/action groups present and `ProofReceiptV0` materialised as `Output#1`. Criterion 6 ("wrong BTC signature rejects") and the stricter authority-binding obligation now have combined local node-verification-stack evidence; `wrong_signature_reject` proves invalid signatures fail, `wrong_pubkey_valid_signature_reject` proves a valid signature from the wrong x-only pubkey still rejects, `authority_hash_mapping_mismatch_reject` proves lock args / authority id mismatch rejects, and `authority_rotation_without_explicit_action_reject` proves ordinary transitions cannot silently rotate authority. Public/shared CellDep attestation, public BTC SPV evidence, and external BIP340 TCB review remain later production evidence layers.

The state transition side now has a separate CKB VM action harness: all eleven fixtures execute against `key_auth_transition` at type/action scope and match the expected type-layer result. Nine of eleven also match the full fixture outcome directly; `wrong_signature_reject` and `authority_hash_mapping_mismatch_reject` are intentionally accepted at this layer because those failures belong to `btc_authority`. The intent layout blocker found by the harness has been resolved at the `.cell`/schema boundary: canonical vectors and the compiled action ABI now both use `NovaSealSignedIntentV0 { core, expected_receipt_hash }` with the 254-byte packed shape.

This is accurately reflected in `proofs/proofplan_mapping.json`. It is not a defect to hide — it is the current honest boundary of the tooling.

See `docs/AUDIT_STATUS.md` section 3 for the full current reality + future directions.

---

## How to Work With This Skeleton (Current CellScript 0.16 Tooling)

```bash
# 1. Type check / metadata
cellc check --target-profile ckb

# 2. Full audit surface (the real deliverable for v0)
cellc audit-bundle --target-profile ckb --json

# 3. Extract NovaSeal-specific audit surface
python3 scripts/novaseal_audit_surface.py --pretty

# 4. Extract the current packed schema layout reference
python3 scripts/novaseal_schema_layout.py --pretty

# 5. Generate deterministic packed-reference fixture vectors
python3 scripts/novaseal_canonical_vectors.py --pretty

# 6. Generate reference BIP340 verifier vectors
python3 scripts/novaseal_btc_verifier_vectors.py --pretty

# 7. Run the host reference verifier against all BIP340 vectors
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-vectors --vectors target/novaseal-btc-verifier-vectors.json

# 8. Generate and check the fixed verifier IPC envelope vectors
python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty
cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-ipc-vectors --vectors target/novaseal-btc-verifier-ipc-vectors.json

# 9. Build the no-std RISC-V BIP340 verifier shell
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --release --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty

# 10. Stage and verify the exact RISC-V verifier shell artifact
python3 scripts/novaseal_riscv_shell_artifact.py --sync --pretty

# 11. Execute the staged child verifier ELF in ckb-vm with inherited-fd input
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty

# 12. Probe the current CellScript VM2 spawn backend boundary
python3 scripts/novaseal_spawn_backend_probe.py --cellc /home/arthur/a19q3/CellScript/target/debug/cellc --pretty

# 13. Build and inspect the parent lock ASM/ELF ABI surface
python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty

# 14. Execute the parent lock ELF and staged child verifier ELF together in ckb-vm,
#     plus official ckb-script resolved lock-group verification
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty

# 15. Build and run the state transition action in ckb-vm over all eleven fixtures
/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_state_type.cell --target riscv64-elf --target-profile ckb --entry-action key_auth_transition -o target/novaseal-state-type-action.elf
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_state_type_harness -- --pretty

# 16. Run the combined eleven-fixture lock+type transaction verifier harness
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_combined_tx_harness -- --pretty

# 17. Run the deterministic fixture harness (source-model plus attached evidence)
python3 scripts/novaseal_fixture_harness.py --pretty

# 18. Generate fixed-width wallet signing vectors from the repository root
python3 /home/arthur/a19q3/CellScript/scripts/novaseal_wallet_signing_vectors.py --pretty

# 19. Generate the local BIP340 TCB review bundle from the repository root
python3 /home/arthur/a19q3/CellScript/scripts/novaseal_bip340_tcb_review.py --pretty

# 20. Run the local production-prep/profile certification gate from the repository root
/home/arthur/a19q3/CellScript/target/debug/cellc certify --plugin novaseal-profile-v0 --json

# 21. Check strict 0.16 primitive closure
cellc check --target-profile ckb --primitive-strict 0.16

# 22. Inspect obligations
cellc constraints --target-profile ckb
cellc explain-assumptions --target-profile ckb
cellc profile --target-profile ckb
```

The value of this skeleton is **not** a complete production contract on day 1. The value is a package whose `audit-bundle` + hand-authored ProofPlan target + eleven fixtures + live local devnet evidence can be reviewed by humans and machines against the 9 acceptance criteria above.

---

## Next Concrete Steps (After Skeleton is Accepted)

**Read `docs/AUDIT_STATUS.md` and `proofs/proofplan_mapping.json` first.** They are the current honest record.
Also read `docs/BTC_VERIFIER_SPEC.md`, `docs/VERIFIER_IPC_CONTRACT.md`, `docs/RISCV_VERIFIER_SHELL.md`, `docs/RISCV_SHELL_ARTIFACT.md`, `docs/CKB_VM_CHILD_VERIFIER.md`, `docs/PARENT_LOCK_ABI_PREFLIGHT.md`, `docs/PARENT_LOCK_CKB_VM_HARNESS.md`, `docs/COMBINED_TX_HARNESS.md`, `docs/SPAWN_BACKEND_BLOCKER.md`, `docs/FIXTURE_HARNESS.md`, `docs/FIELD_GUARD_GAPS.md`, `docs/RESOURCE_CONSERVATION_BLOCKER.md`, `docs/RECEIPT_COMMITMENT_SPEC.md`, `docs/SCHEMA_LAYOUT.md`, and `docs/CANONICAL_VECTORS.md` before changing `.cell` logic, schema layout, receipt hashing, verifier wiring, or compiler behaviour.

Conservative, non-scope-creeping next slice priorities (in rough order):

1. Preserve the generic verifier capability path: `verifier::btc::bip340::require_signature(...)` must continue lowering to `pipe`, `spawn_with_fd`, 18 `pipe_write` words, `fixed_u64_le(...)`, `close(write_fd)`, and `wait`. The low-level VM2 helper ecall blocker is closed, static spawn targets now have a strict `CellDep#0`/`code` manifest-bound builder model, and `spawn_with_fd(target, fd)` supplies a one-entry inherited-fd list to the child.
2. Keep the lock path bound to the pinned `cellscript_btc_bip340_verifier_riscv` shell through the first `deploy.ckb.cell_deps` manifest entry, while keeping builder-required evidence honest.
3. Preserve the fixed-width wallet signing vectors for the shared witness and `NovaSealSignedIntentV0`.
4. Preserve the current combined local CKB transaction verifier layer and live local devnet stateful runner while adding public/shared CellDep attestation, public BTC SPV evidence, and external BIP340 TCB review.
5. Keep the materialised `ProofReceiptV0` output shape stable while dynamic Molecule table/vector profiles remain future extensions.
6. Keep `NovaSealIntentCoreV0.old_cell` schema/.cell alignment locked as `OutPoint`.
7. Only after the above items are closed: consider real Fiber admission tests and broader receipt history semantics.

Do **not** add OP_RETURN, Fiber channel semantics, or new chain-facing protocol features in the next slice. Public BTC SPV should enter first as externally supplied evidence and a certification gate, not as an unreviewed runtime expansion.

---

## Versioning & Governance Note

This skeleton is **not** a declaration that "NovaSeal exists". It is an engineering artifact that lets the community evaluate whether the CellScript + explicit schema-backed CKB Cell state + ProofPlan model is a good host for Bitcoin-authorised CKB objects.

The 9 criteria are met at the local evidence level. Production claims still require the external/public facts identified above: public/shared CellDep attestation, public BTC SPV evidence for BTC-facing profiles, and external BIP340 TCB review.

**Author of this skeleton**: Grok (acting on tightened feedback from human review, 2026-05-30)

**Intended consumers**: CellScript core team, potential NovaSeal implementers, CKB BTCFi explorers.

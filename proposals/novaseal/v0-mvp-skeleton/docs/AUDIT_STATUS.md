# NovaSeal v0 MVP Skeleton — Audit Status (Current Evidence)

**Date of this snapshot**: 2026-05-30 (generated during conservative slice)
**Package**: proposals/novaseal/v0-mvp-skeleton (CellScript 0.16.0)
**Philosophy**: Honest, mechanically useful audit surface. No scope expansion. No production claims.

This document is the single source of truth for what the compiler + tooling currently prove vs. what remains manual/fixture-only/TCB.

---

## 1. Exact Validation Evidence (as of this slice)

All commands run from inside the package directory against the current source tree on disk (no hidden edits).

### Package Default Entry
- Command: `cellc check --target-profile ckb`
- Result: **passes**
- Notes: Uses `entry = "src/nova_state_type.cell"` from Cell.toml. The default entry now exposes the state transition action plus a verifier-wiring `btc_authority` lock surface with spawn/IPC shell wiring.

### Individual Script Compile Status (important for multi-script TCB)
- `cellc src/nova_state_type.cell --target-profile ckb` → **passes**
- `cellc src/nova_btc_authority_lock.cell --target-profile ckb` → **passes**
- `cellc src/nova_receipt_type.cell --target-profile ckb` → **passes**
- Notes: All three source units are syntactically and semantically valid in isolation. The default entry module now also carries the same verifier-wiring `btc_authority` lock shape, so the package audit surface exposes one lock record while the standalone lock file remains independently compileable.

### Audit-Bundle Surface
- Command: `cellc audit-bundle --target-profile ckb --json`
- Result: **passes** (generates target/cellscript-audit-bundle/audit-bundle.json + index.html)
- Generated content (key facts):
  - 1 action exposed: `key_auth_transition`
  - 1 lock exposed: `btc_authority`
  - 28 proof_plan records (soundness: "passed", non-strict, 0 issues)
  - `actions[0].proof_plan_records = 15`
  - `locks[0].proof_plan_records = 11`
  - lock ProofPlan features now include `ckb-lock-args`, `ckb-spawn-ipc`, `pipe`, `pipe-write`, `spawn`, manifest-bound `spawn-target`, `wait`, `close-fd`, `lock-args:ScriptArgs#0`, and two checked `guard-equality:*` records for lock argument binding
  - lock runtime access: `expected_btc_authority_hash` from `ScriptArgs#0`, plus pipe/write/spawn/wait/close process records
  - `source_units`: all 3 .cell files are present (package-level visibility)
  - Explicit "covered" for consume/create of NovaSealCellV0
  - `resource-conservation:NovaSealCellV0` is now `checked-runtime`, `covered`, and `on_chain_checked = true`
  - The resource-conservation detail now classifies the guarded transition field-by-field:
    - `unchecked`: none
    - `preserved`: `version`, `btc_authority_hash`, `policy_hash`, `receipt_root`
    - `guarded`: `state_hash`, `nonce`, `expiry`
    - `allowed fresh`: none
  - The same classification is now machine-readable in the generated ProofPlan record's `input_output_relation_checks`:
    - `resource-field:version=preserved`
    - `resource-field:btc_authority_hash=preserved`
    - `resource-field:policy_hash=preserved`
    - `resource-field:receipt_root=preserved`
    - `resource-field:state_hash=guarded`
    - `resource-field:nonce=guarded`
    - `resource-field:expiry=guarded`
    - `resource-conservation:NovaSealCellV0=checked-runtime`
  - The action now activates the generic hash-commitment guard recogniser for `state_hash` through `hash_blake2b(intent.new_state_hash) == state_hash_commitment`.
  - The action and lock now activate the generic guarded-equality recogniser for conservative fail-closed equality checks:
    - `guard-equality:intent.old_state_hash==old_cell.state_hash`
    - `guard-equality:hash_blake2b(intent.new_state_hash)==state_hash_commitment`
    - `guard-equality:intent.policy_hash==old_cell.policy_hash`
    - `guard-equality:intent.nonce==old_cell.nonce+1`
    - `guard-equality:intent.receipt_hash==receipt_hash`
    - `guard-equality:cell.btc_authority_hash==expected_btc_authority_hash`
    - `guard-equality:cell.policy_hash==intent.policy_hash`
  - Builder assumptions for capacity, tx size, input/output cardinality, timepoint load, lock args, spawn/IPC, spawn target binding, and blake2b profile use
- What the generated ProofPlan **does not yet see**:
  - CKB VM execution of the delegated RISC-V BIP340 verifier result
  - Any receipt output cell creation (receipt_type is pure stub + obligation is only a require hash check inside the action)

### Derived NovaSeal Audit Surface
- Command: `python3 scripts/novaseal_audit_surface.py --pretty`
- Result: **passes** (generates target/novaseal-audit-surface.json)
- Generated content (key facts):
  - `actions = 1`
  - `locks = 1`
  - `proof_plan_records = 28`
  - `runtime_gaps = 0`
  - `strict_prediction_errors = 0`
  - Field guards for `state_hash`, `nonce`, `expiry`, `policy_hash`, and `receipt_hash` are all `source_guard_present: true`
  - `state_hash`, `nonce`, `expiry`, `policy_hash`, and `receipt_hash` are all `generated_named_obligation: true`
  - `receipt_hash` is now generated-visible through `guard-equality:intent.receipt_hash==receipt_hash`
  - Current production blockers in the derived surface:
    - `btc_authority has generated spawn/IPC shell wiring but no CKB VM BIP340 verifier execution result`
    - `ProofReceiptV0 output cell materialisation is not generated`
    - `cycles, tx size, and occupied capacity are not measured`
- Interpretation: the extractor confirms that the authority lock is in the generated audit surface, the transition relation is generated-visible, simple fail-closed equality guards have named generated ProofPlan records, and the lock's spawn/IPC shell wiring is now visible. CKB VM execution of the delegated BIP340 signature decision and receipt output materialisation remain outside generated ProofPlan coverage.

### Model-Level Fixture Harness
- Command: `python3 scripts/novaseal_fixture_harness.py --pretty`
- Result: **passes** (generates target/novaseal-fixture-report.json)
- Generated content (key facts):
  - `fixtures = 6`
  - `matched = 6`
  - `mismatched = 0`
  - `ckb_vm_executed = false`
  - `classification = model_level_fixture_evidence`
- Interpretation: the six fixture JSON files are now deterministic and internally consistent with the current source guard semantics. This is **not** CKB VM / transaction acceptance evidence.
- See `docs/FIXTURE_HARNESS.md` for exact limitations.

### Schema Layout Extractor
- Command: `python3 scripts/novaseal_schema_layout.py --pretty`
- Result: **passes** (generates target/novaseal-schema-layout.json)
- Generated content (key facts):
  - `NovaSealCellV0`: 7 fields, 146 bytes
  - `NovaSealIntentV0`: 9 fields, 213 bytes
  - `ProofReceiptV0`: 12 fields, 279 bytes
  - Encoding profile: `packed-fixed-v0-reference`
  - Molecule status: `not_generated`
  - Fiber fungible profile: `not_defined_in_v0_layout`
- Interpretation: schemas now have a machine-readable fixed-layout reference with offsets, widths, and source hashes. This still does **not** close Molecule alignment, canonical byte-vector, or wallet signing requirements.
- See `docs/SCHEMA_LAYOUT.md` for limitations.

### Canonical Packed-Reference Vectors
- Command: `python3 scripts/novaseal_canonical_vectors.py --pretty`
- Result: **passes** (generates target/novaseal-canonical-vectors.json)
- Generated content (key facts):
  - `vectors = 6`
  - `intent_vectors = 6`
  - `receipt_candidate_vectors = 6`
  - `accepted_new_cell_vectors = 1`
  - `computed_receipt_candidate_hash_matches_intent = 0`
  - `resolved_receipt_hash_matches_intent = 6`
  - `resolved_receipt_verification_preimage_matches = 6`
  - `receipt_commitment_status = resolved_candidate_without_intent_hash`
- Interpretation: fixtures now have deterministic packed-reference bytes and hashes. The legacy full-receipt candidate remains circular (`0` matches), while the selected v0 candidate rule excluding `ProofReceiptV0.intent_hash` is internally consistent across all six fixtures.
- See `docs/CANONICAL_VECTORS.md` and `docs/RECEIPT_COMMITMENT_SPEC.md` for limitations and the exact preimage rule.

### BTC Verifier Reference Vectors
- Command: `python3 scripts/novaseal_btc_verifier_vectors.py --pretty`
- Result: **passes** (generates target/novaseal-btc-verifier-vectors.json)
- Generated content (key facts):
  - Scheme: `bip340_schnorr_secp256k1`
  - Pubkey format: x-only 32-byte
  - Signature format: 64-byte `r || s`
  - Message: `signed_intent_hash_after_resolved_receipt`
  - `positive_vectors = 24`
  - `negative_vectors = 30`
  - `positive_self_verified = 24`
  - `negative_self_rejected = 30`
- Interpretation: the external verifier TCB now has a concrete reference vector contract. On-chain production coverage still requires the later builder/full-node runner, even though parent/child CKB VM, resolved lock-group evidence, and full transaction script-verifier evidence now exist.
- See `docs/BTC_VERIFIER_SPEC.md` for the verifier profile and I/O contract.

### BTC Verifier Host Reference
- Command: `cargo test --manifest-path verifier/novaseal_btc_verifier/Cargo.toml`
- Result: **passes**
- Command: `cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-vectors --vectors target/novaseal-btc-verifier-vectors.json`
- Result: **passes**
- Generated content (key facts):
  - `checked = 54`
  - `matched = 54`
  - `status = ok`
- Interpretation: there is now a host Rust verifier that accepts/rejects every generated BIP340 vector correctly. The same decision path is now shared with the no-std/RISC-V shell, but this is still not CKB VM transaction execution evidence.

### BTC Verifier IPC Envelope Vectors
- Command: `python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty`
- Result: **passes** (generates target/novaseal-btc-verifier-ipc-vectors.json)
- Command: `cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-ipc-vectors --vectors target/novaseal-btc-verifier-ipc-vectors.json`
- Result: **passes**
- Generated content (key facts):
  - Fixed request envelope length: 144 bytes
  - Magic: ASCII `NSBV0IPC`
  - `ipc_vectors = 54`
  - `malformed_vectors = 5`
  - `total_vectors = 59`
  - `expected_accept = 24`
  - `expected_reject = 35`
  - Host verifier result: `checked = 59`, `matched = 59`, `status = ok`
- Interpretation: the lock-to-verifier request blob is now fixed and host-validated against both valid cryptographic vectors and malformed envelope vectors. This is still not CKB spawn evidence, because no CKB VM transaction dry-run has executed the parent lock and child verifier together.
- See `docs/VERIFIER_IPC_CONTRACT.md` for exact offsets and return-code contract.

### BTC Verifier No-Std IPC Core
- Command: `cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml`
- Result: **passes**
- Command: `cargo test --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml`
- Result: **passes**
- Command: `cargo clippy --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --all-targets -- -D warnings`
- Result: **passes**
- Command: `cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf`
- Result: **passes**
- Generated content (key facts):
  - `#![no_std]` crate
  - BIP340 verification path shared by host verifier and RISC-V shell
  - No heap allocation in IPC parsing
  - 4 unit tests for BIP340 and IPC envelope behaviour
  - Host verifier reuses this crate for IPC parsing and crypto verification
- Interpretation: the fixed IPC parser and BIP340 verifier core are now no-std and RISC-V-checkable. This still does **not** mean the parent lock plus child verifier have been executed in CKB VM.

### BTC Verifier RISC-V Shell
- Command: `cargo check --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml`
- Result: **passes**
- Command: `cargo test --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib`
- Result: **passes**
- Command: `cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib -- -D warnings`
- Result: **passes**
- Command: `cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv -- -D warnings`
- Result: **passes**
- Command: `cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv`
- Result: **passes**
- Command: `cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --release --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv`
- Result: **passes**
- Command: `python3 scripts/novaseal_btc_verifier_shell_report.py --pretty`
- Result: **passes** (generates target/novaseal-btc-verifier-shell-report.json)
- Generated content (key facts):
  - RISC-V ELF shell exists
  - Debug ELF size from this slice: 3360424 bytes
  - Release ELF size from this slice: 187768 bytes
  - Shell library unit tests: 5
  - RISC-V `_start` reads inherited fd index `0` as 18 little-endian `u64` words
  - `total_vectors = 59`
  - `parse_ok = 54`
  - `parse_rejected = 5`
  - `spawn_word_representable = 58`
  - `spawn_word_roundtrip = 58`
  - `spawn_io_rejects = 1`
  - `accepted = 24`
  - `rejected = 35`
  - `matched_expected = 59`
  - `all_expected_matched = true`
- Interpretation: there is now a real RISC-V shell artifact boundary with a fixed inherited-fd input adapter and local BIP340 decision evidence. Malformed envelopes reject before crypto, wrong signatures reject with the crypto path, and valid signatures accept. The child verifier is now also exercised by a dedicated CKB VM harness below; this still does **not** mean the parent lock and child verifier have been executed together in a full transaction.
- See `docs/RISCV_VERIFIER_SHELL.md` for exact shell behaviour and limits.

### BTC Verifier RISC-V Shell Artifact Preflight
- Command: `python3 scripts/novaseal_riscv_shell_artifact.py --sync --pretty`
- Result: **passes** (generates `target/novaseal-riscv-shell-artifact.json`, stages `target/novaseal-btc-verifier-riscv-shell-release.elf`, and writes its `.sha256` sidecar)
- Generated content (key facts):
  - `preflight_passed = true`
  - `staged_matches_release = true`
  - Staged release ELF size: 187768 bytes
  - Staged release ELF SHA-256: `d0d1c14c811728c680d8646283cf7961dd850eebea856ac0e281fb493c4bc58d`
  - Shell vector decisions: `accepted = 24`, `rejected = 35`, `matched_expected = 59`
  - Spawn input contract remains fd index `0`, 18 little-endian `u64` words, 144-byte IPC envelope, implemented over the official VM2 buffer/length syscalls
  - Generated audit surface has spawn/pipe/wait records and a manifest-bound spawn target, but this artifact report is not a parent-lock CKB VM execution transcript
  - `lock_wiring_status = wired_to_bip340_shell`
  - `ready_for_ckb_vm_dry_run = true`
  - `production_ready = false`
- Interpretation: the exact release ELF used by the child-verifier and parent-lock CKB VM harnesses is now pinned and mechanically checked against stale artifact drift. This still does **not** prove builder-backed transaction acceptance.
- See `docs/RISCV_SHELL_ARTIFACT.md` for the exact preflight boundary.

### BTC Verifier Child CKB VM Harness
- Command: `cargo clippy --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml -- -D warnings`
- Result: **passes**
- Command: `cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty`
- Result: **passes** (generates `target/novaseal-ckb-vm-child-verifier-report.json`)
- Generated content (key facts):
  - `child_verifier_ckb_vm_executed = true`
  - `parent_lock_spawn_executed = false`
  - Staged release ELF size: 187768 bytes
  - Staged release ELF SHA-256: `d0d1c14c811728c680d8646283cf7961dd850eebea856ac0e281fb493c4bc58d`
  - `total_cases = 59`
  - `expected_accept = 24`
  - `expected_reject = 35`
  - `accepted = 24`
  - `rejected = 35`
  - `matched_expected = 59`
  - `mismatched = 0`
  - `malformed_word_streams = 1`
  - `inherited_fd_calls = 59`
  - `pipe_read_calls = 1062`
  - `close_calls = 59`
  - `min_cycles = 10863`
  - `max_cycles = 3487024`
  - `total_cycles = 148069425`
- Interpretation: the staged child verifier ELF now runs in `ckb-vm` with harness-provided official VM2 `inherited_fd`, `pipe_read`, and `close` syscalls over the frozen IPC vector set. This is real child-verifier VM evidence, but still not a full parent-lock transaction because it does not execute `btc_authority`, VM2 `spawn`, VM2 `wait`, Script.args, witnesses, cell_deps, capacity, or tx-size checks.
- See `docs/CKB_VM_CHILD_VERIFIER.md` for the exact boundary.

### Parent Lock ABI Preflight
- Command: `python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty`
- Result: **passes** (generates `target/novaseal-parent-lock-abi-preflight.json`)
- Generated content (key facts):
  - `preflight_passed = true`
  - `parent_lock_asm_built = true`
  - `parent_lock_elf_built = true`
  - `parent_lock_ckb_vm_executed = false`
  - `parent_spawn_executed = false`
  - `ready_for_parent_child_ckb_vm_harness = true`
  - Script.args loader is visible for `lock_args`.
  - `expected_btc_authority_hash` consumes exactly 32 Script.args bytes.
  - Script.args u32 decoding is pointer-safe.
  - `expected_btc_authority_hash` is **not** rebound from `Input#N` or `CellDep#N` data.
  - protected `cell` remains bound from `Input#0` cell data.
  - VM2 `spawn_with_fd`, `spawn`, `wait`, `pipe_write`, and `close` surfaces are visible.
- Interpretation: this closes a parent-lock ABI readiness issue before VM execution. It is still artifact inspection, not a parent-lock CKB VM transaction transcript.
- See `docs/PARENT_LOCK_ABI_PREFLIGHT.md` for the exact boundary.

### Parent Lock CKB VM Harness
- Command: `cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty`
- Result: **passes** (generates `target/novaseal-parent-lock-ckb-vm-report.json`)
- Generated content (key facts):
  - `parent_lock_ckb_vm_executed = true`
  - `parent_spawn_executed = true`
  - `child_verifier_ckb_vm_executed = true`
  - `transaction_shape_constructed = true`
  - `consensus_packed_tx_constructed = true`
  - `resolved_transaction_constructed = true`
  - `resolved_script_verifier_executed = true`
  - `resolved_script_verifier_matched_expected = true`
  - `full_transaction_constructed = true`
  - `full_transaction_executed = true`
  - `full_transaction_verifier_matched_expected = true`
  - `total_cases = 3`
  - `expected_accept = 1`
  - `expected_reject = 2`
  - `accepted = 1`
  - `rejected = 2`
  - `matched_expected = 3`
  - `mismatched = 0`
  - `parent_max_cycles = 24949`
  - `child_max_cycles = 3487024`
  - `resolved_script_verifier_max_cycles = 3678905`
  - `full_transaction_verifier_max_cycles = 3678905`
  - `max_consensus_tx_size_bytes = 850`
  - `max_output_occupied_capacity_shannons = 21900000000`
  - `min_capacity_margin_shannons = 10000000000`
  - `capacity_shape_checks_passed = true`
  - `under_capacity_shape_rejects = true`
  - `cell_dep0_spawn_target_modelled = true`
  - `parent_lock_dep_modelled = true`
  - `pipe_write_calls = 36`
  - `spawn_calls = 2`
  - `wait_calls = 2`
- Interpretation: the parent `btc_authority` ELF now runs in `ckb-vm`, constructs the fixed IPC envelope using spawn-before-write VM2 pipe ordering, executes VM2 `spawn`, runs the staged child verifier ELF in nested `ckb-vm`, waits, and observes child exit status. The same harness constructs a `ckb-types` consensus-packed `ResolvedTransaction` with `cell_deps[0]` as the spawn-target child verifier, a parent-lock code dep, one lock ScriptGroup input, tx-size measurement, occupied-capacity measurement, under-capacity shape rejection, official `ckb-script` lock-group verifier execution, and official `ckb-script` full transaction script verification. The lock now parses the same 389-byte witness payload as the state action (`intent`, `receipt_hash`, `state_hash_commitment`, `SignaturePayload`). This is still not full production acceptance evidence; six-fixture lock+type evidence exists in the separate combined harness.
- See `docs/PARENT_LOCK_CKB_VM_HARNESS.md` for the exact boundary.

### State Type CKB VM Harness
- Build command: `/Users/arthur/RustroverProjects/CellScript/target/debug/cellc src/nova_state_type.cell --target riscv64-elf --target-profile ckb --entry-action key_auth_transition -o target/novaseal-state-type-action.elf`
- Run command: `cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_state_type_harness -- --pretty`
- Result: **passes at action/type scope** (generates `target/novaseal-state-type-ckb-vm-report.json`)
- Generated content (key facts):
  - `state_type_action_ckb_vm_executed = true`
  - `total_cases = 6`
  - `accepted = 2`
  - `rejected = 4`
  - `state_type_matched_expected = 6`
  - `state_type_mismatched = 0`
  - `source_fixture_matched_by_state_type_only = 5`
  - `source_fixture_requires_lock_or_external_context = 1`
  - `max_cycles = 16621`
  - `load_witness_calls = 6`
  - `load_cell_data_calls = 12`
  - `load_header_by_field_calls = 6`
  - `wrong_signature_is_lock_scope = true`
  - `schema_cell_intent_mismatch_detected = false`
  - `schema_cell_intent_aligned = true`
- Interpretation: the compiled `key_auth_transition` action ELF now executes in `ckb-vm` for all six fixtures at action/type scope. The one fixture not matched by state type alone is `wrong_signature_reject`, which is correct because signature rejection belongs to the authority lock. The `.cell` intent ABI now uses the same 213-byte `old_cell: OutPoint` shape as the canonical schema vectors; no intent-shortening adapter remains. The action also parses the shared 389-byte lock+type witness payload and ignores signature bytes at type scope.
- See `docs/STATE_TYPE_CKB_VM_HARNESS.md` for the exact boundary.

### CellScript VM2 Spawn Backend Probe
- Command: `python3 scripts/novaseal_spawn_backend_probe.py --cellc /Users/arthur/RustroverProjects/CellScript/target/debug/cellc --pretty`
- Result: **passes** as a blocker probe (generates `target/novaseal-spawn-backend-probe.json`)
- Generated content (key facts):
  - `compile_passed = true`
  - `all_spawn_ipc_calls_lowered = true`

### Combined Lock + Type Transaction Harness
- Build prerequisites:
  - `/Users/arthur/RustroverProjects/CellScript/target/debug/cellc build --entry-lock btc_authority --target-profile ckb --target riscv64-elf`
  - `/Users/arthur/RustroverProjects/CellScript/target/debug/cellc src/nova_state_type.cell --target riscv64-elf --target-profile ckb --entry-action key_auth_transition -o target/novaseal-state-type-action.elf`
  - staged `target/novaseal-btc-verifier-riscv-shell-release.elf`
- Run command: `cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_combined_tx_harness -- --pretty`
- Result: **passes** (generates `target/novaseal-combined-tx-report.json`)
- Generated content (key facts):
  - `combined_full_transaction_executed = true`
  - `total_cases = 6`
  - `expected_accept = 1`
  - `expected_reject = 5`
  - `accepted = 1`
  - `rejected = 5`
  - `matched_expected = 6`
  - `mismatched = 0`
  - `failure_scope_matched = 6`
  - `failure_scope_mismatched = 0`
  - `lock_and_type_script_groups_present = true`
  - `child_spawn_target_cell_dep0_modelled = true`
  - `shared_witness_abi_aligned = true`
  - `shared_witness_size_bytes = 389`
  - `builder_shape_checks_passed = true`
  - `fee_shape_checks_passed = true`
  - `under_capacity_shape_rejects = true`
  - `min_fee_shannons = 100000`
  - `max_fee_shannons = 100000`
  - `max_full_transaction_cycles = 3703418`
  - `max_consensus_tx_size_bytes = 972`
  - `max_output_occupied_capacity_shannons = 25200000000`
  - `min_capacity_margin_shannons = 10000000000`
- Interpretation: all six fixture JSONs now run through official `ckb-script` full transaction verification with both the parent `btc_authority` lock and the `key_auth_transition` type/action script present. The transaction includes `cell_deps[0]` for the staged child verifier shell, matching lock/type ScriptGroups, fixture timepoint header deps, and the shared 389-byte `CSARGv1` witness payload. The same report now records builder-candidate fee, occupied-capacity, under-capacity, and code-dep-role shape checks derived from the constructed transaction plus resolved deps. Negative fixtures must match both accept/reject outcome and expected lock/type script scope. This closes the former combined six-fixture transaction-evidence gap at harness level. It is still not production builder/full-node acceptance because the transactions are in-memory harness `ResolvedTransaction` values with deterministic harness cells.
- See `docs/COMBINED_TX_HARNESS.md` for the exact boundary.
  - `backend_ecall_boundary_closed = true`
  - `spawn_with_fd_helper_executable = true`
  - `spawn_with_fd_helper_fail_closed_stub = false`
  - `spawn_with_fd_helper_uses_static_cell_dep0_with_one_inherited_fd = true`
  - `fixed_word_envelope_lowered = true`
  - `strict_rejects_spawn_target = true`
  - `manifest_bound_spawn_target_strict_passes = true`
  - `manifest_bound_spawn_target_builder_required = true`
  - Source-only strict failure captures `PP0150 action:probe:spawn-target:CellDep#0@0x...`
- Interpretation: CellScript source can lower a protocol-agnostic spawn/pipe/fixed-word envelope shape, and the VM2 helpers now emit executable `ecall` wrappers. Strict mode rejects unmanifested `spawn-target:CellDep#0@0x...` records, while a matching first `Cell.toml [[deploy.ckb.cell_deps]]` entry with `dep_type = "code"` promotes the target to `builder-required`. The generated builder assumption now requires both transaction `cell_deps[0]` and `builder_assumption_evidence` to identify `CellDep#0`, the manifest name, `dep_type = "code"`, and any manifest-pinned out-point/hash fields instead of accepting an arbitrary non-empty payload. Later CellDep positions and dep groups remain strict-failing because the current wrapper spawns `CellDep#0` with no argv and exactly one inherited fd. The NovaSeal lock now uses this generic 18-word envelope shape and remains audit-visible.
- See `docs/SPAWN_BACKEND_BLOCKER.md` for the exact blocker.

### Strict 0.16 Status (primitive-strict)
- Command: `cellc check --target-profile ckb --primitive-strict 0.16`
- Result: **passes**
- Exact status:
  - No `PP0150` remains for `resource-conservation:NovaSealCellV0`.
  - No `PP0103` remains for `ckb-runtime` context records. Strict PP0103 now applies only to true `checked-runtime` records that fail to set `on_chain_checked`; `ckb-runtime` and `checked-static` records remain non-on-chain-checked by design.
  - All other non-strict validations passed.
  - The compiler now identifies that the action consumes 1 and creates 1 of the linear resource, emits machine-readable field classifications, and marks the resource transition checked because no output field remains unchecked.
- Do not treat strict ProofPlan soundness as production readiness; it is necessary evidence hygiene, not BTC verifier, transaction-builder, cycle, or capacity evidence.

### Non-Strict Commands Summary (this slice)
- All listed non-strict commands (`check`, individual file checks, `audit-bundle`, `explain-assumptions`) are expected to pass after this slice's documentation work.
- `explain-assumptions` surfaces the builder obligations (capacity, cardinality, runtime context) — these are honest and valuable.

---

## 2. The 9 Strict Acceptance Criteria — Current Status vs. Generated Audit

See `proofs/proofplan_mapping.json` for the machine-readable, brutally honest mapping against the **actual generated** `audit-bundle.json`.

High-level summary (do not rely on this table alone — the JSON is authoritative):

- Criteria 1 & 2 (old consumed, new created): **covered_by_generated_audit** (explicit resource consume/create + covered records in ProofPlan)
- Criterion 3 (state_hash only via signed intent): **partially_covered** (the state transition relation is generated-visible as `resource-field:state_hash=guarded`, and the old/new state hash equality/commitment guards now have named `guard-equality:*` records; the actual BTC signature decision remains outside generated audit coverage)
- Criterion 4 (nonce increments): **covered_by_generated_guard_and_transition_audit** (`guard-equality:intent.nonce==old_cell.nonce+1` plus `resource-field:nonce=guarded`)
- Criterion 5 (expiry): **covered_by_generated_transition_audit** for the resource relation (`resource-field:expiry=guarded`) plus visible timepoint load; still not a standalone named expiry ProofPlan record
- Criterion 6 (wrong BTC signature rejects): **combined_six_fixture_full_transaction_script_verifier_evidence** + **parent_child_ckb_vm_harness_evidence** + **transaction_shape_measurement** (the generated bundle exposes `lock:btc_authority` with lock-args and spawn/IPC ProofPlan records; the lock spawns the verifier reader before writing the fixed 18-word IPC envelope and delegates through `spawn_with_fd` to the RISC-V BIP340 shell; the host verifier accepts/rejects all 54 BIP340 vectors and all 59 IPC envelope vectors; the shared no-std core checks for RISC-V and performs the BIP340 decision; the RISC-V shell has inherited-fd input plumbing over the official VM2 buffer/length ABI, accepts 24 valid shell vectors, rejects 35 invalid/malformed vectors, has a pinned staged release ELF, and executes in `ckb-vm` through the child-verifier harness; the compiler spawn backend probe shows executable VM2 wrappers, strict-rejects unmanifested `spawn-target:CellDep#0@0x...`, accepts only a first-CellDep `code` manifest-bound spawn target as `builder-required`, and lowers a protocol-agnostic 18-word fixed-byte envelope; parent-lock ABI preflight confirms the parent lock ASM/ELF builds, Script.args lock_args binding is pointer-safe, `expected_btc_authority_hash` is not rebound from input data, and VM2 spawn/pipe/wait surfaces are present; the parent-lock CKB VM harness executes parent spawn/wait plus nested child verification and records valid-signature accept, signature-bitflip reject, and authority-hash mismatch reject; the combined harness runs all six fixtures through official full transaction script verification with lock and type/action groups present, accepts the valid fixture, rejects the five negative fixtures with expected lock/type script scope, records `max_consensus_tx_size_bytes=972`, records `max_full_transaction_cycles=3703418`, and records fee/capacity/under-capacity builder-shape checks derived from constructed transaction/resolved deps; production builder/full-node acceptance remains outside generated audit coverage)
- Criterion 7 (policy_hash mismatch rejects): **covered_by_generated_guard_and_transition_audit** (`guard-equality:intent.policy_hash==old_cell.policy_hash`, lock-side `guard-equality:cell.policy_hash==intent.policy_hash`, and `resource-field:policy_hash=preserved`)
- Criterion 8 (receipt_hash mismatch rejects): **covered_by_generated_guard_audit** for the equality guard (`guard-equality:intent.receipt_hash==receipt_hash`); receipt output materialisation remains out of scope and not covered
- Criterion 9 (audit-bundle shows the obligations): **partially_covered** (good surface for the state action, guarded resource transition, fail-closed equality guards, lock authority surface, and spawn/IPC shell wiring; state-type action VM evidence, child verifier, parent-lock VM evidence, resolved lock-group verifier evidence, full transaction script-verifier evidence, transaction-shape measurements, shared lock+type witness ABI evidence, and combined six-fixture transaction verifier evidence now exist outside the generated audit bundle; missing production builder/full-node acceptance and receipt output materialisation)

**Key honesty point**: the guarded resource transition, simple fail-closed equality guards, and lock spawn/IPC shell wiring are now generated-audit-visible. State transition fixtures have action/type CKB VM evidence, and BTC signature rejection (criterion 6) has host-reference evidence, no-std/RISC-V shell evidence, child-verifier CKB VM evidence, parent-lock ABI preflight evidence, parent-lock CKB VM harness evidence, official resolved lock-group verifier evidence, official full transaction script-verifier evidence, transaction-shape tx-size/capacity evidence, shared lock+type witness ABI evidence, combined six-fixture transaction verifier evidence, and visible parent lock wiring. This still lacks production builder/full-node acceptance. Receipt output materialisation is **not** generated-audit-covered today. The rest is carried as source, individual compile evidence where applicable, and fixtures + hand-authored proofplan.json. This is the correct conservative posture.

For the field-level guard gap, see `docs/FIELD_GUARD_GAPS.md`.
For the resource-conservation status, see `docs/RESOURCE_CONSERVATION_BLOCKER.md`.
For the fixture harness evidence level, see `docs/FIXTURE_HARNESS.md`.
For the packed schema layout reference, see `docs/SCHEMA_LAYOUT.md`.
For canonical packed-reference vectors and the receipt commitment rule, see `docs/CANONICAL_VECTORS.md` and `docs/RECEIPT_COMMITMENT_SPEC.md`.
For BTC verifier vectors, see `docs/BTC_VERIFIER_SPEC.md`.
For the fixed verifier IPC envelope, see `docs/VERIFIER_IPC_CONTRACT.md`.
For the RISC-V BIP340 shell, see `docs/RISCV_VERIFIER_SHELL.md`.
For the staged RISC-V shell artifact preflight, see `docs/RISCV_SHELL_ARTIFACT.md`.
For child-verifier CKB VM evidence, see `docs/CKB_VM_CHILD_VERIFIER.md`.
For state-type action CKB VM evidence, see `docs/STATE_TYPE_CKB_VM_HARNESS.md`.
For combined six-fixture lock+type transaction verifier evidence, see `docs/COMBINED_TX_HARNESS.md`.
For the current VM2 spawn backend blocker, see `docs/SPAWN_BACKEND_BLOCKER.md`.

---

## 3. Multi-Script Audit Strategy (Current Reality)

### Why the generated bundle now shows one action and one lock
- `Cell.toml` declares `entry = "src/nova_state_type.cell"`
- The package `source_roots` + include brings in all three .cell files (visible in `source_units` of audit-bundle).
- Only the entry file's actions/locks/invariants become the primary ProofPlan + entry ABI surface.
- To make the authority boundary visible in the package audit, `src/nova_state_type.cell` now carries the same verifier-wiring `lock btc_authority(...)` shape as the standalone lock file.
- The generated bundle now includes `locks[0] = btc_authority`, with `ckb-lock-args`, spawn/IPC, manifest-bound `spawn-target`, wait/close, `lock-args:ScriptArgs#0`, and local guard ProofPlan records.
- This still is not a full BTC verifier proof: the generated records prove the lock args data source and shell wiring, not a CKB VM transaction dry-run of the parent/child verifier pair.

### How the three scripts are audited today (conservative baseline)
1. **Package default** (`cellc check` / `audit-bundle`): the declared entry now exposes both the state transition action and the authority lock surface, including spawn/IPC shell wiring. This is what builders and most tooling will see first.
2. **Individual file checks** (`cellc <file>.cell --target-profile ckb`): still mandatory, because the standalone lock and receipt files remain separate review artefacts.
3. **Source-unit visibility**: The audit-bundle already lists all three files with hashes. This gives package-level provenance even if executable surface is partial.

### Why this matters for NovaSeal’s TCB
- The security-critical piece for v0 (BTC key authorisation) lives in `nova_btc_authority_lock.cell` + its external `novaseal_btc_verifier_riscv` delegate.
- The generated bundle now covers the first authority wiring boundary: the lock exists, reads `expected_btc_authority_hash` from Script.args, spawns the RISC-V BIP340 shell through a manifest-bound first CellDep, then writes the fixed verifier envelope.
- The strongest automated evidence for the actual signature decision remains host-reference verifier vectors + IPC vectors + no-std/RISC-V verifier checks + child-verifier CKB VM execution + parent-lock CKB VM execution + official resolved lock-group verifier execution + official full transaction script-verifier execution + staged shell artifact preflight + parent-lock ABI preflight + spawn backend probe + generated shell wiring.
- This is **not** a defect to paper over. It is an accurate reflection of the current CellScript package model and a real engineering item for future slices (see "Next Recommended Implementation Slice").

Future directions (documented here for clarity, not implemented):
- Package manifest extensions for declaring multiple lock/type entries.
- Explicit `with_lock(...)` attachment on the resource that references the lock defined in another source unit.
- Separate lock package + dependency (cleaner TCB boundary).
- Production builder/full-node acceptance records now that the parent-lock harness has a full `ckb-script` transaction-verifier layer.

Until production builder/full-node acceptance exists, individual compile + manual review of the lock file + verifier TCB spec remains the conservative requirement.

---

## 4. Known Non-Production Blockers (Do Not Claim Readiness)

- BTC signature verifier is still an **external binary TCB**, but no longer merely parse-only or merely host-checked. Reference BIP340 vectors, fixed IPC envelope vectors, a no-std BIP340 verifier core, a RISC-V shell with inherited-fd input plumbing, a pinned staged shell ELF, child-verifier CKB VM execution, parent-lock CKB VM execution, official resolved lock-group verifier execution, official full transaction script-verifier execution, combined six-fixture full transaction script-verifier execution, parent-lock transaction-shape tx-size/capacity evidence, a parent-lock ABI preflight, a compiler spawn backend probe, generated lock spawn/IPC wiring, and a host Rust verifier exist. What still does **not** exist is production builder/full-node acceptance.
- Current CellScript VM2 spawn/IPC helper lowering has executable wrappers, a strict first-CellDep `code` manifest-bound spawn-target model with structured transaction/evidence checks for `CellDep#0` identity, a one-fd `spawn_with_fd` path, and a protocol-agnostic `fixed_u64_le` word extractor. Lock wiring now targets the RISC-V BIP340 shell and is generated-audit-visible, but production remains blocked on builder/full-node acceptance.
- The lock and state action now parse one shared 389-byte `CSARGv1` witness payload order. This removes the previous witness-format split, but it is preparatory evidence only until a combined lock+type transaction harness exists.
- `resource-conservation:NovaSealCellV0` is now `checked-runtime`, and strict 0.16 ProofPlan soundness passes.
- Lock script is now part of the generated ProofPlan surface for Script.args binding and spawn/IPC shell wiring, but not for cryptographic verification.
- No receipt output cell is created (only hash obligation checked inside action).
- Receipt hash materialisation has a packed-reference candidate rule, but it is not implemented in wallet/verifier/.cell logic yet.
- Resolved lock-group cycles, full transaction script-verifier cycles, shape-level tx-size, occupied-capacity, under-capacity measurements, and explicit fee-shape evidence exist for the parent/combined harnesses; no production builder/full-node acceptance exists yet.
- Schemas now have a machine-readable packed fixed-layout reference and deterministic packed-reference vectors, and the `.cell` inline `NovaSealIntentV0.old_cell` now matches `schemas/nova_intent_v0.schema` as `OutPoint`; no Molecule reference implementation, wallet signing vectors, or alignment tests exist yet.
- Fixtures now have action/type CKB VM evidence for `key_auth_transition` and combined lock+type full transaction script-verifier evidence against the compiled artifacts, but no production builder/full-node acceptance exists yet.

Any claim of "production", "ready for mainnet", or "full v0 implemented" is false as of this document.

---

## 5. Commands Used to Produce This Snapshot

(Recorded for reproducibility — run from package root)

```bash
cellc check --target-profile ckb
cellc src/nova_state_type.cell --target-profile ckb
cellc src/nova_btc_authority_lock.cell --target-profile ckb
cellc src/nova_receipt_type.cell --target-profile ckb
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty
cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml
cargo test --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml
cargo clippy --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --all-targets -- -D warnings
cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf
cargo check --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml
cargo test --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib
cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib -- -D warnings
cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv -- -D warnings
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --release --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty
python3 scripts/novaseal_riscv_shell_artifact.py --sync --pretty
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
python3 scripts/novaseal_spawn_backend_probe.py --cellc /Users/arthur/RustroverProjects/CellScript/target/debug/cellc --pretty
python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
cargo test --manifest-path verifier/novaseal_btc_verifier/Cargo.toml
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-vectors --vectors target/novaseal-btc-verifier-vectors.json
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-ipc-vectors --vectors target/novaseal-btc-verifier-ipc-vectors.json
python3 scripts/novaseal_fixture_harness.py --pretty
cellc explain-assumptions --target-profile ckb
cellc check --target-profile ckb --primitive-strict 0.16
```

See the terminal log of the slice that produced this file for exact output and any warnings.

---

**End of AUDIT_STATUS.md** — treat as immutable record for this snapshot. Update only when new evidence is generated by running the validation commands against unchanged source.

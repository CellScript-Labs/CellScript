use crate::error::{CompileError, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub(crate) const IMPLEMENTATION_ID: &str = "cellscript::cli::novaseal_certification";

const AGREEMENT_ROOT: &str = "proposals/novaseal/agreement-profile-v0";
const CORE_ROOT: &str = "proposals/novaseal/v0-mvp-skeleton";
const VERIFIER_ROOT: &str = "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier";
const CORE_MANIFEST: &str = "proposals/novaseal/v0-mvp-skeleton/Cell.toml";
const AGREEMENT_MANIFEST: &str = "proposals/novaseal/agreement-profile-v0/Cell.toml";
const CANONICAL_SCHEMA: &str = "proposals/novaseal/v0-mvp-skeleton/schemas/nova_seal_canonical_envelope_v0.schema";
const CORE_LIVE: &str = "target/novaseal-devnet-stateful-live.json";
const AGREEMENT_LIVE: &str = "target/novaseal-agreement-devnet-stateful-live.json";
const STATEFUL_ACCEPTANCE: &str = "target/novaseal-devnet-stateful-acceptance.json";
const WALLET_VECTORS: &str = "target/novaseal-wallet-signing-vectors.json";
const TCB_REVIEW: &str = "target/novaseal-bip340-tcb-review.json";
const PUBLIC_CELLDEP_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json";
const EXTERNAL_TCB_ATTESTATION: &str = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json";

const EXPECTED_NOVASEAL_CANONICAL_SCHEMA: &str = "NovaSealCanonicalV0";
const EXPECTED_NOVASEAL_CANONICAL_ENVELOPE: &str = "NovaSealCanonicalEnvelopeV0";
const EXPECTED_AGREEMENT_PROFILE: &str = "agreement-profile-v0";
const EXPECTED_AGREEMENT_CONFORMANCE_GATE: &str = "cellc certify --plugin novaseal-profile-v0";
const EXPECTED_PROFILE_CERTIFICATION_GATE: &str = "cellc certify --plugin novaseal-profile-v0";
const EXPECTED_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const EXPECTED_CERTIFICATION_REPORT: &str = "target/cellscript-certification/novaseal-profile-v0.json";

const EXPECTED_VERIFIER: &[(&str, &str)] = &[
    ("name", "cellscript_btc_bip340_verifier_riscv"),
    ("role", "runtime_verifier"),
    ("verifier_id", "btc.bip340.v0"),
    ("ipc_abi", "cellscript-btc-bip340-ipc-v0"),
    ("dep_type", "code"),
    ("hash_type", "data1"),
];

const EXPECTED_CANONICAL_SCHEMA_FIELDS: &[(&str, &str)] = &[
    ("profile_id", "Byte32"),
    ("policy_hash", "Byte32"),
    ("action", "u8"),
    ("terminal_path", "u8"),
    ("subject_id", "Byte32"),
    ("old_state_commitment", "Byte32"),
    ("new_state_commitment", "Byte32"),
    ("old_nonce", "u64"),
    ("new_nonce", "u64"),
    ("expiry", "u64"),
    ("authority_hash", "Byte32"),
    ("profile_body_hash", "Byte32"),
    ("payout_commitment_hash", "Byte32"),
];

const EXPECTED_AGREEMENT_SCHEMA_FILES: &[&str] = &[
    "native_ckb_payout_v0.schema",
    "nova_agreement_cell_v0.schema",
    "nova_agreement_intent_v0.schema",
    "nova_agreement_receipt_v0.schema",
    "nova_agreement_terms_v0.schema",
    "nova_terminal_path_v0.schema",
];

const EXPECTED_AGREEMENT_FIXTURES: &[&str] = &[
    "originate_valid.json",
    "repay_before_expiry_valid.json",
    "claim_after_expiry_valid.json",
    "wrong_originator_reject.json",
    "wrong_borrower_signature_reject.json",
    "wrong_lender_signature_reject.json",
    "wrong_party_reject.json",
    "non_ckb_asset_kind_reject.json",
    "under_capacity_reject.json",
    "payout_capacity_short_reject.json",
    "payout_lock_args_mismatch_reject.json",
    "wrong_settlement_amount_reject.json",
    "early_claim_reject.json",
    "expired_repay_reject.json",
    "nonce_mismatch_reject.json",
    "latest_receipt_hash_mismatch_reject.json",
    "receipt_hash_mismatch_reject.json",
    "preserved_field_mutation_reject.json",
    "wrong_terms_hash_reject.json",
];

const EXPECTED_CERTIFICATION_INVARIANTS: &[&str] = &[
    "profile_separation",
    "ckb_native_only",
    "pre_expiry_repay",
    "post_expiry_claim",
    "party_terminal_rights",
    "receipt_materialized",
    "terms_hash_output_binding",
    "receipt_hash_output_binding",
    "native_capacity_settlement",
    "resolved_transaction_stack",
    "ckb_vm_capacity_settlement",
    "payout_cell_binding",
    "canonical_envelope_binding",
    "wallet_signing_vectors",
    "live_devnet_lifecycle",
];

const EXPECTED_LIVE_NEGATIVE_KEYS: &[&str] = &[
    "wrong_lender_signature_rejected",
    "non_ckb_asset_kind_rejected",
    "wrong_borrower_signature_rejected",
    "repay_payout_capacity_short_rejected",
    "repay_payout_lock_args_mismatch_rejected",
    "repay_wrong_payout_amount_rejected",
    "early_claim_rejected",
    "wrong_lender_claim_signature_rejected",
    "post_negative_active_still_live",
    "post_claim_negative_active_still_live",
];

const REQUIRED_AGREEMENT_CORE_PATTERNS: &[(&str, &str)] = &[
    ("canonical_envelope", "struct NovaSealCanonicalEnvelopeV0"),
    ("canonical_envelope_hash", "canonical_envelope_hash"),
    ("canonical_profile_body_hash", "profile_body_hash"),
    ("canonical_runtime_check", "intent.canonical_envelope_hash == canonical_envelope_hash"),
    ("signed_typed_intent", "struct NovaAgreementSignedIntentV0"),
    ("expected_receipt_hash", "expected_receipt_hash"),
    ("receipt_commitment", "NovaAgreementReceiptCommitmentV0"),
    ("materialized_receipt", "NovaAgreementReceiptV0"),
    ("latest_receipt_hash", "latest_receipt_hash"),
    ("authority_signature", "verifier::btc::bip340::require_signature"),
    ("nonce_rule", "new_nonce == active.nonce + 1"),
    ("expiry_rule", "expiry_timepoint"),
    ("payout_commitment", "payout_commitment_hash"),
];

#[derive(Clone, Copy)]
struct ExpectedWalletAction {
    signers: &'static [&'static str],
    old_status: i64,
    new_status: i64,
    old_nonce: i64,
    new_nonce: i64,
}

const EXPECTED_AGREEMENT_WALLET_ACTIONS: &[(&str, ExpectedWalletAction)] = &[
    (
        "originate_agreement",
        ExpectedWalletAction { signers: &["borrower", "lender"], old_status: 0, new_status: 1, old_nonce: 0, new_nonce: 0 },
    ),
    ("repay_before_expiry", ExpectedWalletAction { signers: &["borrower"], old_status: 1, new_status: 2, old_nonce: 0, new_nonce: 1 }),
    ("claim_after_expiry", ExpectedWalletAction { signers: &["lender"], old_status: 1, new_status: 3, old_nonce: 0, new_nonce: 1 }),
];

pub(crate) fn build_report(repo_root: &Path) -> Result<Value> {
    let core_live = live_verifier_facts(repo_root, CORE_LIVE)?;
    let agreement_live = live_verifier_facts(repo_root, AGREEMENT_LIVE)?;
    let wallet = json_load(repo_root, WALLET_VECTORS)?;
    let tcb = json_load(repo_root, TCB_REVIEW)?;
    let artifact_hash = normalize_hex(json_pointer_str(&tcb, "/runtime_artifact/artifact_hash"));

    let core_manifest = compare_manifest_dep(repo_root, CORE_MANIFEST, &core_live, artifact_hash.as_deref())?;
    let agreement_manifest = compare_manifest_dep(repo_root, AGREEMENT_MANIFEST, &agreement_live, artifact_hash.as_deref())?;
    let public_attestation = validate_public_attestation(repo_root, PUBLIC_CELLDEP_ATTESTATION, artifact_hash.as_deref())?;
    let external_review = validate_external_review(repo_root, EXTERNAL_TCB_ATTESTATION, artifact_hash.as_deref())?;
    let agreement_conformance = validate_agreement_profile_conformance(
        repo_root,
        &repo_root.join(CORE_MANIFEST),
        &repo_root.join(AGREEMENT_MANIFEST),
        &repo_root.join(AGREEMENT_ROOT),
    )?;
    let stateful_acceptance = build_stateful_acceptance_report(repo_root, &agreement_conformance)?;
    write_json_report(&repo_root.join(STATEFUL_ACCEPTANCE), &stateful_acceptance)?;
    let profile_certification = validate_profile_certification(ProfileCertificationInputs {
        repo_root,
        agreement_conformance: &agreement_conformance,
        agreement_manifest: &agreement_manifest,
        wallet: &wallet,
        stateful_acceptance: &stateful_acceptance,
        tcb: &tcb,
        public_attestation: &public_attestation,
        external_review: &external_review,
    })?;

    let gates = vec![
        gate(
            "agreement_profile_conforms_to_novaseal_canonical_v0",
            json_pointer_str(&agreement_conformance, "/status").unwrap_or("failed"),
            "proposals/novaseal/v0-mvp-skeleton/Cell.toml + proposals/novaseal/v0-mvp-skeleton/schemas/nova_seal_canonical_envelope_v0.schema + proposals/novaseal/agreement-profile-v0/Cell.toml + proposals/novaseal/agreement-profile-v0/src",
            agreement_conformance.clone(),
        ),
        gate(
            "agreement_profile_public_ecosystem_certification_v0",
            json_pointer_str(&profile_certification, "/status").unwrap_or("failed"),
            "proposals/novaseal/agreement-profile-v0/Cell.toml + proposals/novaseal/agreement-profile-v0/schemas + proposals/novaseal/agreement-profile-v0/fixtures + target/novaseal-devnet-stateful-acceptance.json + target/novaseal-wallet-signing-vectors.json",
            profile_certification.clone(),
        ),
        gate(
            "core_manifest_local_devnet_verifier_pin",
            if object_values_all_true(core_manifest.get("checks")) { "passed" } else { "failed" },
            CORE_MANIFEST,
            core_manifest.clone(),
        ),
        gate(
            "agreement_manifest_local_devnet_verifier_pin",
            if object_values_all_true(agreement_manifest.get("checks")) { "passed" } else { "failed" },
            AGREEMENT_MANIFEST,
            agreement_manifest.clone(),
        ),
        gate(
            "wallet_molecule_signing_vectors",
            if wallet_gate_passed(&wallet) { "passed" } else { "failed" },
            WALLET_VECTORS,
            wallet.get("summary").cloned().unwrap_or(Value::Null),
        ),
        gate(
            "bip340_runtime_verifier_local_tcb_review",
            if json_pointer_str(&tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")) {
                "passed"
            } else {
                "failed"
            },
            TCB_REVIEW,
            json!({
                "status": json_pointer_str(&tcb, "/status"),
                "artifact_hash": artifact_hash,
                "external_review_required": json_pointer_bool_opt(&tcb, "/external_review/required_for_production"),
            }),
        ),
        gate(
            "live_local_devnet_stateful_core_and_agreement",
            if stateful_acceptance_passed(&stateful_acceptance) { "passed" } else { "failed" },
            "target/novaseal-devnet-stateful-acceptance.json + target/novaseal-devnet-stateful-live.json + target/novaseal-agreement-devnet-stateful-live.json",
            json!({
                "acceptance": {
                    "status": json_pointer_str(&stateful_acceptance, "/status"),
                    "blocker_count": stateful_acceptance.get("blocker_count").and_then(Value::as_i64),
                    "live_devnet_rpc_executed": json_pointer_bool_opt(&stateful_acceptance, "/live_devnet_rpc_executed"),
                    "stateful_lifecycle_executed": json_pointer_bool_opt(&stateful_acceptance, "/stateful_lifecycle_executed"),
                    "missing": stateful_acceptance.get("missing"),
                },
                "core": core_live,
                "agreement": agreement_live,
            }),
        ),
        gate(
            "public_shared_cell_dep_pinning_attestation",
            json_pointer_str(&public_attestation, "/status").unwrap_or("failed"),
            PUBLIC_CELLDEP_ATTESTATION,
            public_attestation.clone(),
        ),
        gate(
            "external_bip340_runtime_verifier_tcb_review_attestation",
            json_pointer_str(&external_review, "/status").unwrap_or("failed"),
            EXTERNAL_TCB_ATTESTATION,
            external_review.clone(),
        ),
    ];

    let local_ready = gates
        .iter()
        .filter(|row| json_pointer_str(row, "/status") != Some("external_required"))
        .all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let production_ready = gates.iter().all(|row| json_pointer_str(row, "/status") == Some("passed"));
    let status = if production_ready {
        "production_ready"
    } else if local_ready && gates.iter().any(|row| json_pointer_str(row, "/status") == Some("external_required")) {
        "local_production_prep_ready_external_attestation_required"
    } else {
        "failed"
    };

    Ok(json!({
        "schema": "novaseal-production-gates-v0.2",
        "status": status,
        "production_ready": production_ready,
        "local_production_prep_ready": local_ready,
        "runtime_artifact_hash": json_pointer_str(&tcb, "/runtime_artifact/artifact_hash").and_then(|value| normalize_hex(Some(value))),
        "conforms_to": {
            "agreement_profile": json_pointer_str(&agreement_conformance, "/conforms_to"),
            "expected": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
            "canonical_schema_hash": json_pointer_str(&agreement_conformance, "/canonical_schema_hash"),
            "status": json_pointer_str(&agreement_conformance, "/status"),
        },
        "profile_certification": profile_certification,
        "gates": gates,
        "policy": {
            "no_placeholder_closure": "production remains false until public/shared CellDep and external TCB attestations are present",
            "attestation_templates": [
                "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json",
                "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            ],
        },
        "generated_by": {
            "implementation": IMPLEMENTATION_ID,
            "language": "rust",
        },
    }))
}

fn build_stateful_acceptance_report(repo_root: &Path, agreement_conformance: &Value) -> Result<Value> {
    let core_source = read_cell_sources(&repo_root.join(CORE_ROOT).join("src"))?;
    let agreement_source = read_cell_sources(&repo_root.join(AGREEMENT_ROOT).join("src"))?;
    let core_actions = find_actions(&core_source);
    let agreement_actions = find_actions(&agreement_source);
    let core_combined = json_load_path_optional(&repo_root.join(CORE_ROOT).join("target/novaseal-combined-tx-report.json"))?;
    let agreement_tx = json_load_path_optional(&repo_root.join(AGREEMENT_ROOT).join("target/nova-agreement-ckb-tx-report.json"))?;
    let live_core_report = json_load_path_optional(&repo_root.join(CORE_LIVE))?;
    let live_agreement_report = json_load_path_optional(&repo_root.join(AGREEMENT_LIVE))?;
    let live_core = live_core_summary(repo_root, live_core_report.as_ref())?;
    let live_agreement = live_agreement_summary(repo_root, live_agreement_report.as_ref())?;

    let core_live_passed = json_pointer_str(&live_core, "/status") == Some("passed")
        && json_pointer_bool(&live_core, "/live_devnet_rpc_executed")
        && json_pointer_bool(&live_core, "/stateful_lifecycle_executed")
        && json_pointer_bool(&live_core, "/provenance_freshness_matched")
        && json_pointer_bool(&live_core, "/old_state_not_live")
        && json_pointer_bool(&live_core, "/new_state_live")
        && json_pointer_bool(&live_core, "/receipt_live")
        && json_pointer_bool(&live_core, "/wrong_signature_rejected");
    let agreement_live_passed = json_pointer_str(&live_agreement, "/status") == Some("passed")
        && json_pointer_bool(&live_agreement, "/live_devnet_rpc_executed")
        && json_pointer_bool(&live_agreement, "/stateful_lifecycle_executed")
        && json_pointer_bool(&live_agreement, "/provenance_freshness_matched")
        && [
            "origin_active_live",
            "origin_principal_payout_live",
            "origin_receipt_live",
            "claim_origin_active_live",
            "claim_origin_principal_payout_live",
            "claim_origin_receipt_live",
            "repay_old_active_not_live",
            "repay_closed_live",
            "repay_lender_repayment_live",
            "repay_borrower_collateral_return_live",
            "repay_receipt_live",
            "claim_old_active_not_live",
            "claim_closed_live",
            "claim_lender_default_claim_live",
            "claim_receipt_live",
            "wrong_lender_signature_rejected",
            "non_ckb_asset_kind_rejected",
            "wrong_borrower_signature_rejected",
            "repay_payout_capacity_short_rejected",
            "repay_payout_lock_args_mismatch_rejected",
            "repay_wrong_payout_amount_rejected",
            "early_claim_rejected",
            "wrong_lender_claim_signature_rejected",
            "post_negative_active_still_live",
            "post_claim_negative_active_still_live",
        ]
        .iter()
        .all(|key| json_pointer_bool(&live_agreement, &format!("/{key}")))
        && json_pointer_str(agreement_conformance, "/status") == Some("passed");

    let mut core_blockers = Vec::new();
    if !has_core_bootstrap_surface(&core_source) {
        core_blockers.push(blocker(
            "NovaSeal core has key_auth_transition but no bootstrap/genesis/seed action that can create the first live NovaSealCellV0.",
            "creating an initial live state cell on devnet before the first transition",
        ));
    }
    if !has_dispatcher_surface(&core_source, &repo_root.join(CORE_ROOT)) {
        core_blockers.push(blocker(
            "NovaSeal core is still compiled as a single entry action/lock surface, not a stable lifecycle dispatcher type script.",
            "preserving one script identity across create, transition, and future terminal paths",
        ));
    }

    let mut agreement_blockers = Vec::new();
    let agreement_action_names = agreement_actions.iter().map(|action| action.name.as_str()).collect::<BTreeSet<_>>();
    let expected_agreement_actions =
        ["originate_agreement", "repay_before_expiry", "claim_after_expiry"].into_iter().collect::<BTreeSet<_>>();
    if expected_agreement_actions.is_subset(&agreement_action_names)
        && !has_dispatcher_surface(&agreement_source, &repo_root.join(AGREEMENT_ROOT))
    {
        agreement_blockers.push(blocker(
            "Agreement Profile compiles originate/repay/claim as separate entry-action ELFs; a live CKB Cell cannot move from originate ELF identity to repay/claim ELF identity.",
            "originate -> repay or originate -> claim live-cell lifecycle",
        ));
    }
    if !has_agreement_origination_surface(&agreement_source) {
        agreement_blockers.push(blocker(
            "Agreement Profile has no output-only origination action suitable for creating the initial agreement cell.",
            "first live agreement cell creation",
        ));
    }
    if json_pointer_str(agreement_conformance, "/status") != Some("passed") {
        let failed = agreement_conformance
            .get("checks")
            .and_then(Value::as_object)
            .map(|checks| {
                checks.iter().filter(|(_, value)| value.as_bool() != Some(true)).map(|(name, _)| name.clone()).collect::<Vec<_>>()
            })
            .unwrap_or_default();
        agreement_blockers.push(blocker(
            &format!("Agreement Profile does not satisfy NovaSealCanonicalV0 conformance: {}.", failed.join(", ")),
            "claiming Agreement Profile as a NovaSeal profile",
        ));
    }

    let scenarios = vec![
        json!({
            "name": "novaseal_core_key_auth_transition",
            "status": if !core_blockers.is_empty() { "blocked" } else if core_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": core_live_passed,
            "stateful_lifecycle_executed": core_live_passed,
            "actions": core_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            "blockers": core_blockers,
            "live_devnet_evidence": live_core,
            "existing_local_evidence": summary_from_report(core_combined.as_ref(), &[
                "combined_full_transaction_executed",
                "ckb_node_verification_stack_executed",
                "total_cases",
                "matched_expected",
                "node_stack_matched_expected",
                "lock_and_type_script_groups_present",
            ]),
        }),
        json!({
            "name": "agreement_profile_originate_to_terminal",
            "status": if !agreement_blockers.is_empty() { "blocked" } else if agreement_live_passed { "passed" } else { "ready_to_wire_live_devnet" },
            "live_devnet_rpc_executed": agreement_live_passed,
            "stateful_lifecycle_executed": agreement_live_passed,
            "actions": agreement_actions.iter().map(|action| action.name.clone()).collect::<Vec<_>>(),
            "blockers": agreement_blockers,
            "live_devnet_evidence": live_agreement,
            "conformance_evidence": agreement_conformance,
            "existing_local_evidence": summary_from_report(agreement_tx.as_ref(), &[
                "resolved_transaction_harness_executed",
                "ckb_node_verification_stack_executed",
                "total_cases",
                "script_matched_expected",
                "node_matched_expected",
                "fixture_files_not_executed_by_tx_harness",
            ]),
        }),
    ];
    let all_blockers = scenarios
        .iter()
        .flat_map(|scenario| scenario.get("blockers").and_then(Value::as_array).into_iter().flatten().cloned())
        .collect::<Vec<_>>();
    let status = if !all_blockers.is_empty() {
        "blocked"
    } else if scenarios.iter().all(|scenario| json_pointer_str(scenario, "/status") == Some("passed")) {
        "passed"
    } else if core_live_passed && !agreement_live_passed {
        "core_live_devnet_passed_agreement_pending"
    } else if agreement_live_passed && !core_live_passed {
        "agreement_live_devnet_passed_core_pending"
    } else {
        "ready_to_run_live_devnet"
    };

    Ok(json!({
        "schema": "novaseal-devnet-stateful-acceptance-v0.1",
        "classification": "live_devnet_stateful_release_gate",
        "status": status,
        "production_ready": false,
        "live_devnet_rpc_executed": scenarios.iter().all(|scenario| json_pointer_bool(scenario, "/live_devnet_rpc_executed")),
        "stateful_lifecycle_executed": scenarios.iter().all(|scenario| json_pointer_bool(scenario, "/stateful_lifecycle_executed")),
        "repo_root": repo_root.display().to_string(),
        "requirements": [
            "deploy runtime verifier and protocol artifacts as live CellDeps",
            "submit transactions through CKB RPC, not only in-memory ResolvedTransaction",
            "commit each valid step and verify old inputs are dead plus new state/receipt/payout outputs are live",
            "verify live output capacity/lock/type/data and reject stale source/artifact provenance",
            "prove negative dry-runs fail from the expected lifecycle script and artifact hash",
            "use one stable type-script identity for a lifecycle, or an explicitly audited dispatcher/bootstrap surface",
            "run negative cases as dry-run/send-test rejections without mutating live state",
            "require every NovaSeal profile to pass conforms_to = NovaSealCanonicalV0 conformance",
        ],
        "scenarios": scenarios,
        "blocker_count": all_blockers.len(),
        "blockers": all_blockers,
        "next_engineering_step": if status == "passed" {
            "Stateful live-devnet acceptance is complete; production readiness is now governed by public CellDep pinning, wallet/Molecule vectors, and external verifier TCB attestation."
        } else {
            "Re-run the live core/agreement devnet runners after source or artifact changes; this gate fails closed until both reports have fresh provenance, strict output checks, and matched negative dry-run errors."
        },
        "generated_by": {
            "implementation": IMPLEMENTATION_ID,
            "language": "rust",
        },
    }))
}

#[derive(Clone)]
struct ActionSurface {
    name: String,
    params: String,
}

impl ActionSurface {
    fn consumes_resource(&self) -> bool {
        self.params.contains("NovaSealCellV0") || self.params.contains("NovaAgreementCellV0")
    }
}

fn find_actions(source: &str) -> Vec<ActionSurface> {
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let rest = line.strip_prefix("action ")?;
            let name_end = rest.find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))?;
            let name = rest[..name_end].to_string();
            let params_start = rest[name_end..].find('(')? + name_end + 1;
            let params_end = rest[params_start..].find(')')? + params_start;
            Some(ActionSurface { name, params: rest[params_start..params_end].to_string() })
        })
        .collect()
}

fn has_dispatcher_surface(source: &str, root: &Path) -> bool {
    let names = find_actions(source).into_iter().map(|action| action.name).collect::<BTreeSet<_>>();
    let manifest = std::fs::read_to_string(root.join("Cell.toml")).unwrap_or_default();
    names.iter().any(|name| ["dispatch", "dispatch_agreement", "novaseal_dispatch", "agreement_dispatch"].contains(&name.as_str()))
        || manifest.contains("stateful_dispatcher")
        || (manifest.contains("dispatcher") && manifest.contains("entry"))
}

fn has_core_bootstrap_surface(source: &str) -> bool {
    let actions = find_actions(source);
    if actions.iter().any(|action| action.name == "novaseal_lifecycle") && source.contains("OP_BOOTSTRAP") {
        return true;
    }
    actions.iter().any(|action| {
        let lowered = action.name.to_ascii_lowercase();
        ["bootstrap", "genesis", "seed", "initialize", "originate"].iter().any(|word| lowered.contains(word))
            && !action.consumes_resource()
    })
}

fn has_agreement_origination_surface(source: &str) -> bool {
    let actions = find_actions(source);
    if actions.iter().any(|action| action.name == "nova_agreement_lifecycle") && source.contains("PATH_ORIGINATE") {
        return true;
    }
    actions.iter().any(|action| action.name == "originate_agreement" && !action.consumes_resource())
}

fn live_core_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let transition = report.get("transition").cloned().unwrap_or(Value::Null);
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            CORE_MANIFEST,
            "proposals/novaseal/v0-mvp-skeleton/src",
            "proposals/novaseal/v0-mvp-skeleton/schemas",
            VERIFIER_ROOT,
            "scripts/novaseal_devnet_stateful_live.py",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "bootstrap_tx_hash": json_pointer_str(report, "/bootstrap/commit/tx_hash"),
        "transition_tx_hash": json_pointer_str(&transition, "/commit/tx_hash"),
        "old_state_not_live": json_pointer_bool_opt(&transition, "/old_state_not_live"),
        "new_state_live": json_pointer_bool_opt(&transition, "/new_state_live"),
        "receipt_live": json_pointer_bool_opt(&transition, "/receipt_live"),
        "wrong_signature_rejected": negative_case_matched(report, "wrong_signature_dry_run"),
    }))
}

fn live_agreement_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            AGREEMENT_MANIFEST,
            "proposals/novaseal/agreement-profile-v0/src",
            "proposals/novaseal/agreement-profile-v0/schemas",
            VERIFIER_ROOT,
            "scripts/novaseal_agreement_devnet_stateful_live.py",
            "scripts/novaseal_devnet_stateful_live.py",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "originate_tx_hash": json_pointer_str(report, "/originate/commit/tx_hash"),
        "repay_tx_hash": json_pointer_str(report, "/repay/commit/tx_hash"),
        "claim_originate_tx_hash": json_pointer_str(report, "/claim_originate/commit/tx_hash"),
        "claim_tx_hash": json_pointer_str(report, "/claim/commit/tx_hash"),
        "origin_active_live": json_pointer_bool_opt(report, "/originate/active_live"),
        "origin_principal_payout_live": json_pointer_bool_opt(report, "/originate/principal_payout_live"),
        "origin_receipt_live": json_pointer_bool_opt(report, "/originate/receipt_live"),
        "claim_origin_active_live": json_pointer_bool_opt(report, "/claim_originate/active_live"),
        "claim_origin_principal_payout_live": json_pointer_bool_opt(report, "/claim_originate/principal_payout_live"),
        "claim_origin_receipt_live": json_pointer_bool_opt(report, "/claim_originate/receipt_live"),
        "repay_old_active_not_live": json_pointer_bool_opt(report, "/repay/old_active_not_live"),
        "repay_closed_live": json_pointer_bool_opt(report, "/repay/closed_live"),
        "repay_lender_repayment_live": json_pointer_bool_opt(report, "/repay/lender_repayment_live"),
        "repay_borrower_collateral_return_live": json_pointer_bool_opt(report, "/repay/borrower_collateral_return_live"),
        "repay_receipt_live": json_pointer_bool_opt(report, "/repay/receipt_live"),
        "claim_old_active_not_live": json_pointer_bool_opt(report, "/claim/old_active_not_live"),
        "claim_closed_live": json_pointer_bool_opt(report, "/claim/closed_live"),
        "claim_lender_default_claim_live": json_pointer_bool_opt(report, "/claim/lender_default_claim_live"),
        "claim_receipt_live": json_pointer_bool_opt(report, "/claim/receipt_live"),
        "wrong_lender_signature_rejected": negative_case_matched(report, "wrong_lender_signature_dry_run"),
        "non_ckb_asset_kind_rejected": negative_case_matched(report, "non_ckb_asset_kind_dry_run"),
        "wrong_borrower_signature_rejected": negative_case_matched(report, "wrong_borrower_signature_dry_run"),
        "repay_payout_capacity_short_rejected": negative_case_matched(report, "repay_payout_capacity_short_dry_run"),
        "repay_payout_lock_args_mismatch_rejected": negative_case_matched(report, "repay_payout_lock_args_mismatch_dry_run"),
        "repay_wrong_payout_amount_rejected": negative_case_matched(report, "repay_wrong_payout_amount_dry_run"),
        "early_claim_rejected": negative_case_matched(report, "early_claim_dry_run"),
        "wrong_lender_claim_signature_rejected": negative_case_matched(report, "wrong_lender_claim_signature_dry_run"),
        "post_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_negative_active_still_live"),
        "post_claim_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_claim_negative_active_still_live"),
    }))
}

fn provenance_summary(report: &Value, repo_root: &Path, source_paths: &[&str]) -> Result<Value> {
    let provenance = report.get("provenance").cloned().unwrap_or(Value::Null);
    let recorded_source = provenance.get("source_tree").cloned().unwrap_or(Value::Null);
    let current_source = source_tree_hash(repo_root, source_paths)?;
    let source_hash_matches = json_pointer_str(&recorded_source, "/sha256") == json_pointer_str(&current_source, "/sha256");
    let mut artifact_checks = Map::new();
    let recorded_artifacts = provenance.get("artifacts").cloned().unwrap_or(Value::Null);
    for name in ["verifier", "lifecycle"] {
        let artifact = recorded_artifacts.get(name).cloned().unwrap_or(Value::Null);
        let raw_path = json_pointer_str(&artifact, "/path");
        let path = raw_path.map(|value| {
            let path = Path::new(value);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            }
        });
        let exists = path.as_ref().is_some_and(|path| path.is_file());
        let current_sha = path.as_ref().filter(|path| path.is_file()).map(|path| sha256_file_hex(path)).transpose()?;
        artifact_checks.insert(
            name.to_string(),
            json!({
                "present": artifact.is_object(),
                "path": raw_path,
                "exists": exists,
                "sha256_matches": current_sha.as_deref() == json_pointer_str(&artifact, "/sha256"),
                "recorded_sha256": json_pointer_str(&artifact, "/sha256"),
                "current_sha256": current_sha,
            }),
        );
    }
    let artifact_hashes_match = artifact_checks.values().all(|row| {
        json_pointer_bool(row, "/present") && json_pointer_bool(row, "/exists") && json_pointer_bool(row, "/sha256_matches")
    });
    let current_commit = git_commit(repo_root);
    Ok(json!({
        "present": provenance.is_object(),
        "freshness_matched": source_hash_matches && artifact_hashes_match,
        "repo_commit": json_pointer_str(&provenance, "/repo_commit"),
        "current_repo_commit": current_commit,
        "repo_commit_matches": json_pointer_str(&provenance, "/repo_commit") == current_commit.as_deref(),
        "source_hash_matches": source_hash_matches,
        "recorded_source_hash": json_pointer_str(&recorded_source, "/sha256"),
        "current_source_hash": json_pointer_str(&current_source, "/sha256"),
        "recorded_file_count": recorded_source.get("file_count").and_then(Value::as_u64),
        "current_file_count": current_source.get("file_count").and_then(Value::as_u64),
        "artifact_hashes_match": artifact_hashes_match,
        "artifacts": artifact_checks,
    }))
}

fn source_tree_hash(repo_root: &Path, paths: &[&str]) -> Result<Value> {
    let mut files = BTreeSet::new();
    for raw_path in paths {
        let path = repo_root.join(raw_path);
        if path.is_file() {
            files.insert(path);
        } else if path.is_dir() {
            collect_source_tree_files(&path, &path, &mut files)?;
        }
    }
    let mut hasher = Sha256::new();
    let mut rows = Vec::new();
    for path in files {
        let rel_path = rel(repo_root, &path);
        let digest = Sha256::digest(std::fs::read(&path)?);
        hasher.update(rel_path.as_bytes());
        hasher.update([0]);
        hasher.update(digest);
        rows.push(rel_path);
    }
    Ok(json!({
        "sha256": format!("0x{}", hex::encode(hasher.finalize())),
        "files": rows,
        "file_count": rows.len(),
    }))
}

fn collect_source_tree_files(root: &Path, path: &Path, files: &mut BTreeSet<std::path::PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let relative_parts = child.strip_prefix(root).unwrap_or(&child).components().map(|part| part.as_os_str().to_string_lossy());
        if relative_parts.clone().any(|part| matches!(part.as_ref(), "target" | "build" | ".git" | "__pycache__")) {
            continue;
        }
        if child.is_dir() {
            collect_source_tree_files(root, &child, files)?;
        } else if child.is_file() && source_tree_file_allowed(&child) {
            files.insert(child);
        }
    }
    Ok(())
}

fn source_tree_file_allowed(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "Cargo.lock")
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "cell" | "schema" | "toml" | "py" | "json" | "rs"))
}

fn git_commit(repo_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git").arg("rev-parse").arg("HEAD").current_dir(repo_root).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn negative_case_matched(report: &Value, key: &str) -> Option<bool> {
    let row = report.pointer(&format!("/negative_cases/{key}"))?;
    Some(json_pointer_str(row, "/status") == Some("rejected") && json_pointer_bool(row, "/matched_expected"))
}

fn summary_from_report(report: Option<&Value>, summary_keys: &[&str]) -> Value {
    let Some(report) = report else {
        return json!({"present": false});
    };
    if let Some(error) = report.get("_invalid_json") {
        return json!({"present": true, "valid_json": false, "error": error});
    }
    let Some(summary) = report.get("summary").and_then(Value::as_object) else {
        return json!({"present": true, "valid_json": true, "summary_present": false});
    };
    let mut out = Map::from_iter([
        ("present".to_string(), Value::Bool(true)),
        ("valid_json".to_string(), Value::Bool(true)),
        ("summary_present".to_string(), Value::Bool(true)),
    ]);
    for key in summary_keys {
        out.insert((*key).to_string(), summary.get(*key).cloned().unwrap_or(Value::Null));
    }
    Value::Object(out)
}

fn blocker(text: &str, required_for: &str) -> Value {
    json!({"blocker": text, "required_for": required_for})
}

pub(crate) fn validate_agreement_profile_conformance(
    repo_root: &Path,
    core_manifest_path: &Path,
    agreement_manifest_path: &Path,
    agreement_root: &Path,
) -> Result<Value> {
    let core_metadata = manifest_metadata(core_manifest_path)?;
    let agreement_metadata = manifest_metadata(agreement_manifest_path)?;
    let agreement_source = read_cell_sources(&agreement_root.join("src"))?;
    let schema_path = repo_root.join(CANONICAL_SCHEMA);
    let schema_hash = canonical_schema_hash(&schema_path)?;
    let schema_checks = canonical_schema_checks(&schema_path)?;
    let source_checks = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| (format!("source_{name}"), Value::Bool(agreement_source.contains(pattern))))
        .collect::<Map<_, _>>();

    let mut checks = schema_checks;
    checks.extend([
        (
            "core_declares_canonical_schema".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "core_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&core_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        ("core_package_role".to_string(), Value::Bool(toml_str(&core_metadata, "package_role") == Some("canonical-example"))),
        ("core_protocol_family".to_string(), Value::Bool(toml_str(&core_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_protocol_family".to_string(), Value::Bool(toml_str(&agreement_metadata, "protocol_family") == Some("NovaSeal"))),
        ("profile_name".to_string(), Value::Bool(toml_str(&agreement_metadata, "profile") == Some(EXPECTED_AGREEMENT_PROFILE))),
        (
            "profile_conforms_to".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conforms_to") == Some(EXPECTED_NOVASEAL_CANONICAL_SCHEMA)),
        ),
        (
            "profile_canonical_schema_hash".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "canonical_schema_hash") == schema_hash.as_deref()),
        ),
        (
            "profile_conformance_gate".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "conformance_gate") == Some(EXPECTED_AGREEMENT_CONFORMANCE_GATE)),
        ),
        (
            "profile_certification_plugin".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_plugin") == Some(EXPECTED_CERTIFICATION_PLUGIN)),
        ),
        (
            "profile_certification_report".to_string(),
            Value::Bool(toml_str(&agreement_metadata, "certification_report") == Some(EXPECTED_CERTIFICATION_REPORT)),
        ),
    ]);
    checks.extend(source_checks);

    let source_patterns = REQUIRED_AGREEMENT_CORE_PATTERNS
        .iter()
        .map(|(name, pattern)| ((*name).to_string(), Value::String((*pattern).to_string())))
        .collect::<Map<_, _>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&Value::Object(checks.clone()))) { "passed" } else { "failed" },
        "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
        "expected_conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
        "canonical_schema_file": rel(repo_root, &schema_path),
        "canonical_schema_hash": schema_hash,
        "canonical_schema_hash_algorithm": "sha256(normalized schema lines: comments/blank lines ignored, whitespace collapsed)",
        "canonical_schema_lines": canonical_schema_lines(&schema_path)?,
        "core_manifest": rel(repo_root, core_manifest_path),
        "profile_manifest": rel(repo_root, agreement_manifest_path),
        "checks": checks,
        "manifest": {
            "canonical_schema": toml_str(&core_metadata, "canonical_schema"),
            "canonical_schema_hash": toml_str(&core_metadata, "canonical_schema_hash"),
            "package_role": toml_str(&core_metadata, "package_role"),
            "core_protocol_family": toml_str(&core_metadata, "protocol_family"),
            "profile": toml_str(&agreement_metadata, "profile"),
            "protocol_family": toml_str(&agreement_metadata, "protocol_family"),
            "conforms_to": toml_str(&agreement_metadata, "conforms_to"),
            "profile_canonical_schema_hash": toml_str(&agreement_metadata, "canonical_schema_hash"),
            "conformance_gate": toml_str(&agreement_metadata, "conformance_gate"),
        },
        "source_patterns": source_patterns,
    }))
}

struct ProfileCertificationInputs<'a> {
    repo_root: &'a Path,
    agreement_conformance: &'a Value,
    agreement_manifest: &'a Value,
    wallet: &'a Value,
    stateful_acceptance: &'a Value,
    tcb: &'a Value,
    public_attestation: &'a Value,
    external_review: &'a Value,
}

fn validate_profile_certification(input: ProfileCertificationInputs<'_>) -> Result<Value> {
    let ProfileCertificationInputs {
        repo_root,
        agreement_conformance,
        agreement_manifest,
        wallet,
        stateful_acceptance,
        tcb,
        public_attestation,
        external_review,
    } = input;
    let schema_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("schemas"), EXPECTED_AGREEMENT_SCHEMA_FILES)?;
    let fixture_files = expected_files(repo_root, &repo_root.join(AGREEMENT_ROOT).join("fixtures"), EXPECTED_AGREEMENT_FIXTURES)?;
    let wallet_detail = validate_wallet_vector_detail(wallet);
    let invariant_matrix = validate_invariant_matrix(repo_root, &repo_root.join(AGREEMENT_ROOT).join("proofs/invariant_matrix.json"))?;
    let live_evidence = agreement_live_evidence(stateful_acceptance);
    let docs = json!({
        "agreement_profile": repo_root.join(AGREEMENT_ROOT).join("docs/AGREEMENT_PROFILE.md").is_file(),
        "security": repo_root.join(AGREEMENT_ROOT).join("docs/SECURITY.md").is_file(),
        "audit_status": repo_root.join(AGREEMENT_ROOT).join("docs/AUDIT_STATUS.md").is_file(),
        "devnet_acceptance": repo_root.join(AGREEMENT_ROOT).join("docs/DEVNET_STATEFUL_ACCEPTANCE.md").is_file(),
    });
    let external_checks = json!({
        "public_shared_cell_dep_attested": json_pointer_str(public_attestation, "/status") == Some("passed"),
        "external_bip340_tcb_review_attested": json_pointer_str(external_review, "/status") == Some("passed"),
    });
    let local_checks = json!({
        "conformance_gate_passed": json_pointer_str(agreement_conformance, "/status") == Some("passed"),
        "profile_schema_set_exact": json_pointer_bool(&schema_files, "/exact"),
        "profile_fixture_set_exact": json_pointer_bool(&fixture_files, "/exact"),
        "wallet_vector_detail_passed": json_pointer_str(&wallet_detail, "/status") == Some("passed"),
        "invariant_matrix_passed": json_pointer_str(&invariant_matrix, "/status") == Some("passed"),
        "live_devnet_evidence_passed": json_pointer_str(&live_evidence, "/status") == Some("passed"),
        "agreement_runtime_verifier_pin_passed": object_values_all_true(agreement_manifest.get("checks")),
        "local_bip340_tcb_review_passed": json_pointer_str(tcb, "/status").is_some_and(|status| status.starts_with("passed_local_review")),
        "required_docs_present": object_values_all_true(Some(&docs)),
    });
    let local_passed = object_values_all_true(Some(&local_checks));
    let production_statement_eligible = local_passed && object_values_all_true(Some(&external_checks));
    let production_statement_blockers = external_checks
        .as_object()
        .into_iter()
        .flat_map(|object| object.iter())
        .filter(|(_, passed)| passed.as_bool() != Some(true))
        .map(|(name, _)| Value::String(name.clone()))
        .collect::<Vec<_>>();

    Ok(json!({
        "schema": "novaseal-profile-certification-v0.1",
        "profile": EXPECTED_AGREEMENT_PROFILE,
        "conforms_to": EXPECTED_NOVASEAL_CANONICAL_SCHEMA,
        "gate": EXPECTED_PROFILE_CERTIFICATION_GATE,
        "status": if local_passed { "passed" } else { "failed" },
        "certification_level": if local_passed {
            "public_ecosystem_profile_certification_local_ready"
        } else {
            "public_ecosystem_profile_certification_failed"
        },
        "production_statement_eligible": production_statement_eligible,
        "production_statement_blockers": production_statement_blockers,
        "local_checks": local_checks,
        "external_checks": external_checks,
        "schema_files": schema_files,
        "fixture_files": fixture_files,
        "wallet_vectors": wallet_detail,
        "invariant_matrix": invariant_matrix,
        "live_devnet": live_evidence,
        "docs": docs,
        "design_boundary": {
            "agreement_calls_core_runtime": false,
            "canonical_constraint": "manifest canonical_schema_hash + signed canonical_envelope_hash + runtime recomputation",
            "rgb_code_vendored": false,
            "rgbplusplus_schema_dependency": false,
            "new_runtime_machinery_added": false,
        },
    }))
}

fn validate_wallet_vector_detail(wallet: &Value) -> Value {
    let vectors = wallet.get("vectors").and_then(Value::as_array).cloned().unwrap_or_default();
    let agreement_vectors = vectors
        .iter()
        .filter(|vector| json_pointer_str(vector, "/suite") == Some("novaseal-agreement-profile-v0"))
        .cloned()
        .collect::<Vec<_>>();
    let mut by_action: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for vector in &agreement_vectors {
        if let Some(action) = json_pointer_str(vector, "/action") {
            by_action.entry(action.to_string()).or_default().push(vector.clone());
        }
    }

    let mut action_checks = Map::new();
    for (action, expected) in EXPECTED_AGREEMENT_WALLET_ACTIONS {
        let matches = by_action.get(*action).cloned().unwrap_or_default();
        let vector = matches.first().cloned().unwrap_or(Value::Null);
        let display = vector.get("wallet_display").cloned().unwrap_or(Value::Null);
        let packed = json_pointer_str(&vector, "/signed_intent_packed_hex");
        let byte_len = packed.and_then(|value| is_hex_bytes(value).then_some((value.len() - 2) / 2));
        action_checks.insert(
            (*action).to_string(),
            json!({
                "exactly_one_vector": matches.len() == 1,
                "status_passed": json_pointer_str(&vector, "/status") == Some("passed"),
                "signed_type": json_pointer_str(&vector, "/signed_type") == Some("NovaAgreementSignedIntentV0"),
                "fixed_width_signed_intent_259_bytes": byte_len == Some(259),
                "bip340_message_hash": json_pointer_str(&vector, "/bip340_message_hash").is_some_and(is_hex32),
                "expected_receipt_hash": json_pointer_str(&vector, "/expected_receipt_hash").is_some_and(is_hex32),
                "canonical_envelope_hash_displayed": json_pointer_str(&display, "/canonical_envelope_hash").is_some_and(is_hex32),
                "payout_commitment_hash_displayed": json_pointer_str(&display, "/payout_commitment_hash").is_some_and(is_hex32),
                "agreement_id_displayed": json_pointer_str(&display, "/agreement_id").is_some_and(is_hex32),
                "terms_hash_displayed": json_pointer_str(&display, "/terms_hash").is_some_and(is_hex32),
                "borrower_authority_displayed": json_pointer_str(&display, "/borrower_authority_hash").is_some_and(is_hex32),
                "lender_authority_displayed": json_pointer_str(&display, "/lender_authority_hash").is_some_and(is_hex32),
                "signers_match": json_array_strings(&vector, "/signers") == expected.signers,
                "status_transition_match": json_pointer_i64(&display, "/old_status") == Some(expected.old_status)
                    && json_pointer_i64(&display, "/new_status") == Some(expected.new_status),
                "nonce_transition_match": json_pointer_i64(&display, "/old_nonce") == Some(expected.old_nonce)
                    && json_pointer_i64(&display, "/new_nonce") == Some(expected.new_nonce),
                "terminal_amount_positive": json_pointer_i64(&display, "/terminal_amount_shannons").is_some_and(|amount| amount > 0),
            }),
        );
    }

    let actions_present = by_action.keys().cloned().collect::<BTreeSet<_>>();
    let expected_actions = EXPECTED_AGREEMENT_WALLET_ACTIONS.iter().map(|(name, _)| (*name).to_string()).collect::<BTreeSet<_>>();
    let checks = json!({
        "wallet_report_passed": json_pointer_str(wallet, "/status") == Some("passed"),
        "summary_counts_match": json_pointer_i64(wallet, "/summary/agreement_vectors") == Some(3)
            && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
            && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total"),
        "exact_agreement_actions": actions_present == expected_actions,
        "agreement_action_details": action_checks.values().all(|row| object_values_all_true(Some(row))),
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "actions": action_checks,
        "expected_actions": expected_actions.into_iter().collect::<Vec<_>>(),
        "agreement_vector_count": agreement_vectors.len(),
    })
}

fn validate_invariant_matrix(repo_root: &Path, path: &Path) -> Result<Value> {
    let payload = json_load_path(repo_root, path)?;
    let invariants = payload.get("invariants").and_then(Value::as_array).cloned().unwrap_or_default();
    let ids = invariants.iter().filter_map(|row| json_pointer_str(row, "/id").map(str::to_string)).collect::<BTreeSet<_>>();
    let required = EXPECTED_CERTIFICATION_INVARIANTS.iter().map(|value| (*value).to_string()).collect::<BTreeSet<_>>();
    let coverage_by_id = invariants
        .iter()
        .filter_map(|row| Some((json_pointer_str(row, "/id")?.to_string(), row.get("coverage").cloned().unwrap_or(Value::Null))))
        .collect::<Map<_, _>>();
    let checks = json!({
        "file_present": payload.get("missing").is_none(),
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-agreement-invariant-matrix-v0.1"),
        "required_invariants_present": required.is_subset(&ids),
        "no_empty_coverage": ids.iter().all(|id| coverage_by_id.get(id).is_some_and(value_is_present)),
    });
    let missing = required.difference(&ids).cloned().collect::<Vec<_>>();
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "required": required.into_iter().collect::<Vec<_>>(),
        "present": ids.into_iter().collect::<Vec<_>>(),
        "missing": missing,
        "coverage_by_id": coverage_by_id,
    }))
}

fn agreement_live_evidence(stateful_acceptance: &Value) -> Value {
    let agreement = stateful_acceptance
        .get("scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| {
            scenarios.iter().find(|scenario| json_pointer_str(scenario, "/name") == Some("agreement_profile_originate_to_terminal"))
        })
        .cloned()
        .unwrap_or(Value::Null);
    let evidence = agreement.get("live_devnet_evidence").cloned().unwrap_or(Value::Null);
    let negative_checks = EXPECTED_LIVE_NEGATIVE_KEYS
        .iter()
        .map(|key| ((*key).to_string(), Value::Bool(json_pointer_bool(&evidence, &format!("/{key}")))))
        .collect::<Map<_, _>>();
    let live_keys = [
        "origin_active_live",
        "origin_principal_payout_live",
        "origin_receipt_live",
        "repay_old_active_not_live",
        "repay_closed_live",
        "repay_lender_repayment_live",
        "repay_borrower_collateral_return_live",
        "repay_receipt_live",
        "claim_old_active_not_live",
        "claim_closed_live",
        "claim_lender_default_claim_live",
        "claim_receipt_live",
    ];
    let checks = json!({
        "acceptance_passed": json_pointer_str(stateful_acceptance, "/status") == Some("passed"),
        "no_blockers": json_pointer_i64(stateful_acceptance, "/blocker_count") == Some(0),
        "live_devnet_rpc_executed": json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed"),
        "agreement_scenario_passed": json_pointer_str(&agreement, "/status") == Some("passed"),
        "agreement_provenance_fresh": json_pointer_bool(&evidence, "/provenance_freshness_matched"),
        "valid_originate_repay_claim_live": live_keys.iter().all(|key| json_pointer_bool(&evidence, &format!("/{key}"))),
        "negative_cases_rejected": object_values_all_true(Some(&Value::Object(negative_checks.clone()))),
    });
    json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "negative_checks": negative_checks,
        "evidence": evidence,
    })
}

fn compare_manifest_dep(repo_root: &Path, manifest_rel: &str, live: &Value, artifact_hash: Option<&str>) -> Result<Value> {
    let manifest_path = repo_root.join(manifest_rel);
    let manifest = toml_value(&manifest_path)?;
    let dep = runtime_dep(&manifest)?;
    let parsed = parse_out_point(toml_str(&dep, "out_point"));
    let expected_metadata = EXPECTED_VERIFIER.iter().all(|(key, value)| toml_str(&dep, key) == Some(*value));
    let production = manifest
        .get("policy")
        .and_then(toml::Value::as_table)
        .and_then(|policy| policy.get("production"))
        .and_then(toml::Value::as_bool);
    let checks = json!({
        "expected_metadata": expected_metadata,
        "out_point_valid": json_pointer_bool(&parsed, "/valid"),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(toml_str(&dep, "data_hash")).as_deref()),
        "artifact_hash_non_placeholder": !placeholder_hash(normalize_hex(toml_str(&dep, "artifact_hash")).as_deref()),
        "matches_live_data_hash": normalize_hex(toml_str(&dep, "data_hash")).as_deref() == json_pointer_str(live, "/data_hash"),
        "matches_live_dep_type": toml_str(&dep, "dep_type") == json_pointer_str(live, "/dep_type"),
        "matches_artifact_hash": normalize_hex(toml_str(&dep, "artifact_hash")).as_deref() == artifact_hash,
        "production_false_until_public_attestation": production == Some(false),
    });
    Ok(json!({
        "manifest": manifest_rel,
        "checks": checks,
        "dep": toml_to_json(&dep),
        "live": live,
        "policy": {
            "out_point": "manifest out_point is a pinned deployment descriptor; local live-devnet runs redeploy ephemeral outpoints and are compared by verifier data hash/artifact hash instead",
        },
    }))
}

fn validate_public_attestation(repo_root: &Path, rel_path: &str, artifact_hash: Option<&str>) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({"status": "external_required", "reason": "missing public/shared CellDep attestation"}));
    }
    let payload = json_load_path(repo_root, &path)?;
    let verifier = payload.get("runtime_verifier").cloned().unwrap_or(Value::Null);
    let parsed = parse_out_point(json_pointer_str(&verifier, "/out_point"));
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-public-shared-cell-dep-attestation-v0.1"),
        "status": json_pointer_str(&payload, "/status") == Some("attested"),
        "network_not_local_devnet": json_pointer_str(&payload, "/network").is_some_and(|network| !network.is_empty() && network != "local-devnet"),
        "artifact_hash": normalize_hex(json_pointer_str(&verifier, "/artifact_hash")).as_deref() == artifact_hash,
        "data_hash_non_placeholder": !placeholder_hash(normalize_hex(json_pointer_str(&verifier, "/data_hash")).as_deref()),
        "out_point_non_placeholder": !placeholder_hash(json_pointer_str(&parsed, "/tx_hash")),
        "verifier_id": json_pointer_str(&verifier, "/verifier_id") == Some("btc.bip340.v0"),
        "ipc_abi": json_pointer_str(&verifier, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn validate_external_review(repo_root: &Path, rel_path: &str, artifact_hash: Option<&str>) -> Result<Value> {
    let path = repo_root.join(rel_path);
    if !path.exists() {
        return Ok(json!({"status": "external_required", "reason": "missing external BIP340 TCB review attestation"}));
    }
    let payload = json_load_path(repo_root, &path)?;
    let checks = json!({
        "schema": json_pointer_str(&payload, "/schema") == Some("novaseal-bip340-external-tcb-review-attestation-v0.1"),
        "status": json_pointer_str(&payload, "/status") == Some("accepted"),
        "artifact_hash": normalize_hex(json_pointer_str(&payload, "/artifact_hash")).as_deref() == artifact_hash,
        "verifier_id": json_pointer_str(&payload, "/verifier_id") == Some("btc.bip340.v0"),
        "ipc_abi": json_pointer_str(&payload, "/ipc_abi") == Some("cellscript-btc-bip340-ipc-v0"),
        "reviewer_present": json_pointer_str(&payload, "/reviewer").is_some_and(|value| !value.is_empty()),
        "review_date_present": json_pointer_str(&payload, "/review_date").is_some_and(|value| !value.is_empty()),
    });
    Ok(json!({
        "status": if object_values_all_true(Some(&checks)) { "passed" } else { "failed" },
        "checks": checks,
        "attestation": payload,
    }))
}

fn live_verifier_facts(repo_root: &Path, rel_path: &str) -> Result<Value> {
    let payload = json_load(repo_root, rel_path)?;
    let verifier = payload.pointer("/artifacts/verifier").cloned().unwrap_or(Value::Null);
    let out_point = verifier.pointer("/cell_dep/out_point").cloned().unwrap_or(Value::Null);
    let index = json_pointer_str(&out_point, "/index")
        .and_then(|value| value.strip_prefix("0x").and_then(|hex| u64::from_str_radix(hex, 16).ok()))
        .or_else(|| out_point.get("index").and_then(Value::as_u64));
    Ok(json!({
        "status": json_pointer_str(&payload, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool_opt(&payload, "/live_devnet_rpc_executed"),
        "name": json_pointer_str(&verifier, "/name"),
        "tx_hash": normalize_hex(json_pointer_str(&out_point, "/tx_hash")),
        "index": index,
        "dep_type": json_pointer_str(&verifier, "/cell_dep/dep_type"),
        "data_hash": normalize_hex(json_pointer_str(&verifier, "/data_hash")),
        "artifact_size_bytes": verifier.get("artifact_size_bytes").and_then(Value::as_u64),
    }))
}

fn runtime_dep(manifest: &toml::Value) -> Result<toml::Value> {
    let deps = manifest
        .get("deploy")
        .and_then(toml::Value::as_table)
        .and_then(|deploy| deploy.get("ckb"))
        .and_then(toml::Value::as_table)
        .and_then(|ckb| ckb.get("cell_deps"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let matches = deps
        .into_iter()
        .filter(|dep| {
            toml_str(dep, "role") == Some("runtime_verifier") || toml_str(dep, "name") == Some("cellscript_btc_bip340_verifier_riscv")
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(CompileError::without_span(format!(
            "expected exactly one NovaSeal runtime verifier dep, found {}",
            matches.len()
        )));
    }
    Ok(matches[0].clone())
}

fn expected_files(repo_root: &Path, root: &Path, names: &[&str]) -> Result<Value> {
    let expected = names.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>();
    let found = if root.is_dir() {
        std::fs::read_dir(root)?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| entry.file_type().ok().filter(|ty| ty.is_file()).map(|_| entry.file_name()))
            .filter_map(|name| name.into_string().ok())
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };
    let mut hashes = Map::new();
    for name in &expected {
        let path = root.join(name);
        if path.is_file() {
            hashes.insert(name.clone(), Value::String(sha256_file_hex(&path)?));
        }
    }
    Ok(json!({
        "root": rel(repo_root, root),
        "expected": expected.iter().cloned().collect::<Vec<_>>(),
        "present": found.intersection(&expected).cloned().collect::<Vec<_>>(),
        "missing": expected.difference(&found).cloned().collect::<Vec<_>>(),
        "extra": found.difference(&expected).cloned().collect::<Vec<_>>(),
        "hashes": hashes,
        "exact": found == expected,
    }))
}

fn canonical_schema_lines(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = std::fs::read_to_string(path)?;
    Ok(source
        .lines()
        .filter_map(|raw| {
            let stripped = raw.split_once('#').map_or(raw, |(before, _)| before).trim();
            if stripped.is_empty() {
                return None;
            }
            if let Some((name, rest)) = stripped.split_once(':') {
                let rest = rest.split_whitespace().collect::<Vec<_>>().join(" ");
                if rest.is_empty() {
                    Some(format!("{}:", name.trim()))
                } else {
                    Some(format!("{}: {rest}", name.trim()))
                }
            } else {
                Some(stripped.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect())
}

fn canonical_schema_hash(path: &Path) -> Result<Option<String>> {
    let lines = canonical_schema_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }
    let mut payload = lines.join("\n").into_bytes();
    payload.push(b'\n');
    Ok(Some(format!("0x{}", hex::encode(Sha256::digest(payload)))))
}

fn canonical_schema_checks(path: &Path) -> Result<Map<String, Value>> {
    let lines = canonical_schema_lines(path)?;
    let mut expected_lines = vec![format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:")];
    expected_lines.extend(EXPECTED_CANONICAL_SCHEMA_FIELDS.iter().map(|(name, ty)| format!("{name}: {ty}")));
    let expected_set = expected_lines.iter().cloned().collect::<BTreeSet<_>>();
    let lines_set = lines.iter().cloned().collect::<BTreeSet<_>>();
    Ok([
        ("canonical_schema_file_present".to_string(), Value::Bool(path.exists())),
        (
            "canonical_schema_name".to_string(),
            Value::Bool(lines.first().is_some_and(|line| line == &format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:"))),
        ),
        ("canonical_schema_exact_field_order".to_string(), Value::Bool(lines == expected_lines)),
        ("canonical_schema_no_extra_fields".to_string(), Value::Bool(lines_set == expected_set)),
        ("canonical_schema_normalized_hash_present".to_string(), Value::Bool(canonical_schema_hash(path)?.is_some())),
    ]
    .into_iter()
    .collect())
}

fn manifest_metadata(path: &Path) -> Result<toml::Value> {
    Ok(toml_value(path)?.get("metadata").cloned().unwrap_or_else(|| toml::Value::Table(Default::default())))
}

fn toml_value(path: &Path) -> Result<toml::Value> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| CompileError::without_span(format!("failed to read TOML '{}': {}", path.display(), error)))?;
    toml::from_str(&source)
        .map_err(|error| CompileError::without_span(format!("failed to parse TOML '{}': {}", path.display(), error)))
}

fn toml_to_json(value: &toml::Value) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn toml_str<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(toml::Value::as_str)
}

fn read_cell_sources(src_root: &Path) -> Result<String> {
    if !src_root.is_dir() {
        return Ok(String::new());
    }
    let mut paths = std::fs::read_dir(src_root)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cell") && path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut source = String::new();
    for path in paths {
        source.push_str(&std::fs::read_to_string(path)?);
        source.push('\n');
    }
    Ok(source)
}

fn json_load(repo_root: &Path, rel_path: &str) -> Result<Value> {
    json_load_path(repo_root, &repo_root.join(rel_path))
}

fn json_load_path(repo_root: &Path, path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({"missing": true, "path": rel(repo_root, path)}));
    }
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| CompileError::without_span(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

fn json_load_path_optional(path: &Path) -> Result<Option<Value>> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(Some(value)),
        Ok(_) => Ok(Some(json!({"_invalid_json": "top-level value is not an object"}))),
        Err(error) => Ok(Some(json!({"_invalid_json": error.to_string()}))),
    }
}

fn write_json_report(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| CompileError::without_span(format!("failed to serialize JSON report '{}': {}", path.display(), error)))?;
    std::fs::write(path, json + "\n")?;
    Ok(())
}

fn gate(name: &str, status: &str, evidence: &str, detail: Value) -> Value {
    json!({"name": name, "status": status, "evidence": evidence, "detail": detail})
}

fn wallet_gate_passed(wallet: &Value) -> bool {
    json_pointer_str(wallet, "/status") == Some("passed")
        && json_pointer_i64(wallet, "/summary/core_vectors").unwrap_or_default() >= 6
        && json_pointer_i64(wallet, "/summary/agreement_vectors").unwrap_or_default() >= 3
        && json_pointer_i64(wallet, "/summary/matched") == json_pointer_i64(wallet, "/summary/total")
}

fn stateful_acceptance_passed(stateful_acceptance: &Value) -> bool {
    json_pointer_str(stateful_acceptance, "/status") == Some("passed")
        && json_pointer_i64(stateful_acceptance, "/blocker_count") == Some(0)
        && json_pointer_bool(stateful_acceptance, "/live_devnet_rpc_executed")
        && json_pointer_bool(stateful_acceptance, "/stateful_lifecycle_executed")
}

fn object_values_all_true(value: Option<&Value>) -> bool {
    value.and_then(Value::as_object).is_some_and(|object| object.values().all(|value| value.as_bool() == Some(true)))
}

fn value_is_present(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(_) => true,
    }
}

fn json_pointer_str<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn json_pointer_i64(value: &Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(Value::as_i64)
}

fn json_pointer_bool(value: &Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(Value::as_bool).unwrap_or(false)
}

fn json_pointer_bool_opt(value: &Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer).and_then(Value::as_bool)
}

fn json_array_strings(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn parse_out_point(value: Option<&str>) -> Value {
    let Some(raw) = value else {
        return json!({"valid": false, "raw": Value::Null});
    };
    let Some((tx_hash, index)) = raw.split_once(':') else {
        return json!({"valid": false, "raw": raw});
    };
    json!({
        "valid": is_hex32(tx_hash) && index.parse::<u64>().is_ok(),
        "tx_hash": tx_hash.to_ascii_lowercase(),
        "index": index.parse::<u64>().ok(),
    })
}

fn normalize_hex(value: Option<&str>) -> Option<String> {
    value.map(|raw| {
        let lower = raw.to_ascii_lowercase();
        if lower.starts_with("0x") {
            lower
        } else {
            format!("0x{lower}")
        }
    })
}

fn placeholder_hash(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return true;
    };
    if !is_hex32(value) {
        return true;
    }
    value[2..].bytes().all(|byte| byte == b'0')
}

fn is_hex32(value: &str) -> bool {
    value.len() == 66 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_hex_bytes(value: &str) -> bool {
    value.len() > 2 && value.len() % 2 == 0 && value.starts_with("0x") && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("0x{}", hex::encode(Sha256::digest(bytes))))
}

fn rel(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_schema_normalisation_hashes_comment_free_lines() {
        let temp = tempfile::tempdir().unwrap();
        let schema = temp.path().join("schema");
        std::fs::write(&schema, "# ignored\nNovaSealCanonicalEnvelopeV0:\nprofile_id:   Byte32 # comment\n\npolicy_hash: Byte32\n")
            .unwrap();

        let lines = canonical_schema_lines(&schema).unwrap();

        assert_eq!(
            lines,
            vec!["NovaSealCanonicalEnvelopeV0:".to_string(), "profile_id: Byte32".to_string(), "policy_hash: Byte32".to_string()]
        );
        assert_eq!(
            canonical_schema_hash(&schema).unwrap().unwrap(),
            "0x6b4277f67ee3e47f391d8591f7efccc6e97dcac5436dd22568d72689ac4db130"
        );
    }

    #[test]
    fn out_point_parser_rejects_placeholder_shapes() {
        let parsed = parse_out_point(Some("0x0000000000000000000000000000000000000000000000000000000000000000:0"));

        assert!(json_pointer_bool(&parsed, "/valid"));
        assert!(placeholder_hash(json_pointer_str(&parsed, "/tx_hash")));
    }
}

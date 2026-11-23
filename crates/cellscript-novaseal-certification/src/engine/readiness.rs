use super::{
    json_pointer_bool, json_pointer_str, stateful_local_acceptance_passed, EXTERNAL_TCB_ATTESTATION, PUBLIC_BTC_SPV_EVIDENCE,
    PUBLIC_CELLDEP_ATTESTATION, RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
};
use serde_json::{json, Value};

pub(super) fn build_v1_readiness(
    profile_certification: &Value,
    stateful_acceptance: &Value,
    gates: &[Value],
    local_ready: bool,
    production_gates_passed: bool,
) -> Value {
    let gate_status = |name: &str| {
        gates
            .iter()
            .find(|gate| json_pointer_str(gate, "/name") == Some(name))
            .and_then(|gate| json_pointer_str(gate, "/status"))
            .unwrap_or("missing")
    };
    let planned_matrix = build_planned_profile_matrix(profile_certification, stateful_acceptance);
    let planned_matrix_passed = json_pointer_str(&planned_matrix, "/status") == Some("passed");
    let dimensions = vec![
        readiness_dimension(
            "architecture_and_profile_conformance",
            json_pointer_str(profile_certification, "/status") == Some("passed")
                && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed"),
            "profile_certification.status + local_checks.conformance_gate_passed",
            "V1 architecture profile eligibility",
        ),
        readiness_dimension(
            "planned_profiles_and_business_scenarios",
            planned_matrix_passed,
            "v1_readiness.planned_profile_matrix",
            "all planned NovaSeal profiles and business scenarios",
        ),
        readiness_dimension(
            "security_audit_coverage",
            json_pointer_str(profile_certification, "/security_audit_coverage/status") == Some("passed"),
            "profile_certification.security_audit_coverage",
            "complete security-audit consideration",
        ),
        readiness_dimension(
            "devnet_multi_profile_coverage",
            json_pointer_str(stateful_acceptance, "/profile_coverage/status") == Some("passed"),
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage",
            "devnet multi-profile evidence",
        ),
        readiness_dimension(
            "multi_business_scenario_coverage",
            json_pointer_str(stateful_acceptance, "/business_scenario_coverage/status") == Some("passed"),
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
            "multi-business scenario evidence",
        ),
        readiness_dimension(
            "full_stateful_acceptance",
            stateful_local_acceptance_passed(stateful_acceptance),
            "target/novaseal-devnet-stateful-acceptance.json",
            "complete local stateful acceptance",
        ),
        readiness_dimension(
            "wallet_signing_vectors",
            json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed"),
            "target/novaseal-wallet-signing-vectors.json",
            "wallet-facing signing vector safety",
        ),
        readiness_dimension(
            "wallet_lock_digest_alignment",
            json_pointer_bool(profile_certification, "/local_checks/wallet_lock_alignment_passed"),
            "proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json",
            "wallet, lock, and verifier message32 alignment",
        ),
        readiness_dimension(
            "profile_operator_fixtures",
            json_pointer_bool(profile_certification, "/local_checks/profile_operator_fixture_detail_passed"),
            "target/novaseal-profile-operator-fixtures.json",
            "planned-profile wallet and service reproducibility",
        ),
        readiness_dimension(
            "service_builder_fixtures",
            json_pointer_bool(profile_certification, "/local_checks/service_builder_fixture_detail_passed"),
            "target/novaseal-service-builder-fixtures.json",
            "planned-profile service request and response reproducibility",
        ),
        readiness_dimension(
            "btc_spv_evidence_adapter",
            json_pointer_bool(profile_certification, "/local_checks/btc_spv_evidence_adapter_passed"),
            "target/novaseal-btc-spv-evidence-adapter.json",
            "public BTC SPV evidence request readiness",
        ),
        readiness_dimension(
            "external_attestation_adapter",
            json_pointer_bool(profile_certification, "/local_checks/external_attestation_adapter_passed"),
            "target/novaseal-external-attestation-adapter.json",
            "public CellDep and external TCB attestation request readiness",
        ),
        readiness_dimension(
            "external_evidence_handoff",
            json_pointer_bool(profile_certification, "/local_checks/external_evidence_handoff_passed"),
            "target/novaseal-external-evidence-handoff-bundle.json",
            "external production evidence provider handoff",
        ),
        readiness_dimension(
            "local_bip340_tcb_review",
            json_pointer_bool(profile_certification, "/local_checks/local_bip340_tcb_review_passed"),
            "target/novaseal-bip340-tcb-review.json",
            "local verifier TCB review",
        ),
        readiness_dimension(
            "local_v1_gate",
            local_ready,
            "all non-external novaseal-production-gates rows",
            "local V1 release readiness",
        ),
        readiness_dimension(
            "external_btc_fiber_endpoint_acceptance",
            gate_status("external_btc_fiber_endpoint_acceptance") == "passed",
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
            "complete BTC SPV and Fiber external endpoint acceptance",
        ),
        readiness_dimension(
            "all_profiles_production_completeness",
            gate_status("all_profiles_production_completeness") == "passed",
            "target/novaseal-production-gates.json#/profile_production_completeness",
            "every NovaSeal profile has local lifecycle evidence plus required external production evidence",
        ),
        readiness_dimension(
            "public_shared_cell_dep_attestation",
            gate_status("public_shared_cell_dep_pinning_attestation") == "passed",
            PUBLIC_CELLDEP_ATTESTATION,
            "public production deployment",
        ),
        readiness_dimension(
            "external_bip340_tcb_review_attestation",
            gate_status("external_bip340_runtime_verifier_tcb_review_attestation") == "passed",
            EXTERNAL_TCB_ATTESTATION,
            "external production TCB sign-off",
        ),
        readiness_dimension(
            "public_btc_spv_evidence",
            gate_status("public_btc_spv_evidence") == "passed",
            PUBLIC_BTC_SPV_EVIDENCE,
            "public BTC inclusion and confirmation proof provenance",
        ),
        readiness_dimension(
            "rwa_legal_registry_review_evidence",
            gate_status("rwa_legal_registry_review_evidence") == "passed",
            RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
            "external legal and registry review for RWA production claims",
        ),
    ];
    let local_dimension_names = [
        "architecture_and_profile_conformance",
        "planned_profiles_and_business_scenarios",
        "security_audit_coverage",
        "devnet_multi_profile_coverage",
        "multi_business_scenario_coverage",
        "full_stateful_acceptance",
        "wallet_signing_vectors",
        "wallet_lock_digest_alignment",
        "profile_operator_fixtures",
        "service_builder_fixtures",
        "btc_spv_evidence_adapter",
        "external_attestation_adapter",
        "external_evidence_handoff",
        "local_bip340_tcb_review",
        "local_v1_gate",
    ];
    let local_dimensions_passed = dimensions
        .iter()
        .filter(|dimension| json_pointer_str(dimension, "/name").is_some_and(|name| local_dimension_names.contains(&name)))
        .all(|dimension| json_pointer_str(dimension, "/status") == Some("passed"));
    let failed_dimensions = dimensions
        .iter()
        .filter(|dimension| json_pointer_str(dimension, "/status") != Some("passed"))
        .filter_map(|dimension| json_pointer_str(dimension, "/name").map(str::to_string))
        .collect::<Vec<_>>();
    let external_blockers =
        profile_certification.get("production_statement_blockers").cloned().unwrap_or_else(|| Value::Array(Vec::new()));
    let production_statement_eligible = json_pointer_bool(profile_certification, "/production_statement_eligible");
    let production_ready = production_gates_passed && production_statement_eligible;
    let status = if production_ready {
        "v1_prod_ready"
    } else if production_gates_passed {
        "production_statement_ineligible"
    } else if local_dimensions_passed {
        "local_v1_ready_external_attestation_required"
    } else if !planned_matrix_passed {
        "planned_profiles_incomplete"
    } else {
        "failed"
    };

    json!({
        "schema": "novaseal-v1-readiness-v0.1",
        "status": status,
        "local_v1_ready": local_dimensions_passed,
        "production_ready": production_ready,
        "production_gates_passed": production_gates_passed,
        "production_statement_eligible": production_statement_eligible,
        "planned_profile_matrix": planned_matrix,
        "dimensions": dimensions,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "acceptance_boundary": {
            "local_ready_means": "architecture, audit, wallet signing vectors, wallet/lock digest alignment, planned-profile operator fixtures, service-builder fixtures, BTC SPV evidence adapter request, external attestation adapter request, external evidence handoff bundle, TCB, multi-profile devnet, multi-business scenarios, and full stateful acceptance are machine checked locally",
            "production_ready_requires": [
                "all NovaSeal profiles pass the profile production-completeness matrix",
                "complete external BTC SPV and Fiber endpoint acceptance",
                "public/shared CellDep pinning attestation",
                "public BTC SPV evidence for BTC-facing profiles",
                "RWA legal/registry review evidence for RWA receipt production claims",
                "external BIP340 runtime verifier TCB review attestation",
                "cellc certify --plugin novaseal-profile-v0 --require-production passes",
            ],
        },
    })
}

pub(super) fn production_gate_status(
    production_ready: bool,
    production_gates_passed: bool,
    local_ready: bool,
    external_required: bool,
) -> &'static str {
    if production_ready {
        "production_ready"
    } else if production_gates_passed {
        "production_statement_ineligible"
    } else if local_ready && external_required {
        "local_production_prep_ready_external_attestation_required"
    } else {
        "failed"
    }
}

pub(super) fn build_planned_profile_matrix(profile_certification: &Value, stateful_acceptance: &Value) -> Value {
    let core_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/0/status") == Some("passed");
    let agreement_passed = json_pointer_str(stateful_acceptance, "/profile_coverage/covered_profiles/1/status") == Some("passed")
        && json_pointer_bool(profile_certification, "/local_checks/conformance_gate_passed");
    let key_signature_passed = json_pointer_bool(profile_certification, "/local_checks/local_bip340_tcb_review_passed")
        && json_pointer_bool(profile_certification, "/local_checks/wallet_vector_detail_passed")
        && json_pointer_bool(profile_certification, "/local_checks/wallet_lock_alignment_passed");
    let btc_tx_commitment_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/btc_tx_commitment/status") == Some("passed");
    let btc_utxo_seal_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/btc_utxo_seal/status") == Some("passed");
    let dual_seal_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/dual_seal/status") == Some("passed");
    let fiber_candidate_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/fiber_candidate/status") == Some("passed");
    let fungible_xudt_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/fungible_xudt/status") == Some("passed");
    let rwa_receipt_package_passed =
        json_pointer_str(profile_certification, "/planned_profile_packages/rwa_receipt/status") == Some("passed");
    let agreement_business_passed = [
        "agreement_originate_live",
        "agreement_repay_live",
        "agreement_claim_live",
        "agreement_negative_business_cases_preserve_live_state",
    ]
    .iter()
    .all(|key| json_pointer_bool(stateful_acceptance, &format!("/business_scenario_coverage/checks/{key}")));
    let btc_tx_commitment_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_transaction_commitment_transition_live");
    let btc_utxo_seal_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/btc_utxo_seal_closure_live");
    let dual_seal_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/dual_seal_finality_live");
    let fungible_xudt_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fungible_xudt_value_flow_live");
    let rwa_receipt_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/rwa_receipt_lifecycle_live");
    let fiber_candidate_business_passed =
        json_pointer_bool(stateful_acceptance, "/business_scenario_coverage/checks/fiber_candidate_path_live");
    let profiles = vec![
        planned_row(
            "seal_profile_btc_key_signature",
            "Seal profile",
            "BTC key signature authority over a typed CKB transition",
            key_signature_passed,
            "target/novaseal-bip340-tcb-review.json + target/novaseal-wallet-signing-vectors.json + proposals/novaseal/v0-mvp-skeleton/target/novaseal-wallet-signing-alignment.json",
        ),
        planned_row(
            "seal_profile_btc_transaction_commitment",
            "Seal profile",
            "BTC transaction commitment to a transition",
            btc_tx_commitment_package_passed,
            "proposals/novaseal/btc-transaction-commitment-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit public-BTC proof gap",
        ),
        planned_row(
            "seal_profile_btc_utxo_seal",
            "Seal profile",
            "proved BTC UTXO spend as a single-use seal",
            btc_utxo_seal_package_passed,
            "proposals/novaseal/btc-utxo-seal-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit public-BTC spend proof gap",
        ),
        planned_row(
            "seal_profile_dual_seal",
            "Seal profile",
            "combined BTC UTXO closure and CKB transition maturity",
            dual_seal_package_passed && dual_seal_business_passed,
            "proposals/novaseal/dual-seal-profile-v0 package plus target/novaseal-dual-seal-devnet-stateful-live.json and explicit public BTC closure evidence gap",
        ),
        planned_row(
            "object_profile_key_signed_cell_movement",
            "Object profile",
            "key-signed Cell movement under NovaSealCanonicalV0",
            core_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage",
        ),
        planned_row(
            "object_profile_agreement",
            "Object profile",
            "CKB-native Agreement profile with deterministic terminal paths",
            agreement_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/profile_coverage + profile certification",
        ),
        planned_row(
            "object_profile_fungible_xudt",
            "Object profile",
            "Fungible/xUDT balance-bearing NovaSeal profile",
            fungible_xudt_package_passed,
            "proposals/novaseal/fungible-xudt-profile-v0 package, schemas, fixtures, docs, source actions, and invariant matrix",
        ),
        planned_row(
            "object_profile_rwa_receipt",
            "Object profile",
            "RWA/receipt object profile with materialised receipt lifecycle",
            rwa_receipt_package_passed,
            "proposals/novaseal/rwa-receipt-profile-v0 package, schemas, fixtures, docs, source actions, and invariant matrix",
        ),
        planned_row(
            "future_fiber_test_path",
            "Application profile",
            "Fiber-facing candidate test path",
            fiber_candidate_package_passed,
            "proposals/novaseal/fiber-candidate-profile-v0 package, schemas, fixtures, docs, source action, invariant matrix, and explicit live Fiber evidence gap",
        ),
    ];
    let business_scenarios = vec![
        planned_row(
            "core_bootstrap_to_key_authorised_transition",
            "Business scenario",
            "Core bootstrap followed by key-authorised state transition",
            core_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
        ),
        planned_row(
            "agreement_originate_repay_claim",
            "Business scenario",
            "Agreement originate, repay-before-expiry, claim-after-expiry, payout, receipt, and negative paths",
            agreement_passed && agreement_business_passed,
            "target/novaseal-devnet-stateful-acceptance.json#/business_scenario_coverage",
        ),
        planned_row(
            "btc_transaction_commitment_transition",
            "Business scenario",
            "Transition authorised by a public BTC transaction commitment",
            btc_tx_commitment_package_passed && btc_tx_commitment_business_passed,
            "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json",
        ),
        planned_row(
            "btc_utxo_seal_closure",
            "Business scenario",
            "Single-use BTC UTXO seal closure over a CKB transition",
            btc_utxo_seal_package_passed && btc_utxo_seal_business_passed,
            "target/novaseal-btc-utxo-seal-devnet-stateful-live.json",
        ),
        planned_row(
            "dual_seal_finality",
            "Business scenario",
            "Dual-seal terminal finalisation after CKB maturity with declared BTC closure commitment",
            dual_seal_package_passed && dual_seal_business_passed,
            "target/novaseal-dual-seal-devnet-stateful-live.json",
        ),
        planned_row(
            "fungible_xudt_value_flow",
            "Business scenario",
            "Fungible/xUDT issue, transfer, settlement, and negative accounting paths",
            fungible_xudt_package_passed && fungible_xudt_business_passed,
            "target/novaseal-fungible-xudt-devnet-stateful-live.json",
        ),
        planned_row(
            "rwa_receipt_lifecycle",
            "Business scenario",
            "RWA/receipt materialisation, claim, settlement, and negative paths",
            rwa_receipt_package_passed && rwa_receipt_business_passed,
            "target/novaseal-rwa-receipt-devnet-stateful-live.json",
        ),
        planned_row(
            "fiber_candidate_path",
            "Business scenario",
            "Fiber-compatible candidate settlement path",
            fiber_candidate_package_passed && fiber_candidate_business_passed,
            "target/novaseal-fiber-candidate-devnet-stateful-live.json",
        ),
    ];
    let missing_profiles = profiles
        .iter()
        .chain(business_scenarios.iter())
        .filter(|row| json_pointer_str(row, "/status") != Some("passed"))
        .filter_map(|row| json_pointer_str(row, "/id").map(str::to_string))
        .collect::<Vec<_>>();
    let passed = missing_profiles.is_empty();
    let remaining_items = missing_profiles.clone();
    let not_implemented_yet = if passed {
        "none; all planned NovaSeal V1 profiles and business scenarios have local package, fixture, and stateful evidence".to_string()
    } else {
        format!("remaining local evidence rows: {}", missing_profiles.join(", "))
    };
    json!({
        "schema": "novaseal-planned-profile-matrix-v0.1",
        "status": if passed { "passed" } else { "incomplete" },
        "source": "proposals/novaseal/v0-mvp-skeleton/NOVASEAL_ARCHITECTURE_EXPLAINED.md",
        "profiles": profiles,
        "business_scenarios": business_scenarios,
        "missing": missing_profiles,
        "boundary": {
            "implemented_now": if passed {
                "BTC key-signature authority, all planned profile packages, key-signed Cell movement, CKB-native Agreement terminal paths, and local stateful live-report evidence for every planned V1 business scenario"
            } else {
                "BTC key-signature authority, implemented profile packages, key-signed Cell movement, CKB-native Agreement terminal paths, and any business scenario rows marked passed in this matrix"
            },
            "not_implemented_yet": not_implemented_yet,
            "remaining_items": remaining_items,
        },
    })
}

fn planned_row(id: &str, category: &str, description: &str, passed: bool, evidence: &str) -> Value {
    json!({
        "id": id,
        "category": category,
        "description": description,
        "status": if passed { "passed" } else { "missing" },
        "evidence": evidence,
    })
}

fn readiness_dimension(name: &str, passed: bool, evidence: &str, required_for: &str) -> Value {
    json!({
        "name": name,
        "status": if passed { "passed" } else { "failed" },
        "evidence": evidence,
        "required_for": required_for,
    })
}

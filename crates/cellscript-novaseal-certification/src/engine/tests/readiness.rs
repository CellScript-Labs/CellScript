#[test]
fn v1_readiness_requires_all_planned_profiles_before_external_only_status() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "production_statement_blockers": [
            "public_shared_cell_dep_attested",
            "external_bip340_tcb_review_attested",
        ],
        "local_checks": {
            "conformance_gate_passed": true,
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "security_audit_coverage": { "status": "passed" },
    });
    let stateful_acceptance = json!({
        "status": "passed",
        "blocker_count": 0,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": { "status": "passed" },
        "business_scenario_coverage": { "status": "passed" },
    });
    let local_gates = vec![
        gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
        gate("external_bip340_runtime_verifier_tcb_review_attestation", "external_required", EXTERNAL_TCB_ATTESTATION, Value::Null),
    ];

    let local = build_v1_readiness(&profile_certification, &stateful_acceptance, &local_gates, true, false);
    assert_eq!(json_pointer_str(&local, "/status"), Some("planned_profiles_incomplete"));
    assert!(!json_pointer_bool(&local, "/local_v1_ready"));
    assert!(!json_pointer_bool(&local, "/production_ready"));
    assert_eq!(json_pointer_str(&local, "/dimensions/1/status"), Some("failed"));
    assert_eq!(json_pointer_str(&local, "/planned_profile_matrix/status"), Some("incomplete"));
    let missing = json_array_strings(&local, "/planned_profile_matrix/missing");
    assert!(missing.iter().any(|id| id == "object_profile_fungible_xudt"));
    assert!(missing.iter().any(|id| id == "seal_profile_btc_utxo_seal"));
}

#[test]
fn production_status_requires_statement_eligibility_even_when_gates_pass() {
    assert_eq!(production_gate_status(true, true, true, false), "production_ready");
    assert_eq!(production_gate_status(false, true, true, false), "production_statement_ineligible");
    assert_eq!(production_gate_status(false, false, true, true), "local_production_prep_ready_external_attestation_required");
    assert_eq!(production_gate_status(false, false, false, true), "failed");
}

#[test]
fn v1_readiness_rejects_production_claim_when_statement_ineligible() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "production_statement_blockers": ["manual_production_statement_missing"],
        "local_checks": {
            "conformance_gate_passed": true,
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": true,
            "profile_operator_fixture_detail_passed": true,
            "service_builder_fixture_detail_passed": true,
            "btc_spv_evidence_adapter_passed": true,
            "external_attestation_adapter_passed": true,
            "external_evidence_handoff_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "security_audit_coverage": { "status": "passed" },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "status": "passed",
        "blocker_count": 0,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": {
            "status": "passed",
            "covered_profiles": [
                { "status": "passed" },
                { "status": "passed" }
            ]
        },
        "business_scenario_coverage": {
            "status": "passed",
            "checks": {
                "agreement_originate_live": true,
                "agreement_repay_live": true,
                "agreement_claim_live": true,
                "agreement_negative_business_cases_preserve_live_state": true,
                "btc_transaction_commitment_transition_live": true,
                "btc_utxo_seal_closure_live": true,
                "dual_seal_finality_live": true,
                "fungible_xudt_value_flow_live": true,
                "rwa_receipt_lifecycle_live": true,
                "fiber_candidate_path_live": true
            }
        },
        "external_endpoint_coverage": {
            "status": "passed",
            "production_complete": true
        },
    });
    let gates = vec![
        gate(
            "external_btc_fiber_endpoint_acceptance",
            "passed",
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
            Value::Null,
        ),
        gate(
            "all_profiles_production_completeness",
            "passed",
            "target/novaseal-production-gates.json#/profile_production_completeness",
            Value::Null,
        ),
        gate("public_shared_cell_dep_pinning_attestation", "passed", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
        gate("external_bip340_runtime_verifier_tcb_review_attestation", "passed", EXTERNAL_TCB_ATTESTATION, Value::Null),
        gate("public_btc_spv_evidence", "passed", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
        gate("rwa_legal_registry_review_evidence", "passed", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
    ];

    let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, true, true);

    assert_eq!(json_pointer_str(&readiness, "/status"), Some("production_statement_ineligible"));
    assert!(json_pointer_bool(&readiness, "/local_v1_ready"));
    assert!(!json_pointer_bool(&readiness, "/production_ready"));
    assert!(json_pointer_bool(&readiness, "/production_gates_passed"));
    assert!(!json_pointer_bool(&readiness, "/production_statement_eligible"));
    assert!(json_array_strings(&readiness, "/failed_dimensions").is_empty());
    assert_eq!(json_array_strings(&readiness, "/external_blockers"), vec!["manual_production_statement_missing".to_string()]);
}

#[test]
fn v1_readiness_allows_local_ready_when_only_external_endpoint_evidence_remains() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "production_statement_blockers": [
            "public_shared_cell_dep_attested",
            "external_bip340_tcb_review_attested",
            "public_btc_spv_evidence_attested",
            "rwa_legal_registry_review_attested"
        ],
        "local_checks": {
            "conformance_gate_passed": true,
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": true,
            "profile_operator_fixture_detail_passed": true,
            "service_builder_fixture_detail_passed": true,
            "btc_spv_evidence_adapter_passed": true,
            "external_attestation_adapter_passed": true,
            "external_evidence_handoff_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "security_audit_coverage": { "status": "passed" },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "status": "local_devnet_passed_external_endpoint_required",
        "blocker_count": 1,
        "local_blocker_count": 0,
        "acceptance_blocker_count": 1,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": {
            "status": "passed",
            "covered_profiles": [
                { "status": "passed" },
                { "status": "passed" }
            ]
        },
        "business_scenario_coverage": {
            "status": "passed",
            "checks": {
                "agreement_originate_live": true,
                "agreement_repay_live": true,
                "agreement_claim_live": true,
                "agreement_negative_business_cases_preserve_live_state": true,
                "btc_transaction_commitment_transition_live": true,
                "btc_utxo_seal_closure_live": true,
                "dual_seal_finality_live": true,
                "fungible_xudt_value_flow_live": true,
                "rwa_receipt_lifecycle_live": true,
                "fiber_candidate_path_live": true
            }
        },
        "external_endpoint_coverage": {
            "status": "external_required",
            "production_complete": false
        },
    });
    let gates = vec![
        gate(
            "external_btc_fiber_endpoint_acceptance",
            "external_required",
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
            Value::Null,
        ),
        gate(
            "all_profiles_production_completeness",
            "external_required",
            "target/novaseal-production-gates.json#/profile_production_completeness",
            Value::Null,
        ),
        gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
        gate("external_bip340_runtime_verifier_tcb_review_attestation", "external_required", EXTERNAL_TCB_ATTESTATION, Value::Null),
        gate("public_btc_spv_evidence", "external_required", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
        gate("rwa_legal_registry_review_evidence", "external_required", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
    ];

    let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, true, false);

    assert_eq!(json_pointer_str(&readiness, "/status"), Some("local_v1_ready_external_attestation_required"));
    assert!(json_pointer_bool(&readiness, "/local_v1_ready"));
    assert!(!json_pointer_bool(&readiness, "/production_ready"));
    assert_eq!(json_pointer_str(&readiness, "/planned_profile_matrix/status"), Some("passed"));
    assert!(json_array_strings(&readiness, "/planned_profile_matrix/missing").is_empty());
    assert_eq!(
        json_array_strings(&readiness, "/external_blockers"),
        vec![
            "public_shared_cell_dep_attested".to_string(),
            "external_bip340_tcb_review_attested".to_string(),
            "public_btc_spv_evidence_attested".to_string(),
            "rwa_legal_registry_review_attested".to_string(),
        ]
    );
}

#[test]
fn v1_readiness_requires_wallet_lock_digest_alignment_for_local_ready() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "production_statement_blockers": [
            "public_shared_cell_dep_attested",
            "external_bip340_tcb_review_attested",
            "public_btc_spv_evidence_attested",
            "rwa_legal_registry_review_attested"
        ],
        "local_checks": {
            "conformance_gate_passed": true,
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": false,
            "profile_operator_fixture_detail_passed": true,
            "service_builder_fixture_detail_passed": true,
            "btc_spv_evidence_adapter_passed": true,
            "external_attestation_adapter_passed": true,
            "external_evidence_handoff_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "security_audit_coverage": { "status": "passed" },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "status": "local_devnet_passed_external_endpoint_required",
        "blocker_count": 1,
        "local_blocker_count": 0,
        "acceptance_blocker_count": 1,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": {
            "status": "passed",
            "covered_profiles": [
                { "status": "passed" },
                { "status": "passed" }
            ]
        },
        "business_scenario_coverage": {
            "status": "passed",
            "checks": {
                "agreement_originate_live": true,
                "agreement_repay_live": true,
                "agreement_claim_live": true,
                "agreement_negative_business_cases_preserve_live_state": true,
                "btc_transaction_commitment_transition_live": true,
                "btc_utxo_seal_closure_live": true,
                "dual_seal_finality_live": true,
                "fungible_xudt_value_flow_live": true,
                "rwa_receipt_lifecycle_live": true,
                "fiber_candidate_path_live": true
            }
        },
        "external_endpoint_coverage": {
            "status": "external_required",
            "production_complete": false
        },
    });
    let gates = vec![
        gate(
            "external_btc_fiber_endpoint_acceptance",
            "external_required",
            "target/novaseal-devnet-stateful-acceptance.json#/external_endpoint_coverage",
            Value::Null,
        ),
        gate(
            "all_profiles_production_completeness",
            "external_required",
            "target/novaseal-production-gates.json#/profile_production_completeness",
            Value::Null,
        ),
        gate("public_shared_cell_dep_pinning_attestation", "external_required", PUBLIC_CELLDEP_ATTESTATION, Value::Null),
        gate("external_bip340_runtime_verifier_tcb_review_attestation", "external_required", EXTERNAL_TCB_ATTESTATION, Value::Null),
        gate("public_btc_spv_evidence", "external_required", PUBLIC_BTC_SPV_EVIDENCE, Value::Null),
        gate("rwa_legal_registry_review_evidence", "external_required", RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, Value::Null),
    ];

    let readiness = build_v1_readiness(&profile_certification, &stateful_acceptance, &gates, false, false);

    assert_eq!(json_pointer_str(&readiness, "/status"), Some("planned_profiles_incomplete"));
    assert!(!json_pointer_bool(&readiness, "/local_v1_ready"));
    assert!(json_array_strings(&readiness, "/failed_dimensions").iter().any(|dimension| dimension == "wallet_lock_digest_alignment"));
    assert!(json_array_strings(&readiness, "/planned_profile_matrix/missing")
        .iter()
        .any(|profile| profile == "seal_profile_btc_key_signature"));
}

#[test]
fn planned_matrix_counts_fungible_package_but_keeps_value_flow_missing() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "local_checks": {
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "profile_coverage": {
            "covered_profiles": [
                { "status": "passed" },
                { "status": "passed" }
            ]
        },
        "business_scenario_coverage": {
            "status": "failed",
            "checks": {
                "agreement_originate_live": true,
                "agreement_repay_live": true,
                "agreement_claim_live": true,
                "agreement_negative_business_cases_preserve_live_state": true,
                "btc_transaction_commitment_transition_live": false,
                "btc_utxo_seal_closure_live": false,
                "dual_seal_finality_live": false,
                "fungible_xudt_value_flow_live": false,
                "rwa_receipt_lifecycle_live": false,
                "fiber_candidate_path_live": false
            }
        },
    });

    let matrix = build_planned_profile_matrix(&profile_certification, &stateful_acceptance);
    let fungible_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("object_profile_fungible_xudt")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let btc_tx_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| {
            profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_btc_transaction_commitment"))
        })
        .and_then(|row| json_pointer_str(row, "/status"));
    let btc_tx_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| {
            scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("btc_transaction_commitment_transition"))
        })
        .and_then(|row| json_pointer_str(row, "/status"));
    let btc_utxo_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_btc_utxo_seal")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let btc_utxo_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("btc_utxo_seal_closure")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let dual_seal_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("seal_profile_dual_seal")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let dual_seal_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("dual_seal_finality")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let fungible_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("fungible_xudt_value_flow")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let rwa_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("object_profile_rwa_receipt")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let rwa_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("rwa_receipt_lifecycle")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let fiber_profile_status = matrix
        .pointer("/profiles")
        .and_then(Value::as_array)
        .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/id") == Some("future_fiber_test_path")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let fiber_flow_status = matrix
        .pointer("/business_scenarios")
        .and_then(Value::as_array)
        .and_then(|scenarios| scenarios.iter().find(|row| json_pointer_str(row, "/id") == Some("fiber_candidate_path")))
        .and_then(|row| json_pointer_str(row, "/status"));
    let missing = json_array_strings(&matrix, "/missing");

    assert_eq!(json_pointer_str(&matrix, "/status"), Some("incomplete"));
    assert_eq!(btc_tx_profile_status, Some("passed"));
    assert_eq!(btc_tx_flow_status, Some("missing"));
    assert_eq!(btc_utxo_profile_status, Some("passed"));
    assert_eq!(btc_utxo_flow_status, Some("missing"));
    assert_eq!(dual_seal_profile_status, Some("missing"));
    assert_eq!(dual_seal_flow_status, Some("missing"));
    assert_eq!(fungible_profile_status, Some("passed"));
    assert_eq!(fungible_flow_status, Some("missing"));
    assert_eq!(rwa_profile_status, Some("passed"));
    assert_eq!(rwa_flow_status, Some("missing"));
    assert_eq!(fiber_profile_status, Some("passed"));
    assert_eq!(fiber_flow_status, Some("missing"));
    assert!(!missing.iter().any(|id| id == "seal_profile_btc_transaction_commitment"));
    assert!(!missing.iter().any(|id| id == "seal_profile_btc_utxo_seal"));
    assert!(missing.iter().any(|id| id == "seal_profile_dual_seal"));
    assert!(!missing.iter().any(|id| id == "object_profile_fungible_xudt"));
    assert!(!missing.iter().any(|id| id == "object_profile_rwa_receipt"));
    assert!(!missing.iter().any(|id| id == "future_fiber_test_path"));
    assert!(missing.iter().any(|id| id == "btc_transaction_commitment_transition"));
    assert!(missing.iter().any(|id| id == "btc_utxo_seal_closure"));
    assert!(missing.iter().any(|id| id == "dual_seal_finality"));
    assert!(missing.iter().any(|id| id == "fungible_xudt_value_flow"));
    assert!(missing.iter().any(|id| id == "rwa_receipt_lifecycle"));
    assert!(missing.iter().any(|id| id == "fiber_candidate_path"));
    assert_eq!(json_array_strings(&matrix, "/boundary/remaining_items"), missing);
    assert!(json_pointer_str(&matrix, "/boundary/not_implemented_yet")
        .is_some_and(|text| text.contains("btc_transaction_commitment_transition")));
}

#[test]
fn planned_matrix_boundary_has_no_stale_missing_text_when_all_rows_pass() {
    let profile_certification = json!({
        "status": "passed",
        "production_statement_eligible": false,
        "local_checks": {
            "conformance_gate_passed": true,
            "wallet_vector_detail_passed": true,
            "wallet_lock_alignment_passed": true,
            "local_bip340_tcb_review_passed": true,
        },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "profile_coverage": {
            "covered_profiles": [
                { "status": "passed" },
                { "status": "passed" }
            ]
        },
        "business_scenario_coverage": {
            "status": "passed",
            "checks": {
                "agreement_originate_live": true,
                "agreement_repay_live": true,
                "agreement_claim_live": true,
                "agreement_negative_business_cases_preserve_live_state": true,
                "btc_transaction_commitment_transition_live": true,
                "btc_utxo_seal_closure_live": true,
                "dual_seal_finality_live": true,
                "fungible_xudt_value_flow_live": true,
                "rwa_receipt_lifecycle_live": true,
                "fiber_candidate_path_live": true
            }
        },
    });

    let matrix = build_planned_profile_matrix(&profile_certification, &stateful_acceptance);
    let not_implemented_yet = json_pointer_str(&matrix, "/boundary/not_implemented_yet").unwrap_or_default();

    assert_eq!(json_pointer_str(&matrix, "/status"), Some("passed"));
    assert!(json_array_strings(&matrix, "/missing").is_empty());
    assert!(json_array_strings(&matrix, "/boundary/remaining_items").is_empty());
    assert!(not_implemented_yet.starts_with("none;"));
    assert!(!not_implemented_yet.contains("fresh live devnet reports proving"));
}

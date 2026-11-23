#[test]
fn btc_spv_adapter_requires_exact_public_field_contract() {
    let full_public_fields = EXPECTED_BTC_SPV_ADAPTER_PUBLIC_FIELDS;
    let report = json!({
        "schema": "novaseal-btc-spv-evidence-adapter-v0.1",
        "status": "passed",
        "adapter_status": "request_ready_external_evidence_required",
        "source_service_builder_report_hash": format!("0x{}", "aa".repeat(32)),
        "source_public_btc_spv_template_hash": format!("0x{}", "bb".repeat(32)),
        "production_output": PUBLIC_BTC_SPV_EVIDENCE,
        "summary": { "total": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len(), "matched": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.len() },
        "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().enumerate().map(|(index, profile)| {
            let profile = *profile;
            let btc_material = test_btc_profile_material(profile, index as u8 + 0x60);
            let mut request = json!({
                "profile": profile,
                "scenario": expected_btc_spv_scenario(profile).unwrap(),
                "minimum_confirmations": 6,
                "required_external_inputs": ["public_btc_spv_evidence"],
                "ckb_live_tx_hash": test_hex32(index as u8 + 0x70),
                "live_report_hash": test_hex32(index as u8 + 0x80),
                "service_builder_case_hash": format!("0x{}", "cc".repeat(32)),
                "service_builder_tx_skeleton_hash": format!("0x{}", "dd".repeat(32)),
                "service_builder_receipt_binding_hash": format!("0x{}", "ee".repeat(32)),
                "ckb_btc_commitment_hash": format!("0x{}", "ab".repeat(32)),
                "template_case_hash": format!("0x{}", "ff".repeat(32)),
                "required_public_fields": full_public_fields,
                "field_constraints": constraint_object(EXPECTED_BTC_SPV_FIELD_CONSTRAINTS),
            });
            add_expected_btc_request_fields(&mut request, &btc_material);
            json!({
                "profile": profile,
                "status": "passed",
                "checks": { "ok": true },
                "request": request,
            })
        }).collect::<Vec<_>>(),
    });

    let valid = validate_btc_spv_evidence_adapter_detail(&report);
    assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
    assert!(json_pointer_bool(&valid, "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"));
    assert!(json_pointer_bool(&valid, "/cases/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

    let mut wrong_anchor_source = report.clone();
    wrong_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("external_public_btc_spend");
    let failed_anchor_source = validate_btc_spv_evidence_adapter_detail(&wrong_anchor_source);
    assert_eq!(json_pointer_str(&failed_anchor_source, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_anchor_source,
        "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
    ));

    let mut fixture_local_anchor_source = report.clone();
    fixture_local_anchor_source["cases"][0]["request"]["local_anchor_source"] = json!("local_deterministic_fixture");
    fixture_local_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("external_public_btc_transaction");
    let valid_fixture_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&fixture_local_anchor_source);
    assert_eq!(json_pointer_str(&valid_fixture_local_anchor_source, "/status"), Some("passed"));
    assert!(json_pointer_bool(
        &valid_fixture_local_anchor_source,
        "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
    ));
    assert!(json_pointer_bool(
        &valid_fixture_local_anchor_source,
        "/cases/btc-transaction-commitment-profile-v0/local_anchor_source_present"
    ));

    let mut missing_local_anchor_source = report.clone();
    missing_local_anchor_source["cases"][0]["request"].as_object_mut().unwrap().remove("local_anchor_source");
    let failed_missing_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&missing_local_anchor_source);
    assert_eq!(json_pointer_str(&failed_missing_local_anchor_source, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_missing_local_anchor_source,
        "/cases/btc-transaction-commitment-profile-v0/local_anchor_source_present"
    ));

    let mut local_anchor_source = report.clone();
    local_anchor_source["cases"][0]["request"]["expected_anchor_source"] = json!("local_deterministic_fixture");
    local_anchor_source["cases"][0]["request"]["local_anchor_source"] = json!("local_deterministic_fixture");
    let failed_local_anchor_source = validate_btc_spv_evidence_adapter_detail(&local_anchor_source);
    assert_eq!(json_pointer_str(&failed_local_anchor_source, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_local_anchor_source,
        "/cases/btc-transaction-commitment-profile-v0/expected_anchor_source_production_eligible"
    ));

    let mut zero_btc_amount = report.clone();
    zero_btc_amount["cases"][0]["request"]["expected_btc_amount_sats"] = json!(0);
    let failed_zero_btc_amount = validate_btc_spv_evidence_adapter_detail(&zero_btc_amount);
    assert_eq!(json_pointer_str(&failed_zero_btc_amount, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_zero_btc_amount,
        "/cases/btc-transaction-commitment-profile-v0/expected_output_fields_present"
    ));

    let mut missing_constraint = report.clone();
    missing_constraint["cases"][0]["request"]["field_constraints"].as_object_mut().unwrap().remove("source_service.commit");
    let failed_constraint = validate_btc_spv_evidence_adapter_detail(&missing_constraint);
    assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_constraint, "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"));

    let mut missing_report_hash_constraint = report.clone();
    missing_report_hash_constraint["cases"][0]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("source_service.report_hash");
    let failed_report_hash_constraint = validate_btc_spv_evidence_adapter_detail(&missing_report_hash_constraint);
    assert_eq!(json_pointer_str(&failed_report_hash_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_report_hash_constraint,
        "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
    ));

    let mut missing_celldep_hash_type_constraint = report.clone();
    missing_celldep_hash_type_constraint["cases"][0]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("spv_client_cell_dep.hash_type");
    let failed_celldep_hash_type_constraint = validate_btc_spv_evidence_adapter_detail(&missing_celldep_hash_type_constraint);
    assert_eq!(json_pointer_str(&failed_celldep_hash_type_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_celldep_hash_type_constraint,
        "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
    ));

    let mut missing_spv_proof_hash_constraint = report.clone();
    missing_spv_proof_hash_constraint["cases"][0]["request"]["field_constraints"].as_object_mut().unwrap().remove("spv_proof_hash");
    let failed_spv_proof_hash_constraint = validate_btc_spv_evidence_adapter_detail(&missing_spv_proof_hash_constraint);
    assert_eq!(json_pointer_str(&failed_spv_proof_hash_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_spv_proof_hash_constraint,
        "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
    ));

    let mut missing_handoff_bundle_constraint = report.clone();
    missing_handoff_bundle_constraint["cases"][0]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("request_handoff.bundle");
    let failed_handoff_bundle_constraint = validate_btc_spv_evidence_adapter_detail(&missing_handoff_bundle_constraint);
    assert_eq!(json_pointer_str(&failed_handoff_bundle_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_handoff_bundle_constraint,
        "/cases/btc-transaction-commitment-profile-v0/field_constraints_exact"
    ));

    let mut stale_scenario = report.clone();
    stale_scenario["cases"][0]["request"]["scenario"] = json!("generic-public-btc-proof");
    let failed_scenario = validate_btc_spv_evidence_adapter_detail(&stale_scenario);
    assert_eq!(json_pointer_str(&failed_scenario, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_scenario, "/cases/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

    let mut source_bound = report.clone();
    let service_builder_cases = source_bound["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let request = &case["request"];
            json!({
                "profile": case["profile"].as_str().unwrap(),
                "response": {
                    "tx_skeleton_hash": request["service_builder_tx_skeleton_hash"].clone(),
                    "receipt_binding_hash": request["service_builder_receipt_binding_hash"].clone(),
                },
                "request": {
                    "required_live_inputs": {
                        "live_devnet_tx_hash": request["ckb_live_tx_hash"].clone(),
                        "live_report_hash": request["live_report_hash"].clone(),
                        "public_btc_anchor": {
                            "ckb_btc_commitment_hash": request["ckb_btc_commitment_hash"].clone(),
                        },
                    },
                },
            })
        })
        .collect::<Vec<_>>();
    let service_builder = json!({ "cases": service_builder_cases });
    for case in source_bound["cases"].as_array_mut().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let builder_case = service_builder["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|builder_case| json_pointer_str(builder_case, "/profile") == Some(profile))
            .unwrap();
        case["request"]["service_builder_case_hash"] =
            json!(novaseal_btc_spv_adapter_report_hash("service_builder_case", builder_case));
    }
    let public_template = json!({ "schema": "novaseal-public-btc-spv-evidence-template-test" });
    source_bound["source_service_builder_report_hash"] =
        json!(novaseal_btc_spv_adapter_report_hash("service_builder_report", &service_builder));
    source_bound["source_public_btc_spv_template_hash"] =
        json!(novaseal_btc_spv_adapter_report_hash("public_btc_spv_template", &public_template));
    let source_bound_valid =
        validate_btc_spv_evidence_adapter_detail_with_sources(&source_bound, Some(&service_builder), Some(&public_template));
    assert_eq!(json_pointer_str(&source_bound_valid, "/status"), Some("passed"));
    assert!(json_pointer_bool(&source_bound_valid, "/checks/service_builder_report_hash_matches_current_report"));
    assert!(json_pointer_bool(
        &source_bound_valid,
        "/cases/btc-transaction-commitment-profile-v0/service_builder_case_hash_matches_current_report"
    ));

    let mut stale_source_hash = source_bound.clone();
    stale_source_hash["source_service_builder_report_hash"] = json!(test_hex32(0xf3));
    let failed_stale_source_hash =
        validate_btc_spv_evidence_adapter_detail_with_sources(&stale_source_hash, Some(&service_builder), Some(&public_template));
    assert_eq!(json_pointer_str(&failed_stale_source_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_source_hash, "/checks/service_builder_report_hash_matches_current_report"));

    let mut stale_case_hash = source_bound.clone();
    stale_case_hash["cases"][0]["request"]["service_builder_case_hash"] = json!(test_hex32(0xf4));
    let failed_stale_case_hash =
        validate_btc_spv_evidence_adapter_detail_with_sources(&stale_case_hash, Some(&service_builder), Some(&public_template));
    assert_eq!(json_pointer_str(&failed_stale_case_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_case_hash,
        "/cases/btc-transaction-commitment-profile-v0/service_builder_case_hash_matches_current_report"
    ));

    let mut stale_live_hash = source_bound.clone();
    stale_live_hash["cases"][0]["request"]["live_report_hash"] = json!(test_hex32(0xf5));
    let failed_stale_live_hash =
        validate_btc_spv_evidence_adapter_detail_with_sources(&stale_live_hash, Some(&service_builder), Some(&public_template));
    assert_eq!(json_pointer_str(&failed_stale_live_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_live_hash,
        "/cases/btc-transaction-commitment-profile-v0/live_report_hash_matches_current_report"
    ));

    let mut unexpected_public_field = report;
    let mut extended_public_fields = full_public_fields.to_vec();
    extended_public_fields.push("unexpected.shadow_field");
    unexpected_public_field["cases"][0]["request"]["required_public_fields"] = json!(extended_public_fields);
    let failed_exact = validate_btc_spv_evidence_adapter_detail(&unexpected_public_field);
    assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_exact, "/cases/btc-transaction-commitment-profile-v0/required_public_fields_exact"));
}

#[test]
fn out_point_parser_rejects_placeholder_shapes() {
    let parsed = parse_out_point(Some("0x0000000000000000000000000000000000000000000000000000000000000000:0"));

    assert!(json_pointer_bool(&parsed, "/valid"));
    assert!(placeholder_hash(json_pointer_str(&parsed, "/tx_hash")));
}

#[test]
fn btc_spv_evidence_requires_public_complete_profile_cases() {
    let temp = tempfile::tempdir().unwrap();
    let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
    std::fs::create_dir_all(&proofs).unwrap();
    let expected_scenarios = EXPECTED_BTC_SPV_PROFILE_SCENARIOS
        .iter()
        .map(|(profile, scenario)| ((*profile).to_string(), json!(*scenario)))
        .collect::<Map<_, _>>();
    let expected_case_bindings = EXPECTED_BTC_SPV_EVIDENCE_PROFILES
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let byte = index as u8 + 0x77;
            let btc_material = test_btc_profile_material(profile, byte);
            let mut binding = json!({
                "ckb_live_tx_hash": test_hex32(byte),
                "live_report_hash": test_hex32(byte + 1),
                "service_builder_case_hash": test_hex32(byte + 2),
                "service_builder_tx_skeleton_hash": test_hex32(byte + 3),
                "service_builder_receipt_binding_hash": test_hex32(byte + 4),
                "ckb_btc_commitment_hash": test_hex32(byte + 5),
            });
            merge_expected_btc_binding_fields(&mut binding, &btc_material);
            ((*profile).to_string(), binding)
        })
        .collect::<Map<_, _>>();
    let handoff = json!({
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": "passed",
        "cases": [
            {
                "group": "public_btc_spv_evidence",
                "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
                "expected_scenarios": expected_scenarios,
                "expected_case_bindings": expected_case_bindings,
            },
        ],
    });
    let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);

    let missing = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&missing, "/status"), Some("external_required"));

    let case_for = |profile: &str| {
        let index = EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().position(|expected| *expected == profile).unwrap_or(9);
        let byte = index as u8 + 0x77;
        let btc_material = test_btc_profile_material(profile, byte);
        let spv_material = test_btc_spv_material(byte, 7, &btc_material);
        json!({
            "profile": profile,
            "scenario": expected_btc_spv_scenario(profile).unwrap_or("unexpected-profile-scenario"),
            "ckb_live_tx_hash": test_hex32(byte),
            "live_report_hash": test_hex32(byte + 1),
            "service_builder_case_hash": test_hex32(byte + 2),
            "service_builder_tx_skeleton_hash": test_hex32(byte + 3),
            "service_builder_receipt_binding_hash": test_hex32(byte + 4),
            "ckb_btc_commitment_hash": test_hex32(byte + 5),
            "btc_txid": spv_material["btc_txid"].clone(),
            "btc_wtxid": spv_material["btc_wtxid"].clone(),
            "btc_tx_hex": spv_material["btc_tx_hex"].clone(),
            "btc_transaction_binding": spv_material["btc_transaction_binding"].clone(),
            "btc_block_hash": spv_material["btc_block_hash"].clone(),
            "btc_block_header": spv_material["btc_block_header"].clone(),
            "btc_merkle_proof": spv_material["btc_merkle_proof"].clone(),
            "spv_proof_hash": spv_material["spv_proof_hash"].clone(),
            "minimum_confirmations": 6,
            "confirmations": 7,
            "spv_client_cell_dep": {
                "out_point": format!("0x{}:0", "44".repeat(32)),
                "dep_type": "code",
                "hash_type": "type",
                "data_hash": format!("0x{}", "55".repeat(32)),
            },
            "source_service": {
                "name": "rgbpp-style-spv-service",
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "report_hash": format!("0x{}", "66".repeat(32)),
            },
        })
    };
    let spv_report = json!({
        "schema": "novaseal-public-btc-spv-evidence-v0.1",
        "status": "attested",
        "network": "testnet",
        "evidence_provider": "external-spv-operator",
        "generated_at": "2026-06-04T00:00:00Z",
        "notes": "external public BTC SPV evidence fixture",
        "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
        "request_handoff": {
            "bundle": EXTERNAL_EVIDENCE_HANDOFF,
            "bundle_hash": handoff_hash,
            "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
            "group": "public_btc_spv_evidence",
        },
        "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES.iter().map(|profile| case_for(profile)).collect::<Vec<_>>(),
    });
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&spv_report).unwrap()).unwrap();

    let passed = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
    assert!(json_pointer_bool(&passed, "/checks/top_level_fields_exact"));
    assert!(json_pointer_bool(&passed, "/checks/request_handoff_fields_exact"));
    assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_algorithm"));
    assert!(json_pointer_bool(&passed, "/checks/handoff_required_profiles_exact"));
    assert!(json_pointer_bool(&passed, "/checks/handoff_expected_scenarios_exact"));
    assert!(json_pointer_bool(&passed, "/checks/handoff_expected_case_bindings_exact"));
    assert!(json_pointer_bool(&passed, "/checks/network_public"));
    assert!(json_pointer_bool(&passed, "/checks/evidence_provider_identity"));
    assert!(json_pointer_bool(&passed, "/checks/generated_at_utc_timestamp"));
    assert!(json_pointer_bool(&passed, "/checks/generated_at_not_future"));
    assert!(json_pointer_bool(&passed, "/checks/required_profiles_field_exact"));
    assert!(json_pointer_bool(&passed, "/checks/required_profiles_match_handoff"));
    assert!(json_pointer_bool(&passed, "/checks/required_profiles_covered_exact"));
    assert!(json_pointer_bool(&passed, "/checks/covered_scenarios_match_handoff"));
    assert!(json_pointer_bool(&passed, "/checks/case_checks_passed"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_expected"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/ckb_live_tx_hash_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/ckb_btc_commitment_hash_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_handoff_when_bound"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_wtxid_matches_handoff_when_bound"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_tx_hex"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_wtxid_matches_tx_hex"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_binding_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_output_matches_anchor"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_spend_input_matches_anchor"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_tx_matches_anchor"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_utxo_commitment_matches_tuple"));
    assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_spend_input_matches_anchor"));
    assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_sealed_tx_matches_anchor"));
    assert!(json_pointer_bool(&passed, "/case_checks/dual-seal-profile-v0/btc_dual_sealed_utxo_commitment_matches_tuple"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_block_hash_matches_header"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_root_matches_header"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/btc_confirmations_match_heights"));
    assert!(json_pointer_bool(&passed, "/case_checks/btc-transaction-commitment-profile-v0/spv_proof_hash_matches_material"));
    assert!(json_pointer_bool(
        &passed,
        "/case_checks/btc-transaction-commitment-profile-v0/service_builder_tx_skeleton_hash_matches_handoff"
    ));

    let single_tx_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x77);
    let single_tx_material = test_btc_single_tx_block_spv_material(0x77, 7, &single_tx_btc);
    let mut single_tx_block = spv_report.clone();
    single_tx_block["cases"][0]["btc_txid"] = single_tx_material["btc_txid"].clone();
    single_tx_block["cases"][0]["btc_wtxid"] = single_tx_material["btc_wtxid"].clone();
    single_tx_block["cases"][0]["btc_tx_hex"] = single_tx_material["btc_tx_hex"].clone();
    single_tx_block["cases"][0]["btc_transaction_binding"] = single_tx_material["btc_transaction_binding"].clone();
    single_tx_block["cases"][0]["btc_block_hash"] = single_tx_material["btc_block_hash"].clone();
    single_tx_block["cases"][0]["btc_block_header"] = single_tx_material["btc_block_header"].clone();
    single_tx_block["cases"][0]["btc_merkle_proof"] = single_tx_material["btc_merkle_proof"].clone();
    single_tx_block["cases"][0]["spv_proof_hash"] = single_tx_material["spv_proof_hash"].clone();
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&single_tx_block).unwrap()).unwrap();
    let passed_single_tx_block = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&passed_single_tx_block, "/status"), Some("passed"));
    assert!(json_pointer_bool(
        &passed_single_tx_block,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_proof_branch_valid"
    ));
    assert!(json_pointer_bool(
        &passed_single_tx_block,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"
    ));

    let mut single_tx_nonzero_index = single_tx_block.clone();
    single_tx_nonzero_index["cases"][0]["btc_merkle_proof"]["tx_index"] = json!(1);
    single_tx_nonzero_index["cases"][0]["spv_proof_hash"] =
        json!(btc_spv_proof_material_hash(&single_tx_nonzero_index["cases"][0]).unwrap());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&single_tx_nonzero_index).unwrap()).unwrap();
    let failed_single_tx_nonzero_index = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_single_tx_nonzero_index, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_single_tx_nonzero_index,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_proof_branch_valid"
    ));

    let mut stale_btc_commitment = spv_report.clone();
    stale_btc_commitment["cases"][0]["ckb_btc_commitment_hash"] = json!(test_hex32(0x19));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_btc_commitment).unwrap()).unwrap();
    let failed_stale_btc_commitment = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_btc_commitment, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_btc_commitment,
        "/case_checks/btc-transaction-commitment-profile-v0/ckb_btc_commitment_hash_matches_handoff"
    ));

    let mut stale_tx_hex = spv_report.clone();
    stale_tx_hex["cases"][0]["btc_tx_hex"] = json!("0x01000000000000000000");
    let stale_tx_hex_hash = btc_spv_proof_material_hash(&stale_tx_hex["cases"][0]).unwrap();
    stale_tx_hex["cases"][0]["spv_proof_hash"] = json!(stale_tx_hex_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_tx_hex).unwrap()).unwrap();
    let failed_stale_tx_hex = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_tx_hex, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_tx_hex, "/case_checks/btc-transaction-commitment-profile-v0/btc_tx_hex_valid"));

    let mut wrong_valid_tx_hex = spv_report.clone();
    let wrong_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x2a);
    wrong_valid_tx_hex["cases"][0]["btc_tx_hex"] = json!(wrong_btc.tx_hex);
    let wrong_valid_tx_hash = btc_spv_proof_material_hash(&wrong_valid_tx_hex["cases"][0]).unwrap();
    wrong_valid_tx_hex["cases"][0]["spv_proof_hash"] = json!(wrong_valid_tx_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&wrong_valid_tx_hex).unwrap()).unwrap();
    let failed_wrong_valid_tx_hex = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_wrong_valid_tx_hex, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_wrong_valid_tx_hex,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_tx_hex"
    ));

    let mut stale_output_handoff = handoff.clone();
    stale_output_handoff["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["btc_amount_sats"] = json!(1);
    let stale_output_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_output_handoff);
    let mut stale_output_amount = spv_report.clone();
    stale_output_amount["request_handoff"]["bundle_hash"] = json!(stale_output_handoff_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_output_amount).unwrap()).unwrap();
    let failed_stale_output_amount = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &stale_output_handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_output_amount, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_output_amount,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_binding_matches_handoff"
    ));
    assert!(!json_pointer_bool(
        &failed_stale_output_amount,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_transaction_output_matches_anchor"
    ));

    let mut stale_sealed_tx = spv_report.clone();
    let wrong_sealed_tx_hex = stale_sealed_tx["cases"][0]["btc_tx_hex"].clone();
    stale_sealed_tx["cases"][1]["btc_transaction_binding"]["sealed_btc_tx_hex"] = wrong_sealed_tx_hex;
    let stale_sealed_hash = btc_spv_proof_material_hash(&stale_sealed_tx["cases"][1]).unwrap();
    stale_sealed_tx["cases"][1]["spv_proof_hash"] = json!(stale_sealed_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_sealed_tx).unwrap()).unwrap();
    let failed_stale_sealed_tx = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_sealed_tx, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_sealed_tx, "/case_checks/btc-utxo-seal-profile-v0/btc_utxo_sealed_tx_matches_anchor"));

    let mut stale_bound_btc_txid = spv_report.clone();
    let replacement_btc = test_btc_profile_material(EXPECTED_BTC_TX_COMMITMENT_PROFILE, 0x21);
    let replacement_material = test_btc_spv_material(0x21, 7, &replacement_btc);
    stale_bound_btc_txid["cases"][0]["btc_txid"] = replacement_material["btc_txid"].clone();
    stale_bound_btc_txid["cases"][0]["btc_wtxid"] = replacement_material["btc_wtxid"].clone();
    stale_bound_btc_txid["cases"][0]["btc_tx_hex"] = replacement_material["btc_tx_hex"].clone();
    stale_bound_btc_txid["cases"][0]["btc_transaction_binding"] = replacement_material["btc_transaction_binding"].clone();
    stale_bound_btc_txid["cases"][0]["btc_block_hash"] = replacement_material["btc_block_hash"].clone();
    stale_bound_btc_txid["cases"][0]["btc_block_header"] = replacement_material["btc_block_header"].clone();
    stale_bound_btc_txid["cases"][0]["btc_merkle_proof"] = replacement_material["btc_merkle_proof"].clone();
    stale_bound_btc_txid["cases"][0]["spv_proof_hash"] = replacement_material["spv_proof_hash"].clone();
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_bound_btc_txid).unwrap()).unwrap();
    let failed_stale_bound_btc_txid = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_bound_btc_txid, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_bound_btc_txid,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_matches_handoff_when_bound"
    ));

    let mut stale_block_header = spv_report.clone();
    stale_block_header["cases"][0]["btc_block_header"] = json!(format!("0x{}", "01".repeat(80)));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_block_header).unwrap()).unwrap();
    let failed_stale_block_header = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_block_header, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_block_header,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_block_hash_matches_header"
    ));

    let mut stale_merkle_branch = spv_report.clone();
    stale_merkle_branch["cases"][0]["btc_merkle_proof"]["merkle_branch"][0] = json!(test_hex32(0x99));
    let stale_hash = btc_spv_proof_material_hash(&stale_merkle_branch["cases"][0]).unwrap();
    stale_merkle_branch["cases"][0]["spv_proof_hash"] = json!(stale_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_merkle_branch).unwrap()).unwrap();
    let failed_stale_merkle_branch = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_merkle_branch, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_merkle_branch,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_merkle_branch_verifies_txid"
    ));

    let mut stale_confirmation_height = spv_report.clone();
    stale_confirmation_height["cases"][0]["btc_merkle_proof"]["observed_tip_height"] = json!(900_001);
    let stale_height_hash = btc_spv_proof_material_hash(&stale_confirmation_height["cases"][0]).unwrap();
    stale_confirmation_height["cases"][0]["spv_proof_hash"] = json!(stale_height_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_confirmation_height).unwrap())
        .unwrap();
    let failed_stale_confirmation_height = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_confirmation_height, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_confirmation_height,
        "/case_checks/btc-transaction-commitment-profile-v0/btc_confirmations_match_heights"
    ));

    let mut stale_spv_proof_hash = spv_report.clone();
    stale_spv_proof_hash["cases"][0]["spv_proof_hash"] = json!(test_hex32(0x88));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_spv_proof_hash).unwrap()).unwrap();
    let failed_stale_spv_proof_hash = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_spv_proof_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_spv_proof_hash,
        "/case_checks/btc-transaction-commitment-profile-v0/spv_proof_hash_matches_material"
    ));

    let mut stale_live_binding = spv_report.clone();
    stale_live_binding["cases"][0]["ckb_live_tx_hash"] = json!(test_hex32(0x22));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_live_binding).unwrap()).unwrap();
    let failed_stale_live_binding = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_live_binding, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_live_binding,
        "/case_checks/btc-transaction-commitment-profile-v0/ckb_live_tx_hash_matches_handoff"
    ));

    let mut stale_handoff_scenario = handoff.clone();
    stale_handoff_scenario["cases"][0]["expected_scenarios"][EXPECTED_BTC_TX_COMMITMENT_PROFILE] = json!("generic-public-btc-proof");
    let stale_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_handoff_scenario);
    let mut spv_with_stale_handoff = spv_report.clone();
    spv_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_handoff_hash);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&spv_with_stale_handoff).unwrap()).unwrap();
    let failed_stale_handoff = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &stale_handoff_scenario).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_handoff, "/status"), Some("failed"));
    assert!(json_pointer_bool(&failed_stale_handoff, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(!json_pointer_bool(&failed_stale_handoff, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_handoff"));
    assert!(!json_pointer_bool(&failed_stale_handoff, "/checks/covered_scenarios_match_handoff"));

    let mut placeholder_generated_at = spv_report.clone();
    placeholder_generated_at["generated_at"] = json!("YYYY-MM-DDTHH:MM:SSZ");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_generated_at).unwrap())
        .unwrap();
    let failed_generated_at = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_generated_at, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_generated_at, "/checks/generated_at_utc_timestamp"));

    let mut future_generated_at = spv_report.clone();
    future_generated_at["generated_at"] = json!("2999-01-01T00:00:00Z");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&future_generated_at).unwrap()).unwrap();
    let failed_future_generated_at = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_future_generated_at, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_future_generated_at, "/checks/generated_at_not_future"));

    let mut placeholder_provider = spv_report.clone();
    placeholder_provider["evidence_provider"] = json!("REPLACE_WITH_EXTERNAL_SPV_OPERATOR_OR_SERVICE");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_provider).unwrap()).unwrap();
    let failed_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_provider, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_provider, "/checks/evidence_provider_identity"));

    let mut unknown_provider = spv_report.clone();
    unknown_provider["evidence_provider"] = json!("unknown-spv-provider");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&unknown_provider).unwrap()).unwrap();
    let failed_unknown_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_unknown_provider, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_unknown_provider, "/checks/evidence_provider_identity"));

    let mut local_provider = spv_report.clone();
    local_provider["evidence_provider"] = json!("local-devnet-spv-provider");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&local_provider).unwrap()).unwrap();
    let failed_local_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_local_provider, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_local_provider, "/checks/evidence_provider_identity"));

    let mut first_party_provider = spv_report.clone();
    first_party_provider["evidence_provider"] = json!("NovaSeal BTC SPV Desk");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&first_party_provider).unwrap()).unwrap();
    let failed_first_party_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_first_party_provider, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_first_party_provider, "/checks/evidence_provider_identity"));

    let mut owner_provider = spv_report.clone();
    owner_provider["evidence_provider"] = json!("a19q3 BTC SPV Desk");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&owner_provider).unwrap()).unwrap();
    let failed_owner_provider = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_owner_provider, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_owner_provider, "/checks/evidence_provider_identity"));

    let mut placeholder_network = spv_report.clone();
    placeholder_network["network"] = json!("testnet-or-mainnet");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&placeholder_network).unwrap()).unwrap();
    let failed_network = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_network, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_network, "/checks/network_public"));

    let mut local_testnet_network = spv_report.clone();
    local_testnet_network["network"] = json!("local-testnet");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&local_testnet_network).unwrap()).unwrap();
    let failed_local_testnet = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_local_testnet, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_local_testnet, "/checks/network_public"));

    let mut top_level_extra = spv_report.clone();
    top_level_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&top_level_extra).unwrap()).unwrap();
    let failed_top_level = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_top_level, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_top_level, "/checks/top_level_fields_exact"));

    let mut case_extra = spv_report.clone();
    case_extra["cases"][0]["unexpected_case_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&case_extra).unwrap()).unwrap();
    let failed_case = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_case, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_case, "/case_checks/btc-transaction-commitment-profile-v0/fields_exact"));

    let mut stale_scenario = spv_report.clone();
    stale_scenario["cases"][0]["scenario"] = json!("generic-public-btc-proof");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_scenario).unwrap()).unwrap();
    let failed_scenario = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_scenario, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_scenario, "/case_checks/btc-transaction-commitment-profile-v0/scenario_matches_expected"));

    let mut zero_btc_txid = spv_report.clone();
    zero_btc_txid["cases"][0]["btc_txid"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&zero_btc_txid).unwrap()).unwrap();
    let failed_zero_btc_txid = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_zero_btc_txid, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_zero_btc_txid, "/case_checks/btc-transaction-commitment-profile-v0/btc_txid_non_placeholder"));

    let mut zero_spv_out_point = spv_report.clone();
    zero_spv_out_point["cases"][0]["spv_client_cell_dep"]["out_point"] = json!(format!("0x{}:0", "00".repeat(32)));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&zero_spv_out_point).unwrap()).unwrap();
    let failed_zero_spv_out_point = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_zero_spv_out_point, "/status"), Some("failed"));
    assert!(json_pointer_bool(
        &failed_zero_spv_out_point,
        "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_out_point_valid"
    ));
    assert!(!json_pointer_bool(
        &failed_zero_spv_out_point,
        "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_out_point_non_placeholder"
    ));

    let mut cell_dep_extra = spv_report.clone();
    cell_dep_extra["cases"][0]["spv_client_cell_dep"]["unexpected_cell_dep_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&cell_dep_extra).unwrap()).unwrap();
    let failed_cell_dep = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_cell_dep, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_cell_dep,
        "/case_checks/btc-transaction-commitment-profile-v0/spv_client_cell_dep_fields_exact"
    ));

    let mut source_service_extra = spv_report.clone();
    source_service_extra["cases"][0]["source_service"]["unexpected_source_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&source_service_extra).unwrap()).unwrap();
    let failed_source_service = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_source_service, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_source_service,
        "/case_checks/btc-transaction-commitment-profile-v0/source_service_fields_exact"
    ));

    let mut source_service_short_commit = spv_report.clone();
    source_service_short_commit["cases"][0]["source_service"]["commit"] = json!("0123456789abcdef");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&source_service_short_commit).unwrap())
        .unwrap();
    let failed_source_service_commit = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_source_service_commit, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_source_service_commit,
        "/case_checks/btc-transaction-commitment-profile-v0/source_service_commit_40_hex"
    ));

    let mut first_party_source_service = spv_report.clone();
    first_party_source_service["cases"][0]["source_service"]["name"] = json!("CellScript SPV Service");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&first_party_source_service).unwrap())
        .unwrap();
    let failed_first_party_source_service = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_first_party_source_service, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_first_party_source_service,
        "/case_checks/btc-transaction-commitment-profile-v0/source_service_name_identity"
    ));

    let mut source_service_zero_report_hash = spv_report.clone();
    source_service_zero_report_hash["cases"][0]["source_service"]["report_hash"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&source_service_zero_report_hash).unwrap())
        .unwrap();
    let failed_source_service_zero_report_hash = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_source_service_zero_report_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_source_service_zero_report_hash,
        "/case_checks/btc-transaction-commitment-profile-v0/source_service_report_hash_non_placeholder"
    ));

    let mut handoff_extra = spv_report.clone();
    handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&handoff_extra).unwrap()).unwrap();
    let failed_handoff = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_handoff, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_handoff, "/checks/request_handoff_fields_exact"));

    let mut handoff_wrong_algorithm = spv_report.clone();
    handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&handoff_wrong_algorithm).unwrap()).unwrap();
    let failed_handoff_algorithm = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_handoff_algorithm, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_handoff_algorithm, "/checks/request_handoff_bundle_hash_algorithm"));

    let mut stale_required_profiles = spv_report.clone();
    stale_required_profiles["required_profiles"] = json!([EXPECTED_BTC_TX_COMMITMENT_PROFILE]);
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&stale_required_profiles).unwrap()).unwrap();
    let failed_required_profiles = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_required_profiles, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_required_profiles, "/checks/required_profiles_field_exact"));

    let mut extra_profile_case = spv_report.clone();
    extra_profile_case["cases"].as_array_mut().unwrap().push(case_for("unexpected-profile-v0"));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&extra_profile_case).unwrap()).unwrap();
    let failed_extra_profile = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_extra_profile, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_extra_profile, "/checks/required_profiles_covered_exact"));
    assert_eq!(json_array_strings(&failed_extra_profile, "/extra_profiles"), vec!["unexpected-profile-v0".to_string()]);

    let mut duplicate_profile_case = spv_report.clone();
    duplicate_profile_case["cases"].as_array_mut().unwrap().push(case_for(EXPECTED_BTC_TX_COMMITMENT_PROFILE));
    std::fs::write(proofs.join("public_btc_spv_evidence.json"), serde_json::to_vec_pretty(&duplicate_profile_case).unwrap()).unwrap();
    let failed_duplicate_profile = validate_btc_spv_evidence(temp.path(), PUBLIC_BTC_SPV_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_duplicate_profile, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_duplicate_profile, "/checks/required_profiles_covered_exact"));
}

#[test]
fn tx_hash_value_is_real_rejects_placeholder_and_accepts_real_hash() {
    let real_hash = test_hex32(0xab);
    assert!(tx_hash_value_is_real(&Value::String(real_hash.clone())));
    assert!(!tx_hash_value_is_real(&Value::String("0x".to_string() + &"00".repeat(32))));
    assert!(!tx_hash_value_is_real(&Value::Null));
    assert!(!tx_hash_value_is_real(&Value::String("not-a-hash".to_string())));
}

#[test]
fn is_real_tx_hash_rejects_zero_hash_placeholder() {
    let real_hash = test_hex32(0x01);
    assert!(is_real_tx_hash(&real_hash));
    assert!(!is_real_tx_hash(&format!("0x{}", "00".repeat(32))));
    assert!(!is_real_tx_hash("0xdead"));
    assert!(!is_real_tx_hash("not-even-hex"));
}

#[test]
fn btc_dual_seal_binding_requires_exact_prevout_and_sealed_tuple() {
    let btc = test_btc_profile_material(EXPECTED_DUAL_SEAL_PROFILE, 0x44);
    let valid_case = json!({
        "btc_txid": btc.txid.clone(),
        "btc_wtxid": btc.wtxid.clone(),
        "btc_tx_hex": btc.tx_hex.clone(),
        "btc_transaction_binding": btc.binding.clone(),
    });
    let valid_checks = validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &valid_case, &btc.expected_binding);
    assert!(json_pointer_bool(&valid_checks, "/btc_txid_matches_tx_hex"));
    assert!(json_pointer_bool(&valid_checks, "/dual_spend_input_matches_anchor"));
    assert!(json_pointer_bool(&valid_checks, "/dual_sealed_tx_matches_anchor"));
    assert!(json_pointer_bool(&valid_checks, "/dual_sealed_utxo_commitment_matches_tuple"));

    let wrong_prevout_tx_hex = test_bitcoin_tx_hex(&[(test_hex32(0xa1), 1)], &[(31_000, vec![0x51, 0x44])]);
    let wrong_prevout_tx = parse_bitcoin_tx_hex(&wrong_prevout_tx_hex).unwrap();
    let wrong_prevout_case = json!({
        "btc_txid": wrong_prevout_tx.txid,
        "btc_wtxid": wrong_prevout_tx.wtxid,
        "btc_tx_hex": wrong_prevout_tx_hex,
        "btc_transaction_binding": valid_case["btc_transaction_binding"].clone(),
    });
    let wrong_prevout_checks =
        validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &wrong_prevout_case, &btc.expected_binding);
    assert!(json_pointer_bool(&wrong_prevout_checks, "/btc_txid_matches_tx_hex"));
    assert!(!json_pointer_bool(&wrong_prevout_checks, "/dual_spend_input_matches_anchor"));

    let mut stale_commitment_binding = valid_case["btc_transaction_binding"].clone();
    stale_commitment_binding["sealed_utxo_commitment_hash"] = json!(test_hex32(0xa2));
    let stale_commitment_case = json!({
        "btc_txid": json_pointer_str(&valid_case, "/btc_txid").unwrap(),
        "btc_wtxid": json_pointer_str(&valid_case, "/btc_wtxid").unwrap(),
        "btc_tx_hex": json_pointer_str(&valid_case, "/btc_tx_hex").unwrap(),
        "btc_transaction_binding": stale_commitment_binding,
    });
    let stale_commitment_checks =
        validate_btc_transaction_binding(EXPECTED_DUAL_SEAL_PROFILE, &stale_commitment_case, &btc.expected_binding);
    assert!(!json_pointer_bool(&stale_commitment_checks, "/binding_matches_handoff"));
    assert!(json_pointer_bool(&stale_commitment_checks, "/dual_sealed_tx_matches_anchor"));
    assert!(!json_pointer_bool(&stale_commitment_checks, "/dual_sealed_utxo_commitment_matches_tuple"));
}

#[test]
fn btc_transaction_parser_rejects_noncanonical_compact_size_encodings() {
    let prevout = test_hex32(0x42);
    let mut input_count_noncanonical = test_bitcoin_tx_bytes(&[(prevout.clone(), 0)], &[(10_000, vec![0x51])]);
    input_count_noncanonical.splice(4..5, [0xfd, 0x01, 0x00]);
    assert!(
        parse_bitcoin_tx(&input_count_noncanonical).is_none(),
        "BTC evidence parser must reject non-minimal input-count CompactSize"
    );

    let mut script_len_noncanonical = Vec::new();
    script_len_noncanonical.extend_from_slice(&2u32.to_le_bytes());
    push_test_compact_size(&mut script_len_noncanonical, 1);
    script_len_noncanonical.extend_from_slice(&bitcoin_internal_hash_from_display(&prevout).unwrap());
    script_len_noncanonical.extend_from_slice(&0u32.to_le_bytes());
    script_len_noncanonical.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    push_test_compact_size(&mut script_len_noncanonical, 1);
    script_len_noncanonical.extend_from_slice(&10_000u64.to_le_bytes());
    script_len_noncanonical.push(0xfd);
    script_len_noncanonical.extend_from_slice(&1u16.to_le_bytes());
    script_len_noncanonical.push(0x51);
    script_len_noncanonical.extend_from_slice(&0u32.to_le_bytes());
    assert!(
        parse_bitcoin_tx(&script_len_noncanonical).is_none(),
        "BTC evidence parser must reject non-minimal script length CompactSize"
    );
}

#[test]
fn btc_transaction_parser_rejects_unknown_segwit_flags() {
    let prevout = test_hex32(0x43);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&[0x00, 0x02]);
    push_test_compact_size(&mut bytes, 1);
    bytes.extend_from_slice(&bitcoin_internal_hash_from_display(&prevout).unwrap());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    push_test_compact_size(&mut bytes, 0);
    bytes.extend_from_slice(&0xffff_ffffu32.to_le_bytes());
    push_test_compact_size(&mut bytes, 1);
    bytes.extend_from_slice(&10_000u64.to_le_bytes());
    push_test_compact_size(&mut bytes, 1);
    bytes.push(0x51);
    push_test_compact_size(&mut bytes, 0);
    bytes.extend_from_slice(&0u32.to_le_bytes());

    assert!(parse_bitcoin_tx(&bytes).is_none(), "unsupported witness flags must not be accepted as public BTC evidence");
}

#[test]
fn external_identity_rejects_local_only_names() {
    assert!(is_external_identity("NervosExternalAuditLtd"));
    for identity in [
        "REPLACE_WITH_EXTERNAL_REVIEWER",
        "unknown-reviewer",
        "NovaSeal Release Bot",
        "CellScript Team",
        "a19q3 Release Desk",
        "A19Q3 External Evidence",
        "self-attested-reviewer",
        "first-party-spv-service",
        "local-reviewer",
        "devnet-provider",
        "regtest-spv-service",
        "simnet-attestor",
        "fake-registry",
        "internal-review-team",
        "mock-custodian",
    ] {
        assert!(!is_external_identity(identity), "{identity}");
    }
}

#[test]
fn report_uri_must_use_public_https_host() {
    assert!(is_https_report_uri("https://audits.nervos.org/novaseal-bip340-tcb-review"));
    assert!(is_https_report_uri("https://audits.nervos.org:443/novaseal-bip340-tcb-review"));
    assert!(is_https_report_uri("https://audits.nervos.org?report=novaseal"));

    for uri in [
        "http://audits.nervos.org/novaseal-bip340-tcb-review",
        "https://localhost/novaseal-bip340-tcb-review",
        "https://127.0.0.1/novaseal-bip340-tcb-review",
        "https://10.0.0.7/novaseal-bip340-tcb-review",
        "https://100.64.0.1/novaseal-bip340-tcb-review",
        "https://169.254.1.1/novaseal-bip340-tcb-review",
        "https://172.20.0.1/novaseal-bip340-tcb-review",
        "https://192.168.1.1/novaseal-bip340-tcb-review",
        "https://192.0.2.1/novaseal-bip340-tcb-review",
        "https://198.18.0.1/novaseal-bip340-tcb-review",
        "https://198.51.100.1/novaseal-bip340-tcb-review",
        "https://203.0.113.1/novaseal-bip340-tcb-review",
        "https://[::1]/novaseal-bip340-tcb-review",
        "https://[fc00::1]/novaseal-bip340-tcb-review",
        "https://[fe80::1]/novaseal-bip340-tcb-review",
        "https://[2001:db8::1]/novaseal-bip340-tcb-review",
        "https://audits.nervos.org:0/novaseal-bip340-tcb-review",
        "https://audits.nervos.org:65536/novaseal-bip340-tcb-review",
        "https://reviewer@audits.nervos.org/novaseal-bip340-tcb-review",
        "https://audits.nervos.local/novaseal-bip340-tcb-review",
        "https://audits.nervos.test/novaseal-bip340-tcb-review",
        "https://123.456/novaseal-bip340-tcb-review",
    ] {
        assert!(!is_https_report_uri(uri), "{uri}");
    }
}

#[test]
fn planned_profile_package_cannot_pass_without_live_lifecycle_when_business_scenario_requires_it() {
    let stateful = json!({
        "status": "passed",
        "blocker_count": 0,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": {"status": "passed", "covered_profiles": []},
        "business_scenario_coverage": {"status": "passed", "checks": {}},
    });
    let profile_cert = json!({
        "status": "passed",
        "local_checks": {"conformance_gate_passed": true},
        "planned_profile_packages": {"fungible_xudt": {"status": "passed"}},
    });

    let matrix = build_planned_profile_matrix(&profile_cert, &stateful);
    assert_eq!(json_pointer_str(&matrix, "/status"), Some("incomplete"));
    let xudt_profile_row = matrix["profiles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| json_pointer_str(row, "/id") == Some("object_profile_fungible_xudt"))
        .unwrap();
    assert_eq!(json_pointer_str(xudt_profile_row, "/status"), Some("passed"));
    let xudt_scenario_row = matrix["business_scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| json_pointer_str(row, "/id") == Some("fungible_xudt_value_flow"))
        .unwrap();
    assert_eq!(json_pointer_str(xudt_scenario_row, "/status"), Some("missing"));
}

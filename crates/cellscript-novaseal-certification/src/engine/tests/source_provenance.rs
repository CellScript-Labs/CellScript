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
    assert_eq!(canonical_schema_hash(&schema).unwrap().unwrap(), "0x6b4277f67ee3e47f391d8591f7efccc6e97dcac5436dd22568d72689ac4db130");
}

#[test]
fn novaseal_handoff_hash_matches_reference_generator_vector() {
    let value = json!({
        "z": 1,
        "a": ["b", true, null],
    });

    assert_eq!(cellscript_canonical_json::canonical_json_string(&value), r#"{"a":["b",true,null],"z":1}"#);
    assert_eq!(
        novaseal_handoff_report_hash("test_label", &value),
        "0x91f5e5cc38c16e792d27a3738a7a7c77053fa15f902e2ccb4b210fd7239a476f"
    );
}

#[test]
fn wallet_lock_alignment_rejects_legacy_domain_hash_source_model() {
    let valid = validate_wallet_lock_alignment_detail(&wallet_lock_alignment_report(false));
    assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));

    let legacy = validate_wallet_lock_alignment_detail(&wallet_lock_alignment_report(true));
    assert_eq!(json_pointer_str(&legacy, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&legacy, "/checks/source_model_uses_packed_intent"));
}

#[test]
fn live_stateful_core_requires_tx_hashes_and_post_negative_liveness() {
    let mut live_core = json!({
        "status": "passed",
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "provenance_freshness_matched": true,
        "bootstrap_tx_hash": test_hex32(1),
        "bootstrap_state_cell_live": true,
        "transition_tx_hash": test_hex32(2),
        "old_state_not_live": true,
        "new_state_live": true,
        "receipt_live": true,
        "wrong_signature_rejected": true,
        "post_negative_state_still_live": true,
    });

    assert!(core_live_summary_passed(&live_core));

    live_core["transition_tx_hash"] = Value::Null;
    assert!(!core_live_summary_passed(&live_core));

    live_core["transition_tx_hash"] = Value::String(test_hex32(2));
    live_core["post_negative_state_still_live"] = Value::Bool(false);
    assert!(!core_live_summary_passed(&live_core));
}

#[test]
fn live_stateful_agreement_requires_all_transaction_hashes() {
    let conformance = json!({"status": "passed"});
    let mut live_agreement = json!({
        "status": "passed",
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "provenance_freshness_matched": true,
        "originate_tx_hash": test_hex32(1),
        "repay_tx_hash": test_hex32(2),
        "claim_originate_tx_hash": test_hex32(3),
        "claim_tx_hash": test_hex32(4),
        "origin_active_live": true,
        "origin_principal_payout_live": true,
        "origin_receipt_live": true,
        "claim_origin_active_live": true,
        "claim_origin_principal_payout_live": true,
        "claim_origin_receipt_live": true,
        "repay_old_active_not_live": true,
        "repay_closed_live": true,
        "repay_lender_repayment_live": true,
        "repay_borrower_collateral_return_live": true,
        "repay_receipt_live": true,
        "claim_old_active_not_live": true,
        "claim_closed_live": true,
        "claim_lender_default_claim_live": true,
        "claim_receipt_live": true,
        "wrong_lender_signature_rejected": true,
        "non_ckb_asset_kind_rejected": true,
        "wrong_borrower_signature_rejected": true,
        "repay_payout_capacity_short_rejected": true,
        "repay_payout_lock_args_mismatch_rejected": true,
        "repay_wrong_payout_amount_rejected": true,
        "early_claim_rejected": true,
        "wrong_lender_claim_signature_rejected": true,
        "post_negative_active_still_live": true,
        "post_claim_negative_active_still_live": true,
    });

    assert!(agreement_live_summary_passed(&live_agreement, &conformance));

    live_agreement["claim_tx_hash"] = Value::Null;
    assert!(!agreement_live_summary_passed(&live_agreement, &conformance));
}

#[test]
fn provenance_freshness_requires_repo_commit_match() {
    let directory = tempfile::tempdir().unwrap();
    let repo_root = directory.path();
    let source_paths = ["src/policy.cell"];
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join(source_paths[0]), "module fixture;\n").unwrap();
    let commit = git_available().then(|| ensure_clean_fixture_repo(repo_root));
    let current_source = source_tree_hash(repo_root, &source_paths).unwrap();
    let artifact_path = repo_root.join(source_paths[0]);
    let artifact_sha = sha256_file_hex(&artifact_path).unwrap();
    let mut report = json!({
        "provenance": {
            "repo_commit": "0000000000000000000000000000000000000000",
            "source_tree": current_source,
            "artifacts": {
                "verifier": {
                    "path": artifact_path.display().to_string(),
                    "sha256": artifact_sha,
                },
                "lifecycle": {
                    "path": artifact_path.display().to_string(),
                    "sha256": artifact_sha,
                },
            },
        },
    });

    let summary = provenance_summary(&report, repo_root, &source_paths).unwrap();

    assert!(json_pointer_bool(&summary, "/source_hash_matches"));
    assert!(json_pointer_bool(&summary, "/artifact_hashes_match"));
    assert!(!json_pointer_bool(&summary, "/repo_commit_matches"));
    assert!(!json_pointer_bool(&summary, "/freshness_matched"));
    if let Some(commit) = commit {
        report["provenance"]["repo_commit"] = json!(commit);
        let summary = provenance_summary(&report, repo_root, &source_paths).unwrap();
        assert!(json_pointer_bool(&summary, "/repo_commit_matches"));
        assert!(json_pointer_bool(&summary, "/freshness_matched"));
    }
}

#[cfg(unix)]
#[test]
fn source_tree_expected_files_and_provenance_reject_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().join("repo");
    let src = repo_root.join("src");
    let schemas = repo_root.join("schemas");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&schemas).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(src.join("real.cell"), "module real;\n").unwrap();
    std::fs::write(outside.join("external.cell"), "module external;\n").unwrap();
    std::fs::write(outside.join("schema.schema"), "array External {}\n").unwrap();
    symlink(outside.join("external.cell"), src.join("linked.cell")).unwrap();
    symlink(outside.join("schema.schema"), schemas.join("expected.schema")).unwrap();

    let source = source_tree_hash(&repo_root, &["src"]).unwrap();
    assert!(!json_pointer_bool(&source, "/valid"));
    assert!(json_pointer_str(&source, "/sha256").is_none());
    let invalid_source_paths = json_array_strings(&source, "/invalid_paths");
    assert!(invalid_source_paths.iter().any(|path| path.ends_with("src/linked.cell")));

    let artifact_sha = sha256_file_hex(&src.join("real.cell")).unwrap();
    let invalid_source_report = json!({
        "provenance": {
            "source_tree": source,
            "artifacts": {
                "verifier": {
                    "path": src.join("real.cell").display().to_string(),
                    "sha256": artifact_sha,
                },
                "lifecycle": {
                    "path": src.join("real.cell").display().to_string(),
                    "sha256": artifact_sha,
                },
            },
        },
    });
    let invalid_source_summary = provenance_summary(&invalid_source_report, &repo_root, &["src"]).unwrap();
    assert!(!json_pointer_bool(&invalid_source_summary, "/source_hash_matches"));
    assert!(!json_pointer_bool(&invalid_source_summary, "/current_source_valid"));
    assert!(!json_pointer_bool(&invalid_source_summary, "/freshness_matched"));

    let expected = expected_files(&repo_root, &schemas, &["expected.schema"]).unwrap();
    assert!(!json_pointer_bool(&expected, "/exact"));
    assert!(json_array_strings(&expected, "/invalid").iter().any(|path| path == "expected.schema"));
    assert!(expected.pointer("/hashes/expected.schema").is_none());

    let current_source = source_tree_hash(&repo_root, &["src/real.cell"]).unwrap();
    assert!(json_pointer_bool(&current_source, "/valid"));
    let outside_artifact = outside.join("artifact.elf");
    std::fs::write(&outside_artifact, b"artifact").unwrap();
    let artifact_sha = sha256_file_hex(&outside_artifact).unwrap();
    let report = json!({
        "provenance": {
            "source_tree": current_source,
            "artifacts": {
                "verifier": {
                    "path": outside_artifact.display().to_string(),
                    "sha256": artifact_sha,
                },
                "lifecycle": {
                    "path": outside_artifact.display().to_string(),
                    "sha256": artifact_sha,
                },
            },
        },
    });

    let summary = provenance_summary(&report, &repo_root, &["src/real.cell"]).unwrap();
    assert!(json_pointer_bool(&summary, "/source_hash_matches"));
    assert!(!json_pointer_bool(&summary, "/artifact_hashes_match"));
    assert!(!json_pointer_bool(&summary, "/artifacts/verifier/regular_file_within_repo"));
    assert!(!json_pointer_bool(&summary, "/freshness_matched"));
}

#[cfg(unix)]
#[test]
fn json_evidence_reports_must_be_regular_files_within_repo_root() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().join("repo");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("external.json"), "{}\n").unwrap();

    symlink(outside.join("external.json"), repo_root.join("linked.json")).unwrap();
    let err = json_load_path(&repo_root, &repo_root.join("linked.json")).unwrap_err();
    assert!(
        err.to_string().contains("refusing to read JSON report") && err.to_string().contains("regular file within repository root"),
        "unexpected error: {}",
        err
    );

    symlink(&outside, repo_root.join("linked-dir")).unwrap();
    let err = json_load_path_optional(&repo_root, &repo_root.join("linked-dir/external.json")).unwrap_err();
    assert!(
        err.to_string().contains("refusing to read JSON report") && err.to_string().contains("resolves outside repository root"),
        "unexpected error: {}",
        err
    );
}

#[test]
fn rwa_lifecycle_event_data_hash_binding_is_required_and_present() {
    let (_, pattern) = REQUIRED_RWA_RECEIPT_SOURCE_PATTERNS
        .iter()
        .find(|(name, _)| *name == "expected_event_data_hash")
        .expect("RWA certification must require event data hash binding");
    let repo_root = unit_resource_root();
    let source = read_cell_sources(&repo_root.join("proposals/novaseal/rwa-receipt-profile-v0/src")).unwrap();

    assert_eq!(*pattern, "intent.expected_event_data_hash == ckb::hash_data_packed(event)");
    assert!(source.contains(pattern), "RWA lifecycle source must bind event output hash to derived event");
}

#[test]
fn profile_operator_fixtures_bind_current_live_reports() {
    let temp = tempfile::tempdir().unwrap();
    write_expected_live_fixture_reports(temp.path());
    let report = operator_fixture_report(temp.path());

    let detail = validate_profile_operator_fixture_detail(temp.path(), &report).unwrap();
    assert_eq!(json_pointer_str(&detail, "/status"), Some("passed"));

    let mut stale = report;
    stale["cases"][0]["live_report_hash"] = Value::String(test_hex32(0xfe));
    let detail = validate_profile_operator_fixture_detail(temp.path(), &stale).unwrap();
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&detail, "/cases/fungible-xudt-profile-v0:issue_xudt/live_report_hash_matches_current_report",));

    let mut stale_anchor = operator_fixture_report(temp.path());
    let btc_index =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_BTC_TX_COMMITMENT_PROFILE).unwrap();
    stale_anchor["cases"][btc_index]["public_btc_anchor"]["ckb_btc_commitment_hash"] = Value::String(test_hex32(0xfc));
    let detail = validate_profile_operator_fixture_detail(temp.path(), &stale_anchor).unwrap();
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &detail,
        "/cases/btc-transaction-commitment-profile-v0:commit_btc_transaction_transition/public_btc_anchor_matches_current_report",
    ));

    let mut stale_dual_anchor = operator_fixture_report(temp.path());
    let dual_index =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_DUAL_SEAL_PROFILE).unwrap();
    stale_dual_anchor["cases"][dual_index]["public_btc_anchor"].as_object_mut().unwrap().remove("sealed_btc_txid");
    stale_dual_anchor["cases"][dual_index]["wallet_display"]["public_btc_anchor"].as_object_mut().unwrap().remove("sealed_btc_txid");
    let detail = validate_profile_operator_fixture_detail(temp.path(), &stale_dual_anchor).unwrap();
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&detail, "/cases/dual-seal-profile-v0:finalize_dual_seal/public_btc_anchor_shape_matches_profile",));
}

#[test]
fn certification_gate_status_uses_detailed_validator_result() {
    let profile_certification = json!({
        "profile_operator_fixtures": {
            "status": "failed",
            "checks": {
                "report_passed": true,
                "summary_counts_match": true,
                "case_details": false
            }
        }
    });

    let gate = gate(
        "planned_profile_operator_fixtures",
        certification_detail_status(&profile_certification, "/profile_operator_fixtures/status"),
        PROFILE_OPERATOR_FIXTURES,
        certification_detail(&profile_certification, "/profile_operator_fixtures"),
    );

    assert_eq!(json_pointer_str(&gate, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&gate, "/detail/checks/case_details"));
}

#[test]
fn service_builder_fixtures_bind_current_operator_report() {
    let temp = tempfile::tempdir().unwrap();
    write_expected_live_fixture_reports(temp.path());
    let operator_report = operator_fixture_report(temp.path());
    let report = service_builder_report(&operator_report);

    let detail = validate_service_builder_fixture_detail(&report, &operator_report);
    assert_eq!(json_pointer_str(&detail, "/status"), Some("passed"));

    let mut stale = report.clone();
    stale["source_operator_fixture_report_hash"] = Value::String(test_hex32(0xfd));
    let detail = validate_service_builder_fixture_detail(&stale, &operator_report);
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&detail, "/checks/source_operator_fixture_report_hash_matches_current_report"));

    let mut stale_anchor = report;
    let btc_index =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_BTC_TX_COMMITMENT_PROFILE).unwrap();
    stale_anchor["cases"][btc_index]["request"]["required_live_inputs"]["public_btc_anchor"]["ckb_btc_commitment_hash"] =
        Value::String(test_hex32(0xfb));
    let detail = validate_service_builder_fixture_detail(&stale_anchor, &operator_report);
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &detail,
        "/cases/btc-transaction-commitment-profile-v0:commit_btc_transaction_transition/public_btc_anchor_input_matches_operator_fixture",
    ));

    let mut stale_dual_anchor = service_builder_report(&operator_report);
    let dual_index =
        EXPECTED_PROFILE_OPERATOR_FIXTURES.iter().position(|fixture| fixture.profile == EXPECTED_DUAL_SEAL_PROFILE).unwrap();
    stale_dual_anchor["cases"][dual_index]["request"]["required_live_inputs"]["public_btc_anchor"]
        .as_object_mut()
        .unwrap()
        .remove("sealed_utxo_commitment_hash");
    stale_dual_anchor["cases"][dual_index]["tx_skeleton"]["public_btc_anchor"]
        .as_object_mut()
        .unwrap()
        .remove("sealed_utxo_commitment_hash");
    let detail = validate_service_builder_fixture_detail(&stale_dual_anchor, &operator_report);
    assert_eq!(json_pointer_str(&detail, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &detail,
        "/cases/dual-seal-profile-v0:finalize_dual_seal/public_btc_anchor_input_shape_matches_profile",
    ));
}

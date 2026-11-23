#[test]
fn external_attestations_require_exact_report_fields() {
    let temp = tempfile::tempdir().unwrap();
    let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
    std::fs::create_dir_all(&proofs).unwrap();
    let artifact_hash = format!("0x{}", "aa".repeat(32));
    let source_tree_hash = format!("0x{}", "bb".repeat(32));
    let tcb_repo_commit = "0123456789abcdef0123456789abcdef01234567";
    let handoff = json!({
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": "passed",
        "cases": [
            {
                "group": "public_shared_cell_dep_attestation",
                "expected_values": {
                    "artifact_hash": artifact_hash,
                    "release.manifest_commit": tcb_repo_commit,
                    "release.package": "novaseal",
                    "release.version": EXPECTED_NOVASEAL_RELEASE_VERSION,
                    "runtime_verifier.dep_type": EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE,
                    "runtime_verifier.hash_type": EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE,
                    "runtime_verifier.ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "runtime_verifier.verifier_id": "btc.bip340.v0",
                },
            },
            {
                "group": "external_bip340_tcb_review_attestation",
                "expected_values": {
                    "artifact_hash": artifact_hash,
                    "artifact_hash_algorithm": "sha256",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                    "source_tree_sha256": source_tree_hash,
                    "verifier_id": "btc.bip340.v0",
                },
            },
        ],
    });
    let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);
    let public_attestation = json!({
        "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
        "status": "attested",
        "network": "testnet",
        "attested_at": "2026-06-04T00:00:00Z",
        "attestor": "external-cell-dep-operator",
        "release": {
            "package": "novaseal",
            "version": "0.0.1-v0-mvp",
            "manifest_commit": tcb_repo_commit,
        },
        "notes": "external public CellDep attestation fixture",
        "request_handoff": {
            "bundle": EXTERNAL_EVIDENCE_HANDOFF,
            "bundle_hash": handoff_hash,
            "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
            "group": "public_shared_cell_dep_attestation",
        },
        "runtime_verifier": {
            "verifier_id": "btc.bip340.v0",
            "ipc_abi": "cellscript-btc-bip340-ipc-v0",
            "artifact_hash": artifact_hash,
            "out_point": format!("0x{}:0", "11".repeat(32)),
            "data_hash": format!("0x{}", "22".repeat(32)),
            "dep_type": "code",
            "hash_type": "data1",
        },
    });
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_attestation).unwrap())
        .unwrap();
    let public_passed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_passed, "/status"), Some("passed"));
    assert!(json_pointer_bool(&public_passed, "/checks/top_level_fields_exact"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_fields_exact"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_version_matches_expected"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_present"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_matches_tcb"));
    assert!(json_pointer_bool(&public_passed, "/checks/network_public"));
    assert!(json_pointer_bool(&public_passed, "/checks/attested_at_utc_timestamp"));
    assert!(json_pointer_bool(&public_passed, "/checks/attested_at_not_future"));
    assert!(json_pointer_bool(&public_passed, "/checks/attestor_identity"));
    assert!(json_pointer_bool(&public_passed, "/checks/request_handoff_fields_exact"));
    assert!(json_pointer_bool(&public_passed, "/checks/request_handoff_bundle_hash_algorithm"));
    assert!(json_pointer_bool(&public_passed, "/checks/handoff_expected_values_exact"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_version_matches_handoff"));
    assert!(json_pointer_bool(&public_passed, "/checks/release_manifest_commit_matches_handoff"));
    assert!(json_pointer_bool(&public_passed, "/checks/runtime_verifier_fields_exact"));
    assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_valid"));
    assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_non_placeholder"));
    assert!(json_pointer_bool(&public_passed, "/checks/artifact_hash_matches_handoff"));
    assert!(json_pointer_bool(&public_passed, "/checks/out_point_valid"));
    assert!(json_pointer_bool(&public_passed, "/checks/dep_type"));
    assert!(json_pointer_bool(&public_passed, "/checks/dep_type_matches_handoff"));
    assert!(json_pointer_bool(&public_passed, "/checks/hash_type"));
    assert!(json_pointer_bool(&public_passed, "/checks/hash_type_matches_expected"));
    assert!(json_pointer_bool(&public_passed, "/checks/hash_type_matches_handoff"));
    assert!(json_pointer_bool(&public_passed, "/checks/data_hash_valid"));

    let mut stale_public_handoff = handoff.clone();
    stale_public_handoff["cases"][0]["expected_values"]["release.version"] = json!("0.0.2");
    let stale_public_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_public_handoff);
    let mut public_with_stale_handoff = public_attestation.clone();
    public_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_public_handoff_hash);
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_with_stale_handoff).unwrap(),
    )
    .unwrap();
    let public_stale_handoff_failed = validate_public_attestation(
        temp.path(),
        PUBLIC_CELLDEP_ATTESTATION,
        Some(&artifact_hash),
        Some(tcb_repo_commit),
        &stale_public_handoff,
    )
    .unwrap();
    assert_eq!(json_pointer_str(&public_stale_handoff_failed, "/status"), Some("failed"));
    assert!(json_pointer_bool(&public_stale_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(!json_pointer_bool(&public_stale_handoff_failed, "/checks/release_version_matches_handoff"));

    let mut public_placeholder_attested_at = public_attestation.clone();
    public_placeholder_attested_at["attested_at"] = json!("YYYY-MM-DDTHH:MM:SSZ");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_placeholder_attested_at).unwrap(),
    )
    .unwrap();
    let public_attested_at_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_attested_at_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_attested_at_failed, "/checks/attested_at_utc_timestamp"));

    let mut public_future_attested_at = public_attestation.clone();
    public_future_attested_at["attested_at"] = json!("2999-01-01T00:00:00Z");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_future_attested_at).unwrap(),
    )
    .unwrap();
    let public_future_attested_at_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_future_attested_at_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_future_attested_at_failed, "/checks/attested_at_not_future"));

    let mut public_placeholder_network = public_attestation.clone();
    public_placeholder_network["network"] = json!("testnet-or-mainnet");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_placeholder_network).unwrap(),
    )
    .unwrap();
    let public_network_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_network_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_network_failed, "/checks/network_public"));

    let mut public_private_network = public_attestation.clone();
    public_private_network["network"] = json!("ckb-private-mainnet");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_private_network).unwrap(),
    )
    .unwrap();
    let public_private_network_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_private_network_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_private_network_failed, "/checks/network_public"));

    let mut public_invalid_out_point = public_attestation.clone();
    public_invalid_out_point["runtime_verifier"]["out_point"] = json!(format!("0x{}:not-an-index", "11".repeat(32)));
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_invalid_out_point).unwrap(),
    )
    .unwrap();
    let public_out_point_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_out_point_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_out_point_failed, "/checks/out_point_valid"));

    let mut public_invalid_dep_type = public_attestation.clone();
    public_invalid_dep_type["runtime_verifier"]["dep_type"] = json!("dep_group");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_invalid_dep_type).unwrap(),
    )
    .unwrap();
    let public_dep_type_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_dep_type_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_dep_type_failed, "/checks/dep_type"));

    let mut public_invalid_hash_type = public_attestation.clone();
    public_invalid_hash_type["runtime_verifier"]["hash_type"] = json!("invalid-hash-type");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_invalid_hash_type).unwrap(),
    )
    .unwrap();
    let public_hash_type_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_hash_type_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_hash_type_failed, "/checks/hash_type"));

    let mut public_stale_hash_type = public_attestation.clone();
    public_stale_hash_type["runtime_verifier"]["hash_type"] = json!("type");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_stale_hash_type).unwrap(),
    )
    .unwrap();
    let public_stale_hash_type_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_stale_hash_type_failed, "/status"), Some("failed"));
    assert!(json_pointer_bool(&public_stale_hash_type_failed, "/checks/hash_type"));
    assert!(!json_pointer_bool(&public_stale_hash_type_failed, "/checks/hash_type_matches_expected"));

    let mut public_invalid_data_hash = public_attestation.clone();
    public_invalid_data_hash["runtime_verifier"]["data_hash"] = json!("0xnot-a-32-byte-hash");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_invalid_data_hash).unwrap(),
    )
    .unwrap();
    let public_data_hash_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_data_hash_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_data_hash_failed, "/checks/data_hash_valid"));

    let mut public_invalid_artifact_hash = public_attestation.clone();
    public_invalid_artifact_hash["runtime_verifier"]["artifact_hash"] = json!("0xnot-a-32-byte-hash");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_invalid_artifact_hash).unwrap(),
    )
    .unwrap();
    let public_artifact_hash_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_artifact_hash_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_artifact_hash_failed, "/checks/artifact_hash_valid"));

    let mut public_zero_artifact_hash = public_attestation.clone();
    public_zero_artifact_hash["runtime_verifier"]["artifact_hash"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_zero_artifact_hash).unwrap(),
    )
    .unwrap();
    let public_zero_artifact_hash_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_zero_artifact_hash_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_zero_artifact_hash_failed, "/checks/artifact_hash_non_placeholder"));

    let mut public_stale_manifest_commit = public_attestation.clone();
    public_stale_manifest_commit["release"]["manifest_commit"] = json!("fedcba9876543210fedcba9876543210fedcba98");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_stale_manifest_commit).unwrap(),
    )
    .unwrap();
    let public_manifest_commit_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_manifest_commit_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_manifest_commit_failed, "/checks/release_manifest_commit_matches_tcb"));

    let mut public_stale_release_version = public_attestation.clone();
    public_stale_release_version["release"]["version"] = json!("0.0.2");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_stale_release_version).unwrap(),
    )
    .unwrap();
    let public_release_version_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_release_version_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_release_version_failed, "/checks/release_version_matches_expected"));

    let mut public_placeholder_attestor = public_attestation.clone();
    public_placeholder_attestor["attestor"] = json!("REPLACE_WITH_DEPLOYER_OR_RELEASE_SIGNER");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_placeholder_attestor).unwrap(),
    )
    .unwrap();
    let public_attestor_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_attestor_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_attestor_failed, "/checks/attestor_identity"));

    let mut public_local_attestor = public_attestation.clone();
    public_local_attestor["attestor"] = json!("local-devnet-deployer");
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_local_attestor).unwrap())
        .unwrap();
    let public_local_attestor_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_local_attestor_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_local_attestor_failed, "/checks/attestor_identity"));

    let mut public_first_party_attestor = public_attestation.clone();
    public_first_party_attestor["attestor"] = json!("NovaSeal Release Bot");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_first_party_attestor).unwrap(),
    )
    .unwrap();
    let public_first_party_attestor_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_first_party_attestor_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_first_party_attestor_failed, "/checks/attestor_identity"));

    let mut public_extra = public_attestation.clone();
    public_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_extra).unwrap()).unwrap();
    let public_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_failed, "/checks/top_level_fields_exact"));

    let mut public_nested_extra = public_attestation.clone();
    public_nested_extra["runtime_verifier"]["unexpected_runtime_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_nested_extra).unwrap())
        .unwrap();
    let public_nested_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_nested_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_nested_failed, "/checks/runtime_verifier_fields_exact"));

    let mut public_release_string = public_attestation.clone();
    public_release_string["release"] = Value::String("novaseal-btc-bip340-v0".to_string());
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_release_string).unwrap())
        .unwrap();
    let public_release_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_release_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_release_failed, "/checks/release_fields_exact"));

    let mut public_handoff_extra = public_attestation.clone();
    public_handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("public_shared_cell_dep_attestation.json"), serde_json::to_vec_pretty(&public_handoff_extra).unwrap())
        .unwrap();
    let public_handoff_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_handoff_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_handoff_failed, "/checks/request_handoff_fields_exact"));

    let mut public_handoff_wrong_algorithm = public_attestation.clone();
    public_handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.json"),
        serde_json::to_vec_pretty(&public_handoff_wrong_algorithm).unwrap(),
    )
    .unwrap();
    let public_handoff_algorithm_failed =
        validate_public_attestation(temp.path(), PUBLIC_CELLDEP_ATTESTATION, Some(&artifact_hash), Some(tcb_repo_commit), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&public_handoff_algorithm_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&public_handoff_algorithm_failed, "/checks/request_handoff_bundle_hash_algorithm"));

    let external_review = json!({
        "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
        "status": "accepted",
        "artifact_hash": artifact_hash,
        "artifact_hash_algorithm": "sha256",
        "source_tree_sha256": source_tree_hash,
        "verifier_id": "btc.bip340.v0",
        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
        "reviewer": "external-tcb-reviewer",
        "review_date": "2026-06-04",
        "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
        "report_uri": "https://audits.nervos.org/novaseal-bip340-tcb-review",
        "notes": "external review fixture",
        "request_handoff": {
            "bundle": EXTERNAL_EVIDENCE_HANDOFF,
            "bundle_hash": handoff_hash,
            "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
            "group": "external_bip340_tcb_review_attestation",
        },
    });
    std::fs::write(proofs.join("bip340_external_tcb_review_attestation.json"), serde_json::to_vec_pretty(&external_review).unwrap())
        .unwrap();
    let review_passed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_passed, "/status"), Some("passed"));
    assert!(json_pointer_bool(&review_passed, "/checks/top_level_fields_exact"));
    assert!(json_pointer_bool(&review_passed, "/checks/request_handoff_fields_exact"));
    assert!(json_pointer_bool(&review_passed, "/checks/request_handoff_bundle_hash_algorithm"));
    assert!(json_pointer_bool(&review_passed, "/checks/handoff_expected_values_exact"));
    assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_valid"));
    assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_non_placeholder"));
    assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_matches_handoff"));
    assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_algorithm"));
    assert!(json_pointer_bool(&review_passed, "/checks/artifact_hash_algorithm_matches_handoff"));
    assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_valid"));
    assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_non_placeholder"));
    assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_matches_current_tcb"));
    assert!(json_pointer_bool(&review_passed, "/checks/source_tree_sha256_matches_handoff"));
    assert!(json_pointer_bool(&review_passed, "/checks/verifier_id"));
    assert!(json_pointer_bool(&review_passed, "/checks/verifier_id_matches_handoff"));
    assert!(json_pointer_bool(&review_passed, "/checks/ipc_abi"));
    assert!(json_pointer_bool(&review_passed, "/checks/ipc_abi_matches_handoff"));
    assert!(json_pointer_bool(&review_passed, "/checks/reviewer_identity"));
    assert!(json_pointer_bool(&review_passed, "/checks/review_date_utc_date"));
    assert!(json_pointer_bool(&review_passed, "/checks/review_date_not_future"));
    assert!(json_pointer_bool(&review_passed, "/checks/report_uri_https"));
    assert!(json_pointer_bool(&review_passed, "/checks/review_scope_exact"));
    assert!(json_pointer_bool(&review_passed, "/checks/review_scope_matches_handoff"));

    let mut stale_review_handoff = handoff.clone();
    stale_review_handoff["cases"][1]["expected_values"]["source_tree_sha256"] = json!(format!("0x{}", "cc".repeat(32)));
    let stale_review_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_handoff);
    let mut review_with_stale_handoff = external_review.clone();
    review_with_stale_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_handoff_hash);
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_with_stale_handoff).unwrap(),
    )
    .unwrap();
    let review_stale_handoff_failed = validate_external_review(
        temp.path(),
        EXTERNAL_TCB_ATTESTATION,
        Some(&artifact_hash),
        Some(&source_tree_hash),
        &stale_review_handoff,
    )
    .unwrap();
    assert_eq!(json_pointer_str(&review_stale_handoff_failed, "/status"), Some("failed"));
    assert!(json_pointer_bool(&review_stale_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(!json_pointer_bool(&review_stale_handoff_failed, "/checks/source_tree_sha256_matches_handoff"));

    let mut stale_review_ipc_handoff = handoff.clone();
    stale_review_ipc_handoff["cases"][1]["expected_values"]["ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
    let stale_review_ipc_handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_ipc_handoff);
    let mut review_with_stale_ipc_handoff = external_review.clone();
    review_with_stale_ipc_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_ipc_handoff_hash);
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_with_stale_ipc_handoff).unwrap(),
    )
    .unwrap();
    let review_stale_ipc_handoff_failed = validate_external_review(
        temp.path(),
        EXTERNAL_TCB_ATTESTATION,
        Some(&artifact_hash),
        Some(&source_tree_hash),
        &stale_review_ipc_handoff,
    )
    .unwrap();
    assert_eq!(json_pointer_str(&review_stale_ipc_handoff_failed, "/status"), Some("failed"));
    assert!(json_pointer_bool(&review_stale_ipc_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(!json_pointer_bool(&review_stale_ipc_handoff_failed, "/checks/ipc_abi_matches_handoff"));

    let mut stale_review_verifier_handoff = handoff.clone();
    stale_review_verifier_handoff["cases"][1]["expected_values"]["verifier_id"] = json!("btc.bip340.v1");
    let stale_review_verifier_handoff_hash =
        novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_review_verifier_handoff);
    let mut review_with_stale_verifier_handoff = external_review.clone();
    review_with_stale_verifier_handoff["request_handoff"]["bundle_hash"] = json!(stale_review_verifier_handoff_hash);
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_with_stale_verifier_handoff).unwrap(),
    )
    .unwrap();
    let review_stale_verifier_handoff_failed = validate_external_review(
        temp.path(),
        EXTERNAL_TCB_ATTESTATION,
        Some(&artifact_hash),
        Some(&source_tree_hash),
        &stale_review_verifier_handoff,
    )
    .unwrap();
    assert_eq!(json_pointer_str(&review_stale_verifier_handoff_failed, "/status"), Some("failed"));
    assert!(json_pointer_bool(&review_stale_verifier_handoff_failed, "/checks/request_handoff_bundle_hash_matches_current"));
    assert!(!json_pointer_bool(&review_stale_verifier_handoff_failed, "/checks/verifier_id_matches_handoff"));

    let mut review_placeholder_date = external_review.clone();
    review_placeholder_date["review_date"] = json!("YYYY-MM-DD");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_placeholder_date).unwrap(),
    )
    .unwrap();
    let review_date_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_date_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_date_failed, "/checks/review_date_utc_date"));

    let mut review_future_date = external_review.clone();
    review_future_date["review_date"] = json!("2999-01-01");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_future_date).unwrap(),
    )
    .unwrap();
    let review_future_date_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_future_date_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_future_date_failed, "/checks/review_date_not_future"));

    let mut review_stale_source_tree = external_review.clone();
    review_stale_source_tree["source_tree_sha256"] = json!(format!("0x{}", "cc".repeat(32)));
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_stale_source_tree).unwrap(),
    )
    .unwrap();
    let review_stale_source_tree_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_stale_source_tree_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_stale_source_tree_failed, "/checks/source_tree_sha256_matches_current_tcb"));

    let mut review_invalid_artifact_hash = external_review.clone();
    review_invalid_artifact_hash["artifact_hash"] = json!("0xnot-a-32-byte-hash");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_invalid_artifact_hash).unwrap(),
    )
    .unwrap();
    let review_invalid_artifact_hash_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_invalid_artifact_hash_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_invalid_artifact_hash_failed, "/checks/artifact_hash_valid"));

    let mut review_zero_artifact_hash = external_review.clone();
    review_zero_artifact_hash["artifact_hash"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_zero_artifact_hash).unwrap(),
    )
    .unwrap();
    let review_zero_artifact_hash_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_zero_artifact_hash_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_zero_artifact_hash_failed, "/checks/artifact_hash_non_placeholder"));

    let mut review_zero_source_tree = external_review.clone();
    review_zero_source_tree["source_tree_sha256"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_zero_source_tree).unwrap(),
    )
    .unwrap();
    let review_zero_source_tree_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_zero_source_tree_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_zero_source_tree_failed, "/checks/source_tree_sha256_non_placeholder"));

    let mut review_placeholder_reviewer = external_review.clone();
    review_placeholder_reviewer["reviewer"] = json!("REPLACE_WITH_EXTERNAL_REVIEWER");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_placeholder_reviewer).unwrap(),
    )
    .unwrap();
    let review_reviewer_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_reviewer_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_reviewer_failed, "/checks/reviewer_identity"));

    let mut review_unknown_reviewer = external_review.clone();
    review_unknown_reviewer["reviewer"] = json!("unknown-reviewer");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_unknown_reviewer).unwrap(),
    )
    .unwrap();
    let review_unknown_reviewer_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_unknown_reviewer_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_unknown_reviewer_failed, "/checks/reviewer_identity"));

    let mut review_local_reviewer = external_review.clone();
    review_local_reviewer["reviewer"] = json!("local-devnet-reviewer");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_local_reviewer).unwrap(),
    )
    .unwrap();
    let review_local_reviewer_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_local_reviewer_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_local_reviewer_failed, "/checks/reviewer_identity"));

    let mut review_first_party_reviewer = external_review.clone();
    review_first_party_reviewer["reviewer"] = json!("CellScript TCB Review Team");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_first_party_reviewer).unwrap(),
    )
    .unwrap();
    let review_first_party_reviewer_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_first_party_reviewer_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_first_party_reviewer_failed, "/checks/reviewer_identity"));

    let mut review_placeholder_uri = external_review.clone();
    review_placeholder_uri["report_uri"] = json!("REPLACE_WITH_EXTERNAL_REVIEW_REPORT_OR_COMMIT_URI");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_placeholder_uri).unwrap(),
    )
    .unwrap();
    let review_uri_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_uri_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_uri_failed, "/checks/report_uri_https"));

    let mut review_example_uri = external_review.clone();
    review_example_uri["report_uri"] = json!("https://audits.nervos.example.org/novaseal-bip340-tcb-review");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_example_uri).unwrap(),
    )
    .unwrap();
    let review_example_uri_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_example_uri_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_example_uri_failed, "/checks/report_uri_https"));

    let mut review_private_uri = external_review.clone();
    review_private_uri["report_uri"] = json!("https://192.168.1.1/novaseal-bip340-tcb-review");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_private_uri).unwrap(),
    )
    .unwrap();
    let review_private_uri_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_private_uri_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_private_uri_failed, "/checks/report_uri_https"));

    let mut review_extra = external_review.clone();
    review_extra["unexpected_provider_field"] = Value::String("must-fail".to_string());
    std::fs::write(proofs.join("bip340_external_tcb_review_attestation.json"), serde_json::to_vec_pretty(&review_extra).unwrap())
        .unwrap();
    let review_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_failed, "/checks/top_level_fields_exact"));

    let mut review_scope_string = external_review.clone();
    review_scope_string["review_scope"] = Value::String("BIP340 runtime verifier TCB".to_string());
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_scope_string).unwrap(),
    )
    .unwrap();
    let review_scope_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_scope_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_scope_failed, "/checks/review_scope_exact"));

    let mut review_scope_incomplete = external_review.clone();
    review_scope_incomplete["review_scope"] = json!(["BIP340 verifier core"]);
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_scope_incomplete).unwrap(),
    )
    .unwrap();
    let review_scope_incomplete_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_scope_incomplete_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_scope_incomplete_failed, "/checks/review_scope_exact"));

    let mut review_handoff_extra = external_review.clone();
    review_handoff_extra["request_handoff"]["unexpected_handoff_field"] = Value::String("must-fail".to_string());
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_handoff_extra).unwrap(),
    )
    .unwrap();
    let review_handoff_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_handoff_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_handoff_failed, "/checks/request_handoff_fields_exact"));

    let mut review_handoff_wrong_algorithm = external_review.clone();
    review_handoff_wrong_algorithm["request_handoff"]["bundle_hash_algorithm"] = json!("sha256");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.json"),
        serde_json::to_vec_pretty(&review_handoff_wrong_algorithm).unwrap(),
    )
    .unwrap();
    let review_handoff_algorithm_failed =
        validate_external_review(temp.path(), EXTERNAL_TCB_ATTESTATION, Some(&artifact_hash), Some(&source_tree_hash), &handoff)
            .unwrap();
    assert_eq!(json_pointer_str(&review_handoff_algorithm_failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&review_handoff_algorithm_failed, "/checks/request_handoff_bundle_hash_algorithm"));
}

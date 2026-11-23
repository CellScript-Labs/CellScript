#[test]
fn attestation_templates_must_match_current_tcb_hashes() {
    let temp = tempfile::tempdir().unwrap();
    let proofs = temp.path().join("proposals/novaseal/v0-mvp-skeleton/proofs");
    std::fs::create_dir_all(&proofs).unwrap();
    let rwa_root = temp.path().join(RWA_RECEIPT_ROOT);
    let rwa_src = rwa_root.join("src");
    let rwa_schemas = rwa_root.join("schemas");
    let rwa_fixtures = rwa_root.join("fixtures");
    let rwa_proofs = rwa_root.join("proofs");
    std::fs::create_dir_all(&rwa_src).unwrap();
    std::fs::create_dir_all(&rwa_schemas).unwrap();
    std::fs::create_dir_all(&rwa_fixtures).unwrap();
    std::fs::create_dir_all(&rwa_proofs).unwrap();
    std::fs::write(rwa_root.join("Cell.toml"), "profile = \"rwa-receipt-profile-v0\"\n").unwrap();
    std::fs::write(rwa_src.join("nova_rwa_receipt_type.cell"), "action materialize_rwa_receipt() {}\n").unwrap();
    std::fs::write(rwa_src.join("nova_rwa_receipt_lifecycle_type.cell"), "action nova_rwa_receipt_lifecycle() {}\n").unwrap();
    std::fs::write(rwa_schemas.join("nova_rwa_receipt_cell_v0.schema"), "cell: Byte32\n").unwrap();
    std::fs::write(rwa_fixtures.join("materialize_valid.json"), "{}\n").unwrap();
    std::fs::write(rwa_proofs.join("invariant_matrix.json"), "{}\n").unwrap();
    let artifact_hash = format!("0x{}", "aa".repeat(32));
    let tcb_source_tree_hash = format!("0x{}", "bb".repeat(32));
    let rwa_profile_source_hash = source_tree_hash(temp.path(), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
        .unwrap()
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
            "status": "attested",
            "network": "testnet",
            "attested_at": "YYYY-MM-DDTHH:MM:SSZ",
            "attestor": "REPLACE_WITH_DEPLOYER_OR_RELEASE_SIGNER",
            "release": {
                "package": "novaseal",
                "version": "0.0.1-v0-mvp",
                "manifest_commit": "0123456789abcdef0123456789abcdef01234567",
            },
            "notes": "template fixture",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": format!("0x{}", "11".repeat(32)),
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "public_shared_cell_dep_attestation",
            },
            "runtime_verifier": {
                "verifier_id": "btc.bip340.v0",
                "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                "artifact_hash": artifact_hash,
                "out_point": format!("0x{}:0", "22".repeat(32)),
                "data_hash": format!("0x{}", "33".repeat(32)),
                "dep_type": "code",
                "hash_type": "data1",
            },
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.template.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
            "status": "accepted",
            "verifier_id": "btc.bip340.v0",
            "ipc_abi": "cellscript-btc-bip340-ipc-v0",
            "artifact_hash": artifact_hash,
            "artifact_hash_algorithm": "sha256",
            "source_tree_sha256": tcb_source_tree_hash,
            "reviewer": "REPLACE_WITH_EXTERNAL_REVIEWER",
            "review_date": "YYYY-MM-DD",
            "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
            "report_uri": "REPLACE_WITH_EXTERNAL_REVIEW_REPORT_OR_COMMIT_URI",
            "notes": "template fixture",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": format!("0x{}", "44".repeat(32)),
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "external_bip340_tcb_review_attestation",
            },
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        proofs.join("public_btc_spv_evidence.template.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "novaseal-public-btc-spv-evidence-v0.1",
            "status": "template",
            "network": "testnet-or-mainnet",
            "generated_at": "YYYY-MM-DDTHH:MM:SSZ",
            "evidence_provider": "REPLACE_WITH_EXTERNAL_SPV_OPERATOR_OR_SERVICE",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": format!("0x{}", "55".repeat(32)),
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "public_btc_spv_evidence",
            },
            "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
            "cases": [],
            "notes": ["template fixture"],
        }))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        rwa_proofs.join("legal_registry_review_evidence.template.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": "novaseal-rwa-legal-registry-review-evidence-v0.1",
            "status": "accepted",
            "profile": EXPECTED_RWA_RECEIPT_PROFILE,
            "reviewer": "REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEWER",
            "review_date": "YYYY-MM-DD",
            "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
            "registry": {
                "authority": "REPLACE_WITH_REAL_REGISTRY_OR_CUSTODIAN_AUTHORITY",
                "jurisdiction": "REPLACE_WITH_REAL_WORLD_JURISDICTION",
                "registry_report_hash": format!("0x{}", "66".repeat(32)),
            },
            "profile_source_tree_sha256": rwa_profile_source_hash,
            "report_uri": "REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEW_REPORT_URI",
            "request_handoff": {
                "bundle": EXTERNAL_EVIDENCE_HANDOFF,
                "bundle_hash": format!("0x{}", "77".repeat(32)),
                "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
                "group": "rwa_legal_registry_review_evidence",
            },
            "notes": ["template fixture"],
        }))
        .unwrap(),
    )
    .unwrap();

    let passed =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    let failed = validate_attestation_templates(
        temp.path(),
        Some(&format!("0x{}", "cc".repeat(32))),
        Some("sha256"),
        Some(&tcb_source_tree_hash),
    )
    .unwrap();

    assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
    assert!(json_pointer_bool(&passed, "/checks/public_dep_type"));
    assert!(json_pointer_bool(&passed, "/checks/public_hash_type"));
    assert!(json_pointer_bool(&passed, "/checks/public_hash_type_matches_expected"));
    assert!(json_pointer_bool(&passed, "/checks/public_release_version_current"));
    assert!(json_pointer_bool(&passed, "/checks/public_request_handoff_bundle_path"));
    assert!(json_pointer_bool(&passed, "/checks/public_request_handoff_group"));
    assert!(json_pointer_bool(&passed, "/checks/external_request_handoff_bundle_path"));
    assert!(json_pointer_bool(&passed, "/checks/external_request_handoff_group"));
    assert!(json_pointer_bool(&passed, "/checks/btc_spv_request_handoff_bundle_path"));
    assert!(json_pointer_bool(&passed, "/checks/btc_spv_request_handoff_group"));
    assert!(json_pointer_bool(&passed, "/checks/rwa_legal_request_handoff_bundle_path"));
    assert!(json_pointer_bool(&passed, "/checks/rwa_legal_request_handoff_group"));
    assert!(json_pointer_bool(&passed, "/checks/rwa_legal_profile_source_tree_hash_matches_current"));
    assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed, "/checks/public_artifact_hash_matches_current_tcb"));
    assert!(!json_pointer_bool(&failed, "/checks/external_artifact_hash_matches_current_tcb"));

    let mut drifted_public_template =
        json_load_path(temp.path(), &proofs.join("public_shared_cell_dep_attestation.template.json")).unwrap();
    drifted_public_template["release"]["version"] = json!("0.0.2");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_release_version =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_release_version, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_release_version, "/checks/public_release_version_current"));

    drifted_public_template["release"]["version"] = json!(EXPECTED_NOVASEAL_RELEASE_VERSION);
    drifted_public_template["runtime_verifier"]["unexpected_template_field"] = Value::String("must-fail".to_string());
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_shape =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_shape, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_shape, "/checks/public_runtime_verifier_fields_exact"));

    drifted_public_template["runtime_verifier"].as_object_mut().unwrap().remove("unexpected_template_field");
    drifted_public_template["runtime_verifier"]["dep_type"] = json!("dep_group");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_dep_type =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_dep_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_dep_type, "/checks/public_dep_type"));

    drifted_public_template["runtime_verifier"]["dep_type"] = json!("code");
    drifted_public_template["runtime_verifier"]["hash_type"] = json!("type");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_stale_hash_type =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_hash_type, "/status"), Some("failed"));
    assert!(json_pointer_bool(&failed_stale_hash_type, "/checks/public_hash_type"));
    assert!(!json_pointer_bool(&failed_stale_hash_type, "/checks/public_hash_type_matches_expected"));

    drifted_public_template["runtime_verifier"]["hash_type"] = json!("invalid-hash-type");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_hash_type =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_hash_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_hash_type, "/checks/public_hash_type"));

    drifted_public_template["runtime_verifier"]["hash_type"] = json!("data1");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&drifted_public_template).unwrap(),
    )
    .unwrap();
    let failed_algorithm =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("ckb-blake2b256"), Some(&tcb_source_tree_hash))
            .unwrap();
    assert_eq!(json_pointer_str(&failed_algorithm, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_algorithm, "/checks/external_artifact_hash_algorithm_matches_current_tcb"));

    let mut public_wrong_handoff_bundle =
        json_load_path(temp.path(), &proofs.join("public_shared_cell_dep_attestation.template.json")).unwrap();
    public_wrong_handoff_bundle["request_handoff"]["bundle"] = json!("target/stale-handoff.json");
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&public_wrong_handoff_bundle).unwrap(),
    )
    .unwrap();
    let failed_public_handoff_bundle =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_public_handoff_bundle, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_public_handoff_bundle, "/checks/public_request_handoff_bundle_path"));

    public_wrong_handoff_bundle["request_handoff"]["bundle"] = json!(EXTERNAL_EVIDENCE_HANDOFF);
    std::fs::write(
        proofs.join("public_shared_cell_dep_attestation.template.json"),
        serde_json::to_vec_pretty(&public_wrong_handoff_bundle).unwrap(),
    )
    .unwrap();

    let mut external_wrong_handoff_group =
        json_load_path(temp.path(), &proofs.join("bip340_external_tcb_review_attestation.template.json")).unwrap();
    external_wrong_handoff_group["request_handoff"]["group"] = json!("public_shared_cell_dep_attestation");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.template.json"),
        serde_json::to_vec_pretty(&external_wrong_handoff_group).unwrap(),
    )
    .unwrap();
    let failed_external_handoff_group =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_external_handoff_group, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_external_handoff_group, "/checks/external_request_handoff_group"));

    external_wrong_handoff_group["request_handoff"]["group"] = json!("external_bip340_tcb_review_attestation");
    std::fs::write(
        proofs.join("bip340_external_tcb_review_attestation.template.json"),
        serde_json::to_vec_pretty(&external_wrong_handoff_group).unwrap(),
    )
    .unwrap();

    let mut btc_wrong_handoff_group = json_load_path(temp.path(), &proofs.join("public_btc_spv_evidence.template.json")).unwrap();
    btc_wrong_handoff_group["request_handoff"]["group"] = json!("external_bip340_tcb_review_attestation");
    std::fs::write(proofs.join("public_btc_spv_evidence.template.json"), serde_json::to_vec_pretty(&btc_wrong_handoff_group).unwrap())
        .unwrap();
    let failed_btc_handoff_group =
        validate_attestation_templates(temp.path(), Some(&artifact_hash), Some("sha256"), Some(&tcb_source_tree_hash)).unwrap();
    assert_eq!(json_pointer_str(&failed_btc_handoff_group, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_btc_handoff_group, "/checks/btc_spv_request_handoff_group"));
}

#[test]
fn rwa_legal_registry_review_requires_current_external_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let rwa_root = temp.path().join(RWA_RECEIPT_ROOT);
    let rwa_src = rwa_root.join("src");
    let rwa_schemas = rwa_root.join("schemas");
    let rwa_fixtures = rwa_root.join("fixtures");
    let rwa_proofs = rwa_root.join("proofs");
    std::fs::create_dir_all(&rwa_src).unwrap();
    std::fs::create_dir_all(&rwa_schemas).unwrap();
    std::fs::create_dir_all(&rwa_fixtures).unwrap();
    std::fs::create_dir_all(&rwa_proofs).unwrap();
    std::fs::write(rwa_root.join("Cell.toml"), format!("profile = \"{}\"\n", EXPECTED_RWA_RECEIPT_PROFILE)).unwrap();
    std::fs::write(rwa_src.join("nova_rwa_receipt_type.cell"), "action materialize_rwa_receipt() {}\n").unwrap();
    std::fs::write(rwa_src.join("nova_rwa_receipt_lifecycle_type.cell"), "action nova_rwa_receipt_lifecycle() {}\n").unwrap();
    std::fs::write(rwa_schemas.join("nova_rwa_receipt_cell_v0.schema"), "cell: Byte32\n").unwrap();
    std::fs::write(rwa_fixtures.join("materialize_valid.json"), "{}\n").unwrap();
    std::fs::write(rwa_proofs.join("invariant_matrix.json"), "{}\n").unwrap();

    let source_hash = source_tree_hash(temp.path(), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS).unwrap();
    let source_sha256 = json_pointer_str(&source_hash, "/sha256").unwrap().to_string();
    let handoff = json!({
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": "passed",
        "cases": [
            {
                "group": "rwa_legal_registry_review_evidence",
                "expected_values": {
                    "profile": EXPECTED_RWA_RECEIPT_PROFILE,
                    "profile_source_tree_sha256": source_sha256,
                    "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
                },
            },
        ],
    });
    let handoff_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &handoff);
    let evidence_path = temp.path().join(RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE);
    let evidence = json!({
        "schema": "novaseal-rwa-legal-registry-review-evidence-v0.1",
        "status": "accepted",
        "profile": EXPECTED_RWA_RECEIPT_PROFILE,
        "reviewer": "NervosRegistryReviewLtd",
        "review_date": "2026-06-04",
        "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
        "registry": {
            "authority": "NervosCustodyRegistry",
            "jurisdiction": "England-and-Wales",
            "registry_report_hash": format!("0x{}", "66".repeat(32)),
        },
        "profile_source_tree_sha256": source_sha256,
        "report_uri": "https://audits.nervos.org/novaseal-rwa-legal-registry-review",
        "request_handoff": {
            "bundle": EXTERNAL_EVIDENCE_HANDOFF,
            "bundle_hash": handoff_hash,
            "bundle_hash_algorithm": NOVASEAL_HANDOFF_HASH_ALGORITHM,
            "group": "rwa_legal_registry_review_evidence",
        },
        "notes": "external RWA legal registry review fixture",
    });
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();

    let passed = validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
    assert!(json_pointer_bool(&passed, "/checks/profile_source_tree_sha256_matches_current"));
    assert!(json_pointer_bool(&passed, "/checks/profile_source_tree_sha256_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/checks/review_scope_matches_handoff"));
    assert!(json_pointer_bool(&passed, "/checks/request_handoff_bundle_hash_matches_current"));

    let mut stale_handoff_scope = handoff.clone();
    stale_handoff_scope["cases"][0]["expected_values"]["review_scope"] = json!(["RWA receipt legal title boundary"]);
    let stale_handoff_scope_hash = novaseal_handoff_report_hash("external_evidence_handoff_bundle", &stale_handoff_scope);
    let mut stale_handoff_scope_evidence = evidence.clone();
    stale_handoff_scope_evidence["request_handoff"]["bundle_hash"] = json!(stale_handoff_scope_hash);
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_handoff_scope_evidence).unwrap()).unwrap();
    let failed_stale_handoff_scope =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &stale_handoff_scope).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_handoff_scope, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_handoff_scope, "/checks/review_scope_matches_handoff"));

    let mut stale_source = evidence.clone();
    stale_source["profile_source_tree_sha256"] = json!(format!("0x{}", "77".repeat(32)));
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_source).unwrap()).unwrap();
    let failed_stale_source = validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_source, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_source, "/checks/profile_source_tree_sha256_matches_current"));

    let mut placeholder_reviewer = evidence.clone();
    placeholder_reviewer["reviewer"] = json!("REPLACE_WITH_EXTERNAL_LEGAL_OR_REGISTRY_REVIEWER");
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&placeholder_reviewer).unwrap()).unwrap();
    let failed_placeholder_reviewer =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_placeholder_reviewer, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_placeholder_reviewer, "/checks/reviewer_identity"));

    let mut first_party_reviewer = evidence.clone();
    first_party_reviewer["reviewer"] = json!("NovaSeal Legal Review Team");
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&first_party_reviewer).unwrap()).unwrap();
    let failed_first_party_reviewer =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_first_party_reviewer, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_first_party_reviewer, "/checks/reviewer_identity"));

    let mut first_party_authority = evidence.clone();
    first_party_authority["registry"]["authority"] = json!("CellScript Custody Registry");
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&first_party_authority).unwrap()).unwrap();
    let failed_first_party_authority =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_first_party_authority, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_first_party_authority, "/checks/registry_authority_identity"));

    let mut local_jurisdiction = evidence.clone();
    local_jurisdiction["registry"]["jurisdiction"] = json!("local-devnet-registry");
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&local_jurisdiction).unwrap()).unwrap();
    let failed_local_jurisdiction =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_local_jurisdiction, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_local_jurisdiction, "/checks/registry_jurisdiction_present"));

    let mut placeholder_registry_hash = evidence.clone();
    placeholder_registry_hash["registry"]["registry_report_hash"] = json!(format!("0x{}", "00".repeat(32)));
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&placeholder_registry_hash).unwrap()).unwrap();
    let failed_placeholder_registry_hash =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_placeholder_registry_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_placeholder_registry_hash, "/checks/registry_report_hash_non_placeholder"));

    let mut private_report_uri = evidence.clone();
    private_report_uri["report_uri"] = json!("https://10.0.0.7/novaseal-rwa-legal-registry-review");
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&private_report_uri).unwrap()).unwrap();
    let failed_private_report_uri =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_private_report_uri, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_private_report_uri, "/checks/report_uri_https"));

    let mut stale_handoff_hash = evidence.clone();
    stale_handoff_hash["request_handoff"]["bundle_hash"] = json!(format!("0x{}", "88".repeat(32)));
    std::fs::write(&evidence_path, serde_json::to_vec_pretty(&stale_handoff_hash).unwrap()).unwrap();
    let failed_stale_handoff_hash =
        validate_rwa_legal_registry_review(temp.path(), RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE, &handoff).unwrap();
    assert_eq!(json_pointer_str(&failed_stale_handoff_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_handoff_hash, "/checks/request_handoff_bundle_hash_matches_current"));
}

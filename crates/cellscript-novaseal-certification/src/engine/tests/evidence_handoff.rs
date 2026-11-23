#[test]
fn external_evidence_handoff_rejects_stale_source_hashes_and_paths() {
    let btc_handoff_fields = EXPECTED_PUBLIC_BTC_SPV_HANDOFF_FIELDS;
    let public_attestation_handoff_fields = EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS;
    let external_review_handoff_fields = EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS;
    let rwa_legal_review_handoff_fields = EXPECTED_RWA_LEGAL_REVIEW_REQUIRED_FIELDS;
    let public_manifest_commit = "0123456789abcdef0123456789abcdef01234567";
    let public_release_package = "novaseal";
    let public_release_version = EXPECTED_NOVASEAL_RELEASE_VERSION;
    let public_dep_type = EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE;
    let public_hash_type = EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE;
    let public_ipc_abi = "cellscript-btc-bip340-ipc-v0";
    let public_verifier_id = "btc.bip340.v0";
    let public_artifact_hash = format!("0x{}", "99".repeat(32));
    let external_artifact_hash = format!("0x{}", "aa".repeat(32));
    let external_source_tree_hash = format!("0x{}", "bb".repeat(32));
    let rwa_profile_source_hash = source_tree_hash(Path::new("."), RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
        .unwrap()
        .get("sha256")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let expected_btc_scenarios = EXPECTED_BTC_SPV_PROFILE_SCENARIOS
        .iter()
        .map(|(profile, scenario)| ((*profile).to_string(), Value::String((*scenario).to_string())))
        .collect::<Map<String, Value>>();
    let expected_btc_bindings = EXPECTED_BTC_SPV_EVIDENCE_PROFILES
        .iter()
        .enumerate()
        .map(|(index, profile)| {
            let byte = index as u8 + 0x50;
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
        .collect::<Map<String, Value>>();
    let btc_spv_adapter = json!({
        "status": "passed",
        "adapter_status": "request_ready_external_evidence_required",
        "production_output": PUBLIC_BTC_SPV_EVIDENCE,
        "summary": { "total": 3, "matched": 3 },
        "cases": EXPECTED_BTC_SPV_EVIDENCE_PROFILES
            .iter()
            .enumerate()
            .map(|(index, profile)| {
                let profile = *profile;
                let bindings = expected_btc_bindings.get(profile).unwrap();
                let mut request = json!({
                    "scenario": expected_btc_spv_scenario(profile).unwrap(),
                    "ckb_live_tx_hash": json_pointer_str(bindings, "/ckb_live_tx_hash").unwrap(),
                    "live_report_hash": json_pointer_str(bindings, "/live_report_hash").unwrap(),
                    "service_builder_case_hash": json_pointer_str(bindings, "/service_builder_case_hash").unwrap(),
                    "service_builder_tx_skeleton_hash": json_pointer_str(bindings, "/service_builder_tx_skeleton_hash").unwrap(),
                    "service_builder_receipt_binding_hash": json_pointer_str(bindings, "/service_builder_receipt_binding_hash").unwrap(),
                    "ckb_btc_commitment_hash": json_pointer_str(bindings, "/ckb_btc_commitment_hash").unwrap(),
                });
                let btc_material = test_btc_profile_material(profile, index as u8 + 0x50);
                add_expected_btc_request_fields(&mut request, &btc_material);
                json!({
                    "profile": profile,
                    "status": "passed",
                    "request": request,
                })
            })
            .collect::<Vec<_>>(),
    });
    let external_attestation_adapter = json!({
        "status": "passed",
        "adapter_status": "request_ready_external_attestations_required",
        "summary": { "total": 2, "matched": 2 },
        "cases": [
            {
                "name": "public_shared_cell_dep_attestation",
                "status": "passed",
                "request": {
                    "production_output": PUBLIC_CELLDEP_ATTESTATION,
                    "required_public_fields": ["network"],
                    "expected_artifact_hash": public_artifact_hash,
                    "expected_release_package": public_release_package,
                    "expected_release_version": public_release_version,
                    "expected_release_manifest_commit": public_manifest_commit,
                    "expected_dep_type": public_dep_type,
                    "expected_hash_type": public_hash_type,
                    "ipc_abi": public_ipc_abi,
                    "verifier_id": public_verifier_id,
                },
            },
            {
                "name": "external_bip340_tcb_review_attestation",
                "status": "passed",
                "request": {
                    "production_output": EXTERNAL_TCB_ATTESTATION,
                    "required_public_fields": ["reviewer"],
                    "expected_artifact_hash": external_artifact_hash,
                    "expected_artifact_hash_algorithm": "sha256",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "verifier_id": "btc.bip340.v0",
                    "expected_source_tree_sha256": external_source_tree_hash,
                    "expected_review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                },
            },
        ],
    });
    let btc_hash = novaseal_handoff_report_hash("btc_spv_adapter", &btc_spv_adapter);
    let attestation_hash = novaseal_handoff_report_hash("external_attestation_adapter", &external_attestation_adapter);
    let mut report = json!({
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": "passed",
        "handoff_status": "request_bundle_ready_external_evidence_required",
        "source_btc_spv_adapter": BTC_SPV_EVIDENCE_ADAPTER,
        "source_btc_spv_adapter_hash": btc_hash,
        "source_external_attestation_adapter": EXTERNAL_ATTESTATION_ADAPTER,
        "source_external_attestation_adapter_hash": attestation_hash,
        "production_outputs": [
            PUBLIC_BTC_SPV_EVIDENCE,
            PUBLIC_CELLDEP_ATTESTATION,
            EXTERNAL_TCB_ATTESTATION,
            RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
        ],
        "summary": {
            "total": 4,
            "matched": 4,
        },
        "cases": [
            {
                "group": "public_btc_spv_evidence",
                "status": "passed",
                "source_adapter": BTC_SPV_EVIDENCE_ADAPTER,
                "source_adapter_hash": btc_hash,
                "production_output": PUBLIC_BTC_SPV_EVIDENCE,
                "required_profiles": EXPECTED_BTC_SPV_EVIDENCE_PROFILES,
                "expected_scenarios": expected_btc_scenarios.clone(),
                "expected_case_bindings": expected_btc_bindings.clone(),
                "required_external_fields": btc_handoff_fields,
                "field_constraints": constraint_object(EXPECTED_BTC_SPV_FIELD_CONSTRAINTS),
                "checks": { "ok": true },
            },
            {
                "group": "public_shared_cell_dep_attestation",
                "status": "passed",
                "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                "source_adapter_hash": attestation_hash,
                "production_output": PUBLIC_CELLDEP_ATTESTATION,
                "required_external_fields": public_attestation_handoff_fields,
                "field_constraints": constraint_object(EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS),
                "expected_values": {
                    "artifact_hash": public_artifact_hash,
                    "release.package": public_release_package,
                    "release.version": public_release_version,
                    "release.manifest_commit": public_manifest_commit,
                    "runtime_verifier.dep_type": public_dep_type,
                    "runtime_verifier.hash_type": public_hash_type,
                    "runtime_verifier.ipc_abi": public_ipc_abi,
                    "runtime_verifier.verifier_id": public_verifier_id,
                },
                "checks": { "ok": true },
            },
            {
                "group": "external_bip340_tcb_review_attestation",
                "status": "passed",
                "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                "source_adapter_hash": attestation_hash,
                "production_output": EXTERNAL_TCB_ATTESTATION,
                "required_external_fields": external_review_handoff_fields,
                "field_constraints": constraint_object(EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS),
                "expected_values": {
                    "artifact_hash": external_artifact_hash,
                    "artifact_hash_algorithm": "sha256",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                    "source_tree_sha256": external_source_tree_hash,
                    "verifier_id": "btc.bip340.v0",
                },
                "checks": { "ok": true },
            },
            {
                "group": "rwa_legal_registry_review_evidence",
                "status": "passed",
                "source_adapter": EXTERNAL_ATTESTATION_ADAPTER,
                "source_adapter_hash": attestation_hash,
                "production_output": RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
                "required_external_fields": rwa_legal_review_handoff_fields,
                "field_constraints": constraint_object(EXPECTED_RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS),
                "expected_values": {
                    "profile": EXPECTED_RWA_RECEIPT_PROFILE,
                    "profile_source_tree_sha256": rwa_profile_source_hash,
                    "review_scope": EXPECTED_RWA_LEGAL_REVIEW_SCOPE,
                },
                "checks": { "ok": true },
            },
        ],
    });
    report["bundle_hash_algorithm"] = json!(NOVASEAL_HANDOFF_HASH_ALGORITHM);
    report["bundle_hash"] = json!(external_evidence_handoff_reference_hash(&report));

    let valid = validate_external_evidence_handoff_detail(Path::new("."), &report, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
    assert!(external_evidence_handoff_gate_passed(&report));
    assert!(json_pointer_bool(&valid, "/checks/bundle_hash_matches_reference"));
    assert!(json_pointer_bool(&valid, "/checks/bundle_hash_algorithm"));
    assert!(json_pointer_bool(&valid, "/cases/public_btc_spv_evidence/expected_scenarios_match_source_adapter"));
    assert!(json_pointer_bool(&valid, "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));
    assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"));
    assert!(json_pointer_bool(&valid, "/cases/rwa_legal_registry_review_evidence/expected_values_match_source_adapter"));

    let mut wrong_btc_anchor_source = report.clone();
    wrong_btc_anchor_source["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["anchor_source"] =
        json!("external_public_btc_spend");
    let failed_btc_anchor_source = validate_external_evidence_handoff_detail(
        Path::new("."),
        &wrong_btc_anchor_source,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_btc_anchor_source, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_btc_anchor_source, "/cases/public_btc_spv_evidence/expected_case_bindings_exact"));
    assert!(!json_pointer_bool(
        &failed_btc_anchor_source,
        "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"
    ));

    let mut zero_btc_amount = report.clone();
    zero_btc_amount["cases"][0]["expected_case_bindings"][EXPECTED_BTC_TX_COMMITMENT_PROFILE]["btc_amount_sats"] = json!(0);
    let failed_zero_btc_amount =
        validate_external_evidence_handoff_detail(Path::new("."), &zero_btc_amount, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_zero_btc_amount, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_zero_btc_amount, "/cases/public_btc_spv_evidence/expected_case_bindings_exact"));
    assert!(!json_pointer_bool(&failed_zero_btc_amount, "/cases/public_btc_spv_evidence/expected_case_bindings_match_source_adapter"));

    let mut stale_bundle_hash = report.clone();
    stale_bundle_hash["bundle_hash"] = json!(format!("0x{}", "22".repeat(32)));
    let stale_bundle =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_bundle_hash, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&stale_bundle, "/status"), Some("failed"));
    assert!(!external_evidence_handoff_gate_passed(&stale_bundle_hash));
    assert!(!json_pointer_bool(&stale_bundle, "/checks/bundle_hash_matches_reference"));

    let mut wrong_bundle_algorithm = report.clone();
    wrong_bundle_algorithm["bundle_hash_algorithm"] = json!("sha256");
    let wrong_algorithm = validate_external_evidence_handoff_detail(
        Path::new("."),
        &wrong_bundle_algorithm,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&wrong_algorithm, "/status"), Some("failed"));
    assert!(!external_evidence_handoff_gate_passed(&wrong_bundle_algorithm));
    assert!(!json_pointer_bool(&wrong_algorithm, "/checks/bundle_hash_algorithm"));

    let mut stale_hash = report.clone();
    stale_hash["source_btc_spv_adapter_hash"] = json!(format!("0x{}", "11".repeat(32)));
    let stale =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_hash, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&stale, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&stale, "/checks/source_btc_spv_adapter_hash_matches_current"));

    let mut wrong_path = report.clone();
    wrong_path["cases"][1]["source_adapter"] = json!("target/other-report.json");
    let failed_path =
        validate_external_evidence_handoff_detail(Path::new("."), &wrong_path, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_path, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_path, "/cases/public_shared_cell_dep_attestation/source_adapter_path_matches_current"));

    let mut missing_required_field = report.clone();
    missing_required_field["cases"][0]["required_external_fields"] = json!(btc_handoff_fields[..20].to_vec());
    let failed_fields = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_required_field,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_fields, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_fields, "/cases/public_btc_spv_evidence/required_external_fields_complete"));

    let mut missing_constraint = report.clone();
    missing_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("source_service.commit");
    let failed_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

    let mut missing_report_hash_constraint = report.clone();
    missing_report_hash_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("source_service.report_hash");
    let failed_report_hash_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_report_hash_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_report_hash_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_report_hash_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

    let mut missing_celldep_out_point_constraint = report.clone();
    missing_celldep_out_point_constraint["cases"][0]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("spv_client_cell_dep.out_point");
    let failed_celldep_out_point_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_celldep_out_point_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_celldep_out_point_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_celldep_out_point_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

    let mut missing_btc_txid_constraint = report.clone();
    missing_btc_txid_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("btc_txid");
    let failed_btc_txid_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_btc_txid_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_btc_txid_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_btc_txid_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

    let mut missing_handoff_group_constraint = report.clone();
    missing_handoff_group_constraint["cases"][0]["field_constraints"].as_object_mut().unwrap().remove("request_handoff.group");
    let failed_handoff_group_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_handoff_group_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_handoff_group_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_handoff_group_constraint, "/cases/public_btc_spv_evidence/field_constraints_exact"));

    let mut missing_public_artifact_constraint = report.clone();
    missing_public_artifact_constraint["cases"][1]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("runtime_verifier.artifact_hash");
    let failed_public_artifact_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_public_artifact_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_public_artifact_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_public_artifact_constraint,
        "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
    ));

    let mut missing_external_source_constraint = report.clone();
    missing_external_source_constraint["cases"][2]["field_constraints"].as_object_mut().unwrap().remove("source_tree_sha256");
    let failed_external_source_constraint = validate_external_evidence_handoff_detail(
        Path::new("."),
        &missing_external_source_constraint,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_external_source_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_external_source_constraint,
        "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
    ));

    let mut stale_expected_scenario = report.clone();
    stale_expected_scenario["cases"][0]["expected_scenarios"][EXPECTED_BTC_TX_COMMITMENT_PROFILE] = json!("generic-public-btc-proof");
    let failed_expected_scenario = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_expected_scenario,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_expected_scenario, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_expected_scenario, "/cases/public_btc_spv_evidence/expected_scenarios_match_source_adapter"));

    let mut stale_expected_value = report.clone();
    stale_expected_value["cases"][1]["expected_values"]["release.manifest_commit"] = json!("fedcba9876543210fedcba9876543210fedcba98");
    let failed_expected_value = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_expected_value,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_expected_value, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_expected_value,
        "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
    ));

    let mut stale_release_package = report.clone();
    stale_release_package["cases"][1]["expected_values"]["release.package"] = json!("other-package");
    let failed_release_package = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_release_package,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_release_package, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_release_package,
        "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
    ));

    let mut stale_release_version = report.clone();
    stale_release_version["cases"][1]["expected_values"]["release.version"] = json!("0.0.2");
    let failed_release_version = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_release_version,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_release_version, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_release_version,
        "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"
    ));

    let mut stale_hash_type = report.clone();
    stale_hash_type["cases"][1]["expected_values"]["runtime_verifier.hash_type"] = json!("type");
    let failed_hash_type =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_hash_type, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_hash_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_hash_type, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));

    let mut stale_dep_type = report.clone();
    stale_dep_type["cases"][1]["expected_values"]["runtime_verifier.dep_type"] = json!("dep_group");
    let failed_dep_type =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_dep_type, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_dep_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_dep_type, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));

    let mut stale_ipc_abi = report.clone();
    stale_ipc_abi["cases"][1]["expected_values"]["runtime_verifier.ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
    let failed_ipc_abi =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_ipc_abi, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_ipc_abi, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_ipc_abi, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));

    let mut stale_verifier_id = report.clone();
    stale_verifier_id["cases"][1]["expected_values"]["runtime_verifier.verifier_id"] = json!("btc.bip340.v1");
    let failed_verifier_id =
        validate_external_evidence_handoff_detail(Path::new("."), &stale_verifier_id, &btc_spv_adapter, &external_attestation_adapter);
    assert_eq!(json_pointer_str(&failed_verifier_id, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_verifier_id, "/cases/public_shared_cell_dep_attestation/expected_values_match_source_adapter"));

    let mut stale_expected_review_scope = report.clone();
    stale_expected_review_scope["cases"][2]["expected_values"]["review_scope"] = json!(["BIP340 runtime verifier TCB"]);
    let failed_review_scope = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_expected_review_scope,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_review_scope, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_review_scope,
        "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
    ));

    let mut stale_external_ipc_abi = report.clone();
    stale_external_ipc_abi["cases"][2]["expected_values"]["ipc_abi"] = json!("cellscript-btc-bip340-ipc-v1");
    let failed_external_ipc_abi = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_external_ipc_abi,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_external_ipc_abi, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_external_ipc_abi,
        "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
    ));

    let mut stale_external_verifier_id = report.clone();
    stale_external_verifier_id["cases"][2]["expected_values"]["verifier_id"] = json!("btc.bip340.v1");
    let failed_external_verifier_id = validate_external_evidence_handoff_detail(
        Path::new("."),
        &stale_external_verifier_id,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_external_verifier_id, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_external_verifier_id,
        "/cases/external_bip340_tcb_review_attestation/expected_values_match_source_adapter"
    ));

    let mut unexpected_required_field = report;
    let mut extended_btc_fields = btc_handoff_fields.to_vec();
    extended_btc_fields.push("unexpected.shadow_field");
    unexpected_required_field["cases"][0]["required_external_fields"] = json!(extended_btc_fields);
    let failed_exact = validate_external_evidence_handoff_detail(
        Path::new("."),
        &unexpected_required_field,
        &btc_spv_adapter,
        &external_attestation_adapter,
    );
    assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_exact, "/cases/public_btc_spv_evidence/required_external_fields_exact"));
}

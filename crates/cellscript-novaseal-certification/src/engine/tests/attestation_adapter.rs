#[test]
fn external_attestation_adapter_requires_handoff_request_fields() {
    let full_public_fields = EXPECTED_PUBLIC_CELLDEP_REQUIRED_FIELDS;
    let full_review_fields = EXPECTED_EXTERNAL_TCB_REQUIRED_FIELDS;
    let public_manifest_commit = "0123456789abcdef0123456789abcdef01234567";
    let public_release_package = "novaseal";
    let public_release_version = EXPECTED_NOVASEAL_RELEASE_VERSION;
    let public_dep_type = EXPECTED_NOVASEAL_CELLDEP_DEP_TYPE;
    let public_hash_type = EXPECTED_NOVASEAL_CELLDEP_HASH_TYPE;
    let report = json!({
        "schema": "novaseal-external-attestation-adapter-v0.1",
        "status": "passed",
        "adapter_status": "request_ready_external_attestations_required",
        "source_tcb_review_hash": format!("0x{}", "aa".repeat(32)),
        "source_public_cell_dep_template_hash": format!("0x{}", "bb".repeat(32)),
        "source_external_tcb_template_hash": format!("0x{}", "cc".repeat(32)),
        "summary": { "total": 2, "matched": 2 },
        "cases": [
            {
                "name": "public_shared_cell_dep_attestation",
                "status": "passed",
                "checks": { "ok": true },
                "request": {
                    "production_output": PUBLIC_CELLDEP_ATTESTATION,
                    "template_schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
                    "template_hash": format!("0x{}", "dd".repeat(32)),
                    "verifier_id": "btc.bip340.v0",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "required_status": "attested",
                    "expected_artifact_hash": format!("0x{}", "ee".repeat(32)),
                    "expected_release_package": public_release_package,
                    "expected_release_version": public_release_version,
                    "expected_release_manifest_commit": public_manifest_commit,
                    "expected_dep_type": public_dep_type,
                    "expected_hash_type": public_hash_type,
                    "required_public_fields": full_public_fields,
                    "field_constraints": constraint_object(EXPECTED_PUBLIC_CELLDEP_FIELD_CONSTRAINTS),
                },
            },
            {
                "name": "external_bip340_tcb_review_attestation",
                "status": "passed",
                "checks": { "ok": true },
                "request": {
                    "production_output": EXTERNAL_TCB_ATTESTATION,
                    "template_schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
                    "template_hash": format!("0x{}", "ff".repeat(32)),
                    "verifier_id": "btc.bip340.v0",
                    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
                    "required_status": "accepted",
                    "expected_artifact_hash": format!("0x{}", "11".repeat(32)),
                    "expected_artifact_hash_algorithm": "sha256",
                    "template_artifact_hash_algorithm": "sha256",
                    "expected_review_scope": EXPECTED_EXTERNAL_TCB_REVIEW_SCOPE,
                    "required_public_fields": full_review_fields,
                    "field_constraints": constraint_object(EXPECTED_EXTERNAL_TCB_FIELD_CONSTRAINTS),
                },
            },
        ],
    });

    let valid = validate_external_attestation_adapter_detail(&report);
    assert_eq!(json_pointer_str(&valid, "/status"), Some("passed"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/field_constraints_exact"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_package_current"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_version_current"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_dep_type_current"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_hash_type_current"));
    assert!(json_pointer_bool(&valid, "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_present"));
    assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"));
    assert!(json_pointer_bool(&valid, "/cases/external_bip340_tcb_review_attestation/expected_review_scope_exact"));

    let mut missing_handoff_field = report.clone();
    missing_handoff_field["cases"][0]["request"]["required_public_fields"] = json!(full_public_fields[..15].to_vec());
    let failed = validate_external_attestation_adapter_detail(&missing_handoff_field);
    assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed, "/cases/public_shared_cell_dep_attestation/required_fields_complete"));

    let mut mismatched_algorithm = report.clone();
    mismatched_algorithm["cases"][1]["request"]["template_artifact_hash_algorithm"] = json!("ckb-blake2b256");
    let failed_algorithm = validate_external_attestation_adapter_detail(&mismatched_algorithm);
    assert_eq!(json_pointer_str(&failed_algorithm, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_algorithm,
        "/cases/external_bip340_tcb_review_attestation/artifact_hash_algorithm_matches_tcb"
    ));

    let mut stale_constraint = report.clone();
    stale_constraint["cases"][1]["request"]["field_constraints"]["report_uri"] = json!("any URI");
    let failed_constraint = validate_external_attestation_adapter_detail(&stale_constraint);
    assert_eq!(json_pointer_str(&failed_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_constraint, "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"));

    let mut missing_public_handoff_hash_constraint = report.clone();
    missing_public_handoff_hash_constraint["cases"][0]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("request_handoff.bundle_hash");
    let failed_public_handoff_hash_constraint = validate_external_attestation_adapter_detail(&missing_public_handoff_hash_constraint);
    assert_eq!(json_pointer_str(&failed_public_handoff_hash_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_public_handoff_hash_constraint,
        "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
    ));

    let mut missing_public_artifact_constraint = report.clone();
    missing_public_artifact_constraint["cases"][0]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("runtime_verifier.artifact_hash");
    let failed_public_artifact_constraint = validate_external_attestation_adapter_detail(&missing_public_artifact_constraint);
    assert_eq!(json_pointer_str(&failed_public_artifact_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_public_artifact_constraint,
        "/cases/public_shared_cell_dep_attestation/field_constraints_exact"
    ));

    let mut missing_tcb_handoff_group_constraint = report.clone();
    missing_tcb_handoff_group_constraint["cases"][1]["request"]["field_constraints"]
        .as_object_mut()
        .unwrap()
        .remove("request_handoff.group");
    let failed_tcb_handoff_group_constraint = validate_external_attestation_adapter_detail(&missing_tcb_handoff_group_constraint);
    assert_eq!(json_pointer_str(&failed_tcb_handoff_group_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_tcb_handoff_group_constraint,
        "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
    ));

    let mut missing_tcb_source_constraint = report.clone();
    missing_tcb_source_constraint["cases"][1]["request"]["field_constraints"].as_object_mut().unwrap().remove("source_tree_sha256");
    let failed_tcb_source_constraint = validate_external_attestation_adapter_detail(&missing_tcb_source_constraint);
    assert_eq!(json_pointer_str(&failed_tcb_source_constraint, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_tcb_source_constraint,
        "/cases/external_bip340_tcb_review_attestation/field_constraints_exact"
    ));

    let mut stale_review_scope = report.clone();
    stale_review_scope["cases"][1]["request"]["expected_review_scope"] = json!(["BIP340 runtime verifier TCB"]);
    let failed_review_scope = validate_external_attestation_adapter_detail(&stale_review_scope);
    assert_eq!(json_pointer_str(&failed_review_scope, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_review_scope, "/cases/external_bip340_tcb_review_attestation/expected_review_scope_exact"));

    let mut missing_expected_commit = report.clone();
    missing_expected_commit["cases"][0]["request"]["expected_release_manifest_commit"] = json!("REPLACE_WITH_GIT_COMMIT");
    let failed_expected_commit = validate_external_attestation_adapter_detail(&missing_expected_commit);
    assert_eq!(json_pointer_str(&failed_expected_commit, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_expected_commit,
        "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_present"
    ));

    let mut stale_expected_version = report.clone();
    stale_expected_version["cases"][0]["request"]["expected_release_version"] = json!("0.0.2");
    let failed_expected_version = validate_external_attestation_adapter_detail(&stale_expected_version);
    assert_eq!(json_pointer_str(&failed_expected_version, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_expected_version,
        "/cases/public_shared_cell_dep_attestation/expected_release_version_current"
    ));

    let mut stale_expected_package = report.clone();
    stale_expected_package["cases"][0]["request"]["expected_release_package"] = json!("other-package");
    let failed_expected_package = validate_external_attestation_adapter_detail(&stale_expected_package);
    assert_eq!(json_pointer_str(&failed_expected_package, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_expected_package,
        "/cases/public_shared_cell_dep_attestation/expected_release_package_current"
    ));

    let mut stale_expected_hash_type = report.clone();
    stale_expected_hash_type["cases"][0]["request"]["expected_hash_type"] = json!("type");
    let failed_expected_hash_type = validate_external_attestation_adapter_detail(&stale_expected_hash_type);
    assert_eq!(json_pointer_str(&failed_expected_hash_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_expected_hash_type, "/cases/public_shared_cell_dep_attestation/expected_hash_type_current"));

    let mut stale_expected_dep_type = report.clone();
    stale_expected_dep_type["cases"][0]["request"]["expected_dep_type"] = json!("dep_group");
    let failed_expected_dep_type = validate_external_attestation_adapter_detail(&stale_expected_dep_type);
    assert_eq!(json_pointer_str(&failed_expected_dep_type, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_expected_dep_type, "/cases/public_shared_cell_dep_attestation/expected_dep_type_current"));

    let tcb_review = json!({
        "repo_commit": public_manifest_commit,
        "runtime_artifact": {
            "artifact_hash": format!("0x{}", "11".repeat(32)),
            "artifact_hash_algorithm": "sha256",
        },
        "source_inventory": {
            "source_tree_sha256": format!("0x{}", "22".repeat(32)),
        },
    });
    let public_template = json!({
        "schema": "novaseal-public-shared-cell-dep-attestation-v0.1",
        "status": "attested",
    });
    let external_template = json!({
        "schema": "novaseal-bip340-external-tcb-review-attestation-v0.1",
        "status": "accepted",
    });
    let mut source_bound = report.clone();
    source_bound["source_tcb_review_hash"] = json!(novaseal_external_attestation_report_hash("tcb_review", &tcb_review));
    source_bound["source_public_cell_dep_template_hash"] =
        json!(novaseal_external_attestation_report_hash("public_celldep_template", &public_template));
    source_bound["source_external_tcb_template_hash"] =
        json!(novaseal_external_attestation_report_hash("external_tcb_template", &external_template));
    source_bound["cases"][0]["request"]["template_hash"] =
        json!(novaseal_external_attestation_report_hash("public_celldep_template", &public_template));
    source_bound["cases"][1]["request"]["template_hash"] =
        json!(novaseal_external_attestation_report_hash("external_tcb_template", &external_template));
    source_bound["cases"][0]["request"]["expected_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
    source_bound["cases"][0]["request"]["template_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
    source_bound["cases"][0]["request"]["expected_release_manifest_commit"] = tcb_review["repo_commit"].clone();
    source_bound["cases"][1]["request"]["expected_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
    source_bound["cases"][1]["request"]["template_artifact_hash"] = tcb_review["runtime_artifact"]["artifact_hash"].clone();
    source_bound["cases"][1]["request"]["expected_source_tree_sha256"] = tcb_review["source_inventory"]["source_tree_sha256"].clone();
    source_bound["cases"][1]["request"]["template_source_tree_sha256"] = tcb_review["source_inventory"]["source_tree_sha256"].clone();
    let source_bound_valid = validate_external_attestation_adapter_detail_with_sources(
        &source_bound,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&source_bound_valid, "/status"), Some("passed"));
    assert!(json_pointer_bool(&source_bound_valid, "/checks/source_tcb_review_hash_matches_current_report"));
    assert!(json_pointer_bool(
        &source_bound_valid,
        "/cases/public_shared_cell_dep_attestation/expected_artifact_hash_matches_current_tcb"
    ));
    assert!(json_pointer_bool(
        &source_bound_valid,
        "/cases/public_shared_cell_dep_attestation/template_artifact_hash_matches_current_tcb"
    ));
    assert!(json_pointer_bool(
        &source_bound_valid,
        "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_matches_current_tcb"
    ));
    assert!(json_pointer_bool(
        &source_bound_valid,
        "/cases/external_bip340_tcb_review_attestation/expected_source_tree_sha256_matches_current_tcb"
    ));

    let mut stale_source_hash = source_bound.clone();
    stale_source_hash["source_tcb_review_hash"] = json!(test_hex32(0xf1));
    let failed_stale_source_hash = validate_external_attestation_adapter_detail_with_sources(
        &stale_source_hash,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&failed_stale_source_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_stale_source_hash, "/checks/source_tcb_review_hash_matches_current_report"));

    let mut stale_template_hash = source_bound.clone();
    stale_template_hash["cases"][0]["request"]["template_hash"] = json!(test_hex32(0xf2));
    let failed_stale_template_hash = validate_external_attestation_adapter_detail_with_sources(
        &stale_template_hash,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&failed_stale_template_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_template_hash,
        "/cases/public_shared_cell_dep_attestation/template_hash_matches_current_template"
    ));

    let mut stale_expected_artifact_hash = source_bound.clone();
    stale_expected_artifact_hash["cases"][0]["request"]["expected_artifact_hash"] = json!(test_hex32(0xf3));
    let failed_stale_expected_artifact_hash = validate_external_attestation_adapter_detail_with_sources(
        &stale_expected_artifact_hash,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&failed_stale_expected_artifact_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_expected_artifact_hash,
        "/cases/public_shared_cell_dep_attestation/expected_artifact_hash_matches_current_tcb"
    ));

    let mut stale_manifest_commit = source_bound.clone();
    stale_manifest_commit["cases"][0]["request"]["expected_release_manifest_commit"] =
        json!("fedcba9876543210fedcba9876543210fedcba98");
    let failed_stale_manifest_commit = validate_external_attestation_adapter_detail_with_sources(
        &stale_manifest_commit,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&failed_stale_manifest_commit, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_manifest_commit,
        "/cases/public_shared_cell_dep_attestation/expected_release_manifest_commit_matches_current_tcb"
    ));

    let mut stale_expected_source_tree = source_bound.clone();
    stale_expected_source_tree["cases"][1]["request"]["expected_source_tree_sha256"] = json!(test_hex32(0xf4));
    let failed_stale_expected_source_tree = validate_external_attestation_adapter_detail_with_sources(
        &stale_expected_source_tree,
        Some(&tcb_review),
        Some(&public_template),
        Some(&external_template),
    );
    assert_eq!(json_pointer_str(&failed_stale_expected_source_tree, "/status"), Some("failed"));
    assert!(!json_pointer_bool(
        &failed_stale_expected_source_tree,
        "/cases/external_bip340_tcb_review_attestation/expected_source_tree_sha256_matches_current_tcb"
    ));

    let mut unexpected_public_field = report;
    let mut extended_public_fields = full_public_fields.to_vec();
    extended_public_fields.push("unexpected.shadow_field");
    unexpected_public_field["cases"][0]["request"]["required_public_fields"] = json!(extended_public_fields);
    let failed_exact = validate_external_attestation_adapter_detail(&unexpected_public_field);
    assert_eq!(json_pointer_str(&failed_exact, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_exact, "/cases/public_shared_cell_dep_attestation/required_fields_exact"));
}

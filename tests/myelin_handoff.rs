use serde_json::Value;

fn contract() -> Value {
    serde_json::from_str(include_str!("../integrations/myelin/cellscript-handoff-contract.json")).unwrap()
}

#[test]
fn myelin_handoff_is_ckb_only_versioned_and_rejects_raw_witness_aliases() {
    let value = contract();
    assert_eq!(value["schema"], "cellscript-myelin-handoff-contract-v1");
    assert_eq!(value["compiler"]["package_version"], cellscript::VERSION);
    assert_eq!(value["compiler"]["edition"], "2027");
    assert_eq!(value["compiler"]["target_profile"], "ckb");
    assert_eq!(value["compatibility_profile"]["metadata_schema_version"], cellscript::METADATA_SCHEMA_VERSION);
    assert_eq!(value["compatibility_profile"]["entry_witness_placement_field"], "input_type");
    assert_eq!(value["compatibility_profile"]["raw_entry_witness_payload_compatible"], false);
    assert_eq!(value["allow_legacy_fallback"], false);
    assert_eq!(value["verified_artifact"]["semantic_equivalence_claimed"], false);

    let forbidden = value["forbidden_cellscript_profiles"].as_array().unwrap();
    assert!(forbidden.iter().any(|profile| profile == "MyelinExtended"));
    assert!(!forbidden.iter().any(|profile| profile == "ckb"));
}

#[test]
fn myelin_adoption_requires_all_artifact_checker_and_source_bindings() {
    let value = contract();
    assert_eq!(value["adoption_state"], "pending-external-release-pin");
    assert_eq!(value["source_revision_policy"], "exact-40-hex-release-commit-required");
    let bindings = value["required_exact_bindings"].as_array().unwrap();
    for required in [
        "compiler_binary_sha256",
        "source_revision",
        "source_tree_digest",
        "artifact_ckb_blake2b256",
        "metadata_ckb_blake2b256",
        "compatibility_profile_ckb_blake2b256",
        "lowering_record_ckb_blake2b256",
        "source_map_ckb_blake2b256",
        "checker_binary_sha256",
        "checker_policy_ckb_blake2b256",
    ] {
        assert!(bindings.iter().any(|binding| binding == required), "missing required handoff binding {required}");
    }
    assert_eq!(value["scheduler_boundary"]["compiler_access_template_authority"], "untrusted-template");
    assert_eq!(value["scheduler_boundary"]["authenticated_concrete_cell_resolution"], "myelin-owned");
    assert_eq!(value["scheduler_boundary"]["scheduler_plan_location"], "sidecar");
}

//! The source-edition contract is explicit at every external boundary.
use cellscript::{CellScriptEdition, CompileOptions, CURRENT_EDITION};

#[test]
fn only_edition_2027_is_constructible_from_external_source_contracts() {
    assert_eq!(CURRENT_EDITION, CellScriptEdition::Edition2027);
    assert_eq!(CompileOptions::default().edition, CURRENT_EDITION);
    assert_eq!("2027".parse::<CellScriptEdition>().unwrap(), CURRENT_EDITION);
    assert_eq!(serde_json::from_str::<CellScriptEdition>("\"2027\"").unwrap(), CURRENT_EDITION);
    for unsupported in ["2099", "unknown", ""] {
        let error = unsupported.parse::<CellScriptEdition>().unwrap_err();
        assert!(error.message.contains("2027"), "{error}");
        assert!(serde_json::from_value::<CellScriptEdition>(serde_json::Value::String(unsupported.into())).is_err());
    }
}

#[test]
fn current_artifacts_record_only_the_supported_source_contract() {
    let source = "module current_edition;\naction verify(witness value: u64) -> u64 { require value == 7; return 0; }";
    let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    result.validate().unwrap();
    assert_eq!(result.metadata.edition, CURRENT_EDITION);
    assert_eq!(result.metadata.compatibility_profile.source_semantics, cellscript_artifact_checker::SOURCE_SEMANTICS_2027);
    assert_eq!(result.metadata.public_interface.edition, "2027");
    let record = result.verified_lowering_record.unwrap();
    assert_eq!(record.edition, "2027");
    assert_eq!(record.compatibility_profile.edition, "2027");
}

#[test]
#[ignore = "explicit source-contract fixture export; writes only to a caller-selected directory"]
fn export_source_contract_fixtures() {
    use sha2::{Digest, Sha256};
    let output = std::path::PathBuf::from(std::env::var_os("CELLSCRIPT_FIXTURE_OUTPUT").expect("set explicit output directory"));
    std::fs::create_dir_all(&output).unwrap();
    for (name, text) in [
        ("bounded_source_identity_current.json", include_str!("fixtures/bounded_source_identity_current.json")),
        ("committed_source_identity_current.json", include_str!("fixtures/committed_source_identity_current.json")),
        ("nested_source_identity_current.json", include_str!("fixtures/nested_source_identity_current.json")),
        ("context_view_identity_current.json", include_str!("fixtures/context_view_identity_current.json")),
    ] {
        let mut fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(fixture["edition"], "2027");
        for case in fixture["cases"].as_array_mut().unwrap() {
            let source = case["source"].as_str().unwrap();
            assert_eq!(hex::encode(Sha256::digest(source.as_bytes())), case["source_sha256"]);
            let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap_or_else(|error| panic!("{name}: {error}\n{source}"));
            result.validate().unwrap();
            case["elf_sha256"] = hex::encode(Sha256::digest(&result.artifact_bytes)).into();
            case["interface_hash"] = result.metadata.interface_hash.into();
            case["typed_semantics_hash"] = result.metadata.typed_semantics_hash.into();
        }
        std::fs::write(output.join(name), serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
    }
    let mut effects: serde_json::Value = serde_json::from_str(include_str!("fixtures/effect_facts_interfaces.json")).unwrap();
    for case in effects["cases"].as_array_mut().unwrap() {
        let result = cellscript::compile(case["source"].as_str().unwrap(), CompileOptions::default()).unwrap();
        case["interface"] = serde_json::to_value(result.metadata.public_interface).unwrap();
    }
    std::fs::write(output.join("effect_facts_interfaces.json"), serde_json::to_string_pretty(&effects).unwrap() + "\n").unwrap();
}

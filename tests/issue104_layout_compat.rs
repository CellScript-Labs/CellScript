//! Compare ordinary-source machine output with the frozen 0.35 baseline.
//! Baseline recording is an explicit local operation, not automatic approval.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

fn embedded_wide_source() -> &'static str {
    let test = include_str!("value_return_abi.rs");
    test.split_once("r#\"module wide_arguments").unwrap().1.split_once("\"#;").map(|(body, _)| body).unwrap()
}

#[test]
fn ordinary_source_layout_matches_frozen_baseline() {
    let wide = format!("module wide_arguments{}", embedded_wide_source());
    let scalar = "module slot_compat;\nfn choose(a: u64, b: u64, flag: bool) -> u64 { if flag { return a + b; } return a; }\naction verify(witness a: u64, witness b: u64, witness flag: bool, witness expected: u64) {  require choose(a, b, flag) == expected; }";
    let cases = [
        ("typed_cell_predicate", include_str!("../examples/language/ckb/typed_cell_predicate.cell")),
        ("document_product_sum_relation", include_str!("fixtures/document_product_sum_relation.cell")),
        ("document_cache_scalar_forwarding", include_str!("fixtures/document_cache_scalar_forwarding.cell")),
        ("nested_migration", include_str!("../examples/language/batches/nested_migration.cell")),
        ("wide_record_return", wide.as_str()),
        ("scalar_cfg", scalar),
    ];
    let mut observations = BTreeMap::new();
    let mut compiled_count = 0;
    for (name, source) in cases {
        for (profile, diagnostics) in
            [("default", CompilerDiagnostics::default()), ("unoptimized", CompilerDiagnostics::without_optimizations())]
        {
            let result = compile_with_executable_surface_policy(
                source,
                CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() },
                ExecutableSurfacePolicy::DenyFailClosed,
            );
            let observation = match result {
                Ok(compiled) => {
                    compiled_count += 1;
                    let checked = cellscript_artifact_checker::check_bundle_values(
                        &compiled.artifact_bytes,
                        &serde_json::to_value(&compiled.metadata).unwrap(),
                        compiled.verified_lowering_record.as_ref().unwrap(),
                        compiled.source_artifact_map.as_ref().unwrap(),
                        &Default::default(),
                    );
                    json!({
                        "status":"compiled", "elf_bytes":compiled.artifact_bytes.len(),
                        "elf_sha256":hex::encode(Sha256::digest(&compiled.artifact_bytes)),
                        "checker":match checked { Ok(_) => "accepted".to_owned(), Err(error) => format!("{error}") },
                    })
                }
                Err(error) => json!({"status":"rejected", "code":error.code, "message":error.message}),
            };
            observations.insert(format!("{name}/{profile}"), observation);
        }
    }
    assert!(compiled_count >= 8, "snapshot must include actual admitted compilation paths");
    let actual = json!({
        "schema":"cellscript-layout-compatibility-v1", "baseline_commit":"c31e31754ac455ea3f071d21efd1ea05af4ec378",
        "scope":"finite ordinary-source ELF and independent-checker compatibility; not VM execution or whole-language equivalence",
        "cases":observations,
    });
    if let Some(path) = std::env::var_os("CELLSCRIPT_RECORD_LAYOUT_BASELINE") {
        std::fs::write(PathBuf::from(path), serde_json::to_vec_pretty(&actual).unwrap()).unwrap();
        return;
    }
    let expected: Value = serde_json::from_str(include_str!("fixtures/layout_compatibility_baseline.json")).unwrap();
    assert_eq!(actual, expected, "explain any intended artifact/admission change; never refresh the baseline silently");
}

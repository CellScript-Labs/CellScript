//! Compare ordinary-source machine output with the frozen 0.35 baseline.
//! Baseline recording is an explicit local operation, not automatic approval.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf};

// Aligned Cell data uses word loads and stores. Keep the original 0.35
// observations in the fixture and pin the affected executable identities here.
const ALIGNED_CELL_DATA_LAYOUTS: [(&str, usize, &str); 8] = [
    ("document_cache_scalar_forwarding/default", 3152, "e0d79ef06aa1f2d4aab875cf730d3da1b10dda089d50deaba95ab3f3138751ef"),
    ("document_cache_scalar_forwarding/unoptimized", 3160, "d3f576b03026d480c2797ea2914e4e9fa8ae8d6a96cc568faf6b58b02d78f129"),
    ("document_product_sum_relation/default", 4640, "f1d1207f521ba8aa1c4f3749462bd423e174401596c2c4a34d55d06a869b6207"),
    ("document_product_sum_relation/unoptimized", 4648, "a0efd6e3738aa2190bb05db3eacb2396ac751a57e1c6eb391d8fafc64c2a1ee1"),
    ("nested_migration/default", 20208, "114381a4da03b9771b2b14dca44d9f4891c419c4c62de8a5d5cff7bb1778dc94"),
    ("nested_migration/unoptimized", 20448, "fd24ed8252b15d4de524addb2ee487ea43d21d994992e683d39a456bf5c0559e"),
    ("typed_cell_predicate/default", 1768, "70e998fc13b84d941a1cc6c54903f7691b592b853ab654ddde3826706f77a122"),
    ("typed_cell_predicate/unoptimized", 1776, "f0074e756b46b4aad6351787b3b985ea43056609eb7676959e31b610f28c63a1"),
];

// Capturing the syscall index in a3 before address setup changes instruction
// order, while preserving these layouts and ELF sizes. Keep the earlier
// alignment observations above and pin this separate backend correction here.
const DYNAMIC_SYSCALL_INDEX_HASHES: [(&str, &str); 2] = [
    ("nested_migration/default", "c3b15d33e262bc276545146f206d4d4e7ff4f8cfbdd968dbf7db518d80ed5bf3"),
    ("nested_migration/unoptimized", "d5d5e0fab82c3da4e245edfce1bdd133ee43d1be06ec74d295132e15f7711599"),
];

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
    let mut expected: Value = serde_json::from_str(include_str!("fixtures/layout_compatibility_baseline.json")).unwrap();
    let cases = expected["cases"].as_object_mut().unwrap();
    for (name, bytes, sha256) in ALIGNED_CELL_DATA_LAYOUTS {
        let entry = cases.get_mut(name).unwrap();
        assert!(u64::try_from(bytes).unwrap() < entry["elf_bytes"].as_u64().unwrap(), "aligned field copy should reduce {name}");
        entry["elf_bytes"] = json!(bytes);
        entry["elf_sha256"] = json!(sha256);
    }
    for (name, sha256) in DYNAMIC_SYSCALL_INDEX_HASHES {
        cases.get_mut(name).unwrap()["elf_sha256"] = json!(sha256);
    }
    assert_eq!(actual, expected, "ordinary layout diverged from the recorded ELF or checker expectations");
}

//! Source arguments retain their roles, bounds and record identity through
//! helper transport and the independently checked nested codec.
#[cfg(feature = "cli")]
mod common;
#[cfg(feature = "cli")]
use camino::Utf8Path;
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn cases() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/nested_source_identity_characterization.json")).unwrap()
}

fn execute(result: &cellscript::CompileResult, ty: &str, record: bool) {
    use cellscript::nested_builder::{encode_nested_values, NestedElementInput};
    result.validate().unwrap();
    let key = 42u64.to_le_bytes();
    let fields: &[&[u8]] = if record { &[&key] } else { &[] };
    let encoded =
        encode_nested_values(&result.metadata, ty, &[NestedElementInput { fixed_fields: fields, payload: &[7, 8] }]).unwrap();
    let params = &result.metadata.actions.iter().find(|action| action.name == "verify").unwrap().params;
    let arguments = cellscript::encode_entry_witness_args_for_params(params, &[EntryWitnessArg::Bytes(encoded)]).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(arguments)).pack()).build().as_bytes()];
    let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
    assert_eq!(execution.exit_code, 0, "{execution:?}");
}

#[test]
fn structured_nested_source_preserves_historical_bytes_and_identity() {
    for case in cases()["cases"].as_array().unwrap() {
        let compiled = cellscript::compile(
            case["source"].as_str().unwrap(),
            CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        compiled.validate().unwrap();
        assert_eq!(hex::encode(Sha256::digest(&compiled.artifact_bytes)), case["elf_sha256"].as_str().unwrap());
        assert_eq!(compiled.metadata.interface_hash, case["interface_hash"].as_str().unwrap());
        assert_eq!(compiled.metadata.typed_semantics_hash, case["typed_semantics_hash"].as_str().unwrap());
    }
}

#[test]
fn structured_nested_source_executes_bytes_records_and_named_views() {
    for case in cases()["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let record = case["name"] == "records";
        let ty = if record { "NestedValues<Entry, 2, 8, 128>" } else { "NestedValues<Vec<u8>, 2, 8, 64>" };
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let source = if record && edition == CellScriptEdition::Edition2027 {
                    format!(
                        "{}\ndata_shape Entries {{ element: Entry, limits {{ items: 2, payload_bytes: 8, encoded_bytes: 128 }} }}",
                        source
                            .replace("NestedValues<Entry, 2, 8, 128>", "Entries")
                            .replace("NestedRecordView<Entry, 8>", "Entries::Item")
                    )
                } else {
                    source.into()
                };
                let compiled = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts: true,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition}/{diagnostics}: {error}\n{source}"));
                execute(&compiled, ty, record);
            }
        }
    }
}

#[test]
fn structured_nested_source_rejects_argument_role_substitutions_and_bound_overflow() {
    for ty in [
        "NestedValues<Vec<u8>, u64, 8, 128>",
        "NestedValues<2, 2, 8, 128>",
        "NestedValues<Entry, 33, 8, 128>",
        "NestedValues<Entry, 2, 257, 128>",
        "NestedValues<Entry, 2, 8, 4097>",
        "NestedValues<Entry, 2, 8, 3>",
        "NestedValues<Entry, 4294967296, 8, 128>",
        "NestedValues<Entry, 2, 8>",
        "NestedRecordView<Vec<u8>, 8>",
        "NestedBytesView<u64>",
        "NestedBytesView<8, 8>",
    ] {
        let source = format!("module wrong_nested\nstruct Entry {{ key: u64, payload: Vec<u8> }}\nfn inspect(value: {ty}) {{}}\naction verify() -> u64 {{ verification return 0 }}");
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            for target in [None, Some("riscv64-elf".into())] {
                assert!(cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).is_err(), "{source}");
            }
        }
    }
}

#[test]
#[cfg(feature = "cli")]
fn nested_record_aliases_bind_the_declaring_owner_across_calls() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let root = tempfile::tempdir().unwrap();
        for owner in ["left", "right", "consumer"] {
            let package = root.path().join(owner);
            std::fs::create_dir_all(package.join("src")).unwrap();
            let dependencies = if owner == "consumer" {
                "[dependencies]\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n"
            } else {
                ""
            };
            std::fs::write(package.join("Cell.toml"),format!("[package]\nname = \"{owner}\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n{dependencies}")).unwrap();
            if owner != "consumer" {
                std::fs::write(
                    package.join("src/main.cell"),
                    format!("module {owner}\npublic struct Entry {{ key: u64, payload: Vec<u8> }}\npublic fn read_foreign(value: NestedRecordView<Entry, 8>) -> u64 {{ return value.key }}\npublic fn read_values(values: NestedValues<Entry, 2, 8, 128>) -> u64 {{ return values[0].key }}"),
                )
                .unwrap();
            }
        }
        let consumer = root.path().join("consumer");
        let source = "module consumer\nuse left::Entry as First\nuse left::Entry as Alias\nuse right::Entry as Other\nuse left::read_foreign\nuse left::read_values\nfn inspect(value: NestedRecordView<Alias, 8>) -> u64 { return value.key }\naction verify(witness values: NestedValues<First, 2, 8, 128>) -> u64 { verification let entry: NestedRecordView<Alias, 8> = values[0] require inspect(entry) == 42 require read_foreign(entry) == 42 require read_values(values) == 42 return 0 }";
        std::fs::write(consumer.join("src/main.cell"), source).unwrap();
        let locked = common::cellc_command().current_dir(&consumer).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
        let manifest_lock = std::fs::read(consumer.join("Cell.lock")).unwrap();
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let options = CompileOptions {
                edition,
                diagnostics,
                source_contracts: true,
                target: Some("riscv64-elf".into()),
                ..Default::default()
            };
            let compiled = cellscript::compile_path_with_executable_surface_policy(
                Utf8Path::from_path(&consumer).unwrap(),
                options.clone(),
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{edition}/{diagnostics}: {error}"));
            let default_compiled = cellscript::compile_path_with_executable_surface_policy(
                Utf8Path::from_path(&consumer).unwrap(),
                CompileOptions { source_contracts: false, ..options.clone() },
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("default alias evidence {edition}/{diagnostics}: {error}"));
            execute(&default_compiled, "NestedValues<First, 2, 8, 128>", true);
            assert!(default_compiled.metadata.typed_semantics.nominal_declarations.is_some());
            assert_eq!(compiled.artifact_bytes, default_compiled.artifact_bytes);
            for _ in 0..3 {
                // This temporary package belongs to the test. Force fresh
                // discovery so cache hits cannot hide randomized body order.
                let cache = consumer.join(".cell/build/cache");
                if cache.exists() {
                    std::fs::remove_dir_all(cache).unwrap();
                }
                let repeated = cellscript::compile_path_with_executable_surface_policy(
                    Utf8Path::from_path(&consumer).unwrap(),
                    options.clone(),
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                assert!(
                    compiled.artifact_bytes == repeated.artifact_bytes,
                    "repeated compilation changed imported callable order: {} -> {}",
                    hex::encode(Sha256::digest(&compiled.artifact_bytes)),
                    hex::encode(Sha256::digest(&repeated.artifact_bytes))
                );
                assert!(!repeated.cache_hit, "determinism requires an actual compiler run");
                assert_eq!(compiled.metadata.typed_semantics_hash, repeated.metadata.typed_semantics_hash);
                assert_eq!(compiled.metadata.interface_hash, repeated.metadata.interface_hash);
                repeated.validate().unwrap();
            }
            execute(&compiled, "NestedValues<First, 2, 8, 128>", true);
            let checked = |result: &cellscript::CompileResult| {
                result.validate().unwrap();
                let record = &result.verified_lowering_record.as_ref().unwrap().typed_semantics;
                let catalog = record.nominal_declarations.as_ref().unwrap();
                let scope = catalog.scopes.iter().find(|scope| scope.module == record.module).unwrap();
                let callable = result.metadata.public_interface.callables.iter().find(|callable| callable.name == "inspect").unwrap();
                cellscript_artifact_checker::interface::qualified_source_type(&callable.params[0].r#type, scope).unwrap()
            };
            std::fs::write(consumer.join("src/main.cell"), source.replace("Alias", "Renamed")).unwrap();
            let renamed = cellscript::compile_path_with_executable_surface_policy(
                Utf8Path::from_path(&consumer).unwrap(),
                options.clone(),
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            let renamed_default = cellscript::compile_path_with_executable_surface_policy(
                Utf8Path::from_path(&consumer).unwrap(),
                CompileOptions { source_contracts: false, ..options.clone() },
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            assert_eq!(default_compiled.artifact_bytes, renamed_default.artifact_bytes);
            assert_eq!(checked(&default_compiled), checked(&renamed_default));
            assert!(
                compiled.artifact_bytes == renamed.artifact_bytes,
                "alias renaming changed ELF: {} -> {}",
                hex::encode(Sha256::digest(&compiled.artifact_bytes)),
                hex::encode(Sha256::digest(&renamed.artifact_bytes))
            );
            assert_eq!(checked(&compiled), checked(&renamed));
            assert!(checked(&compiled).contains("left::Entry"));
            assert_ne!(compiled.metadata.interface_hash, renamed.metadata.interface_hash);
            std::fs::write(
                consumer.join("src/main.cell"),
                source.replacen("NestedRecordView<Alias, 8>", "NestedRecordView<Other, 8>", 1),
            )
            .unwrap();
            let error = cellscript::compile_path_with_executable_surface_policy(
                Utf8Path::from_path(&consumer).unwrap(),
                options,
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_err();
            assert!(error.message.contains("type mismatch"), "different declarations with equal layouts must stay distinct: {error}");
            std::fs::write(consumer.join("src/main.cell"), source).unwrap();
        }
        assert_eq!(std::fs::read(consumer.join("Cell.lock")).unwrap(), manifest_lock);
    }
}

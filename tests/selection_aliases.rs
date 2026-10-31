//! Selected Cell schemas compare defining owners while executable views retain
//! the admitted catalog spelling. Equal layouts do not establish nominal equality.
#![cfg(feature = "cli")]
mod common;

use camino::Utf8Path;
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, prelude::*};
use std::path::Path;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn package(root: &Path, name: &str, edition: CellScriptEdition, source: &str, dependencies: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n{dependencies}")).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
}

fn source(read_owner: &str, read_type: &str, type_hash: &str, lock_hash: &str) -> String {
    format!(
        r#"module consumer
use peer::Token as RoleType
use {read_owner}::{read_type} as ReadAlias
public action verify() -> u64 {{ verification
    let selected = ckb::select_cell_roles(4, 4, 2, 8192,
        ckb::exact_cell_role<RoleType>("{type_hash}", "{lock_hash}", "id", "observe", "preserve", 1, 1, 1, 1))
    require ckb::selected_input_count<ReadAlias>(selected, 0) == 1
    require ckb::selected_output_count<ReadAlias>(selected, 0) == 1
    let input = ckb::selected_input<ReadAlias>(selected, 0, 0)
    let output = ckb::selected_successor<ReadAlias>(selected, 0, 0)
    require input.data.amount == 11
    require output.data.id == input.data.id
    require output.data.amount == input.data.amount
    return 0
}}
"#
    )
}

fn locked_fixture(root: &Path, edition: CellScriptEdition, source: &str) -> std::path::PathBuf {
    for name in ["peer", "decoy"] {
        package(&root.join(name), name, edition, &format!("module {name}\npublic resource Token has store {{ id: u64, amount: u64 }}\npublic resource Other has store {{ id: u64, amount: u64 }}\n"), "");
    }
    let app = root.join("consumer");
    package(&app, "consumer", edition, source, "[dependencies]\npeer = { path = \"../peer\" }\ndecoy = { path = \"../decoy\" }\n");
    let locked = common::cellc_command().current_dir(&app).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    app
}

fn options(edition: CellScriptEdition, diagnostics: CompilerDiagnostics) -> CompileOptions {
    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), source_contracts: true, ..Default::default() }
}

#[test]
fn same_defining_cell_schema_aliases_survive_metadata_machine_checking_and_vm() {
    let foreign_script = ckb_script_runner::deterministic_always_success_script(Bytes::from_static(b"selected-cell"));
    let type_hash = format!("0x{}", hex::encode(foreign_script.calc_script_hash().as_slice()));
    let lock_hash = format!("0x{}", hex::encode(ckb_script_runner::deterministic_always_success_lock_hash()));
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let root = tempfile::tempdir().unwrap();
        let app = locked_fixture(root.path(), edition, &source("peer", "Token", &type_hash, &lock_hash));
        let app = Utf8Path::from_path(&app).unwrap();
        let lock_before = std::fs::read(app.join("Cell.lock")).unwrap();
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let metadata = cellscript::compile_path_metadata_with_diagnostics(app, options(edition, diagnostics));
            assert!(!metadata.has_errors(), "{edition}/{diagnostics}: {:?}", metadata.diagnostics);
            assert!(metadata.metadata.is_some());
            let compiled = cellscript::compile_path_with_executable_surface_policy(
                app,
                options(edition, diagnostics),
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            let handles = &compiled.metadata.runtime.transaction_view_handles;
            assert!(handles.iter().any(|handle| handle.handle_type == "InputView<RoleType>"));
            assert!(handles.iter().any(|handle| handle.handle_type == "OutputView<RoleType>"));
            assert!(!handles.iter().any(|handle| handle.handle_type.contains("ReadAlias")));
            let record = compiled.verified_lowering_record.as_ref().unwrap();
            assert!(!record
                .typed_semantics
                .entries
                .iter()
                .flat_map(|entry| &entry.locals)
                .any(|local| matches!(local.ty.as_str(), "InputView<ReadAlias>" | "OutputView<ReadAlias>")));
            cellscript_artifact_checker::interface::inspect_bundle(
                &compiled.artifact_bytes,
                &serde_json::to_vec(&compiled.metadata).unwrap(),
                &cellscript_artifact_checker::canonical_bytes(record).unwrap(),
                &cellscript_artifact_checker::canonical_bytes(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
                &cellscript_artifact_checker::CheckerBudgets::default(),
            )
            .unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 2);
            fixture.current_type_script_input_indices = vec![0];
            let data: Bytes = [7u64.to_le_bytes(), 11u64.to_le_bytes()].concat().into();
            fixture.inputs[1].type_script = Some(foreign_script.clone());
            fixture.outputs[1].type_script = Some(foreign_script.clone());
            fixture.inputs[1].data = data.clone();
            fixture.outputs[1].data = data;
            assert_eq!(
                ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code,
                0,
                "{edition}/{diagnostics}"
            );
            for (id, amount) in [(7u64, 12u64), (8, 11)] {
                fixture.outputs[1].data = [id.to_le_bytes(), amount.to_le_bytes()].concat().into();
                assert_ne!(
                    ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code,
                    0,
                    "{edition}/{diagnostics}: id={id}, amount={amount}"
                );
            }
        }
        assert_eq!(std::fs::read(app.join("Cell.lock")).unwrap(), lock_before);
    }
}

#[test]
fn same_layout_cells_with_different_defining_owners_or_names_remain_distinct() {
    fn nominal_rejection(error: &cellscript::error::CompileError) -> bool {
        error.message.contains("nominal catalog schema") || error.related.iter().any(nominal_rejection)
    }

    let type_hash = format!("0x{}", "11".repeat(32));
    let lock_hash = format!("0x{}", "44".repeat(32));
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for (owner, declaration) in [("decoy", "Token"), ("peer", "Other")] {
            let root = tempfile::tempdir().unwrap();
            let app = locked_fixture(root.path(), edition, &source(owner, declaration, &type_hash, &lock_hash));
            let app = Utf8Path::from_path(&app).unwrap();
            let metadata = cellscript::compile_path_metadata_with_diagnostics(app, CompileOptions::default());
            assert!(metadata.has_errors());
            assert!(metadata.diagnostics.iter().any(nominal_rejection));
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let error = cellscript::compile_path_with_executable_surface_policy(
                    app,
                    options(edition, diagnostics),
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_err();
                assert!(nominal_rejection(&error), "{owner}::{declaration}/{edition}/{diagnostics}: {error:?}");
            }
        }
    }
}

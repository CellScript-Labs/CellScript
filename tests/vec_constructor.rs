//! Contextual constructors retain the bounded local Vec runtime boundary.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::bytes::Bytes;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn diagnostics(source: &str) -> Vec<cellscript::error::CompileError> {
    let tokens = cellscript::lexer::lex(source).unwrap();
    let module = cellscript::parser::parse(&tokens).unwrap();
    cellscript::types::diagnostics(&module)
}

#[test]
fn typed_vec_constructors_compile_and_check_without_heap_semantics() {
    for constructor in ["Vec::new()", "Vec::with_capacity(52)"] {
        for (ty, value) in [("u8", "7 as u8"), ("u64", "17"), ("Hash", "Hash::zero()"), ("Item", "Item { number: 23 }")] {
            let capacity = match ty {
                "u8" => 256,
                "Hash" => 8,
                _ => 32,
            };
            let source = format!(
                "module typed_vec\nstruct Item {{ number: u64 }}\naction verify() {{ verification\nlet mut values: Vec<{ty}> = {constructor}\nrequire values.len() == 0\nrequire values.capacity() == {capacity}\nvalues.push({value})\nrequire values.len() == 1\n}}"
            );
            for opt_level in 0..=3 {
                let compiled = compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                let collection = compiled
                    .metadata
                    .runtime
                    .collection_instantiations
                    .iter()
                    .find(|collection| collection.collection_ty == format!("Vec<{ty}>"))
                    .expect("constructor context must reach runtime collection metadata");
                assert_eq!(collection.element_ty, ty);
                assert_eq!(collection.max_elements, 256 / collection.element_width_bytes);
                cellscript_artifact_checker::check_bundle_values(
                    &compiled.artifact_bytes,
                    &serde_json::to_value(&compiled.metadata).unwrap(),
                    compiled.verified_lowering_record.as_ref().unwrap(),
                    compiled.source_artifact_map.as_ref().unwrap(),
                    &Default::default(),
                )
                .unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                let execution =
                    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
                assert_eq!(execution.exit_code, 0, "{ty}, {constructor}, O{opt_level}: {:?}", execution.captured_debug);
            }
        }
    }
}

#[test]
fn typed_vec_context_cannot_hide_bad_constructor_or_element_types() {
    for (initialization, use_value, message) in [
        ("Vec::with_capacity(true)", "7 as u8", "expects a u64 capacity"),
        ("Vec::new(52)", "7 as u8", "expects 0"),
        ("Vec::with_capacity()", "7 as u8", "expects 1"),
        ("Vec::with_capacity(52)", "false", "Vec.push type mismatch"),
    ] {
        let source = format!("module bad_vec\naction verify() {{ verification\nlet mut context: Vec<u8> = {initialization}\ncontext.push({use_value})\nrequire context.len() == 1\n}}");
        let errors = diagnostics(&source);
        assert_eq!(errors.len(), 1, "{initialization}: {errors:?}");
        assert!(errors[0].message.contains(message), "{:?}", errors[0]);
        assert!(!errors[0].message.contains("undefined"));
    }
    for ty in ["String", "Vec<u8>", "[u8; 300]"] {
        let source = format!(
            "module dynamic_vec\naction verify() {{ verification\nlet values: Vec<{ty}> = Vec::new()\nrequire values.len() == 0\n}}"
        );
        let errors = diagnostics(&source);
        assert_eq!(errors.len(), 1, "{ty}: {errors:?}");
        assert!(errors[0].message.contains("owned fixed-width element"), "{errors:?}");
    }
}

#[test]
fn cli_and_lsp_keep_the_primary_constructor_error_without_undefined_binding_cascades() {
    let source = "module recover_vec\naction verify() { verification\nlet mut context: Vec<u8> = Vec::with_capacity(true)\nfor i in 0..36 { context.push(i as u8) }\nrequire context.len() == 36\n}";
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("recover.cell");
    std::fs::write(&path, source).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&path).arg("--json").output().unwrap();
    assert!(!output.status.success());
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let json = result.to_string();
    assert!(json.contains("expects a u64 capacity"), "{json}");
    assert!(!json.contains("undefined"), "{json}");
    #[cfg(feature = "lsp")]
    {
        let mut server = cellscript::lsp::LspServer::new();
        let uri = "file:///recover.cell".to_string();
        server.open_document(uri.clone(), source.into());
        let errors = server.get_diagnostics(&uri);
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(errors[0].message.contains("expects a u64 capacity"));
        server.update_document(uri.clone(), source.replace("with_capacity(true)", "with_capacity(52)"));
        assert!(server.get_diagnostics(&uri).is_empty());
    }
}

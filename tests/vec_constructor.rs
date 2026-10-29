//! Contextual constructors retain the bounded local Vec runtime boundary.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::bytes::Bytes;
use ckb_testtool::ckb_types::{packed, prelude::*};

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
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
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
                assert_eq!(execution.exit_code, 0, "{ty}, {constructor}, {diagnostics}: {:?}", execution.captured_debug);
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
fn two_address_witness_args_execute_through_entry_abi() {
    let source =
        "module vec_debug\naction verify(witness first: Address, witness second: Address) { verification\nrequire first != second\n}";
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let mut payload = b"CSARGv1\0".to_vec();
    payload.extend(std::iter::repeat_n(0x11u8, 32));
    payload.extend(std::iter::repeat_n(0xeeu8, 32));
    let mut tx = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    tx.witnesses = vec![entry_witness_bytes(payload)];
    let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &tx);
    assert_eq!(execution.exit_code, 0, "{:?}", execution.captured_debug);
}

fn entry_witness_bytes(payload: Vec<u8>) -> Bytes {
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

#[test]
fn fixed_byte_vec_reads_and_pop_are_owned_snapshots() {
    for (ty, width) in [("Address", 32), ("Hash", 32)] {
        for operation in [
            "let before = values[0]\nvalues.set(0, second)\nrequire before == first",
            "let before = values.pop()\nvalues.push(second)\nrequire before == first",
            "let before = values.remove(0)\nvalues.push(second)\nrequire before == first",
        ] {
            let source = format!("module vec_snapshot\naction verify(witness first: {ty}, witness second: {ty}) {{ verification\nlet mut values: Vec<{ty}> = Vec::new()\nvalues.push(first)\n{operation}\nrequire values[0] == second\n}}");
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                let mut payload = b"CSARGv1\0".to_vec();
                payload.extend(std::iter::repeat_n(0x11, width));
                payload.extend(std::iter::repeat_n(0xee, width));
                let mut tx = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                tx.witnesses = vec![entry_witness_bytes(payload)];
                let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &tx);
                assert_eq!(execution.exit_code, 0, "{ty}, {operation}, {diagnostics}: {:?}", execution.captured_debug);
            }
        }
    }
    // Inline-array elements have no admitted stack-collection codec. The
    // executable surface must fail closed here instead of emitting aliased
    // fixed-byte reads for an unsupported element layout.
    for operation in [
        "let before = values[0]\nvalues.set(0, second)\nrequire before == first",
        "let before = values.pop()\nvalues.push(second)\nrequire before == first",
        "let before = values.remove(0)\nvalues.push(second)\nrequire before == first",
    ] {
        let source = format!("module vec_snapshot\naction verify(witness first: [u8; 16], witness second: [u8; 16]) {{ verification\nlet mut values: Vec<[u8; 16]> = Vec::new()\nvalues.push(first)\n{operation}\nrequire values[0] == second\n}}");
        let rejected = compile_with_executable_surface_policy(
            &source,
            CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        );
        let message = rejected.err().map(|error| error.to_string()).unwrap_or_default();
        assert!(message.contains("deny-fail-closed"), "{operation}: {message}");
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

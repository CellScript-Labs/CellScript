//! Breaking migration diagnostics for the retired digest-shaped byte APIs.
use cellscript::{CellScriptEdition, CompileOptions};
use ckb_testtool::ckb_types::bytes::Bytes;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn retired_byte_buffer_names_explain_the_required_migration() {
    for (call, replacement) in [
        ("witness::raw(source::input(0))", "witness::bounded_raw"),
        ("witness::lock(source::input(0))", "witness::bounded_lock"),
        ("witness::input_type(source::input(0))", "witness::bounded_entry"),
        ("witness::output_type(source::input(0))", "witness::bounded_output_type"),
        ("ckb::cell_data_hash_at(source::input(0), 0)", "ckb::cell_data_bytes32"),
        ("ckb::cell_data_hash(source::input(0))", "ckb::cell_data_hash_field"),
        ("ckb::cell_lock_args_hash(source::input(0))", "ckb::cell_lock_args32"),
        ("ckb::cell_type_args_hash(source::input(0))", "ckb::cell_type_args32"),
        ("ckb::require_cell_lock_args_hash(source::input(0), Hash::zero())", "require_cell_lock_args_exact"),
        ("ckb::require_cell_type_args_hash(source::input(0), Hash::zero())", "require_cell_type_args_exact"),
        ("ckb::require_cell_lock_args_prefix_hash(source::input(0), Hash::zero())", "require_cell_lock_args_prefix32"),
        ("ckb::require_cell_type_args_prefix_hash(source::input(0), Hash::zero())", "require_cell_type_args_prefix32"),
        ("ckb::require_cell_lock_args_suffix_hash(source::input(0), Hash::zero())", "require_cell_lock_args_suffix32"),
        ("ckb::require_cell_type_args_suffix_hash(source::input(0), Hash::zero())", "require_cell_type_args_suffix32"),
    ] {
        let source = format!("module retired_bytes\naction inspect() -> u64 {{ verification\nlet value = {call}\nreturn 0 }}");
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let error = cellscript::compile(&source, CompileOptions { edition, ..Default::default() })
                .expect_err("retired byte-buffer API must not compile");
            assert!(error.to_string().contains("removed legacy byte-buffer API"), "{call}: {error}");
            assert!(error.to_string().contains(replacement), "{call}: {error}");
        }
    }
}

fn compile_checked(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> cellscript::CompileResult {
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            diagnostics,
            target: Some("riscv64-elf".into()),
            ..Default::default()
        },
        cellscript::ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    cellscript_artifact_checker::check_bundle_values(
        &compiled.artifact_bytes,
        &serde_json::to_value(&compiled.metadata).unwrap(),
        compiled.verified_lowering_record.as_ref().unwrap(),
        compiled.source_artifact_map.as_ref().unwrap(),
        &cellscript_artifact_checker::CheckerBudgets::default(),
    )
    .unwrap();
    compiled
}

#[test]
fn streaming_digests_serialize_explicit_bytes_without_the_retired_buffer_limit() {
    for length in [0, 1, 32, 511, 512, 513, 600, 4096] {
        let data = vec![90u8; length];
        let expected = cellscript::ckb_blake2b256(&data).iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
        let source = format!(
            r#"module serialized_digest
action inspect() -> u64 {{
    verification
    let digest = ckb::cell_data_blake2b_span(source::input(0), 0, {length})
    let bytes: [u8; 32] = digest.0
    require bytes == b"{expected}"
    require Hash::from_bytes(bytes) == digest
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, diagnostics);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = Bytes::copy_from_slice(&data);
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{diagnostics}, length={length}: {execution:?}");
            if length > 0 {
                let mut changed = data.clone();
                changed[length - 1] ^= 1;
                fixture.inputs[0].data = Bytes::from(changed);
                assert_ne!(ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code, 0);
            }
        }
    }
}

#[test]
fn exact_cell_bytes_are_arrays_and_preserve_the_complete_slice() {
    let source = r#"module exact_cell_bytes
fn read() -> [u8; 32] { return ckb::cell_data_bytes32(source::input(0), 7) }
action inspect() -> u64 {
    let value: [u8; 32] = read()
    require value == b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
    require value[0] == (90 as u8) && value[31] == (90 as u8)
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_checked(source, diagnostics);
        for remaining in [0, 1, 16, 31, 32, 33, 600] {
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = vec![90; 7 + remaining].into();
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, remaining >= 32, "{diagnostics}, remaining={remaining}: {execution:?}");
        }
    }
}

fn witness_args(field: usize, payload: &[u8]) -> Vec<u8> {
    let mut fields: [Vec<u8>; 3] = std::array::from_fn(|_| Vec::new());
    fields[field].extend_from_slice(&(payload.len() as u32).to_le_bytes());
    fields[field].extend_from_slice(payload);
    let total = 16 + fields.iter().map(Vec::len).sum::<usize>();
    let mut result = (total as u32).to_le_bytes().to_vec();
    let mut offset = 16u32;
    for bytes in &fields {
        result.extend_from_slice(&offset.to_le_bytes());
        offset += bytes.len() as u32;
    }
    for bytes in fields {
        result.extend(bytes);
    }
    result
}

#[test]
fn witness_fields_require_exact_bytes_instead_of_padding_or_truncation() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (field, name) in ["lock", "input_type", "output_type"].into_iter().enumerate() {
            let source = format!(
                "module exact_witness_bytes\nfn read() -> [u8; 32] {{ return witness::{name}_exact32(source::input(0)) }}\naction inspect() -> u64 {{ let value: [u8; 32] = read()\nrequire value == b\"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\"\nreturn 0 }}"
            );
            let compiled = compile_checked(&source, diagnostics);
            for length in [0, 1, 31, 32, 33, 600] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.witnesses = vec![witness_args(field, &vec![90; length]).into()];
                let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code == 0, length == 32, "{name}, {diagnostics}, length={length}: {execution:?}");
            }
        }
    }
}

#[test]
fn complete_bounded_fields_keep_suffixes_above_the_old_buffer_limit() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_checked(
            "module complete_witness_bytes\naction inspect() -> u64 { let bytes = witness::bounded_lock(witness::args(0), 700)\nrequire bytes.size == 600\nrequire witness::byte(bytes, 599) == 91\nreturn 0 }",
            diagnostics,
        );
        for last in [90, 91] {
            let mut bytes = vec![90; 600];
            bytes[599] = last;
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.witnesses = vec![witness_args(0, &bytes).into()];
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, last == 91, "{diagnostics}, last={last}: {execution:?}");
        }
    }
}

#[test]
fn script_args_are_exact_bytes_through_direct_and_typed_helper_reads() {
    let source = r#"module exact_args
resource Token has store { amount: u64 }
fn read(view: InputView<Token>) -> [u8; 32] { return view.type_script.args32 }
action inspect() -> u64 {
    let value: [u8; 32] = read(ckb::group_input<Token>(0))
    require value == ckb::cell_type_args32(source::group_input(0))
    require value == b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_checked(source, diagnostics);
        for length in [0, 31, 32, 33, 600] {
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::from(vec![0; 8]), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.script_args = vec![90; length].into();
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, length == 32, "{diagnostics}, length={length}: {execution:?}");
        }
    }
}

#[test]
fn raw_args_requirements_compare_the_requested_exact_prefix_and_suffix() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for part in ["args32", "args_prefix32", "args_suffix32"] {
            let source = format!(
                "module args_bytes_check\naction inspect() -> u64 {{ ckb::require_cell_type_{part}(source::group_input(0), b\"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\")\nreturn 0 }}"
            );
            let compiled = compile_checked(&source, diagnostics);
            for length in [0, 31, 32, 33, 96] {
                for matching in [false, true] {
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                    fixture.current_type_script_input_indices = vec![0];
                    fixture.script_args = vec![if matching { 90 } else { 91 }; length].into();
                    let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
                    let accepted = matching && if part == "args32" { length == 32 } else { length >= 32 };
                    assert_eq!(execution.exit_code == 0, accepted, "{part}, {diagnostics}, length={length}: {execution:?}");
                }
            }
        }
    }
}

#[test]
fn raw_byte_results_cannot_implicitly_become_hashes() {
    for expression in [
        "ckb::cell_data_bytes32(source::input(0), 0)",
        "ckb::cell_lock_args32(source::input(0))",
        "ckb::cell_type_args32(source::input(0))",
        "witness::bytes32(source::input(0), 0)",
        "witness::lock_exact32(source::input(0))",
        "witness::input_type_exact32(source::input(0))",
        "witness::output_type_exact32(source::input(0))",
    ] {
        let source = format!("module raw_is_not_hash\naction inspect() -> u64 {{ let digest: Hash = {expression}\nreturn 0 }}");
        let error = cellscript::compile(&source, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() })
            .expect_err("raw byte results require an explicit semantic conversion");
        assert!(error.to_string().contains("type mismatch"), "{expression}: {error}");
    }
}

#[test]
fn script_hash_properties_serialize_full_bytes_through_readonly_helpers() {
    for property in ["input.lock_hash", "input.lock.hash"] {
        let source = format!(
            r#"module serialized_script_hash
resource Token has store {{ amount: u64 }}
fn read() -> [u8; 32] {{
    let input = ckb::input<Token>(0)
    return {property}.0
}}
action inspect() -> u64 {{
    verification
    require read() == ckb::cell_lock_hash(source::input(0)).0
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, diagnostics);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = Bytes::from(vec![0; 8]);
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{property}/{diagnostics}: {execution:?}");
        }
    }
}

#[test]
fn raw_witness_slices_require_full_width_and_allow_later_bytes() {
    let source = "module raw_witness_slice\nfn read() -> [u8; 32] { return witness::bytes32(source::input(0), 20) }\naction inspect() -> u64 { verification\nrequire read() == b\"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\" return 0 }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = compile_checked(source, diagnostics);
        for remaining in [0, 1, 31, 32, 33, 600] {
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.witnesses = vec![Bytes::from(vec![90; 20 + remaining])];
            assert_eq!(
                ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code == 0,
                remaining >= 32,
                "{diagnostics}, {remaining}"
            );
        }
    }
}

#[test]
fn retired_implicit_fields_require_explicit_byte_operations() {
    for (base, field, replacement) in [
        ("witness::args(0)", "lock", "bounded"),
        ("witness::args(0)", "input_type", "bounded"),
        ("witness::args(0)", "output_type", "bounded"),
        ("ckb::lock_script(source::input(0))", "args_hash", "args32"),
        ("ckb::type_script(source::input(0))", "args_hash", "args32"),
    ] {
        let source = format!(
            "module removed_field\naction inspect() -> u64 {{ verification\nlet view = {base}\nlet value = view.{field}\nreturn 0 }}"
        );
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let error = cellscript::compile(&source, CompileOptions { edition, ..Default::default() }).unwrap_err();
            assert!(error.message.contains("removed legacy byte-buffer field"), "{field}: {error}");
            assert!(error.message.contains(replacement), "{field}: {error}");
        }
    }
}

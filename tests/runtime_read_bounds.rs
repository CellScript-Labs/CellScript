//! Keep requested capacity, reported remaining length and published length separate.
use cellscript::{compile_with_executable_surface_policy, CellScriptEdition, CompileOptions, ExecutableSurfacePolicy};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn amount_words_reject_partial_reads() {
    for opt_level in [0, 3] {
        for (method, minimum) in [("amount_low", 8), ("amount_high", 16)] {
            let source = format!("module amount_bounds\naction inspect() -> u64 {{ let amount = xudt::{method}(source::input(0)) require amount == amount return 0 }}");
            let result = compile_with_executable_surface_policy(
                &source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            for length in [0, 1, 7, 8, 9, 15, 16, 17, 64] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.inputs[0].data = vec![0x5a; length].into();
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code == 0, length >= minimum, "O{opt_level}, {method}, length={length}: {execution:?}");
            }
        }
    }
}

#[test]
fn legacy_witness_projection_rejects_a_table_larger_than_the_loaded_buffer() {
    for opt_level in [0, 3] {
        let result = compile_with_executable_surface_policy(
            "module witness_bounds\naction inspect() -> u64 { let value = witness::lock(source::input(0)) require value == value return 0 }",
            CompileOptions { edition: CellScriptEdition::Edition2027, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed).unwrap();
        for payload in [492usize, 493, 600] {
            let mut witness = Vec::new();
            for word in [20 + payload as u32, 16, 20 + payload as u32, 20 + payload as u32, payload as u32] {
                witness.extend_from_slice(&word.to_le_bytes());
            }
            witness.extend(vec![0x5a; payload]);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.witnesses = vec![witness.into()];
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, payload <= 492, "O{opt_level}, payload={payload}: {execution:?}");
        }
    }
}

#[test]
fn fixed_cell_data_slice_requires_a_complete_slice_but_allows_a_suffix() {
    for opt_level in [0, 3] {
        for remaining in [0usize, 1, 16, 31, 32, 33, 512, 600] {
            let source = r#"module checked_slice
action inspect() -> u64 {
    let value = ckb::cell_data_bytes32(source::input(0), 7)
    require value == Hash::from_bytes(b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ")
    require value == ckb::cell_data_hash_at(source::input(0), 7)
    return 0
}"#;
            let result = compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            assert!(result
                .verified_lowering_record
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.name == "__ckb_cell_data_bytes32"));
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = vec![0x5a; 7 + remaining].into();
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, remaining >= 32, "O{opt_level}, remaining={remaining}: {execution:?}");
            if remaining < 32 {
                assert_eq!(execution.exit_code, 1);
            }
        }
    }
}

fn data_hash_literal(data: &[u8]) -> String {
    packed::CellOutput::calc_data_hash(data).as_slice().iter().map(|byte| format!("\\x{byte:02x}")).collect()
}

#[test]
fn consensus_cell_data_hash_covers_empty_large_and_all_cell_sources() {
    for opt_level in [0, 3] {
        for length in [0usize, 1, 511, 512, 513, 600, 4096] {
            let input = vec![0x5a; length];
            let output = vec![0x6b; length];
            let dep = vec![0x7c; length];
            let mut source = String::from("module consensus_hash\naction inspect() -> u64 {\n");
            for (view, data) in [
                ("source::input(1)", &input),
                ("source::group_input(0)", &input),
                ("source::output(1)", &output),
                ("source::group_output(1)", &output),
                ("source::cell_dep(0)", &dep),
            ] {
                source.push_str(&format!(
                    "require ckb::cell_data_hash({view}) == Hash::from_bytes(b\"{}\")\nrequire ckb::cell_data_hash({view}) == ckb::cell_data_hash_field({view})\n",
                    data_hash_literal(data),
                ));
            }
            source.push_str("return 0\n}");
            let result = compile_read(&source, opt_level);
            let entries = &result.verified_lowering_record.as_ref().unwrap().entries;
            assert_eq!(entries.iter().filter(|entry| entry.name == "__ckb_cell_data_hash_field").count(), 1);
            assert!(!entries.iter().any(|entry| matches!(
                entry.name.as_str(),
                "__ckb_cell_data_hash" | "__ckb_hash_blake2b_var" | "__ckb_hash_blake2b_compress"
            )));
            assert!(result
                .metadata
                .runtime
                .ckb_runtime_accesses
                .iter()
                .any(|access| access.operation == "cell-data-hash-field" && access.syscall == "LOAD_CELL_BY_FIELD"));
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 2);
            fixture.inputs[0].data = vec![0x11; 32].into();
            fixture.inputs[1].data = input.into();
            fixture.current_type_script_input_indices = vec![1];
            fixture.outputs[0].data = vec![0x22; 32].into();
            fixture.outputs[1].data = output.into();
            fixture.cell_deps.push(ckb_script_runner::FixtureCell { capacity: 100_000_000_000, type_script: None, data: dep.into() });
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "O{opt_level}, length={length}: {execution:?}");
        }
    }
}

fn compile_read(source: &str, opt_level: u8) -> cellscript::CompileResult {
    compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            opt_level,
            target: Some("riscv64-elf".into()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap()
}

#[test]
fn cell_data_read_aliases_share_artifacts_and_reject_invalid_sources() {
    for opt_level in [0, 3] {
        for (canonical, alias, arguments) in [
            ("cell_data_hash", "cell_data_hash_field", "source::input(0)"),
            ("cell_data_bytes32", "cell_data_hash_at", "source::input(0), 0"),
        ] {
            let source = format!("module aliases\naction inspect() -> u64 {{ let value = ckb::{canonical}({arguments}) require value == value return 0 }}");
            let first = compile_read(&source, opt_level);
            let second = compile_read(&source.replace(canonical, alias), opt_level);
            let formatted = cellscript::fmt::Formatter::new(Default::default()).format_module(&first.ast).unwrap();
            assert_eq!(first.artifact_bytes, compile_read(&formatted, opt_level).artifact_bytes);
            assert_eq!(first.artifact_bytes, second.artifact_bytes, "{canonical}, O{opt_level}");
        }
        // Legacy raw u64 handles still pass type checking. Runtime must reject
        // zero/unknown tags, HeaderDep and missing coordinates explicitly.
        for view in [
            "0",
            "30064771072",
            "17179869184",
            "source::input(99)",
            "source::output(99)",
            "source::group_input(0)",
            "source::group_output(99)",
            "source::cell_dep(99)",
        ] {
            for (call, expected) in [
                (format!("ckb::cell_data_hash({view})"), 44),
                (format!("ckb::cell_data_bytes32({view}, 0)"), if view == "0" || view == "30064771072" { 44 } else { 1 }),
            ] {
                let source =
                    format!("module bad_source\naction inspect() -> u64 {{ let value = {call} require value == value return 0 }}");
                let result = compile_read(&source, opt_level);
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code, expected, "{call}, O{opt_level}: {execution:?}");
            }
        }
        let source = "module wrong_view\naction inspect() -> u64 { let value = ckb::cell_data_hash(source::header_dep(0)) require value == value return 0 }";
        assert!(cellscript::compile(
            source,
            CompileOptions { edition: CellScriptEdition::Edition2027, opt_level, ..Default::default() }
        )
        .is_err());
    }
}

#[test]
fn checked_cell_reads_preserve_live_views_and_collection_copies() {
    let data = vec![0x5au8; 40];
    let hash = blake2b_256(&data).iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    let source = format!(
        r#"module composed_cell_reads
resource Token has store {{ amount: u64 }}
collections Work {{ limits {{ bytes: 256, allocations: 2 }}
    values: Vec<[u8; 32]> {{ items: 2 }}
}}
fn inspect(view: InputView<Token>, work: &Work) -> bool {{
    let saved = work.values.get(0)
    let slice = ckb::cell_data_bytes32(source::input(0), 8)
    let hash = ckb::cell_data_hash(source::input(0))
    return view.capacity > 0 && slice == Hash::from_bytes(b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ")
        && hash == Hash::from_bytes(b"{hash}") && saved == b"0123456789abcdef0123456789ABCDEF"
}}
action check() -> u64 {{
    let mut work = Work::new()
    work.values.push(b"0123456789abcdef0123456789ABCDEF")
    require inspect(ckb::input<Token>(0), &work)
    return 0
}}"#
    );
    for opt_level in [0, 3] {
        let result = compile_with_executable_surface_policy(
            &source,
            CompileOptions {
                edition: CellScriptEdition::Edition2027,
                opt_level,
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.inputs[0].data = data.clone().into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
    }
}

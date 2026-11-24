use cellscript::{compile, CompileOptions};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

const SOURCE: &str = include_str!("../examples/language/ckb/typed_cell_data.cell");

#[test]
fn fixed_record_cell_fields_verify_and_execute_dynamic_indexes() {
    use cellscript::{fixed_cell_builder::encode_fixed_cell_data, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
    use std::collections::BTreeMap;

    let source = include_str!("syntax_combo/seeds/fixed-record-cell-data.cell");
    cellscript::fmt::verify_idempotent(source, Default::default()).unwrap();
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let rows = [[1u8; 32].as_slice(), &17u128.to_le_bytes(), &[2; 32], &((1u128 << 96) + 27).to_le_bytes()].concat();
        let fields = BTreeMap::from([("rows".into(), rows.as_slice()), ("tag".into(), [42].as_slice())]);
        let actual = encode_fixed_cell_data(&compiled.metadata, "Table", &fields).unwrap();
        assert_eq!(actual, [[42].as_slice(), rows.as_slice()].concat());
        assert!(encode_fixed_cell_data(
            &compiled.metadata,
            "Table",
            &BTreeMap::from([("tag".into(), [42].as_slice()), ("rows".into(), &rows[..rows.len() - 1])])
        )
        .is_err());

        for (index, alteration, expected) in [(1, 0, 0), (0, 0, 5), (1, 1, 5), (1, 2, 16), (1, 3, 16), (2, 0, 2), (u64::MAX, 0, 2)] {
            let mut output = actual.clone();
            match alteration {
                1 => *output.last_mut().unwrap() ^= 1, // High limb of the second amount.
                2 => output.push(0),
                3 => {
                    output.pop();
                }
                _ => {}
            }
            let witness = compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::U64(index)]).unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.inputs[0].data = actual.clone().into();
            fixture.outputs[0].data = output.into();
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(witness)).pack()).build().as_bytes()];
            let execution =
                ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
            assert_eq!(execution.exit_code, expected, "{diagnostics} index={index} alteration={alteration}: {execution:?}");
        }
    }

    for rejected in [
        source.replace("index: u64", "index: u128"),
        source.replace("[Row; 2]", "[Row; 0]"),
        source.replace("[Row; 2]", "[Row; 11]"),
        source.replace("owner: Hash", "owner: bool"),
    ] {
        assert!(compile(&rejected, Default::default()).is_err());
    }
}

#[test]
fn small_record_rows_are_values_after_the_array_is_reassigned() {
    let source = r#"
module tiny_rows;
private struct Row has fixed, serializable { value: u8, }
private resource Table has store { rows: [Row; 2], }
action inspect() -> u64 {
    let mut rows = ckb::input::<Table>(0).data.rows;
    let saved = rows[0];
    rows = ckb::output::<Table>(0).data.rows;
    require saved.value == 5;
    require rows[1].value == 9;
    return 0;
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.current_type_script_input_indices = vec![0];
        fixture.inputs[0].data = vec![5, 6].into();
        fixture.outputs[0].data = vec![8, 9].into();
        let result =
            ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
        assert_eq!(result.exit_code, 0, "{diagnostics}: {result:?}");
    }
}

#[test]
#[cfg(feature = "cli")]
fn imported_cell_record_arrays_keep_the_declaring_row_type() {
    use cellscript::ExecutableSurfacePolicy;

    let root = tempfile::tempdir().unwrap();
    let layouts = root.path().join("layouts");
    let reader = root.path().join("reader");
    for (package, name, dependencies) in
        [(&layouts, "layout_defs", ""), (&reader, "array_reader", "[dependencies]\nlayout_defs = { path = \"../layouts\" }\n")]
    {
        std::fs::create_dir_all(package.join("src")).unwrap();
        std::fs::write(
            package.join("Cell.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2027\"\ncellscript_version = \">=0.33.0-alpha.2\"\n{dependencies}"),
        )
        .unwrap();
    }
    std::fs::write(
        layouts.join("src/main.cell"),
        "module layout_defs; public struct Row has fixed, serializable { actual: u64, } public resource Table has store { rows: [Row; 2], }",
    )
    .unwrap();
    std::fs::write(
        reader.join("src/main.cell"),
        "module array_reader; use layout_defs::Table as Ledger; private struct Row { decoy: Hash, }
         public action inspect() -> u64 { require ckb::input::<Ledger>(0).data.rows[1].actual == 21; return 0; }",
    )
    .unwrap();
    let locked =
        std::process::Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&reader).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stderr));
    let compiled = cellscript::compile_path_with_executable_surface_policy(
        camino::Utf8Path::from_path(&reader).unwrap(),
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
        None,
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    compiled.validate().unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.inputs[0].data = [7u64.to_le_bytes(), 21u64.to_le_bytes()].concat().into();
    let result = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
    assert_eq!(result.exit_code, 0, "{result:?}");
}

#[test]
fn copied_cell_fields_snapshot_mutable_bindings_before_branch_assignment() {
    for (ty, width) in [("u64", 8), ("u128", 16), ("Hash", 32), ("[u8; 1]", 1), ("[u8; 8]", 8), ("[u8; 9]", 9)] {
        for annotated in [false, true] {
            let annotation = if annotated { format!(": {ty}") } else { String::new() };
            let source = format!(
                "module field_snapshots;\nresource Token has store {{ value: {ty}, }}\n\
                 fn same(before: InputView<Token>, after: OutputView<Token>, change: bool) -> bool {{\n\
                 let mut current = before.data.value;\nlet original{annotation} = current;\n\
                 if change {{ current = after.data.value; }}\nreturn original == current;\n}}\n\
                 action inspect() -> u64 {{ \n\
                 require same(ckb::input::<Token>(0), ckb::output::<Token>(0), true);\nreturn 0;\n}}"
            );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile(
                    &source,
                    CompileOptions {
                        edition: cellscript::CURRENT_EDITION,
                        diagnostics,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                for equal in [false, true] {
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                    fixture.current_type_script_input_indices = vec![0];
                    let original = vec![0x25; width];
                    let mut replacement = original.clone();
                    if !equal {
                        // Also exercise the high word/last byte of wide values.
                        replacement[width - 1] ^= 1;
                    }
                    fixture.inputs[0].data = original.into();
                    fixture.outputs[0].data = replacement.into();
                    let execution = ckb_script_runner::execute_cellscript_script(
                        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                        &fixture,
                    );
                    assert_eq!(
                        execution.exit_code,
                        if equal { 0 } else { 5 },
                        "{ty} {diagnostics} annotated={annotated} equal={equal}"
                    );
                }
            }
        }
    }
}

#[test]
fn nested_data_views_keep_owned_buffers_across_reads_and_helpers() {
    use cellscript::nested_builder::{encode_nested_cell_data, NestedCellFieldInput as Field, NestedElementInput as Item};
    use std::collections::BTreeMap;
    let source = include_str!("syntax_combo/seeds/named-nested-data-views.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for group in [false, true] {
            let source = if group {
                source
                    .replace("ckb::input::<Batch>(1)", "ckb::group_input::<Batch>(0)")
                    .replace("ckb::output::<Batch>(0)", "ckb::group_output::<Batch>(0)")
            } else {
                source.into()
            };
            let compiled = compile(
                &source,
                CompileOptions {
                    edition: cellscript::CURRENT_EDITION,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("nested data {diagnostics} group={group}: {error}"));
            for large in [false, true] {
                let encode = |payload: &[u8]| {
                    let mut messages = vec![Item { fixed_fields: &[], payload }];
                    if large {
                        messages.extend((0..15).map(|_| Item { fixed_fields: &[], payload: &[0; 256] }));
                    }
                    let records = [Item { fixed_fields: &[&[7]], payload: &[8] }];
                    encode_nested_cell_data(
                        &compiled.metadata,
                        "Batch",
                        &BTreeMap::from([
                            ("messages".into(), Field::Nested(&messages)),
                            ("id".into(), Field::FixedBytes(&42u64.to_le_bytes())),
                            ("owner".into(), Field::FixedBytes(&[0xab; 32])),
                            ("entries".into(), Field::Nested(&records)),
                        ]),
                    )
                    .unwrap()
                };
                let input = encode(&[1, 2, 3]);
                let output = encode(&[4, 5]);
                if large {
                    assert!(input.len() > 4000);
                }
                for case in 0..7 {
                    let mut input = input.clone();
                    let mut output = output.clone();
                    let offset = |data: &[u8], field: usize| {
                        u32::from_le_bytes(data[4 + field * 4..8 + field * 4].try_into().unwrap()) as usize
                    };
                    match case {
                        1 => {
                            let at = offset(&output, 1);
                            output[at] ^= 1;
                        }
                        2 => {
                            let at = offset(&output, 2) + 31;
                            output[at] ^= 1;
                        }
                        3 => {
                            let at = offset(&output, 0);
                            let first = offset(&output[at..], 0);
                            output[at + first + 4] ^= 1;
                        }
                        4 => {
                            let last = input.len() - 2;
                            input[last] ^= 1;
                        } // malformed unused input record payload length
                        5 => {
                            output.push(0);
                        }
                        6 => {
                            input.resize(9000, 0);
                        }
                        _ => {}
                    }
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 1);
                    fixture.current_type_script_input_indices = vec![1];
                    fixture.inputs[1].data = input.into();
                    fixture.outputs[0].data = output.into();
                    let execution = ckb_script_runner::execute_cellscript_script(
                        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                        &fixture,
                    );
                    assert_eq!(
                        execution.exit_code,
                        [0, 5, 5, 5, 16, 2, 16][case],
                        "{diagnostics} group={group} large={large} case={case}: {execution:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn data_predicates_compose_through_typed_read_only_helpers() {
    let base = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    cellscript::fmt::verify_idempotent(base, Default::default()).unwrap();
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let source = base
            .replace("before: InputView", &format!("{params}before: InputView"))
            .replace("matches(before", &format!("matches({forwarded}before"))
            .replace("forward(ckb::input", &format!("forward({arguments}ckb::input"));
        for group in [false, true] {
            let source = if group {
                source
                    .replace("ckb::input::<Token>(1)", "ckb::group_input::<Token>(0)")
                    .replace("ckb::output::<Token>(0)", "ckb::group_output::<Token>(0)")
            } else {
                source.clone()
            };
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled =
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap_or_else(|error| panic!("{diagnostics} padding={padding} group={group}: {error}"));
                assert!(!compiled.metadata.runtime.fail_closed_runtime_features.contains(&"cell-data-view-artifact-proof".into()));
                let reads = compiled
                    .metadata
                    .runtime
                    .ckb_runtime_accesses
                    .iter()
                    .filter(|read| read.operation == "typed-cell-data")
                    .collect::<Vec<_>>();
                assert!(!reads.is_empty());
                assert!(reads
                    .iter()
                    .all(|read| read.source == "SourceView" && read.provenance.index.max_inclusive == Some(u64::from(u32::MAX))));
                let mut data = 42u64.to_le_bytes().to_vec();
                data.extend_from_slice(&[0xab; 32]);
                for case in 0..4 {
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 2);
                    fixture.current_type_script_input_indices = vec![1];
                    fixture.inputs[0].data = vec![0; 40].into();
                    fixture.inputs[1].data = data.clone().into();
                    fixture.outputs[0].data = vec![0; 40].into();
                    fixture.outputs[1].data = vec![0; 40].into();
                    let mut output = data.clone();
                    match case {
                        1 => output[0] ^= 1,
                        2 => output[39] ^= 1,
                        3 => output.push(0),
                        _ => {}
                    }
                    fixture.outputs[usize::from(group)].data = output.into();
                    let execution = ckb_script_runner::execute_cellscript_script_with_transaction_transform(
                        cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                        &fixture,
                        |tx, _| {
                            if !group {
                                return tx;
                            }
                            let mut outputs = tx.outputs().into_iter().collect::<Vec<_>>();
                            outputs[0] = outputs[0].clone().as_builder().type_(packed::ScriptOpt::default()).build();
                            tx.as_advanced_builder().set_outputs(outputs).build()
                        },
                    );
                    assert_eq!(
                        execution.exit_code,
                        [0, 5, 5, 16][case],
                        "{diagnostics} padding={padding} group={group} case={case}: {execution:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn helper_coordinate_transport_admits_checked_helpers_without_data_reads() {
    let base = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    for predicate in ["true", "before.capacity == after.capacity"] {
        let source = base.replace("before.data.id == after.data.id && before.data.owner == after.data.owner", predicate);
        let result = compile(&source, Default::default()).unwrap();
        assert!(!result.metadata.runtime.fail_closed_runtime_features.contains(&"cell-data-view-artifact-proof".into()));
        cellscript::compile_with_executable_surface_policy(
            &source,
            Default::default(),
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
    }
}

#[test]
fn data_helpers_do_not_gain_ownership_or_erase_nominal_types() {
    let base = include_str!("../examples/language/ckb/typed_cell_predicate.cell");
    for source in [
        base.replace("ckb::input::<Token>(1)", "ckb::output::<Token>(1)"),
        base.replace("ckb::output::<Token>(0)", "ckb::input::<Token>(0)"),
        base.replace("ckb::input::<Token>(1)", "1"),
        base.replace("before: InputView<Token>", "mut before: InputView<Token>"),
        base.replace("before: InputView<Token>", "before: Vec<InputView<Token>>"),
        base.replace("return matches(before, after);", "change()\nreturn matches(before, after)")
            + "\nresource Change has create, destroy { amount: u64 }\naction change() {  let value = create Change { amount: 1 }; destroy value; }",
        base.replace(
            "return matches(before, after);",
            "consume before
return true",
        ),
        base.replace(
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> bool",
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> InputView<Token>",
        )
        .replace("return matches(before, after);", "return before"),
        format!(
            "{}\nresource Other has store {{ id: u64, owner: Hash }}",
            base.replace("ckb::input::<Token>(1)", "ckb::input::<Other>(1)")
        ),
    ] {
        assert!(compile(&source, Default::default()).is_err(), "invalid helper accepted:\n{source}");
    }
}

#[test]
fn fixed_data_fields_lower_without_confusing_transaction_metadata_or_ownership() {
    cellscript::fmt::verify_idempotent(SOURCE, cellscript::fmt::FormatConfig::default()).unwrap();
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = compile(SOURCE, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
            .unwrap_or_else(|error| panic!("{diagnostics}: {error}"));
        assert!(!result.metadata.runtime.fail_closed_runtime_features.contains(&"cell-data-view-artifact-proof".into()));
        assert!(!result.metadata.runtime.fail_closed_runtime_features.contains(&"field-access".into()));
        let reads = result
            .metadata
            .runtime
            .ckb_runtime_accesses
            .iter()
            .filter(|access| access.operation == "typed-cell-data")
            .collect::<Vec<_>>();
        assert!(reads.iter().any(|access| access.source == "Input"));
        assert!(reads.iter().any(|access| access.source == "Output"));
        assert!(result
            .metadata
            .runtime
            .transaction_view_handles
            .iter()
            .filter(|view| view.handle_type.starts_with("CellDataView<"))
            .all(|view| view.ownership == "read-only-view" && !view.lifecycle_authority));
        cellscript::compile_with_executable_surface_policy(
            SOURCE,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
    }
}

#[test]
fn data_views_reject_mutation_escape_and_unsupported_layouts() {
    for source in [
        SOURCE.replace("let data = input.data;", "let mut data = input.data;"),
        SOURCE.replace("return 0", "return alias").replace("inspect() -> u64", "inspect() -> CellDataView<Token>"),
        SOURCE.replace("let alias = data;", "let alias = [data];"),
        SOURCE.replace("require alias.id == 7;", "require alias == data;"),
        SOURCE.replace("let alias = data;", "let alias = data;\ndata.id = 8;"),
        SOURCE.replace("id: u8", "id: bool"),
        SOURCE.replace("id: u8", "id: i32"),
        SOURCE.replace("bytes: [u8; 3]", "bytes: [u8; 0]"),
        SOURCE.replace("bytes: [u8; 3]", "bytes: [u8; 257]"),
        SOURCE.replace("bytes: [u8; 3]", "bytes: [u8; 256], extra: [u8; 200]"),
        SOURCE.replace("bytes: [u8; 3]", "bytes: Vec<u8>"),
    ] {
        assert!(compile(&source, CompileOptions::default()).is_err(), "invalid data view accepted:\n{source}");
    }
}

#[test]
fn maximum_cell_data_reads_copy_full_arrays_and_check_little_endian_scalars() {
    let source = r#"
module data_view::maximum;
resource Full has store {
    prefix: [u8; 256], padding: [u8; 218], short: u16, word: u32, owner: Hash,
}
action inspect() -> u64 {

    let input = ckb::input::<Full>(0).data;
    let output = ckb::output::<Full>(0).data;
    require input.prefix == output.prefix;
    require input.owner == output.owner;
    require input.short == 4660;
    require input.word == 2309737967;
    return 0;
}
"#;
    let mut data = (0..256).map(|byte| byte as u8).collect::<Vec<_>>();
    data.extend_from_slice(&[0; 218]);
    data.extend_from_slice(&0x1234u16.to_le_bytes());
    data.extend_from_slice(&0x89abcdefu32.to_le_bytes());
    data.extend_from_slice(&[0xbc; 32]);
    assert_eq!(data.len(), 512);
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
            .unwrap_or_else(|error| panic!("{diagnostics}: {error}"));
        for case in 0..8 {
            let mut input = data.clone();
            let mut output = data.clone();
            let expected = match case {
                0 => 0,
                1 | 2 => {
                    output[if case == 1 { 255 } else { 511 }] ^= 1;
                    5
                }
                3 | 4 => {
                    input[if case == 3 { 475 } else { 479 }] ^= 1;
                    5
                }
                5 => {
                    input.pop();
                    16
                }
                6 => {
                    output.push(0);
                    16
                }
                7 => {
                    input[0] ^= 1;
                    5
                }
                _ => unreachable!(),
            };
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.inputs[0].data = input.into();
            fixture.outputs[0].data = output.into();
            let execution =
                ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
            assert_eq!(execution.exit_code, expected, "{diagnostics} case={case}: {execution:?}");
        }
    }
}

#[test]
fn fixed_data_fields_execute_with_exact_lengths_full_widths_and_source_domains() {
    let mut data = vec![7];
    data.extend_from_slice(&42u64.to_le_bytes());
    data.extend_from_slice(&[0xab; 32]);
    data.extend_from_slice(&((1u128 << 100) | 19).to_le_bytes());
    data.extend_from_slice(&[3, 5, 7]);
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for group in [false, true] {
            let source = if group {
                SOURCE.replace("ckb::input", "ckb::group_input").replace("ckb::output", "ckb::group_output")
            } else {
                SOURCE.into()
            };
            let result =
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
            for case in 0..8 {
                let mut input = data.clone();
                let mut output = data.clone();
                let expected = match case {
                    0 => 0,
                    1 => {
                        input.pop();
                        16
                    }
                    2 => {
                        input.push(0);
                        // The raw node syscall reports the complete length;
                        // the field decoder rejects that length explicitly.
                        16
                    }
                    3 => {
                        output.pop();
                        16
                    }
                    4 => {
                        output.push(0);
                        16
                    }
                    5 => {
                        output[40] ^= 1;
                        5
                    }
                    6 => {
                        output[56] ^= 1;
                        5
                    }
                    7 => {
                        output[59] ^= 1;
                        5
                    }
                    _ => unreachable!(),
                };
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.current_type_script_input_indices = vec![0];
                fixture.inputs[0].data = input.into();
                fixture.outputs[0].data = output.into();
                let execution =
                    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
                assert_eq!(execution.exit_code, expected, "{diagnostics} group={group} case={case}: {execution:?}");
            }
        }
    }
}

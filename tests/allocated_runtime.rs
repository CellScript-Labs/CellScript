//! Source-to-CKB-VM evidence for bounded allocations and independent artifact checks.
use cellscript::{CellScriptEdition, CompileOptions};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
#[path = "support/allocated_scalar_definitions.rs"]
mod allocated_scalar_definitions;
#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "allocated_runtime/projections.rs"]
mod projections;
#[path = "allocated_runtime/returns.rs"]
mod returns;
#[path = "allocated_runtime/scalars.rs"]
mod scalars;

#[path = "allocated_runtime/resources.rs"]
mod resources;
// Shared scheduler observers expose additional diagnostics used by other suites.
#[allow(dead_code)]
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[allow(dead_code)]
#[path = "support/cost_trace.rs"]
mod cost_trace;

#[path = "support/collection_coordinate_helpers.rs"]
mod collection_coordinate_helpers;

#[path = "support/borrowed_collection_reads.rs"]
mod borrowed_collection_reads;

#[test]
fn allocated_borrowed_reads_execute_with_copied_records_and_missing_values() {
    for padding in [0, 7, 8] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = cellscript::compile(
                    &borrowed_collection_reads::source(padding, bytes),
                    CompileOptions {
                        edition: CellScriptEdition::Edition2027,
                        diagnostics,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                for (index, key, expected) in [(0, 4, 0), (1, 4, 21), (u64::MAX, 4, 21), (0, 99, 21)] {
                    let outcome =
                        execute_compiled(&compiled, &[cellscript::EntryWitnessArg::U64(index), cellscript::EntryWitnessArg::U64(key)]);
                    assert_eq!(
                        outcome.exit_code, expected,
                        "padding={padding}, bytes={bytes}, {diagnostics}, index={index}, key={key}: {outcome:?}"
                    );
                }
            }
        }
    }
}

const DECLARATION: &str = "module allocated_runtime\ncollections Work { limits { bytes: 2048, allocations: 16 }\nvalues: Vec<u64> { items: 8 }\ntotals: OrderedMap<u64, u64> { items: 8 }\nseen: OrderedSet<u64> { items: 8 } }\n";

fn execute(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> ckb_script_runner::CkbScriptExecutionResult {
    execute_with_args(source, diagnostics, &[])
}

fn execute_with_args(
    source: &str,
    diagnostics: cellscript::CompilerDiagnostics,
    args: &[cellscript::EntryWitnessArg],
) -> ckb_script_runner::CkbScriptExecutionResult {
    let compiled = cellscript::compile(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".into()),
            diagnostics,
            ..Default::default()
        },
    )
    .unwrap();
    execute_compiled(&compiled, args)
}

fn execute_compiled(
    compiled: &cellscript::CompileResult,
    args: &[cellscript::EntryWitnessArg],
) -> ckb_script_runner::CkbScriptExecutionResult {
    assert!(!compiled
        .metadata
        .runtime
        .fail_closed_runtime_features
        .iter()
        .any(|feature| feature == "allocated-values-artifact-proof"));
    let payload = compiled.metadata.actions[0].entry_witness_args(args).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture)
}

fn compile_checked(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> cellscript::CompileResult {
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".into()),
            diagnostics,
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
fn allocated_coordinate_helpers_execute_after_guarded_writes_with_register_and_stack_arguments() {
    for padding in [0, 5, 6] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = cellscript::compile(
                    &collection_coordinate_helpers::source(padding, bytes),
                    CompileOptions {
                        edition: CellScriptEdition::Edition2027,
                        diagnostics,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                for value in [0, 7, 255] {
                    let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::U64(value)]);
                    assert_eq!(result.exit_code, 0, "padding={padding}, bytes={bytes}, {diagnostics}, value={value}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn allocated_modified_records_copy_current_fields_and_keep_snapshots() {
    let source = r#"module modified_record_copies
struct Pair { left: u64, right: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<Pair> { items: 4 }
}
fn total(pair: Pair) -> u64 { return pair.left + pair.right }
action inspect(witness input: Pair) -> u64 {
    let mut pair = input
    let original = pair
    pair.left = 2
    let changed = pair
    let mut work = Work::new()
    work.values.push(pair)
    pair.left = 3
    if original.left != 1 { return 61 }
    if changed.left != 2 { return 62 }
    if total(changed) != 9 { return 63 }
    let saved = work.values.get(0)
    if saved.left != 2 { return 64 }
    if saved.right != 7 { return 65 }
    work.values.push(pair)
    if work.values.get(1).left != 3 { return 66 }
    for round in 0..2 {
        let before = pair.left
        let snapshot = pair
        pair.left += 1
        require before == 3 + round
        require snapshot.left == before
        require pair.left == before + 1
    }
    require pair.left == 5
    return 0
}"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_checked(source, opt);
        let result =
            execute_compiled(&compiled, &[cellscript::EntryWitnessArg::Bytes([1u64.to_le_bytes(), 7u64.to_le_bytes()].concat())]);
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_modified_record_fields_preserve_wide_siblings_and_whole_copies() {
    for width in [1, 8, 16, 32] {
        let source = format!(
            r#"module modified_record_fields
struct Record {{ left: u64, payload: [u8; {width}], right: u64 }}
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<Record> {{ items: 4 }}
}}
action inspect(witness input: Record, witness next: [u8; {width}], witness change: bool) -> u64 {{
    let mut record = input
    let original = record
    let prior = record.payload
    if change {{ record.payload = next }}
    let changed = record
    let mut work = Work::new()
    work.values.push(record)
    record.left += 1
    record.payload = input.payload
    require original.payload == input.payload
    require prior == input.payload
    let saved = work.values.get(0)
    require saved.left == input.left
    require saved.right == input.right
    require changed.left == input.left
    require changed.right == input.right
    if change {{
        require saved.payload == next
        require changed.payload == next
    }} else {{
        require saved.payload == input.payload
        require changed.payload == input.payload
    }}
    require record.left == input.left + 1
    require record.payload == input.payload
    require record.right == input.right
    return 0
}}"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, opt);
            for change in [false, true] {
                let result = execute_compiled(
                    &compiled,
                    &[
                        cellscript::EntryWitnessArg::Bytes(
                            [1u64.to_le_bytes().to_vec(), vec![17; width], 7u64.to_le_bytes().to_vec()].concat(),
                        ),
                        cellscript::EntryWitnessArg::Bytes(vec![29; width]),
                        cellscript::EntryWitnessArg::Bool(change),
                    ],
                );
                assert_eq!(result.exit_code, 0, "width={width}/{opt}/change={change}: {result:?}");
            }
        }
    }
}

#[test]
fn allocated_value_projection_refresh_preserves_explicit_local_writes() {
    let source = r#"module projection_writes
struct Pair { left: u64, right: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
action inspect() -> u64 {
    let mut pair = Pair { left: 1, right: 7 }
    pair.left = 2
    let mut values = [3, 4]
    values[0] = 5
    let mut work = Work::new()
    work.values.push(pair.left)
    work.values.push(values[0])
    require work.values.get(0) == 2
    require work.values.get(1) == 5
    pair = Pair { left: 11, right: 13 }
    values = [17, 19]
    work.values.push(pair.left)
    work.values.push(values[0])
    require work.values.get(2) == 11
    require work.values.get(3) == 17
    return 0
}"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        assert_eq!(execute(source, opt).exit_code, 0);
    }
}

#[test]
fn allocated_sorted_inputs_keep_evaluated_fields_and_local_snapshots() {
    for count in [1, 8, 32] {
        let source = format!(
            r#"module sorted_snapshots
struct Batch {{ keys: [u8; {count}] }}
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    saved: OrderedSet<u8> {{ items: 32 }}
    recent: OrderedSet<u8> {{ items: 32 }}
}}
action inspect(witness first: Batch, witness second: Batch) -> u64 {{
    let mut parent = first
    let mut chosen = parent.keys
    let snapshot = chosen
    parent = second
    chosen = parent.keys
    if !(snapshot == first.keys) {{ return 51 }}
    if !(chosen == second.keys) {{ return 52 }}
    let mut work = Work::new()
    work.saved.load_sorted(snapshot)
    work.recent.load_sorted(chosen)
    if !(work.saved.len() == {count}) {{ return 41 }}
    if !(work.recent.len() == {count}) {{ return 42 }}
    if !work.saved.contains(1) {{ return 43 }}
    if work.saved.contains(101) {{ return 44 }}
    if !work.recent.contains(101) {{ return 45 }}
    if work.recent.contains(1) {{ return 46 }}
    for round in 0..2 {{
        let current = parent.keys
        let copied_parent = parent
        if round == 0 {{
            require current == second.keys
            parent = first
        }} else {{
            require current == first.keys
            parent = second
        }}
        require copied_parent.keys == current
    }}
    return 0
}}"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, opt);
            let result = execute_compiled(
                &compiled,
                &[
                    cellscript::EntryWitnessArg::Bytes((1..=count).map(|i| i as u8).collect()),
                    cellscript::EntryWitnessArg::Bytes((101..101 + count).map(|i| i as u8).collect()),
                ],
            );
            assert_eq!(result.exit_code, 0, "{count}/{opt}: {result:?}");
        }
    }
}

#[test]
fn allocated_sorted_maximum_and_late_invalid_inputs_match_btree() {
    let mut observations = std::collections::BTreeMap::new();
    let source = r#"module sorted_maximum
collections Work { limits { bytes: 8192, allocations: 2 }
    totals: OrderedMap<u16, u8> { items: 1024 }
    padding: Vec<u64> { items: 636 }
}
action inspect(witness input: [(u16, u8); 1024], witness count: u64, witness sum: u64) -> u64 {
    let mut work = Work::new()
    // PRECHARGE
    work.totals.load_sorted(input)
    require work.totals.len() == count
    let mut actual = 0
    for (key, value) in work.totals { actual = actual + value }
    require actual == sum
    return 0
}"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for precharge in [false, true] {
            let text = source.replace("// PRECHARGE", if precharge { "work.padding.reserve(636)" } else { "" });
            let compiled = compile_checked(&text, opt);
            for pattern in 0..4 {
                let entries = (0..1024u64)
                    .map(|index| {
                        let key = match pattern {
                            1 => 1,
                            2 if index == 1023 => 1021,
                            3 if index == 1023 => 1022,
                            _ => index,
                        };
                        (key, index % 251)
                    })
                    .collect::<Vec<_>>();
                let model = entries.iter().copied().collect::<std::collections::BTreeMap<_, _>>();
                let arguments = [
                    cellscript::EntryWitnessArg::Bytes(
                        entries
                            .iter()
                            .flat_map(|&(key, value)| {
                                let [low, high] = (key as u16).to_le_bytes();
                                [low, high, value as u8]
                            })
                            .collect(),
                    ),
                    cellscript::EntryWitnessArg::U64(model.len() as u64),
                    cellscript::EntryWitnessArg::U64(model.values().sum()),
                ];
                let result = execute_compiled(&compiled, &arguments);
                let invalid = pattern == 2 || (precharge && model.len() >= 1023);
                assert_eq!(
                    result.exit_code,
                    if invalid { 21 } else { 0 },
                    "pattern={pattern}/precharge={precharge}/{opt}: {result:?}"
                );
                if opt.is_standard() {
                    let key = format!("precharge-{precharge}-pattern-{pattern}");
                    observations.insert(key, resources::observe(&compiled, &text, &arguments, &result));
                }
            }
        }
    }
    let oversized = source.replace("; 1024]", "; 1025]");
    let error =
        cellscript::compile(&oversized, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() }).unwrap_err();
    assert!(error.to_string().contains("1024"), "{error}");
    resources::check_sorted(observations);
}

#[test]
fn allocated_sorted_batch_validates_order_and_keeps_last_duplicate_value() {
    let source = r#"
module sorted_batch
collections Work { limits { bytes: 512, allocations: 4 }
totals: OrderedMap<u64, u64> { items: 4 }
seen: OrderedSet<u64> { items: 4 }
}
action inspect(witness entries: [(u64, u64); 4], witness keys: [u64; 4], witness expected: u64) -> u64 {
    let mut work = Work::new()
    work.totals.load_sorted(entries)
    work.seen.load_sorted(keys)
    require work.totals.len() == work.seen.len()
    let mut sum = 0
    for (key, value) in work.totals {
        require work.seen.contains(key)
        sum = sum + value
    }
    require sum == expected
    return 0
}
"#;
    for keys in [[1u64, 2, 3, 4], [1, 1, 3, 3], [1, 1, 1, 1], [1, 3, 2, 4], [4, 3, 2, 1]] {
        let values = [7u64, 11, 13, 17];
        let model: std::collections::BTreeMap<_, _> = keys.into_iter().zip(values).collect();
        let args = [
            cellscript::EntryWitnessArg::Bytes(
                keys.into_iter().zip(values).flat_map(|(key, value)| [key, value]).flat_map(u64::to_le_bytes).collect(),
            ),
            cellscript::EntryWitnessArg::Bytes(keys.into_iter().flat_map(u64::to_le_bytes).collect()),
            cellscript::EntryWitnessArg::U64(model.values().sum()),
        ];
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let result = execute_with_args(source, opt, &args);
            assert_eq!(result.exit_code, if keys.is_sorted() { 0 } else { 21 }, "{keys:?}/{opt}: {result:?}");
        }
    }
    let literal = r#"module sorted_literals
collections Work { limits { bytes: 256, allocations: 2 }
totals: OrderedMap<u64, u64> { items: 2 } }
action inspect() -> u64 {
let mut work = Work::new()
work.totals.load_sorted([(1, 7), (1, 11), (2, 13)])
require work.totals.len() == 2
require work.totals.get(1) == 11
require work.totals.get(2) == 13
return 0
}"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        assert_eq!(execute(literal, opt).exit_code, 0);
        let small = literal.replace("u64, u64", "u8, u8");
        assert_eq!(execute(&small, opt).exit_code, 0);
        let variable = literal.replace(
            "work.totals.load_sorted([(1, 7), (1, 11), (2, 13)])",
            "let entries = [(1, 7), (1, 11), (2, 13)]\nwork.totals.load_sorted(entries)",
        );
        assert_eq!(execute(&variable, opt).exit_code, 0);
        let empty = literal.replace(
            "work.totals.load_sorted([(1, 7), (1, 11), (2, 13)])",
            "work.totals.load_sorted([])\nwork.totals.reserve(2)\nwork.totals.load_sorted([(1, 7), (1, 11), (2, 13)])",
        );
        assert_eq!(execute(&empty, opt).exit_code, 0);
        let nonempty = literal.replace("require work.totals.len()", "work.totals.load_sorted([])\nrequire work.totals.len()");
        assert_eq!(execute(&nonempty, opt).exit_code, 21);
        let over_items = literal.replace("(1, 11), (2, 13)", "(2, 11), (3, 13)");
        assert_eq!(execute(&over_items, opt).exit_code, 21);
        let two_members = include_str!("syntax_combo/seeds/collections-sorted.cell");
        assert_eq!(execute(two_members, opt).exit_code, 0);
        for exhausted in [two_members.replace("allocations: 2", "allocations: 1"), two_members.replace("bytes: 256", "bytes: 64")] {
            assert_eq!(execute(&exhausted, opt).exit_code, 21);
        }
    }
}

#[test]
fn allocated_records_import_generic_values_and_helpers_without_losing_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = camino::Utf8Path::from_path(directory.path()).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"imported-records\"\nversion = \"1.0.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    let sources = root.join("src");
    std::fs::create_dir(&sources).unwrap();
    let root = sources.as_path();
    std::fs::write(root.join("values.cell"), "module values\npublic struct Pair<T: fixed_value> { left: T, right: T }\npublic fn swap<T: fixed_value>(pair: Pair<T>) -> Pair<T> { return Pair<T> { left: pair.right, right: pair.left } }").unwrap();
    std::fs::write(root.join("other.cell"), "module other\npublic struct Pair<T: fixed_value> { value: T }").unwrap();
    let input = root.join("main.cell");
    std::fs::write(
        &input,
        r#"
module imported_records
use values::Pair as Duo
use values::swap as flip
use other::Pair as Other
collections Work { limits { bytes: 512, allocations: 4 }
values: Vec<Duo<u64>> { items: 4 }
unused: Vec<Other<u64>> { items: 4 }
}
fn first(view: &Work) -> Duo<u64> { return view.values.get(0) }
action inspect(witness pair: Duo<u64>) -> u64 {
    let mut work = Work::new()
    work.values.push(pair)
    let copied = flip<u64>(first(&work))
    let mut updated = pair
    updated.left += 1
    let snapshot = updated
    work.values.push(updated)
    updated.right += 1
    let changed = work.values.get(1)
    work.drop()
    require copied.left == pair.right
    require copied.right == pair.left
    require changed.left == pair.left + 1
    require changed.right == pair.right
    require snapshot.left == pair.left + 1
    require snapshot.right == pair.right
    require updated.right == pair.right + 1
    return 0
}
"#,
    )
    .unwrap();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_file_with_entry_action(
            &input,
            CompileOptions {
                edition: CellScriptEdition::Edition2027,
                target: Some("riscv64-elf".into()),
                source_contracts: true,
                diagnostics: opt,
                ..Default::default()
            },
            "inspect",
        )
        .unwrap();
        compiled.validate().unwrap();
        let typed = &compiled.verified_lowering_record.as_ref().unwrap().typed_semantics;
        for (module, prefix, fields) in [("values", "Duo__mono__", 2), ("other", "Other__mono__", 1)] {
            assert!(typed.instantiations.iter().any(|instance| instance.module == module && instance.template == "Pair"));
            assert_eq!(typed.types.iter().find(|ty| ty.name.starts_with(prefix)).unwrap().fields.len(), fields);
        }
        let result = execute_compiled(
            &compiled,
            &[cellscript::EntryWitnessArg::Bytes([17u64, 29].into_iter().flat_map(u64::to_le_bytes).collect())],
        );
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_short_byte_snapshots_survive_reassignment_and_owner_drop() {
    for width in 1..=8 {
        for annotated in [false, true] {
            let annotation = if annotated { format!(": [u8; {width}]") } else { String::new() };
            let source = format!(
                r#"
module short_byte_snapshots
struct Value {{ bytes: [u8; {width}] }}
collections Work {{ limits {{ bytes: 512, allocations: 8 }}
    values: Vec<[u8; {width}]> {{ items: 4 }}
}}
fn read(view: &Work) -> [u8; {width}] {{ return view.values.get(0) }}
action inspect(witness first: [u8; {width}], witness second: [u8; {width}], witness record: Value) -> u64 {{
    let mut work = Work::new()
    work.values.push(first)
    let mut current = work.values.get(0)
    require current == first
    let saved{annotation} = current
    work.values.set(0, second)
    current = read(&work)
    work.values.push(saved)
    let copied = work.values.get(1)
    work.drop()
    require saved == first
    require copied == first
    require current == second
    require saved != current
    return 0
}}
"#
            );
            for initial in ["work.values.get(0)".into(), "first".into(), format!("b\"{}\"", "a".repeat(width)), "record.bytes".into()]
            {
                let source = source.replace("let mut current = work.values.get(0)", &format!("let mut current = {initial}"));
                for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                    let mut second = vec![0xfe; width];
                    second[width - 1] = 0xff;
                    let args = [
                        cellscript::EntryWitnessArg::Bytes(vec![b'a'; width]),
                        cellscript::EntryWitnessArg::Bytes(second),
                        cellscript::EntryWitnessArg::Bytes(vec![b'a'; width]),
                    ];
                    let result = execute_with_args(&source, opt, &args);
                    assert_eq!(result.exit_code, 0, "{width} bytes/{opt}/annotated={annotated}/initial={initial}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn allocated_short_bytes_index_the_value_across_storage_representations() {
    for width in 1..=8 {
        let literal = &"abcdefgh"[..width];
        let source = format!(
            r#"
module short_byte_indexes
struct Value {{ bytes: [u8; {width}] }}
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<[u8; {width}]> {{ items: 2 }}
}}
action inspect(witness record: Value, witness index: u64, witness expected: u8) -> u64 {{
    let constant = b"{literal}"
    let mut local = constant
    let mut work = Work::new()
    work.values.push(constant)
    let copied = work.values.get(0)
    work.drop()
    require constant[index] == expected
    require local[index] == expected
    require copied[index] == expected
    require record.bytes[index] == expected
    local = record.bytes
    require local[index] == expected
    return 0
}}
"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = cellscript::compile(
                &source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    target: Some("riscv64-elf".into()),
                    diagnostics: opt,
                    ..Default::default()
                },
            )
            .unwrap();
            for index in [0, width as u64 - 1, width as u64, u64::MAX] {
                let args = [
                    cellscript::EntryWitnessArg::Bytes(literal.as_bytes().to_vec()),
                    cellscript::EntryWitnessArg::U64(index),
                    cellscript::EntryWitnessArg::U8(literal.as_bytes().get(index as usize).copied().unwrap_or_default()),
                ];
                let result = execute_compiled(&compiled, &args);
                let expected = if index < width as u64 {
                    0
                } else {
                    cellscript::runtime_errors::CellScriptRuntimeError::BoundsCheckFailed.code() as i64
                };
                assert_eq!(result.exit_code, expected, "{width} bytes/{opt}/index={index}: {result:?}");
            }
        }
    }
}

#[test]
fn allocated_short_array_elements_copy_into_collections() {
    for width in 1..=8 {
        for dynamic in [false, true] {
            let index = if dynamic { "index" } else { "1" };
            let source = format!(
                r#"
module short_array_elements
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<[u8; {width}]> {{ items: 2 }}
}}
action inspect(witness input: [[u8; {width}]; 2], witness replacement: [[u8; {width}]; 2], witness expected: [u8; {width}], witness expected_new: [u8; {width}], witness index: u64) -> u64 {{
    let mut work = Work::new()
    let mut current = input
    let snapshot = current
    current = replacement
    let element = snapshot[{index}]
    work.values.push(element)
    let saved = work.values.get(0)
    require saved == expected
    require input[{index}] == expected
    require current[{index}] == expected_new
    return 0
}}
"#
            );
            for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = cellscript::compile(
                    &source,
                    CompileOptions {
                        edition: CellScriptEdition::Edition2027,
                        target: Some("riscv64-elf".into()),
                        diagnostics: opt,
                        ..Default::default()
                    },
                )
                .unwrap();
                let indices = if dynamic { vec![0, 1, 2, u64::MAX] } else { vec![1] };
                for index in indices {
                    let mut input = vec![17; width];
                    input.extend(vec![29; width]);
                    let mut replacement = vec![41; width];
                    replacement.extend(vec![53; width]);
                    let result = execute_compiled(
                        &compiled,
                        &[
                            cellscript::EntryWitnessArg::Bytes(input),
                            cellscript::EntryWitnessArg::Bytes(replacement),
                            cellscript::EntryWitnessArg::Bytes(vec![if index == 0 { 17 } else { 29 }; width]),
                            cellscript::EntryWitnessArg::Bytes(vec![if index == 0 { 41 } else { 53 }; width]),
                            cellscript::EntryWitnessArg::U64(index),
                        ],
                    );
                    let expected = if index < 2 {
                        0
                    } else {
                        cellscript::runtime_errors::CellScriptRuntimeError::BoundsCheckFailed.code() as i64
                    };
                    assert_eq!(result.exit_code, expected, "{width}/dynamic={dynamic}/index={index}/{opt}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn allocated_wide_arguments_use_evaluated_fields_after_parent_reassignment() {
    for width in [1, 8, 16, 32] {
        let source = format!(
            r#"
module evaluated_fields
struct Value {{ bytes: [u8; {width}] }}
collections Work {{ limits {{ bytes: 1024, allocations: 8 }}
    values: Vec<[u8; {width}]> {{ items: 2 }}
    totals: OrderedMap<u64, [u8; {width}]> {{ items: 2 }}
}}
fn missing(view: &Work, fallback: [u8; {width}]) -> [u8; {width}] {{
    return view.totals.get_or(99, fallback)
}}
action inspect(witness first: Value, witness second: Value) -> u64 {{
    let mut direct = first.bytes
    let snapshot = direct
    direct = second.bytes
    if !(snapshot == first.bytes) {{ return 41 }}
    if !(direct == second.bytes) {{ return 42 }}
    let mut parent = first
    let copied_field = parent.bytes
    parent = second
    let mut work = Work::new()
    work.values.push(copied_field)
    work.totals.insert(1, copied_field)
    if !(work.values.get(0) == first.bytes) {{ return 43 }}
    if !(work.totals.get(1) == first.bytes) {{ return 44 }}
    if !(work.totals.get_or(99, copied_field) == first.bytes) {{ return 45 }}
    if !(missing(&work, copied_field) == first.bytes) {{ return 46 }}
    if !(parent.bytes == second.bytes) {{ return 47 }}
    return 0
}}
"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let args = [cellscript::EntryWitnessArg::Bytes(vec![17; width]), cellscript::EntryWitnessArg::Bytes(vec![29; width])];
            let result = execute_with_args(&source, opt, &args);
            assert_eq!(result.exit_code, 0, "{width} bytes/{opt}: {result:?}");
        }
    }
}

#[test]
fn allocated_record_snapshots_survive_local_reassignment_and_owner_drop() {
    for width in [8, 16, 32, 64] {
        for annotated in [false, true] {
            let annotation = if annotated { ": Value" } else { "" };
            let source = format!(
                r#"
module record_snapshots
struct Value {{ bytes: [u8; {width}] }}
collections Work {{ limits {{ bytes: 1024, allocations: 8 }}
    values: Vec<Value> {{ items: 4 }}
}}
fn read(view: &Work) -> Value {{ return view.values.get(0) }}
action inspect(witness first: Value, witness second: Value) -> u64 {{
    let mut work = Work::new()
    work.values.push(first)
    let mut current = read(&work)
    let saved{annotation} = current
    work.values.set(0, second)
    current = read(&work)
    work.drop()
    require saved.bytes == first.bytes
    require current.bytes == second.bytes
    require saved.bytes != current.bytes
    return 0
}}
"#
            );
            for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let mut second = vec![17; width];
                second[width - 1] = 29;
                let args = [cellscript::EntryWitnessArg::Bytes(vec![17; width]), cellscript::EntryWitnessArg::Bytes(second)];
                let result = execute_with_args(&source, opt, &args);
                assert_eq!(result.exit_code, 0, "{width} bytes/{opt}/annotated={annotated}: {result:?}");
            }
        }
    }
}

#[test]
fn allocated_records_cover_scalar_sized_and_maximum_copied_layouts() {
    for width in [1, 4, 8, 32, 64] {
        let source = format!(
            r#"
module record_widths
struct Value<T: fixed_value> {{ bytes: T }}
collections Work {{ limits {{ bytes: 1024, allocations: 8 }}
values: Vec<Value<[u8; {width}]>> {{ items: 4 }}
}}
fn copy<T: fixed_value>(value: Value<T>) -> Value<T> {{ return value }}
fn read(view: &Work) -> Value<[u8; {width}]> {{ return view.values.get(0) }}
action inspect(witness first: Value<[u8; {width}]>, witness second: Value<[u8; {width}]>) -> u64 {{
    let mut work = Work::new()
    work.values.push(first)
    let copied = copy<[u8; {width}]>(read(&work))
    work.values.set(0, second)
    let later = read(&work)
    work.drop()
    require copied.bytes == first.bytes
    require later.bytes == second.bytes
    return 0
}}
"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let args = [cellscript::EntryWitnessArg::Bytes(vec![17; width]), cellscript::EntryWitnessArg::Bytes(vec![29; width])];
            let result = execute_with_args(&source, opt, &args);
            assert_eq!(result.exit_code, 0, "{width} bytes/{opt}: {result:?}");
        }
    }
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        assert_eq!(execute(include_str!("syntax_combo/seeds/collections-records.cell"), opt).exit_code, 0);
    }
}

#[test]
fn allocated_records_copy_through_helpers_mutation_and_owner_drop() {
    let source = r#"
module record_collections
struct Pair<T: fixed_value> { left: T, right: T }
struct Envelope { tag: u64, pair: Pair<u64> }
collections Work { limits { bytes: 65536, allocations: 16 }
values: Vec<Envelope> { items: 8 }
totals: OrderedMap<u64, Pair<u64>> { items: 8 }
}
fn read(view: &Work) -> Envelope { return view.values.get(0) }
fn swap<T: fixed_value>(pair: Pair<T>) -> Pair<T> {
    return Pair<T> { left: pair.right, right: pair.left }
}
fn lookup(view: &Work, fallback: Pair<u64>) -> Pair<u64> { return view.totals.get_or(99, fallback) }
fn stacked(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, view: &Work) -> Envelope {
    if a + b + c + d + e + f + g + h != 36 {
        return Envelope { tag: 0, pair: Pair<u64> { left: 0, right: 0 } }
    }
    return read(view)
}
action inspect(witness first: Envelope, witness second: Envelope) -> u64 {
    let mut work = Work::new()
    work.values.push(first)
    work.totals.insert(2, first.pair)
    work.totals.insert(1, second.pair)
    let copied = stacked(1, 2, 3, 4, 5, 6, 7, 8, &work)
    let reversed = swap<u64>(work.totals.get(2))
    let fallback = lookup(&work, second.pair)
    work.values.set(0, second)
    work.values.push(first)
    let removed = work.values.remove(0)
    let popped = work.values.pop()
    work.totals.insert(2, second.pair)
    let mut total = 0
    for (key, value) in work.totals { total = total + key + value.left + value.right }
    require total == 3 + 2 * (second.pair.left + second.pair.right)
    work.drop()
    require copied.tag == first.tag
    require copied.pair.left == first.pair.left
    require copied.pair.right == first.pair.right
    require reversed.left == first.pair.right
    require reversed.right == first.pair.left
    require fallback.left == second.pair.left
    require fallback.right == second.pair.right
    require removed.tag == second.tag
    require removed.pair.left == second.pair.left
    require popped.tag == first.tag
    require popped.pair.right == first.pair.right
    return 0
}
"#;
    let args = [
        cellscript::EntryWitnessArg::Bytes([17u64, 23, 31].into_iter().flat_map(u64::to_le_bytes).collect()),
        cellscript::EntryWitnessArg::Bytes([47u64, 53, 61].into_iter().flat_map(u64::to_le_bytes).collect()),
    ];
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute_with_args(source, opt, &args);
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_known_size_examples_reserve_once_without_changing_results() {
    let hashes = [1u8, 2, 3, 4].map(|id| [id; 32]).concat();
    for (source, args) in [
        (
            include_str!("syntax_combo/seeds/collections-aggregation.cell"),
            vec![
                cellscript::EntryWitnessArg::Bytes(hashes.clone()),
                cellscript::EntryWitnessArg::Bytes([3u64, 7, 11, 13].into_iter().flat_map(u64::to_le_bytes).collect()),
                cellscript::EntryWitnessArg::U64(34),
            ],
        ),
        (include_str!("syntax_combo/seeds/collections-deduplication.cell"), vec![cellscript::EntryWitnessArg::Bytes(hashes)]),
    ] {
        let unreserved = source.lines().filter(|line| !line.contains(".reserve(4)")).collect::<Vec<_>>().join("\n");
        assert_ne!(source, unreserved);
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let before = execute_with_args(&unreserved, opt, &args);
            let after = execute_with_args(source, opt, &args);
            assert_eq!(before.exit_code, 0);
            assert_eq!(after.exit_code, 0);
            assert!(after.cycles < before.cycles, "{opt}: before={}, after={}", before.cycles, after.cycles);
            eprintln!("source-reservation {opt} before_cycles={} after_cycles={}", before.cycles, after.cycles);
        }
    }
}

#[test]
fn allocated_aggregation_and_deduplication_use_real_witnesses_and_host_oracles() {
    use cellscript::EntryWitnessArg;
    use std::collections::{BTreeMap, BTreeSet};

    let aggregation = include_str!("syntax_combo/seeds/collections-aggregation.cell");
    let deduplication = include_str!("syntax_combo/seeds/collections-deduplication.cell");
    // Equal, distinct, descending, interleaved and late-duplicate identifiers.
    for identifiers in [[1u8, 1, 1, 1], [1, 2, 3, 4], [4, 3, 2, 1], [1, 2, 1, 2], [1, 2, 3, 1]] {
        let hashes: Vec<_> = identifiers.into_iter().map(|id| [id; 32]).collect();
        for amounts in [[0u64, 0, 0, 0], [3, 7, 11, 13], [u64::MAX, 0, 0, 0], [u64::MAX, 0, 0, 1]] {
            let mut model = BTreeMap::<[u8; 32], u128>::new();
            for (asset, amount) in hashes.iter().zip(amounts) {
                *model.entry(*asset).or_default() += u128::from(amount);
            }
            let total: u128 = model.values().sum();
            let accepted = total <= u128::from(u64::MAX);
            let args = [
                EntryWitnessArg::Bytes(hashes.concat()),
                EntryWitnessArg::Bytes(amounts.into_iter().flat_map(u64::to_le_bytes).collect()),
                EntryWitnessArg::U64(total as u64),
            ];
            for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                assert_eq!(
                    execute_with_args(aggregation, opt, &args).exit_code,
                    if accepted { 0 } else { 5 },
                    "{identifiers:?}/{amounts:?}/{opt}"
                );
                if accepted {
                    let mut wrong = args.clone();
                    wrong[2] = EntryWitnessArg::U64((total as u64) ^ 1);
                    assert_eq!(execute_with_args(aggregation, opt, &wrong).exit_code, 5);
                }
            }
        }
        let distinct = hashes.iter().collect::<BTreeSet<_>>().len() == hashes.len();
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            assert_eq!(
                execute_with_args(deduplication, opt, &[EntryWitnessArg::Bytes(hashes.concat())]).exit_code,
                if distinct { 0 } else { 5 },
                "{identifiers:?}/{opt}"
            );
        }
    }
}

#[test]
fn allocated_read_only_helpers_retain_caller_arena_and_copy_results() {
    let source = format!(
        "{DECLARATION}\n{}",
        r#"
fn count(view: &Work) -> u64 { return view.values.len() }
fn read(view: &Work, index: u64) -> u64 { return view.values[index] }
fn forwarded(view: &Work, index: u64) -> u64 {
    borrow view as nested { return read(nested, index) }
}
fn local_copy(view: &Work) -> u64 {
    let mut local = Work::new()
    if view.values.is_empty() {
        local.drop()
        return 0
    }
    local.values.push(read(view, 0))
    return local.values.get(0)
}
fn sum(view: &Work, other: &Work, extra: u64) -> u64 {
    let mut total = extra
    for item in view.values { total = total + item }
    for (key, value) in view.totals {
        if !view.totals.contains_key(key) { return 0 }
        if view.totals.get(key) != value { return 0 }
        total = total + value
    }
    if !view.seen.contains(7) { return 0 }
    if view.totals.get_or(99, 13) != 13 { return 0 }
    return total + forwarded(other, 0)
}
action inspect() -> u64 {
    let mut work = Work::new()
    require local_copy(&work) == 0
    work.values.push(7)
    work.totals.insert(4, 11)
    work.seen.insert(7)
    let mut other = Work::new()
    other.values.push(3)
    require count(&work) == 1
    require local_copy(&work) == 7
    let copied = sum(&work, &other, count(&work))
    require copied == 22
    work.values.push(8)
    other.drop()
    borrow work as view { require forwarded(view, 0) == 7 }
    require copied == 22
    work.drop()
    return 0
}

"#
    );
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute(&source, opt);
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_helpers_copy_wide_results_and_handle_large_descriptor_offsets() {
    let source = r#"
module borrowed_widths
collections Work { limits { bytes: 65536, allocations: 16 }
bytes: OrderedMap<[u8; 3], [u8; 9]> { items: 4 }
wide: Vec<u128> { items: 4 }
small: Vec<[u8; 3]> { items: 4 }
}
fn bytes(view: &Work) -> [u8; 9] { return view.bytes.get_or(b"key", b"fallback!") }
fn wide(view: &Work) -> u128 { return view.wide.get(0) }
fn small(view: &Work) -> [u8; 3] { return view.small.get(0) }
fn stacked(a: &Work, b: &Work, c: &Work, d: &Work, e: &Work) -> u64 {
    return a.wide.len() + b.wide.len() + c.wide.len() + d.wide.len() + e.wide.len()
}
action inspect() -> u64 {
    let mut work = Work::new()
    work.bytes.insert(b"key", b"old value")
    work.wide.push(340282366920938463463374607431768211455)
    work.small.push(b"old")
    let copied = bytes(&work)
    let number = wide(&work)
    let short = small(&work)
    require stacked(&work, &work, &work, &work, &work) == 5
    work.bytes.insert(b"key", b"new value")
    work.wide.set(0, 7)
    work.small.set(0, b"new")
    require copied == b"old value"
    require short == b"old"
    require number == 340282366920938463463374607431768211455
    require bytes(&work) == b"new value"
    work.drop()
    return 0
}
"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute(source, opt);
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_source_vector_map_set_and_owner_move_execute() {
    let body = r#"
action inspect() -> u64 {
    let mut work = Work::new()
    require work.values.is_empty()
    work.values.push(7)
    work.values.insert(0, 3)
    work.values.set(1, 9)
    require work.values.len() == 2
    require work.values[0] == 3
    require work.values.remove(0) == 3
    return 0
}

"#.replace("require work.values.remove(0) == 3", "let removed = work.values.remove(0)\nrequire removed == 3\nlet popped = work.values.pop()\nrequire popped == 9\nlet fresh = work.totals.insert(4, 10)\nrequire fresh\nlet replaced = work.totals.insert(4, 12)\nrequire !replaced\nrequire work.totals.get(4) == 12\nrequire work.totals.get_or(5, 17) == 17\nlet seen = work.seen.insert(4)\nrequire seen\nlet repeated = work.seen.insert(4)\nrequire !repeated\nlet moved = work\nrequire moved.seen.contains(4)\nborrow moved as view { require view.totals.len() == 1 }\nmoved.drop()");
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute(&format!("{DECLARATION}\n{body}"), opt);
        assert_eq!(result.exit_code, 0, "{opt}: {result:?}");
    }
}

#[test]
fn allocated_source_iteration_growth_order_and_scope_reentry_execute() {
    let source = format!(
        "{DECLARATION}\n{}",
        r#"
action inspect() -> u64 {
    let mut work = Work::new()
    work.totals.reserve(8)
    work.values.reserve(8)
    for i in 0..8 { work.values.push(i)\nwork.totals.insert(7 - i, i + 1) }
    let mut ordinal = 0
    let mut total = 0
    for (key, amount) in work.totals {
        require key == ordinal
        require amount == 8 - ordinal
        ordinal = ordinal + 1
        total = total + amount
    }
    require total == 36
    for value in work.values { if value == 3 { continue }\nif value == 6 { break }\nrequire value < 6 }
    for i in 0..4 { let mut local = Work::new()\nlocal.values.push(i)\nrequire local.values.get(0) == i }
    let removed = work.totals.remove(4)
    require removed
    require !work.totals.contains_key(4)
    require work.totals.len() == 7
    return 0
}
"#
        .replace("\\n", "\n")
    );
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute(&source, opt);
        assert_eq!(result.exit_code, 0, "{result:?}");
    }
}

#[test]
fn allocated_source_widths_copy_values_without_pointer_aliases() {
    let source = r#"
module allocated_widths
collections Work { limits { bytes: 2048, allocations: 16 }
small: Vec<u8> { items: 4 }
wide: Vec<u128> { items: 4 }
signed: Vec<i32> { items: 4 }
short: Vec<[u8; 3]> { items: 4 }
bytes: OrderedMap<[u8; 3], [u8; 9]> { items: 4 }
}
action inspect() -> u64 {
    let mut work = Work::new()
    let small_one: u8 = 1
    work.small.push(-small_one)
    work.wide.push(340282366920938463463374607431768211455)
    let wide_zero: u128 = 0
    work.wide.push(-wide_zero)
    require work.wide.get(1) == 0
    let seven: i32 = 7
    let negative = -seven
    work.signed.push(negative)
    let minimum = seven << 31
    let wrapped = -minimum
    work.signed.push(wrapped)
    require work.signed.get(1) == minimum
    work.short.push(b"abc")
    let copied_short = work.short.get(0)
    work.short.set(0, b"def")
    require copied_short == b"abc"
    work.bytes.insert(b"key", b"old value")
    let copied = work.bytes.get(b"key")
    work.bytes.insert(b"key", b"new value")
    require copied == b"old value"
    require work.bytes.get(b"key") == b"new value"
    require work.small.get(0) == 255
    require work.wide.get(0) == 340282366920938463463374607431768211455
    require work.signed.get(0) == negative
    return 0
}
"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = execute(source, opt);
        assert_eq!(result.exit_code, 0, "{result:?}");
    }
}

#[test]
fn allocated_source_bounds_and_shared_exhaustion_fail_deterministically() {
    for (operation, argument) in [("view.values.get(index)", 1), ("view.values.get(index)", u64::MAX), ("view.totals.get(index)", 99)]
    {
        let source = format!("{DECLARATION}\nfn read(view: &Work, index: u64) -> u64 {{ return {operation} }}\naction inspect(witness index: u64) -> u64 {{ let mut work = Work::new()\nwork.values.push(7)\nwork.totals.insert(0, 7)\nrequire read(&work, index) == 7\nreturn 0 }}");
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            assert_eq!(execute_with_args(&source, opt, &[cellscript::EntryWitnessArg::U64(0)]).exit_code, 0);
            assert_eq!(execute_with_args(&source, opt, &[cellscript::EntryWitnessArg::U64(argument)]).exit_code, 21);
        }
    }
    for body in [
        "work.values.pop()",
        "work.values.get(0)",
        "work.values.reserve(9)",
        "for i in 0..9 { work.values.push(i) }",
        "work.totals.get(99)",
    ] {
        let source = format!("{DECLARATION}\naction inspect() -> u64 {{ let mut work = Work::new()\n{body}\nreturn 0 }}");
        let result = execute(&source, cellscript::CompilerDiagnostics::default());
        assert_eq!(result.exit_code, 21, "{body}: {result:?}");
    }
    let source = "module exhaustion\ncollections Work { limits { bytes: 64, allocations: 1 }\na: Vec<u64> { items: 4 }\nb: OrderedSet<u64> { items: 4 } }\naction inspect() -> u64 { let mut work = Work::new()\nwork.a.push(1)\nwork.b.insert(1)\nreturn 0 }";
    assert_eq!(execute(source, cellscript::CompilerDiagnostics::default()).exit_code, 21);
    let source = "module checked_wide_negation\ncollections Work { limits { bytes: 128, allocations: 2 }\nvalues: Vec<u128> { items: 2 } }\naction inspect() -> u64 { let mut work = Work::new()\nlet one: u128 = 1\nwork.values.push(-one)\nreturn 0 }";
    assert_eq!(execute(source, cellscript::CompilerDiagnostics::default()).exit_code, 49);
}

#[test]
fn allocated_sorted_batch_copies_wide_values_and_compares_byte_keys() {
    for (key_ty, value_ty, key_bytes, value_bytes) in [("u64", "u128", 8, 16), ("[u8; 32]", "[u8; 32]", 32, 32)] {
        let source = format!("module sorted_wide\ncollections Work {{ limits {{ bytes: 256, allocations: 1 }}\ntotals: OrderedMap<{key_ty}, {value_ty}> {{ items: 2 }} }}\naction inspect(witness entries: [({key_ty}, {value_ty}); 3], witness key: {key_ty}, witness expected: {value_ty}) -> u64 {{ let mut work = Work::new()\nwork.totals.load_sorted(entries)\nrequire work.totals.len() == 2\nrequire work.totals.get(key) == expected\nreturn 0 }}");
        let first = vec![0u8; key_bytes];
        let mut last = first.clone();
        // Numeric little-endian and lexicographic keys both advance, but a
        // byte key shares its entire prefix with the previous key.
        last[key_bytes - 1] = 1;
        let mut entries = Vec::new();
        for (key, value) in [(&first, 7u8), (&last, 11), (&last, 13)] {
            entries.extend_from_slice(key);
            entries.extend(vec![value; value_bytes]);
        }
        let key = if key_ty == "u64" {
            cellscript::EntryWitnessArg::U64(u64::from_le_bytes(last.clone().try_into().unwrap()))
        } else {
            cellscript::EntryWitnessArg::Bytes(last)
        };
        let expected = if value_ty == "u128" {
            cellscript::EntryWitnessArg::U128(u128::from_le_bytes([13; 16]))
        } else {
            cellscript::EntryWitnessArg::Bytes(vec![13; value_bytes])
        };
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            assert_eq!(
                execute_with_args(&source, opt, &[cellscript::EntryWitnessArg::Bytes(entries.clone()), key.clone(), expected.clone()])
                    .exit_code,
                0
            );
        }
    }
}

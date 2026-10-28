use super::*;

fn source(key: &str, value: Option<&str>, count: usize, bytes: usize) -> String {
    let (member, element, contains) = if let Some(value) = value {
        (format!("OrderedMap<{key}, {value}>"), format!("({key}, {value})"), "contains_key")
    } else {
        (format!("OrderedSet<{key}>"), key.to_string(), "contains")
    };
    let params =
        if count == 0 { String::new() } else { format!("witness input: [{element}; {count}], witness other: [{element}; {count}], ") };
    let input = if count == 0 { "[]" } else { "input" };
    let items = count.max(1);
    format!(
        r#"module sorted_input_binding
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}
    values: {member} {{ items: {items} }}
    spare: {member} {{ items: {items} }}
}}
action inspect({params}witness query: {key}, witness skip: bool) -> u64 {{
    let mut work = Work::new()
    if skip {{ return 0 }}
    work.values.load_sorted({input})
    if work.values.len() > 0 {{ require work.values.{contains}(query) }}
    return 0
}}"#
    )
}

fn compile_fixture(source: &str, diagnostics: cellscript::CompilerDiagnostics) -> Fixture {
    Fixture::from_result(
        compile(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap(),
    )
}

fn sorted_ranges(valid: &Fixture) -> Vec<(String, String, u64, u64)> {
    super::allocated_arguments::ranges(valid)
        .into_iter()
        .filter(|(_, _, _, end)| {
            valid.record.blocks.iter().any(|block| {
                block.range.start == *end
                    && block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(".Lallocated_sorted_"))
            })
        })
        .collect()
}

#[test]
fn sorted_inputs_bind_typed_slots_counts_members_and_complete_preparation() {
    for (key, value) in
        [("u8", None), ("u64", None), ("Hash", None), ("u8", Some("u8")), ("u64", Some("u128")), ("[u8; 8]", Some("[u8; 8]"))]
    {
        for count in [0, 1, 4, 1024] {
            let bytes = if count <= 1 { 512 } else { 65536 };
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = compile_fixture(&source(key, value, count, bytes), diagnostics);
                let ranges = sorted_ranges(&valid);
                assert_eq!(ranges.len(), usize::from(count != 0), "{key}/{value:?}/{count}/{diagnostics}");
                for (label, _, start, end) in ranges {
                    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                    for instruction in elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)) {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, 0x13);
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                    let stem = label.strip_suffix("_begin").unwrap();
                    let mut missing = valid.clone();
                    for block in &mut missing.record.blocks {
                        block.allocated_lifecycle_labels.retain(|marker| marker != &label && marker != &format!("{stem}_end"));
                    }
                    missing.rebind_sidecars();
                    assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    for coordinate in [0, 1] {
                        let mut changed = valid.clone();
                        for block in &mut changed.record.blocks {
                            for marker in &mut block.allocated_lifecycle_labels {
                                if marker == &label || marker == &format!("{stem}_end") {
                                    let mut parts = marker
                                        .strip_prefix(".Lallocated_args_")
                                        .unwrap()
                                        .split('_')
                                        .map(str::to_string)
                                        .collect::<Vec<_>>();
                                    parts[coordinate] = (parts[coordinate].parse::<u64>().unwrap() + 1).to_string();
                                    *marker = format!(".Lallocated_args_{}", parts.join("_"));
                                }
                            }
                        }
                        changed.rebind_sidecars();
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                    if count == 1 && !diagnostics.is_standard() {
                        let branch = elf
                            .instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                instruction.word & 0x7f == 0x63 && instruction.address < start && end - instruction.address < 4096
                            })
                            .unwrap();
                        let mut bypass = valid.clone();
                        bypass.redirect_conditional_branch(branch.address, end);
                        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
        }
    }
}

#[test]
fn sorted_input_substitution_changes_vm_result_and_rebound_artifact_rejects() {
    use cellscript::EntryWitnessArg;
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for (key, value) in [("u8", None), ("u64", None), ("u64", Some("u64"))] {
        for count in [1, 3] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile(
                    &source(key, value, count, 512),
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap();
                let encode = |base: u64| {
                    (0..count)
                        .flat_map(|index| {
                            let key_value = base + index as u64;
                            let mut bytes = if key == "u8" { vec![key_value as u8] } else { key_value.to_le_bytes().to_vec() };
                            if value.is_some() {
                                bytes.extend(17u64.to_le_bytes());
                            }
                            bytes
                        })
                        .collect::<Vec<_>>()
                };
                let payload = compiled.metadata.actions[0]
                    .entry_witness_args(&[
                        EntryWitnessArg::Bytes(encode(1)),
                        EntryWitnessArg::Bytes(encode(11)),
                        if key == "u8" { EntryWitnessArg::U8(1) } else { EntryWitnessArg::U64(1) },
                        EntryWitnessArg::Bool(false),
                    ])
                    .unwrap();
                let valid = Fixture::from_result(compiled);
                let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.kind == "action").unwrap();
                let slot = |name| {
                    let id = entry.params.iter().find(|param| param.name == name).unwrap().binding_id;
                    entry.locals.iter().find(|local| local.id == id).unwrap().source_id as i32 * 8
                };
                let (_, _, start, end) = sorted_ranges(&valid).remove(0);
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let binding = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        (start..end).contains(&instruction.address)
                            && word >> 7 & 31 == 14
                            && word >> 15 & 31 == 2
                            && (word as i32) >> 20 == slot("input")
                            && matches!(word & 0x707f, 0x13 | 0x3003)
                    })
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(binding.address, replace_i_immediate(binding.word, slot("other")));
                let mut semantic_substitution = valid.clone();
                let other = entry.params.iter().find(|param| param.name == "other").unwrap().binding_id;
                let op = semantic_substitution
                    .record
                    .typed_semantics
                    .entries
                    .iter_mut()
                    .flat_map(|entry| &mut entry.blocks)
                    .flat_map(|block| &mut block.operations)
                    .find(|op| {
                        matches!(&op.detail, TypedSemanticOperationDetail::AllocatedValue { operation, .. }
                        if *operation == cellscript_artifact_checker::allocated_values::Operation::LoadSorted)
                    })
                    .unwrap();
                op.operands[1].local = Some(other);
                semantic_substitution.rebind_typed_semantics();
                assert_code(&semantic_substitution, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let execute = |artifact: &[u8]| {
                    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(artifact), &transaction)
                };
                assert_eq!(execute(&valid.artifact).exit_code, 0, "{key}/{value:?}/{count}/{diagnostics}");
                assert_eq!(execute(&changed.artifact).exit_code, 5);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn sorted_inputs_bind_short_tuple_field_representation() {
    let template = r#"module sorted_tuple_field
collections Work { limits { bytes: 512, allocations: 2 }
    keys: OrderedSet<u8> { items: 8 }
}
fn make(input: [u8; 8]) -> ([u8; 8], u64) { return (input, 7) }
action inspect(witness input: [u8; 8]) -> u64 {
    HOLDER
    let mut work = Work::new()
    work.keys.load_sorted(holder.0)
    require work.keys.len() > 0
    return 0
}"#;
    for holder in ["let mut holder = (input, 7)", "let holder = make(input)"] {
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = compile_fixture(&template.replace("HOLDER", holder), opt);
            let (_, _, start, end) = sorted_ranges(&valid).remove(0);
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let load = elf
                .instructions
                .iter()
                .find(|instruction| {
                    (start..end).contains(&instruction.address)
                        && matches!(instruction.word & 0x707f, 0x3003 | 0x13)
                        && instruction.word >> 7 & 31 == 14
                })
                .expect("sorted input binds its typed source");
            let mut changed = valid.clone();
            // O3 may forward the immediately preceding checked store. Flip
            // load/address formation even in that case, retaining the base
            // and displacement and rebinding every artifact identity.
            let opcode = if load.word & 0x707f == 0x3003 { 0x13 } else { 0x3003 };
            changed.replace_machine_word(load.address, (load.word & !0x707f) | opcode);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

use super::*;

const PREFIX: &str = ".Lallocated_scalar_";

fn fixture(ty: &str, diagnostics: cellscript::CompilerDiagnostics) -> Fixture {
    let source = crate::allocated_scalar_definitions::source(ty);
    Fixture::from_result(
        compile(
            &source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap_or_else(|error| panic!("{ty}/{diagnostics}: {error}")),
    )
}

fn ranges(valid: &Fixture) -> Vec<(String, u64, u64)> {
    valid
        .record
        .blocks
        .iter()
        .flat_map(|block| {
            block.allocated_lifecycle_labels.iter().filter_map(|label| {
                if !label.starts_with(PREFIX) || !label.ends_with("_begin") {
                    return None;
                }
                let end = format!("{}_end", label.strip_suffix("_begin").unwrap());
                Some((
                    label.clone(),
                    block.range.start,
                    valid
                        .record
                        .blocks
                        .iter()
                        .find(|candidate| candidate.allocated_lifecycle_labels.contains(&end))
                        .unwrap()
                        .range
                        .start,
                ))
            })
        })
        .collect()
}

fn check_arithmetic_operands_guards_and_destinations(ty: &str, opt: cellscript::CompilerDiagnostics) {
    let valid = fixture(ty, opt);
    let ranges = ranges(&valid);
    assert!(ranges.len() >= 16, "{ty}/{opt}");
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for (label, start, end) in ranges {
        for instruction in elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)) {
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, 0x13);
            let rejected = changed.check().unwrap_err();
            assert!(
                rejected == CheckerRejectionCode::V2420TypedMachineBindingInvalid
                    || (matches!(instruction.word & 0x7f, 0x63 | 0x6f) && rejected == CheckerRejectionCode::V2414ControlFlowInvalid),
                "{ty}/{opt}/{label}/{:#x}: {rejected:?}",
                instruction.address
            );
            if instruction.word & 0x7f == 0x63 {
                let mut predicate = valid.clone();
                predicate.replace_machine_word(instruction.address, instruction.word ^ (1 << 12));
                assert_code(&predicate, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
        let stem = label.strip_suffix("_begin").unwrap();
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.allocated_lifecycle_labels.retain(|marker| marker != &label && marker != &format!("{stem}_end"));
        }
        missing.rebind_sidecars();
        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

// Each case has independent in-memory artifacts. Keep the complete former
// type/optimization Cartesian product while letting the test runner schedule
// cases separately; no instruction mutation or assertion is sampled or skipped.
macro_rules! arithmetic_cases {
    ($(($name:ident, $ty:literal, $opt:expr)),+ $(,)?) => {
        $(#[test]
        fn $name() {
            check_arithmetic_operands_guards_and_destinations($ty, $opt);
        })+
    };
}

arithmetic_cases! {
    (scalar_definitions_bind_arithmetic_u8_diagnostic, "u8", cellscript::CompilerDiagnostics::without_optimizations()),
    (scalar_definitions_bind_arithmetic_u8_standard, "u8", cellscript::CompilerDiagnostics::default()),
    (scalar_definitions_bind_arithmetic_u16_diagnostic, "u16", cellscript::CompilerDiagnostics::without_optimizations()),
    (scalar_definitions_bind_arithmetic_u16_standard, "u16", cellscript::CompilerDiagnostics::default()),
    (scalar_definitions_bind_arithmetic_u32_diagnostic, "u32", cellscript::CompilerDiagnostics::without_optimizations()),
    (scalar_definitions_bind_arithmetic_u32_standard, "u32", cellscript::CompilerDiagnostics::default()),
    (scalar_definitions_bind_arithmetic_i32_diagnostic, "i32", cellscript::CompilerDiagnostics::without_optimizations()),
    (scalar_definitions_bind_arithmetic_i32_standard, "i32", cellscript::CompilerDiagnostics::default()),
    (scalar_definitions_bind_arithmetic_u64_diagnostic, "u64", cellscript::CompilerDiagnostics::without_optimizations()),
    (scalar_definitions_bind_arithmetic_u64_standard, "u64", cellscript::CompilerDiagnostics::default()),
}

#[test]
fn scalar_definitions_require_complete_inventory_and_typed_coordinates() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixture("u64", opt);
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| !label.starts_with(PREFIX));
        }
        missing.rebind_sidecars();
        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        for (label, _, _) in ranges(&valid) {
            for coordinate in [0, 1] {
                let mut changed = valid.clone();
                let stem = label.strip_suffix("_begin").unwrap();
                for block in &mut changed.record.blocks {
                    for marker in &mut block.allocated_lifecycle_labels {
                        if marker == &label || marker == &format!("{stem}_end") {
                            let mut parts = marker.strip_prefix(PREFIX).unwrap().split('_').map(str::to_string).collect::<Vec<_>>();
                            parts[coordinate] = (parts[coordinate].parse::<u32>().unwrap() + 1).to_string();
                            *marker = format!("{PREFIX}{}", parts.join("_"));
                        }
                    }
                }
                changed.rebind_sidecars();
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn scalar_definitions_reject_rebound_semantics_and_guard_entry() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixture("u64", diagnostics);
        for substitute_operand in [false, true] {
            let mut changed = valid.clone();
            let op = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .flat_map(|entry| &mut entry.blocks)
                .flat_map(|block| &mut block.operations)
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
                .unwrap();
            if substitute_operand {
                op.operands[0] = op.operands[1].clone();
            } else {
                op.detail = TypedSemanticOperationDetail::BinaryOperator { operator: "sub".into() };
            }
            changed.rebind_typed_semantics();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let source = r#"module scalar_guard_entry
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
action inspect(witness a: u64, witness b: u64, witness skip: bool) -> u64 {
    let mut work = Work::new()
    if skip { return 0 }
    let value = a / b
    work.values.push(value)
    return 0
}"#;
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (_, start, end) = ranges(&valid)
            .into_iter()
            .find(|(_, start, end)| {
                elf.control_flow
                    .iter()
                    .any(|edge| (*start..*end).contains(&edge.address) && edge.target > *start && edge.target < *end)
            })
            .unwrap();
        let target = elf
            .control_flow
            .iter()
            .find(|edge| (start..end).contains(&edge.address) && edge.target > start && edge.target < end)
            .unwrap()
            .target;
        let branch =
            elf.instructions.iter().rev().find(|instruction| instruction.address < start && instruction.word & 0x7f == 0x63).unwrap();
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, target);
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn scalar_definitions_bind_large_stack_slots() {
    let definitions = (0..270).map(|i| format!("let v{i} = a + {i}\n")).collect::<String>();
    // Keep 270 live values to exercise large frame slots, while respecting the
    // independent per-expression parser operation budget (256 operators).
    fn sum_expression(names: &[String]) -> String {
        if names.len() == 1 {
            return names[0].clone();
        }
        let (left, right) = names.split_at(names.len() / 2);
        format!("({} + {})", sum_expression(left), sum_expression(right))
    }
    let names = (0..270).map(|i| format!("v{i}")).collect::<Vec<_>>();
    let sum = sum_expression(&names[..135]);
    let rest = sum_expression(&names[135..]);
    let source = format!(
        r#"module scalar_large_slots
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
action inspect(witness a: u64) -> u64 {{
    let mut work = Work::new()
    {definitions}
    let first_half = {sum}
    let second_half = {rest}
    let total = first_half + second_half
    work.values.push(total)
    return 0
}}"#
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (_, start, end) = ranges(&valid).pop().unwrap();
        let words = elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)).collect::<Vec<_>>();
        assert!(
            words.iter().any(|instruction| instruction.word & 0x707f == 0x13 && instruction.word >> 7 & 31 == 31),
            "expected large slot materialization"
        );
        for instruction in words {
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, 0x13);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn scalar_definitions_cannot_be_bypassed_as_a_complete_range() {
    let source = r#"module scalar_definition_bypass
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(owner: &Work, a: u64, b: u64, skip: bool) -> u64 {
    let initial = owner.values.get(0)
    if skip { return initial }
    let value = a + b
    return value
}
action inspect(witness skip: bool) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    return read(&work, 2, 4, skip)
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let valid = Fixture::from_result(compiled.clone());
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("read")).unwrap();
        let (block, op) = entry
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter().map(move |op| (block, op)))
            .find(|(_, op)| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let prefix = format!("{PREFIX}{}_{}_", block.id, op.index);
        let (_, start, end) = ranges(&valid).into_iter().find(|(label, _, _)| label.starts_with(&prefix)).unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let branch =
            elf.instructions.iter().rev().find(|instruction| instruction.address < start && instruction.word & 0x7f == 0x63).unwrap();
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, end);
        use cellscript::EntryWitnessArg as A;
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut differs = false;
        for skip in [false, true] {
            let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Bool(skip)]).unwrap();
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
            transaction.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let original = ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code;
            assert_eq!(original, if skip { 7 } else { 6 });
            differs |= ckb_script_runner::execute_cellscript_script(&bypass.artifact, &transaction).exit_code != original;
        }
        assert!(differs, "{diagnostics}: bypass must change actual execution");
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn scalar_definitions_cannot_be_bypassed_by_a_forged_returning_syscall() {
    let source = r#"module scalar_definition_local_bypass
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(owner: &Work, a: u64, b: u64, left: Hash, right: Hash) -> u64 {
    let initial = owner.values.get(0)
    let equal = left == right
    let value = a + b
    if equal { return value }
    return initial
}
action inspect(witness left: Hash, witness right: Hash) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    return read(&work, 2, 4, left, right)
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let valid = Fixture::from_result(compiled.clone());
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("read")).unwrap();
        let (block, op) = entry
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter().map(move |op| (block, op)))
            .find(|(_, op)| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let prefix = format!("{PREFIX}{}_{}_", block.id, op.index);
        let (_, start, _) = ranges(&valid).into_iter().find(|(label, _, _)| label.starts_with(&prefix)).unwrap();
        let block_start = valid
            .record
            .blocks
            .iter()
            .filter(|candidate| candidate.owner_entry == entry.id && candidate.lowering_block_id == Some(block.id))
            .map(|candidate| candidate.range.start)
            .min()
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                (block_start..start).contains(&instruction.address)
                    && matches!(instruction.word & 0x7f, 0x6f | 0x67)
                    && instruction.word >> 7 & 31 == 1
            })
            .unwrap();
        use cellscript::EntryWitnessArg as A;
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Hash([1; 32]), A::Hash([1; 32])]).unwrap();
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 6);

        // Replace three argument loads just before the wide comparison call with a
        // successful EXIT. Its sidecar falsely describes a returning read.
        // Merely trusting that number would walk past the actual termination.
        let call_start = if call.word & 0x7f == 0x67 { call.address - 4 } else { call.address };
        let injected = call_start - 12;
        assert!(injected >= block_start);
        for pc in [injected, injected + 4, injected + 8] {
            let word = elf.instructions.iter().find(|instruction| instruction.address == pc).unwrap().word;
            assert!(matches!(word & 0x7f, 3 | 0x13));
            assert!(matches!(word >> 7 & 31, 10..=12));
        }
        let mut exit = valid.clone();
        let mut site = exit.record.syscall_sites[0].clone();
        site.address = injected + 8;
        site.block_id = exit.record.blocks.iter().find(|candidate| candidate.range.contains(site.address)).unwrap().id.clone();
        site.syscall_number = Some(2092);
        exit.record.syscall_sites.push(site);
        exit.record.canonicalize();
        exit.replace_machine_words(&[(injected, 0x05d00893), (injected + 4, 0x00000513), (injected + 8, 0x00000073)]);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&exit.artifact, &transaction).exit_code, 0);
        let error = check_bundle_values(&exit.artifact, &exit.metadata, &exit.record, &exit.source_map, &CheckerBudgets::default())
            .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("bounded operation flow"), "{}", error.message);
    }
}

#[test]
fn scalar_comparisons_bind_results_predicates_and_successors() {
    for ty in ["bool", "u64", "i32"] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    &crate::allocated_scalar_definitions::comparison_source(ty),
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("inspect")).unwrap();
            let a = entry.locals.iter().find(|local| local.name == "a").unwrap().id;
            let b = entry.locals.iter().find(|local| local.name == "b").unwrap().id;
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let spans = ranges(&valid);
            let mut materialized = 0;
            let mut fused = 0;
            for block in &entry.blocks {
                for op in &block.operations {
                    if op.opcode != "binary"
                        || op.operands.len() != 2
                        || op.operands[0].local != Some(a)
                        || op.operands[1].local != Some(b)
                    {
                        continue;
                    }
                    let prefix = format!("{PREFIX}{}_{}_", block.id, op.index);
                    let (label, start, end) = spans.iter().find(|(label, _, _)| label.starts_with(&prefix)).unwrap();
                    let instructions = elf
                        .instructions
                        .iter()
                        .filter(|instruction| (*start..*end).contains(&instruction.address))
                        .collect::<Vec<_>>();
                    let branch = instructions.iter().find(|instruction| instruction.word & 0x7f == 0x63);
                    if let Some(branch) = branch {
                        fused += 1;
                        let mut predicate = valid.clone();
                        predicate.replace_machine_word(branch.address, branch.word ^ (1 << 12));
                        assert_code(&predicate, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        let original_target = elf.control_flow.iter().find(|edge| edge.address == branch.address).unwrap().target;
                        let other = block
                            .successors
                            .iter()
                            .map(|id| {
                                valid
                                    .record
                                    .blocks
                                    .iter()
                                    .filter(|target| target.owner_entry == entry.id && target.lowering_block_id == Some(*id))
                                    .map(|target| target.range.start)
                                    .min()
                                    .unwrap()
                            })
                            .find(|target| *target != original_target)
                            .unwrap();
                        let mut target = valid.clone();
                        target.redirect_conditional_branch(branch.address, other);
                        assert_code(&target, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    } else {
                        materialized += 1;
                    }
                    for instruction in instructions {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, 0x13);
                        let rejection = changed.check().unwrap_err();
                        assert!(
                            rejection == CheckerRejectionCode::V2420TypedMachineBindingInvalid
                                || (matches!(instruction.word & 0x7f, 0x63 | 0x6f)
                                    && rejection == CheckerRejectionCode::V2414ControlFlowInvalid),
                            "{ty}/{diagnostics}/{label}/{:#x}: {rejection:?}",
                            instruction.address
                        );
                    }
                    let mut missing = valid.clone();
                    let stem = label.strip_suffix("_begin").unwrap();
                    for block in &mut missing.record.blocks {
                        block
                            .allocated_lifecycle_labels
                            .retain(|label| label != &format!("{stem}_begin") && label != &format!("{stem}_end"));
                    }
                    missing.rebind_sidecars();
                    assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
            let count = if ty == "bool" { 2 } else { 6 };
            assert_eq!((materialized, fused), if !diagnostics.is_standard() { (count * 2, 0) } else { (count, count) });
        }
    }
}

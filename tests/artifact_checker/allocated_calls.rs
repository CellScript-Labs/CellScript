use super::*;

const PREFIX: &str = ".Lallocated_call_";

#[test]
fn allocated_scalar_calls_reject_orphan_markers_without_collection_helpers() {
    let mut fixture = Fixture::new();
    fixture.record.blocks[0].allocated_lifecycle_labels.push(format!("{PREFIX}0_invoke"));
    fixture.rebind_sidecars();
    assert_code(&fixture, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

fn fixture(padding: usize) -> (Fixture, ckb_script_runner::CkbVmFixture) {
    let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
    let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
    let args = "0, ".repeat(padding);
    let source = format!(
        r#"module scalar_call_binding
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}a: u64, owner: &Work, b: u64) -> u64 {{
    if a == b {{ return 0 }}
    return owner.values.get(0)
}}
fn forward({params}a: u64, owner: &Work, b: u64) -> u64 {{ return read({forwarded}a, owner, b) }}
action inspect(witness a: u64, witness b: u64, witness skip: bool) -> u64 {{
    let mut work = Work::new()
    work.values.push(7)
    if skip {{ return 0 }}
    require forward({args}a, &work, b) == 7
    return 0
}}"#
    );
    let compiled = compile(
        &source,
        CompileOptions { edition: NEXT_EDITION, opt_level: 3, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap();
    use cellscript::EntryWitnessArg as A;
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let payload = compiled.metadata.actions[0].entry_witness_args(&[A::U64(1), A::U64(2), A::Bool(false)]).unwrap();
    let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    transaction.current_type_script_input_indices = vec![0];
    transaction.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    (Fixture::from_result(compiled), transaction)
}

pub(super) fn spans(valid: &Fixture) -> Vec<(String, String, u64, u64)> {
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
                let end =
                    valid.record.blocks.iter().find(|block| block.allocated_lifecycle_labels.contains(&end)).unwrap().range.start;
                Some((block.owner_entry.clone(), label.clone(), block.range.start, end))
            })
        })
        .collect()
}

#[test]
fn allocated_scalar_calls_bind_every_argument_and_inventory() {
    for padding in [0, 8] {
        let (valid, transaction) = fixture(padding);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let spans = spans(&valid);
        assert_eq!(spans.len(), (padding + 2) * 2);
        for (_, label, start, end) in spans {
            for instruction in elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)) {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, 0x13);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
            let mut erased = valid.clone();
            let stem = label.strip_suffix("_begin").unwrap();
            for block in &mut erased.record.blocks {
                block.allocated_lifecycle_labels.retain(|label| label != &format!("{stem}_begin") && label != &format!("{stem}_end"));
            }
            erased.rebind_sidecars();
            assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut erased = valid.clone();
        for block in &mut erased.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| !label.starts_with(PREFIX));
        }
        erased.rebind_sidecars();
        assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn allocated_scalar_calls_reject_substituted_local_and_bypassed_preparation() {
    for padding in [0, 8] {
        let (valid, transaction) = fixture(padding);
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.kind == "action").unwrap();
        let local_slot = |name| {
            let local = entry.params.iter().find(|param| param.name == name).unwrap().binding_id;
            entry.locals.iter().find(|candidate| candidate.id == local).unwrap().source_id as u32 * 8
        };
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (_, _, start, end) = spans(&valid)
            .into_iter()
            .find(|(owner, label, _, _)| {
                owner == &entry.id && label.strip_prefix(PREFIX).unwrap().split('_').nth(1).unwrap() == padding.to_string()
            })
            .unwrap();
        let load = elf
            .instructions
            .iter()
            .find(|instruction| {
                (start..end).contains(&instruction.address) && instruction.word & 0x707f == 0x3003 && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        assert_eq!(load.word >> 20, local_slot("a"));
        let mut substitute = valid.clone();
        substitute.replace_machine_word(load.address, (load.word & 0xfffff) | (local_slot("b") << 20));
        assert_code(&substitute, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&substitute.artifact, &transaction).exit_code, 0);
        let invoke = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == entry.id
                    && block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(PREFIX) && label.ends_with("_invoke"))
            })
            .unwrap()
            .range
            .start;
        let branch = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < start
                    && instruction.word & 0x7f == 0x63
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .unwrap();
        let before_stack_adjustment = spans(&valid)
            .iter()
            .filter(|(owner, _, _, end)| owner == &entry.id && *end <= invoke)
            .map(|(_, _, _, end)| *end)
            .max()
            .unwrap();
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, before_stack_adjustment);
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn allocated_scalar_calls_preserve_values_from_nested_helpers() {
    let source = r#"module scalar_call_forwarded_return
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}

fn identity(value: u64) -> u64 { return value }
fn read(value: u64, owner: &Work) -> u64 { return value + owner.values.get(0) }
fn forward(owner: &Work, value: u64) -> u64 { return read(identity(value), owner) }
action inspect(witness value: u64) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    borrow work as view {
        let actual = forward(view, value)
        if actual != value + 7 { return 81 }
    }
    return 0
}"#;
    for opt_level in [0, 3] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let payload = compiled.metadata.actions[0].entry_witness_args(&[cellscript::EntryWitnessArg::U64(3)]).unwrap();
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = Fixture::from_result(compiled);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_eq!(spans(&valid).len(), 2);
    }
}

#[test]
fn allocated_scalar_calls_reject_clobbers_after_checked_preparation() {
    let source = r#"module scalar_call_clobber
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(value: u64, owner: &Work, bytes: Hash) -> u64 { return value + owner.values.get(0) }
action inspect(witness value: u64, witness bytes: Hash) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    require read(value, &work, bytes) == value + 7
    return 0
}"#;
    for opt_level in [0, 3] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let payload = compiled.metadata.actions[0]
            .entry_witness_args(&[cellscript::EntryWitnessArg::U64(3), cellscript::EntryWitnessArg::Hash([1; 32])])
            .unwrap();
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = Fixture::from_result(compiled);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (_, _, _, end) = spans(&valid).into_iter().find(|(owner, _, _, _)| owner.starts_with("action:")).unwrap();
        let invoke = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry.starts_with("action:")
                    && block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(PREFIX) && label.ends_with("_invoke"))
            })
            .unwrap()
            .range
            .start;
        // Change the unused Hash length setup into a scalar-register clobber.
        // The checked scalar load and owner pointer/extent remain untouched.
        let length = elf
            .instructions
            .iter()
            .find(|instruction| {
                (end..invoke).contains(&instruction.address)
                    && instruction.word >> 7 & 31 == 14
                    && matches!(instruction.word & 0x7f, 3 | 0x13)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(length.address, (length.word & !(31 << 7)) | (10 << 7));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&changed.artifact, &transaction).exit_code, 0);
    }
}

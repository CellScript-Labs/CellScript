use super::*;

#[test]
fn typed_cell_properties_accept_immediate_coordinate_producers() {
    for property in ["capacity", "occupied_capacity", "unoccupied_capacity", "data_size", "output_index"] {
        let source = format!(
            "module immediate_property\nresource Token has store {{ amount: u64 }}\n\
            action inspect() -> u64 {{ let view = ckb::output<Token>(0)\nreturn view.{property}\n}}"
        );
        for opt_level in [0, 3] {
            Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
        }
    }
}

#[test]
fn typed_cell_property_coordinates_remain_live_until_the_call() {
    let source = r#"module property_coordinate_lifetime
resource Token has store { amount: u64 }
fn replacement(seed: u8) -> u64 { let value = 0
    return value
}
action inspect() -> u64 {
    let view = ckb::input<Token>(0)
    let ignored = replacement(0)
    require ignored == 0
    require view.capacity == 100000000000
    return 0
}"#;
    for opt_level in [0, 3] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name == "replacement").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
        let store = elf.instructions.iter().find(|instruction| instruction.address == call.address + 4).unwrap();
        assert_eq!(store.word & 0x707f, 0x3023);
        let action = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let property = action
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_cell_capacity"))
            .unwrap();
        let coordinate = action.locals.iter().find(|local| Some(local.id) == property.operands[0].local).unwrap().source_id as u32 * 8;
        let mut stale = valid.clone();
        let rebound = (store.word & 0x01fff07f) | ((coordinate >> 5) << 25) | ((coordinate & 31) << 7);
        stale.replace_machine_word(store.address, rebound);
        use ckb_testtool::ckb_types::bytes::Bytes;
        let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&stale.artifact, &transaction).exit_code, 0);
        assert_code(&stale, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn typed_cell_properties_bind_runtime_values_and_call_sites() {
    for (property, value_instruction) in [
        ("capacity", 0x01013503), // LD a0,16(sp)
        ("occupied_capacity", 0x01813503),
        ("unoccupied_capacity", 0x40a28533), // SUB a0,t0,a0
        ("data_size", 0x00813503),
        ("output_index", 0x00030513), // ADDI a0,t1,0
    ] {
        for group in [false, true] {
            let before = if property == "output_index" { "OutputView<Token>" } else { "InputView<Token>" };
            let first_source = if property == "output_index" { "output" } else { "input" };
            let prefix = if group { "group_" } else { "" };
            let after_index = usize::from(property == "output_index");
            let source = format!(
                r#"module property_values
resource Token has store {{ amount: u64 }}
fn same(before: {before}, after: OutputView<Token>) -> bool {{
    return before.{property} == after.{property}
}}
action inspect() -> u64 {{
    require same(ckb::{prefix}{first_source}<Token>(0), ckb::{prefix}output<Token>({after_index}))
    return 0
}}"#
            );
            for opt_level in [0, 3] {
                let valid = Fixture::from_result(
                    compile(
                        &source,
                        CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                    )
                    .unwrap_or_else(|error| panic!("{property}/group={group}/O{opt_level}: {error}")),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let runtime = format!("runtime:__ckb_cell_{property}");
                let value = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        instruction.word == value_instruction
                            && valid
                                .record
                                .blocks
                                .iter()
                                .any(|block| block.owner_entry == runtime && block.range.contains(instruction.address))
                    })
                    .unwrap();
                let mut zeroed = valid.clone();
                zeroed.replace_machine_word(value.address, 0x00000513);
                use ckb_testtool::ckb_types::bytes::Bytes;
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 2, 3);
                let index = usize::from(group);
                transaction.current_type_script_input_indices = vec![index];
                transaction.outputs[0].capacity = transaction.inputs[index].capacity + 1;
                transaction.inputs[index].data = vec![0; 8].into();
                transaction.outputs[0].data = vec![0; 9].into();
                assert_eq!(
                    ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code,
                    5,
                    "{property}/group={group}/O{opt_level}"
                );
                assert_eq!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
                assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "same").unwrap();
                let (block, op) = entry
                    .blocks
                    .iter()
                    .flat_map(|block| block.operations.iter().map(move |op| (block, op)))
                    .find(|(_, op)| op.call.as_ref().is_some_and(|call| call.target == format!("__ckb_cell_{property}")))
                    .unwrap();
                let prefix = format!(".Lallocated_scalar_{}_{}_", block.id, op.index);
                let boundary = |suffix: &str| {
                    valid
                        .record
                        .blocks
                        .iter()
                        .find(|block| {
                            block.owner_entry == entry.id
                                && block
                                    .allocated_lifecycle_labels
                                    .iter()
                                    .any(|label| label.starts_with(&prefix) && label.ends_with(suffix))
                        })
                        .unwrap()
                        .range
                        .start
                };
                let (begin, end) = (boundary("_begin"), boundary("_end"));
                let mut wrong_source = valid.clone();
                wrong_source.replace_machine_word(begin, 0x00000513);
                assert_code(&wrong_source, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                let mut wrong_result = valid.clone();
                let store = elf.instructions.iter().find(|instruction| instruction.address == end - 4).unwrap().word;
                assert_eq!(store & 0x707f, 0x3023);
                wrong_result.replace_machine_word(end - 4, store & !(31 << 20));
                assert_code(&wrong_result, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    if block.owner_entry == entry.id {
                        block.allocated_lifecycle_labels.retain(|label| !label.starts_with(&prefix));
                    }
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                if property != "data_size" {
                    let status = elf
                        .instructions
                        .iter()
                        .find(|instruction| {
                            (begin..end).contains(&instruction.address)
                                && instruction.word & 0x707f == 0x63
                                && instruction.word >> 15 & 31 == 11
                        })
                        .unwrap();
                    let mut ignored_error = valid.clone();
                    ignored_error.replace_machine_word(status.address, status.word & !(31 << 15));
                    assert_code(&ignored_error, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
                if property != "output_index" {
                    transaction.outputs[0] = transaction.inputs[index].clone();
                    assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                }
            }
        }
    }
}

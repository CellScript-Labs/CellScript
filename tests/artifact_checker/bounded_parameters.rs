use super::*;

#[test]
fn scalar_only_signatures_still_require_binding_when_helpers_create_bounded_values() {
    for body in [
        "let mut work = Work::new()\nwork.values.push(value)\nreturn work.values.get(0) + 3",
        "return value + ckb::input<Token>(0).data.amount",
    ] {
        let source = format!(
            "module local_bounded_helper\nresource Token has store {{ amount: u64 }}\n\
             collections Work {{ limits {{ bytes: 512, allocations: 4 }}\nvalues: Vec<u64> {{ items: 4 }} }}\n\
             fn bump(value: u64) -> u64 {{ {body} }}\n\
             action inspect() -> u64 {{ verification\nrequire bump(4) == 7\nreturn 0 }}"
        );
        for opt_level in [0, 3] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            use ckb_testtool::ckb_types::bytes::Bytes;
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
            transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
            assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
            let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("bump")).unwrap();
            let entry = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
            let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
            let slot = typed.locals.iter().find(|local| local.id == typed.params[0].binding_id).unwrap().source_id as u32 * 8;
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let spill = elf
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.address >= start
                        && instruction.word & 0x707f == 0x3023
                        && instruction.word >> 15 & 31 == 2
                        && instruction.word >> 20 & 31 == 10
                        && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == slot
                })
                .unwrap();
            let mut zeroed = valid.clone();
            zeroed.replace_machine_word(spill.address, spill.word & !(31 << 20));
            assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert_ne!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
            assert_eq!(allocated_calls::spans(&valid).len(), 1);
            let mut erased = valid.clone();
            for block in &mut erased.record.blocks {
                block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_call_"));
            }
            erased.rebind_sidecars();
            assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn bounded_helpers_bind_scalar_parameters_after_coordinates_and_nested_views() {
    for (ty, argument, read) in [
        ("InputView<Token>", "ckb::input<Token>(0)", "view.data.amount"),
        ("Messages", "messages", "view[0].len()"),
        ("Messages::Item", "messages[0]", "view.len()"),
    ] {
        for padding in [0, 5, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!(
                r#"module bounded_parameter_binding
resource Token has store {{ amount: u64 }}
data_shape Messages {{ element: Vec<u8>, limits {{ items: 2, payload_bytes: 8, encoded_bytes: 32 }} }}
#[effect(ReadOnly)]
fn read({params}view: {ty}, bytes: Hash, a: u8, b: u16, c: u32, d: i32, e: u64, flag: bool) -> u64 {{
    if a != 1 {{ return 0 }}
    if b != 2 {{ return 0 }}
    if c != 3 {{ return 0 }}
    if d != -(1 as i32) {{ return 0 }}
    if !flag {{ return 0 }}
    return {read} + e
}}
action inspect(witness bytes: Hash, witness messages: Messages) -> u64 {{
    verification
    require read({args}{argument}, bytes, 1, 2, 3, -(1 as i32), 4, true) == 7
    return 0
}}"#
            );
            for opt_level in [0, 3] {
                let compiled = compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap_or_else(|error| panic!("{ty}/{padding}/O{opt_level}: {error}"));
                use cellscript::EntryWitnessArg as A;
                use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
                let messages = [15u32.to_le_bytes().as_slice(), &8u32.to_le_bytes(), &3u32.to_le_bytes(), &[1, 2, 3]].concat();
                let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Hash([3; 32]), A::Bytes(messages)]).unwrap();
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let valid = Fixture::from_result(compiled);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("read")).unwrap();
                let entry = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                for param in
                    typed.params.iter().filter(|param| matches!(param.ty.as_str(), "bool" | "u8" | "u16" | "u32" | "i32" | "u64"))
                {
                    let slot = typed.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as u32 * 8;
                    let spill = elf
                        .instructions
                        .iter()
                        .find(|instruction| {
                            (start..end).contains(&instruction.address)
                                && instruction.word & 0x707f == 0x3023
                                && instruction.word >> 15 & 31 == 2
                                && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == slot
                        })
                        .unwrap();
                    let mut zeroed = valid.clone();
                    zeroed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                    assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if param.name == "a" {
                        assert_ne!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
                    }
                    if spill.word >> 20 & 31 == 5 {
                        let load = elf.instructions.iter().find(|instruction| instruction.address + 4 == spill.address).unwrap();
                        assert_eq!(load.word & 0x707f, 0x3003);
                        let mut replaced = valid.clone();
                        replaced.replace_machine_word(load.address, load.word ^ (8 << 20));
                        assert_code(&replaced, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
                let spans = allocated_calls::spans(&valid);
                assert_eq!(spans.len(), padding + 6);
                let position = typed.params.iter().position(|param| param.name == "a").unwrap();
                let (_, _, start, end) = spans.iter().find(|(_, label, _, _)| label.ends_with(&format!("_{position}_begin"))).unwrap();
                let load = elf.instructions.iter().find(|instruction| instruction.address == *start).unwrap();
                assert!(start < end);
                assert_eq!(load.word & 0xfffff07f, 0x00100013); // ADDI argument,zero,1
                let mut substituted = valid.clone();
                substituted.replace_machine_word(load.address, load.word & 0xfffff);
                assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                assert_ne!(ckb_script_runner::execute_cellscript_script(&substituted.artifact, &transaction).exit_code, 0);
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_call_"));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

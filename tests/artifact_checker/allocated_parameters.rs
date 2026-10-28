use super::*;

#[test]
fn allocated_scalar_parameters_bind_register_and_stack_words() {
    for padding in [0, 5, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let args = "0, ".repeat(padding);
        let source = format!(
            r#"module scalar_parameter_binding
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}work: &Work, bytes: Hash, a: u8, b: u16, c: u32, d: i32, e: u64, flag: bool) -> u64 {{
    if a != 1 {{ return 0 }}
    if b != 2 {{ return 0 }}
    if c != 3 {{ return 0 }}
    if d != -(1 as i32) {{ return 0 }}
    if !flag {{ return 0 }}
    return work.values.get(0) + e
}}
action inspect(witness bytes: Hash) -> u64 {{
    let mut work = Work::new()
    work.values.push(7)
    require read({args}&work, bytes, 1, 2, 3, -(1 as i32), 4, true) == 11
    return 0
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap();
            use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
            let payload = compiled.metadata.actions[0].entry_witness_args(&[cellscript::EntryWitnessArg::Hash([3; 32])]).unwrap();
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
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
            for param in typed.params.iter().filter(|param| matches!(param.ty.as_str(), "u8" | "u16" | "u32" | "i32" | "u64" | "bool"))
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
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                if param.name == "a" {
                    assert_ne!(ckb_script_runner::execute_cellscript_script(&changed.artifact, &transaction).exit_code, 0);
                }
                if spill.word >> 20 & 31 == 5 {
                    let load = elf.instructions.iter().find(|instruction| instruction.address + 4 == spill.address).unwrap();
                    assert_eq!(load.word & 0x707f, 0x3003);
                    let mut rebound = valid.clone();
                    rebound.replace_machine_word(load.address, load.word ^ (8 << 20));
                    assert_code(&rebound, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

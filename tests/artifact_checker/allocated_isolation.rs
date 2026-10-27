use super::*;

#[test]
fn inline_tuple_copies_cannot_overwrite_live_arena_elements() {
    let source = r#"module arena_copy_isolation
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn copied(value: [u8; 9]) -> ([u8; 9], u64) { return (value, 3) }
action inspect(witness bytes: [u8; 9]) -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    let result = copied(bytes)
    require result.0 == bytes
    require result.1 == 3
    require work.values.get(0) == 7
    return 0
}"#;
    for opt_level in [0, 3] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        use cellscript::EntryWitnessArg as A;
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Bytes(vec![11; 9])]).unwrap();
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = Fixture::from_result(compiled);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let begin = valid
            .record
            .blocks
            .iter()
            .find(|block| block.allocated_init_labels.iter().any(|label| label.contains("_begin_")))
            .unwrap()
            .range
            .start;
        let header = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.address >= begin && instruction.word & 0x707f == 0x3023 && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let arena = (header.word >> 25 << 5) | (header.word >> 7 & 31);
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("copied")).unwrap();
        let target = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap().address;
        let copy = elf.instructions.iter().find(|instruction| instruction.address > call && instruction.word == 0x00034283).unwrap();
        let destination = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < copy.address && instruction.address > call && instruction.word & 0xfffff == 0x10393
            })
            .unwrap();
        let mut corrupted = valid.clone();
        corrupted.replace_machine_word(destination.address, ((arena + 64) << 20) | 0x10393);
        assert_code(&corrupted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&corrupted.artifact, &transaction).exit_code, 0);
        let count = elf.instructions.iter().find(|instruction| instruction.address == copy.address - 4).unwrap();
        assert_eq!(count.word, 0x00900e13);
        let mut unbounded = valid.clone();
        unbounded.replace_machine_word(count.address, 0xfff00e13);
        assert_code(&unbounded, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let mut wrong_stride = valid.clone();
        wrong_stride.replace_machine_word(copy.address + 12, 0x00238393);
        assert_code(&wrong_stride, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn ordinary_call_results_cannot_rewrite_live_arena_storage() {
    for bytes in [128, 512] {
        let source = format!(
            r#"module arena_write_isolation
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn ordinary(value: u64) -> u64 {{ if value > 1 {{ return value - 1 }}
    return value }}
action inspect(witness zero: u64) -> u64 {{
    let mut work = Work::new()
    work.values.push(7)
    let observed = ordinary(zero)
    require observed == 0
    require work.values.get(0) == 7
    return 0
}}"#
        );
        for opt_level in [0, 3] {
            let compiled = compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap();
            use cellscript::EntryWitnessArg as A;
            use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
            let payload = compiled.metadata.actions[0].entry_witness_args(&[A::U64(0)]).unwrap();
            let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            transaction.current_type_script_input_indices = vec![0];
            transaction.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let valid = Fixture::from_result(compiled);
            assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let begin = valid
                .record
                .blocks
                .iter()
                .find(|block| block.allocated_init_labels.iter().any(|label| label.contains("_begin_")))
                .unwrap()
                .range
                .start;
            let first_store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.address >= begin && instruction.word & 0x707f == 0x3023 && instruction.word >> 15 & 31 == 2
                })
                .unwrap();
            let arena = (first_store.word >> 25 << 5) | (first_store.word >> 7 & 31);
            let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("ordinary")).unwrap();
            let target = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
            let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap().address;
            let result = elf.instructions.iter().find(|instruction| instruction.address == call + 4).unwrap();
            assert_eq!(result.word & 0xfffff, 0x13023 | (result.word & 0xf80));
            assert_eq!(result.word >> 20 & 31, 10);
            // Corrupt the allocator cursor, first record's live flag, first
            // element, descriptor length and descriptor live flag. None of the
            // independently checked collection bodies or their markers change.
            for relative in [0, 32 + 16, 32 + 32, 32 + bytes + 8, 32 + bytes + 24] {
                let offset = arena + relative;
                assert!(offset <= 2047);
                let store = (result.word & !0xfe000f80) | ((offset >> 5) << 25) | ((offset & 31) << 7);
                let mut corrupted = valid.clone();
                corrupted.replace_machine_word(result.address, store);
                let error = check_bundle_values(
                    &corrupted.artifact,
                    &corrupted.metadata,
                    &corrupted.record,
                    &corrupted.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("unverified write overlaps"), "{error}");
                assert_ne!(
                    ckb_script_runner::execute_cellscript_script(&corrupted.artifact, &transaction).exit_code,
                    0,
                    "arena={bytes} offset={relative} O{opt_level}"
                );
            }
        }
    }
}

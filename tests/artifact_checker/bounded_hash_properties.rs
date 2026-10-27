use super::*;

fn hash_ranges(fixture: &Fixture, owner: &str) -> Vec<(u64, u64)> {
    let entry = fixture.record.typed_semantics.entries.iter().find(|entry| entry.name == owner).unwrap();
    entry
        .blocks
        .iter()
        .flat_map(|block| {
            block
                .operations
                .iter()
                .filter(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_cell_data_hash_field"))
                .map(move |op| (block, op))
        })
        .map(|(block, op)| {
            let prefix = format!(".Lallocated_scalar_{}_{}_", block.id, op.index);
            let boundary = |suffix: &str| {
                fixture
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
            (boundary("_begin"), boundary("_end"))
        })
        .collect()
}

#[test]
fn typed_hash_results_keep_distinct_storage_and_reject_intervening_writes() {
    let source = r#"module hash_property_storage
resource Token has store { amount: u64 }
fn noise(seed: u8) -> u64 { let value = 0
    return value
}
fn same(before: InputView<Token>, after: OutputView<Token>) -> bool {
    let first = before.data_hash
    let status = noise(0)
    if status != 0 { return false }
    let second = after.data_hash
    return first == second
}
action inspect() -> u64 {
    require same(ckb::input<Token>(0), ckb::output<Token>(0))
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
        let ranges = hash_ranges(&valid, "same");
        assert_eq!(ranges.len(), 2);
        let size = |start| {
            let word = elf.instructions.iter().find(|instruction| instruction.address == start + 4).unwrap().word;
            assert_eq!(word & 0x01fff07f, (5 << 20) | (2 << 15) | 0x3023);
            ((word >> 25) << 5) | (word >> 7 & 31)
        };
        let (first, second) = (size(ranges[0].0), size(ranges[1].0));
        let mut replacements = Vec::new();
        for instruction in elf.instructions.iter().filter(|instruction| (ranges[1].0..ranges[1].1).contains(&instruction.address)) {
            let word = instruction.word;
            if word & 0x707f == 0x13 && word >> 15 & 31 == 2 && word >> 20 == second + 8 {
                replacements.push((instruction.address, (word & 0xfffff) | (((word >> 20) - second + first) << 20)));
            }
        }
        assert_eq!(replacements.len(), 2, "rebind the runtime output and published pointer while retaining both valid lengths");
        let mut aliased = valid.clone();
        aliased.replace_machine_words(&replacements);
        use ckb_testtool::ckb_types::bytes::Bytes;
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.inputs[0].data = vec![0; 8].into();
        transaction.outputs[0].data = vec![1; 8].into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 5);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&aliased.artifact, &transaction).exit_code, 0);
        assert_code(&aliased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let helper = valid.record.entries.iter().find(|entry| entry.name == "noise").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
        let store = elf.instructions.iter().find(|instruction| instruction.address == call.address + 4).unwrap();
        assert_eq!(store.word & 0x707f, 0x3023);
        let offset = first + 8;
        let mut overwritten = valid.clone();
        overwritten.replace_machine_word(store.address, (store.word & 0x01fff07f) | ((offset >> 5) << 25) | ((offset & 31) << 7));
        transaction.outputs[0].data = transaction.inputs[0].data.clone();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&overwritten.artifact, &transaction).exit_code, 0);
        assert_code(&overwritten, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn typed_hash_property_storage_supports_large_frames() {
    let reads = (0..16)
        .map(|index| format!("let value_{index} = before.data_hash\nif value_{index} != after.data_hash {{ return false }}\n"))
        .collect::<String>();
    let source = format!(
        "module large_hash_properties\nresource Token has store {{ amount: u64 }}\n\
        fn same(before: InputView<Token>, after: OutputView<Token>) -> bool {{ {reads} return true }}\n\
        action inspect() -> u64 {{ require same(ckb::input<Token>(0), ckb::output<Token>(0))\nreturn 0 }}"
    );
    for opt_level in [0, 3] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        assert!(
            hash_ranges(&valid, "same").iter().any(|(start, _)| elf
                .instructions
                .iter()
                .any(|instruction| instruction.address == start + 4 && instruction.word & 0x7f != 0x23)),
            "fixture must exercise a materialized large stack address"
        );
        use ckb_testtool::ckb_types::bytes::Bytes;
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.inputs[0].data = vec![0; 8].into();
        transaction.outputs[0].data = vec![0; 8].into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        transaction.outputs[0].data = vec![1; 8].into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 5);
    }
}

#[test]
fn typed_hash_properties_bind_their_coordinate_arguments() {
    for property in ["data_hash", "lock_hash", "type_hash"] {
        let source = format!(
            r#"module hash_property_sources
resource Token has store {{ amount: u64 }}
fn same(before: InputView<Token>, after: OutputView<Token>) -> bool {{
    return before.{property} == after.{property}
}}
action inspect() -> u64 {{
    require same(ckb::input<Token>(0), ckb::output<Token>(0))
    return 0
}}"#
        );
        for opt_level in [0, 3] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "same").unwrap();
            let before = entry.locals.iter().find(|local| local.id == entry.params[0].binding_id).unwrap().source_id as u32 * 8;
            let after = entry.locals.iter().find(|local| local.id == entry.params[1].binding_id).unwrap().source_id as u32 * 8;
            let load = elf
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.word == (before << 20) | 0x13503
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
                })
                .unwrap();
            let mut substituted = valid.clone();
            substituted.replace_machine_word(load.address, (after << 20) | 0x13503);
            if property == "data_hash" {
                use ckb_testtool::ckb_types::bytes::Bytes;
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.inputs[0].data = vec![0; 8].into();
                transaction.outputs[0].data = vec![1; 8].into();
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 5);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&substituted.artifact, &transaction).exit_code, 0);
            }
            assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

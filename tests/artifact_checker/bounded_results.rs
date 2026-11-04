use super::*;

#[test]
fn collection_helpers_preserve_wide_and_short_return_bytes() {
    for (ty, value) in [
        ("u128", "340282366920938463463374607431768211455"),
        ("[u8; 1]", "b\"a\""),
        ("[u8; 3]", "b\"abc\""),
        ("[u8; 8]", "b\"abcdefgh\""),
    ] {
        let source = format!(
            r#"module collection_return_bytes
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<{ty}> {{ items: 4 }}
    lookup: OrderedMap<u64, {ty}> {{ items: 4 }}
}}
fn read(owner: &Work) -> {ty} {{ return owner.values.get(0) }}
fn lookup(owner: &Work, key: u64, fallback: {ty}) -> {ty} {{ return owner.lookup.get_or(key, fallback) }}
fn owned() -> {ty} {{
    let mut work = Work::new()
    work.values.push({value})
    return work.values.pop()
}}
action inspect() -> u64 {{
    let mut work = Work::new()
    work.values.push({value})
    work.lookup.insert(7, {value})
    if read(&work) != {value} {{ return 61 }}
    if lookup(&work, 7, {value}) != {value} {{ return 62 }}
    if lookup(&work, 9, {value}) != {value} {{ return 63 }}
    if owned() != {value} {{ return 64 }}
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
            let helper = valid.record.entries.iter().find(|entry| entry.name == "read").unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let combine = elf
                .instructions
                .iter()
                .rfind(|instruction| {
                    instruction.word & 0x707f == 0x6033
                        && instruction.word >> 7 & 31 == if ty == "u128" { 11 } else { 10 }
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == helper.id && block.range.contains(instruction.address))
                })
                .unwrap();
            let mut zeroed = valid.clone();
            zeroed.replace_machine_word(combine.address, ((combine.word >> 7 & 31) << 7) | 0x13);
            use ckb_testtool::ckb_types::bytes::Bytes;
            let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0, "{ty}/{diagnostics}");
            assert_ne!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
            assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let begin = valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.owner_entry == helper.id
                        && block
                            .allocated_lifecycle_labels
                            .iter()
                            .any(|label| label.starts_with(".Lallocated_return_") && label.ends_with("_begin"))
                })
                .unwrap()
                .range
                .start;
            let load = elf
                .instructions
                .iter()
                .find(|instruction| (begin..combine.address).contains(&instruction.address) && instruction.word & 0x707f == 0x4003)
                .unwrap();
            let mut wrong_byte = valid.clone();
            wrong_byte.replace_machine_word(load.address, load.word ^ (1 << 20));
            assert_code(&wrong_byte, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let mut erased = valid.clone();
            for block in &mut erased.record.blocks {
                if block.owner_entry == helper.id {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_return_"));
                }
            }
            erased.rebind_sidecars();
            assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn bounded_helpers_must_return_their_typed_scalar_value() {
    let source = r#"module bounded_helper_return
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn read(owner: &Work) -> u64 {
    let value = owner.values.len()
    let capacity = owner.values.capacity()
    if capacity < value { return 0 }
    return value
}
action inspect() -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    work.values.push(9)
    require read(&work) == 2
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let entry = valid.record.entries.iter().find(|entry| entry.name == "read").unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let load = elf
            .instructions
            .iter()
            .rfind(|instruction| {
                instruction.word & 0x7f == 3
                    && instruction.word >> 7 & 31 == 10
                    && instruction.word >> 12 & 7 == 3
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .unwrap();
        let mut zeroed = valid.clone();
        zeroed.replace_machine_word(load.address, 0x00000513); // addi a0, zero, 0
        use ckb_testtool::ckb_types::bytes::Bytes;
        let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
        assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let mut substituted = valid.clone();
        substituted.replace_machine_word(load.address, load.word ^ (8 << 20));
        assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let restore = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.address > load.address
                    && instruction.word & 0x7f == 3
                    && instruction.word >> 7 & 31 == 8
                    && instruction.word >> 12 & 7 == 3
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .unwrap();
        let mut clobbered = valid.clone();
        clobbered.replace_machine_word(restore.address, (restore.word & !(31 << 7)) | (10 << 7));
        assert_ne!(ckb_script_runner::execute_cellscript_script(&clobbered.artifact, &transaction).exit_code, 0);
        assert_code(&clobbered, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        let mut erased = valid.clone();
        for block in &mut erased.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_return_"));
        }
        erased.rebind_sidecars();
        assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn scalar_only_wrappers_preserve_bounded_call_argument_and_result_slots() {
    let source = r#"module scalar_wrapper_slots
resource Token has store { amount: u64 }
fn read(index: u64) -> u64 { return ckb::input<Token>(index).data.amount }
fn twice(index: u64, delta: u64) -> u64 {
    let first = read(index)
    let second = read(index)
    return first + second + delta
}
action inspect(witness index: u64, witness delta: u64) -> u64 {
    verification
    require twice(index, delta) == 9
    return 0
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let payload = compiled.metadata.actions[0]
            .entry_witness_args(&[cellscript::EntryWitnessArg::U64(0), cellscript::EntryWitnessArg::U64(3)])
            .unwrap();
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        let valid = Fixture::from_result(compiled);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let wrapper = valid.record.entries.iter().find(|entry| entry.name.ends_with("twice")).unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("read")).unwrap();
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.id == wrapper.id).unwrap();
        let delta = typed.params.iter().find(|param| param.name == "delta").unwrap().binding_id;
        let delta_slot = typed.locals.iter().find(|local| local.id == delta).unwrap().source_id as u32 * 8;
        let preparations = allocated_calls::spans(&valid);
        let preparations = preparations.iter().filter(|(owner, _, _, _)| owner == &wrapper.id).collect::<Vec<_>>();
        assert_eq!(preparations.len(), 2);
        for (_, _, start, end) in preparations {
            let load = elf
                .instructions
                .iter()
                .find(|instruction| {
                    (*start..*end).contains(&instruction.address)
                        && instruction.word & 0x707f == 0x3003
                        && instruction.word >> 15 & 31 == 2
                })
                .unwrap();
            let mut substituted = valid.clone();
            substituted.replace_machine_word(load.address, (load.word & 0xfffff) | (delta_slot << 20));
            assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert_ne!(ckb_script_runner::execute_cellscript_script(&substituted.artifact, &transaction).exit_code, 0);
        }
        let target = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let calls = elf
            .control_flow
            .iter()
            .filter(|edge| {
                edge.target == target
                    && valid.record.blocks.iter().any(|block| block.owner_entry == wrapper.id && block.range.contains(edge.address))
            })
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2, "both reads survive at {diagnostics}");
        for call in calls {
            let store = elf.instructions.iter().find(|instruction| instruction.address == call.address + 4).unwrap();
            assert_eq!(store.word & 0x01fff07f, (10 << 20) | (2 << 15) | (3 << 12) | 0x23);
            let mut redirected = valid.clone();
            redirected.replace_machine_word(store.address, store.word ^ (8 << 7));
            assert_code(&redirected, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            let mut zeroed = valid.clone();
            zeroed.replace_machine_word(store.address, store.word & !(31 << 20));
            assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert_ne!(ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code, 0);
        }
    }
}

#[test]
fn bounded_scalar_results_bind_call_continuation_and_destination() {
    for (ty, argument, read) in [
        ("InputView<Token>", "ckb::input<Token>(0)", "view.data.amount"),
        ("Messages::Item", "messages[0]", "view.len()"),
        ("&Work", "&work", "view.values.get(0)"),
    ] {
        for padding in [0, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!(
                r#"module bounded_result_binding
resource Token has store {{ amount: u64 }}
data_shape Messages {{ element: Vec<u8>, limits {{ items: 2, payload_bytes: 8, encoded_bytes: 32 }} }}
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}view: {ty}) -> u64 {{ return {read} }}
fn barrier() -> u64 {{ return ckb::input<Token>(0).data.amount }}
action inspect(witness skip: bool, witness messages: Messages) -> u64 {{
    let mut work = Work::new()
    work.values.push(3)
    if skip {{ return 0 }}
    let result = read({args}{argument})
    require barrier() == 3
    require result == 3
    return 0
}}"#
            );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let compiled = compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap_or_else(|error| panic!("{ty}/{padding}/{diagnostics}: {error}"));
                use cellscript::EntryWitnessArg as A;
                use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
                let messages = [15u32.to_le_bytes().as_slice(), &8u32.to_le_bytes(), &3u32.to_le_bytes(), &[1, 2, 3]].concat();
                let payload = compiled.metadata.actions[0].entry_witness_args(&[A::Bool(false), A::Bytes(messages)]).unwrap();
                let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                transaction.current_type_script_input_indices = vec![0];
                transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
                transaction.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let valid = Fixture::from_result(compiled);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("read")).unwrap();
                let target = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
                let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap().address;
                let start = call + 4;
                let store = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        instruction.address >= start && instruction.word & 0x707f == 0x3023 && instruction.word >> 20 & 31 == 10
                    })
                    .unwrap();
                assert!(store.address - start <= 4);
                let mut zeroed = valid.clone();
                zeroed.replace_machine_word(store.address, store.word & !(31 << 20));
                assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                assert_ne!(
                    ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code,
                    0,
                    "zeroed result {ty}/{padding}/{diagnostics}"
                );
                let mut redirected = valid.clone();
                redirected.replace_machine_word(store.address, store.word ^ (8 << 7));
                assert_code(&redirected, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                if padding > 0 {
                    let restore = elf.instructions.iter().find(|instruction| instruction.address == start).unwrap();
                    assert_eq!(restore.word, 0x01010113);
                    let mut wrong_restore = valid.clone();
                    wrong_restore.replace_machine_word(start, 0x02010113);
                    assert_code(&wrong_restore, CheckerRejectionCode::V2407AbiOrStackInvalid);
                }
            }
        }
    }
}

#[test]
fn bounded_call_results_cover_all_six_scalar_types() {
    let types = [("bool", "true"), ("u8", "1"), ("u16", "2"), ("u32", "3"), ("i32", "-(1 as i32)"), ("u64", "4")];
    let helpers = types
        .iter()
        .map(|(ty, _)| {
            let zero = if *ty == "bool" { "false" } else { "0" };
            format!("fn read_{ty}(view: InputView<Token>, value: {ty}) -> {ty} {{ if view.data.amount != 3 {{ return {zero} }}\nreturn value }}\n")
        })
        .collect::<String>();
    let checks = types
        .iter()
        .map(|(ty, value)| {
            format!("let result_{ty} = read_{ty}(view, {value})\nrequire barrier(view) == 3\nrequire result_{ty} == {value}\n")
        })
        .collect::<String>();
    let source = format!(
        "module scalar_result_widths\nresource Token has store {{ amount: u64 }}\n{helpers}\n\
        fn barrier(view: InputView<Token>) -> u64 {{ return view.data.amount }}\n\
        action inspect() -> u64 {{ verification\nlet view = ckb::input<Token>(0)\n{checks}\nreturn 0 }}"
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        use ckb_testtool::ckb_types::bytes::Bytes;
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.current_type_script_input_indices = vec![0];
        transaction.inputs[0].data = 3u64.to_le_bytes().to_vec().into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for (ty, _) in types {
            let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with(&format!("read_{ty}"))).unwrap();
            let target = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
            let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap().address;
            let store = elf.instructions.iter().find(|instruction| instruction.address == call + 4).unwrap();
            assert_eq!(store.word & 0x707f, 0x3023);
            assert_eq!(store.word >> 20 & 31, 10);
            let mut zeroed = valid.clone();
            zeroed.replace_machine_word(store.address, store.word & !(31 << 20));
            assert_code(&zeroed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert_ne!(
                ckb_script_runner::execute_cellscript_script(&zeroed.artifact, &transaction).exit_code,
                0,
                "zeroed result {ty}/{diagnostics}"
            );
        }
    }
}

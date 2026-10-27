use super::*;

#[test]
fn conditional_collection_execution_binds_the_source_boolean_branch() {
    let source = r#"module collection_boolean_branch
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
action inspect(witness take: bool) -> u64 {
    let mut work = Work::new()
    if take { work.values.push(7) }
    require work.values.len() == 1
    return 0
}"#;
    for opt_level in [0, 3] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let payload = compiled.metadata.actions[0].entry_witness_args(&[cellscript::EntryWitnessArg::Bool(true)]).unwrap();
        let valid = Fixture::from_result(compiled);
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let condition = entry
            .blocks
            .iter()
            .find(|block| {
                block.terminator == "branch"
                    && block.operations.last().is_some_and(|op| {
                        op.opcode == "branch-condition"
                            && op.operands.first().and_then(|operand| operand.local) == Some(entry.params[0].binding_id)
                    })
            })
            .unwrap();
        let start = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == entry.id && block.lowering_block_id == Some(condition.id))
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let end = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                block.owner_entry == entry.id
                    && block.range.start > start
                    && block.lowering_block_id.is_some_and(|id| id != condition.id)
            })
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let branch = elf
            .instructions
            .iter()
            .rfind(|instruction| (start..end).contains(&instruction.address) && instruction.word & 0x7f == 0x63)
            .unwrap();
        assert!(matches!((branch.word >> 12) & 7, 0 | 1));
        let mut inverted = valid.clone();
        inverted.replace_machine_word(branch.address, branch.word ^ (1 << 12));
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&inverted.artifact, &transaction).exit_code, 0);
        assert_code(&inverted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let load = elf.instructions.iter().find(|instruction| instruction.address + 4 == branch.address).unwrap();
        assert_eq!(load.word & 0x000fffff, 0x00013283); // ld t0, offset(sp)
        let mut substituted = valid.clone();
        substituted.replace_machine_word(load.address, 0x00000293); // addi t0, zero, 0
        assert_ne!(ckb_script_runner::execute_cellscript_script(&substituted.artifact, &transaction).exit_code, 0);
        assert_code(&substituted, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let mut redirected = valid.clone();
        redirected.redirect_conditional_branch(branch.address, branch.address + 4);
        assert_code(&redirected, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let mut erased = valid.clone();
        for block in &mut erased.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| !label.starts_with(".Lallocated_branch_"));
        }
        erased.rebind_sidecars();
        assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

fn redirect_jump(fixture: &mut Fixture, address: u64, target: u64) {
    let from = fixture.record.blocks.iter().find(|block| block.range.contains(address)).unwrap().id.clone();
    let to = fixture.record.blocks.iter().find(|block| block.range.start == target).unwrap().id.clone();
    fixture.record.edges.iter_mut().find(|edge| edge.from == from && edge.kind == EdgeKind::Jump).unwrap().to = to;
    let mut reached = std::collections::BTreeSet::new();
    let mut pending = fixture.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
    while let Some(id) = pending.pop() {
        if reached.insert(id.clone()) {
            pending.extend(fixture.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
        }
    }
    for block in &mut fixture.record.blocks {
        block.reachable = reached.contains(&block.id);
    }
    fixture.record.canonicalize();
    fixture.replace_machine_word(address, encode_jal((target as i64 - address as i64) as i32));
}

#[test]
fn source_jumps_cannot_be_disguised_as_empty_fallthroughs() {
    let source = r#"module collection_loop_branch
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
action inspect() -> u64 {
    let mut work = Work::new()
    for round in 0..2 { work.values.push(round) }
    require work.values.len() == 2
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
        let mut checked = 0;
        for block in &valid.record.blocks {
            for begin in block
                .allocated_lifecycle_labels
                .iter()
                .filter(|label| label.starts_with(".Lallocated_branch_") && label.ends_with("_begin"))
            {
                let end = format!("{}_end", begin.strip_suffix("_begin").unwrap());
                let finish =
                    valid.record.blocks.iter().find(|block| block.allocated_lifecycle_labels.contains(&end)).unwrap().range.start;
                if finish != block.range.start + 4 {
                    continue;
                }
                let instruction = elf.instructions.iter().find(|instruction| instruction.address == block.range.start).unwrap();
                if instruction.word & 0xfff != 0x6f {
                    continue;
                }
                let target = elf.control_flow.iter().find(|flow| flow.address == instruction.address).unwrap().target;
                if target >= instruction.address {
                    continue;
                }
                let mut disguised = valid.clone();
                for candidate in &mut disguised.record.blocks {
                    candidate.allocated_lifecycle_labels.retain(|label| label != begin && label != &end);
                    if candidate.range.start == target {
                        candidate.allocated_lifecycle_labels.extend([begin.clone(), end.clone()]);
                        candidate.allocated_lifecycle_labels.sort();
                    }
                }
                redirect_jump(&mut disguised, instruction.address, finish);
                use ckb_testtool::ckb_types::bytes::Bytes;
                let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
                assert_ne!(ckb_script_runner::execute_cellscript_script(&disguised.artifact, &transaction).exit_code, 0);
                assert_code(&disguised, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                checked += 1;
            }
        }
        assert!(checked > 0, "O{opt_level}: no source loop backedge");
    }
}

#[test]
fn collection_operations_cannot_be_bypassed_as_complete_ranges() {
    let source = r#"module collection_operation_path
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}

action inspect(witness value: u64) -> u64 {
    let mut work = Work::new()
    work.values.push(value)
    require work.values.len() == 1
    return 0
}"#;
    for opt_level in [0, 3] {
        let compiled = compile(
            source,
            CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap();
        let payload = compiled.metadata.actions[0].entry_witness_args(&[cellscript::EntryWitnessArg::U64(7)]).unwrap();
        let valid = Fixture::from_result(compiled);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let jump = valid
            .record
            .blocks
            .iter()
            .find(|block| block.allocated_init_labels.iter().any(|label| label.contains("_end_")))
            .unwrap()
            .range
            .start;
        assert_eq!(elf.instructions.iter().find(|instruction| instruction.address == jump).unwrap().word & 0xfff, 0x6f);
        let end = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(".Lallocated_push_") && label.ends_with("_end"))
            })
            .unwrap()
            .range
            .start;
        let mut bypass = valid.clone();
        redirect_jump(&mut bypass, jump, end);
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        transaction.witnesses =
            vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&bypass.artifact, &transaction).exit_code, 0);
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn owner_drop_cannot_skip_one_member_retirement() {
    for members in [2, 32] {
        let declarations = (0..members).map(|i| format!("values{i}: Vec<u64> {{ items: 1 }}\n")).collect::<String>();
        let source = format!(
            "module owner_drop_path\ncollections Work {{ limits {{ bytes: 512, allocations: 4 }}\n{declarations}}}\n\
            action inspect() -> u64 {{ let work = Work::new()\nwork.drop()\nreturn 0 }}"
        );
        for opt_level in [0, 3] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let jump = valid
                .record
                .blocks
                .iter()
                .find(|block| block.allocated_init_labels.iter().any(|label| label.contains("_end_")))
                .unwrap()
                .range
                .start;
            let mut ends = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block
                        .allocated_lifecycle_labels
                        .iter()
                        .any(|label| label.starts_with(".Lallocated_drop_") && label.ends_with("_end"))
                })
                .map(|block| block.range.start)
                .collect::<Vec<_>>();
            ends.sort_unstable();
            assert_eq!(ends.len(), members);
            for end in [ends[0], *ends.last().unwrap()] {
                let mut bypass = valid.clone();
                redirect_jump(&mut bypass, jump, end);
                assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn borrowed_queries_cannot_skip_a_complete_producer_and_result() {
    let source = r#"module borrowed_query_path
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn inspect(owner: &Work) -> u64 {
    let length = owner.values.len()
    let capacity = owner.values.capacity()
    return length + capacity
}
action inspect_queries() -> u64 {
    let mut work = Work::new()
    work.values.push(7)
    require inspect(&work) == 2
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
        let end = |prefix: &str| {
            valid
                .record
                .blocks
                .iter()
                .find(|block| {
                    block.allocated_lifecycle_labels.iter().any(|label| label.starts_with(prefix) && label.ends_with("_end"))
                })
                .unwrap()
                .range
                .start
        };
        let jump = end(".Lallocated_borrowed_len_") + 4;
        let after_capacity = end(".Lallocated_borrowed_capacity_") + 4;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for pc in [jump, after_capacity] {
            assert_eq!(elf.instructions.iter().find(|instruction| instruction.address == pc).unwrap().word & 0xfff, 0x6f);
        }
        let target = elf.control_flow.iter().find(|edge| edge.address == after_capacity).unwrap().target;
        let mut bypass = valid.clone();
        redirect_jump(&mut bypass, jump, target);
        use ckb_testtool::ckb_types::bytes::Bytes;
        let transaction = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &transaction).exit_code, 0);
        assert_ne!(ckb_script_runner::execute_cellscript_script(&bypass.artifact, &transaction).exit_code, 0);
        assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

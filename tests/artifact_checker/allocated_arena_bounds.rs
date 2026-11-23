#[test]
fn allocated_writes_preserve_retained_cell_views_after_guard_mutations() {
    let source = r#"
module guarded_collections;
resource Note has store { amount: u64, }
collections Work { limits { bytes: 128, allocations: 2 },
    values: Vec<u64> { items: 4 }
}
action inspect() -> u64 {

    let note = ckb::group_input::<Note>(0).data;
    let mut work = Work::new();
    work.values.push(note.amount);
    require work.values.get(0) == note.amount;
    return 0;
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let words = elf
            .instructions
            .iter()
            .map(|instruction| (instruction.address, instruction.word))
            .collect::<std::collections::BTreeMap<_, _>>();
        let guards = elf
            .instructions
            .iter()
            .filter_map(|instruction| {
                let pc = instruction.address;
                let word = instruction.word;
                let upper = *words.get(&(pc + 4))?;
                let store = *words.get(&(pc + 8))?;
                (word & 0x707f == 0x6063
                    && upper & 0x707f == 0x6063
                    && store & 0x7f == 0x23
                    && word >> 15 & 31 == upper >> 20 & 31
                    && word >> 15 & 31 == store >> 15 & 31)
                    .then_some((pc, word, upper, store))
            })
            .collect::<Vec<_>>();
        assert!(guards.len() >= 4);
        for mutation in 0..8 {
            let replacements = guards
                .iter()
                .map(|&(pc, lower, upper, store)| match mutation {
                    0 => (pc, lower & !0x7000),                        // equality is not an unsigned lower bound
                    1 => (pc + 4, upper & !0x7000),                    // equality is not an unsigned upper bound
                    2 => (pc + 8, (store & !0xfe000f80) | 0xfe000f80), // unchecked -1 address displacement
                    3 => (pc + 4, upper & !(31 << 15)),                // x0 is not a private-stack upper bound
                    4 => (pc + 8, (store & !(0xfe000f80 | (31 << 15))) | 0xfe000c00 | (2 << 15)), // spill below the allowed subframe
                    5 => (pc, encode_jal(8)),                          // bypass both guards and enter the store
                    6 => (pc - 4, 0xfff10013 | ((upper >> 15 & 31) << 7)), // upper bound below private frame
                    7 => (pc + 24, 0x01010113),                        // restore only half the temporary subframe
                    _ => unreachable!(),
                })
                .collect::<Vec<_>>();
            let mut changed = valid.clone();
            changed.replace_machine_words(&replacements);
            assert_eq!(
                changed.check(),
                Err(match mutation {
                    5 => CheckerRejectionCode::V2414ControlFlowInvalid,
                    7 => CheckerRejectionCode::V2407AbiOrStackInvalid,
                    _ => CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                }),
                "{diagnostics}/guard mutation {mutation} did not fail independent machine binding"
            );
        }
    }
}

#[test]
fn mixed_settlement_shared_return_tails_reject_skipped_restores_and_field_checks() {
    let source = include_str!("../../examples/language/batches/mixed_settlement.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("transfer_leg")).unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        // Source-only evidence can share the epilogue address and become its
        // primary machine label. Locate the actual return rather than a label
        // spelling, while retaining the complete restoration mutation range.
        let tail = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == helper.id
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| block.range.contains(instruction.address) && instruction.word == 0x00008067)
            })
            .unwrap();
        assert!(
            elf.stack_adjustments
                .iter()
                .any(|adjustment| tail.range.contains(adjustment.address) && adjustment.delta == i64::from(helper.frame_size_bytes)),
            "return mutation range must include frame restoration"
        );
        for instruction in elf.instructions.iter().filter(|instruction| tail.range.contains(instruction.address)) {
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, instruction.word ^ (1 << 7));
            assert!(changed.check().is_err(), "altered return restoration at {:#x}", instruction.address);
        }
        let edge = elf
            .control_flow
            .iter()
            .find(|edge| {
                edge.target == tail.range.start
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| instruction.address == edge.address && instruction.word & 0xfff == 0x6f)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(edge.address, encode_jal((tail.range.start as i64 + 4 - edge.address as i64) as i32));
        assert!(changed.check().is_err(), "entry into the middle of a return tail");
        let read = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == helper.id
                    && block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_data_") && !label.ends_with("_done"))
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(read.range.start, encode_jal((tail.range.start as i64 - read.range.start as i64) as i32));
        assert!(changed.check().is_err(), "return-tail recognition must not bypass a field check");
    }
}

#[test]
fn nominal_wide_result_contract_rejects_retyping_wrong_width_and_hidden_pointer() {
    for (ty, other) in [("ScriptHash", "SighashAllDigest"), ("SighashAllDigest", "ScriptHash")] {
        let source = format!("module nominal_result;\nfn copy(value: {ty}) -> {ty} {{ let snapshot = value;\nreturn snapshot; }}\naction inspect(witness value: {ty}) -> u64 {{  require copy(value) == value; return 0; }}");
        for diagnostics in [
            cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
            cellscript::CompilerDiagnostics::without_optimizations(),
        ] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            valid.check().unwrap();
            for mutation in 0..6 {
                let mut changed = valid.clone();
                let abi =
                    changed.record.entries.iter_mut().find(|entry| entry.name == "copy").unwrap().fixed_result_abi.as_mut().unwrap();
                match mutation {
                    0 => abi.width_bytes = 31,
                    1 => abi.hidden_argument_index += 1,
                    2 => abi.copy_ranges.clear(),
                    3 => abi.type_name = "hash".into(),
                    4 => abi.type_name = other.into(),
                    _ => abi.copy_sources[0].field_offset = 1,
                }
                changed.rebind_sidecars();
                assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
            }
            let abi = valid.record.entries.iter().find(|entry| entry.name == "copy").unwrap().fixed_result_abi.as_ref().unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let load = elf
                .instructions
                .iter()
                .find(|word| {
                    abi.copy_sources.iter().any(|source| source.range.contains(word.address))
                        && word.word & 0x7f == 3
                        && word.word >> 7 & 31 == 10
                })
                .unwrap();
            let mut changed = valid.clone();
            changed.replace_machine_word(load.address, load.word & !(31 << 15));
            assert!(changed.check().is_err(), "{ty} result cannot read an unproved absolute pointer");
        }
    }
}

#[test]
fn computed_sighash_result_cannot_be_relabelled_as_an_ordinary_hash() {
    let valid = Fixture::from_source(SIGHASH_ZERO_LOCK_SOURCE);
    valid.check().unwrap();
    let mut changed = valid.clone();
    for entry in &mut changed.record.typed_semantics.entries {
        for local in &mut entry.locals {
            if local.ty == "SighashAllDigest" {
                local.ty = "hash".into();
            }
        }
        for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
            for operand in &mut operation.operands {
                if operand.ty == "SighashAllDigest" {
                    operand.ty = "hash".into();
                }
            }
            if let Some(call) = &mut operation.call {
                for param in &mut call.params {
                    if param == "SighashAllDigest" {
                        *param = "hash".into();
                    }
                }
                if call.return_type == "SighashAllDigest" {
                    call.return_type = "hash".into();
                }
            }
        }
    }
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

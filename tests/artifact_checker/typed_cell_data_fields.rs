#[test]
fn cell_data_helper_fields_preserve_independent_machine_checks() {
    let source = include_str!("../../examples/language/ckb/typed_cell_predicate.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let helper = valid.record.entries.iter().find(|entry| entry.name.ends_with("matches")).unwrap();
        let ranges = valid.record.blocks.iter().filter(|block| block.owner_entry == helper.id).collect::<Vec<_>>();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let syscalls = elf
            .instructions
            .iter()
            .filter(|instruction| ranges.iter().any(|block| block.range.contains(instruction.address)) && instruction.word == 0x73)
            .collect::<Vec<_>>();
        assert!(syscalls.len() >= 4);
        for syscall in syscalls {
            let mut changed = valid.clone();
            changed.replace_machine_word(syscall.address, encode_addi(0, 0, 0));
            assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);
        }
        let read = elf
            .instructions
            .iter()
            .filter(|instruction| {
                ranges.iter().any(|block| block.range.contains(instruction.address)) && instruction.word & 0xfffff == 0x13283
            })
            .max_by_key(|instruction| instruction.word >> 20)
            .expect("helper contains an aligned fixed Cell field load");
        let mut changed = valid.clone();
        changed.replace_machine_word(read.address, read.word ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn cell_data_views_keep_nominal_fields_bound_after_admission() {
    let source = selected_roles_source().replace(
        "require input.capacity == partner.capacity;",
        "require input.data.id == partner.data.id;\nrequire input.data.amount == partner.data.amount;",
    );
    let valid = Fixture::from_source(&source);
    assert_bounded_admitted(&valid);
    for case in 1..3 {
        let mut changed = valid.clone();
        {
            let operation = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .flat_map(|entry| &mut entry.blocks)
                .flat_map(|block| &mut block.operations)
                .find(|operation| {
                    operation.opcode == "field-access"
                        && operation.operands.first().is_some_and(|operand| operand.ty.starts_with("CellDataView<"))
                })
                .unwrap();
            if case == 1 {
                operation.detail = TypedSemanticOperationDetail::Field { name: "missing_field".into() };
            } else {
                operation.operands[0].ty = "CellDataView<Receipt>".into();
            }
            changed.rebind_layered_identities();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

#[test]
fn exact_role_lifetime_rejects_entry_into_a_read_before_construction() {
    let base = include_str!("../../examples/language/ckb/exact_role_verification.cell")
        .replace("exact_cell_role::<Receipt>", "exact_cell_role::<Order>");
    let (prefix, tail) = base.split_once("action verify()").unwrap();
    let call = tail.split_once("ckb::verify_cell_roles(").unwrap().1.split_once("    return 0").unwrap().0;
    let source = format!("{prefix}action verify(witness at: u64) -> u64 {{\nif at > 0 {{\nlet selected = ckb::select_cell_roles({call}\nif at > 1 {{ return ckb::selected_input_count::<Order>(selected, 0); }}\n}}\nreturn 0;\n}}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let producer = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_check_") && !label.ends_with("_done"))
            })
            .unwrap();
        let consumer = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_use_") && !label.ends_with("_done"))
            })
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let flow = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.address < producer.range.start
                    && flow.target > producer.range.start
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == producer.owner_entry && block.range.contains(flow.address))
                    && elf
                        .instructions
                        .iter()
                        .any(|instruction| instruction.address == flow.address && instruction.word & 0xfff == 0x6f)
            })
            .expect("the outer false path jumps over the large selection body");
        let origin = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
        let previous_target = valid.record.blocks.iter().find(|block| block.range.start == flow.target).unwrap();
        let mut changed = valid.clone();
        changed.record.edges.iter_mut().find(|edge| edge.from == origin.id && edge.to == previous_target.id).unwrap().to =
            consumer.id.clone();
        let mut reached = std::collections::BTreeSet::new();
        let mut pending = changed.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reached.insert(id.clone()) {
                pending.extend(changed.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut changed.record.blocks {
            block.reachable = reached.contains(&block.id);
        }
        changed.record.canonicalize();
        changed.replace_machine_word(flow.address, encode_jal((consumer.range.start as i64 - flow.address as i64) as i32));
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("selection read has no constructed handle provenance"), "{diagnostics}: {error}");
    }
}

#[test]
fn exact_role_lifetime_preserves_handles_and_tables_across_calls_and_branches() {
    let source = selected_roles_source()
        .replace("action verify()", "fn bump(value: u64) -> u64 { return value + 7; }\naction verify(witness at: u64)")
        .replace(
            "let alias = selected;",
            "let marker = at + 7;\nrequire marker > at;\nif at > 0 { require bump(at) > at; }\nlet alias = selected;",
        );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap_or_else(|error| panic!("selection lifetime {diagnostics}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut ranges = Vec::new();
        let mut use_ranges = Vec::new();
        let mut constructor_stores = Vec::new();
        let mut tables = Vec::new();
        let mut last_constructor_end = 0;
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                if label.ends_with("_done") {
                    continue;
                }
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.exact_role_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                ranges.push(block.range.start..end);
                if let Some(parts) = label.strip_prefix(".Lexact_role_check_") {
                    tables.push(parts.split('_').nth(2).unwrap().parse::<i32>().unwrap());
                    last_constructor_end = last_constructor_end.max(end);
                    constructor_stores.push(end - 4);
                } else {
                    use_ranges.push(block.range.start..end);
                }
            }
        }
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "verify").unwrap();
        let handles = typed
            .locals
            .iter()
            .filter(|local| local.ty.starts_with("ExactCellSelection<"))
            .map(|local| local.source_id as i32 * 8)
            .collect::<Vec<_>>();
        assert!(handles.len() >= 2 && tables.len() == 2, "{diagnostics}: handles {handles:?}, tables {tables:?}");
        // Rebind a second typed handle and all its actual pointer loads/stores
        // to the first one's slot. Both individual instruction grammars still
        // match; the enclosing proof must reject the physical alias.
        let (original, replacement) = (handles[1], handles[0]);
        let mut mutations = Vec::new();
        for instruction in &elf.instructions {
            if !constructor_stores.contains(&instruction.address)
                && !use_ranges.iter().any(|range| range.contains(&instruction.address))
            {
                continue;
            }
            let word = instruction.word;
            if word >> 15 & 31 != 2 {
                continue;
            }
            if word & 0x707f == 0x3003 && (word as i32) >> 20 == original {
                mutations.push((instruction.address, replace_i_immediate(word, replacement)));
            } else if word & 0x707f == 0x3023 && ((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20 >> 20) == original {
                mutations.push((
                    instruction.address,
                    (word & !0xfe000f80) | ((replacement as u32 & 0xfe0) << 20) | ((replacement as u32 & 31) << 7),
                ));
            }
        }
        assert!(mutations.len() >= 2);
        let mut aliased = valid.clone();
        let entry = aliased.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == typed.id).unwrap();
        entry.locals.iter_mut().find(|local| local.source_id * 8 == original as u64).unwrap().source_id = replacement as u64 / 8;
        aliased.rebind_layered_identities();
        aliased.replace_machine_words(&mutations);
        let error = check_bundle_values(
            &aliased.artifact,
            &aliased.metadata,
            &aliased.record,
            &aliased.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("selection pointer slot aliases"), "{diagnostics}: {error}");
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.address >= last_constructor_end
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    && !ranges.iter().any(|range| range.contains(&instruction.address))
                    && instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let targets = handles.iter().copied().chain([tables[0], tables[0] + 768, tables[0] + 896]);
        for offset in targets {
            for kind in [0, 3] {
                assert!((0..2048).contains(&offset));
                let word = (store.word & !0xfe0fff80)
                    | ((offset as u32 & 0xfe0) << 20)
                    | ((offset as u32 & 31) << 7)
                    | (2 << 15)
                    | (kind << 12);
                let mut changed = valid.clone();
                changed.replace_machine_word(store.address, word);
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                assert!(error.message.contains("unverified write overlaps"), "{diagnostics} offset={offset}: {error}");
            }
        }
        // A computed alias must retain the same protection as an SP-relative write.
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (store.address - 4, ((handles[0] as u32) << 20) | (2 << 15) | (3 << 12) | (30 << 7) | 3),
            (store.address, (store.word & !0xfe0f8f80) | (30 << 15)),
        ]);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn exact_role_selected_reads_reject_rebound_words_and_typed_operand_substitutions() {
    use cellscript_artifact_checker::cell_selection::Read;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &selected_roles_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("{diagnostics}: {error}")),
        );
        let mut ranges = Vec::new();
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                if label.starts_with(".Lexact_role_use_") && !label.ends_with("_done") {
                    let end =
                        valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done"))).unwrap();
                    ranges.push(block.range.start..end.range.start);
                }
            }
        }
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for block in &valid.record.blocks {
            if block.exact_role_labels.iter().any(|label| label.starts_with(".Lexact_role_check_") && label.ends_with("_done")) {
                // The retained pointer's address and destination store are
                // inside the constructor proof, after the restored SP.
                for address in [block.range.start - 8, block.range.start - 4] {
                    let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
                    let mask = if word & 0x7f == 0x23 { 1 << 7 } else { 1 << 15 };
                    let mut changed = valid.clone();
                    changed.replace_machine_word(address, word ^ mask);
                    assert!(changed.check().is_err(), "retained pointer mutation accepted at {address:#x}");
                }
            }
        }
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !ranges.iter().any(|range| range.contains(&instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x63 => 1 << 12,
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted changed read at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 60, "all five reads and local aliases must be exercised");
        for case in 0..4 {
            let mut changed = valid.clone();
            let entry = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .find(|entry| {
                    entry.blocks.iter().flat_map(|block| &block.operations).any(|operation| operation.opcode == "read-exact-cell-role")
                })
                .unwrap();
            let other = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|operation| matches!(operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Output, .. }))
                .unwrap()
                .operands[0]
                .local
                .unwrap();
            let operation = entry
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.operations)
                .find(|operation| {
                    matches!(operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Successor, .. })
                })
                .unwrap();
            match case {
                0 => operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 1, read: Read::Successor },
                1 => operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 0, read: Read::Output },
                2 => operation.operands[0].local = Some(other),
                3 => operation.operands[1].constant = Some(TypedSemanticConstant::U64("1".into())),
                _ => unreachable!(),
            }
            changed.rebind_layered_identities();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn selected_hash_reads_prove_caller_writes_while_nested_views_remain_live() {
    let source = selected_roles_source()
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)")
        .replace("let alias = selected;", "let first = items[0];\nlet alias = selected;")
        .replace(
            "require input.capacity == partner.capacity;",
            "require input.type_hash == partner.type_hash;\nrequire input.lock_hash == partner.lock_hash;\nrequire input.data_hash == partner.data_hash;\nrequire first[0] == 170;",
        );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap_or_else(|error| panic!("hash with nested {diagnostics}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let blocks =
            valid.record.blocks.iter().filter(|block| block.owner_entry == "runtime:__ckb_cell_type_hash").collect::<Vec<_>>();
        assert!(!blocks.is_empty());
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x63 => 1 << 12,
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "hash helper mutation accepted at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 45);
        let start = blocks.iter().map(|block| block.range.start).min().unwrap();
        let call = elf.control_flow.iter().find(|edge| edge.target == start).unwrap();
        let owner = &valid.record.blocks.iter().find(|block| block.range.contains(call.address)).unwrap().owner_entry;
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| &entry.id == owner).unwrap();
        let nested_slot = entry.locals.iter().find(|local| local.ty.starts_with("NestedValues<")).unwrap().source_id * 8;
        assert!(nested_slot < 2048);
        for register in [11, 12] {
            let setup = elf
                .instructions
                .iter()
                .rev()
                .find(|instruction| {
                    instruction.address < call.address
                        && instruction.address + 64 >= call.address
                        && instruction.word >> 7 & 31 == register
                        && matches!(instruction.word & 0x7f, 0x13 | 0x33)
                })
                .unwrap();
            let mut changed = valid.clone();
            // Rebind the real caller output or length pointer into a live
            // nested root slot; the helper itself is unchanged and well formed.
            changed.replace_machine_word(setup.address, ((nested_slot as u32) << 20) | (2 << 15) | (register << 7) | 0x13);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("overlaps protected runtime storage"), "{error}");
        }
    }
}

#[test]
fn nested_transaction_data_views_bind_complete_tables_and_retained_results() {
    let source = include_str!("../syntax_combo/seeds/named-nested-data-views.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let result = compile(
            source,
            CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
        )
        .unwrap_or_else(|error| panic!("nested data {diagnostics}: {error}"));
        let valid = Fixture::from_result(result);
        assert_bounded_admitted(&valid);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let (start, end) = valid
            .record
            .blocks
            .iter()
            .find_map(|block| {
                let label = block.exact_role_labels.iter().find(|label| {
                    label.starts_with(".Lexact_role_data_")
                        && !label.ends_with("_done")
                        && label.strip_prefix(".Lexact_role_data_").unwrap().split('_').count() == 6
                        && label.strip_prefix(".Lexact_role_data_").unwrap().split('_').nth(4) != Some("0")
                })?;
                let end =
                    valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done")))?.range.start;
                Some((block.range.start, end))
            })
            .unwrap();
        let body = elf.instructions.iter().filter(|i| (start..end).contains(&i.address)).collect::<Vec<_>>();
        let pointer = body.iter().find(|i| i.word == ((30 << 20) | (29 << 15) | (5 << 7) | 0x33)).unwrap();
        let length = body.iter().find(|i| i.word & 0x7f == 0x23 && i.word >> 12 & 7 == 3 && i.word >> 20 & 31 == 6).unwrap();
        let source = body.iter().find(|i| i.word == ((7 << 15) | (14 << 7) | 0x13)).unwrap();
        for (address, word) in [
            (pointer.address, pointer.word ^ (1 << 15)),
            (length.address, length.word ^ (1 << 10)),
            (source.address, source.word ^ (1 << 15)),
        ] {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}
#[test]
fn fixed_record_cell_data_checker_binds_stride_and_copied_bytes() {
    let source = include_str!("../syntax_combo/seeds/fixed-record-cell-data.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        assert_bounded_admitted(&valid);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let index = elf.instructions.iter().find(|instruction| instruction.word == 0x02730333).expect("record stride");
        let field = elf
            .instructions
            .iter()
            .find(|instruction| instruction.address > index.address && instruction.word & 0x000f_ffff == 0x000e_b283)
            .expect("copied record word");
        for (address, word) in [(index.address, index.word ^ (1 << 20)), (field.address, field.word ^ (1 << 20))] {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

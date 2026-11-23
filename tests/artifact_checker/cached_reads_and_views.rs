#[test]
fn exact_cached_reads_preserve_borrowed_caches_across_helpers_and_cfg_joins() {
    for hot in [false, true] {
        let source = cached_nested_source(hot)
            .replace("action verify", "fn read_one(index: u64) -> u64 { let view = ckb::input::<Token>(index);\nreturn ckb::cell_data_u64_le(view, 0); }\nfn read_twice(index: u64) -> u64 { return read_one(index) + read_one(index); }\naction verify")
            .replace("let view = ckb::input::<Token>(0);", "let view = ckb::input::<Token>(0);\nif first.len() > 0 { require read_twice(0) == 0; }\nrequire read_one(0) == 0;");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("cache helper hot={hot} {diagnostics}: {error}")),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let helper = valid
                .record
                .entries
                .iter()
                .find(|entry| entry.kind == cellscript_artifact_checker::EntryKind::Helper && entry.name.ends_with("read_one"))
                .unwrap();
            let ranges = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.owner_entry == helper.id)
                .map(|block| &block.range)
                .collect::<Vec<_>>();
            let setup = elf
                .instructions
                .iter()
                .find(|instruction| {
                    ranges.iter().any(|range| range.contains(instruction.address))
                        && instruction.word == ((27 << 15) | (12 << 7) | 0x13)
                })
                .unwrap();
            let mut changed = valid.clone();
            // A helper cannot substitute another incoming register for the
            // proved ambient cache, even if all outer hashes are recomputed.
            changed.replace_machine_word(setup.address, (26 << 15) | (12 << 7) | 0x13);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn exact_cached_reads_bind_initialization_regions_and_complete_helper_bodies() {
    for (diagnostics, hot) in
        [(cellscript::CompilerDiagnostics::without_optimizations(), false), (cellscript::CompilerDiagnostics::default(), true)]
    {
        let valid = Fixture::from_result(
            compile(
                &cached_nested_source(hot),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("cache hot={hot}: {error}")),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let cache_blocks = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                block.owner_entry == "runtime:__cellscript_exact_read_cached"
                    || ["runtime:__ckb_cell_data_u8", "runtime:__ckb_cell_data_u32_le", "runtime:__ckb_cell_data_u64_le"]
                        .contains(&block.owner_entry.as_str())
            })
            .collect::<Vec<_>>();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !cache_blocks.iter().any(|block| block.range.contains(instruction.address)) {
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
            assert!(changed.check().is_err(), "cache hot={hot} mutation accepted at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 180);
        let entry = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .find(|entry| entry.params.iter().any(|param| param.ty.contains("NestedValues<")))
            .unwrap();
        let owned =
            valid.record.blocks.iter().filter(|block| block.owner_entry == entry.id).map(|block| &block.range).collect::<Vec<_>>();
        let initialized = elf
            .instructions
            .iter()
            .find(|instruction| {
                owned.iter().any(|range| range.contains(instruction.address))
                    && instruction.word >> 7 & 31 == 27
                    && matches!(instruction.word & 0x7f, 0x13 | 0x33)
            })
            .unwrap()
            .address;
        let mut zero_stores = 0;
        for instruction in &elf.instructions {
            if instruction.address >= initialized || !owned.iter().any(|range| range.contains(instruction.address)) {
                continue;
            }
            if instruction.word & 0x707f == 0x3023 && instruction.word >> 20 & 31 == 0 {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, instruction.word | (2 << 20));
                assert!(changed.check().is_err(), "cache initialization store was not proved");
                zero_stores += 1;
            }
        }
        assert_eq!(zero_stores, 6, "counter, last pointer and four valid flags");
        let setup = elf
            .instructions
            .iter()
            .find(|instruction| {
                owned.iter().any(|range| range.contains(instruction.address)) && instruction.word == ((27 << 15) | (12 << 7) | 0x13)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(setup.address, setup.word | (8 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        if !hot {
            let mut missing_owner = valid.clone();
            let writes = elf
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.address < setup.address
                        && owned.iter().any(|range| range.contains(instruction.address))
                        && instruction.word >> 7 & 31 == 27
                        && matches!(instruction.word & 0x7f, 0x13 | 0x33 | 0x37)
                })
                .map(|instruction| (instruction.address, encode_addi(0, 0, 0)))
                .collect::<Vec<_>>();
            assert!(!writes.is_empty());
            missing_owner.replace_machine_words(&writes);
            let error = check_bundle_values(
                &missing_owner.artifact,
                &missing_owner.metadata,
                &missing_owner.record,
                &missing_owner.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(error.message.contains("public entry borrows an uninitialized"), "{error}");
        }
        let hash_entry = valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_cell_type_hash").unwrap();
        let target = valid.record.blocks.iter().find(|block| block.id == hash_entry.entry_block).unwrap().range.start;
        let call = elf.control_flow.iter().find(|edge| edge.target == target).unwrap();
        let output = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < call.address
                    && instruction.address + 64 >= call.address
                    && instruction.word >> 7 & 31 == 11
                    && matches!(instruction.word & 0x7f, 0x13 | 0x33)
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(output.address, (27 << 15) | (11 << 7) | 0x13);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

fn selected_roles_source() -> String {
    let source = include_str!("../../examples/language/ckb/exact_role_verification.cell")
        .replace("exact_cell_role::<Receipt>", "exact_cell_role::<Order>");
    let call = source.split_once("    ckb::verify_cell_roles(").unwrap().1.split_once("    return 0").unwrap().0;
    source.replacen("ckb::verify_cell_roles(", "let selected = ckb::select_cell_roles(", 1).replace(
        "return 0;",
        &format!(
            r#"
    let other = ckb::select_cell_roles({call}
    let alias = selected;
    let count = ckb::selected_input_count::<Order>(alias, 0);
    let input = ckb::selected_input::<Order>(alias, 0, count - 1);
    let output = ckb::selected_output::<Order>(other, 0, 0);
    let partner = ckb::selected_successor::<Order>(selected, 0, 0);
    require input.capacity == partner.capacity;
    require output.capacity == partner.capacity;
    return ckb::selected_output_count::<Order>(selected, 0);
    "#
        ),
    )
}

#[test]
fn cell_data_field_instructions_reject_rebound_source_length_copy_and_branch_changes() {
    let source = "module typed_data_proof;\nresource Token has store { prefix: [u8; 256], padding: [u8; 216], id: u64, owner: Hash, }\naction inspect() -> u64 { \nlet input = ckb::input::<Token>(0);\nrequire input.data.owner == Hash::zero();\nreturn input.data.id; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let ranges = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block
                    .exact_role_labels
                    .iter()
                    .filter(|label| label.starts_with(".Lexact_role_data_") && !label.ends_with("_done"))
                    .map(|label| {
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.exact_role_labels.contains(&format!("{label}_done")))
                            .unwrap()
                            .range
                            .start;
                        block.range.start..end
                    })
            })
            .collect::<Vec<_>>();
        assert!(ranges.len() >= 3);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !ranges.iter().any(|range| range.contains(&instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let mask = match word & 0x7f {
                0x03 | 0x13 | 0x1b => 1 << 20,
                0x23 => 1 << 7,
                0x33 => 1 << 20,
                0x37 => 1 << 12,
                0x63 => 1 << 12,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted changed data field at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 150, "source tags, exact size, wide copy and scalar assembly must all be exercised");
    }
}

#[test]
fn cell_view_helper_parameters_admit_checked_transport_without_data_projection() {
    let source = include_str!("../../examples/language/ckb/typed_cell_predicate.cell")
        .replace("(before.data.id == after.data.id) && (before.data.owner == after.data.owner)", "true");
    let admitted = Fixture::from_source(&source);
    assert_bounded_admitted(&admitted);
}

#[test]
fn cell_view_helpers_compose_with_branches_repeated_calls_and_wide_arguments() {
    let base = include_str!("../../examples/language/ckb/typed_cell_predicate.cell");
    let branched = base.replace(
        "return matches(before, after);",
        "if before.capacity > 0 { return matches(before, after) && matches(before, after); }\nreturn matches(before, after);",
    );
    let wide = base
        .replace("before: InputView", "owner: Hash, before: InputView")
        .replace("return before.data.id", "return owner == before.data.owner && before.data.id;")
        .replace("matches(before", "matches(owner, before")
        .replace("forward(ckb::input", "forward(ckb::input::<Token>(1).data.owner, ckb::input");
    let same_kind = base.replace("OutputView<Token>", "InputView<Token>").replace("ckb::output::<Token>(0)", "ckb::input::<Token>(0)");
    let fixed_result = base
        .replace(
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> bool",
            "fn forward(before: InputView<Token>, after: OutputView<Token>) -> Check",
        )
        .replace("return matches(before, after);", "return new Check { ok: matches(before, after) };")
        .replace("action inspect()", "struct Check { ok: bool }\naction inspect()")
        .replace("ckb::output::<Token>(0))", "ckb::output::<Token>(0)).ok");
    for (case, source) in [&branched, &wide, &same_kind, &fixed_result].into_iter().enumerate() {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("view composition case={case} {diagnostics}: {error}")),
            );
        }
    }
}

#[test]
fn cell_view_values_bind_constructors_aliases_and_caller_slot_lifetimes() {
    let source = "module coordinates;\nresource Token has store { id: u64, }\nfn inspect(before: InputView<Token>, after: OutputView<Token>) -> bool { let input = before;\nlet data = input.data;\nlet alias = data;\nreturn alias.id == after.data.id; }\naction verify(index: u64, noise: u64) -> u64 { \nif noise == 0 { return 0; }\nlet first = ckb::input::<Token>(index);\nlet second = ckb::input::<Token>(0);\nlet before = first;\nlet ordinary = index + 1;\nrequire ordinary > 0;\nrequire inspect(before, ckb::output::<Token>(0));\nrequire second.capacity > 0;\nreturn 0; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let reject = |fixture: &Fixture| {
            let error = check_bundle_values(
                &fixture.artifact,
                &fixture.metadata,
                &fixture.record,
                &fixture.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            assert!(error.message.contains("Cell view") || error.message.contains("exact-role"), "{error}");
        };
        // Alter the shared constructor's index range, source tag and packing.
        let helper = valid.record.blocks.iter().filter(|block| block.owner_entry == "runtime:__ckb_source_input").collect::<Vec<_>>();
        let start = helper.iter().map(|block| block.range.start).min().unwrap();
        let end = helper.iter().map(|block| block.range.end).max().unwrap();
        for instruction in elf.instructions.iter().filter(|i| start <= i.address && i.address < end) {
            let word = instruction.word;
            let replacement = match word & 0x7f {
                0x13 if word >> 7 & 31 != 10 => Some(word ^ (1 << 20)),
                0x33 => Some(word ^ (1 << 20)),
                0x63 => Some(word ^ (1 << 12)),
                _ => None,
            };
            if let Some(replacement) = replacement {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, replacement);
                reject(&changed);
            }
        }
        // Every local operand load and result publication inside the marked
        // constructors/aliases must use its exact typed scalar slot.
        for block in &valid.record.blocks {
            for label in block.cell_view_value_labels.iter().filter(|label| !label.ends_with("_done")) {
                let done = format!("{label}_done");
                let end = valid.record.blocks.iter().find(|block| block.cell_view_value_labels.contains(&done)).unwrap().range.start;
                for instruction in elf.instructions.iter().filter(|i| block.range.start <= i.address && i.address < end) {
                    let word = instruction.word;
                    let replacement = match word & 0x707f {
                        0x3003 => Some(word ^ (8 << 20)),
                        0x3023 => Some(word ^ (1 << 20)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                    }
                }
            }
        }
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "verify").unwrap();
        let slot = |id| entry.locals.iter().find(|local| local.id == id).unwrap().source_id * 8;
        let arithmetic = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let call = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| op.call.as_ref().is_some_and(|call| call.target == "inspect"))
            .unwrap();
        let ordinary = slot(arithmetic.destinations[0]);
        let destination = slot(call.operands[0].local.unwrap());
        assert!(ordinary < 2048 && destination < 2048);
        let store = elf
            .instructions
            .iter()
            .find(|i| {
                i.word & 0x707f == 0x3023
                    && i.word >> 15 & 31 == 2
                    && u64::from(((i.word >> 25) << 5) | (i.word >> 7 & 31)) == ordinary
                    && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(i.address))
            })
            .unwrap();
        for partial in [false, true] {
            let offset = destination as u32 + u32::from(partial) * 7;
            let word = (store.word & 0x01fff07f) | ((offset >> 5) << 25) | ((offset & 31) << 7);
            let word = if partial { word & !(7 << 12) } else { word };
            let mut changed = valid.clone();
            changed.replace_machine_word(store.address, word);
            reject(&changed);
        }
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.cell_view_value_labels.clear();
        }
        missing.rebind_sidecars();
        reject(&missing);
        let begin = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                block.owner_entry == entry.id && block.cell_view_value_labels.iter().any(|label| !label.ends_with("_done"))
            })
            .min_by_key(|block| block.range.start)
            .unwrap();
        let label = begin.cell_view_value_labels.iter().find(|label| !label.ends_with("_done")).unwrap();
        let end = valid.record.blocks.iter().find(|block| block.cell_view_value_labels.contains(&format!("{label}_done"))).unwrap();
        let branch = elf
            .instructions
            .iter()
            .filter(|i| {
                i.address < begin.range.start
                    && i.word & 0x7f == 0x63
                    && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(i.address))
            })
            .max_by_key(|i| i.address)
            .unwrap();
        let mut bypass = valid.clone();
        bypass.redirect_conditional_branch(branch.address, end.range.start);
        reject(&bypass);
    }
}

#[test]
fn cell_view_constructor_inventory_survives_erased_markers_without_data_reads() {
    let source = "module coordinates;\nresource Token has store { id: u64, }\naction inspect() -> u64 { \nlet view = ckb::input::<Token>(0);\nreturn view.capacity; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut changed = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        assert!(changed.record.blocks.iter().any(|block| !block.cell_view_value_labels.is_empty()));
        for block in &mut changed.record.blocks {
            block.cell_view_value_labels.clear();
        }
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn named_cell_roles_bind_role_projections_and_aliases_independently() {
    use cellscript_artifact_checker::cell_selection::Read;
    let source = include_str!("../syntax_combo/seeds/named-role-selection.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CellScriptEdition::Edition2027,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        for case in 0..3 {
            let mut changed = valid.clone();
            let mut operations = changed
                .record
                .typed_semantics
                .entries
                .iter_mut()
                .flat_map(|entry| &mut entry.blocks)
                .flat_map(|block| &mut block.operations);
            match case {
                0 => {
                    let operation = operations
                        .filter(|operation| operation.opcode == "field-access")
                        .find(
                            |operation| matches!(&operation.detail, TypedSemanticOperationDetail::Field { name } if name == "orders"),
                        )
                        .unwrap();
                    operation.detail = TypedSemanticOperationDetail::Field { name: "refunds".into() };
                }
                1 => {
                    let operation = operations
                        .find(|operation| {
                            matches!(&operation.detail, TypedSemanticOperationDetail::ExactRoleRead { read: Read::Input, .. })
                        })
                        .unwrap();
                    operation.detail = TypedSemanticOperationDetail::ExactRoleRead { role: 1, read: Read::Input };
                }
                2 => {
                    let operation = operations.find(|operation| operation.opcode == "verify-exact-cell-roles").unwrap();
                    let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { unreachable!() };
                    contract.names.as_mut().unwrap().roles.swap(0, 1);
                }
                _ => unreachable!(),
            }
            changed.rebind_layered_identities();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
        // Rewrite every instruction in projection/alias/read regions in turn;
        // hashes and lower-level instruction summaries are rebound by Fixture.
        let ranges: Vec<_> = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block.exact_role_labels.iter().filter_map(|label| {
                    if !label.starts_with(".Lexact_role_use_") || label.ends_with("_done") {
                        return None;
                    }
                    let end = valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&format!("{label}_done")))?;
                    Some(block.range.start..end.range.start)
                })
            })
            .collect();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in
            elf.instructions.iter().filter(|instruction| ranges.iter().any(|range| range.contains(&instruction.address)))
        {
            let mask = match instruction.word & 0x7f {
                0x03 | 0x13 => 1 << 20,
                0x23 => 1 << 7,
                _ => continue,
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, instruction.word ^ mask);
            assert!(changed.check().is_err(), "{diagnostics} accepted named-role machine substitution at {:#x}", instruction.address);
            mutations += 1;
        }
        assert!(mutations > 40, "exercise projections, aliases and reads");
    }
}

#[test]
fn named_cell_roles_verify_with_independent_machine_checks() {
    let source = include_str!("../syntax_combo/seeds/named-role-verification.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CellScriptEdition::Edition2027,
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
    }
}

#[test]
fn cell_view_helper_calls_reject_same_schema_source_substitution() {
    let source = include_str!("../../examples/language/ckb/typed_cell_predicate.cell")
        .replace("OutputView<Token>", "InputView<Token>")
        .replace("ckb::output::<Token>(0)", "ckb::input::<Token>(0)");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut changed = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let forward = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name.ends_with("forward")).unwrap();
        let call = forward
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.call.as_ref().is_some_and(|call| call.target.ends_with("matches")))
            .unwrap();
        assert_eq!(call.operands[0].ty, call.operands[1].ty);
        assert_ne!(call.operands[0].local, call.operands[1].local);
        call.operands.swap(0, 1);
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn cell_view_helpers_bind_register_and_stack_transport_to_typed_slots() {
    let base = include_str!("../../examples/language/ckb/typed_cell_predicate.cell");
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let source = base
            .replace("before: InputView", &format!("{params}before: InputView"))
            .replace("matches(before", &format!("matches({forwarded}before"))
            .replace("forward(ckb::input", &format!("forward({arguments}ckb::input"));
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            for name in ["matches", "forward"] {
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with(name)).unwrap();
                let local = typed.locals.iter().find(|local| local.id == typed.params[padding].binding_id).unwrap();
                let slot = local.source_id as u32 * 8;
                let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let spill = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        let offset = (word >> 7 & 31) | (word >> 25 << 5);
                        (start..body).contains(&instruction.address)
                            && word & 0x707f == 0x3023
                            && word >> 15 & 31 == 2
                            && offset == slot
                    })
                    .expect("coordinate parameter spill");
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                if name == "forward" {
                    let load = elf
                        .instructions
                        .iter()
                        .find(|instruction| {
                            let word = instruction.word;
                            instruction.address >= body
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                && word & 0x707f == 0x3003
                                && word >> 15 & 31 == 2
                                && word >> 20 == slot
                        })
                        .expect("forwarded coordinate load");
                    let mut changed = valid.clone();
                    changed.replace_machine_word(load.address, load.word ^ (8 << 20));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if padding == 8 {
                        let outgoing = elf
                            .instructions
                            .iter()
                            .find(|instruction| {
                                let word = instruction.word;
                                let offset = ((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20;
                                instruction.address >= body
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                    && word & 0x707f == 0x3023
                                    && word >> 15 & 31 == 2
                                    && offset == -32
                            })
                            .expect("coordinate outgoing stack word");
                        let mut changed = valid.clone();
                        changed.replace_machine_word(outgoing.address, outgoing.word ^ (8 << 7));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
        }
    }
}

#[test]
fn cell_view_helper_coordinates_cannot_be_overwritten_by_body_stores() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let source = include_str!("../../examples/language/ckb/typed_cell_predicate.cell")
            .replace("before: InputView", &format!("delta: u64, {params}before: InputView"))
            .replace(
                "return (before.data.id",
                "let adjusted = delta + 1;\nif adjusted <= delta { return false; }\nreturn (before.data.id",
            )
            .replace("matches(before", &format!("matches(delta, {forwarded}before"))
            .replace("action inspect()", "action inspect(witness delta: u64)")
            .replace("forward(ckb::input", &format!("forward(delta, {arguments}ckb::input"));
        for data_projection in [false, true] {
            let source = if data_projection {
                source.clone()
            } else {
                source.replace(
                    "(before.data.id == after.data.id) && (before.data.owner == after.data.owner)",
                    "before.capacity <= after.capacity",
                )
            };
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap_or_else(|error| panic!("padding={padding} data={data_projection} {diagnostics}: {error}")),
                );
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("matches")).unwrap();
                let binary = typed.blocks.iter().flat_map(|block| &block.operations).find(|op| op.opcode == "binary").unwrap();
                let result = typed.locals.iter().find(|local| local.id == binary.destinations[0]).unwrap().source_id as u32 * 8;
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let store = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        let word = instruction.word;
                        word & 0x707f == 0x3023
                            && word >> 15 & 31 == 2
                            && ((word >> 7 & 31) | (word >> 25 << 5)) == result
                            && valid.record.blocks.iter().any(|block| {
                                block.owner_entry == typed.id
                                    && block.lowering_block_id.is_some()
                                    && block.range.contains(instruction.address)
                            })
                    })
                    .expect("arithmetic result has a body store");
                for param in &typed.params[padding + 1..padding + 3] {
                    let coordinate = typed.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as u32 * 8;
                    assert_ne!(coordinate, result);
                    assert!(coordinate < 2040 && result < 2048);
                    // Every write width, including a final-byte overlap, must
                    // preserve the coordinate for subsequent reads or calls.
                    for width in 0..=3 {
                        for offset in [coordinate, coordinate + 7] {
                            let mut changed = valid.clone();
                            let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                                | ((offset & 31) << 7)
                                | (width << 12)
                                | ((offset >> 5) << 25);
                            changed.replace_machine_word(store.address, word);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn nested_migration_mixed_helper_parameters_keep_coordinate_spills_bound() {
    let source = include_str!("../../examples/language/batches/nested_migration.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("migrate_document")).unwrap();
        let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for parameter in &typed.params[..2] {
            let local = typed.locals.iter().find(|local| local.id == parameter.binding_id).unwrap();
            let slot = local.source_id as u32 * 8;
            let spill = elf
                .instructions
                .iter()
                .find(|instruction| {
                    let word = instruction.word;
                    (start..body).contains(&instruction.address)
                        && word & 0x707f == 0x3023
                        && word >> 15 & 31 == 2
                        && ((word >> 7 & 31) | (word >> 25 << 5)) == slot
                })
                .unwrap();
            let mut changed = valid.clone();
            changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn record_migration_helpers_reject_rebound_paired_payload_substitution() {
    let source = include_str!("../../examples/language/batches/record_migration.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        valid.check().unwrap();
        let mut changed = valid.clone();
        let caller = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "same_message").unwrap();
        let caller_id = caller.id.clone();
        let call = caller
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.call.as_ref().is_some_and(|call| call.target.ends_with("same_payload")))
            .unwrap();
        assert_ne!(call.operands[0].local, call.operands[1].local);
        call.operands[1] = call.operands[0].clone();
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let callee = valid.record.entries.iter().find(|entry| entry.id == "helper:same_payload").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == callee.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf
            .control_flow
            .iter()
            .find(|edge| {
                edge.target == start
                    && valid.record.blocks.iter().any(|block| block.owner_entry == caller_id && block.range.contains(edge.address))
            })
            .unwrap();
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == caller_id && block.lowering_block_id.is_some())
            .map(|block| block.range.start)
            .min()
            .unwrap();
        let load = |register| {
            elf.instructions
                .iter()
                .rev()
                .find(|instruction| {
                    let word = instruction.word;
                    (body..call.address).contains(&instruction.address)
                        && word & 0x707f == 0x3003
                        && word >> 7 & 31 == register
                        && word >> 15 & 31 == 2
                })
                .unwrap()
        };
        let mut changed = valid.clone();
        for (target, substitute) in [(10, 12), (11, 13)] {
            let (target, substitute) = (load(target), load(substitute));
            assert_ne!(target.word >> 20, substitute.word >> 20);
            changed.replace_machine_word(target.address, replace_i_immediate(target.word, (substitute.word as i32) >> 20));
        }
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

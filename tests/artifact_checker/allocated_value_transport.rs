#[test]
fn allocated_transport_binds_indices_results_and_adjacent_primitives() {
    for bytes in [512, 65536] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let source = format!(
                r#"module collection_transport;
collections Work {{ limits {{ bytes: {bytes}, allocations: 8 }},
    values: Vec<u64> {{ items: 4 }},
    totals: OrderedMap<u64, u64> {{ items: 4 }}
}}
fn read(view: &Work, index: u64) -> u64 {{ return view.values.get(index); }}
action inspect(witness index: u64, witness other: u64, witness flag: bool) -> u64 {{
    if flag {{ return 0; }}
    let mut work = Work::new();
    work.values.reserve(4);
    work.values.insert(index, 7);
    work.values.set(other, 9);
    if flag {{ return 0; }}
    let copied = work.values.get(index);
    require read(&work, other) == copied;
    let removed = work.values.remove(other);
    work.values.push(removed);
    let popped = work.values.pop();
    work.totals.insert(3, popped);
    require work.totals.get(3) == copied;
    require work.totals.get_or(4, 11) == 11;
    for (key, value) in work.totals {{ require key == 3;
        require value == copied; }}
    work.drop();
    return 0;
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut counts = [0, 0];
            for (kind, prefix) in [".Lallocated_index_", ".Lallocated_result_"].iter().enumerate() {
                let mut mutated_classes = std::collections::BTreeSet::new();
                let mut bypass_checked = false;
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        if !label.starts_with(prefix) || !label.ends_with("_begin") {
                            continue;
                        }
                        counts[kind] += 1;
                        let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|b| b.allocated_lifecycle_labels.contains(&end_label))
                            .unwrap()
                            .range
                            .start;
                        for instruction in elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)) {
                            let word = instruction.word;
                            let opcode = word & 0x7f;
                            let class = (opcode, word >> 12 & 7);
                            if !mutated_classes.insert(class) {
                                continue;
                            }
                            let replacement = match opcode {
                                0x03 => word ^ (8 << 20),
                                0x23 => word ^ (8 << 7),
                                0x37 => word ^ (1 << 12),
                                _ => word ^ (1 << 20),
                            };
                            let mut changed = valid.clone();
                            changed.replace_machine_word(instruction.address, replacement);
                            let error = check_bundle_values(
                                &changed.artifact,
                                &changed.metadata,
                                &changed.record,
                                &changed.source_map,
                                &CheckerBudgets::default(),
                            )
                            .unwrap_err();
                            assert_eq!(
                                error.code,
                                CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                                "{prefix} {diagnostics} bytes={bytes} {word:#x}->{replacement:#x}: {error}"
                            );
                        }
                        if counts[kind] == 1 {
                            let mut missing = valid.clone();
                            for block in &mut missing.record.blocks {
                                block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                            }
                            missing.rebind_sidecars();
                            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            let mut forged = valid.clone();
                            for block in &mut forged.record.blocks {
                                for candidate in &mut block.allocated_lifecycle_labels {
                                    if candidate != label && candidate != &end_label {
                                        continue;
                                    }
                                    let mut parts =
                                        candidate.strip_prefix(prefix).unwrap().split('_').map(str::to_owned).collect::<Vec<_>>();
                                    // The owner source or result scratch coordinate
                                    // cannot be changed by rebinding label evidence.
                                    let field = if kind == 0 { 0 } else { 1 };
                                    parts[field] = (parts[field].parse::<u64>().unwrap() + 8).to_string();
                                    *candidate = format!("{prefix}{}", parts.join("_"));
                                }
                            }
                            forged.rebind_sidecars();
                            assert_code(&forged, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                        // Rebind an earlier source branch to skip the index or
                        // producer entirely, including all CFG/hash sidecars.
                        if !bypass_checked
                            && valid
                                .record
                                .typed_semantics
                                .entries
                                .iter()
                                .any(|entry| entry.id == block.owner_entry && entry.kind == "action")
                        {
                            let target = if kind == 0 { end } else { block.range.start };
                            let before = if kind == 0 {
                                block.range.start
                            } else {
                                valid
                                    .record
                                    .blocks
                                    .iter()
                                    .filter(|b| {
                                        b.owner_entry == block.owner_entry
                                            && b.range.start < block.range.start
                                            && b.allocated_lifecycle_labels
                                                .iter()
                                                .any(|label| label.starts_with(".Lallocated_index_") && label.ends_with("_begin"))
                                    })
                                    .map(|b| b.range.start)
                                    .max()
                                    .unwrap()
                            };
                            let branch = elf
                                .instructions
                                .iter()
                                .rev()
                                .find(|i| {
                                    i.word & 0x7f == 0x63
                                        && i.address < before
                                        && target - i.address < 4096
                                        && valid
                                            .record
                                            .blocks
                                            .iter()
                                            .any(|b| b.owner_entry == block.owner_entry && b.range.contains(i.address))
                                })
                                .unwrap();
                            let mut bypass = valid.clone();
                            bypass.redirect_conditional_branch(branch.address, target);
                            assert_code(&bypass, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                            bypass_checked = true;
                        }
                    }
                }
                // O3 may forward every variable index with ADDI instead of LD.
                assert!(
                    mutated_classes.len() >= if kind == 0 { 1 } else { 5 },
                    "missing transport instruction coverage: {prefix} {diagnostics}: {mutated_classes:?}"
                );
                assert!(bypass_checked, "missing transport bypass mutation");
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
            assert!(counts[0] >= 8 && counts[1] >= 8, "{counts:?}");
        }
    }
}

#[test]
fn allocated_transport_checks_scalar_widths_and_signed_publication() {
    for ty in ["bool", "u8", "u16", "u32", "i32", "u64"] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let literal = if ty == "bool" { "true" } else { "7" };
            let source = format!(
                r#"module scalar_collection_transport;
collections Work {{ limits {{ bytes: 512, allocations: 4 }},
    values: Vec<{ty}> {{ items: 4 }}
}}
fn read(view: &Work, index: u64) -> {ty} {{ return view.values.get(index); }}
action inspect(witness index: u64) -> u64 {{
    let value: {ty} = {literal};
    let mut work = Work::new();
    work.values.push(value);
    require (read(&work, index) == value);
    require (work.values.get(index) == value);
    work.drop();
    return 0;
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut count = 0;
            for block in &valid.record.blocks {
                for label in &block.allocated_lifecycle_labels {
                    if !label.starts_with(".Lallocated_result_") || !label.ends_with("_begin") {
                        continue;
                    }
                    count += 1;
                    let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                    let end =
                        valid.record.blocks.iter().find(|b| b.allocated_lifecycle_labels.contains(&end_label)).unwrap().range.start;
                    let body = elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)).collect::<Vec<_>>();
                    let store = body.iter().find(|i| i.word & 0x7f == 0x23).unwrap();
                    let mut changed = valid.clone();
                    changed.replace_machine_word(store.address, store.word ^ (8 << 7));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if ty == "i32" {
                        let sign = body.iter().find(|i| i.word & 0x7f == 0x13 && i.word >> 12 & 7 == 5).unwrap();
                        let mut unsigned = valid.clone();
                        unsigned.replace_machine_word(sign.address, sign.word ^ (1 << 30));
                        assert_code(&unsigned, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
            }
            assert_eq!(count, 2);
        }
    }
}

#[test]
fn allocated_borrowed_reads_preserve_reborrowed_parameter_storage() {
    let source = r#"module borrowed_alias_provenance;
collections Work { limits { bytes: 512, allocations: 4 },
    values: Vec<u64> { items: 4 }
}
fn read(view: &Work, delta: u64) -> u64 {
    borrow view as alias {
        let noise = delta + 1;
        if (noise <= delta) { return 0; }
        return alias.values.get(0);
    }
}
action inspect(witness delta: u64) -> u64 {
    let mut work = Work::new();
    work.values.push(7);
    require (read(&work, delta) == 7);
    work.drop();
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
        let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "read").unwrap();
        let slot = |local| entry.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
        let alias = entry.locals.iter().find(|value| value.name == "alias").unwrap();
        let pointer = slot(alias.id);
        let arithmetic = entry
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add"))
            .unwrap();
        let result = slot(arithmetic.destinations[0]);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let store = elf
            .instructions
            .iter()
            .find(|instruction| {
                instruction.word & 0x707f == 0x3023
                    && instruction.word >> 15 & 31 == 2
                    && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
            })
            .unwrap();
        for width in 0..=3 {
            for destination in [pointer, pointer + 7] {
                let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                    | ((destination & 31) << 7)
                    | (width << 12)
                    | ((destination >> 5) << 25);
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
            }
        }
    }
}

#[test]
fn allocated_helper_calls_bind_owner_identity_extent_and_intervening_writes() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        for bytes in [512, 65536] {
            let source = format!(
                r#"module arena_call_provenance;
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }},
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}first: &Work, second: &Work) -> u64 {{
    return first.values.get(0) * 10 + second.values.get(0);
}}
fn forward({params}first: &Work, second: &Work) -> u64 {{
    borrow first as alias {{ borrow alias as nested {{
        return read({forwarded}nested, second);
    }} }}
}}
action inspect(witness delta: u64) -> u64 {{
    let mut first = Work::new();
    let mut second = Work::new();
    first.values.push(3);
    second.values.push(7);
    let mut noise = 0;
    if delta > 0 {{ noise = delta + 1; }} else {{ noise = delta + 2; }}
    require noise > delta;
    require forward({arguments}&first, &second) == 37;
    second.drop();
    first.drop();
    return 0;
}}"#
            );
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &source,
                        CompileOptions {
                            edition: CURRENT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                for (caller_name, callee_name) in [("inspect", "forward"), ("forward", "read")] {
                    let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == caller_name).unwrap();
                    let callee = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == callee_name).unwrap();
                    let lowering = valid.record.entries.iter().find(|entry| entry.id == callee.id).unwrap();
                    let target = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                    let call = elf
                        .control_flow
                        .iter()
                        .find(|flow| {
                            flow.target == target
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(flow.address))
                        })
                        .unwrap()
                        .address;
                    let operation = typed
                        .blocks
                        .iter()
                        .flat_map(|block| &block.operations)
                        .find(|op| op.call.as_ref().is_some_and(|call| call.target == callee_name))
                        .unwrap();
                    let slot = |local| typed.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
                    let pointers =
                        [slot(operation.operands[padding].local.unwrap()), slot(operation.operands[padding + 1].local.unwrap())];
                    let body_load = |offset| {
                        elf.instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                instruction.address < call
                                    && instruction.word & 0x707f == 0x3003
                                    && instruction.word >> 15 & 31 == 2
                                    && instruction.word >> 20 == offset
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .expect("arena argument reload")
                    };
                    let load = body_load(pointers[0]);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(load.address, replace_i_immediate(load.word, pointers[1] as i32));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    if caller_name == "forward" {
                        let lowering = valid.record.entries.iter().find(|entry| entry.id == typed.id).unwrap();
                        let start = valid.record.blocks.iter().find(|block| block.id == lowering.entry_block).unwrap().range.start;
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .filter(|block| block.owner_entry == typed.id && block.lowering_block_id.is_some())
                            .map(|block| block.range.start)
                            .min()
                            .unwrap();
                        let spills = elf
                            .instructions
                            .iter()
                            .filter(|instruction| {
                                (start..end).contains(&instruction.address)
                                    && instruction.word & 0x707f == 0x3023
                                    && instruction.word >> 15 & 31 == 2
                            })
                            .collect::<Vec<_>>();
                        let offset = |word: u32| (word >> 7 & 31) | (word >> 25 << 5);
                        let lengths = [padding, padding + 1].map(|position| {
                            let pointer = slot(typed.params[position].binding_id);
                            let index = spills.iter().position(|instruction| offset(instruction.word) == pointer).unwrap();
                            offset(spills[index + 1].word)
                        });
                        let load = body_load(lengths[0]);
                        for replacement in [replace_i_immediate(load.word, lengths[1] as i32), encode_addi(load.word >> 7 & 31, 0, 0)]
                        {
                            let mut changed = valid.clone();
                            changed.replace_machine_word(load.address, replacement);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    } else {
                        let length_word = padding as u32 + 1;
                        let (register, before) = if length_word < 8 {
                            (10 + length_word, call)
                        } else {
                            let outgoing = ((padding as u32 + 4).saturating_sub(8) * 8).next_multiple_of(16);
                            let offset = i64::from(length_word - 8) * 8 - i64::from(outgoing);
                            let store =
                                elf.instructions
                                    .iter()
                                    .rev()
                                    .find(|instruction| {
                                        let word = instruction.word;
                                        let immediate = i64::from(((((word >> 7 & 31) | (word >> 25 << 5)) as i32) << 20) >> 20);
                                        instruction.address < call
                                            && word & 0x707f == 0x3023
                                            && word >> 15 & 31 == 2
                                            && immediate == offset
                                            && valid.record.blocks.iter().any(|block| {
                                                block.owner_entry == typed.id && block.range.contains(instruction.address)
                                            })
                                    })
                                    .expect("owned extent outgoing stack word");
                            (store.word >> 20 & 31, store.address)
                        };
                        let producer = elf
                            .instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                instruction.address < before
                                    && instruction.word >> 7 & 31 == register
                                    && matches!(instruction.word & 0x7f, 0x03 | 0x13 | 0x1b | 0x33 | 0x37)
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .expect("owned extent constant producer");
                        assert_ne!(register, 2);
                        let mut changed = valid.clone();
                        changed.replace_machine_word(producer.address, encode_addi(register, 0, 0));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        let binaries = typed.blocks.iter().flat_map(|block| &block.operations).filter(|op| {
                            matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "add")
                        }).collect::<Vec<_>>();
                        assert_eq!(binaries.len(), 2, "both normal predecessors must be exercised");
                        let stores = binaries
                            .iter()
                            .map(|binary| {
                                let result = slot(binary.destinations[0]);
                                elf.instructions
                                    .iter()
                                    .find(|instruction| {
                                        instruction.word & 0x707f == 0x3023
                                            && instruction.word >> 15 & 31 == 2
                                            && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                                            && valid.record.blocks.iter().any(|block| {
                                                block.owner_entry == typed.id && block.range.contains(instruction.address)
                                            })
                                    })
                                    .unwrap()
                            })
                            .collect::<Vec<_>>();
                        let owner_use = typed
                            .blocks
                            .iter()
                            .flat_map(|block| &block.operations)
                            .find(|op| {
                                matches!(
                                    op.detail,
                                    TypedSemanticOperationDetail::AllocatedValue {
                                        operation: cellscript_artifact_checker::allocated_values::Operation::Push,
                                        ..
                                    }
                                )
                            })
                            .unwrap();
                        // Mutate the retained binding consumed by the call;
                        // the constructor's initial temporary can already be dead.
                        let pointer = slot(owner_use.operands[0].local.unwrap());
                        for store in stores {
                            for width in 0..=3 {
                                for destination in [pointer, pointer + 7] {
                                    let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                                        | ((destination & 31) << 7)
                                        | (width << 12)
                                        | ((destination >> 5) << 25);
                                    let mut changed = valid.clone();
                                    changed.replace_machine_word(store.address, word);
                                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn allocated_helper_calls_recheck_pointer_provenance_on_loop_backedges() {
    for bytes in [512, 65536] {
        let source = format!(
            r#"module arena_call_backedge;
collections Work {{ limits {{ bytes: {bytes}, allocations: 4 }},
    values: Vec<u64> {{ items: 4 }}
}}
fn read(work: &Work) -> u64 {{ return work.values.get(0); }}
action inspect(witness delta: u64) -> u64 {{
    let mut work = Work::new();
    work.values.push(7);
    for i in 0..2 {{
        require read(&work) == 7;
        let noise = delta ^ i;
        require noise <= delta + 2;
    }}
    work.drop();
    return 0;
}}"#
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let entry = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
            let slot = |local| entry.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
            let operations = entry.blocks.iter().flat_map(|block| &block.operations).collect::<Vec<_>>();
            let arithmetic = operations
                .iter()
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "bit-xor"))
                .unwrap();
            let push = operations
                .iter()
                .find(|op| {
                    matches!(
                        op.detail,
                        TypedSemanticOperationDetail::AllocatedValue {
                            operation: cellscript_artifact_checker::allocated_values::Operation::Push,
                            ..
                        }
                    )
                })
                .unwrap();
            let result = slot(arithmetic.destinations[0]);
            let pointer = slot(push.operands[0].local.unwrap());
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let store = elf
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.word & 0x707f == 0x3023
                        && instruction.word >> 15 & 31 == 2
                        && ((instruction.word >> 7 & 31) | (instruction.word >> 25 << 5)) == result
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == entry.id && block.range.contains(instruction.address))
                })
                .unwrap();
            // The first helper call precedes this store and has valid facts.
            // A poisoned retained binding must still invalidate the next call
            // when its incoming state is merged through the loop backedge.
            let helper = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "read").unwrap();
            let call = elf
                .control_flow
                .iter()
                .find(|flow| {
                    valid.record.blocks.iter().any(|block| block.owner_entry == helper.id && block.range.start == flow.target)
                })
                .unwrap()
                .address;
            assert!(call < store.address);
            assert!(elf.control_flow.iter().any(|flow| flow.address > store.address
                && flow.target <= call
                && valid.record.blocks.iter().any(|block| block.owner_entry == entry.id && block.range.contains(flow.address))));
            for width in [0, 3] {
                for destination in [pointer, pointer + 7] {
                    let word = (store.word & !((31 << 7) | (7 << 12) | (127 << 25)))
                        | ((destination & 31) << 7)
                        | (width << 12)
                        | ((destination >> 5) << 25);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(store.address, word);
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

#[test]
fn allocated_coordinate_calls_bind_guard_spills_bounds_and_restoration() {
    for padding in [0, 5, 6] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &collection_coordinate_helpers::source(padding, bytes),
                        CompileOptions {
                            edition: CURRENT_EDITION,
                            diagnostics,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                );
                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let words = elf
                    .instructions
                    .iter()
                    .map(|instruction| (instruction.address, instruction.word))
                    .collect::<std::collections::BTreeMap<_, _>>();
                for name in ["inspect", "read"] {
                    let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == name).unwrap();
                    let guards = elf
                        .instructions
                        .iter()
                        .filter(|instruction| {
                            instruction.word == 0xfe010113
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                                && (1..=3).all(|index| {
                                    words.get(&(instruction.address + index * 4)).is_some_and(|word| word & 0x707f == 0x3023)
                                })
                        })
                        .map(|instruction| instruction.address)
                        .collect::<Vec<_>>();
                    assert!(!guards.is_empty());
                    for start in [guards[0], *guards.last().unwrap()] {
                        let guard = (start + 16..start + 144)
                            .step_by(4)
                            .find(|pc| words.get(pc).is_some_and(|word| word & 0x7f == 0x63))
                            .unwrap();
                        let restore = guard + 12;
                        assert_eq!(words[&(restore + 12)], 0x02010113);
                        let mut mutations = vec![
                            (start, encode_addi(2, 2, -16)),
                            (guard, words[&guard] ^ (1 << 12)),
                            (guard + 4, words[&(guard + 4)] ^ (1 << 12)),
                            (guard + 8, words[&(guard + 8)] | (1 << 7)),
                            (restore + 12, encode_addi(2, 2, 16)),
                        ];
                        if name == "inspect" {
                            // Transport relies on exact restoration across the
                            // sequence. The callee's separate isolation proof
                            // may accept changes to otherwise dead scratch
                            // registers; complete read-value proof is pending.
                            for index in 0..3u64 {
                                let spill = start + 4 + index * 4;
                                mutations.push((spill, words[&spill] & !(31 << 20)));
                                let load = restore + index * 4;
                                mutations.push((load, words[&load] ^ (8 << 20)));
                            }
                        }
                        // Bind the lower/upper bound registers too, not merely
                        // the branch opcode and its error destination.
                        mutations.push((guard, (words[&guard] & !(31 << 20)) | (2 << 20)));
                        mutations.push((guard + 4, (words[&(guard + 4)] & !(31 << 15)) | (2 << 15)));
                        for (pc, word) in mutations {
                            let mut changed = valid.clone();
                            changed.replace_machine_word(pc, word);
                            let code = match changed.check() {
                                Err(code) => code,
                                Ok(()) => panic!("accepted {name} {diagnostics}, padding={padding}, bytes={bytes}, start={start:#x}, pc={pc:#x}, old={:#x}, new={word:#x}", words[&pc]),
                            };
                            assert!(
                                matches!(
                                    code,
                                    CheckerRejectionCode::V2420TypedMachineBindingInvalid
                                        | CheckerRejectionCode::V2407AbiOrStackInvalid
                                ),
                                "{name} at {pc:#x}, {word:#x}: {code:?}"
                            );
                        }
                        // Coherently rebound control flow cannot jump into the
                        // middle of a spill/restore sequence.
                        let mut changed = valid.clone();
                        changed.replace_machine_word(start + 4, encode_jal((guard - start - 4) as i32));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                }
                let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
                let operation = typed
                    .blocks
                    .iter()
                    .flat_map(|block| &block.operations)
                    .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "forward"))
                    .unwrap();
                let local = operation.operands[padding + 1].local.unwrap();
                let coordinate = typed.locals.iter().find(|value| value.id == local).unwrap().source_id as u32 * 8;
                let callee = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "forward").unwrap();
                let callee = valid.record.entries.iter().find(|entry| entry.id == callee.id).unwrap();
                let target = valid.record.blocks.iter().find(|block| block.id == callee.entry_block).unwrap().range.start;
                let call = elf.control_flow.iter().find(|flow| flow.target == target).unwrap().address;
                let load = elf
                    .instructions
                    .iter()
                    .rev()
                    .find(|instruction| {
                        instruction.address < call
                            && instruction.word & 0x707f == 0x3003
                            && instruction.word >> 15 & 31 == 2
                            && instruction.word >> 20 == coordinate
                            && valid
                                .record
                                .blocks
                                .iter()
                                .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                    })
                    .expect("coordinate reload before the helper call");
                let mut changed = valid.clone();
                changed.replace_machine_word(load.address, load.word ^ (8 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_helper_parameters_compose_with_nested_and_coordinate_decoders() {
    let source = r#"module mixed_arena_parameters;
resource Token has store { amount: u64, }
collections Work {
    limits { bytes: 512, allocations: 4 },
    values: Vec<u64> { items: 4 }
}
fn read(view: &Work, before: InputView<Token>, items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {
    if (items.len() == 0) { return 0; }
    return view.values.get(0) + before.capacity;
}
fn forward(view: &Work, before: InputView<Token>, items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {
    return read(view, before, items);
}
action inspect(witness items: NestedValues<Vec<u8>, 2, 8, 32>) -> u64 {

    let mut work = Work::new();
    work.values.push(3);
    let before = ckb::input::<Token>(0);
    require (forward(&work, before, items) > 0);
    return 0;
}"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        for name in ["read", "forward"] {
            let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with(name)).unwrap();
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
            // The owner is the first ABI pair. Corrupt both incoming spills
            // before the nested decoder, whose verified seed must retain them.
            for register in [10, 11] {
                let spill = elf
                    .instructions
                    .iter()
                    .find(|instruction| {
                        (start..end).contains(&instruction.address)
                            && instruction.word & 0x707f == 0x3023
                            && instruction.word >> 20 & 31 == register
                    })
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

#[test]
fn allocated_reads_bind_copy_buffers_pop_and_missing_key_behavior() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module read_proof;
struct Item {{ amount: u128, tag: u64 }}
collections Work {{
    limits {{ bytes: {bytes}, allocations: 16 }},
    values: Vec<Item> {{ items: 4 }},
    totals: OrderedMap<u64, u128> {{ items: 4 }}
}}
fn copied(view: &Work) -> Item {{ return view.values.get(0); }}
action inspect(flag: bool) -> u64 {{
    if flag {{ return 0; }}
    let first = Work::new();
    let mut second = Work::new();
    second.values.push(new Item {{ amount: 3, tag: 4 }});
    second.values.push(new Item {{ amount: 5, tag: 6 }});
    let initial = second.values.get(0);
    borrow second as view {{ require copied(view).tag == 4; }}
    let removed = second.values.remove(0);
    let popped = second.values.pop();
    second.totals.insert(256, 9);
    require second.totals.get(256) == 9;
    require second.totals.get_or(255, 17) == 17;
    require initial.tag == 4;
    require removed.tag == 4;
    require popped.tag == 6;
    second.drop();
    first.drop();
    return 0;
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            for (prefix, children, minimum) in [
                (".Lallocated_read_", &[][..], 10),
                (".Lallocated_pop_", &[".Lallocated_read_", ".Lallocated_remove_"][..], 1),
                (".Lallocated_take_", &[".Lallocated_read_", ".Lallocated_remove_"][..], 0),
                (".Lallocated_lookup_", &[".Lallocated_find_", ".Lallocated_read_"][..], 2),
                (".Lallocated_required_", &[".Lallocated_lookup_"][..], 1),
            ] {
                allocated_body_mutations(&valid, prefix, children, minimum, false);
            }
        }
    }
}

#[test]
fn allocated_sorted_bodies_bind_counts_order_overlap_and_last_value_copy() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module sorted_proof;
collections Work {{
    limits {{ bytes: {bytes}, allocations: 8 }},
    totals: OrderedMap<u64, u128> {{ items: 4 }},
    seen: OrderedSet<Hash> {{ items: 4 }}
}}
action inspect(flag: bool, witness entries: [(u64, u128); 4], witness keys: [Hash; 4]) -> u64 {{
    if flag {{ return 0; }}
    let first = Work::new();
    let mut second = Work::new();
    second.totals.load_sorted(entries);
    second.seen.load_sorted(keys);
    require second.totals.len() <= 4;
    require second.seen.len() <= 4;
    second.drop();
    first.drop();
    return 0;
}}"#
            );
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            allocated_body_mutations(&valid, ".Lallocated_sorted_4_", &[".Lallocated_reserve_"], 45, true);
            let mut wrong_count = valid.clone();
            for block in &mut wrong_count.record.blocks {
                for label in &mut block.allocated_lifecycle_labels {
                    *label = label.replace(".Lallocated_sorted_4_", ".Lallocated_sorted_3_");
                }
            }
            wrong_count.rebind_sidecars();
            assert_code(&wrong_count, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
    for input in ["[]", "[(1, 7)]"] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let source = format!("module short_sorted;\ncollections Work {{ limits {{ bytes: 256, allocations: 2 }},\nvalues: OrderedMap<u64, u64> {{ items: 4 }} }}\naction inspect() -> u64 {{ let mut work = Work::new();\nwork.values.reserve(4);\nwork.values.load_sorted({input});\nreturn 0; }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            allocated_body_mutations(&valid, ".Lallocated_sorted_", &[".Lallocated_reserve_"], 1, true);
        }
    }
}

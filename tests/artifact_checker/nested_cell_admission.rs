#[test]
fn nested_cell_codec_rejects_rehashed_load_table_and_decoder_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = std::collections::BTreeSet::new();
        let mut sites = 0;
        for block in &valid.record.blocks {
            for label in &block.nested_cell_labels {
                if label.ends_with("_done") || label.ends_with("_validate") {
                    continue;
                }
                sites += 1;
                let validation = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_cell_labels.contains(&format!("{label}_validate")))
                    .unwrap()
                    .range
                    .start;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_cell_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                for instruction in elf.instructions.iter().filter(|i| i.address >= block.range.start && i.address < end) {
                    let word = instruction.word;
                    let rd = word >> 7 & 31;
                    // Every table/element condition and byte load; transport
                    // constants bind the capacity, source and ordinal too.
                    let changed = if word & 0x7f == 0x63 {
                        Some(word ^ (1 << 15))
                    } else if word & 0x707f == 0x4003
                        || (instruction.address < validation && word & 0x707f == 0x13 && [5, 10, 11, 13, 14].contains(&rd))
                    {
                        Some(word ^ (1 << 20))
                    } else {
                        None
                    };
                    if let Some(changed) = changed {
                        mutations.insert((instruction.address, changed));
                    }
                    if instruction.address < validation && word & 0x707f == 0x13 && word >> 15 & 31 == 0 && [5, 13, 14].contains(&rd) {
                        mutations.insert((instruction.address, word | (rd << 15)));
                    }
                }
            }
        }
        assert!(sites >= 2, "input and output codecs must both be bound");
        assert!(mutations.len() > 100);
        for (address, word) in mutations {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{diagnostics}, {address:#x}: {error}");
        }
    }
}

#[test]
fn nested_cell_codec_requires_markers_for_unused_fields_and_group_roles() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = "module unused_nested_cell;\nresource Document has store, consume { id: u64, entries: NestedValues<Vec<u8>, 2, 3, 32>, }\naction remove(input before: Document) { \nconsume before; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "nested-document".into(),
                    context: ArtifactContext::TypeGroup {},
                    bindings: cellscript::artifact::EntryCellBindings::GroupRelative,
                    coverage: cellscript::artifact::EntryCoverage::FixedGroup,
                    layout: Some("Document".into()),
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    action: None,
                    actions: vec![ArtifactAction { tag: 1, action: "remove".into() }],
                    common_checks: vec![],
                },
                cellscript::ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap(),
        );
        assert!(valid.record.blocks.iter().any(|block| !block.nested_cell_labels.is_empty()));
        let mut changed = valid.clone();
        for block in &mut changed.record.blocks {
            block.nested_cell_labels.clear();
        }
        // Rebind the complete sidecar envelope without changing ELF bytes.
        let elf = parse_elf(&changed.artifact, CheckerBudgets::default().instructions).unwrap();
        let instruction = &elf.instructions[0];
        changed.replace_machine_word(instruction.address, instruction.word);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("lacks a complete load codec"), "{error}");
    }
}

#[test]
fn nested_cell_codec_rejects_a_rebound_entry_after_the_syscall() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let target = valid
            .record
            .blocks
            .iter()
            .find(|block| block.nested_cell_labels.iter().any(|label| label.ends_with("_validate")))
            .unwrap();
        let label = target.nested_cell_labels.iter().find(|label| label.ends_with("_validate")).unwrap();
        let done_label = label.replace("_validate", "_done");
        let done = valid.record.blocks.iter().find(|block| block.nested_cell_labels.contains(&done_label)).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let flow = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.address >= done
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == target.owner_entry && block.range.contains(flow.address))
                    && valid.record.verifier_failure_exits.iter().any(|exit| exit.code == 5 && exit.address == flow.target)
            })
            .unwrap();
        let block = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
        let word = elf.instructions.iter().find(|instruction| instruction.address == flow.address).unwrap().word;
        assert_eq!(word & 0x7f, 0x63);
        let offset = target.range.start as i64 - flow.address as i64;
        assert!((-4096..4096).contains(&offset));
        let immediate = offset as u32;
        let branch = (word & 0x01fff07f)
            | (((immediate >> 12) & 1) << 31)
            | (((immediate >> 5) & 0x3f) << 25)
            | (((immediate >> 1) & 15) << 8)
            | (((immediate >> 11) & 1) << 7);
        let mut changed = valid.clone();
        changed.record.edges.iter_mut().find(|edge| edge.from == block.id && edge.kind == EdgeKind::ConditionalTaken).unwrap().to =
            target.id.clone();
        let mut reachable = std::collections::BTreeSet::new();
        let mut pending = changed.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if reachable.insert(id.clone()) {
                pending.extend(changed.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
            }
        }
        for block in &mut changed.record.blocks {
            block.reachable = reachable.contains(&block.id);
        }
        changed.record.canonicalize();
        changed.replace_machine_word(flow.address, branch);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("external control flow bypasses Cell load"), "{error}");
    }
}

#[test]
fn nested_lifetime_preserves_hidden_result_pointer_provenance() {
    let source = "module nested_result_write;\nstruct Pair { left: u64, right: u64, }\nfn make(at: u64) -> Pair { new Pair { left: at, right: at + 1, } }\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { \nlet bytes = values[0];\nlet pair = make(bytes.len());\nrequire pair.right > pair.left;\nreturn bytes.len(); }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let callee = valid.record.entries.iter().find(|entry| entry.name == "make").unwrap();
        assert_eq!(callee.fixed_result_abi.as_ref().unwrap().hidden_argument_index, 1);
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let spill = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                    && instruction.word & 0x707f == 0x3023
                    && instruction.word >> 20 & 31 == 10
            })
            .unwrap();
        let mut changed = valid.clone();
        // Replace the ordinary parameter spill with sd a0, 0(a1). The incoming
        // a1 is a valid result capability, but only the checked return copy may
        // exercise it. Rebinding hashes must not turn it into arbitrary writes.
        changed.replace_machine_word(spill.address, (10 << 20) | (11 << 15) | 0x3023);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("private stack provenance"), "{error}");

        let abi = callee.fixed_result_abi.as_ref().unwrap();
        let body_store = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                    && instruction.address >= abi.save_range.end
                    && instruction.word & 0x707f == 0x3023
            })
            .unwrap();
        // Corrupt the final byte of the saved hidden pointer through FP. The
        // copy range still contains its original load and exact width; only a
        // lifetime proof notices that the loaded capability was invalidated.
        let offset = (i64::from(abi.saved_pointer_offset) + 7 - i64::from(abi.owned_frame_bytes)) as u32 & 0xfff;
        let byte_store = ((offset >> 5) << 25) | (body_store.word & (31 << 20)) | (8 << 15) | ((offset & 31) << 7) | 0x23;
        let mut changed = valid.clone();
        changed.replace_machine_word(body_store.address, byte_store);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("private stack provenance"), "{error}");
    }
}

#[test]
fn nested_lifetime_checks_transitive_callee_memory_effects() {
    let source = "module nested_callee;\nfn bump(at: u64) -> u64 { return at + 1; }\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness at: u64) -> u64 { \nlet bytes = values[at];\nrequire bump(at) > at;\nreturn bytes.len(); }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let callee = valid.record.entries.iter().find(|entry| entry.name == "bump").unwrap();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let store = elf
        .instructions
        .iter()
        .find(|instruction| {
            valid.record.blocks.iter().any(|block| block.owner_entry == callee.id && block.range.contains(instruction.address))
                && instruction.word & 0x707f == 0x3023
        })
        .unwrap();
    let mut changed = valid.clone();
    // An argument-derived write has no proof that it stays in the helper's own frame.
    changed.replace_machine_word(store.address, (store.word & !(31 << 15)) | (10 << 15));
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
    assert!(error.to_string().contains("private stack provenance"), "{error}");
}

#[test]
fn nested_lifetime_checks_fixed_copy_destinations_after_hash_rebinding() {
    use cellscript::artifact::{compile_artifact, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    let source = "module nested_copy_effects;\nresource Token has store, consume { expected: Hash, }\naction inspect(input token: Token, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness expected: Hash) { \nrequire values.len() > 0;\nrequire token.expected == expected;\nconsume token; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile_artifact(
                source,
                CompileOptions {
                    diagnostics,
                    target: Some("riscv64-elf".into()),
                    target_profile: Some("ckb".into()),
                    ..Default::default()
                },
                ArtifactDeclaration {
                    name: "nested-copy".into(),
                    context: ArtifactContext::TypeGroup {},
                    bindings: cellscript::artifact::EntryCellBindings::GroupRelative,
                    coverage: cellscript::artifact::EntryCoverage::FixedGroup,
                    layout: Some("Token".into()),
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    action: None,
                    actions: vec![ArtifactAction { tag: 1, action: "inspect".into() }],
                    common_checks: vec![],
                },
                cellscript::ExecutableSurfacePolicy::AllowFailClosed,
            )
            .unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let pointer = typed.locals.iter().find(|local| local.ty.starts_with("NestedValues<")).unwrap().source_id as i32 * 8;
        let copy = valid.record.entries.iter().find(|entry| entry.name == "__cellscript_memcpy_fixed").unwrap();
        let target = valid.record.blocks.iter().find(|block| block.id == copy.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf
            .control_flow
            .iter()
            .find(|flow| {
                flow.target == target
                    && valid.record.blocks.iter().any(|block| block.owner_entry == typed.id && block.range.contains(flow.address))
            })
            .unwrap();
        let destination = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < call.address
                    && instruction.address + 32 >= call.address
                    && instruction.word & 0x707f == 0x13
                    && instruction.word >> 7 & 31 == 11
                    && instruction.word >> 15 & 31 == 2
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(destination.address, replace_i_immediate(destination.word, pointer));
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("unverified write overlaps"), "{error}");
        let mut changed = valid.clone();
        let helper_store = elf.instructions.iter().find(|instruction| instruction.address == target + 8).unwrap();
        changed.replace_machine_word(helper_store.address, helper_store.word ^ (1 << 15));
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        // The fixed-copy body is independently checked before the lifetime
        // walk; either way this exact machine-binding rejection is mandatory.
    }
}

#[test]
fn nested_lifetime_rejects_coherent_derived_pointer_aliasing_of_a_root_slot() {
    let source = "module nested_physical_alias;\naction inspect(witness values: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 { \nlet bytes = values[0];\nreturn bytes.len() + values.len(); }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "inspect").unwrap();
        let root = typed.locals.iter().find(|local| local.id == typed.params[0].binding_id).unwrap().source_id;
        let destination = typed
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find(|operation| operation.opcode == "index")
            .unwrap()
            .destinations[0];
        let old = typed.locals.iter().find(|local| local.id == destination).unwrap().source_id;
        assert_ne!(old, root);
        let mut changed = valid.clone();
        changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .find(|entry| entry.id == typed.id)
            .unwrap()
            .locals
            .iter_mut()
            .find(|local| local.id == destination)
            .unwrap()
            .source_id = root;
        let prefix = format!(".Lnested_use_index_{old}_");
        for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == typed.id) {
            for label in &mut block.nested_read_labels {
                if let Some(suffix) = label.strip_prefix(&prefix) {
                    *label = format!(".Lnested_use_index_{root}_{suffix}");
                }
            }
        }
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let replacements = elf
            .instructions
            .iter()
            .filter(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
            })
            .filter_map(|instruction| {
                let word = instruction.word;
                if word >> 15 & 31 != 2 {
                    return None;
                }
                if word & 0x707f == 0x3003 && (word as i32) >> 20 == old as i32 * 8 {
                    Some((instruction.address, replace_i_immediate(word, root as i32 * 8)))
                } else if word & 0x707f == 0x3023 && decode_s_immediate(word) == old as i32 * 8 {
                    let offset = root as u32 * 8;
                    Some((instruction.address, (word & !0xfe000f80) | ((offset & 0xfe0) << 20) | ((offset & 31) << 7)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert!(replacements.len() >= 2, "rewrite both derived pointer producer and consumer");
        changed.record.canonicalize();
        changed.rebind_layered_identities();
        changed.replace_machine_words(&replacements);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("nested pointer slot aliases another typed local"), "{error}");
    }
}

#[test]
fn allocated_values_sorted_arrays_reject_rebound_shapes_after_admission() {
    let source = "module sorted_proof;\ncollections Work { limits { bytes: 128, allocations: 2 },\ntotals: OrderedMap<u8, u16> { items: 4 } }\naction inspect() -> u64 { let mut work = Work::new();\nwork.totals.load_sorted([(1, 7), (1, 11), (2, 13)]);\nreturn 0; }";
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() })
            .unwrap(),
    );
    valid.check().unwrap();
    for mutation in 0..6 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| {
                matches!(
                    op.detail,
                    TypedSemanticOperationDetail::AllocatedValue {
                        operation: cellscript_artifact_checker::allocated_values::Operation::LoadSorted,
                        ..
                    }
                )
            })
            .unwrap();
        match mutation {
            0 => op.operands[1].ty = "[(u16, u8); 3]".into(),
            1 => op.operands[1].ty = "[(u8, u16); 1025]".into(),
            2 => op.operands[1].ty = "[(u8, u16); 2]".into(),
            3 => op.operands[0].ty = "&Work".into(),
            4 => {
                op.operands.pop();
            }
            _ => op.operands.push(op.operands[1].clone()),
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    for mutation in 0..3 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "tuple" && op.operands.len() == 3)
            .unwrap();
        match mutation {
            0 => {
                op.operands.pop();
            }
            1 => op.operands.push(op.operands[0].clone()),
            _ => op.operands[0].ty = "(u16, u8)".into(),
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    assert_bounded_admitted(&valid);
}

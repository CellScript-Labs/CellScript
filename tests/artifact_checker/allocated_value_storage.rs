/// Mutate core instructions outside already-checked children. Guard spills are
/// covered by dedicated tests; retain byte-store mutations inside those guards.
fn allocated_body_mutations(valid: &Fixture, prefix: &str, children: &[&str], minimum: usize, all: bool) {
    let ranges = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
            .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
            .map(|(block, label)| {
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                (label.clone(), end_label, block.range.start..end)
            })
            .collect::<Vec<_>>()
    };
    let candidates = ranges(prefix);
    assert!(!candidates.is_empty(), "{prefix}");
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
        assert!(error.message.contains("allocated allocation"), "{error}");
    };
    for (begin_label, end_label, range) in candidates.iter().take(if all { candidates.len() } else { 1 }) {
        let child_ranges = children
            .iter()
            .flat_map(|prefix| ranges(prefix))
            .map(|(_, _, range)| range)
            .filter(|child| range.start <= child.start && child.end <= range.end)
            .collect::<Vec<_>>();
        let body = valid
            .record
            .blocks
            .iter()
            .filter(|block| {
                range.contains(&block.range.start)
                    && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lvalue_vector_checked_"))
            })
            .map(|block| block.range.start)
            .min()
            .unwrap_or(range.start);
        let mut in_guard = false;
        let mut mutated = 0;
        for instruction in elf
            .instructions
            .iter()
            .filter(|i| body <= i.address && i.address < range.end && !child_ranges.iter().any(|child| child.contains(&i.address)))
        {
            let word = instruction.word;
            if word == 0xfe010113 {
                in_guard = true;
            }
            if word == 0x02010113 {
                in_guard = false;
                continue;
            }
            let replacement = if word & 0x707f == 0x23 {
                Some(word ^ (1 << 20))
            } else if in_guard {
                None
            } else {
                match word & 0x7f {
                    0x63 => Some(word ^ (1 << 12)),
                    0x03 => Some(word ^ (1 << 15)),
                    0x13 | 0x1b | 0x33 => Some(word ^ (1 << 20)),
                    0x37 => Some(word ^ (1 << 12)),
                    0x23 => Some(word ^ (1 << 20)),
                    _ => None,
                }
            };
            if let Some(replacement) = replacement {
                let mut changed = valid.clone();
                changed.replace_machine_word(instruction.address, replacement);
                reject(&changed);
                mutated += 1;
            }
        }
        assert!(mutated >= minimum, "{prefix}: {mutated} mutations");
        let mut missing = valid.clone();
        for block in &mut missing.record.blocks {
            block.allocated_lifecycle_labels.retain(|label| label != begin_label && label != end_label);
        }
        missing.rebind_sidecars();
        reject(&missing);
        if let Some(child) = child_ranges.iter().filter(|child| child.start > range.start).min_by_key(|child| child.start) {
            let branch = elf
                .instructions
                .iter()
                .find(|i| range.start <= i.address && i.address < child.start && i.word & 0x7f == 0x63)
                .unwrap();
            let interior = valid
                .record
                .blocks
                .iter()
                .filter(|block| child.start < block.range.start && block.range.start < child.end)
                .map(|block| block.range.start)
                .min()
                .unwrap();
            let mut bypass = valid.clone();
            bypass.redirect_conditional_branch(branch.address, interior);
            reject(&bypass);
        }
    }
    let mut erased = valid.clone();
    for block in &mut erased.record.blocks {
        block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
    }
    erased.rebind_sidecars();
    reject(&erased);
}

#[test]
fn allocated_initializers_bind_every_state_word_and_reject_rebound_bypasses() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module initialization;\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }},\nvalues: Vec<u64> {{ items: 4 }},\ntotals: OrderedMap<Hash, u128> {{ items: 4 }},\nseen: OrderedSet<u64> {{ items: 4 }} }}\naction inspect(flag: bool) -> u64 {{ if flag {{ return 0; }}\nlet mut first = Work::new();\nfirst.values.push(7);\nlet mut second = Work::new();\nsecond.values.push(8);\nrequire first.values.get(0) == 7;\nrequire second.values.get(0) == 8;\nreturn 0; }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let begin = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.allocated_init_labels.iter().any(|label| label.contains("_begin_")))
                .min_by_key(|block| block.range.start)
                .unwrap();
            let label = begin.allocated_init_labels.iter().find(|label| label.contains("_begin_")).unwrap();
            let prefix = format!("{}_end_", label.split("_begin_").next().unwrap());
            let end = valid
                .record
                .blocks
                .iter()
                .find(|block| block.allocated_init_labels.iter().any(|label| label.starts_with(&prefix)))
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let stores = elf
                .instructions
                .iter()
                .filter(|instruction| {
                    begin.range.start <= instruction.address
                        && instruction.address < end.range.start
                        && instruction.word & 0x707f == 0x3023
                })
                .collect::<Vec<_>>();
            assert_eq!(stores.len(), 17); // Four state words, four per member, owner pointer.
            for store in &stores {
                let mut changed = valid.clone();
                changed.replace_machine_word(store.address, 0x00000013);
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.message.contains("allocated initialization"), "{error}");
            }
            for index in [7, 11, 15, 16] {
                // Live flags and published owner pointer must not be zero.
                let mut changed = valid.clone();
                changed.replace_machine_word(stores[index].address, stores[index].word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_init_labels.clear();
            }
            missing.rebind_sidecars();
            assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

            // Rebind the machine CFG as well as all byte/outer hashes. A
            // structurally valid branch must not enter after initialization.
            let flow = elf
                .control_flow
                .iter()
                .find(|flow| {
                    flow.address < begin.range.start
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == begin.owner_entry && block.range.contains(flow.address))
                        && elf
                            .instructions
                            .iter()
                            .any(|instruction| instruction.address == flow.address && instruction.word & 0x7f == 0x63)
                })
                .unwrap();
            let origin = valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap();
            let word = elf.instructions.iter().find(|instruction| instruction.address == flow.address).unwrap().word;
            let offset = end.range.start as i64 - flow.address as i64;
            assert!((-4096..4096).contains(&offset));
            let immediate = offset as u32;
            let branch = (word & 0x01fff07f)
                | (((immediate >> 12) & 1) << 31)
                | (((immediate >> 5) & 0x3f) << 25)
                | (((immediate >> 1) & 15) << 8)
                | (((immediate >> 11) & 1) << 7);
            let mut bypass = valid.clone();
            bypass
                .record
                .edges
                .iter_mut()
                .find(|edge| edge.from == origin.id && edge.kind == EdgeKind::ConditionalTaken)
                .unwrap()
                .to = end.id.clone();
            let mut reachable = std::collections::BTreeSet::new();
            let mut pending = bypass.record.entries.iter().map(|entry| entry.entry_block.clone()).collect::<Vec<_>>();
            while let Some(id) = pending.pop() {
                if reachable.insert(id.clone()) {
                    pending.extend(bypass.record.edges.iter().filter(|edge| edge.from == id).map(|edge| edge.to.clone()));
                }
            }
            for block in &mut bypass.record.blocks {
                block.reachable = reachable.contains(&block.id);
            }
            bypass.record.canonicalize();
            bypass.replace_machine_word(flow.address, branch);
            let error = check_bundle_values(
                &bypass.artifact,
                &bypass.metadata,
                &bypass.record,
                &bypass.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
            // The source-branch recognizer can reject the redirected successor
            // before the generic initialization-path coverage check reaches it.
            assert!(error.message.contains("bypass") || error.message.contains("control-flow target differs"), "{error}");
        }
    }
}

#[test]
fn allocated_values_owner_lifetimes_reject_rebound_stale_and_borrowed_handles() {
    use cellscript_artifact_checker::allocated_values::Operation as O;
    let source = "module lifetime;\ncollections Work { limits { bytes: 256, allocations: 4 },\nvalues: Vec<u64> { items: 8 } }\nfn count(view: &Work) -> u64 { return view.values.len(); }\naction inspect() -> u64 { let mut original = Work::new();\noriginal.values.push(7);\nlet moved = original;\nlet observed = count(&moved);\nmoved.drop();\nreturn observed; }";
    let reject = |mut fixture: Fixture, signature_rejection: bool| {
        fixture.rebind_layered_identities();
        let error = check_bundle_values(
            &fixture.artifact,
            &fixture.metadata,
            &fixture.record,
            &fixture.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
        let boundary = if signature_rejection { "typed move has an invalid shape or type" } else { "allocated owner lifetime" };
        assert!(
            error.to_string().contains(boundary)
                || error.message.contains("uses an unknown local or wrong type")
                || error.message.contains("typed move has an invalid shape or type"),
            "mutation missed {boundary} or the stricter one-layer reference type boundary: {error}"
        );
    };
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        for mutation in 0..6 {
            let mut changed = valid.clone();
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
            let original = entry.locals.iter().find(|local| local.name == "original").unwrap().id;
            let constructor = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::New, .. }))
                .unwrap()
                .destinations[0];
            let reference = entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .find(|op| matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref"))
                .unwrap()
                .destinations[0];
            let op = entry
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.operations)
                .find(|op| match mutation {
                    0 | 3 => matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Push, .. }),
                    1 | 4 => matches!(&op.detail, TypedSemanticOperationDetail::UnaryOperator { operator } if operator == "ref"),
                    _ => matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Drop, .. }),
                })
                .unwrap();
            match mutation {
                0 => op.operands[0].local = Some(constructor), // Constructor result has moved into original.
                1 | 2 => op.operands[0].local = Some(original), // Original has moved into moved.
                3 => {
                    // Retire original before the later move and read.
                    let TypedSemanticOperationDetail::AllocatedValue { operation, member, .. } = &mut op.detail else {
                        unreachable!()
                    };
                    *operation = O::Drop;
                    *member = None;
                    op.operands.truncate(1);
                }
                4 => {
                    // Same canonical ABI type does not grant a read-only call an owned argument.
                    op.opcode = "move".into();
                    op.detail = TypedSemanticOperationDetail::None;
                }
                _ => op.operands[0].local = Some(reference), // Ref temporaries carry Work in the ABI, but cannot drop it.
            }
            reject(changed, mutation == 4);
        }
        let evaluation = source
            .replace(
                "fn count(view: &Work) -> u64 { return view.values.len(); }",
                "fn count(view: &Work, n: u64) -> u64 { return view.values.len() + n; }",
            )
            .replace(
                "let observed = count(&moved);",
                "let mut other = Work::new();\nlet observed = count(&moved, { other.values.push(2);\n3 });",
            );
        let mut changed = Fixture::from_result(
            compile(
                &evaluation,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
        let moved = entry.locals.iter().find(|local| local.name == "moved").unwrap().id;
        let op = entry
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .filter(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Push, .. }))
            .nth(1)
            .unwrap();
        op.operands[0].local = Some(moved); // Invalidate the first argument while evaluating the second.
        reject(changed, false);
    }
}

#[test]
fn allocated_values_lifetimes_follow_joins_backedges_and_lexical_loans() {
    use cellscript_artifact_checker::allocated_values::Operation as O;
    let prefix = "module lifetime_cfg;\ncollections Work { limits { bytes: 256, allocations: 4 },\nvalues: Vec<u64> { items: 8 } }\n";
    let bodies = [
        "action inspect(flag: bool) -> u64 { let mut work = Work::new();\nif flag { work.values.reserve(1) } else { work.values.reserve(2) }\nreturn work.values.len(); }",
        "action inspect() -> u64 { let mut work = Work::new();\nfor i in 0..2 { work.values.reserve(i) }\nreturn work.values.len(); }",
        "action inspect() -> u64 { let work = Work::new();\nlet mut other = Work::new();\nborrow work as view { other.values.push(9);\nrequire view.values.len() == 0; }\nreturn 0; }",
        "action inspect() -> u64 { let work = Work::new();\nlet mut other = Work::new();\nborrow work as view { borrow view as inner { other.values.push(9);\nrequire inner.values.len() == 0; } }\nreturn 0; }",
        "action inspect(flag: bool) -> u64 { let mut work = Work::new();\nif flag { borrow work as view { require view.values.len() == 0; } }\nwork.values.push(1);\nreturn 0; }",
    ];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (case, body) in bodies.iter().enumerate() {
            let source = format!("{prefix}{body}");
            let mut changed = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
            );
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
            let work = entry.locals.iter().find(|local| local.name == "work").unwrap().id;
            let (block_id, op) = entry
                .blocks
                .iter_mut()
                .flat_map(|block| {
                    let id = block.id;
                    block.operations.iter_mut().map(move |op| (id, op))
                })
                .find(|(_, op)| {
                    matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { operation: O::Reserve | O::Push, .. })
                })
                .unwrap();
            if case < 2 {
                let TypedSemanticOperationDetail::AllocatedValue { operation, member, .. } = &mut op.detail else { unreachable!() };
                *operation = O::Drop;
                *member = None;
                op.operands.truncate(1);
            } else if case < 4 {
                op.operands[0].local = Some(work);
            } else {
                // Keep a lexical loan alive on only one predecessor. Joining
                // available handles by intersection must not discard the loan.
                let position = op.index + 1;
                entry.borrows[0].end_block = Some(block_id);
                entry.borrows[0].end_operation = Some(position);
            }
            changed.rebind_layered_identities();
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "case {case}/{diagnostics}: {error}");
            assert!(error.to_string().contains("allocated owner lifetime"), "case {case}/{diagnostics}: {error}");
        }
    }
}

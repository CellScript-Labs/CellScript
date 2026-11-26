#[test]
fn allocated_allocations_reject_rebound_budget_state_and_zeroing_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [64, 65536] {
            let source = format!("module allocation;\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 2 }},\nvalues: Vec<u64> {{ items: 4 }} }}\naction inspect() -> u64 {{ let mut first = Work::new();\nfirst.values.reserve(1);\nlet mut moved = first;\nmoved.values.push(7);\nlet mut second = Work::new();\nsecond.values.push(8);\nrequire moved.values.get(0) == 7;\nreturn second.values.get(0); }}");
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
                .filter(|block| block.allocated_allocation_labels.iter().any(|label| label.ends_with("_begin")))
                .min_by_key(|block| block.range.start)
                .unwrap();
            let label = begin.allocated_allocation_labels.iter().find(|label| label.ends_with("_begin")).unwrap();
            let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
            let end = valid.record.blocks.iter().find(|block| block.allocated_allocation_labels.contains(&end_label)).unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let body =
                elf.instructions.iter().filter(|i| begin.range.start <= i.address && i.address < end.range.start).collect::<Vec<_>>();
            let first_store = body.iter().find(|i| i.word & 0x7f == 0x23).unwrap().address;
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
            let mut mutations = 0;
            for instruction in &body {
                let word = instruction.word;
                let opcode = word & 0x7f;
                // Preserve branch addresses and CFG while reversing predicates.
                // Mutate the actual record/state/zero-byte writes, excluding
                // the spill subframe, and arithmetic outside guard wrappers.
                let replacement = if opcode == 0x63 {
                    Some(word ^ (1 << 12))
                } else if opcode == 0x23 && word >> 15 & 31 == 16 {
                    Some(word ^ (1 << 20))
                } else if instruction.address < first_store && matches!(opcode, 0x13 | 0x33 | 0x37 | 0x1b) && word >> 7 & 31 != 2 {
                    Some(0x00000013)
                } else if [0x020f8513, 0x00050293, 0x00128293, 0xfff38393].contains(&word) {
                    // Result pointer +32, zero-loop pointer, stride and count.
                    Some(word ^ (1 << 20))
                } else {
                    None
                };
                if let Some(replacement) = replacement {
                    let mut changed = valid.clone();
                    changed.replace_machine_word(instruction.address, replacement);
                    reject(&changed);
                    mutations += 1;
                }
            }
            assert!(mutations >= 40, "insufficient allocation mutation inventory: {mutations}");
            for all in [false, true] {
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_allocation_labels.retain(|candidate| !all && candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
            }
            // A marker must not manufacture a second alias of the same bytes.
            let mut duplicate = valid.clone();
            let duplicate_block = duplicate.record.blocks.iter_mut().find(|block| block.id == begin.id).unwrap();
            duplicate_block.allocated_allocation_labels.push(label.replace("_begin", "0_begin"));
            duplicate_block.allocated_allocation_labels.sort();
            duplicate.rebind_sidecars();
            reject(&duplicate);

            let loop_branch = body.iter().find(|i| i.word & 0x707f == 0x1063 && i.word >> 15 & 31 == 7).unwrap();
            let zero_loop = elf.control_flow.iter().find(|edge| edge.address == loop_branch.address).unwrap().target;
            let outside = elf
                .instructions
                .iter()
                .filter(|i| i.address < begin.range.start && i.word & 0x7f == 0x63)
                .max_by_key(|i| i.address)
                .unwrap()
                .address;
            for address in [begin.range.start, outside] {
                // Keep machine CFG, reachability and all digests consistent;
                // reject skipped admission and external entry into zeroing.
                let mut bypass = valid.clone();
                bypass.redirect_conditional_branch(address, zero_loop);
                reject(&bypass);
            }
        }
    }
}

#[test]
fn allocated_lifecycle_rejects_rebound_lookup_copy_and_retirement_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module lifecycle;\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }},\nvalues: Vec<u64> {{ items: 8 }},\nseen: OrderedSet<u64> {{ items: 8 }},\ntotals: OrderedMap<u64, u128> {{ items: 8 }} }}\naction inspect() -> u64 {{ let mut first = Work::new();\nfirst.values.push(7);\nlet mut second = Work::new();\nsecond.values.push(8);\nsecond.values.push(9);\nsecond.values.reserve(8);\nrequire second.values.get(0) == 8;\nrequire second.values.get(1) == 9;\nsecond.drop();\nfirst.drop();\nreturn 0; }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
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
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            for prefix in [".Lallocated_resize_", ".Lallocated_release_"] {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let largest_offset = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let exact_prefix = format!("{prefix}{largest_offset}_");
                let (begin, label) = candidates
                    .into_iter()
                    .filter(|(_, label)| label.starts_with(&exact_prefix))
                    .min_by_key(|(block, _)| block.range.start)
                    .unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid.record.blocks.iter().find(|block| block.allocated_lifecycle_labels.contains(&end_label)).unwrap();
                let allocations = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start <= block.range.start && block.range.start < end.range.start)
                    .flat_map(|block| {
                        block
                            .allocated_allocation_labels
                            .iter()
                            .filter(|label| label.ends_with("_begin"))
                            .map(move |label| (block, label))
                    })
                    .map(|(block, label)| {
                        let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_allocation_labels.contains(&finish))
                            .unwrap()
                            .range
                            .start;
                        (block.range.start, end)
                    })
                    .collect::<Vec<_>>();
                assert_eq!(allocations.len(), if prefix.contains("resize") { 2 } else { 0 });
                if bytes == 65536 {
                    assert!(largest_offset > 65536);
                }
                let body = elf
                    .instructions
                    .iter()
                    .filter(|i| {
                        begin.range.start <= i.address
                            && i.address < end.range.start
                            && !allocations.iter().any(|(start, end)| (*start..*end).contains(&i.address))
                    })
                    .collect::<Vec<_>>();
                let mut mutations = 0;
                for instruction in &body {
                    let word = instruction.word;
                    let opcode = word & 0x7f;
                    let base = word >> 15 & 31;
                    let source = word >> 20 & 31;
                    let replacement = match opcode {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 if base != 2 => Some(word ^ (8 << 20)),
                        0x23 if base != 2 || matches!(source, 6 | 10) => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 if word >> 7 & 31 != 2 => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 45, "insufficient lifecycle mutations: {mutations}");
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                if let Some(&(child, _)) = allocations.first() {
                    let outside = elf
                        .instructions
                        .iter()
                        .filter(|i| i.address < begin.range.start && i.word & 0x7f == 0x63)
                        .max_by_key(|i| i.address)
                        .unwrap()
                        .address;
                    for address in [begin.range.start, outside] {
                        let mut bypass = valid.clone();
                        bypass.redirect_conditional_branch(address, child);
                        reject(&bypass);
                    }
                }
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_lifecycle_labels.clear();
            }
            missing.rebind_sidecars();
            reject(&missing);
        }
    }
}

#[test]
fn allocated_descriptors_bind_member_growth_publication_and_drop() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module descriptors;\nstruct Record {{ amount: u128, flag: u64 }}\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 8 }},\nvalues: Vec<Record> {{ items: 4 }},\ntotals: OrderedMap<u64, Record> {{ items: 6 }},\nseen: OrderedSet<Hash> {{ items: 5 }} }}\naction inspect(witness flag: bool) -> u64 {{ if flag {{ return 0; }}\nlet mut first = Work::new();\nlet mut second = Work::new();\nsecond.values.reserve(4);\nsecond.totals.reserve(6);\nsecond.seen.reserve(5);\nsecond.drop();\nfirst.drop();\nreturn 0; }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
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
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            for prefix in [".Lallocated_reserve_", ".Lallocated_drop_"] {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let owner = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let descriptor = owner + 32 + bytes + 32; // The second, differently shaped member.
                let (begin, label) =
                    candidates.into_iter().find(|(_, label)| label.starts_with(&format!("{prefix}{owner}_{descriptor}_"))).unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                let child_prefix = if prefix.contains("reserve") { ".Lallocated_resize_" } else { ".Lallocated_release_" };
                let (child, child_label) = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start < block.range.start && block.range.start < end)
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .find(|(_, label)| label.starts_with(child_prefix) && label.ends_with("_begin"))
                    .unwrap();
                let child_end_label = format!("{}_end", child_label.strip_suffix("_begin").unwrap());
                let child_end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&child_end_label))
                    .unwrap()
                    .range
                    .start;
                let mut mutations = 0;
                for instruction in elf.instructions.iter().filter(|i| {
                    begin.range.start <= i.address && i.address < end && !(child.range.start..child_end).contains(&i.address)
                }) {
                    let word = instruction.word;
                    let replacement = match word & 0x7f {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 => Some(word ^ (8 << 20)),
                        0x23 => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 60, "descriptor checks and publication must both be exercised: {mutations}");
                if bytes == 65536 {
                    assert!(owner > 65536);
                }
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                // Swap two valid member coordinates without changing the
                // inventory counts or machine bytes. Equal entry widths must
                // not erase their distinct declared item limits.
                let mut swapped = valid.clone();
                let left = format!("{prefix}{owner}_{descriptor}_");
                let right = format!("{prefix}{owner}_{}_", descriptor + 32);
                for block in &mut swapped.record.blocks {
                    for label in &mut block.allocated_lifecycle_labels {
                        if let Some(suffix) = label.strip_prefix(&left) {
                            *label = format!("{right}{suffix}");
                        } else if let Some(suffix) = label.strip_prefix(&right) {
                            *label = format!("{left}{suffix}");
                        }
                    }
                    block.allocated_lifecycle_labels.sort();
                }
                swapped.rebind_sidecars();
                reject(&swapped);
                let branch = elf
                    .instructions
                    .iter()
                    .find(|i| begin.range.start <= i.address && i.address < child.range.start && i.word & 0x7f == 0x63)
                    .unwrap();
                let mut bypass = valid.clone();
                bypass.redirect_conditional_branch(branch.address, child.range.start);
                reject(&bypass);
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block
                    .allocated_lifecycle_labels
                    .retain(|label| !label.starts_with(".Lallocated_reserve_") && !label.starts_with(".Lallocated_drop_"));
            }
            missing.rebind_sidecars();
            reject(&missing);
            let tail = valid.record.blocks.iter().find(|block| block.machine_label.as_deref() == Some(".Linspect_epilogue")).unwrap();
            let restore = elf.instructions.iter().find(|i| tail.range.contains(i.address) && i.word & 0x7fff == 0x3083).unwrap();
            let mut skipped_restore = valid.clone();
            skipped_restore.replace_machine_word(restore.address, restore.word ^ (1 << 7));
            assert_code(&skipped_restore, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn allocated_elements_bind_copy_direction_extent_and_length_transitions() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!("module elements;\nstruct Item {{ amount: u128, tag: u64 }}\ncollections Work {{ limits {{ bytes: {bytes}, allocations: 16 }},\nvalues: Vec<Item> {{ items: 4 }},\ntotals: OrderedMap<u64, u128> {{ items: 4 }},\nseen: OrderedSet<u64> {{ items: 4 }} }}\naction inspect(witness flag: bool) -> u64 {{ if flag {{ return 0; }}\nlet mut first = Work::new();\nlet mut second = Work::new();\nsecond.values.push(new Item {{ amount: 3, tag: 4 }});\nsecond.values.insert(0, new Item {{ amount: 5, tag: 6 }});\nsecond.values.set(1, new Item {{ amount: 7, tag: 8 }});\nlet removed = second.values.remove(0);\nrequire removed.tag == 6;\nlet popped = second.values.pop();\nrequire popped.tag == 8;\nsecond.totals.insert(1, 2);\nsecond.totals.insert(1, 3);\nsecond.totals.remove(1);\nsecond.seen.insert(1);\nsecond.seen.remove(1);\nsecond.drop();\nfirst.drop();\nreturn 0; }}");
            let valid = Fixture::from_result(
                compile(
                    &source,
                    CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                )
                .unwrap(),
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
                assert!(error.message.contains("allocated allocation"), "{error}");
            };
            let prefixes = [".Lallocated_push_", ".Lallocated_insert_", ".Lallocated_set_", ".Lallocated_remove_"];
            for prefix in prefixes {
                let candidates = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .filter(|(_, label)| label.starts_with(prefix) && label.ends_with("_begin"))
                    .collect::<Vec<_>>();
                let owner = candidates
                    .iter()
                    .map(|(_, label)| label.strip_prefix(prefix).unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
                    .max()
                    .unwrap();
                let descriptor = owner + 32 + bytes;
                let (begin, label) = candidates
                    .into_iter()
                    .filter(|(_, label)| label.starts_with(&format!("{prefix}{owner}_{descriptor}_")))
                    .min_by_key(|(block, _)| block.range.start)
                    .unwrap();
                let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                    .unwrap()
                    .range
                    .start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| {
                        begin.range.start <= block.range.start
                            && block.range.start < end
                            && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lvalue_vector_checked_"))
                    })
                    .map(|block| block.range.start)
                    .min()
                    .unwrap();
                let child = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| begin.range.start < block.range.start && block.range.start < end)
                    .flat_map(|block| block.allocated_lifecycle_labels.iter().map(move |label| (block, label)))
                    .find(|(_, label)| label.starts_with(".Lallocated_reserve_") && label.ends_with("_begin"))
                    .map(|(block, label)| {
                        let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_lifecycle_labels.contains(&finish))
                            .unwrap()
                            .range
                            .start;
                        block.range.start..end
                    });
                let mut mutations = 0;
                for instruction in elf.instructions.iter().filter(|i| {
                    body <= i.address && i.address < end && !child.as_ref().is_some_and(|child| child.contains(&i.address))
                }) {
                    let word = instruction.word;
                    let rd = word >> 7 & 31;
                    let replacement = match word & 0x7f {
                        0x63 => Some(word ^ (1 << 12)),
                        0x03 if word >> 12 & 7 == 4 => Some(word ^ (1 << 15)),
                        0x23 if word >> 12 & 7 == 0 || word >> 20 & 31 == 6 => Some(word ^ (1 << 20)),
                        0x13 | 0x1b | 0x33 if matches!(rd, 5 | 6 | 7 | 11 | 14 | 28 | 29) => Some(word ^ (1 << 20)),
                        0x37 => Some(word ^ (1 << 12)),
                        _ => None,
                    };
                    if let Some(replacement) = replacement {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(instruction.address, replacement);
                        reject(&changed);
                        mutations += 1;
                    }
                }
                assert!(mutations >= 10, "missing element mutation coverage: {prefix} {mutations}");
                if bytes == 65536 {
                    assert!(owner > 65536);
                }
                let mut missing = valid.clone();
                for block in &mut missing.record.blocks {
                    block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                }
                missing.rebind_sidecars();
                reject(&missing);
                if let Some(child) = child {
                    let branch = elf
                        .instructions
                        .iter()
                        .find(|i| begin.range.start <= i.address && i.address < child.start && i.word & 0x7f == 0x63)
                        .unwrap();
                    let mut bypass = valid.clone();
                    bypass.redirect_conditional_branch(branch.address, child.start);
                    reject(&bypass);
                }
            }
            let mut missing = valid.clone();
            for block in &mut missing.record.blocks {
                block.allocated_lifecycle_labels.retain(|label| !prefixes.iter().any(|prefix| label.starts_with(prefix)));
            }
            missing.rebind_sidecars();
            reject(&missing);
        }
    }
}

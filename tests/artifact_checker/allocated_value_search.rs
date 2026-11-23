#[test]
fn allocated_ordered_searches_bind_key_order_bounds_and_update_selection() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for bytes in [512, 65536] {
            let source = format!(
                r#"module ordered_proof;
collections Work {{
    limits {{ bytes: {bytes}, allocations: 16 }},
    totals: OrderedMap<u64, u128> {{ items: 4 }},
    seen: OrderedSet<Hash> {{ items: 4 }}
}}
fn lookup(view: &Work, key: u64) -> u128 {{
    borrow view as alias {{ return alias.totals.get_or(key, 0); }}
}}
action inspect(flag: bool, hash: Hash) -> u64 {{
    if flag {{ return 0; }}
    let first = Work::new();
    let mut second = Work::new();
    let added = second.totals.insert(256, 3);
    require added;
    let replaced = second.totals.insert(256, 5);
    require !replaced;
    require second.totals.contains_key(256);
    require second.totals.get(256) == 5;
    require second.totals.get_or(255, 8) == 8;
    borrow second as view {{
        require view.totals.contains_key(256);
        require lookup(view, 256) == 5;
    }}
    let removed = second.totals.remove(256);
    require removed;
    let missing = second.totals.remove(256);
    require !missing;
    let fresh = second.seen.insert(hash);
    require fresh;
    let duplicate = second.seen.insert(hash);
    require !duplicate;
    require second.seen.contains(hash);
    let erased = second.seen.remove(hash);
    require erased;
    let absent = second.seen.remove(hash);
    require !absent;
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
            let prefixes = [".Lallocated_find_", ".Lallocated_put_", ".Lallocated_erase_"];
            // Both owners have the same contract. Rebinding the lexical loan
            // to the other live owner must not certify reads from the old one.
            let mut wrong_owner = valid.clone();
            let entry = wrong_owner.record.typed_semantics.entries.iter_mut().find(|entry| entry.kind == "action").unwrap();
            entry.borrows.iter_mut().find(|borrow| borrow.root == "second").unwrap().root = "first".into();
            wrong_owner.rebind_layered_identities();
            reject(&wrong_owner);
            for prefix in prefixes {
                let mut first_per_member = std::collections::BTreeMap::new();
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        if let Some(suffix) = label.strip_prefix(prefix).filter(|_| label.ends_with("_begin")) {
                            let parts = suffix.split('_').collect::<Vec<_>>();
                            let descriptor = parts[1].parse::<u32>().unwrap();
                            first_per_member.entry(descriptor).or_insert((block, label));
                        }
                    }
                }
                assert_eq!(first_per_member.len(), 2);
                for (descriptor, (begin, label)) in first_per_member {
                    if bytes == 65536 {
                        assert!(descriptor > 131072);
                    }
                    let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                    let end = valid
                        .record
                        .blocks
                        .iter()
                        .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                        .unwrap()
                        .range
                        .start;
                    let body = if prefix == ".Lallocated_find_" {
                        valid
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
                            .unwrap()
                    } else {
                        begin.range.start
                    };
                    let mut children = Vec::new();
                    if prefix != ".Lallocated_find_" {
                        for block in &valid.record.blocks {
                            if !(begin.range.start..end).contains(&block.range.start) {
                                continue;
                            }
                            for label in &block.allocated_lifecycle_labels {
                                if [".Lallocated_find_", ".Lallocated_set_", ".Lallocated_insert_", ".Lallocated_remove_"]
                                    .iter()
                                    .any(|prefix| label.starts_with(prefix))
                                    && label.ends_with("_begin")
                                {
                                    let finish = format!("{}_end", label.strip_suffix("_begin").unwrap());
                                    let child_end = valid
                                        .record
                                        .blocks
                                        .iter()
                                        .find(|block| block.allocated_lifecycle_labels.contains(&finish))
                                        .unwrap()
                                        .range
                                        .start;
                                    children.push(block.range.start..child_end);
                                }
                            }
                        }
                    }
                    let mut mutations = 0;
                    for instruction in elf
                        .instructions
                        .iter()
                        .filter(|i| body <= i.address && i.address < end && !children.iter().any(|range| range.contains(&i.address)))
                    {
                        let word = instruction.word;
                        let replacement = match word & 0x7f {
                            0x63 => Some(word ^ (1 << 12)),
                            0x03 => Some(word ^ (1 << 15)),
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
                    assert!(mutations >= if prefix == ".Lallocated_find_" { 25 } else { 2 });
                    let mut missing = valid.clone();
                    for block in &mut missing.record.blocks {
                        block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                    }
                    missing.rebind_sidecars();
                    reject(&missing);
                    if prefix != ".Lallocated_find_" {
                        let find = children.iter().min_by_key(|range| range.start).unwrap();
                        let branch = elf.instructions.iter().find(|i| i.address == find.end).unwrap();
                        let update = children.iter().filter(|range| range.start > find.end).min_by_key(|range| range.start).unwrap();
                        let mut bypass = valid.clone();
                        let interior = valid
                            .record
                            .blocks
                            .iter()
                            .filter(|block| update.start < block.range.start && block.range.start < update.end)
                            .map(|block| block.range.start)
                            .min()
                            .unwrap();
                        bypass.redirect_conditional_branch(branch.address, interior);
                        reject(&bypass);
                    }
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

#[test]
fn allocated_ordered_search_inventory_accepts_all_key_profiles_and_local_borrows() {
    for key in ["u8", "u16", "u32", "u64", "Hash", "Address", "[u8; 1]", "[u8; 8]", "[u8; 32]"] {
        for member in [format!("OrderedMap<{key}, u64>"), format!("OrderedSet<{key}>")] {
            let args = if member.starts_with("OrderedMap") { "key, 7" } else { "key" };
            let contains = if member.starts_with("OrderedMap") { "contains_key" } else { "contains" };
            let source = format!(
                r#"module key_profile;
collections Work {{
    limits {{ bytes: 512, allocations: 4 }},
    values: {member} {{ items: 4 }}
}}
action inspect(key: {key}) -> u64 {{
    let mut work = Work::new();
    let added = work.values.insert({args});
    require added;
    borrow work as view {{ borrow view as nested {{ require nested.values.{contains}(key); }} }}
    let removed = work.values.remove(key);
    require removed;
    work.drop();
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
                let starts = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| &block.allocated_lifecycle_labels)
                    .filter(|label| label.starts_with(".Lallocated_find_") && label.ends_with("_begin"))
                    .count();
                assert_eq!(starts, 3, "{member} at {diagnostics}");
            }
        }
    }
}

#[test]
fn allocated_helper_parameters_bind_both_abi_words_and_preserve_slots() {
    for padding in [0, 7, 8] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let forwarded = (0..padding).map(|i| format!("p{i}, ")).collect::<String>();
        let arguments = "0, ".repeat(padding);
        let source = format!(
            r#"module arena_parameters;
collections Work {{
    limits {{ bytes: 512, allocations: 4 }},
    values: Vec<u64> {{ items: 4 }}
}}
fn read({params}first: &Work, second: &Work, delta: u64) -> u64 {{
    let adjusted = delta + 1;
    if (adjusted <= delta) {{ return 0; }}
    return first.values.get(0) + second.values.get(0) + adjusted;
}}
fn forward({params}first: &Work, second: &Work, delta: u64) -> u64 {{
    borrow first as alias {{ borrow alias as nested {{
        return read({forwarded}nested, second, delta);
    }} }}
}}
action inspect(witness delta: u64) -> u64 {{
    let mut first = Work::new();
    let mut second = Work::new();
    first.values.push(3);
    second.values.push(7);
    borrow first as a {{ borrow second as b {{
        require (forward({arguments}a, b, delta) > 0);
    }} }}
    second.drop();
    first.drop();
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
                for param in &typed.params[padding..padding + 2] {
                    let pointer = typed.locals.iter().find(|local| local.id == param.binding_id).unwrap().source_id as u32 * 8;
                    let index = spills.iter().position(|instruction| offset(instruction.word) == pointer).unwrap();
                    let length = offset(spills[index + 1].word);
                    assert!(length > pointer && length < 2040);
                    for spill in [spills[index], spills[index + 1]] {
                        let mut changed = valid.clone();
                        changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    }
                    if name == "read" {
                        let binary = typed.blocks.iter().flat_map(|block| &block.operations).find(|op| op.opcode == "binary").unwrap();
                        let result =
                            typed.locals.iter().find(|local| local.id == binary.destinations[0]).unwrap().source_id as u32 * 8;
                        let store = elf
                            .instructions
                            .iter()
                            .find(|instruction| {
                                instruction.address >= end
                                    && instruction.word & 0x707f == 0x3023
                                    && instruction.word >> 15 & 31 == 2
                                    && offset(instruction.word) == result
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == typed.id && block.range.contains(instruction.address))
                            })
                            .unwrap();
                        // Full stores and partial final-byte overlaps must not
                        // replace either incoming word in the helper body.
                        for slot in [pointer, length] {
                            for width in 0..=3 {
                                for destination in [slot, slot + 7] {
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
fn allocated_borrowed_reads_bind_descriptors_searches_and_copied_results() {
    let prefix = ".Lallocated_borrowed_";
    for padding in [0, 7, 8] {
        for bytes in [512, 65536] {
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(
                        &borrowed_collection_reads::source(padding, bytes),
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
                let mut kinds = std::collections::BTreeSet::new();
                let mut mutations = 0;
                for block in &valid.record.blocks {
                    for label in &block.allocated_lifecycle_labels {
                        let Some(suffix) = label.strip_prefix(prefix).filter(|suffix| suffix.ends_with("_begin")) else { continue };
                        let kind = suffix.split('_').next().unwrap();
                        if !kinds.insert(kind) {
                            continue;
                        }
                        let end_label = format!("{}_end", label.strip_suffix("_begin").unwrap());
                        let end = valid
                            .record
                            .blocks
                            .iter()
                            .find(|block| block.allocated_lifecycle_labels.contains(&end_label))
                            .unwrap()
                            .range
                            .start;
                        let body =
                            elf.instructions.iter().filter(|i| (block.range.start..end).contains(&i.address)).collect::<Vec<_>>();
                        for opcode in [0x03, 0x13, 0x1b, 0x33, 0x37, 0x23, 0x63] {
                            let candidates = body
                                .iter()
                                .filter(|i| i.word & 0x7f == opcode && (opcode == 0x23 || i.word >> 7 & 31 != 2))
                                .collect::<Vec<_>>();
                            for instruction in candidates.first().into_iter().chain(candidates.last()).copied() {
                                let word = instruction.word;
                                let replacement = word
                                    ^ match opcode {
                                        0x37 | 0x63 => 1 << 12,
                                        0x03 => 8 << 20,
                                        0x13 if word >> 12 & 7 == 3 => 1 << 15,
                                        _ => 1 << 20,
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
                                    "{kind} at {:#x}: {word:#x} -> {replacement:#x}: {error}",
                                    instruction.address
                                );
                                mutations += 1;
                            }
                        }
                        let mut missing = valid.clone();
                        for block in &mut missing.record.blocks {
                            block.allocated_lifecycle_labels.retain(|candidate| candidate != label && candidate != &end_label);
                        }
                        missing.rebind_sidecars();
                        assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        if kind == "read" {
                            // Rebind the CFG as well: the extent check cannot
                            // be redirected into an already checked read body.
                            let branch = elf
                                .instructions
                                .iter()
                                .rev()
                                .find(|i| {
                                    i.address < block.range.start
                                        && block.range.start - i.address < 4096
                                        && i.word & 0x7f == 0x63
                                        && valid
                                            .record
                                            .blocks
                                            .iter()
                                            .any(|b| b.owner_entry == block.owner_entry && b.range.contains(i.address))
                                })
                                .unwrap();
                            let target = valid
                                .record
                                .blocks
                                .iter()
                                .find(|b| b.range.start > block.range.start && b.range.start < end)
                                .unwrap()
                                .range
                                .start;
                            let mut changed = valid.clone();
                            changed.redirect_conditional_branch(branch.address, target);
                            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                        }
                    }
                }
                assert_eq!(
                    kinds,
                    std::collections::BTreeSet::from(["extent", "len", "capacity", "empty", "find", "read", "lookup", "required"])
                );
                assert!(mutations > 55);
                let mut erased = valid.clone();
                for block in &mut erased.record.blocks {
                    block.allocated_lifecycle_labels.retain(|label| !label.starts_with(prefix));
                }
                erased.rebind_sidecars();
                assert_code(&erased, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}

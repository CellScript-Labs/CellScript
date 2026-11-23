#[test]
fn nested_cell_borrow_calls_bind_register_stack_and_forwarded_views() {
    for count in [0, 1, 6, 7, 8, 9] {
        let params = (0..count).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let arguments = "0, ".repeat(count);
        let forwarded = (0..count).map(|i| format!("p{i}, ")).collect::<String>();
        let source = format!("{}\nfn count({params}document: &Document) -> u64 {{ return document.entries.len(); }}\nfn wrap({params}document: &Document) -> u64 {{ return count({forwarded}document); }}\n",
            NESTED_CELL_CODEC_SOURCE.replace("require values.len() == 1;", &format!("require wrap({arguments}&before) == 1;\nrequire count({arguments}&before) == 1;")));
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            if count > 0 {
                let mut changed = valid.clone();
                let helper = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "wrap").unwrap();
                let scalar = helper.locals.iter().find(|local| local.id == helper.params[0].binding_id).unwrap().source_id;
                let borrowed = helper.params.last().unwrap().binding_id;
                helper.locals.iter_mut().find(|local| local.id == borrowed).unwrap().source_id = scalar;
                changed.rebind_layered_identities();
                let error = check_bundle_values(
                    &changed.artifact,
                    &changed.metadata,
                    &changed.record,
                    &changed.source_map,
                    &CheckerBudgets::default(),
                )
                .unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.to_string().contains("borrowed Cell parameter aliases"), "{error}");
            }
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let helpers =
                valid.record.entries.iter().filter(|entry| ["count", "wrap"].contains(&entry.name.as_str())).collect::<Vec<_>>();
            let mut mutations = std::collections::BTreeSet::new();
            for helper in helpers {
                let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
                for flow in elf.control_flow.iter().filter(|flow| flow.target == start) {
                    let owner =
                        valid.record.blocks.iter().find(|block| block.range.contains(flow.address)).unwrap().owner_entry.as_str();
                    // The latest ABI-register definitions include optimized
                    // register moves. Stack arguments share t0, so inspect both.
                    for register in if count < 7 {
                        vec![10 + count, 11 + count]
                    } else if count == 7 {
                        vec![17, 5]
                    } else {
                        vec![5]
                    } {
                        let loads = elf
                            .instructions
                            .iter()
                            .rev()
                            .filter(|i| {
                                i.address < flow.address
                                    && matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x37 | 0x1b | 0x3b)
                                    && i.word >> 7 & 31 == register as u32
                                    && valid
                                        .record
                                        .blocks
                                        .iter()
                                        .any(|block| block.owner_entry == owner && block.range.contains(i.address))
                            })
                            .take(if register == 5 && count >= 8 { 2 } else { 1 })
                            .collect::<Vec<_>>();
                        assert!(!loads.is_empty(), "caller {diagnostics}, {count} args");
                        for load in loads {
                            mutations.insert((load.address, ((register as u32) << 7) | 0x13));
                        }
                    }
                }
                let first_use = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| {
                        block.owner_entry == helper.id && (!block.nested_read_labels.is_empty() || block.lowering_block_id.is_some())
                    })
                    .map(|block| block.range.start)
                    .filter(|address| *address > start)
                    .min()
                    .unwrap();
                for i in elf.instructions.iter().filter(|i| i.address >= start && i.address < first_use) {
                    let word = i.word;
                    let register = word >> 20 & 31;
                    if word & 0x707f == 0x3023
                        && ((count < 8 && register == 10 + count as u32) || (count < 7 && register == 11 + count as u32))
                    {
                        mutations.insert((i.address, word & !(31 << 20)));
                    } else if word & 0x707f == 0x3003
                        && word >> 15 & 31 == 8
                        && [count, count + 1]
                            .into_iter()
                            .filter(|index| *index >= 8)
                            .any(|index| ((word as i32) >> 20) == ((index - 8) * 8) as i32)
                    {
                        mutations.insert((i.address, word ^ (8 << 20)));
                    }
                }
            }
            assert!(mutations.len() >= 6, "{diagnostics}, {count} args: {}", mutations.len());
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
                assert_eq!(
                    error.code,
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    "{diagnostics}, {count} args, {address:#x}: {error}"
                );
            }
        }
    }
}

#[test]
fn nested_cell_lifetime_rejects_buffer_pointer_and_size_clobbers() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                NESTED_CELL_CODEC_SOURCE,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let projection = valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.nested_read_labels.iter().any(|label| label.starts_with(".Lnested_use_cellfield_") && !label.ends_with("_done"))
            })
            .unwrap();
        let codec = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| &block.nested_cell_labels)
            .filter(|label| !label.ends_with("_done") && !label.ends_with("_validate"))
            .min_by_key(|label| label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').next().unwrap().parse::<u32>().unwrap())
            .unwrap();
        let coordinates = codec
            .strip_prefix(".Lnested_cell_codec_")
            .unwrap()
            .split('_')
            .map(|value| value.parse::<i32>().unwrap())
            .collect::<Vec<_>>();
        let [root, size, buffer, _] = coordinates.as_slice() else { unreachable!() };
        let ranges = valid
            .record
            .blocks
            .iter()
            .flat_map(|block| {
                block.nested_read_labels.iter().filter(|label| !label.ends_with("_done")).map(|label| {
                    let done = format!("{label}_done");
                    (
                        block.range.start,
                        valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start,
                    )
                })
            })
            .collect::<Vec<_>>();
        let store = elf
            .instructions
            .iter()
            .find(|i| {
                i.address > projection.range.start
                    && valid
                        .record
                        .blocks
                        .iter()
                        .any(|block| block.owner_entry == projection.owner_entry && block.range.contains(i.address))
                    && !ranges.iter().any(|(start, end)| *start <= i.address && i.address < *end)
                    && i.word & 0x707f == 0x3023
                    && i.word >> 15 & 31 == 2
            })
            .unwrap();
        // The first ordinary scalar store after projection is redirected to
        // each protected region, including one-byte corruption at its edge.
        for offset in [root * 8, *size, *buffer, buffer + 511] {
            for kind in [0, 3] {
                assert!((0..2048).contains(&offset));
                let word = (store.word & !0xfe007f80) | ((offset as u32 & 0xfe0) << 20) | ((offset as u32 & 31) << 7) | (kind << 12);
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
                assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
                assert!(error.to_string().contains("unverified write overlaps"), "{error}");
            }
        }
    }
}

#[test]
fn nested_cell_borrow_rejects_another_valid_cell_with_the_same_layout() {
    let source = format!(
        "{}\nfn count(document: &Document) -> u64 {{ return document.entries.len(); }}\n",
        NESTED_CELL_CODEC_SOURCE
            .replace("input before: Document", "input before: Document, input peer: Document")
            .replace("require values.len() == 1;", "require count(&before) == 1;")
            .replace("consume before", "consume peer;\nconsume before")
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.name == "roll").unwrap();
        let peer = typed.params.iter().find(|param| param.name == "peer").unwrap();
        let peer_id = typed.locals.iter().find(|local| local.id == peer.binding_id).unwrap().source_id;
        let label = valid
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == typed.id)
            .flat_map(|block| &block.nested_cell_labels)
            .find(|label| {
                label.starts_with(&format!(".Lnested_cell_codec_{peer_id}_"))
                    && !label.ends_with("_done")
                    && !label.ends_with("_validate")
            })
            .unwrap();
        let size = label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').nth(1).unwrap().parse::<i32>().unwrap();
        let helper = valid.record.entries.iter().find(|entry| entry.name == "count").unwrap();
        let start = valid.record.blocks.iter().find(|block| block.id == helper.entry_block).unwrap().range.start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let call = elf.control_flow.iter().find(|flow| flow.target == start).unwrap();
        let mut changes = Vec::new();
        for (register, offset) in [(10, (peer_id * 8) as i32), (11, size)] {
            let load = elf
                .instructions
                .iter()
                .rev()
                .find(|i| {
                    i.address < call.address
                        && matches!(i.word & 0x7f, 0x03 | 0x13 | 0x33 | 0x37 | 0x1b | 0x3b)
                        && i.word >> 7 & 31 == register
                })
                .unwrap();
            assert!((-2048..2048).contains(&offset));
            changes.push((load.address, ((offset as u32 & 0xfff) << 20) | (2 << 15) | (3 << 12) | (register << 7) | 3));
        }
        // Both buffers have already passed their own complete codec. Swapping
        // pointer AND length must still reject the wrong declared Cell source.
        let mut changed = valid.clone();
        changed.replace_machine_words(&changes);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.to_string().contains("do not match their typed source"), "{error}");
    }
}

#[test]
fn nested_cell_field_views_bind_spans_lengths_and_load_initialization() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &nested_cell_two_fields_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = std::collections::BTreeSet::new();
        let mut projections = 0;
        for block in &valid.record.blocks {
            for label in &block.nested_read_labels {
                if !label.starts_with(".Lnested_use_cellfield_") || label.ends_with("_done") {
                    continue;
                }
                projections += 1;
                let end = valid
                    .record
                    .blocks
                    .iter()
                    .find(|block| block.nested_read_labels.contains(&format!("{label}_done")))
                    .unwrap()
                    .range
                    .start;
                for instruction in elf.instructions.iter().filter(|i| i.address >= block.range.start && i.address < end) {
                    let word = instruction.word;
                    let replacement = match word & 0x7f {
                        3 => Some(word ^ (1 << 23)),    // Different byte/word displacement.
                        0x23 => Some(word ^ (1 << 10)), // Different scalar or size slot.
                        0x63 | 0x33 => Some(word ^ (1 << 15)),
                        _ => None,
                    };
                    if let Some(word) = replacement {
                        mutations.insert((instruction.address, word));
                    }
                }
            }
        }
        assert_eq!(projections, 2);
        let done = valid
            .record
            .blocks
            .iter()
            .find(|block| block.nested_cell_labels.iter().any(|label| label.ends_with("_done")))
            .unwrap()
            .range
            .start;
        let status = elf.instructions.iter().find(|i| i.address == done).unwrap();
        mutations.insert((done, status.word ^ (1 << 12)));
        let initialization = elf.instructions.iter().find(|i| i.address > done && i.word & 0x7f == 0x23).unwrap();
        mutations.insert((initialization.address, initialization.word ^ (1 << 10)));
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
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{diagnostics} at {address:#x}: {error}");
        }
    }
}

#[test]
fn nested_cell_field_order_is_explicit_and_authenticated() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &nested_cell_two_fields_source(),
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let definition = valid.record.typed_semantics.types.iter().find(|ty| ty.name == "Document").unwrap();
        for (name, index) in [("id", 0), ("entries", 1), ("spare", 2)] {
            assert_eq!(definition.fields.iter().find(|field| field.name == name).unwrap().molecule_field_index, Some(index));
        }
        for mutation in 0..3 {
            let mut changed = valid.clone();
            let ty = changed.record.typed_semantics.types.iter_mut().find(|ty| ty.name == "Document").unwrap();
            for field in &mut ty.fields {
                field.molecule_field_index = match mutation {
                    0 => None,
                    1 => Some(0),
                    _ => field.molecule_field_index.map(|index| if index == 0 { 0 } else { 3 - index }),
                };
            }
            ty.layout_hash = canonical_hash(
                "cellscript-typed-layout-v2",
                &(
                    ty.kind.as_str(),
                    ty.encoded_size,
                    &ty.fields,
                    ty.tag_width_bytes,
                    &ty.variants,
                    &ty.capabilities,
                    &ty.identity_policy,
                ),
            )
            .unwrap();
            let layout_hash = ty.layout_hash.clone();
            // Rebind schema-dependent roles and their source-map identities as
            // well; swapped field indices must reach the machine proof.
            for role in changed.record.typed_semantics.foundation.roles.iter_mut().filter(|role| role.ty == "Document") {
                let old_node = role.semantic_node_id.clone();
                role.schema_identity.clone_from(&layout_hash);
                role.semantic_node_id = canonical_hash(
                    "cellscript-semantic-node-role-v1",
                    &(
                        role.role_id.as_str(),
                        role.entry_id.as_str(),
                        role.binding.as_str(),
                        role.ty.as_str(),
                        role.direction.as_str(),
                        role.source.as_str(),
                        role.selector.as_str(),
                        role.cardinality.as_str(),
                        role.lock_or_type_role.as_str(),
                        role.script_identity_policy.as_str(),
                        role.schema_identity.as_str(),
                        role.correspondence_policy.as_str(),
                    ),
                )
                .unwrap();
                for mapping in &mut changed.source_map.semantic_mappings {
                    if mapping.semantic_node_id == old_node {
                        mapping.semantic_node_id.clone_from(&role.semantic_node_id);
                    }
                }
            }
            changed.source_map.canonicalize();
            changed.rebind_layered_identities();
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
                if mutation < 2 {
                    CheckerRejectionCode::V2419TypedSemanticsInvalid
                } else {
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid
                },
                "{error}"
            );
        }
    }
}

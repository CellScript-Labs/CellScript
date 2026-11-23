#[test]
fn nested_data_view_buffers_cannot_alias_after_coherent_machine_and_marker_rebinding() {
    let source = include_str!("../syntax_combo/seeds/named-nested-data-views.cell")
        .split("fn total")
        .next()
        .unwrap()
        .replace("encoded_bytes: 4096", "encoded_bytes: 128");
    let source = format!("{source}\naction verify() -> u64 {{ let before = ckb::input::<Batch>(1).data.messages;\nlet after = ckb::output::<Batch>(0).data.messages;\nrequire before[0][0] == 1;\nrequire after[0][0] == 4;\nreturn 0;\n}}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                &source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap(),
        );
        let mut ranges = Vec::new();
        for block in &valid.record.blocks {
            for label in &block.exact_role_labels {
                let Some(parts) = label.strip_prefix(".Lexact_role_data_") else { continue };
                let parts = parts.split('_').collect::<Vec<_>>();
                if parts.len() != 6 || parts[4] == "0" {
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
                ranges.push((label.clone(), block.range.start..end, parts[3].parse::<i32>().unwrap()));
            }
        }
        assert_eq!(ranges.len(), 2);
        let (label, range, original) = &ranges[1];
        let replacement = ranges[0].2;
        assert!(*original + 520 < 2048, "fixture must exercise directly encoded offsets");
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut rewrites = Vec::new();
        for instruction in elf.instructions.iter().filter(|i| range.contains(&i.address)) {
            let word = instruction.word;
            if word >> 15 & 31 != 2 {
                continue;
            }
            let opcode = word & 0x7f;
            let offset = if opcode == 0x23 {
                (((word >> 25) << 5) | ((word >> 7) & 31)) as i32
            } else if opcode == 3 || opcode == 0x13 {
                (word as i32) >> 20
            } else {
                continue;
            };
            if (*original..*original + 520).contains(&offset) {
                let offset = offset - original + replacement;
                rewrites.push((
                    instruction.address,
                    if opcode == 0x23 { replace_s_immediate(word, offset) } else { replace_i_immediate(word, offset) },
                ));
            }
        }
        assert!(rewrites.len() >= 10);
        let mut changed = valid.clone();
        let mut parts = label.strip_prefix(".Lexact_role_data_").unwrap().split('_').map(str::to_string).collect::<Vec<_>>();
        parts[3] = replacement.to_string();
        let renamed = format!(".Lexact_role_data_{}", parts.join("_"));
        for block in &mut changed.record.blocks {
            for marker in &mut block.exact_role_labels {
                if marker == label || marker == &format!("{label}_done") {
                    *marker = marker.replace(label, &renamed);
                }
            }
            if let Some(marker) = &mut block.machine_label
                && (marker == label || marker == &format!("{label}_done"))
            {
                *marker = marker.replace(label, &renamed);
            }
        }
        changed.replace_machine_words(&rewrites);
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        assert!(error.message.contains("share a retained buffer"), "{error}");
    }
}

#[test]
fn nested_role_selection_checks_profiles_layouts_and_complete_machine_code() {
    let source = include_str!("../syntax_combo/seeds/named-nested-selection.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(
                source,
                CompileOptions { edition: CURRENT_EDITION, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("nested selection {diagnostics}: {error}")),
        );
        let profiles = valid
            .record
            .typed_semantics
            .entries
            .iter()
            .flat_map(|entry| &entry.blocks)
            .flat_map(|block| &block.operations)
            .filter_map(|op| match &op.detail {
                TypedSemanticOperationDetail::ExactRoleSelection { contract } => Some(contract.profile.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            profiles.iter().filter(|profile| **profile == cellscript_artifact_checker::cell_selection::BOUNDED_PROFILE).count(),
            2
        );
        assert_eq!(profiles.iter().filter(|profile| **profile == cellscript_artifact_checker::cell_selection::PROFILE).count(), 1);
        assert_bounded_admitted(&valid);
        let mut downgraded = valid.clone();
        for op in downgraded
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
        {
            if let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut op.detail {
                contract.profile = cellscript_artifact_checker::cell_selection::PROFILE.into();
            }
        }
        downgraded.rebind_layered_identities();
        assert_code(&downgraded, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        // Change selected identity offset, actual-byte accounting, nested
        // key normalization after rebinding hashes.
        // Each pattern is specific to the admitted selection's instruction body.
        let patterns = [
            ((8 << 20) | (29 << 15) | (4 << 12) | (30 << 7) | 3, (4 << 20) | (29 << 15) | (4 << 12) | (30 << 7) | 3),
            ((16 << 20) | (2 << 15) | (3 << 12) | (7 << 7) | 3, (24 << 20) | (2 << 15) | (3 << 12) | (7 << 7) | 3),
            ((30 << 20) | (29 << 15) | (29 << 7) | 0x33, (7 << 20) | (29 << 15) | (29 << 7) | 0x33),
        ];
        for (original, replacement) in patterns {
            let instruction =
                elf.instructions.iter().find(|instruction| instruction.word == original).expect("nested selection obligation");
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, replacement);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut reordered = valid.clone();
        let ty = reordered.record.typed_semantics.types.iter_mut().find(|ty| ty.name == "Order").unwrap();
        for field in &mut ty.fields {
            if field.molecule_field_index == Some(0) {
                field.molecule_field_index = Some(2);
            } else if field.molecule_field_index == Some(2) {
                field.molecule_field_index = Some(0);
            }
        }
        reordered.rebind_layered_identities();
        assert!(check_bundle_values(
            &reordered.artifact,
            &reordered.metadata,
            &reordered.record,
            &reordered.source_map,
            &CheckerBudgets::default()
        )
        .is_err());
    }
}

#[test]
fn exact_role_declarations_reject_rebound_substitutions_after_admission() {
    let valid = Fixture::from_source(include_str!("../../examples/language/ckb/exact_role_verification.cell"));
    assert_bounded_admitted(&valid);

    // These validate the declaration boundary. The separate machine test also
    // rejects well-formed catalog substitutions after hashes are rebound.
    for case in 0..7 {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "verify-exact-cell-roles")
            .unwrap();
        let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { panic!() };
        match case {
            0 => contract.input_scan = 65,
            1 => contract.roles[1].type_hash = contract.roles[0].type_hash,
            2 => contract.roles[0].type_name = "UnknownCell".into(),
            3 => contract.roles[0].identity_field = "unknown".into(),
            4 => contract.roles[0].outputs = (2, 1),
            5 => contract.roles[0].correspondence = cellscript_artifact_checker::cell_selection::Correspondence::Merge(vec![0]),
            6 => contract.roles.clear(),
            _ => unreachable!(),
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn exact_role_instruction_contract_checks_each_lifecycle_and_local_authority() {
    let source = include_str!("../../examples/language/ckb/exact_role_verification.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for mode in ["preserve", "create", "burn", "merge"] {
            let mut source = source.replace("\"preserve\"", &format!("\"{mode}\""));
            if mode == "merge" {
                source = source.replace("1, 16, 1, 16", "2, 16, 1, 1, b\"0123456789abcdef\"");
            }
            source = source.replacen("\"observe\"", "\"local-type\"", 1);
            let valid = compile(&source, CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() })
                .unwrap_or_else(|error| panic!("{mode} {diagnostics}: {error}"));
            Fixture::from_result(valid);
        }
    }
}

#[test]
fn exact_role_machine_rejects_rebound_predicate_load_store_and_policy_mutations() {
    let valid = Fixture::from_source(include_str!("../../examples/language/ckb/exact_role_verification.cell"));
    let start = valid
        .record
        .blocks
        .iter()
        .find(|block| block.exact_role_labels.iter().any(|label| !label.ends_with("_done")))
        .unwrap()
        .range
        .start;
    let end = valid
        .record
        .blocks
        .iter()
        .find(|block| block.exact_role_labels.iter().any(|label| label.ends_with("_done")))
        .unwrap()
        .range
        .start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let mut mutations = 0;
    for instruction in elf.instructions.iter().filter(|instruction| (start..end).contains(&instruction.address)) {
        let word = instruction.word;
        let opcode = word & 0x7f;
        let mask = match opcode {
            0x63 => 1 << 12, // Invert a predicate without changing its CFG destinations.
            0x03 => 1 << 20, // Read the wrong frame, hash or key byte.
            0x23 => 1 << 7,  // Write another counter/record byte.
            0x13 if word >> 12 & 7 == 0 && word >> 7 & 31 != 2 => 1 << 20,
            _ => continue,
        };
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ mask);
        assert!(changed.check().is_err(), "rebound selection instruction accepted at {:#x}", instruction.address);
        mutations += 1;
    }
    assert!(mutations > 500, "exercise both complete scans, every role, key bytes and correspondence loops");
    for case in 0..7 {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| op.opcode == "verify-exact-cell-roles")
            .unwrap();
        let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &mut operation.detail else { panic!() };
        match case {
            0 => contract.roles[0].type_hash[31] ^= 1,
            1 => contract.roles[0].lock_hash[31] ^= 1,
            2 => contract.roles[0].identity_field = "amount".into(),
            3 => contract.input_scan -= 1,
            4 => contract.selected_per_side -= 1,
            5 => contract.roles[0].inputs.0 = 0,
            6 => contract.roles[0].local_type = true,
            _ => unreachable!(),
        }
        changed.rebind_layered_identities();
        if case == 6 {
            // Local roles additionally observe the current Script. Rebound
            // contract bytes alone cannot justify omitting that inferred fact.
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        } else {
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
    let mut missing = valid.clone();
    for block in &mut missing.record.blocks {
        block.exact_role_labels.clear();
    }
    missing.rebind_sidecars();
    assert_code(&missing, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn exact_role_machine_binds_verification_order_and_conditional_block_ownership() {
    let source = include_str!("../../examples/language/ckb/exact_role_verification.cell");
    let begin = source.find("    ckb::verify_cell_roles(").unwrap();
    let end = source.rfind("    return 0").unwrap();
    let repeated = format!("{}{}{}{}", &source[..begin], &source[begin..end], &source[begin..end], &source[end..]);
    let valid = Fixture::from_source(&repeated);
    let starts = valid
        .record
        .blocks
        .iter()
        .flat_map(|block| &block.exact_role_labels)
        .filter(|label| !label.ends_with("_done"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    let mut reversed = valid.clone();
    for block in &mut reversed.record.blocks {
        for label in &mut block.exact_role_labels {
            let done = label.ends_with("_done");
            let base = label.strip_suffix("_done").unwrap_or(label);
            let replacement = if base == starts[0] {
                &starts[1]
            } else if base == starts[1] {
                &starts[0]
            } else {
                continue;
            };
            *label = format!("{replacement}{}", if done { "_done" } else { "" });
        }
        block.exact_role_labels.sort();
    }
    reversed.rebind_sidecars();
    assert_code(&reversed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let conditional = format!("{}if flag {{\n{}}}\n{}", &source[..begin], &source[begin..end], &source[end..])
        .replace("verify()", "verify(witness flag: bool)");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(&conditional, CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..Default::default() })
            .unwrap_or_else(|error| panic!("conditional {diagnostics}: {error}"));
        Fixture::from_result(compiled);
    }
}

#[test]
fn exact_role_scratch_preserves_nested_views_and_rejects_rebound_aliases() {
    let source = include_str!("../../examples/language/ckb/exact_role_verification.cell")
        .replace("verify()", "verify(witness items: NestedValues<Vec<u8>, 2, 8, 32>)")
        .replace(
            "    ckb::verify_cell_roles(",
            "    let first = items[0];\n    let before = first.len();\n    ckb::verify_cell_roles(",
        )
        .replace(
            "    return 0;",
            "    require items.len() == 1;\n    require first.len() == before;\n    require first[0] == 170;\n    return 0;",
        );
    let cell_source = source
        .replace("witness items: NestedValues<Vec<u8>, 2, 8, 32>", "input document: Document")
        .replace("action verify(", "resource Document has store, consume { entries: NestedValues<Vec<u8>, 2, 8, 32> }\naction verify(")
        .replace("let first = items[0];", "let items = document.entries;\n    let first = items[0];")
        .replace("return 0;", "consume document;\n    return 0;");
    for (source, cell) in [(&source, false), (&cell_source, true)] {
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("nested selection {diagnostics}: {error}")),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let (block, label) = valid
                .record
                .blocks
                .iter()
                .find_map(|block| block.exact_role_labels.iter().find(|label| !label.ends_with("_done")).map(|label| (block, label)))
                .unwrap();
            let end_label = format!("{label}_done");
            let end = valid.record.blocks.iter().find(|block| block.exact_role_labels.contains(&end_label)).unwrap().range.start;
            let typed = valid.record.typed_semantics.entries.iter().find(|entry| entry.id == block.owner_entry).unwrap();
            let scalar_end = typed.locals.iter().map(|local| (local.source_id + 1) * 8).max().unwrap();
            let sizes = valid
                .record
                .blocks
                .iter()
                .flat_map(|block| {
                    block
                        .nested_read_labels
                        .iter()
                        .filter(|label| label.starts_with(".Lnested_use_length_") && !label.ends_with("_done"))
                        .map(|label| {
                            let done = format!("{label}_done");
                            let end =
                                valid.record.blocks.iter().find(|block| block.nested_read_labels.contains(&done)).unwrap().range.start;
                            (block.range.start, end)
                        })
                })
                .collect::<Vec<_>>();
            let size = if cell {
                let label = valid
                    .record
                    .blocks
                    .iter()
                    .flat_map(|block| &block.nested_cell_labels)
                    .find(|label| !label.ends_with("_done") && !label.ends_with("_validate"))
                    .unwrap();
                let buffer = label.strip_prefix(".Lnested_cell_codec_").unwrap().split('_').nth(2).unwrap().parse::<i32>().unwrap();
                // Start inside the loaded Cell's bytes, after its scalar pointer.
                (buffer + 15) & !15
            } else {
                elf.instructions
                    .iter()
                    .filter(|instruction| {
                        sizes.iter().any(|(start, end)| (*start..*end).contains(&instruction.address))
                            && instruction.word & 0x707f == 0x3003
                            && instruction.word >> 15 & 31 == 2
                            && instruction.word >> 7 & 31 == 11
                    })
                    .map(|instruction| (instruction.word as i32) >> 20)
                    .find(|offset| *offset >= scalar_end as i32 && offset % 16 == 0)
                    .expect("a nested length slot outside scalar locals can alias an aligned selection scratch")
            };
            assert!(size >= scalar_end as i32 && size < 2048);
            let start_word = elf.instructions.iter().find(|instruction| instruction.address == block.range.start).unwrap().word;
            let end_word = elf.instructions.iter().find(|instruction| instruction.address == end - 4).unwrap().word;
            assert_eq!(start_word & 0xfffff, 0x10113, "small positive SP adjustment");
            assert_eq!(end_word & 0xfffff, 0x10113, "small negative SP adjustment");
            let mut changed = valid.clone();
            changed.replace_machine_words(&[
                (block.range.start, replace_i_immediate(start_word, size)),
                (end - 4, replace_i_immediate(end_word, -size)),
            ]);
            let mut parts = label.strip_prefix(".Lexact_role_check_").unwrap().split('_').map(str::to_owned).collect::<Vec<_>>();
            parts[2] = size.to_string();
            let replacement = format!(".Lexact_role_check_{}", parts.join("_"));
            for block in &mut changed.record.blocks {
                for candidate in &mut block.exact_role_labels {
                    if candidate == label {
                        *candidate = replacement.clone();
                    } else if candidate == &end_label {
                        *candidate = format!("{replacement}_done");
                    }
                }
                block.exact_role_labels.sort();
            }
            changed.rebind_sidecars();
            let error = check_bundle_values(
                &changed.artifact,
                &changed.metadata,
                &changed.record,
                &changed.source_map,
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("overlaps protected runtime storage"), "{diagnostics}: {error:?}");
        }
    }
}

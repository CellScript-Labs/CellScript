#[test]
fn nested_value_helpers_bind_validated_spans_through_calls() {
    let source = include_str!("../syntax_combo/seeds/named-nested-helpers.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CURRENT_EDITION,
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
fn nested_derived_helpers_bind_validated_elements_through_calls() {
    let source = include_str!("../syntax_combo/seeds/named-derived-helpers.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(
                source,
                CompileOptions {
                    edition: cellscript::CURRENT_EDITION,
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
fn nested_value_helpers_bind_cell_fields_alongside_borrowed_cells() {
    let shape = "NestedValues<Vec<u8>, 32, 256, 4096>";
    for padding in [0, 5, 7, 9] {
        let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
        let args = "0, ".repeat(padding);
        let source = format!(
            "module mixed_nested_helpers;\nresource Document has store, consume {{ entries: {shape}, }}\nfn count({params}values: {shape}, document: &Document) -> u64 {{ let alias = values;\nreturn alias.len() + document.entries.len(); }}\nfn forward(document: &Document) -> u64 {{ let values = document.entries;\nreturn count({args}values, document); }}\naction inspect(input before: Document) {{ \nlet values = before.entries;\nrequire (count({args}values, &before) == 2);\nrequire (forward(&before) == 2);\nconsume before; }}"
        );
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
        }
    }
}

#[test]
fn nested_value_helpers_reject_rebound_sources_spills_and_paired_arguments() {
    for (shape, parameter, read, argument, declaration) in [
        ("NestedValues<Vec<u8>, 2, 3, 32>", "NestedValues<Vec<u8>, 2, 3, 32>", "values[0][0]", "", ""),
        ("NestedValues<Vec<u8>, 2, 3, 32>", "NestedBytesView<3>", "values[0]", "[0]", ""),
        (
            "NestedValues<Record, 2, 3, 64>",
            "NestedRecordView<Record, 3>",
            "values.id",
            "[0]",
            "struct Record { id: u8, payload: Vec<u8> }",
        ),
    ] {
        for padding in [0, 7, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!("module nested_helper_binding;\n{declaration}\nfn first({params}values: {parameter}) -> u64 {{ return {read} as u64; }}\nfn forward(a: {shape}, b: {shape}) -> u64 {{ let alias = a;\nlet left_value = alias{argument};\nlet right_value = b{argument};\nlet left = first({args}left_value);\nlet right = first({args}right_value);\nreturn left + right; }}\naction inspect(witness a: {shape}, witness b: {shape}, witness expected: u64) {{ require forward(a, b) == expected; }}");
            for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
                let valid = Fixture::from_result(
                    compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() })
                        .unwrap(),
                );
                let forward = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("forward")).unwrap();
                let mut changed = valid.clone();
                let mutated = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == forward.id).unwrap();
                let calls = mutated
                    .blocks
                    .iter_mut()
                    .flat_map(|block| &mut block.operations)
                    .filter(|op| op.call.as_ref().is_some_and(|call| call.target.ends_with("first")))
                    .collect::<Vec<_>>();
                let [first, second]: [_; 2] = calls.try_into().unwrap();
                assert_ne!(first.operands[padding].local, second.operands[padding].local);
                first.operands[padding] = second.operands[padding].clone();
                changed.rebind_layered_identities();
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
                let helper = valid.record.typed_semantics.entries.iter().find(|entry| entry.name.ends_with("first")).unwrap();
                let parameter = helper.locals.iter().find(|local| local.id == helper.params[padding].binding_id).unwrap();
                let slot = parameter.source_id as u32 * 8;
                let entry = valid.record.entries.iter().find(|entry| entry.id == helper.id).unwrap();
                let start = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start;
                let body = valid
                    .record
                    .blocks
                    .iter()
                    .filter(|block| block.owner_entry == helper.id && block.lowering_block_id.is_some())
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
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(spill.address, spill.word & !(31 << 20));
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                // Widening a callee's element payload bound must fail even
                // though its callers also validate their complete values.
                let bound = elf
                    .instructions
                    .iter()
                    .find(|instruction| (start..body).contains(&instruction.address) && instruction.word == (3 << 20 | 28 << 7 | 0x13))
                    .unwrap();
                let mut changed = valid.clone();
                changed.replace_machine_word(bound.address, 4 << 20 | 28 << 7 | 0x13);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

                if padding == 0 {
                    let calls = elf
                        .control_flow
                        .iter()
                        .filter(|flow| {
                            flow.target == start
                                && valid
                                    .record
                                    .blocks
                                    .iter()
                                    .any(|block| block.owner_entry == forward.id && block.range.contains(flow.address))
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(calls.len(), 2);
                    let mut calls = calls;
                    calls.sort_by_key(|flow| flow.address);
                    let forward_start = valid
                        .record
                        .blocks
                        .iter()
                        .filter(|block| block.owner_entry == forward.id && block.lowering_block_id.is_some())
                        .map(|block| block.range.start)
                        .min()
                        .unwrap();
                    let loads = |from, until, register| {
                        elf.instructions
                            .iter()
                            .rev()
                            .find(|instruction| {
                                let word = instruction.word;
                                (from..until).contains(&instruction.address)
                                    && word & 0x707f == 0x3003
                                    && word >> 7 & 31 == register
                                    && word >> 15 & 31 == 2
                            })
                            .unwrap()
                    };
                    // Substitute both words from another completely valid value of
                    // identical type and bounds. Revalidating the callee's codec
                    // alone cannot detect that its caller selected the wrong root.
                    let mut changed = valid.clone();
                    for register in [10, 11] {
                        let first = loads(forward_start, calls[0].address, register);
                        let second = loads(calls[0].address + 4, calls[1].address, register);
                        assert_ne!(first.word >> 20, second.word >> 20);
                        changed.replace_machine_word(first.address, (first.word & 0x000f_ffff) | (second.word & 0xfff0_0000));
                    }
                    let error = check_bundle_values(
                        &changed.artifact,
                        &changed.metadata,
                        &changed.record,
                        &changed.source_map,
                        &CheckerBudgets::default(),
                    )
                    .unwrap_err();
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    assert!(error.to_string().contains("pointer/length do not match"), "{error}");
                }
            }
        }
    }
}

#[test]
fn nested_witness_decoder_rejects_rehashed_instruction_and_marker_changes() {
    for (element, declarations) in [
        ("Vec<u8>", String::new()),
        (
            "Record",
            format!(
                "struct Record {{ {} payload: Vec<u8> }}",
                (0..8).map(|index| format!("id{index}: [u8; 256], ")).collect::<String>()
            ),
        ),
    ] {
        let source = format!("module nested_machine;\n{declarations}\naction inspect(witness values: NestedValues<{element}, 32, 256, 4096>) -> u64 {{ return 0; }}");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let start = valid
                .record
                .blocks
                .iter()
                .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
                .unwrap();
            let end = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block.range.start > start.range.start
                        && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_ready_"))
                })
                .map(|block| block.range.start)
                .min()
                .unwrap();
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let mut count = 0;
            for instruction in &elf.instructions {
                if !(start.range.start..end).contains(&instruction.address) || instruction.word & 0x7f == 0x6f {
                    continue;
                }
                let mut changed = valid.clone();
                let word = instruction.word ^ if instruction.word & 0x7f == 0x63 { 1 << 12 } else { 1 << 20 };
                changed.replace_machine_word(instruction.address, word);
                assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                count += 1;
            }
            assert!(count >= 80, "complete decoder mutation coverage for {element} {diagnostics}");
            let mut changed = valid.clone();
            changed.record.blocks.iter_mut().find(|block| block.id == start.id).unwrap().machine_label = None;
            changed.rebind_sidecars();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn nested_witness_machine_contract_covers_zero_bounds_and_multiple_parameters() {
    let source = "module nested_multiple;\naction inspect(witness empty: NestedValues<Vec<u8>, 0, 0, 4>, witness nonempty: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {  return 0; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
    }
}

#[test]
fn nested_witness_inputs_bind_register_stack_and_mixed_width_arguments() {
    let prefixes = [
        String::new(),
        (0..7).map(|index| format!("witness p{index}: u64, ")).collect(),
        (0..9).map(|index| format!("witness p{index}: u64, ")).collect(),
        "witness hash: Hash, witness wide: u128, witness tag: [u8; 17], witness flag: u64, ".into(),
        "witness flag: bool, witness delta: i32, witness empty: (), witness small: [u8; 3], ".into(),
    ];
    for prefix in prefixes {
        let source = format!("module nested_abi;\naction inspect({prefix}witness first: NestedValues<Vec<u8>, 2, 3, 32>, witness second: NestedValues<Vec<u8>, 2, 3, 32>) -> u64 {{ return 0; }}");
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::from_result(
                compile(&source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
            );
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let decoders = valid
                .record
                .blocks
                .iter()
                .filter(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
                .collect::<Vec<_>>();
            assert_eq!(decoders.len(), 2);
            for decoder in &decoders {
                // The two final reloads supply the pointer and length. Changing
                // either to an adjacent initialized slot keeps the CFG and all
                // hashes valid but changes which incoming ABI word is checked.
                for distance in [4, 8] {
                    let at = decoder.range.start - distance;
                    let instruction = elf.instructions.iter().find(|instruction| instruction.address == at).unwrap();
                    assert_eq!(instruction.word & 0x707f, 0x3003);
                    let mut changed = valid.clone();
                    changed.replace_machine_word(at, replace_i_immediate(instruction.word, ((instruction.word as i32) >> 20) + 8));
                    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
            // Equal codec bounds do not make two parameter identities fungible.
            let mut changed = valid.clone();
            let left = decoders[0].machine_label.clone();
            let right = decoders[1].machine_label.clone();
            changed.record.blocks.iter_mut().find(|block| block.id == decoders[0].id).unwrap().machine_label = right;
            changed.record.blocks.iter_mut().find(|block| block.id == decoders[1].id).unwrap().machine_label = left;
            changed.rebind_sidecars();
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

            // Redirect the real caller past the complete decoder prefix, and
            // update both its machine target and declared CFG edge together.
            let entry = valid.record.entries.iter().find(|entry| entry.id == "action:inspect").unwrap();
            let entry_block = valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap();
            let caller = elf.control_flow.iter().find(|flow| flow.target == entry_block.range.start).unwrap();
            let caller_block = valid.record.blocks.iter().find(|block| block.range.contains(caller.address)).unwrap();
            let ready = valid
                .record
                .blocks
                .iter()
                .filter(|block| {
                    block.owner_entry == entry.id
                        && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_ready_"))
                })
                .max_by_key(|block| block.range.start)
                .unwrap();
            let old_word = elf.instructions.iter().find(|instruction| instruction.address == caller.address).unwrap().word;
            let mut changed = valid.clone();
            for edge in &mut changed.record.edges {
                if edge.from == caller_block.id && edge.to == entry_block.id {
                    edge.to = ready.id.clone();
                }
            }
            changed.record.canonicalize();
            if old_word & 0x7f == 0x6f {
                let jump = encode_jal((ready.range.start as i64 - caller.address as i64) as i32) | (old_word & (31 << 7));
                changed.replace_machine_word(caller.address, jump);
            } else {
                assert_eq!(old_word & 0x7f, 0x67);
                let high_address = caller.address - 4;
                let high_word = elf.instructions.iter().find(|instruction| instruction.address == high_address).unwrap().word;
                assert_eq!(high_word & 0x7f, 0x17);
                let offset = ready.range.start as i64 - high_address as i64;
                let high = (offset + 0x800) >> 12;
                let low = offset - (high << 12);
                changed.replace_machine_words(&[
                    (high_address, (high_word & 0xfff) | ((high as u32) << 12)),
                    (caller.address, replace_i_immediate(old_word, low as i32)),
                ]);
            }
            // The general ABI boundary already rejects a call into an entry's
            // interior before the feature-specific prefix checker runs.
            assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
        }
    }
}

#[test]
fn nested_witness_transport_rejects_rehashed_envelope_and_argument_mutations() {
    let source = "module nested_transport;\naction inspect(witness salt: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, witness tail: Hash) -> u64 {  return 0; }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let wrapper = valid.record.entries.iter().find(|entry| entry.name == "_cellscript_entry").unwrap();
        let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == wrapper.id).collect::<Vec<_>>();
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let opcode = word & 0x7f;
            let rd = (word >> 7) & 31;
            // Invert each conditional without changing its two CFG successors;
            // shift each byte read; change each register argument's origin.
            let changed_word = if opcode == 0x63 {
                word ^ (1 << 12)
            } else if word & 0x707f == 0x4003 {
                replace_i_immediate(word, ((word as i32) >> 20) + 1)
            } else if word & 0x707f == 0x13 && (10..=17).contains(&rd) && ((word >> 15) & 31) != 0 {
                word ^ (1 << 15)
            } else {
                continue;
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, changed_word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            mutations += 1;
        }
        assert!(mutations >= 60, "complete envelope and positional source checks at {diagnostics}");
    }
}

#[test]
fn nested_mixed_script_args_transport_binds_disjoint_buffers_and_live_cursors() {
    let source = "module mixed_transport;\nlock inspect(lock_args domain: u64, witness a: u64, witness b: u64, witness c: u64, witness d: u64, witness e: u64, witness f: u64, witness values: NestedValues<Vec<u8>, 2, 3, 32>, lock_args owner: Hash, lock_args nonce: u16, lock_args delta: i32) -> bool {  true }";
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
            compile(source, CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap(),
        );
        let wrapper = valid.record.entries.iter().find(|entry| entry.name == "_cellscript_entry").unwrap();
        let blocks = valid.record.blocks.iter().filter(|block| block.owner_entry == wrapper.id).collect::<Vec<_>>();
        let fail = blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_fail_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut mutations = 0;
        for instruction in &elf.instructions {
            if instruction.address >= fail || !blocks.iter().any(|block| block.range.contains(instruction.address)) {
                continue;
            }
            let word = instruction.word;
            let kind = word & 0x707f;
            let rd = (word >> 7) & 31;
            // Disturb Script buffer/size/cursor constants, real byte reads,
            // cursor guards and outgoing argument sources after rebinding.
            let changed_word = if word & 0x7f == 0x63 {
                word ^ (1 << 12)
            } else if kind == 0x4003 || (kind == 0x13 && (5..=31).contains(&rd) && ((word as i32) >> 20) >= 8) {
                replace_i_immediate(word, ((word as i32) >> 20) + 1)
            } else if kind == 0x3023 && decode_s_immediate(word) < 0 {
                word ^ (1 << 20)
            } else {
                continue;
            };
            let mut changed = valid.clone();
            changed.replace_machine_word(instruction.address, changed_word);
            assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            mutations += 1;
        }
        assert!(mutations >= 100, "mixed transport mutations at {diagnostics}");
        // Rewrite all uses of one address-helper scratch register together.
        // Its address calculation remains internally consistent, but t3 still
        // owns the Script total length and must survive the args-offset read.
        let header = blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_script_table_header_ok_")))
            .unwrap();
        let helper = header.range.start + 4;
        let words = (0..4)
            .map(|index| elf.instructions.iter().find(|instruction| instruction.address == helper + index * 4).unwrap().word)
            .collect::<Vec<_>>();
        assert_eq!(words[0] & 0x7f, 0x37);
        assert_eq!(words[1] & 0x707f, 0x13);
        assert_eq!(words[2] & 0x7f, 0x33);
        assert_eq!(words[3] & 0x707f, 0x4003);
        let mut changed = valid.clone();
        changed.replace_machine_words(&[
            (helper, (words[0] & !(31 << 7)) | (28 << 7)),
            (helper + 4, (words[1] & !((31 << 7) | (31 << 15))) | (28 << 7) | (28 << 15)),
            (helper + 8, (words[2] & !((31 << 7) | (31 << 20))) | (28 << 7) | (28 << 20)),
            (helper + 12, (words[3] & !(31 << 15)) | (28 << 15)),
        ]);
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn nested_view_type_rules_reject_rehashed_mutation_and_pointer_operations() {
    let valid = Fixture::from_source(
        r#"
module checked_nested_views;
struct Record { id: u64, payload: Vec<u8>, }
action inspect(witness values: NestedValues<Record, 3, 8, 128>) -> u64 {

        require values.len() == 1;
        let record = values[0];
        require record.id == 42;
        require record.payload.len() == 1;
        require record.payload[0] == 7;
        return 0;
}
"#,
    );
    for opcode in ["collection-clear", "collection-reverse", "collection-capacity", "type-hash", "return"] {
        let mut changed = valid.clone();
        let operation = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.opcode == "length")
            .unwrap();
        operation.opcode = opcode.into();
        if matches!(opcode, "collection-clear" | "collection-reverse" | "return") {
            operation.destinations.clear();
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }

    for replacement in ["NestedRecordView<Record, 9>", "NestedBytesView<8>", "Record"] {
        let mut changed = valid.clone();
        // Rebind every use and declaration of the derived value together so
        // rejection must enforce the owner's declared element/payload shape.
        for entry in &mut changed.record.typed_semantics.entries {
            for local in &mut entry.locals {
                if local.ty == "NestedRecordView<Record, 8>" {
                    local.ty = replacement.into();
                }
            }
            for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
                for operand in &mut operation.operands {
                    if operand.ty == "NestedRecordView<Record, 8>" {
                        operand.ty = replacement.into();
                    }
                }
            }
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

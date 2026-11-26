#[test]
fn fixed_cell_fields_check_inline_capacity_and_hash_observation_memory() {
    let source = SOURCE.replace("consume token;", "require token.amount == 7; consume token;");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, selected.clone());
        fixed_cell_fields(&fixture).unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let site = fixture
            .record
            .syscall_sites
            .iter()
            .find(|site| {
                fixture.record.blocks.iter().any(|block| {
                    block.id == site.block_id
                        && block.owner_entry == "action:burn"
                        && elf.instructions.iter().any(|instruction| {
                            instruction.address < site.address
                                && block.range.contains(instruction.address)
                                && instruction.word == (15 << 7) | 0x13
                        })
                })
            })
            .unwrap();
        let block = fixture.record.blocks.iter().find(|block| block.id == site.block_id).unwrap();
        let definition = |register| {
            elf.instructions
                .iter()
                .rev()
                .find(|instruction| {
                    instruction.address < site.address
                        && block.range.contains(instruction.address)
                        && instruction.word & 0x7f == 0x13
                        && ((instruction.word >> 7) & 31) == register
                })
                .unwrap()
        };
        let buffer = definition(10);
        let size = definition(11);
        let field = definition(15);
        let index = definition(13);
        let capacity = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < buffer.address
                    && block.range.contains(instruction.address)
                    && instruction.word & 0x7f == 0x13
                    && ((instruction.word >> 7) & 31) == 5
            })
            .unwrap();
        let store = elf.instructions.iter().find(|instruction| instruction.address == capacity.address + 4).unwrap();
        for (address, word) in [
            (capacity.address, (capacity.word & !(0xfff << 20)) | (64 << 20)),
            (store.address, store.word & !(7 << 12)),
            (buffer.address, (buffer.word & !(31 << 15)) | (3 << 15)),
            (size.address, (size.word & !(31 << 15)) | (3 << 15)),
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, word);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        // This call is the fixed-group count probe, so its field and index
        // participate in the main bundle contract.
        for hash_field in [3, 5] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(capacity.address, (capacity.word & !(0xfff << 20)) | (32 << 20));
            changed.replace_machine_word(field.address, (field.word & !(0xfff << 20)) | (hash_field << 20));
            let error = changed.check().unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("fixed group machine coverage"), "{error}");
        }
        let storage: Value = serde_json::from_slice(&fixed_cell_storage(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
        let cell_buffer = storage["cells"][0]["buffer_offset"].as_i64().unwrap();
        let delta = cell_buffer - i64::from((buffer.word as i32) >> 20);
        assert!((-2048..=2047).contains(&delta));
        for instruction in [
            (((delta as u32) & 0xfff) << 20) | (10 << 15) | (10 << 7) | 0x1b,
            (29 << 20) | (10 << 15) | (10 << 7) | 0x3b,
            (0x20 << 25) | (29 << 20) | (10 << 15) | (10 << 7) | 0x3b,
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(index.address, instruction);
            let error = changed.check().unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            assert!(error.message.contains("fixed group machine coverage"), "{error}");
        }
        let number = elf.instructions.iter().find(|instruction| instruction.address == site.address - 4).unwrap();
        assert_eq!(number.word & 0x7f, 0x13);
        let mut changed = fixture.clone();
        changed.replace_machine_word(number.address, (number.word & !0x7f) | 0x1b);
        changed.check().unwrap();
        fixed_cell_storage(&changed).unwrap();
        fixed_cell_fields(&changed).unwrap();
    }
}

#[test]
fn fixed_cell_fields_reject_unproved_owner_syscalls_after_native_materialization() {
    let source = SOURCE.replace("consume token;", "require token.amount == 7; consume token;");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, selected.clone());
        let checked = fixed_cell_fields(&fixture).unwrap();
        let fields: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let storage: Value = serde_json::from_slice(&checked.storage().canonical_bytes().unwrap()).unwrap();
        let load = fields["fields"][0]["pointer_load"].as_u64().unwrap() + 4;
        let end = fields["fields"][0]["decode_end"].as_u64().unwrap();
        let register = fields["fields"][0]["register"].as_u64().unwrap() as u32;
        let size = storage["cells"][0]["size_offset"].as_u64().unwrap() as u32;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let helper = fixture
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == "runtime:__cellscript_require_cell_membership")
            .collect::<Vec<_>>();
        let syscall =
            elf.syscall_addresses.iter().copied().find(|address| helper.iter().any(|block| block.range.contains(*address))).unwrap();
        let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        fixture.replace_machine_word(load, (29 << 15) | (3 << 12) | (register << 7) | 0x03);
        for address in (load + 4..end).step_by(4) {
            fixture.replace_machine_word(address, 0x00000013);
        }
        for (index, instruction) in [
            (29 << 15) | (10 << 7) | 0x13,
            (size << 20) | (2 << 15) | (11 << 7) | 0x13,
            word(syscall - 8),
            word(syscall - 4),
            0x00000073,
        ]
        .into_iter()
        .enumerate()
        {
            fixture.replace_machine_word(load + 4 + 4 * index as u64, instruction);
        }
        let mut site = fixture.record.syscall_sites.iter().find(|site| site.address == syscall).unwrap().clone();
        site.address = load + 20;
        site.block_id = fixture.record.blocks.iter().find(|block| block.range.contains(site.address)).unwrap().id.clone();
        fixture.record.syscall_sites.push(site);
        fixture.record.syscall_sites.sort_by_key(|site| site.address);
        fixture.rebind_sidecars();
        fixture.check().unwrap();
        fixed_cell_storage(&fixture).unwrap();
        assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_fields_enforce_256_sites_without_raising_variant_or_read_limits() {
    for extra in [false, true] {
        let mut source =
            "module field_site_bounds;\nresource Token has store, consume { a: u8, b: u16, c: u32, d: u64, e: u8, }\n".to_owned();
        let mut selected = declaration();
        selected.actions.clear();
        selected.common_checks.clear();
        for index in 0..64 {
            let name = format!("burn{index}");
            selected.actions.push(ArtifactAction { tag: index, action: name.clone() });
            let tail = if extra && index == 0 { "require token.e == 17;" } else { "" };
            source.push_str(&format!("action {name}(input token: Token) {{  require token.a == 42; require token.b == 513; require token.c == 67305985; require token.d == 123456789012345; {tail} consume token; }}\n"));
        }
        let fixture = Fixture::new_source_with(
            &source,
            CellScriptEdition::Edition2027,
            cellscript::CompilerDiagnostics::without_optimizations(),
            selected,
        );
        fixed_cell_storage(&fixture).unwrap();
        if extra {
            assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        } else {
            let record: Value = serde_json::from_slice(&fixed_cell_fields(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["fields"].as_array().unwrap().len(), 256);
        }
    }
}

#[test]
fn fixed_cell_parameter_storage_binds_source_ids_and_actual_pointer_reception() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, declaration());
        let checked = fixed_cell_storage(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(value["cells"].as_array().unwrap().len(), 1);
        assert_eq!(value["cells"][0]["parameter"], "token");
        assert_eq!(value["cells"][0]["pointer_offset"], value["cells"][0]["source_id"].as_u64().unwrap() * 8);
        assert_eq!(value["read_gates"], checked.reads().identity());
        assert_eq!(value["parameter_decoder"], checked.parameters().identity());
        let cell = &value["cells"][0];
        let receiver = cell["receiver_start"].as_u64().unwrap();
        let spill = cell["spill_address"].as_u64().unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, mask, bits) in [
            (receiver, 31 << 15, 3 << 15),
            (receiver, 31 << 7, 6 << 7),
            (receiver + 4, 31 << 15, 3 << 15),
            (receiver + 4, 31 << 20, 6 << 20),
            (spill, 31 << 20, 11 << 20),
        ] {
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, (original & !mask) | bits);
            changed.check().unwrap();
            fixed_cell_reads(&changed).expect("read gates alone do not attest parameter storage");
            assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut changed = fixture.clone();
        let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == "action:burn").unwrap();
        let local_id = entry.cell_bindings[0].local_id.unwrap();
        entry.locals.iter_mut().find(|local| local.id == local_id).unwrap().source_id += 1;
        changed.rebind_policy_identity();
        changed.check().unwrap();
        fixed_cell_reads(&changed).unwrap();
        assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_parameter_storage_rejects_overlapping_argument_spills_and_output_only() {
    let source = SOURCE.replace(
        "action burn(input token: Token) {  consume token; }",
        "action burn(input token: Token, witness n: u64) {  require n > 0; consume token; }",
    );
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, declaration());
        let checked = fixed_cell_storage(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let spill = value["cells"][0]["spill_address"].as_u64().unwrap();
        let pointer = value["cells"][0]["pointer_offset"].as_u64().unwrap() as u32;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let second = elf.instructions.iter().find(|instruction| instruction.address == spill + 4).unwrap().word;
        assert_eq!((second >> 20) & 31, 11);
        let mut changed = fixture.clone();
        changed
            .replace_machine_word(spill + 4, (second & !((0x7f << 25) | (31 << 7))) | ((pointer >> 5) << 25) | ((pointer & 31) << 7));
        changed.check().unwrap();
        fixed_cell_reads(&changed).unwrap();
        assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let mut output = declaration();
        output.actions.retain(|action| action.action == "mint");
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, output);
        fixed_cell_reads(&fixture).unwrap();
        assert_eq!(fixed_cell_storage(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_reads_check_actual_source_size_and_error_gates() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, declaration());
        let checked = fixed_cell_reads(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(!value["reads"].as_array().unwrap().is_empty());
        assert_eq!(value["reads"][0]["width_bytes"], 8);
        assert_eq!(value["reads"][0]["capacity_bytes"], 512);
        assert_eq!(
            value["bundle_hashes"][0],
            cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&fixture.artifact))
        );
        assert!(!checked.module_projection().artifact_report().semantic_equivalence_claimed);
    }
}

#[test]
fn fixed_cell_reads_reject_rebound_machine_setup_and_length_mutations() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, declaration());
        let value: Value = serde_json::from_slice(&fixed_cell_reads(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
        let read = &value["reads"][0];
        let start = read["setup_start"].as_u64().unwrap();
        let syscall = read["syscall_address"].as_u64().unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, mask, bits) in [
            (start, 0xfff0_0000, 513 << 20),                                          // capacity
            (start + 4, 31 << 15, 3 << 15),                                           // size initializer base
            (start + 8, 31 << 15, 3 << 15),                                           // buffer base
            (start + 12, 31 << 15, 3 << 15),                                          // length pointer base
            (start + 16, 0xfff0_0000, 1 << 20),                                       // nonzero data byte offset
            (start + 20, 0xfff0_0000, 1 << 20),                                       // other output ordinal
            (syscall - 12, 0xfff0_0000, 3 << 20),                                     // other Cell source
            (syscall - 4, 0xfff0_0000, ((2093u32.wrapping_sub(4096)) & 0xfff) << 20), // another syscall
            (syscall + 4, 7 << 12, 1 << 12),                                          // inverted status test
            (syscall + 12, 31 << 15, 3 << 15),                                        // length guard reads another pointer
            (syscall + 16, 0xfff0_0000, 9 << 20),                                     // wrong exact length
            (syscall + 20, 31 << 20, 12 << 20),                                       // comparison with wrong register
            (syscall + 24, 7 << 12, 1 << 12),                                         // inverted length test
        ] {
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, (original & !mask) | bits);
            changed.check().expect("ordinary inspection does not certify these Cell read arguments");
            let error = fixed_cell_reads(&changed).unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn fixed_cell_reads_reject_missing_source_contracts_and_noncanonical_boolean_layouts() {
    let source = "module bool_read;\nresource Token has store, consume { amount: bool, }\naction mint(witness recipient: Address) {  create Token { amount: true, } with_lock(recipient) }\n";
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "mint");
    selected.common_checks.clear();
    let fixture = Fixture::new_source_with(
        source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        selected,
    );
    assert!(fixed_cell_reads(&fixture).unwrap_err().message.contains("boolean canonicality"));
    let mut missing = Fixture::new(CellScriptEdition::Edition2027);
    missing.record.typed_semantics.nominal_declarations = None;
    missing.rebind_policy_identity();
    assert!(fixed_cell_reads(&missing).is_err());
}

#[test]
fn fixed_cell_reads_have_real_read_count_and_instruction_budget_limits() {
    for count in [64, 65] {
        let mut source = "module read_count;\nresource Token has store, consume { amount: u64, }\n".to_string();
        for index in 0..64 {
            let input = if count == 65 && index == 0 { "input before: Token, " } else { "" };
            let consume = if input.is_empty() { "" } else { "require before.amount == 7;\nconsume before;\n" };
            source.push_str(&format!("action read_{index}({input}witness recipient: Address) {{ \n{consume}create Token {{ amount: 7 }} with_lock(recipient) }}\n"));
        }
        let selected = ArtifactDeclaration {
            name: "ReadCount".into(),
            context: ArtifactContext::TypeGroup {},
            bindings: cellscript::artifact::EntryCellBindings::GroupRelative,
            coverage: cellscript::artifact::EntryCoverage::FixedGroup,
            layout: Some("Token".into()),
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            entry: None,
            actions: (0..64).map(|index| ArtifactAction { tag: index, action: format!("read_{index}") }).collect(),
            common_checks: Vec::new(),
        };
        let fixture =
            Fixture::new_source_with(&source, CellScriptEdition::Edition2027, cellscript::CompilerDiagnostics::default(), selected);
        if count == 64 {
            let checked = fixed_cell_reads(&fixture).unwrap();
            let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(value["reads"].as_array().unwrap().len(), 64);
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            for read in value["reads"].as_array().unwrap() {
                assert!(elf.syscall_addresses.contains(&read["syscall_address"].as_u64().unwrap()));
            }
        } else {
            assert!(fixed_cell_reads(&fixture).unwrap_err().message.contains("more than 64"));
        }
    }
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    let error = cellscript_artifact_checker::fixed_cell_reads::check_fixed_cell_reads(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets { instructions: 1, ..Default::default() },
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
}

#[test]
fn fixed_policy_parameter_decoders_bind_actual_offsets_and_source_contracts() {
    let mut identity = None;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, declaration());
        let checked = cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(value["variants"]["10"]["action"], "mint");
        assert_eq!(value["variants"]["10"]["parameters"][0]["width_bytes"], 8);
        assert_eq!(value["variants"]["10"]["parameters"][1]["payload_offset"], 8);
        assert_eq!(value["variants"]["10"]["parameters"][1]["transport"], "fixed-byte-pointer");
        assert_eq!(value["variants"]["40"]["parameters"][0]["transport"], "runtime-bound-null");
        assert!(!checked.module_projection().artifact_report().semantic_equivalence_claimed);
        if let Some(previous) = &identity {
            assert_eq!(previous, checked.identity())
        } else {
            identity = Some(checked.identity().to_owned())
        }
    }
}

#[test]
fn fixed_decoder_rejects_rebound_parameter_instructions_that_ordinary_inspection_does_not_certify() {
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2027, opt, declaration());
        let params = fixture
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_exact_size_ok_")))
            .unwrap()
            .range
            .start;
        let size_ok = fixture
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_size_ok_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, kind) in [(params + 4, 0), (params + 8, 1), (params + 16, 2), (size_ok + 4, 3)] {
            let mut changed = fixture.clone();
            let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let modified = match kind {
                0 => (word & 0x000f_ffff) | (17 << 20), // duplicate next byte instead of byte zero
                1 => (word & !(31 << 7)) | (11 << 7),   // overwrite the following ABI argument
                2 => (word & 0x000f_ffff) | (16 << 20), // shift second byte by sixteen instead of eight
                _ => (word & 0x000f_ffff) | (68 << 20), // accept a different witness magic
            };
            changed.replace_machine_word(address, modified);
            changed.check().expect("ordinary inspection does not grant a decoder certificate");
            let error = cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
                &changed.artifact,
                &serde_json::to_vec(&changed.metadata).unwrap(),
                &serde_json::to_vec(&changed.record).unwrap(),
                &serde_json::to_vec(&changed.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{opt} mutation {kind}: {error}");
        }
    }
}

#[test]
fn fixed_decoder_evidence_never_downgrades_unsupported_codec_or_stack_profiles() {
    for source in [
        SOURCE.replace("witness recipient: Address)", "witness recipient: Address, witness flag: bool)"),
        STACK_ARGS_SOURCE.to_owned(),
    ] {
        let mut artifact = declaration();
        artifact.actions.retain(|action| action.action == "mint");
        artifact.common_checks.clear();
        let fixture = Fixture::new_source_with(
            &source,
            CellScriptEdition::Edition2027,
            cellscript::CompilerDiagnostics::without_optimizations(),
            artifact,
        );
        assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .is_err());
    }
}

#[test]
fn fixed_decoder_checks_signed_extension_small_aggregates_and_shared_variants() {
    let source = SOURCE.replace("witness amount: u64, witness recipient: Address", "witness amount: u64, witness recipient: Address, witness signed: i32, witness bytes: [u8; 4], witness pair: (u16, u8)")
        + "\naction other(witness amount: u64, witness recipient: Address, witness signed: i32, witness bytes: [u8; 4], witness pair: (u16, u8)) {  require amount > 0; create Token { amount: amount, } with_lock(recipient) }\n";
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut declaration = declaration();
        declaration.actions.push(ArtifactAction { tag: 11, action: "other".into() });
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt, declaration);
        let checked = certify_fixed_decoder(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let params = &value["variants"]["10"]["parameters"];
        assert_eq!(params[2]["width_bytes"], 4);
        assert_eq!(params[3]["width_bytes"], 4);
        assert_eq!(params[4]["width_bytes"], 3);
        assert_eq!(value["variants"]["11"]["action"], "other");
        assert!(fixture.record.entries.iter().any(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")));
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let signed = elf
            .instructions
            .iter()
            .find(|instruction| {
                let word = instruction.word;
                word & 0x707f == 0x5013 && word >> 20 == 0x420 && (word >> 7) & 31 == 13
            })
            .expect("signed a3 must be extended from 32 bits");
        let mut changed = fixture.clone();
        changed.replace_machine_word(signed.address, signed.word & !(0x400 << 20));
        changed.check().unwrap();
        assert!(certify_fixed_decoder(&changed).is_err(), "unsigned substitute cannot certify i32 at {opt}");
    }
}

#[test]
fn fixed_decoder_profile_limits_are_exact_and_never_raise_checker_budgets() {
    let mut single = declaration();
    single.actions.retain(|action| action.action == "mint");
    single.common_checks.clear();
    for (padding, accepted) in [(1496, true), (1497, false)] {
        let source =
            SOURCE.replace("witness recipient: Address)", &format!("witness recipient: Address, witness padding: [u8; {padding}])"));
        let fixture = Fixture::new_source_with(
            &source,
            CellScriptEdition::Edition2027,
            cellscript::CompilerDiagnostics::without_optimizations(),
            single.clone(),
        );
        assert_eq!(certify_fixed_decoder(&fixture).is_ok(), accepted, "padding={padding}");
    }
    for (extra, accepted) in [(5, true), (6, false)] {
        let parameters = (0..extra).map(|index| format!(", witness p{index}: u8")).collect::<String>();
        let source = SOURCE.replace("witness recipient: Address)", &format!("witness recipient: Address{parameters})"));
        let fixture = Fixture::new_source_with(
            &source,
            CellScriptEdition::Edition2027,
            cellscript::CompilerDiagnostics::without_optimizations(),
            single.clone(),
        );
        assert_eq!(certify_fixed_decoder(&fixture).is_ok(), accepted, "extra={extra}");
    }
    let fixture =
        Fixture::new_with(CellScriptEdition::Edition2027, cellscript::CompilerDiagnostics::without_optimizations(), declaration());
    let budgets = CheckerBudgets { instructions: 1, ..CheckerBudgets::default() };
    assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &budgets,
    )
    .is_err());
    assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &vec![b' '; 4 * 1024 * 1024 + 1],
        b"{}",
        b"{}",
        &CheckerBudgets::default(),
    )
    .is_err());
}

fn certify_fixed_decoder(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::entry_codec::CheckedFixedPolicyParameterDecoders, CheckerError> {
    cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

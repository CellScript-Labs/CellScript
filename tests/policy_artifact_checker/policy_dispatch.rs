#[test]
fn instantiated_interface_templates_reject_rebound_layout_and_signature_substitution() {
    let source = format!(
        "{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic enum Choice<T: fixed_value> {{ First(Pair<T>), Second([T; 2]) }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"
    )
    .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice<Hash>, witness pair: Pair<u64>)")
    .replace("require amount > 0", "require amount > 0;\nrequire first::<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.entries.iter().any(|entry| entry.name.starts_with("first__mono__")));
    for mutation in [
        "field",
        "field-order",
        "field-name",
        "enum-payload",
        "enum-order",
        "arity",
        "return",
        "parameter",
        "constraint",
        "ability",
        "return-concrete",
        "parameter-concrete",
    ] {
        let mut changed = baseline.clone();
        if matches!(mutation, "return" | "parameter" | "constraint" | "return-concrete" | "parameter-concrete") {
            let callable = changed.metadata["public_interface"]["callables"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["name"] == "first")
                .unwrap();
            if mutation == "constraint" {
                callable["type_parameters"][0]["constraints"] = serde_json::json!(["cell"]);
            } else if matches!(mutation, "return" | "return-concrete") {
                callable["return_type"] = serde_json::json!(if mutation == "return" { "BlockNumber" } else { "u64" });
            } else {
                callable["params"][0]["type"] = serde_json::json!(if mutation == "parameter" { "BlockNumber" } else { "u64" });
                // Generic templates have no source-name entry in the execution
                // metadata; recompute their declaration builder digest as well.
                let canonical = cellscript::canonical_json::canonical_json_value(&callable["params"]);
                callable["builder_contract_hash"] = cellscript_artifact_checker::hex_encode(
                    &cellscript_artifact_checker::ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()),
                )
                .into();
            }
            let contracts = changed.metadata["public_interface"]["callables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| serde_json::json!([entry["identity"], entry["builder_contract_hash"]]))
                .collect::<Vec<_>>();
            let canonical = cellscript::canonical_json::canonical_json_value(&serde_json::json!(contracts));
            changed.metadata["public_interface"]["builder_contract_hash"] = cellscript_artifact_checker::hex_encode(
                &cellscript_artifact_checker::ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()),
            )
            .into();
        } else {
            let name = if mutation.starts_with("enum") { "Choice" } else { "Pair" };
            let ty = changed.metadata["public_interface"]["types"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|ty| ty["name"] == name)
                .unwrap();
            match mutation {
                "field" => ty["fields"][0]["type"] = serde_json::json!("Address"),
                "field-order" => ty["fields"].as_array_mut().unwrap().reverse(),
                "field-name" => ty["fields"][0]["name"] = serde_json::json!("forged"),
                "enum-payload" => ty["variants"][1]["fields"][0] = serde_json::json!("[Address; 2]"),
                "enum-order" => ty["variants"].as_array_mut().unwrap().reverse(),
                "arity" => ty["type_parameters"] = serde_json::json!([]),
                "ability" => ty["value_abilities"] = serde_json::json!(["drop"]),
                _ => unreachable!(),
            }
            let canonical =
                cellscript::canonical_json::canonical_json_value(&serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]));
            ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
                &serde_json::to_vec(&canonical).unwrap(),
            ))
            .into();
        }
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("generic") || error.message.contains("checked entry"), "{mutation}: {error}");
    }
}

#[test]
fn interface_cell_capabilities_cannot_be_forged_by_rebinding_outer_hashes() {
    let baseline = Fixture::new(CellScriptEdition::Edition2027);
    baseline.assert_interface_inspection();
    let mut reordered = baseline.clone();
    reordered.metadata["public_interface"]["types"][0]["cell_capabilities"].as_array_mut().unwrap().reverse();
    reordered.rebind_interface_identity();
    reordered.assert_interface_inspection();

    for capabilities in [
        serde_json::json!([]),
        serde_json::json!(["store"]),
        serde_json::json!(["consume", "store", "copy"]),
        serde_json::json!(["consume", "store", "store"]),
    ] {
        let mut changed = baseline.clone();
        changed.metadata["public_interface"]["types"][0]["cell_capabilities"] = capabilities;
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert_eq!(error.message, "interface Cell capabilities differ from checked concrete type");
    }
}

#[test]
fn interface_variant_cardinality_cannot_be_forged_by_rebinding_layout_hashes() {
    let source = format!("{SOURCE}\npublic enum Choice {{ First(u64), Second(Hash) }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice)");
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Choice"));
    for mutation in ["omit", "reorder", "rename", "payload", "same-width-type"] {
        let mut changed = baseline.clone();
        let ty = changed.metadata["public_interface"]["types"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ty| ty["name"] == "Choice")
            .unwrap();
        match mutation {
            "omit" => {
                ty["variants"].as_array_mut().unwrap().pop();
            }
            "reorder" => ty["variants"].as_array_mut().unwrap().reverse(),
            "rename" => ty["variants"][0]["name"] = serde_json::json!("forged"),
            "payload" => ty["variants"][0]["fields"] = serde_json::json!([]),
            "same-width-type" => ty["variants"][1]["fields"][0] = serde_json::json!("Address"),
            _ => unreachable!(),
        }
        let layout = serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]);
        let canonical = cellscript::canonical_json::canonical_json_value(&layout);
        ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("variant set differs"), "{mutation}: {error}");
    }
}

#[test]
fn interface_nested_type_arguments_cannot_be_forged_by_rebinding_layout_hashes() {
    let source = format!(
        "{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic struct Envelope {{ pairs: [Pair<Hash>; 2] }}\npublic enum Choice {{ Nested(Envelope), Direct(Pair<Address>) }}"
    )
    .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice)");
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Envelope"));
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Choice"));
    for name in ["Envelope", "Choice"] {
        let mut changed = baseline.clone();
        let ty =
            changed.metadata["public_interface"]["types"].as_array_mut().unwrap().iter_mut().find(|ty| ty["name"] == name).unwrap();
        if name == "Envelope" {
            ty["fields"][0]["type"] = serde_json::json!("[Pair<Address>; 2]");
        } else {
            ty["variants"][1]["fields"][0] = serde_json::json!("Pair<Hash>");
        }
        let layout = serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]);
        let canonical = cellscript::canonical_json::canonical_json_value(&layout);
        ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("checked concrete layout"), "{name}: {error}");
    }
}

#[test]
fn policy_dispatch_machine_contract_covers_optimizers_tag_extremes_and_no_common_checks() {
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            Fixture::new_with(edition, diagnostics, declaration()).check().unwrap();
        }
    }

    let mut one_payload = declaration();
    one_payload.actions = vec![ArtifactAction { tag: 1, action: "mint".into() }];
    one_payload.common_checks = vec!["check_z".into()];
    let mut one_payload_free = declaration();
    one_payload_free.actions = vec![ArtifactAction { tag: 7, action: "burn".into() }];
    one_payload_free.common_checks.clear();
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::new_with(CellScriptEdition::Edition2027, diagnostics, one_payload.clone()).check().unwrap();
        Fixture::new_with(CellScriptEdition::Edition2027, diagnostics, one_payload_free.clone()).check().unwrap();
    }

    let mut extreme = declaration();
    extreme.actions = vec![ArtifactAction { tag: u32::MAX, action: "burn".into() }, ArtifactAction { tag: 0, action: "mint".into() }];
    extreme.common_checks.clear();
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        Fixture::new_with(CellScriptEdition::Edition2027, diagnostics, extreme.clone()).check().unwrap();
    }
}

#[test]
fn low_mask_policy_tags_reject_rebound_shift_and_seed_mutations() {
    let mut policy = declaration();
    policy.actions =
        vec![ArtifactAction { tag: 0x7fff_ffff, action: "burn".into() }, ArtifactAction { tag: u32::MAX, action: "mint".into() }];
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let valid = Fixture::new_with(edition, diagnostics, policy.clone());
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let sequences: Vec<_> = elf
                .instructions
                .windows(2)
                .filter(|pair| {
                    // addi t1, zero, -1; srli t1, t1, 32/33. These tags
                    // appear in both the precheck and final action routing.
                    pair[0].word == 0xfff0_0313
                        && matches!(pair[1].word, 0x0203_5313 | 0x0213_5313)
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == "wrapper:_cellscript_entry" && block.range.contains(pair[1].address))
                })
                .collect();
            assert_eq!(sequences.len(), 4, "both tags use the short plan in both dispatch checks");
            for pair in sequences {
                for (address, word) in [
                    (pair[0].address, 0x0000_0313),              // zero instead of all ones
                    (pair[1].address, pair[1].word ^ (1 << 20)), // neighboring shift amount
                    (pair[1].address, pair[1].word | (1 << 30)), // arithmetic instead of logical shift
                ] {
                    let mut changed = valid.clone();
                    changed.replace_machine_word(address, word);
                    let error = changed.check().expect_err("altered tag must reject after all outer hashes are rebound");
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

#[test]
fn rebound_policy_machine_mutations_cannot_change_selector_dispatch_or_adapter_dataflow() {
    let valid = Fixture::new(CellScriptEdition::Edition2027);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let wrapper_blocks =
        valid.record.blocks.iter().filter(|block| block.owner_entry == "wrapper:_cellscript_entry").collect::<Vec<_>>();
    let current_hash_syscall = elf
        .syscall_addresses
        .iter()
        .copied()
        .filter(|address| wrapper_blocks.iter().any(|block| block.range.contains(*address)))
        .max()
        .unwrap();
    let copied = policy_block(&valid, ".Lentry_witness_v2_copy_done_").range.start;
    let record_loop = policy_block(&valid, ".Lpolicy_record_").range.start;
    let record_base = elf
        .instructions
        .iter()
        .find(|instruction| {
            wrapper_blocks.iter().any(|block| block.range.contains(instruction.address)) && is_add(instruction.word, 15, 14, 5)
        })
        .unwrap()
        .address;
    let key_loop = policy_block(&valid, ".Lpolicy_key_order_loop_").range.start;
    let ordered = policy_block(&valid, ".Lpolicy_key_ordered_").range.start;
    let hash_loop = policy_block(&valid, ".Lpolicy_current_hash_loop_").range.start;
    let first_variant = valid
        .record
        .blocks
        .iter()
        .filter(|block| {
            block.owner_entry == "wrapper:_cellscript_entry"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_variant_"))
        })
        .min_by_key(|block| block.range.start)
        .unwrap()
        .range
        .start;
    let first_adapter = valid
        .record
        .entries
        .iter()
        .filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_"))
        .min_by_key(|entry| valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start)
        .unwrap();
    let adapter_copy = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == first_adapter.id)
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_args_copy_")))
        .unwrap()
        .range
        .start;
    let common_target = valid
        .record
        .entries
        .iter()
        .find(|entry| entry.id == "action:check_z")
        .and_then(|entry| valid.record.blocks.iter().find(|block| block.id == entry.entry_block))
        .unwrap()
        .range
        .start;
    let common_call = elf
        .control_flow
        .iter()
        .find(|flow| flow.target == common_target && wrapper_blocks.iter().any(|block| block.range.contains(flow.address)))
        .unwrap()
        .address;
    let mutations = [
        ("current-script-hash-syscall", current_hash_syscall - 4),
        ("policy-magic", copied + 32),
        ("dynvec-record-bound", record_loop - 36),
        ("record-layout", record_base + 4),
        ("strict-key-order", key_loop + 24),
        ("type-role", ordered + 8),
        ("selected-tag", hash_loop + 68),
        ("common-check-failure", common_call + 4),
        ("variant-args-pointer", first_variant + 4),
        ("adapter-private-copy", adapter_copy + 8),
    ];
    for (name, address) in mutations {
        let mut changed = valid.clone();
        let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{name}: {error}");
    }
}

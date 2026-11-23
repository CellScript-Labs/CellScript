#[test]
fn typed_outgoing_stack_args_are_bound_to_the_policy_adapter_frame() {
    let declaration = ArtifactDeclaration {
        name: "stack-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 1, action: "mint".into() }],
        common_checks: Vec::new(),
    };
    let valid = Fixture::new_source_with(
        STACK_ARGS_SOURCE,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::default(),
        declaration,
    );
    let adapter = valid.record.entries.iter().find(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).unwrap();
    // 104 payload bytes + 8-byte CSARG header + length/RA slots,
    // aligned to 16, plus the 32-byte outgoing argument reservation.
    assert_eq!(adapter.frame_size_bytes, 160);

    let mut changed = valid.clone();
    let adapter_id = adapter.id.clone();
    changed.record.entries.iter_mut().find(|entry| entry.id == adapter_id).unwrap().frame_size_bytes += 16;
    for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == adapter_id) {
        block.frame_size_bytes += 16;
    }
    changed.rebind_sidecars();
    let error = changed.check().unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
    assert!(error.message.contains("policy positional adapter frame contract changed"), "{error}");
}

#[test]
fn shared_decoder_preserves_private_return_storage_with_outgoing_stack_arguments() {
    let second = STACK_ARGS_SOURCE.split_once("action mint(").unwrap().1;
    let source = format!("{STACK_ARGS_SOURCE}\naction mint_other({second}");
    let declaration = ArtifactDeclaration {
        name: "shared-stack-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 1, action: "mint".into() }, ArtifactAction { tag: 2, action: "mint_other".into() }],
        common_checks: Vec::new(),
    };
    let valid =
        Fixture::new_source_with(&source, CellScriptEdition::Edition2027, cellscript::CompilerDiagnostics::default(), declaration);
    let decoders: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")).collect();
    assert_eq!(decoders.len(), 1);
    assert_eq!(decoders[0].frame_size_bytes, 0);
    let adapters: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).collect();
    assert_eq!(adapters.len(), 2);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for adapter in adapters {
        assert_eq!(adapter.frame_size_bytes, 160, "128 private bytes plus 32 outgoing bytes");
        let start = valid.record.blocks.iter().find(|block| block.id == adapter.entry_block).unwrap().range.start;
        let saved_ra = elf.instructions.iter().find(|instruction| instruction.address == start + 4).unwrap();
        let mut changed = valid.clone();
        // Move saved ra from sp+120 to sp+112, into the selected payload.
        // Rebind hashes so rejection depends on the independent frame contract.
        changed.replace_machine_word(saved_ra.address, saved_ra.word ^ (1 << 10));
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.message.contains("private copy frame"), "{error}");
    }
}

fn is_add(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

#[test]
fn raw_builder_param_mutations_reject_despite_rebound_outer_identities() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in [
        "name",
        "type",
        "source",
        "mut",
        "ref",
        "cell-skip",
        "scalar-skip",
        "lock-args",
        "schema",
        "fixed-width",
        "fixed-flag",
        "hash-flag",
        "bounded",
    ] {
        let mut changed = fixture.clone();
        match mutation {
            "name" => changed.param_mut("mint", 0)["name"] = "other".into(),
            "type" => changed.param_mut("mint", 0)["ty"] = "u128".into(),
            "source" => changed.param_mut("mint", 0)["source"] = "lock_args".into(),
            "mut" => changed.param_mut("mint", 0)["is_mut"] = true.into(),
            "ref" => changed.param_mut("mint", 0)["is_ref"] = true.into(),
            "cell-skip" => changed.param_mut("burn", 0)["cell_bound_abi"] = false.into(),
            "scalar-skip" => changed.param_mut("mint", 0)["cell_bound_abi"] = true.into(),
            "lock-args" => changed.param_mut("mint", 0)["lock_args_data_source"] = true.into(),
            "schema" => changed.param_mut("mint", 0)["schema_pointer_abi"] = true.into(),
            "fixed-width" => changed.param_mut("mint", 1)["fixed_byte_len"] = 31.into(),
            "fixed-flag" => changed.param_mut("mint", 1)["fixed_byte_length_abi"] = false.into(),
            "hash-flag" => changed.param_mut("burn", 0)["type_hash_pointer_abi"] = true.into(),
            "bounded" => changed.param_mut("burn", 0)["bounded_runtime_contract"] = "type-group-inputs-v1".into(),
            _ => unreachable!(),
        }
        changed.rebind_sidecars();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("policy builder parameter"), "{mutation}: {error}");
    }
}

#[test]
fn raw_policy_declaration_and_outer_abi_mutations_reject_after_rebinding() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in
        ["tag", "action", "common-order", "resource", "name", "records", "bytes", "payload", "placement", "field", "source", "missing"]
    {
        let mut changed = fixture.clone();
        let policy = &mut changed.metadata["runtime"]["policy_artifact"];
        match mutation {
            "tag" => policy["declaration"]["actions"][0]["tag"] = 11.into(),
            "action" => policy["declaration"]["actions"][0]["action"] = "burn".into(),
            "common-order" => policy["declaration"]["common_checks"].as_array_mut().unwrap().swap(0, 1),
            "resource" => policy["declaration"]["context"]["resource"] = "OtherToken".into(),
            "name" => policy["declaration"]["name"] = "other-policy".into(),
            "records" => policy["max_records"] = 9.into(),
            "bytes" => policy["max_witness_bytes"] = 4097.into(),
            "payload" => policy["payload_abi"] = "cellscript-entry-witness-v1".into(),
            "placement" => policy["placement_abi"] = "raw".into(),
            "field" => policy["placement_field"] = "lock".into(),
            "source" => policy["placement_source"] = "input[0]".into(),
            "missing" => {
                changed.metadata["runtime"].as_object_mut().unwrap().remove("policy_artifact");
            }
            _ => unreachable!(),
        }
        changed.rebind_sidecars();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("runtime.policy_artifact"), "{mutation}: {error}");
    }
}

#[test]
fn typed_policy_counts_payload_identity_and_selector_require_concrete_evidence() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in ["input-count", "output-count", "payload-schema", "selector-content", "selector-label", "unknown", "wrapper"] {
        let mut changed = fixture.clone();
        match mutation {
            "input-count" => changed.policy_mut().variants[0].input_count = 1,
            "output-count" => changed.policy_mut().variants[0].output_count = 2,
            "payload-schema" => changed.policy_mut().variants[0].payload_schema_hash = "unbound-params".into(),
            "selector-label" => changed.policy_mut().selector_node_id = "caller-chosen-label".into(),
            "selector-content" => {
                let id = changed.policy_mut().selector_node_id.clone();
                let node = changed.record.typed_semantics.foundation.provenance.nodes.iter_mut().find(|node| node.id == id).unwrap();
                let ValueProvenance::EntryWitness { field_path, .. } = &mut node.provenance else { panic!("selector root") };
                *field_path = "input_type.unauthenticated_tag".into();
                node.id = canonical_hash("cellscript-value-provenance-node-v1", &node.provenance).unwrap();
                let replacement = node.id.clone();
                changed.policy_mut().selector_node_id = replacement;
            }
            "unknown" => changed.policy_mut().unknown_selector = "accept".into(),
            "wrapper" => changed.record.typed_semantics.foundation.entry_contract.exact_entry = "action:mint".into(),
            _ => unreachable!(),
        }
        changed.rebind_policy_identity();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{mutation}: {error}");
    }
}

#[test]
fn interface_callable_signature_cannot_be_forged_after_outer_hash_rebinding() {
    let source =
        SOURCE.replace("require amount > 0;", "require echo(amount) > 0;") + "\npublic fn echo(value: u64) -> u64 { value }\n";
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.entries.iter().any(|entry| entry.name == "echo" && entry.kind == "helper"));
    for mutation in
        ["same-width-type", "name", "source", "mutable", "reference", "omit", "duplicate", "order", "output", "return", "kind"]
    {
        let mut changed = baseline.clone();
        let callable = changed.metadata["public_interface"]["callables"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["name"] == "mint")
            .unwrap();
        match mutation {
            "same-width-type" => callable["params"][1]["type"] = "Hash".into(),
            "name" => callable["params"][1]["name"] = "substituted".into(),
            "source" => callable["params"][1]["source"] = "default".into(),
            "mutable" => callable["params"][1]["mutable"] = true.into(),
            "reference" => callable["params"][1]["reference"] = true.into(),
            "omit" => {
                callable["params"].as_array_mut().unwrap().pop();
            }
            "duplicate" => {
                let param = callable["params"][1].clone();
                callable["params"].as_array_mut().unwrap().push(param);
            }
            "order" => callable["params"].as_array_mut().unwrap().reverse(),
            "output" => {
                callable["outputs"] = serde_json::json!([{
                    "name": "forged", "type": "Token", "source": "output", "mutable": false, "reference": false
                }])
            }
            "return" => callable["return_type"] = "u64".into(),
            "kind" => callable["kind"] = "function".into(),
            _ => unreachable!(),
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
        assert!(
            error.message.starts_with("interface callable ") && error.message.ends_with("differs from checked entry"),
            "{mutation}: {error}"
        );
    }
    let mut changed = baseline.clone();
    let callable = changed.metadata["public_interface"]["callables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "echo")
        .unwrap();
    callable["return_type"] = "BlockNumber".into();
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
    assert_eq!(error.message, "interface callable return type differs from checked entry");
}

fn fixed_result_fixture(diagnostics: cellscript::CompilerDiagnostics) -> Fixture {
    let source = r#"
module checked_result;
struct Pair { left: u64, right: u64, }
fn build(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, value: Pair) -> Pair {
    new Pair { left: value.right + a + b + c + d, right: value.left + e + f + g + h, }
}
action verify(witness value: Pair, witness expected: Pair) {

    let result = build(1, 2, 3, 4, 5, 6, 7, 8, value);
    require result.left == expected.left;
    require result.right == expected.right;
}
"#;
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions { source_contracts: true, diagnostics, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let fixture = Fixture {
        artifact: compiled.artifact_bytes,
        metadata: serde_json::to_value(compiled.metadata).unwrap(),
        record: compiled.verified_lowering_record.unwrap(),
        source_map: compiled.source_artifact_map.unwrap(),
    };
    fixture.check().unwrap();
    fixture
}

#[test]
fn fixed_result_contracts_reject_rebound_placement_extent_and_coverage_mutations() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixed_result_fixture(diagnostics);
        for mutation in 0..19 {
            let mut changed = valid.clone();
            let helper = changed.record.entries.iter().position(|entry| entry.name == "build").unwrap();
            let caller = changed.record.entries.iter().position(|entry| entry.name == "verify").unwrap();
            match mutation {
                0 => changed.record.entries[helper].fixed_result_abi = None,
                1 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().hidden_argument_index -= 1,
                2 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().width_bytes -= 1,
                3 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().saved_pointer_offset += 8,
                4 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().type_name = "u64".into(),
                5 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_ranges.clear(),
                6 => changed.record.entries[caller].fixed_result_calls.clear(),
                7 => changed.record.entries[caller].fixed_result_calls[0].hidden_argument_index += 1,
                8 => changed.record.entries[caller].fixed_result_calls[0].outgoing_stack_bytes += 16,
                9 => changed.record.entries[caller].fixed_result_calls[0].buffer_offset = 0,
                10 => {
                    changed.record.entries[caller].fixed_result_calls[0].buffer_offset =
                        changed.record.entries[caller].frame_size_bytes
                }
                11 => changed.record.entries[caller].fixed_result_calls[0].buffers[0].width_bytes = u32::MAX,
                12 => changed.record.entries[caller].fixed_result_calls[0].receive_range.end = u64::MAX - 3,
                13 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().schema = "unknown-result-abi".into(),
                14 => {
                    changed.record.entries[caller].fixed_result_calls[0].pointer_slot_offset =
                        changed.record.entries[caller].fixed_result_calls[0].buffer_offset
                }
                15 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources.clear(),
                16 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].base_local = u64::MAX,
                17 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].field_offset = 8,
                18 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].source_local = u64::MAX,
                _ => unreachable!(),
            }
            changed.rebind_sidecars();
            let error = changed.check().expect_err("rebound fixed result contract must fail independently");
            assert_eq!(error.code, CheckerRejectionCode::V2407AbiOrStackInvalid, "{diagnostics}, mutation {mutation}: {error}");
            assert!(error.message.contains("fixed struct result ABI"), "{error}");
        }
    }
}

#[test]
fn fixed_result_machine_mutations_reject_after_elf_and_sidecars_are_rebound() {
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = fixed_result_fixture(diagnostics);
        let helper = valid.record.entries.iter().find(|entry| entry.name == "build").unwrap();
        let abi = helper.fixed_result_abi.as_ref().unwrap();
        let call = &valid.record.entries.iter().find(|entry| entry.name == "verify").unwrap().fixed_result_calls[0];
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let word_in = |range: cellscript_artifact_checker::MachineRange, opcode: u32, rd: Option<u32>| {
            *elf.instructions
                .iter()
                .find(|instruction| {
                    range.contains(instruction.address)
                        && instruction.word & 0x7f == opcode
                        && rd.is_none_or(|rd| (instruction.word >> 7) & 31 == rd)
                })
                .unwrap()
        };
        let save_load = word_in(abi.save_range, 0x03, Some(5));
        let save_store = word_in(abi.save_range, 0x23, None);
        let copy_load = word_in(abi.copy_ranges[0], 0x03, Some(11));
        let source_load = word_in(abi.copy_sources[0].range, 0x03, Some(10));
        let copy_length = word_in(abi.copy_ranges[0], 0x13, Some(12));
        let outgoing_store = word_in(call.setup_range, 0x23, None);
        let receive_pointer = word_in(call.receive_range, 0x13, Some(5));
        let cases = [
            (source_load.address, source_load.word ^ (8 << 20)),
            (source_load.address, (source_load.word & !(31 << 7)) | (11 << 7)),
            (save_load.address, save_load.word ^ (8 << 20)),
            (save_store.address, save_store.word ^ (8 << 7)),
            (copy_load.address, copy_load.word ^ (8 << 20)),
            (copy_load.address, (copy_load.word & !(31 << 7)) | (10 << 7)),
            (copy_length.address, copy_length.word ^ (1 << 20)),
            (outgoing_store.address, outgoing_store.word ^ (8 << 7)),
            (receive_pointer.address, (receive_pointer.word & !(31 << 15)) | (10 << 15)),
        ];
        for (address, word) in cases {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            let error = changed.check().expect_err("machine ABI mutation must fail after complete hash rebinding");
            assert_eq!(error.code, CheckerRejectionCode::V2407AbiOrStackInvalid, "{diagnostics}, address {address:#x}: {error}");
            assert!(error.message.contains("fixed struct result ABI"), "{error}");
        }
    }
}

#[test]
fn exact_handle_contract_cannot_be_reclassified_after_hash_rebinding() {
    let fixture = Fixture::from_result(
        compile(
            EXACT_HANDLE_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );

    let mut changed = fixture.clone();
    exact_handle_operation(&mut changed).call.as_mut().unwrap().target = "__foreign_exact_handle".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    exact_handle_operation(&mut changed).call.as_mut().unwrap().contract = "typed-local".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    let operation = exact_handle_operation(&mut changed);
    operation.operands[2].ty = "address".to_string();
    operation.operands[2].constant = Some(TypedSemanticConstant::Address("00".repeat(32)));
    operation.call.as_mut().unwrap().params[2] = "address".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture;
    let operation = exact_handle_operation(&mut changed);
    let Some(TypedSemanticConstant::Hash(hash)) = operation.operands[2].constant.as_mut() else {
        panic!("expected exact-handle hash constant")
    };
    hash.replace_range(0..2, "ff");
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn deployment_line_handle_contract_cannot_be_reclassified_after_hash_rebinding() {
    let fixture = Fixture::from_result(
        compile(
            DEPLOYMENT_LINE_HANDLE_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert!(check_bundle_values(
        &fixture.artifact,
        &fixture.metadata,
        &fixture.record,
        &fixture.source_map,
        &CheckerBudgets::default(),
    )
    .is_ok());

    let mut changed = fixture.clone();
    deployment_line_handle_operation(&mut changed).call.as_mut().unwrap().target = "__foreign_deployment_line".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    deployment_line_handle_operation(&mut changed).call.as_mut().unwrap().contract = "typed-local".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture.clone();
    let operation = deployment_line_handle_operation(&mut changed);
    operation.operands[2].ty = "ExactScriptHandle".to_string();
    operation.call.as_mut().unwrap().params[2] = "ExactScriptHandle".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = fixture;
    let operation = deployment_line_handle_operation(&mut changed);
    operation.operands[3].ty = "address".to_string();
    operation.operands[3].constant = Some(TypedSemanticConstant::Address("00".repeat(32)));
    operation.call.as_mut().unwrap().params[3] = "address".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_an_explicit_versioned_dispatch_contract() {
    let mut fixture = Fixture::new();
    let foundation = &mut fixture.record.typed_semantics.foundation;
    let selector_node_id = foundation
        .provenance
        .nodes
        .iter()
        .find(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
        .expect("fixture must have a provenance root")
        .id
        .clone();
    let exact_entry = foundation.entry_contract.exact_entry.clone();
    let previous_contract_node = foundation.entry_contract.semantic_node_id.clone();
    foundation.entry_contract.dispatch = cellscript_artifact_checker::EntryDispatchContract::ExplicitVersionedDispatch {
        selector_node_id,
        selector_type: "u32-le".to_string(),
        variants: vec![cellscript_artifact_checker::EntryDispatchVariant { tag: "0".to_string(), entry_id: exact_entry.clone() }],
        unknown_selector: "reject".to_string(),
    };
    foundation.entry_contract.semantic_node_id = canonical_hash(
        "cellscript-semantic-node-entry-contract-v1",
        &(
            foundation.entry_contract.script_role.as_str(),
            foundation.entry_contract.trigger.as_str(),
            exact_entry.as_str(),
            "explicit-versioned-dispatch",
            foundation.entry_contract.entry_payload_abi.as_str(),
            foundation.entry_contract.witness_placement_abi.as_str(),
            foundation.entry_contract.witness_placement_field.as_str(),
            foundation.entry_contract.witness_placement_source.as_str(),
        ),
    )
    .unwrap();
    let roots = foundation
        .provenance
        .nodes
        .iter()
        .filter(|node| !matches!(node.provenance, cellscript_artifact_checker::ValueProvenance::Derived { .. }))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    foundation.identities.entry_contract_id = canonical_hash(
        "cellscript-entry-contract-id-v1",
        &(
            foundation.identities.core_semantic_id.as_str(),
            &foundation.entry_contract,
            roots,
            foundation.entry_contract.entry_payload_abi.as_str(),
            foundation.entry_contract.witness_placement_abi.as_str(),
        ),
    )
    .unwrap();
    foundation.identities.artifact_contract_id = canonical_hash(
        "cellscript-artifact-contract-id-v1",
        &(foundation.identities.entry_contract_id.as_str(), &foundation.artifact_contract),
    )
    .unwrap();
    let replacement_contract_node = foundation.entry_contract.semantic_node_id.clone();
    for mapping in &mut fixture.source_map.semantic_mappings {
        if mapping.semantic_node_id == previous_contract_node {
            mapping.semantic_node_id.clone_from(&replacement_contract_node);
        }
    }
    fixture.source_map.canonicalize();
    fixture.metadata["verified_artifact"]["entry_contract_id"] = Value::String(foundation.identities.entry_contract_id.clone());
    fixture.metadata["verified_artifact"]["artifact_contract_id"] = Value::String(foundation.identities.artifact_contract_id.clone());
    fixture.rebind_layered_identities();

    assert!(fixture.check().is_ok());
}

#[test]
fn stable_rejection_codes_cover_json_budget_graph_abi_proof_and_binding_mutations() {
    let valid = Fixture::new();
    let budgets = CheckerBudgets::default();
    let metadata_bytes = serde_json::to_vec(&valid.metadata).unwrap();
    let record_bytes = canonical_bytes(&valid.record).unwrap();
    let source_map_bytes = canonical_bytes(&valid.source_map).unwrap();

    let mut tiny = budgets.clone();
    tiny.artifact_bytes = 1;
    assert_eq!(
        check_bundle(&valid.artifact, &metadata_bytes, &record_bytes, &source_map_bytes, &tiny).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded,
    );
    assert_eq!(
        check_bundle(&valid.artifact, &metadata_bytes, b"{", &source_map_bytes, &budgets).unwrap_err().code,
        CheckerRejectionCode::V2401MalformedJson,
    );
    assert_eq!(
        check_bundle(
            &valid.artifact,
            &metadata_bytes,
            &serde_json::to_vec_pretty(&valid.record).unwrap(),
            &source_map_bytes,
            &budgets,
        )
        .unwrap_err()
        .code,
        CheckerRejectionCode::V2402NonCanonicalJson,
    );

    let mut changed = valid.clone();
    changed.record.schema = "future-schema".to_string();
    assert_code(&changed, CheckerRejectionCode::V2403UnsupportedSchema);

    let mut changed = valid.clone();
    changed.record.entries[0].id = "zz-noncanonical".to_string();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2404CanonicalOrder);

    let mut changed = valid.clone();
    changed.record.entries[0].entry_block = "missing:block".to_string();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2405ReferentialIntegrity);

    let mut changed = valid.clone();
    let index = changed
        .record
        .blocks
        .iter()
        .position(|block| changed.record.edges.iter().any(|edge| edge.from == block.id && edge.kind != EdgeKind::Call))
        .expect("fixture must contain a non-return CFG edge");
    changed.record.blocks[index].terminator = cellscript_artifact_checker::MachineTerminator::Return;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    changed.record.runtime_error_exits.push(cellscript_artifact_checker::RuntimeErrorExit {
        block_id: changed.record.blocks[0].id.clone(),
        address: changed.record.blocks[0].range.end,
        code: 5,
        name: "assertion-failed".to_string(),
    });
    changed.record.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    changed.record.blocks[0].reachable = !changed.record.blocks[0].reachable;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2406CfgInvalid);

    let mut changed = valid.clone();
    let param = changed.record.entries.iter_mut().find_map(|entry| entry.params.first_mut()).unwrap();
    param.alignment_bytes = 3;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);

    let mut changed = valid.clone();
    let framed_entry =
        changed.record.entries.iter().position(|entry| entry.frame_size_bytes > 0).expect("fixture must contain a stack-framed entry");
    let owner = changed.record.entries[framed_entry].id.clone();
    changed.record.entries[framed_entry].frame_size_bytes = 0;
    changed.record.entries[framed_entry].outgoing_argument_bytes = 0;
    for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == owner) {
        block.frame_size_bytes = 0;
        block.outgoing_argument_bytes = 0;
        block.stack_slots.clear();
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);

    let mut changed = valid.clone();
    changed.record.entries[0].proof_ids.push("zz-missing-proof".to_string());
    changed.record.entries[0].proof_ids.sort();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2408ProofCoverageInvalid);

    let mut changed = valid.clone();
    changed.artifact[0] ^= 1;
    assert_code(&changed, CheckerRejectionCode::V2409ArtifactIdentityMismatch);

    let mut changed = valid.clone();
    changed.metadata["module"] = Value::String("tampered".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].effects = cellscript_artifact_checker::effect_facts::EffectFacts::process(
        cellscript_artifact_checker::effect_facts::ProcessOperation::Wait,
    );
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let typed_param = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .find_map(|entry| entry.params.first_mut())
        .expect("fixture must contain a typed parameter");
    typed_param.ty = "u128".to_string();
    let binding_id = typed_param.binding_id;
    let typed_local = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| entry.locals.iter_mut())
        .find(|local| local.id == binding_id)
        .expect("typed parameter must bind a local");
    typed_local.ty = "u128".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    if let Some(interval) = changed.source_map.intervals.first_mut() {
        interval.source_path = "../escape.cell".to_string();
    } else {
        changed.source_map.schema = "bad-map".to_string();
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2416SourceMapInvalid);

    let mut changed = valid.clone();
    if let Some(site) = changed.record.syscall_sites.first_mut() {
        site.contract.clear();
    } else {
        changed.record.syscall_sites.push(cellscript_artifact_checker::SyscallSite {
            block_id: changed.record.blocks[0].id.clone(),
            address: changed.record.blocks[0].range.start,
            syscall_number: None,
            contract: "declared-but-not-present".to_string(),
            source_domain: "test".to_string(),
            index_domain: "test".to_string(),
            return_code_checked: true,
            buffer_limit_bytes: 1,
        });
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);

    let mut changed = valid.clone();
    let distinct = changed
        .record
        .entries
        .iter()
        .enumerate()
        .find_map(|(left, a)| changed.record.entries.iter().enumerate().find(|(_, b)| b.id != a.id).map(|(right, _)| (left, right)))
        .unwrap();
    let left = changed.record.entries[distinct.0].entry_block.clone();
    let right = changed.record.entries[distinct.1].entry_block.clone();
    changed.record.edges.push(cellscript_artifact_checker::LoweringEdge {
        from: left.clone(),
        to: right.clone(),
        kind: EdgeKind::Call,
    });
    changed.record.edges.push(cellscript_artifact_checker::LoweringEdge { from: right, to: left, kind: EdgeKind::Call });
    changed.record.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2418RecursionPolicyInvalid);
}

#[test]
fn stable_rejection_codes_cover_elf_sections_instructions_flow_and_digests() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    changed.artifact[0] = 0;
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2411ElfFormatInvalid);

    let mut changed = valid.clone();
    let section_table = u64::from_le_bytes(changed.artifact[40..48].try_into().unwrap()) as usize;
    let rodata_type = section_table + 2 * 64 + 4;
    changed.artifact[rodata_type..rodata_type + 4].copy_from_slice(&6_u32.to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2412ElfSectionInvalid);

    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let text_offset = elf.text.offset as usize;

    let mut changed = valid.clone();
    changed.artifact[text_offset..text_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2413InstructionInvalid);

    let mut changed = valid.clone();
    changed.artifact[text_offset..text_offset + 4].copy_from_slice(&encode_jal(1_048_574).to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);

    let mut changed = valid.clone();
    let candidate = elf
        .instructions
        .windows(2)
        .find(|pair| {
            let word = pair[0].word;
            let rd = (word >> 7) & 0x1f;
            let next = pair[1].word;
            let next_uses_rd_for_sp =
                next & 0x7f == 0x33 && (next >> 7) & 0x1f == 2 && (next >> 15) & 0x1f == 2 && (next >> 20) & 0x1f == rd;
            valid.record.text_range.contains(pair[0].address)
                && word & 0x7f == 0x13
                && rd != 2
                && !next_uses_rd_for_sp
                && valid
                    .record
                    .blocks
                    .iter()
                    .any(|block| block.range.contains(pair[0].address) && pair[0].address + 4 < block.range.end)
        })
        .map(|pair| pair[0])
        .expect("fixture must contain a non-terminating add-immediate instruction");
    let block_offset = elf.text.offset as usize + (candidate.address - elf.text.address) as usize;
    changed.artifact[block_offset..block_offset + 4].copy_from_slice(&(candidate.word ^ (1 << 20)).to_le_bytes());
    changed.bind_artifact_identity();
    assert_code(&changed, CheckerRejectionCode::V2415BlockDigestMismatch);
}

#[test]
fn named_data_shapes_retain_independent_codec_bounds_and_source_checks() {
    let unused = Fixture::from_source("module unused_nested_schema;\nresource Batch has store { entries: NestedValues<Vec<u8>, 3, 8, 128>, }\naction inspect() -> u64 {  return 0; }");
    assert!(
        unused.record.blocks.iter().all(|block| block.nested_cell_labels.is_empty() && block.nested_read_labels.is_empty()),
        "a declaration alone must not imply runtime nested decoding"
    );
    for source in [
        NESTED_CELL_CODEC_SOURCE,
        "module unused_nested_field;\nresource Batch has store, consume { entries: NestedValues<Vec<u8>, 2, 3, 32>, }\naction remove(input before: Batch) {  consume before; }",
    ] {
        assert_bounded_admitted(&Fixture::from_source(source));
    }
    let source = include_str!("../syntax_combo/seeds/named-data-shapes.cell");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let valid = Fixture::from_result(
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
        for old in ["NestedValues<Vec<u8>, 3, 8, 128>", "NestedValues<Entry, 3, 8, 128>"] {
            for new in [old.replace(", 3,", ", 2,"), old.replace(", 8,", ", 7,"), old.replace("128", "127")] {
                let mut changed = valid.clone();
                let mut parameters_changed = 0;
                for entry in &mut changed.record.typed_semantics.entries {
                    for param in &mut entry.params {
                        if param.ty == old {
                            param.ty = new.clone();
                            parameters_changed += 1;
                        }
                    }
                    for local in &mut entry.locals {
                        if local.ty == old {
                            local.ty = new.clone();
                        }
                    }
                    for operation in entry.blocks.iter_mut().flat_map(|block| &mut block.operations) {
                        for operand in &mut operation.operands {
                            if operand.ty == old {
                                operand.ty = new.clone();
                            }
                        }
                    }
                }
                assert!(parameters_changed > 0, "the resolved parameter shape must be present: {old}");
                changed.rebind_layered_identities();
                assert!(changed.check().is_err(), "accepted changed resolved shape {new}");
            }
        }
        let start = valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lnested_param_codec_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let word = elf.instructions.iter().find(|instruction| instruction.address == start).unwrap().word;
        let mut changed = valid.clone();
        changed.replace_machine_word(start, word ^ (1 << 20));
        assert!(changed.check().is_err(), "named shape must not bypass the codec instruction proof");
        assert_bounded_admitted(&valid);
    }
}

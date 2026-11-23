#[test]
fn allocated_values_keep_structured_contract_after_admission_and_rebinding() {
    let source = "module allocated_proof;\ncollections Work { limits { bytes: 128, allocations: 2 },\nvalues: Vec<u64> { items: 8 } }\nfn count(view: &Work) -> u64 { return view.values.len(); }\naction inspect() -> u64 { let mut work = Work::new();\nwork.values.push(9);\nrequire (work.values.get(0) == 9);\nrequire (count(&work) == 1);\nreturn 0; }";
    let valid = Fixture::from_result(
        compile(source, CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() })
            .unwrap(),
    );
    valid.check().unwrap();
    assert_bounded_admitted(&valid);
    for mutation in 0..4 {
        let mut changed = valid.clone();
        let op = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. }))
            .unwrap();
        let TypedSemanticOperationDetail::AllocatedValue { contract, member, operation } = &mut op.detail else { unreachable!() };
        match mutation {
            0 => contract.bytes = 63,
            1 => contract.allocations = 0,
            2 => *member = Some(99),
            _ => *operation = cellscript_artifact_checker::allocated_values::Operation::Drop,
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    let mut drift = valid.clone();
    let op = drift
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|op| matches!(op.detail, TypedSemanticOperationDetail::AllocatedValue { .. }))
        .unwrap();
    if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &mut op.detail {
        contract.bytes = 256;
    }
    drift.rebind_layered_identities();
    assert_code(&drift, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    let mut callee_drift = valid;
    let helper = callee_drift.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "count").unwrap();
    for op in helper.blocks.iter_mut().flat_map(|block| &mut block.operations) {
        if let TypedSemanticOperationDetail::AllocatedValue { contract, .. } = &mut op.detail {
            contract.bytes = 256;
        }
    }
    callee_drift.rebind_layered_identities();
    assert_code(&callee_drift, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn fixed_byte_result_contract_rejects_rebound_width_and_missing_copy() {
    let source = "module byte_copy_proof;\nfn bytes() -> [u8; 9] { return b\"constant!\"; }\naction inspect() -> u64 { require bytes() == b\"constant!\";\nreturn 0; }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                edition: CURRENT_EDITION,
                diagnostics: cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    valid.check().unwrap();
    for mutation in 0..4 {
        let mut changed = valid.clone();
        let entry = changed.record.entries.iter_mut().find(|entry| entry.name == "bytes").unwrap();
        match mutation {
            0 => entry.fixed_result_abi = None,
            1 => entry.fixed_result_abi.as_mut().unwrap().width_bytes = 8,
            2 => entry.fixed_result_abi.as_mut().unwrap().copy_ranges.clear(),
            _ => entry.fixed_result_abi.as_mut().unwrap().type_name = "[u8; 8]".into(),
        }
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2407AbiOrStackInvalid);
    }
}

#[test]
fn explicit_hash_constructor_rejects_implicit_retyping_extra_fields_and_wrong_copy_source() {
    let source = "module hash_constructor;\nfn make(bytes: [u8; 32]) -> Hash { return Hash::from_bytes(bytes); }\naction inspect(witness bytes: [u8; 32]) -> u64 {  require make(bytes) == Hash::from_bytes(b\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"); return 0; }";
    let valid = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::default().without(cellscript::OptimizationPass::Inlining),
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let constructor = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .enumerate()
        .find_map(|(entry_index, entry)| {
            entry.blocks.iter().enumerate().find_map(|(block_index, block)| {
                block.operations.iter().enumerate().find_map(|(operation_index, operation)| {
                    (operation.opcode == "tuple"
                        && operation.destinations.len() == 1
                        && entry.locals.iter().any(|local| local.id == operation.destinations[0] && local.ty == "hash"))
                    .then_some((entry_index, block_index, operation_index))
                })
            })
        })
        .expect("explicit constructor retained in typed semantics");
    for extra_field in [false, true] {
        let mut changed = valid.clone();
        let operation = &mut changed.record.typed_semantics.entries[constructor.0].blocks[constructor.1].operations[constructor.2];
        if extra_field {
            operation.operands.push(operation.operands[0].clone());
        } else {
            operation.opcode = "move".into();
        }
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
    let abi = valid.record.entries.iter().find(|entry| entry.name == "make").unwrap().fixed_result_abi.as_ref().unwrap();
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let load = elf
        .instructions
        .iter()
        .find(|instruction| {
            abi.copy_sources.iter().any(|source| source.range.contains(instruction.address))
                && instruction.word & 0x7f == 0x03
                && (instruction.word >> 7) & 31 == 10
        })
        .expect("owned result source-pointer load");
    let mut changed = valid.clone();
    changed.replace_machine_word(load.address, load.word & !(31 << 15));
    assert!(changed.check().is_err(), "absolute memory cannot replace the proved owned source pointer");
}

#[test]
fn committed_state_typed_call_contracts_cannot_be_relabelled_after_rebinding() {
    let valid = Fixture::from_source(COMMITTED_STATE_SOURCE);
    let calls = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter_map(|operation| operation.call.as_ref())
        .map(|call| call.target.as_str())
        .collect::<Vec<_>>();
    assert!(calls.contains(&"__ckb_commitment_open"));
    assert!(calls.contains(&"__ckb_hash_blake2b_packed"));

    let mut changed_opening = valid.clone();
    let opening = changed_opening
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| operation.call.as_mut().filter(|call| call.target == "__ckb_commitment_open"))
        .expect("typed commitment opening call");
    opening.params[1] = "Opening<u64>".to_string();
    changed_opening.rebind_layered_identities();
    assert_code(&changed_opening, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed_commit = valid;
    let commit = changed_commit
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| {
            operation
                .call
                .as_mut()
                .filter(|call| call.target == "__ckb_hash_blake2b_packed" && call.return_type.starts_with("Commitment<"))
        })
        .expect("typed commitment construction call");
    commit.params[0] = "u64".to_string();
    changed_commit.rebind_layered_identities();
    assert_code(&changed_commit, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn committed_state_machine_blocks_reject_rebound_removal_reordering_and_instruction_mutations() {
    let valid = Fixture::from_source(COMMITTED_STATE_SOURCE);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let marker = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| {
                block.owner_entry == "action:inspect" && block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix))
            })
            .expect("committed-state marker")
    };
    let opening = marker(".Lcommitment_opening_header_ready_");
    let authenticated = marker(".Lcommitment_opening_verified_");
    let recommit = marker(".Lcommitment_commit_header_ready_");
    assert!(opening.range.start < authenticated.range.start && authenticated.range.start < recommit.range.start);

    let runtime_target = |name: &str| {
        let entry = valid.record.entries.iter().find(|entry| entry.id == format!("runtime:{name}")).expect("runtime entry");
        valid.record.blocks.iter().find(|block| block.id == entry.entry_block).expect("runtime entry block").range.start
    };
    let calls = |name: &str, start: u64, end: u64| {
        let target = runtime_target(name);
        elf.control_flow
            .iter()
            .filter(|flow| start <= flow.address && flow.address < end && flow.target == target)
            .map(|flow| flow.address)
            .collect::<Vec<_>>()
    };
    let opening_hash = calls("__ckb_hash_blake2b_var", opening.range.start, authenticated.range.start)[0];
    let opening_compare = calls("__cellscript_memcmp_fixed", opening.range.start, authenticated.range.start)[0];
    let recommit_hash = calls("__ckb_hash_blake2b_var", recommit.range.start, recommit.range.end + 512)[0];
    let word_at = |address| elf.instructions.iter().find(|instruction| instruction.address == address).expect("machine word").word;
    let last_immediate_definition = |address: u64, register: u32| {
        elf.instructions
            .iter()
            .filter(|instruction| instruction.address < address)
            .rev()
            .find(|instruction| {
                instruction.word & 0x7f == 0x13
                    && (instruction.word >> 12) & 0x7 == 0
                    && (instruction.word >> 7) & 0x1f == register
                    && (instruction.word >> 15) & 0x1f == 0
            })
            .expect("immediate argument definition")
            .address
    };

    let mut changed = valid.clone();
    changed.record.blocks.iter_mut().find(|block| block.id == opening.id).unwrap().machine_label = None;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let mut changed = valid.clone();
    let opening_index = changed.record.blocks.iter().position(|block| block.id == opening.id).unwrap();
    let recommit_index = changed.record.blocks.iter().position(|block| block.id == recommit.id).unwrap();
    let opening_label = changed.record.blocks[opening_index].machine_label.take();
    let recommit_label = changed.record.blocks[recommit_index].machine_label.take();
    changed.record.blocks[opening_index].machine_label = recommit_label;
    changed.record.blocks[recommit_index].machine_label = opening_label;
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    for marker in [opening, recommit] {
        let mut changed = valid.clone();
        let address = marker.range.start - 8;
        changed.replace_machine_word(address, word_at(address) ^ (1 << 20));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    for (call, register) in [(opening_hash, 11), (opening_compare, 12), (recommit_hash, 11)] {
        let mut changed = valid.clone();
        let address = last_immediate_definition(call, register);
        let immediate = (word_at(address) as i32) >> 20;
        changed.replace_machine_word(address, replace_i_immediate(word_at(address), immediate + 1));
        assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }

    let mut changed = valid.clone();
    changed.replace_machine_word(opening_compare + 4, word_at(opening_compare + 4) ^ (1 << 12));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

    let opening_destination = valid
        .record
        .typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_commitment_open"))
        .and_then(|operation| operation.destinations.first())
        .copied()
        .unwrap();
    let destination_offset = i32::try_from(opening_destination * 8).unwrap();
    let destination_store = elf
        .instructions
        .iter()
        .find(|instruction| {
            authenticated.range.start <= instruction.address
                && instruction.address < recommit.range.start
                && instruction.word & 0x7f == 0x23
                && (instruction.word >> 12) & 0x7 == 3
                && (instruction.word >> 20) & 0x1f == 5
                && (instruction.word >> 15) & 0x1f == 2
                && decode_s_immediate(instruction.word) == destination_offset
        })
        .expect("authenticated opening destination store");
    let mut changed = valid;
    changed.replace_machine_word(destination_store.address, replace_s_immediate(destination_store.word, destination_offset + 8));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn committed_state_machine_binds_schema_field_pointer_and_offset() {
    let valid = Fixture::from_source(COMMITTED_STATE_FIELD_SOURCE);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let opening = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "action:inspect"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_header_ready_"))
        })
        .expect("committed-state field marker");
    let authenticated = valid
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "action:inspect"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lcommitment_opening_verified_"))
        })
        .expect("committed-state field authenticated marker");
    let runtime = valid.record.entries.iter().find(|entry| entry.id == "runtime:__cellscript_memcmp_fixed").unwrap();
    let target = valid.record.blocks.iter().find(|block| block.id == runtime.entry_block).unwrap().range.start;
    let compare = elf
        .control_flow
        .iter()
        .find(|flow| opening.range.start <= flow.address && flow.address < authenticated.range.start && flow.target == target)
        .expect("committed-state field comparison")
        .address;
    let field_offset = elf
        .instructions
        .iter()
        .filter(|instruction| opening.range.start <= instruction.address && instruction.address < compare)
        .rev()
        .find(|instruction| {
            instruction.word & 0x7f == 0x13
                && (instruction.word >> 12) & 0x7 == 0
                && (instruction.word >> 7) & 0x1f == 11
                && (instruction.word >> 15) & 0x1f == 11
                && (instruction.word as i32 >> 20) == 8
        })
        .expect("typed field pointer offset");

    let mut changed = valid;
    changed.replace_machine_word(field_offset.address, replace_i_immediate(field_offset.word, 16));
    assert_code(&changed, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
}

#[test]
fn verified_artifact_sidecars_are_deterministic_and_canonical() {
    let first = Fixture::new();
    let second = Fixture::new();
    assert_eq!(first.artifact, second.artifact);
    assert_eq!(canonical_bytes(&first.record).unwrap(), canonical_bytes(&second.record).unwrap());
    assert_eq!(canonical_bytes(&first.source_map).unwrap(), canonical_bytes(&second.source_map).unwrap());
    assert!(first.source_map.intervals.iter().all(|interval| interval.source_path == "<memory>"));
    let foundation = &first.record.typed_semantics.foundation;
    assert_eq!(foundation.schema, cellscript_artifact_checker::SEMANTIC_FOUNDATION_SCHEMA);
    assert!(!foundation.identities.core_semantic_id.is_empty());
    assert!(!foundation.identities.entry_contract_id.is_empty());
    assert!(!foundation.identities.artifact_contract_id.is_empty());
    assert_eq!(first.metadata["verified_artifact"]["deployable_artifact_id"], Value::String(first.record.artifact_hash.clone()));
    assert_eq!(
        first.metadata["verified_artifact"]["boundary_schema"],
        Value::String(cellscript_artifact_checker::VERIFIED_ARTIFACT_BOUNDARY_SCHEMA.to_string())
    );
    let mapped = first
        .source_map
        .semantic_mappings
        .iter()
        .map(|mapping| mapping.semantic_node_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(foundation.provenance.nodes.iter().all(|node| mapped.contains(node.id.as_str())));
}

#[test]
fn checker_accepts_native_edition_2027_type_script_trigger_and_disposition() {
    let result = compile(
        NATIVE_EDITION_2027_SOURCE,
        CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(result);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert_eq!(foundation.entry_contract.trigger, "type-group<Token>");
    assert_eq!(foundation.entry_contract.exact_entry, "action:transfer");
    assert!(foundation.dispositions.iter().any(|disposition| {
        matches!(
            &disposition.input,
            Some(cellscript_artifact_checker::InputDisposition::Successor { output_role })
                if output_role.ends_with(":next:output[0]")
        ) && disposition.envelope.completeness == "exhaustive"
    }));
    let enforced = foundation.claims.iter().find(|claim| claim.execution.is_some()).expect("group require claim");
    assert_eq!(enforced.statement, "require token.amount > 0");
    assert_eq!(enforced.enforcement, "checked-runtime");
    assert!(enforced.on_chain_checked);
    let mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == enforced.semantic_node_id)
        .expect("group require claim must have a source mapping");
    let mapped_source = &NATIVE_EDITION_2027_SOURCE[mapping.source_start as usize..mapping.source_end as usize];
    assert_eq!(mapped_source, "require token.amount > 0");

    let mut invalid = fixture.clone();
    invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("group require execution binding")
        .condition_node_id = "00".repeat(32);
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture.clone();
    let execution = invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("group require execution binding");
    std::mem::swap(&mut execution.success_block, &mut execution.failure_block);
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    invalid
        .record
        .typed_semantics
        .foundation
        .claims
        .iter_mut()
        .find_map(|claim| claim.execution.as_mut())
        .expect("group require execution binding")
        .failure_error_code = 1;
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_fresh_retire_and_audit_but_rejects_reclassified_evidence() {
    let fresh = compile(
        NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE,
        CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(fresh);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert!(matches!(
        foundation.dispositions[0].output,
        Some(cellscript_artifact_checker::OutputOrigin::Fresh { ref identity_policy }) if identity_policy == "ckb-type-id"
    ));
    let fresh_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == foundation.dispositions[0].semantic_node_id)
        .expect("fresh disposition source mapping");
    assert!(NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE[fresh_mapping.source_start as usize..fresh_mapping.source_end as usize]
        .starts_with("fresh next"));
    let audit = foundation.claims.iter().find(|claim| claim.category == "audit").expect("audit claim");
    assert_eq!(audit.enforcement, "metadata-only");
    assert!(!audit.on_chain_checked);
    assert!(audit.execution.is_none());
    let audit_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == audit.semantic_node_id)
        .expect("audit claim source mapping");
    assert!(NATIVE_FRESH_AUDIT_EDITION_2027_SOURCE[audit_mapping.source_start as usize..audit_mapping.source_end as usize]
        .starts_with("audit issuance_policy"));

    let mut invalid = fixture.clone();
    invalid.record.typed_semantics.foundation.claims.iter_mut().find(|claim| claim.category == "audit").unwrap().evidence_reference =
        "proof-plan:pretend-on-chain".to_string();
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    invalid.record.typed_semantics.foundation.claims.iter_mut().find(|claim| claim.category == "audit").unwrap().on_chain_checked =
        true;
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let retired = compile(
        NATIVE_RETIRE_EDITION_2027_SOURCE,
        CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(retired);
    assert!(matches!(
        fixture.record.typed_semantics.foundation.dispositions[0].input,
        Some(cellscript_artifact_checker::InputDisposition::Retired { ref absence_policy })
            if absence_policy == "same-field-identity-output-absent:note_id"
    ));

    let mut invalid = fixture;
    invalid.record.typed_semantics.foundation.dispositions[0].envelope.completeness = "partial".to_string();
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_checked_pool_and_rejects_divergent_accounting() {
    let result = compile(
        NATIVE_POOL_EDITION_2027_SOURCE,
        CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    let fixture = Fixture::from_result(result);
    let foundation = &fixture.record.typed_semantics.foundation;
    assert_eq!(
        foundation
            .dispositions
            .iter()
            .filter(|disposition| matches!(disposition.input, Some(cellscript_artifact_checker::InputDisposition::Pooled { .. })))
            .count(),
        2
    );
    assert!(foundation.dispositions.iter().any(|disposition| {
        matches!(
            disposition.output,
            Some(cellscript_artifact_checker::OutputOrigin::PoolResult {
                ref accounting_obligation,
                ..
            }) if accounting_obligation == "checked-u128-field-sum-equality:amount"
        )
    }));
    let pool_result = foundation
        .dispositions
        .iter()
        .find(|disposition| matches!(disposition.output, Some(cellscript_artifact_checker::OutputOrigin::PoolResult { .. })))
        .unwrap();
    let pool_mapping = fixture
        .source_map
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == pool_result.semantic_node_id)
        .expect("pool disposition source mapping");
    assert!(NATIVE_POOL_EDITION_2027_SOURCE[pool_mapping.source_start as usize..pool_mapping.source_end as usize]
        .starts_with("pool value_flow"));
    assert!(foundation.claims.iter().any(|claim| {
        claim.statement == "require left.amount as u128 + right.amount as u128 == merged.amount as u128"
            && claim.on_chain_checked
            && claim.execution.is_some()
    }));

    let mut invalid = fixture.clone();
    let Some(cellscript_artifact_checker::OutputOrigin::PoolResult { accounting_obligation, .. }) =
        invalid.record.typed_semantics.foundation.dispositions.iter_mut().find_map(|disposition| disposition.output.as_mut())
    else {
        panic!("pool result disposition");
    };
    *accounting_obligation = "checked-u128-field-sum-equality:forged".to_string();
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut invalid = fixture;
    for disposition in &mut invalid.record.typed_semantics.foundation.dispositions {
        if let Some(cellscript_artifact_checker::InputDisposition::Pooled { accounting_obligation, .. }) = &mut disposition.input {
            accounting_obligation.clear();
        }
        if let Some(cellscript_artifact_checker::OutputOrigin::PoolResult { accounting_obligation, .. }) = &mut disposition.output {
            accounting_obligation.clear();
        }
    }
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_accepts_native_lock_script_with_byte_identical_ordinary_lowering() {
    let ordinary =
        compile(ORDINARY_LOCK_SOURCE, CompileOptions { target: Some("riscv64-elf".to_string()), ..CompileOptions::default() })
            .unwrap();
    let native = compile(
        NATIVE_LOCK_EDITION_2027_SOURCE,
        CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
    )
    .unwrap();
    assert_eq!(ordinary.artifact_bytes, native.artifact_bytes);
    assert_eq!(ordinary.metadata.typed_semantics.foundation.identities, native.metadata.typed_semantics.foundation.identities);
    let ordinary_claim = ordinary
        .metadata
        .typed_semantics
        .foundation
        .claims
        .iter()
        .find(|claim| claim.execution.is_some())
        .expect("ordinary require claim");
    let ordinary_mapping = ordinary
        .source_artifact_map
        .as_ref()
        .unwrap()
        .semantic_mappings
        .iter()
        .find(|mapping| mapping.semantic_node_id == ordinary_claim.semantic_node_id)
        .expect("ordinary require claim must have a source mapping");
    assert_eq!(
        &ORDINARY_LOCK_SOURCE[ordinary_mapping.source_start as usize..ordinary_mapping.source_end as usize],
        "require vault.owner == owner"
    );

    let fixture = Fixture::from_result(native);
    let contract = &fixture.record.typed_semantics.foundation.entry_contract;
    assert_eq!(contract.script_role, "lock");
    assert_eq!(contract.trigger, "lock-group");
    assert_eq!(contract.exact_entry, "lock:unlock");

    let mut invalid = fixture;
    let disposition = invalid.record.typed_semantics.foundation.dispositions.first_mut().expect("lock disposition");
    let Some(cellscript_artifact_checker::InputDisposition::AuthorizationOnly { disposition_owner }) = &mut disposition.input else {
        panic!("native Lock Script must emit AuthorizationOnly");
    };
    disposition_owner.clear();
    invalid.rebind_layered_identities();
    assert_code(&invalid, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn semantic_foundation_and_source_mapping_mutations_are_rejected() {
    let valid = Fixture::new();

    let mut changed = valid.clone();
    changed.record.typed_semantics.foundation.provenance.nodes[0].id = "00".repeat(32);
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.foundation.identities.core_semantic_id = "11".repeat(32);
    changed.metadata["verified_artifact"]["core_semantic_id"] = Value::String("11".repeat(32));
    changed.rebind_typed_semantics();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid;
    changed.source_map.semantic_mappings[0].semantic_node_id = "22".repeat(32);
    changed.source_map.canonicalize();
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2416SourceMapInvalid);
}

#[test]
fn fixed_cell_binding_mutations_are_rejected_after_outer_hash_rebinding() {
    use cellscript_artifact_checker::{CellBindingMembership, CellBindingSource};
    let valid = Fixture::from_source(
        r#"
module fixed_binding_mutations;
resource Token has consume { amount: u64, }
shared Config { value: u64, }
action inspect(input token: Token, read config: Config, witness expected: u64) -> u64 {

        require token.amount == expected;
        require config.value == expected;
        consume token;
        return 0;
}
"#,
    );
    assert_eq!(valid.record.typed_semantics.entries[0].cell_bindings.len(), 2);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].cell_bindings[0].ordinal += 1;
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let binding = changed.record.typed_semantics.entries[0]
        .cell_bindings
        .iter_mut()
        .find(|binding| binding.source == CellBindingSource::Input)
        .unwrap();
    binding.source = CellBindingSource::GroupInput;
    binding.membership = CellBindingMembership::CurrentTypeGroup;
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let binding = changed.record.typed_semantics.entries[0]
        .cell_bindings
        .iter_mut()
        .find(|binding| binding.source == CellBindingSource::CellDep)
        .unwrap();
    binding.membership = CellBindingMembership::CurrentTypeGroup;
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    changed.record.typed_semantics.entries[0].cell_bindings.remove(0);
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);

    let mut changed = valid.clone();
    let entry = &mut changed.record.typed_semantics.entries[0];
    let removed = entry
        .cell_bindings
        .remove(entry.cell_bindings.iter().position(|binding| binding.source == CellBindingSource::CellDep).unwrap());
    let removed_role = removed.role_id(&entry.id);
    changed.record.typed_semantics.foundation.roles.retain(|role| role.role_id != removed_role);
    changed.rebind_layered_identities();
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    assert!(error.message.contains("fixed Cell parameter 'config' has no resolved binding"), "{error:?}");

    let mut changed = valid;
    let duplicate = changed.record.typed_semantics.entries[0].cell_bindings[0].clone();
    changed.record.typed_semantics.entries[0].cell_bindings.insert(0, duplicate);
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn deferred_sighash_effect_cannot_be_reclassified_after_hash_rebinding() {
    let source = r#"
module deferred_sighash_checker;
action main() -> u64 {

    let digest = env::sighash_all(source::group_input(0));
    return 0;
}
"#;
    let fixture = Fixture::from_result(
        compile(
            source,
            CompileOptions {
                diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
                target: Some("riscv64-elf".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    for failure in
        [None, Some(cellscript_artifact_checker::TypedSemanticRuntimeError { code: 86, name: "since-constraints-unsupported".into() })]
    {
        let mut changed = fixture.clone();
        let call = changed
            .record
            .typed_semantics
            .entries
            .iter_mut()
            .flat_map(|entry| &mut entry.blocks)
            .flat_map(|block| &mut block.operations)
            .filter_map(|operation| operation.call.as_mut())
            .find(|call| call.target == "__ckb_sighash_all")
            .expect("deferred call is present");
        call.runtime_failure = failure;
        changed.rebind_layered_identities();
        let error = check_bundle_values(
            &changed.artifact,
            &changed.metadata,
            &changed.record,
            &changed.source_map,
            &CheckerBudgets::default(),
        )
        .expect_err("deferred failure classification cannot be stripped");
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

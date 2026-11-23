#[test]
fn instance_runtime_checks_and_transitive_dependencies_reject_rehashed_machine_changes() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("foreign.cell"),
        "module foreign::documents;\npublic struct Marker has copy, drop { value: u64, }\n",
    )
    .unwrap();
    std::fs::write(directory.path().join("main.cell"), "module observer;\nuse foreign::documents::Marker;\nresource Document has store { digest: Hash, }\npublic action verify(witness handle: InstanceScriptHandle<foreign::documents>) {  ckb::require_instance_script_participant_from_args(ckb::cell_dep(0), handle, ckb::input::<Document>(1)) }\n").unwrap();
    let valid = Fixture::from_result(
        cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8PathBuf::from_path_buf(directory.path().join("main.cell")).unwrap(),
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".into()),
                target_profile: Some("ckb".into()),
                source_contracts: true,
                ..Default::default()
            },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap(),
    );
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let owner = "runtime:__ckb_require_instance_script_participant_from_args";
    let mut changed_count = 0;
    for instruction in &elf.instructions {
        if !valid.record.blocks.iter().any(|block| block.owner_entry == owner && block.range.contains(instruction.address)) {
            continue;
        }
        let word = instruction.word;
        let conditional = word & 0x7f == 0x63;
        let byte_load = word & 0x7f == 0x03 && (word >> 12) & 7 == 4;
        let constant = word & 0x7f == 0x13
            && (word >> 12) & 7 == 0
            && (word >> 15) & 31 == 0
            && matches!((word >> 7) & 31, 5 | 6 | 7 | 12 | 13 | 14 | 15);
        if !conditional && !byte_load && !constant {
            continue;
        }
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ if conditional { 1 << 12 } else { 1 << 20 });
        assert!(changed.check().is_err(), "rehashed instance check accepted at {:#x}", instruction.address);
        changed_count += 1;
    }
    assert!(changed_count > 200, "all canonical-field, bounds, predicate, subject and failure branches must be exercised");
    for helper in [
        "__ckb_require_cell_dep_open_script_handle",
        "__ckb_hash_blake2b_var",
        "__cellscript_blake2b_compress",
        "__cellscript_memcpy_fixed",
        "__cellscript_memcmp_fixed",
    ] {
        let owner = format!("runtime:{helper}");
        let instruction = elf
            .instructions
            .iter()
            .find(|instruction| {
                valid.record.blocks.iter().any(|block| block.owner_entry == owner && block.range.contains(instruction.address))
                    && instruction.word & 0x7f == 0x03
            })
            .unwrap();
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, instruction.word ^ (1 << 20));
        assert!(changed.check().is_err(), "unchecked runtime dependency {helper}");
    }
    // Reclassification must not remove the machine profile obligation.
    let mut changed = valid.clone();
    let operation = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|op| op.call.as_ref().is_some_and(|call| call.target == "__ckb_require_instance_script_participant_from_args"))
        .unwrap();
    operation.call.as_mut().unwrap().target = "__ckb_require_open_script_participant_from_args".into();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn exact_zk_machine_mutations_reject_after_hash_rebinding() {
    let hash = |byte: u8| format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32));
    let source = format!(
        r#"module zk_mutations;
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{

    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {});
    return 0;
}}
"#,
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    );
    let valid = Fixture::from_result(
        compile(
            &source,
            CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        )
        .unwrap(),
    );
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let begin = valid
        .record
        .blocks
        .iter()
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lzk_begin_")))
        .unwrap()
        .range
        .start;
    let end = valid
        .record
        .blocks
        .iter()
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lzk_verified_")))
        .unwrap()
        .range
        .start;
    let instructions: Vec<_> =
        elf.instructions.iter().filter(|instruction| begin <= instruction.address && instruction.address < end).collect();
    // Request identities, context source/index, read/write lengths, mandatory
    // result conditions and error exits. Rehashing must not bless any change.
    let mut changed_count = 0;
    for instruction in instructions {
        let word = instruction.word;
        let opcode = word & 0x7f;
        let rd = (word >> 7) & 31;
        let source1 = (word >> 15) & 31;
        let function = (word >> 12) & 7;
        let is_constant = opcode == 0x13 && function == 0 && source1 == 0 && matches!(rd, 5 | 6 | 13 | 14 | 15);
        let is_checked_branch = opcode == 0x63 && function == 0;
        let is_dependency_shift = opcode == 0x13 && matches!(function, 1 | 5) && rd == 10 && source1 == 10;
        if !is_constant && !is_checked_branch && !is_dependency_shift {
            continue;
        }
        let mut changed = valid.clone();
        changed.replace_machine_word(instruction.address, word ^ if is_checked_branch { 1 << 12 } else { 1 << 20 });
        assert!(changed.check().is_err(), "ZK machine mutation accepted at {:#x}: {word:#x}", instruction.address);
        changed_count += 1;
    }
    let stack_store = elf
        .instructions
        .iter()
        .find(|instruction| {
            let word = instruction.word;
            begin <= instruction.address
                && instruction.address < end
                && word & 0x7f == 0x23
                && (word >> 12) & 7 == 3
                && (word >> 15) & 31 == 2
                && (word >> 20) & 31 == 5
        })
        .expect("ZK canonical request has a stack store");
    let mut changed_stack = valid.clone();
    changed_stack.replace_machine_word(stack_store.address, stack_store.word ^ (1 << 10));
    assert!(changed_stack.check().is_err(), "accepted changed request stack slot");
    assert!(changed_count > 150);
    let mut changed = valid.clone();
    changed.metadata["runtime"]["zk_verifiers"][0]["verification_key_hash"] = Value::String("ff".repeat(32));
    changed.rebind_sidecars();
    assert!(changed.check().is_err());
    for (field, value) in [
        ("request_bytes", serde_json::json!(465)),
        ("max_calls", serde_json::json!(2)),
        ("max_cycles", serde_json::json!(250_000_001)),
        ("context_binding", serde_json::json!("witness-selected")),
    ] {
        let mut changed = valid.clone();
        changed.metadata["runtime"]["zk_verifiers"][0][field] = value;
        changed.rebind_sidecars();
        assert!(changed.check().is_err(), "accepted changed ZK {field}");
    }
    let mut changed = valid.clone();
    changed.metadata["runtime"]["zk_verifiers"][0]["source"]["fields"][0] = serde_json::json!("domain=witness");
    changed.rebind_sidecars();
    assert!(changed.check().is_err());
}

#[test]
fn zk_request_bytes_reject_any_post_initialization_store_into_the_request_window() {
    let hash = |byte: u8| format!("Hash::from_bytes(b\"{}\")", format!("\\x{byte:02x}").repeat(32));
    let source = format!(
        r#"module zk_request_lifetime;
action update(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    zk::require_valid("transition", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {});
    return 0;
}}
"#,
        hash(1),
        hash(2).replace("Hash::", "VerificationKeyCommitment::"),
        hash(3),
        hash(4)
    );
    let valid = Fixture::from_result(
        compile(
            &source,
            CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        )
        .unwrap(),
    );
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let label = |prefix: &str| {
        valid
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(prefix)))
            .unwrap()
            .range
            .start
    };
    let begin = label(".Lzk_begin_");
    let initialized = label(".Lzk_request_initialized_");
    let verified = label(".Lzk_verified_");
    // The first bound initialization store writes request word zero through
    // the frame base. Replaying it over any post-initialization instruction
    // would rewrite the transmitted request after the checked binding.
    let store = elf
        .instructions
        .iter()
        .find(|instruction| {
            begin <= instruction.address
                && instruction.address < initialized
                && instruction.word & 0x7f == 0x23
                && (instruction.word >> 12) & 7 == 3
                && (instruction.word >> 15) & 31 == 2
                && (instruction.word >> 20) & 31 == 5
        })
        .expect("ZK canonical request has an initialization store");
    let victim = elf
        .instructions
        .iter()
        .find(|instruction| initialized <= instruction.address && instruction.address < verified && instruction.word & 0x7f == 0x13)
        .expect("ZK region has a post-initialization setup instruction");
    let mut changed = valid.clone();
    changed.replace_machine_word(victim.address, store.word);
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert!(
        error.to_string().contains("request bytes modified after checked initialization"),
        "accepted post-initialization request rewrite: {error}"
    );
}

fn bounded_lock_handle(metadata: &mut Value) -> &mut Value {
    metadata["runtime"]["transaction_view_handles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|handle| handle["handle_type"] == "WitnessBytesView<lock,64>")
        .expect("bounded lock witness handle")
}

fn exact_handle_operation(fixture: &mut Fixture) -> &mut TypedSemanticOperation {
    fixture
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_require_cell_dep_exact_verifier_handle"))
        .expect("exact verifier handle operation")
}

fn deployment_line_handle_operation(fixture: &mut Fixture) -> &mut TypedSemanticOperation {
    fixture
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| {
            operation.call.as_ref().is_some_and(|call| call.target == "__ckb_require_cell_dep_deployment_line_verifier_handle")
        })
        .expect("deployment line verifier handle operation")
}

#[test]
fn checker_rejects_vm2_isa_or_data2_contract_tampering() {
    let valid = Fixture::new();
    for pointer in ["/target_profile/minimum_vm_version", "/constraints/ckb/profile_abi_contract/minimum_vm_version"] {
        let mut changed = valid.clone();
        *changed.metadata.pointer_mut(pointer).unwrap() = Value::from(1);
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["riscv_isa"] = Value::String("rv64imac".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["deployment_hash_types"] = serde_json::json!(["data1"]);
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

#[test]
fn checker_accepts_type_hash_profile_and_rejects_cross_profile_hash_type_tampering() {
    let valid = Fixture::from_result(
        compile(
            FIXTURE_SOURCE,
            CompileOptions {
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb-type-hash".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["target_profile"]["deployment_hash_types"], serde_json::json!(["type"]));
    assert_eq!(
        valid.metadata["constraints"]["ckb"]["hash_type_policy"]["default_script_hash_type"],
        Value::String("type".to_string())
    );

    let mut changed = valid.clone();
    changed.metadata["target_profile"]["deployment_hash_types"] = serde_json::json!(["data2"]);
    changed.metadata["constraints"]["ckb"]["profile_abi_contract"]["deployment_hash_types"] = serde_json::json!(["data2"]);
    changed.metadata["constraints"]["ckb"]["hash_type_policy"]["default_script_hash_type"] = Value::String("data2".to_string());
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

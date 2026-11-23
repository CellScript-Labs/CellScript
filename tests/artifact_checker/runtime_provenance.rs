#[test]
fn checker_rejects_runtime_access_provenance_tampering_after_hash_rebinding() {
    let valid = Fixture::from_result(
        compile(
            RUNTIME_PROVENANCE_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(cellscript::METADATA_SCHEMA_VERSION));
    assert_eq!(
        valid.metadata["runtime"]["ckb_runtime_access_provenance_contract"],
        Value::String("cellscript-ckb-runtime-access-provenance-v1".to_string())
    );

    let mut changed = valid.clone();
    changed.metadata["runtime"]["ckb_runtime_access_provenance_contract"] = Value::String("tampered".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mutate_matching_accesses = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            let access = accesses
                .iter_mut()
                .find(|access| access["operation"] == "cell-data-hash-field")
                .expect("dynamic CellDep data-hash access");
            mutation(access);
        }
    };

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["index"]["max_inclusive"] = Value::from(u64::from(u32::MAX) - 1);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["source"]["resolved_source"] = Value::String("HeaderDep".to_string());
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_matching_accesses(&mut changed.metadata, |access| {
        access["provenance"]["range"]["length"]["value"] = Value::from(0);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    changed.metadata["actions"][0]["ckb_runtime_accesses"][0]["binding"] = Value::String("tampered".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let set_transaction_hash_access_field = |metadata: &mut Value, field: &str, value: &Value| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            let access = accesses
                .iter_mut()
                .find(|access| access["operation"] == "transaction-hash")
                .expect("canonical transaction-hash access");
            access[field] = value.clone();
        }
    };

    for mutation in [
        ("operation", Value::String("transaction-hash-unbound".to_string())),
        ("syscall", Value::String("LOAD_TRANSACTION".to_string())),
        ("binding", Value::String("ckb::transaction_bytes".to_string())),
    ] {
        let mut changed = valid.clone();
        set_transaction_hash_access_field(&mut changed.metadata, mutation.0, &mutation.1);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
        let access = changed
            .metadata
            .pointer_mut(pointer)
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|access| access["operation"] == "transaction-hash")
            .expect("canonical transaction-hash access");
        access["provenance"]["range"]["length"]["value"] = Value::from(31);
        access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(31);
    }
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let handle = changed.metadata["runtime"]["transaction_view_handles"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|handle| handle["handle_type"] == "InputView<Token>")
        .expect("typed Input view handle");
    handle["provenance"]["index"]["binding"] = Value::String(String::new());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
}

#[test]
fn checker_binds_header_dep_fields_widths_syscalls_and_terminal_errors_to_machine_code() {
    let valid = Fixture::from_result(
        compile(
            HEADER_DEP_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );

    let mutate_access = |metadata: &mut Value, operation: &str, mutation: &dyn Fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let access = metadata
                .pointer_mut(pointer)
                .and_then(Value::as_array_mut)
                .unwrap()
                .iter_mut()
                .find(|access| access["operation"] == operation)
                .unwrap_or_else(|| panic!("missing HeaderDep access {operation}"));
            mutation(access);
        }
    };
    for (operation, mutation) in [
        ("header-dep-epoch-number", ("syscall", Value::String("LOAD_HEADER".to_string()))),
        ("header-dep-epoch-start-block-number", ("binding", Value::String("HeaderDepView.timestamp".to_string()))),
        ("header-dep-epoch-length", ("source", Value::String("Input".to_string()))),
    ] {
        let mut changed = valid.clone();
        mutate_access(&mut changed.metadata, operation, &|access| access[mutation.0] = mutation.1.clone());
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    mutate_access(&mut changed.metadata, "header-dep-block-number", &|access| {
        access["provenance"]["range"]["length"]["value"] = Value::from(207);
        access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(207);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    let timestamp_call = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_header_dep_timestamp_millis"))
        .expect("timestamp typed helper call");
    timestamp_call.call.as_mut().unwrap().return_type = "BlockNumber".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let scalar_site = valid
        .record
        .syscall_sites
        .iter()
        .find(|site| site.contract == "ckb-header-dep-epoch-number-v1")
        .expect("epoch-number syscall site")
        .clone();
    let full_site = valid
        .record
        .syscall_sites
        .iter()
        .find(|site| site.contract == "ckb-header-dep-block-number-v1")
        .expect("block-number syscall site")
        .clone();

    for mutate in [
        |site: &mut cellscript_artifact_checker::SyscallSite| site.syscall_number = Some(2081),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.source_domain = "HeaderDep".to_string(),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.index_domain = "unbounded".to_string(),
        |site: &mut cellscript_artifact_checker::SyscallSite| site.return_code_checked = false,
        |site: &mut cellscript_artifact_checker::SyscallSite| site.buffer_limit_bytes = 7,
    ] {
        let mut changed = valid.clone();
        let site = changed.record.syscall_sites.iter_mut().find(|site| site.address == scalar_site.address).unwrap();
        mutate(site);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2417SyscallContractInvalid);
    }

    for delta in [-48_i64, -40, -12, -4, 4, 20] {
        let mut changed = valid.clone();
        let address = scalar_site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "scalar delta {delta}");
    }

    for delta in [-44_i64, -36, -4, 4, 20, 28] {
        let mut changed = valid.clone();
        let address = full_site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "full delta {delta}");
    }

    for (site, delta) in [(&scalar_site, 8_i64), (&full_site, 8), (&full_site, 40)] {
        let mut changed = valid.clone();
        let address = site.address.checked_add_signed(delta).unwrap();
        let word = parse_elf(&changed.artifact, CheckerBudgets::default().instructions)
            .unwrap()
            .instructions
            .iter()
            .find(|instruction| instruction.address == address)
            .unwrap()
            .word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2414ControlFlowInvalid), "terminal error delta {delta}");
    }

    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::from_result(
            compile(
                HEADER_DEP_SOURCE,
                CompileOptions {
                    edition: CURRENT_EDITION,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some("ckb".to_string()),
                    diagnostics,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert_eq!(
            fixture.record.syscall_sites.iter().filter(|site| site.contract.starts_with("ckb-header-dep-")).count(),
            5,
            "strategy {diagnostics} must retain every specialized HeaderDep syscall site"
        );
    }

    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let invalid_source_error = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == "runtime:__ckb_header_dep_epoch_number")
        .flat_map(|block| elf.instructions.iter().filter(move |instruction| block.range.contains(instruction.address)))
        .find(|instruction| {
            instruction.word & 0x7f == 0x13
                && (instruction.word >> 7) & 0x1f == 10
                && (instruction.word >> 15) & 0x1f == 0
                && (instruction.word as i32) >> 20 == 44
        })
        .expect("invalid HeaderDep source error")
        .address;
    let mut changed = valid;
    let word = elf.instructions.iter().find(|instruction| instruction.address == invalid_source_error).unwrap().word;
    changed.replace_machine_word(invalid_source_error, word ^ (1 << 20));
    assert_code(&changed, CheckerRejectionCode::V2414ControlFlowInvalid);
}

#[test]
fn checker_binds_canonical_script_hash_metadata_types_and_machine_serialization() {
    let valid = Fixture::from_result(
        compile(
            SCRIPT_HASH_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.record.version, cellscript_artifact_checker::LOWERING_RECORD_VERSION);

    let mutate_access = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let access = metadata
                .pointer_mut(pointer)
                .and_then(Value::as_array_mut)
                .unwrap()
                .iter_mut()
                .find(|access| access["operation"] == "script-hash-v1")
                .expect("canonical Script hash access");
            mutation(access);
        }
    };
    for mutation in [
        |access: &mut Value| access["syscall"] = Value::String("SHA256".to_string()),
        |access: &mut Value| access["source"] = Value::String("Expression".to_string()),
        |access: &mut Value| access["provenance"]["source"]["origin"] = Value::String("implicit-lowering".to_string()),
        |access: &mut Value| {
            access["provenance"]["range"]["length"]["value"] = Value::from(56);
            access["provenance"]["range"]["length"]["max_inclusive"] = Value::from(56);
        },
    ] {
        let mut changed = valid.clone();
        mutate_access(&mut changed.metadata, mutation);
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    let call = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| operation.call.as_mut().filter(|call| call.target == "__ckb_script_hash"))
        .expect("typed Script hash call");
    call.return_type = "ScriptHash".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let script_entry =
        valid.record.entries.iter().find(|entry| entry.id == "runtime:__ckb_script_hash").expect("Script hash runtime entry");
    let base =
        valid.record.blocks.iter().find(|block| block.id == script_entry.entry_block).expect("Script hash entry block").range.start;
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for delta in [36_u64, 108, 144, 188, 200, 252, 280, 300] {
        let mut changed = valid.clone();
        let address = base + delta;
        let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        assert_eq!(changed.check(), Err(CheckerRejectionCode::V2417SyscallContractInvalid), "machine delta {delta}");
    }

    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = Fixture::from_result(
            compile(
                SCRIPT_HASH_SOURCE,
                CompileOptions {
                    edition: CURRENT_EDITION,
                    target: Some("riscv64-elf".to_string()),
                    target_profile: Some("ckb".to_string()),
                    diagnostics,
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        assert_eq!(fixture.check(), Ok(()), "strategy {diagnostics} must preserve the Script hash machine contract");
    }
}

#[test]
fn checker_binds_bounded_witness_owner_limit_range_and_typed_retyping() {
    let valid = Fixture::from_result(
        compile(
            BOUNDED_WITNESS_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(cellscript::METADATA_SCHEMA_VERSION));

    let mutate_bounded_accesses = |metadata: &mut Value, mutation: fn(&mut Value)| {
        for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
            let accesses = metadata.pointer_mut(pointer).and_then(Value::as_array_mut).unwrap();
            for access in accesses
                .iter_mut()
                .filter(|access| access["operation"].as_str().is_some_and(|operation| operation.starts_with("witness-bounded-lock-")))
            {
                mutation(access);
            }
        }
    };

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["witness_owner"] = Value::String("entry".to_string());
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["max_bytes"] = Value::from(63);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    bounded_lock_handle(&mut changed.metadata)["provenance"]["range"]["length"]["max_inclusive"] = Value::from(63);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid.clone();
    mutate_bounded_accesses(&mut changed.metadata, |access| {
        access["provenance"]["range"]["offset"]["max_inclusive"] = Value::from(63);
    });
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid;
    let local = changed
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.locals)
        .find(|local| local.ty == "WitnessBytesView<lock,64>")
        .expect("typed bounded witness local");
    local.ty = "WitnessBytesView<signer,64>".to_string();
    changed.rebind_layered_identities();
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn checker_binds_zero_lock_signing_domain_to_runtime_access_and_typed_call() {
    let valid = Fixture::from_result(
        compile(
            SIGHASH_ZERO_LOCK_SOURCE,
            CompileOptions {
                edition: CURRENT_EDITION,
                target: Some("riscv64-elf".to_string()),
                target_profile: Some("ckb".to_string()),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert_eq!(valid.metadata["metadata_schema_version"], Value::from(cellscript::METADATA_SCHEMA_VERSION));
    assert_eq!(valid.metadata["runtime"]["signing_message_domains"].as_array().unwrap().len(), 1);

    for (field, tampered) in [
        ("contract", "tampered-signing-domain"),
        ("digest_type", "Hash"),
        ("first_witness_lock_transform", "preserve-prefix-and-zero-signatures"),
        ("runtime_helper", "__tampered_sighash"),
    ] {
        let mut changed = valid.clone();
        changed.metadata["runtime"]["signing_message_domains"][0][field] = Value::String(tampered.to_string());
        changed.rebind_sidecars();
        assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);
    }

    let mut changed = valid.clone();
    changed.metadata["runtime"]["signing_message_domains"] = serde_json::json!([]);
    changed.rebind_sidecars();
    assert_code(&changed, CheckerRejectionCode::V2410MetadataBindingMismatch);

    let mut changed = valid;
    changed.metadata["runtime"]["signing_message_domains"][0]["max_group_inputs"] = Value::from(5);
    for pointer in ["/runtime/ckb_runtime_accesses", "/actions/0/ckb_runtime_accesses"] {
        let access = changed
            .metadata
            .pointer_mut(pointer)
            .and_then(Value::as_array_mut)
            .unwrap()
            .iter_mut()
            .find(|access| access["operation"] == "sighash-all-zero-lock-v1")
            .expect("bounded sighash runtime access");
        access["provenance"]["index"]["max_inclusive"] = Value::from(4);
    }
    changed.rebind_sidecars();
    let error =
        check_bundle_values(&changed.artifact, &changed.metadata, &changed.record, &changed.source_map, &CheckerBudgets::default())
            .expect_err("metadata and access bounds cannot diverge from the typed call");
    assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
    assert!(error.message.contains("typed bounded sighash call"), "{error}");
}

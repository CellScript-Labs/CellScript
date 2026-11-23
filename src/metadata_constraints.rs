use super::entry_witness::entry_witness_scalar_param_width;
use super::error::Result;
use super::{
    ckb_limits_source, codegen, env_u64, ir, runtime_errors, ArtifactConstraintsMetadata, ArtifactFormat,
    CkbCapacityEvidenceContractMetadata, CkbCapacityFloorMetadata, CkbConstraintsMetadata, CkbDepGroupManifestMetadata,
    CkbHashTypePolicyMetadata, CkbProfileAbiContractMetadata, CkbRuntimeAccessMetadata, CkbScriptReferenceMetadata,
    CkbTimelockPolicyMetadata, CompileMetadata, ConstraintsMetadata, CreatePatternMetadata, EntryAbiConstraintsMetadata,
    ParamAbiConstraintsMetadata, ParamMetadata, RuntimeErrorConstraintsMetadata, TargetProfile, CKB_DEFAULT_MAX_BLOCK_BYTES,
    CKB_DEFAULT_MAX_BLOCK_CYCLES, CKB_DEFAULT_MAX_TX_VERIFY_CYCLES, CKB_DEFAULT_SCRIPT_HASH_TYPE, CKB_SHANNONS_PER_CKB,
    CKB_TYPE_ID_HASH_TYPE, ENTRY_WITNESS_ABI_MAGIC,
};

pub(super) fn bind_constraints_metadata(
    metadata: &mut CompileMetadata,
    artifact_bytes: &[u8],
    artifact_format: ArtifactFormat,
    target_profile: TargetProfile,
    ir: &ir::IrModule,
    codegen_options: &codegen::CodegenOptions,
) -> Result<()> {
    let assembly_for_shape = if artifact_format == ArtifactFormat::RiscvAssembly {
        std::str::from_utf8(artifact_bytes).ok().map(str::to_string)
    } else {
        codegen::generate(ir, codegen_options, ArtifactFormat::RiscvAssembly).ok().and_then(|bytes| String::from_utf8(bytes).ok())
    };
    let backend_shape = assembly_for_shape.as_deref().and_then(|assembly| codegen::analyze_backend_shape(assembly).ok());
    metadata.constraints =
        constraints_metadata(metadata, artifact_bytes.len(), artifact_format, target_profile, backend_shape.as_ref());
    Ok(())
}

fn constraints_metadata(
    metadata: &CompileMetadata,
    artifact_size_bytes: usize,
    artifact_format: ArtifactFormat,
    target_profile: TargetProfile,
    backend_shape: Option<&codegen::BackendShapeMetrics>,
) -> ConstraintsMetadata {
    let mut warnings = Vec::new();
    let mut failures = Vec::new();
    let mut entry_abi = Vec::new();
    entry_abi.extend(metadata.actions.iter().map(|action| entry_abi_constraints("action", &action.name, &action.params)));
    entry_abi.extend(metadata.locks.iter().map(|lock| entry_abi_constraints("lock", &lock.name, &lock.params)));
    for entry in &entry_abi {
        if entry.unsupported {
            failures.push(format!(
                "{} '{}' has unsupported entry ABI: {}",
                entry.entry_kind,
                entry.entry_name,
                entry.unsupported_reasons.join("; ")
            ));
        }
        for param in &entry.params {
            if param.pointer_pair_crosses_register_boundary {
                warnings.push(format!(
                    "{} '{}' parameter '{}' pointer/length ABI pair crosses a0-a7 boundary at slots {}..{}",
                    entry.entry_kind, entry.entry_name, param.name, param.slot_start, param.slot_end
                ));
            }
        }
    }

    let artifact = ArtifactConstraintsMetadata {
        format: artifact_format.display_name().to_string(),
        artifact_size_bytes,
        text_bytes: backend_shape.map(|shape| shape.text_size),
        rodata_bytes: backend_shape.map(|shape| shape.rodata_size),
        executable_text_op_count: backend_shape.map(|shape| shape.executable_text_op_count),
        covered_text_op_count: backend_shape.map(|shape| shape.covered_text_op_count),
        relaxed_branch_count: backend_shape.map(|shape| shape.relaxed_branch_count),
        max_cond_branch_abs_distance: backend_shape.map(|shape| shape.max_cond_branch_abs_distance),
        machine_block_count: backend_shape.map(|shape| shape.machine_block_count),
        max_machine_block_size: backend_shape.map(|shape| shape.max_machine_block_size),
        conditional_branch_block_count: backend_shape.map(|shape| shape.conditional_branch_block_count),
        labeled_machine_block_count: backend_shape.map(|shape| shape.labeled_machine_block_count),
        machine_cfg_edge_count: backend_shape.map(|shape| shape.machine_cfg_edge_count),
        machine_call_edge_count: backend_shape.map(|shape| shape.machine_call_edge_count),
        unreachable_machine_block_count: backend_shape.map(|shape| shape.unreachable_machine_block_count),
        layout_order_block_count: backend_shape.map(|shape| shape.layout_order_block_count),
        layout_order_text_size: backend_shape.map(|shape| shape.layout_order_text_size),
    };
    if backend_shape.is_none() {
        warnings.push("backend shape metrics were not available for this artifact".to_string());
    }
    use runtime_errors::CellScriptRuntimeError as RuntimeError;
    let uses_zk = metadata.runtime.ckb_runtime_features.iter().any(|feature| feature == "zk-exact-transition-verifier");
    let runtime_errors = runtime_errors::ALL_RUNTIME_ERRORS
        .iter()
        // Keep existing artifacts' metadata identities and size budgets stable.
        // The experimental profile's diagnostics belong only to ZK artifacts.
        .filter(|error| uses_zk || !matches!(error,
            RuntimeError::ZkContextInvalid | RuntimeError::ZkPipeFailed | RuntimeError::ZkSpawnFailed
            | RuntimeError::ZkWriteFailed | RuntimeError::ZkCloseFailed | RuntimeError::ZkChildRejected
            | RuntimeError::ZkCycleBoundExceeded))
        .map(|error| {
            let info = runtime_errors::runtime_error_info(*error);
            RuntimeErrorConstraintsMetadata {
                code: info.code,
                name: info.name.to_string(),
                description: info.description.to_string(),
                hint: info.hint.to_string(),
            }
        })
        .collect();

    let max_entry_witness_bytes = entry_abi.iter().map(|entry| entry.min_witness_bytes).max().unwrap_or(0);
    let estimated_cycles = metadata.actions.iter().map(|action| action.estimated_cycles).chain(metadata.locks.iter().map(|_| 0)).max();
    let ckb = target_profile.is_ckb().then(|| {
        warnings.push(
            "CKB cycles and transaction size are not measured by the compiler; require builder dry-run for production".to_string(),
        );
        let ckb = ckb_constraints(metadata, artifact_size_bytes, max_entry_witness_bytes, estimated_cycles);
        if ckb.uses_input_since || ckb.uses_header_epoch {
            warnings.push(format!(
                "CKB timelock-related runtime features are in use (input_since={}, header_epoch={}); declarative DSL policy surface is not yet first-class",
                ckb.uses_input_since, ckb.uses_header_epoch
            ));
        }
        if ckb.capacity_planning_required {
            warnings.push(format!(
                "CKB output capacity planning is required for this artifact (create outputs={}, mutate outputs={}); full transaction-level capacity remains builder/runtime-managed",
                ckb.created_output_count, ckb.mutated_output_count
            ));
        }
        ckb
    });

    let status = if !failures.is_empty() {
        "fail"
    } else if !warnings.is_empty() {
        "warn"
    } else {
        "pass"
    }
    .to_string();

    ConstraintsMetadata {
        edition: metadata.edition,
        compatibility_profile: metadata.compatibility_profile.id.clone(),
        target_profile: metadata.target_profile.name.clone(),
        status,
        entry_abi,
        artifact,
        collection_instantiations: metadata.runtime.collection_instantiations.clone(),
        runtime_errors,
        ckb,
        warnings,
        failures,
    }
}

fn entry_abi_constraints(entry_kind: &str, entry_name: &str, params: &[ParamMetadata]) -> EntryAbiConstraintsMetadata {
    let mut abi_index = 0usize;
    let mut witness_payload_bytes = 0usize;
    let mut unsupported_reasons = Vec::new();
    let mut param_constraints = Vec::new();

    for param in params {
        let mut abi_slots = 0usize;
        let mut witness_bytes = 0usize;
        let mut abi_kind = "unsupported".to_string();
        let mut supported = true;
        let mut unsupported_reason = None;
        let pointer_length_pair;

        if is_bounded_collection_metadata_type(&param.ty)
            && param.bounded_runtime_contract.as_deref() == Some(ir::BOUNDED_TYPE_GROUP_INPUTS_V1)
            && param.source == "input"
        {
            abi_kind = ir::BOUNDED_TYPE_GROUP_INPUTS_V1.to_string();
            abi_slots = 2;
            witness_bytes = 0;
            pointer_length_pair = true;
        } else if is_bounded_collection_metadata_type(&param.ty)
            && param.bounded_runtime_contract.as_deref() == Some(ir::BOUNDED_OUTPUT_PLAN_V1)
            && param.source == "witness"
        {
            abi_kind = ir::BOUNDED_OUTPUT_PLAN_V1.to_string();
            abi_slots = 2;
            witness_bytes = 4;
            pointer_length_pair = true;
        } else if is_bounded_collection_metadata_type(&param.ty) {
            supported = false;
            unsupported_reason = Some(format!("bounded collection parameter '{}' has no canonical executable entry ABI", param.ty));
            unsupported_reasons.push(format!(
                "parameter '{}': bounded collection type '{}' is metadata-only until its source/codec runtime contract is implemented",
                param.name, param.ty
            ));
            pointer_length_pair = false;
        } else if param.lock_args_data_source {
            abi_kind =
                if param.fixed_byte_len.is_some() { "lock-args-fixed-byte".to_string() } else { "lock-args-scalar".to_string() };
            abi_slots = if param.fixed_byte_len.is_some() { 2 } else { 1 };
            witness_bytes = 0;
            pointer_length_pair = param.fixed_byte_len.is_some();
        } else if param.schema_pointer_abi || param.schema_length_abi {
            abi_kind = if param.type_hash_pointer_abi || param.type_hash_length_abi {
                "schema-pointer+type-hash-pointer".to_string()
            } else {
                "schema-pointer".to_string()
            };
            abi_slots = 2 + usize::from(param.type_hash_pointer_abi || param.type_hash_length_abi) * 2;
            witness_bytes = 4;
            pointer_length_pair = true;
        } else if param.fixed_byte_pointer_abi || param.fixed_byte_length_abi {
            abi_kind = "fixed-byte-pointer".to_string();
            abi_slots = 2;
            witness_bytes = param.fixed_byte_len.unwrap_or_default();
            pointer_length_pair = true;
        } else if let Some(width) = entry_witness_scalar_param_width(&param.ty) {
            abi_kind = "scalar".to_string();
            abi_slots = 1;
            witness_bytes = width;
            pointer_length_pair = false;
        } else {
            supported = false;
            unsupported_reason = Some(format!("unsupported entry witness parameter type '{}'", param.ty));
            unsupported_reasons.push(format!("parameter '{}': unsupported type '{}'", param.name, param.ty));
            pointer_length_pair = false;
        }

        let slot_start = abi_index;
        let slot_end = abi_index.saturating_add(abi_slots).saturating_sub(1);
        let register_slots = (slot_start..slot_start + abi_slots).filter(|slot| *slot < 8).count();
        let stack_spill_slots = abi_slots.saturating_sub(register_slots);
        let pointer_pair_crosses_register_boundary = pointer_length_pair && slot_start < 8 && slot_start + abi_slots > 8;
        if param.source == "lock_args" || param.cell_bound_abi || param.ty.starts_with('&') {
            witness_bytes = 0;
        }
        witness_payload_bytes += witness_bytes;
        param_constraints.push(ParamAbiConstraintsMetadata {
            name: param.name.clone(),
            ty: param.ty.clone(),
            abi_kind,
            abi_slots,
            slot_start,
            slot_end,
            register_slots,
            stack_spill_slots,
            stack_spill_bytes: stack_spill_slots * 8,
            witness_bytes,
            pointer_length_pair,
            pointer_pair_crosses_register_boundary,
            supported,
            unsupported_reason,
        });
        abi_index += abi_slots;
    }

    let register_slots_used = abi_index.min(8);
    let stack_spill_slots = abi_index.saturating_sub(8);
    let min_witness_bytes = if witness_payload_bytes == 0 { 0 } else { ENTRY_WITNESS_ABI_MAGIC.len() + witness_payload_bytes };
    EntryAbiConstraintsMetadata {
        entry_kind: entry_kind.to_string(),
        entry_name: entry_name.to_string(),
        param_count: params.len(),
        abi_slots_used: abi_index,
        register_slots_used,
        stack_spill_slots,
        stack_spill_bytes: stack_spill_slots * 8,
        witness_payload_bytes,
        min_witness_bytes,
        unsupported: !unsupported_reasons.is_empty(),
        unsupported_reasons,
        params: param_constraints,
    }
}

fn is_bounded_collection_metadata_type(ty: &str) -> bool {
    ty.starts_with("BoundedCellSet<") || ty.starts_with("BoundedList<")
}

pub(super) fn ckb_constraints(
    metadata: &CompileMetadata,
    artifact_size_bytes: usize,
    max_entry_witness_bytes: usize,
    estimated_cycles: Option<u64>,
) -> CkbConstraintsMetadata {
    let deployment_hash_type =
        metadata.target_profile.deployment_hash_types.first().cloned().unwrap_or_else(|| CKB_DEFAULT_SCRIPT_HASH_TYPE.to_string());
    let max_tx_verify_cycles = env_u64("CELLSCRIPT_CKB_MAX_TX_VERIFY_CYCLES").unwrap_or(CKB_DEFAULT_MAX_TX_VERIFY_CYCLES);
    let max_block_cycles = env_u64("CELLSCRIPT_CKB_MAX_BLOCK_CYCLES").unwrap_or(CKB_DEFAULT_MAX_BLOCK_CYCLES);
    let max_block_bytes = env_u64("CELLSCRIPT_CKB_MAX_BLOCK_BYTES").unwrap_or(CKB_DEFAULT_MAX_BLOCK_BYTES);
    let min_code_cell_data_capacity_shannons = (artifact_size_bytes as u64 + 8) * CKB_SHANNONS_PER_CKB;
    let recommended_code_cell_capacity_shannons = (artifact_size_bytes as u64 + 1_000) * CKB_SHANNONS_PER_CKB;
    let ckb_runtime_features = metadata.runtime.ckb_runtime_features.clone();
    let uses_input_since = ckb_runtime_features.iter().any(|feature| feature == "ckb-input-since");
    let uses_header_epoch = ckb_runtime_features.iter().any(|feature| feature.starts_with("ckb-header-epoch-"));
    let created_output_count = metadata
        .actions
        .iter()
        .map(|action| action.create_set.len())
        .chain(metadata.locks.iter().map(|lock| lock.create_set.len()))
        .sum();
    let mutated_output_count = metadata
        .actions
        .iter()
        .map(|action| action.mutate_set.len())
        .chain(metadata.locks.iter().map(|lock| lock.mutate_set.len()))
        .sum();
    let capacity_planning_required = created_output_count > 0 || mutated_output_count > 0;
    let declared_capacity_floors = metadata
        .types
        .iter()
        .filter_map(|ty| {
            ty.capacity_floor_shannons.map(|shannons| CkbCapacityFloorMetadata {
                type_name: ty.name.clone(),
                shannons,
                source: ty.capacity_floor_source.clone().unwrap_or_else(|| "dsl-with_capacity_floor".to_string()),
                status: "builder-must-preserve-output-capacity-at-or-above-floor".to_string(),
            })
        })
        .collect::<Vec<_>>();
    let has_declared_capacity_floors = !declared_capacity_floors.is_empty();
    CkbConstraintsMetadata {
        limits_source: ckb_limits_source(),
        profile_abi_contract: CkbProfileAbiContractMetadata {
            witness_abi: metadata.target_profile.witness_abi.clone(),
            lock_args_abi: metadata.target_profile.lock_args_abi.clone(),
            source_encoding: metadata.target_profile.source_encoding.clone(),
            spawn_ipc_abi: metadata.target_profile.spawn_ipc_abi.clone(),
            since_abi: metadata.target_profile.since_abi.clone(),
            cell_dep_abi: metadata.target_profile.cell_dep_abi.clone(),
            script_ref_abi: metadata.target_profile.script_ref_abi.clone(),
            output_data_abi: metadata.target_profile.output_data_abi.clone(),
            capacity_floor_abi: metadata.target_profile.capacity_floor_abi.clone(),
            type_id_abi: metadata.target_profile.type_id_abi.clone(),
            minimum_vm_version: metadata.target_profile.minimum_vm_version,
            riscv_isa: metadata.target_profile.riscv_isa.clone(),
            deployment_hash_types: metadata.target_profile.deployment_hash_types.clone(),
            tx_version: metadata.target_profile.tx_version,
        },
        hash_domain: "ckb-packed-molecule-blake2b".to_string(),
        script_hash_algorithm: "blake2b-256(personal=ckb-default-hash) over packed Script".to_string(),
        transaction_hash_algorithm: "blake2b-256(personal=ckb-default-hash) over packed RawTransaction".to_string(),
        sighash_algorithm: "ckb witness-sighash blake2b-256".to_string(),
        supported_script_hash_types: vec!["data".to_string(), "type".to_string(), "data1".to_string(), "data2".to_string()],
        declared_type_id_hash_type: CKB_TYPE_ID_HASH_TYPE.to_string(),
        hash_type_policy_surface: "compiler-declared-type-id-hash-type; builder must preserve script hash_type in deployed cells"
            .to_string(),
        hash_type_policy: CkbHashTypePolicyMetadata {
            source: "compiler-default".to_string(),
            default_script_hash_type: deployment_hash_type.clone(),
            declared_hash_type: None,
            type_id_hash_type: CKB_TYPE_ID_HASH_TYPE.to_string(),
            supported_hash_types: ckb_supported_hash_types(),
            status: format!("builder-must-use-{deployment_hash_type}-for-vm2-zbb-artifact"),
        },
        dep_group_manifest: CkbDepGroupManifestMetadata {
            source: "not-declared".to_string(),
            dep_group_supported: true,
            production_manifest_required: false,
            declared_cell_deps: Vec::new(),
            status: "no-cell-deps-declared".to_string(),
            warnings: Vec::new(),
        },
        script_references: ckb_script_reference_metadata(metadata),
        trusted_external_verifiers: metadata.runtime.trusted_external_verifiers.clone(),
        max_tx_verify_cycles,
        max_block_cycles,
        max_block_bytes,
        estimated_cycles,
        measured_cycles: None,
        cycles_status: "not-measured-by-compiler".to_string(),
        min_code_cell_data_capacity_shannons,
        recommended_code_cell_capacity_shannons,
        min_witness_bytes: ENTRY_WITNESS_ABI_MAGIC.len(),
        max_entry_witness_bytes,
        dry_run_required_for_production: true,
        tx_size_bytes: None,
        tx_size_measurement_required: true,
        tx_size_status: "builder-required".to_string(),
        occupied_capacity_measurement_required: capacity_planning_required,
        capacity_status: if capacity_planning_required {
            "builder-occupied-capacity-measurement-required".to_string()
        } else {
            "code-cell-data-lower-bound".to_string()
        },
        ckb_runtime_features: ckb_runtime_features.clone(),
        uses_input_since,
        uses_header_epoch,
        transaction_runtime_input_requirement_count: metadata.runtime.transaction_runtime_input_requirements.len(),
        timelock_policy_surface: if uses_input_since || uses_header_epoch {
            "runtime-metadata-visible; declarative-dsl-policy-not-yet-first-class".to_string()
        } else {
            "not-applicable".to_string()
        },
        timelock_policy: CkbTimelockPolicyMetadata {
            uses_input_since,
            uses_header_epoch,
            policy_kind: if uses_input_since || uses_header_epoch {
                "runtime-assertion-policy".to_string()
            } else {
                "not-applicable".to_string()
            },
            runtime_features: ckb_runtime_features
                .iter()
                .filter(|feature| feature.starts_with("ckb-header-epoch-") || feature.as_str() == "ckb-input-since")
                .cloned()
                .collect(),
            status: if uses_input_since || uses_header_epoch {
                "reported-runtime-policy-builder-must-preserve-since/header-context".to_string()
            } else {
                "not-applicable".to_string()
            },
        },
        created_output_count,
        mutated_output_count,
        capacity_planning_required,
        capacity_policy_surface: if has_declared_capacity_floors {
            "dsl-declared-capacity-floor; builder/runtime-required-for-change-and-measurement".to_string()
        } else if capacity_planning_required {
            "builder/runtime-required; declarative-dsl-capacity-not-yet-first-class".to_string()
        } else {
            "not-applicable".to_string()
        },
        declared_capacity_floors,
        capacity_evidence_contract: CkbCapacityEvidenceContractMetadata {
            required: true,
            code_cell_lower_bound_shannons: min_code_cell_data_capacity_shannons,
            recommended_code_cell_capacity_shannons,
            occupied_capacity_measurement_required: capacity_planning_required,
            tx_size_measurement_required: true,
            measured_occupied_capacity_shannons: None,
            measured_tx_size_bytes: None,
            status: if capacity_planning_required {
                "builder-must-attach-occupied-capacity-and-tx-size-evidence".to_string()
            } else {
                "builder-must-attach-tx-size-evidence-code-cell-lower-bound-available".to_string()
            },
        },
    }
}

pub(super) fn ckb_supported_hash_types() -> Vec<String> {
    vec!["data".to_string(), "type".to_string(), "data1".to_string(), "data2".to_string()]
}

pub(super) fn ckb_script_reference_metadata(metadata: &CompileMetadata) -> Vec<CkbScriptReferenceMetadata> {
    let mut refs = Vec::new();
    for action in &metadata.actions {
        collect_ckb_entry_script_references("action", &action.name, &action.create_set, &action.ckb_runtime_accesses, &mut refs);
    }
    for lock in &metadata.locks {
        collect_ckb_entry_script_references("lock", &lock.name, &lock.create_set, &lock.ckb_runtime_accesses, &mut refs);
    }
    refs.extend(metadata.runtime.trusted_external_verifiers.iter().map(|verifier| CkbScriptReferenceMetadata {
        scope: verifier.scope.clone(),
        purpose: format!("trusted-external-{}", verifier.operation),
        name: verifier.name.clone(),
        code_hash: Some(verifier.code_hash.clone()),
        hash_type: Some(verifier.hash_type.clone()),
        args: None,
        dep_source: "CellDep selected at runtime and bound by DATA_HASH".to_string(),
        profile: "ckb".to_string(),
        status: "trusted-external-identity-bound".to_string(),
    }));
    refs.sort_by(|left, right| (&left.scope, &left.purpose, &left.name).cmp(&(&right.scope, &right.purpose, &right.name)));
    refs.dedup_by(|left, right| left.scope == right.scope && left.purpose == right.purpose && left.name == right.name);
    refs
}

fn collect_ckb_entry_script_references(
    entry_kind: &str,
    entry_name: &str,
    create_set: &[CreatePatternMetadata],
    accesses: &[CkbRuntimeAccessMetadata],
    refs: &mut Vec<CkbScriptReferenceMetadata>,
) {
    let scope = format!("{}:{}", entry_kind, entry_name);
    for pattern in create_set {
        if let Some(type_id) = &pattern.ckb_type_id {
            refs.push(CkbScriptReferenceMetadata {
                scope: scope.clone(),
                purpose: "type-id-create-output".to_string(),
                name: pattern.ty.clone(),
                code_hash: Some(type_id.script_code_hash.clone()),
                hash_type: Some(type_id.hash_type.clone()),
                args: Some(type_id.args_source.clone()),
                dep_source: type_id.output_source.clone(),
                profile: "ckb".to_string(),
                status: "builder-must-install-type-id-script".to_string(),
            });
        }
    }
    for access in accesses {
        match access.operation.as_str() {
            "spawn" => refs.push(CkbScriptReferenceMetadata {
                scope: scope.clone(),
                purpose: "spawn-target".to_string(),
                name: access.binding.clone(),
                code_hash: None,
                hash_type: None,
                args: None,
                dep_source: "CellDep-or-DepGroup".to_string(),
                profile: "ckb".to_string(),
                status: "runtime-required-builder-resolved".to_string(),
            }),
            "read_ref" => refs.push(CkbScriptReferenceMetadata {
                scope: scope.clone(),
                purpose: "read-ref-cell-dep".to_string(),
                name: access.binding.clone(),
                code_hash: None,
                hash_type: None,
                args: None,
                dep_source: format!("CellDep#{}", access.index),
                profile: "ckb".to_string(),
                status: "checked-runtime-load-cell-dep".to_string(),
            }),
            _ => {}
        }
    }
}

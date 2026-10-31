//! Native evidence and bounded browser-summary construction from completed IR.
use super::*;

#[cfg(feature = "wasm")]
fn browser_type_metadata(type_def: &ir::IrTypeDef, type_defs: &BTreeMap<String, &ir::IrTypeDef>) -> TypeMetadata {
    TypeMetadata {
        name: type_def.name.clone(),
        type_id: None,
        default_hash_type: None,
        hash_type_source: if type_def.default_hash_type.is_some() {
            "dsl-with_default_hash_type".to_string()
        } else {
            "target-default".to_string()
        },
        capacity_floor_shannons: None,
        capacity_floor_source: None,
        type_id_hash: None,
        ckb_type_id: None,
        kind: format!("{:?}", type_def.kind),
        capability_set_version: ast::Capability::REGISTRY_VERSION,
        capabilities: ast::Capability::ALL
            .into_iter()
            .filter(|capability| type_def.capabilities.contains(capability))
            .map(|capability| metadata_capability_name(&capability))
            .collect(),
        claim_output: None,
        flow_states: Vec::new(),
        flow_state_field: None,
        flow_transitions: Vec::new(),
        flow_initial_state: None,
        flow_terminal_states: Vec::new(),
        flow_terminal_discharge: None,
        flow_terminal_evidence_tier: None,
        flow_state_model: None,
        flow_audit_warnings: Vec::new(),
        validity_predicates: Vec::new(),
        encoded_size: type_encoded_size(type_def, type_defs),
        fields: Vec::new(),
        molecule_schema: None,
        identity_policy: None,
    }
}

#[cfg(feature = "wasm")]
fn browser_param_metadata(param: &ir::IrParam) -> ParamMetadata {
    ParamMetadata {
        name: param.name.clone(),
        ty: ir_type_to_string(&param.ty),
        is_mut: false,
        is_ref: false,
        source: "default".to_string(),
        protected_spend_surface: false,
        witness_data_source: false,
        lock_args_data_source: false,
        cell_bound_abi: false,
        schema_pointer_abi: false,
        schema_length_abi: false,
        fixed_byte_pointer_abi: false,
        fixed_byte_length_abi: false,
        fixed_byte_len: None,
        type_hash_pointer_abi: false,
        type_hash_length_abi: false,
        type_hash_len: None,
        bounded_runtime_contract: None,
    }
}

#[cfg(feature = "wasm")]
fn browser_fail_closed_metadata(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> (Vec<String>, Vec<VerifierObligationMetadata>) {
    let mut module_features = BTreeSet::new();
    let mut obligations = Vec::new();
    for item in &ir.items {
        let (scope, body, params, return_type) = match item {
            ir::IrItem::Action(action) => {
                (format!("action:{}", action.name), &action.body, action.params.as_slice(), action.return_type.as_ref())
            }
            ir::IrItem::Function(function) => {
                (format!("fn:{}", function.name), &function.body, function.params.as_slice(), function.return_type.as_ref())
            }
            ir::IrItem::Lock(lock) => (format!("lock:{}", lock.name), &lock.body, lock.params.as_slice(), None),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        let param_schema_vars = schema_pointer_var_ids(body, params);
        let features = body_fail_closed_runtime_features(
            body,
            &param_schema_vars,
            type_layouts,
            params,
            cell_type_kinds,
            return_type,
            pure_const_returns,
        );
        for feature in features {
            module_features.insert(feature.clone());
            obligations.push(VerifierObligationMetadata {
                scope: scope.clone(),
                category: "runtime".to_string(),
                feature: feature.clone(),
                status: "fail-closed".to_string(),
                detail: executable_surface::experimental_artifact_admission_detail(&feature).map_or_else(
                    || format!("the browser compiler reports '{feature}' as an explicit fail-closed runtime path"),
                    str::to_owned,
                ),
            });
        }
    }
    (module_features.into_iter().collect(), obligations)
}

#[cfg(feature = "wasm")]
fn browser_action_metadata(
    ir: &ir::IrModule,
    action: &ir::IrAction,
    type_layouts: &MetadataTypeLayouts,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> ActionMetadata {
    let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
    let fail_closed_runtime_features = body_fail_closed_runtime_features(
        &action.body,
        &param_schema_vars,
        type_layouts,
        &action.params,
        cell_type_kinds,
        action.return_type.as_ref(),
        pure_const_returns,
    );
    let mut ckb_runtime_features = auto_lowered_aggregate_runtime_features_for_action(ir, action);
    ckb_runtime_features.extend(merge_ckb_runtime_features(
        body_ckb_runtime_features(&action.name, &action.body, cell_type_kinds, type_layouts),
        &action.params,
    ));
    let ckb_runtime_features = ckb_runtime_features.into_iter().collect::<Vec<_>>();
    ActionMetadata {
        name: action.name.clone(),
        params: action.params.iter().map(browser_param_metadata).collect(),
        return_type: None,
        effects: action.effects.clone(),
        parallelizable: false,
        touches_shared: Vec::new(),
        estimated_cycles: action.scheduler_hints.estimated_cycles,
        scheduler_witness_abi: String::new(),
        scheduler_witness_hex: String::new(),
        consume_set: action.body.consume_set.iter().map(cell_pattern_metadata).collect(),
        read_refs: action.body.read_refs.iter().map(cell_pattern_metadata).collect(),
        create_set: action
            .body
            .create_set
            .iter()
            .map(|pattern| CreatePatternMetadata {
                operation: String::new(),
                ty: pattern.ty.clone(),
                binding: pattern.binding.clone(),
                fields: Vec::new(),
                has_lock: false,
                identity_policy: None,
                ckb_output_data: None,
                ckb_type_id: None,
            })
            .collect(),
        mutate_set: action
            .body
            .mutate_set
            .iter()
            .map(|pattern| MutatePatternMetadata {
                operation: String::new(),
                ty: pattern.ty.clone(),
                binding: pattern.binding.clone(),
                fields: Vec::new(),
                preserved_fields: Vec::new(),
                input_source: String::new(),
                input_index: 0,
                output_source: String::new(),
                output_index: 0,
                preserve_type_hash: false,
                preserve_lock_hash: false,
                type_hash_preservation_status: String::new(),
                lock_hash_preservation_status: String::new(),
                field_equality_status: String::new(),
                field_transition_status: String::new(),
            })
            .collect(),
        state_transition_edges: Vec::new(),
        protocol_role_candidates: Vec::new(),
        pool_primitives: Vec::new(),
        ckb_runtime_accesses: Vec::new(),
        ckb_script_group: None,
        standalone_runner_compatible: ckb_runtime_features.is_empty() && action.params.is_empty(),
        ckb_runtime_features,
        fail_closed_runtime_features,
        verifier_obligations: Vec::new(),
        proof_plan: Vec::new(),
        transaction_runtime_input_requirements: Vec::new(),
        elf_compatible: true,
        block_count: action.body.blocks.len(),
    }
}

#[cfg(feature = "wasm")]
pub(super) fn compile_metadata_from_ir(
    ir: &ir::IrModule,
    artifact_format: ArtifactFormat,
    target_profile: TargetProfile,
    edition: CellScriptEdition,
    primitive_assurance: Option<&str>,
) -> CompileMetadata {
    let type_layouts = metadata_type_layouts(ir);
    let type_defs = metadata_type_defs_by_name(ir);
    let cell_type_kinds = metadata_cell_type_kinds(ir);
    let pure_const_returns = metadata_pure_const_returns(ir);
    let (fail_closed_runtime_features, verifier_obligations) =
        browser_fail_closed_metadata(ir, &type_layouts, &cell_type_kinds, &pure_const_returns);
    let mut types = ir.external_type_defs.iter().map(|type_def| browser_type_metadata(type_def, &type_defs)).collect::<Vec<_>>();
    types.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::TypeDef(type_def) => Some(browser_type_metadata(type_def, &type_defs)),
        _ => None,
    }));
    let actions = ir
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Action(action) => {
                Some(browser_action_metadata(ir, action, &type_layouts, &cell_type_kinds, &pure_const_returns))
            }
            _ => None,
        })
        .collect();
    CompileMetadata {
        metadata_schema_version: METADATA_SCHEMA_VERSION,
        source_metadata_schema_version: SOURCE_METADATA_SCHEMA_VERSION,
        artifact_metadata_schema_version: ARTIFACT_METADATA_SCHEMA_VERSION,
        constraints_metadata_schema_version: CONSTRAINTS_METADATA_SCHEMA_VERSION,
        compiler_version: VERSION.to_string(),
        edition,
        compatibility_profile: resolve_compatibility_profile(edition, target_profile.name(), primitive_assurance),
        module: ir.name.clone(),
        public_interface: interface::PackageInterface::default(),
        interface_hash: String::new(),
        typed_semantics: cellscript_artifact_checker::TypedSemanticRecord::default(),
        typed_semantics_hash: String::new(),
        artifact_format: artifact_format.display_name().to_string(),
        target_profile: target_profile.metadata(artifact_format),
        artifact_hash: None,
        artifact_size_bytes: None,
        source_hash: None,
        source_content_hash: None,
        source_units: Vec::new(),
        verified_artifact: VerifiedArtifactMetadata::default(),
        lowering: LoweringMetadata {
            protocol_semantics: String::new(),
            assembly_path: String::new(),
            elf_path: String::new(),
            semantics_preserving_claim: String::new(),
        },
        capability_registry: CapabilityRegistryMetadata::default(),
        enum_layouts: Vec::new(),
        generic_instantiations: Vec::new(),
        runtime: RuntimeMetadata {
            vm_target: String::new(),
            vm_version: String::new(),
            syscall_abi: String::new(),
            ckb_runtime_view_contract: CKB_RUNTIME_VIEW_CONTRACT.to_string(),
            ckb_runtime_access_provenance_contract: CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT.to_string(),
            vm_abi: VmAbiMetadata {
                format: String::new(),
                version: 0,
                default: false,
                embedded_in_artifact: false,
                scope: String::new(),
                selection: String::new(),
            },
            pure_elf_runner: String::new(),
            ckb_runtime_required: false,
            ckb_runtime_features: Vec::new(),
            standalone_runner_compatible: false,
            fail_closed_runtime_features,
            ckb_runtime_accesses: Vec::new(),
            signing_message_domains: ckb_signing_message_domains(ir),
            zk_verifiers: Vec::new(),
            trusted_external_verifiers: Vec::new(),
            transaction_view_handles: Vec::new(),
            borrow_regions: Vec::new(),
            capability_proofs: Vec::new(),
            verifier_obligations,
            proof_plan: Vec::new(),
            proof_plan_soundness: ProofPlanSoundnessReport::default(),
            builder_assumptions: Vec::new(),
            collection_instantiations: Vec::new(),
            transaction_runtime_input_requirements: Vec::new(),
            pool_primitives: Vec::new(),
            fungible_type_group_entry: None,
            policy_artifact: None,
        },
        constraints: ConstraintsMetadata::default(),
        molecule_schema_manifest: MoleculeSchemaManifestMetadata::default(),
        cell_data_codec_manifest: CellDataCodecManifestMetadata::default(),
        template_layouts: Vec::new(),
        types,
        actions,
        functions: Vec::new(),
        locks: Vec::new(),
        debug_info_sections: Vec::new(),
    }
}

#[cfg(not(feature = "wasm"))]
pub(super) fn compile_metadata_from_ir(
    ir: &ir::IrModule,
    artifact_format: ArtifactFormat,
    target_profile: TargetProfile,
    edition: CellScriptEdition,
    primitive_assurance: Option<&str>,
) -> CompileMetadata {
    let type_layouts = metadata_type_layouts(ir);
    let type_defs = metadata_type_defs_by_name(ir);
    let flow_states = metadata_flow_states(ir);
    let flow_state_fields = metadata_flow_state_fields(ir);
    let cell_type_kinds = metadata_cell_type_kinds(ir);
    let pure_const_returns = metadata_pure_const_returns(ir);
    let fail_closed_runtime_features = module_fail_closed_runtime_features(ir, &type_layouts, &cell_type_kinds, &pure_const_returns);
    let ckb_runtime_features = module_ckb_runtime_features(ir, &cell_type_kinds, &type_layouts);
    let ckb_runtime_accesses = module_ckb_runtime_accesses(ir, &cell_type_kinds, &type_layouts);
    let verifier_obligations =
        module_verifier_obligations(ir, &type_layouts, &flow_states, &flow_state_fields, &cell_type_kinds, &pure_const_returns);
    let proof_plan = module_proof_plan_metadata(
        ir,
        &type_layouts,
        &flow_states,
        &flow_state_fields,
        &cell_type_kinds,
        &pure_const_returns,
        &ckb_runtime_accesses,
    );
    let collection_instantiations = module_collection_instantiation_metadata(ir, &type_layouts, &pure_const_returns);
    let transaction_runtime_input_requirements = transaction_runtime_input_requirements_from_obligations(&verifier_obligations);
    let pool_primitives = module_pool_primitive_metadata(ir, &type_layouts, &cell_type_kinds, &pure_const_returns);
    let has_entry_params = module_has_entry_params(ir);
    let ckb_runtime_required = !ckb_runtime_features.is_empty();
    let standalone_runner_compatible = !ckb_runtime_required && !has_entry_params;
    let embeds_vm_abi_trailer = target_profile.embeds_vm_abi_trailer(artifact_format);
    let validity_paths = module_validity_path_coverage(ir);
    let mut types = ir
        .external_type_defs
        .iter()
        .map(|type_def| {
            type_metadata(
                type_def,
                &type_defs,
                target_profile,
                validity_paths.selected_create_count(&type_def.name),
                validity_paths.checked_create_count(&type_def.name),
                validity_paths.update_count(&type_def.name),
            )
        })
        .collect::<Vec<_>>();
    types.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::TypeDef(type_def) => Some(type_metadata(
            type_def,
            &type_defs,
            target_profile,
            validity_paths.selected_create_count(&type_def.name),
            validity_paths.checked_create_count(&type_def.name),
            validity_paths.update_count(&type_def.name),
        )),
        _ => None,
    }));
    let molecule_schema_manifest = molecule_schema_manifest_metadata(&types, target_profile);
    let cell_data_codec_manifest = cell_data_codec_manifest_metadata(&ckb_runtime_accesses, &molecule_schema_manifest, target_profile);
    let transaction_view_handles = transaction_view_handle_metadata(ir);
    let borrow_regions = borrow_region_metadata(ir);
    let capability_proofs = capability_proof_metadata(ir);
    let compatibility_profile = resolve_compatibility_profile(edition, target_profile.name(), primitive_assurance);
    #[cfg(not(feature = "wasm"))]
    let generic_instantiations = generic_instantiation_metadata(ir);
    #[cfg(feature = "wasm")]
    let generic_instantiations = Vec::new();
    let mut metadata = CompileMetadata {
        metadata_schema_version: METADATA_SCHEMA_VERSION,
        source_metadata_schema_version: SOURCE_METADATA_SCHEMA_VERSION,
        artifact_metadata_schema_version: ARTIFACT_METADATA_SCHEMA_VERSION,
        constraints_metadata_schema_version: CONSTRAINTS_METADATA_SCHEMA_VERSION,
        compiler_version: VERSION.to_string(),
        edition,
        compatibility_profile,
        module: ir.name.clone(),
        public_interface: interface::PackageInterface::default(),
        interface_hash: String::new(),
        typed_semantics: cellscript_artifact_checker::TypedSemanticRecord::default(),
        typed_semantics_hash: String::new(),
        artifact_format: artifact_format.display_name().to_string(),
        target_profile: target_profile.metadata(artifact_format),
        artifact_hash: None,
        artifact_size_bytes: None,
        source_hash: None,
        source_content_hash: None,
        source_units: Vec::new(),
        verified_artifact: VerifiedArtifactMetadata::default(),
        lowering: LoweringMetadata {
            protocol_semantics: "CellScript IR records consume/read_ref/create summaries before RISC-V codegen".to_string(),
            assembly_path: "riscv64-asm emits executable CKB-style syscall paths plus metadata for verifier obligations".to_string(),
            elf_path: "riscv64-elf is always enabled; all operations have real RISC-V lowerings or fail-closed traps with specific error codes".to_string(),
            semantics_preserving_claim:
                "Pure computation lowering is executable; stateful protocol lowering is represented in metadata and asm but is not yet a proved schema decoder/verifier"
                    .to_string(),
        },
        capability_registry: capability_registry_metadata(),
        enum_layouts: payload_enum_layout_metadata(ir),
        generic_instantiations,
        runtime: RuntimeMetadata {
            vm_target: "CKB-VM compatible RISC-V 64 IMC+B+MOP".to_string(),
            vm_version: "VERSION2".to_string(),
            syscall_abi: "CKB store_data ABI: A0=buffer, A1=size pointer, A2=offset, A3=index, A4=source".to_string(),
            ckb_runtime_view_contract: CKB_RUNTIME_VIEW_CONTRACT.to_string(),
            ckb_runtime_access_provenance_contract: CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT.to_string(),
            vm_abi: VmAbiMetadata {
                format: "molecule".to_string(),
                version: MOLECULE_VM_ABI_VERSION,
                default: true,
                embedded_in_artifact: embeds_vm_abi_trailer,
                scope: "CKB-style full object load syscalls: LOAD_SCRIPT, LOAD_INPUT, LOAD_CELL, LOAD_HEADER".to_string(),
                selection: if embeds_vm_abi_trailer {
                    "RISC-V ELF artifacts embed a fixed VM ABI trailer; verifier callers strip the trailer and select the declared ABI"
                        .to_string()
                } else if target_profile.is_ckb() && artifact_format == ArtifactFormat::RiscvElf {
                    "CKB-target ELF artifacts do not embed VM ABI trailer bytes; the profile selects CKB Molecule ABI out of band"
                        .to_string()
                } else {
                    "Compiler artifact metadata declares the required VM object ABI; verifier callers must pass this ABI to the runtime"
                        .to_string()
                },
            },
            pure_elf_runner: "cellc run --features vm-runner executes no-argument pure ELF with ckb-vm 0.24".to_string(),
            ckb_runtime_required,
            ckb_runtime_features,
            standalone_runner_compatible,
            fail_closed_runtime_features,
            ckb_runtime_accesses,
            signing_message_domains: ckb_signing_message_domains(ir),
            zk_verifiers: Vec::new(),
            trusted_external_verifiers: Vec::new(),
            transaction_view_handles,
            borrow_regions,
            capability_proofs,
            verifier_obligations,
            proof_plan,
            proof_plan_soundness: ProofPlanSoundnessReport::default(),
            builder_assumptions: Vec::new(),
            collection_instantiations,
            transaction_runtime_input_requirements,
            pool_primitives,
            fungible_type_group_entry: fungible_type_group_entry_metadata(ir),
            policy_artifact: None,
        },
        constraints: ConstraintsMetadata::default(),
        molecule_schema_manifest,
        cell_data_codec_manifest,
        template_layouts: template_layout_metadata(&types),
        types,
        actions: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::Action(action) => {
                    let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                    let fail_closed_runtime_features = body_fail_closed_runtime_features(
                        &action.body,
                        &param_schema_vars,
                        &type_layouts,
                        &action.params,
                        &cell_type_kinds,
                        action.return_type.as_ref(),
                        &pure_const_returns,
                    );
                    let mut ckb_runtime_features = auto_lowered_aggregate_runtime_features_for_action(ir, action);
                    ckb_runtime_features.extend(merge_ckb_runtime_features(
                        body_ckb_runtime_features(&action.name, &action.body, &cell_type_kinds, &type_layouts),
                        &action.params,
                    ));
                    let ckb_runtime_features = ckb_runtime_features.into_iter().collect::<Vec<_>>();
                    let mut ckb_runtime_accesses = auto_lowered_aggregate_runtime_accesses_for_action(ir, action);
                    ckb_runtime_accesses.extend(merge_ckb_runtime_accesses(
                        body_ckb_runtime_accesses(&action.name, &action.body, &cell_type_kinds, &type_layouts),
                        &action.params,
                    ));
                    let mut verifier_obligations = body_verifier_obligations(
                        "action",
                        &action.name,
                        &action.body,
                        &fail_closed_runtime_features,
                        &ckb_runtime_features,
                        &ckb_runtime_accesses,
                        &type_layouts,
                        &action.params,
                        &flow_states,
                        &flow_state_fields,
                        &cell_type_kinds,
                        action.return_type.as_ref(),
                        &pure_const_returns,
                    );
                    verifier_obligations.extend(action_terminal_flow_obligations(ir, action));
                    let pool_primitives = body_pool_primitive_metadata(
                        "action",
                        &action.name,
                        &action.body,
                        &action.params,
                        &type_layouts,
                        &cell_type_kinds,
                        &pure_const_returns,
                    );
                    let transaction_runtime_input_requirements =
                        transaction_runtime_input_requirements_from_obligations(&verifier_obligations);
                    let proof_plan = crate::proof_plan::build_for_body(
                        "action",
                        &action.name,
                        &action.body,
                        &action.params,
                        &verifier_obligations,
                        &ckb_runtime_accesses,
                        &pool_primitives,
                    );
                    let standalone_runner_compatible = ckb_runtime_features.is_empty() && action.params.is_empty();
                    let scheduler_accesses = scheduler_accesses_from_metadata(&ckb_runtime_accesses);
                    let scheduler_effects = action.effects.clone();
                    let scheduler_witness_molecule = crate::stdlib::SchedulerMetadata::generate(
                        &scheduler_effects,
                        action.scheduler_hints.parallelizable,
                        action.scheduler_hints.touches_shared.clone(),
                        action.scheduler_hints.estimated_cycles,
                        scheduler_accesses,
                    );
                    let scheduler_witness_hex = hex_bytes(&scheduler_witness_molecule);
                    Some(ActionMetadata {
                        name: action.name.clone(),
                        params: param_metadata_for_body(
                            &action.params,
                            &action.body,
                            &cell_type_kinds,
                            &ir.enum_layouts,
                        ),
                        return_type: action.return_type.as_ref().map(ir_type_to_string),
                        effects: scheduler_effects,
                        parallelizable: action.scheduler_hints.parallelizable,
                        touches_shared: action.scheduler_hints.touches_shared.iter().map(hex_hash).collect(),
                        estimated_cycles: action.scheduler_hints.estimated_cycles,
                        scheduler_witness_abi: SCHEDULER_WITNESS_ABI_MOLECULE.to_string(),
                        scheduler_witness_hex,
                                        consume_set: action.body.consume_set.iter().map(cell_pattern_metadata).collect(),
                        read_refs: action.body.read_refs.iter().map(cell_pattern_metadata).collect(),
                        create_set: action
                            .body
                            .create_set
                            .iter()
                            .enumerate()
                            .map(|(index, pattern)| create_pattern_metadata(pattern, index, &type_defs, target_profile))
                            .collect(),
                        mutate_set: action.body.mutate_set.iter().map(|pattern| mutate_pattern_metadata(pattern, &type_layouts)).collect(),
                        state_transition_edges: action
                            .state_transition_edges
                            .iter()
                            .map(state_transition_edge_metadata)
                            .collect(),
                        protocol_role_candidates: action
                            .protocol_role_candidates
                            .iter()
                            .map(protocol_role_candidate_metadata)
                            .collect(),
                        pool_primitives,
                        ckb_script_group: ckb_script_group_metadata("action", &ckb_runtime_accesses, target_profile),
                        ckb_runtime_accesses,
                        ckb_runtime_features,
                        elf_compatible: true,
                        standalone_runner_compatible,
                        fail_closed_runtime_features,
                        verifier_obligations,
                        proof_plan,
                        transaction_runtime_input_requirements,
                        block_count: action.body.blocks.len(),
                    })
                }
                _ => None,
            })
            .collect(),
        functions: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::Function(function) => {
                    let param_schema_vars = schema_pointer_var_ids(&function.body, &function.params);
                    let fail_closed_runtime_features = body_fail_closed_runtime_features(
                        &function.body,
                        &param_schema_vars,
                        &type_layouts,
                        &function.params,
                        &cell_type_kinds,
                        function.return_type.as_ref(),
                        &pure_const_returns,
                    );
                    let ckb_runtime_features = merge_ckb_runtime_features(
                        body_ckb_runtime_features(&function.name, &function.body, &cell_type_kinds, &type_layouts),
                        &function.params,
                    );
                    let ckb_runtime_accesses = merge_ckb_runtime_accesses(
                        body_ckb_runtime_accesses(&function.name, &function.body, &cell_type_kinds, &type_layouts),
                        &function.params,
                    );
                    let verifier_obligations = body_verifier_obligations(
                        "fn",
                        &function.name,
                        &function.body,
                        &fail_closed_runtime_features,
                        &ckb_runtime_features,
                        &ckb_runtime_accesses,
                        &type_layouts,
                        &function.params,
                        &flow_states,
                        &flow_state_fields,
                        &cell_type_kinds,
                        function.return_type.as_ref(),
                        &pure_const_returns,
                    );
                    let pool_primitives = body_pool_primitive_metadata(
                        "fn",
                        &function.name,
                        &function.body,
                        &function.params,
                        &type_layouts,
                        &cell_type_kinds,
                        &pure_const_returns,
                    );
                    let transaction_runtime_input_requirements =
                        transaction_runtime_input_requirements_from_obligations(&verifier_obligations);
                    let proof_plan = crate::proof_plan::build_for_body(
                        "fn",
                        &function.name,
                        &function.body,
                        &function.params,
                        &verifier_obligations,
                        &ckb_runtime_accesses,
                        &pool_primitives,
                    );
                    Some(FunctionMetadata {
                        name: function.name.clone(),
                        params: param_metadata_for_body(
                            &function.params,
                            &function.body,
                            &cell_type_kinds,
                            &ir.enum_layouts,
                        ),
                        return_type: function.return_type.as_ref().map(ir_type_to_string),
                        effects: function.effects.clone(),
                        effect_evidence_tier: EvidenceTier::CheckedStatic,
                        mutate_set: function.body.mutate_set.iter().map(|pattern| mutate_pattern_metadata(pattern, &type_layouts)).collect(),
                        pool_primitives,
                        ckb_runtime_accesses,
                        ckb_runtime_features,
                        elf_compatible: true,
                        fail_closed_runtime_features,
                        verifier_obligations,
                        proof_plan,
                        transaction_runtime_input_requirements,
                        block_count: function.body.blocks.len(),
                    })
                }
                _ => None,
            })
            .collect(),
        locks: ir
            .items
            .iter()
            .filter_map(|item| match item {
                ir::IrItem::Lock(lock) => {
                    let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                    let fail_closed_runtime_features = body_fail_closed_runtime_features(
                        &lock.body,
                        &param_schema_vars,
                        &type_layouts,
                        &lock.params,
                        &cell_type_kinds,
                        None,
                        &pure_const_returns,
                    );
                    let ckb_runtime_features =
                        merge_ckb_runtime_features(body_ckb_runtime_features(&lock.name, &lock.body, &cell_type_kinds, &type_layouts), &lock.params);
                    let ckb_runtime_accesses =
                        merge_ckb_runtime_accesses(body_ckb_runtime_accesses(&lock.name, &lock.body, &cell_type_kinds, &type_layouts), &lock.params);
                    let verifier_obligations = body_verifier_obligations(
                        "lock",
                        &lock.name,
                        &lock.body,
                        &fail_closed_runtime_features,
                        &ckb_runtime_features,
                        &ckb_runtime_accesses,
                        &type_layouts,
                        &lock.params,
                        &flow_states,
                        &flow_state_fields,
                        &cell_type_kinds,
                        None,
                        &pure_const_returns,
                    );
                    let pool_primitives =
                        body_pool_primitive_metadata("lock", &lock.name, &lock.body, &lock.params, &type_layouts, &cell_type_kinds, &pure_const_returns);
                    let transaction_runtime_input_requirements =
                        transaction_runtime_input_requirements_from_obligations(&verifier_obligations);
                    let proof_plan = crate::proof_plan::build_for_body(
                        "lock",
                        &lock.name,
                        &lock.body,
                        &lock.params,
                        &verifier_obligations,
                        &ckb_runtime_accesses,
                        &pool_primitives,
                    );
                    let standalone_runner_compatible = ckb_runtime_features.is_empty() && lock.params.is_empty();
                    Some(LockMetadata {
                        name: lock.name.clone(),
                        effects: lock.effects.clone(),
                        params: param_metadata_for_body(
                            &lock.params,
                            &lock.body,
                            &cell_type_kinds,
                            &ir.enum_layouts,
                        ),
                        consume_set: lock.body.consume_set.iter().map(cell_pattern_metadata).collect(),
                        read_refs: lock.body.read_refs.iter().map(cell_pattern_metadata).collect(),
                        create_set: lock
                            .body
                            .create_set
                            .iter()
                            .enumerate()
                            .map(|(index, pattern)| create_pattern_metadata(pattern, index, &type_defs, target_profile))
                            .collect(),
                        mutate_set: lock.body.mutate_set.iter().map(|pattern| mutate_pattern_metadata(pattern, &type_layouts)).collect(),
                        pool_primitives,
                        ckb_script_group: ckb_script_group_metadata("lock", &ckb_runtime_accesses, target_profile),
                        ckb_runtime_accesses,
                        ckb_runtime_features,
                        elf_compatible: true,
                        standalone_runner_compatible,
                        fail_closed_runtime_features,
                        verifier_obligations,
                        proof_plan,
                        transaction_runtime_input_requirements,
                        block_count: lock.body.blocks.len(),
                    })
                }
                _ => None,
            })
            .collect(),
        debug_info_sections: Vec::new(),
    };
    artifact::bind_policy_metadata(&mut metadata, ir).expect("validated IR entry has canonical policy metadata");
    metadata.runtime.builder_assumptions = crate::assumptions::builder_assumptions_from_metadata(&metadata);
    metadata.runtime.proof_plan_soundness = crate::proof_plan::soundness::check_metadata(&metadata, false);
    metadata
}

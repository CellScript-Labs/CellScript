use super::super::output::CommandOutcome;
use super::entry_witness_values::{parse_entry_witness_arg, select_entry_witness_metadata};
use super::intent_records::{action_scan_selectors_json, cellfabric_app_conflict_key_templates, cellfabric_intent_envelope_json};
use super::typescript_builder::{default_builder_package_name, write_typescript_builder_package};
use super::{
    hash_json_value, policy_entry_witness, read_metadata_json, selected_builder_actions, verify_builder_deployment_identity,
    verify_builder_lockfile_identity, ActionBuildArgs, CommandExecutor, EntryWitnessArgs, GenBuilderArgs,
};
use crate::error::{CompileError, Result};
use crate::{
    compile_path, ArtifactFormat, CompileOptions, ExecutableSurfacePolicy, ENTRY_WITNESS_ABI, ENTRY_WITNESS_PLACEMENT_ABI,
    ENTRY_WITNESS_PLACEMENT_FIELD, ENTRY_WITNESS_PLACEMENT_SOURCE,
};
use camino::Utf8Path;
use colored::Colorize;
use std::path::PathBuf;

impl CommandExecutor {
    pub(super) fn action_build(args: ActionBuildArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile.or_else(|| Some("ckb".to_string())),
                primitive_compat: None,
            },
        )?;

        let action = if let Some(name) = args.action.as_deref() {
            result
                .metadata
                .actions
                .iter()
                .find(|action| action.name == name)
                .ok_or_else(|| crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", name)))?
        } else {
            result
                .metadata
                .actions
                .first()
                .ok_or_else(|| crate::error::CompileError::without_span("no actions found in compiled metadata"))?
        };
        let entry_constraints =
            result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_kind == "action" && entry.entry_name == action.name);

        let ckb = result.metadata.constraints.ckb.as_ref();
        let metadata_bytes = serde_json::to_vec(&result.metadata).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize metadata for digest: {}", error))
        })?;
        let metadata_hash = crate::hex_encode(&crate::ckb_blake2b256(&metadata_bytes));
        let ckb_contract = ckb.map(|ckb| {
            serde_json::json!({
                "hash_type_policy": ckb.hash_type_policy,
                "capacity_evidence_contract": ckb.capacity_evidence_contract,
                "timelock_policy": ckb.timelock_policy,
                "tx_size_measurement_required": ckb.tx_size_measurement_required,
                "occupied_capacity_measurement_required": ckb.occupied_capacity_measurement_required,
                "dry_run_required_for_production": ckb.dry_run_required_for_production,
            })
        });
        let action_scan_selectors = action_scan_selectors_json(action);
        let bounded_output_plans = result
            .metadata
            .runtime
            .collection_instantiations
            .iter()
            .filter(|collection| collection.scope_kind == "action" && collection.scope_name == action.name)
            .filter_map(|collection| collection.bounded_output_plan.as_ref())
            .collect::<Vec<_>>();
        let transaction_draft = serde_json::json!({
            "format": "cellscript-ccc-transaction-draft-v1",
            "state": "ActionPlan",
            "status": "template",
            "ccc_compatible": true,
            "can_submit": false,
            "ckb_vm_execution": false,
            "tx_pool_acceptance": false,
            "requires_live_cell_resolution": true,
            "requires_packed_materialization": true,
            "packed_materialization": {
                "transaction": "ckb_types::packed::Transaction",
                "cell_output": "ckb_types::packed::CellOutput",
                "cell_dep": "ckb_types::packed::CellDep",
                "out_point": "ckb_types::packed::OutPoint",
                "script": "ckb_types::packed::Script",
                "witness_args": "ckb_types::packed::WitnessArgs",
                "realizer": "cellscript-ckb-adapter via ckb-sdk-rust or CCC",
            },
            "cell_deps": [],
            "header_deps": [],
            "inputs": [],
            "outputs": [],
            "outputs_data": [],
            "witnesses": [],
            "bounded_output_plan_evidence": [],
            "required_evidence": [
                "live_cell_resolution",
                "outputs_data_pairing",
                "witness_args_placement",
                "celldep_resolution",
                "occupied_capacity",
                "fee_and_change",
                "estimate_cycles",
                "tx_pool_acceptance"
            ],
            "notes": [
                "This is a headless draft template produced from compiler metadata.",
                "A builder adapter must resolve live cells, fill args, calculate fees/capacity, dry-run, sign, and submit."
            ]
        });
        let resolved_tx_required_fields = serde_json::json!([
            "schema",
            "state",
            "metadata_hash",
            "artifact_hash",
            "deployment_ref",
            "action_selector",
            "inputs",
            "outputs",
            "outputs_data",
            "witnesses",
            "cell_deps",
            "header_deps",
            "capacity_evidence",
            "fee_policy",
            "change_policy",
            "lineage"
        ]);
        let acceptance_report_template = serde_json::json!({
            "schema": "cellscript-ckb-action-acceptance-report-v0.19",
            "state": "AcceptedActionTx",
            "metadata_hash": metadata_hash,
            "artifact_hash": result.metadata.artifact_hash,
            "deployment_ref": serde_json::Value::Null,
            "action_selector": action.name,
            "ckb_vm_execution": serde_json::Value::Null,
            "estimate_cycles": serde_json::Value::Null,
            "tx_pool_acceptance": serde_json::Value::Null,
            "submitted_tx_hash": serde_json::Value::Null,
            "serialized_tx_size_bytes": serde_json::Value::Null,
            "occupied_capacity_shannons": serde_json::Value::Null,
            "fee_shannons": serde_json::Value::Null,
            "lineage": [],
            "known_limitations": [
                "Template only: adapter must fill live cells, deployment refs, packed transaction bytes, signer policy, and node evidence."
            ],
        });
        let adapter_contract = serde_json::json!({
            "schema": "cellscript-ckb-adapter-contract-v0.19",
            "headless": true,
            "compiler_core_dependency": "no-ckb-sdk-rust",
            "compiler_output_state": "ActionPlan",
            "adapter_output_state": "ResolvedActionTx",
            "accepted_output_state": "AcceptedActionTx",
            "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
            "must_not_infer_protocol_semantics_from_action_name": true,
            "must_keep_signer_authority_explicit": true,
            "must_preserve_outputs_outputs_data_pairing": true,
            "must_emit_lineage": true,
            "witness_policy": {
                "entry_payload_abi": ENTRY_WITNESS_ABI,
                "placement_abi": ENTRY_WITNESS_PLACEMENT_ABI,
                "entry_payload_owner": "compiler",
                "final_witness_args_owner": "adapter",
                "default_action_payload_field": ENTRY_WITNESS_PLACEMENT_FIELD,
                "runtime_source": ENTRY_WITNESS_PLACEMENT_SOURCE,
                "raw_v1_compatible": false,
                "lock_signature_policy": "explicit-adapter-owned-do-not-overwrite",
                "placement_requires_deployment_role": true,
            },
            "acceptance_methods": ["estimate_cycles", "test_tx_pool_accept", "send_transaction_optional"],
            "not_proven_by_this_plan": ["live_cell_availability", "ckb_vm_execution", "tx_pool_acceptance", "submission"],
            "resolved_tx_required_fields": resolved_tx_required_fields,
            "acceptance_report_template": acceptance_report_template,
        });
        let preview = serde_json::json!({
            "format": "cellscript-action-preview-v1",
            "action": action.name,
            "summary": format!("Build a CKB transaction for CellScript action {}", action.name),
            "consumes": action.transaction_runtime_input_requirements,
            "creates": action.create_set,
            "transitions": action.mutate_set,
            "witnesses": {
                "selector": action.name,
                "args": action.params,
            },
            "warnings": [
                "Builder preview is metadata-backed; live cell freshness and final fee/capacity must be checked at build time."
            ],
            "estimatedFee": serde_json::Value::Null,
            "requiredSigners": []
        });
        let plan = serde_json::json!({
            "status": "ok",
            "policy": "cellscript-action-builder-plan-v1",
            "headless": true,
            "ui_scope": "none",
            "input": input_path.display().to_string(),
            "action": action.name,
            "target_profile": result.metadata.target_profile.name,
            "artifact_hash": result.metadata.artifact_hash,
            "entry_witness_abi": {
                "required": !action.params.is_empty(),
                "params": action.params,
                "constraints": entry_constraints,
            },
            "builder_requirements": {
                "created_outputs": action.create_set,
                "mutated_outputs": action.mutate_set,
                "read_refs": action.read_refs,
                "verifier_obligations": action.verifier_obligations,
                "runtime_input_requirements": action.transaction_runtime_input_requirements,
                "action_scan_selectors": action_scan_selectors,
                "fail_closed_runtime_features": action.fail_closed_runtime_features,
                "bounded_output_plans": bounded_output_plans,
            },
            "ckb": ckb_contract,
            "transaction_draft": transaction_draft,
            "adapter_contract": adapter_contract,
            "action_scan_selectors": action_scan_selectors,
            "bounded_output_plans": bounded_output_plans,
            "preview": preview,
            "constraints_status": result.metadata.constraints.status,
            "constraints_failures": result.metadata.constraints.failures,
            "constraints_warnings": result.metadata.constraints.warnings,
        });
        let output_value = if args.fabric_intent {
            cellfabric_intent_envelope_json(&result.metadata, action, &plan, &input_path, &metadata_hash)?
        } else {
            plan
        };
        let json = serde_json::to_string_pretty(&output_value).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize action build output: {}", error))
        })?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            let label = if args.fabric_intent { "CellFabric intent envelope generated" } else { "Action build plan generated" };
            println!("{}", label.green());
            println!("  Output: {}", output_path.display());
        } else if args.json {
            println!("{}", json);
        } else if args.fabric_intent {
            println!("CellFabric intent envelope: {}", action.name);
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Status: requires-runtime-binding");
            println!("  App conflict key templates: {}", cellfabric_app_conflict_key_templates(&result.metadata.module, action).len());
            println!("  Embedded action plan: yes");
        } else {
            println!("Action build plan: {}", action.name);
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Constraints: {}", result.metadata.constraints.status);
            println!("  Created outputs: {}", action.create_set.len());
            println!("  Mutated outputs: {}", action.mutate_set.len());
            println!("  Runtime input requirements: {}", action.transaction_runtime_input_requirements.len());
        }
        Ok(())
    }

    pub(super) fn gen_builder(args: GenBuilderArgs) -> Result<()> {
        if args.target != "typescript" {
            return Err(crate::error::CompileError::without_span(format!(
                "unsupported builder target '{}'; supported targets: typescript",
                args.target
            )));
        }

        let metadata = if let Some(metadata_path) = args.metadata.as_deref() {
            read_metadata_json(metadata_path)?
        } else {
            let input_path = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
            let input = Utf8Path::from_path(&input_path).ok_or_else(|| {
                crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display()))
            })?;
            let options = CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: None,
                target_profile: args.target_profile.clone().or_else(|| Some("ckb".to_string())),
                primitive_compat: None,
            };
            if let Some(name) = args.artifact.as_deref() {
                crate::compile_path_with_artifact_name(input, options, name, ExecutableSurfacePolicy::DenyFailClosed)?.metadata
            } else {
                compile_path(input, options)?.metadata
            }
        };

        if args.artifact.is_some() || metadata.runtime.policy_artifact.is_some() {
            crate::validate_compile_metadata(&metadata, ArtifactFormat::from_display_name(&metadata.artifact_format)?)?;
            let policy = metadata.runtime.policy_artifact.as_ref().ok_or_else(|| {
                CompileError::without_span("gen-builder --artifact requires metadata for an explicitly selected policy artifact")
            })?;
            if args.artifact.as_deref().is_some_and(|name| name != policy.declaration.name) {
                return Err(CompileError::without_span("gen-builder --artifact does not match the selected metadata declaration"));
            }
        }

        let metadata_hash = hash_json_value("metadata", &metadata)?;
        let selected_actions = selected_builder_actions(&metadata, args.action.as_deref())?;
        let locked_identity = if let Some(lockfile_path) = args.lockfile.as_deref() {
            Some(verify_builder_lockfile_identity(lockfile_path, &metadata, &metadata_hash)?)
        } else {
            None
        };
        let deployment_identity = if let Some(deployed_path) = args.deployed.as_deref() {
            let lockfile_path = args.lockfile.as_deref().ok_or_else(|| {
                crate::error::CompileError::without_span("gen-builder --deployed requires --lockfile for deployment identity binding")
            })?;
            Some(verify_builder_deployment_identity(
                lockfile_path,
                deployed_path,
                &metadata,
                &metadata_hash,
                args.deployment_network.as_deref(),
            )?)
        } else {
            None
        };
        let output_dir = args.output.unwrap_or_else(|| PathBuf::from("target").join("cellscript-builder").join("typescript"));
        let package_name = args.package_name.unwrap_or_else(|| default_builder_package_name(&metadata));
        let summary = write_typescript_builder_package(
            &output_dir,
            &package_name,
            &metadata,
            &metadata_hash,
            &selected_actions,
            locked_identity.as_ref(),
            deployment_identity.as_ref(),
            args.lockfile.as_deref(),
            args.deployed.as_deref(),
        )?;

        CommandOutcome {
            machine: summary,
            human_lines: vec![
                "TypeScript action builder generated".green().to_string(),
                format!("  Output: {}", output_dir.display()),
                format!("  Package: {}", package_name),
                format!("  Actions: {}", selected_actions.len()),
            ],
        }
        .emit(args.json)
    }

    /// Encode witness bytes for the generated `_cellscript_entry` wrapper.
    pub(super) fn entry_witness(args: EntryWitnessArgs) -> Result<()> {
        if args.action.is_some() && args.lock.is_some() {
            return Err(crate::error::CompileError::without_span("entry-witness accepts either --action or --lock, not both"));
        }

        let input_path = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        if args.artifact.is_some() {
            return policy_entry_witness(&args, input);
        }
        if args.script_hash.is_some() {
            return Err(CompileError::without_span("entry-witness --script-hash requires --artifact"));
        }
        let result = compile_path(
            input,
            CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;

        let selected = select_entry_witness_metadata(&result.metadata, args.action.as_deref(), args.lock.as_deref())?;
        if selected.params.is_empty() {
            return Err(crate::error::CompileError::without_span(format!(
                "{} '{}' has no parameters; `_cellscript_entry` witness ABI is only emitted for parameterized entries",
                selected.kind, selected.name
            )));
        }

        let payload_params = selected
            .params
            .iter()
            .filter(|param| {
                !param.lock_args_data_source
                    && !param.cell_bound_abi
                    && !param.ty.starts_with('&')
                    && !selected.runtime_bound_param_names.contains(&param.name)
            })
            .collect::<Vec<_>>();
        if args.args.len() != payload_params.len() {
            return Err(crate::error::CompileError::without_span(format!(
                "{} '{}' expects {} witness payload arg(s), got {}",
                selected.kind,
                selected.name,
                payload_params.len(),
                args.args.len()
            )));
        }

        let witness_args = payload_params
            .iter()
            .zip(args.args.iter())
            .map(|(param, value)| parse_entry_witness_arg(param, value))
            .collect::<Result<Vec<_>>>()?;
        let witness = crate::encode_entry_witness_args_for_params_with_runtime_bound(
            selected.params,
            &witness_args,
            &selected.runtime_bound_param_names,
        )
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to encode entry witness: {}", error)))?;
        let witness_hex = crate::hex_encode(&witness);

        if let Some(output_path) = &args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, &witness)?;
        }

        let payload_param_names = payload_params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>();
        let human_lines = if let Some(output_path) = &args.output {
            vec![
                "Entry witness encoded".green().to_string(),
                format!("  ABI: {}", ENTRY_WITNESS_ABI),
                format!("  Entry: {} {}", selected.kind, selected.name),
                format!("  Output: {}", output_path.display()),
                format!("  Hex: {}", witness_hex),
            ]
        } else {
            vec![witness_hex.clone()]
        };
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "abi": ENTRY_WITNESS_ABI,
                "placement_abi": ENTRY_WITNESS_PLACEMENT_ABI,
                "witness_args_field": ENTRY_WITNESS_PLACEMENT_FIELD,
                "witness_source": ENTRY_WITNESS_PLACEMENT_SOURCE,
                "raw_v1_compatible": false,
                "entry_kind": selected.kind,
                "entry": selected.name,
                "witness_hex": witness_hex,
                "witness_size_bytes": witness.len(),
                "payload_args": witness_args.len(),
                "payload_params": payload_param_names,
                "output": args.output.as_ref().map(|path| path.display().to_string()),
            }),
            human_lines,
        }
        .emit(args.json)
    }
}

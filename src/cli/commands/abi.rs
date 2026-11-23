use super::entry_witness_values::select_entry_witness_metadata;
use super::{AbiArgs, CommandExecutor, ConstraintsArgs, SchedulerPlanArgs};
use crate::error::Result;
use crate::{
    compile_path, compile_path_with_entry_action, compile_path_with_entry_lock, CompileOptions, ENTRY_WITNESS_ABI,
    ENTRY_WITNESS_PLACEMENT_ABI, ENTRY_WITNESS_PLACEMENT_FIELD, ENTRY_WITNESS_PLACEMENT_SOURCE,
};
use camino::Utf8Path;
use colored::Colorize;
use std::path::PathBuf;

impl CommandExecutor {
    pub(super) fn constraints(args: ConstraintsArgs) -> Result<()> {
        if args.entry_action.is_some() && args.entry_lock.is_some() {
            return Err(crate::error::CompileError::without_span(
                "constraints accepts either --entry-action or --entry-lock, not both",
            ));
        }
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let options = CompileOptions {
            source_contracts: false,
            edition: crate::CURRENT_EDITION,
            diagnostics: crate::CompilerDiagnostics::default(),
            output: None,
            debug: false,
            target: args.target,
            target_profile: args.target_profile,
            primitive_compat: None,
        };
        let result = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
            (Some(action), None) => compile_path_with_entry_action(input, options, action),
            (None, Some(lock)) => compile_path_with_entry_lock(input, options, lock),
            (None, None) => compile_path(input, options),
            (Some(_), Some(_)) => unreachable!("validated above"),
        }?;
        let json = serde_json::to_string_pretty(&result.metadata.constraints)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize constraints: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Constraints generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    pub(super) fn abi(args: AbiArgs) -> Result<()> {
        if args.action.is_some() && args.lock.is_some() {
            return Err(crate::error::CompileError::without_span("abi accepts either --action or --lock, not both"));
        }

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
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let selected = select_entry_witness_metadata(&result.metadata, args.action.as_deref(), args.lock.as_deref())?;
        let entry_constraints = result
            .metadata
            .constraints
            .entry_abi
            .iter()
            .find(|entry| entry.entry_kind == selected.kind && entry.entry_name == selected.name)
            .ok_or_else(|| {
                crate::error::CompileError::without_span(format!(
                    "entry ABI constraints for {} '{}' were not found in metadata",
                    selected.kind, selected.name
                ))
            })?;

        let params = selected
            .params
            .iter()
            .map(|param| {
                let runtime_bound = selected.runtime_bound_param_names.contains(&param.name) || param.lock_args_data_source;
                let payload_bound =
                    !param.lock_args_data_source && !param.cell_bound_abi && !param.ty.starts_with('&') && !runtime_bound;
                let layout = entry_constraints.params.iter().find(|candidate| candidate.name == param.name);
                serde_json::json!({
                    "name": param.name,
                    "type": param.ty,
                    "payload_bound": payload_bound,
                    "runtime_bound": runtime_bound,
                    "cell_bound": param.cell_bound_abi,
                    "schema_pointer_abi": param.schema_pointer_abi,
                    "fixed_byte_len": param.fixed_byte_len,
                    "abi_kind": layout.map(|layout| layout.abi_kind.as_str()),
                    "abi_slots": layout.map(|layout| layout.abi_slots),
                    "slot_start": layout.map(|layout| layout.slot_start),
                    "slot_end": layout.map(|layout| layout.slot_end),
                    "witness_bytes": layout.map(|layout| layout.witness_bytes),
                    "stack_spill_bytes": layout.map(|layout| layout.stack_spill_bytes),
                    "supported": layout.map(|layout| layout.supported).unwrap_or(false),
                    "unsupported_reason": layout.and_then(|layout| layout.unsupported_reason.as_deref()),
                })
            })
            .collect::<Vec<_>>();
        let payload_params = selected
            .params
            .iter()
            .filter(|param| {
                !param.lock_args_data_source
                    && !param.cell_bound_abi
                    && !param.ty.starts_with('&')
                    && !selected.runtime_bound_param_names.contains(&param.name)
            })
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        let runtime_bound_params = selected
            .runtime_bound_param_names
            .iter()
            .map(|name| name.as_str())
            .chain(selected.params.iter().filter(|param| param.lock_args_data_source).map(|param| param.name.as_str()))
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "status": if entry_constraints.unsupported { "fail" } else { "ok" },
            "abi": ENTRY_WITNESS_ABI,
            "placement_abi": ENTRY_WITNESS_PLACEMENT_ABI,
            "witness_args_field": ENTRY_WITNESS_PLACEMENT_FIELD,
            "witness_source": ENTRY_WITNESS_PLACEMENT_SOURCE,
            "raw_v1_compatible": false,
            "target_profile": result.metadata.target_profile.name,
            "entry_kind": selected.kind,
            "entry": selected.name,
            "payload_params": payload_params,
            "runtime_bound_params": runtime_bound_params,
            "layout": entry_constraints,
            "params": params,
        });
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize ABI report: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "ABI report generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    pub(super) fn scheduler_plan(args: SchedulerPlanArgs) -> Result<()> {
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
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;

        let actions = result
            .metadata
            .actions
            .iter()
            .map(|action| {
                let mut reasons = Vec::new();
                if !action.parallelizable {
                    reasons.push("parallelizable=false".to_string());
                }
                if !action.touches_shared.is_empty() {
                    reasons.push("touches-shared-state".to_string());
                }
                serde_json::json!({
                    "action": action.name,
                    "effects": action.effects,
                    "parallelizable": action.parallelizable,
                    "touches_shared": action.touches_shared,
                    "estimated_cycles": action.estimated_cycles,
                    "scheduler_witness_abi": action.scheduler_witness_abi,
                    "admission": if action.parallelizable && action.touches_shared.is_empty() {
                        "parallel-candidate"
                    } else {
                        "serial-required"
                    },
                    "reasons": reasons,
                })
            })
            .collect::<Vec<_>>();

        let mut conflicts = Vec::new();
        for (left_index, left) in result.metadata.actions.iter().enumerate() {
            for right in result.metadata.actions.iter().skip(left_index + 1) {
                let shared =
                    left.touches_shared.iter().filter(|touch| right.touches_shared.contains(*touch)).cloned().collect::<Vec<_>>();
                if !shared.is_empty() {
                    conflicts.push(serde_json::json!({
                        "left": left.name,
                        "right": right.name,
                        "shared_touches": shared,
                        "policy": "must-not-run-in-parallel",
                    }));
                }
            }
        }

        let total_estimated_cycles = result.metadata.actions.iter().map(|action| action.estimated_cycles).sum::<u64>();
        let max_estimated_cycles = result.metadata.actions.iter().map(|action| action.estimated_cycles).max().unwrap_or_default();
        let serial_required_actions = result
            .metadata
            .actions
            .iter()
            .filter(|action| !action.parallelizable || !action.touches_shared.is_empty())
            .map(|action| action.name.as_str())
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "status": "ok",
            "target_profile": result.metadata.target_profile.name,
            "policy": "cellscript-scheduler-hints-v1",
            "action_count": result.metadata.actions.len(),
            "serial_required_actions": serial_required_actions,
            "conflict_count": conflicts.len(),
            "conflicts": conflicts,
            "estimated_cycles": {
                "total": total_estimated_cycles,
                "max_action": max_estimated_cycles,
            },
            "actions": actions,
        });
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize scheduler plan: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Scheduler plan generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }
}

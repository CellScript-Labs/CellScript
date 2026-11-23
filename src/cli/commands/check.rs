use super::super::output::CommandOutcome;
use super::check_policy::{
    checked_pool_invariant_family_count, executable_surface_policy, fail_closed_obligation_count,
    pool_invariant_family_blocker_class_count, pool_invariant_family_blocker_class_summaries, pool_runtime_input_requirement_count,
    pool_runtime_input_requirement_summaries, runtime_required_obligation_count, runtime_required_pool_invariant_family_count,
    runtime_required_transaction_invariant_checked_subcondition_count, runtime_required_transaction_invariant_count,
    target_profile_policy_violations, transaction_invariant_checked_subcondition_summaries,
    transaction_runtime_input_blocker_class_count_by_status, transaction_runtime_input_blocker_class_summaries_by_status,
    transaction_runtime_input_blocker_count_by_status, transaction_runtime_input_blocker_summaries_by_status,
    transaction_runtime_input_requirement_count, transaction_runtime_input_requirement_count_by_status,
    transaction_runtime_input_requirement_summaries, transaction_runtime_input_requirement_summaries_by_status, validate_check_policy,
};
use super::{
    check_resolution_options, compile_failure_diagnostics, compile_target_profile_for_check, diagnostics_to_error,
    effective_check_args, effective_check_target_profile, metadata_schema_versions_json, CheckArgs, CommandExecutor,
};
use crate::error::{CompileError, Result};
use crate::{compile_path_with_executable_surface_policy, CompileOptions};
use camino::Utf8Path;
use colored::Colorize;
use std::collections::BTreeSet;

impl CommandExecutor {
    pub(super) fn check(args: CheckArgs) -> Result<()> {
        // Workspace mode: check all members or a specific member.
        if args.workspace || args.package.is_some() {
            return Self::check_workspace(args);
        }
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?;
        if let Some(ws_root) = ws_root {
            let members = crate::resolve_workspace_members(&ws_root)?;
            if !members.is_empty() {
                let mut ws_args = args;
                ws_args.workspace = true;
                return Self::check_workspace(ws_args);
            }
        }

        let args = effective_check_args(args)?;
        let requested_profile = effective_check_target_profile(&args)?;
        let compile_target_profile = compile_target_profile_for_check(requested_profile);
        let mut checked_targets = Vec::new();
        let mut checked_target_json = Vec::new();
        let targets: Vec<Option<&'static str>> =
            if args.all_targets { vec![Some("riscv64-asm"), Some("riscv64-elf")] } else { vec![None] };

        for target in targets {
            let compile_options = CompileOptions {
                source_contracts: false,
                edition: crate::CURRENT_EDITION,
                diagnostics: crate::CompilerDiagnostics::default(),
                output: None,
                debug: false,
                target: target.map(str::to_string),
                target_profile: compile_target_profile.clone(),
                primitive_compat: args.primitive_compat.clone(),
            };
            let resolution_options = check_resolution_options(&args, crate::package::DependencyScope::Runtime);
            let result = match crate::package::with_resolution_options(resolution_options, || {
                if let Some(name) = args.artifact.as_deref() {
                    crate::compile_path_with_artifact_name(".", compile_options.clone(), name, executable_surface_policy(&args))
                } else {
                    compile_path_with_executable_surface_policy(".", compile_options.clone(), None, executable_surface_policy(&args))
                }
            }) {
                Ok(result) => result,
                Err(error) if args.artifact.is_some() => return Err(error),
                Err(error) => {
                    let diagnostics = compile_failure_diagnostics(Utf8Path::new("."), compile_options, error);
                    return Err(diagnostics_to_error(&diagnostics));
                }
            };
            validate_check_policy(&result.metadata, &args)?;
            let target_profile_policy_violations =
                target_profile_policy_violations(&result.metadata, result.artifact_format, requested_profile);
            if !target_profile_policy_violations.is_empty() {
                return Err(crate::error::CompileError::without_span(format!(
                    "target profile policy failed for '{}':\n  - {}",
                    requested_profile.name(),
                    target_profile_policy_violations.join("\n  - ")
                )));
            }
            let target_label = match target {
                Some(target) => format!("{} ({})", target, result.artifact_format.display_name()),
                None => format!("package default ({})", result.artifact_format.display_name()),
            };
            let requested_profile_name = requested_profile.name();
            checked_target_json.push(serde_json::json!({
                "requested_target": target.unwrap_or("package-default"),
                "artifact_format": result.artifact_format.display_name(),
                "target_profile": requested_profile_name,
                "compiled_target_profile": result.metadata.target_profile.name.as_str(),
                "target_profile_policy_violations": target_profile_policy_violations,
                "metadata_schema_version": result.metadata.metadata_schema_version,
                "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
                "compiler_version": result.metadata.compiler_version,
                "standalone_runner_compatible": result.metadata.runtime.standalone_runner_compatible,
                "ckb_runtime_required": result.metadata.runtime.ckb_runtime_required,
                "fail_closed_runtime_features": result.metadata.runtime.fail_closed_runtime_features,
                "verifier_obligations": result.metadata.runtime.verifier_obligations.len(),
                "runtime_required_verifier_obligations": runtime_required_obligation_count(&result.metadata),
                "fail_closed_verifier_obligations": fail_closed_obligation_count(&result.metadata),
                "runtime_required_transaction_invariants": runtime_required_transaction_invariant_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subconditions": runtime_required_transaction_invariant_checked_subcondition_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subcondition_summaries": transaction_invariant_checked_subcondition_summaries(&result.metadata),
                "transaction_runtime_input_requirements": transaction_runtime_input_requirement_count(&result.metadata),
                "transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries(&result.metadata),
                "checked_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "checked-runtime"),
                "checked_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "checked-runtime"),
                "runtime_required_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blockers": transaction_runtime_input_blocker_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_summaries": transaction_runtime_input_blocker_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_classes": transaction_runtime_input_blocker_class_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_class_summaries": transaction_runtime_input_blocker_class_summaries_by_status(&result.metadata, "runtime-required"),
                "checked_pool_invariant_families": checked_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_families": runtime_required_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_blocker_classes": pool_invariant_family_blocker_class_count(&result.metadata, "runtime-required"),
                "runtime_required_pool_invariant_blocker_class_summaries": pool_invariant_family_blocker_class_summaries(&result.metadata, "runtime-required"),
                "pool_runtime_input_requirements": pool_runtime_input_requirement_count(&result.metadata),
                "pool_runtime_input_requirement_summaries": pool_runtime_input_requirement_summaries(&result.metadata),
                "constraints": &result.metadata.constraints,
            }));
            checked_targets.push(target_label);
        }

        let policy_verified = args.production || args.deny_fail_closed || args.deny_ckb_runtime;
        let policy_verified = policy_verified || args.deny_runtime_obligations;
        let summary = serde_json::json!({
            "status": "ok",
            "checked_targets": checked_target_json,
            "policy_artifact": args.artifact,
            "all_targets": args.all_targets,
            "policy_verified": policy_verified,
            "policy": {
                "production": args.production,
                "deny_fail_closed": args.deny_fail_closed,
                "deny_ckb_runtime": args.deny_ckb_runtime,
                "deny_runtime_obligations": args.deny_runtime_obligations,
            },
        });
        let mut human_lines = vec!["Check succeeded".green().to_string(), format!("  Target profile: {}", requested_profile.name())];
        human_lines.extend(checked_targets.into_iter().map(|target| format!("  Checked: {}", target)));
        CommandOutcome { machine: summary, human_lines }.emit(args.json)
    }

    fn check_workspace(args: CheckArgs) -> Result<()> {
        let args = effective_check_args(args)?;
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?.ok_or_else(|| {
            CompileError::without_span("no workspace root found; run from a directory containing a [workspace] Cell.toml")
        })?;
        let resolution_options = check_resolution_options(&args, crate::package::DependencyScope::Runtime);
        let workspace_graph = crate::package::workspace::resolve_workspace_graph(ws_root.as_std_path(), &resolution_options)?;
        let selected_order = workspace_graph.selected_build_order(args.package.as_deref())?;
        let members = selected_order
            .iter()
            .map(|name| {
                let member = &workspace_graph.members[name];
                (name.clone(), ws_root.join(&member.path))
            })
            .collect::<Vec<_>>();

        let mut member_results = Vec::new();
        let mut human_lines = Vec::new();
        let mut failure_diagnostics = Vec::new();
        let mut failed = 0usize;
        let mut failed_members = BTreeSet::new();
        let requested_profile = effective_check_target_profile(&args)?;
        let compile_target_profile = compile_target_profile_for_check(requested_profile);
        let targets: Vec<Option<&'static str>> =
            if args.all_targets { vec![Some("riscv64-asm"), Some("riscv64-elf")] } else { vec![None] };

        for (order_index, (member_name, member_dir)) in members.iter().enumerate() {
            let blocked_by = workspace_graph
                .member_edges
                .iter()
                .filter(|edge| edge.from_member == *member_name && failed_members.contains(&edge.to_member))
                .map(|edge| edge.to_member.clone())
                .collect::<Vec<_>>();
            if !blocked_by.is_empty() {
                let error = CompileError::without_span(format!(
                    "workspace member '{}' was not checked because dependencies failed: {}",
                    member_name,
                    blocked_by.join(", ")
                ))
                .with_code(crate::error::DiagnosticCode::E2700)
                .with_file(member_dir.clone());
                failure_diagnostics.push(error.clone());
                member_results.push(serde_json::json!({
                    "member": member_dir.as_str(),
                    "member_name": member_name,
                    "status": "blocked",
                    "blocked_by": blocked_by,
                    "error": error.message,
                }));
                failed_members.insert(member_name.clone());
                failed += 1;
                continue;
            }

            let mut unit_results = Vec::new();
            let mut member_error = None;
            for target in targets.iter().copied() {
                let compile_options = CompileOptions {
                    source_contracts: false,
                    edition: crate::CURRENT_EDITION,
                    diagnostics: crate::CompilerDiagnostics::default(),
                    output: None,
                    debug: false,
                    target: target.map(str::to_string),
                    target_profile: compile_target_profile.clone(),
                    primitive_compat: args.primitive_compat.clone(),
                };
                let compile_result = crate::package::with_resolution_options(resolution_options.clone(), || {
                    if let Some(name) = args.artifact.as_deref() {
                        crate::compile_path_with_artifact_name(member_dir, compile_options, name, executable_surface_policy(&args))
                    } else {
                        compile_path_with_executable_surface_policy(
                            member_dir,
                            compile_options,
                            None,
                            executable_surface_policy(&args),
                        )
                    }
                });
                let result = match compile_result {
                    Ok(result) => result,
                    Err(error) => {
                        member_error = Some(error);
                        break;
                    }
                };
                if let Err(error) = validate_check_policy(&result.metadata, &args) {
                    member_error = Some(error);
                    break;
                }
                let profile_violations = target_profile_policy_violations(&result.metadata, result.artifact_format, requested_profile);
                if !profile_violations.is_empty() {
                    member_error = Some(CompileError::without_span(format!(
                        "target profile policy failed for '{}':\n  - {}",
                        requested_profile.name(),
                        profile_violations.join("\n  - ")
                    )));
                    break;
                }
                unit_results.push(serde_json::json!({
                    "requested_target": target.unwrap_or("package-default"),
                    "artifact_format": result.artifact_format.display_name(),
                    "requested_target_profile": requested_profile.name(),
                    "compiled_target_profile": result.metadata.target_profile.name,
                    "cache_hit": result.cache_hit,
                }));
            }

            if let Some(error) = member_error {
                failure_diagnostics.push(error.clone().with_file(member_dir.clone()));
                member_results.push(serde_json::json!({
                    "member": member_dir.as_str(),
                    "member_name": member_name,
                    "status": "failed",
                    "checked_units": unit_results,
                    "error": error.message,
                }));
                failed_members.insert(member_name.clone());
                failed += 1;
            } else {
                let cache_hit = unit_results.iter().all(|unit| unit["cache_hit"].as_bool() == Some(true));
                member_results.push(serde_json::json!({
                    "member": member_dir.as_str(),
                    "member_name": member_name,
                    "member_id": workspace_graph.members[member_name].id,
                    "order": order_index,
                    "dependencies": workspace_graph.member_edges.iter()
                        .filter(|edge| edge.from_member == *member_name)
                        .map(|edge| edge.to_member.as_str())
                        .collect::<Vec<_>>(),
                    "status": "ok",
                    "checked_units": unit_results,
                    "cache_hit": cache_hit,
                }));
                human_lines.push(format!("{} {}", "Checked".green(), member_name));
            }
        }

        if failed > 0 {
            return Err(diagnostics_to_error(&failure_diagnostics).with_details(serde_json::json!({
                "mode": "workspace-check",
                "graph_schema": workspace_graph.schema,
                "dependency_selection": workspace_graph.selection,
                "check_selection": {
                    "target_profile": args.target_profile,
                    "artifact": args.artifact,
                    "all_targets": args.all_targets,
                    "primitive_compat": args.primitive_compat,
                    "production": args.production,
                },
                "selected_members": selected_order,
                "member_edges": workspace_graph.member_edges,
                "members": members.len(),
                "succeeded": members.len().saturating_sub(failed),
                "failed": failed,
                "results": member_results,
            })));
        }
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "mode": "workspace-check",
                "graph_schema": workspace_graph.schema,
                "dependency_selection": workspace_graph.selection,
                "check_selection": {
                    "target_profile": args.target_profile,
                    "artifact": args.artifact,
                    "all_targets": args.all_targets,
                    "primitive_compat": args.primitive_compat,
                    "production": args.production,
                },
                "selected_members": selected_order,
                "member_edges": workspace_graph.member_edges,
                "members": members.len(),
                "succeeded": members.len(),
                "failed": 0,
                "results": member_results,
            }),
            human_lines: {
                human_lines.push(format!("Workspace check complete: {} members", members.len()).green().to_string());
                human_lines
            },
        }
        .emit(args.json)
    }
}

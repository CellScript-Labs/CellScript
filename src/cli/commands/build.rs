use super::super::output::CommandOutcome;
use super::check_policy::{
    checked_pool_invariant_family_count, effective_build_check_args, executable_surface_policy, fail_closed_obligation_count,
    pool_invariant_family_blocker_class_count, pool_invariant_family_blocker_class_summaries, pool_runtime_input_requirement_count,
    pool_runtime_input_requirement_summaries, runtime_required_obligation_count, runtime_required_pool_invariant_family_count,
    runtime_required_transaction_invariant_checked_subcondition_count, runtime_required_transaction_invariant_count,
    transaction_invariant_checked_subcondition_summaries, transaction_runtime_input_blocker_class_count_by_status,
    transaction_runtime_input_blocker_class_summaries_by_status, transaction_runtime_input_blocker_count_by_status,
    transaction_runtime_input_blocker_summaries_by_status, transaction_runtime_input_requirement_count,
    transaction_runtime_input_requirement_count_by_status, transaction_runtime_input_requirement_summaries,
    transaction_runtime_input_requirement_summaries_by_status, validate_build_entry_selection, validate_check_policy,
};
use super::{
    build_resolution_options, diagnostics_to_error, inspect_build_plan_for_build, metadata_schema_versions_json,
    refresh_lockfile_from_build, validate_build_unit_result, BuildArgs, CommandExecutor,
};
use crate::error::{CompileError, Result};
use crate::{
    compile_path_with_executable_surface_policy, default_metadata_path_for_artifact, default_output_path_for_input,
    resolve_input_path, CompileEntryScope, CompileOptions,
};
use camino::Utf8Path;
use colored::Colorize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

impl CommandExecutor {
    pub(super) fn build(args: BuildArgs) -> Result<()> {
        validate_build_entry_selection(&args)?;
        // Workspace mode: build all members or a specific member.
        if args.workspace || args.package.is_some() {
            return Self::build_workspace(args);
        }
        // Also check if the current directory is a workspace root without explicit flags.
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?;
        if let Some(ws_root) = ws_root {
            let members = crate::resolve_workspace_members(&ws_root)?;
            if !members.is_empty() {
                // Current dir is a workspace root; build all members.
                let mut ws_args = args;
                ws_args.workspace = true;
                return Self::build_workspace(ws_args);
            }
        }

        let diagnostics = crate::CompilerDiagnostics::default();
        let input = Utf8Path::new(".");
        let options = CompileOptions {
            source_contracts: args.source_contracts,
            edition: crate::CURRENT_EDITION,
            diagnostics,
            output: None,
            debug: false,
            target: args.target.clone(),
            target_profile: args.target_profile.clone(),
            primitive_compat: args.primitive_compat.clone(),
        };
        if args.entry_action.is_some() && args.entry_lock.is_some() {
            return Err(crate::error::CompileError::without_span("--entry-action and --entry-lock are mutually exclusive"));
        }
        let policy_args = effective_build_check_args(&args)?;
        let executable_surface_policy = executable_surface_policy(&policy_args);
        let entry_scope = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
            (Some(action), None) => Some(CompileEntryScope::Action(action.to_string())),
            (None, Some(lock)) => Some(CompileEntryScope::Lock(lock.to_string())),
            (None, None) => None,
            (Some(_), Some(_)) => unreachable!("validated above"),
        };
        let cache_options = options.clone();
        let resolution_options = build_resolution_options(&args, crate::package::DependencyScope::Runtime);
        let result = crate::package::with_resolution_options(resolution_options, || {
            if let Some(name) = args.artifact.as_deref() {
                crate::compile_path_with_artifact_name(input, options, name, executable_surface_policy)
            } else {
                compile_path_with_executable_surface_policy(input, options, entry_scope, executable_surface_policy)
            }
        })?;
        validate_check_policy(&result.metadata, &policy_args)?;
        let resolved = resolve_input_path(input)?;
        let output_path = default_output_path_for_input(input, &resolved, result.artifact_format)?;
        let inspected_plan = inspect_build_plan_for_build(Path::new("."), &args)?;
        let planned_unit =
            inspected_plan.units.first().ok_or_else(|| CompileError::without_span("build plan did not select the current package"))?;
        validate_build_unit_result(planned_unit, &result, &output_path)?;
        result.write_to_path(&output_path)?;
        let metadata_path = default_metadata_path_for_artifact(&output_path);
        result.write_metadata_to_path(&metadata_path)?;
        let verified_sidecars = result.write_verified_artifact_sidecars(&output_path)?;

        if !args.frozen {
            refresh_lockfile_from_build(std::path::Path::new("."), &result.metadata)?;
        }
        if args.entry_action.is_none() && args.entry_lock.is_none() && args.artifact.is_none() {
            crate::refresh_incremental_cache_for_input(input, &cache_options, &result)?;
        }

        let policy_verified = policy_args.production
            || policy_args.deny_fail_closed
            || policy_args.deny_ckb_runtime
            || policy_args.deny_runtime_obligations;
        let mut summary = serde_json::json!({
            "status": "ok",
            "artifact": output_path.to_string(),
            "policy_artifact": args.artifact,
            "metadata": metadata_path.to_string(),
            "artifact_format": result.artifact_format.display_name(),
            "compilation_policy": crate::COMPILATION_POLICY,
            "target_profile": result.metadata.target_profile.name.as_str(),
            "artifact_hash": result.metadata.artifact_hash,
            "artifact_size_bytes": result.artifact_bytes.len(),
            "source_hash": result.metadata.source_hash,
            "source_content_hash": result.metadata.source_content_hash,
            "metadata_schema_version": result.metadata.metadata_schema_version,
            "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
            "compiler_version": result.metadata.compiler_version,
            "standalone_runner_compatible": result.metadata.runtime.standalone_runner_compatible,
            "ckb_runtime_required": result.metadata.runtime.ckb_runtime_required,
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
            "policy_verified": policy_verified,
            "cache_hit": result.cache_hit,
            "constraints": &result.metadata.constraints,
        });
        if let Some(object) = summary.as_object_mut() {
            object.insert("build_plan_schema".to_string(), serde_json::json!(&inspected_plan.schema));
            object.insert("build_plan_digest".to_string(), serde_json::json!(&inspected_plan.plan_digest));
            object.insert("resolve_graph_digest".to_string(), serde_json::json!(&inspected_plan.resolve_graph_digest));
            object.insert("resolve_resolution_digest".to_string(), serde_json::json!(&inspected_plan.resolve_resolution_digest));
            object.insert("build_unit_id".to_string(), serde_json::json!(&planned_unit.id));
            object.insert(
                "dependency_lock_mode".to_string(),
                serde_json::json!(if args.frozen {
                    "frozen"
                } else if args.locked {
                    "locked"
                } else {
                    "authoritative"
                }),
            );
            object.insert("dependency_environment".to_string(), serde_json::json!(args.environment.as_deref()));
            object.insert("dependency_offline".to_string(), serde_json::json!(args.offline || args.frozen));
            object.insert("dependency_features".to_string(), serde_json::json!(&args.features));
            object.insert("dependency_all_features".to_string(), serde_json::json!(args.all_features));
            object.insert("dependency_default_features".to_string(), serde_json::json!(!args.no_default_features));
            object
                .insert("lowering_record".to_string(), serde_json::json!(verified_sidecars.as_ref().map(|paths| paths.0.to_string())));
            object.insert("source_map".to_string(), serde_json::json!(verified_sidecars.as_ref().map(|paths| paths.1.to_string())));
        }
        let mut human_lines = vec![
            "Build complete".green().to_string(),
            format!("  Artifact format: {}", result.artifact_format.display_name()),
            format!("  Target profile: {}", result.metadata.target_profile.name),
            format!("  Output: {}", output_path),
            format!("  Metadata: {}", metadata_path),
        ];
        if result.cache_hit {
            human_lines.push(format!("  {}", "(incremental cache hit)".yellow()));
        }
        CommandOutcome { machine: summary, human_lines }.emit(args.json)
    }

    fn build_workspace(args: BuildArgs) -> Result<()> {
        validate_build_entry_selection(&args)?;
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?.ok_or_else(|| {
            CompileError::without_span("no workspace root found; run from a directory containing a [workspace] Cell.toml")
        })?;
        let resolution_options = build_resolution_options(&args, crate::package::DependencyScope::Runtime);
        let workspace_graph = crate::package::workspace::resolve_workspace_graph(ws_root.as_std_path(), &resolution_options)?;
        let selected_order = workspace_graph.selected_build_order(args.package.as_deref())?;
        let members = selected_order
            .iter()
            .map(|name| {
                let member = &workspace_graph.members[name];
                (name.clone(), ws_root.join(&member.path))
            })
            .collect::<Vec<_>>();
        let inspected_plan = inspect_build_plan_for_build(ws_root.as_std_path(), &args)?;
        let planned_units =
            inspected_plan.units.iter().map(|unit| (unit.package_name.clone(), unit.clone())).collect::<BTreeMap<_, _>>();

        let diagnostics = crate::CompilerDiagnostics::default();
        let mut member_results = Vec::new();
        let mut human_lines = Vec::new();
        let mut failure_diagnostics = Vec::new();
        let mut failed = 0usize;
        let mut failed_members = BTreeSet::new();
        let mut successful_builds = Vec::new();
        let policy_args = effective_build_check_args(&args)?;
        let executable_surface_policy = executable_surface_policy(&policy_args);

        for (order_index, (member_name, member_dir)) in members.iter().enumerate() {
            let blocked_by = workspace_graph
                .member_edges
                .iter()
                .filter(|edge| edge.from_member == *member_name && failed_members.contains(&edge.to_member))
                .map(|edge| edge.to_member.clone())
                .collect::<Vec<_>>();
            if !blocked_by.is_empty() {
                let error = CompileError::without_span(format!(
                    "workspace member '{}' was not built because dependencies failed: {}",
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

            let options = CompileOptions {
                source_contracts: args.source_contracts,
                edition: crate::CURRENT_EDITION,
                diagnostics,
                output: None,
                debug: false,
                target: args.target.clone(),
                target_profile: args.target_profile.clone(),
                primitive_compat: args.primitive_compat.clone(),
            };
            let cache_options = options.clone();
            let entry_scope = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
                (Some(action), None) => Some(CompileEntryScope::Action(action.to_string())),
                (None, Some(lock)) => Some(CompileEntryScope::Lock(lock.to_string())),
                (None, None) => None,
                (Some(_), Some(_)) => {
                    return Err(CompileError::without_span("--entry-action and --entry-lock are mutually exclusive"))
                }
            };
            let compile_result = crate::package::with_resolution_options(resolution_options.clone(), || {
                if let Some(name) = args.artifact.as_deref() {
                    crate::compile_path_with_artifact_name(member_dir, options, name, executable_surface_policy)
                } else {
                    compile_path_with_executable_surface_policy(member_dir, options, entry_scope, executable_surface_policy)
                }
            });

            match compile_result {
                Ok(result) => {
                    if let Err(error) = validate_check_policy(&result.metadata, &policy_args) {
                        failure_diagnostics.push(error.clone().with_file(member_dir.clone()));
                        member_results.push(serde_json::json!({
                            "member": member_dir.as_str(),
                            "member_name": member_name,
                            "status": "failed",
                            "error": error.message,
                        }));
                        failed_members.insert(member_name.clone());
                        failed += 1;
                        continue;
                    }

                    let resolved = resolve_input_path(member_dir)?;
                    let output_path = default_output_path_for_input(member_dir, &resolved, result.artifact_format)?;
                    let planned_unit = planned_units.get(member_name).ok_or_else(|| {
                        CompileError::without_span(format!("build plan has no unit for workspace member '{member_name}'"))
                    })?;
                    if let Err(error) = validate_build_unit_result(planned_unit, &result, &output_path) {
                        failure_diagnostics.push(error.clone().with_file(member_dir.clone()));
                        member_results.push(serde_json::json!({
                            "member": member_dir.as_str(),
                            "member_name": member_name,
                            "status": "failed",
                            "error": error.message,
                        }));
                        failed_members.insert(member_name.clone());
                        failed += 1;
                        continue;
                    }
                    result.write_to_path(&output_path)?;
                    let metadata_path = default_metadata_path_for_artifact(&output_path);
                    result.write_metadata_to_path(&metadata_path)?;
                    let verified_sidecars = result.write_verified_artifact_sidecars(&output_path)?;
                    if args.entry_action.is_none() && args.entry_lock.is_none() && args.artifact.is_none() {
                        crate::refresh_incremental_cache_for_input(member_dir, &cache_options, &result)?;
                    }
                    successful_builds.push((member_dir.clone(), result.metadata.clone()));
                    member_results.push(serde_json::json!({
                        "member": member_dir.as_str(),
                        "member_name": member_name,
                        "member_id": workspace_graph.members[member_name].id,
                        "build_unit_id": planned_unit.id,
                        "order": order_index,
                        "dependencies": workspace_graph.member_edges.iter()
                            .filter(|edge| edge.from_member == *member_name)
                            .map(|edge| edge.to_member.as_str())
                            .collect::<Vec<_>>(),
                        "status": "ok",
                        "artifact": output_path.to_string(),
                        "metadata": metadata_path.to_string(),
                        "lowering_record": verified_sidecars.as_ref().map(|paths| paths.0.to_string()),
                        "source_map": verified_sidecars.as_ref().map(|paths| paths.1.to_string()),
                        "artifact_format": result.artifact_format.display_name(),
                        "target_profile": result.metadata.target_profile.name,
                        "artifact_hash": result.metadata.artifact_hash,
                        "artifact_size_bytes": result.artifact_bytes.len(),
                        "cache_hit": result.cache_hit,
                    }));
                    human_lines.push(format!("{} {}", "Built".green(), member_name));
                }
                Err(error) => {
                    failure_diagnostics.push(error.clone().with_file(member_dir.clone()));
                    member_results.push(serde_json::json!({
                        "member": member_dir.as_str(),
                        "member_name": member_name,
                        "status": "failed",
                        "error": error.message,
                    }));
                    failed_members.insert(member_name.clone());
                    failed += 1;
                }
            }
        }

        if failed > 0 {
            return Err(diagnostics_to_error(&failure_diagnostics).with_details(serde_json::json!({
                "mode": "workspace-build",
                "build_plan_schema": inspected_plan.schema,
                "build_plan_digest": inspected_plan.plan_digest,
                "resolve_graph_digest": inspected_plan.resolve_graph_digest,
                "resolve_resolution_digest": inspected_plan.resolve_resolution_digest,
                "graph_schema": workspace_graph.schema,
                "dependency_selection": workspace_graph.selection,
                "build_selection": {
                    "target": args.target,
                    "target_profile": args.target_profile,
                    "release": args.release,
                    "entry_action": args.entry_action,
                    "entry_lock": args.entry_lock,
                    "artifact": args.artifact,
                },
                "selected_members": selected_order,
                "member_edges": workspace_graph.member_edges,
                "members": members.len(),
                "succeeded": members.len().saturating_sub(failed),
                "failed": failed,
                "results": member_results,
            })));
        }

        if !args.frozen {
            for (member_dir, metadata) in &successful_builds {
                refresh_lockfile_from_build(member_dir.as_std_path(), metadata)?;
            }
        }

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "mode": "workspace-build",
                "build_plan_schema": inspected_plan.schema,
                "build_plan_digest": inspected_plan.plan_digest,
                "resolve_graph_digest": inspected_plan.resolve_graph_digest,
                "resolve_resolution_digest": inspected_plan.resolve_resolution_digest,
                "graph_schema": workspace_graph.schema,
                "dependency_selection": workspace_graph.selection,
                "build_selection": {
                    "target": args.target,
                    "target_profile": args.target_profile,
                    "release": args.release,
                    "entry_action": args.entry_action,
                    "entry_lock": args.entry_lock,
                    "artifact": args.artifact,
                },
                "selected_members": selected_order,
                "member_edges": workspace_graph.member_edges,
                "members": members.len(),
                "succeeded": members.len(),
                "failed": 0,
                "results": member_results,
            }),
            human_lines: {
                human_lines.push(format!("Workspace build complete: {} members", members.len()).green().to_string());
                human_lines
            },
        }
        .emit(args.json)
    }
}

use super::super::output::{print_json, read_json_value, CommandOutcome};
use super::analysis_reports::{
    profile_report_json, proof_diff_report, protocol_graph_json, protocol_graph_mermaid, trace_tx_report_json,
};
use super::audit_reports::{
    audit_bundle_html, audit_bundle_json, print_proof_plan_record, print_proof_plan_summary, proof_plan_summary_json,
};
use super::{
    compile_cli_input, print_or_text_json, read_metadata_json, runtime_error_info_from_query, AuditBundleArgs, CommandExecutor,
    ExplainArgs, ExplainAssumptionsArgs, ExplainCollectionsArgs, ExplainGenericsArgs, ExplainGraphArgs, ExplainProfileArgs,
    ExplainProofArgs, ProfileArgs, ProofDiffArgs, TraceTxArgs,
};
use crate::error::Result;
use crate::{compile_path, ArtifactFormat, CompileOptions, TargetProfile};
use camino::Utf8Path;
use std::path::PathBuf;

impl CommandExecutor {
    pub(super) fn explain(args: ExplainArgs) -> Result<()> {
        let code = args.code.to_ascii_uppercase();
        if let Some((domain, info)) = crate::error::compiler_error_info_by_code(&code)
            .map(|info| ("compiler", info))
            .or_else(|| crate::error::cli_error_info_by_code(&code).map(|info| ("cli", info)))
        {
            let outcome = CommandOutcome {
                machine: serde_json::json!({
                    "status": "ok",
                    "domain": domain,
                    "code": info.code,
                    "name": info.name,
                    "description": info.description,
                    "hint": info.hint,
                }),
                human_lines: vec![
                    format!("CellScript {domain} error {}: {}", info.code, info.name),
                    format!("  Description: {}", info.description),
                    format!("  Hint: {}", info.hint),
                ],
            };
            return outcome.emit(args.json);
        }

        let info = runtime_error_info_from_query(&args.code).ok_or_else(|| {
            crate::error::CompileError::without_span(format!(
                "unknown CellScript error '{}'; use a runtime numeric/E-code/name, compiler E-code or CLI code",
                args.code
            ))
        })?;

        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "code": info.code,
                "ecode": format!("E{:04}", info.code),
                "name": info.name,
                "description": info.description,
                "hint": info.hint,
            }),
            human_lines: vec![
                format!("CellScript runtime error E{:04} ({}): {}", info.code, info.code, info.name),
                format!("  Description: {}", info.description),
                format!("  Hint: {}", info.hint),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn explain_proof(args: ExplainProofArgs) -> Result<()> {
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
        let proof_plan = result.metadata.runtime.proof_plan;

        if args.json {
            let proof_plan_summary = proof_plan_summary_json(&proof_plan);
            let summary = serde_json::json!({
                "status": "ok",
                "module": result.metadata.module,
                "target_profile": result.metadata.target_profile.name,
                "proof_plan_summary": proof_plan_summary,
                "proof_plan": proof_plan,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize ProofPlan: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("Covenant ProofPlan for module `{}`", result.metadata.module);
        print_proof_plan_summary(&proof_plan);
        if proof_plan.is_empty() {
            println!("  No ProofPlan records emitted.");
            return Ok(());
        }
        for plan in &proof_plan {
            print_proof_plan_record(plan);
        }
        Ok(())
    }

    pub(super) fn explain_assumptions(args: ExplainAssumptionsArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
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
        let assumptions = result.metadata.runtime.builder_assumptions.clone();
        let summary = serde_json::json!({
            "status": "ok",
            "module": result.metadata.module,
            "target_profile": result.metadata.target_profile.name,
            "assumption_count": assumptions.len(),
            "proof_plan_soundness": result.metadata.runtime.proof_plan_soundness,
            "builder_assumptions": assumptions,
        });
        let mut human_lines = vec![
            format!("Builder assumptions for module `{}`", result.metadata.module),
            format!("  Assumptions: {}", summary["assumption_count"]),
            format!("  ProofPlan soundness: {}", summary["proof_plan_soundness"]["status"].as_str().unwrap_or("unknown")),
        ];
        human_lines.extend(
            result
                .metadata
                .runtime
                .builder_assumptions
                .iter()
                .map(|assumption| format!("  - {} [{}] {}", assumption.assumption_id, assumption.kind, assumption.feature)),
        );
        CommandOutcome { machine: summary, human_lines }.emit(args.json)
    }

    pub(super) fn proof_diff(args: ProofDiffArgs) -> Result<()> {
        let old = read_metadata_json(&args.old)?;
        let new = read_metadata_json(&args.new)?;
        let diff = proof_diff_report(&old, &new);
        print_or_text_json(args.json, &diff, "Proof diff")?;
        Ok(())
    }

    pub(super) fn profile(args: ProfileArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
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
        let report = profile_report_json(&result.metadata, args.entry.as_deref());
        print_or_text_json(args.json, &report, "Profile")?;
        Ok(())
    }

    pub(super) fn trace_tx(args: TraceTxArgs) -> Result<()> {
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        let validation = crate::assumptions::validate_transaction_against_metadata(&metadata, &tx);
        let trace = trace_tx_report_json(&metadata, &validation);
        let outcome = CommandOutcome {
            human_lines: vec![format!("Transaction trace: {}", trace["status"].as_str().unwrap_or("unknown"))],
            machine: trace,
        };
        if validation.status == "failed" {
            return Err(outcome.failure("transaction trace found builder assumption violations"));
        }
        outcome.emit(args.json)
    }

    pub(super) fn audit_bundle(args: AuditBundleArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
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
        let output = args.output.unwrap_or_else(|| PathBuf::from("target/cellscript-audit-bundle"));
        std::fs::create_dir_all(&output)?;
        let bundle = audit_bundle_json(&result.metadata);
        let json_path = output.join("audit-bundle.json");
        let html_path = output.join("index.html");
        std::fs::write(
            &json_path,
            serde_json::to_string_pretty(&bundle)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize audit bundle: {}", error)))?,
        )?;
        std::fs::write(&html_path, audit_bundle_html(&bundle))?;
        let summary = serde_json::json!({
            "status": "ok",
            "output": output.display().to_string(),
            "json": json_path.display().to_string(),
            "html": html_path.display().to_string(),
        });
        CommandOutcome {
            machine: summary,
            human_lines: vec![
                "Audit bundle generated".to_string(),
                format!("  JSON: {}", json_path.display()),
                format!("  HTML: {}", html_path.display()),
            ],
        }
        .emit(args.json)
    }

    pub(super) fn explain_generics(args: ExplainGenericsArgs) -> Result<()> {
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
        let value_instantiations = result.metadata.generic_instantiations;
        let collection_instantiations = result.metadata.runtime.collection_instantiations;

        let mut human_lines = Vec::new();
        if value_instantiations.is_empty() {
            human_lines.push("No value-generic monomorphizations found.".to_string());
        } else {
            human_lines.push("Checked value-generic monomorphizations:".to_string());
            for instantiation in &value_instantiations {
                human_lines.push(format!("  {} {} -> {}", instantiation.kind, instantiation.identity, instantiation.concrete_name));
                human_lines.push(format!("    type arguments: {}", instantiation.type_arguments.join(", ")));
                human_lines.push(format!("    value abilities: passed (registry v{})", instantiation.value_ability_registry_version));
                human_lines.push(format!(
                    "    layout policy: {}",
                    if instantiation.fixed_layout_required {
                        "fixed value layout required"
                    } else {
                        "callable specialization; no container layout"
                    }
                ));
                human_lines.push(format!(
                    "    Cell argument policy: {}",
                    if instantiation.cell_backed_layout_rejected {
                        "passed; hidden Cell-backed values are forbidden in this layout"
                    } else {
                        "passed; Cell-backed arguments require an explicit cell constraint"
                    }
                ));
            }
        }
        if collection_instantiations.is_empty() {
            human_lines.push("No checked bounded generic collection instantiations found.".to_string());
        } else {
            human_lines.push("Checked bounded generic collection instantiations:".to_string());
            for instantiation in &collection_instantiations {
                human_lines.push(format!(
                    "  {} {}: {} -> {} ({} byte element, max {}, {})",
                    instantiation.scope_kind,
                    instantiation.scope_name,
                    instantiation.collection_ty,
                    instantiation.element_ty,
                    instantiation.element_width_bytes,
                    instantiation.max_elements,
                    instantiation.status
                ));
                human_lines.push(format!("    backing: {}", instantiation.backing));
                human_lines.push(format!("    helpers: {}", instantiation.helpers.join(", ")));
            }
        }
        let value_instantiation_view = value_instantiations
            .iter()
            .map(|instantiation| {
                serde_json::json!({
                    "kind": instantiation.kind,
                    "module": instantiation.module,
                    "template": instantiation.template,
                    "concrete_name": instantiation.concrete_name,
                    "identity": instantiation.identity,
                    "type_arguments": instantiation.type_arguments,
                    "verification": {
                        "value_abilities": "passed",
                        "value_ability_registry_version": instantiation.value_ability_registry_version,
                        "layout_policy": if instantiation.fixed_layout_required {
                            "fixed-value-layout-required"
                        } else {
                            "callable-specialization-no-container-layout"
                        },
                        "cell_argument_policy": if instantiation.cell_backed_layout_rejected {
                            "passed-hidden-cell-values-forbidden"
                        } else {
                            "passed-explicit-cell-constraint-required"
                        },
                        "phantom_arguments_in_identity": instantiation.identity_includes_phantom_arguments,
                    },
                })
            })
            .collect::<Vec<_>>();
        CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "count": value_instantiations.len() + collection_instantiations.len(),
                "value_instantiations": value_instantiation_view,
                "collection_instantiations": collection_instantiations,
            }),
            human_lines,
        }
        .emit(args.json)
    }

    pub(super) fn explain_collections(args: ExplainCollectionsArgs) -> Result<()> {
        let input = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input)
            .ok_or_else(|| crate::error::CompileError::without_span("collection budget input path is not valid UTF-8"))?;
        let (machine, human_lines) = crate::cli::collection_budget::explain(input)?;
        CommandOutcome { machine, human_lines }.emit(args.json)
    }

    pub(super) fn explain_graph(args: ExplainGraphArgs) -> Result<()> {
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
        let graph = protocol_graph_json(&result.metadata);
        let format = args.format.as_deref().unwrap_or(if args.json { "json" } else { "summary" });
        match format {
            "json" => print_json(&graph),
            "mermaid" => {
                print!("{}", protocol_graph_mermaid(&graph));
                Ok(())
            }
            "summary" => {
                println!("ProtocolGraph: {}", graph["schema"].as_str().unwrap_or("unknown"));
                println!("  Vertices: {}", graph["vertex_count"].as_u64().unwrap_or_default());
                println!("  Edges: {}", graph["edge_count"].as_u64().unwrap_or_default());
                println!("  Cycles: {}", if graph["cycle_detected"].as_bool().unwrap_or(false) { "detected" } else { "not detected" });
                println!("  Role lints: {}", graph["role_warning_count"].as_u64().unwrap_or_default());
                println!("  Role authority: metadata-only (never authorization proof)");
                println!("  Consensus checked: no");
                Ok(())
            }
            other => Err(crate::error::CompileError::without_span(format!(
                "unsupported graph format '{}'; expected json or mermaid",
                other
            ))),
        }
    }

    pub(super) fn explain_profile(args: ExplainProfileArgs) -> Result<()> {
        let profile = TargetProfile::from_name(&args.profile)?;
        let metadata = profile.metadata(ArtifactFormat::RiscvElf);
        let summary = serde_json::json!({
            "profile": metadata.name,
            "target_chain": metadata.target_chain,
            "vm_abi": metadata.vm_abi,
            "hash_domain": metadata.hash_domain,
            "syscall_set": metadata.syscall_set,
            "artifact_packaging": metadata.artifact_packaging,
            "header_abi": metadata.header_abi,
            "scheduler_abi": metadata.scheduler_abi,
            "witness_abi": metadata.witness_abi,
            "lock_args_abi": metadata.lock_args_abi,
            "source_encoding": metadata.source_encoding,
            "spawn_ipc_abi": metadata.spawn_ipc_abi,
            "since_abi": metadata.since_abi,
            "cell_dep_abi": metadata.cell_dep_abi,
            "script_ref_abi": metadata.script_ref_abi,
            "output_data_abi": metadata.output_data_abi,
            "capacity_floor_abi": metadata.capacity_floor_abi,
            "type_id_abi": metadata.type_id_abi,
            "tx_version": metadata.tx_version,
            "boundaries": [
                "WitnessArgs fields are explicit CKB witness surfaces, not implicit signer authority",
                "lock_args parameters are typed script args, not implicit signer authority",
                "Source group views are scoped to the active script group",
                "outputs and outputs_data are index-aligned CKB transaction surfaces",
                "capacity floors are declared in shannons and still require builder measurement",
                "script references keep code_hash, hash_type, and args visible",
                "TYPE_ID metadata uses the CKB TYPE_ID ABI and does not hide builder obligations",
                "Spawn/IPC is bounded verifier reuse and does not make type scripts multi-tenant",
                "hash_blake2b(input: Hash) uses CKB Blake2b-256 for one Hash",
                "hash_pair(left: Hash, right: Hash) uses CKB Blake2b-256 over two Hash values; wider byte serialization hashing remains out of scope"
            ],
        });
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize profile explanation: {}", error))
                })?
            );
        } else {
            println!("Target profile: {}", summary["profile"].as_str().unwrap_or("unknown"));
            println!("  Target chain: {}", summary["target_chain"].as_str().unwrap_or("unknown"));
            println!("  VM ABI: {}", summary["vm_abi"].as_str().unwrap_or("unknown"));
            println!("  Witness ABI: {}", summary["witness_abi"].as_str().unwrap_or("unknown"));
            println!("  Lock args ABI: {}", summary["lock_args_abi"].as_str().unwrap_or("unknown"));
            println!("  Source encoding: {}", summary["source_encoding"].as_str().unwrap_or("unknown"));
            println!("  Spawn/IPC ABI: {}", summary["spawn_ipc_abi"].as_str().unwrap_or("unknown"));
            println!("  Since ABI: {}", summary["since_abi"].as_str().unwrap_or("unknown"));
            println!("  CellDep ABI: {}", summary["cell_dep_abi"].as_str().unwrap_or("unknown"));
            println!("  Script ref ABI: {}", summary["script_ref_abi"].as_str().unwrap_or("unknown"));
            println!("  Output data ABI: {}", summary["output_data_abi"].as_str().unwrap_or("unknown"));
            println!("  Capacity floor ABI: {}", summary["capacity_floor_abi"].as_str().unwrap_or("unknown"));
            println!("  TYPE_ID ABI: {}", summary["type_id_abi"].as_str().unwrap_or("unknown"));
        }
        Ok(())
    }
}

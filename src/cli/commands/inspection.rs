use super::{
    compile_failure_diagnostics, diagnostics_to_error, emit_inspection_document, inspection_resolution_options,
    read_or_compile_interface, render_public_interface_human, BuildPlanArgs, CommandExecutor, ExpandArgs, InterfaceArgs,
    InterfaceDiffArgs, MetadataArgs, ResolveGraphArgs, SchemaAckArgs,
};
use crate::error::{CompileError, Result};
use crate::{compile_path, CompileOptions};
use camino::Utf8Path;
use colored::Colorize;
use std::path::{Path, PathBuf};

impl CommandExecutor {
    pub(super) fn metadata(args: MetadataArgs) -> Result<()> {
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
        let metadata = if let Some(name) = args.artifact.as_deref() {
            crate::artifact::compile_path_artifact_metadata(input, options, name)?
        } else {
            match compile_path(input, options.clone()) {
                Ok(result) => result.metadata,
                Err(error) => return Err(diagnostics_to_error(&compile_failure_diagnostics(input, options, error))),
            }
        };
        let json = serde_json::to_string_pretty(&metadata)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize metadata: {}", error)))?;

        if let Some(output_path) = args.output.as_ref() {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, json)?;
            println!("{}", "Metadata generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    pub(super) fn resolve_graph(args: ResolveGraphArgs) -> Result<()> {
        crate::package::inspection::validate_schema_version(args.schema_version)?;
        let input = args.input.as_deref().unwrap_or_else(|| Path::new("."));
        let options = inspection_resolution_options(
            &args.scope,
            &args.features,
            args.all_features,
            args.no_default_features,
            args.environment.as_deref(),
            args.offline,
        )?;
        let graph = crate::package::inspection::resolve_graph(input, args.package.as_deref(), &options)?;
        emit_inspection_document("resolve graph", &graph, args.output.as_deref(), args.json)
    }

    pub(super) fn build_plan(args: BuildPlanArgs) -> Result<()> {
        crate::package::inspection::validate_build_plan_schema_version(args.schema_version)?;
        let input = args.input.as_deref().unwrap_or_else(|| Path::new("."));
        let resolution = inspection_resolution_options(
            &args.scope,
            &args.features,
            args.all_features,
            args.no_default_features,
            args.environment.as_deref(),
            args.offline,
        )?;
        let graph = crate::package::inspection::resolve_graph(input, args.package.as_deref(), &resolution)?;
        let plan = crate::package::inspection::build_plan(
            &graph,
            &crate::package::inspection::BuildPlanOptions {
                source_contracts: args.source_contracts,
                target: args.target,
                target_profile: args.target_profile,
                release: args.release,
                debug: args.debug,
                primitive_compat: args.primitive_compat,
                entry_action: args.entry_action,
                entry_lock: args.entry_lock,
                artifact: args.artifact,
                production: args.production,
                deny_fail_closed: args.deny_fail_closed,
                deny_ckb_runtime: args.deny_ckb_runtime,
                deny_runtime_obligations: args.deny_runtime_obligations,
            },
        )?;
        emit_inspection_document("build plan", &plan, args.output.as_deref(), args.json)
    }

    pub(super) fn expand(args: ExpandArgs) -> Result<()> {
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
        let metadata = if let Some(name) = args.artifact.as_deref() {
            crate::artifact::compile_path_artifact_metadata(input, options, name)?
        } else {
            match compile_path(input, options.clone()) {
                Ok(result) => result.metadata,
                Err(error) => return Err(diagnostics_to_error(&compile_failure_diagnostics(input, options, error))),
            }
        };
        let rendered = if args.json {
            serde_json::to_string_pretty(&metadata.typed_semantics.foundation)
                .map_err(|error| CompileError::without_span(format!("failed to serialize semantic foundation: {error}")))?
        } else {
            crate::semantic_expansion::render(&metadata)?
        };
        if let Some(output_path) = args.output.as_ref() {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, rendered)?;
            if !args.json {
                println!("{}", "Canonical semantic expansion generated".green());
                println!("  Output: {}", output_path.display());
            }
        } else {
            println!("{}", rendered);
        }
        Ok(())
    }

    pub(super) fn interface(args: InterfaceArgs) -> Result<()> {
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
        let interface = &result.metadata.public_interface;
        let value = serde_json::json!({
            "interface_hash": result.metadata.interface_hash,
            "interface": interface,
        });
        let json = serde_json::to_string_pretty(&value)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize public interface: {error}")))?;
        if let Some(output_path) = args.output.as_ref() {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, format!("{json}\n"))?;
            println!("{}", "Public interface generated".green());
            println!("  Hash: {}", result.metadata.interface_hash);
            println!("  Output: {}", output_path.display());
        } else if args.json {
            println!("{json}");
        } else {
            print!("{}", render_public_interface_human(interface, &result.metadata.interface_hash));
        }
        Ok(())
    }

    pub(super) fn interface_diff(args: InterfaceDiffArgs) -> Result<()> {
        if args.checked {
            return crate::cli::interface_diff::run(args);
        }
        let old = read_or_compile_interface(&args.old)?;
        let new = read_or_compile_interface(&args.new)?;
        let report = crate::interface::compare(&old, &new);
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize interface report: {error}")))?;
        if let Some(output_path) = args.output.as_ref() {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, format!("{json}\n"))?;
        }
        if !report.compatible && args.json {
            let details = serde_json::to_value(&report)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize interface report: {error}")))?;
            return Err(crate::error::CompileError::without_span("public interface contains breaking changes")
                .with_code(crate::error::DiagnosticCode::E2501)
                .with_details(details));
        }
        if args.json || args.output.is_none() {
            println!("{json}");
        } else {
            println!("Interface compatibility: {}", if report.compatible { "compatible" } else { "breaking" });
            for dimension in &report.dimensions {
                println!("  {}: {}", dimension.dimension, dimension.classification);
            }
            if let Some(output_path) = args.output.as_ref() {
                println!("  Output: {}", output_path.display());
            }
        }
        if !report.compatible {
            return Err(crate::error::CompileError::without_span("public interface contains breaking changes")
                .with_code(crate::error::DiagnosticCode::E2501));
        }
        Ok(())
    }

    pub(super) fn schema_ack(args: SchemaAckArgs) -> Result<()> {
        let selector = crate::schema_acknowledgement::SchemaRelationSelector {
            action: args.action.clone(),
            before: args.before.clone(),
            after: args.after.clone(),
        };
        let plan = if let Some(upgrade_plan) = &args.upgrade_plan {
            let upgrade = crate::package::upgrade::read_upgrade_plan(upgrade_plan)?;
            let member = args
                .subject
                .as_deref()
                .ok_or_else(|| crate::error::CompileError::without_span("schema-ack --upgrade-plan requires --subject"))?;
            let matches = upgrade
                .schema_reviews
                .iter()
                .filter(|review| {
                    review.subject == member
                        && review.environment == args.environment
                        && review.plan.selector == selector
                        && args.module.as_ref().is_none_or(|module| &review.plan.module == module)
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [review] => review.plan.clone(),
                _ => {
                    return Err(crate::error::CompileError::without_span(
                        "upgrade plan does not contain exactly one schema review matching subject/environment/module/action/roles; use --module to select a retired or fresh module endpoint",
                    ))
                }
            }
        } else {
            let compile_candidate = |path: &Path| -> Result<crate::CompileResult> {
                let input = Utf8Path::from_path(path).ok_or_else(|| {
                    crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", path.display()))
                })?;
                crate::without_incremental_cache(|| crate::compile_path(input, CompileOptions::default()))
            };
            let old = compile_candidate(&args.old)?;
            let new = compile_candidate(&args.new)?;
            crate::schema_acknowledgement::build_schema_change_plan(&old, &new, selector)?
        };

        let (value, label) = if let Some(receipt_path) = args.verify.as_ref() {
            if args.acknowledge_by.is_some() || args.rationale.is_some() {
                return Err(crate::error::CompileError::without_span(
                    "schema-ack --verify cannot be combined with --acknowledge-by or --rationale",
                ));
            }
            let bytes = std::fs::read(receipt_path)?;
            let receipt: crate::schema_acknowledgement::SchemaAcknowledgementReceipt =
                serde_json::from_slice(&bytes).map_err(|error| {
                    crate::error::CompileError::without_span(format!(
                        "failed to parse schema acknowledgement '{}': {error}",
                        receipt_path.display()
                    ))
                })?;
            crate::schema_acknowledgement::verify_schema_acknowledgement(&plan, &receipt)?;
            (
                serde_json::json!({
                    "schema": "cellscript-schema-acknowledgement-verification-v2",
                    "version": 2,
                    "status": "verified",
                    "plan_hash": plan.plan_hash,
                    "acknowledgement_hash": receipt.acknowledgement_hash,
                }),
                "Schema acknowledgement verified",
            )
        } else if args.acknowledge_by.is_some() || args.rationale.is_some() {
            let reviewer = args
                .acknowledge_by
                .ok_or_else(|| crate::error::CompileError::without_span("schema-ack receipt creation requires --acknowledge-by"))?;
            let rationale = args
                .rationale
                .ok_or_else(|| crate::error::CompileError::without_span("schema-ack receipt creation requires --rationale"))?;
            let receipt = crate::schema_acknowledgement::acknowledge_schema_change(&plan, reviewer, rationale)?;
            (serde_json::to_value(receipt)?, "Schema acknowledgement created")
        } else {
            (serde_json::to_value(plan)?, "Schema change plan generated")
        };

        let json = serde_json::to_string_pretty(&value).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize schema acknowledgement: {error}"))
        })?;
        if let Some(output_path) = args.output.as_ref() {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, format!("{json}\n"))?;
            println!("{label}");
            println!("  Output: {}", output_path.display());
        } else {
            println!("{json}");
        }
        Ok(())
    }
}

use super::super::output::{read_json_value, CommandOutcome};
use super::check_policy::{
    checked_pool_invariant_family_count, fail_closed_obligation_count, pool_invariant_family_blocker_class_count,
    pool_invariant_family_blocker_class_summaries, pool_runtime_input_requirement_count, pool_runtime_input_requirement_summaries,
    runtime_required_obligation_count, runtime_required_pool_invariant_family_count,
    runtime_required_transaction_invariant_checked_subcondition_count, runtime_required_transaction_invariant_count,
    transaction_invariant_checked_subcondition_summaries, transaction_runtime_input_blocker_class_count_by_status,
    transaction_runtime_input_blocker_class_summaries_by_status, transaction_runtime_input_blocker_count_by_status,
    transaction_runtime_input_blocker_summaries_by_status, transaction_runtime_input_requirement_count,
    transaction_runtime_input_requirement_count_by_status, transaction_runtime_input_requirement_summaries,
    transaction_runtime_input_requirement_summaries_by_status, validate_check_policy, validate_expected_metadata_hash,
    validate_expected_target_profile,
};
use super::receipt_records::verify_compile_receipt_against_metadata;
use super::{metadata_schema_versions_json, CheckArgs, CommandExecutor, ProtocolBundleCheckArgs, VerifyArtifactArgs};
use crate::error::Result;
use crate::{default_metadata_path_for_artifact, validate_artifact_metadata, validate_source_units_on_disk, CompileMetadata};
use camino::Utf8Path;
use colored::Colorize;

impl CommandExecutor {
    pub(super) fn verify_artifact(args: VerifyArtifactArgs) -> Result<()> {
        let artifact_path = Utf8Path::from_path(&args.artifact).ok_or_else(|| {
            crate::error::CompileError::without_span(format!("artifact path '{}' is not valid UTF-8", args.artifact.display()))
        })?;
        let metadata_path = match args.metadata.as_ref() {
            Some(path) => path.clone(),
            None => default_metadata_path_for_artifact(artifact_path).into_std_path_buf(),
        };

        let artifact_bytes = std::fs::read(&args.artifact).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read artifact '{}': {}", args.artifact.display(), error))
        })?;
        let metadata_bytes = std::fs::read(&metadata_path).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read metadata '{}': {}", metadata_path.display(), error))
        })?;
        let metadata: CompileMetadata = serde_json::from_slice(&metadata_bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to parse metadata '{}': {}", metadata_path.display(), error))
        })?;
        let (checker_report, lowering_record_path, source_map_path) = if metadata.artifact_format == "RISC-V ELF" {
            let lowering_record_path = args
                .lowering_record
                .clone()
                .unwrap_or_else(|| crate::lowering_record_output_path_from_artifact(artifact_path).into_std_path_buf());
            let source_map_path = args
                .source_map
                .clone()
                .unwrap_or_else(|| crate::source_map_output_path_from_artifact(artifact_path).into_std_path_buf());
            let lowering_record_bytes = std::fs::read(&lowering_record_path).map_err(|error| {
                crate::error::CompileError::without_span(format!(
                    "failed to read lowering record '{}': {}",
                    lowering_record_path.display(),
                    error
                ))
            })?;
            let source_map_bytes = std::fs::read(&source_map_path).map_err(|error| {
                crate::error::CompileError::without_span(format!(
                    "failed to read source map '{}': {}",
                    source_map_path.display(),
                    error
                ))
            })?;
            let report = cellscript_artifact_checker::check_bundle(
                &artifact_bytes,
                &metadata_bytes,
                &lowering_record_bytes,
                &source_map_bytes,
                &crate::CheckerBudgets::default(),
            )
            .map_err(|error| crate::error::CompileError::without_span(error.to_string()))?;
            (Some(report), Some(lowering_record_path), Some(source_map_path))
        } else {
            if args.lowering_record.is_some() || args.source_map.is_some() {
                return Err(crate::error::CompileError::without_span(
                    "--lowering-record/--source-map are only valid for RISC-V ELF artifacts",
                ));
            }
            (None, None, None)
        };
        let result = validate_artifact_metadata(artifact_bytes, metadata)?;
        if args.verify_sources {
            validate_source_units_on_disk(&result.metadata)?;
        }
        validate_expected_target_profile(result.metadata.target_profile.name.as_str(), args.expect_target_profile.as_deref())?;
        validate_expected_metadata_hash(
            "artifact_hash",
            result.metadata.artifact_hash.as_deref(),
            args.expect_artifact_hash.as_deref(),
        )?;
        validate_expected_metadata_hash("source_hash", result.metadata.source_hash.as_deref(), args.expect_source_hash.as_deref())?;
        validate_expected_metadata_hash(
            "source_content_hash",
            result.metadata.source_content_hash.as_deref(),
            args.expect_source_content_hash.as_deref(),
        )?;
        validate_check_policy(
            &result.metadata,
            &CheckArgs {
                production: args.production,
                deny_fail_closed: args.deny_fail_closed,
                deny_ckb_runtime: args.deny_ckb_runtime,
                deny_runtime_obligations: args.deny_runtime_obligations,
                primitive_compat: args.primitive_compat,
                ..CheckArgs::default()
            },
        )?;
        let receipt_report = if let Some(receipt_path) = args.receipt.as_deref() {
            let receipt = read_json_value(receipt_path)?;
            Some(verify_compile_receipt_against_metadata(&receipt, &result.metadata)?)
        } else {
            None
        };

        let expected_target_profile_verified = args.expect_target_profile.is_some();
        let expected_hashes_verified =
            args.expect_artifact_hash.is_some() || args.expect_source_hash.is_some() || args.expect_source_content_hash.is_some();
        let policy_verified = args.production || args.deny_fail_closed || args.deny_ckb_runtime || args.deny_runtime_obligations;

        if args.json {
            let mut summary = serde_json::json!({
                "status": "ok",
                "artifact": args.artifact.display().to_string(),
                "metadata": metadata_path.display().to_string(),
                "metadata_schema_version": result.metadata.metadata_schema_version,
                "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
                "compiler_version": result.metadata.compiler_version,
                "artifact_format": result.artifact_format.display_name(),
                "target_profile": result.metadata.target_profile.name.as_str(),
                "artifact_hash": result.metadata.artifact_hash,
                "artifact_size_bytes": result.artifact_bytes.len(),
                "source_hash": result.metadata.source_hash,
                "source_content_hash": result.metadata.source_content_hash,
                "source_units": result.metadata.source_units.len(),
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
                "sources_verified": args.verify_sources,
                "expected_target_profile_verified": expected_target_profile_verified,
                "expected_hashes_verified": expected_hashes_verified,
                "policy_verified": policy_verified,
                "constraints": &result.metadata.constraints,
            });
            if let Some(object) = summary.as_object_mut() {
                object.insert(
                    "lowering_record".to_string(),
                    serde_json::json!(lowering_record_path.as_ref().map(|path| path.display().to_string())),
                );
                object.insert(
                    "source_map".to_string(),
                    serde_json::json!(source_map_path.as_ref().map(|path| path.display().to_string())),
                );
                object.insert("binding_verification".to_string(), serde_json::json!("verified"));
                object.insert(
                    "structural_verification".to_string(),
                    serde_json::json!(if checker_report.is_some() { "verified" } else { "not-applicable" }),
                );
                object.insert(
                    "lowering_record_verification".to_string(),
                    serde_json::json!(if checker_report.is_some() { "verified" } else { "not-applicable" }),
                );
                object.insert("ckb_vm_evidence".to_string(), serde_json::json!("not-executed"));
                object.insert("chain_evidence".to_string(), serde_json::json!("not-provided"));
                object.insert("semantic_equivalence_claimed".to_string(), serde_json::json!(false));
                object.insert("checker_report".to_string(), serde_json::json!(&checker_report));
                object.insert("receipt_verified".to_string(), serde_json::json!(receipt_report.is_some()));
                object.insert(
                    "receipt_payload_hash".to_string(),
                    serde_json::json!(receipt_report.as_ref().map(|report| report.payload_hash.as_str())),
                );
                object.insert(
                    "receipt_signatures_verified".to_string(),
                    serde_json::json!(receipt_report.as_ref().map(|report| report.signatures_verified).unwrap_or(0)),
                );
                object.insert(
                    "receipt_unsigned_advisory".to_string(),
                    serde_json::json!(receipt_report.as_ref().map(|report| report.unsigned_advisory).unwrap_or(false)),
                );
            }
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize verification summary: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Artifact verification succeeded".green());
        println!("  Artifact: {}", args.artifact.display());
        println!("  Metadata: {}", metadata_path.display());
        println!("  Metadata schema: {}", result.metadata.metadata_schema_version);
        println!(
            "  Metadata schema components: source={}, artifact={}, constraints={}",
            result.metadata.source_metadata_schema_version,
            result.metadata.artifact_metadata_schema_version,
            result.metadata.constraints_metadata_schema_version
        );
        println!("  Compiler: {}", result.metadata.compiler_version);
        println!("  Format: {}", result.artifact_format.display_name());
        println!("  Target profile: {}", result.metadata.target_profile.name);
        println!("  Hash: {}", result.metadata.artifact_hash.as_deref().unwrap_or("missing"));
        println!("  Size: {} bytes", result.artifact_bytes.len());
        println!("  Binding verification: verified");
        if checker_report.is_some() {
            println!("  Structural verification: verified");
            println!("  Lowering-record verification: verified");
            if let Some(path) = lowering_record_path {
                println!("  Lowering record: {}", path.display());
            }
            if let Some(path) = source_map_path {
                println!("  Source map: {}", path.display());
            }
        } else {
            println!("  Structural verification: not applicable to assembly");
            println!("  Lowering-record verification: not applicable to assembly");
        }
        println!("  CKB-VM evidence: not executed");
        println!("  Chain evidence: not provided");
        println!("  Semantic equivalence: not claimed");
        if expected_target_profile_verified {
            println!("  Expected target profile: verified");
        }
        if expected_hashes_verified {
            println!("  Expected hashes: verified");
        }
        if args.verify_sources {
            println!("  Sources: verified {} unit(s)", result.metadata.source_units.len());
        }
        if policy_verified {
            println!("  Policy: verified");
        }
        if let Some(report) = receipt_report {
            println!("  Receipt: verified");
            println!("  Receipt payload hash: {}", report.payload_hash);
            if report.unsigned_advisory {
                println!("  Receipt signatures: unsigned advisory");
            } else {
                println!("  Receipt signatures verified: {}", report.signatures_verified);
            }
        }
        Ok(())
    }

    pub(super) fn protocol_bundle_check(args: ProtocolBundleCheckArgs) -> Result<()> {
        let report = if let Some(open) = args.open_admission.as_deref() {
            crate::protocol_bundle::check_protocol_bundle_with_open_handles_file(&args.manifest, open)?
        } else {
            crate::protocol_bundle::check_protocol_bundle_file(&args.manifest)?
        };
        let machine = serde_json::to_value(&report).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize ProtocolBundle report: {}", error))
        })?;
        if let Some(output) = args.output.as_deref() {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(
                output,
                serde_json::to_vec_pretty(&report).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize ProtocolBundle report: {}", error))
                })?,
            )?;
        }

        let human_lines = vec![
            format!("ProtocolBundle check: {}", report.status),
            format!("  Bundle hash: {}", report.bundle_hash),
            format!("  Artifacts: {}", report.bundle.artifacts.len()),
            format!("  Conflicts: {}", report.conflicts.len()),
            "  Runtime evidence: not executed".to_string(),
        ];
        let failed = report.status != "ok";
        let outcome = CommandOutcome { machine, human_lines };
        if failed {
            Err(outcome.failure(format!("ProtocolBundle has {} unresolved conflict(s)", report.conflicts.len())))
        } else {
            outcome.emit(args.json)
        }
    }
}

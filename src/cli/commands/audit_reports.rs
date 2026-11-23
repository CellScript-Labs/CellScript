use super::analysis_reports::protocol_graph_json;
use super::metadata_schema_versions_json;
use crate::{CompileMetadata, ProofPlanMetadata};
use std::collections::BTreeMap;

pub(super) fn audit_bundle_json(metadata: &CompileMetadata) -> serde_json::Value {
    // Source-to-codegen mapping: link ProofPlan records to source spans, IR effects, and codegen coverage
    let source_to_codegen = metadata
        .runtime
        .proof_plan
        .iter()
        .map(|plan| {
            serde_json::json!({
                "origin": plan.origin,
                "feature": plan.feature,
                "status": plan.status,
                "source_span": plan.source_span.as_ref().map(|span| serde_json::json!({
                    "start": span.start,
                    "end": span.end,
                    "line": span.line,
                    "column": span.column,
                })).unwrap_or(serde_json::Value::Null),
                "trigger": plan.trigger,
                "scope": plan.scope,
                "codegen_coverage_status": plan.codegen_coverage_status,
                "on_chain_checked": plan.on_chain_checked,
                "reads": plan.reads,
                "coverage": plan.coverage,
                "builder_assumptions": plan.builder_assumptions,
                "diagnostics": plan.diagnostics.iter().map(|diag| serde_json::json!({
                    "severity": diag.severity,
                    "message": diag.message,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    // Action-level source-to-IR-to-codegen trace
    let action_traces = metadata
        .actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "estimated_cycles": action.estimated_cycles,
                "proof_plan_records": action.proof_plan.len(),
                "proof_plan_source_mappings": action.proof_plan.iter().map(|plan| serde_json::json!({
                    "origin": plan.origin,
                    "feature": plan.feature,
                    "source_span": plan.source_span,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                })).collect::<Vec<_>>(),
                "runtime_accesses": action.ckb_runtime_accesses.iter().map(|access| serde_json::json!({
                    "source": access.source,
                    "operation": access.operation,
                    "index": access.index,
                    "binding": access.binding,
                    "provenance": access.provenance,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    // Lock-level source-to-codegen trace
    let lock_traces = metadata
        .locks
        .iter()
        .map(|lock| {
            serde_json::json!({
                "name": lock.name,
                "proof_plan_records": lock.proof_plan.len(),
                "proof_plan_source_mappings": lock.proof_plan.iter().map(|plan| serde_json::json!({
                    "origin": plan.origin,
                    "feature": plan.feature,
                    "source_span": plan.source_span,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                })).collect::<Vec<_>>(),
                "runtime_accesses": lock.ckb_runtime_accesses.iter().map(|access| serde_json::json!({
                    "source": access.source,
                    "operation": access.operation,
                    "index": access.index,
                    "binding": access.binding,
                    "provenance": access.provenance,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-audit-bundle-v0.16",
        "module": metadata.module,
        "compiler_version": metadata.compiler_version,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "target_profile": metadata.target_profile,
        "source_to_codegen": source_to_codegen,
        "proof_plan": metadata.runtime.proof_plan,
        "proof_plan_soundness": metadata.runtime.proof_plan_soundness,
        "protocol_graph": protocol_graph_json(metadata),
        "template_layouts": metadata.template_layouts,
        "builder_assumptions": metadata.runtime.builder_assumptions,
        "constraints": metadata.constraints,
        "actions": action_traces,
        "locks": lock_traces,
        "source_units": metadata.source_units,
        "lowering": metadata.lowering,
        "debug_info_sections": metadata.debug_info_sections,
    })
}

pub(super) fn audit_bundle_html(bundle: &serde_json::Value) -> String {
    // The bundle interleaves publisher-controlled names and diagnostics; the
    // report opens in a browser, so every interpolated text is escaped.
    fn escape_html(value: &str) -> String {
        value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
    }
    let module = bundle.get("module").and_then(serde_json::Value::as_str).unwrap_or("unknown");
    let status = bundle.pointer("/proof_plan_soundness/status").and_then(serde_json::Value::as_str).unwrap_or("unknown");
    let body = serde_json::to_string_pretty(bundle).unwrap_or_else(|_| "{}".to_string());
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>CellScript Audit Bundle</title>\
         <h1>CellScript Audit Bundle</h1><p>Module: {}</p><p>ProofPlan soundness: {}</p>\
         <pre>{}</pre>",
        escape_html(module),
        escape_html(status),
        escape_html(&body)
    )
}

pub(super) fn proof_plan_summary_json(proof_plan: &[ProofPlanMetadata]) -> serde_json::Value {
    let record_count = proof_plan.len();
    let on_chain_checked_count = proof_plan.iter().filter(|plan| plan.on_chain_checked).count();
    let runtime_required_count = proof_plan.iter().filter(|plan| plan.status == "runtime-required").count();
    let metadata_only_gap_count = proof_plan.iter().filter(|plan| plan.codegen_coverage_status == "gap:metadata-only").count();
    let fail_closed_count =
        proof_plan.iter().filter(|plan| plan.status == "fail-closed" || plan.codegen_coverage_status == "fail-closed").count();
    let diagnostic_error_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "error").count();
    let diagnostic_warning_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "warning").count();
    let macro_provenance_count =
        proof_plan.iter().flat_map(|plan| &plan.coverage).filter(|coverage| coverage.starts_with("macro_expansion:")).count();
    let has_runtime_required_gaps = proof_plan.iter().any(|plan| plan.status == "runtime-required" && !plan.on_chain_checked);
    let has_fail_closed_gaps = fail_closed_count > 0;
    let evidence_tier_counts = crate::EvidenceTier::ALL
        .into_iter()
        .map(|tier| (tier.as_str().to_string(), proof_plan.iter().filter(|plan| plan.evidence_tier == tier).count()))
        .collect::<BTreeMap<_, _>>();

    serde_json::json!({
        "record_count": record_count,
        "on_chain_checked_count": on_chain_checked_count,
        "runtime_required_count": runtime_required_count,
        "metadata_only_gap_count": metadata_only_gap_count,
        "fail_closed_count": fail_closed_count,
        "diagnostic_error_count": diagnostic_error_count,
        "diagnostic_warning_count": diagnostic_warning_count,
        "macro_provenance_count": macro_provenance_count,
        "has_runtime_required_gaps": has_runtime_required_gaps,
        "has_fail_closed_gaps": has_fail_closed_gaps,
        "evidence_tier_counts": evidence_tier_counts,
        "has_blocking_diagnostics": has_runtime_required_gaps || has_fail_closed_gaps || diagnostic_error_count > 0,
    })
}

pub(super) fn print_proof_plan_summary(proof_plan: &[ProofPlanMetadata]) {
    let summary = proof_plan_summary_json(proof_plan);
    println!("  Summary:");
    println!("    records: {}", summary["record_count"]);
    println!("    on_chain_checked: {}", summary["on_chain_checked_count"]);
    println!("    runtime_required: {}", summary["runtime_required_count"]);
    println!("    metadata_only_gaps: {}", summary["metadata_only_gap_count"]);
    println!("    fail_closed: {}", summary["fail_closed_count"]);
    println!("    diagnostic_errors: {}", summary["diagnostic_error_count"]);
    println!("    diagnostic_warnings: {}", summary["diagnostic_warning_count"]);
    println!("    macro_provenance_records: {}", summary["macro_provenance_count"]);
    println!("    evidence_tiers: {}", summary["evidence_tier_counts"]);
}

pub(super) fn print_proof_plan_record(plan: &ProofPlanMetadata) {
    let coverage_notes = plan.coverage.iter().filter(|coverage| !coverage.starts_with("macro_expansion:")).collect::<Vec<_>>();
    let macro_provenance = plan.coverage.iter().filter(|coverage| coverage.starts_with("macro_expansion:")).collect::<Vec<_>>();

    println!();
    println!("constraint: {}", plan.name);
    println!("  origin: {}", plan.origin);
    println!("  evidence_tier: {}", plan.evidence_tier.as_str());
    println!("  trigger: {}", plan.trigger);
    println!("  scope: {}", plan.scope);
    println!("  reads:");
    if plan.reads.is_empty() {
        println!("    - none");
    } else {
        for read in &plan.reads {
            println!("    - {}", proof_plan_read_label(read));
        }
    }
    println!("  coverage:");
    if coverage_notes.is_empty() {
        println!("    - none");
    } else {
        for coverage in coverage_notes {
            println!("    - {}", coverage);
        }
    }
    if !macro_provenance.is_empty() {
        println!("  macro_provenance:");
        for provenance in macro_provenance {
            println!("    - {}", provenance);
        }
    }
    println!("  relation_checks:");
    if plan.input_output_relation_checks.is_empty() {
        println!("    - none");
    } else {
        for check in &plan.input_output_relation_checks {
            println!("    - {}", check);
        }
    }
    println!("  on_chain_checked: {}", if plan.on_chain_checked { "yes" } else { "no" });
    println!("  codegen_coverage_status: {}", plan.codegen_coverage_status);
    if !plan.witness_fields.is_empty() {
        println!("  witness_fields:");
        for field in &plan.witness_fields {
            println!("    - {}", field);
        }
    }
    if !plan.lock_args_fields.is_empty() {
        println!("  lock_args_fields:");
        for field in &plan.lock_args_fields {
            println!("    - {}", field);
        }
    }
    println!("  builder_assumption:");
    if plan.builder_assumptions.is_empty() {
        println!("    - none");
    } else {
        for assumption in &plan.builder_assumptions {
            println!("    - {}", assumption);
        }
    }
    for diagnostic in &plan.diagnostics {
        println!("  {}: {}", diagnostic.severity, diagnostic.message);
    }
}

fn proof_plan_read_label(read: &str) -> String {
    match read {
        "input" => "Source::Input".to_string(),
        "output" => "Source::Output".to_string(),
        "group_input" => "Source::GroupInput".to_string(),
        "group_output" => "Source::GroupOutput".to_string(),
        "cell_dep" => "Source::CellDep".to_string(),
        "header_dep" => "Source::HeaderDep".to_string(),
        "witness" => "WitnessArgs".to_string(),
        "lock_args" => "Script.args".to_string(),
        other => other.to_string(),
    }
}

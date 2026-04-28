//! Builder assumption schema and transaction-shape validation for v0.16.

use crate::{ckb_blake2b256, hex_encode, CompileMetadata, ProofPlanMetadata};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuilderAssumptionMetadata {
    pub assumption_id: String,
    pub kind: String,
    pub origin: String,
    pub feature: String,
    pub proof_plan_status: String,
    pub required_inputs: Vec<String>,
    pub required_outputs: Vec<String>,
    pub required_cell_deps: Vec<String>,
    pub required_witness_fields: Vec<String>,
    pub capacity_policy: String,
    pub fee_policy: String,
    pub change_policy: String,
    pub signature_policy: String,
    pub failure_mode: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxValidationReport {
    pub status: String,
    pub assumption_count: usize,
    pub checked_assumptions: Vec<String>,
    pub input_count: usize,
    pub output_count: usize,
    pub cell_dep_count: usize,
    pub witness_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<TxValidationViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TxValidationViolation {
    pub assumption_id: String,
    pub kind: String,
    pub message: String,
}

pub fn builder_assumptions_from_metadata(metadata: &CompileMetadata) -> Vec<BuilderAssumptionMetadata> {
    let mut assumptions = Vec::new();
    let mut seen = BTreeSet::new();

    for plan in &metadata.runtime.proof_plan {
        for assumption in &plan.builder_assumptions {
            push_assumption(&mut assumptions, &mut seen, plan, classify_plan_assumption(plan, assumption), assumption.clone());
        }

        let local_boundary = format!("{} {}", plan.detail, plan.feature).to_ascii_lowercase();
        if plan.feature.starts_with("create-unique-identity:")
            && (local_boundary.contains("global field uniqueness")
                || local_boundary.contains("script-args")
                || local_boundary.contains("script_args")
                || local_boundary.contains("singleton-type")
                || local_boundary.contains("singleton_type")
                || local_boundary.contains("builder/indexer"))
        {
            push_assumption(
                &mut assumptions,
                &mut seen,
                plan,
                "create_unique_global_uniqueness",
                "builder/indexer must prove chain-wide uniqueness for this create_unique identity policy".to_string(),
            );
        }

        if plan.detail.to_ascii_lowercase().contains("type_id uniqueness remains bound") {
            push_assumption(
                &mut assumptions,
                &mut seen,
                plan,
                "type_id_builder_plan",
                "builder must construct the CKB TYPE_ID output from the declared first-input/output-index rule".to_string(),
            );
        }
    }

    if metadata.constraints.ckb.as_ref().is_some_and(|ckb| ckb.capacity_planning_required) {
        let detail = "builder must satisfy occupied-capacity and transaction-size limits for created or mutated outputs";
        let synthetic = ProofPlanMetadata {
            name: "ckb_capacity_planning".to_string(),
            origin: "constraints.ckb".to_string(),
            category: "builder-assumption".to_string(),
            feature: "capacity-planning".to_string(),
            source_span: None,
            trigger: "builder".to_string(),
            scope: "transaction".to_string(),
            reads: vec!["output".to_string()],
            coverage: Vec::new(),
            input_output_relation_checks: Vec::new(),
            group_cardinality: "not a script-group cardinality obligation".to_string(),
            identity_lifecycle_policy: "none".to_string(),
            preserved_fields: Vec::new(),
            witness_fields: Vec::new(),
            lock_args_fields: Vec::new(),
            on_chain_checked: false,
            on_chain_checked_obligations: Vec::new(),
            builder_assumptions: vec![detail.to_string()],
            codegen_coverage_status: "builder-required".to_string(),
            status: "builder-required".to_string(),
            detail: detail.to_string(),
            diagnostics: Vec::new(),
        };
        push_assumption(&mut assumptions, &mut seen, &synthetic, "capacity_policy", detail.to_string());
    }

    assumptions
}

pub fn validate_transaction_against_metadata(metadata: &CompileMetadata, tx: &Value) -> TxValidationReport {
    let assumptions = if metadata.runtime.builder_assumptions.is_empty() {
        builder_assumptions_from_metadata(metadata)
    } else {
        metadata.runtime.builder_assumptions.clone()
    };
    validate_transaction_against_assumptions(&assumptions, tx)
}

pub fn validate_transaction_against_assumptions(assumptions: &[BuilderAssumptionMetadata], tx: &Value) -> TxValidationReport {
    let input_count = json_array_len(tx, "inputs");
    let output_count = json_array_len(tx, "outputs");
    let cell_dep_count = json_array_len(tx, "cell_deps");
    let witness_count = json_array_len(tx, "witnesses");
    let evidence = assumption_evidence_ids(tx);
    let mut violations = Vec::new();
    let mut checked_assumptions = Vec::new();

    for assumption in assumptions {
        checked_assumptions.push(assumption.assumption_id.clone());
        if !assumption.required_inputs.is_empty() && input_count == 0 {
            push_violation(&mut violations, assumption, "transaction has no inputs required by this assumption");
        }
        if !assumption.required_outputs.is_empty() && output_count == 0 {
            push_violation(&mut violations, assumption, "transaction has no outputs required by this assumption");
        }
        if !assumption.required_cell_deps.is_empty() && cell_dep_count == 0 {
            push_violation(&mut violations, assumption, "transaction has no cell_deps required by this assumption");
        }
        if !assumption.required_witness_fields.is_empty() && witness_count == 0 {
            push_violation(&mut violations, assumption, "transaction has no witnesses required by this assumption");
        }
        if requires_explicit_evidence(&assumption.kind) && !evidence.contains(&assumption.assumption_id) {
            push_violation(
                &mut violations,
                assumption,
                "missing builder_assumption_evidence entry for this non-structural assumption",
            );
        }
    }

    let status = if violations.is_empty() { "ok" } else { "failed" };
    TxValidationReport {
        status: status.to_string(),
        assumption_count: assumptions.len(),
        checked_assumptions,
        input_count,
        output_count,
        cell_dep_count,
        witness_count,
        violations,
    }
}

fn push_assumption(
    assumptions: &mut Vec<BuilderAssumptionMetadata>,
    seen: &mut BTreeSet<String>,
    plan: &ProofPlanMetadata,
    kind: &str,
    detail: String,
) {
    let assumption_id = assumption_id(plan, kind, &detail);
    if !seen.insert(assumption_id.clone()) {
        return;
    }
    assumptions.push(BuilderAssumptionMetadata {
        assumption_id,
        kind: kind.to_string(),
        origin: plan.origin.clone(),
        feature: plan.feature.clone(),
        proof_plan_status: plan.status.clone(),
        required_inputs: required_reads(plan, &["input", "group_input"]),
        required_outputs: required_reads(plan, &["output", "group_output"]),
        required_cell_deps: required_reads(plan, &["cell_dep"]),
        required_witness_fields: plan.witness_fields.clone(),
        capacity_policy: if plan.reads.iter().any(|read| read == "output" || read == "group_output") {
            "occupied-capacity-and-tx-size-evidence-required".to_string()
        } else {
            "none".to_string()
        },
        fee_policy: "builder-balances-fee-before-signing".to_string(),
        change_policy: "change-outputs-must-not-violate-proof-plan-shape".to_string(),
        signature_policy: if plan.reads.iter().any(|read| read == "witness" || read == "lock_args") {
            "signature-material-explicit-no-implicit-signer-authority".to_string()
        } else {
            "none".to_string()
        },
        failure_mode: "reject-before-signing".to_string(),
        detail,
    });
}

fn classify_plan_assumption(plan: &ProofPlanMetadata, assumption: &str) -> &'static str {
    let text = format!("{} {} {}", plan.feature, plan.detail, assumption).to_ascii_lowercase();
    if text.contains("lock transaction scan") || text.contains("only protects the lock group") {
        "lock_group_transaction_scope"
    } else if text.contains("runtime-required") {
        "runtime_required_proof_plan"
    } else if text.contains("metadata-only") {
        "metadata_only_gap"
    } else if text.contains("type_id") || text.contains("type-id") {
        "type_id_builder_plan"
    } else if text.contains("global") || text.contains("builder/indexer") {
        "create_unique_global_uniqueness"
    } else {
        "builder_evidence"
    }
}

fn assumption_id(plan: &ProofPlanMetadata, kind: &str, detail: &str) -> String {
    let material = format!("{}|{}|{}|{}|{}", plan.origin, plan.feature, plan.status, kind, detail);
    let hash = hex_encode(&ckb_blake2b256(material.as_bytes()));
    format!("ba-{}", &hash[..16])
}

fn required_reads(plan: &ProofPlanMetadata, reads: &[&str]) -> Vec<String> {
    reads.iter().filter(|read| plan.reads.iter().any(|actual| actual == **read)).map(|read| format!("{}:*", read)).collect()
}

fn json_array_len(tx: &Value, key: &str) -> usize {
    tx.get(key).and_then(Value::as_array).map_or(0, Vec::len)
}

fn assumption_evidence_ids(tx: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for key in ["builder_assumption_evidence", "builder_assumptions"] {
        let Some(value) = tx.get(key) else { continue };
        match value {
            Value::Array(items) => {
                for item in items {
                    match item {
                        Value::String(id) => {
                            out.insert(id.clone());
                        }
                        Value::Object(object) => {
                            if let Some(id) = object.get("assumption_id").and_then(Value::as_str) {
                                out.insert(id.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
            Value::Object(object) => {
                for (id, value) in object {
                    if value.as_bool().unwrap_or(true) {
                        out.insert(id.clone());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn requires_explicit_evidence(kind: &str) -> bool {
    matches!(
        kind,
        "create_unique_global_uniqueness"
            | "type_id_builder_plan"
            | "metadata_only_gap"
            | "runtime_required_proof_plan"
            | "lock_group_transaction_scope"
            | "capacity_policy"
    )
}

fn push_violation(violations: &mut Vec<TxValidationViolation>, assumption: &BuilderAssumptionMetadata, message: &str) {
    violations.push(TxValidationViolation {
        assumption_id: assumption.assumption_id.clone(),
        kind: assumption.kind.clone(),
        message: message.to_string(),
    });
}

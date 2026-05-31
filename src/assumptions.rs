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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedCellDepBinding {
    dep_source: String,
    index: usize,
    name: Option<String>,
    dep_type: Option<String>,
    tx_hash: Option<String>,
    out_index: Option<u32>,
    hash_type: Option<String>,
    data_hash: Option<String>,
    artifact_hash: Option<String>,
    role: Option<String>,
    verifier_id: Option<String>,
    ipc_abi: Option<String>,
    type_id: Option<String>,
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

enum EvidenceValidation {
    Valid,
    Missing,
    Invalid(String),
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
            executable_evidence: Vec::new(),
            diagnostics: Vec::new(),
        };
        push_assumption(&mut assumptions, &mut seen, &synthetic, "capacity_policy", detail.to_string());
    }

    enrich_manifest_bound_spawn_target_assumptions(&mut assumptions, metadata);

    assumptions
}

pub fn validate_transaction_against_metadata(metadata: &CompileMetadata, tx: &Value) -> TxValidationReport {
    let mut assumptions = if metadata.runtime.builder_assumptions.is_empty() {
        builder_assumptions_from_metadata(metadata)
    } else {
        metadata.runtime.builder_assumptions.clone()
    };
    enrich_manifest_bound_spawn_target_assumptions(&mut assumptions, metadata);
    validate_transaction_against_assumptions(&assumptions, tx)
}

pub fn validate_transaction_against_assumptions(assumptions: &[BuilderAssumptionMetadata], tx: &Value) -> TxValidationReport {
    let input_count = json_array_len(tx, "inputs");
    let output_count = json_array_len(tx, "outputs");
    let cell_dep_count = json_array_len(tx, "cell_deps");
    let witness_count = json_array_len(tx, "witnesses");
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
        if !assumption.required_cell_deps.is_empty() {
            if let Some(expected) = expected_spawn_cell_dep_binding(assumption) {
                if cell_dep_count <= expected.index {
                    push_violation(
                        &mut violations,
                        assumption,
                        &format!("transaction is missing required {} for this spawn target", expected.dep_source),
                    );
                } else {
                    validate_spawn_target_cell_dep_in_tx(tx, assumption, &expected, &mut violations);
                }
            } else if cell_dep_count == 0 {
                push_violation(&mut violations, assumption, "transaction has no cell_deps required by this assumption");
            }
        }
        if !assumption.required_witness_fields.is_empty() && witness_count == 0 {
            push_violation(&mut violations, assumption, "transaction has no witnesses required by this assumption");
        }
        if requires_explicit_evidence(&assumption.kind) {
            match validate_assumption_evidence(tx, assumption) {
                EvidenceValidation::Valid => {}
                EvidenceValidation::Missing => {
                    push_violation(
                        &mut violations,
                        assumption,
                        "missing builder_assumption_evidence entry for this non-structural assumption",
                    );
                }
                EvidenceValidation::Invalid(message) => push_violation(&mut violations, assumption, &message),
            }
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
    } else if plan.category == "spawn-target" || plan.feature.starts_with("spawn-target:") || text.contains("spawn target") {
        "spawn_target_cell_dep_binding"
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

fn enrich_manifest_bound_spawn_target_assumptions(assumptions: &mut [BuilderAssumptionMetadata], metadata: &CompileMetadata) {
    let Some(ckb) = metadata.constraints.ckb.as_ref() else {
        return;
    };
    let spawn_references = ckb
        .script_references
        .iter()
        .filter(|reference| {
            reference.purpose == "spawn-target"
                && reference.status == "builder-required-manifest-bound-cell-dep"
                && reference.dep_source.starts_with("CellDep#")
        })
        .collect::<Vec<_>>();
    if spawn_references.is_empty() {
        return;
    }

    for assumption in assumptions.iter_mut().filter(|assumption| assumption.kind == "spawn_target_cell_dep_binding") {
        let reference = spawn_references
            .iter()
            .find(|reference| assumption.detail.contains(&format!("'{}'", reference.name)))
            .copied()
            .or_else(|| (spawn_references.len() == 1).then_some(spawn_references[0]));
        let Some(reference) = reference else {
            continue;
        };
        let Some(index) = parse_cell_dep_source_index(&reference.dep_source) else {
            continue;
        };
        let dep = ckb.dep_group_manifest.declared_cell_deps.get(index);
        let dep_type = dep.map(|dep| dep.dep_type.as_str()).unwrap_or("code");
        let mut required = format!("{}:name={}:dep_type={}", reference.dep_source, reference.name, dep_type);
        if let Some(tx_hash) = dep.and_then(|dep| dep.tx_hash.as_deref()) {
            required.push_str(&format!(":tx_hash={tx_hash}"));
        }
        if let Some(out_index) = dep.and_then(|dep| dep.index) {
            required.push_str(&format!(":out_index={out_index}"));
        }
        if let Some(hash_type) = dep.and_then(|dep| dep.hash_type.as_deref()) {
            required.push_str(&format!(":hash_type={hash_type}"));
        }
        if let Some(data_hash) = dep.and_then(|dep| dep.data_hash.as_deref()) {
            required.push_str(&format!(":data_hash={data_hash}"));
        }
        if let Some(artifact_hash) = dep.and_then(|dep| dep.artifact_hash.as_deref()) {
            required.push_str(&format!(":artifact_hash={artifact_hash}"));
        }
        if let Some(role) = dep.and_then(|dep| dep.role.as_deref()) {
            required.push_str(&format!(":role={role}"));
        }
        if let Some(verifier_id) = dep.and_then(|dep| dep.verifier_id.as_deref()) {
            required.push_str(&format!(":verifier_id={verifier_id}"));
        }
        if let Some(ipc_abi) = dep.and_then(|dep| dep.ipc_abi.as_deref()) {
            required.push_str(&format!(":ipc_abi={ipc_abi}"));
        }
        if let Some(type_id) = dep.and_then(|dep| dep.type_id.as_deref()) {
            required.push_str(&format!(":type_id={type_id}"));
        }
        assumption.required_cell_deps = vec![required];
        if !assumption.detail.contains("builder_assumption_evidence must identify") {
            assumption.detail.push_str(&format!(
                "; builder_assumption_evidence must identify {} name={} dep_type={}",
                reference.dep_source, reference.name, dep_type
            ));
        }
    }
}

fn parse_cell_dep_source_index(dep_source: &str) -> Option<usize> {
    dep_source.strip_prefix("CellDep#")?.parse().ok()
}

fn json_array_len(tx: &Value, key: &str) -> usize {
    tx.get(key).and_then(Value::as_array).map_or(0, Vec::len)
}

fn validate_spawn_target_cell_dep_in_tx(
    tx: &Value,
    assumption: &BuilderAssumptionMetadata,
    expected: &ExpectedCellDepBinding,
    violations: &mut Vec<TxValidationViolation>,
) {
    let Some(cell_dep) = tx.get("cell_deps").and_then(Value::as_array).and_then(|cell_deps| cell_deps.get(expected.index)) else {
        return;
    };
    let Some(object) = cell_dep.as_object() else {
        push_violation(
            violations,
            assumption,
            &format!("transaction {} must be an object carrying the manifest-bound spawn target identity", expected.dep_source),
        );
        return;
    };

    let mut mismatches = Vec::new();
    validate_cell_dep_identity_fields(object, expected, "transaction cell_dep", false, &mut mismatches);
    if !mismatches.is_empty() {
        push_violation(violations, assumption, &mismatches.join("; "));
    }
}

fn validate_assumption_evidence(tx: &Value, assumption: &BuilderAssumptionMetadata) -> EvidenceValidation {
    let mut invalid = None;
    for key in ["builder_assumption_evidence", "builder_assumptions"] {
        let Some(value) = tx.get(key) else { continue };
        match value {
            Value::Array(items) => {
                for item in items {
                    match validate_evidence_item(item, None, assumption) {
                        EvidenceValidation::Valid => return EvidenceValidation::Valid,
                        EvidenceValidation::Missing => {}
                        EvidenceValidation::Invalid(message) => {
                            invalid.get_or_insert(message);
                        }
                    }
                }
            }
            Value::Object(object) => {
                if let Some(value) = object.get(&assumption.assumption_id) {
                    match validate_evidence_item(value, Some(&assumption.assumption_id), assumption) {
                        EvidenceValidation::Valid => return EvidenceValidation::Valid,
                        EvidenceValidation::Missing => {}
                        EvidenceValidation::Invalid(message) => {
                            invalid.get_or_insert(message);
                        }
                    }
                }
                match validate_evidence_item(value, None, assumption) {
                    EvidenceValidation::Valid => return EvidenceValidation::Valid,
                    EvidenceValidation::Missing => {}
                    EvidenceValidation::Invalid(message) => {
                        invalid.get_or_insert(message);
                    }
                }
            }
            _ => {}
        }
    }
    invalid.map(EvidenceValidation::Invalid).unwrap_or(EvidenceValidation::Missing)
}

fn validate_evidence_item(item: &Value, map_key: Option<&str>, assumption: &BuilderAssumptionMetadata) -> EvidenceValidation {
    match item {
        Value::String(id) if id == &assumption.assumption_id => EvidenceValidation::Invalid(
            "builder_assumption_evidence must be an object with assumption_id, kind, origin, feature, proof_plan_status, and evidence payload"
                .to_string(),
        ),
        Value::Object(object) => validate_evidence_object(object, map_key, assumption),
        Value::Bool(true) if map_key == Some(assumption.assumption_id.as_str()) => EvidenceValidation::Invalid(
            "builder_assumption_evidence map values must be evidence objects, not booleans".to_string(),
        ),
        _ => EvidenceValidation::Missing,
    }
}

fn validate_evidence_object(
    object: &serde_json::Map<String, Value>,
    map_key: Option<&str>,
    assumption: &BuilderAssumptionMetadata,
) -> EvidenceValidation {
    let id = object.get("assumption_id").and_then(Value::as_str).or(map_key);
    let Some(id) = id else {
        return EvidenceValidation::Missing;
    };
    if id != assumption.assumption_id {
        return if map_key == Some(assumption.assumption_id.as_str()) {
            EvidenceValidation::Invalid("builder_assumption_evidence object assumption_id does not match its map key".to_string())
        } else {
            EvidenceValidation::Missing
        };
    }

    let mut mismatches = Vec::new();
    push_evidence_mismatch(&mut mismatches, object, "kind", &assumption.kind);
    push_evidence_mismatch(&mut mismatches, object, "origin", &assumption.origin);
    push_evidence_mismatch(&mut mismatches, object, "feature", &assumption.feature);

    let status = object.get("proof_plan_status").or_else(|| object.get("status")).and_then(Value::as_str).unwrap_or("");
    if status != assumption.proof_plan_status {
        mismatches
            .push(format!("proof_plan_status must be '{}' for assumption {}", assumption.proof_plan_status, assumption.assumption_id));
    }

    let payload = object.get("evidence").or_else(|| object.get("payload"));
    let has_payload = payload.is_some_and(non_empty_evidence_payload);
    if !has_payload {
        mismatches
            .push(format!("builder_assumption_evidence for {} must include non-empty evidence or payload", assumption.assumption_id));
    }
    if let Some(payload) = payload {
        validate_spawn_target_evidence_payload(payload, assumption, &mut mismatches);
    }

    if mismatches.is_empty() {
        EvidenceValidation::Valid
    } else {
        EvidenceValidation::Invalid(mismatches.join("; "))
    }
}

fn push_evidence_mismatch(mismatches: &mut Vec<String>, object: &serde_json::Map<String, Value>, field: &str, expected: &str) {
    match object.get(field).and_then(Value::as_str) {
        Some(actual) if actual == expected => {}
        _ => mismatches.push(format!("{field} must be '{expected}'")),
    }
}

fn non_empty_evidence_payload(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(_) | Value::Number(_) => true,
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(object) => !object.is_empty(),
    }
}

fn validate_spawn_target_evidence_payload(payload: &Value, assumption: &BuilderAssumptionMetadata, mismatches: &mut Vec<String>) {
    let Some(expected) = expected_spawn_cell_dep_binding(assumption) else {
        return;
    };
    let Some(object) = spawn_target_payload_object(payload) else {
        mismatches.push("spawn_target_cell_dep_binding evidence must be an object identifying the required CellDep".to_string());
        return;
    };

    validate_cell_dep_identity_fields(object, &expected, "spawn_target_cell_dep_binding evidence", true, mismatches);
}

fn spawn_target_payload_object(payload: &Value) -> Option<&serde_json::Map<String, Value>> {
    let object = payload.as_object()?;
    object.get("cell_dep").and_then(Value::as_object).or(Some(object))
}

fn evidence_dep_source_matches(object: &serde_json::Map<String, Value>, dep_source: &str, index: usize) -> bool {
    let mut saw_locator = false;
    let mut matches = true;
    if let Some(actual) = object.get("dep_source").and_then(Value::as_str) {
        saw_locator = true;
        matches &= actual == dep_source;
    }
    match object.get("cell_dep_index") {
        Some(Value::Number(number)) => {
            saw_locator = true;
            matches &= number.as_u64() == Some(index as u64);
        }
        Some(Value::String(text)) => {
            saw_locator = true;
            matches &= text.parse::<usize>().ok() == Some(index);
        }
        _ => {}
    }
    saw_locator && matches
}

fn first_string_field<'a>(object: &'a serde_json::Map<String, Value>, fields: &[&str]) -> Option<&'a str> {
    fields.iter().find_map(|field| object.get(*field).and_then(Value::as_str))
}

fn first_u64_field(object: &serde_json::Map<String, Value>, fields: &[&str]) -> Option<u64> {
    fields.iter().find_map(|field| match object.get(*field) {
        Some(Value::Number(number)) => number.as_u64(),
        Some(Value::String(text)) => text.parse::<u64>().ok(),
        _ => None,
    })
}

fn validate_cell_dep_identity_fields(
    object: &serde_json::Map<String, Value>,
    expected: &ExpectedCellDepBinding,
    context: &str,
    require_dep_source: bool,
    mismatches: &mut Vec<String>,
) {
    if require_dep_source && !evidence_dep_source_matches(object, &expected.dep_source, expected.index) {
        mismatches.push(format!("{context} must identify {} with cell_dep_index {}", expected.dep_source, expected.index));
    }
    if let Some(name) = expected.name.as_deref() {
        match first_string_field(object, &["cell_dep_name", "name"]) {
            Some(actual) if actual == name => {}
            _ => mismatches.push(format!("{context} cell_dep_name must be '{name}'")),
        }
    }
    if let Some(dep_type) = expected.dep_type.as_deref() {
        match object.get("dep_type").and_then(Value::as_str) {
            Some(actual) if actual == dep_type => {}
            _ => mismatches.push(format!("{context} dep_type must be '{dep_type}'")),
        }
    }
    if let Some(tx_hash) = expected.tx_hash.as_deref() {
        match object.get("tx_hash").and_then(Value::as_str) {
            Some(actual) if actual == tx_hash => {}
            _ => mismatches.push(format!("{context} tx_hash must be '{tx_hash}'")),
        }
    }
    if let Some(out_index) = expected.out_index {
        match first_u64_field(object, &["out_index", "out_point_index", "index"]) {
            Some(actual) if actual == u64::from(out_index) => {}
            _ => mismatches.push(format!("{context} out_index must be '{out_index}'")),
        }
    }
    if let Some(hash_type) = expected.hash_type.as_deref() {
        match object.get("hash_type").and_then(Value::as_str) {
            Some(actual) if actual == hash_type => {}
            _ => mismatches.push(format!("{context} hash_type must be '{hash_type}'")),
        }
    }
    if let Some(data_hash) = expected.data_hash.as_deref() {
        match object.get("data_hash").and_then(Value::as_str) {
            Some(actual) if actual == data_hash => {}
            _ => mismatches.push(format!("{context} data_hash must be '{data_hash}'")),
        }
    }
    if let Some(artifact_hash) = expected.artifact_hash.as_deref() {
        match object.get("artifact_hash").and_then(Value::as_str) {
            Some(actual) if actual == artifact_hash => {}
            _ => mismatches.push(format!("{context} artifact_hash must be '{artifact_hash}'")),
        }
    }
    if let Some(role) = expected.role.as_deref() {
        match object.get("role").and_then(Value::as_str) {
            Some(actual) if actual == role => {}
            _ => mismatches.push(format!("{context} role must be '{role}'")),
        }
    }
    if let Some(verifier_id) = expected.verifier_id.as_deref() {
        match object.get("verifier_id").and_then(Value::as_str) {
            Some(actual) if actual == verifier_id => {}
            _ => mismatches.push(format!("{context} verifier_id must be '{verifier_id}'")),
        }
    }
    if let Some(ipc_abi) = expected.ipc_abi.as_deref() {
        match object.get("ipc_abi").and_then(Value::as_str) {
            Some(actual) if actual == ipc_abi => {}
            _ => mismatches.push(format!("{context} ipc_abi must be '{ipc_abi}'")),
        }
    }
    if let Some(type_id) = expected.type_id.as_deref() {
        match object.get("type_id").and_then(Value::as_str) {
            Some(actual) if actual == type_id => {}
            _ => mismatches.push(format!("{context} type_id must be '{type_id}'")),
        }
    }
}

fn expected_spawn_cell_dep_binding(assumption: &BuilderAssumptionMetadata) -> Option<ExpectedCellDepBinding> {
    if assumption.kind != "spawn_target_cell_dep_binding" {
        return None;
    }
    assumption.required_cell_deps.iter().find_map(|required| parse_expected_cell_dep_binding(required))
}

fn parse_expected_cell_dep_binding(required: &str) -> Option<ExpectedCellDepBinding> {
    let mut parts = required.split(':');
    let dep_source = parts.next()?.to_string();
    let index = parse_cell_dep_source_index(&dep_source)?;
    let mut name = None;
    let mut dep_type = None;
    let mut tx_hash = None;
    let mut out_index = None;
    let mut hash_type = None;
    let mut data_hash = None;
    let mut artifact_hash = None;
    let mut role = None;
    let mut verifier_id = None;
    let mut ipc_abi = None;
    let mut type_id = None;
    for part in parts {
        if let Some(value) = part.strip_prefix("name=") {
            name = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("dep_type=") {
            dep_type = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("tx_hash=") {
            tx_hash = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("out_index=") {
            out_index = value.parse::<u32>().ok();
        } else if let Some(value) = part.strip_prefix("hash_type=") {
            hash_type = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("data_hash=") {
            data_hash = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("artifact_hash=") {
            artifact_hash = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("role=") {
            role = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("verifier_id=") {
            verifier_id = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("ipc_abi=") {
            ipc_abi = Some(value.to_string());
        } else if let Some(value) = part.strip_prefix("type_id=") {
            type_id = Some(value.to_string());
        }
    }
    Some(ExpectedCellDepBinding {
        dep_source,
        index,
        name,
        dep_type,
        tx_hash,
        out_index,
        hash_type,
        data_hash,
        artifact_hash,
        role,
        verifier_id,
        ipc_abi,
        type_id,
    })
}

fn requires_explicit_evidence(kind: &str) -> bool {
    matches!(
        kind,
        "create_unique_global_uniqueness"
            | "type_id_builder_plan"
            | "metadata_only_gap"
            | "runtime_required_proof_plan"
            | "spawn_target_cell_dep_binding"
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

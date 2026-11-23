use super::hash_json_value;
use crate::error::Result;
use crate::CompileMetadata;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn action_scan_selectors_json(action: &crate::ActionMetadata) -> serde_json::Value {
    let runtime_required_selector_count =
        action.transaction_runtime_input_requirements.iter().filter(|requirement| requirement.status == "runtime-required").count();
    let checked_runtime_selector_count =
        action.transaction_runtime_input_requirements.iter().filter(|requirement| requirement.status == "checked-runtime").count();
    let ckb_sources = action
        .transaction_runtime_input_requirements
        .iter()
        .map(|requirement| requirement.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let selectors = action
        .transaction_runtime_input_requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| action_scan_selector_json(action, index, requirement))
        .collect::<Vec<_>>();

    serde_json::json!({
        "schema": "cellscript-action-scan-selectors-v0.21",
        "status": action_scan_selector_status(selectors.len(), runtime_required_selector_count),
        "evidence_level": "compile-only",
        "source": "transaction_runtime_input_requirements",
        "action": action.name,
        "selector_count": selectors.len(),
        "checked_runtime_selector_count": checked_runtime_selector_count,
        "runtime_required_selector_count": runtime_required_selector_count,
        "ckb_sources": ckb_sources,
        "selectors": selectors,
        "non_claims": [
            "does-not-query-live-cells",
            "does-not-bind-outpoints",
            "does-not-prove-ckb-vm-execution",
            "does-not-prove-tx-pool-acceptance"
        ],
    })
}

fn action_scan_selector_status(selector_count: usize, runtime_required_selector_count: usize) -> &'static str {
    if selector_count == 0 {
        "no-action-scans-required"
    } else if runtime_required_selector_count == 0 {
        "compile-checked-runtime-selectors"
    } else {
        "requires-runtime-resolution"
    }
}

fn action_scan_selector_json(
    action: &crate::ActionMetadata,
    index: usize,
    requirement: &crate::TransactionRuntimeInputRequirementMetadata,
) -> serde_json::Value {
    serde_json::json!({
        "selector_index": index,
        "requirement_index": index,
        "action": action.name,
        "scope": requirement.scope,
        "feature": requirement.feature,
        "component": requirement.component,
        "requirement_status": requirement.status,
        "scan_status": action_scan_requirement_status(requirement),
        "ckb_source": requirement.source,
        "role": action_scan_source_role(&requirement.source),
        "binding": requirement.binding,
        "field": requirement.field,
        "abi": requirement.abi,
        "byte_len": requirement.byte_len,
        "script_field": action_scan_script_field(requirement),
        "script_args": serde_json::Value::Null,
        "args_pattern": action_scan_args_pattern(requirement),
        "selector": {
            "kind": action_scan_kind(&requirement.source),
            "source": requirement.source,
            "binding": requirement.binding,
            "field": requirement.field,
            "component": requirement.component,
            "abi": requirement.abi,
        },
        "resolution": {
            "status": action_scan_requirement_status(requirement),
            "blocker": requirement.blocker,
            "blocker_class": requirement.blocker_class,
            "adapter_action": action_scan_adapter_action(requirement),
        },
    })
}

fn action_scan_requirement_status(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> &'static str {
    if requirement.status == "runtime-required" {
        "requires-runtime-resolution"
    } else {
        "verifier-covered"
    }
}

fn action_scan_source_role(source: &str) -> &'static str {
    match source {
        "Input" => "transaction-input-live-cell",
        "Output" => "transaction-output",
        "InputOutput" => "paired-input-output",
        "CellDep" => "cell-dep",
        "HeaderDep" => "header-dep",
        "Transaction" => "whole-transaction",
        _ => "runtime-transaction-component",
    }
}

fn action_scan_kind(source: &str) -> &'static str {
    match source {
        "Input" => "input-cell-selector",
        "Output" => "output-cell-selector",
        "InputOutput" => "input-output-pair-selector",
        "CellDep" => "cell-dep-selector",
        "HeaderDep" => "header-dep-selector",
        "Transaction" => "transaction-selector",
        _ => "transaction-component-selector",
    }
}

fn action_scan_script_field(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> Option<&'static str> {
    match requirement.field.as_deref() {
        Some("lock_hash") => Some("lock"),
        Some("identity" | "ckb_type_script_hash-absence") => Some("type"),
        _ => None,
    }
}

fn action_scan_args_pattern(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> &'static str {
    if action_scan_script_field(requirement).is_some() {
        "runtime-supplied-from-resolved-script"
    } else {
        "not-applicable"
    }
}

fn action_scan_adapter_action(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> &'static str {
    if requirement.status == "runtime-required" {
        "resolve-or-reject-before-signing"
    } else {
        "materialize-and-preserve-verifier-covered-shape"
    }
}

pub(super) fn cellfabric_intent_envelope_json(
    metadata: &CompileMetadata,
    action: &crate::ActionMetadata,
    action_plan: &serde_json::Value,
    input_path: &Path,
    metadata_hash: &str,
) -> Result<serde_json::Value> {
    let action_plan_hash = hash_json_value("CellScript action plan", action_plan)?;
    let app_namespace = metadata.module.clone();
    let app_conflict_key_templates = cellfabric_app_conflict_key_templates(&app_namespace, action);
    Ok(serde_json::json!({
        "schema": "cellscript-cellfabric-intent-envelope-v0.20",
        "status": "requires-runtime-binding",
        "bridge_boundary": {
            "kind": "json-bridge",
            "cellscript_core_dependency": "no-cell-fabric-rust-crate",
            "cellfabric_expected_role": "intent-ordering-soft-confirmation-and-settlement-tracking",
            "not_a_cellfabric_signed_intent": true,
            "not_a_soft_confirmation": true,
            "not_l1_finality": true,
            "compiler_must_not_infer_cellfabric_finality": true,
        },
        "source": {
            "input": input_path.display().to_string(),
            "module": metadata.module.clone(),
            "action": action.name.clone(),
            "target_profile": metadata.target_profile.name.clone(),
            "compiler_version": metadata.compiler_version.clone(),
            "metadata_hash": metadata_hash,
            "artifact_hash": &metadata.artifact_hash,
            "action_plan_hash": action_plan_hash.clone(),
        },
        "cellfabric_mapping": {
            "target": "CellFabric IntentBody template",
            "candidate_intent_action": "App",
            "payload_format": "cellscript-action-plan-json-v1",
            "payload_hash_field": "cellscript_action_plan_hash",
            "resource_binding": "runtime-resolved-live-cells",
            "auth_binding": "runtime-wallet-or-live-cell-context",
            "settlement_compiler": "cellscript-ckb-adapter-or-generated-builder",
        },
        "cellfabric_intent_template": {
            "version": 1,
            "domain": {
                "chain_id": metadata.target_profile.name.clone(),
                "app_namespace": app_namespace.clone(),
            },
            "author": {
                "lock_script_hash": serde_json::Value::Null,
                "source": "runtime-wallet-or-live-cell-context",
            },
            "nonce": serde_json::Value::Null,
            "validity": {
                "valid_after_ms": serde_json::Value::Null,
                "valid_until_ms": serde_json::Value::Null,
            },
            "resources": {
                "consumes": [],
                "reads": [],
                "app_keys": app_conflict_key_templates,
                "status": "template-only-runtime-outpoints-required",
            },
            "action": {
                "kind": "App",
                "action": action.name.clone(),
                "payload_format": "cellscript-action-plan-json-v1",
                "payload_hash": action_plan_hash.clone(),
            },
            "constraints": {
                "source": "cellscript-action-plan",
                "runtime_input_requirements": &action.transaction_runtime_input_requirements,
                "verifier_obligations": &action.verifier_obligations,
                "fail_closed_runtime_features": &action.fail_closed_runtime_features,
            },
            "dependencies": {
                "requires": [],
                "source": "service-supplied-cellfabric-intent-ids",
            },
            "replacement": {
                "supersedes": [],
                "rule": "service-policy",
            },
            "fee": {
                "fee_bid_shannons": serde_json::Value::Null,
                "max_fee_shannons": serde_json::Value::Null,
                "source": "runtime-builder-policy",
            },
            "auth_mode": "CoSignConcreteTx",
            "metadata": {
                "cellscript_action": action.name.clone(),
                "cellscript_metadata_hash": metadata_hash,
                "cellscript_action_plan_hash": action_plan_hash.clone(),
                "cellscript_artifact_hash": &metadata.artifact_hash,
            },
        },
        "resource_access_template": {
            "hard_conflicts": {
                "status": "runtime-required",
                "consumed_cell_patterns": &action.consume_set,
                "runtime_input_requirements": &action.transaction_runtime_input_requirements,
                "note": "CellFabric OutPointRef conflicts must be filled from resolved live cells before submitting a SignedIntent.",
            },
            "reads": &action.read_refs,
            "writes": {
                "creates": &action.create_set,
                "mutates": &action.mutate_set,
            },
            "app_conflict_key_templates": cellfabric_app_conflict_key_templates(&app_namespace, action),
        },
        "required_runtime_evidence": [
            "author_lock_script_hash",
            "intent_nonce",
            "resolved_consumed_outpoints",
            "resolved_read_outpoints",
            "cellfabric_auth_signature",
            "deployment_identity",
            "live_cell_resolution",
            "capacity_fee_balance",
            "estimate_cycles",
            "tx_pool_acceptance",
            "l1_status_observation"
        ],
        "non_claims": [
            "does not create a CellFabric SignedIntent",
            "does not prove CellFabric orderer acceptance",
            "does not soft-confirm the action",
            "does not prove live-cell availability",
            "does not prove CKB tx-pool acceptance",
            "does not prove L1 finality"
        ],
        "action_plan": action_plan,
    }))
}

pub(super) fn cellfabric_app_conflict_key_templates(app_namespace: &str, action: &crate::ActionMetadata) -> Vec<serde_json::Value> {
    let mut keys = BTreeSet::<(String, String)>::new();
    for shared in &action.touches_shared {
        keys.insert(("cellscript-shared-resource".to_string(), shared.clone()));
    }
    for pattern in &action.mutate_set {
        keys.insert(("cellscript-mutate-binding".to_string(), format!("{}:{}", pattern.ty, pattern.binding)));
    }
    for primitive in &action.pool_primitives {
        if let Ok(value) = serde_json::to_value(primitive) {
            keys.insert(("cellscript-pool-primitive".to_string(), value.to_string()));
        }
    }
    keys.into_iter()
        .map(|(key_type, key)| {
            serde_json::json!({
                "namespace": app_namespace,
                "key_type": key_type,
                "key": key,
                "key_encoding": "utf8",
                "key_bytes_hex": crate::hex_encode(key.as_bytes()),
            })
        })
        .collect()
}

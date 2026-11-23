use crate::CompileMetadata;
use std::collections::BTreeSet;

pub(super) fn transaction_solver_template(metadata: &CompileMetadata) -> serde_json::Value {
    let assumptions = &metadata.runtime.builder_assumptions;
    let ckb = metadata.constraints.ckb.as_ref();

    // Cell selection: derive input requirements from actions and ProofPlan
    let mut input_slots = Vec::new();
    let mut output_slots = Vec::new();
    let mut dep_slots = Vec::new();
    let mut witness_slots = Vec::new();

    // Build input slots from consume/consume_set patterns in actions
    for action in &metadata.actions {
        for plan in &action.proof_plan {
            if plan.reads.iter().any(|r| r == "input" || r == "group_input") {
                input_slots.push(serde_json::json!({
                    "source": "proof-plan-input",
                    "scope_kind": "action",
                    "scope_name": action.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "input" || **r == "group_input").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Build output slots from create/create_set patterns
    for action in &metadata.actions {
        for plan in &action.proof_plan {
            if plan.reads.iter().any(|r| r == "output" || r == "group_output") {
                output_slots.push(serde_json::json!({
                    "source": "proof-plan-output",
                    "scope_kind": "action",
                    "scope_name": action.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "output" || **r == "group_output").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Build lock input/output slots
    for lock in &metadata.locks {
        for plan in &lock.proof_plan {
            if plan.reads.iter().any(|r| r == "input" || r == "group_input") {
                input_slots.push(serde_json::json!({
                    "source": "proof-plan-input",
                    "scope_kind": "lock",
                    "scope_name": lock.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "input" || **r == "group_input").cloned().collect::<Vec<_>>(),
                }));
            }
            if plan.reads.iter().any(|r| r == "output" || r == "group_output") {
                output_slots.push(serde_json::json!({
                    "source": "proof-plan-output",
                    "scope_kind": "lock",
                    "scope_name": lock.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "output" || **r == "group_output").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Dep resolution from CKB constraints
    if let Some(ckb_constraints) = ckb {
        for dep in &ckb_constraints.dep_group_manifest.declared_cell_deps {
            dep_slots.push(serde_json::json!({
                "source": "metadata-script-reference",
                "name": dep.name,
                "dep_type": dep.dep_type,
                "hash_type": dep.hash_type,
            }));
        }
        for script_ref in &ckb_constraints.script_references {
            dep_slots.push(serde_json::json!({
                "source": "metadata-script-reference",
                "name": script_ref.name,
                "scope": script_ref.scope,
                "purpose": script_ref.purpose,
            }));
        }
    }

    // Witness placement from builder assumptions
    let witness_fields = assumptions
        .iter()
        .flat_map(|assumption| assumption.required_witness_fields.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !witness_fields.is_empty() {
        witness_slots.push(serde_json::json!({
            "source": "builder-assumption-witness-fields",
            "fields": witness_fields,
        }));
    }

    // Evidence requirements
    let evidence = assumptions
        .iter()
        .filter(|assumption| {
            matches!(
                assumption.kind.as_str(),
                "create_unique_global_uniqueness"
                    | "type_id_builder_plan"
                    | "metadata_only_gap"
                    | "runtime_required_proof_plan"
                    | "lock_group_transaction_scope"
                    | "capacity_policy"
            )
        })
        .map(|assumption| {
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "proof_plan_status": assumption.proof_plan_status,
                "detail": assumption.detail,
                "evidence_schema": evidence_schema_for_assumption(assumption),
            })
        })
        .collect::<Vec<_>>();

    // Fee/change planning from CKB constraints
    let fee_planning = ckb
        .map(|c| {
            serde_json::json!({
                "capacity_planning_required": c.capacity_planning_required,
                "capacity_policy": c.capacity_policy_surface,
                "created_output_count": c.created_output_count,
                "mutated_output_count": c.mutated_output_count,
                "occupied_capacity_evidence": c.capacity_evidence_contract.measured_occupied_capacity_shannons,
                "tx_size_bytes": c.tx_size_bytes,
            })
        })
        .unwrap_or(serde_json::json!(null));

    // Deterministic signing manifest
    let signature_requests = metadata
        .locks
        .iter()
        .map(|lock| {
            serde_json::json!({
                "lock_name": lock.name,
                "witness_index": format!("lock:{}:witness_0", lock.name),
                "signature_policy": "explicit-witness-no-implicit-signer",
            })
        })
        .collect::<Vec<_>>();

    let header_dep_slots = ckb
        .map(|c| {
            if c.uses_header_epoch {
                vec![serde_json::json!({
                    "source": "metadata-requirement",
                    "kind": "header_dep",
                    "status": "unresolved",
                    "required_external_step": "resolve concrete epoch/header dep before transaction construction",
                })]
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();

    serde_json::json!({
        "status": "template-only",
        "solver": "cellscript-v0.16-transaction-template-emitter",
        "solver_capability": "template-emitter-only",
        "solver_readiness": "not-a-solver",
        "execution_mode": "non-executable-template",
        "can_submit": false,
        "requires_validate_tx": true,
        "module": metadata.module,
        "target_profile": metadata.target_profile.name,
        "transaction_plan": {
            "version": 0,
            "inputs": input_slots,
            "outputs": output_slots,
            "cell_deps": dep_slots,
            "witnesses": witness_slots,
            "header_deps": header_dep_slots,
            "header_deps_status": "unresolved-template-slots",
            "builder_assumption_evidence_requirements": evidence,
        },
        "fee_change_plan": fee_planning,
        "signing_manifest": {
            "policy": "explicit-witness-no-implicit-signer",
            "signature_requests": signature_requests,
        },
        "builder_assumptions": assumptions,
        "required_external_steps": [
            "live cell selection",
            "concrete CellDep and HeaderDep resolution",
            "fee and change calculation",
            "occupied-capacity and under-capacity measurement",
            "witness placement and signing",
            "CKB dry-run or VM verification"
        ],
        "limitations": [
            "template only: does not perform live cell selection",
            "template only: does not resolve concrete deps/header deps",
            "template only: does not calculate fee/change or occupied capacity",
            "template only: does not place final witnesses or signatures",
            "CKB dry-run required for production acceptance"
        ],
    })
}

fn evidence_schema_for_assumption(assumption: &crate::BuilderAssumptionMetadata) -> serde_json::Value {
    let mut payload_arrays = Vec::new();
    if !assumption.required_inputs.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "inputs",
            "aliases": ["input_cells", "required_inputs"],
            "transaction_array": "inputs",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["source", "out_point", "type_hash", "lock_hash", "capacity"],
        }));
    }
    if !assumption.required_outputs.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "outputs",
            "aliases": ["output_cells", "required_outputs"],
            "transaction_array": "outputs",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["source", "type_hash", "lock_hash", "capacity", "data"],
        }));
    }
    if !assumption.required_cell_deps.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "cell_deps",
            "aliases": ["required_cell_deps"],
            "transaction_array": "cell_deps",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["name", "out_point", "code_hash", "tx_hash", "dep_type"],
        }));
    }
    if !assumption.required_witness_fields.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "witnesses",
            "aliases": ["witness_fields", "required_witness_fields"],
            "transaction_array": "witnesses",
            "item_required_fields": ["index", "field"],
            "item_concrete_fields": ["field", "lock", "input_type", "output_type", "bytes"],
        }));
    }

    let mut payload_objects = Vec::new();
    if assumption.kind == "capacity_policy" || assumption.capacity_policy != "none" {
        payload_objects.push(serde_json::json!({
            "name": "capacity",
            "required_fields": ["occupied_capacity_shannons", "tx_size_bytes", "under_capacity_output_indexes"],
            "failure_rule": "under_capacity_output_indexes must be an empty array for tx validate success",
        }));
    }
    if assumption.kind == "type_id_builder_plan" {
        payload_objects.push(serde_json::json!({
            "name": "type_id",
            "required_fields": ["first_input_out_point", "output_index", "expected_type_id_args"],
            "expected_type_id_args": "canonical 0x-prefixed 32-byte hex",
            "transaction_cross_check": "output_index must point to the output whose type args equal expected_type_id_args when the tx JSON exposes type args",
        }));
    }
    if assumption.kind == "create_unique_global_uniqueness" {
        payload_objects.push(serde_json::json!({
            "name": "uniqueness",
            "required_any_of": ["uniqueness_checked=true", "uniqueness_proof", "unique_cell"],
        }));
    }
    if assumption.kind == "lock_group_transaction_scope" {
        payload_objects.push(serde_json::json!({
            "name": "lock_group_transaction_scope",
            "required_any_of": ["transaction_scope_reviewed=true", "covered_lock_groups"],
        }));
    }
    if matches!(assumption.kind.as_str(), "metadata_only_gap" | "runtime_required_proof_plan") {
        payload_objects.push(serde_json::json!({
            "name": "manual_review",
            "required_any_of": ["manual_review", "checked=true"],
        }));
    }

    serde_json::json!({
        "required_fields": ["assumption_id", "kind", "origin", "feature", "proof_plan_status", "evidence"],
        "payload_type": "object",
        "payload_arrays": payload_arrays,
        "payload_objects": payload_objects,
        "cross_checks": [
            "array evidence items must include numeric index fields that bind to the transaction array",
            "when evidence and the indexed transaction object both expose a concrete field, tx validate requires equality",
            "capacity evidence must fail closed when under-capacity outputs are reported",
            "TYPE_ID evidence must use canonical 32-byte args and match output type args when present"
        ],
        "note": "builder must replace this requirement with concrete evidence before tx validate can pass",
    })
}

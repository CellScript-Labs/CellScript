use super::ckb_runtime_accesses::body_ckb_runtime_accesses;
use super::metadata_layout::module_validity_path_coverage;
use super::pool_metadata::body_pool_primitive_metadata;
use super::proof_plan::ProofPlanMetadata;
use super::runtime_availability::body_fail_closed_runtime_features;
use super::runtime_metadata::{
    auto_lowered_aggregate_runtime_accesses_for_action, auto_lowered_aggregate_runtime_features_for_action, body_ckb_runtime_features,
    merge_ckb_runtime_accesses, merge_ckb_runtime_features, schema_pointer_var_ids,
};
use super::verifier_obligations::{action_terminal_flow_obligations, body_verifier_obligations};
use super::{ir, CkbRuntimeAccessMetadata, MetadataTypeLayouts};
use std::collections::HashMap;

pub(super) fn module_proof_plan_metadata(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    flow_states: &HashMap<String, Vec<String>>,
    flow_state_fields: &HashMap<String, String>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
    module_ckb_runtime_accesses: &[CkbRuntimeAccessMetadata],
) -> Vec<ProofPlanMetadata> {
    let mut proof_plan = Vec::new();
    let validity_paths = module_validity_path_coverage(ir);
    for type_def in &ir.external_type_defs {
        proof_plan.extend(crate::proof_plan::build_for_type_validity(
            type_def,
            validity_paths.selected_create_count(&type_def.name),
            validity_paths.checked_create_count(&type_def.name),
            validity_paths.update_count(&type_def.name),
        ));
    }
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &action.body,
                    &param_schema_vars,
                    type_layouts,
                    &action.params,
                    cell_type_kinds,
                    action.return_type.as_ref(),
                    pure_const_returns,
                );
                let mut ckb_runtime_features = auto_lowered_aggregate_runtime_features_for_action(ir, action);
                ckb_runtime_features.extend(merge_ckb_runtime_features(
                    body_ckb_runtime_features(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
                let ckb_runtime_features = ckb_runtime_features.into_iter().collect::<Vec<_>>();
                let mut ckb_runtime_accesses = auto_lowered_aggregate_runtime_accesses_for_action(ir, action);
                ckb_runtime_accesses.extend(merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
                let mut verifier_obligations = body_verifier_obligations(
                    "action",
                    &action.name,
                    &action.body,
                    &action.source_dispositions,
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &action.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    action.return_type.as_ref(),
                    pure_const_returns,
                );
                verifier_obligations.extend(action_terminal_flow_obligations(ir, action));
                let pool_primitives = body_pool_primitive_metadata(
                    "action",
                    &action.name,
                    &action.body,
                    &action.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                );
                proof_plan.extend(crate::proof_plan::build_for_body(
                    "action",
                    &action.name,
                    &action.body,
                    &action.params,
                    &verifier_obligations,
                    &ckb_runtime_accesses,
                    &pool_primitives,
                ));
            }
            ir::IrItem::Function(function) => {
                let param_schema_vars = schema_pointer_var_ids(&function.body, &function.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &function.body,
                    &param_schema_vars,
                    type_layouts,
                    &function.params,
                    cell_type_kinds,
                    function.return_type.as_ref(),
                    pure_const_returns,
                );
                let ckb_runtime_features = merge_ckb_runtime_features(
                    body_ckb_runtime_features(&function.name, &function.body, cell_type_kinds, type_layouts),
                    &function.params,
                );
                let ckb_runtime_accesses = merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&function.name, &function.body, cell_type_kinds, type_layouts),
                    &function.params,
                );
                let verifier_obligations = body_verifier_obligations(
                    "fn",
                    &function.name,
                    &function.body,
                    &[],
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &function.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    function.return_type.as_ref(),
                    pure_const_returns,
                );
                let pool_primitives = body_pool_primitive_metadata(
                    "fn",
                    &function.name,
                    &function.body,
                    &function.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                );
                proof_plan.extend(crate::proof_plan::build_for_body(
                    "fn",
                    &function.name,
                    &function.body,
                    &function.params,
                    &verifier_obligations,
                    &ckb_runtime_accesses,
                    &pool_primitives,
                ));
            }
            ir::IrItem::Lock(lock) => {
                let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &lock.body,
                    &param_schema_vars,
                    type_layouts,
                    &lock.params,
                    cell_type_kinds,
                    None,
                    pure_const_returns,
                );
                let ckb_runtime_features = merge_ckb_runtime_features(
                    body_ckb_runtime_features(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                    &lock.params,
                );
                let ckb_runtime_accesses = merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                    &lock.params,
                );
                let verifier_obligations = body_verifier_obligations(
                    "lock",
                    &lock.name,
                    &lock.body,
                    &[],
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &lock.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    None,
                    pure_const_returns,
                );
                let pool_primitives = body_pool_primitive_metadata(
                    "lock",
                    &lock.name,
                    &lock.body,
                    &lock.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                );
                proof_plan.extend(crate::proof_plan::build_for_body(
                    "lock",
                    &lock.name,
                    &lock.body,
                    &lock.params,
                    &verifier_obligations,
                    &ckb_runtime_accesses,
                    &pool_primitives,
                ));
            }
            ir::IrItem::Invariant(invariant) => {
                proof_plan.extend(crate::proof_plan::build_for_invariant(ir, invariant, module_ckb_runtime_accesses));
            }
            ir::IrItem::TypeDef(type_def) => {
                proof_plan.extend(crate::proof_plan::build_for_type_validity(
                    type_def,
                    validity_paths.selected_create_count(&type_def.name),
                    validity_paths.checked_create_count(&type_def.name),
                    validity_paths.update_count(&type_def.name),
                ));
            }
        }
    }
    proof_plan
}

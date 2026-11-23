use super::verifier_obligations::TransactionResourceObligation;
use super::{ir, runtime_errors, PoolInvariantMetadata, PoolPrimitiveMetadata, PoolRuntimeInputRequirementMetadata};
use std::collections::{BTreeSet, HashMap};

pub(super) fn pool_runtime_required_components(name: &str, operation: &str, checked_protocol_components: &[String]) -> Vec<String> {
    let components = match operation {
        "create" => &[
            "token-pair-symbol-admission",
            "token-pair-identity-admission",
            "positive-reserve-admission",
            "fee-policy",
            "lp-supply-invariant",
        ][..],
        "mutation-invariants" => match name {
            "swap_a_for_b" => &[
                "reserve-conservation",
                "fee-accounting",
                "constant-product-pricing",
                "lp-supply-consistency",
                "pool-specific-admission",
            ][..],
            "add_liquidity" => {
                &["reserve-conservation", "proportional-liquidity-accounting", "lp-supply-consistency", "pool-specific-admission"][..]
            }
            "remove_liquidity" => {
                &["reserve-conservation", "proportional-withdrawal-accounting", "lp-supply-consistency", "pool-specific-admission"][..]
            }
            _ => &["reserve-conservation", "fee-accounting", "lp-supply-consistency", "pool-specific-admission"][..],
        },
        "composition" => &["callee-pool-admission", "launch-pool-atomicity", "pool-id-continuity"][..],
        _ => &[][..],
    };
    components
        .iter()
        .filter(|component| !checked_protocol_components.iter().any(|checked| checked == **component))
        .map(|component| (*component).to_string())
        .collect()
}

pub(super) fn pool_runtime_input_requirements(
    name: &str,
    operation: &str,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    checked_protocol_components: &[String],
) -> Vec<PoolRuntimeInputRequirementMetadata> {
    let mut requirements = match (operation, name) {
        ("create", "seed_pool") => pool_seed_token_pair_identity_runtime_inputs(body),
        ("mutation-invariants", "swap_a_for_b") => pool_swap_a_for_b_runtime_inputs(body),
        ("mutation-invariants", "add_liquidity") => pool_add_liquidity_runtime_inputs(body),
        ("mutation-invariants", "remove_liquidity") => pool_remove_liquidity_runtime_inputs(body),
        ("composition", "launch_token") => pool_launch_token_composition_runtime_inputs(body, params),
        _ => Vec::new(),
    };
    requirements.retain(|requirement| !checked_protocol_components.iter().any(|component| component == &requirement.component));
    requirements
}

fn pool_seed_token_pair_identity_runtime_inputs(body: &ir::IrBody) -> Vec<PoolRuntimeInputRequirementMetadata> {
    ["token_a", "token_b"]
        .into_iter()
        .filter_map(|binding| {
            body.consume_set.iter().enumerate().find(|(_, pattern)| pattern.operation == "consume" && pattern.binding == binding).map(
                |(index, pattern)| PoolRuntimeInputRequirementMetadata {
                    component: "token-pair-identity-admission".to_string(),
                    source: "Input".to_string(),
                    index,
                    binding: pattern.binding.clone(),
                    field: None,
                    abi: "input-type-id-32".to_string(),
                    byte_len: 32,
                    blocker: Some(pool_runtime_protocol_component_blocker("token-pair-identity-admission").to_string()),
                    blocker_class: Some(pool_runtime_protocol_component_blocker_class("token-pair-identity-admission").to_string()),
                },
            )
        })
        .collect()
}

fn pool_swap_a_for_b_runtime_inputs(body: &ir::IrBody) -> Vec<PoolRuntimeInputRequirementMetadata> {
    let mut requirements = Vec::new();
    let Some((input_index, input_pattern)) = consumed_input_pattern(body, "input") else {
        return requirements;
    };
    let Some(pool_pattern) = body.mutate_set.iter().find(|pattern| pattern.binding == "pool" && pattern.ty == "Pool") else {
        return requirements;
    };

    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        input_index,
        &input_pattern.binding,
        "symbol",
        "input-cell-field-bytes-8",
        8,
    ));
    for field in ["token_a_symbol", "token_b_symbol"] {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-bytes-8",
            8,
        ));
    }
    if let Some((token_index, token_pattern)) = created_output_pattern(body, "create_Token", "Token") {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Output",
            token_index,
            &token_pattern.binding,
            "symbol",
            "create-output-field-bytes-8",
            8,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-input-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Output",
        pool_pattern.output_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-output-field-u64",
        8,
    ));

    requirements.push(pool_runtime_field_requirement(
        "reserve-conservation",
        "Input",
        input_index,
        &input_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    for field in ["reserve_a", "reserve_b"] {
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }
    if let Some((token_index, token_pattern)) = created_output_pattern(body, "create_Token", "Token") {
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Output",
            token_index,
            &token_pattern.binding,
            "amount",
            "create-output-field-u64",
            8,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "fee-accounting",
        "Input",
        input_index,
        &input_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "fee-accounting",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "fee_rate_bps",
        "mutate-input-field-u16",
        2,
    ));

    for field in ["reserve_a", "reserve_b"] {
        requirements.push(pool_runtime_field_requirement(
            "constant-product-pricing",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "constant-product-pricing",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }
    requirements.push(pool_runtime_field_requirement(
        "constant-product-pricing",
        "Input",
        input_index,
        &input_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));

    requirements
}

fn pool_add_liquidity_runtime_inputs(body: &ir::IrBody) -> Vec<PoolRuntimeInputRequirementMetadata> {
    let mut requirements = Vec::new();
    let Some((token_a_index, token_a_pattern)) = consumed_input_pattern(body, "token_a") else {
        return requirements;
    };
    let Some((token_b_index, token_b_pattern)) = consumed_input_pattern(body, "token_b") else {
        return requirements;
    };
    let Some(pool_pattern) = body.mutate_set.iter().find(|pattern| pattern.binding == "pool" && pattern.ty == "Pool") else {
        return requirements;
    };

    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        token_a_index,
        &token_a_pattern.binding,
        "symbol",
        "input-cell-field-bytes-8",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        token_b_index,
        &token_b_pattern.binding,
        "symbol",
        "input-cell-field-bytes-8",
        8,
    ));
    for field in ["token_a_symbol", "token_b_symbol"] {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-bytes-8",
            8,
        ));
    }
    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "type_hash",
        "mutate-input-type-id-32",
        32,
    ));
    if let Some((receipt_index, receipt_pattern)) = created_output_pattern(body, "create_LPReceipt", "LPReceipt") {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Output",
            receipt_index,
            &receipt_pattern.binding,
            "pool_id",
            "create-output-field-hash-32",
            32,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "reserve-conservation",
        "Input",
        token_a_index,
        &token_a_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "reserve-conservation",
        "Input",
        token_b_index,
        &token_b_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    for field in ["reserve_a", "reserve_b"] {
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "proportional-liquidity-accounting",
        "Input",
        token_a_index,
        &token_a_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "proportional-liquidity-accounting",
        "Input",
        token_b_index,
        &token_b_pattern.binding,
        "amount",
        "input-cell-field-u64",
        8,
    ));
    for field in ["reserve_a", "reserve_b", "total_lp"] {
        requirements.push(pool_runtime_field_requirement(
            "proportional-liquidity-accounting",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "proportional-liquidity-accounting",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }

    if let Some((receipt_index, receipt_pattern)) = created_output_pattern(body, "create_LPReceipt", "LPReceipt") {
        requirements.push(pool_runtime_field_requirement(
            "proportional-liquidity-accounting",
            "Output",
            receipt_index,
            &receipt_pattern.binding,
            "lp_amount",
            "create-output-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "lp-supply-consistency",
            "Output",
            receipt_index,
            &receipt_pattern.binding,
            "lp_amount",
            "create-output-field-u64",
            8,
        ));
    }
    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-input-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Output",
        pool_pattern.output_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-output-field-u64",
        8,
    ));

    requirements
}

fn pool_remove_liquidity_runtime_inputs(body: &ir::IrBody) -> Vec<PoolRuntimeInputRequirementMetadata> {
    let mut requirements = Vec::new();
    let Some((receipt_index, receipt_pattern)) = consumed_input_pattern(body, "receipt") else {
        return requirements;
    };
    let Some(pool_pattern) = body.mutate_set.iter().find(|pattern| pattern.binding == "pool" && pattern.ty == "Pool") else {
        return requirements;
    };

    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        receipt_index,
        &receipt_pattern.binding,
        "pool_id",
        "input-cell-field-hash-32",
        32,
    ));
    requirements.push(pool_runtime_field_requirement(
        "pool-specific-admission",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "type_hash",
        "mutate-input-type-id-32",
        32,
    ));
    for field in ["token_a_symbol", "token_b_symbol"] {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-bytes-8",
            8,
        ));
    }
    for (token_index, token_pattern) in
        body.create_set.iter().enumerate().filter(|(_, pattern)| pattern.operation == "create" && pattern.ty == "Token").take(2)
    {
        requirements.push(pool_runtime_field_requirement(
            "pool-specific-admission",
            "Output",
            token_index,
            &token_pattern.binding,
            "symbol",
            "create-output-field-bytes-8",
            8,
        ));
    }

    for field in ["reserve_a", "reserve_b"] {
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }
    for (token_index, token_pattern) in
        body.create_set.iter().enumerate().filter(|(_, pattern)| pattern.operation == "create" && pattern.ty == "Token").take(2)
    {
        requirements.push(pool_runtime_field_requirement(
            "reserve-conservation",
            "Output",
            token_index,
            &token_pattern.binding,
            "amount",
            "create-output-field-u64",
            8,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "proportional-withdrawal-accounting",
        "Input",
        receipt_index,
        &receipt_pattern.binding,
        "lp_amount",
        "input-cell-field-u64",
        8,
    ));
    for field in ["reserve_a", "reserve_b", "total_lp"] {
        requirements.push(pool_runtime_field_requirement(
            "proportional-withdrawal-accounting",
            "Input",
            pool_pattern.input_index,
            &pool_pattern.binding,
            field,
            "mutate-input-field-u64",
            8,
        ));
    }
    for field in ["reserve_a", "reserve_b"] {
        requirements.push(pool_runtime_field_requirement(
            "proportional-withdrawal-accounting",
            "Output",
            pool_pattern.output_index,
            &pool_pattern.binding,
            field,
            "mutate-output-field-u64",
            8,
        ));
    }
    for (token_index, token_pattern) in
        body.create_set.iter().enumerate().filter(|(_, pattern)| pattern.operation == "create" && pattern.ty == "Token").take(2)
    {
        requirements.push(pool_runtime_field_requirement(
            "proportional-withdrawal-accounting",
            "Output",
            token_index,
            &token_pattern.binding,
            "amount",
            "create-output-field-u64",
            8,
        ));
    }

    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Input",
        receipt_index,
        &receipt_pattern.binding,
        "lp_amount",
        "input-cell-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Input",
        pool_pattern.input_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-input-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "lp-supply-consistency",
        "Output",
        pool_pattern.output_index,
        &pool_pattern.binding,
        "total_lp",
        "mutate-output-field-u64",
        8,
    ));

    requirements
}

fn pool_launch_token_composition_runtime_inputs(
    body: &ir::IrBody,
    params: &[ir::IrParam],
) -> Vec<PoolRuntimeInputRequirementMetadata> {
    let mut requirements = Vec::new();
    let Some((auth_index, auth_pattern)) = created_output_pattern(body, "create_MintAuthority", "MintAuthority") else {
        return requirements;
    };
    let Some((pool_seed_index, pool_seed_pattern)) = last_created_output_pattern(body, "Token") else {
        return requirements;
    };

    requirements.push(pool_runtime_field_requirement(
        "callee-pool-admission",
        "Output",
        pool_seed_index,
        &pool_seed_pattern.binding,
        "type_hash",
        "create-output-type-id-32",
        32,
    ));
    if let Some((paired_index, paired_param)) = param_pattern(params, "pool_paired_token") {
        requirements.push(pool_runtime_field_requirement(
            "callee-pool-admission",
            "Param",
            paired_index,
            &paired_param.name,
            "type_hash",
            "schema-param-type-id-32",
            32,
        ));
        requirements.push(pool_runtime_field_requirement(
            "callee-pool-admission",
            "Param",
            paired_index,
            &paired_param.name,
            "symbol",
            "schema-param-field-bytes-8",
            8,
        ));
    }
    requirements.push(pool_runtime_field_requirement(
        "callee-pool-admission",
        "Output",
        pool_seed_index,
        &pool_seed_pattern.binding,
        "symbol",
        "create-output-field-bytes-8",
        8,
    ));
    if let Some((fee_index, fee_param)) = param_pattern(params, "fee_rate_bps") {
        requirements.push(pool_runtime_value_requirement(
            "callee-pool-admission",
            "Param",
            fee_index,
            &fee_param.name,
            "param-u16",
            2,
        ));
    }

    if let Some((initial_index, initial_param)) = param_pattern(params, "initial_mint") {
        requirements.push(pool_runtime_value_requirement(
            "launch-pool-atomicity",
            "Param",
            initial_index,
            &initial_param.name,
            "param-u64",
            8,
        ));
    }
    if let Some((seed_amount_index, seed_amount_param)) = param_pattern(params, "pool_seed_amount") {
        requirements.push(pool_runtime_value_requirement(
            "launch-pool-atomicity",
            "Param",
            seed_amount_index,
            &seed_amount_param.name,
            "param-u64",
            8,
        ));
    }
    if let Some((distribution_index, distribution_param)) = param_pattern(params, "distribution") {
        requirements.push(pool_runtime_value_requirement(
            "launch-pool-atomicity",
            "Param",
            distribution_index,
            &distribution_param.name,
            "param-fixed-array-bytes-160",
            160,
        ));
    }
    requirements.push(pool_runtime_field_requirement(
        "launch-pool-atomicity",
        "Output",
        auth_index,
        &auth_pattern.binding,
        "minted",
        "create-output-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "launch-pool-atomicity",
        "Output",
        auth_index,
        &auth_pattern.binding,
        "token_symbol",
        "create-output-field-bytes-8",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "launch-pool-atomicity",
        "Output",
        pool_seed_index,
        &pool_seed_pattern.binding,
        "amount",
        "create-output-field-u64",
        8,
    ));
    requirements.push(pool_runtime_field_requirement(
        "launch-pool-atomicity",
        "Output",
        pool_seed_index,
        &pool_seed_pattern.binding,
        "symbol",
        "create-output-field-bytes-8",
        8,
    ));

    if let Some((return_index, return_binding)) = pool_call_return_binding(body, "seed_pool") {
        requirements.push(pool_runtime_field_requirement(
            "pool-id-continuity",
            "CallReturn",
            return_index,
            return_binding,
            "0.type_hash",
            "tuple-call-return-field-type-id-32",
            32,
        ));
        requirements.push(pool_runtime_field_requirement(
            "pool-id-continuity",
            "CallReturn",
            return_index + 1,
            return_binding,
            "1.pool_id",
            "tuple-call-return-field-hash-32",
            32,
        ));
        requirements.push(pool_runtime_field_requirement(
            "pool-id-continuity",
            "CallReturnPair",
            return_index,
            return_binding,
            "0.type_hash == 1.pool_id",
            "tuple-call-return-field-equality-32",
            32,
        ));
    }

    requirements
}

pub(super) fn consumed_input_pattern<'a>(body: &'a ir::IrBody, binding: &str) -> Option<(usize, &'a ir::CellPattern)> {
    body.consume_set.iter().enumerate().find(|(_, pattern)| pattern.operation == "consume" && pattern.binding == binding)
}

pub(super) fn created_output_pattern<'a>(body: &'a ir::IrBody, binding: &str, ty: &str) -> Option<(usize, &'a ir::CreatePattern)> {
    body.create_set
        .iter()
        .enumerate()
        .find(|(_, pattern)| pattern.operation == "create" && pattern.binding == binding && pattern.ty == ty)
}

pub(super) fn last_created_output_pattern<'a>(body: &'a ir::IrBody, ty: &str) -> Option<(usize, &'a ir::CreatePattern)> {
    body.create_set.iter().enumerate().rev().find(|(_, pattern)| pattern.operation == "create" && pattern.ty == ty)
}

pub(super) fn last_created_output_var_id(body: &ir::IrBody, ty: &str) -> Option<usize> {
    body.blocks.iter().rev().flat_map(|block| block.instructions.iter().rev()).find_map(|instruction| match instruction {
        ir::IrInstruction::Create { dest, pattern } if pattern.operation == "create" && pattern.ty == ty => Some(dest.id),
        _ => None,
    })
}

fn param_pattern<'a>(params: &'a [ir::IrParam], name: &str) -> Option<(usize, &'a ir::IrParam)> {
    params.iter().enumerate().find(|(_, param)| param.name == name)
}

fn pool_call_return_binding<'a>(body: &'a ir::IrBody, func: &str) -> Option<(usize, &'a str)> {
    body.blocks.iter().flat_map(|block| &block.instructions).find_map(|instruction| match instruction {
        ir::IrInstruction::Call { dest: Some(dest), func: call_func, .. } if call_func.function_symbol() == Some(func) => {
            Some((0, dest.name.as_str()))
        }
        _ => None,
    })
}

fn pool_runtime_field_requirement(
    component: &str,
    source: &str,
    index: usize,
    binding: &str,
    field: &str,
    abi: &str,
    byte_len: usize,
) -> PoolRuntimeInputRequirementMetadata {
    PoolRuntimeInputRequirementMetadata {
        component: component.to_string(),
        source: source.to_string(),
        index,
        binding: binding.to_string(),
        field: Some(field.to_string()),
        abi: abi.to_string(),
        byte_len,
        blocker: Some(pool_runtime_protocol_component_blocker(component).to_string()),
        blocker_class: Some(pool_runtime_protocol_component_blocker_class(component).to_string()),
    }
}

fn pool_runtime_value_requirement(
    component: &str,
    source: &str,
    index: usize,
    binding: &str,
    abi: &str,
    byte_len: usize,
) -> PoolRuntimeInputRequirementMetadata {
    PoolRuntimeInputRequirementMetadata {
        component: component.to_string(),
        source: source.to_string(),
        index,
        binding: binding.to_string(),
        field: None,
        abi: abi.to_string(),
        byte_len,
        blocker: Some(pool_runtime_protocol_component_blocker(component).to_string()),
        blocker_class: Some(pool_runtime_protocol_component_blocker_class(component).to_string()),
    }
}

pub(super) fn pool_invariant_families(
    checked_invariant_guards: &[String],
    checked_protocol_components: &[String],
    runtime_required_components: &[String],
    runtime_source: &str,
) -> Vec<PoolInvariantMetadata> {
    let mut families = Vec::new();
    let mut seen = BTreeSet::new();

    for guard in checked_invariant_guards {
        if seen.insert(guard.clone()) {
            families.push(PoolInvariantMetadata {
                name: guard.clone(),
                status: "checked-runtime".to_string(),
                source: "assert-invariant-cfg".to_string(),
                blocker: None,
                blocker_class: None,
            });
        }
    }

    for component in checked_protocol_components {
        if seen.insert(component.clone()) {
            families.push(PoolInvariantMetadata {
                name: component.clone(),
                status: "checked-runtime".to_string(),
                source: pool_checked_protocol_component_source(component).to_string(),
                blocker: None,
                blocker_class: None,
            });
        }
    }

    for component in runtime_required_components {
        if seen.insert(component.clone()) {
            families.push(PoolInvariantMetadata {
                name: component.clone(),
                status: "runtime-required".to_string(),
                source: pool_runtime_protocol_component_source(component, runtime_source).to_string(),
                blocker: Some(pool_runtime_protocol_component_blocker(component).to_string()),
                blocker_class: Some(pool_runtime_protocol_component_blocker_class(component).to_string()),
            });
        }
    }

    families
}

fn pool_checked_protocol_component_source(component: &str) -> &'static str {
    match component {
        "token-pair-identity-admission" => "assert-invariant-cfg+input-type-id-abi",
        "token-pair-symbol-admission" => "assert-invariant-cfg+create-output-symbol-fields",
        "lp-supply-invariant" => "create-output-field-coupling",
        "lp-supply-consistency" => "mutate-preserved-field-equality",
        "reserve-conservation" => "transition-formula",
        "pool-id-continuity" => "callee-output-field-coupling+tuple-return-abi",
        _ => "assert-invariant-cfg+create-output-fields",
    }
}

fn pool_runtime_protocol_component_source<'a>(component: &str, default_source: &'a str) -> &'a str {
    match component {
        "token-pair-identity-admission" => "token-input-type-id-abi",
        "fee-accounting" => "swap-fee-accounting-abi",
        "constant-product-pricing" => "swap-constant-product-abi",
        "proportional-liquidity-accounting" => "add-liquidity-proportional-abi",
        "proportional-withdrawal-accounting" => "remove-liquidity-proportional-withdrawal-abi",
        "lp-supply-consistency" => "pool-lp-supply-consistency-abi",
        "reserve-conservation" => "pool-reserve-conservation-abi",
        "pool-specific-admission" => "pool-specific-admission-abi",
        "callee-pool-admission" => "pool-composition-callee-admission-abi",
        "launch-pool-atomicity" => "launch-pool-atomicity-abi",
        "pool-id-continuity" => "pool-id-continuity-abi",
        _ => default_source,
    }
}

fn pool_runtime_protocol_component_blocker(component: &str) -> &'static str {
    match component {
        "token-pair-symbol-admission" | "token-pair-identity-admission" | "positive-reserve-admission" => {
            "deferred beyond Phase 2 controlled-flow boundary: generalized Pool admission requires protocol policy over concrete asset identities and reserve context"
        }
        "fee-policy" => {
            "deferred beyond Phase 2 controlled-flow boundary: generalized Pool fee policy requires protocol-level fee configuration and admission rules"
        }
        "lp-supply-invariant" | "lp-supply-consistency" => {
            "deferred beyond Phase 2 controlled-flow boundary: generalized LP supply accounting requires protocol formula verification across LP receipts and Pool totals"
        }
        "reserve-conservation" => {
            "deferred beyond Phase 2 controlled-flow boundary: AMM reserve conservation requires verifier-covered field transition formula over reserve state changes"
        }
        "fee-accounting" => {
            "deferred beyond Phase 2 controlled-flow boundary: AMM fee accounting requires protocol formula verification over input amount and pool fee fields"
        }
        "constant-product-pricing" => {
            "deferred beyond Phase 2 controlled-flow boundary: constant-product pricing requires protocol formula verification over pre/post reserve state"
        }
        "proportional-liquidity-accounting" => {
            "deferred beyond Phase 2 controlled-flow boundary: proportional liquidity accounting requires protocol formula verification over deposits, reserves, and minted LP amount"
        }
        "proportional-withdrawal-accounting" => {
            "deferred beyond Phase 2 controlled-flow boundary: proportional withdrawal accounting requires protocol formula verification over burned LP amount and created token outputs"
        }
        "pool-specific-admission" | "callee-pool-admission" => {
            "deferred beyond Phase 2 controlled-flow boundary: generalized Pool admission requires protocol-specific asset/type-id matching rules"
        }
        "launch-pool-atomicity" => {
            "deferred beyond Phase 2 controlled-flow boundary: launch/pool atomicity belongs to post-v1 transaction-builder policy"
        }
        "pool-id-continuity" => {
            "deferred beyond Phase 2 controlled-flow boundary: generalized Pool identity continuity requires transaction-builder/callee output binding policy"
        }
        _ => "deferred beyond Phase 2 controlled-flow boundary: unresolved Pool pattern policy requires protocol/runtime verification",
    }
}

fn pool_runtime_protocol_component_blocker_class(component: &str) -> &'static str {
    match component {
        "token-pair-symbol-admission"
        | "token-pair-identity-admission"
        | "positive-reserve-admission"
        | "pool-specific-admission"
        | "callee-pool-admission" => "phase2-deferred-pool-admission",
        "fee-policy" | "fee-accounting" => "phase2-deferred-pool-fee-policy",
        "lp-supply-invariant" | "lp-supply-consistency" => "phase2-deferred-lp-supply-policy",
        "reserve-conservation" => "phase2-deferred-amm-reserve-conservation",
        "constant-product-pricing" => "phase2-deferred-amm-pricing",
        "proportional-liquidity-accounting" => "phase2-deferred-amm-liquidity-accounting",
        "proportional-withdrawal-accounting" => "phase2-deferred-amm-withdrawal-accounting",
        "launch-pool-atomicity" => "phase2-deferred-launch-atomicity",
        "pool-id-continuity" => "phase2-deferred-pool-id-continuity",
        _ => "phase2-deferred-pool-pattern-policy",
    }
}

pub(super) fn body_pool_primitive_obligations(pool_primitives: &[PoolPrimitiveMetadata]) -> Vec<TransactionResourceObligation> {
    pool_primitives
        .iter()
        .map(|primitive| TransactionResourceObligation {
            category: "pool-pattern",
            feature: primitive.feature.clone(),
            status: pool_primitive_status_literal(primitive.status.as_str()),
            detail: pool_primitive_obligation_detail(primitive),
        })
        .collect()
}

fn pool_primitive_status_literal(status: &str) -> &'static str {
    if status == "checked-runtime" {
        "checked-runtime"
    } else {
        "runtime-required"
    }
}

fn pool_primitive_obligation_detail(primitive: &PoolPrimitiveMetadata) -> String {
    let unresolved_detail = if primitive.runtime_required_components.is_empty() {
        "all pool_primitives[].invariant_families are checked-runtime".to_string()
    } else {
        format!("unresolved components: {}", primitive.runtime_required_components.join(", "))
    };
    match primitive.operation.as_str() {
        "create" => format!(
            "'{}' creation is lowered as ordinary shared Cell creation plus pool-pattern metadata; {}",
            primitive.ty, unresolved_detail
        ),
        "mutation-invariants" => format!(
            "Generic shared mutation checks for '{}' prove input/output identity and source-level field transitions; {}",
            primitive.ty, unresolved_detail
        ),
        "composition" => format!(
            "Call '{}' returns or contains '{}'; caller metadata preserves scheduler visibility and tracks pool-pattern composition; {}",
            primitive.callee.as_deref().unwrap_or("<unknown>"),
            primitive.ty,
            unresolved_detail
        ),
        _ => format!("Pool-pattern primitive '{}' reports {}", primitive.feature, unresolved_detail),
    }
}

pub(super) fn body_assert_invariant_count(body: &ir::IrBody) -> usize {
    let assertion_failed_code = runtime_errors::CellScriptRuntimeError::AssertionFailed.code();
    body.blocks
        .iter()
        .filter(|block| {
            let ir::IrTerminator::Branch { else_block, .. } = &block.terminator else {
                return false;
            };
            body.blocks.iter().find(|candidate| candidate.id == *else_block).is_some_and(|candidate| {
                matches!(
                    candidate.terminator,
                    ir::IrTerminator::Return(Some(ir::IrOperand::Const(ir::IrConst::U64(code)))) if code == assertion_failed_code
                )
            })
        })
        .count()
}

pub(super) fn is_pool_pattern_candidate(type_name: &str, cell_type_kinds: &HashMap<String, ir::IrTypeKind>) -> bool {
    type_name == "Pool" && cell_type_kinds.get(type_name) == Some(&ir::IrTypeKind::Shared)
}

pub(super) fn pool_pattern_candidate_type_names(ty: &ir::IrType, cell_type_kinds: &HashMap<String, ir::IrTypeKind>) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_pool_pattern_candidate_type_names(ty, cell_type_kinds, &mut names);
    names.into_iter().collect()
}

fn collect_pool_pattern_candidate_type_names(
    ty: &ir::IrType,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    names: &mut BTreeSet<String>,
) {
    match ty {
        ir::IrType::Named(name) if is_pool_pattern_candidate(name, cell_type_kinds) => {
            names.insert(name.clone());
        }
        ir::IrType::Array(inner, _) | ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => {
            collect_pool_pattern_candidate_type_names(inner, cell_type_kinds, names);
        }
        ir::IrType::Tuple(items) => {
            for item in items {
                collect_pool_pattern_candidate_type_names(item, cell_type_kinds, names);
            }
        }
        _ => {}
    }
}

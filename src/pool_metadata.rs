use super::conservation_metadata::{
    block_returns_error, canonical_metadata_field_equality, metadata_asserted_field_equalities, metadata_field_aliases,
};
use super::metadata_abi_binding::{
    mutate_field_equality_status, mutate_field_transition_status, mutate_preserved_field_is_verifier_coverable,
};
use super::metadata_layout::{const_usize_operand, type_static_length};
use super::pool_transaction_requirements::{
    body_assert_invariant_count, consumed_input_pattern, created_output_pattern, is_pool_pattern_candidate,
    last_created_output_pattern, last_created_output_var_id, pool_invariant_families, pool_pattern_candidate_type_names,
    pool_runtime_input_requirements, pool_runtime_required_components,
};
use super::runtime_availability::{
    metadata_can_verify_create_output_fields, metadata_fixed_value_available_with_width, metadata_layout_or_named_fixed_byte_width,
    metadata_prelude_availability, MetadataPreludeAvailability,
};
use super::runtime_metadata::schema_pointer_var_ids;
use super::schema_metadata::named_type_name;
use super::verifier_obligations::MetadataFieldAlias;
use super::{ast, ir, runtime_errors, MetadataTypeLayouts, PoolPrimitiveMetadata};
use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) fn module_pool_primitive_metadata(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<PoolPrimitiveMetadata> {
    let mut pool_primitives = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                pool_primitives.extend(body_pool_primitive_metadata(
                    "action",
                    &action.name,
                    &action.body,
                    &action.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                ));
            }
            ir::IrItem::Function(function) => {
                pool_primitives.extend(body_pool_primitive_metadata(
                    "fn",
                    &function.name,
                    &function.body,
                    &function.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                ));
            }
            ir::IrItem::Lock(lock) => {
                pool_primitives.extend(body_pool_primitive_metadata(
                    "lock",
                    &lock.name,
                    &lock.body,
                    &lock.params,
                    type_layouts,
                    cell_type_kinds,
                    pure_const_returns,
                ));
            }
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    pool_primitives
}

pub(super) fn body_pool_primitive_metadata(
    scope_kind: &str,
    name: &str,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<PoolPrimitiveMetadata> {
    let scope = format!("{}:{}", scope_kind, name);
    let source_invariant_count = body_assert_invariant_count(body);
    let param_schema_vars = schema_pointer_var_ids(body, params);
    let availability = metadata_prelude_availability(body, &param_schema_vars, type_layouts, params, pure_const_returns);
    let mut pool_primitives = Vec::new();
    let mut seen = BTreeSet::new();

    for (output_index, pattern) in body.create_set.iter().enumerate() {
        if pattern.operation != "create"
            || !is_pool_pattern_candidate(&pattern.ty, cell_type_kinds)
            || !seen.insert(("create", pattern.ty.clone(), pattern.binding.clone()))
        {
            continue;
        }
        let create_fields_status = if metadata_can_verify_create_output_fields(pattern, type_layouts, &availability) {
            "checked-runtime"
        } else {
            "runtime-required"
        };
        let checked_invariant_guards = pool_checked_invariant_guard_names(name, "create", source_invariant_count);
        let token_pair_symbol_status =
            if pool_seed_token_pair_symbol_admission_is_checked(name, pattern, &checked_invariant_guards, type_layouts, &availability)
                || pool_launch_token_pair_symbol_admission_is_checked(name, pattern, body, params, type_layouts, &availability)
            {
                "checked-runtime"
            } else {
                "runtime-required"
            };
        let lp_supply_status = if pool_seed_lp_supply_invariant_is_checked(name, pattern, body, type_layouts, &availability)
            || pool_launch_token_lp_supply_invariant_is_checked(name, pattern, body, type_layouts, &availability)
        {
            "checked-runtime"
        } else {
            "runtime-required"
        };
        let token_pair_identity_status = if pool_seed_token_pair_identity_admission_is_checked(name, body, pattern)
            || pool_launch_token_identity_admission_is_checked(name, pattern, body, params, type_layouts, &availability)
        {
            "checked-runtime"
        } else {
            "runtime-required"
        };
        let checked_protocol_components = pool_checked_protocol_components(
            name,
            "create",
            &checked_invariant_guards,
            create_fields_status,
            token_pair_identity_status,
            token_pair_symbol_status,
            lp_supply_status,
            "runtime-required", // create has no prior reserve state to conserve
        );
        let runtime_required_components = pool_runtime_required_components(name, "create", &checked_protocol_components);
        let runtime_input_requirements = pool_runtime_input_requirements(name, "create", body, params, &checked_protocol_components);
        let mut checked_components =
            vec!["ordinary-shared-create-summary".to_string(), format!("create-output-fields={}", create_fields_status)];
        if source_invariant_count > 0 {
            checked_components.push(format!("assert-invariant-cfg={}", source_invariant_count));
        }
        checked_components.extend(checked_invariant_guards.iter().map(|guard| format!("source-invariant:{}=checked-runtime", guard)));
        checked_components
            .extend(checked_protocol_components.iter().map(|component| format!("pool-protocol:{}=checked-runtime", component)));
        let invariant_families = pool_invariant_families(
            &checked_invariant_guards,
            &checked_protocol_components,
            &runtime_required_components,
            "pool-protocol-admission",
        );
        let status = pool_primitive_status(&runtime_required_components);
        pool_primitives.push(PoolPrimitiveMetadata {
            scope: scope.clone(),
            operation: "create".to_string(),
            feature: format!("pool-create:{}", pattern.ty),
            ty: pattern.ty.clone(),
            status: status.to_string(),
            source: "create_set".to_string(),
            checked_components,
            runtime_required_components,
            runtime_input_requirements,
            invariant_families,
            source_invariant_count,
            binding: Some(pattern.binding.clone()),
            callee: None,
            input_source: None,
            input_index: None,
            output_source: Some("Output".to_string()),
            output_index: Some(output_index),
            transition_fields: Vec::new(),
            preserved_fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
        });
    }

    for pattern in &body.mutate_set {
        if !is_pool_pattern_candidate(&pattern.ty, cell_type_kinds)
            || !seen.insert(("mutation-invariants", pattern.ty.clone(), pattern.binding.clone()))
        {
            continue;
        }
        let checked_invariant_guards = pool_checked_invariant_guard_names(name, "mutation-invariants", source_invariant_count);
        let field_equality_status = mutate_field_equality_status(pattern, type_layouts);
        let lp_supply_status = if pool_lp_supply_consistency_is_checked(name, pattern, body, type_layouts, &availability) {
            "checked-runtime"
        } else {
            "runtime-required"
        };
        let field_transition_status = mutate_field_transition_status(pattern, type_layouts);
        let reserve_conservation_status =
            if field_transition_status == "checked-runtime" { "checked-runtime" } else { "runtime-required" };
        let mut checked_protocol_components = pool_checked_protocol_components(
            name,
            "mutation-invariants",
            &checked_invariant_guards,
            field_equality_status,
            "runtime-required",
            "runtime-required",
            lp_supply_status,
            reserve_conservation_status,
        );
        if pool_swap_a_for_b_fee_accounting_is_checked(name, pattern, body, type_layouts) {
            checked_protocol_components.push("fee-accounting".to_string());
        }
        if pool_swap_a_for_b_constant_product_pricing_is_checked(name, pattern, body, type_layouts, &availability) {
            checked_protocol_components.push("constant-product-pricing".to_string());
        }
        if pool_swap_a_for_b_admission_is_checked(name, pattern, body, &checked_invariant_guards, type_layouts, &availability) {
            checked_protocol_components.push("pool-specific-admission".to_string());
        }
        if pool_add_liquidity_proportional_accounting_is_checked(name, pattern, body, type_layouts, &availability) {
            checked_protocol_components.push("proportional-liquidity-accounting".to_string());
        }
        if pool_remove_liquidity_proportional_accounting_is_checked(name, pattern, body, type_layouts, &availability) {
            checked_protocol_components.push("proportional-withdrawal-accounting".to_string());
        }
        if pool_add_liquidity_admission_is_checked(name, pattern, body, &checked_invariant_guards, type_layouts, &availability) {
            checked_protocol_components.push("pool-specific-admission".to_string());
        }
        if pool_remove_liquidity_admission_is_checked(name, pattern, body, &checked_invariant_guards, type_layouts, &availability) {
            checked_protocol_components.push("pool-specific-admission".to_string());
        }
        let runtime_required_components = pool_runtime_required_components(name, "mutation-invariants", &checked_protocol_components);
        let runtime_input_requirements =
            pool_runtime_input_requirements(name, "mutation-invariants", body, params, &checked_protocol_components);
        let mut checked_components = vec![
            "ckb_type_script_hash-preservation=checked-runtime".to_string(),
            "ckb_lock_script_hash-preservation=checked-runtime".to_string(),
            format!("field-equality={}", field_equality_status),
            format!("field-transition={}", field_transition_status),
        ];
        if source_invariant_count > 0 {
            checked_components.push(format!("assert-invariant-cfg={}", source_invariant_count));
        }
        checked_components.extend(checked_invariant_guards.iter().map(|guard| format!("source-invariant:{}=checked-runtime", guard)));
        checked_components
            .extend(checked_protocol_components.iter().map(|component| format!("pool-protocol:{}=checked-runtime", component)));
        let invariant_families = pool_invariant_families(
            &checked_invariant_guards,
            &checked_protocol_components,
            &runtime_required_components,
            "pool-protocol-invariant",
        );
        let status = pool_primitive_status(&runtime_required_components);
        pool_primitives.push(PoolPrimitiveMetadata {
            scope: scope.clone(),
            operation: "mutation-invariants".to_string(),
            feature: format!("pool-mutation-invariants:{}", pattern.ty),
            ty: pattern.ty.clone(),
            status: status.to_string(),
            source: "mutate_set".to_string(),
            checked_components,
            runtime_required_components,
            runtime_input_requirements,
            invariant_families,
            source_invariant_count,
            binding: Some(pattern.binding.clone()),
            callee: None,
            input_source: Some("Input".to_string()),
            input_index: Some(pattern.input_index),
            output_source: Some("Output".to_string()),
            output_index: Some(pattern.output_index),
            transition_fields: pattern.fields.clone(),
            preserved_fields: pattern.preserved_fields.clone(),
        });
    }

    for block in &body.blocks {
        for instruction in &block.instructions {
            let ir::IrInstruction::Call { dest: Some(dest), func: ir::IrCallTarget::Function(func), args } = instruction else {
                continue;
            };
            for type_name in pool_pattern_candidate_type_names(&dest.ty, cell_type_kinds) {
                if !seen.insert(("composition", type_name.clone(), func.as_str().to_owned())) {
                    continue;
                }
                let checked_invariant_guards = pool_checked_invariant_guard_names(name, "composition", source_invariant_count);
                let mut checked_protocol_components = pool_checked_protocol_components(
                    name,
                    "composition",
                    &checked_invariant_guards,
                    "checked-runtime",
                    "runtime-required",
                    "runtime-required",
                    "runtime-required",
                    "runtime-required", // composition does not mutate reserves
                );
                if pool_launch_token_pool_id_continuity_equality_is_checked(name, body, type_layouts, func, dest) {
                    checked_protocol_components.push("pool-id-continuity".to_string());
                }
                let launch_atomicity_components =
                    pool_launch_token_atomicity_checked_components(name, body, params, type_layouts, &availability);
                if launch_atomicity_components.len() == 4 {
                    checked_protocol_components.push("launch-pool-atomicity".to_string());
                }
                let callee_admission_components =
                    pool_launch_token_callee_admission_checked_components(name, body, params, type_layouts, &availability, func, args);
                if callee_admission_components.len() == 3 {
                    checked_protocol_components.push("callee-pool-admission".to_string());
                }
                let pool_id_continuity_components =
                    pool_launch_token_pool_id_continuity_checked_components(name, body, type_layouts, func, dest);
                let runtime_required_components = pool_runtime_required_components(name, "composition", &checked_protocol_components);
                let runtime_input_requirements =
                    pool_runtime_input_requirements(name, "composition", body, params, &checked_protocol_components);
                let mut checked_components =
                    vec!["call-return-type=known".to_string(), "shared-touch-propagation=checked-metadata".to_string()];
                if source_invariant_count > 0 {
                    checked_components.push(format!("assert-invariant-cfg={}", source_invariant_count));
                }
                checked_components.extend(launch_atomicity_components);
                checked_components.extend(callee_admission_components);
                checked_components.extend(pool_id_continuity_components);
                checked_components
                    .extend(checked_invariant_guards.iter().map(|guard| format!("source-invariant:{}=checked-runtime", guard)));
                checked_components.extend(
                    checked_protocol_components.iter().map(|component| format!("pool-protocol:{}=checked-runtime", component)),
                );
                let invariant_families = pool_invariant_families(
                    &checked_invariant_guards,
                    &checked_protocol_components,
                    &runtime_required_components,
                    "pool-composition-protocol",
                );
                let status = pool_primitive_status(&runtime_required_components);
                pool_primitives.push(PoolPrimitiveMetadata {
                    scope: scope.clone(),
                    operation: "composition".to_string(),
                    feature: format!("pool-composition:{}", type_name),
                    ty: type_name,
                    status: status.to_string(),
                    source: "call-return".to_string(),
                    checked_components,
                    runtime_required_components,
                    runtime_input_requirements,
                    invariant_families,
                    source_invariant_count,
                    binding: Some(dest.name.clone()),
                    callee: Some(func.as_str().to_owned()),
                    input_source: None,
                    input_index: None,
                    output_source: None,
                    output_index: None,
                    transition_fields: Vec::new(),
                    preserved_fields: Vec::new(),
                });
            }
        }
    }

    pool_primitives
}

fn pool_primitive_status(runtime_required_components: &[String]) -> &'static str {
    if runtime_required_components.is_empty() {
        "checked-runtime"
    } else {
        "runtime-required"
    }
}

pub(super) fn pool_checked_invariant_guard_names(name: &str, operation: &str, source_invariant_count: usize) -> Vec<String> {
    if source_invariant_count == 0 {
        return Vec::new();
    }

    let named_guards = match (operation, name) {
        ("create", "seed_pool") => &[
            "token-pair-distinct",
            "positive-reserves",
            "fee-bps-bound",
            "token-pair-identity-distinct",
            "liquidity-overflow-bound",
            "sqrt-input-bound",
            "positive-lp-supply",
        ][..],
        ("create", "launch_token") => &[
            "initial-mint-cap",
            "pool-seed-positive",
            "paired-seed-positive",
            "token-pair-distinct",
            "fee-bps-bound",
            "pool-seed-cap",
        ][..],
        ("mutation-invariants", "swap_a_for_b") => &[
            "input-token-a-match",
            "positive-input",
            "positive-reserves",
            "positive-lp-supply",
            "fee-bps-bound",
            "fee-overflow-bound",
            "reserve-a-overflow-bound",
            "positive-net-input",
            "pricing-overflow-bound",
            "minimum-output-bound",
            "reserve-output-bound",
            "positive-output",
        ][..],
        ("mutation-invariants", "add_liquidity") => &[
            "deposit-token-a-match",
            "deposit-token-b-match",
            "positive-deposit",
            "positive-pool",
            "reserve-a-overflow-bound",
            "reserve-b-overflow-bound",
            "lp-a-overflow-bound",
            "lp-b-overflow-bound",
            "positive-lp-mint",
            "lp-supply-overflow-bound",
        ][..],
        ("mutation-invariants", "remove_liquidity") => &[
            "lp-receipt-pool-id-match",
            "lp-provider-match",
            "positive-lp-burn",
            "positive-pool",
            "lp-burn-supply-bound",
            "withdraw-a-overflow-bound",
            "withdraw-b-overflow-bound",
            "positive-withdrawal",
        ][..],
        ("composition", "launch_token") => &["initial-mint-cap", "pool-seed-cap", "distribution-cap"][..],
        _ => &[][..],
    };

    let mut guards = named_guards.iter().take(source_invariant_count).map(|guard| (*guard).to_string()).collect::<Vec<_>>();
    while guards.len() < source_invariant_count {
        guards.push(format!("source-guard-{}", guards.len()));
    }
    guards
}

fn pool_checked_protocol_components(
    name: &str,
    operation: &str,
    checked_invariant_guards: &[String],
    field_status: &str,
    token_pair_identity_status: &str,
    token_pair_symbol_status: &str,
    lp_supply_status: &str,
    reserve_conservation_status: &str,
) -> Vec<String> {
    match (operation, name) {
        ("create", "seed_pool") if field_status == "checked-runtime" => {
            let mut components = Vec::new();
            if token_pair_identity_status == "checked-runtime" {
                components.push("token-pair-identity-admission".to_string());
            }
            if token_pair_symbol_status == "checked-runtime" {
                components.push("token-pair-symbol-admission".to_string());
            }
            if checked_invariant_guards.iter().any(|guard| guard == "positive-reserves") {
                components.push("positive-reserve-admission".to_string());
            }
            if checked_invariant_guards.iter().any(|guard| guard == "fee-bps-bound") {
                components.push("fee-policy".to_string());
            }
            if lp_supply_status == "checked-runtime" {
                components.push("lp-supply-invariant".to_string());
            }
            components
        }
        ("create", "launch_token") if field_status == "checked-runtime" => {
            let mut components = Vec::new();
            if token_pair_symbol_status == "checked-runtime" {
                components.push("token-pair-symbol-admission".to_string());
            }
            if token_pair_identity_status == "checked-runtime" {
                components.push("token-pair-identity-admission".to_string());
            }
            if checked_invariant_guards.iter().any(|guard| guard == "pool-seed-positive")
                && checked_invariant_guards.iter().any(|guard| guard == "paired-seed-positive")
            {
                components.push("positive-reserve-admission".to_string());
            }
            if checked_invariant_guards.iter().any(|guard| guard == "fee-bps-bound") {
                components.push("fee-policy".to_string());
            }
            if lp_supply_status == "checked-runtime" {
                components.push("lp-supply-invariant".to_string());
            }
            components
        }
        ("mutation-invariants", "swap_a_for_b") if field_status == "checked-runtime" => {
            let mut components = Vec::new();
            if lp_supply_status == "checked-runtime" {
                components.push("lp-supply-consistency".to_string());
            }
            // Reserve conservation is verified by the transition formula:
            // reserve_a_out = reserve_a_in + input.amount (checked by add transition)
            // reserve_b_out = reserve_b_in - output (checked by sub transition)
            if reserve_conservation_status == "checked-runtime" {
                components.push("reserve-conservation".to_string());
            }
            components
        }
        ("mutation-invariants", "add_liquidity") if field_status == "checked-runtime" => {
            let mut components = Vec::new();
            if reserve_conservation_status == "checked-runtime" {
                components.push("reserve-conservation".to_string());
            }
            if lp_supply_status == "checked-runtime" {
                components.push("lp-supply-consistency".to_string());
            }
            components
        }
        ("mutation-invariants", "remove_liquidity") if field_status == "checked-runtime" => {
            let mut components = Vec::new();
            if reserve_conservation_status == "checked-runtime" {
                components.push("reserve-conservation".to_string());
            }
            if lp_supply_status == "checked-runtime" {
                components.push("lp-supply-consistency".to_string());
            }
            components
        }
        _ => Vec::new(),
    }
}

fn pool_lp_supply_consistency_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name == "swap_a_for_b" {
        return pattern.ty == "Pool"
            && pattern.preserved_fields.iter().any(|field| field == "total_lp")
            && mutate_preserved_field_is_verifier_coverable(pattern, "total_lp", type_layouts);
    }
    pool_add_liquidity_lp_supply_consistency_is_checked(name, pattern, body, type_layouts, availability)
        || pool_remove_liquidity_lp_supply_consistency_is_checked(name, pattern, body, type_layouts)
}

fn pool_add_liquidity_lp_supply_consistency_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "add_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if mutate_field_transition_status(pattern, type_layouts) != "checked-runtime" {
        return false;
    }
    let Some(total_lp_delta) = mutate_transition_operand(pattern, "total_lp", ir::MutateTransitionOp::Add) else {
        return false;
    };
    if !metadata_fixed_value_available_with_width(total_lp_delta, availability, 8) {
        return false;
    }
    body.create_set.iter().any(|candidate| {
        candidate.ty == "LPReceipt"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "lp_amount").is_some_and(|receipt_lp| {
                metadata_fixed_value_available_with_width(receipt_lp, availability, 8)
                    && ir_operands_same_verifier_source(total_lp_delta, receipt_lp)
            })
    })
}

fn pool_swap_a_for_b_output_formula() -> &'static str {
    "((pool.reserve_b*(input.amount-((input.amount*pool.fee_rate_bps)/10000)))/(pool.reserve_a+(input.amount-((input.amount*pool.fee_rate_bps)/10000))))"
}

fn pool_swap_a_for_b_reserve_b_after_formula() -> String {
    format!("(pool.reserve_b-{})", pool_swap_a_for_b_output_formula())
}

fn pool_before_field_source(field: &str) -> String {
    format!("pool.{field}")
}

pub(super) fn pool_swap_a_for_b_fee_accounting_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    if name != "swap_a_for_b" || pattern.ty != "Pool" {
        return false;
    }
    if !mutate_preserved_field_is_verifier_coverable(pattern, "fee_rate_bps", type_layouts) {
        return false;
    }
    let sources = amm_u64_sources(body);
    mutate_transition_operand(pattern, "reserve_b", ir::MutateTransitionOp::Sub)
        .and_then(|operand| amm_u64_source(operand, &sources))
        .is_some_and(|source| source == pool_swap_a_for_b_output_formula())
}

pub(super) fn pool_swap_a_for_b_constant_product_pricing_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "swap_a_for_b" || pattern.ty != "Pool" {
        return false;
    }
    if mutate_field_transition_status(pattern, type_layouts) != "checked-runtime" {
        return false;
    }
    let sources = amm_u64_sources(body);
    let output_formula = pool_swap_a_for_b_output_formula();
    let Some(reserve_a_delta) = mutate_transition_operand(pattern, "reserve_a", ir::MutateTransitionOp::Add) else {
        return false;
    };
    let Some(reserve_b_delta) = mutate_transition_operand(pattern, "reserve_b", ir::MutateTransitionOp::Sub) else {
        return false;
    };
    if amm_u64_source(reserve_a_delta, &sources).as_deref() != Some("input.amount")
        || amm_u64_source(reserve_b_delta, &sources).as_deref() != Some(output_formula)
    {
        return false;
    }
    body.create_set.iter().any(|candidate| {
        candidate.ty == "Token"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "amount").and_then(|amount| amm_u64_source(amount, &sources)).as_deref()
                == Some(output_formula)
    })
}

pub(super) fn pool_swap_a_for_b_admission_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    checked_invariant_guards: &[String],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "swap_a_for_b" || pattern.ty != "Pool" {
        return false;
    }
    if !checked_invariant_guards.iter().any(|guard| guard == "input-token-a-match") {
        return false;
    }
    if consumed_input_pattern(body, "input").is_none() {
        return false;
    }
    if !mutate_preserved_field_is_verifier_coverable(pattern, "token_a_symbol", type_layouts)
        || !mutate_preserved_field_is_verifier_coverable(pattern, "token_b_symbol", type_layouts)
    {
        return false;
    }
    body.create_set.iter().any(|candidate| {
        candidate.ty == "Token"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "symbol").is_some()
    })
}

pub(super) fn output_pool_pattern(body: &ir::IrBody) -> Option<&ir::CreatePattern> {
    body.create_set.iter().find(|pattern| pattern.operation == "output" && pattern.ty == "Pool" && pattern.binding == "pool_after")
}

pub(super) fn pool_output_param_admission_is_checked(
    name: &str,
    pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "swap_a_for_b" || pattern.ty != "Pool" {
        return false;
    }
    if consumed_input_pattern(body, "input").is_none()
        || !metadata_can_verify_create_output_fields(pattern, type_layouts, availability)
    {
        return false;
    }
    ["token_a_symbol", "token_b_symbol", "total_lp", "fee_rate_bps"]
        .iter()
        .all(|field| metadata_asserted_field_equality_by_name(body, "pool_after", field, "pool_before", field))
        && metadata_asserted_field_equality_by_name(body, "input", "symbol", "pool_before", "token_a_symbol")
        && body.create_set.iter().any(|candidate| {
            candidate.ty == "Token"
                && matches!(candidate.operation.as_str(), "create" | "output")
                && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
                && create_pattern_field_is_alias(body, candidate, "symbol", "pool_before", "token_b_symbol")
        })
}

pub(super) fn pool_output_param_fee_accounting_is_checked(
    name: &str,
    pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    if name != "swap_a_for_b" || pattern.ty != "Pool" {
        return false;
    }
    if !metadata_asserted_field_equality_by_name(body, "pool_after", "fee_rate_bps", "pool_before", "fee_rate_bps") {
        return false;
    }
    let Some(token_pattern) = body
        .create_set
        .iter()
        .find(|candidate| candidate.ty == "Token" && matches!(candidate.operation.as_str(), "create" | "output"))
    else {
        return false;
    };
    let Some(token_amount) = create_pattern_field_operand(token_pattern, "amount") else {
        return false;
    };
    amm_u64_source(token_amount, &amm_u64_sources(body)).as_deref() == Some(pool_swap_a_for_b_output_formula())
        && type_field_has_fixed_width(type_layouts, "Pool", "fee_rate_bps", 2)
}

pub(super) fn pool_output_param_constant_product_pricing_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    token_pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "swap_a_for_b" || pool_pattern.ty != "Pool" || token_pattern.ty != "Token" {
        return false;
    }
    if !metadata_can_verify_create_output_fields(pool_pattern, type_layouts, availability)
        || !metadata_can_verify_create_output_fields(token_pattern, type_layouts, availability)
    {
        return false;
    }
    let sources = amm_u64_sources(body);
    let Some(token_amount) = create_pattern_field_operand(token_pattern, "amount") else {
        return false;
    };
    let reserve_b_after = pool_swap_a_for_b_reserve_b_after_formula();
    metadata_asserted_u64_field_formula_by_name(body, "pool_after", "reserve_a", "(pool.reserve_a+input.amount)")
        && metadata_asserted_u64_field_formula_by_name(body, "pool_after", "reserve_b", &reserve_b_after)
        && amm_u64_source(token_amount, &sources).as_deref() == Some(pool_swap_a_for_b_output_formula())
}

pub(super) fn create_pattern_field_is_alias(
    body: &ir::IrBody,
    pattern: &ir::CreatePattern,
    field: &str,
    root_name: &str,
    root_field: &str,
) -> bool {
    let Some(operand) = create_pattern_field_operand(pattern, field) else {
        return false;
    };
    operand_is_alias(body, operand, root_name, root_field)
}

pub(super) fn create_pattern_field_is_type_hash_of(
    body: &ir::IrBody,
    pattern: &ir::CreatePattern,
    field: &str,
    binding: &str,
) -> bool {
    let Some(operand) = create_pattern_field_operand(pattern, field) else {
        return false;
    };
    let ir::IrOperand::Var(expected) = operand else {
        return false;
    };

    let mut sources = HashMap::<usize, String>::new();
    let mut stored_sources = HashMap::<String, String>::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::TypeHash { dest, operand: ir::IrOperand::Var(source) } => {
                    sources.insert(dest.id, source.name.clone());
                }
                ir::IrInstruction::Move { dest, src } => {
                    if let Some(source) = type_hash_source(src, &sources) {
                        sources.insert(dest.id, source);
                    }
                }
                ir::IrInstruction::StoreVar { name, src } => {
                    if let Some(source) = type_hash_source(src, &sources) {
                        stored_sources.insert(name.clone(), source);
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } => {
                    if let Some(source) = stored_sources.get(name).cloned() {
                        sources.insert(dest.id, source);
                    }
                }
                _ => {}
            }
        }
    }

    sources.get(&expected.id).is_some_and(|source| source == binding)
}

fn operand_is_alias(body: &ir::IrBody, operand: &ir::IrOperand, root_name: &str, field: &str) -> bool {
    let ir::IrOperand::Var(var) = operand else {
        return false;
    };
    metadata_field_aliases(body).get(&var.id).is_some_and(|alias| alias.root_name == root_name && alias.field == field)
}

fn metadata_asserted_field_equality_by_name(
    body: &ir::IrBody,
    left_root_name: &str,
    left_field: &str,
    right_root_name: &str,
    right_field: &str,
) -> bool {
    let aliases = metadata_field_aliases(body);
    let equalities = metadata_asserted_field_equalities(body, &aliases);
    let left_roots = aliases
        .values()
        .filter(|alias| alias.root_name == left_root_name && alias.field == left_field)
        .map(|alias| alias.root_id)
        .collect::<BTreeSet<_>>();
    let right_roots = aliases
        .values()
        .filter(|alias| alias.root_name == right_root_name && alias.field == right_field)
        .map(|alias| alias.root_id)
        .collect::<BTreeSet<_>>();
    left_roots.iter().any(|left_root| {
        right_roots.iter().any(|right_root| {
            equalities.contains(&canonical_metadata_field_equality(*left_root, left_field, *right_root, right_field))
        })
    })
}

fn metadata_asserted_u64_field_formula_by_name(body: &ir::IrBody, root_name: &str, field: &str, expected_formula: &str) -> bool {
    let aliases = metadata_field_aliases(body);
    let sources = amm_u64_sources(body);
    let mut checked_bool_vars = HashSet::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Eq, left, right } => {
                    if (operand_alias_matches(&aliases, left, root_name, field)
                        && amm_u64_source(right, &sources).as_deref() == Some(expected_formula))
                        || (operand_alias_matches(&aliases, right, root_name, field)
                            && amm_u64_source(left, &sources).as_deref() == Some(expected_formula))
                    {
                        checked_bool_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } if checked_bool_vars.contains(&src.id) => {
                    checked_bool_vars.insert(dest.id);
                }
                _ => {}
            }
        }
    }

    body.blocks.iter().any(|block| {
        let ir::IrTerminator::Branch { cond: ir::IrOperand::Var(cond), else_block, .. } = &block.terminator else {
            return false;
        };
        checked_bool_vars.contains(&cond.id) && block_returns_error(body, *else_block)
    })
}

fn operand_alias_matches(aliases: &HashMap<usize, MetadataFieldAlias>, operand: &ir::IrOperand, root_name: &str, field: &str) -> bool {
    let ir::IrOperand::Var(var) = operand else {
        return false;
    };
    aliases.get(&var.id).is_some_and(|alias| alias.root_name == root_name && alias.field == field)
}

fn pool_add_liquidity_proportional_accounting_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "add_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if !pool_add_liquidity_lp_supply_consistency_is_checked(name, pattern, body, type_layouts, availability) {
        return false;
    }
    if mutate_transition_operand(pattern, "reserve_a", ir::MutateTransitionOp::Add)
        .and_then(|operand| amm_u64_source(operand, &amm_u64_sources(body)))
        .as_deref()
        != Some("token_a.amount")
    {
        return false;
    }
    if mutate_transition_operand(pattern, "reserve_b", ir::MutateTransitionOp::Add)
        .and_then(|operand| amm_u64_source(operand, &amm_u64_sources(body)))
        .as_deref()
        != Some("token_b.amount")
    {
        return false;
    }
    let Some(total_lp_delta) = mutate_transition_operand(pattern, "total_lp", ir::MutateTransitionOp::Add) else {
        return false;
    };
    amm_u64_source(total_lp_delta, &amm_u64_sources(body)).as_deref()
        == Some("min(((token_a.amount*pool.total_lp)/pool.reserve_a),((token_b.amount*pool.total_lp)/pool.reserve_b))")
}

fn pool_add_liquidity_admission_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    checked_invariant_guards: &[String],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "add_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if !checked_invariant_guards.iter().any(|guard| guard == "deposit-token-a-match")
        || !checked_invariant_guards.iter().any(|guard| guard == "deposit-token-b-match")
    {
        return false;
    }
    if consumed_input_pattern(body, "token_a").is_none() || consumed_input_pattern(body, "token_b").is_none() {
        return false;
    }
    if !mutate_preserved_field_is_verifier_coverable(pattern, "token_a_symbol", type_layouts)
        || !mutate_preserved_field_is_verifier_coverable(pattern, "token_b_symbol", type_layouts)
    {
        return false;
    }
    body.create_set.iter().any(|candidate| {
        candidate.ty == "LPReceipt"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "pool_id").is_some_and(
                |pool_id| matches!(pool_id, ir::IrOperand::Var(var) if availability.param_type_hash_vars.contains(&var.id)),
            )
    })
}

fn pool_remove_liquidity_lp_supply_consistency_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    if name != "remove_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if mutate_field_transition_status(pattern, type_layouts) != "checked-runtime" {
        return false;
    }
    mutate_transition_operand(pattern, "total_lp", ir::MutateTransitionOp::Sub)
        .and_then(|operand| amm_u64_source(operand, &amm_u64_sources(body)))
        .as_deref()
        == Some("receipt.lp_amount")
}

fn pool_remove_liquidity_proportional_accounting_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "remove_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if mutate_field_transition_status(pattern, type_layouts) != "checked-runtime" {
        return false;
    }
    let sources = amm_u64_sources(body);
    let amount_a = "((receipt.lp_amount*pool.reserve_a)/pool.total_lp)";
    let amount_b = "((receipt.lp_amount*pool.reserve_b)/pool.total_lp)";
    let Some(reserve_a_delta) = mutate_transition_operand(pattern, "reserve_a", ir::MutateTransitionOp::Sub) else {
        return false;
    };
    let Some(reserve_b_delta) = mutate_transition_operand(pattern, "reserve_b", ir::MutateTransitionOp::Sub) else {
        return false;
    };
    let Some(total_lp_delta) = mutate_transition_operand(pattern, "total_lp", ir::MutateTransitionOp::Sub) else {
        return false;
    };
    if amm_u64_source(reserve_a_delta, &sources).as_deref() != Some(amount_a)
        || amm_u64_source(reserve_b_delta, &sources).as_deref() != Some(amount_b)
        || amm_u64_source(total_lp_delta, &sources).as_deref() != Some("receipt.lp_amount")
    {
        return false;
    }
    let token_amount_sources = body
        .create_set
        .iter()
        .filter(|candidate| candidate.ty == "Token" && metadata_can_verify_create_output_fields(candidate, type_layouts, availability))
        .filter_map(|candidate| create_pattern_field_operand(candidate, "amount"))
        .filter_map(|operand| amm_u64_source(operand, &sources))
        .collect::<BTreeSet<_>>();
    token_amount_sources.contains(amount_a) && token_amount_sources.contains(amount_b)
}

fn pool_remove_liquidity_admission_is_checked(
    name: &str,
    pattern: &ir::MutatePattern,
    body: &ir::IrBody,
    checked_invariant_guards: &[String],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "remove_liquidity" || pattern.ty != "Pool" {
        return false;
    }
    if !checked_invariant_guards.iter().any(|guard| guard == "lp-receipt-pool-id-match") {
        return false;
    }
    if consumed_input_pattern(body, "receipt").is_none() {
        return false;
    }
    if !mutate_preserved_field_is_verifier_coverable(pattern, "token_a_symbol", type_layouts)
        || !mutate_preserved_field_is_verifier_coverable(pattern, "token_b_symbol", type_layouts)
    {
        return false;
    }
    let token_symbol_count = body
        .create_set
        .iter()
        .filter(|candidate| candidate.ty == "Token" && metadata_can_verify_create_output_fields(candidate, type_layouts, availability))
        .filter(|candidate| create_pattern_field_operand(candidate, "symbol").is_some())
        .count();
    token_symbol_count >= 2
}

fn mutate_transition_operand<'a>(
    pattern: &'a ir::MutatePattern,
    field: &str,
    op: ir::MutateTransitionOp,
) -> Option<&'a ir::IrOperand> {
    pattern.transitions.iter().find_map(|transition| (transition.field == field && transition.op == op).then_some(&transition.operand))
}

fn amm_u64_sources(body: &ir::IrBody) -> HashMap<usize, String> {
    let mut sources = HashMap::new();
    let mut named_sources = HashMap::<String, String>::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } if dest.ty == ir::IrType::U64 => {
                    match (obj.name.as_str(), field.as_str()) {
                        ("input", "amount") => {
                            sources.insert(dest.id, "input.amount".to_string());
                        }
                        ("token_a", "amount") => {
                            sources.insert(dest.id, "token_a.amount".to_string());
                        }
                        ("token_b", "amount") => {
                            sources.insert(dest.id, "token_b.amount".to_string());
                        }
                        ("pool" | "pool_before", "reserve_a") => {
                            sources.insert(dest.id, pool_before_field_source("reserve_a"));
                        }
                        ("pool" | "pool_before", "reserve_b") => {
                            sources.insert(dest.id, pool_before_field_source("reserve_b"));
                        }
                        ("pool" | "pool_before", "total_lp") => {
                            sources.insert(dest.id, pool_before_field_source("total_lp"));
                        }
                        ("receipt", "lp_amount") => {
                            sources.insert(dest.id, "receipt.lp_amount".to_string());
                        }
                        _ => {}
                    }
                }
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field }
                    if matches!(obj.name.as_str(), "pool" | "pool_before") && field == "fee_rate_bps" =>
                {
                    sources.insert(dest.id, pool_before_field_source("fee_rate_bps"));
                }
                ir::IrInstruction::Binary { dest, op, left, right } if dest.ty == ir::IrType::U64 => {
                    let Some(left) = amm_u64_source(left, &sources) else {
                        continue;
                    };
                    let Some(right) = amm_u64_source(right, &sources) else {
                        continue;
                    };
                    let op = match op {
                        ast::BinaryOp::Add => "+",
                        ast::BinaryOp::Sub => "-",
                        ast::BinaryOp::Mul => "*",
                        ast::BinaryOp::Div => "/",
                        _ => continue,
                    };
                    sources.insert(dest.id, format!("({left}{op}{right})"));
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args }
                    if dest.ty == ir::IrType::U64 && func.function_symbol().is_some_and(is_min_call) && args.len() == 2 =>
                {
                    let Some(left) = amm_u64_source(&args[0], &sources) else {
                        continue;
                    };
                    let Some(right) = amm_u64_source(&args[1], &sources) else {
                        continue;
                    };
                    sources.insert(dest.id, format!("min({left},{right})"));
                }
                ir::IrInstruction::Move { dest, src } if dest.ty == ir::IrType::U64 => {
                    if let Some(source) = amm_u64_source(src, &sources) {
                        sources.insert(dest.id, source);
                    }
                }
                ir::IrInstruction::StoreVar { name, src } => {
                    if let Some(source) = amm_u64_source(src, &sources) {
                        named_sources.insert(name.clone(), source);
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } if dest.ty == ir::IrType::U64 => {
                    if let Some(source) = named_sources.get(name).cloned() {
                        sources.insert(dest.id, source);
                    }
                }
                _ => {}
            }
        }
    }
    sources
}

fn amm_u64_source(operand: &ir::IrOperand, sources: &HashMap<usize, String>) -> Option<String> {
    match operand {
        ir::IrOperand::Var(var) => sources.get(&var.id).cloned(),
        ir::IrOperand::Const(ir::IrConst::U64(value)) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn is_min_call(func: &str) -> bool {
    func == "min" || func.ends_with("::min")
}

fn pool_seed_token_pair_symbol_admission_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    checked_invariant_guards: &[String],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "seed_pool" || pool_pattern.ty != "Pool" {
        return false;
    }
    if !checked_invariant_guards.iter().any(|guard| guard == "token-pair-distinct") {
        return false;
    }
    if !metadata_can_verify_create_output_fields(pool_pattern, type_layouts, availability) {
        return false;
    }

    let Some(token_a_symbol) = create_pattern_field_operand(pool_pattern, "token_a_symbol") else {
        return false;
    };
    let Some(token_b_symbol) = create_pattern_field_operand(pool_pattern, "token_b_symbol") else {
        return false;
    };
    metadata_fixed_value_available_with_width(token_a_symbol, availability, 8)
        && metadata_fixed_value_available_with_width(token_b_symbol, availability, 8)
        && operand_var_name(token_a_symbol) == Some("token_a_symbol")
        && operand_var_name(token_b_symbol) == Some("token_b_symbol")
        && !ir_operands_same_verifier_source(token_a_symbol, token_b_symbol)
}

fn pool_launch_token_pair_symbol_admission_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "launch_token" || pool_pattern.ty != "Pool" {
        return false;
    }
    let Some(token_a_symbol) = create_pattern_field_operand(pool_pattern, "token_a_symbol") else {
        return false;
    };
    let Some(token_b_symbol) = create_pattern_field_operand(pool_pattern, "token_b_symbol") else {
        return false;
    };
    create_field_matches_param(pool_pattern, "token_a_symbol", "symbol", params, type_layouts, availability, 8)
        && metadata_fixed_value_available_with_width(token_b_symbol, availability, 8)
        && metadata_asserted_field_inequality(body, token_a_symbol, token_b_symbol)
}

fn pool_launch_token_identity_admission_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "launch_token" || pool_pattern.ty != "Pool" {
        return false;
    }
    create_field_matches_param(pool_pattern, "token_a_symbol", "symbol", params, type_layouts, availability, 8)
        && param_field_has_fixed_width(params, "pool_paired_token", "symbol", type_layouts, 8)
        && consumed_input_pattern(body, "pool_paired_token").is_some()
}

pub(super) fn pool_launch_token_lp_supply_invariant_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "launch_token" || pool_pattern.ty != "Pool" {
        return false;
    }
    if !metadata_can_verify_create_output_fields(pool_pattern, type_layouts, availability) {
        return false;
    }
    let Some(pool_total_lp) = create_pattern_field_operand(pool_pattern, "total_lp") else {
        return false;
    };
    body.create_set.iter().any(|candidate| {
        candidate.ty == "LPReceipt"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "lp_amount")
                .is_some_and(|receipt_lp| ir_operands_same_verifier_source(pool_total_lp, receipt_lp))
    })
}

fn pool_seed_token_pair_identity_admission_is_checked(name: &str, body: &ir::IrBody, pool_pattern: &ir::CreatePattern) -> bool {
    if name != "seed_pool" || pool_pattern.ty != "Pool" {
        return false;
    }
    consumed_input_pattern(body, "token_a").is_some()
        && consumed_input_pattern(body, "token_b").is_some()
        && body_has_asserted_type_hash_inequality(body, "token_a", "token_b")
}

fn body_has_asserted_type_hash_inequality(body: &ir::IrBody, left_binding: &str, right_binding: &str) -> bool {
    let assertion_failed_code = runtime_errors::CellScriptRuntimeError::AssertionFailed.code();
    let assert_fail_blocks = body
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator,
                ir::IrTerminator::Return(Some(ir::IrOperand::Const(ir::IrConst::U64(code)))) if code == assertion_failed_code
            )
        })
        .map(|block| block.id)
        .collect::<HashSet<_>>();
    let mut type_hash_sources = HashMap::<usize, String>::new();
    let mut named_type_hash_sources = HashMap::<String, String>::new();
    let mut checked_inequality_vars = HashSet::<usize>::new();

    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::TypeHash { dest, operand: ir::IrOperand::Var(var) } => {
                    if var.name == left_binding || var.name == right_binding {
                        type_hash_sources.insert(dest.id, var.name.clone());
                    }
                }
                ir::IrInstruction::Move { dest, src } => {
                    if let Some(source) = type_hash_source(src, &type_hash_sources) {
                        type_hash_sources.insert(dest.id, source);
                    }
                }
                ir::IrInstruction::StoreVar { name, src } => {
                    if let Some(source) = type_hash_source(src, &type_hash_sources) {
                        named_type_hash_sources.insert(name.clone(), source);
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } => {
                    if let Some(source) = named_type_hash_sources.get(name).cloned() {
                        type_hash_sources.insert(dest.id, source);
                    }
                }
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Ne, left, right } => {
                    let Some(left_source) = type_hash_source(left, &type_hash_sources) else {
                        continue;
                    };
                    let Some(right_source) = type_hash_source(right, &type_hash_sources) else {
                        continue;
                    };
                    if type_hash_sources_are_pair(&left_source, &right_source, left_binding, right_binding) {
                        checked_inequality_vars.insert(dest.id);
                    }
                }
                _ => {}
            }
        }
    }

    body.blocks.iter().any(|block| {
        let ir::IrTerminator::Branch { cond: ir::IrOperand::Var(cond), else_block, .. } = &block.terminator else {
            return false;
        };
        checked_inequality_vars.contains(&cond.id) && assert_fail_blocks.contains(else_block)
    })
}

fn metadata_asserted_field_inequality(body: &ir::IrBody, left: &ir::IrOperand, right: &ir::IrOperand) -> bool {
    let ir::IrOperand::Var(left) = left else {
        return false;
    };
    let ir::IrOperand::Var(right) = right else {
        return false;
    };
    let assertion_failed_code = runtime_errors::CellScriptRuntimeError::AssertionFailed.code();
    let assert_fail_blocks = body
        .blocks
        .iter()
        .filter(|block| match &block.terminator {
            ir::IrTerminator::Return(Some(ir::IrOperand::Const(ir::IrConst::U64(code)))) => *code == assertion_failed_code,
            _ => false,
        })
        .map(|block| block.id)
        .collect::<HashSet<_>>();
    let mut inequality_vars = HashSet::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Ne, left: lhs, right: rhs }
                    if (operand_matches_var_id(lhs, left.id) && operand_matches_var_id(rhs, right.id))
                        || (operand_matches_var_id(lhs, right.id) && operand_matches_var_id(rhs, left.id)) =>
                {
                    inequality_vars.insert(dest.id);
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } if inequality_vars.contains(&src.id) => {
                    inequality_vars.insert(dest.id);
                }
                _ => {}
            }
        }
    }

    body.blocks.iter().any(|block| {
        let ir::IrTerminator::Branch { cond: ir::IrOperand::Var(cond), else_block, .. } = &block.terminator else {
            return false;
        };
        inequality_vars.contains(&cond.id) && assert_fail_blocks.contains(else_block)
    })
}

fn type_hash_source(operand: &ir::IrOperand, type_hash_sources: &HashMap<usize, String>) -> Option<String> {
    let ir::IrOperand::Var(var) = operand else {
        return None;
    };
    type_hash_sources.get(&var.id).cloned()
}

fn type_hash_sources_are_pair(left: &str, right: &str, left_binding: &str, right_binding: &str) -> bool {
    (left == left_binding && right == right_binding) || (left == right_binding && right == left_binding)
}

fn pool_seed_lp_supply_invariant_is_checked(
    name: &str,
    pool_pattern: &ir::CreatePattern,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if name != "seed_pool" || pool_pattern.ty != "Pool" {
        return false;
    }
    if !metadata_can_verify_create_output_fields(pool_pattern, type_layouts, availability) {
        return false;
    }

    let Some(pool_total_lp) = create_pattern_field_operand(pool_pattern, "total_lp") else {
        return false;
    };
    if !metadata_fixed_value_available_with_width(pool_total_lp, availability, 8) {
        return false;
    }

    body.create_set.iter().any(|candidate| {
        candidate.ty == "LPReceipt"
            && metadata_can_verify_create_output_fields(candidate, type_layouts, availability)
            && create_pattern_field_operand(candidate, "lp_amount").is_some_and(|receipt_lp| {
                metadata_fixed_value_available_with_width(receipt_lp, availability, 8)
                    && ir_operands_same_verifier_source(pool_total_lp, receipt_lp)
            })
    })
}

fn pool_launch_token_atomicity_checked_components(
    name: &str,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> Vec<String> {
    if name != "launch_token" {
        return Vec::new();
    }

    let mut components = Vec::new();
    let Some((_, auth_pattern)) = created_output_pattern(body, "create_MintAuthority", "MintAuthority") else {
        return components;
    };
    let Some((_, seed_pattern)) = last_created_output_pattern(body, "Token") else {
        return components;
    };

    if create_field_matches_param(auth_pattern, "minted", "initial_mint", params, type_layouts, availability, 8) {
        components.push("launch-pool-atomicity:minted-equals-initial-mint=checked-runtime".to_string());
    }
    if create_field_matches_param(seed_pattern, "amount", "pool_seed_amount", params, type_layouts, availability, 8) {
        components.push("launch-pool-atomicity:seed-token-amount=checked-runtime".to_string());
    }
    if create_field_matches_param(auth_pattern, "token_symbol", "symbol", params, type_layouts, availability, 8)
        && create_field_matches_param(seed_pattern, "symbol", "symbol", params, type_layouts, availability, 8)
    {
        components.push("launch-pool-atomicity:symbol-consistency=checked-runtime".to_string());
    }
    if launch_distribution_sum_coupling_is_checked(body, params, availability) {
        components.push("launch-pool-atomicity:distribution-sum-plus-seed-lte-initial-mint=checked-runtime".to_string());
    }

    components
}

fn pool_launch_token_callee_admission_checked_components(
    name: &str,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    func: &str,
    args: &[ir::IrOperand],
) -> Vec<String> {
    if name != "launch_token" || !is_seed_pool_call(func) || args.len() < 3 {
        return Vec::new();
    }

    let mut components = Vec::new();
    let Some((_, seed_pattern)) = last_created_output_pattern(body, "Token") else {
        return components;
    };

    if last_created_output_var_id(body, "Token").is_some_and(|seed_var_id| operand_matches_var_id(&args[0], seed_var_id))
        && create_field_matches_param(seed_pattern, "symbol", "symbol", params, type_layouts, availability, 8)
    {
        components.push("callee-pool-admission:seed-token-symbol-handoff=checked-runtime".to_string());
    }

    if operand_matches_param(&args[1], params, "pool_paired_token")
        && param_field_has_fixed_width(params, "pool_paired_token", "symbol", type_layouts, 8)
    {
        components.push("callee-pool-admission:paired-token-symbol-handoff=checked-runtime".to_string());
    }

    if operand_matches_param(&args[2], params, "fee_rate_bps") && metadata_fixed_value_available_with_width(&args[2], availability, 2)
    {
        components.push("callee-pool-admission:fee-bound-handoff=checked-runtime".to_string());
    }

    components
}

fn pool_launch_token_pool_id_continuity_checked_components(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    func: &str,
    dest: &ir::IrVar,
) -> Vec<String> {
    if name != "launch_token" || !is_seed_pool_call(func) {
        return Vec::new();
    }
    let ir::IrType::Tuple(items) = &dest.ty else {
        return Vec::new();
    };
    if !matches!(items.first(), Some(ir::IrType::Named(type_name)) if type_name == "Pool")
        || !matches!(items.get(1), Some(ir::IrType::Named(type_name)) if type_name == "LPReceipt")
    {
        return Vec::new();
    }

    let mut components = Vec::new();
    let pool_projected = tuple_return_field_is_projected(body, dest.id, "0");
    let lp_receipt_projected = tuple_return_field_is_projected(body, dest.id, "1");
    if pool_projected && lp_receipt_projected {
        components.push("pool-id-continuity:tuple-return-projection=checked-runtime".to_string());
    }
    if pool_projected {
        components.push("pool-id-continuity:pool-type-hash-return-abi=checked-runtime".to_string());
    }
    if lp_receipt_projected && type_field_has_fixed_width(type_layouts, "LPReceipt", "pool_id", 32) {
        components.push("pool-id-continuity:lp-receipt-pool-id-return-abi=checked-runtime".to_string());
    }
    if pool_launch_token_pool_id_continuity_equality_is_checked(name, body, type_layouts, func, dest) {
        components.push("pool-id-continuity:callee-output-field-equality=checked-runtime".to_string());
    }

    components
}

fn pool_launch_token_pool_id_continuity_equality_is_checked(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    func: &str,
    dest: &ir::IrVar,
) -> bool {
    if name != "launch_token" || !is_seed_pool_call(func) {
        return false;
    }
    let ir::IrType::Tuple(items) = &dest.ty else {
        return false;
    };
    if !matches!(items.first(), Some(ir::IrType::Named(type_name)) if type_name == "Pool")
        || !matches!(items.get(1), Some(ir::IrType::Named(type_name)) if type_name == "LPReceipt")
    {
        return false;
    }

    tuple_return_field_is_projected(body, dest.id, "0")
        && tuple_return_field_is_projected(body, dest.id, "1")
        && type_field_has_fixed_width(type_layouts, "LPReceipt", "pool_id", 32)
}

fn is_seed_pool_call(func: &str) -> bool {
    func == "seed_pool" || func.ends_with("::seed_pool")
}

#[derive(Debug, Clone, Copy)]
enum LaunchAggregateSource {
    DistributionArray { len: usize },
    DistributionElement { index: usize },
}

pub(super) fn launch_distribution_sum_coupling_is_checked(
    body: &ir::IrBody,
    params: &[ir::IrParam],
    availability: &MetadataPreludeAvailability,
) -> bool {
    let Some(distribution_param) = params.iter().find(|param| param.name == "distribution") else {
        return false;
    };
    let Some(distribution_len) = launch_distribution_amount_count(&distribution_param.ty) else {
        return false;
    };
    if !availability.aggregate_pointer_vars.contains_key(&distribution_param.binding.id) {
        return false;
    }

    let mut required_left_sources = (0..distribution_len).map(launch_distribution_amount_source).collect::<BTreeSet<_>>();
    required_left_sources.insert(launch_param_source("pool_seed_amount"));
    let initial_mint_sources = BTreeSet::from([launch_param_source("initial_mint")]);

    let mut aggregate_sources = HashMap::new();
    aggregate_sources.insert(distribution_param.binding.id, LaunchAggregateSource::DistributionArray { len: distribution_len });

    let mut u64_sources = HashMap::new();
    for param in params {
        if matches!(param.name.as_str(), "initial_mint" | "pool_seed_amount")
            && param.ty == ir::IrType::U64
            && availability.u64_operand_vars.contains(&param.binding.id)
        {
            u64_sources.insert(param.binding.id, BTreeSet::from([launch_param_source(&param.name)]));
        }
    }

    let mut checked_bool_vars = HashSet::new();
    let mut named_u64_sources = HashMap::new();
    let mut named_bool_sources = HashSet::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Index { dest, arr: ir::IrOperand::Var(arr), idx } => {
                    let Some(index) = const_usize_operand(idx) else {
                        continue;
                    };
                    let Some(LaunchAggregateSource::DistributionArray { len }) = aggregate_sources.get(&arr.id).copied() else {
                        continue;
                    };
                    if index < len {
                        aggregate_sources.insert(dest.id, LaunchAggregateSource::DistributionElement { index });
                    }
                }
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } => {
                    let Some(LaunchAggregateSource::DistributionElement { index }) = aggregate_sources.get(&obj.id).copied() else {
                        continue;
                    };
                    if field == "1" && dest.ty == ir::IrType::U64 && availability.u64_value_vars.contains(&dest.id) {
                        u64_sources.insert(dest.id, BTreeSet::from([launch_distribution_amount_source(index)]));
                    }
                }
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Add, left, right } if dest.ty == ir::IrType::U64 => {
                    let Some(mut sources) = launch_u64_sources(left, &u64_sources) else {
                        continue;
                    };
                    let Some(right_sources) = launch_u64_sources(right, &u64_sources) else {
                        continue;
                    };
                    sources.extend(right_sources);
                    u64_sources.insert(dest.id, sources);
                }
                ir::IrInstruction::Binary { dest, op, left, right } if dest.ty == ir::IrType::Bool => {
                    let Some(left_sources) = launch_u64_sources(left, &u64_sources) else {
                        continue;
                    };
                    let Some(right_sources) = launch_u64_sources(right, &u64_sources) else {
                        continue;
                    };
                    if launch_distribution_allocation_comparison_is_checked(
                        *op,
                        &left_sources,
                        &right_sources,
                        &required_left_sources,
                        &initial_mint_sources,
                    ) {
                        checked_bool_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Move { dest, src } if dest.ty == ir::IrType::U64 => {
                    if let Some(sources) = launch_u64_sources(src, &u64_sources) {
                        u64_sources.insert(dest.id, sources);
                    }
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } if dest.ty == ir::IrType::Bool => {
                    if checked_bool_vars.contains(&src.id) {
                        checked_bool_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::StoreVar { name, src } => {
                    if let Some(sources) = launch_u64_sources(src, &u64_sources) {
                        named_u64_sources.insert(name.clone(), sources);
                    }
                    if matches!(src, ir::IrOperand::Var(var) if checked_bool_vars.contains(&var.id)) {
                        named_bool_sources.insert(name.clone());
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } if dest.ty == ir::IrType::U64 => {
                    if let Some(sources) = named_u64_sources.get(name).cloned() {
                        u64_sources.insert(dest.id, sources);
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } if dest.ty == ir::IrType::Bool && named_bool_sources.contains(name) => {
                    checked_bool_vars.insert(dest.id);
                }
                _ => {}
            }
        }
    }

    body.blocks.iter().any(|block| match &block.terminator {
        ir::IrTerminator::Branch { cond: ir::IrOperand::Var(cond), .. } => checked_bool_vars.contains(&cond.id),
        _ => false,
    })
}

pub(super) fn launch_distribution_amount_count(ty: &ir::IrType) -> Option<usize> {
    let ir::IrType::Array(inner, len) = ty else {
        return None;
    };
    let ir::IrType::Tuple(items) = inner.as_ref() else {
        return None;
    };
    if items.len() == 2 && items.get(1) == Some(&ir::IrType::U64) && type_static_length(inner.as_ref()).is_some() {
        Some(*len)
    } else {
        None
    }
}

fn launch_u64_sources(operand: &ir::IrOperand, sources: &HashMap<usize, BTreeSet<String>>) -> Option<BTreeSet<String>> {
    match operand {
        ir::IrOperand::Var(var) => sources.get(&var.id).cloned(),
        ir::IrOperand::Const(ir::IrConst::U64(_)) => Some(BTreeSet::new()),
        _ => None,
    }
}

fn launch_distribution_allocation_comparison_is_checked(
    op: ast::BinaryOp,
    left_sources: &BTreeSet<String>,
    right_sources: &BTreeSet<String>,
    required_left_sources: &BTreeSet<String>,
    initial_mint_sources: &BTreeSet<String>,
) -> bool {
    match op {
        ast::BinaryOp::Le => left_sources == required_left_sources && right_sources == initial_mint_sources,
        ast::BinaryOp::Ge => left_sources == initial_mint_sources && right_sources == required_left_sources,
        _ => false,
    }
}

fn launch_param_source(name: &str) -> String {
    format!("param:{name}")
}

fn launch_distribution_amount_source(index: usize) -> String {
    format!("distribution[{index}].1")
}

pub(super) fn create_field_matches_param(
    pattern: &ir::CreatePattern,
    field: &str,
    param_name: &str,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    byte_len: usize,
) -> bool {
    if !metadata_can_verify_create_output_fields(pattern, type_layouts, availability) {
        return false;
    }
    let Some(field_operand) = create_pattern_field_operand(pattern, field) else {
        return false;
    };
    let Some(param_operand) = param_operand(params, param_name) else {
        return false;
    };

    metadata_fixed_value_available_with_width(field_operand, availability, byte_len)
        && metadata_fixed_value_available_with_width(&param_operand, availability, byte_len)
        && ir_operands_same_verifier_source(field_operand, &param_operand)
}

fn operand_matches_param(operand: &ir::IrOperand, params: &[ir::IrParam], param_name: &str) -> bool {
    param_operand(params, param_name).is_some_and(|param_operand| ir_operands_same_verifier_source(operand, &param_operand))
}

fn operand_matches_var_id(operand: &ir::IrOperand, var_id: usize) -> bool {
    matches!(operand, ir::IrOperand::Var(var) if var.id == var_id)
}

fn param_field_has_fixed_width(
    params: &[ir::IrParam],
    param_name: &str,
    field: &str,
    type_layouts: &MetadataTypeLayouts,
    expected_width: usize,
) -> bool {
    let Some(param) = params.iter().find(|param| param.name == param_name) else {
        return false;
    };
    let Some(type_name) = named_type_name(&param.ty) else {
        return false;
    };
    type_field_has_fixed_width(type_layouts, type_name, field, expected_width)
}

fn type_field_has_fixed_width(type_layouts: &MetadataTypeLayouts, type_name: &str, field: &str, expected_width: usize) -> bool {
    type_layouts
        .get(type_name)
        .and_then(|fields| fields.get(field))
        .and_then(|layout| metadata_layout_or_named_fixed_byte_width(layout, type_layouts))
        .is_some_and(|width| width == expected_width)
}

fn tuple_return_field_is_projected(body: &ir::IrBody, tuple_var_id: usize, field_name: &str) -> bool {
    body.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| {
        matches!(
            instruction,
            ir::IrInstruction::FieldAccess {
                obj: ir::IrOperand::Var(obj),
                field,
                ..
            } if obj.id == tuple_var_id && field == field_name
        )
    })
}

fn param_operand(params: &[ir::IrParam], name: &str) -> Option<ir::IrOperand> {
    params.iter().find(|param| param.name == name).map(|param| ir::IrOperand::Var(param.binding.clone()))
}

fn create_pattern_field_operand<'a>(pattern: &'a ir::CreatePattern, name: &str) -> Option<&'a ir::IrOperand> {
    pattern.fields.iter().find_map(|(field, operand)| (field == name).then_some(operand))
}

pub(super) fn operand_var_name(operand: &ir::IrOperand) -> Option<&str> {
    match operand {
        ir::IrOperand::Var(var) => Some(var.name.as_str()),
        ir::IrOperand::Const(_) => None,
    }
}

pub(super) fn ir_operands_same_verifier_source(left: &ir::IrOperand, right: &ir::IrOperand) -> bool {
    match (left, right) {
        (ir::IrOperand::Var(left), ir::IrOperand::Var(right)) => left.id == right.id,
        (ir::IrOperand::Const(left), ir::IrOperand::Const(right)) => ir_consts_equal(left, right),
        _ => false,
    }
}

fn ir_consts_equal(left: &ir::IrConst, right: &ir::IrConst) -> bool {
    match (left, right) {
        (ir::IrConst::Unit, ir::IrConst::Unit) => true,
        (ir::IrConst::U8(left), ir::IrConst::U8(right)) => left == right,
        (ir::IrConst::U16(left), ir::IrConst::U16(right)) => left == right,
        (ir::IrConst::U32(left), ir::IrConst::U32(right)) => left == right,
        (ir::IrConst::U64(left), ir::IrConst::U64(right)) => left == right,
        (ir::IrConst::U128(left), ir::IrConst::U128(right)) => left == right,
        (ir::IrConst::Bool(left), ir::IrConst::Bool(right)) => left == right,
        (ir::IrConst::Address(left), ir::IrConst::Address(right)) => left == right,
        (ir::IrConst::Hash(left), ir::IrConst::Hash(right)) => left == right,
        (ir::IrConst::Array(left), ir::IrConst::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right.iter()).all(|(left, right)| ir_consts_equal(left, right))
        }
        _ => false,
    }
}

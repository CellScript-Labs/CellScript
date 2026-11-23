use super::pool_metadata::{
    create_field_matches_param, create_pattern_field_is_alias, create_pattern_field_is_type_hash_of, ir_operands_same_verifier_source,
    launch_distribution_amount_count, launch_distribution_sum_coupling_is_checked, output_pool_pattern,
    pool_checked_invariant_guard_names, pool_launch_token_lp_supply_invariant_is_checked, pool_output_param_admission_is_checked,
    pool_output_param_constant_product_pricing_is_checked, pool_output_param_fee_accounting_is_checked,
    pool_swap_a_for_b_admission_is_checked, pool_swap_a_for_b_constant_product_pricing_is_checked,
    pool_swap_a_for_b_fee_accounting_is_checked,
};
use super::pool_transaction_requirements::body_assert_invariant_count;
use super::runtime_availability::{
    metadata_can_verify_create_output_fields, metadata_can_verify_output_lock, metadata_layout_fixed_scalar_width,
    MetadataPreludeAvailability,
};
use super::schema_metadata::named_type_name;
use super::verifier_obligations::{MetadataFieldAlias, MetadataU64Source, TransactionResourceObligation};
use super::{ast, ir, MetadataTypeLayouts};
use std::collections::{BTreeSet, HashMap};

pub(super) fn body_resource_conservation_obligations(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    params: &[ir::IrParam],
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> Vec<TransactionResourceObligation> {
    let resource_param_types = params
        .iter()
        .filter_map(|param| {
            let type_name = named_type_name(&param.ty)?;
            (cell_type_kinds.get(type_name) == Some(&ir::IrTypeKind::Resource)).then_some((param.binding.id, type_name.to_string()))
        })
        .collect::<HashMap<_, _>>();
    if resource_param_types.is_empty() {
        return Vec::new();
    }

    let mut consumed_params: HashMap<String, Vec<ir::IrVar>> = HashMap::new();
    let mut created_outputs: HashMap<String, Vec<ir::CreatePattern>> = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Consume { operand: ir::IrOperand::Var(var) } => {
                    let type_name = resource_param_types.get(&var.id).cloned().or_else(|| {
                        named_type_name(&var.ty)
                            .filter(|type_name| cell_type_kinds.get(*type_name) == Some(&ir::IrTypeKind::Resource))
                            .map(str::to_string)
                    });
                    if let Some(type_name) = type_name {
                        consumed_params.entry(type_name).or_default().push(var.clone());
                    }
                }
                ir::IrInstruction::Create { pattern, .. }
                    if matches!(pattern.operation.as_str(), "create" | "output")
                        && cell_type_kinds.get(&pattern.ty) == Some(&ir::IrTypeKind::Resource) =>
                {
                    created_outputs.entry(pattern.ty.clone()).or_default().push(pattern.clone());
                }
                _ => {}
            }
        }
    }
    for pattern in &body.consume_set {
        if pattern.operation != "consume" {
            continue;
        }
        let Some(param) = params.iter().find(|param| param.name == pattern.binding) else {
            continue;
        };
        let Some(type_name) = named_type_name(&param.ty) else {
            continue;
        };
        if cell_type_kinds.get(type_name) != Some(&ir::IrTypeKind::Resource) {
            continue;
        }
        let entry = consumed_params.entry(type_name.to_string()).or_default();
        if !entry.iter().any(|var| var.id == param.binding.id || var.name == param.binding.name) {
            entry.push(param.binding.clone());
        }
    }
    for pattern in &body.create_set {
        if !matches!(pattern.operation.as_str(), "create" | "output")
            || cell_type_kinds.get(&pattern.ty) != Some(&ir::IrTypeKind::Resource)
        {
            continue;
        }
        let entry = created_outputs.entry(pattern.ty.clone()).or_default();
        if !entry.iter().any(|candidate| candidate.binding == pattern.binding) {
            entry.push(pattern.clone());
        }
    }

    let type_names = consumed_params.keys().chain(created_outputs.keys()).cloned().collect::<BTreeSet<_>>();
    type_names
        .iter()
        .filter_map(|type_name| {
            let consumed = consumed_params.get(type_name).cloned().unwrap_or_default();
            let created = created_outputs.get(type_name).cloned().unwrap_or_default();
            if consumed.is_empty() || created.is_empty() {
                return None;
            }
            let fields = type_layouts
                .get(type_name)
                .map(|layouts| layouts.keys().cloned().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "<unknown>".to_string());
            let checked_detail =
                resource_conservation_checked_detail(name, body, type_layouts, availability, params, type_name, &consumed, &created, &fields);
            Some(TransactionResourceObligation {
                category: "transaction-invariant",
                feature: format!("resource-conservation:{}", type_name),
                status: if checked_detail.is_some() { "checked-runtime" } else { "runtime-required" },
                detail: checked_detail.unwrap_or_else(|| {
                    format!(
                        "Runtime verifier must prove '{}' resource conservation across {} consumed Input cell(s) and {} created Output cell(s); resource-conservation=runtime-required",
                        type_name,
                        consumed.len(),
                        created.len()
                    )
                }),
            })
        })
        .collect()
}

fn resource_conservation_checked_detail(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    params: &[ir::IrParam],
    type_name: &str,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
    fields: &str,
) -> Option<String> {
    if consumed.len() == 1
        && created.len() == 1
        && resource_conservation_pair_is_checked(body, type_layouts, availability, &consumed[0], &created[0])
    {
        return Some(format!(
            "Compiler-emitted runtime verifier checks one consumed '{}' Input is preserved into one created Output; resource-conservation=checked-runtime; fields: {}",
            type_name, fields
        ));
    }

    if resource_conservation_updated_successor_is_checked(body, type_layouts, availability, consumed, created) {
        return Some(format!(
            "Compiler-emitted runtime verifier checks one consumed '{}' Input is preserved into one created Output with verifier-checked field updates derived from the consumed Input or fixed by explicit constants; resource-conservation=checked-runtime; fields: {}",
            type_name, fields
        ));
    }

    if resource_conservation_pairs_are_checked(body, type_layouts, availability, consumed, created) {
        return Some(format!(
            "Compiler-emitted runtime verifier checks {} consumed '{}' Inputs are preserved into {} paired Outputs; resource-conservation=checked-runtime; fields: {}",
            consumed.len(),
            type_name,
            created.len(),
            fields
        ));
    }

    if resource_conservation_amount_merge_is_checked(body, type_layouts, availability, consumed, created) {
        return Some(format!(
            "Compiler-emitted runtime verifier checks {} consumed '{}' Inputs are merged into one created Output by a verifier-recomputed u64 amount sum; resource-conservation=checked-runtime; fields: amount",
            consumed.len(),
            type_name
        ));
    }

    if resource_conservation_amount_split_is_checked(body, type_layouts, availability, consumed, created) {
        return Some(format!(
            "Compiler-emitted runtime verifier checks one consumed '{}' Input is split across {} created Outputs by a verifier-recomputed u64 amount subtraction with matching split outputs; resource-conservation=checked-runtime; fields: amount",
            type_name,
            created.len()
        ));
    }

    if resource_conservation_amm_swap_is_checked(name, body, type_layouts, availability, type_name, consumed, created) {
        return Some(format!(
            "Compiler-emitted AMM verifier checks one consumed '{}' input is exchanged for one created output through Pool symbol admission, fee accounting, and constant-product pricing; resource-conservation=checked-runtime; fields: amount, symbol",
            type_name
        ));
    }

    if resource_conservation_launch_token_is_checked(name, body, type_layouts, availability, params, type_name, consumed, created) {
        return Some(format!(
            "Compiler-emitted launch verifier checks '{}' issuance through MintAuthority minted supply, distribution plus pool-seed bounds, paired-token Pool reserve accounting, and verifier-covered created token outputs; resource-conservation=checked-runtime; fields: amount, symbol",
            type_name
        ));
    }

    None
}

fn resource_conservation_pair_is_checked(
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    consumed: &ir::IrVar,
    created: &ir::CreatePattern,
) -> bool {
    if !metadata_can_verify_create_output_fields(created, type_layouts, availability)
        || !metadata_can_verify_output_lock(created, availability)
    {
        return false;
    }
    let aliases = metadata_field_aliases(body);
    created.fields.iter().all(|(field, operand)| {
        let ir::IrOperand::Var(var) = operand else {
            return false;
        };
        aliases.get(&var.id).is_some_and(|alias| alias.root_id == consumed.id && alias.field == field.as_str())
    })
}

fn resource_conservation_updated_successor_is_checked(
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if consumed.len() != 1 || created.len() != 1 {
        return false;
    }
    let created = &created[0];
    if !metadata_can_verify_create_output_fields(created, type_layouts, availability)
        || !metadata_can_verify_output_lock(created, availability)
    {
        return false;
    }
    let aliases = metadata_field_aliases(body);
    let sources = metadata_u64_sources(body);
    let consumed_id = consumed[0].id;
    created.fields.iter().all(|(field, operand)| {
        // A literal initializer is checked directly against the created
        // output. It deliberately carries no predecessor provenance: schema
        // migrations use this shape for reviewed resets such as
        // `approval_nonce = 0`.
        let ir::IrOperand::Var(var) = operand else { return matches!(operand, ir::IrOperand::Const(_)) };
        // Preserved verbatim: a pure alias of the same field on the consumed
        // input.
        if aliases.get(&var.id).is_some_and(|alias| alias.root_id == consumed_id && alias.field == *field) {
            return true;
        }
        // Updated: a u64 value whose field provenance still roots entirely in
        // the consumed input (for example `note.amount + 1`), so the create's
        // field check verifies the update against the consumed input.
        if var.ty != ir::IrType::U64 {
            return false;
        }
        let Some(source) = sources.get(&var.id) else {
            return false;
        };
        let mut roots = Vec::new();
        metadata_u64_source_collect_field_roots(source, field, &mut roots)
            && !roots.is_empty()
            && roots.iter().all(|root| *root == consumed_id)
    })
}

fn resource_conservation_pairs_are_checked(
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if consumed.len() < 2 || consumed.len() != created.len() {
        return false;
    }
    let mut unmatched = consumed.iter().collect::<Vec<_>>();
    for output in created {
        let Some(index) =
            unmatched.iter().position(|input| resource_conservation_pair_is_checked(body, type_layouts, availability, input, output))
        else {
            return false;
        };
        unmatched.remove(index);
    }
    unmatched.is_empty()
}

fn resource_conservation_amount_merge_is_checked(
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if consumed.len() < 2 || created.len() != 1 {
        return false;
    }
    let created = &created[0];
    if !metadata_can_verify_create_output_fields(created, type_layouts, availability)
        || !metadata_can_verify_output_lock(created, availability)
        || !resource_conservation_has_u64_amount_field(type_layouts, &created.ty)
    {
        return false;
    }
    let Some((_, operand)) = created.fields.iter().find(|(field, _)| field == "amount") else {
        return false;
    };
    let ir::IrOperand::Var(var) = operand else {
        return false;
    };

    let sources = metadata_u64_sources(body);
    let Some(source) = sources.get(&var.id) else {
        return false;
    };
    let mut source_roots = Vec::new();
    if !metadata_u64_source_collect_field_roots(source, "amount", &mut source_roots) {
        return false;
    }
    source_roots.sort_unstable();
    let mut consumed_roots = consumed.iter().map(|var| var.id).collect::<Vec<_>>();
    consumed_roots.sort_unstable();
    source_roots == consumed_roots && resource_conservation_created_identity_fields_are_checked(body, consumed, created)
}

fn resource_conservation_amount_split_is_checked(
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if consumed.len() != 1 || created.len() < 2 {
        return false;
    }
    if !resource_conservation_has_single_u64_amount_field(type_layouts, &created[0].ty) {
        return false;
    }
    if !created.iter().all(|pattern| {
        metadata_can_verify_create_output_fields(pattern, type_layouts, availability)
            && metadata_can_verify_output_lock(pattern, availability)
            && resource_conservation_has_single_u64_amount_field(type_layouts, &pattern.ty)
            && pattern.fields.len() == 1
            && pattern.fields.first().is_some_and(|(field, _)| field == "amount")
    }) {
        return false;
    }

    let sources = metadata_u64_sources(body);
    let amount_operands = created.iter().filter_map(|pattern| pattern.fields.first().map(|(_, operand)| operand)).collect::<Vec<_>>();
    let mut split_remainders = Vec::new();
    for (index, operand) in amount_operands.iter().enumerate() {
        let Some(source) = metadata_u64_source_for_operand(operand, &sources) else {
            continue;
        };
        let Some(subtrahends) = metadata_u64_source_collect_amount_split_subtrahends(&source, consumed[0].id) else {
            continue;
        };
        split_remainders.push((index, subtrahends));
    }
    if split_remainders.len() != 1 {
        return false;
    }

    let (remainder_index, subtrahends) = split_remainders.remove(0);
    let mut unmatched_outputs = amount_operands
        .iter()
        .enumerate()
        .filter_map(|(index, operand)| (index != remainder_index).then_some((*operand).clone()))
        .collect::<Vec<_>>();
    if unmatched_outputs.len() != subtrahends.len() {
        return false;
    }
    for subtrahend in subtrahends {
        let Some(position) = unmatched_outputs.iter().position(|operand| ir_operands_same_verifier_source(operand, &subtrahend))
        else {
            return false;
        };
        unmatched_outputs.remove(position);
    }
    unmatched_outputs.is_empty()
}

fn resource_conservation_amm_swap_is_checked(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    type_name: &str,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if name != "swap_a_for_b" || type_name != "Token" || consumed.len() != 1 || created.len() != 1 {
        return false;
    }
    let created = &created[0];
    if created.ty != "Token"
        || !metadata_can_verify_create_output_fields(created, type_layouts, availability)
        || !metadata_can_verify_output_lock(created, availability)
    {
        return false;
    }
    if let Some(pool_pattern) = body.mutate_set.iter().find(|pattern| pattern.binding == "pool" && pattern.ty == "Pool") {
        return pool_swap_a_for_b_admission_is_checked(
            name,
            pool_pattern,
            body,
            &pool_checked_invariant_guard_names(name, "mutation-invariants", body_assert_invariant_count(body)),
            type_layouts,
            availability,
        ) && pool_swap_a_for_b_fee_accounting_is_checked(name, pool_pattern, body, type_layouts)
            && pool_swap_a_for_b_constant_product_pricing_is_checked(name, pool_pattern, body, type_layouts, availability);
    }

    let Some(pool_output) = output_pool_pattern(body) else {
        return false;
    };
    let admission = pool_output_param_admission_is_checked(name, pool_output, body, type_layouts, availability);
    let fee = pool_output_param_fee_accounting_is_checked(name, pool_output, body, type_layouts);
    let pricing = pool_output_param_constant_product_pricing_is_checked(name, pool_output, created, body, type_layouts, availability);
    admission && fee && pricing
}

#[allow(clippy::too_many_arguments)]
fn resource_conservation_launch_token_is_checked(
    name: &str,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
    params: &[ir::IrParam],
    type_name: &str,
    consumed: &[ir::IrVar],
    created: &[ir::CreatePattern],
) -> bool {
    if name != "launch_token"
        || type_name != "Token"
        || consumed.len() != 1
        || consumed.first().is_none_or(|var| var.name != "pool_paired_token")
    {
        return false;
    }
    let Some(distribution_param) = params.iter().find(|param| param.name == "distribution") else {
        return false;
    };
    let Some(distribution_len) = launch_distribution_amount_count(&distribution_param.ty) else {
        return false;
    };
    if created.len() != distribution_len + 1 {
        return false;
    }
    if !created.iter().all(|pattern| {
        pattern.ty == "Token"
            && matches!(pattern.operation.as_str(), "create" | "output")
            && metadata_can_verify_create_output_fields(pattern, type_layouts, availability)
            && metadata_can_verify_output_lock(pattern, availability)
            && create_field_matches_param(pattern, "symbol", "symbol", params, type_layouts, availability, 8)
    }) {
        return false;
    }
    if !launch_distribution_sum_coupling_is_checked(body, params, availability) {
        return false;
    }
    let Some((_, auth_pattern)) = body.create_set.iter().enumerate().find(|(_, pattern)| pattern.ty == "MintAuthority") else {
        return false;
    };
    if !create_field_matches_param(auth_pattern, "token_symbol", "symbol", params, type_layouts, availability, 8)
        || !create_field_matches_param(auth_pattern, "minted", "initial_mint", params, type_layouts, availability, 8)
    {
        return false;
    }
    let Some((_, pool_pattern)) =
        body.create_set.iter().enumerate().find(|(_, pattern)| pattern.ty == "Pool" && pattern.binding == "pool")
    else {
        return false;
    };
    if !create_field_matches_param(pool_pattern, "token_a_symbol", "symbol", params, type_layouts, availability, 8)
        || !create_field_matches_param(pool_pattern, "reserve_a", "pool_seed_amount", params, type_layouts, availability, 8)
        || !create_field_matches_param(pool_pattern, "fee_rate_bps", "fee_rate_bps", params, type_layouts, availability, 2)
        || !create_pattern_field_is_alias(body, pool_pattern, "token_b_symbol", "pool_paired_token", "symbol")
        || !create_pattern_field_is_alias(body, pool_pattern, "reserve_b", "pool_paired_token", "amount")
        || !create_pattern_field_is_type_hash_of(body, pool_pattern, "token_a_type", "dist0")
        || !create_pattern_field_is_type_hash_of(body, pool_pattern, "token_b_type", "pool_paired_token")
    {
        return false;
    }
    let Some((_, lp_pattern)) =
        body.create_set.iter().enumerate().find(|(_, pattern)| pattern.ty == "LPReceipt" && pattern.binding == "lp_receipt")
    else {
        return false;
    };
    metadata_can_verify_create_output_fields(lp_pattern, type_layouts, availability)
        && pool_launch_token_lp_supply_invariant_is_checked(name, pool_pattern, body, type_layouts, availability)
}

fn resource_conservation_has_single_u64_amount_field(type_layouts: &MetadataTypeLayouts, type_name: &str) -> bool {
    let Some(layouts) = type_layouts.get(type_name) else {
        return false;
    };
    if layouts.len() != 1 {
        return false;
    }
    layouts.get("amount").is_some_and(|layout| layout.ty == ir::IrType::U64 && metadata_layout_fixed_scalar_width(layout) == Some(8))
}

fn resource_conservation_has_u64_amount_field(type_layouts: &MetadataTypeLayouts, type_name: &str) -> bool {
    type_layouts
        .get(type_name)
        .and_then(|layouts| layouts.get("amount"))
        .is_some_and(|layout| layout.ty == ir::IrType::U64 && metadata_layout_fixed_scalar_width(layout) == Some(8))
}

fn resource_conservation_created_identity_fields_are_checked(
    body: &ir::IrBody,
    consumed: &[ir::IrVar],
    created: &ir::CreatePattern,
) -> bool {
    let aliases = metadata_field_aliases(body);
    let equalities = metadata_asserted_field_equalities(body, &aliases);
    let consumed_roots = consumed.iter().map(|var| var.id).collect::<BTreeSet<_>>();

    created.fields.iter().all(|(field, operand)| {
        if field == "amount" {
            return true;
        }
        let ir::IrOperand::Var(var) = operand else {
            return false;
        };
        let Some(alias) = aliases.get(&var.id) else {
            return false;
        };
        if alias.field != *field || !consumed_roots.contains(&alias.root_id) {
            return false;
        }
        consumed.iter().all(|root| {
            root.id == alias.root_id || equalities.contains(&canonical_metadata_field_equality(alias.root_id, field, root.id, field))
        })
    })
}

pub(super) fn metadata_asserted_field_equalities(
    body: &ir::IrBody,
    aliases: &HashMap<usize, MetadataFieldAlias>,
) -> BTreeSet<(usize, String, usize, String)> {
    let mut eq_by_var = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::Binary {
                dest,
                op: ast::BinaryOp::Eq,
                left: ir::IrOperand::Var(left),
                right: ir::IrOperand::Var(right),
            } = instruction
            {
                let Some(left_alias) = aliases.get(&left.id) else {
                    continue;
                };
                let Some(right_alias) = aliases.get(&right.id) else {
                    continue;
                };
                eq_by_var.insert(
                    dest.id,
                    canonical_metadata_field_equality(left_alias.root_id, &left_alias.field, right_alias.root_id, &right_alias.field),
                );
            }
        }
    }

    let mut asserted = BTreeSet::new();
    for block in &body.blocks {
        let ir::IrTerminator::Branch { cond: ir::IrOperand::Var(cond), else_block, .. } = &block.terminator else {
            continue;
        };
        if !block_returns_error(body, *else_block) {
            continue;
        }
        if let Some(equality) = eq_by_var.get(&cond.id) {
            asserted.insert(equality.clone());
        }
    }
    asserted
}

pub(super) fn canonical_metadata_field_equality(
    left_root: usize,
    left_field: &str,
    right_root: usize,
    right_field: &str,
) -> (usize, String, usize, String) {
    let left = (left_root, left_field.to_string());
    let right = (right_root, right_field.to_string());
    if left <= right {
        (left.0, left.1, right.0, right.1)
    } else {
        (right.0, right.1, left.0, left.1)
    }
}

pub(super) fn block_returns_error(body: &ir::IrBody, block_id: ir::BlockId) -> bool {
    body.blocks.iter().find(|block| block.id == block_id).is_some_and(
        |block| matches!(block.terminator, ir::IrTerminator::Return(Some(ir::IrOperand::Const(ir::IrConst::U64(code)))) if code != 0),
    )
}

pub(super) fn metadata_field_aliases(body: &ir::IrBody) -> HashMap<usize, MetadataFieldAlias> {
    let mut aliases = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } => {
                    let alias = aliases.get(&obj.id).cloned().map_or_else(
                        || MetadataFieldAlias { root_id: obj.id, root_name: obj.name.clone(), field: field.clone() },
                        |parent: MetadataFieldAlias| MetadataFieldAlias {
                            root_id: parent.root_id,
                            root_name: parent.root_name,
                            field: format!("{}.{}", parent.field, field),
                        },
                    );
                    aliases.insert(dest.id, alias);
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } => {
                    if let Some(alias) = aliases.get(&src.id).cloned() {
                        aliases.insert(dest.id, alias);
                    }
                }
                _ => {}
            }
        }
    }
    aliases
}

fn metadata_u64_sources(body: &ir::IrBody) -> HashMap<usize, MetadataU64Source> {
    let mut sources = HashMap::new();
    let aliases = metadata_field_aliases(body);
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } if dest.ty == ir::IrType::U64 => {
                    let alias = aliases.get(&dest.id).cloned().unwrap_or_else(|| MetadataFieldAlias {
                        root_id: obj.id,
                        root_name: obj.name.clone(),
                        field: field.clone(),
                    });
                    sources.insert(dest.id, MetadataU64Source::Field(alias));
                }
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Add, left, right } if dest.ty == ir::IrType::U64 => {
                    let left_source = metadata_u64_source_for_operand(left, &sources);
                    let right_source = metadata_u64_source_for_operand(right, &sources);
                    // A constant offset does not change field provenance, so it
                    // propagates the other side's source exactly like the
                    // opaque subtrahend of a subtraction.
                    match (left_source, right_source) {
                        (Some(left), Some(right)) => {
                            sources.insert(dest.id, MetadataU64Source::Add(Box::new(left), Box::new(right)));
                        }
                        (Some(left), None) if matches!(right, ir::IrOperand::Const(_)) => {
                            sources.insert(dest.id, left);
                        }
                        (None, Some(right)) if matches!(left, ir::IrOperand::Const(_)) => {
                            sources.insert(dest.id, right);
                        }
                        _ => {}
                    }
                }
                ir::IrInstruction::Binary { dest, op: ast::BinaryOp::Sub, left, right } if dest.ty == ir::IrType::U64 => {
                    if let Some(left) = metadata_u64_source_for_operand(left, &sources) {
                        sources.insert(dest.id, MetadataU64Source::Sub(Box::new(left), right.clone()));
                    }
                }
                ir::IrInstruction::Move { dest, src } if dest.ty == ir::IrType::U64 => {
                    if let Some(source) = metadata_u64_source_for_operand(src, &sources) {
                        sources.insert(dest.id, source);
                    }
                }
                _ => {}
            }
        }
    }
    sources
}

fn metadata_u64_source_for_operand(operand: &ir::IrOperand, sources: &HashMap<usize, MetadataU64Source>) -> Option<MetadataU64Source> {
    match operand {
        ir::IrOperand::Var(var) => sources.get(&var.id).cloned(),
        ir::IrOperand::Const(_) => None,
    }
}

fn metadata_u64_source_collect_field_roots(source: &MetadataU64Source, field: &str, roots: &mut Vec<usize>) -> bool {
    match source {
        MetadataU64Source::Field(alias) if alias.field == field => {
            roots.push(alias.root_id);
            true
        }
        MetadataU64Source::Field(_) => false,
        MetadataU64Source::Add(left, right) => {
            metadata_u64_source_collect_field_roots(left, field, roots) && metadata_u64_source_collect_field_roots(right, field, roots)
        }
        MetadataU64Source::Sub(_, _) => false,
    }
}

fn metadata_u64_source_collect_amount_split_subtrahends(
    source: &MetadataU64Source,
    consumed_root: usize,
) -> Option<Vec<ir::IrOperand>> {
    match source {
        MetadataU64Source::Field(alias) if alias.root_id == consumed_root && alias.field == "amount" => Some(Vec::new()),
        MetadataU64Source::Sub(left, right) => {
            let mut subtrahends = metadata_u64_source_collect_amount_split_subtrahends(left, consumed_root)?;
            subtrahends.push(right.clone());
            Some(subtrahends)
        }
        MetadataU64Source::Field(_) | MetadataU64Source::Add(_, _) => None,
    }
}

use crate::ast::{AggregateInvariantKind, AggregateRelation};
use crate::ir;
use std::collections::HashMap;

pub(crate) const XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER: &str = "__xudt_require_group_amount_conserved";
pub(crate) const XUDT_GROUP_AMOUNT_CONSERVED_METADATA_HELPER: &str = "xudt::require_group_amount_conserved";

#[derive(Debug, Clone)]
struct FieldAlias {
    root_id: usize,
    field: String,
}

pub(crate) fn xudt_group_amount_conservation_type<'a>(
    invariant: &ir::IrInvariant,
    aggregate: &'a ir::IrAggregateInvariant,
) -> Option<&'a str> {
    if invariant.trigger.as_deref() != Some("type_group") || aggregate.scope != "group" {
        return None;
    }
    if aggregate.kind != AggregateInvariantKind::Sum || aggregate.relation != Some(AggregateRelation::Eq) {
        return None;
    }
    let rhs = aggregate.rhs.as_deref()?;
    let (left_source, left_type) = aggregate_group_amount_endpoint(&aggregate.target)?;
    let (right_source, right_type) = aggregate_group_amount_endpoint(rhs)?;
    (left_type == right_type
        && ((left_source == "group_outputs" && right_source == "group_inputs")
            || (left_source == "group_inputs" && right_source == "group_outputs")))
        .then_some(left_type)
}

pub(crate) fn action_has_group_amount_conservation_evidence(action: &ir::IrAction, type_name: &str) -> bool {
    let consumed = action
        .body
        .consume_set
        .iter()
        .filter(|pattern| pattern.operation == "consume")
        .filter_map(|pattern| {
            action.params.iter().find(|param| param.name == pattern.binding && named_type_name(&param.ty) == Some(type_name))
        })
        .collect::<Vec<_>>();
    let created = action
        .body
        .create_set
        .iter()
        .filter(|pattern| matches!(pattern.operation.as_str(), "create" | "output") && pattern.ty == type_name)
        .collect::<Vec<_>>();
    let ([consumed], [created]) = (consumed.as_slice(), created.as_slice()) else {
        return false;
    };
    let Some((_, amount)) = created.fields.iter().find(|(field, _)| field == "amount") else {
        return false;
    };
    let ir::IrOperand::Var(amount) = amount else {
        return false;
    };
    let aliases = field_aliases(&action.body);
    aliases.get(&amount.id).is_some_and(|alias| alias.root_id == consumed.binding.id && alias.field == "amount")
}

pub(crate) fn body_contains_runtime_helper(body: &ir::IrBody, helper: &str) -> bool {
    body.blocks.iter().any(|block| {
        block.instructions.iter().any(|instruction| matches!(instruction, ir::IrInstruction::Call { func, .. } if func == helper))
    })
}

pub(crate) fn aggregate_group_amount_endpoint(target: &str) -> Option<(&str, &str)> {
    let (source, rest) = target.split_once('<')?;
    if source != "group_inputs" && source != "group_outputs" {
        return None;
    }
    let (type_name, field) = rest.split_once(">.")?;
    (field == "amount" && !type_name.is_empty()).then_some((source, type_name))
}

fn field_aliases(body: &ir::IrBody) -> HashMap<usize, FieldAlias> {
    let mut aliases = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } => {
                    let alias = aliases.get(&obj.id).cloned().map_or_else(
                        || FieldAlias { root_id: obj.id, field: field.clone() },
                        |parent: FieldAlias| FieldAlias { root_id: parent.root_id, field: format!("{}.{}", parent.field, field) },
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

fn named_type_name(ty: &ir::IrType) -> Option<&str> {
    match ty {
        ir::IrType::Named(name) => Some(name.as_str()),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => named_type_name(inner),
        _ => None,
    }
}

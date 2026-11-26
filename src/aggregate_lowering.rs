use crate::ast::{AggregateInvariantKind, AggregateRelation, AggregateTarget, SourceView};
use crate::ir;
use std::collections::HashMap;
mod coverage;

pub(crate) use coverage::body_checks_fungible_profile;
pub(crate) use coverage::conservation_checked_for_selected_entries;

pub(crate) const XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER: &str = "__xudt_require_group_amount_conserved";
pub(crate) const XUDT_GROUP_AMOUNT_CONSERVED_METADATA_HELPER: &str = "xudt::require_group_amount_conserved";
pub(crate) const FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER: &str = "__cellscript_require_fungible_type_group_v1";
pub(crate) const FUNGIBLE_TYPE_GROUP_V1_METADATA_HELPER: &str = "fungible::require_type_group_v1";

#[derive(Debug, Clone)]
struct FieldAlias {
    root_id: usize,
    field: String,
}

#[derive(Clone, Copy)]
pub(crate) struct GroupField<'a> {
    pub type_name: &'a str,
    pub field_name: &'a str,
}

/// One resolved domain for conservation shorthand and explicit group sum
/// equality. The runtime helper checks exactly sixteen bytes as one u128.
pub(crate) fn group_u128_conservation_field<'a>(
    module: &'a ir::IrModule,
    invariant: &ir::IrInvariant,
    aggregate: &'a ir::IrAggregateInvariant,
) -> Option<GroupField<'a>> {
    let (type_name, field_name) = conservation_target(invariant, aggregate)?;
    resolved_group_u128_field(module, type_name, field_name)
}

pub(crate) fn resolved_group_u128_field<'a>(module: &'a ir::IrModule, type_name: &str, field_name: &str) -> Option<GroupField<'a>> {
    let definition = module
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::TypeDef(def) => Some(def),
            _ => None,
        })
        .chain(&module.external_type_defs)
        .find(|definition| definition.name == type_name)?;
    if definition.kind == ir::IrTypeKind::Struct {
        return None;
    }
    let field = definition.fields.iter().find(|field| field.name == field_name)?;
    let size = definition.fields.iter().try_fold(0usize, |size, field| size.checked_add(field.fixed_size?))?;
    (field.ty == ir::IrType::U128 && field.offset == 0 && field.fixed_size == Some(16) && size == 16)
        .then_some(GroupField { type_name: &definition.name, field_name: &field.name })
}

fn conservation_target<'a>(invariant: &ir::IrInvariant, aggregate: &'a ir::IrAggregateInvariant) -> Option<(&'a str, &'a str)> {
    if invariant.trigger.as_deref() != Some("type_group") || invariant.scope.as_deref() != Some("group") || aggregate.scope != "group"
    {
        return None;
    }
    if aggregate.kind == AggregateInvariantKind::Conserved {
        return aggregate.target.type_and_field().filter(|_| has_group_conservation_reads(invariant, &aggregate.target));
    }
    if aggregate.kind != AggregateInvariantKind::Sum || aggregate.relation != Some(AggregateRelation::Eq) {
        return None;
    }
    let rhs = aggregate.rhs.as_ref()?;
    let (left_source, left_type, left_field) = aggregate_group_field_endpoint(&aggregate.target)?;
    let (right_source, right_type, right_field) = aggregate_group_field_endpoint(rhs)?;
    (left_type == right_type
        && left_field == right_field
        && ((left_source == SourceView::GroupOutput && right_source == SourceView::GroupInput)
            || (left_source == SourceView::GroupInput && right_source == SourceView::GroupOutput)))
        .then_some((left_type, left_field))
}

pub(crate) fn has_group_conservation_reads(invariant: &ir::IrInvariant, target: &AggregateTarget) -> bool {
    [SourceView::GroupInput, SourceView::GroupOutput].into_iter().all(|source| {
        invariant.reads.iter().any(|read| {
            read.source == source
                && read.type_name == target.type_name
                && read.type_args == target.type_args
                && read.field == target.field
        })
    })
}

pub(crate) fn auto_group_conservation_helper(module: &ir::IrModule, action: &ir::IrAction) -> Option<&'static str> {
    // Retain the existing non-duplicating direct-call path, but a helper in
    // only one branch cannot suppress an otherwise required entry prelude.
    if coverage::body_checks_conservation(&action.body, &std::collections::HashSet::new()) {
        return None;
    }
    module
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Invariant(value) => Some(value),
            _ => None,
        })
        .any(|invariant| {
            invariant.aggregates.iter().any(|aggregate| {
                group_u128_conservation_field(module, invariant, aggregate)
                    .is_some_and(|field| action_has_group_conservation_evidence(action, field))
            })
        })
        .then_some(XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER)
}

fn action_has_group_conservation_evidence(action: &ir::IrAction, field: GroupField<'_>) -> bool {
    let consumed = action
        .body
        .consume_set
        .iter()
        .filter(|pattern| pattern.operation == "consume")
        .filter_map(|pattern| {
            action.params.iter().find(|param| param.name == pattern.binding && named_type_name(&param.ty) == Some(field.type_name))
        })
        .collect::<Vec<_>>();
    let created = action
        .body
        .create_set
        .iter()
        .filter(|pattern| matches!(pattern.operation.as_str(), "create" | "output") && pattern.ty == field.type_name)
        .collect::<Vec<_>>();
    let ([consumed], [created]) = (consumed.as_slice(), created.as_slice()) else {
        return false;
    };
    let Some((_, amount)) = created.fields.iter().find(|(name, _)| name == field.field_name) else {
        return false;
    };
    let ir::IrOperand::Var(amount) = amount else {
        return false;
    };
    let aliases = field_aliases(&action.body);
    aliases.get(&amount.id).is_some_and(|alias| alias.root_id == consumed.binding.id && alias.field == field.field_name)
}

pub(crate) fn aggregate_group_amount_endpoint(target: &AggregateTarget) -> Option<(SourceView, &str)> {
    if !matches!(target.source, SourceView::GroupInput | SourceView::GroupOutput) || target.field.as_deref() != Some("amount") {
        return None;
    }
    Some((target.source, target.type_name.as_deref()?))
}

pub(crate) fn aggregate_group_field_endpoint(target: &AggregateTarget) -> Option<(SourceView, &str, &str)> {
    if !matches!(target.source, SourceView::GroupInput | SourceView::GroupOutput) {
        return None;
    }
    Some((target.source, target.type_name.as_deref()?, target.field.as_deref()?))
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

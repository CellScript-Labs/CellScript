//! Declaration labels remain lookup/output keys. Compare their retained
//! defining identities before propagating semantic facts such as owned extent.
use super::{IrType, TypeArgument};
use crate::resolve::ResolvedTypeIdentity;
use std::collections::BTreeMap;

/// This does not admit an unresolved type or establish any layout/ABI proof.
/// An ambiguous label cannot supply nominal equivalence, even with itself.
pub(crate) fn resolved_types_equal(left: &IrType, right: &IrType, origins: &BTreeMap<String, Option<ResolvedTypeIdentity>>) -> bool {
    let same = |left: &IrType, right: &IrType| resolved_types_equal(left, right, origins);
    match (left, right) {
        (IrType::Named(left), IrType::Named(right)) => match (origins.get(left), origins.get(right)) {
            (Some(Some(left)), Some(Some(right))) => left == right,
            (None, None) => left == right,
            _ => false,
        },
        (IrType::Array(left, a), IrType::Array(right, b)) => a == b && same(left, right),
        (IrType::Tuple(left), IrType::Tuple(right)) => left.len() == right.len() && left.iter().zip(right).all(|(a, b)| same(a, b)),
        (IrType::Ref(left), IrType::Ref(right)) | (IrType::MutRef(left), IrType::MutRef(right)) => same(left, right),
        (IrType::Commitment(left), IrType::Commitment(right)) | (IrType::Opening(left), IrType::Opening(right)) => {
            left.source_text() == right.source_text() && same(left, right)
        }
        (IrType::Vector(Some(left)), IrType::Vector(Some(right))) => same(left, right),
        (IrType::Context(left), IrType::Context(right)) => {
            left.constructor() == right.constructor()
                && match (left.schema(), right.schema()) {
                    (Some(left), Some(right)) => same(left, right),
                    (None, None) => true,
                    _ => false,
                }
        }
        (IrType::Bounded(left), IrType::Bounded(right)) => {
            left.kind == right.kind && left.max_elements == right.max_elements && same(&left.element_type, &right.element_type)
        }
        (IrType::Applied(left), IrType::Applied(right)) => {
            left.constructor == right.constructor
                && left.arguments.len() == right.arguments.len()
                && left.arguments.iter().zip(&right.arguments).all(|(left, right)| match (left, right) {
                    (TypeArgument::Type(left), TypeArgument::Type(right)) => same(left, right),
                    (TypeArgument::UnsignedConstant(left), TypeArgument::UnsignedConstant(right)) => left == right,
                    _ => false,
                })
        }
        _ => left == right,
    }
}

/// Declaration lookup closure for imported signature/layout nodes. This only
/// collects retained nominal names; it does not interpret rendered families.
pub(super) fn collect_names(ty: &IrType, names: &mut std::collections::BTreeSet<String>) {
    match ty {
        IrType::Named(name) => {
            names.insert(name.clone());
        }
        IrType::Array(inner, _) | IrType::Ref(inner) | IrType::MutRef(inner) | IrType::Commitment(inner) | IrType::Opening(inner) => {
            collect_names(inner, names)
        }
        IrType::Tuple(items) => {
            for ty in items {
                collect_names(ty, names);
            }
        }
        IrType::Vector(inner) => {
            if let Some(inner) = inner {
                collect_names(inner, names);
            }
        }
        IrType::Context(view) => {
            if let Some(inner) = view.schema() {
                collect_names(inner, names);
            }
        }
        IrType::Bounded(collection) => collect_names(&collection.element_type, names),
        IrType::Applied(application) => {
            for ty in application.arguments.iter().filter_map(TypeArgument::as_type) {
                collect_names(ty, names);
            }
        }
        IrType::Selection(selection) => names.extend(selection.schemas.iter().cloned()),
        IrType::Nested(view) => {
            use crate::nested_values::{Element, NestedType};
            if let NestedType::Values(crate::nested_values::ValuesType { element: Element::Record(record), .. })
            | NestedType::RecordView { element: Element::Record(record), .. } = view.description()
            {
                names.insert(record.spelling().into());
            }
        }
        IrType::U8
        | IrType::U16
        | IrType::U32
        | IrType::I32
        | IrType::U64
        | IrType::U128
        | IrType::Bool
        | IrType::Unit
        | IrType::Hash
        | IrType::Address
        | IrType::OpenHandle(_)
        | IrType::WitnessBytes(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::DeclarationIdentity;

    #[test]
    fn semantic_alias_equality_requires_one_unambiguous_defining_identity() {
        let origins = BTreeMap::from([
            (
                "First".into(),
                Some(ResolvedTypeIdentity::Nominal {
                    declaration: DeclarationIdentity { module: "left".into(), name: "Value".into() },
                    arguments: vec![],
                }),
            ),
            (
                "Alias".into(),
                Some(ResolvedTypeIdentity::Nominal {
                    declaration: DeclarationIdentity { module: "left".into(), name: "Value".into() },
                    arguments: vec![],
                }),
            ),
            (
                "Other".into(),
                Some(ResolvedTypeIdentity::Nominal {
                    declaration: DeclarationIdentity { module: "right".into(), name: "Value".into() },
                    arguments: vec![],
                }),
            ),
            ("Ambiguous".into(), None),
        ]);
        let first = IrType::Named("First".into());
        let alias = IrType::Named("Alias".into());
        let other = IrType::Named("Other".into());
        assert!(resolved_types_equal(&first, &alias, &origins));
        assert!(!resolved_types_equal(&first, &other, &origins));
        assert!(resolved_types_equal(&IrType::vector(first.clone()), &IrType::vector(alias.clone()), &origins));
        assert!(!resolved_types_equal(&IrType::vector(first), &IrType::vector(other), &origins));
        assert!(!resolved_types_equal(&IrType::Named("Ambiguous".into()), &IrType::Named("Ambiguous".into()), &origins));
        assert!(!resolved_types_equal(&IrType::Ref(Box::new(alias.clone())), &alias, &origins));
    }
}

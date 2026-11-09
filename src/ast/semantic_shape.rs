//! Exhaustive structural semantic comparison for source rewrites.
//! Every AST field and variant is matched explicitly. Only spans and doc-comment
//! presentation are excluded; this is not an optimizer or a debug-string hash.
use super::*;
use std::sync::Arc;

pub(crate) trait SemanticEq {
    fn same_semantics(&self, other: &Self) -> bool;
}
macro_rules! exact {
    ($($ty:ty),* $(,)?) => {$(impl SemanticEq for $ty {
        fn same_semantics(&self, other: &Self) -> bool { self == other }
    })*};
}
exact!(String, bool, usize, u8, u16, u32, u64, u128, i32, i64, cellscript_artifact_checker::cell_selection::SelectionType);
impl SemanticEq for Span {
    fn same_semantics(&self, _: &Self) -> bool {
        true
    }
}
impl<T: SemanticEq> SemanticEq for Vec<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other).all(|(a, b)| a.same_semantics(b))
    }
}
impl<T: SemanticEq> SemanticEq for Option<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        match (self, other) {
            (Some(a), Some(b)) => a.same_semantics(b),
            (None, None) => true,
            _ => false,
        }
    }
}
impl<T: SemanticEq> SemanticEq for Box<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        (**self).same_semantics(other)
    }
}
impl<T: SemanticEq> SemanticEq for Arc<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        (**self).same_semantics(other)
    }
}
impl<K: Ord, V: SemanticEq> SemanticEq for BTreeMap<K, V> {
    fn same_semantics(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other).all(|((ka, a), (kb, b))| ka == kb && a.same_semantics(b))
    }
}
impl<A: SemanticEq, B: SemanticEq> SemanticEq for (A, B) {
    fn same_semantics(&self, other: &Self) -> bool {
        self.0.same_semantics(&other.0) && self.1.same_semantics(&other.1)
    }
}
impl<T: SemanticEq, const N: usize> SemanticEq for [T; N] {
    fn same_semantics(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| a.same_semantics(b))
    }
}

impl SemanticEq for Module {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, items: _, interface_templates: _, specializations: _, visibilities: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.items.same_semantics(&other.items)
            && self.interface_templates.same_semantics(&other.interface_templates)
            && self.specializations.same_semantics(&other.specializations)
            && self.visibilities.same_semantics(&other.visibilities)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for Visibility {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::ImplicitPublic => matches!(other, Self::ImplicitPublic),
            Self::Public => matches!(other, Self::Public),
            Self::Package => matches!(other, Self::Package),
            Self::Private => matches!(other, Self::Private),
        }
    }
}

impl SemanticEq for Item {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::DataShape(a0) => {
                if let Self::DataShape(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Collections(a0) => {
                if let Self::Collections(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::CellRoles(a0) => {
                if let Self::CellRoles(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Resource(a0) => {
                if let Self::Resource(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Shared(a0) => {
                if let Self::Shared(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Receipt(a0) => {
                if let Self::Receipt(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Struct(a0) => {
                if let Self::Struct(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Flow(a0) => {
                if let Self::Flow(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Invariant(a0) => {
                if let Self::Invariant(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Const(a0) => {
                if let Self::Const(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Enum(a0) => {
                if let Self::Enum(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Action(a0) => {
                if let Self::Action(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Function(a0) => {
                if let Self::Function(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Lock(a0) => {
                if let Self::Lock(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Use(a0) => {
                if let Self::Use(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for CellRolesDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, limits: _, roles: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.limits.same_semantics(&other.limits)
            && self.roles.same_semantics(&other.roles)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for DataShapeDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, element: _, limits: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.element.same_semantics(&other.element)
            && self.limits.same_semantics(&other.limits)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for DataShapeLimits {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { items: _, payload_bytes: _, encoded_bytes: _ } = self;
        self.items.same_semantics(&other.items)
            && self.payload_bytes.same_semantics(&other.payload_bytes)
            && self.encoded_bytes.same_semantics(&other.encoded_bytes)
    }
}

impl SemanticEq for CollectionsDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, bytes: _, allocations: _, members: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.bytes.same_semantics(&other.bytes)
            && self.allocations.same_semantics(&other.allocations)
            && self.members.same_semantics(&other.members)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for CollectionMember {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, kind: _, items: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.kind.same_semantics(&other.kind)
            && self.items.same_semantics(&other.items)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ValueCollectionKind {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Vec(a0) => {
                if let Self::Vec(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::OrderedMap { key: a0, value: a1 } => {
                if let Self::OrderedMap { key: b0, value: b1 } = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1)
                } else {
                    false
                }
            }
            Self::OrderedSet(a0) => {
                if let Self::OrderedSet(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for CellRoleLimits {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { input_scan: _, output_scan: _, selected_per_side: _, aggregate_bytes: _ } = self;
        self.input_scan.same_semantics(&other.input_scan)
            && self.output_scan.same_semantics(&other.output_scan)
            && self.selected_per_side.same_semantics(&other.selected_per_side)
            && self.aggregate_bytes.same_semantics(&other.aggregate_bytes)
    }
}

impl SemanticEq for NamedCellRole {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            name: _,
            schema: _,
            type_hash: _,
            lock_hash: _,
            identity_field: _,
            authority: _,
            correspondence: _,
            inputs: _,
            outputs: _,
            span: _,
        } = self;
        self.name.same_semantics(&other.name)
            && self.schema.same_semantics(&other.schema)
            && self.type_hash.same_semantics(&other.type_hash)
            && self.lock_hash.same_semantics(&other.lock_hash)
            && self.identity_field.same_semantics(&other.identity_field)
            && self.authority.same_semantics(&other.authority)
            && self.correspondence.same_semantics(&other.correspondence)
            && self.inputs.same_semantics(&other.inputs)
            && self.outputs.same_semantics(&other.outputs)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for CellRoleAuthority {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Observe => matches!(other, Self::Observe),
            Self::LocalType => matches!(other, Self::LocalType),
        }
    }
}

impl SemanticEq for CellRoleCorrespondence {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Preserve => matches!(other, Self::Preserve),
            Self::Create => matches!(other, Self::Create),
            Self::Burn => matches!(other, Self::Burn),
            Self::Merge(a0) => {
                if let Self::Merge(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for ResourceDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            name: _,
            type_id: _,
            identity: _,
            default_hash_type: _,
            capacity_floor: _,
            capabilities: _,
            fields: _,
            validity: _,
            span: _,
        } = self;
        self.name.same_semantics(&other.name)
            && self.type_id.same_semantics(&other.type_id)
            && self.identity.same_semantics(&other.identity)
            && self.default_hash_type.same_semantics(&other.default_hash_type)
            && self.capacity_floor.same_semantics(&other.capacity_floor)
            && self.capabilities.same_semantics(&other.capabilities)
            && self.fields.same_semantics(&other.fields)
            && self.validity.same_semantics(&other.validity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for SharedDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            name: _,
            type_id: _,
            identity: _,
            default_hash_type: _,
            capacity_floor: _,
            capabilities: _,
            fields: _,
            validity: _,
            span: _,
        } = self;
        self.name.same_semantics(&other.name)
            && self.type_id.same_semantics(&other.type_id)
            && self.identity.same_semantics(&other.identity)
            && self.default_hash_type.same_semantics(&other.default_hash_type)
            && self.capacity_floor.same_semantics(&other.capacity_floor)
            && self.capabilities.same_semantics(&other.capabilities)
            && self.fields.same_semantics(&other.fields)
            && self.validity.same_semantics(&other.validity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReceiptDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            name: _,
            type_id: _,
            identity: _,
            default_hash_type: _,
            capacity_floor: _,
            claim_output: _,
            capabilities: _,
            fields: _,
            validity: _,
            span: _,
        } = self;
        self.name.same_semantics(&other.name)
            && self.type_id.same_semantics(&other.type_id)
            && self.identity.same_semantics(&other.identity)
            && self.default_hash_type.same_semantics(&other.default_hash_type)
            && self.capacity_floor.same_semantics(&other.capacity_floor)
            && self.claim_output.same_semantics(&other.claim_output)
            && self.capabilities.same_semantics(&other.capabilities)
            && self.fields.same_semantics(&other.fields)
            && self.validity.same_semantics(&other.validity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for StructDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            origin: _,
            name: _,
            type_params: _,
            abilities: _,
            type_id: _,
            default_hash_type: _,
            capacity_floor: _,
            fields: _,
            validity: _,
            span: _,
        } = self;
        self.origin.same_semantics(&other.origin)
            && self.name.same_semantics(&other.name)
            && self.type_params.same_semantics(&other.type_params)
            && self.abilities.same_semantics(&other.abilities)
            && self.type_id.same_semantics(&other.type_id)
            && self.default_hash_type.same_semantics(&other.default_hash_type)
            && self.capacity_floor.same_semantics(&other.capacity_floor)
            && self.fields.same_semantics(&other.fields)
            && self.validity.same_semantics(&other.validity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ValidityBlock {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { predicates: _, span: _ } = self;
        self.predicates.same_semantics(&other.predicates) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for TypeIdentity {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { value: _, span: _ } = self;
        self.value.same_semantics(&other.value) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for HashTypeDecl {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { value: _, span: _ } = self;
        self.value.same_semantics(&other.value) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for CapacityFloorDecl {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { shannons: _, span: _ } = self;
        self.shannons.same_semantics(&other.shannons) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ConstDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, ty: _, value: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.ty.same_semantics(&other.ty)
            && self.value.same_semantics(&other.value)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for EnumDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { origin: _, name: _, type_params: _, abilities: _, variants: _, span: _ } = self;
        self.origin.same_semantics(&other.origin)
            && self.name.same_semantics(&other.name)
            && self.type_params.same_semantics(&other.type_params)
            && self.abilities.same_semantics(&other.abilities)
            && self.variants.same_semantics(&other.variants)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for TypeParam {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { uses_fixed_value_profile: _, name: _, constraints: _, phantom: _, span: _ } = self;
        self.uses_fixed_value_profile.same_semantics(&other.uses_fixed_value_profile)
            && self.name.same_semantics(&other.name)
            && self.constraints.same_semantics(&other.constraints)
            && self.phantom.same_semantics(&other.phantom)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ValueAbility {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Copy => matches!(other, Self::Copy),
            Self::Drop => matches!(other, Self::Drop),
            Self::Store => matches!(other, Self::Store),
            Self::Fixed => matches!(other, Self::Fixed),
            Self::Serializable => matches!(other, Self::Serializable),
            Self::NonLinear => matches!(other, Self::NonLinear),
            Self::Cell => matches!(other, Self::Cell),
        }
    }
}

impl SemanticEq for EnumVariant {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, fields: _, span: _ } = self;
        self.name.same_semantics(&other.name) && self.fields.same_semantics(&other.fields) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for StateFieldPath {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { base: _, field: _, span: _ } = self;
        self.base.same_semantics(&other.base) && self.field.same_semantics(&other.field) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for StateTransition {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { from: _, to: _, action: _, span: _ } = self;
        self.from.same_semantics(&other.from)
            && self.to.same_semantics(&other.to)
            && self.action.same_semantics(&other.action)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for FlowDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, target: _, initial_states: _, terminal_states: _, transitions: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.target.same_semantics(&other.target)
            && self.initial_states.same_semantics(&other.initial_states)
            && self.terminal_states.same_semantics(&other.terminal_states)
            && self.transitions.same_semantics(&other.transitions)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for Capability {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Store => matches!(other, Self::Store),
            Self::Destroy => matches!(other, Self::Destroy),
            Self::Create => matches!(other, Self::Create),
            Self::Consume => matches!(other, Self::Consume),
            Self::Replace => matches!(other, Self::Replace),
            Self::Burn => matches!(other, Self::Burn),
            Self::Relock => matches!(other, Self::Relock),
            Self::RetargetType => matches!(other, Self::RetargetType),
            Self::ReadRef => matches!(other, Self::ReadRef),
        }
    }
}

impl SemanticEq for CapabilityOperation {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Destroy => matches!(other, Self::Destroy),
            Self::ReplaceUnique => matches!(other, Self::ReplaceUnique),
        }
    }
}

impl SemanticEq for CapabilityEntailment {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { required: _, entailed: _, missing: _, legacy_destroy: _ } = self;
        self.required.same_semantics(&other.required)
            && self.entailed.same_semantics(&other.entailed)
            && self.missing.same_semantics(&other.missing)
            && self.legacy_destroy.same_semantics(&other.legacy_destroy)
    }
}

impl SemanticEq for Field {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, ty: _, span: _ } = self;
        self.name.same_semantics(&other.name) && self.ty.same_semantics(&other.ty) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for InvariantDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, trigger: _, scope: _, reads: _, aggregates: _, quantifiers: _, asserts: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.trigger.same_semantics(&other.trigger)
            && self.scope.same_semantics(&other.scope)
            && self.reads.same_semantics(&other.reads)
            && self.aggregates.same_semantics(&other.aggregates)
            && self.quantifiers.same_semantics(&other.quantifiers)
            && self.asserts.same_semantics(&other.asserts)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for BoundedQuantifierKind {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::ForAll => matches!(other, Self::ForAll),
            Self::Count => matches!(other, Self::Count),
        }
    }
}

impl SemanticEq for BoundedQuantifier {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { kind: _, role: _, binding: _, range: _, predicates: _, relation: _, expected: _, span: _ } = self;
        self.kind.same_semantics(&other.kind)
            && self.role.same_semantics(&other.role)
            && self.binding.same_semantics(&other.binding)
            && self.range.same_semantics(&other.range)
            && self.predicates.same_semantics(&other.predicates)
            && self.relation.same_semantics(&other.relation)
            && self.expected.same_semantics(&other.expected)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for AggregateInvariantKind {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Sum => matches!(other, Self::Sum),
            Self::Conserved => matches!(other, Self::Conserved),
            Self::Delta => matches!(other, Self::Delta),
            Self::Distinct => matches!(other, Self::Distinct),
            Self::Singleton => matches!(other, Self::Singleton),
        }
    }
}

impl SemanticEq for AggregateRelation {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Lt => matches!(other, Self::Lt),
            Self::Le => matches!(other, Self::Le),
            Self::Eq => matches!(other, Self::Eq),
            Self::Ge => matches!(other, Self::Ge),
            Self::Gt => matches!(other, Self::Gt),
        }
    }
}

impl SemanticEq for SourceView {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Input => matches!(other, Self::Input),
            Self::Output => matches!(other, Self::Output),
            Self::GroupInput => matches!(other, Self::GroupInput),
            Self::GroupOutput => matches!(other, Self::GroupOutput),
            Self::CellDep => matches!(other, Self::CellDep),
            Self::HeaderDep => matches!(other, Self::HeaderDep),
            Self::WitnessArgs => matches!(other, Self::WitnessArgs),
            Self::LockArgs => matches!(other, Self::LockArgs),
            Self::SelectedCells => matches!(other, Self::SelectedCells),
            Self::TypeIdentity => matches!(other, Self::TypeIdentity),
        }
    }
}

impl SemanticEq for AggregateTarget {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { source: _, type_name: _, type_args: _, field: _ } = self;
        self.source.same_semantics(&other.source)
            && self.type_name.same_semantics(&other.type_name)
            && self.type_args.same_semantics(&other.type_args)
            && self.field.same_semantics(&other.field)
    }
}

impl SemanticEq for AggregateInvariant {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { kind: _, target: _, scope: _, argument: _, relation: _, rhs: _, span: _ } = self;
        self.kind.same_semantics(&other.kind)
            && self.target.same_semantics(&other.target)
            && self.scope.same_semantics(&other.scope)
            && self.argument.same_semantics(&other.argument)
            && self.relation.same_semantics(&other.relation)
            && self.rhs.same_semantics(&other.rhs)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ActionDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self {
            name: _,
            params: _,
            return_type: _,
            outputs: _,
            state_edges: _,
            body: _,
            scheduler_hint: _,
            next_surface: _,
            doc_comment: _,
            span: _,
        } = self;
        self.name.same_semantics(&other.name)
            && self.params.same_semantics(&other.params)
            && self.return_type.same_semantics(&other.return_type)
            && self.outputs.same_semantics(&other.outputs)
            && self.state_edges.same_semantics(&other.state_edges)
            && self.body.same_semantics(&other.body)
            && self.scheduler_hint.same_semantics(&other.scheduler_hint)
            && self.next_surface.same_semantics(&other.next_surface)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for NextEntrySurface {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { container_name: _, trigger_type: _, audits: _, body_items: _, dispositions: _ } = self;
        self.container_name.same_semantics(&other.container_name)
            && self.trigger_type.same_semantics(&other.trigger_type)
            && self.audits.same_semantics(&other.audits)
            && self.body_items.same_semantics(&other.body_items)
            && self.dispositions.same_semantics(&other.dispositions)
    }
}

impl SemanticEq for NextAudit {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, evidence: _, subject: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.evidence.same_semantics(&other.evidence)
            && self.subject.same_semantics(&other.subject)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for NextAuditEvidence {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::ExternalPolicy => matches!(other, Self::ExternalPolicy),
        }
    }
}

impl SemanticEq for NextDisposition {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Replace(a0) => {
                if let Self::Replace(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Pool(a0) => {
                if let Self::Pool(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Retire(a0) => {
                if let Self::Retire(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Fresh(a0) => {
                if let Self::Fresh(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for NextReplacement {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { statement_index: _, relation_index: _ } = self;
        self.statement_index.same_semantics(&other.statement_index) && self.relation_index.same_semantics(&other.relation_index)
    }
}

impl SemanticEq for NextBodyItem {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Statement(a0) => {
                if let Self::Statement(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Audit(a0) => {
                if let Self::Audit(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Disposition { index: a0, start_statement: a1, statement_count: a2 } => {
                if let Self::Disposition { index: b0, start_statement: b1, statement_count: b2 } = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1) && a2.same_semantics(b2)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for NextPool {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, ty: _, inputs: _, outputs: _, data_fields: _, output_locks: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.ty.same_semantics(&other.ty)
            && self.inputs.same_semantics(&other.inputs)
            && self.outputs.same_semantics(&other.outputs)
            && self.data_fields.same_semantics(&other.data_fields)
            && self.output_locks.same_semantics(&other.output_locks)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for NextPoolField {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { span: _, field: _, treatment: _ } = self;
        self.span.same_semantics(&other.span)
            && self.field.same_semantics(&other.field)
            && self.treatment.same_semantics(&other.treatment)
    }
}

impl SemanticEq for NextPoolFieldTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Conserve => matches!(other, Self::Conserve),
            Self::Set(a0) => {
                if let Self::Set(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for NextRetirement {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { input: _, absence_policy: _, span: _ } = self;
        self.input.same_semantics(&other.input)
            && self.absence_policy.same_semantics(&other.absence_policy)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for NextFreshOutput {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { output: _, ty: _, data_fields: _, identity: _, lock: _, span: _ } = self;
        self.output.same_semantics(&other.output)
            && self.ty.same_semantics(&other.ty)
            && self.data_fields.same_semantics(&other.data_fields)
            && self.identity.same_semantics(&other.identity)
            && self.lock.same_semantics(&other.lock)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ActionOutput {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, ty: _, span: _ } = self;
        self.name.same_semantics(&other.name) && self.ty.same_semantics(&other.ty) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ActionStateEdge {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { path: _, to_path: _, from: _, to: _, span: _ } = self;
        self.path.same_semantics(&other.path)
            && self.to_path.same_semantics(&other.to_path)
            && self.from.same_semantics(&other.from)
            && self.to.same_semantics(&other.to)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for FnDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { origin: _, name: _, type_params: _, params: _, return_type: _, body: _, doc_comment: _, span: _ } = self;
        self.origin.same_semantics(&other.origin)
            && self.name.same_semantics(&other.name)
            && self.type_params.same_semantics(&other.type_params)
            && self.params.same_semantics(&other.params)
            && self.return_type.same_semantics(&other.return_type)
            && self.body.same_semantics(&other.body)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for LockDef {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, params: _, return_type: _, body: _, next_surface: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.params.same_semantics(&other.params)
            && self.return_type.same_semantics(&other.return_type)
            && self.body.same_semantics(&other.body)
            && self.next_surface.same_semantics(&other.next_surface)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for NextLockSurface {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { container_name: _, audits: _, body_items: _ } = self;
        self.container_name.same_semantics(&other.container_name)
            && self.audits.same_semantics(&other.audits)
            && self.body_items.same_semantics(&other.body_items)
    }
}

impl SemanticEq for UseStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { module_path: _, imports: _, span: _ } = self;
        self.module_path.same_semantics(&other.module_path)
            && self.imports.same_semantics(&other.imports)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for UseImport {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, alias: _ } = self;
        self.name.same_semantics(&other.name) && self.alias.same_semantics(&other.alias)
    }
}

impl SemanticEq for Param {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { name: _, ty: _, is_mut: _, is_ref: _, is_read_ref: _, source: _, span: _ } = self;
        self.name.same_semantics(&other.name)
            && self.ty.same_semantics(&other.ty)
            && self.is_mut.same_semantics(&other.is_mut)
            && self.is_ref.same_semantics(&other.is_ref)
            && self.is_read_ref.same_semantics(&other.is_read_ref)
            && self.source.same_semantics(&other.source)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ParamSource {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Default => matches!(other, Self::Default),
            Self::Input => matches!(other, Self::Input),
            Self::Output => matches!(other, Self::Output),
            Self::Protected => matches!(other, Self::Protected),
            Self::Witness => matches!(other, Self::Witness),
            Self::LockArgs => matches!(other, Self::LockArgs),
        }
    }
}

impl SemanticEq for Type {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::U8 => matches!(other, Self::U8),
            Self::U16 => matches!(other, Self::U16),
            Self::U32 => matches!(other, Self::U32),
            Self::I32 => matches!(other, Self::I32),
            Self::U64 => matches!(other, Self::U64),
            Self::U128 => matches!(other, Self::U128),
            Self::Bool => matches!(other, Self::Bool),
            Self::Unit => matches!(other, Self::Unit),
            Self::Address => matches!(other, Self::Address),
            Self::Hash => matches!(other, Self::Hash),
            Self::Array(a0, a1) => {
                if let Self::Array(b0, b1) = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1)
                } else {
                    false
                }
            }
            Self::Tuple(a0) => {
                if let Self::Tuple(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Named(a0) => {
                if let Self::Named(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Applied(a0) => {
                if let Self::Applied(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Selection(a0) => {
                if let Self::Selection(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Ref(a0) => {
                if let Self::Ref(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::MutRef(a0) => {
                if let Self::MutRef(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for BoundedCollectionKind {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::CellSet => matches!(other, Self::CellSet),
            Self::List => matches!(other, Self::List),
        }
    }
}

impl<T: SemanticEq> SemanticEq for BoundedCollectionType<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { kind: _, element_type: _, max_elements: _ } = self;
        self.kind.same_semantics(&other.kind)
            && self.element_type.same_semantics(&other.element_type)
            && self.max_elements.same_semantics(&other.max_elements)
    }
}

impl SemanticEq for Stmt {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Let(a0) => {
                if let Self::Let(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Expr(a0) => {
                if let Self::Expr(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Discard(a0) => {
                if let Self::Discard(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Return(a0) => {
                if let Self::Return(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::If(a0) => {
                if let Self::If(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::For(a0) => {
                if let Self::For(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::While(a0) => {
                if let Self::While(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Break(a0) => {
                if let Self::Break(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Continue(a0) => {
                if let Self::Continue(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Borrow(a0) => {
                if let Self::Borrow(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for BindingPattern {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Name(a0) => {
                if let Self::Name(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Tuple(a0) => {
                if let Self::Tuple(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Wildcard => matches!(other, Self::Wildcard),
        }
    }
}

impl SemanticEq for LetStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { pattern: _, ty: _, value: _, is_mut: _, span: _ } = self;
        self.pattern.same_semantics(&other.pattern)
            && self.ty.same_semantics(&other.ty)
            && self.value.same_semantics(&other.value)
            && self.is_mut.same_semantics(&other.is_mut)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReturnStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { value: _, span: _ } = self;
        self.value.same_semantics(&other.value) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for IfStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { condition: _, then_branch: _, else_branch: _, span: _ } = self;
        self.condition.same_semantics(&other.condition)
            && self.then_branch.same_semantics(&other.then_branch)
            && self.else_branch.same_semantics(&other.else_branch)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ForStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { label: _, pattern: _, iterable: _, body: _, span: _ } = self;
        self.label.same_semantics(&other.label)
            && self.pattern.same_semantics(&other.pattern)
            && self.iterable.same_semantics(&other.iterable)
            && self.body.same_semantics(&other.body)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for WhileStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { label: _, condition: _, body: _, span: _ } = self;
        self.label.same_semantics(&other.label)
            && self.condition.same_semantics(&other.condition)
            && self.body.same_semantics(&other.body)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for LoopControlStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { label: _, span: _ } = self;
        self.label.same_semantics(&other.label) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for BorrowStmt {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { root: _, path: _, binding: _, body: _, span: _ } = self;
        self.root.same_semantics(&other.root)
            && self.path.same_semantics(&other.path)
            && self.binding.same_semantics(&other.binding)
            && self.body.same_semantics(&other.body)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for Expr {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Integer(a0) => {
                if let Self::Integer(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Bool(a0) => {
                if let Self::Bool(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::String(a0) => {
                if let Self::String(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ByteString(a0) => {
                if let Self::ByteString(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Identifier(a0) => {
                if let Self::Identifier(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Assign(a0) => {
                if let Self::Assign(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Binary(a0) => {
                if let Self::Binary(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Unary(a0) => {
                if let Self::Unary(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Call(a0) => {
                if let Self::Call(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::FieldAccess(a0) => {
                if let Self::FieldAccess(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Index(a0) => {
                if let Self::Index(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Create(a0) => {
                if let Self::Create(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Consume(a0) => {
                if let Self::Consume(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Destroy(a0) => {
                if let Self::Destroy(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ReadRef(a0) => {
                if let Self::ReadRef(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Claim(a0) => {
                if let Self::Claim(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Settle(a0) => {
                if let Self::Settle(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::CreateUnique(a0) => {
                if let Self::CreateUnique(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ReplaceUnique(a0) => {
                if let Self::ReplaceUnique(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Assert(a0) => {
                if let Self::Assert(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Require(a0) => {
                if let Self::Require(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::RequireBlock(a0) => {
                if let Self::RequireBlock(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Preserve(a0) => {
                if let Self::Preserve(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ReplaceRelation(a0) => {
                if let Self::ReplaceRelation(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Block(a0) => {
                if let Self::Block(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Tuple(a0) => {
                if let Self::Tuple(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Array(a0) => {
                if let Self::Array(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::If(a0) => {
                if let Self::If(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Cast(a0) => {
                if let Self::Cast(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Range(a0) => {
                if let Self::Range(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::StructInit(a0) => {
                if let Self::StructInit(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Match(a0) => {
                if let Self::Match(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::StdlibCall(a0) => {
                if let Self::StdlibCall(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for AssignExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { target: _, op: _, value: _, span: _ } = self;
        self.target.same_semantics(&other.target)
            && self.op.same_semantics(&other.op)
            && self.value.same_semantics(&other.value)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for AssignOp {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Assign => matches!(other, Self::Assign),
            Self::AddAssign => matches!(other, Self::AddAssign),
        }
    }
}

impl SemanticEq for BinaryExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { op: _, left: _, right: _, span: _ } = self;
        self.op.same_semantics(&other.op)
            && self.left.same_semantics(&other.left)
            && self.right.same_semantics(&other.right)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for BinaryOp {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Add => matches!(other, Self::Add),
            Self::Sub => matches!(other, Self::Sub),
            Self::Mul => matches!(other, Self::Mul),
            Self::Div => matches!(other, Self::Div),
            Self::Mod => matches!(other, Self::Mod),
            Self::Eq => matches!(other, Self::Eq),
            Self::Ne => matches!(other, Self::Ne),
            Self::Lt => matches!(other, Self::Lt),
            Self::Le => matches!(other, Self::Le),
            Self::Gt => matches!(other, Self::Gt),
            Self::Ge => matches!(other, Self::Ge),
            Self::And => matches!(other, Self::And),
            Self::Or => matches!(other, Self::Or),
            Self::BitAnd => matches!(other, Self::BitAnd),
            Self::BitOr => matches!(other, Self::BitOr),
            Self::BitXor => matches!(other, Self::BitXor),
            Self::Shl => matches!(other, Self::Shl),
            Self::Shr => matches!(other, Self::Shr),
        }
    }
}

impl SemanticEq for UnaryExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { op: _, expr: _, span: _ } = self;
        self.op.same_semantics(&other.op) && self.expr.same_semantics(&other.expr) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for UnaryOp {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Neg => matches!(other, Self::Neg),
            Self::Not => matches!(other, Self::Not),
            Self::Ref => matches!(other, Self::Ref),
            Self::Deref => matches!(other, Self::Deref),
        }
    }
}

impl SemanticEq for CallExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { func: _, type_args: _, args: _, span: _ } = self;
        self.func.same_semantics(&other.func)
            && self.type_args.same_semantics(&other.type_args)
            && self.args.same_semantics(&other.args)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for FieldAccessExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, field: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.field.same_semantics(&other.field) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for IndexExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, index: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.index.same_semantics(&other.index) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for CreateExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { target: _, ty: _, fields: _, lock: _, span: _ } = self;
        self.target.same_semantics(&other.target)
            && self.ty.same_semantics(&other.ty)
            && self.fields.same_semantics(&other.fields)
            && self.lock.same_semantics(&other.lock)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ConsumeExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for IdentityPolicy {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::None => matches!(other, Self::None),
            Self::CkbTypeId => matches!(other, Self::CkbTypeId),
            Self::Field(a0) => {
                if let Self::Field(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ScriptArgs => matches!(other, Self::ScriptArgs),
            Self::SingletonType => matches!(other, Self::SingletonType),
        }
    }
}

impl SemanticEq for DestructionPolicy {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Default => matches!(other, Self::Default),
            Self::SingletonType => matches!(other, Self::SingletonType),
            Self::Unique { identity: a0 } => {
                if let Self::Unique { identity: b0 } = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Instance { identity_field: a0 } => {
                if let Self::Instance { identity_field: b0 } = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::BurnAmount { field: a0 } => {
                if let Self::BurnAmount { field: b0 } = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for DestroyExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, policy: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.policy.same_semantics(&other.policy) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReadRefExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { ty: _, type_args: _, span: _ } = self;
        self.ty.same_semantics(&other.ty) && self.type_args.same_semantics(&other.type_args) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ClaimExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { receipt: _, span: _ } = self;
        self.receipt.same_semantics(&other.receipt) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for SettleExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for AssertExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { condition: _, message: _, span: _ } = self;
        self.condition.same_semantics(&other.condition)
            && self.message.same_semantics(&other.message)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for RequireExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { condition: _, message: _, span: _ } = self;
        self.condition.same_semantics(&other.condition)
            && self.message.same_semantics(&other.message)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for RequireBlockExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expressions: _, span: _ } = self;
        self.expressions.same_semantics(&other.expressions) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for PreserveExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { output_name: _, input_name: _, fields: _, span: _ } = self;
        self.output_name.same_semantics(&other.output_name)
            && self.input_name.same_semantics(&other.input_name)
            && self.fields.same_semantics(&other.fields)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReplaceRelation {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { before: _, after: _, data: _, lock: _, capacity: _, type_script: _, span: _ } = self;
        self.before.same_semantics(&other.before)
            && self.after.same_semantics(&other.after)
            && self.data.same_semantics(&other.data)
            && self.lock.same_semantics(&other.lock)
            && self.capacity.same_semantics(&other.capacity)
            && self.type_script.same_semantics(&other.type_script)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReplaceDataTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Fields(a0) => {
                if let Self::Fields(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::SameExcept(a0) => {
                if let Self::SameExcept(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for ReplaceFieldTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Same(a0) => {
                if let Self::Same(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Assign(a0, a1) => {
                if let Self::Assign(b0, b1) = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for ReplaceLockTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Same => matches!(other, Self::Same),
            Self::Exact(a0) => {
                if let Self::Exact(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::ExactHash(a0) => {
                if let Self::ExactHash(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for ReplaceCapacityTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Same => matches!(other, Self::Same),
        }
    }
}

impl SemanticEq for ReplaceTypeScriptTreatment {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Same => matches!(other, Self::Same),
        }
    }
}

impl SemanticEq for IfExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { condition: _, then_branch: _, else_branch: _, span: _ } = self;
        self.condition.same_semantics(&other.condition)
            && self.then_branch.same_semantics(&other.then_branch)
            && self.else_branch.same_semantics(&other.else_branch)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for CastExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, ty: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.ty.same_semantics(&other.ty) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for RangeExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { start: _, end: _, span: _ } = self;
        self.start.same_semantics(&other.start) && self.end.same_semantics(&other.end) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for StructInitExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { ty: _, type_args: _, fields: _, span: _ } = self;
        self.ty.same_semantics(&other.ty)
            && self.type_args.same_semantics(&other.type_args)
            && self.fields.same_semantics(&other.fields)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for MatchExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, arms: _, span: _ } = self;
        self.expr.same_semantics(&other.expr) && self.arms.same_semantics(&other.arms) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for MatchArm {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { pattern: _, value: _, span: _ } = self;
        self.pattern.same_semantics(&other.pattern) && self.value.same_semantics(&other.value) && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for MatchPattern {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Wildcard => matches!(other, Self::Wildcard),
            Self::Binding(a0) => {
                if let Self::Binding(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Tuple(a0) => {
                if let Self::Tuple(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::Struct { path: a0, fields: a1 } => {
                if let Self::Struct { path: b0, fields: b1 } = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1)
                } else {
                    false
                }
            }
            Self::Variant { path: a0, fields: a1 } => {
                if let Self::Variant { path: b0, fields: b1 } = other {
                    a0.same_semantics(b0) && a1.same_semantics(b1)
                } else {
                    false
                }
            }
            Self::Or(a0) => {
                if let Self::Or(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for StdlibCallExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { namespace: _, name: _, args: _, preserve_fields: _, span: _ } = self;
        self.namespace.same_semantics(&other.namespace)
            && self.name.same_semantics(&other.name)
            && self.args.same_semantics(&other.args)
            && self.preserve_fields.same_semantics(&other.preserve_fields)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for SchedulerHint {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { parallelizable: _, estimated_cycles: _ } = self;
        self.parallelizable.same_semantics(&other.parallelizable) && self.estimated_cycles.same_semantics(&other.estimated_cycles)
    }
}

impl SemanticEq for CreateUniqueExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { target: _, ty: _, type_args: _, fields: _, lock: _, identity: _, span: _ } = self;
        self.target.same_semantics(&other.target)
            && self.ty.same_semantics(&other.ty)
            && self.type_args.same_semantics(&other.type_args)
            && self.fields.same_semantics(&other.fields)
            && self.lock.same_semantics(&other.lock)
            && self.identity.same_semantics(&other.identity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for ReplaceUniqueExpr {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { expr: _, ty: _, type_args: _, fields: _, identity: _, span: _ } = self;
        self.expr.same_semantics(&other.expr)
            && self.ty.same_semantics(&other.ty)
            && self.type_args.same_semantics(&other.type_args)
            && self.fields.same_semantics(&other.fields)
            && self.identity.same_semantics(&other.identity)
            && self.span.same_semantics(&other.span)
    }
}

impl SemanticEq for DeclarationIdentity {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { module: _, name: _ } = self;
        self.module.same_semantics(&other.module) && self.name.same_semantics(&other.name)
    }
}

impl SemanticEq for SpecializationKind {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Struct => matches!(other, Self::Struct),
            Self::Enum => matches!(other, Self::Enum),
            Self::Function => matches!(other, Self::Function),
        }
    }
}

impl SemanticEq for SpecializationOrigin {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { kind: _, template: _, local_template: _, arguments: _ } = self;
        self.kind.same_semantics(&other.kind)
            && self.template.same_semantics(&other.template)
            && self.local_template.same_semantics(&other.local_template)
            && self.arguments.same_semantics(&other.arguments)
    }
}

impl SemanticEq for DefinitionOrigin {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Source => matches!(other, Self::Source),
            Self::Specialization => matches!(other, Self::Specialization),
        }
    }
}

impl<T: SemanticEq> SemanticEq for TypeApplication<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { constructor: _, arguments: _ } = self;
        self.constructor.same_semantics(&other.constructor) && self.arguments.same_semantics(&other.arguments)
    }
}

impl<T: SemanticEq> SemanticEq for TypeArgument<T> {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Type(a0) => {
                if let Self::Type(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
            Self::UnsignedConstant(a0) => {
                if let Self::UnsignedConstant(b0) = other {
                    a0.same_semantics(b0)
                } else {
                    false
                }
            }
        }
    }
}

impl SemanticEq for OpenHandleClass {
    fn same_semantics(&self, other: &Self) -> bool {
        match self {
            Self::Script => matches!(other, Self::Script),
            Self::Verifier => matches!(other, Self::Verifier),
            Self::Instance => matches!(other, Self::Instance),
        }
    }
}

impl SemanticEq for OpenHandleType {
    fn same_semantics(&self, other: &Self) -> bool {
        let Self { class: _, interface: _ } = self;
        self.class.same_semantics(&other.class) && self.interface.same_semantics(&other.interface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_comparison_ignores_spans_but_keeps_operations_types_and_scopes() {
        let source = "module shape;\naction main() -> u64 { let _: u128 = -(1 as u128);\nreturn 0; }";
        let original = crate::frontend::parse(source, crate::CURRENT_EDITION).unwrap();
        let moved = crate::frontend::parse(&source.replace("\naction", "\n\n\naction"), crate::CURRENT_EDITION).unwrap();
        assert!(original.same_semantics(&moved));
        for changed in [
            source.replace("-(1 as u128)", "-1 as u128"),
            source.replace("as u128", "as u64"),
            source.replace("return 0", "return 1"),
            source.replace("module shape;", "module other;"),
        ] {
            let different = crate::frontend::parse(&changed, crate::CURRENT_EDITION).unwrap();
            assert!(!original.same_semantics(&different), "{changed}");
        }
    }
}

//! Source-only unsigned bounds. Names remain structured until their lexical
//! constant owner is resolved; executable layouts consume normalized integers.
use super::*;
use crate::error::{Result, Span};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundReference {
    pub path: String,
    pub span: Span,
}

impl Hash for BoundReference {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.span.start.hash(state);
        self.span.end.hash(state);
        self.span.line.hash(state);
        self.span.column.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnsignedBound {
    Literal(u128),
    Reference(BoundReference),
}

impl std::fmt::Display for UnsignedBound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Literal(value) => value.fmt(f),
            Self::Reference(reference) => reference.path.fmt(f),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BoundPolicy {
    DataShape(DataShapeDef<UnsignedBound>),
    Collections(CollectionsDef<UnsignedBound>),
    CellRoles(CellRolesDef<UnsignedBound>),
}

impl BoundPolicy {
    pub fn name(&self) -> &str {
        match self {
            Self::DataShape(def) => &def.name,
            Self::Collections(def) => &def.name,
            Self::CellRoles(def) => &def.name,
        }
    }

    pub fn keyword(&self) -> &'static str {
        match self {
            Self::DataShape(_) => "data_shape",
            Self::Collections(_) => "collections",
            Self::CellRoles(_) => "cell_roles",
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::DataShape(def) => def.span,
            Self::Collections(def) => def.span,
            Self::CellRoles(def) => def.span,
        }
    }

    pub(crate) fn bounds(&self) -> Vec<&UnsignedBound> {
        match self {
            Self::DataShape(def) => vec![&def.limits.items, &def.limits.payload_bytes, &def.limits.encoded_bytes],
            Self::Collections(def) => std::iter::once(&def.bytes)
                .chain(std::iter::once(&def.allocations))
                .chain(def.members.iter().map(|member| &member.items))
                .collect(),
            Self::CellRoles(def) => {
                [&def.limits.input_scan, &def.limits.output_scan, &def.limits.selected_per_side, &def.limits.aggregate_bytes]
                    .into_iter()
                    .chain(def.roles.iter().flat_map(|role| [&role.inputs.0, &role.inputs.1, &role.outputs.0, &role.outputs.1]))
                    .collect()
            }
        }
    }

    pub(crate) fn resolve(self, mut resolve: impl FnMut(&UnsignedBound) -> Result<u32>) -> Result<Item> {
        Ok(match self {
            Self::DataShape(def) => Item::DataShape(DataShapeDef {
                name: def.name,
                element: def.element,
                span: def.span,
                limits: DataShapeLimits {
                    items: resolve(&def.limits.items)?,
                    payload_bytes: resolve(&def.limits.payload_bytes)?,
                    encoded_bytes: resolve(&def.limits.encoded_bytes)?,
                },
            }),
            Self::Collections(def) => Item::Collections(CollectionsDef {
                name: def.name,
                span: def.span,
                bytes: resolve(&def.bytes)?,
                allocations: resolve(&def.allocations)?,
                members: def
                    .members
                    .into_iter()
                    .map(|member| {
                        Ok(CollectionMember {
                            name: member.name,
                            kind: member.kind,
                            span: member.span,
                            items: resolve(&member.items)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            }),
            Self::CellRoles(def) => Item::CellRoles(CellRolesDef {
                name: def.name,
                span: def.span,
                limits: CellRoleLimits {
                    input_scan: resolve(&def.limits.input_scan)?,
                    output_scan: resolve(&def.limits.output_scan)?,
                    selected_per_side: resolve(&def.limits.selected_per_side)?,
                    aggregate_bytes: resolve(&def.limits.aggregate_bytes)?,
                },
                roles: def
                    .roles
                    .into_iter()
                    .map(|role| {
                        Ok(NamedCellRole {
                            name: role.name,
                            schema: role.schema,
                            span: role.span,
                            type_hash: role.type_hash,
                            lock_hash: role.lock_hash,
                            identity_field: role.identity_field,
                            authority: role.authority,
                            correspondence: role.correspondence,
                            inputs: (resolve(&role.inputs.0)?, resolve(&role.inputs.1)?),
                            outputs: (resolve(&role.outputs.0)?, resolve(&role.outputs.1)?),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            }),
        })
    }
}

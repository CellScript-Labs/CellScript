//! Retained specialization provenance. Generated labels are output/lookup keys;
//! they do not encode semantic evidence for later compiler phases.
use super::{DeclarationIdentity, Type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecializationKind {
    Struct,
    Enum,
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecializationOrigin {
    pub kind: SpecializationKind,
    pub template: DeclarationIdentity,
    /// The requesting scope's template binding. It can be an import alias;
    /// the defining declaration identity above remains separate.
    pub local_template: String,
    pub arguments: Vec<Type>,
}

/// Declared provenance is independent of the spelling of a generated symbol.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DefinitionOrigin {
    #[default]
    Source,
    Specialization,
}

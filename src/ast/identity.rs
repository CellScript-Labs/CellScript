//! A defining declaration identity is independent of local aliases, layout,
//! source presentation and emitted symbol names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeclarationIdentity {
    pub module: String,
    pub name: String,
}

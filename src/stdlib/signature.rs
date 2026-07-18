//! Canonical signature registry for public `std::*` CellScript helpers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StdlibSignature {
    pub namespace: &'static str,
    pub name: &'static str,
    pub arity: usize,
    pub allows_preserve_fields: bool,
}

impl StdlibSignature {
    pub fn qualified_name(self) -> String {
        format!("std::{}::{}", self.namespace, self.name)
    }
}

pub const SIGNATURES: [StdlibSignature; 9] = [
    StdlibSignature { namespace: "cell", name: "same_lock", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "cell", name: "preserve_lock", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "cell", name: "preserve_capacity", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "cell", name: "same_type", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "cell", name: "preserve_type", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "accounting", name: "conserved", arity: 2, allows_preserve_fields: false },
    StdlibSignature { namespace: "lifecycle", name: "transfer", arity: 3, allows_preserve_fields: true },
    StdlibSignature { namespace: "receipt", name: "claim", arity: 3, allows_preserve_fields: true },
    StdlibSignature { namespace: "lifecycle", name: "settle", arity: 3, allows_preserve_fields: true },
];

pub fn lookup(namespace: &str, name: &str) -> Option<&'static StdlibSignature> {
    SIGNATURES.iter().find(|signature| signature.namespace == namespace && signature.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_unique_qualified_names() {
        let mut names = SIGNATURES.iter().map(|signature| signature.qualified_name()).collect::<Vec<_>>();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), SIGNATURES.len());
    }
}

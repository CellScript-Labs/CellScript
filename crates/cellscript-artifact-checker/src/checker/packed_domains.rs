//! Nominal alias equivalence cannot erase a V0 commitment's spelling domain.
use crate::interface::SourceType;

pub(crate) fn same(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    fn domains(ty: &SourceType, result: &mut Vec<SourceType>) {
        match ty {
            SourceType::Named(name, arguments) => {
                if matches!(name.as_str(), "Commitment" | "Opening") {
                    result.push(ty.clone());
                }
                for argument in arguments {
                    if let crate::interface::SourceArgument::Type(ty) = argument {
                        domains(ty, result);
                    }
                }
            }
            SourceType::Array(inner, _) | SourceType::Reference { inner, .. } => domains(inner, result),
            SourceType::Tuple(items) => items.iter().for_each(|ty| domains(ty, result)),
        }
    }
    let (Ok(left), Ok(right)) = (crate::interface::parse_source_type(left), crate::interface::parse_source_type(right)) else {
        return false;
    };
    let mut a = Vec::new();
    let mut b = Vec::new();
    domains(&left, &mut a);
    domains(&right, &mut b);
    a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_leaf_normalization_never_normalizes_a_packed_domain() {
        assert!(same("Hash", "hash"));
        assert!(same("(Hash, ())", "(hash, unit)"));
        assert!(same("Commitment<(Hash, ())>", "Commitment<(Hash, ())>"));
        for (left, right) in [
            ("Commitment<First>", "Commitment<Alias>"),
            ("Commitment<Hash>", "Commitment<hash>"),
            ("Commitment<()>", "Commitment<unit>"),
            ("(u64, Commitment<First>)", "(u64, Commitment<Alias>)"),
            ("[Commitment<Hash>; 1]", "[Commitment<hash>; 1]"),
            ("&Commitment<First>", "&Commitment<Alias>"),
        ] {
            assert!(!same(left, right), "{left} / {right}");
        }
    }
}

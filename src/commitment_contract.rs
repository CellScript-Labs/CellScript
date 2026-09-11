//! Stable names and string-level helpers for the fixed-width committed-state
//! profile.  Keep these helpers independent of the AST and IR so every
//! compiler layer recognizes exactly the same nominal wrappers.

pub(crate) const COMMITMENT_TYPE: &str = "Commitment";
pub(crate) const OPENING_TYPE: &str = "Opening";
pub(crate) const COMMIT_FUNCTION: &str = "commitment::commit";
pub(crate) const OPEN_FUNCTION: &str = "commitment::open";
pub(crate) const PACKED_HASH_DOMAIN_PREFIX: &[u8] = b"CellScriptPackedHashV0\0";
pub(crate) const PACKED_HASH_SCRATCH_BYTES: usize = 512;

pub(crate) fn commitment_inner_type(name: &str) -> Option<&str> {
    unary_type_argument(name, COMMITMENT_TYPE)
}

pub(crate) fn opening_inner_type(name: &str) -> Option<&str> {
    unary_type_argument(name, OPENING_TYPE)
}

pub(crate) fn commitment_type(inner: &str) -> String {
    format!("{COMMITMENT_TYPE}<{inner}>")
}

fn unary_type_argument<'a>(name: &'a str, wrapper: &str) -> Option<&'a str> {
    let inner = name.strip_prefix(wrapper)?.strip_prefix('<')?.strip_suffix('>')?.trim();
    if inner.is_empty() || has_top_level_comma(inner) {
        return None;
    }
    Some(inner)
}

fn has_top_level_comma(value: &str) -> bool {
    let mut angle = 0usize;
    let mut square = 0usize;
    let mut paren = 0usize;
    for ch in value.chars() {
        match ch {
            '<' => angle += 1,
            '>' => angle = angle.saturating_sub(1),
            '[' => square += 1,
            ']' => square = square.saturating_sub(1),
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            ',' if angle == 0 && square == 0 && paren == 0 => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_exact_unary_wrappers() {
        assert_eq!(commitment_inner_type("Commitment<State>"), Some("State"));
        assert_eq!(opening_inner_type("Opening<[u8; 32]>"), Some("[u8; 32]"));
        assert_eq!(commitment_inner_type("Commitment<State, Hash>"), None);
        assert_eq!(opening_inner_type("Opening<>"), None);
        assert_eq!(opening_inner_type("Other<State>"), None);
    }
}

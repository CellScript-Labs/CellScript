//! Shared view-type spelling only. Parsing does not establish source identity,
//! ownership, runtime evidence or an admissible extent; each consumer checks
//! those obligations independently at its own boundary.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessBytesOwner {
    Raw,
    Lock,
    Entry,
    OutputType,
}

impl WitnessBytesOwner {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Lock => "lock",
            Self::Entry => "entry",
            Self::OutputType => "output_type",
        }
    }

    pub const fn tag(self) -> u64 {
        match self {
            Self::Raw => 0,
            Self::Lock => 1,
            Self::Entry => 2,
            Self::OutputType => 3,
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "raw" => Some(Self::Raw),
            "lock" => Some(Self::Lock),
            "entry" => Some(Self::Entry),
            "output_type" => Some(Self::OutputType),
            _ => None,
        }
    }
}

/// Existing source/IR spellings are exact, while checker records trim the two
/// arguments. Neither mode trims the outer type name or accepts extra tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessViewSyntax {
    ExactArguments,
    TrimArguments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WitnessBytesViewType {
    pub owner: WitnessBytesOwner,
    pub maximum: u64,
}

/// Parse borrowed spelling without allocation. The caller's source/metadata
/// byte budget must already hold; this function does not impose an admission
/// extent or silently canonicalize the accepted numeric spelling.
pub fn parse_witness_bytes_view_type(name: &str, syntax: WitnessViewSyntax) -> Option<WitnessBytesViewType> {
    let payload = name.strip_prefix("WitnessBytesView<")?.strip_suffix('>')?;
    let (owner, maximum) = payload.split_once(',')?;
    let (owner, maximum) = match syntax {
        WitnessViewSyntax::ExactArguments => (owner, maximum),
        WitnessViewSyntax::TrimArguments => (owner.trim(), maximum.trim()),
    };
    Some(WitnessBytesViewType { owner: WitnessBytesOwner::parse(owner)?, maximum: maximum.parse().ok()? })
}

pub const WITNESS_BYTES_VIEW_CONFORMANCE_VECTORS: &str = include_str!("view_types_vectors.json");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn witness_view_syntax_matches_explicit_acceptance_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(WITNESS_BYTES_VIEW_CONFORMANCE_VECTORS).unwrap();
        for vector in vectors["cases"].as_array().unwrap() {
            let input = vector["input"].as_str().unwrap();
            for (mode, field) in [(WitnessViewSyntax::ExactArguments, "exact"), (WitnessViewSyntax::TrimArguments, "trimmed")] {
                let actual = parse_witness_bytes_view_type(input, mode);
                assert_eq!(actual.is_some(), vector[field].as_bool().unwrap(), "{input:?}/{mode:?}");
                if let Some(actual) = actual {
                    assert_eq!(actual.owner.as_str(), vector["owner"].as_str().unwrap());
                    assert_eq!(actual.owner.tag(), vector["tag"].as_u64().unwrap());
                    assert_eq!(actual.maximum, vector["maximum"].as_u64().unwrap());
                }
            }
        }
    }
}

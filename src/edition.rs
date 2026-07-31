use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::CompileError;

/// CellScript source-language edition.
///
/// Editions are a closed set. A package must opt into the current edition
/// explicitly in `Cell.toml`; missing or unknown editions are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CellScriptEdition {
    #[serde(rename = "2026")]
    Edition2026,
}

pub const CURRENT_EDITION: CellScriptEdition = CellScriptEdition::Edition2026;

impl Default for CellScriptEdition {
    fn default() -> Self {
        CURRENT_EDITION
    }
}

impl CellScriptEdition {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edition2026 => "2026",
        }
    }

    pub fn resolve_compatibility_profile(
        self,
        target_profile: &str,
        primitive_assurance: Option<&str>,
    ) -> ResolvedCompatibilityProfile {
        let primitive_assurance = primitive_assurance.unwrap_or("default").to_string();
        ResolvedCompatibilityProfile {
            id: format!(
                "cellscript-edition-{}-{}-witnessargs-input-type-v2-csargv1-{}",
                self.as_str(),
                target_profile,
                primitive_assurance
            ),
            edition: self,
            source_semantics: "cellscript-source-semantics-2026".to_string(),
            target_profile: target_profile.to_string(),
            primitive_assurance,
            entry_witness_payload_abi: crate::ENTRY_WITNESS_ABI.to_string(),
            entry_witness_placement_abi: crate::ENTRY_WITNESS_PLACEMENT_ABI.to_string(),
            entry_witness_placement_field: crate::ENTRY_WITNESS_PLACEMENT_FIELD.to_string(),
            entry_witness_placement_source: crate::ENTRY_WITNESS_PLACEMENT_SOURCE.to_string(),
            raw_entry_witness_payload_compatible: false,
        }
    }
}

impl fmt::Display for CellScriptEdition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CellScriptEdition {
    type Err = CompileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "2026" => Ok(Self::Edition2026),
            other => Err(CompileError::without_span(format!("unsupported CellScript edition '{}'; expected 2026", other))),
        }
    }
}

/// Fully resolved compile-time compatibility contract.
///
/// The edition selects language semantics and safe defaults. Wire contracts
/// remain independently named because CKB-VM cannot read `Cell.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCompatibilityProfile {
    pub id: String,
    pub edition: CellScriptEdition,
    pub source_semantics: String,
    pub target_profile: String,
    pub primitive_assurance: String,
    pub entry_witness_payload_abi: String,
    pub entry_witness_placement_abi: String,
    pub entry_witness_placement_field: String,
    pub entry_witness_placement_source: String,
    pub raw_entry_witness_payload_compatible: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_edition_2026_is_accepted() {
        assert_eq!("2026".parse::<CellScriptEdition>().unwrap(), CellScriptEdition::Edition2026);
        assert!("unsupported".parse::<CellScriptEdition>().unwrap_err().message.contains("expected 2026"));
    }

    #[test]
    fn serde_uses_the_manifest_year() {
        assert_eq!(serde_json::to_string(&CURRENT_EDITION).unwrap(), "\"2026\"");
        assert_eq!(serde_json::from_str::<CellScriptEdition>("\"2026\"").unwrap(), CURRENT_EDITION);
        assert!(serde_json::from_str::<CellScriptEdition>("\"unsupported\"").is_err());
    }
}

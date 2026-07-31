use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::CompileError;

pub const COMPATIBILITY_PROFILE_SCHEMA: &str = "cellscript-resolved-compatibility-profile-v1";

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

    /// Stable source-language semantics selected by this edition.
    ///
    /// Target, wire ABI, assurance, metadata, and compiler release versions
    /// are deliberately not edition properties. They are independent axes
    /// assembled by [`resolve_compatibility_profile`].
    pub const fn source_semantics(self) -> &'static str {
        match self {
            Self::Edition2026 => "cellscript-source-semantics-2026",
        }
    }
}

/// Resolve the complete compile-time compatibility contract from independent
/// version axes.
///
/// The edition contributes source semantics only. Target behavior, primitive
/// assurance, metadata schemas, and entry/witness wire ABIs retain their own
/// version identities so that any of them can advance without inventing a new
/// source edition.
pub fn resolve_compatibility_profile(
    edition: CellScriptEdition,
    target_profile: &str,
    primitive_assurance: Option<&str>,
) -> ResolvedCompatibilityProfile {
    let primitive_assurance = primitive_assurance.unwrap_or("default").to_string();
    let source_semantics = edition.source_semantics().to_string();
    ResolvedCompatibilityProfile {
        schema: COMPATIBILITY_PROFILE_SCHEMA.to_string(),
        id: format!(
            "{}-{}-target-{}-primitive-{}-entry-{}-placement-{}-metadata-{}-{}-{}-{}",
            COMPATIBILITY_PROFILE_SCHEMA,
            source_semantics,
            target_profile,
            primitive_assurance,
            crate::ENTRY_WITNESS_ABI,
            crate::ENTRY_WITNESS_PLACEMENT_ABI,
            crate::METADATA_SCHEMA_VERSION,
            crate::SOURCE_METADATA_SCHEMA_VERSION,
            crate::ARTIFACT_METADATA_SCHEMA_VERSION,
            crate::CONSTRAINTS_METADATA_SCHEMA_VERSION,
        ),
        edition,
        source_semantics,
        target_profile: target_profile.to_string(),
        primitive_assurance,
        metadata_schema_version: crate::METADATA_SCHEMA_VERSION,
        source_metadata_schema_version: crate::SOURCE_METADATA_SCHEMA_VERSION,
        artifact_metadata_schema_version: crate::ARTIFACT_METADATA_SCHEMA_VERSION,
        constraints_metadata_schema_version: crate::CONSTRAINTS_METADATA_SCHEMA_VERSION,
        entry_witness_payload_abi: crate::ENTRY_WITNESS_ABI.to_string(),
        entry_witness_placement_abi: crate::ENTRY_WITNESS_PLACEMENT_ABI.to_string(),
        entry_witness_placement_field: crate::ENTRY_WITNESS_PLACEMENT_FIELD.to_string(),
        entry_witness_placement_source: crate::ENTRY_WITNESS_PLACEMENT_SOURCE.to_string(),
        raw_entry_witness_payload_compatible: false,
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
/// The edition contributes source semantics. Target, assurance, metadata, and
/// wire contracts remain independently named because they evolve on separate
/// schedules and CKB-VM cannot read `Cell.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCompatibilityProfile {
    pub schema: String,
    pub id: String,
    pub edition: CellScriptEdition,
    pub source_semantics: String,
    pub target_profile: String,
    pub primitive_assurance: String,
    pub metadata_schema_version: u32,
    pub source_metadata_schema_version: u32,
    pub artifact_metadata_schema_version: u32,
    pub constraints_metadata_schema_version: u32,
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

    #[test]
    fn edition_owns_source_semantics_only() {
        assert_eq!(CURRENT_EDITION.source_semantics(), "cellscript-source-semantics-2026");
    }

    #[test]
    fn compatibility_profile_composes_independent_version_axes() {
        let profile = resolve_compatibility_profile(CURRENT_EDITION, "ckb", Some("0.16"));

        assert_eq!(profile.schema, COMPATIBILITY_PROFILE_SCHEMA);
        assert_eq!(profile.edition, CURRENT_EDITION);
        assert_eq!(profile.source_semantics, CURRENT_EDITION.source_semantics());
        assert_eq!(profile.target_profile, "ckb");
        assert_eq!(profile.primitive_assurance, "0.16");
        assert_eq!(profile.metadata_schema_version, crate::METADATA_SCHEMA_VERSION);
        assert_eq!(profile.source_metadata_schema_version, crate::SOURCE_METADATA_SCHEMA_VERSION);
        assert_eq!(profile.artifact_metadata_schema_version, crate::ARTIFACT_METADATA_SCHEMA_VERSION);
        assert_eq!(profile.constraints_metadata_schema_version, crate::CONSTRAINTS_METADATA_SCHEMA_VERSION);
        assert_eq!(profile.entry_witness_payload_abi, crate::ENTRY_WITNESS_ABI);
        assert_eq!(profile.entry_witness_placement_abi, crate::ENTRY_WITNESS_PLACEMENT_ABI);

        let other_assurance = resolve_compatibility_profile(CURRENT_EDITION, "ckb", Some("0.17"));
        assert_ne!(profile.id, other_assurance.id);
    }
}

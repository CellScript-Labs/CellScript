//! Lightweight policy declaration and outer-witness metadata.
//!
//! This is an authoring/builder contract. Shape validation here does not replace
//! independent bundle verification or prove authority for a caller-supplied hash.

use super::{ArtifactContext, ArtifactDeclaration};
use crate::error::{CompileError, Result};
use crate::ir::{IrEntrySelection, IrModule};
use crate::policy_witness::{
    encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord, MAX_POLICY_WITNESS_BYTES, MAX_POLICY_WITNESS_RECORDS,
    POLICY_WITNESS_ABI,
};
use crate::{CompileMetadata, EntryWitnessArg};
use serde::{Deserialize, Serialize};

pub const POLICY_ARTIFACT_METADATA_SCHEMA: &str = "cellscript-policy-artifact-v1";
pub const POLICY_WITNESS_PLACEMENT_ABI: &str = "cellscript-policy-witnessargs-input-type-v1";
pub const POLICY_WITNESS_PLACEMENT_FIELD: &str = "input_type";
pub const POLICY_WITNESS_PLACEMENT_SOURCE: &str = "group-input[0]-or-output[0]-if-no-inputs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyArtifactMetadata {
    pub schema: String,
    pub declaration: ArtifactDeclaration,
    pub max_records: usize,
    pub max_witness_bytes: usize,
    pub payload_abi: String,
    pub placement_abi: String,
    pub placement_field: String,
    pub placement_source: String,
}

impl PolicyArtifactMetadata {
    pub fn new(declaration: &ArtifactDeclaration) -> Result<Self> {
        Ok(Self {
            schema: POLICY_ARTIFACT_METADATA_SCHEMA.to_string(),
            declaration: declaration.canonicalized()?,
            max_records: MAX_POLICY_WITNESS_RECORDS,
            max_witness_bytes: MAX_POLICY_WITNESS_BYTES,
            payload_abi: POLICY_WITNESS_ABI.to_string(),
            placement_abi: POLICY_WITNESS_PLACEMENT_ABI.to_string(),
            placement_field: POLICY_WITNESS_PLACEMENT_FIELD.to_string(),
            placement_source: POLICY_WITNESS_PLACEMENT_SOURCE.to_string(),
        })
    }

    /// Validate the bounded metadata shape, not its binding to an executable.
    pub fn validate(&self) -> Result<()> {
        if self.schema != POLICY_ARTIFACT_METADATA_SCHEMA
            || self.max_records != MAX_POLICY_WITNESS_RECORDS
            || self.max_witness_bytes != MAX_POLICY_WITNESS_BYTES
            || self.payload_abi != POLICY_WITNESS_ABI
            || self.placement_abi != POLICY_WITNESS_PLACEMENT_ABI
            || self.placement_field != POLICY_WITNESS_PLACEMENT_FIELD
            || self.placement_source != POLICY_WITNESS_PLACEMENT_SOURCE
        {
            return Err(policy_metadata_error("policy artifact metadata has an unknown schema, ABI, placement, or limit"));
        }
        if self.declaration.canonicalized()? != self.declaration {
            return Err(policy_metadata_error("policy artifact action tags must be in canonical numeric order"));
        }
        Ok(())
    }
}

/// Bind the already-scoped artifact selection without changing per-action
/// CSARGv1 parameter metadata. The runtime/profile describe the outer envelope.
pub(crate) fn bind_policy_metadata(metadata: &mut CompileMetadata, module: &IrModule) -> Result<()> {
    let IrEntrySelection::Artifact(declaration) = &module.entry_selection else {
        return Ok(());
    };
    let policy = PolicyArtifactMetadata::new(declaration)?;
    policy.validate()?;
    for variant in &policy.declaration.actions {
        selected_action(metadata, &variant.action)?;
    }
    crate::edition::set_entry_compatibility_profile(
        &mut metadata.compatibility_profile,
        &policy.payload_abi,
        &policy.placement_abi,
        &policy.placement_field,
        &policy.placement_source,
    );
    metadata.runtime.policy_artifact = Some(policy);
    Ok(())
}

/// Encode a request for one explicitly exported policy action.
///
/// script_hash must be the complete hash of the intended deployed Script,
/// including its code_hash, hash_type, and args. It is opaque input here, not an
/// authentication proof. Callers requiring verified deployment/bundle identity
/// must validate that evidence before using this builder helper.
pub fn encode_policy_action_record(
    metadata: &CompileMetadata,
    script_hash: &[u8; 32],
    action: &str,
    args: &[EntryWitnessArg],
) -> Result<PolicyWitnessRecord> {
    let policy = metadata
        .runtime
        .policy_artifact
        .as_ref()
        .ok_or_else(|| policy_metadata_error("policy action encoding requires explicitly selected policy artifact metadata"))?;
    policy.validate()?;
    let profile = &metadata.compatibility_profile;
    if profile.entry_witness_payload_abi != policy.payload_abi
        || profile.entry_witness_placement_abi != policy.placement_abi
        || profile.entry_witness_placement_field != policy.placement_field
        || profile.entry_witness_placement_source != policy.placement_source
        || profile.raw_entry_witness_payload_compatible
    {
        return Err(policy_metadata_error("policy artifact metadata disagrees with its outer-witness compatibility profile"));
    }
    let variant = policy.declaration.action(action).ok_or_else(|| {
        policy_metadata_error(format!("action '{action}' is not an exported variant of policy artifact '{}'", policy.declaration.name))
    })?;
    let action_metadata = selected_action(metadata, action)?;
    let role = match &policy.declaration.context {
        ArtifactContext::TypeGroup { .. } => PolicyScriptRole::Type,
    };
    let record =
        PolicyWitnessRecord { role, script_hash: *script_hash, tag: variant.tag, args: action_metadata.entry_witness_args(args)? };
    // A single record must fit before a builder attempts to combine it with other
    // requests. Final placement additionally counts lock/output_type bytes.
    encode_policy_witness_bundle(std::slice::from_ref(&record)).map_err(|error| policy_metadata_error(error.to_string()))?;
    Ok(record)
}

fn selected_action<'a>(metadata: &'a CompileMetadata, name: &str) -> Result<&'a crate::ActionMetadata> {
    let mut matches = metadata.actions.iter().filter(|action| action.name == name);
    let action = matches.next().ok_or_else(|| policy_metadata_error(format!("policy action '{name}' is missing from metadata")))?;
    if matches.next().is_some() {
        return Err(policy_metadata_error(format!("policy action '{name}' is ambiguous in metadata")));
    }
    Ok(action)
}

fn policy_metadata_error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message).with_code(crate::error::DiagnosticCode::E2101)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::{ArtifactAction, ArtifactDispatch};

    const SOURCE: &str = r#"
module policy_metadata
resource Token has store { amount: u64 }
action mint() { verification require true }
action transfer(witness amount: u64) { verification require amount > 0 }
action retained(witness amount: u64) { verification require amount > 0 }
"#;

    fn declaration() -> ArtifactDeclaration {
        ArtifactDeclaration {
            name: "token_policy".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: vec![
                ArtifactAction { tag: 17, action: "transfer".to_string() },
                ArtifactAction { tag: 0, action: "mint".to_string() },
            ],
            common_checks: Vec::new(),
        }
    }

    fn metadata_and_module() -> (CompileMetadata, IrModule) {
        let metadata = crate::compile_metadata(SOURCE, crate::CURRENT_EDITION, None).unwrap();
        let ast = crate::frontend::parse(SOURCE, crate::CURRENT_EDITION).unwrap();
        let mut module = crate::ir::generate(&ast).unwrap();
        module.entry_selection = IrEntrySelection::Artifact(declaration());
        (metadata, module)
    }

    #[test]
    fn policy_binding_changes_only_the_outer_contract_and_preserves_inner_args() {
        let (mut metadata, module) = metadata_and_module();
        let old_profile = metadata.compatibility_profile.clone();
        let old_actions = serde_json::to_value(&metadata.actions).unwrap();
        let old_constraints = serde_json::to_value(&metadata.constraints.entry_abi).unwrap();
        bind_policy_metadata(&mut metadata, &module).unwrap();
        let policy = metadata.runtime.policy_artifact.as_ref().unwrap();
        policy.validate().unwrap();
        assert_eq!(policy.declaration.actions[0].tag, 0);
        assert_eq!(policy.declaration.actions[1].tag, 17);
        assert_eq!(metadata.compatibility_profile.entry_witness_payload_abi, POLICY_WITNESS_ABI);
        assert_eq!(metadata.compatibility_profile.entry_witness_placement_abi, POLICY_WITNESS_PLACEMENT_ABI);
        assert_eq!(metadata.compatibility_profile.entry_witness_placement_source, POLICY_WITNESS_PLACEMENT_SOURCE);
        assert_ne!(metadata.compatibility_profile.id, old_profile.id);
        assert_eq!(serde_json::to_value(&metadata.actions).unwrap(), old_actions);
        assert_eq!(serde_json::to_value(&metadata.constraints.entry_abi).unwrap(), old_constraints);
        let record = encode_policy_action_record(&metadata, &[0x24; 32], "transfer", &[EntryWitnessArg::U64(42)]).unwrap();
        assert_eq!(record.role, PolicyScriptRole::Type);
        assert_eq!(record.script_hash, [0x24; 32]);
        assert_eq!(record.tag, 17);
        assert_eq!(record.args, b"CSARGv1\0\x2a\0\0\0\0\0\0\0");
        assert!(encode_policy_action_record(&metadata, &[0x24; 32], "mint", &[]).unwrap().args.is_empty());
        assert!(encode_policy_action_record(&metadata, &[0x24; 32], "transfer", &[]).is_err());
        assert!(encode_policy_action_record(&metadata, &[0x24; 32], "mint", &[EntryWitnessArg::U64(42)]).is_err());
    }

    #[test]
    fn ordinary_metadata_is_unchanged_and_retained_actions_are_not_exports() {
        let (mut metadata, mut module) = metadata_and_module();
        module.entry_selection = IrEntrySelection::Action("transfer".to_string());
        let before = serde_json::to_value(&metadata).unwrap();
        bind_policy_metadata(&mut metadata, &module).unwrap();
        assert_eq!(serde_json::to_value(&metadata).unwrap(), before);
        assert!(encode_policy_action_record(&metadata, &[0; 32], "transfer", &[EntryWitnessArg::U64(42)]).is_err());
        module.entry_selection = IrEntrySelection::Artifact(declaration());
        bind_policy_metadata(&mut metadata, &module).unwrap();
        assert!(metadata.actions.iter().any(|action| action.name == "retained"));
        let error = encode_policy_action_record(&metadata, &[0; 32], "retained", &[EntryWitnessArg::U64(42)]).unwrap_err();
        assert!(error.message.contains("not an exported variant"));
    }

    #[test]
    fn policy_metadata_rejects_changed_contract_fields_and_ambiguous_lookups() {
        let (mut metadata, module) = metadata_and_module();
        bind_policy_metadata(&mut metadata, &module).unwrap();
        let base = metadata.runtime.policy_artifact.as_ref().unwrap().clone();
        let encoded = serde_json::to_value(&base).unwrap();
        for (field, value) in [
            ("schema", serde_json::json!("other")),
            ("max_records", serde_json::json!(9)),
            ("max_witness_bytes", serde_json::json!(4097)),
            ("payload_abi", serde_json::json!(crate::ENTRY_WITNESS_ABI)),
            ("placement_abi", serde_json::json!(crate::ENTRY_WITNESS_PLACEMENT_ABI)),
            ("placement_field", serde_json::json!("lock")),
            ("placement_source", serde_json::json!("input[0]")),
        ] {
            let mut changed = encoded.clone();
            changed[field] = value;
            assert!(serde_json::from_value::<PolicyArtifactMetadata>(changed).unwrap().validate().is_err(), "{field}");
        }
        let mut unknown_field = encoded;
        unknown_field["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<PolicyArtifactMetadata>(unknown_field).is_err());
        let mut unsorted = base.clone();
        unsorted.declaration.actions.reverse();
        assert!(unsorted.validate().is_err());
        let mut duplicate_tag = base;
        duplicate_tag.declaration.actions[1].tag = 0;
        assert!(duplicate_tag.validate().is_err());

        let duplicate = selected_action(&metadata, "transfer").unwrap().clone();
        metadata.actions.push(duplicate);
        assert!(encode_policy_action_record(&metadata, &[0; 32], "transfer", &[EntryWitnessArg::U64(42)])
            .unwrap_err()
            .message
            .contains("ambiguous"));
        metadata.actions.retain(|action| action.name != "transfer");
        assert!(encode_policy_action_record(&metadata, &[0; 32], "transfer", &[EntryWitnessArg::U64(42)])
            .unwrap_err()
            .message
            .contains("missing"));
    }
}

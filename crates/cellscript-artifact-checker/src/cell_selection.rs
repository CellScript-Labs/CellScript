//! Source-committed exact-role selection policy. Declaration checks and the
//! independent bounded instruction/CFG contract live here. Complete enclosing
//! memory effects and source equivalence remain separate admission obligations.
use serde::{Deserialize, Serialize};

pub(crate) mod layout;
pub(crate) mod machine;
pub mod transaction;
mod types;
pub use types::{parse_selection_type, project_role_type, SelectionType};

pub const PROFILE: &str = "cellscript-exact-role-cell-selection-fixed-v1";
pub const BOUNDED_PROFILE: &str = "cellscript-exact-role-cell-selection-bounded-v2";
pub const PROOF_BLOCKER: &str = "exact-role-selection-artifact-proof";

/// Invocation-local immutable catalog, never a serializable Cell resource.
pub fn selection_type(contract: &Contract) -> String {
    SelectionType {
        catalog: contract.names.as_ref().map(|names| names.catalog.clone()),
        names: contract.names.as_ref().map(|names| names.roles.clone()).unwrap_or_default(),
        schemas: contract.roles.iter().map(|role| role.type_name.clone()).collect(),
        selected: None,
    }
    .encode()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Read {
    InputCount,
    OutputCount,
    Input,
    Output,
    Successor,
}

impl Read {
    pub fn indexed(self) -> bool {
        !matches!(self, Self::InputCount | Self::OutputCount)
    }
    pub fn input(self) -> bool {
        !matches!(self, Self::OutputCount | Self::Output)
    }
    pub fn result_type(self, element: &str) -> String {
        match self {
            Self::InputCount | Self::OutputCount => "u64".into(),
            Self::Input => format!("InputView<{element}>"),
            Self::Output | Self::Successor => format!("OutputView<{element}>"),
        }
    }
}

/// Until all machine/source obligations close, rebinding hashes must not turn an
/// experimental selection opcode into a stable-admission claim.
pub(crate) fn validate_metadata(metadata: &serde_json::Value, typed: &crate::TypedSemanticRecord) -> Result<(), crate::CheckerError> {
    let contains = |ty: &str| crate::checker::canonical_abi_type(ty).contains("ExactCellSelection<");
    let invalid = || {
        crate::CheckerError::new(crate::CheckerRejectionCode::V2419TypedSemanticsInvalid, "exact-role selection handles must be locally constructed immutable values; storage, ABI, arithmetic and unrelated calls are forbidden")
    };
    for ty in &typed.types {
        if ty.fields.iter().any(|field| contains(&field.ty)) {
            return Err(invalid());
        }
    }
    for entry in &typed.entries {
        if entry.params.iter().any(|param| contains(&param.ty)) || contains(&entry.return_type) {
            return Err(invalid());
        }
        let mut selection_locals = std::collections::BTreeSet::new();
        for local in &entry.locals {
            if contains(&local.ty) && !local.ty.starts_with("ExactCellSelection<") {
                return Err(invalid());
            }
            if contains(&local.ty) {
                selection_locals.insert(local.id);
            }
        }
        for operation in entry.blocks.iter().flat_map(|block| &block.operations) {
            if operation.operands.iter().any(|operand| contains(&operand.ty))
                && !matches!(operation.opcode.as_str(), "move" | "field-access" | "read-exact-cell-role")
            {
                return Err(invalid());
            }
            if operation.destinations.iter().any(|id| selection_locals.contains(id))
                && !matches!(operation.opcode.as_str(), "move" | "field-access" | "verify-exact-cell-roles")
            {
                return Err(invalid());
            }
        }
    }
    let present = typed
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .any(|operation| matches!(operation.opcode.as_str(), "verify-exact-cell-roles" | "read-exact-cell-role"));
    if present
        && !metadata
            .get("runtime")
            .and_then(|runtime| runtime.get("fail_closed_runtime_features"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|features| features.iter().any(|feature| feature.as_str() == Some(PROOF_BLOCKER)))
    {
        return Err(crate::CheckerError::new(
            crate::CheckerRejectionCode::V2410MetadataBindingMismatch,
            "exact-role selection requires its independent-machine-proof admission blocker",
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "mode", content = "target_key", deny_unknown_fields)]
pub enum Correspondence {
    Preserve,
    Create,
    Burn,
    Merge(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    pub type_name: String,
    pub type_hash: [u8; 32],
    pub lock_hash: [u8; 32],
    pub identity_field: String,
    pub inputs: (u32, u32),
    pub outputs: (u32, u32),
    pub local_type: bool,
    pub correspondence: Correspondence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contract {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<CatalogNames>,
    pub roles: Vec<Role>,
    pub input_scan: u32,
    pub output_scan: u32,
    pub selected_per_side: u32,
    pub aggregate_bytes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogNames {
    pub catalog: String,
    pub roles: Vec<String>,
}

impl Contract {
    pub fn validate(&self) -> Result<(), &'static str> {
        if let Some(names) = &self.names
            && (!types::identifier(&names.catalog)
                || names.roles.len() != self.roles.len()
                || names.roles.iter().enumerate().any(|(index, name)| !types::identifier(name) || names.roles[..index].contains(name)))
        {
            return Err("named selection requires one catalog identifier and distinct role identifiers matching its schemas");
        }
        if !matches!(self.profile.as_str(), PROFILE | BOUNDED_PROFILE)
            || !(1..=8).contains(&self.roles.len())
            || !(1..=64).contains(&self.input_scan)
            || !(1..=64).contains(&self.output_scan)
            || !(1..=32).contains(&self.selected_per_side)
            || !(1..=32768).contains(&self.aggregate_bytes)
        {
            return Err("exact-role selection requires 1–8 roles, scan limits 1–64, selected limit 1–32 and aggregate bytes 1–32768");
        }
        for (index, role) in self.roles.iter().enumerate() {
            if role.type_name.is_empty()
                || role.identity_field.is_empty()
                || [role.inputs, role.outputs].iter().any(|&(min, max)| min > max || max > 32)
            {
                return Err(
                    "exact-role declarations require a nominal Cell type, an identity field and ordered count intervals in 0–32",
                );
            }
            if self.roles[..index].iter().any(|other| other.type_hash == role.type_hash) {
                return Err("exact-role Type hashes must be unique across the complete catalog");
            }
            if matches!(&role.correspondence, Correspondence::Merge(key) if !(1..=32).contains(&key.len())) {
                return Err("exact-role merge target must contain 1–32 key bytes");
            }
        }
        Ok(())
    }
}

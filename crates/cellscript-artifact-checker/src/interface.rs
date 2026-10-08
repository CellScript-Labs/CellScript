//! Versioned public-interface wire models shared without compiler dependencies.
//! Construction from source belongs to the compiler; checked projections belong
//! to the independent artifact boundary. The field/serialization order is kept
//! identical to the existing v3 package interface.

use crate::generic_projection::checked_source_type;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod generics;
mod layouts;
mod nominals;
mod projection;
mod source_types;
pub use projection::{project_bundle, CheckedModuleProjection};
mod universal;
pub(crate) use source_types::SourceType;
pub(crate) fn parse_source_type(value: &str) -> Result<SourceType, crate::CheckerError> {
    SourceType::parse(value)
}
pub(crate) use nominals::verify_catalog as verify_nominal_declarations;
pub use nominals::{qualified_source_type, qualified_source_type_with_parameters};

use crate::{CheckerBudgets, CheckerError, CheckerRejectionCode, CheckerReport, EvidenceState, TypedSemanticRecord};

/// A checked bundle's interface inputs. This is not a compatibility receipt or
/// deployment authorization: declared source API and effective typed contracts
/// deliberately remain separate. Only `inspect_bundle` constructs this value.
#[derive(Debug)]
pub struct InterfaceInspection {
    declared: PackageInterface,
    effective: TypedSemanticRecord,
    report: CheckerReport,
    bundle_bytes: [usize; 4],
}

impl InterfaceInspection {
    pub fn declared(&self) -> &PackageInterface {
        &self.declared
    }

    /// Includes inferred effects, concrete layouts, entry bindings and source
    /// provenance checked against the lowering/machine boundary. In particular,
    /// a source declaration's default effect need not equal its inferred effect.
    pub fn effective(&self) -> &TypedSemanticRecord {
        &self.effective
    }

    pub fn report(&self) -> &CheckerReport {
        &self.report
    }

    /// Project qualified source API and its retained effective bindings. This
    /// is not package ownership, codec admission or deployment authorization.
    pub fn project_module_contract(&self) -> Result<CheckedModuleProjection, CheckerError> {
        projection::project(self)
    }

    /// Check universal type/ability guarantees, including absent templates.
    /// This prerequisite does not establish execution availability, compatibility
    /// or deployment admission; historical bundles without catalogs reject here.
    pub fn validate_symbolic_declarations(&self) -> Result<(), CheckerError> {
        universal::verify(self)
    }
}

/// Inspect actual, bounded bundle bytes before constructing receipt projections.
///
/// The declaration model must round-trip without dropping unknown fields or
/// synthesizing absent defaults. This preserves its existing canonical hash.
/// No claim of interface compatibility, source equivalence, peer execution,
/// network availability or deployment authority is made by this operation.
pub fn inspect_bundle(
    artifact: &[u8],
    metadata_bytes: &[u8],
    lowering_bytes: &[u8],
    source_map_bytes: &[u8],
    budgets: &CheckerBudgets,
) -> Result<InterfaceInspection, CheckerError> {
    // The checker policy has no distinct metadata budget. This consumer applies
    // its record-byte ceiling before parsing or cloning metadata.
    if metadata_bytes.len() as u64 > budgets.record_bytes {
        return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "interface metadata exceeds record byte budget"));
    }
    let report = crate::check_bundle(artifact, metadata_bytes, lowering_bytes, source_map_bytes, budgets)?;
    if [
        report.binding_verification,
        report.structural_verification,
        report.lowering_record_verification,
        report.typed_semantics_verification,
    ]
    .iter()
    .any(|state| *state != EvidenceState::Verified)
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "interface inspection requires complete checked artifact evidence",
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(metadata_bytes)
        .map_err(|_| CheckerError::new(CheckerRejectionCode::V2401MalformedJson, "invalid interface metadata JSON"))?;
    let raw = metadata.get("public_interface").ok_or_else(|| {
        CheckerError::new(CheckerRejectionCode::V2410MetadataBindingMismatch, "checked metadata has no public interface")
    })?;
    let declared: PackageInterface = serde_json::from_value(raw.clone())
        .map_err(|_| CheckerError::new(CheckerRejectionCode::V2410MetadataBindingMismatch, "invalid public interface wire model"))?;
    let modeled = serde_json::to_value(&declared)
        .map_err(|_| CheckerError::new(CheckerRejectionCode::V2401MalformedJson, "public interface serialization failed"))?;
    if modeled != *raw {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "public interface model would discard unknown fields or synthesize missing fields",
        ));
    }
    let item_count = declared.types.len().saturating_add(declared.constants.len()).saturating_add(declared.callables.len());
    if item_count > budgets.entries as usize {
        return Err(CheckerError::new(CheckerRejectionCode::V2400BudgetExceeded, "public interface exceeds item count budget"));
    }
    let effective = crate::parse_lowering_record(lowering_bytes, budgets)?.typed_semantics;
    check_declaration_bindings(&declared, &metadata, &effective)?;
    Ok(InterfaceInspection {
        declared,
        effective,
        report,
        bundle_bytes: [artifact.len(), metadata_bytes.len(), lowering_bytes.len(), source_map_bytes.len()],
    })
}

fn mismatch(message: &'static str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2410MetadataBindingMismatch, message)
}

fn declaration_hash(value: &impl Serialize) -> Result<String, CheckerError> {
    let value = serde_json::to_value(value).map_err(|_| mismatch("interface digest value cannot be serialized"))?;
    let bytes = serde_json::to_vec(&crate::checker::canonical_json_value(&value))
        .map_err(|_| mismatch("interface digest value cannot be canonicalized"))?;
    Ok(crate::hex_encode(&crate::ckb_blake2b256(&bytes)))
}

/// Bind every existing declaration digest to its constituent fields. These
/// checks do not equate source-declared effects with inferred execution effects;
/// the latter remain in the separately checked typed semantic record.
fn check_declaration_bindings(
    interface: &PackageInterface,
    metadata: &serde_json::Value,
    effective: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    let instances = crate::generic_projection::type_instances(effective);
    let mut module = b"cellscript-module-identity-v1\0".to_vec();
    module.extend_from_slice(interface.module.as_bytes());
    if interface.module_identity != format!("blake2b:{}", crate::hex_encode(&crate::ckb_blake2b256(&module))) {
        return Err(mismatch("interface module identity does not match its module"));
    }
    for ty in &interface.types {
        if ty.identity != format!("{}::{}", interface.module, ty.name)
            || ty.layout_identity != declaration_hash(&(&ty.kind, &ty.fields, &ty.variants))?
        {
            return Err(mismatch("interface type identity or layout digest differs from its declaration"));
        }
        // Uninstantiated templates have no concrete layout. Concrete offsets
        // and widths must instead agree with the independently checked record;
        // recomputing a declaration's own layout digest is not sufficient.
        let checked = effective.types.iter().find(|entry| entry.name == ty.name);
        let mut names = ty.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>();
        names.sort_unstable();
        if names.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(mismatch("interface has duplicate fields in checked concrete layout"));
        }
        if let Some(checked) = checked {
            // Source declarations retain the author's capability order, while
            // checked concrete types use canonical order. Compare the complete
            // multiset so a duplicate cannot disappear through deduplication.
            let mut capabilities = ty.cell_capabilities.clone();
            capabilities.sort_unstable();
            if capabilities != checked.capabilities {
                return Err(mismatch("interface Cell capabilities differ from checked concrete type"));
            }
            if ty.value_abilities != checked.value_abilities {
                return Err(mismatch("interface value abilities differ from checked concrete type"));
            }
            let mut checked_names = checked.fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>();
            checked_names.sort_unstable();
            if ty.kind != checked.kind || names != checked_names {
                return Err(mismatch("interface field set or kind differs from checked concrete layout"));
            }
            if ty.variants.len() != checked.variants.len()
                || ty.variants.iter().zip(&checked.variants).enumerate().any(|(index, (declared, concrete))| {
                    declared.name != concrete.name
                        || concrete.tag as usize != index
                        || declared.fields.len() != concrete.fields.len()
                        || declared.fields.iter().zip(&concrete.fields).any(|(declared, concrete)| {
                            crate::checker::canonical_abi_type(declared) != checked_source_type(&concrete.ty, &instances)
                        })
                })
            {
                return Err(mismatch("interface variant set differs from checked concrete layout"));
            }
        }
        for field in &ty.fields {
            let layout = checked.and_then(|entry| entry.fields.iter().find(|candidate| candidate.name == field.name));
            if field.offset != layout.map(|field| field.offset as usize)
                || field.encoded_size != layout.and_then(|field| field.width_bytes.map(|width| width as usize))
                || layout.is_some_and(|layout| {
                    crate::checker::canonical_abi_type(&field.r#type) != checked_source_type(&layout.ty, &instances)
                })
            {
                return Err(mismatch("interface field layout differs from checked concrete layout"));
            }
        }
    }
    for constant in &interface.constants {
        if constant.identity != format!("{}::{}", interface.module, constant.name) {
            return Err(mismatch("interface constant identity differs from its declaration"));
        }
    }
    for callable in &interface.callables {
        if callable.identity != format!("{}::{}", interface.module, callable.name) {
            return Err(mismatch("interface callable identity differs from its declaration"));
        }
        check_callable_binding(callable, effective, &instances)?;
        let family = match callable.kind.as_str() {
            "action" => "actions",
            "function" => "functions",
            "lock" => "locks",
            _ => return Err(mismatch("interface callable kind is not supported")),
        };
        let expected_witness = (callable.kind != "function").then_some(interface.runtime_contract.witness_abi.as_str());
        if callable.entry_witness_abi.as_deref() != expected_witness {
            return Err(mismatch("interface callable witness ABI differs from its runtime contract"));
        }
        let entries = metadata[family].as_array().ok_or_else(|| mismatch("interface callable metadata family is missing"))?;
        let matching: Vec<_> = entries.iter().filter(|entry| entry["name"].as_str() == Some(&callable.name)).collect();
        let expected = match matching.as_slice() {
            [entry] => {
                let params = entry.get("params").ok_or_else(|| mismatch("interface callable parameters are missing"))?;
                let empty = serde_json::json!([]);
                let requirements = entry.get("transaction_runtime_input_requirements").unwrap_or(&empty);
                declaration_hash(&(params, requirements))?
            }
            [] if callable.kind == "action" => declaration_hash(&(&callable.params, &callable.outputs))?,
            [] => declaration_hash(&callable.params)?,
            _ => return Err(mismatch("interface callable metadata is ambiguous")),
        };
        if callable.builder_contract_hash != expected {
            return Err(mismatch("interface callable builder digest differs from checked metadata"));
        }
    }
    if interface.builder_contract_hash
        != declaration_hash(&interface.callables.iter().map(|item| (&item.identity, &item.builder_contract_hash)).collect::<Vec<_>>())?
    {
        return Err(mismatch("interface builder digest differs from its callable digests"));
    }
    let runtime = &interface.runtime_contract;
    for (key, value) in [
        ("target_profile", &runtime.target_profile),
        ("vm_abi", &runtime.vm_abi),
        ("witness_abi", &runtime.witness_abi),
        ("lock_args_abi", &runtime.lock_args_abi),
        ("source_encoding", &runtime.source_encoding),
        ("spawn_ipc_abi", &runtime.spawn_ipc_abi),
    ] {
        let metadata_key = if key == "target_profile" { "name" } else { key };
        if metadata["target_profile"][metadata_key].as_str() != Some(value.as_str()) {
            return Err(mismatch("interface runtime contract differs from checked target profile"));
        }
    }
    if metadata["compatibility_profile"]["id"].as_str() != Some(runtime.compatibility_profile_id.as_str()) {
        return Err(mismatch("interface compatibility profile differs from checked metadata"));
    }
    let since = metadata["target_profile"]["since_abi"].as_str().ok_or_else(|| mismatch("target temporal ABI is missing"))?;
    if runtime.temporal != temporal_contract(since) {
        return Err(mismatch("interface temporal contract differs from the versioned target ABI"));
    }
    if interface.deployment_contract_hash != declaration_hash(runtime)? {
        return Err(mismatch("interface deployment digest differs from its runtime contract"));
    }
    generics::check(interface, effective, &instances)?;
    nominals::check_public(interface, effective)?;
    Ok(())
}

/// Source outputs become trailing execution parameters. Compare their complete
/// ordered signature, not their encoded widths or producer-supplied hashes.
/// Entries pruned by artifact selection and uninstantiated public templates
/// remain declarations only; inspecting them is not executable API admission.
fn check_callable_binding(
    callable: &InterfaceCallable,
    effective: &TypedSemanticRecord,
    instances: &BTreeMap<&str, String>,
) -> Result<(), CheckerError> {
    let mut matching = effective.entries.iter().filter(|entry| entry.name == callable.name);
    let Some(entry) = matching.next() else { return Ok(()) };
    if matching.next().is_some() {
        return Err(mismatch("interface callable kind differs from checked entry"));
    }
    check_callable_entry(callable, entry, instances, &BTreeMap::new())
}

fn check_callable_entry(
    callable: &InterfaceCallable,
    entry: &crate::TypedSemanticEntry,
    instances: &BTreeMap<&str, String>,
    substitutions: &BTreeMap<&str, String>,
) -> Result<(), CheckerError> {
    let expected_kind = match callable.kind.as_str() {
        "function" => "helper",
        "action" => "action",
        "lock" => "lock",
        _ => return Err(mismatch("interface callable kind is not supported")),
    };
    if entry.kind != expected_kind {
        return Err(mismatch("interface callable kind differs from checked entry"));
    }
    if callable.params.len().saturating_add(callable.outputs.len()) != entry.params.len() {
        return Err(mismatch("interface callable parameter count differs from checked entry"));
    }
    for (declared, checked) in callable.params.iter().chain(&callable.outputs).zip(&entry.params) {
        // The existing source interface spells this one enum variant with an
        // underscore; typed records use the lowercase Rust variant spelling.
        let source = if declared.source == "lock_args" { "lockargs" } else { &declared.source };
        if declared.name != checked.name
            || !generics::matches_type(&declared.r#type, &checked.ty, instances, substitutions)
            || source != checked.source
            || declared.mutable != checked.mutable
            || declared.reference != checked.reference
        {
            return Err(mismatch("interface callable parameter differs from checked entry"));
        }
    }
    if !generics::matches_type(callable.return_type.as_deref().unwrap_or("unit"), &entry.return_type, instances, substitutions) {
        return Err(mismatch("interface callable return type differs from checked entry"));
    }
    // Source effect annotations and the effective inference are distinct (a
    // default Pure declaration can lower to ReadOnly). Preserve both rather
    // than accepting the source label as the machine's effect guarantee.
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageInterface {
    pub schema: String,
    pub version: u32,
    pub module: String,
    pub module_identity: String,
    pub edition: String,
    pub visibility_default: String,
    pub types: Vec<InterfaceType>,
    pub constants: Vec<InterfaceConstant>,
    pub callables: Vec<InterfaceCallable>,
    pub runtime_contract: InterfaceRuntimeContract,
    pub builder_contract_hash: String,
    pub deployment_contract_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceType {
    pub identity: String,
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub type_parameters: Vec<InterfaceTypeParameter>,
    pub value_abilities: Vec<String>,
    pub cell_capabilities: Vec<String>,
    pub fields: Vec<InterfaceField>,
    pub variants: Vec<InterfaceVariant>,
    pub layout_identity: String,
    pub type_identity: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceTypeParameter {
    pub name: String,
    pub phantom: bool,
    pub constraints: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceField {
    pub name: String,
    pub r#type: String,
    pub offset: Option<usize>,
    pub encoded_size: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceVariant {
    pub name: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceConstant {
    pub identity: String,
    pub name: String,
    pub visibility: String,
    pub r#type: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceCallable {
    pub identity: String,
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub type_parameters: Vec<InterfaceTypeParameter>,
    pub params: Vec<InterfaceParam>,
    pub return_type: Option<String>,
    pub outputs: Vec<InterfaceParam>,
    pub effect: String,
    pub entry_witness_abi: Option<String>,
    pub builder_contract_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceParam {
    pub name: String,
    pub r#type: String,
    pub source: String,
    pub mutable: bool,
    pub reference: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceRuntimeContract {
    pub target_profile: String,
    pub vm_abi: String,
    pub witness_abi: String,
    pub lock_args_abi: String,
    pub source_encoding: String,
    pub spawn_ipc_abi: String,
    pub compatibility_profile_id: String,
    #[serde(default)]
    pub temporal: InterfaceTemporalContract,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceTemporalContract {
    pub schema: String,
    pub wire_representation: String,
    pub since_abi: String,
    pub constructors: Vec<String>,
    pub decoder: String,
    pub domains: Vec<String>,
    pub migration: String,
}

pub const TEMPORAL_INTERFACE_SCHEMA: &str = "cellscript-ckb-temporal-interface-v1";

pub fn temporal_contract(since_abi: &str) -> InterfaceTemporalContract {
    InterfaceTemporalContract {
        schema: TEMPORAL_INTERFACE_SCHEMA.to_string(),
        wire_representation: "fixed-u64-register-and-little-endian-wire".to_string(),
        since_abi: since_abi.to_string(),
        constructors: vec![
            "ckb::since_absolute_block(u64)->AbsoluteBlockSince".to_string(),
            "ckb::since_absolute_epoch(u64,u64,u64)->AbsoluteEpochSince".to_string(),
            "ckb::since_absolute_timestamp(u64-seconds)->AbsoluteTimestampSince".to_string(),
            "ckb::since_relative_block(u64)->RelativeBlockSince".to_string(),
            "ckb::since_relative_epoch(u64,u64,u64)->RelativeEpochSince".to_string(),
            "ckb::since_relative_timestamp(u64-seconds)->RelativeTimestampSince".to_string(),
        ],
        decoder: "ckb::since_decode(EncodedSince)->DecodedSince;ckb::since_from_raw_checked(u64)->DecodedSince".to_string(),
        domains: vec![
            "EpochNumber".to_string(),
            "EpochDuration".to_string(),
            "BlockNumber".to_string(),
            "EpochLength".to_string(),
            "TimestampMillis".to_string(),
            "EncodedSince".to_string(),
            "DecodedSince".to_string(),
            "AbsoluteBlockSince".to_string(),
            "AbsoluteEpochSince".to_string(),
            "AbsoluteTimestampSince".to_string(),
            "RelativeBlockSince".to_string(),
            "RelativeEpochSince".to_string(),
            "RelativeTimestampSince".to_string(),
        ],
        migration: "legacy-raw-ckb-temporal-to-explicit-typed-v1".to_string(),
    }
}

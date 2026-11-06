use crate::elf::{parse_elf, DecodedControlFlowKind, ElfErrorKind, ElfParseError, ElfSummary, ParsedElf};
use crate::schema::*;
use crate::{ckb_blake2b256, hex_encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod isa;
use isa::{is_add, is_and, is_bgeu, is_bltu, is_lui, is_mul, is_sb, is_srli, is_sw};
pub(crate) use isa::{
    is_addi, is_auipc, is_beq, is_bne, is_jal_zero, is_jalr_call, is_lbu, is_ld, is_or, is_sd, is_slli, is_sltu, is_sub,
};

mod source_map;
#[cfg(test)]
use source_map::safe_source_path;
use source_map::validate_source_map;

mod commitment_materialization;

mod record_graph;
use record_graph::*;
mod elf_structure;
use elf_structure::*;
mod script_hash_machine;
pub(crate) use script_hash_machine::validate_script_hash_helper;
use script_hash_machine::*;
mod commitment_machine;
pub(crate) use commitment_machine::committed_state_type_width;
use commitment_machine::*;
mod policy_dispatch;
use policy_dispatch::*;
pub(crate) use policy_dispatch::{
    block_for_address, entry_start, flow_targets, jump_targets_runtime_error, last_register_definition_before,
    register_constant_before, stack_address_offset, unique_policy_block,
};
mod successor_relations;
mod typed_semantics;
use typed_semantics::*;
pub(crate) use typed_semantics::{
    canonical_abi_type, is_nested_source_type, nested_parameter_contract, nested_source_parts, nested_value_contract,
    same_nested_type, same_nominal_type,
};

mod packed_domains;
pub(crate) use packed_domains::same as same_packed_domains;
mod zk;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckerRejectionCode {
    V2400BudgetExceeded,
    V2401MalformedJson,
    V2402NonCanonicalJson,
    V2403UnsupportedSchema,
    V2404CanonicalOrder,
    V2405ReferentialIntegrity,
    V2406CfgInvalid,
    V2407AbiOrStackInvalid,
    V2408ProofCoverageInvalid,
    V2409ArtifactIdentityMismatch,
    V2410MetadataBindingMismatch,
    V2411ElfFormatInvalid,
    V2412ElfSectionInvalid,
    V2413InstructionInvalid,
    V2414ControlFlowInvalid,
    V2415BlockDigestMismatch,
    V2416SourceMapInvalid,
    V2417SyscallContractInvalid,
    V2418RecursionPolicyInvalid,
    V2419TypedSemanticsInvalid,
    V2420TypedMachineBindingInvalid,
}

impl CheckerRejectionCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V2400BudgetExceeded => "V2400",
            Self::V2401MalformedJson => "V2401",
            Self::V2402NonCanonicalJson => "V2402",
            Self::V2403UnsupportedSchema => "V2403",
            Self::V2404CanonicalOrder => "V2404",
            Self::V2405ReferentialIntegrity => "V2405",
            Self::V2406CfgInvalid => "V2406",
            Self::V2407AbiOrStackInvalid => "V2407",
            Self::V2408ProofCoverageInvalid => "V2408",
            Self::V2409ArtifactIdentityMismatch => "V2409",
            Self::V2410MetadataBindingMismatch => "V2410",
            Self::V2411ElfFormatInvalid => "V2411",
            Self::V2412ElfSectionInvalid => "V2412",
            Self::V2413InstructionInvalid => "V2413",
            Self::V2414ControlFlowInvalid => "V2414",
            Self::V2415BlockDigestMismatch => "V2415",
            Self::V2416SourceMapInvalid => "V2416",
            Self::V2417SyscallContractInvalid => "V2417",
            Self::V2418RecursionPolicyInvalid => "V2418",
            Self::V2419TypedSemanticsInvalid => "V2419",
            Self::V2420TypedMachineBindingInvalid => "V2420",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckerError {
    pub code: CheckerRejectionCode,
    pub message: String,
}

impl CheckerError {
    pub(crate) fn new(code: CheckerRejectionCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    fn bounded(mut self, max_bytes: u32) -> Self {
        let max_bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
        if self.message.len() > max_bytes {
            let mut end = max_bytes.min(self.message.len());
            while end > 0 && !self.message.is_char_boundary(end) {
                end -= 1;
            }
            self.message.truncate(end);
        }
        self
    }
}

impl std::fmt::Display for CheckerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for CheckerError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    Verified,
    NotProvided,
    NotExecuted,
    NotClaimed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckerReport {
    pub schema: String,
    pub checker_name: String,
    pub checker_version: String,
    pub checker_policy_schema: String,
    pub artifact_hash: String,
    pub lowering_record_hash: String,
    pub source_map_hash: String,
    pub binding_verification: EvidenceState,
    pub structural_verification: EvidenceState,
    pub lowering_record_verification: EvidenceState,
    pub typed_semantics_verification: EvidenceState,
    pub ckb_vm_evidence: EvidenceState,
    pub chain_evidence: EvidenceState,
    pub semantic_equivalence_claimed: bool,
    pub elf: ElfSummary,
}

const CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT: &str = "cellscript-ckb-runtime-access-provenance-v1";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeScalarProvenance {
    kind: String,
    value: Option<u64>,
    binding: Option<String>,
    max_inclusive: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSourceProvenance {
    resolved_source: String,
    origin: String,
    binding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeRangeProvenance {
    kind: String,
    offset: RuntimeScalarProvenance,
    length: RuntimeScalarProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeAccessProvenance {
    contract: String,
    source: RuntimeSourceProvenance,
    index: RuntimeScalarProvenance,
    range: RuntimeRangeProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeAccess {
    operation: String,
    syscall: String,
    source: String,
    index: u64,
    binding: String,
    provenance: RuntimeAccessProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeTransactionViewHandle {
    scope_kind: String,
    scope_name: String,
    binding: String,
    handle_type: String,
    source: String,
    provenance: RuntimeAccessProvenance,
    ownership: String,
    witness_owner: Option<String>,
    max_bytes: Option<u64>,
    lifecycle_authority: bool,
    typing_evidence_tier: String,
    read_evidence_tier: String,
}

pub fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CheckerError> {
    serde_json::to_vec(value).map_err(|error| {
        CheckerError::new(CheckerRejectionCode::V2401MalformedJson, format!("failed to serialize canonical checker value: {error}"))
    })
}

pub fn canonical_hash<T: Serialize>(domain: &str, value: &T) -> Result<String, CheckerError> {
    let bytes = canonical_bytes(value)?;
    let mut material = Vec::with_capacity(domain.len() + 1 + bytes.len());
    material.extend_from_slice(domain.as_bytes());
    material.push(0);
    material.extend_from_slice(&bytes);
    Ok(hex_encode(&ckb_blake2b256(&material)))
}

pub fn parse_lowering_record(bytes: &[u8], budgets: &CheckerBudgets) -> Result<VerifiedLoweringRecord, CheckerError> {
    ensure_byte_budget("lowering record", bytes.len(), budgets.record_bytes)?;
    let record: VerifiedLoweringRecord = serde_json::from_slice(bytes).map_err(|error| {
        CheckerError::new(CheckerRejectionCode::V2401MalformedJson, format!("failed to parse lowering record: {error}"))
    })?;
    ensure_canonical("lowering record", bytes, &record)?;
    Ok(record)
}

pub fn parse_source_map(bytes: &[u8], budgets: &CheckerBudgets) -> Result<SourceArtifactMap, CheckerError> {
    ensure_byte_budget("source map", bytes.len(), budgets.source_map_bytes)?;
    let source_map: SourceArtifactMap = serde_json::from_slice(bytes).map_err(|error| {
        CheckerError::new(CheckerRejectionCode::V2401MalformedJson, format!("failed to parse source map: {error}"))
    })?;
    ensure_canonical("source map", bytes, &source_map)?;
    Ok(source_map)
}

pub fn check_bundle(
    artifact: &[u8],
    metadata_bytes: &[u8],
    lowering_record_bytes: &[u8],
    source_map_bytes: &[u8],
    budgets: &CheckerBudgets,
) -> Result<CheckerReport, CheckerError> {
    let result = (|| {
        if budgets.schema != CHECKER_POLICY_SCHEMA {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2403UnsupportedSchema,
                format!("unsupported checker policy schema '{}'", budgets.schema),
            ));
        }
        ensure_byte_budget("artifact", artifact.len(), budgets.artifact_bytes)?;
        let metadata: Value = serde_json::from_slice(metadata_bytes).map_err(|error| {
            CheckerError::new(CheckerRejectionCode::V2401MalformedJson, format!("failed to parse compile metadata: {error}"))
        })?;
        let record = parse_lowering_record(lowering_record_bytes, budgets)?;
        let source_map = parse_source_map(source_map_bytes, budgets)?;
        check_bundle_values(artifact, &metadata, &record, &source_map, budgets)
    })();
    result.map_err(|error| error.bounded(budgets.diagnostic_bytes))
}

pub fn check_bundle_values(
    artifact: &[u8],
    metadata: &Value,
    record: &VerifiedLoweringRecord,
    source_map: &SourceArtifactMap,
    budgets: &CheckerBudgets,
) -> Result<CheckerReport, CheckerError> {
    validate_record_schema(record)?;
    validate_declared_limits(&record.limits, budgets)?;
    validate_counts(record, source_map, budgets)?;
    validate_metadata_binding(artifact, metadata, record, source_map)?;
    validate_record_graph(record, budgets)?;
    validate_typed_semantics(record)?;

    let elf = parse_elf(artifact, budgets.instructions).map_err(map_elf_error)?;
    validate_elf_binding(artifact, record, &elf)?;
    validate_block_digests(artifact, record, &elf)?;
    validate_control_flow(record, &elf)?;
    validate_machine_terminators(record, &elf)?;
    let terminal_sink = crate::failure::validate_verifier_failures(record, &elf)?;
    validate_stack_discipline(record, &elf, terminal_sink)?;
    crate::fixed_results::validate(record, &elf)?;
    validate_syscalls(record, &elf)?;
    validate_script_hash_machine_contract(record, &elf)?;
    crate::instance_machine::validate(record, &elf)?;
    crate::nested_machine::validate_cell_data_reads(record, &elf)?;
    crate::nested_machine::validate_scalar_status_calls(record, &elf, terminal_sink)?;
    crate::pipe_machine::validate(record, &elf, terminal_sink)?;
    crate::spawn_machine::validate(record, &elf, terminal_sink)?;
    zk::validate(record, &elf)?;
    let zk_contracts = crate::zk_profile::contracts(&record.typed_semantics)
        .map_err(|message| CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message))?;
    if !zk_contracts.is_empty() || metadata.pointer("/runtime/zk_verifiers").is_some() {
        let expected = serde_json::to_value(&zk_contracts)
            .map_err(|error| CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, error.to_string()))?;
        if metadata.pointer("/runtime/zk_verifiers") != Some(&expected) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2410MetadataBindingMismatch,
                "ZK builder contract differs from typed lowering",
            ));
        }
    }
    validate_committed_state_machine_contract(record, &elf)?;
    validate_bounded_group_input_machine_contract(record, &elf)?;
    validate_bounded_output_plan_machine_contract(metadata, record, &elf)?;
    validate_policy_dispatch_machine_contract(record, &elf)?;
    let selections = crate::cell_selection::machine::validate(record, &elf, terminal_sink)?;
    let allocated = crate::allocated_values::machine::validate(record, &elf, terminal_sink)?;
    let bodies = crate::nested_machine::validate(record, &elf, terminal_sink, &selections, &allocated)?;
    crate::scalar_machine::validate(record, &elf, terminal_sink, &selections, &bodies, &allocated)?;
    crate::code_origin::validate_open_verifier_claims(record, &elf)?;
    // Slot pairing runs after the typed machine-binding layers: tampering
    // with a save's base register is their provenance contract, and this
    // check is the cross-machine backstop for everything else.
    validate_ra_frame_slots(record, &elf)?;
    validate_source_map(source_map, record, artifact, &elf)?;

    Ok(CheckerReport {
        schema: CHECKER_REPORT_SCHEMA.to_string(),
        checker_name: "cellscript-artifact-checker".to_string(),
        checker_version: CHECKER_VERSION.to_string(),
        checker_policy_schema: budgets.schema.clone(),
        artifact_hash: record.artifact_hash.clone(),
        lowering_record_hash: canonical_hash(LOWERING_RECORD_SCHEMA, record)?,
        source_map_hash: canonical_hash(SOURCE_MAP_SCHEMA, source_map)?,
        binding_verification: EvidenceState::Verified,
        structural_verification: EvidenceState::Verified,
        lowering_record_verification: EvidenceState::Verified,
        typed_semantics_verification: EvidenceState::Verified,
        ckb_vm_evidence: EvidenceState::NotExecuted,
        chain_evidence: EvidenceState::NotProvided,
        semantic_equivalence_claimed: false,
        elf: elf.summary(),
    })
}

fn validate_metadata_binding(
    artifact: &[u8],
    metadata: &Value,
    record: &VerifiedLoweringRecord,
    source_map: &SourceArtifactMap,
) -> Result<(), CheckerError> {
    if json_u64(metadata, &["metadata_schema_version"]) != Some(u64::from(COMPILE_METADATA_SCHEMA_VERSION)) {
        return Err(metadata_binding_error(
            "unsupported effect metadata generation; recompile and regenerate the complete artifact bundle",
        ));
    }
    for (collection, kind) in [("actions", "action"), ("functions", "helper"), ("locks", "lock")] {
        for entry in metadata.get(collection).and_then(Value::as_array).into_iter().flatten() {
            if ["effect", "effect_class", "declared_effect", "declared_effect_class", "effect_declared", "inferred_effect_class"]
                .iter()
                .any(|field| entry.get(field).is_some())
            {
                return Err(metadata_binding_error("retired effect declaration or duplicate inference field in current metadata"));
            }
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| metadata_binding_error("effect entry has no callable name"))?;
            let checked = record
                .typed_semantics
                .entries
                .iter()
                .find(|candidate| candidate.name == name && candidate.kind == kind)
                .ok_or_else(|| metadata_binding_error("effect metadata has no bound typed callable"))?;
            let expected =
                serde_json::to_value(&checked.effects).map_err(|_| metadata_binding_error("effect facts cannot be serialized"))?;
            if entry.get("effects") != Some(&expected) {
                return Err(metadata_binding_error("metadata effect facts differ from the bound typed callable"));
            }
        }
    }
    if metadata
        .get("borrow_regions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|region| region.get("allowed_effects").is_some())
    {
        return Err(metadata_binding_error("retired borrow-region effect category list"));
    }
    crate::policy::validate_policy_metadata(metadata, &record.typed_semantics)?;
    crate::cell_selection::validate_types(&record.typed_semantics)?;
    crate::cell_data_view::validate_types(&record.typed_semantics)?;
    crate::allocated_values::validate_types(&record.typed_semantics)?;
    validate_ckb_vm2_target_contract(metadata)?;
    validate_runtime_access_provenance_metadata(metadata, &record.typed_semantics)?;
    let artifact_hash = hex_encode(&ckb_blake2b256(artifact));
    if artifact_hash != record.artifact_hash || artifact.len() as u64 != record.artifact_size_bytes {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2409ArtifactIdentityMismatch,
            "artifact bytes do not match the lowering record identity",
        ));
    }
    let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, record)?;
    let source_map_hash = canonical_hash(SOURCE_MAP_SCHEMA, source_map)?;
    let comparisons = [
        (
            "verified_artifact.boundary_schema",
            json_string(metadata, &["verified_artifact", "boundary_schema"]),
            VERIFIED_ARTIFACT_BOUNDARY_SCHEMA,
        ),
        ("verified_artifact.state", json_string(metadata, &["verified_artifact", "state"]), "emitted"),
        (
            "verified_artifact.lowering_record_schema",
            json_string(metadata, &["verified_artifact", "lowering_record_schema"]),
            LOWERING_RECORD_SCHEMA,
        ),
        ("verified_artifact.source_map_schema", json_string(metadata, &["verified_artifact", "source_map_schema"]), SOURCE_MAP_SCHEMA),
        ("compiler_version", json_string(metadata, &["compiler_version"]), record.compiler_version.as_str()),
        ("module", json_string(metadata, &["module"]), record.module.as_str()),
        ("edition", json_string(metadata, &["edition"]), record.edition.as_str()),
        ("target_profile.name", json_string(metadata, &["target_profile", "name"]), record.target_profile.as_str()),
        ("artifact_format", json_string(metadata, &["artifact_format"]), record.artifact_format.as_str()),
        ("artifact_hash", json_string(metadata, &["artifact_hash"]), record.artifact_hash.as_str()),
        ("source_content_hash", json_string(metadata, &["source_content_hash"]), record.source_content_hash.as_str()),
        (
            "verified_artifact.lowering_record_hash",
            json_string(metadata, &["verified_artifact", "lowering_record_hash"]),
            record_hash.as_str(),
        ),
        (
            "verified_artifact.source_map_hash",
            json_string(metadata, &["verified_artifact", "source_map_hash"]),
            source_map_hash.as_str(),
        ),
    ];
    for (field, actual, expected) in comparisons {
        if actual != Some(expected) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2410MetadataBindingMismatch,
                format!("compile metadata field '{field}' does not match lowering boundary"),
            ));
        }
    }
    let identities = &record.typed_semantics.foundation.identities;
    let expected_bundle_id = canonical_hash(
        "cellscript-verified-bundle-id-v1",
        &(
            record.artifact_hash.as_str(),
            record.typed_semantics_hash.as_str(),
            record.compatibility_profile_hash.as_str(),
            record_hash.as_str(),
            source_map_hash.as_str(),
            source_map.source_digest.as_str(),
        ),
    )?;
    let identity_comparisons = [
        (
            "verified_artifact.source_digest",
            json_string(metadata, &["verified_artifact", "source_digest"]),
            source_map.source_digest.as_str(),
        ),
        (
            "verified_artifact.core_semantic_id",
            json_string(metadata, &["verified_artifact", "core_semantic_id"]),
            identities.core_semantic_id.as_str(),
        ),
        (
            "verified_artifact.entry_contract_id",
            json_string(metadata, &["verified_artifact", "entry_contract_id"]),
            identities.entry_contract_id.as_str(),
        ),
        (
            "verified_artifact.artifact_contract_id",
            json_string(metadata, &["verified_artifact", "artifact_contract_id"]),
            identities.artifact_contract_id.as_str(),
        ),
        (
            "verified_artifact.deployable_artifact_id",
            json_string(metadata, &["verified_artifact", "deployable_artifact_id"]),
            record.artifact_hash.as_str(),
        ),
        (
            "verified_artifact.verified_bundle_id",
            json_string(metadata, &["verified_artifact", "verified_bundle_id"]),
            expected_bundle_id.as_str(),
        ),
    ];
    for (field, actual, expected) in identity_comparisons {
        if actual != Some(expected) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2410MetadataBindingMismatch,
                format!("compile metadata field '{field}' does not match the layered artifact identity"),
            ));
        }
    }
    if json_u64(metadata, &["artifact_size_bytes"]) != Some(record.artifact_size_bytes) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "compile metadata artifact_size_bytes does not match lowering record",
        ));
    }
    let profile_value = metadata.get("compatibility_profile").cloned().ok_or_else(|| {
        CheckerError::new(CheckerRejectionCode::V2410MetadataBindingMismatch, "compile metadata has no compatibility_profile")
    })?;
    let profile: CompatibilityProfileIdentity = serde_json::from_value(profile_value).map_err(|error| {
        CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            format!("compile metadata compatibility_profile shape is invalid: {error}"),
        )
    })?;
    if profile != record.compatibility_profile {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            "compile metadata compatibility profile differs from lowering record",
        ));
    }
    for (field, expected) in [
        ("metadata_schema_version", profile.metadata_schema_version),
        ("source_metadata_schema_version", profile.source_metadata_schema_version),
        ("artifact_metadata_schema_version", profile.artifact_metadata_schema_version),
        ("constraints_metadata_schema_version", profile.constraints_metadata_schema_version),
    ] {
        if json_u64(metadata, &[field]) != Some(u64::from(expected)) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2410MetadataBindingMismatch,
                format!("compile metadata field '{field}' differs from the resolved compatibility profile"),
            ));
        }
    }
    let typed_value = metadata.get("typed_semantics").cloned().ok_or_else(|| {
        CheckerError::new(CheckerRejectionCode::V2420TypedMachineBindingInvalid, "compile metadata has no typed_semantics record")
    })?;
    let typed: TypedSemanticRecord = serde_json::from_value(typed_value).map_err(|error| {
        CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            format!("compile metadata typed_semantics shape is invalid: {error}"),
        )
    })?;
    if typed != record.typed_semantics
        || json_string(metadata, &["typed_semantics_hash"]) != Some(record.typed_semantics_hash.as_str())
        || json_string(metadata, &["interface_hash"]) != Some(record.typed_semantics.interface_hash.as_str())
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "compile metadata typed semantics or interface identity differs from the lowering record",
        ));
    }
    validate_public_interface_metadata(metadata, &record.module, &typed.interface_hash)?;
    let runtime_trusted = metadata
        .get("runtime")
        .and_then(|runtime| runtime.get("trusted_external_verifiers"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let runtime_trusted: Vec<TrustedExternalVerifierRecord> = serde_json::from_value(runtime_trusted).map_err(|error| {
        CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            format!("compile metadata trusted external verifier shape is invalid: {error}"),
        )
    })?;
    let constraints_trusted = metadata
        .get("constraints")
        .and_then(|constraints| constraints.get("ckb"))
        .and_then(|ckb| ckb.get("trusted_external_verifiers"))
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));
    let constraints_trusted: Vec<TrustedExternalVerifierRecord> = serde_json::from_value(constraints_trusted).map_err(|error| {
        CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            format!("compile metadata CKB trusted external verifier shape is invalid: {error}"),
        )
    })?;
    if runtime_trusted != record.typed_semantics.trusted_external_verifiers
        || constraints_trusted != record.typed_semantics.trusted_external_verifiers
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2420TypedMachineBindingInvalid,
            "compile metadata runtime/CKB trusted external verifier bindings differ from typed semantics",
        ));
    }
    if source_map.lowering_record_hash != record_hash
        || source_map.artifact_hash != record.artifact_hash
        || source_map.source_set_hash != record.source_set_hash
        || source_map.source_digest != record.source_content_hash
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2416SourceMapInvalid,
            "source map identity does not bind to record, artifact, and source set",
        ));
    }
    Ok(())
}

fn validate_ckb_vm2_target_contract(metadata: &Value) -> Result<(), CheckerError> {
    let target_profile = json_string(metadata, &["target_profile", "name"])
        .and_then(ckb_deployment_hash_type)
        .ok_or_else(|| metadata_binding_error("compile metadata has an unsupported CKB target profile"))?;
    let deployment_hash_types = metadata
        .pointer("/target_profile/deployment_hash_types")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .and_then(|values| values[0].as_str());
    let constraints_hash_types = metadata
        .pointer("/constraints/ckb/profile_abi_contract/deployment_hash_types")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 1)
        .and_then(|values| values[0].as_str());
    if json_u64(metadata, &["target_profile", "minimum_vm_version"]) != Some(2)
        || json_string(metadata, &["target_profile", "riscv_isa"]) != Some("rv64imac_zbb")
        || deployment_hash_types != Some(target_profile)
        || json_u64(metadata, &["constraints", "ckb", "profile_abi_contract", "minimum_vm_version"]) != Some(2)
        || json_string(metadata, &["constraints", "ckb", "profile_abi_contract", "riscv_isa"]) != Some("rv64imac_zbb")
        || constraints_hash_types != Some(target_profile)
        || json_string(metadata, &["constraints", "ckb", "hash_type_policy", "default_script_hash_type"]) != Some(target_profile)
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2410MetadataBindingMismatch,
            format!("compile metadata does not bind the CKB VM2, rv64imac_zbb, {target_profile} deployment contract"),
        ));
    }
    Ok(())
}

pub(crate) fn ckb_deployment_hash_type(target_profile: &str) -> Option<&'static str> {
    match target_profile {
        "ckb" => Some("data2"),
        "ckb-type-hash" => Some("type"),
        _ => None,
    }
}

fn validate_runtime_access_provenance_metadata(metadata: &Value, typed_semantics: &TypedSemanticRecord) -> Result<(), CheckerError> {
    if json_string(metadata, &["runtime", "ckb_runtime_access_provenance_contract"]) != Some(CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT) {
        return Err(metadata_binding_error("compile metadata does not bind the current CKB runtime access provenance contract"));
    }

    let mut module_accesses =
        parse_runtime_accesses(metadata.pointer("/runtime/ckb_runtime_accesses"), "runtime.ckb_runtime_accesses")?;
    let mut entry_accesses = Vec::new();
    for collection in ["actions", "functions", "locks"] {
        let entries = metadata
            .get(collection)
            .and_then(Value::as_array)
            .ok_or_else(|| metadata_binding_error(format!("compile metadata field '{collection}' is not an array")))?;
        for (entry_index, entry) in entries.iter().enumerate() {
            let entry_name = entry.get("name").and_then(Value::as_str).unwrap_or("<unnamed>");
            let label = format!("{collection}[{entry_index}] '{entry_name}'.ckb_runtime_accesses");
            entry_accesses.extend(parse_runtime_accesses(entry.get("ckb_runtime_accesses"), &label)?);
        }
    }
    module_accesses.sort();
    entry_accesses.sort();
    if module_accesses != entry_accesses {
        return Err(metadata_binding_error(
            "compile metadata module runtime accesses differ from action/function/lock runtime accesses",
        ));
    }

    let handles = match metadata.pointer("/runtime/transaction_view_handles") {
        Some(value) => value
            .as_array()
            .ok_or_else(|| metadata_binding_error("compile metadata runtime.transaction_view_handles is not an array"))?
            .as_slice(),
        None => &[],
    };
    let mut typed_handles = Vec::new();
    for (index, handle) in handles.iter().enumerate() {
        let source = handle
            .get("source")
            .and_then(Value::as_str)
            .ok_or_else(|| metadata_binding_error(format!("runtime.transaction_view_handles[{index}].source is invalid")))?;
        let provenance = handle
            .get("provenance")
            .cloned()
            .ok_or_else(|| metadata_binding_error(format!("runtime.transaction_view_handles[{index}].provenance is missing")))?;
        let provenance: RuntimeAccessProvenance = serde_json::from_value(provenance).map_err(|error| {
            metadata_binding_error(format!("runtime.transaction_view_handles[{index}].provenance has an invalid shape: {error}"))
        })?;
        validate_runtime_access_provenance(&format!("runtime.transaction_view_handles[{index}]"), source, None, &provenance)?;
        let typed: RuntimeTransactionViewHandle = serde_json::from_value(handle.clone()).map_err(|error| {
            metadata_binding_error(format!("runtime.transaction_view_handles[{index}] has an invalid current shape: {error}"))
        })?;
        validate_runtime_transaction_view_handle(&format!("runtime.transaction_view_handles[{index}]"), &typed)?;
        typed_handles.push(typed);
    }
    for access in &module_accesses {
        let Some((owner, maximum)) = validate_bounded_witness_runtime_access(access)? else {
            continue;
        };
        if !typed_handles.iter().any(|handle| {
            bounded_witness_view_parts(&handle.handle_type) == Some((owner.as_str(), maximum))
                && handle.provenance.source.resolved_source == access.provenance.source.resolved_source
                && handle.provenance.index == access.provenance.index
        }) {
            return Err(metadata_binding_error(format!(
                "bounded witness runtime access '{}' has no matching transaction-view handle",
                access.operation
            )));
        }
    }
    validate_header_dep_runtime_accesses(&module_accesses, &typed_handles, typed_semantics)?;
    validate_script_hash_runtime_accesses(&module_accesses, typed_semantics)?;
    validate_sighash_zero_lock_domains(metadata, &module_accesses, typed_semantics)?;
    Ok(())
}

fn validate_sighash_zero_lock_domains(
    metadata: &Value,
    accesses: &[RuntimeAccess],
    typed_semantics: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    let domains = match metadata.pointer("/runtime/signing_message_domains") {
        Some(value) => {
            value.as_array().ok_or_else(|| metadata_binding_error("runtime.signing_message_domains is not an array"))?.as_slice()
        }
        None => &[],
    };
    let canonical_accesses = accesses.iter().filter(|access| access.operation == "sighash-all-zero-lock-v1").collect::<Vec<_>>();
    if domains.len() != canonical_accesses.len() {
        return Err(metadata_binding_error("runtime.signing_message_domains do not match bounded sighash runtime accesses"));
    }
    let mut semantic_calls = typed_semantics
        .entries
        .iter()
        .flat_map(|entry| {
            entry.blocks.iter().flat_map(move |block| {
                block.operations.iter().filter_map(move |operation| {
                    let call = operation.call.as_ref()?;
                    (call.target == "__ckb_sighash_all_zero_lock").then(|| {
                        let bounds = operation
                            .operands
                            .iter()
                            .map(|operand| match operand.constant.as_ref() {
                                Some(TypedSemanticConstant::U64(value)) => value.parse::<u64>().ok(),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>();
                        (entry.kind.clone(), entry.name.clone(), bounds)
                    })
                })
            })
        })
        .collect::<Vec<_>>();
    if semantic_calls.len() != domains.len()
        || semantic_calls.iter().any(|(_, _, bounds)| bounds.as_ref().is_none_or(|values| values.len() != 4))
    {
        return Err(metadata_binding_error("runtime.signing_message_domains do not match typed bounded sighash calls"));
    }
    let mut unmatched = canonical_accesses;
    for (index, domain) in domains.iter().enumerate() {
        let label = format!("runtime.signing_message_domains[{index}]");
        let u64_field = |field: &str| domain.get(field).and_then(Value::as_u64);
        let string_field = |field: &str| domain.get(field).and_then(Value::as_str);
        let max_group_inputs = u64_field("max_group_inputs");
        let max_inputs = u64_field("max_inputs");
        let max_extra_witnesses = u64_field("max_extra_witnesses");
        let max_witness_bytes = u64_field("max_witness_bytes");
        let scope_kind = string_field("scope_kind");
        let scope_name = string_field("scope_name");
        let scope_exists = scope_kind.zip(scope_name).is_some_and(|(kind, name)| {
            let collection = match kind {
                "action" => "actions",
                "function" => "functions",
                "lock" => "locks",
                _ => return false,
            };
            metadata
                .get(collection)
                .and_then(Value::as_array)
                .is_some_and(|entries| entries.iter().any(|entry| entry.get("name").and_then(Value::as_str) == Some(name)))
        });
        if !scope_exists
            || string_field("contract") != Some("cellscript-ckb-sighash-all-zero-lock-v1")
            || string_field("binding") != Some("env::sighash_all_zero_lock")
            || string_field("digest_type") != Some("SighashAllDigest")
            || string_field("hash_algorithm") != Some("ckb-default-hash-blake2b-256")
            || string_field("transaction_hash_source") != Some("LOAD_TX_HASH-exact-32")
            || string_field("group_scope") != Some("current-input-script-group")
            || string_field("first_witness_source") != Some("GroupInput[0]-WitnessArgs")
            || string_field("first_witness_lock_transform") != Some("replace-entire-lock-payload-with-equal-length-zero-bytes")
            || string_field("witness_length_prefix") != Some("u64-little-endian-byte-length")
            || string_field("later_group_witness_order") != Some("GroupInput[1..]-script-group-order-if-present")
            || string_field("extra_witness_source") != Some("Input[input_count..witness_count]-transaction-order")
            || !max_group_inputs.is_some_and(|value| (1..=64).contains(&value))
            || !max_inputs.is_some_and(|value| (1..=256).contains(&value))
            || max_group_inputs.zip(max_inputs).is_none_or(|(group, inputs)| group > inputs)
            || !max_extra_witnesses.is_some_and(|value| value <= 64)
            || !max_witness_bytes.is_some_and(|value| (1..=65_536).contains(&value))
            || string_field("runtime_helper") != Some("__ckb_sighash_all_zero_lock")
            || string_field("evidence_tier") != Some("checked-runtime")
        {
            return Err(metadata_binding_error(format!("{label} does not match the canonical zero-lock signing domain")));
        }
        let Some(position) = unmatched.iter().position(|access| {
            access.provenance.index.max_inclusive == max_group_inputs.map(|value| value - 1)
                && access.provenance.range.length.max_inclusive == max_witness_bytes
        }) else {
            return Err(metadata_binding_error(format!("{label} has no runtime access with matching group and witness bounds")));
        };
        unmatched.remove(position);
        let domain_bounds = [max_group_inputs, max_inputs, max_extra_witnesses, max_witness_bytes];
        let Some(position) = semantic_calls.iter().position(|(kind, name, bounds)| {
            let kind = if kind == "helper" { "function" } else { kind.as_str() };
            Some(kind) == scope_kind
                && Some(name.as_str()) == scope_name
                && bounds.as_ref().is_some_and(|values| values.iter().copied().map(Some).eq(domain_bounds))
        }) else {
            return Err(metadata_binding_error(format!("{label} has no typed bounded sighash call with matching scope and bounds")));
        };
        semantic_calls.remove(position);
    }
    Ok(())
}

fn bounded_witness_view_parts(handle_type: &str) -> Option<(&str, u64)> {
    use crate::view_types::{parse_witness_bytes_view_type, WitnessViewSyntax};
    let view = parse_witness_bytes_view_type(handle_type, WitnessViewSyntax::TrimArguments)?;
    (view.maximum <= 65_536).then_some((view.owner.as_str(), view.maximum))
}

fn validate_runtime_transaction_view_handle(prefix: &str, handle: &RuntimeTransactionViewHandle) -> Result<(), CheckerError> {
    for (field, value) in [
        ("scope_kind", handle.scope_kind.as_str()),
        ("scope_name", handle.scope_name.as_str()),
        ("binding", handle.binding.as_str()),
        ("handle_type", handle.handle_type.as_str()),
        ("source", handle.source.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(metadata_binding_error(format!("{prefix}.{field} must not be empty")));
        }
    }
    let base_type = handle.handle_type.split('<').next().unwrap_or(handle.handle_type.as_str());
    if !matches!(
        base_type,
        "InputView"
            | "OutputView"
            | "CellDataView"
            | "CellDepView"
            | "HeaderDepView"
            | "WitnessArgsView"
            | "WitnessBytesView"
            | "OutPoint"
            | "ScriptView"
    ) {
        return Err(metadata_binding_error(format!(
            "{prefix}.handle_type '{}' is not a supported transaction view",
            handle.handle_type
        )));
    }
    if handle.ownership != "read-only-view"
        || handle.lifecycle_authority
        || handle.typing_evidence_tier != "checked-static"
        || handle.read_evidence_tier != "checked-runtime"
    {
        return Err(metadata_binding_error(format!("{prefix} has an invalid ownership or evidence contract")));
    }
    if base_type != "WitnessBytesView" {
        if handle.witness_owner.is_some() || handle.max_bytes.is_some() {
            return Err(metadata_binding_error(format!("{prefix} non-witness handle carries bounded witness fields")));
        }
        return Ok(());
    }

    let Some((owner, maximum)) = bounded_witness_view_parts(&handle.handle_type) else {
        return Err(metadata_binding_error(format!("{prefix}.handle_type has an invalid bounded witness owner or maximum")));
    };
    if handle.witness_owner.as_deref() != Some(owner) || handle.max_bytes != Some(maximum) {
        return Err(metadata_binding_error(format!("{prefix} owner and maximum do not match handle_type")));
    }
    let source_matches = match handle.source.as_str() {
        "WitnessArgs/Input" => handle.provenance.source.resolved_source == "Input",
        "Input" | "Output" | "GroupInput" | "GroupOutput" => handle.provenance.source.resolved_source == handle.source,
        _ => false,
    };
    if !source_matches {
        return Err(metadata_binding_error(format!("{prefix} has an invalid bounded witness source")));
    }
    let expected_range = RuntimeRangeProvenance {
        kind: "bounded-range".to_string(),
        offset: RuntimeScalarProvenance { kind: "static".to_string(), value: Some(0), binding: None, max_inclusive: Some(maximum) },
        length: RuntimeScalarProvenance {
            kind: "dynamic".to_string(),
            value: None,
            binding: Some(format!("{}.size", handle.binding)),
            max_inclusive: Some(maximum),
        },
    };
    if handle.provenance.range != expected_range {
        return Err(metadata_binding_error(format!("{prefix} bounded witness range does not match its declared maximum")));
    }
    Ok(())
}

fn validate_bounded_witness_runtime_access(access: &RuntimeAccess) -> Result<Option<(String, u64)>, CheckerError> {
    if !access.operation.starts_with("witness-bounded-") {
        return Ok(None);
    }
    let mut contract = None;
    for owner in ["raw", "lock", "entry", "output_type"] {
        for (suffix, width) in [("size", None), ("u8", Some(1)), ("u32-le", Some(4)), ("u64-le", Some(8)), ("blake2b", None)] {
            if access.operation == format!("witness-bounded-{owner}-{suffix}") {
                contract = Some((owner, suffix, width));
            }
        }
    }
    let Some((owner, suffix, width)) = contract else {
        return Err(metadata_binding_error(format!("bounded witness runtime operation '{}' is not canonical", access.operation)));
    };
    if access.syscall != "LOAD_WITNESS"
        || access.source != "Witness"
        || access.binding != format!("witness::bounded_{owner}")
        || !matches!(access.provenance.source.resolved_source.as_str(), "Input" | "Output" | "GroupInput" | "GroupOutput")
        || access.provenance.source.origin != "inherited-source-view"
    {
        return Err(metadata_binding_error(format!(
            "bounded witness runtime access '{}' has an invalid source contract",
            access.operation
        )));
    }
    let maximum = if matches!(suffix, "size" | "blake2b") {
        let range = &access.provenance.range;
        if range.kind != "bounded-range"
            || range.offset.kind != "static"
            || range.offset.value != Some(0)
            || range.offset.binding.is_some()
            || range.length.kind != "dynamic"
            || range.length.value.is_some()
            || range.length.binding.as_deref() != Some(format!("bounded_{owner}.size").as_str())
            || range.offset.max_inclusive != range.length.max_inclusive
        {
            return Err(metadata_binding_error(format!(
                "bounded witness runtime access '{}' has an invalid whole-view range",
                access.operation
            )));
        }
        range.length.max_inclusive
    } else {
        let width = width.expect("bounded scalar width");
        let range = &access.provenance.range;
        if range.kind != "bounded-range"
            || !matches!(range.offset.kind.as_str(), "static" | "dynamic")
            || range.length.kind != "static"
            || range.length.value != Some(width)
            || range.length.binding.is_some()
            || range.length.max_inclusive != Some(width)
        {
            return Err(metadata_binding_error(format!(
                "bounded witness runtime access '{}' has an invalid scalar range",
                access.operation
            )));
        }
        range.offset.max_inclusive
    };
    let Some(maximum) = maximum.filter(|maximum| *maximum <= 65_536) else {
        return Err(metadata_binding_error(format!(
            "bounded witness runtime access '{}' exceeds the maximum byte domain",
            access.operation
        )));
    };
    Ok(Some((owner.to_string(), maximum)))
}

fn parse_runtime_accesses(value: Option<&Value>, label: &str) -> Result<Vec<RuntimeAccess>, CheckerError> {
    let value = value.cloned().ok_or_else(|| metadata_binding_error(format!("compile metadata field '{label}' is missing")))?;
    let accesses: Vec<RuntimeAccess> = serde_json::from_value(value)
        .map_err(|error| metadata_binding_error(format!("compile metadata field '{label}' has an invalid shape: {error}")))?;
    for (index, access) in accesses.iter().enumerate() {
        let prefix = format!("{label}[{index}]");
        for (field, value) in [
            ("operation", access.operation.as_str()),
            ("syscall", access.syscall.as_str()),
            ("source", access.source.as_str()),
            ("binding", access.binding.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(metadata_binding_error(format!("{prefix}.{field} must not be empty")));
            }
        }
        if !known_runtime_source(&access.source)
            || !known_runtime_syscall(&access.syscall)
            || !runtime_syscall_allows_source(&access.syscall, &access.source)
        {
            return Err(metadata_binding_error(format!(
                "{prefix} has an unknown or incompatible CKB runtime syscall/source pair '{}/{}'",
                access.syscall, access.source
            )));
        }
        validate_runtime_access_provenance(&prefix, &access.source, Some(access.index), &access.provenance)?;
        validate_canonical_transaction_hash_runtime_access(&prefix, access)?;
        validate_canonical_sighash_zero_lock_runtime_access(&prefix, access)?;
        validate_canonical_script_hash_runtime_access(&prefix, access)?;
        validate_canonical_header_dep_runtime_access(&prefix, access)?;
    }
    Ok(accesses)
}

#[derive(Clone, Copy)]
struct HeaderDepAccessContract {
    target: &'static str,
    operation: &'static str,
    syscall: &'static str,
    binding: &'static str,
    return_type: &'static str,
    width: u64,
}

const HEADER_DEP_ACCESS_CONTRACTS: [HeaderDepAccessContract; 5] = [
    HeaderDepAccessContract {
        target: "__ckb_header_dep_epoch_number",
        operation: "header-dep-epoch-number",
        syscall: "LOAD_HEADER_BY_FIELD",
        binding: "HeaderDepView.epoch_number",
        return_type: "EpochNumber",
        width: 8,
    },
    HeaderDepAccessContract {
        target: "__ckb_header_dep_epoch_start_block_number",
        operation: "header-dep-epoch-start-block-number",
        syscall: "LOAD_HEADER_BY_FIELD",
        binding: "HeaderDepView.epoch_start_block_number",
        return_type: "BlockNumber",
        width: 8,
    },
    HeaderDepAccessContract {
        target: "__ckb_header_dep_epoch_length",
        operation: "header-dep-epoch-length",
        syscall: "LOAD_HEADER_BY_FIELD",
        binding: "HeaderDepView.epoch_length",
        return_type: "EpochLength",
        width: 8,
    },
    HeaderDepAccessContract {
        target: "__ckb_header_dep_block_number",
        operation: "header-dep-block-number",
        syscall: "LOAD_HEADER",
        binding: "HeaderDepView.block_number",
        return_type: "BlockNumber",
        width: 208,
    },
    HeaderDepAccessContract {
        target: "__ckb_header_dep_timestamp_millis",
        operation: "header-dep-timestamp-millis",
        syscall: "LOAD_HEADER",
        binding: "HeaderDepView.timestamp",
        return_type: "TimestampMillis",
        width: 208,
    },
];

fn validate_canonical_header_dep_runtime_access(prefix: &str, access: &RuntimeAccess) -> Result<(), CheckerError> {
    let expected = HEADER_DEP_ACCESS_CONTRACTS.iter().find(|contract| contract.operation == access.operation);
    let identifies_header_field =
        expected.is_some() || access.operation.starts_with("header-dep-") || access.binding.starts_with("HeaderDepView.");
    if !identifies_header_field {
        return Ok(());
    }
    let Some(expected) = expected else {
        return Err(metadata_binding_error(format!("{prefix} names an unknown HeaderDep field contract")));
    };
    let range = &access.provenance.range;
    if access.syscall != expected.syscall
        || access.source != "HeaderDep"
        || access.binding != expected.binding
        || access.provenance.source.resolved_source != "HeaderDep"
        || access.provenance.source.origin != "inherited-source-view"
        || access.provenance.source.binding.as_deref().is_none_or(str::is_empty)
        || !matches!(access.provenance.index.kind.as_str(), "static" | "dynamic")
        || access.provenance.index.max_inclusive != Some(u64::from(u32::MAX))
        || range.kind != "fixed-width"
        || range.offset.kind != "not-applicable"
        || range.length.kind != "static"
        || range.length.value != Some(expected.width)
        || range.length.max_inclusive != Some(expected.width)
    {
        return Err(metadata_binding_error(format!("{prefix} does not match the canonical {} contract", expected.binding)));
    }
    Ok(())
}

fn validate_header_dep_runtime_accesses(
    accesses: &[RuntimeAccess],
    handles: &[RuntimeTransactionViewHandle],
    typed_semantics: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    for expected in HEADER_DEP_ACCESS_CONTRACTS {
        let matching_accesses = accesses.iter().filter(|access| access.operation == expected.operation).collect::<Vec<_>>();
        let matching_calls = typed_semantics
            .entries
            .iter()
            .flat_map(|entry| &entry.blocks)
            .flat_map(|block| &block.operations)
            .filter(|operation| operation.call.as_ref().is_some_and(|call| call.target == expected.target))
            .collect::<Vec<_>>();
        if matching_accesses.len() != matching_calls.len() {
            return Err(metadata_binding_error(format!(
                "HeaderDep runtime access '{}' does not match typed helper calls",
                expected.operation
            )));
        }
        for operation in matching_calls {
            let call = operation.call.as_ref().expect("filtered typed call");
            if call.params != ["HeaderDepView"]
                || call.return_type != expected.return_type
                || operation.operands.len() != 1
                || operation.operands[0].ty != "HeaderDepView"
            {
                return Err(metadata_binding_error(format!(
                    "typed helper '{}' does not preserve the canonical HeaderDep operand and result types",
                    expected.target
                )));
            }
        }
        for access in matching_accesses {
            if !handles.iter().any(|handle| {
                handle.handle_type == "HeaderDepView"
                    && handle.source == "HeaderDep"
                    && access.provenance.source.binding.as_deref() == Some(handle.binding.as_str())
                    && access.provenance.index == handle.provenance.index
            }) {
                return Err(metadata_binding_error(format!(
                    "HeaderDep runtime access '{}' has no matching typed source-view handle",
                    expected.operation
                )));
            }
        }
    }
    Ok(())
}

fn validate_script_hash_runtime_accesses(
    accesses: &[RuntimeAccess],
    typed_semantics: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    const MAX_ARGS_BYTES: u64 = 459;
    const MOLECULE_SCRIPT_PREFIX_BYTES: u64 = 53;
    let mut matching_accesses = accesses.iter().filter(|access| access.operation == "script-hash-v1").collect::<Vec<_>>();
    let matching_calls = typed_semantics
        .entries
        .iter()
        .flat_map(|entry| &entry.blocks)
        .flat_map(|block| &block.operations)
        .filter(|operation| operation.call.as_ref().is_some_and(|call| call.target == "__ckb_script_hash"))
        .collect::<Vec<_>>();
    if matching_accesses.len() != matching_calls.len() {
        return Err(metadata_binding_error("canonical Script hash runtime accesses do not match typed helper calls"));
    }
    for operation in matching_calls {
        let call = operation.call.as_ref().expect("filtered typed call");
        let args_width = operation.operands.get(2).and_then(|operand| script_hash_args_width(&operand.ty));
        if operation.destinations.len() != 1
            || operation.operands.len() != 3
            || call.params != operation.operands.iter().map(|operand| operand.ty.clone()).collect::<Vec<_>>()
            || call.return_type != "hash"
            || operation.operands.first().is_none_or(|operand| canonical_abi_type(&operand.ty) != "hash")
            || operation.operands.get(1).is_none_or(|operand| operand.ty != "u64")
            || args_width.is_none_or(|width| width > MAX_ARGS_BYTES)
        {
            return Err(metadata_binding_error(
                "typed __ckb_script_hash call does not preserve the canonical code-hash, hash-type, args, and result domains",
            ));
        }
        let encoded_width = MOLECULE_SCRIPT_PREFIX_BYTES + args_width.expect("validated Script args width");
        let Some(position) = matching_accesses.iter().position(|access| access.provenance.range.length.value == Some(encoded_width))
        else {
            return Err(metadata_binding_error(
                "typed __ckb_script_hash call has no runtime access with its canonical Molecule width",
            ));
        };
        matching_accesses.remove(position);
    }
    Ok(())
}

fn fixed_u8_array_width(ty: &str) -> Option<u64> {
    let inner = ty.strip_prefix('[')?.strip_suffix(']')?;
    let (element, width) = inner.rsplit_once(';')?;
    (element.trim() == "u8").then(|| width.trim().parse().ok()).flatten()
}

fn script_hash_args_width(ty: &str) -> Option<u64> {
    (canonical_abi_type(ty) == "hash").then_some(32).or_else(|| fixed_u8_array_width(ty))
}

fn validate_canonical_transaction_hash_runtime_access(prefix: &str, access: &RuntimeAccess) -> Result<(), CheckerError> {
    let identifies_transaction_hash =
        access.operation == "transaction-hash" || access.syscall == "LOAD_TX_HASH" || access.binding == "ckb::transaction_hash";
    if !identifies_transaction_hash {
        return Ok(());
    }
    let not_applicable =
        RuntimeScalarProvenance { kind: "not-applicable".to_string(), value: None, binding: None, max_inclusive: None };
    let fixed_32 = RuntimeRangeProvenance {
        kind: "fixed-width".to_string(),
        offset: not_applicable.clone(),
        length: RuntimeScalarProvenance { kind: "static".to_string(), value: Some(32), binding: None, max_inclusive: Some(32) },
    };
    if access.operation != "transaction-hash"
        || access.syscall != "LOAD_TX_HASH"
        || access.source != "Transaction"
        || access.index != 0
        || access.binding != "ckb::transaction_hash"
        || access.provenance.source.resolved_source != "Transaction"
        || access.provenance.source.origin != "implicit-lowering"
        || access.provenance.source.binding.is_some()
        || access.provenance.index != not_applicable
        || access.provenance.range != fixed_32
    {
        return Err(metadata_binding_error(format!("{prefix} does not match the canonical 32-byte LOAD_TX_HASH contract")));
    }
    Ok(())
}

fn validate_canonical_sighash_zero_lock_runtime_access(prefix: &str, access: &RuntimeAccess) -> Result<(), CheckerError> {
    let identifies_domain = access.operation == "sighash-all-zero-lock-v1"
        || access.syscall == "CKB_SIGHASH_ALL_ZERO_LOCK_V1"
        || access.binding == "env::sighash_all_zero_lock";
    if !identifies_domain {
        return Ok(());
    }
    let range = &access.provenance.range;
    if access.operation != "sighash-all-zero-lock-v1"
        || access.syscall != "CKB_SIGHASH_ALL_ZERO_LOCK_V1"
        || access.source != "GroupInput"
        || access.index != 0
        || access.binding != "env::sighash_all_zero_lock"
        || access.provenance.source.resolved_source != "GroupInput"
        || access.provenance.source.origin != "bounded-scan"
        || access.provenance.source.binding.is_some()
        || access.provenance.index.kind != "bounded-scan"
        || access.provenance.index.value.is_some()
        || access.provenance.index.binding.is_some()
        || access.provenance.index.max_inclusive.is_none_or(|maximum| maximum > 63)
        || range.kind != "bounded-range"
        || range.offset.kind != "static"
        || range.offset.value != Some(0)
        || range.offset.binding.is_some()
        || range.length.kind != "dynamic"
        || range.length.value.is_some()
        || range.length.binding.as_deref() != Some("sighash_all_zero_lock.witness_size")
        || range.offset.max_inclusive != range.length.max_inclusive
        || range.length.max_inclusive.is_none_or(|maximum| maximum == 0 || maximum > 65_536)
    {
        return Err(metadata_binding_error(format!("{prefix} does not match the bounded CKB sighash-all zero-lock runtime contract")));
    }
    Ok(())
}

fn validate_canonical_script_hash_runtime_access(prefix: &str, access: &RuntimeAccess) -> Result<(), CheckerError> {
    const MAX_ARGS_BYTES: u64 = 459;
    let identifies_script_hash = access.operation == "script-hash-v1"
        || access.binding == "script::hash"
        || (access.syscall == "CKB_BLAKE2B" && access.source == "Script");
    if !identifies_script_hash {
        return Ok(());
    }
    let range = &access.provenance.range;
    if access.operation != "script-hash-v1"
        || access.syscall != "CKB_BLAKE2B"
        || access.source != "Script"
        || access.index != 0
        || access.binding != "script::hash"
        || access.provenance.source.resolved_source != "Script"
        || access.provenance.source.origin != "constructed-script"
        || access.provenance.source.binding.is_some()
        || access.provenance.index.kind != "not-applicable"
        || access.provenance.index.value.is_some()
        || access.provenance.index.binding.is_some()
        || access.provenance.index.max_inclusive.is_some()
        || range.kind != "fixed-width"
        || range.offset.kind != "not-applicable"
        || range.offset.value.is_some()
        || range.offset.binding.is_some()
        || range.offset.max_inclusive.is_some()
        || range.length.kind != "static"
        || range.length.value.is_none_or(|width| !(53..=53 + MAX_ARGS_BYTES).contains(&width))
        || range.length.value != range.length.max_inclusive
        || range.length.binding.is_some()
    {
        return Err(metadata_binding_error(format!("{prefix} does not match the canonical bounded Molecule Script hash contract")));
    }
    Ok(())
}

fn validate_runtime_access_provenance(
    prefix: &str,
    declared_source: &str,
    compatibility_index: Option<u64>,
    provenance: &RuntimeAccessProvenance,
) -> Result<(), CheckerError> {
    if provenance.contract != CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT
        || provenance.source.resolved_source.trim().is_empty()
        || !known_runtime_source(&provenance.source.resolved_source)
    {
        return Err(metadata_binding_error(format!("{prefix}.provenance has an invalid contract or resolved source")));
    }
    if !matches!(
        provenance.source.origin.as_str(),
        "explicit-source-view"
            | "inherited-source-view"
            | "implicit-lowering"
            | "bounded-scan"
            | "constructed-script"
            | "metadata-summary"
            | "external-adapter"
    ) {
        return Err(metadata_binding_error(format!(
            "{prefix}.provenance.source.origin '{}' is not admitted",
            provenance.source.origin
        )));
    }
    match provenance.source.origin.as_str() {
        "inherited-source-view" if provenance.source.binding.as_deref().is_none_or(str::is_empty) => {
            return Err(metadata_binding_error(format!(
                "{prefix}.provenance.source.binding is required for an inherited source view"
            )));
        }
        "inherited-source-view" => {}
        _ if provenance.source.binding.is_some() => {
            return Err(metadata_binding_error(format!(
                "{prefix}.provenance.source.binding is only valid for an inherited source view"
            )));
        }
        _ => {}
    }
    validate_runtime_scalar(&format!("{prefix}.provenance.index"), &provenance.index)?;
    if matches!(provenance.source.origin.as_str(), "explicit-source-view" | "inherited-source-view")
        && (!matches!(provenance.index.kind.as_str(), "static" | "dynamic")
            || provenance.index.max_inclusive != Some(u64::from(u32::MAX)))
    {
        return Err(metadata_binding_error(format!(
            "{prefix}.provenance.index must bind a static or dynamic 32-bit source-view index"
        )));
    }
    if let Some(compatibility_index) = compatibility_index {
        let expected = provenance.index.value.unwrap_or(0);
        if compatibility_index != expected {
            return Err(metadata_binding_error(format!(
                "{prefix}.index does not match the structured provenance compatibility projection"
            )));
        }
    }
    validate_runtime_range(&format!("{prefix}.provenance.range"), &provenance.range)?;
    if declared_source == "SourceView"
        && !matches!(
            provenance.source.resolved_source.as_str(),
            "Input" | "Output" | "CellDep" | "GroupInput" | "GroupOutput" | "SourceView"
        )
    {
        return Err(metadata_binding_error(format!("{prefix}.provenance resolved source is incompatible with declared SourceView")));
    }
    Ok(())
}

fn validate_runtime_scalar(prefix: &str, value: &RuntimeScalarProvenance) -> Result<(), CheckerError> {
    let valid = match value.kind.as_str() {
        "not-applicable" => value.value.is_none() && value.binding.is_none() && value.max_inclusive.is_none(),
        "static" | "metadata-ordinal" => {
            value.value.is_some()
                && value.binding.is_none()
                && !value.value.zip(value.max_inclusive).is_some_and(|(actual, maximum)| actual > maximum)
        }
        "dynamic" => value.value.is_none() && value.binding.as_deref().is_some_and(|binding| !binding.is_empty()),
        "bounded-scan" => value.value.is_none() && value.binding.is_none() && value.max_inclusive.is_some(),
        _ => false,
    };
    if !valid {
        return Err(metadata_binding_error(format!("{prefix} has invalid scalar provenance")));
    }
    Ok(())
}

fn validate_runtime_range(prefix: &str, range: &RuntimeRangeProvenance) -> Result<(), CheckerError> {
    validate_runtime_scalar(&format!("{prefix}.offset"), &range.offset)?;
    validate_runtime_scalar(&format!("{prefix}.length"), &range.length)?;
    let valid = match range.kind.as_str() {
        "not-applicable" | "whole-value" => range.offset.kind == "not-applicable" && range.length.kind == "not-applicable",
        "fixed-width" => {
            range.offset.kind == "not-applicable"
                && range.length.kind == "static"
                && range.length.value.is_some_and(|length| length > 0)
        }
        "bounded-range" => {
            matches!(range.offset.kind.as_str(), "static" | "dynamic") && matches!(range.length.kind.as_str(), "static" | "dynamic")
        }
        _ => false,
    };
    if !valid {
        return Err(metadata_binding_error(format!("{prefix} has invalid range provenance")));
    }
    Ok(())
}

fn known_runtime_source(source: &str) -> bool {
    matches!(
        source,
        "Input"
            | "Output"
            | "CellDep"
            | "HeaderDep"
            | "GroupInput"
            | "GroupOutput"
            | "GroupCellDep"
            | "GroupHeaderDep"
            | "Witness"
            | "ScriptArgs"
            | "Script"
            | "Expression"
            | "SourceView"
            | "Input/Output"
            | "Input/GroupInput"
            | "Input/HeaderDep"
            | "GroupInput/GroupOutput"
            | "CurrentScript"
            | "CurrentScript/Output"
            | "CurrentScript/SourceView"
            | "CurrentScript/Input/GroupInput/GroupOutput"
            | "Process"
            | "Profile"
            | "Transaction"
    )
}

fn known_runtime_syscall(syscall: &str) -> bool {
    matches!(
        syscall,
        "LOAD_CELL"
            | "LOAD_TRANSACTION"
            | "LOAD_TX_HASH"
            | "LOAD_CELL_BY_FIELD"
            | "LOAD_CELL_DATA"
            | "LOAD_HEADER"
            | "LOAD_HEADER_BY_FIELD"
            | "LOAD_INPUT_BY_FIELD"
            | "LOAD_SCRIPT"
            | "LOAD_SCRIPT_HASH"
            | "LOAD_SCRIPT_ARGS"
            | "SOURCE_VIEW"
            | "CKB_SINCE_ENCODING"
            | "CKB_EPOCH_ARITHMETIC"
            | "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA"
            | "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS"
            | "LOAD_INPUT_BY_FIELD/SOURCE_VIEW"
            | "LOAD_SCRIPT+LOAD_CELL_BY_FIELD"
            | "LOAD_SCRIPT+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA"
            | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD"
            | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW"
            | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD"
            | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW"
            | "LOAD_WITNESS"
            | "LOAD_WITNESS_ARGS_LOCK"
            | "LOAD_WITNESS_ARGS_INPUT_TYPE"
            | "LOAD_WITNESS_ARGS_OUTPUT_TYPE"
            | "CKB_SIGHASH_ALL_ZERO_LOCK_V1"
            | "EXIT"
            | "CAPACITY_POLICY"
            | "CKB_BLAKE2B"
            | "SHA256"
            | "SPAWN"
            | "EXEC"
            | "WAIT"
            | "PROCESS_ID"
            | "PIPE"
            | "PIPE_WRITE"
            | "PIPE_READ"
            | "INHERITED_FD"
            | "CLOSE"
    )
}

fn runtime_syscall_allows_source(syscall: &str, source: &str) -> bool {
    match syscall {
        "LOAD_TRANSACTION" | "LOAD_TX_HASH" => source == "Transaction",
        "LOAD_CELL" => matches!(source, "Input" | "Output" | "CellDep" | "GroupInput" | "GroupOutput"),
        "LOAD_CELL_BY_FIELD" => {
            matches!(source, "Input" | "Output" | "GroupInput" | "GroupOutput" | "CellDep" | "SourceView")
        }
        "LOAD_CELL_DATA" => {
            matches!(source, "Input" | "Output" | "GroupInput" | "GroupOutput" | "SourceView" | "GroupInput/GroupOutput")
        }
        "LOAD_HEADER" => matches!(source, "HeaderDep" | "Input/GroupInput" | "Input/HeaderDep"),
        "LOAD_HEADER_BY_FIELD" => source == "HeaderDep",
        "LOAD_INPUT_BY_FIELD" => matches!(source, "GroupInput" | "SourceView" | "Input/GroupInput"),
        "LOAD_SCRIPT" | "LOAD_SCRIPT_HASH" => source == "CurrentScript",
        "CKB_SIGHASH_ALL_ZERO_LOCK_V1" => source == "GroupInput",
        "EXIT" => source == "Process",
        "LOAD_SCRIPT_ARGS" => source == "ScriptArgs",
        "SOURCE_VIEW" => {
            matches!(source, "Input" | "Output" | "CellDep" | "HeaderDep" | "GroupInput" | "GroupOutput" | "SourceView" | "Expression")
        }
        "CKB_SINCE_ENCODING" | "CKB_EPOCH_ARITHMETIC" => source == "Expression",
        "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA" | "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS" => source == "SourceView",
        "LOAD_INPUT_BY_FIELD/SOURCE_VIEW" => source == "Input/Output",
        "LOAD_SCRIPT+LOAD_CELL_BY_FIELD" => source == "CurrentScript/Output",
        "LOAD_SCRIPT+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA" => source == "CurrentScript/Input/GroupInput/GroupOutput",
        "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD" => source == "CurrentScript/SourceView",
        "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW"
        | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW"
        | "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD" => source == "Input/Output",
        "LOAD_WITNESS" => source == "Witness",
        "LOAD_WITNESS_ARGS_LOCK" | "LOAD_WITNESS_ARGS_INPUT_TYPE" => source == "GroupInput",
        "LOAD_WITNESS_ARGS_OUTPUT_TYPE" => source == "GroupOutput",
        "CAPACITY_POLICY" => source == "Output",
        "CKB_BLAKE2B" => matches!(source, "Profile" | "Script"),
        "SHA256" => source == "Profile",
        "SPAWN" | "EXEC" => source == "CellDep",
        "WAIT" | "PROCESS_ID" | "PIPE" | "PIPE_WRITE" | "PIPE_READ" | "INHERITED_FD" | "CLOSE" => source == "Process",
        _ => false,
    }
}

fn metadata_binding_error(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2410MetadataBindingMismatch, message)
}

fn validate_public_interface_metadata(metadata: &Value, module: &str, expected_hash: &str) -> Result<(), CheckerError> {
    let interface = metadata
        .get("public_interface")
        .and_then(Value::as_object)
        .ok_or_else(|| metadata_binding_error("compile metadata has no canonical public_interface object"))?;
    if interface.get("schema").and_then(Value::as_str) != Some(crate::interface::PACKAGE_INTERFACE_SCHEMA)
        || interface.get("version").and_then(Value::as_u64) != Some(u64::from(crate::interface::PACKAGE_INTERFACE_VERSION))
        || interface.get("module").and_then(Value::as_str) != Some(module)
    {
        return Err(metadata_binding_error("compile metadata public interface has an invalid schema, version, or module identity"));
    }
    let current: crate::interface::PackageInterface = serde_json::from_value(Value::Object(interface.clone())).map_err(|_| {
        metadata_binding_error("invalid current public interface; retired fields and category contracts are not accepted")
    })?;
    if current.callables.iter().any(|callable| callable.type_parameters.is_empty() != callable.effects.is_some()) {
        return Err(metadata_binding_error(
            "concrete callable effects are required; uninstantiated templates have no executable fact contract",
        ));
    }
    if serde_json::to_value(current).map_err(|_| metadata_binding_error("public interface serialization failed"))?
        != Value::Object(interface.clone())
    {
        return Err(metadata_binding_error("public interface has unknown or missing current-generation fields"));
    }
    let edition = interface.get("edition").and_then(Value::as_str);
    if edition != Some("2027") || edition != metadata.get("edition").and_then(Value::as_str) {
        return Err(metadata_binding_error(
            "public interface edition must be 2027 and match compile metadata; rebuild with the current source contract",
        ));
    }
    let canonical = canonical_json_value(&Value::Object(interface.clone()));
    let canonical_bytes = serde_json::to_vec(&canonical)
        .map_err(|error| metadata_binding_error(format!("failed to serialize canonical public interface: {error}")))?;
    let actual_hash = hex_encode(&ckb_blake2b256(&canonical_bytes));
    if actual_hash != expected_hash {
        return Err(metadata_binding_error("compile metadata public interface does not match its interface_hash"));
    }

    let types = validate_public_interface_items(interface.get("types"), "type")?;
    validate_public_interface_items(interface.get("constants"), "constant")?;
    let callables = validate_public_interface_items(interface.get("callables"), "callable")?;
    for item in types {
        let identity = item.get("identity").and_then(Value::as_str).unwrap_or("<unknown>");
        validate_public_type_parameters(item.get("type_parameters"), &format!("{identity}.type_parameters"), true)?;
        validate_public_value_abilities(item.get("value_abilities"), &format!("{identity}.value_abilities"))?;
    }
    for item in callables {
        let identity = item.get("identity").and_then(Value::as_str).unwrap_or("<unknown>");
        validate_public_type_parameters(item.get("type_parameters"), &format!("{identity}.type_parameters"), false)?;
    }
    Ok(())
}

pub(crate) fn canonical_json_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonical_json_value(&object[key]));
            }
            Value::Object(canonical)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical_json_value).collect()),
        other => other.clone(),
    }
}

fn validate_public_interface_items<'a>(
    value: Option<&'a Value>,
    kind: &str,
) -> Result<Vec<&'a serde_json::Map<String, Value>>, CheckerError> {
    let items =
        value.and_then(Value::as_array).ok_or_else(|| metadata_binding_error(format!("public interface {kind}s must be an array")))?;
    let mut validated = Vec::with_capacity(items.len());
    let mut previous = None::<&str>;
    for item in items {
        let object = item.as_object().ok_or_else(|| metadata_binding_error(format!("public interface {kind} must be an object")))?;
        let identity = object
            .get("identity")
            .and_then(Value::as_str)
            .filter(|identity| !identity.is_empty())
            .ok_or_else(|| metadata_binding_error(format!("public interface {kind} has no identity")))?;
        if previous.is_some_and(|previous| previous >= identity) {
            return Err(metadata_binding_error(format!("public interface {kind} identities must be unique and canonically ordered")));
        }
        previous = Some(identity);
        validated.push(object);
    }
    Ok(validated)
}

fn validate_public_type_parameters(value: Option<&Value>, label: &str, layout_type: bool) -> Result<(), CheckerError> {
    let params = value.and_then(Value::as_array).ok_or_else(|| metadata_binding_error(format!("{label} must be an array")))?;
    let mut names = BTreeSet::new();
    for param in params {
        let param = param.as_object().ok_or_else(|| metadata_binding_error(format!("{label} parameter must be an object")))?;
        let name = param.get("name").and_then(Value::as_str).unwrap_or("");
        let valid_name = !name.is_empty()
            && name.chars().next().is_some_and(|ch| ch.is_alphabetic() || ch == '_')
            && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_');
        if !valid_name || !names.insert(name) {
            return Err(metadata_binding_error(format!("{label} has an invalid or duplicate parameter '{name}'")));
        }
        let phantom = param
            .get("phantom")
            .and_then(Value::as_bool)
            .ok_or_else(|| metadata_binding_error(format!("{label}.{name}.phantom must be boolean")))?;
        let constraints = validate_public_value_abilities(param.get("constraints"), &format!("{label}.{name}.constraints"))?;
        if layout_type
            && !phantom
            && ["fixed", "serializable", "non_linear"].into_iter().any(|required| !constraints.contains(&required))
        {
            return Err(metadata_binding_error(format!(
                "{label}.{name} must preserve the fixed, serializable, non_linear public layout boundary"
            )));
        }
    }
    Ok(())
}

fn validate_public_value_abilities<'a>(value: Option<&'a Value>, label: &str) -> Result<Vec<&'a str>, CheckerError> {
    const ORDER: [&str; 7] = ["copy", "drop", "store", "fixed", "serializable", "non_linear", "cell"];
    let values = value.and_then(Value::as_array).ok_or_else(|| metadata_binding_error(format!("{label} must be an array")))?;
    let abilities = values
        .iter()
        .map(|value| value.as_str().ok_or_else(|| metadata_binding_error(format!("{label} must contain only value abilities"))))
        .collect::<Result<Vec<_>, _>>()?;
    let canonical = ORDER.into_iter().filter(|ability| abilities.contains(ability)).collect::<Vec<_>>();
    if canonical != abilities {
        return Err(metadata_binding_error(format!("{label} must contain unique, known value abilities in canonical order")));
    }
    if abilities.contains(&"cell") && abilities.contains(&"non_linear") {
        return Err(metadata_binding_error(format!("{label} cannot combine cell and non_linear")));
    }
    Ok(abilities)
}

fn ensure_canonical<T: Serialize>(label: &str, input: &[u8], value: &T) -> Result<(), CheckerError> {
    let canonical = canonical_bytes(value)?;
    if canonical != input {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2402NonCanonicalJson,
            format!("{label} is not byte-for-byte canonical JSON"),
        ));
    }
    Ok(())
}

fn ensure_byte_budget(label: &str, actual: usize, limit: u64) -> Result<(), CheckerError> {
    if actual as u64 > limit {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2400BudgetExceeded,
            format!("{label} bytes {actual} exceed budget {limit}"),
        ));
    }
    Ok(())
}

fn ensure_count(label: &str, actual: usize, limit: u32) -> Result<(), CheckerError> {
    if actual as u64 > u64::from(limit) {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2400BudgetExceeded,
            format!("{label} count {actual} exceeds budget {limit}"),
        ));
    }
    Ok(())
}

fn ensure_sorted_unique<'a, T, F>(values: &'a [T], key: F, label: &str) -> Result<(), CheckerError>
where
    F: Fn(&'a T) -> &'a str,
{
    if values.windows(2).all(|pair| key(&pair[0]) < key(&pair[1])) {
        Ok(())
    } else {
        Err(CheckerError::new(
            CheckerRejectionCode::V2404CanonicalOrder,
            format!("{label} identifiers are not strictly sorted and unique"),
        ))
    }
}

fn strictly_sorted<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn valid_alignment(value: u32) -> bool {
    value.is_power_of_two() && value <= 16
}

fn artifact_declared_too_large(actual: u64, limit: u64) -> bool {
    actual > limit
}

fn json_string<'a>(root: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter().try_fold(root, |value, key| value.get(*key)).and_then(Value::as_str)
}

fn json_u64(root: &Value, path: &[&str]) -> Option<u64> {
    path.iter().try_fold(root, |value, key| value.get(*key)).and_then(Value::as_u64)
}

pub fn domain_hash_bytes(domain: &str, bytes: &[u8]) -> String {
    let mut material = Vec::with_capacity(domain.len() + 1 + bytes.len());
    material.extend_from_slice(domain.as_bytes());
    material.push(0);
    material.extend_from_slice(bytes);
    hex_encode(&ckb_blake2b256(&material))
}

fn map_elf_error(error: ElfParseError) -> CheckerError {
    let code = match error.kind {
        ElfErrorKind::BudgetExceeded => CheckerRejectionCode::V2400BudgetExceeded,
        ElfErrorKind::InvalidSection | ElfErrorKind::ProhibitedLinkState | ElfErrorKind::MissingText => {
            CheckerRejectionCode::V2412ElfSectionInvalid
        }
        ElfErrorKind::InvalidInstruction => CheckerRejectionCode::V2413InstructionInvalid,
        ElfErrorKind::InvalidBranchTarget => CheckerRejectionCode::V2414ControlFlowInvalid,
        _ => CheckerRejectionCode::V2411ElfFormatInvalid,
    };
    CheckerError::new(code, error.message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn witness_view_checker_admission_retains_trim_source_and_extent_rules() {
        let vectors: serde_json::Value = serde_json::from_str(crate::view_types::WITNESS_BYTES_VIEW_CONFORMANCE_VECTORS).unwrap();
        for vector in vectors["cases"].as_array().unwrap() {
            let input = vector["input"].as_str().unwrap();
            let expected = vector["trimmed"].as_bool().unwrap() && vector["maximum"].as_u64().is_some_and(|maximum| maximum <= 65_536);
            assert_eq!(super::bounded_witness_view_parts(input).is_some(), expected, "{input:?}");
            for source in ["u64", "WitnessArgsView"] {
                assert_eq!(super::bounded_witness_view_retyping_move(source, input), expected, "{source}/{input:?}");
            }
            for source in ["Hash", "SourceView", "InputView<Token>", "WitnessBytesView<raw,64>", " u64"] {
                assert!(!super::bounded_witness_view_retyping_move(source, input), "{source}/{input:?}");
            }
        }
    }

    #[test]
    fn witness_view_handle_validation_still_rejects_source_owner_and_range_substitution() {
        use super::*;
        let valid = RuntimeTransactionViewHandle {
            scope_kind: "action".into(),
            scope_name: "inspect".into(),
            binding: "bytes".into(),
            handle_type: "WitnessBytesView<lock,64>".into(),
            source: "WitnessArgs/Input".into(),
            provenance: RuntimeAccessProvenance {
                contract: CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT.into(),
                source: RuntimeSourceProvenance { resolved_source: "Input".into(), origin: "witness-args".into(), binding: None },
                index: RuntimeScalarProvenance { kind: "static".into(), value: Some(0), binding: None, max_inclusive: Some(0) },
                range: RuntimeRangeProvenance {
                    kind: "bounded-range".into(),
                    offset: RuntimeScalarProvenance { kind: "static".into(), value: Some(0), binding: None, max_inclusive: Some(64) },
                    length: RuntimeScalarProvenance {
                        kind: "dynamic".into(),
                        value: None,
                        binding: Some("bytes.size".into()),
                        max_inclusive: Some(64),
                    },
                },
            },
            ownership: "read-only-view".into(),
            witness_owner: Some("lock".into()),
            max_bytes: Some(64),
            lifecycle_authority: false,
            typing_evidence_tier: "checked-static".into(),
            read_evidence_tier: "checked-runtime".into(),
        };
        validate_runtime_transaction_view_handle("view", &valid).unwrap();
        let mut wrong_source = valid.clone();
        wrong_source.provenance.source.resolved_source = "Output".into();
        assert!(validate_runtime_transaction_view_handle("view", &wrong_source).unwrap_err().message.contains("source"));
        let mut wrong_range = valid.clone();
        wrong_range.provenance.range.length.max_inclusive = Some(65);
        assert!(validate_runtime_transaction_view_handle("view", &wrong_range).unwrap_err().message.contains("range"));
        let mut wrong_owner = valid.clone();
        wrong_owner.witness_owner = Some("raw".into());
        assert!(validate_runtime_transaction_view_handle("view", &wrong_owner).unwrap_err().message.contains("owner"));
        let mut oversized = valid;
        oversized.handle_type = "WitnessBytesView<lock,65537>".into();
        oversized.max_bytes = Some(65537);
        oversized.provenance.range.offset.max_inclusive = Some(65537);
        oversized.provenance.range.length.max_inclusive = Some(65537);
        assert!(validate_runtime_transaction_view_handle("view", &oversized).unwrap_err().message.contains("maximum"));
    }

    #[test]
    fn canonical_json_protocol_vectors() {
        let vectors: serde_json::Value = serde_json::from_str(crate::CANONICAL_JSON_CONFORMANCE_VECTORS).unwrap();
        for vector in vectors["valid"].as_array().unwrap() {
            let input: serde_json::Value = serde_json::from_str(vector["input"].as_str().unwrap()).unwrap();
            assert_eq!(serde_json::to_string(&super::canonical_json_value(&input)).unwrap(), vector["canonical"].as_str().unwrap());
        }
        for input in vectors["invalid"].as_array().unwrap() {
            assert!(serde_json::from_str::<serde_json::Value>(input.as_str().unwrap()).is_err());
        }
    }

    use super::*;

    #[test]
    fn frame_store_overlap_covers_every_width_and_preserves_adjacent_slots() {
        for saved in [-2048, -8, 0, 8, 2047] {
            for width in [1, 2, 4, 8] {
                for delta in -8..=8 {
                    for stores_ra in [false, true] {
                        let mut slots = BTreeMap::from([(saved, true)]);
                        record_ra_frame_store(&mut slots, saved + delta, width, stores_ra);
                        let overlaps = delta < 8 && delta + width > 0;
                        let full_resave = delta == 0 && width == 8 && stores_ra;
                        assert_eq!(slots[&saved], !overlaps || full_resave, "{saved}/{width}/{delta}/{stores_ra}");
                    }
                }
            }
            let mut slots = BTreeMap::new();
            for width in [1, 2, 4] {
                record_ra_frame_store(&mut slots, saved, width, true);
                assert!(!slots.contains_key(&saved), "partial stores cannot establish a save");
            }
            record_ra_frame_store(&mut slots, saved, 8, true);
            record_ra_frame_store(&mut slots, saved + 7, 1, false);
            assert!(!slots[&saved]);
            record_ra_frame_store(&mut slots, saved, 8, true);
            assert!(slots[&saved]);
        }
    }

    #[test]
    fn empty_field_canonicalization_preserves_overlap_rejection_and_nonfixed_order() {
        let mut record = TypedSemanticRecord {
            types: vec![TypedSemanticType {
                name: "Mixed".into(),
                kind: "struct".into(),
                encoded_size: Some(8),
                fields: vec![
                    crate::TypedSemanticField {
                        name: "z".into(),
                        ty: "unit".into(),
                        offset: 0,
                        width_bytes: Some(0),
                        ..Default::default()
                    },
                    crate::TypedSemanticField {
                        name: "a".into(),
                        ty: "unit".into(),
                        offset: 0,
                        width_bytes: Some(0),
                        ..Default::default()
                    },
                    crate::TypedSemanticField {
                        name: "value".into(),
                        ty: "u64".into(),
                        offset: 0,
                        width_bytes: Some(8),
                        ..Default::default()
                    },
                    crate::TypedSemanticField {
                        name: "tail".into(),
                        ty: "unit".into(),
                        offset: 8,
                        width_bytes: Some(0),
                        ..Default::default()
                    },
                ],
                identity_policy: "none".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut dynamic = record.clone();
        dynamic.types[0].encoded_size = None;
        dynamic.types[0].fields[2].width_bytes = None;
        dynamic.canonicalize();
        assert_eq!(dynamic.types[0].fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["a", "value", "z", "tail"]);

        let rehash = |ty: &mut TypedSemanticType| {
            ty.layout_hash = canonical_hash(
                "cellscript-typed-layout-v2",
                &(
                    ty.kind.as_str(),
                    ty.encoded_size,
                    &ty.fields,
                    ty.tag_width_bytes,
                    &ty.variants,
                    &ty.capabilities,
                    &ty.identity_policy,
                ),
            )
            .unwrap();
        };
        record.canonicalize();
        assert_eq!(record.types[0].fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["a", "z", "value", "tail"]);
        rehash(&mut record.types[0]);
        validate_typed_type(&record.types[0]).unwrap();
        for outside in [false, true] {
            let mut changed = record.clone();
            if outside {
                changed.types[0].fields.iter_mut().find(|field| field.name == "tail").unwrap().offset = 9;
            } else {
                changed.types[0].fields.push(crate::TypedSemanticField {
                    name: "overlap".into(),
                    ty: "u64".into(),
                    offset: 0,
                    width_bytes: Some(8),
                    ..Default::default()
                });
            }
            changed.canonicalize();
            rehash(&mut changed.types[0]);
            assert!(validate_typed_type(&changed.types[0]).is_err(), "outside={outside}");
        }
    }

    fn public_interface_hash(interface: &Value) -> String {
        let canonical = canonical_json_value(interface);
        hex_encode(&ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()))
    }

    #[test]
    fn canonical_parser_rejects_whitespace_and_unknown_fields() {
        let budgets = CheckerBudgets::default();
        let unknown = br#"{"schema":"cellscript-source-artifact-map-v1","version":1,"module":"m","artifact_hash":"h","lowering_record_hash":"r","source_set_hash":"s","text_range":{"start":1,"end":2},"intervals":[],"coverage_claim":{"mapped_instruction_ranges_only":true,"complete_text_coverage":false,"source_semantic_equivalence":false},"unknown":true}"#;
        assert_eq!(parse_source_map(unknown, &budgets).unwrap_err().code, CheckerRejectionCode::V2401MalformedJson);

        let map = SourceArtifactMap {
            schema: SOURCE_MAP_SCHEMA.to_string(),
            version: SOURCE_MAP_VERSION,
            module: "m".to_string(),
            artifact_hash: "h".to_string(),
            lowering_record_hash: "r".to_string(),
            source_set_hash: "s".to_string(),
            source_digest: "d".to_string(),
            text_range: MachineRange { start: 1, end: 2 },
            intervals: Vec::new(),
            semantic_mappings: Vec::new(),
            coverage_claim: SourceMapCoverageClaim {
                mapped_instruction_ranges_only: true,
                complete_text_coverage: false,
                source_semantic_equivalence: false,
            },
        };
        let mut pretty = serde_json::to_vec_pretty(&map).unwrap();
        pretty.push(b'\n');
        assert_eq!(parse_source_map(&pretty, &budgets).unwrap_err().code, CheckerRejectionCode::V2402NonCanonicalJson);
    }

    #[test]
    fn independent_checker_enforces_canonical_public_value_generics() {
        use crate::interface::{InterfaceType, InterfaceTypeParameter, PackageInterface};
        let abilities = ["copy", "drop", "store", "fixed", "serializable", "non_linear"].map(str::to_string).to_vec();
        let interface = serde_json::to_value(PackageInterface {
            schema: crate::interface::PACKAGE_INTERFACE_SCHEMA.into(),
            version: crate::interface::PACKAGE_INTERFACE_VERSION,
            module: "api".into(),
            edition: "2027".into(),
            types: vec![InterfaceType {
                identity: "api::Pair".into(),
                name: "Pair".into(),
                kind: "struct".into(),
                visibility: "public".into(),
                type_parameters: vec![InterfaceTypeParameter { name: "T".into(), phantom: false, constraints: abilities.clone() }],
                value_abilities: abilities,
                ..Default::default()
            }],
            ..Default::default()
        })
        .unwrap();
        let hash = public_interface_hash(&interface);
        let metadata = serde_json::json!({ "edition": "2027", "public_interface": interface });
        validate_public_interface_metadata(&metadata, "api", &hash).unwrap();

        let mut compact_machine_form = metadata.clone();
        compact_machine_form["public_interface"]["types"][0]["type_parameters"][0]["constraints"] = serde_json::json!(["fixed_value"]);
        let compact_hash = public_interface_hash(&compact_machine_form["public_interface"]);
        let error = validate_public_interface_metadata(&compact_machine_form, "api", &compact_hash).unwrap_err();
        assert!(error.message.contains("canonical order"), "{}", error.message);

        let mut unsafe_layout = metadata;
        unsafe_layout["public_interface"]["types"][0]["type_parameters"][0]["constraints"] =
            serde_json::json!(["copy", "drop", "store", "fixed", "serializable"]);
        let unsafe_hash = public_interface_hash(&unsafe_layout["public_interface"]);
        let error = validate_public_interface_metadata(&unsafe_layout, "api", &unsafe_hash).unwrap_err();
        assert!(error.message.contains("public layout boundary"), "{}", error.message);
        unsafe_layout["public_interface"]["edition"] = serde_json::json!("2099");
        let legacy_hash = public_interface_hash(&unsafe_layout["public_interface"]);
        assert!(validate_public_interface_metadata(&unsafe_layout, "api", &legacy_hash).unwrap_err().message.contains("edition"));
        unsafe_layout["edition"] = serde_json::json!("2099");
        assert!(validate_public_interface_metadata(&unsafe_layout, "api", &legacy_hash).unwrap_err().message.contains("edition"));
    }

    #[test]
    fn checker_error_diagnostics_are_utf8_bounded() {
        let error = CheckerError::new(CheckerRejectionCode::V2401MalformedJson, "边界".repeat(100)).bounded(10);
        assert!(error.message.len() <= 10);
        assert!(std::str::from_utf8(error.message.as_bytes()).is_ok());
    }

    #[test]
    fn malformed_corpus_is_bounded_and_never_panics() {
        let budgets = CheckerBudgets {
            artifact_bytes: 4_096,
            record_bytes: 4_096,
            source_map_bytes: 4_096,
            diagnostic_bytes: 64,
            ..CheckerBudgets::default()
        };
        let corpus = [
            Vec::new(),
            vec![0xff],
            b"{".to_vec(),
            vec![b'{'; 4_097],
            (0..4_096).map(|index| (index % 251) as u8).collect::<Vec<_>>(),
        ];
        for bytes in corpus {
            let outcome = std::panic::catch_unwind(|| check_bundle(&bytes, &bytes, &bytes, &bytes, &budgets));
            let error = outcome.expect("checker must not panic on malformed bounded corpus").unwrap_err();
            assert!(error.message.len() <= budgets.diagnostic_bytes as usize);
        }
    }

    #[test]
    fn source_paths_are_confined() {
        assert!(safe_source_path("src/main.cell"));
        assert!(safe_source_path("<memory>"));
        assert!(!safe_source_path("../main.cell"));
        assert!(!safe_source_path("/tmp/main.cell"));
        assert!(!safe_source_path("C:/main.cell"));
    }

    #[test]
    fn canonical_abi_types_normalize_nested_builtin_names() {
        assert_ne!(canonical_abi_type("Hash::Value"), canonical_abi_type("hash::Value"));
        assert_ne!(canonical_abi_type("owner::Hash"), canonical_abi_type("owner::hash"));
        assert_eq!(canonical_abi_type("Hash :: Value<Hash>"), "Hash::Value<hash>");

        assert_eq!(canonical_abi_type("[(Address, u64); 2]"), canonical_abi_type("[(address, u64); 2]"));
        assert_ne!(canonical_abi_type("&[Hash; 4]"), canonical_abi_type("[hash; 4]"));
        assert_ne!(canonical_abi_type("Pair<u64>"), canonical_abi_type("Pair<u128>"));
        assert_ne!(canonical_abi_type("AddressBook"), canonical_abi_type("addressBook"));
        assert_ne!(canonical_abi_type("αHash"), canonical_abi_type("αhash"));
    }

    #[test]
    fn validated_nominal_coordinates_keep_builtin_spelling_case_distinct() {
        let mut catalog = NominalDeclarationCatalog { schema: NOMINAL_DECLARATION_CATALOG_SCHEMA.into(), ..Default::default() };
        let mut types = Vec::new();
        for (owner, name) in [("Hash", "Value"), ("hash", "Value"), ("owner", "Hash"), ("owner", "hash")] {
            catalog.declarations.push(NominalDeclarationContract {
                module: owner.into(),
                name: name.into(),
                kind: "struct".into(),
                visibility: "public".into(),
                fields: vec![TypedSemanticGenericField { name: "amount".into(), ty: "u64".into() }],
                identity_policy: "none".into(),
                ..Default::default()
            });
            catalog.layout_bindings.push(NominalLayoutBinding {
                owner_module: owner.into(),
                source_name: name.into(),
                lowered_name: format!("{owner}::{name}"),
            });
            let binding = NominalDeclarationBinding { local_name: name.into(), owner_module: owner.into(), source_name: name.into() };
            if let Some(scope) = catalog.scopes.iter_mut().find(|scope| scope.module == owner) {
                scope.bindings.push(binding);
            } else {
                catalog.scopes.push(NominalDeclarationScope { module: owner.into(), bindings: vec![binding] });
            }
            types.push(TypedSemanticType {
                name: format!("{owner}::{name}"),
                kind: "struct".into(),
                encoded_size: Some(8),
                identity_policy: "none".into(),
                fields: vec![TypedSemanticField {
                    name: "amount".into(),
                    ty: "u64".into(),
                    offset: 0,
                    width_bytes: Some(8),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        catalog.scopes.push(NominalDeclarationScope { module: "root".into(), bindings: vec![] });
        let record = TypedSemanticRecord { module: "root".into(), types, nominal_declarations: Some(catalog), ..Default::default() };
        crate::interface::verify_nominal_declarations(&record).unwrap();
        assert!(same_nominal_type("Hash::Value", "Hash::Value", &record));
        assert!(!same_nominal_type("Hash::Value", "hash::Value", &record));
        assert!(!same_nominal_type("owner::Hash", "owner::hash", &record));
    }

    #[test]
    fn committed_state_machine_widths_follow_canonical_fixed_layouts() {
        let layouts = [TypedSemanticType { name: "State".to_string(), encoded_size: Some(19), ..TypedSemanticType::default() }];
        assert_eq!(committed_state_type_width("bool", &layouts), Some(1));
        assert_eq!(committed_state_type_width("Hash", &layouts), Some(32));
        assert_eq!(committed_state_type_width("State", &layouts), Some(19));
        assert_eq!(committed_state_type_width("[u16; 3]", &layouts), Some(6));
        assert_eq!(committed_state_type_width("([u8; 4], (u32, State))", &layouts), Some(27));
        assert_eq!(committed_state_type_width("Vec<u8>", &layouts), None);
        assert_eq!(committed_state_type_width("[u128; 18446744073709551615]", &layouts), None);
    }

    #[test]
    fn script_projection_moves_accept_only_closed_cell_views_and_internal_references() {
        for source in ["InputView<Token>", "OutputView<Token>", "CellDepView", "SourceView"] {
            assert!(script_view_retyping_move(source, "__ckb_lock_script_ref"));
            assert!(script_view_retyping_move(source, "__ckb_type_script_ref"));
        }
        for source in ["HeaderDepView", "WitnessArgsView", "u64", "OutputView<"] {
            assert!(!script_view_retyping_move(source, "__ckb_lock_script_ref"));
        }
        for destination in ["ScriptView", "__ckb_script_ref", "Hash"] {
            assert!(!script_view_retyping_move("OutputView<Token>", destination));
        }
    }

    #[test]
    fn out_point_projection_moves_accept_only_typed_input_views_and_the_internal_reference() {
        assert!(input_out_point_retyping_move("InputView<Token>", "__ckb_input_out_point_ref"));
        for source in ["InputView<>", "OutputView<Token>", "CellDepView", "SourceView", "HeaderDepView", "u64"] {
            assert!(!input_out_point_retyping_move(source, "__ckb_input_out_point_ref"));
        }
        for destination in ["OutPoint", "__ckb_lock_script_ref", "Hash"] {
            assert!(!input_out_point_retyping_move("InputView<Token>", destination));
        }
    }

    #[test]
    fn lossless_unsigned_moves_include_the_rv64_usize_alias() {
        assert!(typed_value_assignable("u8", "usize"));
        assert!(typed_value_assignable("usize", "u64"));
        assert_eq!(arithmetic_result_type("u8", "usize").as_deref(), Some("u64"));
        assert!(!typed_value_assignable("u128", "usize"));
        assert!(!typed_value_assignable("i32", "usize"));
    }
}

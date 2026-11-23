//! CellScript - Domain-specific language compiler for CKB blockchain
//! Currently the backend can output RISC-V assembly or ELF artifacts.
#![cfg_attr(feature = "wasm", allow(dead_code))]

pub(crate) mod aggregate_lowering;
mod allocated_values;
pub mod artifact;
pub mod assumptions;
pub mod ast;
pub mod canonical_json;
pub mod ckb_abi;
mod commitment_contract;
mod compile_cache;
mod compile_pipeline;
mod metadata_build;
use compile_cache::*;
use compile_pipeline::*;
use metadata_build::compile_metadata_from_ir;

mod capability_metadata;
mod ckb_metadata_validation;
mod ckb_runtime_accesses;
mod collection_metadata;
mod conservation_metadata;
mod deployment_metadata;
mod entry_scope;
mod entry_witness;
mod metadata_abi_binding;
mod metadata_constraints;
mod metadata_layout;
mod metadata_validation;
mod metadata_validation_policy;
mod pool_metadata;
mod pool_transaction_requirements;
mod proof_plan_metadata;
mod runtime_availability;
mod runtime_metadata;
mod schema_metadata;
mod schema_metadata_validation;
mod source_loading;
mod source_provenance;
mod template_metadata;
mod transaction_requirements;
mod verifier_obligations;

#[cfg(not(feature = "wasm"))]
use capability_metadata::{capability_proof_metadata, capability_registry_metadata, payload_enum_layout_metadata};
#[cfg(not(feature = "wasm"))]
use ckb_runtime_accesses::body_ckb_runtime_accesses;
#[cfg(not(feature = "wasm"))]
use collection_metadata::module_collection_instantiation_metadata;
use deployment_metadata::apply_manifest_deploy_metadata;
#[cfg(feature = "wasm")]
use deployment_metadata::apply_trusted_external_verifiers;
#[cfg(not(feature = "wasm"))]
use deployment_metadata::apply_trusted_external_verifiers;
#[cfg(not(feature = "wasm"))]
use deployment_metadata::parse_ckb_cell_dep_location;
#[cfg(not(feature = "wasm"))]
use entry_scope::fungible_type_group_entry_metadata;
use entry_scope::scope_ir_to_entry;
#[cfg(not(feature = "wasm"))]
pub(crate) use entry_witness::entry_witness_static_type_len;
pub use entry_witness::{
    decode_scheduler_witness_hex, encode_bounded_output_plan_v1, encode_entry_witness_args_for_params,
    encode_entry_witness_args_for_params_with_runtime_bound, EntryWitnessArg,
};
use entry_witness::{entry_witness_scalar_param_width, runtime_bound_param_names};
#[cfg(not(feature = "wasm"))]
use entry_witness::{entry_witness_type_is_small_aggregate, param_consumes_entry_witness_payload};
use metadata_abi_binding::cell_pattern_metadata;
#[cfg(not(feature = "wasm"))]
use metadata_abi_binding::{create_pattern_metadata, mutate_pattern_metadata, param_metadata_for_body};
use metadata_constraints::bind_constraints_metadata;
use metadata_layout::{metadata_cell_type_kinds, metadata_type_defs_by_name, metadata_type_layouts};
#[cfg(not(feature = "wasm"))]
use metadata_layout::{metadata_flow_state_fields, metadata_flow_states, module_validity_path_coverage};
use metadata_validation::target_profile_artifact_policy_violations;
pub use metadata_validation::validate_compile_metadata;
#[cfg(not(feature = "wasm"))]
use pool_metadata::{body_pool_primitive_metadata, module_pool_primitive_metadata};
#[cfg(not(feature = "wasm"))]
use proof_plan_metadata::module_proof_plan_metadata;
use runtime_availability::{body_fail_closed_runtime_features, metadata_pure_const_returns};
#[cfg(test)]
use runtime_availability::{CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_NAMES, COLLECTION_FAIL_CLOSED_FEATURE_NAMES};
#[cfg(not(feature = "wasm"))]
use runtime_metadata::{
    auto_lowered_aggregate_runtime_accesses_for_action, borrow_region_metadata, ckb_script_group_metadata, merge_ckb_runtime_accesses,
    module_ckb_runtime_accesses, module_ckb_runtime_features, module_fail_closed_runtime_features, scheduler_accesses_from_metadata,
    transaction_view_handle_metadata,
};
use runtime_metadata::{
    auto_lowered_aggregate_runtime_features_for_action, body_ckb_runtime_features, ckb_signing_message_domains,
    merge_ckb_runtime_features, schema_pointer_var_ids,
};
use schema_metadata::{ir_type_to_string, is_default_identity_policy, type_molecule_schema_metadata};
#[cfg(not(feature = "wasm"))]
use schema_metadata::{molecule_schema_manifest_metadata, type_metadata};
pub use schema_metadata_validation::validate_source_units_on_disk;
use source_loading::{
    build_module_resolver_with_package_ids, canonical_utf8_path, collect_cell_files, collect_package_cell_files, load_manifest,
    load_project_for_entry, load_project_for_entry_diagnostics, load_virtual_project_for_entry_diagnostics,
    loaded_module_package_id_from_location, manifest_has_table, manifest_source_has_table, parse_package_manifest,
    prepare_loaded_project_for_entry_diagnostics, read_module_source, LoadedProject,
};
pub use source_loading::{
    explicit_manifest_entry, load_modules_for_input, load_own_package_modules_for_input, package_source_file_set, source_edition,
    LoadedModule,
};
#[cfg(not(feature = "wasm"))]
use source_loading::{loaded_module_package_id, parse_loaded_module};
use source_provenance::{bind_compile_result_source_metadata, bind_source_metadata};
#[cfg(not(feature = "wasm"))]
use template_metadata::{cell_data_codec_manifest_metadata, template_layout_metadata};
#[cfg(not(feature = "wasm"))]
use transaction_requirements::transaction_runtime_input_requirements_from_obligations;
#[cfg(not(feature = "wasm"))]
use verifier_obligations::{action_terminal_flow_obligations, body_verifier_obligations, module_verifier_obligations};
#[cfg(test)]
#[path = "compile_pipeline/diagnostic_tests.rs"]
mod compile_diagnostic_tests;
mod legacy_bytes;
mod metadata_abi;
mod optimization_policy;
pub use optimization_policy::COMPILATION_POLICY;
#[doc(hidden)]
pub use optimization_policy::{CompilerDiagnostics, OptimizationPass};
// CLI, LSP, REPL, incremental, and debug tooling are gated out of the
// wasm build: they depend on tokio/tower-lsp/clap/colored/env_logger,
// which pull native I/O (mio/std::net) that wasm32 cannot link. The
// pure compiler core (lexer/parser/types/flow/ir/codegen/proof_plan)
// stays ungated and is what the browser playground calls.
mod cell_data_view;
pub mod cell_selection;
#[cfg(not(feature = "wasm"))]
pub mod cli;
pub mod codegen;
#[cfg(not(feature = "wasm"))]
pub mod debug;
#[cfg(not(feature = "wasm"))]
pub mod deployment_line_handle;
pub mod docgen;
pub mod edition;
pub mod effects;
pub mod error;
pub mod executable_surface;
pub mod fixed_cell_builder;
pub mod flow;
pub mod fmt;
pub mod frontend;
mod generics;
#[cfg(not(feature = "wasm"))]
pub mod incremental;
pub mod interface;
pub mod ir;
pub mod lexer;
pub mod lsp;
pub mod nested_builder;
mod nested_values;
pub mod optimize;
pub mod package;
pub mod parser;
pub mod policy_witness;
pub mod proof_plan;
#[cfg(not(feature = "wasm"))]
pub mod protocol_bundle;
#[cfg(not(feature = "wasm"))]
pub mod repl;
pub mod resolve;
pub mod runtime_errors;
#[cfg(not(feature = "wasm"))]
pub mod schema_acknowledgement;
#[cfg(not(feature = "wasm"))]
pub mod script_handle;
mod script_handle_contract;
#[cfg(not(feature = "wasm"))]
pub mod semantic_expansion;
pub mod simulate;
pub mod stdlib;
pub mod syntax;
#[cfg(not(feature = "wasm"))]
mod typed_semantics;
pub mod types;
mod verified_artifact;
pub mod wasm;
#[cfg(not(feature = "wasm"))]
pub mod zk_client;
pub mod zk_contract;
pub mod zk_package;

pub use assumptions::{BuilderAssumptionMetadata, TxValidationReport, TxValidationViolation};
pub use cellscript_artifact_checker::{
    CheckerBudgets, CheckerReport, LayeredSemanticIdentities, SemanticFoundationRecord, SourceArtifactMap, VerifiedArtifactMetadata,
    VerifiedArtifactState, VerifiedLoweringRecord, CHECKER_POLICY_SCHEMA, CHECKER_VERSION, LOWERING_RECORD_SCHEMA,
    PROVENANCE_GRAPH_SCHEMA, SEMANTIC_FOUNDATION_SCHEMA, SOURCE_MAP_SCHEMA, TYPED_SEMANTICS_SCHEMA, VERIFIED_ARTIFACT_BOUNDARY_SCHEMA,
};
pub use edition::{
    resolve_compatibility_profile, source_language_contract, CellScriptEdition, ResolvedCompatibilityProfile, SourceLanguageContract,
    COMPATIBILITY_PROFILE_SCHEMA, CURRENT_EDITION,
};
pub use proof_plan::soundness::{ProofPlanSoundnessIssue, ProofPlanSoundnessReport};
pub use proof_plan::{EvidenceTier, ProofPlanDiagnosticMetadata, ProofPlanMetadata, ProofPlanSourceSpanMetadata};

use camino::{Utf8Path, Utf8PathBuf};
use error::{CompileError, DiagnosticSeverity, Result};
use package::{BuildConfig, PackageManifest};
use resolve::ModuleResolver;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Compile options
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Source-language edition for in-memory or standalone-file compilation.
    /// Package compilation uses the mandatory edition in `Cell.toml`.
    pub edition: CellScriptEdition,
    /// Internal compiler diagnostics; normal clients use the default strategy.
    #[doc(hidden)]
    pub diagnostics: crate::CompilerDiagnostics,
    /// Output file path
    pub output: Option<String>,
    /// Whether to generate debug information
    pub debug: bool,
    /// Emit bounded source declarations for independent interface projection.
    /// This supplies evidence; it does not authorize compatibility or deployment.
    /// Required nominal-alias and interface-handle evidence is retained regardless of this option.
    pub source_contracts: bool,
    /// Target artifact
    pub target: Option<String>,
    /// Target chain/profile. Only CKB is supported.
    pub target_profile: Option<String>,
    /// Primitive compatibility mode.
    /// - `None`: default (compat) behaviour
    /// - `Some("0.14")`: accept v0.14 syntax, emit migration hints
    /// - `Some("0.15")`: require v0.15 syntax, reject legacy forms
    /// - `Some("0.16")`: require v0.15 syntax plus v0.16 ProofPlan soundness
    /// - `Some("0.17")`: require v0.15 syntax plus v0.17 CKB source/ProofPlan gates
    /// - `Some("0.18")`: require v0.17 CKB source gates plus v0.18 ScriptRef/ScriptArgs surface
    pub primitive_compat: Option<String>,
}

impl CompileOptions {
    /// Returns true when running in v0.15 strict primitive mode.
    pub fn is_primitive_strict_015(&self) -> bool {
        matches!(self.primitive_compat.as_deref(), Some("0.15" | "0.16" | "0.17" | "0.18"))
    }

    /// Returns true when running in v0.16 strict assurance mode.
    pub fn is_assurance_strict_016(&self) -> bool {
        self.primitive_compat.as_deref() == Some("0.16")
    }

    /// Returns true when running in v0.17 strict CKB source mode.
    pub fn is_ckb_source_strict_017(&self) -> bool {
        matches!(self.primitive_compat.as_deref(), Some("0.17" | "0.18"))
    }

    /// Returns true when running in v0.14 compat mode (the default when
    /// no explicit flag is given, or when `--primitive-compat=0.14` is set).
    pub fn is_primitive_compat_014(&self) -> bool {
        !matches!(self.primitive_compat.as_deref(), Some("0.15" | "0.16" | "0.17" | "0.18"))
    }
}

fn validate_compile_options(options: &CompileOptions) -> Result<()> {
    #[cfg(feature = "wasm")]
    if options.source_contracts {
        return Err(CompileError::without_span(
            "source declaration contracts require the native compiler; the browser summary does not emit interface evidence",
        ));
    }
    if let Some(mode) = options.primitive_compat.as_deref() {
        match mode {
            "0.14" | "0.15" | "0.16" | "0.17" | "0.18" => {}
            other => {
                return Err(CompileError::without_span(format!(
                    "unsupported primitive compatibility mode '{}'; expected 0.14, 0.15, 0.16, 0.17, or 0.18",
                    other
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_primitive_strict_017_metadata(metadata: &CompileMetadata) -> Result<()> {
    let gaps = metadata
        .runtime
        .proof_plan
        .iter()
        .filter_map(|plan| {
            if !matches!(plan.category.as_str(), "declared-invariant" | "aggregate-invariant") {
                return None;
            }
            let rejects_metadata_only = plan.status == "runtime-required" || plan.codegen_coverage_status == "gap:metadata-only";
            let rejects_missing_runtime_helper = plan.codegen_coverage_status == "gap:runtime-helper-required";
            if rejects_metadata_only || rejects_missing_runtime_helper {
                let detail = if rejects_missing_runtime_helper {
                    let required = strict_017_required_runtime_helpers(plan);
                    if required.is_empty() {
                        "0.17 strict mode requires matching runtime-helper-required coverage in generated runtime accesses".to_string()
                    } else {
                        format!("0.17 strict mode requires matching {} coverage in generated runtime accesses", required.join(", "))
                    }
                } else {
                    "0.17 strict mode rejects metadata-only declared/aggregate invariant gaps".to_string()
                };
                Some(format!("PP0170 {}:{} - {}", plan.origin, plan.feature, detail))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if gaps.is_empty() {
        Ok(())
    } else {
        Err(CompileError::without_span(format!("0.17 CKB source strict check failed:\n  - {}", gaps.join("\n  - "))))
    }
}

fn strict_017_required_runtime_helpers(plan: &ProofPlanMetadata) -> Vec<String> {
    let mut helpers = BTreeSet::new();
    for coverage in &plan.coverage {
        if let Some(helper) = coverage.strip_prefix("runtime_helper:") {
            helpers.insert(format!("runtime-helper-required:{helper}"));
        }
    }
    for check in &plan.input_output_relation_checks {
        if let Some((_, helper)) = check.split_once("runtime-helper-required:") {
            helpers.insert(format!("runtime-helper-required:{helper}"));
        }
    }
    helpers.into_iter().collect()
}

/// In `--primitive-strict=0.15` or newer strict mode, reject v0.14 protocol verbs (`destroy`)
/// in capability declarations. They must be replaced by their kernel-effect equivalents.
fn check_primitive_strict_015(module: &ast::Module) -> Result<()> {
    use crate::error::MigrationDiagnostic;
    for item in &module.items {
        let (capabilities, type_name, span) = match item {
            ast::Item::Resource(r) => (&r.capabilities, r.name.as_str(), r.span),
            ast::Item::Shared(s) => (&s.capabilities, s.name.as_str(), s.span),
            ast::Item::Receipt(r) => (&r.capabilities, r.name.as_str(), r.span),
            _ => continue,
        };
        for cap in capabilities {
            if cap.is_protocol_verb() {
                let diag = MigrationDiagnostic::Cs0151;
                return Err(CompileError::new(
                    format!(
                        "{}: type '{}' declares legacy capability '{}', which is not allowed in primitive strict mode; use kernel effects '{}'",
                        diag.full_message(),
                        type_name,
                        strict_capability_name(*cap),
                        cap.kernel_effects().iter().map(|c| strict_capability_name(*c)).collect::<Vec<_>>().join(", "),
                    ),
                    span,
                ));
            }
        }
    }
    Ok(())
}

fn strict_capability_name(capability: ast::Capability) -> &'static str {
    capability.as_str()
}

const DEFAULT_TARGET: &str = "riscv64-asm";
const DEFAULT_TARGET_PROFILE: &str = "ckb";
pub const METADATA_SCHEMA_VERSION: u32 = cellscript_artifact_checker::COMPILE_METADATA_SCHEMA_VERSION;
pub const SOURCE_METADATA_SCHEMA_VERSION: u32 = 2;
pub const ARTIFACT_METADATA_SCHEMA_VERSION: u32 = 1;
pub const CONSTRAINTS_METADATA_SCHEMA_VERSION: u32 = 4;
pub const CKB_RUNTIME_VIEW_CONTRACT: &str = "cellscript-ckb-runtime-view-v1";
pub const CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT: &str = "cellscript-ckb-runtime-access-provenance-v1";
pub const CKB_SIGHASH_ALL_ZERO_LOCK_CONTRACT: &str = "cellscript-ckb-sighash-all-zero-lock-v1";
/// Maximum args payload admitted by source-level canonical CKB Script hashing.
///
/// A packed Molecule Script occupies 53 bytes before its args payload. Keeping
/// `53 + args.len()` within the backend's 512-byte bounded scratch buffer makes
/// the complete preimage size static and independently checkable.
pub const CKB_SCRIPT_HASH_MAX_ARGS_BYTES: usize = 459;
/// Maximum UTF-8 source bytes accepted by a single compiler input.
///
/// This is a process-safety boundary shared by native, LSP, and WASM callers.
pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const STACK_COLLECTION_BACKING_BYTES: usize = 256;
pub const ENTRY_WITNESS_ABI: &str = "cellscript-entry-witness-v1";

pub(crate) const ENTRY_WITNESS_ABI_MAGIC: &[u8; 8] = b"CSARGv1\0";
/// Versioned CKB placement contract for parameterized entry payloads.
pub const ENTRY_WITNESS_PLACEMENT_ABI: &str = "cellscript-witnessargs-input-type-v2";
/// Canonical `WitnessArgs` field owned by the CellScript entry placement ABI.
pub const ENTRY_WITNESS_PLACEMENT_FIELD: &str = "input_type";
/// Script-group-relative witness lookup order used by generated CKB entries.
pub const ENTRY_WITNESS_PLACEMENT_SOURCE: &str = "group-input-0-then-group-output-0";
pub const CKB_DEFAULT_HASH_PERSONALIZATION: &[u8; 16] = b"ckb-default-hash";
pub const CKB_BLANK_HASH: [u8; 32] = [
    68, 244, 198, 151, 68, 213, 248, 197, 93, 100, 32, 98, 148, 157, 202, 228, 155, 196, 231, 239, 67, 211, 136, 197, 161, 47, 66,
    181, 99, 61, 22, 62,
];
const METADATA_MUTATE_CELL_BUFFER_SIZE: usize = 512;
const CKB_TYPE_ID_CODE_HASH: [u8; 32] = ckb_abi::type_id::CODE_HASH;
const CKB_TYPE_ID_ABI: &str = "ckb-type-id-v1";
const CKB_TYPE_ID_HASH_TYPE: &str = "type";
const CKB_DEFAULT_SCRIPT_HASH_TYPE: &str = "data2";
const CKB_TYPE_HASH_SCRIPT_HASH_TYPE: &str = "type";
const CKB_MINIMUM_VM_VERSION: u8 = 2;
const CKB_RISCV_ISA: &str = cellscript_artifact_checker::CKB_RISCV_ISA;
const CKB_TYPE_ID_ARGS_SOURCE: &str = "first-input-output-index";
const CKB_TYPE_ID_GROUP_RULE: &str = "at-most-one-input-and-one-output";
const CKB_TYPE_ID_BUILDER: &str = "ckb_apply_type_id_script_to_output_molecule";
const CKB_TYPE_ID_VERIFIER: &str = "ckb_verify_type_id_script_molecule";
const CKB_TYPE_ID_OUTPUT_SOURCE: &str = "Output";
const CKB_TYPE_ID_GENERATOR_SETTING: &str = "ckb_type_id_output_indexes";
const CKB_TYPE_ID_WASM_SETTING: &str = "ckbTypeIdOutputs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProfile {
    Ckb,
    CkbTypeHash,
}

impl TargetProfile {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "ckb" => Ok(Self::Ckb),
            "ckb-type-hash" => Ok(Self::CkbTypeHash),
            other => Err(CompileError::without_span(format!(
                "unsupported target profile '{}'; supported profiles: ckb, ckb-type-hash",
                other
            ))),
        }
    }

    fn from_options(options: &CompileOptions, build: Option<&BuildConfig>) -> Result<Self> {
        let profile = options
            .target_profile
            .as_deref()
            .or_else(|| build.and_then(|build| build.target_profile.as_deref()))
            .unwrap_or(DEFAULT_TARGET_PROFILE);
        Self::from_name(profile)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Ckb => "ckb",
            Self::CkbTypeHash => "ckb-type-hash",
        }
    }

    pub fn is_ckb(self) -> bool {
        matches!(self, Self::Ckb | Self::CkbTypeHash)
    }

    pub fn deployment_hash_type(self) -> &'static str {
        match self {
            Self::Ckb => CKB_DEFAULT_SCRIPT_HASH_TYPE,
            Self::CkbTypeHash => CKB_TYPE_HASH_SCRIPT_HASH_TYPE,
        }
    }

    fn ensure_compile_supported(self) -> Result<()> {
        Ok(())
    }

    fn embeds_vm_abi_trailer(self, _artifact_format: ArtifactFormat) -> bool {
        false
    }

    fn metadata(self, artifact_format: ArtifactFormat) -> TargetProfileMetadata {
        TargetProfileMetadata {
            name: self.name().to_string(),
            target_chain: "ckb".to_string(),
            vm_abi: "ckb-molecule".to_string(),
            hash_domain: "ckb-packed-molecule-blake2b".to_string(),
            syscall_set: "ckb-mainnet-syscalls".to_string(),
            artifact_packaging: match artifact_format {
                ArtifactFormat::RiscvAssembly => "ckb-asm-sidecar".to_string(),
                ArtifactFormat::RiscvElf => "ckb-elf".to_string(),
            },
            header_abi: "ckb-header".to_string(),
            scheduler_abi: "none".to_string(),
            witness_abi: "ckb-molecule-witness-args-input-type-v2+cellscript-entry-witness-v1".to_string(),
            lock_args_abi: "ckb-script-args-typed-fixed-bytes".to_string(),
            source_encoding: "ckb-source-group-high-bit".to_string(),
            spawn_ipc_abi: "ckb-vm-v2-spawn-ipc-syscalls-2601-2608".to_string(),
            since_abi: "ckb-since-rfc0017-typed-v1".to_string(),
            cell_dep_abi: "ckb-cell-dep-outpoint-and-dep-group".to_string(),
            script_ref_abi: "ckb-script-code-hash-hash-type-args".to_string(),
            output_data_abi: "ckb-outputs-and-outputs-data-index-aligned".to_string(),
            capacity_floor_abi: "ckb-output-capacity-floor-shannons".to_string(),
            type_id_abi: CKB_TYPE_ID_ABI.to_string(),
            minimum_vm_version: CKB_MINIMUM_VM_VERSION,
            riscv_isa: CKB_RISCV_ISA.to_string(),
            deployment_hash_types: vec![self.deployment_hash_type().to_string()],
            tx_version: 0,
        }
    }
}

/// Artifact format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    /// RISC-V assembly text
    RiscvAssembly,
    /// RISC-V ELF executable
    RiscvElf,
}

impl ArtifactFormat {
    pub fn from_target(target: &str) -> Result<Self> {
        match target {
            "asm" | "riscv64" | "riscv64-asm" => Ok(Self::RiscvAssembly),
            "elf" | "riscv64-elf" => Ok(Self::RiscvElf),
            other => Err(CompileError::new(
                format!("unsupported target '{}'; supported targets: asm, riscv64-asm, riscv64, riscv64-elf", other),
                error::Span::default(),
            )),
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::RiscvAssembly => "s",
            Self::RiscvElf => "elf",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::RiscvAssembly => "RISC-V assembly",
            Self::RiscvElf => "RISC-V ELF",
        }
    }

    pub fn from_display_name(value: &str) -> Result<Self> {
        match value {
            "RISC-V assembly" => Ok(Self::RiscvAssembly),
            "RISC-V ELF" => Ok(Self::RiscvElf),
            other => Err(CompileError::without_span(format!("unsupported artifact format in metadata: '{}'", other))),
        }
    }
}

/// Compile result
#[derive(Debug, Clone)]
pub struct CompileResult {
    /// Generated artifact bytes
    pub artifact_bytes: Vec<u8>,
    /// Artifact format
    pub artifact_format: ArtifactFormat,
    /// Artifact hash
    pub artifact_hash: [u8; 32],
    /// Compile metadata consumable by schedulers/tools
    pub metadata: CompileMetadata,
    /// Parsed AST (for simulation, etc.)
    pub ast: crate::ast::Module,
    /// Canonical verified-lowering sidecar for ELF artifacts.
    pub verified_lowering_record: Option<VerifiedLoweringRecord>,
    /// Canonical source-to-artifact sidecar for ELF artifacts.
    pub source_artifact_map: Option<SourceArtifactMap>,
    pub(crate) verified_artifact_draft: Option<verified_artifact::VerifiedArtifactDraft>,
    /// Compiler-owned schema/nominal evidence for exact off-chain upgrade review.
    /// This is never serialized into browser metadata or reconstructed from a cache.
    #[cfg(not(feature = "wasm"))]
    pub(crate) schema_review: Option<schema_acknowledgement::SchemaReviewCompilation>,
    /// Whether this result was served from the incremental compilation cache
    pub cache_hit: bool,
}

/// Validated artifact/metadata pair loaded from disk.
#[derive(Debug, Clone)]
pub struct ValidatedArtifact {
    /// Artifact bytes read from disk
    pub artifact_bytes: Vec<u8>,
    /// Artifact format declared by metadata
    pub artifact_format: ArtifactFormat,
    /// Computed artifact hash
    pub artifact_hash: [u8; 32],
    /// Compile metadata bound to the artifact
    pub metadata: CompileMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileMetadata {
    pub metadata_schema_version: u32,
    #[serde(default = "missing_metadata_component_schema_version")]
    pub source_metadata_schema_version: u32,
    #[serde(default = "missing_metadata_component_schema_version")]
    pub artifact_metadata_schema_version: u32,
    #[serde(default = "missing_metadata_component_schema_version")]
    pub constraints_metadata_schema_version: u32,
    pub compiler_version: String,
    pub edition: CellScriptEdition,
    pub compatibility_profile: ResolvedCompatibilityProfile,
    pub module: String,
    // Full interface and typed-operation records are native artifact evidence.
    // The bounded browser summary omits them so the metadata-only Playground
    // does not ship a second copy of their large construction/serde surface.
    #[cfg_attr(feature = "wasm", serde(skip))]
    #[serde(default)]
    pub public_interface: interface::PackageInterface,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub interface_hash: String,
    #[cfg_attr(feature = "wasm", serde(skip))]
    #[serde(default)]
    pub typed_semantics: cellscript_artifact_checker::TypedSemanticRecord,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub typed_semantics_hash: String,
    pub artifact_format: String,
    pub target_profile: TargetProfileMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_size_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_units: Vec<SourceUnitMetadata>,
    #[serde(default)]
    pub verified_artifact: VerifiedArtifactMetadata,
    pub lowering: LoweringMetadata,
    #[serde(default)]
    pub capability_registry: CapabilityRegistryMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_layouts: Vec<PayloadEnumLayoutMetadata>,
    #[cfg_attr(feature = "wasm", serde(skip))]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generic_instantiations: Vec<GenericInstantiationMetadata>,
    pub runtime: RuntimeMetadata,
    #[serde(default)]
    pub constraints: ConstraintsMetadata,
    #[serde(default)]
    pub molecule_schema_manifest: MoleculeSchemaManifestMetadata,
    #[serde(default)]
    pub cell_data_codec_manifest: CellDataCodecManifestMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub template_layouts: Vec<TemplateLayoutMetadata>,
    pub types: Vec<TypeMetadata>,
    pub actions: Vec<ActionMetadata>,
    pub functions: Vec<FunctionMetadata>,
    pub locks: Vec<LockMetadata>,
    /// Embedded DWARF debug section names (non-empty when debug mode is enabled for ELF artifacts)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub debug_info_sections: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenericInstantiationMetadata {
    pub schema: String,
    pub version: u32,
    pub kind: String,
    pub module: String,
    pub template: String,
    pub concrete_name: String,
    pub identity: String,
    pub type_arguments: Vec<String>,
    pub lowered_names: Vec<String>,
    pub parameters: Vec<cellscript_artifact_checker::TypedSemanticGenericParameter>,
    pub declaration: cellscript_artifact_checker::TypedSemanticGenericDeclaration,
    pub value_ability_registry_version: u32,
    pub constraints_verified: bool,
    pub fixed_layout_required: bool,
    pub cell_backed_layout_rejected: bool,
    pub identity_includes_phantom_arguments: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityRegistryMetadata {
    pub capability_set_version: u32,
    pub entailment_version: u32,
    pub capabilities: Vec<String>,
    pub operation_rules: Vec<CapabilityOperationRuleMetadata>,
    pub inheritance_syntax: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityOperationRuleMetadata {
    pub operation: String,
    pub required: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub legacy_alternatives: Vec<String>,
    pub identity_preservation_required: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadEnumLayoutMetadata {
    pub name: String,
    pub generic: bool,
    pub layout: String,
    pub abi: String,
    pub storage: String,
    pub tag_width_bytes: usize,
    pub encoded_size_bytes: usize,
    pub contains_linear_payload: bool,
    pub variants: Vec<PayloadEnumVariantMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadEnumVariantMetadata {
    pub name: String,
    pub tag: u8,
    pub payload_width_bytes: usize,
    pub fields: Vec<PayloadEnumFieldMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PayloadEnumFieldMetadata {
    pub index: usize,
    pub r#type: String,
    pub offset_bytes: usize,
    pub width_bytes: usize,
    pub ownership: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProtocolRoleCandidateMetadata {
    pub role: String,
    pub source: String,
    pub strength: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_span: Option<ProofPlanSourceSpanMetadata>,
    pub evidence_tier: EvidenceTier,
    pub authorization_proven: bool,
}

fn missing_metadata_component_schema_version() -> u32 {
    0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoleculeSchemaManifestMetadata {
    pub schema: String,
    pub version: u32,
    pub abi: String,
    pub target_profile: String,
    pub type_count: usize,
    pub fixed_type_count: usize,
    pub dynamic_type_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<MoleculeSchemaManifestEntryMetadata>,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoleculeSchemaManifestEntryMetadata {
    pub type_name: String,
    pub kind: String,
    pub layout: String,
    pub fixed_size: usize,
    pub encoded_size: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic_fields: Vec<String>,
    pub schema_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_offsets: Vec<MoleculeSchemaManifestFieldMetadata>,
    pub target_profile_compatible: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoleculeSchemaManifestFieldMetadata {
    pub name: String,
    pub ty: String,
    pub offset: usize,
    pub encoded_size: Option<usize>,
    pub fixed_width: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellDataCodecManifestMetadata {
    pub schema: String,
    pub version: u32,
    pub target_profile: String,
    pub abi: String,
    pub default_codec: String,
    pub molecule_schema_manifest_hash: String,
    pub raw_bytes_required: bool,
    pub raw_runtime_access_count: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_runtime_accesses: Vec<CellDataCodecRuntimeAccessMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_codec_adapters: Vec<ExternalCodecAdapterMetadata>,
    pub manifest_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellDataCodecRuntimeAccessMetadata {
    pub operation: String,
    pub syscall: String,
    pub source: String,
    pub index: usize,
    pub binding: String,
    pub codec: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExternalCodecAdapterMetadata {
    pub name: String,
    pub version: String,
    pub package: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_vector_hashes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateLayoutMetadata {
    pub schema: String,
    pub version: u32,
    pub scope: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_field: Option<String>,
    pub state_machine_acyclic: bool,
    pub cycle_policy: String,
    pub layout: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_hash: Option<String>,
    pub leaf_schema: TemplateLayoutLeafSchemaMetadata,
    pub consensus_checked: bool,
    pub template_layout_hash: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TemplateLayoutLeafSchemaMetadata {
    pub abi: String,
    pub layout: String,
    pub schema_hash: String,
    pub encoded_size: Option<usize>,
    pub field_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConstraintsMetadata {
    pub edition: CellScriptEdition,
    pub compatibility_profile: String,
    pub target_profile: String,
    pub status: String,
    pub entry_abi: Vec<EntryAbiConstraintsMetadata>,
    pub artifact: ArtifactConstraintsMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collection_instantiations: Vec<CollectionInstantiationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_errors: Vec<RuntimeErrorConstraintsMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ckb: Option<CkbConstraintsMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeErrorConstraintsMetadata {
    pub code: u64,
    pub name: String,
    pub description: String,
    pub hint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryAbiConstraintsMetadata {
    pub entry_kind: String,
    pub entry_name: String,
    pub param_count: usize,
    pub abi_slots_used: usize,
    pub register_slots_used: usize,
    pub stack_spill_slots: usize,
    pub stack_spill_bytes: usize,
    pub witness_payload_bytes: usize,
    pub min_witness_bytes: usize,
    pub unsupported: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported_reasons: Vec<String>,
    pub params: Vec<ParamAbiConstraintsMetadata>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParamAbiConstraintsMetadata {
    pub name: String,
    pub ty: String,
    pub abi_kind: String,
    pub abi_slots: usize,
    pub slot_start: usize,
    pub slot_end: usize,
    pub register_slots: usize,
    pub stack_spill_slots: usize,
    pub stack_spill_bytes: usize,
    pub witness_bytes: usize,
    pub pointer_length_pair: bool,
    pub pointer_pair_crosses_register_boundary: bool,
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactConstraintsMetadata {
    pub format: String,
    pub artifact_size_bytes: usize,
    pub text_bytes: Option<usize>,
    pub rodata_bytes: Option<usize>,
    pub executable_text_op_count: Option<usize>,
    pub covered_text_op_count: Option<usize>,
    pub relaxed_branch_count: Option<usize>,
    pub max_cond_branch_abs_distance: Option<u64>,
    pub machine_block_count: Option<usize>,
    pub max_machine_block_size: Option<usize>,
    pub conditional_branch_block_count: Option<usize>,
    pub labeled_machine_block_count: Option<usize>,
    pub machine_cfg_edge_count: Option<usize>,
    pub machine_call_edge_count: Option<usize>,
    pub unreachable_machine_block_count: Option<usize>,
    pub layout_order_block_count: Option<usize>,
    pub layout_order_text_size: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbConstraintsMetadata {
    pub limits_source: String,
    #[serde(default)]
    pub profile_abi_contract: CkbProfileAbiContractMetadata,
    #[serde(default)]
    pub hash_domain: String,
    #[serde(default)]
    pub script_hash_algorithm: String,
    #[serde(default)]
    pub transaction_hash_algorithm: String,
    #[serde(default)]
    pub sighash_algorithm: String,
    #[serde(default)]
    pub supported_script_hash_types: Vec<String>,
    #[serde(default)]
    pub declared_type_id_hash_type: String,
    #[serde(default)]
    pub hash_type_policy_surface: String,
    #[serde(default)]
    pub hash_type_policy: CkbHashTypePolicyMetadata,
    #[serde(default)]
    pub dep_group_manifest: CkbDepGroupManifestMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub script_references: Vec<CkbScriptReferenceMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_external_verifiers: Vec<cellscript_artifact_checker::TrustedExternalVerifierRecord>,
    pub max_tx_verify_cycles: u64,
    pub max_block_cycles: u64,
    pub max_block_bytes: u64,
    pub estimated_cycles: Option<u64>,
    pub measured_cycles: Option<u64>,
    pub cycles_status: String,
    pub min_code_cell_data_capacity_shannons: u64,
    pub recommended_code_cell_capacity_shannons: u64,
    pub min_witness_bytes: usize,
    pub max_entry_witness_bytes: usize,
    pub dry_run_required_for_production: bool,
    pub tx_size_bytes: Option<usize>,
    #[serde(default)]
    pub tx_size_measurement_required: bool,
    pub tx_size_status: String,
    #[serde(default)]
    pub occupied_capacity_measurement_required: bool,
    pub capacity_status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ckb_runtime_features: Vec<String>,
    pub uses_input_since: bool,
    pub uses_header_epoch: bool,
    pub transaction_runtime_input_requirement_count: usize,
    pub timelock_policy_surface: String,
    #[serde(default)]
    pub timelock_policy: CkbTimelockPolicyMetadata,
    pub created_output_count: usize,
    pub mutated_output_count: usize,
    pub capacity_planning_required: bool,
    pub capacity_policy_surface: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared_capacity_floors: Vec<CkbCapacityFloorMetadata>,
    #[serde(default)]
    pub capacity_evidence_contract: CkbCapacityEvidenceContractMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbCapacityFloorMetadata {
    pub type_name: String,
    pub shannons: u64,
    pub source: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbScriptReferenceMetadata {
    pub scope: String,
    pub purpose: String,
    pub name: String,
    pub code_hash: Option<String>,
    pub hash_type: Option<String>,
    pub args: Option<String>,
    pub dep_source: String,
    pub profile: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbProfileAbiContractMetadata {
    pub witness_abi: String,
    pub lock_args_abi: String,
    pub source_encoding: String,
    pub spawn_ipc_abi: String,
    pub since_abi: String,
    pub cell_dep_abi: String,
    pub script_ref_abi: String,
    pub output_data_abi: String,
    pub capacity_floor_abi: String,
    pub type_id_abi: String,
    #[serde(default)]
    pub minimum_vm_version: u8,
    #[serde(default)]
    pub riscv_isa: String,
    #[serde(default)]
    pub deployment_hash_types: Vec<String>,
    pub tx_version: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbHashTypePolicyMetadata {
    pub source: String,
    pub default_script_hash_type: String,
    pub declared_hash_type: Option<String>,
    pub type_id_hash_type: String,
    pub supported_hash_types: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbDepGroupManifestMetadata {
    pub source: String,
    pub dep_group_supported: bool,
    pub production_manifest_required: bool,
    pub declared_cell_deps: Vec<CkbCellDepMetadata>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbCellDepMetadata {
    pub name: String,
    pub artifact_hash: Option<String>,
    pub tx_hash: Option<String>,
    pub index: Option<u32>,
    pub dep_type: String,
    pub data_hash: Option<String>,
    pub hash_type: Option<String>,
    pub type_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbTimelockPolicyMetadata {
    pub uses_input_since: bool,
    pub uses_header_epoch: bool,
    pub policy_kind: String,
    pub runtime_features: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CkbCapacityEvidenceContractMetadata {
    pub required: bool,
    pub code_cell_lower_bound_shannons: u64,
    pub recommended_code_cell_capacity_shannons: u64,
    pub occupied_capacity_measurement_required: bool,
    pub tx_size_measurement_required: bool,
    pub measured_occupied_capacity_shannons: Option<u64>,
    pub measured_tx_size_bytes: Option<usize>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceUnitMetadata {
    pub path: String,
    pub role: String,
    pub hash: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoweringMetadata {
    pub protocol_semantics: String,
    pub assembly_path: String,
    pub elf_path: String,
    pub semantics_preserving_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub vm_target: String,
    pub vm_version: String,
    pub syscall_abi: String,
    #[serde(default)]
    pub ckb_runtime_view_contract: String,
    #[serde(default)]
    pub ckb_runtime_access_provenance_contract: String,
    pub vm_abi: VmAbiMetadata,
    pub pure_elf_runner: String,
    pub ckb_runtime_required: bool,
    pub ckb_runtime_features: Vec<String>,
    pub standalone_runner_compatible: bool,
    pub fail_closed_runtime_features: Vec<String>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signing_message_domains: Vec<CkbSigningMessageDomainMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zk_verifiers: Vec<cellscript_artifact_checker::zk_profile::ZkVerifierContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_external_verifiers: Vec<cellscript_artifact_checker::TrustedExternalVerifierRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transaction_view_handles: Vec<TransactionViewHandleMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub borrow_regions: Vec<BorrowRegionMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capability_proofs: Vec<CapabilityProofMetadata>,
    pub verifier_obligations: Vec<VerifierObligationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_plan: Vec<ProofPlanMetadata>,
    #[serde(default)]
    pub proof_plan_soundness: ProofPlanSoundnessReport,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub builder_assumptions: Vec<BuilderAssumptionMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collection_instantiations: Vec<CollectionInstantiationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transaction_runtime_input_requirements: Vec<TransactionRuntimeInputRequirementMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_primitives: Vec<PoolPrimitiveMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fungible_type_group_entry: Option<FungibleTypeGroupEntryMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_artifact: Option<artifact::PolicyArtifactMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbSigningMessageDomainMetadata {
    pub contract: String,
    pub scope_kind: String,
    pub scope_name: String,
    pub binding: String,
    pub digest_type: String,
    pub hash_algorithm: String,
    pub transaction_hash_source: String,
    pub group_scope: String,
    pub first_witness_source: String,
    pub first_witness_lock_transform: String,
    pub witness_length_prefix: String,
    pub later_group_witness_order: String,
    pub extra_witness_source: String,
    pub max_group_inputs: u64,
    pub max_inputs: u64,
    pub max_extra_witnesses: u64,
    pub max_witness_bytes: u64,
    pub runtime_helper: String,
    pub evidence_tier: EvidenceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FungibleTypeGroupEntryMetadata {
    pub contract: String,
    pub entry_action: String,
    pub invariant: String,
    pub type_name: String,
    pub field: String,
    pub data_length_bytes: usize,
    pub amount_offset_bytes: usize,
    pub amount_width_bytes: usize,
    pub endianness: String,
    pub arithmetic: String,
    pub group_scope: String,
    pub owner_mode: String,
    pub owner_args_length_bytes: usize,
    pub authority_modes: Vec<String>,
    pub authority_args_lengths_bytes: Vec<usize>,
    pub owner_authorized_mint: bool,
    pub owner_authorized_burn: bool,
    pub non_owner_input_group_non_empty: bool,
    pub non_owner_output_group_non_empty: bool,
    pub non_owner_conservation_required: bool,
    pub payload_required: bool,
    pub witness_policy: String,
    pub runtime_helper: String,
    pub evidence_tier: EvidenceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransactionViewHandleMetadata {
    pub scope_kind: String,
    pub scope_name: String,
    pub binding: String,
    pub handle_type: String,
    pub source: String,
    #[serde(default)]
    pub provenance: CkbRuntimeAccessProvenanceMetadata,
    pub ownership: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u64>,
    pub lifecycle_authority: bool,
    pub typing_evidence_tier: EvidenceTier,
    pub read_evidence_tier: EvidenceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BorrowRegionMetadata {
    pub scope_kind: String,
    pub scope_name: String,
    pub root: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    pub binding: String,
    pub view_type: String,
    pub storage: String,
    pub abi: String,
    pub evidence_tier: EvidenceTier,
    pub source_span: crate::proof_plan::ProofPlanSourceSpanMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityProofMetadata {
    pub scope_kind: String,
    pub scope_name: String,
    pub operation: String,
    pub type_name: String,
    pub binding: String,
    pub required: Vec<String>,
    pub provided: Vec<String>,
    pub entailed: Vec<String>,
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_condition: Option<String>,
    pub capability_set_version: u32,
    pub entailment_version: u32,
    pub evidence_tier: EvidenceTier,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CollectionInstantiationMetadata {
    pub scope_kind: String,
    pub scope_name: String,
    pub collection_ty: String,
    pub element_ty: String,
    pub element_width_bytes: usize,
    pub backing: String,
    pub max_elements: usize,
    pub status: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ownership: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_scanned_cardinality: Option<usize>,
    #[serde(default)]
    pub vacuous_possible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cardinality_max: Option<usize>,
    #[serde(default)]
    pub capacity_builder_evidence_required: bool,
    #[serde(default = "default_metadata_only_evidence_tier")]
    pub evidence_tier: EvidenceTier,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub helpers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounded_output_plan: Option<BoundedOutputPlanContractMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedOutputPlanContractMetadata {
    pub schema: String,
    pub version: u32,
    pub binding: String,
    pub codec: String,
    pub codec_magic_hex: String,
    pub placement: String,
    pub allow_zero: bool,
    pub element_ty: String,
    pub element_width_bytes: usize,
    pub max_elements: usize,
    pub output_source: String,
    pub ordering: String,
    pub correspondence: String,
    pub output_ty: String,
    pub output_width_bytes: usize,
    pub type_script_policy: String,
    pub lock_policy: String,
    pub capacity_floor_shannons: u64,
    pub identity_policy: String,
    pub field_bindings: Vec<BoundedOutputFieldBindingMetadata>,
    pub lock_binding: BoundedOutputValueBindingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedOutputFieldBindingMetadata {
    pub output_field: String,
    pub output_offset_bytes: usize,
    pub plan_field: String,
    pub plan_offset_bytes: usize,
    pub width_bytes: usize,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundedOutputValueBindingMetadata {
    pub plan_field: String,
    pub plan_offset_bytes: usize,
    pub width_bytes: usize,
    pub ty: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetProfileMetadata {
    pub name: String,
    pub target_chain: String,
    pub vm_abi: String,
    pub hash_domain: String,
    pub syscall_set: String,
    pub artifact_packaging: String,
    pub header_abi: String,
    pub scheduler_abi: String,
    #[serde(default)]
    pub witness_abi: String,
    #[serde(default)]
    pub lock_args_abi: String,
    #[serde(default)]
    pub source_encoding: String,
    #[serde(default)]
    pub spawn_ipc_abi: String,
    #[serde(default)]
    pub since_abi: String,
    #[serde(default)]
    pub cell_dep_abi: String,
    #[serde(default)]
    pub script_ref_abi: String,
    #[serde(default)]
    pub output_data_abi: String,
    #[serde(default)]
    pub capacity_floor_abi: String,
    #[serde(default)]
    pub type_id_abi: String,
    #[serde(default)]
    pub minimum_vm_version: u8,
    #[serde(default)]
    pub riscv_isa: String,
    #[serde(default)]
    pub deployment_hash_types: Vec<String>,
    #[serde(default)]
    pub tx_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmAbiMetadata {
    pub format: String,
    pub version: u16,
    pub default: bool,
    pub embedded_in_artifact: bool,
    pub scope: String,
    pub selection: String,
}

const VM_ABI_TRAILER_MAGIC: &[u8; 8] = b"CSABITR0";
const VM_ABI_TRAILER_LEN: usize = 16;
const MOLECULE_VM_ABI_VERSION: u16 = 0x8001;

/// Strip CellScript's fixed VM ABI trailer before directly loading an ELF into CKB-VM.
pub fn strip_vm_abi_trailer(bytes: &[u8]) -> &[u8] {
    if has_vm_abi_trailer_magic(bytes) {
        &bytes[..bytes.len() - VM_ABI_TRAILER_LEN]
    } else {
        bytes
    }
}

fn has_vm_abi_trailer_magic(bytes: &[u8]) -> bool {
    if bytes.len() < VM_ABI_TRAILER_LEN {
        return false;
    }
    let trailer_start = bytes.len() - VM_ABI_TRAILER_LEN;
    &bytes[trailer_start..trailer_start + VM_ABI_TRAILER_MAGIC.len()] == VM_ABI_TRAILER_MAGIC
}

fn vm_abi_trailer_version(bytes: &[u8]) -> Result<Option<u16>> {
    if !has_vm_abi_trailer_magic(bytes) {
        return Ok(None);
    }

    let trailer_start = bytes.len() - VM_ABI_TRAILER_LEN;
    let trailer = &bytes[trailer_start..];
    let version = u16::from_le_bytes([trailer[8], trailer[9]]);
    let flags = u16::from_le_bytes([trailer[10], trailer[11]]);
    let reserved = u32::from_le_bytes([trailer[12], trailer[13], trailer[14], trailer[15]]);
    if flags != 0 || reserved != 0 {
        return Err(CompileError::without_span("invalid VM ABI trailer: flags/reserved bytes must be zero"));
    }
    Ok(Some(version))
}

fn append_vm_abi_trailer(mut artifact: Vec<u8>, abi_version: u16) -> Vec<u8> {
    if strip_vm_abi_trailer(&artifact).len() != artifact.len() {
        artifact.truncate(artifact.len() - VM_ABI_TRAILER_LEN);
    }
    artifact.extend_from_slice(VM_ABI_TRAILER_MAGIC);
    artifact.extend_from_slice(&abi_version.to_le_bytes());
    artifact.extend_from_slice(&0u16.to_le_bytes());
    artifact.extend_from_slice(&0u32.to_le_bytes());
    artifact
}

#[cfg(feature = "wasm")]
fn validate_browser_record_boundary(metadata: &CompileMetadata) -> Result<()> {
    let native_entry_records =
        metadata.actions.iter().any(|entry| !entry.ckb_runtime_accesses.is_empty() || !entry.proof_plan.is_empty())
            || metadata.functions.iter().any(|entry| !entry.ckb_runtime_accesses.is_empty() || !entry.proof_plan.is_empty())
            || metadata.locks.iter().any(|entry| !entry.ckb_runtime_accesses.is_empty() || !entry.proof_plan.is_empty());
    if native_entry_records
        || !metadata.runtime.vm_target.is_empty()
        || !metadata.runtime.ckb_runtime_accesses.is_empty()
        || !metadata.runtime.transaction_view_handles.is_empty()
        || !metadata.runtime.proof_plan.is_empty()
        || !metadata.runtime.trusted_external_verifiers.is_empty()
        || metadata.runtime.proof_plan_soundness != ProofPlanSoundnessReport::default()
        || !metadata.typed_semantics.schema.is_empty()
        || !metadata.typed_semantics.entries.is_empty()
    {
        return Err(CompileError::without_span("browser metadata cannot carry native proof or provenance records"));
    }
    Ok(())
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn ckb_blake2b256_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut state = blake2b_simd::Params::new().hash_length(32).personal(CKB_DEFAULT_HASH_PERSONALIZATION).to_state();
    for part in parts {
        state.update(part);
    }
    let digest = state.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_bytes());
    out
}

pub fn ckb_blake2b256(data: &[u8]) -> [u8; 32] {
    ckb_blake2b256_parts(&[data])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CkbScriptHashTypeValue {
    Data,
    Type,
    Data1,
    Data2,
}

impl CkbScriptHashTypeValue {
    pub fn from_byte(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Data),
            1 => Some(Self::Type),
            2 => Some(Self::Data1),
            4 => Some(Self::Data2),
            _ => None,
        }
    }

    pub fn as_byte(self) -> u8 {
        match self {
            Self::Data => 0,
            Self::Type => 1,
            Self::Data1 => 2,
            Self::Data2 => 4,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Type => "type",
            Self::Data1 => "data1",
            Self::Data2 => "data2",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CkbScriptArgsValue {
    bytes: Vec<u8>,
}

impl CkbScriptArgsValue {
    pub fn empty() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn exact(bytes: impl Into<Vec<u8>>) -> Self {
        Self { bytes: bytes.into() }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn packed_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + self.bytes.len());
        out.extend_from_slice(&(self.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.bytes);
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CkbScriptValue {
    pub code_hash: [u8; 32],
    pub hash_type: CkbScriptHashTypeValue,
    pub args: CkbScriptArgsValue,
}

impl CkbScriptValue {
    pub fn new(code_hash: [u8; 32], hash_type: CkbScriptHashTypeValue, args: CkbScriptArgsValue) -> Self {
        Self { code_hash, hash_type, args }
    }

    pub fn packed_bytes(&self) -> Vec<u8> {
        let args = self.args.packed_bytes();
        let total_size = 16 + 32 + 1 + args.len();
        let mut out = Vec::with_capacity(total_size);
        out.extend_from_slice(&(total_size as u32).to_le_bytes());
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&48u32.to_le_bytes());
        out.extend_from_slice(&49u32.to_le_bytes());
        out.extend_from_slice(&self.code_hash);
        out.push(self.hash_type.as_byte());
        out.extend_from_slice(&args);
        out
    }

    pub fn hash(&self) -> [u8; 32] {
        ckb_blake2b256(&self.packed_bytes())
    }
}

fn is_canonical_hash_hex(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod ckb_hash_tests {
    #[test]
    fn ckb_blake2b256_matches_blank_hash_vector() {
        assert_eq!(super::ckb_blake2b256(b""), super::CKB_BLANK_HASH);
        assert_eq!(super::hex_encode(&super::ckb_blake2b256(b"")), "44f4c69744d5f8c55d642062949dcae49bc4e7ef43d388c5a12f42b5633d163e");
    }
}

fn bind_artifact_metadata(metadata: &mut CompileMetadata, artifact_bytes: &[u8], artifact_hash: &[u8; 32]) {
    metadata.artifact_hash = Some(hex_encode(artifact_hash));
    metadata.artifact_size_bytes = Some(artifact_bytes.len());
}

const CKB_SHANNONS_PER_CKB: u64 = 100_000_000;
const CKB_DEFAULT_MAX_TX_VERIFY_CYCLES: u64 = 70_000_000;
const CKB_DEFAULT_MAX_BLOCK_CYCLES: u64 = 10_000_000_000;
const CKB_DEFAULT_MAX_BLOCK_BYTES: u64 = 597_000;

fn refresh_constraints_status(constraints: &mut ConstraintsMetadata) {
    constraints.status = if !constraints.failures.is_empty() {
        "fail"
    } else if !constraints.warnings.is_empty() {
        "warn"
    } else {
        "pass"
    }
    .to_string();
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|value| value.parse::<u64>().ok())
}

fn ckb_limits_source() -> String {
    let overridden = ["CELLSCRIPT_CKB_MAX_TX_VERIFY_CYCLES", "CELLSCRIPT_CKB_MAX_BLOCK_CYCLES", "CELLSCRIPT_CKB_MAX_BLOCK_BYTES"]
        .iter()
        .filter(|name| std::env::var(name).is_ok())
        .copied()
        .collect::<Vec<_>>();
    if overridden.is_empty() {
        "builtin-ckb-defaults".to_string()
    } else {
        format!("environment:{}", overridden.join(","))
    }
}

/// Extract the span from an AST statement, for debug info line table generation.
#[cfg_attr(feature = "wasm", allow(dead_code))]
fn stmt_span(stmt: &ast::Stmt) -> error::Span {
    match stmt {
        ast::Stmt::Let(let_stmt) => let_stmt.span,
        ast::Stmt::Expr(expr) | ast::Stmt::Discard(expr) => expr_span(expr),
        ast::Stmt::Return(ast::ReturnStmt { value: Some(expr), span }) => {
            let expr_span = expr_span(expr);
            if expr_span.line == 0 || expr_span.column == 0 {
                *span
            } else {
                expr_span
            }
        }
        ast::Stmt::Return(ast::ReturnStmt { value: None, span }) => *span,
        ast::Stmt::If(if_stmt) => if_stmt.span,
        ast::Stmt::For(for_stmt) => for_stmt.span,
        ast::Stmt::While(while_stmt) => while_stmt.span,
        ast::Stmt::Break(control) | ast::Stmt::Continue(control) => control.span,
        ast::Stmt::Borrow(borrow_stmt) => borrow_stmt.span,
    }
}

/// Extract span from an expression.
#[cfg_attr(feature = "wasm", allow(dead_code))]
fn expr_span(expr: &ast::Expr) -> error::Span {
    // AST expressions don't carry their own Span in the current definition,
    // so we fall back to a default span.
    let _ = expr;
    error::Span::default()
}

pub fn validate_compile_result(result: &CompileResult) -> Result<()> {
    validate_compile_metadata(&result.metadata, result.artifact_format)?;

    if result.artifact_bytes.is_empty() {
        return Err(CompileError::without_span("compiler produced an empty artifact"));
    }

    let computed_hash = ckb_blake2b256(&result.artifact_bytes);
    if computed_hash != result.artifact_hash {
        return Err(CompileError::without_span("artifact_hash does not match artifact_bytes"));
    }
    let computed_hash_hex = hex_encode(&computed_hash);
    match &result.metadata.artifact_hash {
        Some(metadata_hash) if metadata_hash == &computed_hash_hex => {}
        Some(metadata_hash) => {
            return Err(CompileError::without_span(format!(
                "metadata artifact_hash '{}' does not match artifact bytes '{}'",
                metadata_hash, computed_hash_hex
            )));
        }
        None => return Err(CompileError::without_span("metadata is missing artifact_hash")),
    }
    match result.metadata.artifact_size_bytes {
        Some(size) if size == result.artifact_bytes.len() => {}
        Some(size) => {
            return Err(CompileError::without_span(format!(
                "metadata artifact_size_bytes {} does not match artifact size {}",
                size,
                result.artifact_bytes.len()
            )));
        }
        None => return Err(CompileError::without_span("metadata is missing artifact_size_bytes")),
    }

    match result.artifact_format {
        ArtifactFormat::RiscvAssembly => {
            if vm_abi_trailer_version(&result.artifact_bytes)?.is_some() {
                return Err(CompileError::without_span("RISC-V assembly artifacts must not embed a VM ABI trailer"));
            }
            if result.metadata.verified_artifact.state != VerifiedArtifactState::NotEmittedNonElf
                || result.verified_lowering_record.is_some()
                || result.source_artifact_map.is_some()
            {
                return Err(CompileError::without_span(
                    "RISC-V assembly result must not claim or carry the ELF verified-artifact boundary",
                ));
            }
        }
        ArtifactFormat::RiscvElf => {
            if !result.artifact_bytes.starts_with(b"\x7fELF") {
                return Err(CompileError::without_span("RISC-V ELF artifact does not start with ELF magic"));
            }
            match vm_abi_trailer_version(&result.artifact_bytes)? {
                Some(trailer_version) if result.metadata.runtime.vm_abi.embedded_in_artifact => {
                    if trailer_version != result.metadata.runtime.vm_abi.version {
                        return Err(CompileError::without_span(format!(
                            "ELF VM ABI trailer version 0x{:04x} does not match metadata runtime.vm_abi.version 0x{:04x}",
                            trailer_version, result.metadata.runtime.vm_abi.version
                        )));
                    }
                    if !strip_vm_abi_trailer(&result.artifact_bytes).starts_with(b"\x7fELF") {
                        return Err(CompileError::without_span("stripped RISC-V ELF artifact does not start with ELF magic"));
                    }
                }
                Some(_) => {
                    return Err(CompileError::without_span(
                        "RISC-V ELF artifact embeds a VM ABI trailer but metadata says this profile must not embed one",
                    ));
                }
                None if result.metadata.runtime.vm_abi.embedded_in_artifact => {
                    return Err(CompileError::without_span("RISC-V ELF artifact is missing its VM ABI trailer"));
                }
                None => {}
            }
            if result.metadata.verified_artifact.state != VerifiedArtifactState::Emitted {
                return Err(CompileError::without_span("RISC-V ELF metadata does not claim emitted verified-artifact sidecars"));
            }
            let record = result
                .verified_lowering_record
                .as_ref()
                .ok_or_else(|| CompileError::without_span("RISC-V ELF result is missing its verified lowering record"))?;
            let source_map = result
                .source_artifact_map
                .as_ref()
                .ok_or_else(|| CompileError::without_span("RISC-V ELF result is missing its source artifact map"))?;
            verified_artifact::validate_boundary_values(&result.artifact_bytes, &result.metadata, record, source_map)?;
        }
    }

    Ok(())
}

pub fn validate_artifact_metadata(artifact_bytes: Vec<u8>, metadata: CompileMetadata) -> Result<ValidatedArtifact> {
    let artifact_format = ArtifactFormat::from_display_name(&metadata.artifact_format)?;
    let artifact_hash = ckb_blake2b256(&artifact_bytes);
    let result = ValidatedArtifact { artifact_bytes, artifact_format, artifact_hash, metadata };
    result.validate()?;
    Ok(result)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbRuntimeScalarProvenanceMetadata {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_inclusive: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbRuntimeSourceProvenanceMetadata {
    pub resolved_source: String,
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbRuntimeRangeProvenanceMetadata {
    pub kind: String,
    #[serde(default)]
    pub offset: CkbRuntimeScalarProvenanceMetadata,
    #[serde(default)]
    pub length: CkbRuntimeScalarProvenanceMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbRuntimeAccessProvenanceMetadata {
    pub contract: String,
    #[serde(default)]
    pub source: CkbRuntimeSourceProvenanceMetadata,
    #[serde(default)]
    pub index: CkbRuntimeScalarProvenanceMetadata,
    #[serde(default)]
    pub range: CkbRuntimeRangeProvenanceMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CkbRuntimeAccessMetadata {
    pub operation: String,
    pub syscall: String,
    pub source: String,
    /// Compatibility projection for consumers that predate structured index
    /// provenance. Dynamic accesses use zero here; `provenance.index` is
    /// authoritative in metadata schema 69 and later.
    pub index: usize,
    pub binding: String,
    #[serde(default)]
    pub provenance: CkbRuntimeAccessProvenanceMetadata,
}

fn runtime_scalar_not_applicable() -> CkbRuntimeScalarProvenanceMetadata {
    CkbRuntimeScalarProvenanceMetadata { kind: "not-applicable".to_string(), ..Default::default() }
}

fn runtime_scalar_static(value: u64, max_inclusive: Option<u64>) -> CkbRuntimeScalarProvenanceMetadata {
    CkbRuntimeScalarProvenanceMetadata { kind: "static".to_string(), value: Some(value), binding: None, max_inclusive }
}

fn runtime_scalar_dynamic(binding: impl Into<String>, max_inclusive: Option<u64>) -> CkbRuntimeScalarProvenanceMetadata {
    CkbRuntimeScalarProvenanceMetadata { kind: "dynamic".to_string(), value: None, binding: Some(binding.into()), max_inclusive }
}

fn runtime_scalar_bounded_scan(max_inclusive: u64) -> CkbRuntimeScalarProvenanceMetadata {
    CkbRuntimeScalarProvenanceMetadata {
        kind: "bounded-scan".to_string(),
        value: None,
        binding: None,
        max_inclusive: Some(max_inclusive),
    }
}

fn runtime_scalar_metadata_ordinal(value: u64) -> CkbRuntimeScalarProvenanceMetadata {
    CkbRuntimeScalarProvenanceMetadata { kind: "metadata-ordinal".to_string(), value: Some(value), binding: None, max_inclusive: None }
}

fn runtime_scalar_from_operand(operand: Option<&ir::IrOperand>, max_inclusive: Option<u64>) -> CkbRuntimeScalarProvenanceMetadata {
    match operand {
        Some(ir::IrOperand::Const(value)) => {
            ir_const_u64(value).map(|value| runtime_scalar_static(value, max_inclusive)).unwrap_or_else(runtime_scalar_not_applicable)
        }
        Some(ir::IrOperand::Var(var)) => runtime_scalar_dynamic(var.name.clone(), max_inclusive),
        None => runtime_scalar_not_applicable(),
    }
}

fn ir_const_u64(value: &ir::IrConst) -> Option<u64> {
    match value {
        ir::IrConst::U8(value) => Some(u64::from(*value)),
        ir::IrConst::U16(value) => Some(u64::from(*value)),
        ir::IrConst::U32(value) => Some(u64::from(*value)),
        ir::IrConst::U64(value) => Some(*value),
        _ => None,
    }
}

fn runtime_range_not_applicable() -> CkbRuntimeRangeProvenanceMetadata {
    CkbRuntimeRangeProvenanceMetadata {
        kind: "not-applicable".to_string(),
        offset: runtime_scalar_not_applicable(),
        length: runtime_scalar_not_applicable(),
    }
}

fn runtime_range_whole_value() -> CkbRuntimeRangeProvenanceMetadata {
    CkbRuntimeRangeProvenanceMetadata {
        kind: "whole-value".to_string(),
        offset: runtime_scalar_not_applicable(),
        length: runtime_scalar_not_applicable(),
    }
}

fn runtime_range_fixed(width: u64) -> CkbRuntimeRangeProvenanceMetadata {
    CkbRuntimeRangeProvenanceMetadata {
        kind: "fixed-width".to_string(),
        offset: runtime_scalar_not_applicable(),
        length: runtime_scalar_static(width, Some(width)),
    }
}

fn runtime_range_at(offset: Option<&ir::IrOperand>, length: CkbRuntimeScalarProvenanceMetadata) -> CkbRuntimeRangeProvenanceMetadata {
    CkbRuntimeRangeProvenanceMetadata { kind: "bounded-range".to_string(), offset: runtime_scalar_from_operand(offset, None), length }
}

fn runtime_range_bounded_value(binding: impl Into<String>, maximum: u64) -> CkbRuntimeRangeProvenanceMetadata {
    CkbRuntimeRangeProvenanceMetadata {
        kind: "bounded-range".to_string(),
        offset: runtime_scalar_static(0, Some(maximum)),
        length: runtime_scalar_dynamic(binding, Some(maximum)),
    }
}

fn runtime_access_metadata(
    operation: impl Into<String>,
    syscall: impl Into<String>,
    source: impl Into<String>,
    binding: impl Into<String>,
    resolved_source: impl Into<String>,
    source_origin: impl Into<String>,
    source_binding: Option<String>,
    index: CkbRuntimeScalarProvenanceMetadata,
    range: CkbRuntimeRangeProvenanceMetadata,
) -> CkbRuntimeAccessMetadata {
    let compatibility_index = index.value.and_then(|value| usize::try_from(value).ok()).unwrap_or_default();
    CkbRuntimeAccessMetadata {
        operation: operation.into(),
        syscall: syscall.into(),
        source: source.into(),
        index: compatibility_index,
        binding: binding.into(),
        provenance: CkbRuntimeAccessProvenanceMetadata {
            contract: CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT.to_string(),
            source: CkbRuntimeSourceProvenanceMetadata {
                resolved_source: resolved_source.into(),
                origin: source_origin.into(),
                binding: source_binding,
            },
            index,
            range,
        },
    }
}

fn static_runtime_access(
    operation: impl Into<String>,
    syscall: impl Into<String>,
    source: impl Into<String>,
    index: usize,
    binding: impl Into<String>,
) -> CkbRuntimeAccessMetadata {
    let source = source.into();
    let syscall = syscall.into();
    let range = if syscall == "LOAD_CELL" { runtime_range_whole_value() } else { runtime_range_not_applicable() };
    runtime_access_metadata(
        operation,
        syscall,
        source.clone(),
        binding,
        source,
        "implicit-lowering",
        None,
        runtime_scalar_static(index as u64, None),
        range,
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CkbScriptGroupMetadata {
    pub entry_kind: String,
    pub group_kind: String,
    pub active_script_group: String,
    pub source_encoding: String,
    pub selected_sources: Vec<String>,
    pub input_sources: Vec<String>,
    pub output_sources: Vec<String>,
    pub cell_dep_sources: Vec<String>,
    pub header_dep_sources: Vec<String>,
    pub group_scoped_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierObligationMetadata {
    pub scope: String,
    pub category: String,
    pub feature: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRuntimeInputRequirementMetadata {
    pub scope: String,
    pub feature: String,
    pub status: String,
    pub component: String,
    pub source: String,
    pub binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub abi: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolInvariantMetadata {
    pub name: String,
    pub status: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolRuntimeInputRequirementMetadata {
    pub component: String,
    pub source: String,
    pub index: usize,
    pub binding: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub abi: String,
    pub byte_len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolPrimitiveMetadata {
    pub scope: String,
    pub operation: String,
    pub feature: String,
    pub ty: String,
    pub status: String,
    pub source: String,
    pub checked_components: Vec<String>,
    pub runtime_required_components: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_input_requirements: Vec<PoolRuntimeInputRequirementMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariant_families: Vec<PoolInvariantMetadata>,
    pub source_invariant_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transition_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_hash_type: Option<String>,
    #[serde(default)]
    pub hash_type_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_floor_shannons: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity_floor_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_type_id: Option<CkbTypeIdMetadata>,
    pub kind: String,
    #[serde(default)]
    pub capability_set_version: u32,
    pub capabilities: Vec<String>,
    pub claim_output: Option<String>,
    pub flow_states: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_state_field: Option<String>,
    pub flow_transitions: Vec<FlowTransitionMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_initial_state: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow_terminal_states: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_terminal_discharge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_terminal_evidence_tier: Option<EvidenceTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_state_model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flow_audit_warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validity_predicates: Vec<ValidityPredicateMetadata>,
    pub encoded_size: Option<usize>,
    pub fields: Vec<FieldMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub molecule_schema: Option<MoleculeSchemaMetadata>,
    #[serde(default, skip_serializing_if = "is_default_identity_policy")]
    pub identity_policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidityPredicateMetadata {
    pub expression: String,
    pub evidence_tier: EvidenceTier,
    pub dependencies: Vec<String>,
    pub runtime_checked_on_create: bool,
    pub create_paths_selected: usize,
    pub create_paths_checked: usize,
    pub update_paths_selected: usize,
    pub create_path_status: String,
    pub update_path_status: String,
    pub source_span: ProofPlanSourceSpanMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CkbTypeIdMetadata {
    pub abi: String,
    pub script_code_hash: String,
    pub hash_type: String,
    pub args_source: String,
    pub group_rule: String,
    pub builder: String,
    pub verifier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoleculeSchemaMetadata {
    pub abi: String,
    pub layout: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dynamic_fields: Vec<String>,
    pub fixed_size: usize,
    pub schema_hash: String,
    pub schema: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowTransitionMetadata {
    pub from: String,
    pub to: String,
    pub from_index: usize,
    pub to_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMetadata {
    pub name: String,
    pub ty: String,
    pub offset: usize,
    pub encoded_size: Option<usize>,
    pub fixed_width: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionMetadata {
    pub name: String,
    pub params: Vec<ParamMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub effects: ir::EffectFacts,
    pub parallelizable: bool,
    pub touches_shared: Vec<String>,
    pub estimated_cycles: u64,
    pub scheduler_witness_abi: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scheduler_witness_hex: String,
    pub consume_set: Vec<CellPatternMetadata>,
    pub read_refs: Vec<CellPatternMetadata>,
    pub create_set: Vec<CreatePatternMetadata>,
    pub mutate_set: Vec<MutatePatternMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_transition_edges: Vec<StateTransitionEdgeMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protocol_role_candidates: Vec<ProtocolRoleCandidateMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_primitives: Vec<PoolPrimitiveMetadata>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_script_group: Option<CkbScriptGroupMetadata>,
    pub ckb_runtime_features: Vec<String>,
    pub fail_closed_runtime_features: Vec<String>,
    pub verifier_obligations: Vec<VerifierObligationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_plan: Vec<ProofPlanMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transaction_runtime_input_requirements: Vec<TransactionRuntimeInputRequirementMetadata>,
    pub elf_compatible: bool,
    pub standalone_runner_compatible: bool,
    pub block_count: usize,
}

impl ActionMetadata {
    /// Output indexes where a CKB builder should install the built-in TYPE_ID
    /// script for newly-created cells.
    pub fn ckb_type_id_output_indexes(&self) -> Vec<usize> {
        ckb_type_id_output_indexes_from_create_set(&self.create_set)
    }

    /// Decode the public compiled CellScript scheduler witness bytes for this action.
    ///
    /// Transaction builders can pass the returned bytes to
    /// `CellTx::push_cellscript_compiled_scheduler_witness` and use the returned
    /// access summary as the trusted scheduler policy input. Public scheduler
    /// witness bytes are Molecule-only.
    pub fn scheduler_witness_bytes(&self) -> Result<Vec<u8>> {
        if self.scheduler_witness_abi != SCHEDULER_WITNESS_ABI_MOLECULE {
            return Err(CompileError::without_span(format!("unsupported scheduler witness ABI '{}'", self.scheduler_witness_abi)));
        }
        if self.scheduler_witness_hex.is_empty() {
            return Err(CompileError::without_span("scheduler witness metadata is missing"));
        }
        let bytes = decode_scheduler_witness_hex(&self.scheduler_witness_hex)?;
        crate::stdlib::SchedulerMetadata::validate_current(&bytes, &self.effects)?;
        Ok(bytes)
    }

    /// Encode positional entry witness bytes for the generated `_cellscript_entry` wrapper.
    ///
    /// Cell-bound parameters are loaded from transaction cells by the wrapper and
    /// are intentionally omitted from `args`; remaining scalar, fixed-byte, and
    /// dynamic schema parameters are encoded in source order after the `CSARGv1\0` header.
    pub fn entry_witness_args(&self, args: &[EntryWitnessArg]) -> Result<Vec<u8>> {
        encode_entry_witness_args_for_params_with_runtime_bound(&self.params, args, &runtime_bound_param_names(&self.params))
    }
}

const SCHEDULER_WITNESS_ABI_MOLECULE: &str = "cellscript-scheduler-facts-molecule-v2";

impl LockMetadata {
    /// Output indexes where a CKB builder should install the built-in TYPE_ID
    /// script for newly-created cells.
    pub fn ckb_type_id_output_indexes(&self) -> Vec<usize> {
        ckb_type_id_output_indexes_from_create_set(&self.create_set)
    }

    /// Encode positional entry witness bytes for the generated `_cellscript_entry` wrapper.
    pub fn entry_witness_args(&self, args: &[EntryWitnessArg]) -> Result<Vec<u8>> {
        encode_entry_witness_args_for_params_with_runtime_bound(&self.params, args, &runtime_bound_param_names(&self.params))
    }
}

fn ckb_type_id_output_indexes_from_create_set(create_set: &[CreatePatternMetadata]) -> Vec<usize> {
    create_set.iter().filter_map(|pattern| pattern.ckb_type_id.as_ref().map(|plan| plan.output_index)).collect()
}

fn state_transition_edge_metadata(edge: &ir::IrStateTransitionEdge) -> StateTransitionEdgeMetadata {
    StateTransitionEdgeMetadata {
        input_binding: edge.input_binding.clone(),
        output_binding: edge.output_binding.clone(),
        type_name: edge.type_name.clone(),
        field_name: edge.field_name.clone(),
        from: edge.from.clone(),
        to: edge.to.clone(),
        from_index: edge.from_index,
        to_index: edge.to_index,
    }
}

fn protocol_role_candidate_metadata(candidate: &ir::IrProtocolRoleCandidate) -> ProtocolRoleCandidateMetadata {
    ProtocolRoleCandidateMetadata {
        role: candidate.role.clone(),
        source: candidate.source.clone(),
        strength: candidate.strength.clone(),
        actor_binding: candidate.actor_binding.clone(),
        actor_source: candidate.actor_source.clone(),
        cell_binding: candidate.cell_binding.clone(),
        field_name: candidate.field_name.clone(),
        predicate: candidate.predicate.clone(),
        source_span: candidate.span.map(|span| ProofPlanSourceSpanMetadata {
            start: span.start,
            end: span.end,
            line: span.line,
            column: span.column,
        }),
        evidence_tier: EvidenceTier::MetadataOnly,
        authorization_proven: false,
    }
}

fn default_metadata_only_evidence_tier() -> EvidenceTier {
    EvidenceTier::MetadataOnly
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionMetadata {
    pub name: String,
    pub params: Vec<ParamMetadata>,
    pub return_type: Option<String>,
    pub effects: ir::EffectFacts,
    pub effect_evidence_tier: EvidenceTier,
    pub mutate_set: Vec<MutatePatternMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_primitives: Vec<PoolPrimitiveMetadata>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    pub ckb_runtime_features: Vec<String>,
    pub fail_closed_runtime_features: Vec<String>,
    pub verifier_obligations: Vec<VerifierObligationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_plan: Vec<ProofPlanMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transaction_runtime_input_requirements: Vec<TransactionRuntimeInputRequirementMetadata>,
    pub elf_compatible: bool,
    pub block_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockMetadata {
    pub name: String,
    pub effects: ir::EffectFacts,
    pub params: Vec<ParamMetadata>,
    pub consume_set: Vec<CellPatternMetadata>,
    pub read_refs: Vec<CellPatternMetadata>,
    pub create_set: Vec<CreatePatternMetadata>,
    pub mutate_set: Vec<MutatePatternMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pool_primitives: Vec<PoolPrimitiveMetadata>,
    pub ckb_runtime_accesses: Vec<CkbRuntimeAccessMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_script_group: Option<CkbScriptGroupMetadata>,
    pub ckb_runtime_features: Vec<String>,
    pub fail_closed_runtime_features: Vec<String>,
    pub verifier_obligations: Vec<VerifierObligationMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_plan: Vec<ProofPlanMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transaction_runtime_input_requirements: Vec<TransactionRuntimeInputRequirementMetadata>,
    pub elf_compatible: bool,
    pub standalone_runner_compatible: bool,
    pub block_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamMetadata {
    pub name: String,
    pub ty: String,
    pub is_mut: bool,
    pub is_ref: bool,
    #[serde(default = "default_param_source", skip_serializing_if = "is_default_param_source")]
    pub source: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub protected_spend_surface: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub witness_data_source: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub lock_args_data_source: bool,
    #[serde(default)]
    pub cell_bound_abi: bool,
    pub schema_pointer_abi: bool,
    pub schema_length_abi: bool,
    pub fixed_byte_pointer_abi: bool,
    pub fixed_byte_length_abi: bool,
    pub fixed_byte_len: Option<usize>,
    pub type_hash_pointer_abi: bool,
    pub type_hash_length_abi: bool,
    pub type_hash_len: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounded_runtime_contract: Option<String>,
}

fn default_param_source() -> String {
    "default".to_string()
}

fn is_default_param_source(source: &String) -> bool {
    source == "default"
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellPatternMetadata {
    pub operation: String,
    pub type_hash: Option<String>,
    pub binding: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePatternMetadata {
    pub operation: String,
    pub ty: String,
    pub binding: String,
    pub fields: Vec<String>,
    pub has_lock: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_output_data: Option<CkbOutputDataBindingMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ckb_type_id: Option<CkbTypeIdOutputMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CkbOutputDataBindingMetadata {
    pub abi: String,
    pub output_source: String,
    pub output_index: usize,
    pub output_data_source: String,
    pub output_data_index: usize,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CkbTypeIdOutputMetadata {
    pub abi: String,
    pub type_id: String,
    pub output_source: String,
    pub output_index: usize,
    pub script_code_hash: String,
    pub hash_type: String,
    pub args_source: String,
    pub builder: String,
    pub generator_setting: String,
    pub wasm_setting: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutatePatternMetadata {
    pub operation: String,
    pub ty: String,
    pub binding: String,
    pub fields: Vec<String>,
    pub preserved_fields: Vec<String>,
    pub input_source: String,
    pub input_index: usize,
    pub output_source: String,
    pub output_index: usize,
    pub preserve_type_hash: bool,
    pub preserve_lock_hash: bool,
    pub type_hash_preservation_status: String,
    pub lock_hash_preservation_status: String,
    pub field_equality_status: String,
    pub field_transition_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateTransitionEdgeMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_binding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_binding: Option<String>,
    pub type_name: String,
    pub field_name: String,
    pub from: String,
    pub to: String,
    pub from_index: usize,
    pub to_index: usize,
}

#[derive(Debug, Clone)]
struct MetadataFieldLayout {
    ty: ir::IrType,
    offset: usize,
    fixed_size: Option<usize>,
    fixed_enum_size: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct MetadataTypeLayouts {
    fields: HashMap<String, HashMap<String, MetadataFieldLayout>>,
    enum_fixed_sizes: HashMap<String, usize>,
    fixed_struct_returns: HashMap<String, ir::IrType>,
    nominal_identities: BTreeMap<String, Option<crate::resolve::ResolvedTypeIdentity>>,
    enum_payload_storage: HashMap<(String, String, usize), bool>,
}

impl MetadataTypeLayouts {
    fn types_equal(&self, left: &ir::IrType, right: &ir::IrType) -> bool {
        ir::resolved_types_equal(left, right, &self.nominal_identities)
    }
}

impl std::ops::Deref for MetadataTypeLayouts {
    type Target = HashMap<String, HashMap<String, MetadataFieldLayout>>;

    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}

impl std::ops::DerefMut for MetadataTypeLayouts {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.fields
    }
}

impl CompileResult {
    /// Validate that artifact bytes, hash, format, and metadata agree.
    pub fn validate(&self) -> Result<()> {
        validate_compile_result(self)
    }

    pub(crate) fn refresh_verified_artifact_boundary(&mut self) -> Result<()> {
        if self.artifact_format != ArtifactFormat::RiscvElf {
            self.metadata.verified_artifact = VerifiedArtifactMetadata::default();
            self.verified_lowering_record = None;
            self.source_artifact_map = None;
            return Ok(());
        }
        let draft = self.verified_artifact_draft.as_ref().ok_or_else(|| {
            CompileError::without_span("RISC-V ELF result is missing the verified-artifact draft")
                .with_code(crate::error::DiagnosticCode::E2400)
        })?;
        let (record, source_map, boundary) =
            verified_artifact::build_verified_artifact_boundary(&self.artifact_bytes, &self.metadata, draft)?;
        self.metadata.verified_artifact = boundary;
        self.verified_lowering_record = Some(record);
        self.source_artifact_map = Some(source_map);
        Ok(())
    }

    /// Default output path
    pub fn default_output_path(&self, input_path: &Utf8Path) -> Utf8PathBuf {
        input_path.with_extension(self.artifact_format.file_extension())
    }

    /// Write artifact to file
    pub fn write_to_path(&self, output_path: &Utf8Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CompileError::new(format!("failed to create output directory '{}': {}", parent, e), error::Span::default())
                    .with_category(error::CompileErrorCategory::Io)
                    .with_file(output_path.to_owned())
                    .with_source(e)
            })?;
        }

        std::fs::write(output_path, &self.artifact_bytes).map_err(|e| {
            CompileError::new(format!("failed to write output '{}': {}", output_path, e), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(output_path.to_owned())
                .with_source(e)
        })
    }

    pub fn default_metadata_path(&self, artifact_path: &Utf8Path) -> Utf8PathBuf {
        metadata_output_path_from_artifact(artifact_path)
    }

    pub fn default_lowering_record_path(&self, artifact_path: &Utf8Path) -> Utf8PathBuf {
        lowering_record_output_path_from_artifact(artifact_path)
    }

    pub fn default_source_map_path(&self, artifact_path: &Utf8Path) -> Utf8PathBuf {
        source_map_output_path_from_artifact(artifact_path)
    }

    pub fn write_verified_artifact_sidecars(&self, artifact_path: &Utf8Path) -> Result<Option<(Utf8PathBuf, Utf8PathBuf)>> {
        if self.artifact_format != ArtifactFormat::RiscvElf {
            return Ok(None);
        }
        self.validate()?;
        let record = self.verified_lowering_record.as_ref().ok_or_else(|| {
            CompileError::without_span("ELF result is missing verified lowering record").with_code(crate::error::DiagnosticCode::E2400)
        })?;
        let source_map = self.source_artifact_map.as_ref().ok_or_else(|| {
            CompileError::without_span("ELF result is missing source artifact map").with_code(crate::error::DiagnosticCode::E2400)
        })?;
        let record_path = self.default_lowering_record_path(artifact_path);
        let source_map_path = self.default_source_map_path(artifact_path);
        if let Some(parent) = record_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CompileError::new(
                    format!("failed to create verified artifact sidecar directory '{}': {}", parent, error),
                    error::Span::default(),
                )
                .with_category(error::CompileErrorCategory::Io)
                .with_source(error)
            })?;
        }
        let record_bytes = cellscript_artifact_checker::canonical_bytes(record)
            .map_err(|error| CompileError::without_span(error.to_string()).with_code(crate::error::DiagnosticCode::E2400))?;
        let source_map_bytes = cellscript_artifact_checker::canonical_bytes(source_map)
            .map_err(|error| CompileError::without_span(error.to_string()).with_code(crate::error::DiagnosticCode::E2400))?;
        std::fs::write(&record_path, record_bytes).map_err(|error| {
            CompileError::new(format!("failed to write lowering record '{}': {}", record_path, error), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(record_path.clone())
                .with_source(error)
        })?;
        std::fs::write(&source_map_path, source_map_bytes).map_err(|error| {
            CompileError::new(format!("failed to write source map '{}': {}", source_map_path, error), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(source_map_path.clone())
                .with_source(error)
        })?;
        Ok(Some((record_path, source_map_path)))
    }

    pub fn write_metadata_to_path(&self, output_path: &Utf8Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                CompileError::new(format!("failed to create metadata directory '{}': {}", parent, e), error::Span::default())
                    .with_category(error::CompileErrorCategory::Io)
                    .with_file(output_path.to_owned())
                    .with_source(e)
            })?;
        }

        let json = serde_json::to_vec_pretty(&self.metadata).map_err(|e| {
            CompileError::new(format!("failed to serialize metadata: {}", e), error::Span::default())
                .with_category(error::CompileErrorCategory::Internal)
                .with_source(e)
        })?;
        std::fs::write(output_path, json).map_err(|e| {
            CompileError::new(format!("failed to write metadata '{}': {}", output_path, e), error::Span::default())
                .with_category(error::CompileErrorCategory::Io)
                .with_file(output_path.to_owned())
                .with_source(e)
        })
    }
}

impl ValidatedArtifact {
    /// Validate that artifact bytes, hash, format, and metadata agree.
    pub fn validate(&self) -> Result<()> {
        validate_compile_metadata(&self.metadata, self.artifact_format)?;

        if self.artifact_bytes.is_empty() {
            return Err(CompileError::without_span("artifact bytes are empty"));
        }

        let computed_hash = ckb_blake2b256(&self.artifact_bytes);
        if computed_hash != self.artifact_hash {
            return Err(CompileError::without_span("artifact_hash does not match artifact_bytes"));
        }
        let computed_hash_hex = hex_encode(&computed_hash);
        match &self.metadata.artifact_hash {
            Some(metadata_hash) if metadata_hash == &computed_hash_hex => {}
            Some(metadata_hash) => {
                return Err(CompileError::without_span(format!(
                    "metadata artifact_hash '{}' does not match artifact bytes '{}'",
                    metadata_hash, computed_hash_hex
                )));
            }
            None => {
                return Err(CompileError::without_span("metadata is missing artifact_hash"));
            }
        }

        Ok(())
    }
}

/// Parse compile input to specific CellScript source files
pub fn resolve_input_path<P: AsRef<Utf8Path>>(input: P) -> Result<Utf8PathBuf> {
    resolve_input_file(input.as_ref())
}

/// Derive default output path from original input
pub fn default_output_path_for_input<P: AsRef<Utf8Path>>(
    input: P,
    resolved_input: &Utf8Path,
    artifact_format: ArtifactFormat,
) -> Result<Utf8PathBuf> {
    default_output_path_from_input(input.as_ref(), resolved_input, artifact_format)
}

pub fn default_metadata_path_for_artifact<P: AsRef<Utf8Path>>(artifact_path: P) -> Utf8PathBuf {
    metadata_output_path_from_artifact(artifact_path.as_ref())
}

/// Compile CellScript source code
pub fn compile(source: &str, options: CompileOptions) -> Result<CompileResult> {
    compile_with_executable_surface_policy(source, options, ExecutableSurfacePolicy::AllowFailClosed)
}

/// Compile CellScript source under an explicit executable-surface policy.
pub fn compile_with_executable_surface_policy(
    source: &str,
    options: CompileOptions,
    policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    let ast = generics::monomorphize_for_edition(&frontend::parse(source, options.edition)?, options.edition)?;

    let mut result = compile_ast_with_build(&ast, &options, None, None, None, None, policy)?;
    bind_compile_result_source_metadata(&mut result, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())])?;
    result.validate()?;
    Ok(result)
}

/// Compile source against the same named ZK deployment declarations used by Cell.toml.
/// This checks package intent before code generation; deployment liveness is external.
#[cfg(not(feature = "wasm"))]
pub fn compile_with_zk_deploy(source: &str, options: CompileOptions, deploy: &package::CkbDeployConfig) -> Result<CompileResult> {
    let ast = generics::monomorphize_for_edition(&frontend::parse(source, options.edition)?, options.edition)?;
    let mut result = compile_ast_with_build(&ast, &options, None, None, Some(deploy), None, ExecutableSurfacePolicy::AllowFailClosed)?;
    bind_compile_result_source_metadata(&mut result, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())])?;
    result.validate()?;
    Ok(result)
}

/// Compile the unique structurally eligible fungible Type Script invariant
/// from in-memory source as the payload-free `fungible-type-group-v1` entry.
pub fn compile_fungible_type_group_entry(source: &str, options: CompileOptions) -> Result<CompileResult> {
    let ast = generics::monomorphize_for_edition(&frontend::parse(source, options.edition)?, options.edition)?;
    let mut result = compile_ast_with_build(
        &ast,
        &options,
        None,
        None,
        None,
        Some(&CompileEntryScope::FungibleTypeGroupV1),
        ExecutableSurfacePolicy::AllowFailClosed,
    )?;
    bind_compile_result_source_metadata(&mut result, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())])?;
    result.validate()?;
    Ok(result)
}

/// Compile one named structurally eligible fungible type from an in-memory
/// multi-asset source as the payload-free `fungible-type-group-v1` entry.
pub fn compile_fungible_type_group_entry_for(
    source: &str,
    options: CompileOptions,
    type_name: impl Into<String>,
) -> Result<CompileResult> {
    let ast = generics::monomorphize_for_edition(&frontend::parse(source, options.edition)?, options.edition)?;
    let scope = CompileEntryScope::FungibleTypeGroupV1For(type_name.into());
    let mut result = compile_ast_with_build(&ast, &options, None, None, None, Some(&scope), ExecutableSurfacePolicy::AllowFailClosed)?;
    bind_compile_result_source_metadata(&mut result, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())])?;
    result.validate()?;
    Ok(result)
}

/// Only generate compile metadata, without asm/elf artifact.
pub fn compile_metadata(source: &str, edition: CellScriptEdition, target: Option<String>) -> Result<CompileMetadata> {
    let ast = generics::monomorphize_for_edition(&frontend::parse(source, edition)?, edition)?;
    let artifact_format = ArtifactFormat::from_target(target.as_deref().unwrap_or(DEFAULT_TARGET))?;
    let target_profile = TargetProfile::Ckb;
    types::check(&ast)?;
    flow::check(&ast)?;
    let ir = ir::generate(&ast)?;
    executable_surface::validate_ir_module(&ir)?;
    let mut metadata = compile_metadata_from_ir(&ir, artifact_format, target_profile, edition, None);
    bind_public_interface(&mut metadata, &ast, None)?;
    apply_trusted_external_verifiers(&mut metadata, &ir, &[])?;
    bind_typed_semantics(&mut metadata, &ir, &ast, None, false)?;
    bind_source_metadata(&mut metadata, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())]);
    validate_compile_metadata(&metadata, artifact_format)?;
    Ok(metadata)
}

#[derive(Debug, Clone)]
pub struct CompileMetadataDiagnosticReport {
    pub metadata: Option<CompileMetadata>,
    pub diagnostics: Vec<CompileError>,
}

impl CompileMetadataDiagnosticReport {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InMemorySource {
    pub path: String,
    pub source: String,
    #[serde(default)]
    pub role: Option<String>,
}

/// Generate compile metadata while collecting all recoverable diagnostics.
/// Lexer failures remain fatal single diagnostics. Parser recovery collects
/// independent item and statement errors, then semantic phases collect type,
/// flow, and IR diagnostics when parsing succeeds.
pub fn compile_metadata_with_diagnostics(
    source: &str,
    edition: CellScriptEdition,
    target: Option<String>,
) -> CompileMetadataDiagnosticReport {
    let ast = match frontend::parse_diagnostics(source, edition) {
        Ok(ast) => ast,
        Err(diagnostics) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics },
    };
    let ast = match generics::monomorphize_for_edition(&ast, edition) {
        Ok(ast) => ast,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    let artifact_format = match ArtifactFormat::from_target(target.as_deref().unwrap_or(DEFAULT_TARGET)) {
        Ok(format) => format,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    let target_profile = TargetProfile::Ckb;

    let mut diagnostics = visibility_migration_diagnostics(&ast);
    diagnostics.extend(frontend::legacy_temporal_migration_diagnostics(source));
    diagnostics.extend(types::diagnostics(&ast));
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error) {
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    diagnostics.extend(flow::diagnostics(&ast));
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error) {
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    let ir = match ir::generate_diagnostics(&ast) {
        Ok(ir) => ir,
        Err(errors) => {
            diagnostics.extend(errors);
            return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
        }
    };
    if let Err(error) = executable_surface::validate_ir_module(&ir) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    let mut metadata = compile_metadata_from_ir(&ir, artifact_format, target_profile, edition, None);
    if let Err(error) = bind_public_interface(&mut metadata, &ast, None) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Err(error) = apply_trusted_external_verifiers(&mut metadata, &ir, &[]) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Err(error) = bind_typed_semantics(&mut metadata, &ir, &ast, None, false) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    bind_source_metadata(&mut metadata, vec![source_unit_from_bytes("<memory>", "memory", source.as_bytes())]);
    if let Err(error) = validate_compile_metadata(&metadata, artifact_format) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    CompileMetadataDiagnosticReport { metadata: Some(metadata), diagnostics }
}

pub fn compile_sources_metadata_with_diagnostics(
    sources: &[InMemorySource],
    entry_path: &str,
    edition: CellScriptEdition,
    target: Option<String>,
) -> CompileMetadataDiagnosticReport {
    let project = match load_virtual_project_for_entry_diagnostics(sources, entry_path, edition) {
        Ok(project) => project,
        Err(diagnostics) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics },
    };
    let options = CompileOptions { edition, target, ..CompileOptions::default() };
    let artifact_format = match ArtifactFormat::from_target(resolve_target(&options, None)) {
        Ok(format) => format,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    let target_profile = TargetProfile::Ckb;

    let mut diagnostics = project_frontend_diagnostics(&project, &options, true);
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error) {
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    let entry = project.entry();
    let ir = match ir::generate_with_resolver_diagnostics(&entry.ast, &project.resolver, &entry.ast.name) {
        Ok(ir) => ir,
        Err(errors) => {
            diagnostics.extend(attach_default_file(errors, &entry.path));
            return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
        }
    };
    if let Err(error) = executable_surface::validate_ir_module(&ir) {
        diagnostics.push(attach_file_if_missing(error, &entry.path));
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    let mut metadata = compile_metadata_from_ir(&ir, artifact_format, target_profile, edition, None);
    if let Err(error) = bind_public_interface(&mut metadata, &entry.ast, Some(&project.resolver)) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Err(error) = apply_trusted_external_verifiers(&mut metadata, &ir, &[]) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    if let Err(error) = bind_typed_semantics(&mut metadata, &ir, &entry.ast, Some(&project.resolver), options.source_contracts) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }
    let source_units = sources
        .iter()
        .map(|source| {
            source_unit_from_bytes(
                source.path.clone(),
                source
                    .role
                    .clone()
                    .unwrap_or_else(|| if source.path == entry_path { "entry".to_string() } else { "memory".to_string() }),
                source.source.as_bytes(),
            )
        })
        .collect();
    bind_source_metadata(&mut metadata, source_units);
    if let Err(error) = validate_compile_metadata(&metadata, artifact_format) {
        diagnostics.push(error);
        return CompileMetadataDiagnosticReport { metadata: None, diagnostics };
    }

    CompileMetadataDiagnosticReport { metadata: Some(metadata), diagnostics }
}

/// Generate compile metadata for a file/package path while collecting semantic
/// diagnostics. This mirrors `compile_path` input resolution and module
/// resolution, but skips artifact codegen so CLI tools can report multiple
/// independent frontend diagnostics before the builder/deployment pipeline.
pub fn compile_path_metadata_with_diagnostics<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
) -> CompileMetadataDiagnosticReport {
    let resolved = match resolve_input_path(path.as_ref()) {
        Ok(path) => path,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    compile_pipeline::diagnostics::compile_file_metadata_with_diagnostics(&resolved, options, &BTreeMap::new())
}

/// Analyze a file with unsaved sources overlaid before parsing and resolution.
/// Keys are canonical source paths in the same locked project graph.
pub fn compile_path_metadata_with_diagnostics_for_sources<P: AsRef<Utf8Path>>(
    path: P,
    sources: &BTreeMap<Utf8PathBuf, String>,
    options: CompileOptions,
) -> CompileMetadataDiagnosticReport {
    let resolved = match resolve_input_path(path.as_ref()) {
        Ok(path) => path,
        Err(error) => return CompileMetadataDiagnosticReport { metadata: None, diagnostics: vec![error] },
    };
    compile_pipeline::diagnostics::compile_file_metadata_with_diagnostics(&resolved, options, sources)
}

fn attach_default_file(diagnostics: Vec<CompileError>, path: &Utf8Path) -> Vec<CompileError> {
    diagnostics.into_iter().map(|error| attach_file_if_missing(error, path)).collect()
}

fn attach_file_if_missing(error: CompileError, path: &Utf8Path) -> CompileError {
    if error.file.is_none() {
        error.with_file(path.to_owned())
    } else {
        error
    }
}

fn project_frontend_diagnostics(project: &LoadedProject, options: &CompileOptions, check_all_modules: bool) -> Vec<CompileError> {
    let mut diagnostics = Vec::new();
    for (index, module) in project.modules.iter().enumerate() {
        if !check_all_modules && index != project.entry_index {
            continue;
        }
        let module_error_start = diagnostics.len();

        if options.is_primitive_strict_015()
            && let Err(error) = check_primitive_strict_015(&module.ast)
        {
            diagnostics.push(attach_file_if_missing(error, &module.path));
        }

        diagnostics.extend(
            visibility_migration_diagnostics(&module.ast).into_iter().map(|warning| attach_file_if_missing(warning, &module.path)),
        );
        diagnostics.extend(
            frontend::legacy_temporal_migration_diagnostics(&module.source)
                .into_iter()
                .map(|warning| attach_file_if_missing(warning, &module.path)),
        );

        diagnostics.extend(attach_default_file(
            types::diagnostics_with_resolver(&module.ast, &project.resolver, &module.ast.name),
            &module.path,
        ));
        diagnostics.extend(attach_default_file(flow::diagnostics(&module.ast), &module.path));

        let module_has_errors =
            diagnostics[module_error_start..].iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
        if !module_has_errors
            && let Err(errors) = ir::generate_with_resolver_diagnostics(&module.ast, &project.resolver, &module.ast.name)
        {
            diagnostics.extend(attach_default_file(errors, &module.path));
        }
    }
    diagnostics
}

fn diagnostics_to_compile_error(mut diagnostics: Vec<CompileError>) -> CompileError {
    match diagnostics.len() {
        0 => CompileError::without_span("compilation failed"),
        1 => diagnostics.remove(0),
        len => CompileError::without_span(format!("aborting due to {} diagnostics", len)).with_related(diagnostics),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileEntryScope {
    Action(String),
    Lock(String),
    Artifact(artifact::ArtifactDeclaration),
    FungibleTypeGroupV1,
    FungibleTypeGroupV1For(String),
}

/// Controls whether artifact generation may retain compiler-recognized
/// fail-closed lowering placeholders.
///
/// Metadata-only compilation is intentionally unaffected: the Playground and
/// audit tools still need to describe reserved constructs. Production-facing
/// callers should use `DenyFailClosed` so the compiler rejects the source
/// before ASM or ELF bytes are generated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutableSurfacePolicy {
    #[default]
    AllowFailClosed,
    DenyFailClosed,
}

/// Validate the executable lowering boundary represented by compile metadata.
///
/// This function is public for tooling that consumes metadata directly. The
/// production compile path also invokes the same validation before codegen.
pub fn validate_executable_surface(metadata: &CompileMetadata, policy: ExecutableSurfacePolicy) -> Result<()> {
    executable_surface::validate_fail_closed_features(
        metadata
            .runtime
            .fail_closed_runtime_features
            .iter()
            .map(String::as_str)
            .chain(metadata.actions.iter().flat_map(|item| item.fail_closed_runtime_features.iter().map(String::as_str)))
            .chain(metadata.functions.iter().flat_map(|item| item.fail_closed_runtime_features.iter().map(String::as_str)))
            .chain(metadata.locks.iter().flat_map(|item| item.fail_closed_runtime_features.iter().map(String::as_str))),
    )?;
    if policy == ExecutableSurfacePolicy::AllowFailClosed {
        return Ok(());
    }

    let consensus_gaps =
        metadata.runtime.proof_plan.iter().filter(|plan| proof_plan_has_blocking_consensus_gap(plan)).collect::<Vec<_>>();
    if metadata.runtime.fail_closed_runtime_features.is_empty() && consensus_gaps.is_empty() {
        return Ok(());
    }

    let scopes = metadata
        .actions
        .iter()
        .filter(|action| !action.fail_closed_runtime_features.is_empty())
        .map(|action| format!("action {}: {}", action.name, action.fail_closed_runtime_features.join(", ")))
        .chain(
            metadata
                .functions
                .iter()
                .filter(|function| !function.fail_closed_runtime_features.is_empty())
                .map(|function| format!("fn {}: {}", function.name, function.fail_closed_runtime_features.join(", "))),
        )
        .chain(
            metadata
                .locks
                .iter()
                .filter(|lock| !lock.fail_closed_runtime_features.is_empty())
                .map(|lock| format!("lock {}: {}", lock.name, lock.fail_closed_runtime_features.join(", "))),
        )
        .collect::<Vec<_>>();
    let scope_detail = if scopes.is_empty() { metadata.runtime.fail_closed_runtime_features.join(", ") } else { scopes.join("; ") };

    let remediation = "use metadata-only analysis, or replace the operation with explicitly lowered fixed-arity checks until its runtime selection, codec, and correspondence semantics are implemented";
    let message = if let Some(gap) = consensus_gaps.first() {
        let fail_closed_context = if metadata.runtime.fail_closed_runtime_features.is_empty() {
            String::new()
        } else {
            format!("; fail-closed executable surface: {scope_detail}")
        };
        format!(
            "production artifact generation stopped before codegen: unresolved consensus ProofPlan enforcement for operation '{}', origin '{}', missing enforcement '{}'{}. Remediation: {}",
            gap.feature, gap.origin, gap.codegen_coverage_status, fail_closed_context, remediation
        )
    } else {
        format!(
            "executable surface is incomplete under deny-fail-closed; production artifact generation stopped before codegen ({scope_detail})"
        )
    };
    let consensus_gap_details = consensus_gaps
        .iter()
        .map(|gap| {
            serde_json::json!({
                "operation": gap.feature,
                "origin": gap.origin,
                "category": gap.category,
                "missing_enforcement": gap.codegen_coverage_status,
                "evidence_tier": gap.evidence_tier.as_str(),
                "remediation": remediation,
            })
        })
        .collect::<Vec<_>>();

    Err(CompileError::without_span(message).with_code(crate::error::DiagnosticCode::E2105).with_details(serde_json::json!({
        "policy": "deny-fail-closed",
        "phase": "pre-codegen",
        "module": metadata.module,
        "features": metadata.runtime.fail_closed_runtime_features,
        "scopes": scopes,
        "consensus_gaps": consensus_gap_details,
        "remediation": remediation,
    })))
}

/// A ProofPlan gap blocks production when it describes a consensus-relevant
/// source promise that the selected artifact does not enforce. Compile-time
/// borrow markers are deliberately not runtime promises. An unused validity
/// predicate is likewise not selected until a create or update path exists.
pub(crate) fn proof_plan_has_blocking_consensus_gap(plan: &ProofPlanMetadata) -> bool {
    if !plan.codegen_coverage_status.starts_with("gap:") || plan.category == "borrow-region" {
        return false;
    }
    if plan.category == "type-validity"
        && plan.coverage.iter().any(|item| item == "create_paths_selected:0")
        && !plan.coverage.iter().any(|item| item.starts_with("update_paths_selected:") && item != "update_paths_selected:0")
    {
        return false;
    }
    true
}

/// Compile from file, package directory, or Cell.toml
pub fn compile_path<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file(&resolved, options)
}

/// Compile from file
pub fn compile_file<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    compile_file_with_entry_scope(path, options, None, ExecutableSurfacePolicy::AllowFailClosed)
}

pub fn compile_file_with_entry_action<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    action: impl Into<String>,
) -> Result<CompileResult> {
    compile_file_with_entry_scope(
        path,
        options,
        Some(CompileEntryScope::Action(action.into())),
        ExecutableSurfacePolicy::AllowFailClosed,
    )
}

pub fn compile_file_with_entry_lock<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    lock: impl Into<String>,
) -> Result<CompileResult> {
    compile_file_with_entry_scope(path, options, Some(CompileEntryScope::Lock(lock.into())), ExecutableSurfacePolicy::AllowFailClosed)
}

/// Compile the unique structurally eligible fungible Type Script invariant as
/// the payload-free `fungible-type-group-v1` verifier entry.
pub fn compile_file_with_fungible_type_group_entry<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    compile_file_with_entry_scope(
        path,
        options,
        Some(CompileEntryScope::FungibleTypeGroupV1),
        ExecutableSurfacePolicy::AllowFailClosed,
    )
}

/// Compile one named structurally eligible fungible type from a file or
/// package containing multiple Fiber-compatible assets.
pub fn compile_file_with_fungible_type_group_entry_for<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    type_name: impl Into<String>,
) -> Result<CompileResult> {
    compile_file_with_entry_scope(
        path,
        options,
        Some(CompileEntryScope::FungibleTypeGroupV1For(type_name.into())),
        ExecutableSurfacePolicy::AllowFailClosed,
    )
}

pub fn compile_path_with_entry_action<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    action: impl Into<String>,
) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file_with_entry_action(&resolved, options, action)
}

pub fn compile_path_with_entry_lock<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    lock: impl Into<String>,
) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file_with_entry_lock(&resolved, options, lock)
}

/// Resolve a file or package and compile its unique structurally eligible
/// fungible Type Script invariant as a payload-free verifier entry.
pub fn compile_path_with_fungible_type_group_entry<P: AsRef<Utf8Path>>(path: P, options: CompileOptions) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file_with_fungible_type_group_entry(&resolved, options)
}

/// Resolve a file or package and compile one named structurally eligible
/// fungible type from a multi-asset package.
pub fn compile_path_with_fungible_type_group_entry_for<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    type_name: impl Into<String>,
) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file_with_fungible_type_group_entry_for(&resolved, options, type_name)
}

/// Compile a file or package under an explicit executable-surface policy.
///
/// This is the production-facing entry used by CLI build/check commands. It
/// keeps the existing permissive APIs source-compatible while making the
/// policy decision explicit for callers that promise executable closure.
pub fn compile_path_with_executable_surface_policy<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    entry_scope: Option<CompileEntryScope>,
    policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    compile_file_with_entry_scope(&resolved, options, entry_scope, policy)
}

/// Select one explicit declaration from the entry package's `[[artifacts]]`.
pub fn compile_path_with_artifact_name<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    name: &str,
    policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    let resolved = resolve_input_path(path)?;
    let declaration = resolve_named_artifact(&resolved, name)?;
    compile_file_with_entry_scope(&resolved, options, Some(CompileEntryScope::Artifact(declaration)), policy)
}

/// Hosts that embed read-only compiler tooling (the MCP server) set this so
/// analysis subprocesses neither write persistent caches nor fetch missing
/// dependencies over the network.
#[cfg(not(feature = "wasm"))]
fn offline_environment_active() -> bool {
    std::env::var("CELLSCRIPT_OFFLINE").is_ok_and(|value| value == "1")
}

#[cfg(feature = "wasm")]
fn offline_environment_active() -> bool {
    false
}

fn resolve_named_artifact(path: &Utf8Path, name: &str) -> Result<artifact::ArtifactDeclaration> {
    let package_root =
        find_package_root(path)?.ok_or_else(|| CompileError::without_span("--artifact requires a package Cell.toml declaration"))?;
    let manifest = load_manifest(&package_root)?;
    artifact::validate_declarations(&manifest.artifacts)?;
    let declaration = manifest
        .artifacts
        .iter()
        .find(|declaration| declaration.name == name)
        .ok_or_else(|| CompileError::without_span(format!("artifact '{name}' is not declared in Cell.toml")))?;
    Ok(declaration.clone())
}

fn compile_file_with_entry_scope<P: AsRef<Utf8Path>>(
    path: P,
    options: CompileOptions,
    entry_scope: Option<CompileEntryScope>,
    executable_surface_policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    if offline_environment_active() {
        return crate::package::with_offline_dependencies(|| {
            compile_file_with_entry_scope_inner(path, options, entry_scope, executable_surface_policy)
        });
    }
    compile_file_with_entry_scope_inner(path, options, entry_scope, executable_surface_policy)
}

fn compile_file_with_entry_scope_inner<P: AsRef<Utf8Path>>(
    path: P,
    mut options: CompileOptions,
    entry_scope: Option<CompileEntryScope>,
    executable_surface_policy: ExecutableSurfacePolicy,
) -> Result<CompileResult> {
    let path = path.as_ref();
    let path = canonical_utf8_path(path)?;
    let manifest = find_package_root(&path)?.map(|root| load_manifest(&root)).transpose()?;
    if let Some(manifest) = manifest.as_ref() {
        options.edition = manifest.package.edition;
    }
    let source_units = collect_source_units_for_compile_file(&path)?;
    let cache_units = collect_cache_units_for_compile_file(&path, &source_units)?;

    // Incremental compilation: skip recompilation if cache hit and source unchanged.
    // Cache is only used for default entry scope (no --entry-action / --entry-lock).
    if entry_scope.is_none()
        && incremental_cache_enabled()
        && let Some(cached) = incremental_cache_hit(&path, &cache_units, &options)
    {
        validate_executable_surface(&cached.metadata, executable_surface_policy)?;
        return Ok(cached);
    }

    let project = load_project_for_entry(&path, None)?;
    let diagnostics = project_frontend_diagnostics(&project, &options, manifest.is_some());
    if diagnostics.iter().any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error) {
        return Err(diagnostics_to_compile_error(diagnostics));
    }

    let entry = project.entry();
    let ast = &entry.ast;
    let empty_ckb = package::CkbDeployConfig::default();
    let mut result = compile_ast_with_build(
        ast,
        &options,
        Some((&project.resolver, &ast.name)),
        manifest.as_ref().map(|manifest| &manifest.build),
        manifest.as_ref().map(|manifest| manifest.deploy.ckb.as_ref().unwrap_or(&empty_ckb)),
        entry_scope.as_ref(),
        executable_surface_policy,
    )?;
    bind_compile_result_source_metadata(&mut result, source_units)?;
    if let Some(manifest) = manifest.as_ref() {
        apply_manifest_deploy_metadata(&mut result.metadata, manifest)?;
    }
    result.validate()?;

    // Incremental compilation: store successful compilation result in cache.
    // Entry-scoped compiles use the same public options as default compiles,
    // so they must not write the default cache key.
    if entry_scope.is_none() && incremental_cache_enabled() {
        incremental_cache_store(&path, &cache_units, &options, &result);
    }

    Ok(result)
}

fn hex_decode(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn source_unit_from_bytes(path: impl Into<String>, role: impl Into<String>, bytes: &[u8]) -> SourceUnitMetadata {
    SourceUnitMetadata { path: path.into(), role: role.into(), hash: hex_encode(&ckb_blake2b256(bytes)), size_bytes: bytes.len() }
}

fn source_unit_from_file(path: &Utf8Path, role: &str) -> Result<SourceUnitMetadata> {
    let bytes = std::fs::read(path)
        .map_err(|e| CompileError::new(format!("failed to read source unit '{}': {}", path, e), error::Span::default()))?;
    Ok(source_unit_from_bytes(path.to_string(), role.to_string(), &bytes))
}

fn collect_source_paths_for_compile_file(entry_path: &Utf8Path) -> Result<BTreeSet<Utf8PathBuf>> {
    let entry_path = canonical_utf8_path(entry_path)?;
    let mut source_paths = BTreeSet::new();
    let package_root = find_package_root(&entry_path)?;

    if let Some(package_root) = &package_root {
        let mut visited_roots = HashSet::new();
        let scope = if entry_path.starts_with(package_root.join("tests")) {
            crate::package::DependencyScope::Test
        } else {
            crate::package::DependencyScope::Runtime
        };
        collect_package_source_paths_recursive(package_root, scope, &mut visited_roots, &mut source_paths)?;
    } else if let Some(parent) = entry_path.parent() {
        for source_path in collect_cell_files(parent)? {
            source_paths.insert(source_path);
        }
    }
    source_paths.insert(entry_path.clone());

    Ok(source_paths)
}

fn collect_source_units_for_compile_file(entry_path: &Utf8Path) -> Result<Vec<SourceUnitMetadata>> {
    let entry_path = canonical_utf8_path(entry_path)?;
    let package_root = find_package_root(&entry_path)?;
    let source_paths = collect_source_paths_for_compile_file(&entry_path)?;

    source_paths
        .into_iter()
        .map(|source_path| {
            let role = if source_path == entry_path {
                "entry"
            } else if package_root.as_ref().is_some_and(|root| source_path.starts_with(root)) {
                "package"
            } else {
                "dependency"
            };
            source_unit_from_file(&source_path, role)
        })
        .collect()
}

fn collect_cache_units_for_compile_file(
    entry_path: &Utf8Path,
    source_units: &[SourceUnitMetadata],
) -> Result<Vec<SourceUnitMetadata>> {
    let entry_path = canonical_utf8_path(entry_path)?;
    let mut units = source_units.to_vec();
    if let Some(package_root) = find_package_root(&entry_path)? {
        let mut package_roots = BTreeSet::new();
        let mut visited_roots = HashSet::new();
        let scope = if entry_path.starts_with(package_root.join("tests")) {
            crate::package::DependencyScope::Test
        } else {
            crate::package::DependencyScope::Runtime
        };
        collect_package_roots_recursive(&package_root, scope, &mut visited_roots, &mut package_roots)?;
        for package_root in package_roots {
            let manifest_path = package_root.join("Cell.toml");
            if manifest_path.exists() {
                units.push(source_unit_from_file(&manifest_path, "manifest")?);
            }
            let lock_path = package_root.join("Cell.lock");
            if let Some(document) = crate::package::active_lockfile_override(package_root.as_std_path())? {
                let content = toml::to_string_pretty(&document).map_err(|error| {
                    CompileError::without_span(format!("failed to encode planned lock source '{}': {error}", lock_path))
                })?;
                units.push(source_unit_from_bytes(lock_path.to_string(), "lockfile", content.as_bytes()));
            } else if lock_path.exists() {
                units.push(source_unit_from_file(&lock_path, "lockfile")?);
            }
        }
    }
    units.sort_by(|left, right| left.path.cmp(&right.path).then_with(|| left.role.cmp(&right.role)));
    Ok(units)
}

fn collect_package_roots_recursive(
    package_root: &Utf8Path,
    scope: crate::package::DependencyScope,
    visited_roots: &mut HashSet<Utf8PathBuf>,
    package_roots: &mut BTreeSet<Utf8PathBuf>,
) -> Result<()> {
    let package_root = canonical_utf8_path(package_root)?;
    if !visited_roots.insert(package_root.clone()) {
        return Ok(());
    }
    package_roots.insert(package_root.clone());
    for dep_root in local_dependency_roots(&package_root, scope)? {
        let dep_root = canonical_utf8_path(&dep_root)?;
        if visited_roots.insert(dep_root.clone()) {
            package_roots.insert(dep_root);
        }
    }
    Ok(())
}

fn collect_package_source_paths_recursive(
    package_root: &Utf8Path,
    scope: crate::package::DependencyScope,
    visited_roots: &mut HashSet<Utf8PathBuf>,
    source_paths: &mut BTreeSet<Utf8PathBuf>,
) -> Result<()> {
    let package_root = canonical_utf8_path(package_root)?;
    if !visited_roots.insert(package_root.clone()) {
        return Ok(());
    }

    for source_path in collect_package_cell_files(&package_root)? {
        source_paths.insert(source_path);
    }
    for dep_root in local_dependency_roots(&package_root, scope)? {
        let dep_root = canonical_utf8_path(&dep_root)?;
        if visited_roots.insert(dep_root.clone()) {
            for source_path in collect_package_cell_files(&dep_root)? {
                source_paths.insert(source_path);
            }
        }
    }

    Ok(())
}

fn find_package_root(path: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let mut current = path.parent();
    while let Some(dir) = current {
        let manifest = dir.join("Cell.toml");
        if manifest.exists() && manifest_has_table(&manifest, "package")? {
            return Ok(Some(dir.to_path_buf()));
        }
        current = dir.parent();
    }
    Ok(None)
}

/// Find the workspace root by walking upward from `path`.
/// A workspace root is a directory whose `Cell.toml` contains a `[workspace]` section
/// and optionally no `[package]` section (virtual manifest).
pub fn find_workspace_root(path: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let mut current = path.parent();
    while let Some(dir) = current {
        let manifest_path = dir.join("Cell.toml");
        if manifest_path.exists() && manifest_has_table(&manifest_path, "workspace")? {
            return Ok(Some(dir.to_path_buf()));
        }
        current = dir.parent();
    }
    Ok(None)
}

/// Resolve workspace member directories from a workspace root.
/// Reads the `[workspace]` section and resolves member paths.
pub fn resolve_workspace_members(workspace_root: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    package::workspace::resolve_workspace_member_paths(workspace_root.as_std_path())?
        .into_iter()
        .map(|path| {
            Utf8PathBuf::from_path_buf(path)
                .map_err(|path| CompileError::without_span(format!("workspace member path '{}' is not valid UTF-8", path.display())))
        })
        .collect()
}

fn local_dependency_roots(package_root: &Utf8Path, scope: crate::package::DependencyScope) -> Result<Vec<Utf8PathBuf>> {
    let mut manager = crate::package::PackageManager::new(package_root.as_std_path());
    manager.resolve_locked_dependencies(&crate::package::active_resolution_options(scope))?;
    let mut roots = Vec::new();
    for package in manager.get_resolved().values() {
        let dep_root = Utf8PathBuf::from_path_buf(package.path.clone()).map_err(|path| {
            CompileError::new(
                format!("dependency '{}' resolved to non-UTF-8 path '{}'", package.name, path.display()),
                error::Span::default(),
            )
        })?;
        roots.push(canonical_utf8_path(&dep_root)?);
    }

    Ok(roots)
}

fn resolve_input_file(input: &Utf8Path) -> Result<Utf8PathBuf> {
    if input.is_dir() {
        return resolve_package_entry(input);
    }

    if input.file_name() == Some("Cell.toml") {
        let Some(parent) = input.parent() else {
            return Err(CompileError::new("Cell.toml must live inside a package directory", error::Span::default())
                .with_category(error::CompileErrorCategory::Usage));
        };
        return resolve_package_entry(parent);
    }

    if input.extension() == Some("cell") {
        if !input.exists() {
            return Err(CompileError::new(format!("input file '{}' does not exist", input), error::Span::default())
                .with_code(error::DiagnosticCode::CLI0005)
                .with_category(error::CompileErrorCategory::Io)
                .with_file(input.to_owned()));
        }
        return canonical_utf8_path(input);
    }

    Err(CompileError::new(
        format!("unsupported input '{}'; expected a .cell file, package directory, or Cell.toml", input),
        error::Span::default(),
    )
    .with_code(error::DiagnosticCode::CLI0004)
    .with_category(error::CompileErrorCategory::Usage)
    .with_file(input.to_owned()))
}

fn resolve_package_entry(package_root: &Utf8Path) -> Result<Utf8PathBuf> {
    let manifest = load_manifest(package_root)?;

    let entry = manifest.package.entry;
    let entry_path = package_root.join(entry);
    if !entry_path.exists() {
        return Err(CompileError::new(format!("package entry '{}' does not exist", entry_path), error::Span::default())
            .with_category(error::CompileErrorCategory::Io)
            .with_file(entry_path));
    }

    canonical_utf8_path(&entry_path)
}

fn default_output_path_from_input(
    input: &Utf8Path,
    resolved_input: &Utf8Path,
    artifact_format: ArtifactFormat,
) -> Result<Utf8PathBuf> {
    if input.is_dir() || input.file_name() == Some("Cell.toml") {
        let package_root = if input.is_dir() {
            canonical_utf8_path(input)?
        } else {
            let Some(parent) = input.parent() else {
                return Err(CompileError::new("Cell.toml must live inside a package directory", error::Span::default()));
            };
            canonical_utf8_path(parent)?
        };
        let stem = resolved_input.file_stem().ok_or_else(|| {
            CompileError::new(format!("cannot derive output filename from '{}'", resolved_input), error::Span::default())
        })?;
        let manifest = load_manifest(&package_root)?;
        let out_dir = manifest.build.out_dir.as_deref().unwrap_or("build");
        return Ok(package_root.join(out_dir).join(format!("{}.{}", stem, artifact_format.file_extension())));
    }

    Ok(resolved_input.with_extension(artifact_format.file_extension()))
}

fn metadata_output_path_from_artifact(artifact_path: &Utf8Path) -> Utf8PathBuf {
    let file_name = artifact_path.file_name().unwrap_or("artifact");
    let metadata_name = format!("{}.meta.json", file_name);
    artifact_path.with_file_name(metadata_name)
}

pub fn lowering_record_output_path_from_artifact(artifact_path: &Utf8Path) -> Utf8PathBuf {
    let file_name = artifact_path.file_name().unwrap_or("artifact");
    artifact_path.with_file_name(format!("{}.lowering.json", file_name))
}

pub fn source_map_output_path_from_artifact(artifact_path: &Utf8Path) -> Utf8PathBuf {
    let file_name = artifact_path.file_name().unwrap_or("artifact");
    artifact_path.with_file_name(format!("{}.sourcemap.json", file_name))
}

#[cfg(not(feature = "wasm"))]
fn generic_instantiation_metadata(ir: &ir::IrModule) -> Vec<GenericInstantiationMetadata> {
    let mut entries = BTreeMap::<String, GenericInstantiationMetadata>::new();
    let mut insert = |kind: &str, lowered_name: &str, fixed_layout_required: bool| {
        let Some(contract) = ir.generic_contracts.get(lowered_name) else { return };
        let identity = format!("{}::{}<{}>", contract.module, contract.template, contract.argument_texts().join(","));
        let entry = entries.entry(identity.clone()).or_insert_with(|| GenericInstantiationMetadata {
            schema: "cellscript-generic-instantiation-v2".to_string(),
            version: 2,
            kind: kind.to_string(),
            module: contract.module.clone(),
            template: contract.template.clone(),
            concrete_name: contract.concrete_name.clone(),
            identity,
            type_arguments: contract.argument_texts(),
            lowered_names: Vec::new(),
            declaration: typed_semantics::generic_declaration(&contract.declaration),
            parameters: contract
                .parameters
                .iter()
                .map(|parameter| cellscript_artifact_checker::TypedSemanticGenericParameter {
                    name: parameter.name.clone(),
                    constraints: parameter.constraints.iter().map(|ability| ability.as_str().to_string()).collect(),
                    phantom: parameter.phantom,
                })
                .collect(),
            value_ability_registry_version: ast::ValueAbility::REGISTRY_VERSION,
            constraints_verified: true,
            fixed_layout_required,
            cell_backed_layout_rejected: fixed_layout_required,
            identity_includes_phantom_arguments: true,
        });
        entry.lowered_names.push(lowered_name.to_string());
        entry.lowered_names.sort();
        entry.lowered_names.dedup();
    };
    for item in &ir.items {
        match item {
            ir::IrItem::TypeDef(definition) => insert("struct", &definition.name, true),
            ir::IrItem::Function(function) => insert("function", &function.name, false),
            _ => {}
        }
    }
    for definition in &ir.external_type_defs {
        insert("struct", &definition.name, true);
    }
    for name in ir.enum_layouts.keys() {
        insert("enum", name, true);
    }
    entries.into_values().collect()
}

#[cfg(not(feature = "wasm"))]
fn bind_public_interface(metadata: &mut CompileMetadata, ast: &ast::Module, resolver: Option<&ModuleResolver>) -> Result<()> {
    for warning in visibility_migration_diagnostics(ast) {
        if !metadata.constraints.warnings.contains(&warning.message) {
            metadata.constraints.warnings.push(warning.message.clone());
        }
    }
    metadata.public_interface = interface::build_with_resolver(ast, metadata, resolver)?;
    metadata.interface_hash = interface::hash(&metadata.public_interface);
    Ok(())
}

#[cfg(feature = "wasm")]
fn bind_public_interface(metadata: &mut CompileMetadata, ast: &ast::Module, resolver: Option<&ModuleResolver>) -> Result<()> {
    effects::infer_module(ast, resolver)?;
    for warning in visibility_migration_diagnostics(ast) {
        if !metadata.constraints.warnings.contains(&warning.message) {
            metadata.constraints.warnings.push(warning.message.clone());
        }
    }
    Ok(())
}

pub fn visibility_migration_diagnostics(module: &ast::Module) -> Vec<CompileError> {
    let source_items = module.items.iter().chain(module.interface_templates.iter()).filter_map(|item| {
        let name = item.name()?;
        if item.is_specialization() {
            return None;
        }
        let span = match item {
            ast::Item::DataShape(def) => def.span,
            ast::Item::Collections(def) => def.span,
            ast::Item::CellRoles(def) => def.span,
            ast::Item::Resource(def) => def.span,
            ast::Item::Shared(def) => def.span,
            ast::Item::Receipt(def) => def.span,
            ast::Item::Struct(def) => def.span,
            ast::Item::Invariant(def) => def.span,
            ast::Item::Const(def) => def.span,
            ast::Item::Enum(def) => def.span,
            ast::Item::Action(def) => def.span,
            ast::Item::Function(def) => def.span,
            ast::Item::Lock(def) => def.span,
            ast::Item::Flow(_) | ast::Item::Use(_) => return None,
        };
        Some((name, module.visibility_of(name), span))
    });
    let items = source_items.collect::<Vec<_>>();
    if !items.iter().any(|(_, visibility, _)| *visibility != ast::Visibility::ImplicitPublic) {
        return Vec::new();
    }
    let implicit = items
        .iter()
        .filter(|(_, visibility, _)| *visibility == ast::Visibility::ImplicitPublic)
        .map(|(name, _, _)| (*name).to_string())
        .collect::<Vec<_>>();
    if implicit.is_empty() {
        return Vec::new();
    }
    vec![CompileError::warning(
        format!(
            "module '{}' mixes explicit visibility with public-by-default declarations; add public, public(package), or private to: {}",
            module.name,
            implicit.join(", ")
        ),
        items
            .iter()
            .find(|(_, visibility, _)| *visibility == ast::Visibility::ImplicitPublic)
            .map_or(module.span, |(_, _, span)| *span),
    )
    .with_code(crate::error::DiagnosticCode::W2500)
    .with_details(serde_json::json!({
        "module": module.name,
        "implicit_visibility": "implicit-public",
        "items": implicit,
        "migration": "choose public, public(package), or private explicitly",
    }))]
}

#[cfg(not(feature = "wasm"))]
fn bind_typed_semantics(
    metadata: &mut CompileMetadata,
    ir: &ir::IrModule,
    ast: &ast::Module,
    resolver: Option<&ModuleResolver>,
    source_contracts: bool,
) -> Result<()> {
    let mut typed = typed_semantics::build(ir, metadata);
    let required_declaration_evidence = typed_semantics::required_declaration_evidence(ir);
    if source_contracts || required_declaration_evidence {
        typed.generic_declarations = typed_semantics::generic_catalog_for_context(ast, resolver);
        typed.nominal_declarations = typed_semantics::nominal_catalog(ast, resolver, ir, &typed);
        if required_declaration_evidence && typed.nominal_declarations.is_none() {
            return Err(CompileError::without_span(
                "aliased or ambiguous nominal bindings and interface handles require a complete bounded declaration/owner catalog",
            )
            .with_code(error::DiagnosticCode::E2111));
        }
    }
    if source_contracts {
        typed.constant_values = typed_semantics::constant_catalog(ast);
    }
    typed.interface_hash = metadata.interface_hash.clone();
    typed.canonicalize();
    metadata.typed_semantics_hash =
        cellscript_artifact_checker::canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &typed)
            .expect("compiler-emitted typed semantics are serializable");
    metadata.runtime.zk_verifiers =
        cellscript_artifact_checker::zk_profile::contracts(&typed).expect("validated exact ZK IR has canonical typed operands");
    metadata.typed_semantics = typed;
    Ok(())
}

#[cfg(feature = "wasm")]
fn bind_typed_semantics(
    metadata: &mut CompileMetadata,
    ir: &ir::IrModule,
    _ast: &ast::Module,
    _resolver: Option<&ModuleResolver>,
    _source_contracts: bool,
) -> Result<()> {
    metadata.runtime.zk_verifiers = zk_contract::browser_contracts(ir);
    Ok(())
}

fn module_has_entry_params(ir: &ir::IrModule) -> bool {
    ir.items.iter().any(|item| match item {
        ir::IrItem::Action(action) => !action.params.is_empty(),
        ir::IrItem::Lock(lock) => !lock.params.is_empty(),
        ir::IrItem::Function(_) => false,
        ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => false,
    })
}

fn hex_hash(bytes: &[u8; 32]) -> String {
    hex_bytes(bytes)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{:02x}", byte);
    }
    out
}

fn resolve_target<'a>(options: &'a CompileOptions, build: Option<&'a BuildConfig>) -> &'a str {
    options.target.as_deref().or_else(|| build.and_then(|build| build.target.as_deref())).unwrap_or(DEFAULT_TARGET)
}

/// Compiler version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Compiler name
pub const NAME: &str = "cellc";

#[cfg(test)]
mod tests;

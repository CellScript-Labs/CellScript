use crate::{FiberAssetDescriptor, FIBER_COMPATIBILITY_SCHEMA, FUNGIBLE_ENTRY_CONTRACT};
use cellscript::{ArtifactFormat, CompileOptions, CompileResult, EvidenceTier};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberDiagnostic {
    pub code: String,
    pub boundary: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
}

impl FiberDiagnostic {
    fn incompatible(code: &str, boundary: &str, message: impl Into<String>, next_action: impl Into<String>) -> Self {
        Self { code: code.to_string(), boundary: boundary.to_string(), message: message.into(), next_action: Some(next_action.into()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "result", rename_all = "snake_case")]
pub enum FiberCompatibility {
    Compatible(FiberAssetDescriptor),
    Incompatible(Vec<FiberDiagnostic>),
    RequiresRuntimeEvidence { descriptor: FiberAssetDescriptor, diagnostics: Vec<FiberDiagnostic> },
}

#[derive(Debug, Clone)]
pub struct CheckedFiberAsset {
    pub descriptor: FiberAssetDescriptor,
    pub compile_result: CompileResult,
}

pub fn check_path(path: impl AsRef<Path>) -> anyhow::Result<CheckedFiberAsset> {
    let path =
        camino::Utf8Path::from_path(path.as_ref()).ok_or_else(|| anyhow::anyhow!("CellScript source path is not valid UTF-8"))?;
    let compile_result = cellscript::compile_path_with_fungible_type_group_entry(
        path,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.17".to_string()),
            ..Default::default()
        },
    )?;
    match analyze_compile_result(&compile_result) {
        FiberCompatibility::Compatible(descriptor) => Ok(CheckedFiberAsset { descriptor, compile_result }),
        FiberCompatibility::Incompatible(diagnostics) => Err(anyhow::anyhow!(render_diagnostics(&diagnostics))),
        FiberCompatibility::RequiresRuntimeEvidence { diagnostics, .. } => Err(anyhow::anyhow!(render_diagnostics(&diagnostics))),
    }
}

pub fn analyze_compile_result(result: &CompileResult) -> FiberCompatibility {
    if let Err(error) = result.validate() {
        return FiberCompatibility::Incompatible(vec![FiberDiagnostic::incompatible(
            "FBR1000",
            "artifact-integrity",
            format!("artifact and compiler metadata validation failed: {}", error.message),
            "recompile the unchanged source with CellScript 0.22 and do not edit generated metadata",
        )]);
    }
    let metadata = &result.metadata;
    let mut diagnostics = Vec::new();
    if result.artifact_format != ArtifactFormat::RiscvElf || metadata.target_profile.name != "ckb" {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1001",
            "artifact-target",
            format!(
                "Fiber v1 requires a CKB RISC-V ELF artifact, got {} for target profile '{}'",
                metadata.artifact_format, metadata.target_profile.name
            ),
            "compile the dedicated fungible entry with target_profile=ckb and target=riscv64-elf",
        ));
    }
    let Some(contract) = metadata.runtime.fungible_type_group_entry.as_ref() else {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1002",
            "entry-contract",
            "artifact does not declare the dedicated fungible-type-group-v1 runtime contract",
            "compile through compile_path_with_fungible_type_group_entry; ordinary business actions are not Fiber entries",
        ));
        return FiberCompatibility::Incompatible(diagnostics);
    };
    if contract.contract != FUNGIBLE_ENTRY_CONTRACT
        || contract.data_length_bytes != 16
        || contract.amount_offset_bytes != 0
        || contract.amount_width_bytes != 16
        || contract.endianness != "little"
    {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1003",
            "cell-data-codec",
            format!(
                "Fiber v1 requires exactly one 16-byte little-endian u128 at offset zero, but '{}' declares length={}, offset={}, width={}, endian={}",
                contract.type_name,
                contract.data_length_bytes,
                contract.amount_offset_bytes,
                contract.amount_width_bytes,
                contract.endianness
            ),
            "keep only the u128 quantity in the Fiber-managed Type Script and move application state to a separate Cell",
        ));
    }
    if contract.evidence_tier != EvidenceTier::CheckedRuntime
        || contract.runtime_helper != "fungible::require_type_group_v1"
        || contract.owner_mode != "script-args-32-byte-owner-lock-hash"
        || contract.owner_args_length_bytes != 32
        || !contract.owner_authorized_mint
        || !contract.owner_authorized_burn
        || !contract.non_owner_input_group_non_empty
        || !contract.non_owner_output_group_non_empty
        || !contract.non_owner_conservation_required
    {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1004",
            "conservation",
            "entry lacks the closed owner-lock issuance plus checked-runtime non-owner complete-group conservation contract",
            "use one type_group/group sum-equality invariant eligible for fungible-type-group-v1",
        ));
    }
    if contract.payload_required
        || metadata
            .runtime
            .ckb_runtime_accesses
            .iter()
            .any(|access| access.source.contains("Witness") || access.syscall.contains("WITNESS"))
    {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1005",
            "entry-witness",
            "entry requires or inspects an application witness; Fiber v1 supplies no CellScript payload",
            "make the channel verifier payload-free and tolerate Fiber's existing xUDT-compatible WitnessArgs bytes",
        ));
    }
    if metadata.actions.len() != 1
        || metadata.actions[0].name != contract.entry_action
        || !metadata.actions[0].params.is_empty()
        || !metadata.actions[0].consume_set.is_empty()
        || !metadata.actions[0].create_set.is_empty()
        || !metadata.actions[0].mutate_set.is_empty()
    {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1006",
            "entry-isolation",
            "artifact contains ordinary action lifecycle behavior instead of only the dedicated invariant entry",
            "compile the structural invariant entry rather than selecting a business action",
        ));
    }
    if !metadata.runtime.transaction_runtime_input_requirements.is_empty() {
        diagnostics.push(FiberDiagnostic::incompatible(
            "FBR1007",
            "ambient-runtime-inputs",
            "entry declares transaction runtime inputs beyond complete type-group Cell data",
            "remove dynamic HeaderDep, oracle, fixed-index, and application-specific runtime requirements from the channel entry",
        ));
    }
    if !diagnostics.is_empty() {
        return FiberCompatibility::Incompatible(diagnostics);
    }

    let source_hash = metadata
        .source_content_hash
        .clone()
        .or_else(|| metadata.source_hash.clone())
        .unwrap_or_else(|| hex::encode(cellscript::ckb_blake2b256(b"missing-source-evidence")));
    let artifact_hash = format!("0x{}", hex::encode(result.artifact_hash));
    FiberCompatibility::Compatible(FiberAssetDescriptor {
        schema: FIBER_COMPATIBILITY_SCHEMA.to_string(),
        contract: contract.contract.clone(),
        module: metadata.module.clone(),
        display_name: format!("{}::{}", metadata.module, contract.type_name),
        selected_type: contract.type_name.clone(),
        selected_invariant: contract.invariant.clone(),
        selected_field: contract.field.clone(),
        compiler_version: metadata.compiler_version.clone(),
        metadata_schema_version: metadata.metadata_schema_version,
        source_hash,
        artifact_hash,
        artifact_format: metadata.artifact_format.clone(),
        target_profile: metadata.target_profile.name.clone(),
        data_length_bytes: contract.data_length_bytes,
        amount_offset_bytes: contract.amount_offset_bytes,
        amount_width_bytes: contract.amount_width_bytes,
        endianness: contract.endianness.clone(),
        arithmetic: contract.arithmetic.clone(),
        group_scope: contract.group_scope.clone(),
        owner_mode: contract.owner_mode.clone(),
        owner_args_length_bytes: contract.owner_args_length_bytes,
        owner_authorized_mint: contract.owner_authorized_mint,
        owner_authorized_burn: contract.owner_authorized_burn,
        non_owner_input_group_non_empty: contract.non_owner_input_group_non_empty,
        non_owner_output_group_non_empty: contract.non_owner_output_group_non_empty,
        non_owner_conservation_required: contract.non_owner_conservation_required,
        payload_required: contract.payload_required,
        witness_policy: contract.witness_policy.clone(),
        runtime_helper: contract.runtime_helper.clone(),
    })
}

fn render_diagnostics(diagnostics: &[FiberDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{} [{}]: {}", diagnostic.code, diagnostic.boundary, diagnostic.message))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
module sample

invariant supply {
    trigger: type_group
    scope: group
    reads: group_inputs<Asset>.quantity, group_outputs<Asset>.quantity
    assert_sum(group_outputs<Asset>.quantity) == assert_sum(group_inputs<Asset>.quantity)
}

resource Asset { quantity: u128 }
"#;

    const SOURCE_WITH_ADDITIONAL_ASSET_RULE: &str = r#"
module sample

invariant supply {
    trigger: type_group
    scope: group
    reads: group_inputs<Asset>.quantity, group_outputs<Asset>.quantity
    assert_sum(group_outputs<Asset>.quantity) == assert_sum(group_inputs<Asset>.quantity)
}

invariant singleton_output {
    trigger: type_group
    scope: group
    reads: group_outputs<Asset>.quantity
    assert_singleton(Asset.quantity, scope = group)
}

resource Asset { quantity: u128 }
"#;

    #[test]
    fn analyzer_accepts_only_the_dedicated_structural_artifact() {
        let result = cellscript::compile_fungible_type_group_entry(
            SOURCE,
            CompileOptions { target: Some("riscv64-elf".to_string()), target_profile: Some("ckb".to_string()), ..Default::default() },
        )
        .unwrap();
        let FiberCompatibility::Compatible(descriptor) = analyze_compile_result(&result) else {
            panic!("expected compatible descriptor")
        };
        assert_eq!(descriptor.selected_field, "quantity");
        assert_eq!(descriptor.data_length_bytes, 16);
        assert!(!descriptor.payload_required);
        assert_eq!(descriptor.owner_mode, "script-args-32-byte-owner-lock-hash");
        assert_eq!(descriptor.owner_args_length_bytes, 32);
        assert!(descriptor.non_owner_conservation_required);
    }

    #[test]
    fn selector_rejects_additional_invariants_for_the_asset_type() {
        let error = cellscript::compile_fungible_type_group_entry(
            SOURCE_WITH_ADDITIONAL_ASSET_RULE,
            CompileOptions { target: Some("riscv64-elf".to_string()), target_profile: Some("ckb".to_string()), ..Default::default() },
        )
        .expect_err("additional asset rules must not be discarded by the dedicated entry");
        assert!(error.message.contains("no eligible invariant"), "unexpected diagnostic: {}", error.message);
    }
}

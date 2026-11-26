use super::deployment_metadata::validate_ckb_hash_type;
use super::entry_scope::{FUNGIBLE_TYPE_GROUP_V1_CONTRACT, FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION};
use super::error::{CompileError, Result};
use super::metadata_constraints::ckb_script_reference_metadata;
use super::proof_plan::EvidenceTier;
use super::runtime_metadata::ckb_script_group_metadata;
use super::{
    aggregate_lowering, hex_encode, is_canonical_hash_hex, runtime_range_fixed, runtime_scalar_not_applicable,
    CkbRuntimeAccessMetadata, CkbRuntimeAccessProvenanceMetadata, CkbRuntimeRangeProvenanceMetadata,
    CkbRuntimeScalarProvenanceMetadata, CkbScriptGroupMetadata, CkbScriptReferenceMetadata, CompileMetadata, CreatePatternMetadata,
    TargetProfile, TypeMetadata, CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT, CKB_SCRIPT_HASH_MAX_ARGS_BYTES,
    CKB_SIGHASH_ALL_ZERO_LOCK_CONTRACT, CKB_TYPE_ID_ABI, CKB_TYPE_ID_ARGS_SOURCE, CKB_TYPE_ID_BUILDER, CKB_TYPE_ID_CODE_HASH,
    CKB_TYPE_ID_GENERATOR_SETTING, CKB_TYPE_ID_HASH_TYPE, CKB_TYPE_ID_OUTPUT_SOURCE, CKB_TYPE_ID_WASM_SETTING,
};
use std::collections::HashMap;

pub(super) fn validate_ckb_output_data_create_set_metadata(
    scope: &str,
    name: &str,
    create_set: &[CreatePatternMetadata],
    profile_is_ckb: bool,
) -> Result<()> {
    for (index, pattern) in create_set.iter().enumerate() {
        if let Some(binding) = &pattern.ckb_output_data {
            if !profile_is_ckb {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_output_data outside the ckb target profile",
                    scope, name, index
                )));
            }
            let mismatches = [
                ("abi", binding.abi.as_str(), "ckb-outputs-and-outputs-data-index-aligned"),
                ("output_source", binding.output_source.as_str(), "Output"),
                ("output_data_source", binding.output_data_source.as_str(), "outputs_data"),
                ("relation", binding.relation.as_str(), "same-index"),
            ];
            for (field, actual, expected) in mismatches {
                if actual != expected {
                    return Err(CompileError::without_span(format!(
                        "metadata {} '{}' create_set[{}].ckb_output_data.{} '{}' does not match expected '{}'",
                        scope, name, index, field, actual, expected
                    )));
                }
            }
            if binding.output_index != index {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}].ckb_output_data.output_index {} does not match create_set index",
                    scope, name, index, binding.output_index
                )));
            }
            if binding.output_data_index != index {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}].ckb_output_data.output_data_index {} does not match output index",
                    scope, name, index, binding.output_data_index
                )));
            }
        } else if profile_is_ckb
            && matches!(
                pattern.operation.as_str(),
                "create" | "output" | "create_unique" | "replace_unique" | "transfer" | "claim" | "settle"
            )
        {
            return Err(CompileError::without_span(format!(
                "metadata {} '{}' create_set[{}] creates output '{}' but is missing ckb_output_data index binding",
                scope, name, index, pattern.binding
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_ckb_type_id_create_set_metadata(
    scope: &str,
    name: &str,
    create_set: &[CreatePatternMetadata],
    types_by_name: &HashMap<&str, &TypeMetadata>,
    profile_is_ckb: bool,
) -> Result<()> {
    let expected_code_hash = hex_encode(&CKB_TYPE_ID_CODE_HASH);
    for (index, pattern) in create_set.iter().enumerate() {
        let ty = types_by_name.get(pattern.ty.as_str()).copied().ok_or_else(|| {
            CompileError::without_span(format!(
                "metadata {} '{}' create_set[{}] references unknown type '{}'",
                scope, name, index, pattern.ty
            ))
        })?;

        if let Some(plan) = &pattern.ckb_type_id {
            if !profile_is_ckb {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_type_id outside the ckb target profile",
                    scope, name, index
                )));
            }
            if !matches!(pattern.operation.as_str(), "create" | "output" | "create_unique") {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_type_id for unsupported operation '{}'",
                    scope, name, index, pattern.operation
                )));
            }
            if pattern.operation == "create_unique" && pattern.identity_policy.as_deref() != Some("ckb_type_id") {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_type_id but create_unique identity policy is {:?}",
                    scope, name, index, pattern.identity_policy
                )));
            }
            if ty.ckb_type_id.is_none() {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_type_id but type '{}' has no ckb_type_id contract",
                    scope, name, index, pattern.ty
                )));
            }
            let Some(source_type_id) = ty.type_id.as_deref() else {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] has ckb_type_id but type '{}' has no source type_id",
                    scope, name, index, pattern.ty
                )));
            };
            let mismatches = [
                ("abi", plan.abi.as_str(), CKB_TYPE_ID_ABI),
                ("type_id", plan.type_id.as_str(), source_type_id),
                ("output_source", plan.output_source.as_str(), CKB_TYPE_ID_OUTPUT_SOURCE),
                ("script_code_hash", plan.script_code_hash.as_str(), expected_code_hash.as_str()),
                ("hash_type", plan.hash_type.as_str(), CKB_TYPE_ID_HASH_TYPE),
                ("args_source", plan.args_source.as_str(), CKB_TYPE_ID_ARGS_SOURCE),
                ("builder", plan.builder.as_str(), CKB_TYPE_ID_BUILDER),
                ("generator_setting", plan.generator_setting.as_str(), CKB_TYPE_ID_GENERATOR_SETTING),
                ("wasm_setting", plan.wasm_setting.as_str(), CKB_TYPE_ID_WASM_SETTING),
            ];
            for (field, actual, expected) in mismatches {
                if actual != expected {
                    return Err(CompileError::without_span(format!(
                        "metadata {} '{}' create_set[{}].ckb_type_id.{} '{}' does not match expected '{}'",
                        scope, name, index, field, actual, expected
                    )));
                }
            }
            if plan.output_index != index {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}].ckb_type_id.output_index {} does not match create_set index",
                    scope, name, index, plan.output_index
                )));
            }
        } else if profile_is_ckb && matches!(pattern.operation.as_str(), "create" | "output") && ty.ckb_type_id.is_some() {
            return Err(CompileError::without_span(format!(
                "metadata {} '{}' create_set[{}] creates CKB TYPE_ID type '{}' but is missing ckb_type_id output plan",
                scope, name, index, pattern.ty
            )));
        } else if profile_is_ckb && pattern.operation == "create_unique" && pattern.identity_policy.as_deref() == Some("ckb_type_id") {
            if ty.ckb_type_id.is_none() {
                return Err(CompileError::without_span(format!(
                    "metadata {} '{}' create_set[{}] create_unique uses ckb_type_id identity but type '{}' has no ckb_type_id contract",
                    scope, name, index, pattern.ty
                )));
            }
            return Err(CompileError::without_span(format!(
                "metadata {} '{}' create_set[{}] create_unique CKB TYPE_ID type '{}' is missing ckb_type_id output plan",
                scope, name, index, pattern.ty
            )));
        }
    }

    Ok(())
}

pub(super) fn validate_ckb_runtime_access_metadata(metadata: &CompileMetadata) -> Result<()> {
    validate_ckb_runtime_access_list("runtime", "module", &metadata.runtime.ckb_runtime_accesses)?;
    for action in &metadata.actions {
        validate_ckb_runtime_access_list("action", &action.name, &action.ckb_runtime_accesses)?;
    }
    for function in &metadata.functions {
        validate_ckb_runtime_access_list("function", &function.name, &function.ckb_runtime_accesses)?;
    }
    for lock in &metadata.locks {
        validate_ckb_runtime_access_list("lock", &lock.name, &lock.ckb_runtime_accesses)?;
    }

    let mut expected_runtime_accesses = Vec::new();
    for action in &metadata.actions {
        expected_runtime_accesses.extend(ckb_runtime_access_fingerprints(action.ckb_runtime_accesses.iter()));
    }
    for function in &metadata.functions {
        expected_runtime_accesses.extend(ckb_runtime_access_fingerprints(function.ckb_runtime_accesses.iter()));
    }
    for lock in &metadata.locks {
        expected_runtime_accesses.extend(ckb_runtime_access_fingerprints(lock.ckb_runtime_accesses.iter()));
    }
    let mut actual_runtime_accesses = ckb_runtime_access_fingerprints(metadata.runtime.ckb_runtime_accesses.iter());
    expected_runtime_accesses.sort();
    actual_runtime_accesses.sort();
    if actual_runtime_accesses != expected_runtime_accesses {
        let missing = ckb_runtime_access_multiset_difference(&expected_runtime_accesses, &actual_runtime_accesses);
        let extra = ckb_runtime_access_multiset_difference(&actual_runtime_accesses, &expected_runtime_accesses);
        return Err(CompileError::without_span(format!(
            "metadata runtime.ckb_runtime_accesses do not match action/function/lock runtime metadata; missing={:?}; extra={:?}",
            missing, extra
        )));
    }
    for access in &metadata.runtime.ckb_runtime_accesses {
        let Some((owner, maximum)) = validate_bounded_witness_runtime_access(access)? else {
            continue;
        };
        if !metadata.runtime.transaction_view_handles.iter().any(|handle| {
            cellscript_artifact_checker::view_types::parse_witness_bytes_view_type(
                &handle.handle_type,
                cellscript_artifact_checker::view_types::WitnessViewSyntax::ExactArguments,
            )
            .is_some_and(|view| {
                view.owner.as_str() == owner
                    && view.maximum == maximum
                    && handle.provenance.source.resolved_source == access.provenance.source.resolved_source
                    && handle.provenance.index == access.provenance.index
            })
        }) {
            return Err(CompileError::without_span(format!(
                "bounded witness runtime access '{}' has no matching transaction-view handle",
                access.operation
            )));
        }
    }
    Ok(())
}

fn validate_bounded_witness_runtime_access(access: &CkbRuntimeAccessMetadata) -> Result<Option<(&'static str, u64)>> {
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
        return Err(CompileError::without_span(format!("bounded witness runtime operation '{}' is not canonical", access.operation)));
    };
    if access.syscall != "LOAD_WITNESS"
        || access.source != "Witness"
        || access.binding != format!("witness::bounded_{owner}")
        || !matches!(access.provenance.source.resolved_source.as_str(), "Input" | "Output" | "GroupInput" | "GroupOutput")
        || access.provenance.source.origin != "inherited-source-view"
    {
        return Err(CompileError::without_span(format!(
            "bounded witness runtime access '{}' has an invalid source contract",
            access.operation
        )));
    }
    let range = &access.provenance.range;
    let maximum = if matches!(suffix, "size" | "blake2b") {
        if range.kind != "bounded-range"
            || range.offset.kind != "static"
            || range.offset.value != Some(0)
            || range.offset.binding.is_some()
            || range.length.kind != "dynamic"
            || range.length.value.is_some()
            || range.length.binding.as_deref() != Some(format!("bounded_{owner}.size").as_str())
            || range.offset.max_inclusive != range.length.max_inclusive
        {
            return Err(CompileError::without_span(format!(
                "bounded witness runtime access '{}' has an invalid whole-view range",
                access.operation
            )));
        }
        range.length.max_inclusive
    } else {
        let width = width.expect("bounded scalar width");
        if range.kind != "bounded-range"
            || !matches!(range.offset.kind.as_str(), "static" | "dynamic")
            || range.length.kind != "static"
            || range.length.value != Some(width)
            || range.length.binding.is_some()
            || range.length.max_inclusive != Some(width)
        {
            return Err(CompileError::without_span(format!(
                "bounded witness runtime access '{}' has an invalid scalar range",
                access.operation
            )));
        }
        range.offset.max_inclusive
    };
    let Some(maximum) = maximum.filter(|maximum| *maximum <= 65_536) else {
        return Err(CompileError::without_span(format!(
            "bounded witness runtime access '{}' exceeds the maximum byte domain",
            access.operation
        )));
    };
    Ok(Some((owner, maximum)))
}

pub(super) fn validate_fungible_type_group_entry_metadata(metadata: &CompileMetadata) -> Result<()> {
    let declared = metadata.runtime.fungible_type_group_entry.as_ref();
    let entry = metadata.actions.iter().find(|action| action.name == FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION);
    let Some(contract) = declared else {
        if entry.is_some() {
            return Err(CompileError::without_span(
                "metadata contains the reserved fungible type-group entry action without its structured runtime contract",
            ));
        }
        return Ok(());
    };
    let Some(entry) = entry else {
        return Err(CompileError::without_span(
            "metadata declares a fungible type-group runtime contract without its reserved entry action",
        ));
    };
    if metadata.actions.len() != 1 {
        return Err(CompileError::without_span(
            "fungible-type-group-v1 artifact metadata must contain only its dedicated verifier action",
        ));
    }
    if contract.contract != FUNGIBLE_TYPE_GROUP_V1_CONTRACT
        || contract.entry_action != FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION
        || contract.data_length_bytes != 16
        || contract.amount_offset_bytes != 0
        || contract.amount_width_bytes != 16
        || contract.endianness != "little"
        || contract.arithmetic != "checked-u128-sum-equality"
        || contract.group_scope != "complete-ckb-type-script-group"
        || contract.owner_mode != "script-args-32-byte-owner-lock-hash"
        || contract.owner_args_length_bytes != 32
        || contract.authority_modes != ["input-lock-hash".to_string(), "tagged-input-type-script-hash".to_string()]
        || contract.authority_args_lengths_bytes != [32, 33]
        || !contract.owner_authorized_mint
        || !contract.owner_authorized_burn
        || !contract.non_owner_input_group_non_empty
        || !contract.non_owner_output_group_non_empty
        || !contract.non_owner_conservation_required
        || contract.payload_required
        || contract.witness_policy != "ignored; no empty-witness requirement"
        || contract.runtime_helper != aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_METADATA_HELPER
        || contract.evidence_tier != EvidenceTier::CheckedRuntime
    {
        return Err(CompileError::without_span(
            "metadata fungible_type_group_entry does not match the closed fungible-type-group-v1 contract",
        ));
    }
    if !entry.params.is_empty()
        || !entry.consume_set.is_empty()
        || !entry.read_refs.is_empty()
        || !entry.create_set.is_empty()
        || !entry.mutate_set.is_empty()
        || entry.block_count != 1
    {
        return Err(CompileError::without_span(
            "fungible-type-group-v1 entry must be payload-free and must not carry ordinary action lifecycle sets",
        ));
    }
    if entry.ckb_runtime_accesses.len() != 1
        || !entry.ckb_runtime_accesses.iter().any(|access| {
            access.operation == "fungible-type-group-owner-or-conservation"
                && access.syscall == "LOAD_SCRIPT+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA"
                && access.source == "CurrentScript/Input/GroupInput/GroupOutput"
                && access.binding == aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_METADATA_HELPER
        })
    {
        return Err(CompileError::without_span(
            "fungible-type-group-v1 entry must have exactly one complete-group checked conservation runtime access",
        ));
    }
    let Some(type_metadata) = metadata.types.iter().find(|type_metadata| type_metadata.name == contract.type_name) else {
        return Err(CompileError::without_span(format!(
            "fungible-type-group-v1 selected type '{}' is missing from metadata",
            contract.type_name
        )));
    };
    let [field] = type_metadata.fields.as_slice() else {
        return Err(CompileError::without_span("fungible-type-group-v1 selected type must encode exactly one field"));
    };
    if type_metadata.encoded_size != Some(16)
        || field.name != contract.field
        || field.ty != "u128"
        || field.offset != 0
        || field.encoded_size != Some(16)
        || !field.fixed_width
    {
        return Err(CompileError::without_span(
            "fungible-type-group-v1 selected type metadata must encode one little-endian u128 in exactly 16 bytes at offset zero",
        ));
    }
    let proof_origin = format!("invariant:{}", contract.invariant);
    let aggregate_origin = format!("{proof_origin}#aggregate:0");
    let feature = format!("fungible-type-group-v1:owner-or-conservation:{}.{}", contract.type_name, contract.field);
    let relation = format!(
        "owner_authorized || (nonempty_group_inputs && nonempty_group_outputs && checked_u128_sum(group_inputs<{}>.{})==checked_u128_sum(group_outputs<{}>.{}))",
        contract.type_name, contract.field, contract.type_name, contract.field,
    );
    for (origin, category) in [(&proof_origin, "selected-profile-invariant"), (&aggregate_origin, "selected-profile-aggregate")] {
        let plans = metadata.runtime.proof_plan.iter().filter(|plan| &plan.origin == origin).collect::<Vec<_>>();
        if !matches!(plans.as_slice(), [plan] if plan.category == category
            && plan.feature == feature && plan.input_output_relation_checks == [relation.clone()]
            && plan.status == "checked-runtime" && plan.evidence_tier == EvidenceTier::CheckedRuntime
            && plan.on_chain_checked && plan.codegen_coverage_status == "covered"
            && plan.coverage.iter().any(|coverage| coverage == "runtime_helper_checked:fungible::require_type_group_v1"))
        {
            return Err(CompileError::without_span(
                "fungible-type-group-v1 invariant requires exact checked owner-or-conservation profile coverage; unconditional source equality is not this profile's contract",
            ));
        }
    }
    Ok(())
}

type CkbRuntimeAccessFingerprint = (String, String, String, usize, String, String);

fn ckb_runtime_access_fingerprints<'a>(
    accesses: impl Iterator<Item = &'a CkbRuntimeAccessMetadata>,
) -> Vec<CkbRuntimeAccessFingerprint> {
    accesses
        .map(|access| {
            (
                access.operation.clone(),
                access.syscall.clone(),
                access.source.clone(),
                access.index,
                access.binding.clone(),
                serde_json::to_string(&access.provenance).expect("runtime access provenance must serialize"),
            )
        })
        .collect()
}

fn ckb_runtime_access_multiset_difference(
    left: &[CkbRuntimeAccessFingerprint],
    right: &[CkbRuntimeAccessFingerprint],
) -> Vec<CkbRuntimeAccessFingerprint> {
    let mut right_remaining = right.to_vec();
    let mut diff = Vec::new();
    for item in left {
        if let Some(position) = right_remaining.iter().position(|candidate| candidate == item) {
            right_remaining.remove(position);
        } else {
            diff.push(item.clone());
        }
    }
    diff
}

fn validate_ckb_runtime_access_list(scope: &str, name: &str, accesses: &[CkbRuntimeAccessMetadata]) -> Result<()> {
    for (index, access) in accesses.iter().enumerate() {
        let prefix = format!("metadata {} '{}' ckb_runtime_accesses[{}]", scope, name, index);
        for (field, value) in [
            ("operation", access.operation.as_str()),
            ("syscall", access.syscall.as_str()),
            ("source", access.source.as_str()),
            ("binding", access.binding.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(CompileError::without_span(format!("{}.{} must not be empty", prefix, field)));
            }
        }
        if !is_known_ckb_runtime_source(&access.source) {
            return Err(CompileError::without_span(format!(
                "{}.source '{}' is not a known CKB runtime source",
                prefix, access.source
            )));
        }
        if !is_known_ckb_runtime_syscall(&access.syscall) {
            return Err(CompileError::without_span(format!(
                "{}.syscall '{}' is not a known CKB runtime syscall surface",
                prefix, access.syscall
            )));
        }
        if !ckb_runtime_syscall_allows_source(&access.syscall, &access.source) {
            return Err(CompileError::without_span(format!(
                "{} syscall '{}' does not allow source '{}'",
                prefix, access.syscall, access.source
            )));
        }
        validate_ckb_runtime_access_provenance(&prefix, &access.source, Some(access.index), &access.provenance)?;
        validate_canonical_transaction_hash_runtime_access(&prefix, access)?;
        validate_canonical_sighash_zero_lock_runtime_access(&prefix, access)?;
        validate_canonical_script_hash_runtime_access(&prefix, access)?;
    }
    Ok(())
}

fn validate_canonical_transaction_hash_runtime_access(prefix: &str, access: &CkbRuntimeAccessMetadata) -> Result<()> {
    let identifies_transaction_hash =
        access.operation == "transaction-hash" || access.syscall == "LOAD_TX_HASH" || access.binding == "ckb::transaction_hash";
    if !identifies_transaction_hash {
        return Ok(());
    }
    if access.operation != "transaction-hash"
        || access.syscall != "LOAD_TX_HASH"
        || access.source != "Transaction"
        || access.index != 0
        || access.binding != "ckb::transaction_hash"
        || access.provenance.source.resolved_source != "Transaction"
        || access.provenance.source.origin != "implicit-lowering"
        || access.provenance.source.binding.is_some()
        || access.provenance.index != runtime_scalar_not_applicable()
        || access.provenance.range != runtime_range_fixed(32)
    {
        return Err(CompileError::without_span(format!("{} does not match the canonical 32-byte LOAD_TX_HASH contract", prefix)));
    }
    Ok(())
}

fn validate_canonical_sighash_zero_lock_runtime_access(prefix: &str, access: &CkbRuntimeAccessMetadata) -> Result<()> {
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
        return Err(CompileError::without_span(format!(
            "{} does not match the bounded CKB sighash-all zero-lock runtime contract",
            prefix
        )));
    }
    Ok(())
}

fn validate_canonical_script_hash_runtime_access(prefix: &str, access: &CkbRuntimeAccessMetadata) -> Result<()> {
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
        || access.provenance.index != runtime_scalar_not_applicable()
        || range.kind != "fixed-width"
        || range.offset != runtime_scalar_not_applicable()
        || range.length.kind != "static"
        || range.length.value.is_none_or(|width| !(53..=53 + CKB_SCRIPT_HASH_MAX_ARGS_BYTES as u64).contains(&width))
        || range.length.value != range.length.max_inclusive
        || range.length.binding.is_some()
    {
        return Err(CompileError::without_span(format!(
            "{} does not match the canonical bounded Molecule Script hash contract",
            prefix
        )));
    }
    Ok(())
}

pub(super) fn validate_ckb_signing_message_domain_metadata(metadata: &CompileMetadata) -> Result<()> {
    // WASM metadata has already rejected native records at the entry boundary.
    let browser_summary =
        cfg!(feature = "wasm") || (metadata.runtime.vm_target.is_empty() && metadata.typed_semantics.schema.is_empty());
    let accesses = metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .filter(|access| access.operation == "sighash-all-zero-lock-v1")
        .collect::<Vec<_>>();
    if !browser_summary && accesses.len() != metadata.runtime.signing_message_domains.len() {
        return Err(CompileError::without_span(
            "metadata runtime.signing_message_domains do not match bounded sighash runtime accesses",
        ));
    }
    let mut semantic_calls = if browser_summary {
        Vec::new()
    } else {
        metadata
            .typed_semantics
            .entries
            .iter()
            .flat_map(|entry| {
                entry.blocks.iter().flat_map(move |block| {
                    block.operations.iter().filter_map(move |operation| {
                        let call = operation.call.as_ref()?;
                        (call.contract == "versioned-runtime-helper" && call.target == "__ckb_sighash_all_zero_lock").then(|| {
                            let bounds = operation
                                .operands
                                .iter()
                                .map(|operand| match operand.constant.as_ref() {
                                    Some(cellscript_artifact_checker::TypedSemanticConstant::U64(value)) => value.parse::<u64>().ok(),
                                    _ => None,
                                })
                                .collect::<Option<Vec<_>>>();
                            (entry.kind.clone(), entry.name.clone(), bounds)
                        })
                    })
                })
            })
            .collect::<Vec<_>>()
    };
    if !browser_summary
        && (semantic_calls.len() != metadata.runtime.signing_message_domains.len()
            || semantic_calls.iter().any(|(_, _, bounds)| bounds.as_ref().is_none_or(|values| values.len() != 4)))
    {
        return Err(CompileError::without_span("metadata runtime.signing_message_domains do not match typed bounded sighash calls"));
    }
    let mut unmatched_accesses = accesses;
    for (index, domain) in metadata.runtime.signing_message_domains.iter().enumerate() {
        let prefix = format!("metadata runtime.signing_message_domains[{index}]");
        let scope_exists = browser_summary
            || match domain.scope_kind.as_str() {
                "action" => metadata.actions.iter().any(|entry| entry.name == domain.scope_name),
                "function" => metadata.functions.iter().any(|entry| entry.name == domain.scope_name),
                "lock" => metadata.locks.iter().any(|entry| entry.name == domain.scope_name),
                _ => false,
            };
        if !scope_exists
            || domain.contract != CKB_SIGHASH_ALL_ZERO_LOCK_CONTRACT
            || domain.binding != "env::sighash_all_zero_lock"
            || domain.digest_type != "SighashAllDigest"
            || domain.hash_algorithm != "ckb-default-hash-blake2b-256"
            || domain.transaction_hash_source != "LOAD_TX_HASH-exact-32"
            || domain.group_scope != "current-input-script-group"
            || domain.first_witness_source != "GroupInput[0]-WitnessArgs"
            || domain.first_witness_lock_transform != "replace-entire-lock-payload-with-equal-length-zero-bytes"
            || domain.witness_length_prefix != "u64-little-endian-byte-length"
            || domain.later_group_witness_order != "GroupInput[1..]-script-group-order-if-present"
            || domain.extra_witness_source != "Input[input_count..witness_count]-transaction-order"
            || !(1..=64).contains(&domain.max_group_inputs)
            || !(1..=256).contains(&domain.max_inputs)
            || domain.max_group_inputs > domain.max_inputs
            || domain.max_extra_witnesses > 64
            || !(1..=65_536).contains(&domain.max_witness_bytes)
            || domain.runtime_helper != "__ckb_sighash_all_zero_lock"
            || domain.evidence_tier != EvidenceTier::CheckedRuntime
        {
            return Err(CompileError::without_span(format!("{prefix} does not match the canonical zero-lock signing domain")));
        }
        if browser_summary {
            continue;
        }
        let Some(position) = unmatched_accesses.iter().position(|access| {
            access.provenance.index.max_inclusive == Some(domain.max_group_inputs - 1)
                && access.provenance.range.length.max_inclusive == Some(domain.max_witness_bytes)
        }) else {
            return Err(CompileError::without_span(format!("{prefix} has no runtime access with matching group and witness bounds")));
        };
        unmatched_accesses.remove(position);
        let domain_bounds = [domain.max_group_inputs, domain.max_inputs, domain.max_extra_witnesses, domain.max_witness_bytes];
        let Some(position) = semantic_calls.iter().position(|(kind, name, bounds)| {
            let kind = if kind == "helper" { "function" } else { kind.as_str() };
            kind == domain.scope_kind && name == &domain.scope_name && bounds.as_deref() == Some(domain_bounds.as_slice())
        }) else {
            return Err(CompileError::without_span(format!(
                "{prefix} has no typed bounded sighash call with matching scope and bounds"
            )));
        };
        semantic_calls.remove(position);
    }
    Ok(())
}

pub(super) fn validate_ckb_runtime_access_provenance(
    prefix: &str,
    declared_source: &str,
    compatibility_index: Option<usize>,
    provenance: &CkbRuntimeAccessProvenanceMetadata,
) -> Result<()> {
    if provenance.contract != CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT {
        return Err(CompileError::without_span(format!(
            "{}.provenance.contract '{}' does not match current contract '{}'",
            prefix, provenance.contract, CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT
        )));
    }
    if provenance.source.resolved_source.trim().is_empty() {
        return Err(CompileError::without_span(format!("{}.provenance.source.resolved_source must not be empty", prefix)));
    }
    if !is_known_ckb_runtime_source(&provenance.source.resolved_source) {
        return Err(CompileError::without_span(format!(
            "{}.provenance.source.resolved_source '{}' is not a known CKB runtime source",
            prefix, provenance.source.resolved_source
        )));
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
        return Err(CompileError::without_span(format!(
            "{}.provenance.source.origin '{}' is not admitted by the runtime access provenance contract",
            prefix, provenance.source.origin
        )));
    }
    match provenance.source.origin.as_str() {
        "inherited-source-view" => {
            if provenance.source.binding.as_deref().is_none_or(str::is_empty) {
                return Err(CompileError::without_span(format!(
                    "{}.provenance.source.binding is required for an inherited source view",
                    prefix
                )));
            }
        }
        _ if provenance.source.binding.is_some() => {
            return Err(CompileError::without_span(format!(
                "{}.provenance.source.binding is only valid for an inherited source view",
                prefix
            )));
        }
        _ => {}
    }
    validate_ckb_runtime_scalar_provenance(&format!("{}.provenance.index", prefix), &provenance.index)?;
    if matches!(provenance.source.origin.as_str(), "explicit-source-view" | "inherited-source-view")
        && (!matches!(provenance.index.kind.as_str(), "static" | "dynamic")
            || provenance.index.max_inclusive != Some(u64::from(u32::MAX)))
    {
        return Err(CompileError::without_span(format!(
            "{}.provenance.index must bind a static or dynamic 32-bit source-view index",
            prefix
        )));
    }
    if let Some(compatibility_index) = compatibility_index {
        let expected = provenance.index.value.and_then(|value| usize::try_from(value).ok()).unwrap_or_default();
        if compatibility_index != expected {
            return Err(CompileError::without_span(format!(
                "{}.index {} does not match the structured provenance compatibility projection {}",
                prefix, compatibility_index, expected
            )));
        }
    }
    validate_ckb_runtime_range_provenance(&format!("{}.provenance.range", prefix), &provenance.range)?;
    if declared_source == "SourceView"
        && !matches!(
            provenance.source.resolved_source.as_str(),
            "Input" | "Output" | "CellDep" | "GroupInput" | "GroupOutput" | "SourceView"
        )
    {
        return Err(CompileError::without_span(format!(
            "{}.provenance.source.resolved_source '{}' is incompatible with declared SourceView",
            prefix, provenance.source.resolved_source
        )));
    }
    Ok(())
}

fn validate_ckb_runtime_scalar_provenance(prefix: &str, value: &CkbRuntimeScalarProvenanceMetadata) -> Result<()> {
    match value.kind.as_str() {
        "not-applicable" => {
            if value.value.is_some() || value.binding.is_some() || value.max_inclusive.is_some() {
                return Err(CompileError::without_span(format!(
                    "{} not-applicable value must not carry value, binding, or bound",
                    prefix
                )));
            }
        }
        "static" | "metadata-ordinal" => {
            let Some(static_value) = value.value else {
                return Err(CompileError::without_span(format!("{} {} value requires a static value", prefix, value.kind)));
            };
            if value.binding.is_some() {
                return Err(CompileError::without_span(format!("{} {} value must not carry a binding", prefix, value.kind)));
            }
            if value.max_inclusive.is_some_and(|maximum| static_value > maximum) {
                return Err(CompileError::without_span(format!(
                    "{} static value {} exceeds max_inclusive {:?}",
                    prefix, static_value, value.max_inclusive
                )));
            }
        }
        "dynamic" => {
            if value.value.is_some() || value.binding.as_deref().is_none_or(str::is_empty) {
                return Err(CompileError::without_span(format!(
                    "{} dynamic value requires a non-empty binding and no static value",
                    prefix
                )));
            }
        }
        "bounded-scan" => {
            if value.value.is_some() || value.binding.is_some() || value.max_inclusive.is_none() {
                return Err(CompileError::without_span(format!("{} bounded scan requires only max_inclusive", prefix)));
            }
        }
        _ => {
            return Err(CompileError::without_span(format!(
                "{}.kind '{}' is not admitted by the runtime access provenance contract",
                prefix, value.kind
            )));
        }
    }
    Ok(())
}

fn validate_ckb_runtime_range_provenance(prefix: &str, range: &CkbRuntimeRangeProvenanceMetadata) -> Result<()> {
    validate_ckb_runtime_scalar_provenance(&format!("{}.offset", prefix), &range.offset)?;
    validate_ckb_runtime_scalar_provenance(&format!("{}.length", prefix), &range.length)?;
    match range.kind.as_str() {
        "not-applicable" | "whole-value" => {
            if range.offset.kind != "not-applicable" || range.length.kind != "not-applicable" {
                return Err(CompileError::without_span(format!(
                    "{} {} range must not carry offset or length provenance",
                    prefix, range.kind
                )));
            }
        }
        "fixed-width" => {
            if range.offset.kind != "not-applicable"
                || range.length.kind != "static"
                || range.length.value.is_none_or(|length| length == 0)
            {
                return Err(CompileError::without_span(format!(
                    "{} fixed-width range requires a positive static length and no offset",
                    prefix
                )));
            }
        }
        "bounded-range" => {
            if !matches!(range.offset.kind.as_str(), "static" | "dynamic")
                || !matches!(range.length.kind.as_str(), "static" | "dynamic")
            {
                return Err(CompileError::without_span(format!(
                    "{} bounded range requires static or dynamic offset and length provenance",
                    prefix
                )));
            }
        }
        _ => {
            return Err(CompileError::without_span(format!(
                "{}.kind '{}' is not admitted by the runtime access provenance contract",
                prefix, range.kind
            )));
        }
    }
    Ok(())
}

fn is_known_ckb_runtime_source(source: &str) -> bool {
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

fn is_known_ckb_runtime_syscall(syscall: &str) -> bool {
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

fn ckb_runtime_syscall_allows_source(syscall: &str, source: &str) -> bool {
    match syscall {
        "LOAD_TRANSACTION" | "LOAD_TX_HASH" => source == "Transaction",
        "LOAD_CELL" => matches!(source, "Input" | "Output" | "CellDep" | "GroupInput" | "GroupOutput"),
        "LOAD_CELL_BY_FIELD" => matches!(source, "Input" | "Output" | "GroupInput" | "GroupOutput" | "CellDep" | "SourceView"),
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
        "CKB_SINCE_ENCODING" => source == "Expression",
        "CKB_EPOCH_ARITHMETIC" => source == "Expression",
        "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA" => source == "SourceView",
        "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS" => source == "SourceView",
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

pub(super) fn validate_ckb_script_group_metadata(metadata: &CompileMetadata) -> Result<()> {
    let target_profile = TargetProfile::from_name(&metadata.target_profile.name)?;
    for action in &metadata.actions {
        validate_ckb_entry_script_group_metadata(
            "action",
            &action.name,
            &action.ckb_runtime_accesses,
            action.ckb_script_group.as_ref(),
            target_profile,
        )?;
    }
    for lock in &metadata.locks {
        validate_ckb_entry_script_group_metadata(
            "lock",
            &lock.name,
            &lock.ckb_runtime_accesses,
            lock.ckb_script_group.as_ref(),
            target_profile,
        )?;
    }
    Ok(())
}

fn validate_ckb_entry_script_group_metadata(
    entry_kind: &str,
    name: &str,
    accesses: &[CkbRuntimeAccessMetadata],
    actual: Option<&CkbScriptGroupMetadata>,
    target_profile: TargetProfile,
) -> Result<()> {
    let expected = ckb_script_group_metadata(entry_kind, accesses, target_profile);
    match (actual, expected) {
        (None, None) => Ok(()),
        (Some(_), None) => Err(CompileError::without_span(format!(
            "metadata {} '{}' has ckb_script_group outside the ckb target profile",
            entry_kind, name
        ))),
        (None, Some(_)) => {
            Err(CompileError::without_span(format!("metadata {} '{}' is missing ckb_script_group metadata", entry_kind, name)))
        }
        (Some(actual), Some(expected)) if actual == &expected => Ok(()),
        (Some(actual), Some(expected)) => Err(CompileError::without_span(format!(
            "metadata {} '{}' ckb_script_group does not match runtime access metadata; expected={:?}; actual={:?}",
            entry_kind, name, expected, actual
        ))),
    }
}

pub(super) fn validate_ckb_script_reference_metadata(metadata: &CompileMetadata) -> Result<()> {
    let profile_is_ckb = TargetProfile::from_name(&metadata.target_profile.name)?.is_ckb();
    let Some(ckb_constraints) = metadata.constraints.ckb.as_ref() else {
        return Ok(());
    };

    for (index, reference) in ckb_constraints.script_references.iter().enumerate() {
        let prefix = format!("metadata constraints.ckb.script_references[{}]", index);
        for (field, value) in [
            ("scope", reference.scope.as_str()),
            ("purpose", reference.purpose.as_str()),
            ("name", reference.name.as_str()),
            ("dep_source", reference.dep_source.as_str()),
            ("profile", reference.profile.as_str()),
            ("status", reference.status.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(CompileError::without_span(format!("{}.{} must not be empty", prefix, field)));
            }
        }
        if !profile_is_ckb {
            return Err(CompileError::without_span(format!("{} is present outside the ckb target profile", prefix)));
        }
        if reference.profile != "ckb" {
            return Err(CompileError::without_span(format!(
                "{}.profile '{}' does not match expected 'ckb'",
                prefix, reference.profile
            )));
        }

        match reference.purpose.as_str() {
            "type-id-create-output" => {
                let Some(code_hash) = reference.code_hash.as_deref() else {
                    return Err(CompileError::without_span(format!("{}.code_hash is required for TYPE_ID script references", prefix)));
                };
                if !is_canonical_hash_hex(code_hash) {
                    return Err(CompileError::without_span(format!(
                        "{}.code_hash '{}' is not a canonical 32-byte lowercase hex hash",
                        prefix, code_hash
                    )));
                }
                let expected_code_hash = hex_encode(&CKB_TYPE_ID_CODE_HASH);
                if code_hash != expected_code_hash {
                    return Err(CompileError::without_span(format!(
                        "{}.code_hash '{}' does not match expected TYPE_ID code hash '{}'",
                        prefix, code_hash, expected_code_hash
                    )));
                }
                let Some(hash_type) = reference.hash_type.as_deref() else {
                    return Err(CompileError::without_span(format!("{}.hash_type is required for TYPE_ID script references", prefix)));
                };
                validate_ckb_hash_type(hash_type)
                    .map_err(|err| CompileError::without_span(format!("{}.hash_type is invalid: {}", prefix, err.message)))?;
                if hash_type != CKB_TYPE_ID_HASH_TYPE {
                    return Err(CompileError::without_span(format!(
                        "{}.hash_type '{}' does not match expected '{}'",
                        prefix, hash_type, CKB_TYPE_ID_HASH_TYPE
                    )));
                }
                if reference.args.as_deref() != Some(CKB_TYPE_ID_ARGS_SOURCE) {
                    return Err(CompileError::without_span(format!(
                        "{}.args {:?} does not match expected '{}'",
                        prefix, reference.args, CKB_TYPE_ID_ARGS_SOURCE
                    )));
                }
                if reference.dep_source != CKB_TYPE_ID_OUTPUT_SOURCE {
                    return Err(CompileError::without_span(format!(
                        "{}.dep_source '{}' does not match expected '{}'",
                        prefix, reference.dep_source, CKB_TYPE_ID_OUTPUT_SOURCE
                    )));
                }
                if reference.status != "builder-must-install-type-id-script" {
                    return Err(CompileError::without_span(format!(
                        "{}.status '{}' does not match expected 'builder-must-install-type-id-script'",
                        prefix, reference.status
                    )));
                }
            }
            "spawn-target" => {
                if reference.code_hash.is_some() || reference.hash_type.is_some() || reference.args.is_some() {
                    return Err(CompileError::without_span(format!(
                        "{} must leave code_hash/hash_type/args unset until the builder resolves the spawn target",
                        prefix
                    )));
                }
                if reference.dep_source != "CellDep-or-DepGroup" {
                    return Err(CompileError::without_span(format!(
                        "{}.dep_source '{}' does not match expected 'CellDep-or-DepGroup'",
                        prefix, reference.dep_source
                    )));
                }
                if reference.status != "runtime-required-builder-resolved" {
                    return Err(CompileError::without_span(format!(
                        "{}.status '{}' does not match expected 'runtime-required-builder-resolved'",
                        prefix, reference.status
                    )));
                }
            }
            "read-ref-cell-dep" => {
                if reference.code_hash.is_some() || reference.hash_type.is_some() || reference.args.is_some() {
                    return Err(CompileError::without_span(format!(
                        "{} must leave code_hash/hash_type/args unset for runtime CellDep reads",
                        prefix
                    )));
                }
                let Some(index_text) = reference.dep_source.strip_prefix("CellDep#") else {
                    return Err(CompileError::without_span(format!(
                        "{}.dep_source '{}' must use CellDep#<index>",
                        prefix, reference.dep_source
                    )));
                };
                if index_text.parse::<usize>().is_err() {
                    return Err(CompileError::without_span(format!(
                        "{}.dep_source '{}' has an invalid CellDep index",
                        prefix, reference.dep_source
                    )));
                }
                if reference.status != "checked-runtime-load-cell-dep" {
                    return Err(CompileError::without_span(format!(
                        "{}.status '{}' does not match expected 'checked-runtime-load-cell-dep'",
                        prefix, reference.status
                    )));
                }
            }
            "trusted-external-exec" | "trusted-external-spawn-wait" => {
                let Some(code_hash) = reference.code_hash.as_deref() else {
                    return Err(CompileError::without_span(format!(
                        "{}.code_hash is required for trusted external verifier references",
                        prefix
                    )));
                };
                if !is_canonical_hash_hex(code_hash) || reference.hash_type.as_deref() != Some("data") {
                    return Err(CompileError::without_span(format!("{} must bind a canonical data code hash", prefix)));
                }
                if reference.args.is_some()
                    || reference.dep_source != "CellDep selected at runtime and bound by DATA_HASH"
                    || reference.status != "trusted-external-identity-bound"
                {
                    return Err(CompileError::without_span(format!(
                        "{} does not carry the canonical trusted external CellDep binding contract",
                        prefix
                    )));
                }
            }
            other => {
                return Err(CompileError::without_span(format!(
                    "{}.purpose '{}' is not a known CKB script-reference purpose",
                    prefix, other
                )));
            }
        }
    }

    let mut expected_refs = ckb_script_reference_fingerprints(ckb_script_reference_metadata(metadata).iter());
    let mut actual_refs = ckb_script_reference_fingerprints(ckb_constraints.script_references.iter());
    expected_refs.sort();
    actual_refs.sort();
    if actual_refs != expected_refs {
        let missing_refs = ckb_script_reference_multiset_difference(&expected_refs, &actual_refs);
        let extra_refs = ckb_script_reference_multiset_difference(&actual_refs, &expected_refs);
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.script_references do not match the compiled CKB runtime metadata; missing={:?}; extra={:?}",
            missing_refs, extra_refs
        )));
    }

    Ok(())
}

type CkbScriptReferenceFingerprint = (String, String, String, Option<String>, Option<String>, Option<String>, String, String, String);

fn ckb_script_reference_fingerprints<'a>(
    refs: impl Iterator<Item = &'a CkbScriptReferenceMetadata>,
) -> Vec<CkbScriptReferenceFingerprint> {
    refs.map(|reference| {
        (
            reference.scope.clone(),
            reference.purpose.clone(),
            reference.name.clone(),
            reference.code_hash.clone(),
            reference.hash_type.clone(),
            reference.args.clone(),
            reference.dep_source.clone(),
            reference.profile.clone(),
            reference.status.clone(),
        )
    })
    .collect()
}

fn ckb_script_reference_multiset_difference(
    left: &[CkbScriptReferenceFingerprint],
    right: &[CkbScriptReferenceFingerprint],
) -> Vec<CkbScriptReferenceFingerprint> {
    let mut right_remaining = right.to_vec();
    let mut diff = Vec::new();
    for item in left {
        if let Some(position) = right_remaining.iter().position(|candidate| candidate == item) {
            right_remaining.remove(position);
        } else {
            diff.push(item.clone());
        }
    }
    diff
}

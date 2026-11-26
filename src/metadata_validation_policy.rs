use super::ckb_metadata_validation::{validate_ckb_output_data_create_set_metadata, validate_ckb_type_id_create_set_metadata};
use super::error::{CompileError, Result};
use super::{
    ckb_blake2b256, hex_encode, is_canonical_hash_hex, CkbCapacityFloorMetadata, CkbTypeIdMetadata, CompileMetadata, TargetProfile,
    TypeMetadata, CKB_TYPE_ID_ABI, CKB_TYPE_ID_ARGS_SOURCE, CKB_TYPE_ID_BUILDER, CKB_TYPE_ID_CODE_HASH, CKB_TYPE_ID_GROUP_RULE,
    CKB_TYPE_ID_HASH_TYPE, CKB_TYPE_ID_VERIFIER,
};
use std::collections::HashMap;

pub(super) fn validate_type_identity_metadata(metadata: &CompileMetadata) -> Result<()> {
    let mut seen_type_ids = HashMap::new();
    // Aliases may retain several lowered names for one defining declaration.
    // Only explicit owner/catalog evidence can distinguish that from two
    // declarations reusing a Type ID; equal layouts or name suffixes cannot.
    let type_id_owners = declared_type_id_owners(metadata);
    for ty in &metadata.types {
        match (&ty.type_id, &ty.type_id_hash) {
            (None, None) => {}
            (Some(type_id), Some(hash)) => {
                if type_id.is_empty() {
                    return Err(CompileError::without_span(format!("metadata type '{}' has an empty type_id", ty.name)));
                }
                if type_id.chars().any(char::is_control) {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' has invalid control characters in type_id",
                        ty.name
                    )));
                }
                if !is_canonical_hash_hex(hash) {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' has invalid type_id_hash '{}'; expected 64 lowercase hex characters",
                        ty.name, hash
                    )));
                }
                let expected = hex_encode(&ckb_blake2b256(type_id.as_bytes()));
                if hash != &expected {
                    return Err(CompileError::without_span(format!(
                        "metadata type '{}' type_id_hash '{}' does not match type_id '{}'",
                        ty.name, hash, expected
                    )));
                }
                if let Some(previous) = seen_type_ids.insert(type_id.clone(), ty.name.clone())
                    && !type_id_owners
                        .get(previous.as_str())
                        .and_then(Option::as_ref)
                        .zip(type_id_owners.get(ty.name.as_str()).and_then(Option::as_ref))
                        .is_some_and(|(left, right)| left == right && left.2 == type_id.as_str())
                {
                    return Err(CompileError::without_span(format!(
                        "metadata type_id '{}' is declared by both '{}' and '{}'",
                        type_id, previous, ty.name
                    )));
                }
            }
            (Some(_), None) => {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' has type_id but is missing type_id_hash",
                    ty.name
                )));
            }
            (None, Some(_)) => {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' has type_id_hash but is missing type_id",
                    ty.name
                )));
            }
        }

        if let Some(ckb_type_id) = &ty.ckb_type_id {
            validate_ckb_type_id_metadata(metadata, ty, ckb_type_id)?;
        }
    }
    Ok(())
}

fn declared_type_id_owners(metadata: &CompileMetadata) -> HashMap<&str, Option<(&str, &str, &str)>> {
    let Some(catalog) = &metadata.typed_semantics.nominal_declarations else { return HashMap::new() };
    if catalog.schema != cellscript_artifact_checker::NOMINAL_DECLARATION_CATALOG_SCHEMA {
        return HashMap::new();
    }
    let mut declarations = HashMap::new();
    for declaration in &catalog.declarations {
        declarations
            .entry((declaration.module.as_str(), declaration.name.as_str()))
            .and_modify(|value| *value = None)
            .or_insert(Some(declaration));
    }
    let mut owners = HashMap::new();
    for binding in &catalog.layout_bindings {
        let identity = declarations
            .get(&(binding.owner_module.as_str(), binding.source_name.as_str()))
            .and_then(Option::as_ref)
            .and_then(|declaration| declaration.type_identity.as_deref())
            .map(|identity| (binding.owner_module.as_str(), binding.source_name.as_str(), identity));
        owners.entry(binding.lowered_name.as_str()).and_modify(|value| *value = None).or_insert(identity);
    }
    owners
}

pub(super) fn validate_capacity_floor_metadata(metadata: &CompileMetadata) -> Result<()> {
    for ty in &metadata.types {
        match (ty.capacity_floor_shannons, ty.capacity_floor_source.as_deref()) {
            (Some(0), _) => {
                return Err(CompileError::without_span(format!("metadata type '{}' has zero capacity_floor_shannons", ty.name)));
            }
            (Some(_), Some("dsl-with_capacity_floor")) => {}
            (Some(_), Some(source)) => {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' has unsupported capacity_floor_source '{}'",
                    ty.name, source
                )));
            }
            (Some(_), None) => {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' has capacity_floor_shannons but is missing capacity_floor_source",
                    ty.name
                )));
            }
            (None, Some(source)) => {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' has capacity_floor_source '{}' but no capacity_floor_shannons",
                    ty.name, source
                )));
            }
            (None, None) => {}
        }
    }
    let expected_floors = expected_ckb_capacity_floor_metadata(metadata);
    let Some(ckb_constraints) = metadata.constraints.ckb.as_ref() else {
        // Metadata-only compilation has no artifact constraints summary. Type
        // declarations above still validate their floor and source; require the
        // redundant CKB summary once an artifact has actually been produced.
        if expected_floors.is_empty() || (metadata.artifact_hash.is_none() && metadata.artifact_size_bytes.is_none()) {
            return Ok(());
        }
        return Err(CompileError::without_span(
            "metadata has type-level capacity floors but is missing constraints.ckb.declared_capacity_floors",
        ));
    };
    let mut expected = ckb_capacity_floor_fingerprints(expected_floors.iter());
    let mut actual = ckb_capacity_floor_fingerprints(ckb_constraints.declared_capacity_floors.iter());
    expected.sort();
    actual.sort();
    if actual != expected {
        let missing = ckb_capacity_floor_multiset_difference(&expected, &actual);
        let extra = ckb_capacity_floor_multiset_difference(&actual, &expected);
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.declared_capacity_floors do not match type-level capacity floors; missing={:?}; extra={:?}",
            missing, extra
        )));
    }
    Ok(())
}

fn expected_ckb_capacity_floor_metadata(metadata: &CompileMetadata) -> Vec<CkbCapacityFloorMetadata> {
    metadata
        .types
        .iter()
        .filter_map(|ty| {
            ty.capacity_floor_shannons.map(|shannons| CkbCapacityFloorMetadata {
                type_name: ty.name.clone(),
                shannons,
                source: ty.capacity_floor_source.clone().unwrap_or_else(|| "dsl-with_capacity_floor".to_string()),
                status: "builder-must-preserve-output-capacity-at-or-above-floor".to_string(),
            })
        })
        .collect()
}

type CkbCapacityFloorFingerprint = (String, u64, String, String);

fn ckb_capacity_floor_fingerprints<'a>(
    floors: impl Iterator<Item = &'a CkbCapacityFloorMetadata>,
) -> Vec<CkbCapacityFloorFingerprint> {
    floors.map(|floor| (floor.type_name.clone(), floor.shannons, floor.source.clone(), floor.status.clone())).collect()
}

fn ckb_capacity_floor_multiset_difference(
    left: &[CkbCapacityFloorFingerprint],
    right: &[CkbCapacityFloorFingerprint],
) -> Vec<CkbCapacityFloorFingerprint> {
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

pub(super) fn validate_ckb_constraints_summary_metadata(metadata: &CompileMetadata) -> Result<()> {
    let profile_is_ckb = TargetProfile::from_name(&metadata.target_profile.name)?.is_ckb();
    let Some(ckb_constraints) = metadata.constraints.ckb.as_ref() else {
        if profile_is_ckb && (metadata.artifact_hash.is_some() || metadata.artifact_size_bytes.is_some()) {
            return Err(CompileError::without_span("metadata is missing constraints.ckb for the ckb target profile"));
        }
        return Ok(());
    };

    let created_output_count = metadata
        .actions
        .iter()
        .map(|action| action.create_set.len())
        .chain(metadata.locks.iter().map(|lock| lock.create_set.len()))
        .sum::<usize>();
    let mutated_output_count = metadata
        .actions
        .iter()
        .map(|action| action.mutate_set.len())
        .chain(metadata.locks.iter().map(|lock| lock.mutate_set.len()))
        .sum::<usize>();
    let capacity_planning_required = created_output_count > 0 || mutated_output_count > 0;
    let uses_input_since = metadata.runtime.ckb_runtime_features.iter().any(|feature| feature == "ckb-input-since");
    let uses_header_epoch = metadata.runtime.ckb_runtime_features.iter().any(|feature| feature.starts_with("ckb-header-epoch-"));

    validate_ckb_summary_value("created_output_count", ckb_constraints.created_output_count, created_output_count)?;
    validate_ckb_summary_value("mutated_output_count", ckb_constraints.mutated_output_count, mutated_output_count)?;
    validate_ckb_summary_value(
        "transaction_runtime_input_requirement_count",
        ckb_constraints.transaction_runtime_input_requirement_count,
        metadata.runtime.transaction_runtime_input_requirements.len(),
    )?;
    validate_ckb_summary_bool("uses_input_since", ckb_constraints.uses_input_since, uses_input_since)?;
    validate_ckb_summary_bool("uses_header_epoch", ckb_constraints.uses_header_epoch, uses_header_epoch)?;
    validate_ckb_summary_bool("capacity_planning_required", ckb_constraints.capacity_planning_required, capacity_planning_required)?;
    validate_ckb_summary_bool(
        "occupied_capacity_measurement_required",
        ckb_constraints.occupied_capacity_measurement_required,
        capacity_planning_required,
    )?;

    let mut expected_features = metadata.runtime.ckb_runtime_features.clone();
    let mut actual_features = ckb_constraints.ckb_runtime_features.clone();
    expected_features.sort();
    actual_features.sort();
    if actual_features != expected_features {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.ckb_runtime_features {:?} do not match runtime.ckb_runtime_features {:?}",
            actual_features, expected_features
        )));
    }

    let expected_capacity_policy_surface = if expected_ckb_capacity_floor_metadata(metadata).is_empty() {
        if capacity_planning_required {
            "builder/runtime-required; declarative-dsl-capacity-not-yet-first-class"
        } else {
            "not-applicable"
        }
    } else {
        "dsl-declared-capacity-floor; builder/runtime-required-for-change-and-measurement"
    };
    if ckb_constraints.capacity_policy_surface != expected_capacity_policy_surface {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.capacity_policy_surface '{}' does not match expected '{}'",
            ckb_constraints.capacity_policy_surface, expected_capacity_policy_surface
        )));
    }

    let evidence = &ckb_constraints.capacity_evidence_contract;
    if !evidence.required {
        return Err(CompileError::without_span("metadata constraints.ckb.capacity_evidence_contract.required must be true"));
    }
    validate_ckb_summary_bool(
        "capacity_evidence_contract.occupied_capacity_measurement_required",
        evidence.occupied_capacity_measurement_required,
        capacity_planning_required,
    )?;
    validate_ckb_summary_bool("capacity_evidence_contract.tx_size_measurement_required", evidence.tx_size_measurement_required, true)?;
    let expected_capacity_status =
        if capacity_planning_required { "builder-occupied-capacity-measurement-required" } else { "code-cell-data-lower-bound" };
    if ckb_constraints.capacity_status != expected_capacity_status {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.capacity_status '{}' does not match expected '{}'",
            ckb_constraints.capacity_status, expected_capacity_status
        )));
    }
    let expected_evidence_status = if capacity_planning_required {
        "builder-must-attach-occupied-capacity-and-tx-size-evidence"
    } else {
        "builder-must-attach-tx-size-evidence-code-cell-lower-bound-available"
    };
    if evidence.status != expected_evidence_status {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.capacity_evidence_contract.status '{}' does not match expected '{}'",
            evidence.status, expected_evidence_status
        )));
    }

    Ok(())
}

fn validate_ckb_summary_value(field: &str, actual: usize, expected: usize) -> Result<()> {
    if actual != expected {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.{} {} does not match expected {}",
            field, actual, expected
        )));
    }
    Ok(())
}

fn validate_ckb_summary_bool(field: &str, actual: bool, expected: bool) -> Result<()> {
    if actual != expected {
        return Err(CompileError::without_span(format!(
            "metadata constraints.ckb.{} {} does not match expected {}",
            field, actual, expected
        )));
    }
    Ok(())
}

fn validate_ckb_type_id_metadata(metadata: &CompileMetadata, ty: &TypeMetadata, ckb_type_id: &CkbTypeIdMetadata) -> Result<()> {
    if !TargetProfile::from_name(&metadata.target_profile.name)?.is_ckb() {
        return Err(CompileError::without_span(format!("metadata type '{}' has ckb_type_id outside the ckb target profile", ty.name)));
    }
    if ty.type_id.is_none() {
        return Err(CompileError::without_span(format!("metadata type '{}' has ckb_type_id but is missing source type_id", ty.name)));
    }
    let expected_code_hash = hex_encode(&CKB_TYPE_ID_CODE_HASH);
    let mismatches = [
        ("abi", ckb_type_id.abi.as_str(), CKB_TYPE_ID_ABI),
        ("script_code_hash", ckb_type_id.script_code_hash.as_str(), expected_code_hash.as_str()),
        ("hash_type", ckb_type_id.hash_type.as_str(), CKB_TYPE_ID_HASH_TYPE),
        ("args_source", ckb_type_id.args_source.as_str(), CKB_TYPE_ID_ARGS_SOURCE),
        ("group_rule", ckb_type_id.group_rule.as_str(), CKB_TYPE_ID_GROUP_RULE),
        ("builder", ckb_type_id.builder.as_str(), CKB_TYPE_ID_BUILDER),
        ("verifier", ckb_type_id.verifier.as_str(), CKB_TYPE_ID_VERIFIER),
    ];
    for (field, actual, expected) in mismatches {
        if actual != expected {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' ckb_type_id.{} '{}' does not match expected '{}'",
                ty.name, field, actual, expected
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_ckb_type_id_output_metadata(metadata: &CompileMetadata) -> Result<()> {
    let types_by_name = metadata.types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<HashMap<_, _>>();
    let profile_is_ckb = TargetProfile::from_name(&metadata.target_profile.name)?.is_ckb();

    for action in &metadata.actions {
        validate_ckb_type_id_create_set_metadata("action", &action.name, &action.create_set, &types_by_name, profile_is_ckb)?;
    }
    for lock in &metadata.locks {
        validate_ckb_type_id_create_set_metadata("lock", &lock.name, &lock.create_set, &types_by_name, profile_is_ckb)?;
    }

    Ok(())
}

pub(super) fn validate_ckb_output_data_binding_metadata(metadata: &CompileMetadata) -> Result<()> {
    let profile_is_ckb = TargetProfile::from_name(&metadata.target_profile.name)?.is_ckb();

    for action in &metadata.actions {
        validate_ckb_output_data_create_set_metadata("action", &action.name, &action.create_set, profile_is_ckb)?;
    }
    for lock in &metadata.locks {
        validate_ckb_output_data_create_set_metadata("lock", &lock.name, &lock.create_set, profile_is_ckb)?;
    }

    Ok(())
}

use super::capability_metadata::capability_registry_metadata;
#[cfg(not(feature = "wasm"))]
use super::ckb_metadata_validation::{
    validate_ckb_runtime_access_metadata, validate_ckb_script_group_metadata, validate_ckb_script_reference_metadata,
    validate_fungible_type_group_entry_metadata,
};
use super::ckb_metadata_validation::{validate_ckb_runtime_access_provenance, validate_ckb_signing_message_domain_metadata};
use super::deployment_metadata::TRUSTED_EXTERNAL_VERIFIER_SCHEMA;
#[cfg(not(feature = "wasm"))]
use super::edition::resolve_compatibility_profile;
use super::error::{CompileError, Result};
#[cfg(not(feature = "wasm"))]
use super::metadata_validation_policy::{
    validate_capacity_floor_metadata, validate_ckb_constraints_summary_metadata, validate_ckb_output_data_binding_metadata,
    validate_ckb_type_id_output_metadata, validate_type_identity_metadata,
};
use super::proof_plan::EvidenceTier;
#[cfg(not(feature = "wasm"))]
use super::schema_metadata_validation::{
    validate_cell_data_codec_manifest_metadata, validate_molecule_schema_manifest_metadata, validate_molecule_schema_metadata,
    validate_template_layout_metadata,
};
#[cfg(not(feature = "wasm"))]
use super::source_provenance::validate_source_metadata;
#[cfg(feature = "wasm")]
use super::validate_browser_record_boundary;
use super::{
    ast, hex_encode, ir, ArtifactFormat, BoundedOutputPlanContractMetadata, CollectionInstantiationMetadata, CompileMetadata,
    TargetProfile, ARTIFACT_METADATA_SCHEMA_VERSION, CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT, CKB_RUNTIME_VIEW_CONTRACT,
    CONSTRAINTS_METADATA_SCHEMA_VERSION, METADATA_SCHEMA_VERSION, SOURCE_METADATA_SCHEMA_VERSION, VERSION,
};
#[cfg(not(feature = "wasm"))]
use super::{edition, generics, interface, MOLECULE_VM_ABI_VERSION};
use std::collections::{BTreeSet, HashSet};

pub fn validate_compile_metadata(metadata: &CompileMetadata, artifact_format: ArtifactFormat) -> Result<()> {
    if metadata.metadata_schema_version != METADATA_SCHEMA_VERSION {
        return Err(CompileError::without_span(format!(
            "unsupported metadata_schema_version {}; expected {}",
            metadata.metadata_schema_version, METADATA_SCHEMA_VERSION
        )));
    }
    validate_metadata_component_schema_version(
        "source_metadata_schema_version",
        metadata.source_metadata_schema_version,
        SOURCE_METADATA_SCHEMA_VERSION,
    )?;
    validate_metadata_component_schema_version(
        "artifact_metadata_schema_version",
        metadata.artifact_metadata_schema_version,
        ARTIFACT_METADATA_SCHEMA_VERSION,
    )?;
    validate_metadata_component_schema_version(
        "constraints_metadata_schema_version",
        metadata.constraints_metadata_schema_version,
        CONSTRAINTS_METADATA_SCHEMA_VERSION,
    )?;
    if metadata.compiler_version != VERSION {
        return Err(CompileError::without_span(format!(
            "metadata compiler_version '{}' does not match current compiler '{}'",
            metadata.compiler_version, VERSION
        )));
    }
    if metadata.runtime.ckb_runtime_view_contract != CKB_RUNTIME_VIEW_CONTRACT {
        return Err(CompileError::without_span(format!(
            "metadata runtime.ckb_runtime_view_contract '{}' does not match current contract '{}'",
            metadata.runtime.ckb_runtime_view_contract, CKB_RUNTIME_VIEW_CONTRACT
        )));
    }
    if metadata.runtime.ckb_runtime_access_provenance_contract != CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT {
        return Err(CompileError::without_span(format!(
            "metadata runtime.ckb_runtime_access_provenance_contract '{}' does not match current contract '{}'",
            metadata.runtime.ckb_runtime_access_provenance_contract, CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT
        )));
    }
    #[cfg(not(feature = "wasm"))]
    {
        validate_ckb_runtime_access_metadata(metadata)?;
        validate_transaction_view_handle_metadata(metadata)?;
    }
    #[cfg(feature = "wasm")]
    validate_browser_record_boundary(metadata)?;
    validate_ckb_signing_message_domain_metadata(metadata)?;
    #[cfg(not(feature = "wasm"))]
    {
        if metadata.public_interface.schema != interface::INTERFACE_SCHEMA
            || metadata.public_interface.version != interface::INTERFACE_SCHEMA_VERSION
            || metadata.public_interface.module != metadata.module
            || metadata.public_interface.edition != metadata.edition.as_str()
        {
            return Err(CompileError::without_span(
                "compile metadata public interface has an invalid schema, version, edition, or module identity",
            ));
        }
        interface::validate(&metadata.public_interface)?;
        if metadata.public_interface.runtime_contract.temporal != interface::temporal_contract(&metadata.target_profile.since_abi) {
            return Err(CompileError::without_span("compile metadata public interface has an invalid typed CKB temporal contract"));
        }
        let expected_interface_hash = interface::hash(&metadata.public_interface);
        if metadata.interface_hash != expected_interface_hash {
            return Err(CompileError::without_span(format!(
                "compile metadata interface_hash '{}' does not match canonical public interface '{}'",
                metadata.interface_hash, expected_interface_hash
            )));
        }
        if metadata.typed_semantics.schema != cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA
            || metadata.typed_semantics.version != cellscript_artifact_checker::TYPED_SEMANTICS_VERSION
            || metadata.typed_semantics.module != metadata.module
            || metadata.typed_semantics.interface_hash != metadata.interface_hash
        {
            return Err(CompileError::without_span(
                "compile metadata typed semantics has an invalid schema, version, module, or interface identity",
            ));
        }
        let expected_typed_semantics_hash = cellscript_artifact_checker::canonical_hash(
            cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA,
            &metadata.typed_semantics,
        )
        .map_err(|error| CompileError::without_span(format!("failed to hash typed semantics: {error}")))?;
        if metadata.typed_semantics_hash != expected_typed_semantics_hash {
            return Err(CompileError::without_span(format!(
                "compile metadata typed_semantics_hash '{}' does not match canonical typed semantics '{}'",
                metadata.typed_semantics_hash, expected_typed_semantics_hash
            )));
        }
        if metadata.runtime.policy_artifact.is_some()
            || matches!(
                metadata.typed_semantics.foundation.entry_contract.dispatch,
                cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(_)
            )
        {
            // Metadata-only consumers need the same declared-tag and builder
            // parameter projection checks as ELF consumers. This bounded
            // check returns no machine or deployment verification claim.
            let projection = serde_json::json!({
                "runtime": { "policy_artifact": &metadata.runtime.policy_artifact },
                "actions": &metadata.actions,
            });
            cellscript_artifact_checker::validate_policy_metadata(&projection, &metadata.typed_semantics)
                .map_err(|error| CompileError::without_span(format!("policy metadata projection failed: {error}")))?;
        }
        let mut previous_generic_identity = None::<&str>;
        for instantiation in &metadata.generic_instantiations {
            if instantiation.schema != "cellscript-generic-instantiation-v2"
                || instantiation.version != 2
                || instantiation.value_ability_registry_version != ast::ValueAbility::REGISTRY_VERSION
                || !instantiation.constraints_verified
                || instantiation.module.is_empty()
                || instantiation.type_arguments.is_empty()
                || !matches!(instantiation.kind.as_str(), "struct" | "enum" | "function")
            {
                return Err(CompileError::without_span(format!(
                    "generic instantiation metadata '{}' has an invalid schema, registry version, kind, or verification state",
                    instantiation.identity
                )));
            }
            let expected_name = generics::encode_wire_specialization(&instantiation.template, &instantiation.type_arguments)?;
            let expected_identity =
                format!("{}::{}<{}>", instantiation.module, instantiation.template, instantiation.type_arguments.join(","));
            if instantiation.concrete_name != expected_name
                || instantiation.identity != expected_identity
                || !instantiation.identity_includes_phantom_arguments
            {
                return Err(CompileError::without_span(format!(
                    "generic instantiation '{}' does not match its canonical monomorphization identity",
                    instantiation.identity
                )));
            }
            if previous_generic_identity.is_some_and(|previous| previous >= instantiation.identity.as_str()) {
                return Err(CompileError::without_span(
                    "generic instantiation metadata must be unique and sorted by canonical identity",
                ));
            }
            previous_generic_identity = Some(&instantiation.identity);
        }
    }
    #[cfg(feature = "wasm")]
    {
        if metadata.artifact_format != artifact_format.display_name() {
            return Err(CompileError::without_span(format!(
                "metadata artifact_format '{}' does not match compiler artifact format '{}'",
                metadata.artifact_format,
                artifact_format.display_name()
            )));
        }
        if metadata.target_profile.name != "ckb" || metadata.target_profile.since_abi != "ckb-since-rfc0017-typed-v1" {
            return Err(CompileError::without_span(
                "browser metadata target profile does not match the bounded CKB temporal contract",
            ));
        }
        Ok(())
    }
    #[cfg(not(feature = "wasm"))]
    {
        let primitive_assurance = (metadata.compatibility_profile.primitive_assurance != "default")
            .then_some(metadata.compatibility_profile.primitive_assurance.as_str());
        let mut expected_compatibility_profile =
            resolve_compatibility_profile(metadata.edition, &metadata.target_profile.name, primitive_assurance);
        if let Some(policy) = &metadata.runtime.policy_artifact {
            policy.validate()?;
            edition::set_entry_compatibility_profile(
                &mut expected_compatibility_profile,
                &policy.payload_abi,
                &policy.placement_abi,
                &policy.placement_field,
                &policy.placement_source,
            );
        }
        if metadata.compatibility_profile != expected_compatibility_profile {
            return Err(CompileError::without_span(format!(
                "metadata compatibility_profile '{}' does not match the resolved compatibility axes for edition {} and target profile '{}'",
                metadata.compatibility_profile.id, metadata.edition, metadata.target_profile.name
            )));
        }
        if !metadata.constraints.status.is_empty()
            && (metadata.constraints.edition != metadata.edition
                || metadata.constraints.compatibility_profile != metadata.compatibility_profile.id)
        {
            return Err(CompileError::without_span(
                "metadata constraints edition/compatibility_profile does not match the top-level compile identity",
            ));
        }

        if metadata.artifact_format != artifact_format.display_name() {
            return Err(CompileError::without_span(format!(
                "metadata artifact_format '{}' does not match compiler artifact format '{}'",
                metadata.artifact_format,
                artifact_format.display_name()
            )));
        }
        validate_target_profile_metadata(metadata, artifact_format)?;

        if metadata.runtime.vm_abi.format != "molecule" {
            return Err(CompileError::without_span(format!(
                "metadata runtime.vm_abi.format must be 'molecule', got '{}'",
                metadata.runtime.vm_abi.format
            )));
        }
        if metadata.runtime.vm_abi.version != MOLECULE_VM_ABI_VERSION {
            return Err(CompileError::without_span(format!(
                "metadata runtime.vm_abi.version must be 0x{:04x}, got 0x{:04x}",
                MOLECULE_VM_ABI_VERSION, metadata.runtime.vm_abi.version
            )));
        }

        let profile = TargetProfile::from_name(&metadata.target_profile.name)?;
        let should_embed_abi = profile.embeds_vm_abi_trailer(artifact_format);
        if metadata.runtime.vm_abi.embedded_in_artifact != should_embed_abi {
            return Err(CompileError::without_span(format!(
                "metadata runtime.vm_abi.embedded_in_artifact must be {} for {} {} artifacts",
                should_embed_abi,
                profile.name(),
                artifact_format.display_name()
            )));
        }

        if metadata.runtime.standalone_runner_compatible && metadata.runtime.ckb_runtime_required {
            return Err(CompileError::without_span(
                "metadata marks artifact as standalone-compatible while CKB runtime features are required",
            ));
        }
        validate_type_identity_metadata(metadata)?;
        validate_capability_metadata(metadata)?;
        validate_payload_enum_layout_metadata(metadata)?;
        validate_protocol_role_metadata(metadata)?;
        validate_type_validity_metadata(metadata)?;
        validate_capacity_floor_metadata(metadata)?;
        validate_ckb_constraints_summary_metadata(metadata)?;
        validate_ckb_output_data_binding_metadata(metadata)?;
        validate_ckb_type_id_output_metadata(metadata)?;
        validate_fungible_type_group_entry_metadata(metadata)?;
        validate_borrow_region_metadata(metadata)?;
        validate_collection_instantiation_metadata(metadata)?;
        validate_ckb_script_group_metadata(metadata)?;
        validate_ckb_script_reference_metadata(metadata)?;
        validate_trusted_external_verifier_metadata(metadata)?;
        validate_molecule_schema_metadata(metadata)?;
        validate_molecule_schema_manifest_metadata(metadata)?;
        validate_cell_data_codec_manifest_metadata(metadata)?;
        validate_template_layout_metadata(metadata)?;
        validate_source_metadata(metadata)?;
        crate::proof_plan::soundness::validate_metadata(metadata, false)?;

        Ok(())
    }
}

fn validate_trusted_external_verifier_metadata(metadata: &CompileMetadata) -> Result<()> {
    if metadata.typed_semantics.trusted_external_verifiers != metadata.runtime.trusted_external_verifiers {
        return Err(CompileError::without_span("typed semantics and runtime metadata disagree on trusted external verifier bindings"));
    }
    if let Some(ckb) = metadata.constraints.ckb.as_ref()
        && ckb.trusted_external_verifiers != metadata.runtime.trusted_external_verifiers
    {
        return Err(CompileError::without_span("CKB constraints and runtime metadata disagree on trusted external verifier bindings"));
    }
    for verifier in &metadata.runtime.trusted_external_verifiers {
        if verifier.schema != TRUSTED_EXTERNAL_VERIFIER_SCHEMA
            || verifier.version != 1
            || verifier.evidence_tier != EvidenceTier::TrustedExternal.as_str()
            || verifier.compiler_proves_internal_semantics
            || verifier.identity_binding != "runtime-load-cell-data-hash-before-delegation-v1"
            || !matches!(
                (verifier.operation.as_str(), verifier.adapter.as_str()),
                ("exec", "u8-args-v1" | "hex4-v1") | ("spawn-wait", "hex4-v1")
            )
        {
            return Err(CompileError::without_span(format!(
                "trusted external verifier '{}' has an invalid compiler evidence record",
                verifier.name
            )));
        }
        let category = if verifier.operation == "exec" { "exec-delegation" } else { "spawn-delegation" };
        let proof_scope = verifier.scope.strip_prefix("helper:").map_or_else(|| verifier.scope.clone(), |name| format!("fn:{name}"));
        if !metadata.runtime.proof_plan.iter().any(|plan| {
            plan.origin == proof_scope
                && plan.category == category
                && plan.evidence_tier == EvidenceTier::TrustedExternal
                && plan.on_chain_checked
                && plan.codegen_coverage_status == "covered"
        }) {
            return Err(CompileError::without_span(format!(
                "trusted external verifier '{}' has no matching checked ProofPlan binding",
                verifier.name
            )));
        }
    }
    Ok(())
}

fn validate_capability_metadata(metadata: &CompileMetadata) -> Result<()> {
    let expected = capability_registry_metadata();
    if metadata.capability_registry != expected {
        return Err(CompileError::without_span(format!(
            "metadata capability_registry does not match the compiler's closed registry: expected capability-set version {} and entailment version {}",
            expected.capability_set_version, expected.entailment_version
        )));
    }
    let canonical = expected.capabilities.iter().cloned().collect::<HashSet<_>>();
    for ty in &metadata.types {
        if ty.capability_set_version != ast::Capability::REGISTRY_VERSION {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' capability_set_version {} does not match registry version {}",
                ty.name,
                ty.capability_set_version,
                ast::Capability::REGISTRY_VERSION
            )));
        }
        let mut seen = HashSet::new();
        for capability in &ty.capabilities {
            if !canonical.contains(capability) {
                return Err(CompileError::without_span(format!(
                    "metadata type '{}' contains unknown capability '{}'",
                    ty.name, capability
                )));
            }
            if !seen.insert(capability) {
                return Err(CompileError::without_span(format!("metadata type '{}' repeats capability '{}'", ty.name, capability)));
            }
        }
        let canonical_order =
            expected.capabilities.iter().filter(|capability| seen.contains(*capability)).cloned().collect::<Vec<_>>();
        if ty.capabilities != canonical_order {
            return Err(CompileError::without_span(format!(
                "metadata type '{}' capabilities must follow closed registry order",
                ty.name
            )));
        }
    }
    for (index, proof) in metadata.runtime.capability_proofs.iter().enumerate() {
        let prefix = format!("metadata runtime.capability_proofs[{index}]");
        if proof.capability_set_version != ast::Capability::REGISTRY_VERSION
            || proof.entailment_version != ast::CapabilityOperation::ENTAILMENT_VERSION
        {
            return Err(CompileError::without_span(format!("{prefix} uses an unsupported capability or entailment version")));
        }
        if proof.evidence_tier != EvidenceTier::CheckedStatic {
            return Err(CompileError::without_span(format!("{prefix}.evidence_tier must be checked-static")));
        }
        if !proof.missing.is_empty() {
            return Err(CompileError::without_span(format!("{prefix} is emitted for an operation with missing authority")));
        }
        let operation =
            ast::CapabilityOperation::ALL.into_iter().find(|operation| operation.as_str() == proof.operation).ok_or_else(|| {
                CompileError::without_span(format!("{prefix}.operation '{}' is not in the closed relation", proof.operation))
            })?;
        let mut expected_required =
            operation.required_capabilities().into_iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
        if operation.requires_identity_preservation() {
            expected_required.push("identity-preservation".to_string());
            if proof
                .identity_condition
                .as_deref()
                .is_none_or(|condition| !condition.starts_with("identity(") || !condition.ends_with(')'))
            {
                return Err(CompileError::without_span(format!("{prefix} is missing its exact identity condition")));
            }
        } else if proof.identity_condition.is_some() {
            return Err(CompileError::without_span(format!("{prefix} must not attach an identity condition")));
        }
        if proof.required != expected_required {
            return Err(CompileError::without_span(format!("{prefix}.required does not match the closed operation rule")));
        }
        if proof.required.iter().any(|required| !proof.entailed.contains(required)) {
            return Err(CompileError::without_span(format!("{prefix}.entailed does not cover every required authority")));
        }
        if proof.provided.iter().any(|provided| !canonical.contains(provided)) {
            return Err(CompileError::without_span(format!("{prefix}.provided contains a capability outside the closed registry")));
        }
        let ty = metadata
            .types
            .iter()
            .find(|ty| ty.name == proof.type_name)
            .ok_or_else(|| CompileError::without_span(format!("{prefix}.type_name '{}' has no type metadata", proof.type_name)))?;
        if proof.provided != ty.capabilities {
            return Err(CompileError::without_span(format!(
                "{prefix}.provided does not match the per-resource capability set of '{}'",
                proof.type_name
            )));
        }
        if operation.requires_identity_preservation() {
            let expected_identity = ty.identity_policy.as_ref().map(|identity| format!("identity({identity})"));
            if proof.identity_condition != expected_identity {
                return Err(CompileError::without_span(format!(
                    "{prefix}.identity_condition does not match the declared per-resource identity policy"
                )));
            }
        }
    }
    Ok(())
}

fn validate_payload_enum_layout_metadata(metadata: &CompileMetadata) -> Result<()> {
    let mut names = HashSet::new();
    let mut previous_name: Option<&str> = None;
    for layout in &metadata.enum_layouts {
        if layout.name.is_empty() || !names.insert(layout.name.as_str()) {
            return Err(CompileError::without_span(format!(
                "metadata enum_layouts contains an empty or duplicate enum name '{}'",
                layout.name
            )));
        }
        if previous_name.is_some_and(|previous| previous >= layout.name.as_str()) {
            return Err(CompileError::without_span("metadata enum_layouts must use canonical name order"));
        }
        previous_name = Some(&layout.name);
        if layout.generic {
            return Err(CompileError::without_span(format!(
                "metadata enum '{}' cannot be generic in the concrete 0.22 payload ADT surface",
                layout.name
            )));
        }
        if layout.tag_width_bytes != 1 || layout.variants.is_empty() || layout.variants.len() > u8::MAX as usize + 1 {
            return Err(CompileError::without_span(format!(
                "metadata enum '{}' must use a one-byte tag and declare 1..=256 variants",
                layout.name
            )));
        }
        let has_payload = layout.variants.iter().any(|variant| !variant.fields.is_empty());
        let expected_layout = if has_payload { "packed-tagged-union-v1" } else { "u8-discriminant-v1" };
        let expected_abi = if has_payload && layout.encoded_size_bytes <= 16 {
            "fixed-bytes-pointer-v1+register-pair-return-v1"
        } else if has_payload {
            "fixed-bytes-pointer-v1"
        } else {
            "scalar-u8-v1"
        };
        let expected_storage = if layout.contains_linear_payload { "local-linear-handle-v1" } else { "serializable-fixed-width" };
        if layout.layout != expected_layout || layout.abi != expected_abi || layout.storage != expected_storage {
            return Err(CompileError::without_span(format!(
                "metadata enum '{}' layout/ABI/storage contract is not canonical",
                layout.name
            )));
        }

        let mut variant_names = HashSet::new();
        let mut max_payload_width = 0usize;
        let mut observed_linear = false;
        for (tag, variant) in layout.variants.iter().enumerate() {
            if variant.name.is_empty() || !variant_names.insert(variant.name.as_str()) || variant.tag as usize != tag {
                return Err(CompileError::without_span(format!(
                    "metadata enum '{}' variants must have unique names and canonical sequential tags",
                    layout.name
                )));
            }
            let mut next_offset = 1usize;
            for (index, field) in variant.fields.iter().enumerate() {
                if field.index != index || field.offset_bytes != next_offset || field.r#type.is_empty() {
                    return Err(CompileError::without_span(format!(
                        "metadata enum '{}::{}' fields must use canonical indices and packed offsets",
                        layout.name, variant.name
                    )));
                }
                let linear = field.ownership == "linear-cell-handle";
                if linear && field.width_bytes != 8 {
                    return Err(CompileError::without_span(format!(
                        "metadata enum '{}::{}' linear payload handles must be eight bytes",
                        layout.name, variant.name
                    )));
                }
                if !linear && field.ownership != "owned-fixed-width" {
                    return Err(CompileError::without_span(format!(
                        "metadata enum '{}::{}' field ownership '{}' is not canonical",
                        layout.name, variant.name, field.ownership
                    )));
                }
                observed_linear |= linear;
                next_offset = next_offset.checked_add(field.width_bytes).ok_or_else(|| {
                    CompileError::without_span(format!("metadata enum '{}::{}' payload width overflow", layout.name, variant.name))
                })?;
            }
            let payload_width = next_offset - 1;
            if variant.payload_width_bytes != payload_width {
                return Err(CompileError::without_span(format!(
                    "metadata enum '{}::{}' payload width does not match its field layout",
                    layout.name, variant.name
                )));
            }
            max_payload_width = max_payload_width.max(payload_width);
        }
        if layout.encoded_size_bytes != 1 + max_payload_width || layout.contains_linear_payload != observed_linear {
            return Err(CompileError::without_span(format!(
                "metadata enum '{}' encoded size or linear payload flag does not match its variants",
                layout.name
            )));
        }
    }
    Ok(())
}

fn validate_protocol_role_metadata(metadata: &CompileMetadata) -> Result<()> {
    for action in &metadata.actions {
        let mut seen = HashSet::new();
        let mut previous_key = None;
        for (index, candidate) in action.protocol_role_candidates.iter().enumerate() {
            let prefix = format!("metadata action '{}'.protocol_role_candidates[{index}]", action.name);
            if candidate.role.trim().is_empty() {
                return Err(CompileError::without_span(format!("{prefix}.role must not be empty")));
            }
            if candidate.evidence_tier != EvidenceTier::MetadataOnly || candidate.authorization_proven {
                return Err(CompileError::without_span(format!("{prefix} must remain metadata-only with authorization_proven=false")));
            }
            let expected_strength = match candidate.source.as_str() {
                "verification-predicate" => {
                    if candidate.actor_binding.is_none()
                        || candidate.actor_source.is_none()
                        || candidate.cell_binding.is_none()
                        || candidate.field_name.is_none()
                        || candidate.predicate.as_deref().is_none_or(str::is_empty)
                        || candidate.source_span.is_none()
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} explicit predicate source is missing actor, Cell field, predicate, or span attribution"
                        )));
                    }
                    "explicit"
                }
                "witness-binding" | "lock-args-binding" => {
                    if candidate.actor_binding.is_none()
                        || candidate.actor_source.is_none()
                        || candidate.cell_binding.is_some()
                        || candidate.field_name.is_some()
                        || candidate.predicate.is_some()
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} binding source must attribute only its actor binding and ABI source"
                        )));
                    }
                    "binding"
                }
                "field-name" => {
                    if candidate.cell_binding.is_none()
                        || candidate.field_name.is_none()
                        || candidate.actor_binding.is_some()
                        || candidate.actor_source.is_some()
                        || candidate.predicate.is_some()
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} weak field source must attribute exactly one Cell binding and field"
                        )));
                    }
                    "weak"
                }
                other => {
                    return Err(CompileError::without_span(format!("{prefix}.source '{other}' is not canonical")));
                }
            };
            if candidate.strength != expected_strength {
                return Err(CompileError::without_span(format!(
                    "{prefix}.strength '{}' does not match source '{}'",
                    candidate.strength, candidate.source
                )));
            }
            let key = (
                protocol_role_metadata_source_rank(&candidate.source),
                candidate.role.clone(),
                candidate.actor_binding.clone(),
                candidate.cell_binding.clone(),
                candidate.field_name.clone(),
                candidate.predicate.clone(),
            );
            if previous_key.as_ref().is_some_and(|previous| previous > &key) {
                return Err(CompileError::without_span(format!(
                    "metadata action '{}' protocol role candidates are not in canonical source-precedence order",
                    action.name
                )));
            }
            if !seen.insert(key.clone()) {
                return Err(CompileError::without_span(format!(
                    "metadata action '{}' repeats a ProtocolGraph role candidate",
                    action.name
                )));
            }
            previous_key = Some(key);
        }
        if action.proof_plan.iter().any(|plan| plan.category == "protocol-role") {
            return Err(CompileError::without_span(format!(
                "metadata action '{}' must not represent ProtocolGraph roles as ProofPlan authorization evidence",
                action.name
            )));
        }
    }
    Ok(())
}

fn protocol_role_metadata_source_rank(source: &str) -> u8 {
    match source {
        "verification-predicate" => 0,
        "witness-binding" | "lock-args-binding" => 1,
        "field-name" => 2,
        _ => u8::MAX,
    }
}

fn validate_transaction_view_handle_metadata(metadata: &CompileMetadata) -> Result<()> {
    for (index, handle) in metadata.runtime.transaction_view_handles.iter().enumerate() {
        let prefix = format!("metadata runtime.transaction_view_handles[{}]", index);
        for (field, value) in [
            ("scope_kind", handle.scope_kind.as_str()),
            ("scope_name", handle.scope_name.as_str()),
            ("binding", handle.binding.as_str()),
            ("handle_type", handle.handle_type.as_str()),
            ("source", handle.source.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(CompileError::without_span(format!("{}.{} must not be empty", prefix, field)));
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
            return Err(CompileError::without_span(format!(
                "{}.handle_type '{}' is not a supported typed transaction-view handle",
                prefix, handle.handle_type
            )));
        }
        if handle.ownership != "read-only-view" || handle.lifecycle_authority {
            return Err(CompileError::without_span(format!("{} must remain a read-only view with lifecycle_authority=false", prefix)));
        }
        let witness_view = base_type == "WitnessBytesView";
        let declared_witness = cellscript_artifact_checker::view_types::parse_witness_bytes_view_type(
            &handle.handle_type,
            cellscript_artifact_checker::view_types::WitnessViewSyntax::ExactArguments,
        )
        .map(|view| (view.owner.as_str(), view.owner.tag(), view.maximum));
        if witness_view {
            let Some((owner, _, maximum)) = declared_witness else {
                return Err(CompileError::without_span(format!(
                    "{}.handle_type '{}' has an invalid bounded witness owner or maximum",
                    prefix, handle.handle_type
                )));
            };
            if handle.witness_owner.as_deref() != Some(owner) || handle.max_bytes != Some(maximum) {
                return Err(CompileError::without_span(format!("{} bounded witness owner/maximum do not match handle_type", prefix)));
            }
            let source_matches = match handle.source.as_str() {
                "WitnessArgs/Input" => handle.provenance.source.resolved_source == "Input",
                "Input" | "Output" | "GroupInput" | "GroupOutput" => handle.provenance.source.resolved_source == handle.source,
                _ => false,
            };
            if !source_matches {
                return Err(CompileError::without_span(format!("{} has an invalid bounded witness source", prefix)));
            }
            let range = &handle.provenance.range;
            if range.kind != "bounded-range"
                || range.offset.kind != "static"
                || range.offset.value != Some(0)
                || range.offset.binding.is_some()
                || range.offset.max_inclusive != Some(maximum)
                || range.length.kind != "dynamic"
                || range.length.value.is_some()
                || range.length.binding.as_deref() != Some(format!("{}.size", handle.binding).as_str())
                || range.length.max_inclusive != Some(maximum)
            {
                return Err(CompileError::without_span(format!(
                    "{} bounded witness range does not match its declared maximum",
                    prefix
                )));
            }
        } else if handle.witness_owner.is_some() || handle.max_bytes.is_some() {
            return Err(CompileError::without_span(format!(
                "{} non-witness handle must not carry witness_owner or max_bytes",
                prefix
            )));
        }
        if handle.typing_evidence_tier != EvidenceTier::CheckedStatic || handle.read_evidence_tier != EvidenceTier::CheckedRuntime {
            return Err(CompileError::without_span(format!(
                "{} must use checked-static typing evidence and checked-runtime read evidence",
                prefix
            )));
        }
        validate_ckb_runtime_access_provenance(&prefix, &handle.source, None, &handle.provenance)?;
    }
    Ok(())
}

fn validate_borrow_region_metadata(metadata: &CompileMetadata) -> Result<()> {
    for (index, region) in metadata.runtime.borrow_regions.iter().enumerate() {
        let prefix = format!("metadata runtime.borrow_regions[{}]", index);
        for (field, value) in [
            ("scope_kind", region.scope_kind.as_str()),
            ("scope_name", region.scope_name.as_str()),
            ("root", region.root.as_str()),
            ("binding", region.binding.as_str()),
            ("view_type", region.view_type.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(CompileError::without_span(format!("{}.{} must not be empty", prefix, field)));
            }
        }
        if !region.view_type.starts_with("View<") || !region.view_type.ends_with('>') {
            return Err(CompileError::without_span(format!(
                "{}.view_type '{}' must be a canonical View<T> marker",
                prefix, region.view_type
            )));
        }
        if region.storage != "none" || region.abi != "none" {
            return Err(CompileError::without_span(format!("{} must declare storage=none and abi=none", prefix)));
        }

        if region.evidence_tier != EvidenceTier::CheckedStatic {
            return Err(CompileError::without_span(format!("{}.evidence_tier must be checked-static", prefix)));
        }
    }
    Ok(())
}

fn validate_type_validity_metadata(metadata: &CompileMetadata) -> Result<()> {
    for ty in &metadata.types {
        for (index, predicate) in ty.validity_predicates.iter().enumerate() {
            let prefix = format!("metadata type '{}'.validity_predicates[{index}]", ty.name);
            if predicate.expression.trim().is_empty() {
                return Err(CompileError::without_span(format!("{prefix}.expression must not be empty")));
            }
            if predicate.source_span.end <= predicate.source_span.start || predicate.source_span.line == 0 {
                return Err(CompileError::without_span(format!("{prefix}.source_span must identify a non-empty source range")));
            }
            if predicate.create_paths_checked > predicate.create_paths_selected {
                return Err(CompileError::without_span(format!("{prefix}.create_paths_checked cannot exceed create_paths_selected")));
            }
            match predicate.evidence_tier {
                EvidenceTier::CheckedStatic => {
                    if predicate.runtime_checked_on_create
                        || predicate.create_path_status != "checked-static"
                        || predicate.update_path_status != "checked-static"
                        || predicate.create_paths_checked != predicate.create_paths_selected
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} checked-static evidence cannot depend on runtime create/update checks"
                        )));
                    }
                }
                EvidenceTier::CheckedRuntime => {
                    if !predicate.runtime_checked_on_create
                        || predicate.create_path_status != "checked-runtime"
                        || predicate.create_paths_selected == 0
                        || predicate.create_paths_checked != predicate.create_paths_selected
                        || !matches!(
                            predicate.update_path_status.as_str(),
                            "runtime-helper-required" | "local-struct-not-cell-update" | "not-selected"
                        )
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} checked-runtime evidence requires a selected fail-closed create-path check and a canonical update status"
                        )));
                    }
                }
                EvidenceTier::RuntimeHelperRequired => {
                    if predicate.runtime_checked_on_create
                        || !matches!(
                            predicate.create_path_status.as_str(),
                            "not-selected-runtime-helper-required"
                                | "selected-runtime-helper-required"
                                | "partial-runtime-helper-required"
                        )
                        || match predicate.create_path_status.as_str() {
                            "not-selected-runtime-helper-required" => {
                                predicate.create_paths_selected != 0 || predicate.create_paths_checked != 0
                            }
                            "selected-runtime-helper-required" => {
                                predicate.create_paths_selected == 0 || predicate.create_paths_checked != 0
                            }
                            "partial-runtime-helper-required" => {
                                predicate.create_paths_checked == 0
                                    || predicate.create_paths_checked >= predicate.create_paths_selected
                            }
                            _ => true,
                        }
                        || !matches!(
                            predicate.update_path_status.as_str(),
                            "runtime-helper-required" | "local-struct-not-cell-update" | "not-selected"
                        )
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} runtime-helper evidence must identify an unselected create path and a canonical update status"
                        )));
                    }
                }
                EvidenceTier::BuilderEvidenceRequired => {
                    if predicate.runtime_checked_on_create
                        || predicate.create_path_status != "builder-header-evidence-required"
                        || predicate.update_path_status != "builder-header-evidence-required"
                        || predicate.create_paths_checked != 0
                        || !predicate.dependencies.iter().any(|dependency| dependency == "environment:env::block_number")
                        || !predicate.dependencies.iter().any(|dependency| dependency == "builder:header-dep-block-number-evidence")
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} builder evidence must name env::block_number and the header-dep block-number contract"
                        )));
                    }
                }
                EvidenceTier::TrustedExternal | EvidenceTier::MetadataOnly | EvidenceTier::ChainEvidenceRequired => {
                    return Err(CompileError::without_span(format!(
                        "{prefix} uses unsupported validity evidence tier '{}'",
                        predicate.evidence_tier.as_str()
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_collection_instantiation_metadata(metadata: &CompileMetadata) -> Result<()> {
    for (index, collection) in metadata.runtime.collection_instantiations.iter().enumerate() {
        let prefix = format!("metadata runtime.collection_instantiations[{index}]");
        for (field, value) in [
            ("scope_kind", collection.scope_kind.as_str()),
            ("scope_name", collection.scope_name.as_str()),
            ("collection_ty", collection.collection_ty.as_str()),
            ("element_ty", collection.element_ty.as_str()),
            ("backing", collection.backing.as_str()),
            ("status", collection.status.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(CompileError::without_span(format!("{prefix}.{field} must not be empty")));
            }
        }
        if collection.max_elements == 0 {
            return Err(CompileError::without_span(format!("{prefix}.max_elements must be greater than zero")));
        }

        if collection.backing.starts_with("bounded-source:") {
            if !collection.vacuous_possible || collection.actual_scanned_cardinality.is_some() {
                return Err(CompileError::without_span(format!(
                    "{prefix} must declare vacuous_possible=true and leave actual_scanned_cardinality for runtime evidence"
                )));
            }
            match collection.ownership.as_str() {
                "linear-cell-set" => {
                    let pending = collection.status == "runtime-helper-required"
                        && collection.evidence_tier == EvidenceTier::RuntimeHelperRequired;
                    let checked = collection.status == "checked-runtime"
                        && collection.evidence_tier == EvidenceTier::CheckedRuntime
                        && collection.helpers.iter().any(|helper| helper == ir::BOUNDED_TYPE_GROUP_INPUTS_V1);
                    if collection.source != "input"
                        || !(pending || checked)
                        || collection.output_cardinality_max.is_some()
                        || collection.capacity_builder_evidence_required
                        || !collection.helpers.iter().any(|helper| helper == "consume_each")
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} bounded Cell set must use input source, consume_each evidence, a recognised runtime contract when checked, and no output builder obligation"
                        )));
                    }
                }
                "bounded-output-plan" => {
                    let pending = collection.status == "builder-evidence-required"
                        && collection.evidence_tier == EvidenceTier::BuilderEvidenceRequired
                        && collection.capacity_builder_evidence_required;
                    let checked = collection.status == "checked-runtime"
                        && collection.evidence_tier == EvidenceTier::CheckedRuntime
                        && !collection.capacity_builder_evidence_required
                        && collection.helpers.iter().any(|helper| helper == ir::BOUNDED_OUTPUT_PLAN_V1)
                        && collection.bounded_output_plan.is_some();
                    if collection.source != "witness"
                        || !(pending || checked)
                        || collection.output_cardinality_max != Some(collection.max_elements)
                        || !collection.helpers.iter().any(|helper| helper == "create_each")
                        || (pending && collection.bounded_output_plan.is_some())
                    {
                        return Err(CompileError::without_span(format!(
                            "{prefix} bounded output plan must use witness source and carry either builder evidence or the recognised checked runtime contract"
                        )));
                    }
                    if let Some(contract) = &collection.bounded_output_plan {
                        validate_bounded_output_plan_contract_metadata(&prefix, metadata, collection, contract)?;
                    }
                }
                ownership => {
                    return Err(CompileError::without_span(format!(
                        "{prefix}.ownership '{ownership}' is not a supported bounded collection ownership model"
                    )));
                }
            }
        } else if collection.backing.starts_with("stack-fixed-buffer:")
            && (collection.source != "local-stack"
                || collection.ownership != "owned-local-values"
                || collection.status != "checked-runtime"
                || collection.evidence_tier != EvidenceTier::CheckedRuntime)
        {
            return Err(CompileError::without_span(format!(
                "{prefix} stack collection must use checked-runtime local owned-value evidence"
            )));
        }
    }
    Ok(())
}

fn validate_bounded_output_plan_contract_metadata(
    prefix: &str,
    metadata: &CompileMetadata,
    collection: &CollectionInstantiationMetadata,
    contract: &BoundedOutputPlanContractMetadata,
) -> Result<()> {
    let total_bytes = contract.max_elements.checked_mul(contract.element_width_bytes).and_then(|bytes| bytes.checked_add(12));
    if contract.schema != "cellscript-bounded-output-plan-contract"
        || contract.version != 1
        || contract.binding.trim().is_empty()
        || contract.codec != "molecule-fixvec-fixed-item-v1"
        || contract.codec_magic_hex != format!("0x{}", hex_encode(ir::BOUNDED_OUTPUT_PLAN_MAGIC_V1))
        || contract.placement != "entry-witness-args-v1-param-payload"
        || !contract.allow_zero
        || contract.element_ty != collection.element_ty
        || contract.element_width_bytes != collection.element_width_bytes
        || contract.max_elements != collection.max_elements
        || total_bytes.is_none_or(|bytes| bytes > ir::BOUNDED_OUTPUT_PLAN_MAX_BYTES)
        || contract.output_source != "ckb-group-output"
        || contract.ordering != "plan-index-equals-group-output-index"
        || contract.correspondence != "exactly-one-group-output-per-plan-element"
        || contract.output_ty.trim().is_empty()
        || contract.output_width_bytes == 0
        || contract.output_width_bytes > 512
        || contract.type_script_policy != "exact-current-script-hash"
        || contract.lock_policy != "exact-plan-field-hash"
        || contract.capacity_floor_shannons == 0
        || contract.identity_policy != "fresh-output-outpoint-plus-type-group-ordinal"
        || contract.field_bindings.is_empty()
        || contract.lock_binding.width_bytes != 32
    {
        return Err(CompileError::without_span(format!(
            "{prefix}.bounded_output_plan is not the accepted bounded-output-plan-v1 contract"
        )));
    }
    let mut output_fields = BTreeSet::new();
    let plan_type = metadata
        .types
        .iter()
        .find(|ty| ty.name == contract.element_ty)
        .ok_or_else(|| CompileError::without_span(format!("{prefix}.bounded_output_plan element type is missing")))?;
    let output_type = metadata
        .types
        .iter()
        .find(|ty| ty.name == contract.output_ty)
        .ok_or_else(|| CompileError::without_span(format!("{prefix}.bounded_output_plan output type is missing")))?;
    if plan_type.kind != "Struct"
        || plan_type.encoded_size != Some(contract.element_width_bytes)
        || output_type.kind != "Resource"
        || output_type.encoded_size != Some(contract.output_width_bytes)
        || output_type.capacity_floor_shannons != Some(contract.capacity_floor_shannons)
        || output_type.identity_policy.is_some()
        || contract.field_bindings.len() != output_type.fields.len()
    {
        return Err(CompileError::without_span(format!(
            "{prefix}.bounded_output_plan does not match its fixed Plan and Resource type metadata"
        )));
    }
    let mut covered = vec![false; contract.output_width_bytes];
    for binding in &contract.field_bindings {
        let plan_field = plan_type.fields.iter().find(|field| field.name == binding.plan_field);
        let output_field = output_type.fields.iter().find(|field| field.name == binding.output_field);
        if binding.output_field.trim().is_empty()
            || binding.plan_field.trim().is_empty()
            || binding.width_bytes == 0
            || binding.output_offset_bytes.checked_add(binding.width_bytes).is_none_or(|end| end > contract.output_width_bytes)
            || binding.plan_offset_bytes.checked_add(binding.width_bytes).is_none_or(|end| end > contract.element_width_bytes)
            || !output_fields.insert(binding.output_field.as_str())
            || plan_field.is_none_or(|field| {
                field.offset != binding.plan_offset_bytes
                    || field.encoded_size != Some(binding.width_bytes)
                    || field.ty != binding.ty
                    || !field.fixed_width
            })
            || output_field.is_none_or(|field| {
                field.offset != binding.output_offset_bytes
                    || field.encoded_size != Some(binding.width_bytes)
                    || field.ty != binding.ty
                    || !field.fixed_width
            })
        {
            return Err(CompileError::without_span(format!(
                "{prefix}.bounded_output_plan.field_bindings contain an invalid or duplicate exact field mapping"
            )));
        }
        for byte in covered.iter_mut().skip(binding.output_offset_bytes).take(binding.width_bytes) {
            if *byte {
                return Err(CompileError::without_span(format!("{prefix}.bounded_output_plan.field_bindings overlap")));
            }
            *byte = true;
        }
    }
    let lock_field = plan_type.fields.iter().find(|field| field.name == contract.lock_binding.plan_field);
    if contract
        .lock_binding
        .plan_offset_bytes
        .checked_add(contract.lock_binding.width_bytes)
        .is_none_or(|end| end > contract.element_width_bytes)
        || contract.lock_binding.plan_field.trim().is_empty()
        || lock_field.is_none_or(|field| {
            field.offset != contract.lock_binding.plan_offset_bytes
                || field.encoded_size != Some(32)
                || field.ty != contract.lock_binding.ty
                || !field.fixed_width
        })
        || covered.iter().any(|covered| !covered)
    {
        return Err(CompileError::without_span(format!(
            "{prefix}.bounded_output_plan.lock_binding falls outside the fixed plan element"
        )));
    }
    Ok(())
}

fn validate_metadata_component_schema_version(field: &str, actual: u32, expected: u32) -> Result<()> {
    if actual != expected {
        return Err(CompileError::without_span(format!("unsupported {field} {actual}; expected {expected}")));
    }
    Ok(())
}

fn validate_target_profile_metadata(metadata: &CompileMetadata, artifact_format: ArtifactFormat) -> Result<()> {
    let profile = TargetProfile::from_name(&metadata.target_profile.name)?;
    let expected = profile.metadata(artifact_format);
    let actual = &metadata.target_profile;

    let mismatches = [
        ("target_chain", actual.target_chain.as_str(), expected.target_chain.as_str()),
        ("vm_abi", actual.vm_abi.as_str(), expected.vm_abi.as_str()),
        ("hash_domain", actual.hash_domain.as_str(), expected.hash_domain.as_str()),
        ("syscall_set", actual.syscall_set.as_str(), expected.syscall_set.as_str()),
        ("artifact_packaging", actual.artifact_packaging.as_str(), expected.artifact_packaging.as_str()),
        ("header_abi", actual.header_abi.as_str(), expected.header_abi.as_str()),
        ("scheduler_abi", actual.scheduler_abi.as_str(), expected.scheduler_abi.as_str()),
        ("witness_abi", actual.witness_abi.as_str(), expected.witness_abi.as_str()),
        ("lock_args_abi", actual.lock_args_abi.as_str(), expected.lock_args_abi.as_str()),
        ("source_encoding", actual.source_encoding.as_str(), expected.source_encoding.as_str()),
        ("spawn_ipc_abi", actual.spawn_ipc_abi.as_str(), expected.spawn_ipc_abi.as_str()),
        ("since_abi", actual.since_abi.as_str(), expected.since_abi.as_str()),
        ("cell_dep_abi", actual.cell_dep_abi.as_str(), expected.cell_dep_abi.as_str()),
        ("script_ref_abi", actual.script_ref_abi.as_str(), expected.script_ref_abi.as_str()),
        ("output_data_abi", actual.output_data_abi.as_str(), expected.output_data_abi.as_str()),
        ("capacity_floor_abi", actual.capacity_floor_abi.as_str(), expected.capacity_floor_abi.as_str()),
        ("type_id_abi", actual.type_id_abi.as_str(), expected.type_id_abi.as_str()),
        ("riscv_isa", actual.riscv_isa.as_str(), expected.riscv_isa.as_str()),
    ];
    for (field, actual, expected) in mismatches {
        if actual != expected {
            return Err(CompileError::without_span(format!(
                "metadata target_profile.{} '{}' does not match expected '{}' for profile '{}' and {} artifact",
                field,
                actual,
                expected,
                profile.name(),
                artifact_format.display_name()
            )));
        }
    }
    if actual.minimum_vm_version != expected.minimum_vm_version {
        return Err(CompileError::without_span(format!(
            "metadata target_profile.minimum_vm_version '{}' does not match expected '{}' for profile '{}' and {} artifact",
            actual.minimum_vm_version,
            expected.minimum_vm_version,
            profile.name(),
            artifact_format.display_name()
        )));
    }
    if actual.deployment_hash_types != expected.deployment_hash_types {
        return Err(CompileError::without_span(format!(
            "metadata target_profile.deployment_hash_types {:?} does not match expected {:?} for profile '{}' and {} artifact",
            actual.deployment_hash_types,
            expected.deployment_hash_types,
            profile.name(),
            artifact_format.display_name()
        )));
    }
    if actual.tx_version != expected.tx_version {
        return Err(CompileError::without_span(format!(
            "metadata target_profile.tx_version '{}' does not match expected '{}' for profile '{}' and {} artifact",
            actual.tx_version,
            expected.tx_version,
            profile.name(),
            artifact_format.display_name()
        )));
    }

    Ok(())
}

pub(super) fn target_profile_artifact_policy_violations(_metadata: &CompileMetadata, profile: TargetProfile) -> Vec<String> {
    match profile {
        TargetProfile::Ckb | TargetProfile::CkbTypeHash => Vec::new(),
    }
}

use super::pool_metadata::operand_var_name;
use super::proof_plan::EvidenceTier;
use super::schema_metadata::metadata_identity_policy;
use super::verifier_obligations::operand_named_type_name;
use super::{
    ast, ir, CapabilityOperationRuleMetadata, CapabilityProofMetadata, CapabilityRegistryMetadata, CkbRuntimeScalarProvenanceMetadata,
    PayloadEnumFieldMetadata, PayloadEnumLayoutMetadata, PayloadEnumVariantMetadata,
};
use std::collections::{HashMap, HashSet};

pub(super) fn capability_registry_metadata() -> CapabilityRegistryMetadata {
    CapabilityRegistryMetadata {
        capability_set_version: ast::Capability::REGISTRY_VERSION,
        entailment_version: ast::CapabilityOperation::ENTAILMENT_VERSION,
        capabilities: ast::Capability::canonical_names(),
        operation_rules: ast::CapabilityOperation::ALL
            .into_iter()
            .map(|operation| CapabilityOperationRuleMetadata {
                operation: operation.as_str().to_string(),
                required: operation
                    .required_capabilities()
                    .into_iter()
                    .map(|capability| capability.as_str().to_string())
                    .chain(operation.requires_identity_preservation().then(|| "identity-preservation".to_string()))
                    .collect(),
                legacy_alternatives: if operation == ast::CapabilityOperation::Destroy {
                    vec!["destroy (legacy compatibility only)".to_string()]
                } else {
                    Vec::new()
                },
                identity_preservation_required: operation.requires_identity_preservation(),
            })
            .collect(),
        inheritance_syntax: "none; authority remains an explicit per-resource capability set".to_string(),
    }
}

pub(super) fn payload_enum_layout_metadata(ir: &ir::IrModule) -> Vec<PayloadEnumLayoutMetadata> {
    let mut layouts = ir.enum_layouts.values().cloned().collect::<Vec<_>>();
    layouts.sort_by(|left, right| left.name.cmp(&right.name));
    layouts
        .into_iter()
        .map(|layout| {
            let has_payload = layout.has_payload();
            PayloadEnumLayoutMetadata {
                name: layout.name,
                generic: false,
                layout: if has_payload { "packed-tagged-union-v1" } else { "u8-discriminant-v1" }.to_string(),
                abi: if has_payload && layout.encoded_size <= 16 {
                    "fixed-bytes-pointer-v1+register-pair-return-v1"
                } else if has_payload {
                    "fixed-bytes-pointer-v1"
                } else {
                    "scalar-u8-v1"
                }
                .to_string(),
                storage: if layout.contains_linear_payload { "local-linear-handle-v1" } else { "serializable-fixed-width" }
                    .to_string(),
                tag_width_bytes: layout.tag_width,
                encoded_size_bytes: layout.encoded_size,
                contains_linear_payload: layout.contains_linear_payload,
                variants: layout
                    .variants
                    .into_iter()
                    .map(|variant| PayloadEnumVariantMetadata {
                        name: variant.name,
                        tag: variant.tag,
                        payload_width_bytes: variant.payload_width,
                        fields: variant
                            .fields
                            .into_iter()
                            .map(|field| PayloadEnumFieldMetadata {
                                index: field.index,
                                r#type: payload_ir_type_name(&field.ty),
                                offset_bytes: field.offset,
                                width_bytes: field.width,
                                ownership: if field.linear { "linear-cell-handle" } else { "owned-fixed-width" }.to_string(),
                            })
                            .collect(),
                    })
                    .collect(),
            }
        })
        .collect()
}

fn payload_ir_type_name(ty: &ir::IrType) -> String {
    ty.source_text()
}

pub(super) fn capability_proof_metadata(ir: &ir::IrModule) -> Vec<CapabilityProofMetadata> {
    let mut type_defs = HashMap::<String, &ir::IrTypeDef>::new();
    for type_def in &ir.external_type_defs {
        type_defs.insert(type_def.name.clone(), type_def);
    }
    for item in &ir.items {
        if let ir::IrItem::TypeDef(type_def) = item {
            type_defs.insert(type_def.name.clone(), type_def);
        }
    }

    let mut proofs = Vec::new();
    for item in &ir.items {
        let (scope_kind, scope_name, body) = match item {
            ir::IrItem::Action(action) => ("action", action.name.as_str(), &action.body),
            ir::IrItem::Function(function) => ("function", function.name.as_str(), &function.body),
            ir::IrItem::Lock(lock) => ("lock", lock.name.as_str(), &lock.body),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        for block in &body.blocks {
            for instruction in &block.instructions {
                let (operation, operand, identity) = match instruction {
                    ir::IrInstruction::Destroy { operand, .. } => (ast::CapabilityOperation::Destroy, operand, None),
                    ir::IrInstruction::ReplaceUnique { operand, identity, .. } => {
                        (ast::CapabilityOperation::ReplaceUnique, operand, Some(identity))
                    }
                    _ => continue,
                };
                let Some(type_name) = operand_named_type_name(operand) else {
                    continue;
                };
                let Some(type_def) = type_defs.get(&type_name) else {
                    continue;
                };
                let provided_set = type_def.capabilities.iter().copied().collect::<HashSet<_>>();
                let evaluation = operation.evaluate(&provided_set);
                let provided = ast::Capability::ALL
                    .into_iter()
                    .filter(|capability| provided_set.contains(capability))
                    .map(|capability| capability.as_str().to_string())
                    .collect::<Vec<_>>();
                let mut required = evaluation.required.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
                let mut entailed = evaluation.entailed.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
                let mut missing = evaluation.missing.iter().map(|capability| capability.as_str().to_string()).collect::<Vec<_>>();
                let identity_condition = identity.and_then(capability_identity_condition);
                if operation.requires_identity_preservation() {
                    required.push("identity-preservation".to_string());
                    if identity_condition.is_some() {
                        entailed.push("identity-preservation".to_string());
                    } else {
                        missing.push("identity(...)".to_string());
                    }
                }
                proofs.push(CapabilityProofMetadata {
                    scope_kind: scope_kind.to_string(),
                    scope_name: scope_name.to_string(),
                    operation: operation.as_str().to_string(),
                    type_name,
                    binding: operand_var_name(operand).unwrap_or("<anonymous>").to_string(),
                    required,
                    provided,
                    entailed,
                    missing,
                    identity_condition,
                    capability_set_version: ast::Capability::REGISTRY_VERSION,
                    entailment_version: ast::CapabilityOperation::ENTAILMENT_VERSION,
                    evidence_tier: EvidenceTier::CheckedStatic,
                });
            }
        }
    }
    proofs.sort_by(|left, right| {
        (left.scope_kind.as_str(), left.scope_name.as_str(), left.operation.as_str(), left.type_name.as_str(), left.binding.as_str())
            .cmp(&(
                right.scope_kind.as_str(),
                right.scope_name.as_str(),
                right.operation.as_str(),
                right.type_name.as_str(),
                right.binding.as_str(),
            ))
    });
    proofs
}

fn capability_identity_condition(identity: &ir::IrIdentityPolicy) -> Option<String> {
    metadata_identity_policy(identity).map(|policy| format!("identity({policy})"))
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeViewProvenance {
    pub(super) resolved_source: String,
    pub(super) index: CkbRuntimeScalarProvenanceMetadata,
}

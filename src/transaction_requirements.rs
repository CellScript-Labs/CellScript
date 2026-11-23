use super::metadata_abi_binding::{mutate_field_equality_status, mutate_field_transition_status};
use super::schema_metadata::named_type_name;
use super::verifier_obligations::{cell_metadata_field_byte_len, TransactionResourceObligation};
use super::{ir, MetadataTypeLayouts, TransactionRuntimeInputRequirementMetadata, VerifierObligationMetadata};
use std::collections::HashMap;

fn operation_input_feature(feature: &str) -> Option<(&'static str, &str)> {
    if let Some(binding) = feature.strip_prefix("consume-input:") {
        Some(("consume", binding))
    } else if let Some(binding) = feature.strip_prefix("transfer-input:") {
        Some(("transfer", binding))
    } else if let Some(binding) = feature.strip_prefix("destroy-input:") {
        Some(("destroy", binding))
    } else if let Some(binding) = feature.strip_prefix("claim-input:") {
        Some(("claim", binding))
    } else if let Some(binding) = feature.strip_prefix("settle-input:") {
        Some(("settle", binding))
    } else if let Some(binding) = feature.strip_prefix("replace-unique-input:") {
        Some(("replace_unique", binding))
    } else {
        feature.strip_prefix("destroy-input:").map(|binding| ("destroy", binding))
    }
}

pub(super) fn transaction_runtime_input_requirements_from_obligations(
    obligations: &[VerifierObligationMetadata],
) -> Vec<TransactionRuntimeInputRequirementMetadata> {
    let mut requirements = Vec::new();
    for obligation in obligations {
        if obligation.category == "spawn-delegation" && obligation.status == "runtime-required" {
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "spawn-verifier-cell-dep",
                "runtime-required",
                Some("authenticate the selected CellDep ELF and audit its complete child validation path; successful WAIT alone is not proof of the child's claimed semantics"),
                Some("spawn-delegated-verifier-gap"),
                "CellDep",
                "spawn-verifier",
                Some("script"),
                "ckb-spawn-ipc",
                None,
            ));
            continue;
        }
        if obligation.category == "exec-delegation" && obligation.status == "runtime-required" {
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "exec-target-cell-dep",
                "runtime-required",
                Some("authenticate the selected CellDep ELF and audit its complete delegated validation path; parent post-EXEC checks are not executed"),
                Some("exec-delegated-verifier-gap"),
                "CellDep",
                "exec-target",
                Some("script"),
                "ckb-exec-process-replacement",
                None,
            ));
            continue;
        }
        if obligation.category == "spawn-target" && obligation.status == "runtime-required" {
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "spawn-target-cell-dep",
                "runtime-required",
                Some("spawn target script reference must be supplied by the transaction builder as a resolvable CellDep or DepGroup entry"),
                Some("spawn-target-cell-dep-gap"),
                "CellDep",
                "spawn-target",
                Some("script"),
                "ckb-spawn-cell-dep-script-reference",
                None,
            ));
            continue;
        }

        if let Some(binding) = mutable_state_obligation_binding(obligation) {
            if obligation.status == "runtime-required" {
                let field_equality_status = obligation_detail_status(obligation, "field equality");
                if field_equality_status.is_some_and(|status| status != "checked-runtime") {
                    requirements.push(transaction_runtime_input_requirement(
                        obligation,
                        "mutate-field-equality",
                        "runtime-required",
                        Some("mutable preserved-field equality is not fully verifier-covered"),
                        Some("state-field-equality-gap"),
                        "InputOutput",
                        binding,
                        Some("preserved-fields"),
                        "mutate-preserved-field-equality",
                        None,
                    ));
                }

                let field_transition_status = obligation_detail_status(obligation, "field transition");
                if field_transition_status.is_some_and(|status| status != "checked-runtime") {
                    requirements.push(transaction_runtime_input_requirement(
                        obligation,
                        "mutate-field-transition",
                        "runtime-required",
                        Some("mutable field transition formula is not fully verifier-covered"),
                        Some("state-transition-formula-gap"),
                        "InputOutput",
                        binding,
                        Some("transition-fields"),
                        "mutate-field-transition-policy",
                        None,
                    ));
                }
            }
            continue;
        }

        let include_checked_destroy_scan =
            obligation.status == "checked-runtime" && obligation.feature.starts_with("destroy-output-scan:");
        let include_checked_operation_input =
            obligation.status == "checked-runtime" && operation_input_feature(obligation.feature.as_str()).is_some();
        let include_checked_read_ref = obligation.status == "checked-runtime" && obligation.feature.starts_with("read-ref:");
        let include_checked_create_output = obligation.status == "checked-runtime" && obligation.feature.starts_with("create-output:");
        let include_checked_cell_metadata =
            obligation.status == "checked-runtime" && obligation.feature.starts_with("cell-metadata-equality:");
        let include_checked_unique_output = obligation.status == "checked-runtime"
            && (obligation.feature.starts_with("create-unique-output:") || obligation.feature.starts_with("replace-unique-output:"));
        let include_checked_unique_identity = obligation.status == "checked-runtime"
            && (obligation.feature.starts_with("create-unique-identity:")
                || obligation.feature.starts_with("replace-unique-identity:"));
        let include_checked_resource_conservation =
            obligation.status == "checked-runtime" && obligation.feature.starts_with("resource-conservation:");
        if obligation.category != "transaction-invariant"
            || (obligation.status != "runtime-required"
                && !include_checked_destroy_scan
                && !include_checked_operation_input
                && !include_checked_read_ref
                && !include_checked_create_output
                && !include_checked_cell_metadata
                && !include_checked_unique_output
                && !include_checked_unique_identity
                && !include_checked_resource_conservation)
        {
            continue;
        }
        if let Some(binding) = obligation.feature.strip_prefix("destroy-output-scan:") {
            let absence_status = if transaction_obligation_has_checked_subcondition(obligation, "destroy-output-absence") {
                "checked-runtime"
            } else {
                "runtime-required"
            };
            let output_scan_status = if transaction_obligation_has_checked_subcondition(obligation, "destroy-output-scan") {
                "checked-runtime"
            } else {
                "runtime-required"
            };
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "destroy-output-absence",
                absence_status,
                (absence_status == "runtime-required").then_some("destroy lowering has no executable output type-id absence scan"),
                (absence_status == "runtime-required").then_some("output-scan-gap"),
                "Output",
                binding,
                Some("ckb_type_script_hash-absence"),
                "destroy-output-scan-type-id",
                None,
            ));
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "destroy-output-scan",
                output_scan_status,
                (output_scan_status == "runtime-required")
                    .then_some("destroy lowering does not bind transaction output scan boundaries"),
                (output_scan_status == "runtime-required").then_some("output-scan-boundary-gap"),
                "Transaction",
                binding,
                Some("outputs"),
                "destroy-output-scan-transaction-boundary",
                None,
            ));
        } else if let Some((operation, binding)) = operation_input_feature(obligation.feature.as_str()) {
            let input_binding = binding.rsplit_once(':').map(|(_, binding)| binding).unwrap_or(binding);
            let component = format!("{operation}-input-data");
            let abi = format!("{operation}-load-cell-input");
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                &component,
                "checked-runtime",
                None,
                None,
                "Input",
                input_binding,
                Some("data"),
                &abi,
                None,
            ));
        } else if let Some(binding) = obligation.feature.strip_prefix("read-ref:") {
            let cell_dep_binding = binding.rsplit_once('#').map(|(binding, _)| binding).unwrap_or(binding);
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "read-ref-cell-dep-data",
                "checked-runtime",
                None,
                None,
                "CellDep",
                cell_dep_binding,
                Some("data"),
                "read-ref-load-cell-dep",
                None,
            ));
        } else if let Some(binding) = obligation.feature.strip_prefix("create-output:") {
            let output_binding = binding.rsplit_once(':').map(|(_, binding)| binding).unwrap_or(binding);
            let fields_status = obligation_detail_status(obligation, "create-output-fields").unwrap_or("runtime-required");
            let lock_status = obligation_detail_status(obligation, "create-output-lock").unwrap_or("runtime-required");
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "create-output-fields",
                fields_status,
                (fields_status == "runtime-required").then_some("create output field verifier is incomplete for this output shape"),
                (fields_status == "runtime-required").then_some("create-output-verification-gap"),
                "Output",
                output_binding,
                Some("fields"),
                "create-output-field-verifier",
                None,
            ));
            match lock_status {
                "checked-runtime" | "runtime-required" => {
                    requirements.push(transaction_runtime_input_requirement(
                        obligation,
                        "create-output-lock",
                        lock_status,
                        (lock_status == "runtime-required").then_some("create output lock binding is not fully verifier-covered"),
                        (lock_status == "runtime-required").then_some("create-output-lock-verification-gap"),
                        "Output",
                        output_binding,
                        Some("lock_hash"),
                        "create-output-lock-hash-32",
                        Some(32),
                    ));
                }
                _ => {}
            }
        } else if let Some(rest) = obligation.feature.strip_prefix("cell-metadata-equality:") {
            let Some((field_name, bindings)) = rest.split_once(':') else {
                continue;
            };
            let binding = bindings.to_string();
            let component = format!("cell-metadata-{}", field_name);
            let abi = format!(
                "cell-metadata-{}-equality-{}",
                field_name.replace('_', "-"),
                cell_metadata_field_byte_len(field_name).unwrap_or(0)
            );
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                &component,
                "checked-runtime",
                None,
                None,
                "InputOutput",
                &binding,
                Some(field_name),
                &abi,
                cell_metadata_field_byte_len(field_name),
            ));
        } else if let Some((operation, binding)) = obligation
            .feature
            .strip_prefix("create-unique-output:")
            .map(|binding| ("create-unique", binding))
            .or_else(|| obligation.feature.strip_prefix("replace-unique-output:").map(|binding| ("replace-unique", binding)))
        {
            let output_binding = binding.rsplit_once(':').map(|(_, binding)| binding).unwrap_or(binding);
            let fields_key = format!("{operation}-output-fields");
            let lock_key = format!("{operation}-output-lock");
            let fields_status = obligation_detail_status(obligation, &fields_key).unwrap_or("runtime-required");
            let lock_status = obligation_detail_status(obligation, &lock_key).unwrap_or("runtime-required");
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                &fields_key,
                fields_status,
                (fields_status == "runtime-required").then_some("unique output field verifier is incomplete for this output shape"),
                (fields_status == "runtime-required").then_some("unique-output-verification-gap"),
                "Output",
                output_binding,
                Some("fields"),
                &format!("{operation}-output-field-verifier"),
                None,
            ));
            if matches!(lock_status, "checked-runtime" | "runtime-required") {
                requirements.push(transaction_runtime_input_requirement(
                    obligation,
                    &lock_key,
                    lock_status,
                    (lock_status == "runtime-required").then_some("unique output lock binding is not fully verifier-covered"),
                    (lock_status == "runtime-required").then_some("unique-output-lock-verification-gap"),
                    "Output",
                    output_binding,
                    Some("lock_hash"),
                    &format!("{operation}-output-lock-hash-32"),
                    Some(32),
                ));
            }
        } else if let Some((operation, binding)) = obligation
            .feature
            .strip_prefix("create-unique-identity:")
            .map(|binding| ("create-unique", binding))
            .or_else(|| obligation.feature.strip_prefix("replace-unique-identity:").map(|binding| ("replace-unique", binding)))
        {
            let output_binding = binding.split(':').next().unwrap_or(binding);
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                &format!("{operation}-identity"),
                "checked-runtime",
                None,
                None,
                "Output",
                output_binding,
                Some("identity"),
                &format!("{operation}-identity-verifier"),
                None,
            ));
        } else if let Some(binding) = obligation.feature.strip_prefix("linear-collection:") {
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "linear-collection-ownership",
                "runtime-required",
                Some("cell-backed collection ownership is not backed by an executable linear collection model"),
                Some("linear-collection-ownership-gap"),
                "Transaction",
                binding,
                Some("collection-payload"),
                "cell-backed-collection-linear-ownership-model",
                None,
            ));
        } else if let Some(binding) = obligation.feature.strip_prefix("resource-conservation:") {
            requirements.push(transaction_runtime_input_requirement(
                obligation,
                "resource-conservation-proof",
                obligation.status.as_str(),
                (obligation.status == "runtime-required")
                    .then_some("resource conservation is not fully lowered for this consumed-input/created-output shape"),
                (obligation.status == "runtime-required").then_some("resource-conservation-proof-gap"),
                "Transaction",
                binding,
                Some("input-output-conservation"),
                "resource-conservation-consume-create-accounting",
                None,
            ));
        }
    }
    requirements
}

fn mutable_state_obligation_binding(obligation: &VerifierObligationMetadata) -> Option<&str> {
    match obligation.category.as_str() {
        "shared-state" => obligation.feature.strip_prefix("shared-mutation:"),
        "cell-state" => obligation.feature.strip_prefix("mutable-cell:"),
        _ => None,
    }
}

fn obligation_detail_status<'a>(obligation: &'a VerifierObligationMetadata, label: &str) -> Option<&'a str> {
    let needle = format!("{}=", label);
    let start = obligation.detail.find(&needle)? + needle.len();
    let suffix = &obligation.detail[start..];
    let end = suffix.find([';', ',', ')']).unwrap_or(suffix.len());
    Some(suffix[..end].trim())
}

fn transaction_obligation_has_checked_subcondition(obligation: &VerifierObligationMetadata, name: &str) -> bool {
    obligation.detail.contains(&format!("{}=checked-runtime", name))
}

fn transaction_runtime_input_requirement(
    obligation: &VerifierObligationMetadata,
    component: &str,
    status: &str,
    blocker: Option<&str>,
    blocker_class: Option<&str>,
    source: &str,
    binding: &str,
    field: Option<&str>,
    abi: &str,
    byte_len: Option<usize>,
) -> TransactionRuntimeInputRequirementMetadata {
    TransactionRuntimeInputRequirementMetadata {
        scope: obligation.scope.clone(),
        feature: obligation.feature.clone(),
        status: status.to_string(),
        component: component.to_string(),
        source: source.to_string(),
        binding: binding.to_string(),
        field: field.map(str::to_string),
        abi: abi.to_string(),
        byte_len,
        blocker: blocker.map(str::to_string),
        blocker_class: blocker_class.map(str::to_string),
    }
}

pub(super) fn destroy_group_output_absence_scan_is_checked(body: &ir::IrBody, _type_name: &str, binding: &str) -> bool {
    body.consume_set.iter().any(|pattern| pattern.operation == "destroy" && pattern.binding == binding && pattern.type_hash.is_some())
}

pub(super) fn body_mutable_cell_state_obligations(
    body: &ir::IrBody,
    params: &[ir::IrParam],
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    type_layouts: &MetadataTypeLayouts,
) -> Vec<TransactionResourceObligation> {
    let mut obligations = Vec::new();
    for param in params {
        let Some(type_name) = named_type_name(&param.ty) else {
            continue;
        };
        if !(param.is_mut || matches!(param.ty, ir::IrType::MutRef(_))) {
            continue;
        }
        let mutated_fields = mutate_set_field_summary(body, type_name, &param.name, type_layouts);
        let obligation_status = mutate_set_obligation_status(body, type_name, &param.name, type_layouts);
        match cell_type_kinds.get(type_name).copied() {
            Some(ir::IrTypeKind::Shared) => obligations.push(TransactionResourceObligation {
                category: "shared-state",
                feature: format!("shared-mutation:{}", type_name),
                status: obligation_status,
                detail: format!(
                    "Runtime verifier must bind mutable shared parameter '{}' to the consumed '{}' cell and proposed output, preserve type/lock identity, and prove the allowed field transition; current lowering exposes scheduler contention and mutate_set field summary ({})",
                    param.name, type_name, mutated_fields
                ),
            }),
            Some(ir::IrTypeKind::Resource | ir::IrTypeKind::Receipt) => obligations.push(TransactionResourceObligation {
                category: "cell-state",
                feature: format!("mutable-cell:{}", type_name),
                status: obligation_status,
                detail: format!(
                    "Runtime verifier must bind mutable cell parameter '{}' to the consumed '{}' cell and proposed output, preserve type/lock identity, and prove the allowed field transition; current lowering exposes mutate_set field summary ({})",
                    param.name, type_name, mutated_fields
                ),
            }),
            Some(ir::IrTypeKind::Struct) | None => {}
        }
    }
    obligations
}

fn mutate_set_field_summary(body: &ir::IrBody, type_name: &str, binding: &str, type_layouts: &MetadataTypeLayouts) -> String {
    body.mutate_set
        .iter()
        .find(|pattern| pattern.ty == type_name && pattern.binding == binding)
        .map(|pattern| {
            let transition_fields = if pattern.fields.is_empty() {
                "transition fields: none".to_string()
            } else {
                format!("transition fields: {}", pattern.fields.join(", "))
            };
            let preserved_fields = if pattern.preserved_fields.is_empty() {
                "preserved fields: none".to_string()
            } else {
                format!("preserved fields: {}", pattern.preserved_fields.join(", "))
            };
            format!(
                "Input#{} -> Output#{}; type_hash preservation=checked-runtime; lock_hash preservation=checked-runtime; field equality={}; field transition={}; {}; {}",
                pattern.input_index,
                pattern.output_index,
                mutate_field_equality_status(pattern, type_layouts),
                mutate_field_transition_status(pattern, type_layouts),
                transition_fields,
                preserved_fields
            )
        })
        .unwrap_or_else(|| "no mutate_set entry".to_string())
}

fn mutate_set_obligation_status(
    body: &ir::IrBody,
    type_name: &str,
    binding: &str,
    type_layouts: &MetadataTypeLayouts,
) -> &'static str {
    body.mutate_set
        .iter()
        .find(|pattern| pattern.ty == type_name && pattern.binding == binding)
        .map(|pattern| {
            if !pattern.fields.is_empty()
                && mutate_field_equality_status(pattern, type_layouts) == "checked-runtime"
                && mutate_field_transition_status(pattern, type_layouts) == "checked-runtime"
            {
                "checked-runtime"
            } else {
                "runtime-required"
            }
        })
        .unwrap_or("runtime-required")
}

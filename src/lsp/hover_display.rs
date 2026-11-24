use super::type_to_string;
use crate::ast::{Capability, CapabilityOperation, EnumDef, FnDef, ReceiptDef, TypeParam, ValueAbility};

pub(super) fn capability_hover(capabilities: &[Capability]) -> String {
    let rendered = if capabilities.is_empty() {
        "none".to_string()
    } else {
        Capability::ALL
            .into_iter()
            .filter(|capability| capabilities.contains(capability))
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "\n\nCapabilities (set v{}, entailment v{}): `{}`\n\nDerived rules: `destroy <= consume + burn`; `replace_unique <= replace + identity-preservation`. Authority is per resource and never inherited.",
        Capability::REGISTRY_VERSION,
        CapabilityOperation::ENTAILMENT_VERSION,
        rendered
    )
}

pub(super) fn receipt_flow_hover(receipt: &ReceiptDef, metadata: Option<&crate::CompileMetadata>) -> String {
    if let Some(type_metadata) =
        metadata.and_then(|metadata| metadata.types.iter().find(|type_metadata| type_metadata.name == receipt.name))
    {
        if type_metadata.flow_states.is_empty() {
            return String::new();
        }

        let transitions = if type_metadata.flow_transitions.is_empty() {
            "none".to_string()
        } else {
            type_metadata
                .flow_transitions
                .iter()
                .map(|transition| {
                    format!("{}[{}] -> {}[{}]", transition.from, transition.from_index, transition.to, transition.to_index)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        return format!(
            "\n\n**Flow metadata**\n\nState model: `{}`\n\nInitial: `{}`\n\nTerminals: `{}`\n\nTerminal discharge: `{}`\n\nTerminal evidence: `{}`\n\nStates: `{}`\n\nTransitions: `{}`",
            type_metadata.flow_state_model.as_deref().unwrap_or("unspecified"),
            type_metadata.flow_initial_state.as_deref().unwrap_or("legacy-undeclared"),
            if type_metadata.flow_terminal_states.is_empty() {
                "legacy-undeclared".to_string()
            } else {
                type_metadata.flow_terminal_states.join(", ")
            },
            type_metadata.flow_terminal_discharge.as_deref().unwrap_or("legacy-undeclared"),
            type_metadata.flow_terminal_evidence_tier.map_or("legacy-undeclared", crate::EvidenceTier::as_str),
            type_metadata.flow_states.join(" -> "),
            transitions
        );
    }

    String::new()
}

pub(super) fn payload_enum_hover(enum_def: &EnumDef, metadata: Option<&crate::CompileMetadata>) -> String {
    let variants = enum_def
        .variants
        .iter()
        .map(|variant| {
            if variant.fields.is_empty() {
                variant.name.clone()
            } else {
                format!("{}({})", variant.name, variant.fields.iter().map(type_to_string).collect::<Vec<_>>().join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut hover = format!(
        "```cellscript\nenum {}{}{} {{ {} }}\n```",
        enum_def.name,
        generic_params_hover(&enum_def.type_params),
        value_abilities_hover(&enum_def.abilities),
        variants
    );
    if let Some(layout) = metadata.and_then(|metadata| metadata.enum_layouts.iter().find(|layout| layout.name == enum_def.name)) {
        hover.push_str(&format!(
            "\n\n**Layout metadata**\n\nLayout: `{}`\n\nABI: `{}`\n\nStorage: `{}`\n\nTag: `{}` byte\n\nEncoded size: `{}` bytes\n\nLinear payload: `{}`",
            layout.layout,
            layout.abi,
            layout.storage,
            layout.tag_width_bytes,
            layout.encoded_size_bytes,
            layout.contains_linear_payload
        ));
    } else if enum_def.variants.iter().any(|variant| !variant.fields.is_empty()) {
        hover.push_str("\n\nFixed-width payload enum; generic templates use deterministic pre-IR monomorphization.");
    }
    hover
}

pub(super) fn generic_params_hover(params: &[TypeParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    format!(
        "<{}>",
        params
            .iter()
            .map(|param| {
                let mut value = if param.phantom { format!("phantom {}", param.name) } else { param.name.clone() };
                if !param.constraints.is_empty() {
                    value.push_str(": ");
                    if ValueAbility::is_fixed_value_profile(&param.constraints) {
                        value.push_str(ValueAbility::FIXED_VALUE_PROFILE_NAME);
                    } else {
                        value.push_str(&param.constraints.iter().map(|ability| ability.as_str()).collect::<Vec<_>>().join(" + "));
                    }
                }
                value
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub(super) fn value_abilities_hover(abilities: &[ValueAbility]) -> String {
    if abilities.is_empty() {
        String::new()
    } else {
        format!(" has {}", abilities.iter().map(|ability| ability.as_str()).collect::<Vec<_>>().join(", "))
    }
}

pub(super) fn type_validity_hover(name: &str, metadata: Option<&crate::CompileMetadata>) -> String {
    let Some(type_metadata) = metadata.and_then(|metadata| metadata.types.iter().find(|type_metadata| type_metadata.name == name))
    else {
        return String::new();
    };
    if type_metadata.validity_predicates.is_empty() {
        return String::new();
    }

    let predicates = type_metadata
        .validity_predicates
        .iter()
        .map(|predicate| {
            format!(
                "- `{}` — `{}`; create: `{}` ({}/{} paths); update: `{}` ({} paths)",
                predicate.expression,
                predicate.evidence_tier.as_str(),
                predicate.create_path_status,
                predicate.create_paths_checked,
                predicate.create_paths_selected,
                predicate.update_path_status,
                predicate.update_paths_selected
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n\n**Validity metadata**\n\n{predicates}")
}

pub(super) fn action_metadata_hover(name: &str, metadata: Option<&crate::CompileMetadata>) -> String {
    let Some(metadata) = metadata else {
        return String::new();
    };
    let Some(action) = metadata.actions.iter().find(|action| action.name == name) else {
        return String::new();
    };

    let fail_closed_features = if action.fail_closed_runtime_features.is_empty() {
        "none".to_string()
    } else {
        action.fail_closed_runtime_features.join(", ")
    };
    let ckb_features =
        if action.ckb_runtime_features.is_empty() { "none".to_string() } else { action.ckb_runtime_features.join(", ") };
    let accesses = if action.ckb_runtime_accesses.is_empty() {
        "none".to_string()
    } else {
        action
            .ckb_runtime_accesses
            .iter()
            .map(|access| format!("{}:{}#{} ({})", access.operation, access.source, access.index, access.binding))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let obligations = if action.verifier_obligations.is_empty() {
        "none".to_string()
    } else {
        action
            .verifier_obligations
            .iter()
            .map(|obligation| format!("{}:{} ({})", obligation.category, obligation.feature, obligation.status))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut detail = format!(
        "\n\n**Lowering metadata**\n\nEffect: `{}`\n\nELF compatible: `{}`\n\nStandalone runner compatible: `{}`\n\nFail-closed runtime features: `{}`\n\nCKB runtime features: `{}`\n\nCKB runtime accesses: `{}`\n\nVerifier obligations: `{}`",
        action.effects,
        action.elf_compatible,
        action.standalone_runner_compatible,
        fail_closed_features,
        ckb_features,
        accesses,
        obligations
    );
    let typed = &metadata.typed_semantics;
    let contract = &typed.foundation.entry_contract;
    let id = format!("action:{name}");
    let selected = contract.exact_entry == id
        || matches!(&contract.dispatch, cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(policy)
            if policy.variants.iter().any(|variant| variant.entry_id == id));
    if selected && let Some(cells) = &contract.cells {
        detail.push_str(&format!(
            "\n\n**Entry Cell bindings** `{}`\n\n**Coverage** `{}`",
            cells.bindings.as_str(),
            cells.coverage.as_str()
        ));
        if let Some(layout) = &cells.layout {
            detail.push_str(&format!("\n\n**Layout** `{}`", layout.schema));
        }
    }
    if let Some(entry) = typed.entries.iter().find(|entry| entry.id == id)
        && !entry.cell_bindings.is_empty()
    {
        detail.push_str("\n\n**Resolved Cell roles**\n");
        for binding in &entry.cell_bindings {
            detail.push_str(&format!("\n- `{}` at `{}`", binding.binding, binding.selector()));
        }
    }
    detail
}

pub(super) fn function_metadata_hover(name: &str, _function: &FnDef, metadata: Option<&crate::CompileMetadata>) -> String {
    if let Some(function) = metadata.and_then(|metadata| metadata.functions.iter().find(|function| function.name == name)) {
        return format!("\n\n**Inferred effect**: `{}`\n\nEvidence: `{}`", function.effects, function.effect_evidence_tier.as_str());
    }
    "\n\n**Effect**: inference evidence unavailable".to_string()
}

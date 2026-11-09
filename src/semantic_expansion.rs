//! Deterministic human view of the canonical typed semantic foundation.
//!
//! The rendering is intentionally not a hash input. Stable identities are
//! computed from canonical records before this view is produced.

use crate::error::{CompileError, Result};
use crate::CompileMetadata;

pub fn render(metadata: &CompileMetadata) -> Result<String> {
    let typed = &metadata.typed_semantics;
    let foundation = &typed.foundation;
    let mut lines = vec![
        format!("semantic-foundation {} version {}", foundation.schema, foundation.version),
        format!("module {}", typed.module),
        format!("edition {}", metadata.edition),
        format!("verifier-failure {} ordinary-return-abi=unchanged", typed.failure_semantics.as_str()),
        format!("core-semantic-id {}", foundation.identities.core_semantic_id),
        format!("entry-contract-id {}", foundation.identities.entry_contract_id),
        format!("artifact-contract-id {}", foundation.identities.artifact_contract_id),
        format!(
            "artifact-contract target={} format={} lowering={} typed={}",
            foundation.artifact_contract.target_profile,
            foundation.artifact_contract.artifact_format,
            foundation.artifact_contract.lowering_record_schema,
            foundation.artifact_contract.typed_semantics_schema,
        ),
        format!(
            "entry {} role={} trigger={} dispatch={} payload={} placement={}.{} source={}",
            foundation.entry_contract.exact_entry,
            foundation.entry_contract.script_role,
            foundation.entry_contract.trigger,
            dispatch_label(&foundation.entry_contract.dispatch),
            foundation.entry_contract.entry_payload_abi,
            foundation.entry_contract.witness_placement_abi,
            foundation.entry_contract.witness_placement_field,
            foundation.entry_contract.witness_placement_source,
        ),
    ];
    if let cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(policy) = &foundation.entry_contract.dispatch {
        lines.push(format!(
            "policy {} schema={} version={} resource={} layout={}",
            policy.artifact_name, policy.schema, policy.version, policy.resource, policy.resource_layout_hash,
        ));
        let selector = foundation
            .provenance
            .nodes
            .iter()
            .find(|node| node.id == policy.selector_node_id)
            .ok_or_else(|| CompileError::without_span("cannot render policy dispatch: its selector provenance node is missing"))?;
        let cellscript_artifact_checker::ValueProvenance::EntryWitness { field_path, group_witness_source, .. } = &selector.provenance
        else {
            return Err(CompileError::without_span("cannot render policy dispatch: its selector is not entry-witness provenance"));
        };
        lines.push(format!(
            "  selector {} source={} node={} unknown={}",
            field_path, group_witness_source, policy.selector_node_id, policy.unknown_selector,
        ));
        lines.push(format!("  limits records=1..{} witness-bytes<={}", policy.max_records, policy.max_witness_bytes));
        lines.push("  variants (numeric tag order)".to_string());
        for variant in &policy.variants {
            lines.push(format!(
                "    tag {} -> {} payload-schema={} group-inputs={} group-outputs={}",
                variant.tag, variant.entry_id, variant.payload_schema_hash, variant.input_count, variant.output_count,
            ));
        }
        if policy.common_checks.is_empty() {
            lines.push("  common-checks (ordered): none".to_string());
        } else {
            lines.push("  common-checks (declaration order)".to_string());
            for (index, entry) in policy.common_checks.iter().enumerate() {
                lines.push(format!("    {} -> {}", index + 1, entry));
            }
        }
    }
    lines.push("types".to_string());
    for ty in &typed.types {
        lines.push(format!(
            "  {} kind={} layout={} identity={} fields=[{}]",
            ty.name,
            ty.kind,
            ty.layout_hash,
            ty.identity_policy,
            ty.fields.iter().map(|field| format!("{}:{}@{}", field.name, field.ty, field.offset)).collect::<Vec<_>>().join(","),
        ));
    }
    lines.push("provenance".to_string());
    for node in &foundation.provenance.nodes {
        lines.push(format!("  {} {}", node.id, compact_json(&node.provenance)?));
    }
    for binding in &foundation.provenance.bindings {
        lines.push(format!("  bind {} local={} -> {}", binding.entry_id, binding.local_id, binding.node_id));
    }
    lines.push("roles".to_string());
    for role in &foundation.roles {
        lines.push(format!(
            "  {} binding={} type={} direction={} locality={} source={} selector={} cardinality={} script-role={} script-identity={} schema={} correspondence={}",
            role.role_id,
            role.binding,
            role.ty,
            role.direction,
            role.locality,
            role.source,
            role.selector,
            role.cardinality,
            role.lock_or_type_role,
            role.script_identity_policy,
            role.schema_identity,
            role.correspondence_policy,
        ));
    }
    lines.push("dispositions".to_string());
    for disposition in &foundation.dispositions {
        lines.push(format!(
            "  {} input={} output={} input-disposition={} output-origin={} enforcement={}",
            disposition.id,
            disposition.input_role.as_deref().unwrap_or("none"),
            disposition.output_role.as_deref().unwrap_or("none"),
            compact_json(&disposition.input)?,
            compact_json(&disposition.output)?,
            disposition.enforcement,
        ));
        lines.push(format!(
            "    envelope completeness={} data=[{}] object-identity={} lock={} type-script={} capacity={} cardinality={} correspondence={}",
            disposition.envelope.completeness,
            disposition
                .envelope
                .data_fields
                .iter()
                .map(|field| format!("{}={}", field.field, field.treatment))
                .collect::<Vec<_>>()
                .join(","),
            disposition.envelope.logical_identity,
            disposition.envelope.lock_script,
            disposition.envelope.type_script,
            disposition.envelope.capacity,
            disposition.envelope.cardinality,
            disposition.envelope.correspondence,
        ));
        if let Some(execution) = &disposition.execution {
            lines.push(format!(
                "    execution occurrence={} input-local={} output-local={} consume-block={} create-block={} success-block={}",
                execution.occurrence,
                execution.input_local,
                execution.output_local,
                execution.consume_block,
                execution.create_block,
                execution.success_block,
            ));
            for field in &execution.fields {
                lines.push(format!("      field {} value={}", field.field, compact_json(&field.value)?));
            }
            lines.push(format!("      lock-target={}", compact_json(&execution.lock_target)?));
            if let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) {
                render_successor_values(&mut lines, entry, execution)?;
            }
        }
    }
    lines.push("claims".to_string());
    for claim in &foundation.claims {
        lines.push(format!(
            "  {} category={} enforcement={} on-chain={} evidence={} execution={} statement={}",
            claim.id,
            claim.category,
            claim.enforcement,
            claim.on_chain_checked,
            claim.evidence_reference,
            compact_json(&claim.execution)?,
            claim.statement,
        ));
    }
    lines.push("operation-semantics".to_string());
    for operation in &foundation.operation_nodes {
        lines
            .push(
                format!("  {} kind={} meaning={} boundary={}", operation.id, operation.kind, operation.meaning, operation.boundary,),
            );
    }
    Ok(lines.join("\n") + "\n")
}

fn render_successor_values(
    lines: &mut Vec<String>,
    entry: &cellscript_artifact_checker::TypedSemanticEntry,
    execution: &cellscript_artifact_checker::SuccessorExecutionBinding,
) -> Result<()> {
    let mut pending = execution
        .fields
        .iter()
        .map(|field| &field.value)
        .chain(execution.lock_target.iter())
        .filter_map(|operand| operand.local)
        .collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
    while let Some(local) = pending.pop() {
        if !seen.insert(local) {
            continue;
        }
        if let Some(param) = entry.params.iter().find(|param| param.binding_id == local) {
            lines.push(format!("      value-local {local} parameter={} type={} source={}", param.name, param.ty, param.source));
        }
        for block in &entry.blocks {
            for operation in block.operations.iter().filter(|operation| operation.destinations.contains(&local)) {
                lines.push(format!("      value-local {local} block={} operation={}", block.id, compact_json(operation)?));
                pending.extend(operation.operands.iter().filter_map(|operand| operand.local));
            }
        }
    }
    Ok(())
}

fn dispatch_label(dispatch: &cellscript_artifact_checker::EntryDispatchContract) -> &'static str {
    match dispatch {
        cellscript_artifact_checker::EntryDispatchContract::SingleEntry => "single-entry",
        cellscript_artifact_checker::EntryDispatchContract::ExplicitVersionedDispatch { .. } => "explicit-versioned-dispatch",
        cellscript_artifact_checker::EntryDispatchContract::PolicyWitnessV1(_) => "policy-witness-v1",
    }
}

fn compact_json(value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map_err(|error| CompileError::without_span(format!("failed to render canonical semantic expansion: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compile_metadata, CellScriptEdition};

    #[test]
    fn expansion_is_deterministic_and_not_the_source_identity() {
        let first = compile_metadata(
            "module demo;\naction main(witness value: u64) -> u64 {  return value; }",
            CellScriptEdition::Edition2027,
            None,
        )
        .unwrap();
        let second = compile_metadata(
            "module demo;\n// formatting-only source change\naction main(witness value: u64) -> u64 {\n\nreturn value;\n}",
            CellScriptEdition::Edition2027,
            None,
        )
        .unwrap();

        assert_eq!(first.typed_semantics.foundation.identities, second.typed_semantics.foundation.identities);
        assert_ne!(first.source_content_hash, second.source_content_hash);
        assert_eq!(render(&first).unwrap(), render(&second).unwrap());
    }

    #[test]
    fn source_trivia_preserves_layered_semantic_ids() {
        let source = "module demo;\naction main(witness value: u64) -> u64 {  return value; }";
        let stable = compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
        let preview =
            compile_metadata(&source.replace("return value;", "/* same value */ return value;"), CellScriptEdition::Edition2027, None)
                .unwrap();

        assert_eq!(stable.compatibility_profile.id, preview.compatibility_profile.id);
        assert_ne!(stable.source_content_hash, preview.source_content_hash);
        assert_eq!(stable.typed_semantics.foundation.identities, preview.typed_semantics.foundation.identities);
    }

    #[test]
    fn core_identity_binds_computed_fields_lock_targets_and_helper_bodies() {
        let successor = r#"
module fingerprint;
resource Token has store, replace, relock { amount: u64, }
action update(input before: Token, witness owner: Address) -> after: Token {
    replace before -> after {
        data = same except { amount = before.amount + 1, };
        lock = exact(owner);
        type_script = same;
        capacity = same;
    }
}
"#;
        let fresh = r#"
module fingerprint;
resource Token has store, create { amount: u64, }
type_script Mint on type_group<Token> {
    action mint(
        witness left: Address from group_witness.input_type,
        witness right: Address from group_witness.input_type,
        output after: Token from group_output[0],
    ) {
        fresh after {
            data { amount = 7; }
            identity = none;
            type_script = declared;
            lock = exact(left);
            capacity = builder_computed;
            cardinality = one;
        }
    }
}
"#;
        let helper = "module fingerprint;\nfn increment(value: u64) -> u64 { return value + 1; }\naction main(witness value: u64) -> u64 { return increment(value); }";
        for (source, changed_source) in [
            (successor, successor.replace("before.amount + 1", "before.amount + 2")),
            (fresh, fresh.replace("exact(left)", "exact(right)")),
            (helper, helper.replace("value + 1", "value + 2")),
        ] {
            let first = compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
            let second = compile_metadata(&changed_source, CellScriptEdition::Edition2027, None).unwrap();
            assert_ne!(
                first.typed_semantics.foundation.identities.core_semantic_id,
                second.typed_semantics.foundation.identities.core_semantic_id,
                "executable values must be bound: {changed_source}"
            );
        }
    }

    #[test]
    fn enforced_condition_is_bound_into_the_core_semantic_identity() {
        let first = compile_metadata(
            "module demo;\naction main(witness value: u64) -> u64 {  require value > 0; return value; }",
            CellScriptEdition::Edition2027,
            None,
        )
        .unwrap();
        let second = compile_metadata(
            "module demo;\naction main(witness value: u64) -> u64 {  require value > 1; return value; }",
            CellScriptEdition::Edition2027,
            None,
        )
        .unwrap();

        let claim = first
            .typed_semantics
            .foundation
            .claims
            .iter()
            .find(|claim| claim.execution.is_some())
            .expect("require must emit an executable semantic claim");
        let execution = claim.execution.as_ref().unwrap();
        assert_eq!(claim.category, "entry-condition");
        assert_eq!(claim.statement, "require value > 0");
        assert_eq!(claim.enforcement, "checked-runtime");
        assert!(claim.on_chain_checked);
        assert!(first.typed_semantics.foundation.provenance.nodes.iter().any(|node| node.id == execution.condition_node_id));
        assert_ne!(
            first.typed_semantics.foundation.identities.core_semantic_id,
            second.typed_semantics.foundation.identities.core_semantic_id
        );
    }

    #[test]
    fn group_and_absolute_ports_have_distinct_core_semantics() {
        let legacy = r#"
module demo;
resource Token has store, replace, relock { owner: Address, amount: u64, }
action transfer(input token: Token, witness recipient: Address) -> next: Token {

        require token.amount > 0;
        replace token -> next {
            data { owner = same, amount = same, };
            lock = exact(recipient);
            type_script = same;
            capacity = same;
        }
}
"#;
        let native = r#"
module demo;
resource Token has store, replace, relock { owner: Address, amount: u64, }
type_script TokenTransfer on type_group<Token> {
    action transfer(
        input token: Token from group_input[0],
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        require token.amount > 0;
        replace token -> next {
            data { owner = same, amount = same, };
            type_script = same;
            lock = exact(recipient);
            capacity = same;
        }
    }

}
"#;
        let stable = compile_metadata(legacy, CellScriptEdition::Edition2027, None).unwrap();
        let preview = compile_metadata(native, CellScriptEdition::Edition2027, None).unwrap();

        assert_ne!(
            stable.typed_semantics.foundation.identities.core_semantic_id,
            preview.typed_semantics.foundation.identities.core_semantic_id
        );
        assert_ne!(
            stable.typed_semantics.foundation.identities.entry_contract_id,
            preview.typed_semantics.foundation.identities.entry_contract_id
        );
        let stable_entry = &stable.typed_semantics.entries[0];
        let preview_entry = &preview.typed_semantics.entries[0];
        assert_eq!(stable_entry.blocks, preview_entry.blocks);
        assert!(stable_entry
            .cell_bindings
            .iter()
            .any(|binding| binding.source == cellscript_artifact_checker::CellBindingSource::Input));
        assert!(preview_entry
            .cell_bindings
            .iter()
            .any(|binding| binding.source == cellscript_artifact_checker::CellBindingSource::GroupInput));
        assert_eq!(stable.actions[0].proof_plan, preview.actions[0].proof_plan);
        assert_eq!(stable.actions[0].fail_closed_runtime_features, preview.actions[0].fail_closed_runtime_features);
    }

    #[test]
    fn group_lock_and_ordinary_lock_share_semantic_ids() {
        let legacy = r#"
module demo;
resource Vault has store { owner: Address, }
lock unlock(protected vault: Vault, lock_args owner: Address, witness claimed_owner: Address) -> bool {

        require vault.owner == owner;
        require claimed_owner == owner;
        true
}
"#;
        let native = r#"
module demo;
resource Vault has store { owner: Address, }
lock_script VaultOwner on lock_group {
    lock unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
        true
    }
}
"#;
        let stable = compile_metadata(legacy, CellScriptEdition::Edition2027, None).unwrap();
        let preview = compile_metadata(native, CellScriptEdition::Edition2027, None).unwrap();

        assert_eq!(stable.typed_semantics.foundation.identities, preview.typed_semantics.foundation.identities);
        assert_eq!(stable.typed_semantics.entries, preview.typed_semantics.entries);
        assert_eq!(stable.locks[0].proof_plan, preview.locks[0].proof_plan);
        assert_eq!(stable.locks[0].fail_closed_runtime_features, preview.locks[0].fail_closed_runtime_features);
    }

    #[test]
    fn checked_one_to_one_resource_conservation_is_a_successor_disposition() {
        let source = r#"
module demo;
resource Token has store, replace, relock { owner: Address, amount: u64, }
action transfer(input token: Token, witness recipient: Address) -> next: Token {

        std::lifecycle::transfer(token, next, recipient) { owner, amount, }
}
"#;
        let metadata = compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
        let dispositions = &metadata.typed_semantics.foundation.dispositions;
        assert_eq!(dispositions.len(), 1);
        assert!(matches!(dispositions[0].input, Some(cellscript_artifact_checker::InputDisposition::Successor { .. })));
        assert!(matches!(dispositions[0].output, Some(cellscript_artifact_checker::OutputOrigin::SuccessorOf { .. })));
    }

    #[test]
    fn native_fresh_output_and_audit_are_explicit_and_identity_bound() {
        let first = r#"
module demo;
#[type_id("demo::Token:v1")]
resource Token has store, create, burn identity(ckb_type_id) { amount: u64, }
type_script TokenMint on type_group<Token> {
    action mint(
        witness amount: u64 from group_witness.input_type,
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        require amount > 0;
        audit issuance_policy {
            expected_evidence = external_policy(recipient);
        }
        fresh next {
            data { amount = amount; }
            identity = ckb_type_id;
            type_script = declared;
            lock = exact(recipient);
            capacity = builder_computed;
            cardinality = one;
        }
    }
}
"#;
        let second = first.replace("data { amount = amount; }", "data { amount = amount + 1; }");
        assert_ne!(second, first);
        let audit_changed = first.replace("external_policy(recipient)", "external_policy(amount)");
        let linear_audit = first.replace("external_policy(recipient)", "external_policy(next)");
        let metadata = compile_metadata(first, CellScriptEdition::Edition2027, None).unwrap();
        let changed = compile_metadata(&second, CellScriptEdition::Edition2027, None).unwrap();
        let audit_changed = compile_metadata(&audit_changed, CellScriptEdition::Edition2027, None).unwrap();

        let dispositions = &metadata.typed_semantics.foundation.dispositions;
        assert_eq!(dispositions.len(), 1);
        assert!(matches!(
            dispositions[0].output,
            Some(cellscript_artifact_checker::OutputOrigin::Fresh { ref identity_policy }) if identity_policy == "ckb-type-id"
        ));
        assert!(metadata.typed_semantics.foundation.operation_nodes.is_empty());
        let audit =
            metadata.typed_semantics.foundation.claims.iter().find(|claim| claim.category == "audit").expect("native audit claim");
        assert_eq!(audit.enforcement, "metadata-only");
        assert!(!audit.on_chain_checked);
        assert!(audit.execution.is_none());
        assert_eq!(audit.evidence_reference, "audit:external-policy");
        assert_ne!(
            metadata.typed_semantics.foundation.identities.core_semantic_id,
            changed.typed_semantics.foundation.identities.core_semantic_id,
            "changing a fresh-output field expression must change the core semantic identity"
        );
        assert_ne!(
            metadata.typed_semantics.foundation.identities.core_semantic_id,
            audit_changed.typed_semantics.foundation.identities.core_semantic_id,
            "changing an external-policy audit subject must change the core semantic identity"
        );
        assert!(compile_metadata(&linear_audit, CellScriptEdition::Edition2027, None)
            .unwrap_err()
            .message
            .contains("cannot capture a Cell-backed linear value"));
    }

    #[test]
    fn native_retirement_has_no_legacy_ambiguity_node() {
        let source = r#"
module demo;
resource Note has store, consume, burn identity(field(note_id)) { note_id: u64, amount: u64, }
type_script NoteRetirement on type_group<Note> {
    action retire_note(
        input note: Note from group_input[0],
    ) {
        require note.amount == 0;
        retire note {
            absence = field(note_id);
            data = discarded;
            lock = none;
            type_script = absent;
            capacity = released;
            cardinality = one;
        }
    }
}
"#;
        let metadata = compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
        let dispositions = &metadata.typed_semantics.foundation.dispositions;
        assert_eq!(dispositions.len(), 1);
        assert!(matches!(
            dispositions[0].input,
            Some(cellscript_artifact_checker::InputDisposition::Retired { ref absence_policy })
                if absence_policy == "same-field-identity-output-absent:note_id"
        ));
        assert_eq!(dispositions[0].envelope.completeness, "exhaustive");
        assert!(metadata.typed_semantics.foundation.operation_nodes.is_empty());
    }

    #[test]
    fn native_pool_has_checked_many_to_many_accounting_and_explicit_results() {
        let source = r#"
module demo;
resource Token has store, create, consume { owner: Address, amount: u64, }
type_script TokenPool on type_group<Token> {
    action merge(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        witness recipient: Address from group_witness.input_type,
        output merged: Token from group_output[0],
    ) {
        require left.amount > 0;
        pool value_flow {
            inputs { left, right }
            outputs { merged }
            data {
                owner { merged = recipient; }
                amount = conserve;
            }
            identity = pooled;
            type_script = same;
            lock { merged = exact(recipient); }
            capacity = builder_computed;
            cardinality = declared;
        }
    }
}
"#;
        let metadata = compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
        let foundation = &metadata.typed_semantics.foundation;
        assert_eq!(foundation.dispositions.len(), 3);
        assert_eq!(
            foundation
                .dispositions
                .iter()
                .filter(|disposition| matches!(disposition.input, Some(cellscript_artifact_checker::InputDisposition::Pooled { .. })))
                .count(),
            2
        );
        assert_eq!(
            foundation
                .dispositions
                .iter()
                .filter(|disposition| matches!(disposition.output, Some(cellscript_artifact_checker::OutputOrigin::PoolResult { .. })))
                .count(),
            1
        );
        assert!(foundation.claims.iter().any(|claim| {
            claim.on_chain_checked
                && claim.statement == "require left.amount as u128 + right.amount as u128 == merged.amount as u128"
                && claim.execution.is_some()
        }));
        assert!(foundation.operation_nodes.is_empty());

        let changed = source.replace("owner { merged = recipient; }", "owner { merged = left.owner; }");
        assert_ne!(changed, source);
        let changed = compile_metadata(&changed, CellScriptEdition::Edition2027, None).unwrap();
        assert_ne!(
            foundation.identities.core_semantic_id, changed.typed_semantics.foundation.identities.core_semantic_id,
            "changing a pool-result field expression must change CoreSemanticId"
        );

        let non_numeric = source.replace("amount: u64", "amount: bool").replace("require left.amount > 0;", "");
        let error = compile_metadata(&non_numeric, CellScriptEdition::Edition2027, None).unwrap_err();
        assert!(error.message.contains("cast") || error.message.contains("numeric"), "unexpected diagnostic: {}", error.message);
    }
}

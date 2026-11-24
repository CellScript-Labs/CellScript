//! Semantic entry contract for the bounded persistent Type policy dispatcher.

use super::*;
use cellscript_artifact_checker::{
    PolicyWitnessContract, PolicyWitnessVariant, POLICY_DISPATCH_SCHEMA, POLICY_DISPATCH_VERSION, POLICY_SELECTOR_FIELD,
};

pub(super) fn build_entry_contract(
    module: &ir::IrModule,
    types: &[TypedSemanticType],
    entries: &[TypedSemanticEntry],
    provenance: &mut ProvenanceGraph,
) -> Option<ArtifactEntryContract> {
    let ir::IrEntrySelection::Artifact(declaration) = &module.entry_selection else { return None };
    let selector = ValueProvenance::EntryWitness {
        placement_abi: crate::artifact::POLICY_WITNESS_PLACEMENT_ABI.to_string(),
        payload_abi: crate::policy_witness::POLICY_WITNESS_ABI.to_string(),
        group_witness_source: crate::artifact::POLICY_WITNESS_PLACEMENT_SOURCE.to_string(),
        field_path: POLICY_SELECTOR_FIELD.to_string(),
    };
    let selector_node_id = canonical_hash("cellscript-value-provenance-node-v1", &selector).expect("policy selector is serializable");
    provenance.nodes.push(ProvenanceNode { id: selector_node_id.clone(), provenance: selector });
    provenance.canonicalize();
    let variants = declaration
        .actions
        .iter()
        .map(|variant| {
            let entry_id = format!("action:{}", variant.action);
            let entry = entries.iter().find(|entry| entry.id == entry_id).expect("policy export was resolved before metadata");
            let input_count = entry
                .cell_bindings
                .iter()
                .filter(|binding| binding.source == CellBindingSource::GroupInput)
                .map(|binding| binding.ordinal)
                .collect::<BTreeSet<_>>()
                .len() as u32;
            let output_count = entry
                .cell_bindings
                .iter()
                .filter(|binding| binding.source == CellBindingSource::GroupOutput)
                .map(|binding| binding.ordinal)
                .collect::<BTreeSet<_>>()
                .len() as u32;
            PolicyWitnessVariant {
                tag: variant.tag,
                entry_id,
                input_count,
                output_count,
                payload_schema_hash: canonical_hash("cellscript-policy-variant-payload-v1", &entry.params)
                    .expect("policy parameter schema is serializable"),
            }
        })
        .collect();
    let policy = PolicyWitnessContract {
        schema: POLICY_DISPATCH_SCHEMA.to_string(),
        version: POLICY_DISPATCH_VERSION,
        artifact_name: declaration.name.clone(),
        selector_node_id,
        variants,
        common_checks: declaration.common_checks.iter().map(|name| format!("action:{name}")).collect(),
        max_records: crate::policy_witness::MAX_POLICY_WITNESS_RECORDS as u32,
        max_witness_bytes: crate::policy_witness::MAX_POLICY_WITNESS_BYTES as u32,
        unknown_selector: "reject".to_string(),
    };
    let mut contract = ArtifactEntryContract {
        semantic_node_id: String::new(),
        script_role: "type".to_string(),
        trigger: "type-group".to_string(),
        exact_entry: "wrapper:_cellscript_entry".to_string(),
        cells: Some(entry_cell_contract(declaration.bindings, declaration.coverage, declaration.layout.as_deref(), types)),
        dispatch: EntryDispatchContract::PolicyWitnessV1(policy),
        entry_payload_abi: crate::policy_witness::POLICY_WITNESS_ABI.to_string(),
        witness_placement_abi: crate::artifact::POLICY_WITNESS_PLACEMENT_ABI.to_string(),
        witness_placement_field: crate::artifact::POLICY_WITNESS_PLACEMENT_FIELD.to_string(),
        witness_placement_source: crate::artifact::POLICY_WITNESS_PLACEMENT_SOURCE.to_string(),
    };
    contract.semantic_node_id = entry_contract_id(&contract);
    Some(contract)
}

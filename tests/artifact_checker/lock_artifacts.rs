//! Rehashed Lock artifact mutations must reach independent contract validation.
use super::*;
use cellscript::artifact::{compile_artifact, ArtifactDeclaration};
use cellscript_artifact_checker::{
    CellBindingMembership, CellBindingSource, EntryCellBindings, EntryCellLayout, EntryCoverage, ProvenanceBinding, ProvenanceNode,
    TypedSemanticCellBinding,
};
use std::sync::OnceLock;

fn valid() -> Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            Fixture::from_result(
                compile_artifact(
                    include_str!("../syntax_combo/authoring/lock-artifact.cell"),
                    CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
                    ArtifactDeclaration::single_lock_group("vault", "unlock"),
                    cellscript::ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap(),
            )
        })
        .clone()
}

fn rebind_contract(fixture: &mut Fixture) {
    let contract = &mut fixture.record.typed_semantics.foundation.entry_contract;
    let previous = contract.semantic_node_id.clone();
    contract.semantic_node_id = canonical_hash(
        "cellscript-semantic-node-entry-contract-v3",
        &(
            contract.script_role.as_str(),
            contract.trigger.as_str(),
            contract.exact_entry.as_str(),
            contract.cells.as_ref().unwrap(),
            &contract.dispatch,
            contract.entry_payload_abi.as_str(),
            contract.witness_placement_abi.as_str(),
            contract.witness_placement_field.as_str(),
            contract.witness_placement_source.as_str(),
        ),
    )
    .unwrap();
    for mapping in &mut fixture.source_map.semantic_mappings {
        if mapping.semantic_node_id == previous {
            mapping.semantic_node_id.clone_from(&contract.semantic_node_id);
        }
    }
    fixture.source_map.canonicalize();
    fixture.record.canonicalize();
    fixture.rebind_layered_identities();
}

fn rejected(fixture: &Fixture, expected: &str) {
    let error =
        check_bundle_values(&fixture.artifact, &fixture.metadata, &fixture.record, &fixture.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
    assert!(error.message.contains(expected), "{error}");
}

fn rebind_physical_projection(fixture: &mut Fixture, binding: &TypedSemanticCellBinding) {
    let entry_id = "lock:unlock";
    let foundation = &mut fixture.record.typed_semantics.foundation;
    let role = foundation.roles.iter_mut().find(|role| role.role_id == binding.role_id(entry_id)).unwrap();
    let previous = role.semantic_node_id.clone();
    role.source = binding.source_scope().into();
    role.selector = binding.selector();
    role.script_identity_policy = binding.membership_policy().into();
    role.semantic_node_id = canonical_hash(
        "cellscript-semantic-node-role-v1",
        &(
            role.role_id.as_str(),
            role.entry_id.as_str(),
            role.binding.as_str(),
            role.ty.as_str(),
            role.direction.as_str(),
            role.source.as_str(),
            role.selector.as_str(),
            role.cardinality.as_str(),
            role.lock_or_type_role.as_str(),
            role.script_identity_policy.as_str(),
            role.schema_identity.as_str(),
            role.correspondence_policy.as_str(),
        ),
    )
    .unwrap();
    for mapping in &mut fixture.source_map.semantic_mappings {
        if mapping.semantic_node_id == previous {
            mapping.semantic_node_id.clone_from(&role.semantic_node_id);
        }
    }
    // Keep prior roots used by derived operations, while giving the forged
    // physical binding its own coherent, independently hashed root as well.
    let provenance = binding.provenance(entry_id);
    let node_id = canonical_hash("cellscript-value-provenance-node-v1", &provenance).unwrap();
    foundation.provenance.nodes.push(ProvenanceNode { id: node_id.clone(), provenance });
    foundation.provenance.bindings.push(ProvenanceBinding { entry_id: entry_id.into(), local_id: binding.local_id.unwrap(), node_id });
    foundation.provenance.canonicalize();
}

#[test]
fn lock_artifact_rehashed_coverage_bindings_and_layout_forgery_is_rejected() {
    for mutation in ["coverage", "bindings", "layout"] {
        let mut fixture = valid();
        let schema = &fixture.record.typed_semantics.types[0];
        let layout = EntryCellLayout { schema: schema.name.clone(), layout_hash: schema.layout_hash.clone() };
        let cells = fixture.record.typed_semantics.foundation.entry_contract.cells.as_mut().unwrap();
        match mutation {
            "coverage" => cells.coverage = EntryCoverage::ActionDefined,
            "bindings" => cells.bindings = EntryCellBindings::TransactionAbsolute,
            "layout" => cells.layout = Some(layout),
            _ => unreachable!(),
        }
        rebind_contract(&mut fixture);
        rejected(&fixture, "unsupported entry Cell binding, coverage or dispatch combination");
    }
}

#[test]
fn lock_artifact_rehashed_ports_and_protected_coordinates_are_rejected() {
    for mutation in ["source", "ordinal", "absolute-source", "membership", "param-source", "mutable"] {
        let mut fixture = valid();
        let entry = fixture.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == "lock:unlock").unwrap();
        let param = entry.params.iter_mut().find(|param| param.source == "protected").unwrap();
        let binding = entry.cell_bindings.iter_mut().find(|binding| binding.binding == param.name).unwrap();
        match mutation {
            "source" => binding.source = CellBindingSource::Input,
            "ordinal" => binding.ordinal = 1,
            "absolute-source" => {
                binding.source = CellBindingSource::Input;
                binding.membership = CellBindingMembership::Unproven;
            }
            "membership" => binding.membership = CellBindingMembership::Unproven,
            "param-source" => param.source = "input".into(),
            "mutable" => param.mutable = true,
            _ => unreachable!(),
        }
        let physical = binding.clone();
        if matches!(mutation, "ordinal" | "absolute-source") {
            rebind_physical_projection(&mut fixture, &physical);
        }
        rebind_contract(&mut fixture);
        let expected = match mutation {
            "source" | "membership" => "confuses source, role or Script membership",
            _ => "Lock authorization entry has inconsistent ports or protected source",
        };
        rejected(&fixture, expected);
    }
}

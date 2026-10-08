//! Real compiler/checker boundary mutations. Rebinding sidecar and semantic
//! identities must not turn a contradictory builder/policy projection valid.
//! The machine cases cover the exact bounded policy wrapper and adapters; they
//! do not establish arbitrary program equivalence or action predicate meaning.

use cellscript::artifact::{ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
use cellscript::{
    compile_path_with_executable_surface_policy, CellScriptEdition, CompileEntryScope, CompileOptions, ExecutableSurfacePolicy,
};
use cellscript_artifact_checker::{
    canonical_hash, check_bundle_values, parse_elf, CheckerBudgets, CheckerError, CheckerRejectionCode, EntryDispatchContract,
    PolicyWitnessContract, SourceArtifactMap, ValueProvenance, VerifiedLoweringRecord, LOWERING_RECORD_SCHEMA, SOURCE_MAP_SCHEMA,
    TYPED_SEMANTICS_SCHEMA,
};
use serde_json::Value;

const SOURCE: &str = r#"
module policy_artifact_checker
resource Token has store, consume { amount: u64 }
action check_z() { verification require true }
action check_a() { verification require true }
action mint(witness amount: u64, witness recipient: Address) {
    verification
    require amount > 0
    create Token { amount: amount } with_lock(recipient)
}
action burn(input token: Token) { verification consume token }
"#;

const STACK_ARGS_SOURCE: &str = r#"
module policy_stack_args
resource Token has store, consume { amount: u64 }
action mint(
    witness amount: u64, witness p1: u64, witness p2: u64,
    witness p3: u64, witness p4: u64, witness p5: u64,
    witness p6: u64, witness p7: u64, witness p8: u64,
    witness recipient: Address
) {
    verification
    require amount > 0
    create Token { amount: amount } with_lock(recipient)
}
"#;

fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        name: "token-policy".to_string(),
        context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 40, action: "burn".to_string() }, ArtifactAction { tag: 10, action: "mint".to_string() }],
        common_checks: vec!["check_z".to_string(), "check_a".to_string()],
    }
}

#[derive(Clone)]
struct Fixture {
    artifact: Vec<u8>,
    metadata: Value,
    record: VerifiedLoweringRecord,
    source_map: SourceArtifactMap,
}

impl Fixture {
    fn new(edition: CellScriptEdition) -> Self {
        Self::new_with(edition, 0, declaration())
    }

    fn new_with(edition: CellScriptEdition, opt_level: u8, declaration: ArtifactDeclaration) -> Self {
        Self::new_source_with(SOURCE, edition, opt_level, declaration)
    }

    fn new_source_with(source_text: &str, edition: CellScriptEdition, opt_level: u8, declaration: ArtifactDeclaration) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.cell");
        std::fs::write(&source, source_text).unwrap();
        let result = compile_path_with_executable_surface_policy(
            source.to_str().unwrap(),
            CompileOptions { edition, opt_level, target: Some("riscv64-elf".to_string()), ..CompileOptions::default() },
            Some(CompileEntryScope::Artifact(declaration)),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let fixture = Self {
            artifact: result.artifact_bytes,
            metadata: serde_json::to_value(result.metadata).unwrap(),
            record: result.verified_lowering_record.unwrap(),
            source_map: result.source_artifact_map.unwrap(),
        };
        fixture.check().unwrap();
        fixture
    }

    fn check(&self) -> Result<(), CheckerError> {
        check_bundle_values(&self.artifact, &self.metadata, &self.record, &self.source_map, &CheckerBudgets::default()).map(|_| ())
    }

    fn assert_interface_inspection(&self) {
        cellscript_artifact_checker::interface::inspect_bundle(
            &self.artifact,
            &serde_json::to_vec(&self.metadata).unwrap(),
            &serde_json::to_vec(&self.record).unwrap(),
            &serde_json::to_vec(&self.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .expect("unmutated compiler-produced interface must pass inspection");
    }

    fn policy_mut(&mut self) -> &mut PolicyWitnessContract {
        let EntryDispatchContract::PolicyWitnessV1(policy) = &mut self.record.typed_semantics.foundation.entry_contract.dispatch
        else {
            panic!("expected policy dispatch");
        };
        policy
    }

    fn param_mut(&mut self, action: &str, index: usize) -> &mut Value {
        &mut self.metadata["actions"].as_array_mut().unwrap().iter_mut().find(|entry| entry["name"] == action).unwrap()["params"]
            [index]
    }

    fn rebind_sidecars(&mut self) {
        let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, &self.record).unwrap();
        self.source_map.lowering_record_hash = record_hash.clone();
        let source_map_hash = canonical_hash(SOURCE_MAP_SCHEMA, &self.source_map).unwrap();
        let verified_bundle_id = canonical_hash(
            "cellscript-verified-bundle-id-v1",
            &(
                self.record.artifact_hash.as_str(),
                self.record.typed_semantics_hash.as_str(),
                self.record.compatibility_profile_hash.as_str(),
                record_hash.as_str(),
                source_map_hash.as_str(),
                self.source_map.source_digest.as_str(),
            ),
        )
        .unwrap();
        self.metadata["verified_artifact"]["lowering_record_hash"] = record_hash.into();
        self.metadata["verified_artifact"]["source_map_hash"] = source_map_hash.into();
        self.metadata["verified_artifact"]["verified_bundle_id"] = verified_bundle_id.into();
    }

    fn rebind_interface_identity(&mut self) {
        let canonical = cellscript::package::registry::canonical_json_value(&self.metadata["public_interface"]);
        let hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ));
        self.metadata["interface_hash"] = hash.clone().into();
        self.record.typed_semantics.interface_hash = hash;
        self.rebind_policy_identity();
    }

    fn bind_artifact_identity(&mut self) {
        let artifact_hash = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&self.artifact));
        self.record.artifact_hash.clone_from(&artifact_hash);
        self.record.artifact_size_bytes = self.artifact.len() as u64;
        self.source_map.artifact_hash.clone_from(&artifact_hash);
        self.metadata["artifact_hash"] = artifact_hash.clone().into();
        self.metadata["artifact_size_bytes"] = (self.artifact.len() as u64).into();
        self.metadata["verified_artifact"]["deployable_artifact_id"] = artifact_hash.into();
        self.rebind_sidecars();
    }

    fn replace_machine_word(&mut self, address: u64, word: u32) {
        let elf = parse_elf(&self.artifact, CheckerBudgets::default().instructions).unwrap();
        let offset = (elf.text.offset + address - elf.text.address) as usize;
        self.artifact[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        for block in &mut self.record.blocks {
            let start = (elf.text.offset + block.range.start - elf.text.address) as usize;
            let end = (elf.text.offset + block.range.end - elf.text.address) as usize;
            block.byte_digest =
                cellscript_artifact_checker::domain_hash_bytes("cellscript-machine-block-v1", &self.artifact[start..end]);
        }
        self.bind_artifact_identity();
    }

    fn rebind_policy_identity(&mut self) {
        let typed = &mut self.record.typed_semantics;
        typed.canonicalize();
        let foundation = &mut typed.foundation;
        let contract = &mut foundation.entry_contract;
        let previous_node = contract.semantic_node_id.clone();
        contract.semantic_node_id = canonical_hash(
            "cellscript-semantic-node-entry-contract-v2",
            &(
                contract.script_role.as_str(),
                contract.trigger.as_str(),
                contract.exact_entry.as_str(),
                &contract.dispatch,
                contract.entry_payload_abi.as_str(),
                contract.witness_placement_abi.as_str(),
                contract.witness_placement_field.as_str(),
                contract.witness_placement_source.as_str(),
            ),
        )
        .unwrap();
        for mapping in &mut self.source_map.semantic_mappings {
            if mapping.semantic_node_id == previous_node {
                mapping.semantic_node_id = contract.semantic_node_id.clone();
            }
        }
        let roots = foundation
            .provenance
            .nodes
            .iter()
            .filter(|node| !matches!(node.provenance, ValueProvenance::Derived { .. }))
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        foundation.identities.core_semantic_id = canonical_hash(
            "cellscript-core-semantic-id-v2",
            &(
                typed.failure_semantics,
                &typed.types,
                &foundation.roles,
                &foundation.dispositions,
                &foundation.claims,
                &foundation.legacy_nodes,
            ),
        )
        .unwrap();
        foundation.identities.entry_contract_id = canonical_hash(
            "cellscript-entry-contract-id-v1",
            &(
                foundation.identities.core_semantic_id.as_str(),
                &foundation.entry_contract,
                roots,
                foundation.entry_contract.entry_payload_abi.as_str(),
                foundation.entry_contract.witness_placement_abi.as_str(),
            ),
        )
        .unwrap();
        foundation.identities.artifact_contract_id = canonical_hash(
            "cellscript-artifact-contract-id-v1",
            &(foundation.identities.entry_contract_id.as_str(), &foundation.artifact_contract),
        )
        .unwrap();
        self.source_map.canonicalize();
        self.metadata["verified_artifact"]["core_semantic_id"] = foundation.identities.core_semantic_id.clone().into();
        self.metadata["verified_artifact"]["entry_contract_id"] = foundation.identities.entry_contract_id.clone().into();
        self.metadata["verified_artifact"]["artifact_contract_id"] = foundation.identities.artifact_contract_id.clone().into();
        self.record.typed_semantics_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, typed).unwrap();
        self.metadata["typed_semantics"] = serde_json::to_value(typed).unwrap();
        self.metadata["typed_semantics_hash"] = self.record.typed_semantics_hash.clone().into();
        self.rebind_sidecars();
    }
}

fn policy_block<'a>(fixture: &'a Fixture, prefix: &str) -> &'a cellscript_artifact_checker::LoweringBlock {
    fixture
        .record
        .blocks
        .iter()
        .find(|block| {
            block.owner_entry == "wrapper:_cellscript_entry"
                && block.machine_label.as_deref().is_some_and(|label| {
                    label
                        .strip_prefix(prefix)
                        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
                })
        })
        .unwrap()
}

#[test]
fn real_policy_bundle_and_unchanged_identity_rebinding_are_valid_in_both_editions() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let mut fixture = Fixture::new(edition);
        let original_record = canonical_hash(LOWERING_RECORD_SCHEMA, &fixture.record).unwrap();
        fixture.rebind_policy_identity();
        assert_eq!(canonical_hash(LOWERING_RECORD_SCHEMA, &fixture.record).unwrap(), original_record);
        fixture.check().unwrap();
    }
}

#[test]
fn interface_inspection_rejects_unknown_fields_after_all_outer_hashes_are_rebound() {
    let baseline = Fixture::new(CellScriptEdition::Edition2027);
    let inspect = |fixture: &Fixture| {
        cellscript_artifact_checker::interface::inspect_bundle(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
    };
    inspect(&baseline).unwrap();
    for nested in [false, true] {
        let mut changed = baseline.clone();
        if nested {
            changed.metadata["public_interface"]["types"][0]["fields"][0]["unrecognized"] = true.into();
        } else {
            changed.metadata["public_interface"]["unrecognized"] = true.into();
        }
        changed.rebind_interface_identity();
        // Existing artifact binding accepts this otherwise consistent record.
        // The new projection must reject losing the unknown declaration field.
        changed.check().unwrap();
        let error = inspect(&changed).unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("discard unknown fields"));
    }
}

#[test]
fn interface_inspection_recomputes_inner_digests_and_profile_bindings_after_rebinding() {
    let baseline = Fixture::new(CellScriptEdition::Edition2027);
    for pointer in [
        "/module_identity",
        "/types/0/layout_identity",
        "/types/0/fields/0/type",
        "/callables/0/builder_contract_hash",
        "/callables/0/entry_witness_abi",
        "/builder_contract_hash",
        "/deployment_contract_hash",
        "/runtime_contract/vm_abi",
        "/runtime_contract/source_encoding",
        "/runtime_contract/compatibility_profile_id",
        "/runtime_contract/temporal/migration",
    ] {
        let mut changed = baseline.clone();
        *changed.metadata["public_interface"].pointer_mut(pointer).unwrap() = "substituted".into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{pointer}: {error}");
    }
}

#[test]
fn interface_layout_cannot_be_forged_by_rebinding_its_inner_and_outer_hashes() {
    let baseline = Fixture::new(CellScriptEdition::Edition2027);
    baseline.assert_interface_inspection();
    for mutation in ["offset", "encoded_size", "omit", "duplicate", "rename", "kind", "same-width-type"] {
        let mut changed = baseline.clone();
        let ty = &mut changed.metadata["public_interface"]["types"][0];
        match mutation {
            "offset" | "encoded_size" => ty["fields"][0][mutation] = serde_json::json!(1024),
            "omit" => ty["fields"] = serde_json::json!([]),
            "duplicate" => {
                let field = ty["fields"][0].clone();
                ty["fields"].as_array_mut().unwrap().push(field);
            }
            "rename" => ty["fields"][0]["name"] = serde_json::json!("forged"),
            "kind" => ty["kind"] = serde_json::json!("struct"),
            "same-width-type" => ty["fields"][0]["type"] = serde_json::json!("[u8; 8]"),
            _ => unreachable!(),
        }
        let layout = serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]);
        let canonical = cellscript::package::registry::canonical_json_value(&layout);
        ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("checked concrete layout"));
    }
}

#[test]
fn generic_call_alias_matching_rejects_same_width_distinct_nominal_values() {
    let source = r#"
module nominal_call_contract
private struct Pair<T: fixed_value> { left: T, right: T }
private struct Twin<T: fixed_value> { left: T, right: T }
private fn inspect_pair(value: Pair<u64>) -> u64 { value.left }
action verify(witness pair: Pair<u64>, witness twin: Twin<u64>) {
    verification
    require inspect_pair(pair) == pair.left
}
"#;
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions { opt_level: 0, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let baseline = Fixture {
        artifact: compiled.artifact_bytes,
        metadata: serde_json::to_value(compiled.metadata).unwrap(),
        record: compiled.verified_lowering_record.unwrap(),
        source_map: compiled.source_artifact_map.unwrap(),
    };
    baseline.check().unwrap();
    let mut changed = baseline.clone();
    let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "verify").unwrap();
    let twin = entry.params.iter().find(|param| param.name == "twin").unwrap().clone();
    let call = entry
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "inspect_pair"))
        .unwrap();
    call.operands[0].local = Some(twin.binding_id);
    call.operands[0].ty = twin.ty;
    changed.record.typed_semantics_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, &changed.record.typed_semantics).unwrap();
    changed.metadata["typed_semantics"] = serde_json::to_value(&changed.record.typed_semantics).unwrap();
    changed.metadata["typed_semantics_hash"] = changed.record.typed_semantics_hash.clone().into();
    changed.rebind_sidecars();
    let error = changed.check().unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
    assert!(error.message.contains("invalid signature contract"), "{error}");
}

#[test]
fn private_generic_shapes_reject_rebound_layout_signature_and_phantom_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Shape;
    let source = format!("{SOURCE}\nprivate struct Pair<T: fixed_value> {{ left: T, right: T }}\nprivate enum Choice<T: fixed_value> {{ First(Pair<T>), Second([T; 2]) }}\nprivate struct Marker<phantom T> {{ value: u64 }}\nprivate fn first<T: fixed_value>(value: T) -> T {{ value }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness pair: Pair<u64>, witness choice: Choice<Hash>, witness marker: Marker<Hash>)")
        .replace("require amount > 0", "require amount > 0\nrequire first<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    for mutation in [
        "missing",
        "field-type",
        "field-name",
        "field-order",
        "phantom-used",
        "nonphantom-unused",
        "enum-order",
        "enum-payload",
        "return",
        "reference",
        "source",
        "coherence",
    ] {
        let mut changed = baseline.clone();
        let template = match mutation {
            "enum-order" | "enum-payload" => "Choice",
            "nonphantom-unused" => "Marker",
            "return" | "reference" | "source" => "first",
            _ => "Pair",
        };
        let instance =
            changed.record.typed_semantics.instantiations.iter_mut().find(|instance| instance.template == template).unwrap();
        match mutation {
            "missing" => instance.declaration = Shape::Unavailable,
            "phantom-used" => instance.parameters[0].phantom = true,
            "nonphantom-unused" => instance.parameters[0].phantom = false,
            "field-type" | "field-name" | "field-order" | "coherence" => {
                let Shape::Struct { fields, .. } = &mut instance.declaration else { unreachable!() };
                match mutation {
                    "field-type" => fields[0].ty = "u32".into(),
                    "field-name" => fields[0].name = "forged".into(),
                    "field-order" => fields.reverse(),
                    // Preserve this instance's concrete type, but disagree
                    // with the declaration retained by its sibling instance.
                    "coherence" => fields[0].ty = instance.type_arguments[0].clone(),
                    _ => unreachable!(),
                }
            }
            "enum-order" | "enum-payload" => {
                let Shape::Enum { variants, .. } = &mut instance.declaration else { unreachable!() };
                if mutation == "enum-order" {
                    variants.reverse();
                } else {
                    variants[0].fields[0] = "u64".into();
                }
            }
            "return" | "reference" | "source" => {
                let Shape::Function { params, return_type } = &mut instance.declaration else { unreachable!() };
                match mutation {
                    "return" => *return_type = "Hash".into(),
                    "reference" => params[0].reference = true,
                    "source" => params[0].source = "input".into(),
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
        changed.rebind_policy_identity();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{mutation}: {error}");
        assert!(error.message.contains("generic"), "{mutation}: {error}");
    }
}

#[test]
fn private_generic_contracts_cannot_be_omitted_or_forged_after_rebinding() {
    let source = format!("{SOURCE}\nprivate struct Pair<T: fixed_value> {{ left: T, right: T }}\nprivate fn first<T: fixed_value>(value: T) -> T {{ value }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness pair: Pair<u64>)")
        .replace("require amount > 0", "require amount > 0\nrequire first<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    assert!(baseline.metadata["public_interface"]["callables"].as_array().unwrap().iter().all(|entry| entry["name"] != "first"));
    for mutation in
        ["omit-instance", "omit-parameters", "constraint", "phantom-function", "omit-binding", "wrong-binding", "duplicate-binding"]
    {
        let mut changed = baseline.clone();
        let instances = &mut changed.record.typed_semantics.instantiations;
        let index = instances.iter().position(|instance| instance.template == "first").unwrap();
        if mutation == "omit-instance" {
            instances.remove(index);
        } else {
            let instance = &mut instances[index];
            match mutation {
                "omit-parameters" => instance.parameters.clear(),
                "constraint" => instance.parameters[0].constraints = vec!["cell".into()],
                "phantom-function" => instance.parameters[0].phantom = true,
                "omit-binding" => instance.lowered_names.clear(),
                "wrong-binding" => instance.lowered_names = vec!["Token".into()],
                "duplicate-binding" => instance.lowered_names.push(instance.lowered_names[0].clone()),
                _ => unreachable!(),
            }
        }
        changed.rebind_policy_identity();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{mutation}: {error}");
        assert!(error.message.contains("generic"), "{mutation}: {error}");
    }
}

#[test]
fn typed_value_abilities_reject_field_contradictions_after_hash_rebinding() {
    let source = format!("{SOURCE}\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64 }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness value: Value)");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    for mutation in ["cell-kind", "ordinary-cell", "missing-field-evidence"] {
        let mut changed = baseline.clone();
        let name = if mutation == "cell-kind" { "Token" } else { "Value" };
        let ty = changed.record.typed_semantics.types.iter_mut().find(|ty| ty.name == name).unwrap();
        match mutation {
            "cell-kind" => ty.value_abilities = vec!["copy".into()],
            "ordinary-cell" => ty.value_abilities = vec!["cell".into()],
            "missing-field-evidence" => {
                ty.fields[0].ty = "OmittedNominal".into();
                ty.layout_hash = canonical_hash(
                    "cellscript-typed-layout-v2",
                    &(
                        ty.kind.as_str(),
                        ty.encoded_size,
                        &ty.fields,
                        ty.tag_width_bytes,
                        &ty.variants,
                        &ty.capabilities,
                        &ty.identity_policy,
                    ),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        changed.rebind_policy_identity();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{mutation}: {error}");
        assert!(error.message.contains("abilit"), "{mutation}: {error}");
    }
    let mut serialized = serde_json::to_value(&baseline.record).unwrap();
    serialized["typed_semantics"]["types"][0].as_object_mut().unwrap().remove("value_abilities");
    assert!(serde_json::from_value::<VerifiedLoweringRecord>(serialized).is_err());
}

#[test]
fn instantiated_interface_templates_reject_rebound_layout_and_signature_substitution() {
    let source = format!(
        "{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic enum Choice<T: fixed_value> {{ First(Pair<T>), Second([T; 2]) }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"
    )
    .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice<Hash>, witness pair: Pair<u64>)")
    .replace("require amount > 0", "require amount > 0\nrequire first<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.entries.iter().any(|entry| entry.name.starts_with("first__mono__")));
    for mutation in [
        "field",
        "field-order",
        "field-name",
        "enum-payload",
        "enum-order",
        "arity",
        "return",
        "parameter",
        "constraint",
        "ability",
        "return-concrete",
        "parameter-concrete",
    ] {
        let mut changed = baseline.clone();
        if matches!(mutation, "return" | "parameter" | "constraint" | "return-concrete" | "parameter-concrete") {
            let callable = changed.metadata["public_interface"]["callables"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["name"] == "first")
                .unwrap();
            if mutation == "constraint" {
                callable["type_parameters"][0]["constraints"] = serde_json::json!(["cell"]);
            } else if matches!(mutation, "return" | "return-concrete") {
                callable["return_type"] = serde_json::json!(if mutation == "return" { "BlockNumber" } else { "u64" });
            } else {
                callable["params"][0]["type"] = serde_json::json!(if mutation == "parameter" { "BlockNumber" } else { "u64" });
                // Generic templates have no source-name entry in the execution
                // metadata; recompute their declaration builder digest as well.
                let canonical = cellscript::package::registry::canonical_json_value(&callable["params"]);
                callable["builder_contract_hash"] = cellscript_artifact_checker::hex_encode(
                    &cellscript_artifact_checker::ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()),
                )
                .into();
            }
            let contracts = changed.metadata["public_interface"]["callables"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| serde_json::json!([entry["identity"], entry["builder_contract_hash"]]))
                .collect::<Vec<_>>();
            let canonical = cellscript::package::registry::canonical_json_value(&serde_json::json!(contracts));
            changed.metadata["public_interface"]["builder_contract_hash"] = cellscript_artifact_checker::hex_encode(
                &cellscript_artifact_checker::ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()),
            )
            .into();
        } else {
            let name = if mutation.starts_with("enum") { "Choice" } else { "Pair" };
            let ty = changed.metadata["public_interface"]["types"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|ty| ty["name"] == name)
                .unwrap();
            match mutation {
                "field" => ty["fields"][0]["type"] = serde_json::json!("Address"),
                "field-order" => ty["fields"].as_array_mut().unwrap().reverse(),
                "field-name" => ty["fields"][0]["name"] = serde_json::json!("forged"),
                "enum-payload" => ty["variants"][1]["fields"][0] = serde_json::json!("[Address; 2]"),
                "enum-order" => ty["variants"].as_array_mut().unwrap().reverse(),
                "arity" => ty["type_parameters"] = serde_json::json!([]),
                "ability" => ty["value_abilities"] = serde_json::json!(["drop"]),
                _ => unreachable!(),
            }
            let canonical =
                cellscript::package::registry::canonical_json_value(&serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]));
            ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
                &serde_json::to_vec(&canonical).unwrap(),
            ))
            .into();
        }
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("generic") || error.message.contains("checked entry"), "{mutation}: {error}");
    }
}

#[test]
fn interface_cell_capabilities_cannot_be_forged_by_rebinding_outer_hashes() {
    let baseline = Fixture::new(CellScriptEdition::Edition2027);
    baseline.assert_interface_inspection();
    let mut reordered = baseline.clone();
    reordered.metadata["public_interface"]["types"][0]["cell_capabilities"].as_array_mut().unwrap().reverse();
    reordered.rebind_interface_identity();
    reordered.assert_interface_inspection();

    for capabilities in [
        serde_json::json!([]),
        serde_json::json!(["store"]),
        serde_json::json!(["consume", "store", "copy"]),
        serde_json::json!(["consume", "store", "store"]),
    ] {
        let mut changed = baseline.clone();
        changed.metadata["public_interface"]["types"][0]["cell_capabilities"] = capabilities;
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert_eq!(error.message, "interface Cell capabilities differ from checked concrete type");
    }
}

#[test]
fn interface_variant_cardinality_cannot_be_forged_by_rebinding_layout_hashes() {
    let source = format!("{SOURCE}\npublic enum Choice {{ First(u64), Second(Hash) }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice)");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Choice"));
    for mutation in ["omit", "reorder", "rename", "payload", "same-width-type"] {
        let mut changed = baseline.clone();
        let ty = changed.metadata["public_interface"]["types"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ty| ty["name"] == "Choice")
            .unwrap();
        match mutation {
            "omit" => {
                ty["variants"].as_array_mut().unwrap().pop();
            }
            "reorder" => ty["variants"].as_array_mut().unwrap().reverse(),
            "rename" => ty["variants"][0]["name"] = serde_json::json!("forged"),
            "payload" => ty["variants"][0]["fields"] = serde_json::json!([]),
            "same-width-type" => ty["variants"][1]["fields"][0] = serde_json::json!("Address"),
            _ => unreachable!(),
        }
        let layout = serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]);
        let canonical = cellscript::package::registry::canonical_json_value(&layout);
        ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("variant set differs"), "{mutation}: {error}");
    }
}

#[test]
fn interface_nested_type_arguments_cannot_be_forged_by_rebinding_layout_hashes() {
    let source = format!(
        "{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic struct Envelope {{ pairs: [Pair<Hash>; 2] }}\npublic enum Choice {{ Nested(Envelope), Direct(Pair<Address>) }}"
    )
    .replace("witness recipient: Address)", "witness recipient: Address, witness choice: Choice)");
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Envelope"));
    assert!(baseline.record.typed_semantics.types.iter().any(|ty| ty.name == "Choice"));
    for name in ["Envelope", "Choice"] {
        let mut changed = baseline.clone();
        let ty =
            changed.metadata["public_interface"]["types"].as_array_mut().unwrap().iter_mut().find(|ty| ty["name"] == name).unwrap();
        if name == "Envelope" {
            ty["fields"][0]["type"] = serde_json::json!("[Pair<Address>; 2]");
        } else {
            ty["variants"][1]["fields"][0] = serde_json::json!("Pair<Hash>");
        }
        let layout = serde_json::json!([ty["kind"], ty["fields"], ty["variants"]]);
        let canonical = cellscript::package::registry::canonical_json_value(&layout);
        ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        assert!(error.message.contains("checked concrete layout"), "{name}: {error}");
    }
}

#[test]
fn policy_dispatch_machine_contract_covers_editions_optimizers_tag_extremes_and_no_common_checks() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            Fixture::new_with(edition, opt_level, declaration()).check().unwrap();
        }
    }

    let mut one_payload = declaration();
    one_payload.actions = vec![ArtifactAction { tag: 1, action: "mint".into() }];
    one_payload.common_checks = vec!["check_z".into()];
    let mut one_payload_free = declaration();
    one_payload_free.actions = vec![ArtifactAction { tag: 7, action: "burn".into() }];
    one_payload_free.common_checks.clear();
    for opt_level in 0..=3 {
        Fixture::new_with(CellScriptEdition::Edition2027, opt_level, one_payload.clone()).check().unwrap();
        Fixture::new_with(CellScriptEdition::Edition2027, opt_level, one_payload_free.clone()).check().unwrap();
    }

    let mut extreme = declaration();
    extreme.actions = vec![ArtifactAction { tag: u32::MAX, action: "burn".into() }, ArtifactAction { tag: 0, action: "mint".into() }];
    extreme.common_checks.clear();
    for opt_level in 0..=3 {
        Fixture::new_with(CellScriptEdition::Edition2027, opt_level, extreme.clone()).check().unwrap();
    }
}

#[test]
fn low_mask_policy_tags_reject_rebound_shift_and_seed_mutations() {
    let mut policy = declaration();
    policy.actions =
        vec![ArtifactAction { tag: 0x7fff_ffff, action: "burn".into() }, ArtifactAction { tag: u32::MAX, action: "mint".into() }];
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            let valid = Fixture::new_with(edition, opt_level, policy.clone());
            let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
            let sequences: Vec<_> = elf
                .instructions
                .windows(2)
                .filter(|pair| {
                    // addi t1, zero, -1; srli t1, t1, 32/33. These tags
                    // appear in both the precheck and final action routing.
                    pair[0].word == 0xfff0_0313
                        && matches!(pair[1].word, 0x0203_5313 | 0x0213_5313)
                        && valid
                            .record
                            .blocks
                            .iter()
                            .any(|block| block.owner_entry == "wrapper:_cellscript_entry" && block.range.contains(pair[1].address))
                })
                .collect();
            assert_eq!(sequences.len(), 4, "both tags use the short plan in both dispatch checks");
            for pair in sequences {
                for (address, word) in [
                    (pair[0].address, 0x0000_0313),              // zero instead of all ones
                    (pair[1].address, pair[1].word ^ (1 << 20)), // neighboring shift amount
                    (pair[1].address, pair[1].word | (1 << 30)), // arithmetic instead of logical shift
                ] {
                    let mut changed = valid.clone();
                    changed.replace_machine_word(address, word);
                    let error = changed.check().expect_err("altered tag must reject after all outer hashes are rebound");
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
            }
        }
    }
}

#[test]
fn rebound_policy_machine_mutations_cannot_change_selector_dispatch_or_adapter_dataflow() {
    let valid = Fixture::new(CellScriptEdition::Edition2027);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    let wrapper_blocks =
        valid.record.blocks.iter().filter(|block| block.owner_entry == "wrapper:_cellscript_entry").collect::<Vec<_>>();
    let current_hash_syscall = elf
        .syscall_addresses
        .iter()
        .copied()
        .filter(|address| wrapper_blocks.iter().any(|block| block.range.contains(*address)))
        .max()
        .unwrap();
    let copied = policy_block(&valid, ".Lentry_witness_v2_copy_done_").range.start;
    let record_loop = policy_block(&valid, ".Lpolicy_record_").range.start;
    let record_base = elf
        .instructions
        .iter()
        .find(|instruction| {
            wrapper_blocks.iter().any(|block| block.range.contains(instruction.address)) && is_add(instruction.word, 15, 14, 5)
        })
        .unwrap()
        .address;
    let key_loop = policy_block(&valid, ".Lpolicy_key_order_loop_").range.start;
    let ordered = policy_block(&valid, ".Lpolicy_key_ordered_").range.start;
    let hash_loop = policy_block(&valid, ".Lpolicy_current_hash_loop_").range.start;
    let first_variant = valid
        .record
        .blocks
        .iter()
        .filter(|block| {
            block.owner_entry == "wrapper:_cellscript_entry"
                && block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_variant_"))
        })
        .min_by_key(|block| block.range.start)
        .unwrap()
        .range
        .start;
    let first_adapter = valid
        .record
        .entries
        .iter()
        .filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_"))
        .min_by_key(|entry| valid.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start)
        .unwrap();
    let adapter_copy = valid
        .record
        .blocks
        .iter()
        .filter(|block| block.owner_entry == first_adapter.id)
        .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lpolicy_args_copy_")))
        .unwrap()
        .range
        .start;
    let common_target = valid
        .record
        .entries
        .iter()
        .find(|entry| entry.id == "action:check_z")
        .and_then(|entry| valid.record.blocks.iter().find(|block| block.id == entry.entry_block))
        .unwrap()
        .range
        .start;
    let common_call = elf
        .control_flow
        .iter()
        .find(|flow| flow.target == common_target && wrapper_blocks.iter().any(|block| block.range.contains(flow.address)))
        .unwrap()
        .address;
    let mutations = [
        ("current-script-hash-syscall", current_hash_syscall - 4),
        ("policy-magic", copied + 32),
        ("dynvec-record-bound", record_loop - 36),
        ("record-layout", record_base + 4),
        ("strict-key-order", key_loop + 24),
        ("type-role", ordered + 8),
        ("selected-tag", hash_loop + 68),
        ("common-check-failure", common_call + 4),
        ("variant-args-pointer", first_variant + 4),
        ("adapter-private-copy", adapter_copy + 8),
    ];
    for (name, address) in mutations {
        let mut changed = valid.clone();
        let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        changed.replace_machine_word(address, word ^ (1 << 20));
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{name}: {error}");
    }
}

#[test]
fn typed_outgoing_stack_args_are_bound_to_the_policy_adapter_frame() {
    let declaration = ArtifactDeclaration {
        name: "stack-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 1, action: "mint".into() }],
        common_checks: Vec::new(),
    };
    let valid = Fixture::new_source_with(STACK_ARGS_SOURCE, CellScriptEdition::Edition2027, 3, declaration);
    let adapter = valid.record.entries.iter().find(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).unwrap();
    // 104 payload bytes + 8-byte CSARG header + length/RA slots,
    // aligned to 16, plus the 32-byte outgoing argument reservation.
    assert_eq!(adapter.frame_size_bytes, 160);

    let mut changed = valid.clone();
    let adapter_id = adapter.id.clone();
    changed.record.entries.iter_mut().find(|entry| entry.id == adapter_id).unwrap().frame_size_bytes += 16;
    for block in changed.record.blocks.iter_mut().filter(|block| block.owner_entry == adapter_id) {
        block.frame_size_bytes += 16;
    }
    changed.rebind_sidecars();
    let error = changed.check().unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
    assert!(error.message.contains("policy positional adapter frame contract changed"), "{error}");
}

#[test]
fn shared_decoder_preserves_private_return_storage_with_outgoing_stack_arguments() {
    let second = STACK_ARGS_SOURCE.split_once("action mint(").unwrap().1;
    let source = format!("{STACK_ARGS_SOURCE}\naction mint_other({second}");
    let declaration = ArtifactDeclaration {
        name: "shared-stack-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 1, action: "mint".into() }, ArtifactAction { tag: 2, action: "mint_other".into() }],
        common_checks: Vec::new(),
    };
    let valid = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 3, declaration);
    let decoders: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")).collect();
    assert_eq!(decoders.len(), 1);
    assert_eq!(decoders[0].frame_size_bytes, 0);
    let adapters: Vec<_> = valid.record.entries.iter().filter(|entry| entry.name.starts_with(".Lpolicy_action_adapter_")).collect();
    assert_eq!(adapters.len(), 2);
    let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
    for adapter in adapters {
        assert_eq!(adapter.frame_size_bytes, 160, "128 private bytes plus 32 outgoing bytes");
        let start = valid.record.blocks.iter().find(|block| block.id == adapter.entry_block).unwrap().range.start;
        let saved_ra = elf.instructions.iter().find(|instruction| instruction.address == start + 4).unwrap();
        let mut changed = valid.clone();
        // Move saved ra from sp+120 to sp+112, into the selected payload.
        // Rebind hashes so rejection depends on the independent frame contract.
        changed.replace_machine_word(saved_ra.address, saved_ra.word ^ (1 << 10));
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "{error}");
        assert!(error.message.contains("private copy frame"), "{error}");
    }
}

fn is_add(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

#[test]
fn raw_builder_param_mutations_reject_despite_rebound_outer_identities() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in [
        "name",
        "type",
        "source",
        "mut",
        "ref",
        "cell-skip",
        "scalar-skip",
        "lock-args",
        "schema",
        "fixed-width",
        "fixed-flag",
        "hash-flag",
        "bounded",
    ] {
        let mut changed = fixture.clone();
        match mutation {
            "name" => changed.param_mut("mint", 0)["name"] = "other".into(),
            "type" => changed.param_mut("mint", 0)["ty"] = "u128".into(),
            "source" => changed.param_mut("mint", 0)["source"] = "lock_args".into(),
            "mut" => changed.param_mut("mint", 0)["is_mut"] = true.into(),
            "ref" => changed.param_mut("mint", 0)["is_ref"] = true.into(),
            "cell-skip" => changed.param_mut("burn", 0)["cell_bound_abi"] = false.into(),
            "scalar-skip" => changed.param_mut("mint", 0)["cell_bound_abi"] = true.into(),
            "lock-args" => changed.param_mut("mint", 0)["lock_args_data_source"] = true.into(),
            "schema" => changed.param_mut("mint", 0)["schema_pointer_abi"] = true.into(),
            "fixed-width" => changed.param_mut("mint", 1)["fixed_byte_len"] = 31.into(),
            "fixed-flag" => changed.param_mut("mint", 1)["fixed_byte_length_abi"] = false.into(),
            "hash-flag" => changed.param_mut("burn", 0)["type_hash_pointer_abi"] = true.into(),
            "bounded" => changed.param_mut("burn", 0)["bounded_runtime_contract"] = "type-group-inputs-v1".into(),
            _ => unreachable!(),
        }
        changed.rebind_sidecars();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("policy builder parameter"), "{mutation}: {error}");
    }
}

#[test]
fn raw_policy_declaration_and_outer_abi_mutations_reject_after_rebinding() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in
        ["tag", "action", "common-order", "resource", "name", "records", "bytes", "payload", "placement", "field", "source", "missing"]
    {
        let mut changed = fixture.clone();
        let policy = &mut changed.metadata["runtime"]["policy_artifact"];
        match mutation {
            "tag" => policy["declaration"]["actions"][0]["tag"] = 11.into(),
            "action" => policy["declaration"]["actions"][0]["action"] = "burn".into(),
            "common-order" => policy["declaration"]["common_checks"].as_array_mut().unwrap().swap(0, 1),
            "resource" => policy["declaration"]["context"]["resource"] = "OtherToken".into(),
            "name" => policy["declaration"]["name"] = "other-policy".into(),
            "records" => policy["max_records"] = 9.into(),
            "bytes" => policy["max_witness_bytes"] = 4097.into(),
            "payload" => policy["payload_abi"] = "cellscript-entry-witness-v1".into(),
            "placement" => policy["placement_abi"] = "raw".into(),
            "field" => policy["placement_field"] = "lock".into(),
            "source" => policy["placement_source"] = "input[0]".into(),
            "missing" => {
                changed.metadata["runtime"].as_object_mut().unwrap().remove("policy_artifact");
            }
            _ => unreachable!(),
        }
        changed.rebind_sidecars();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(error.message.contains("runtime.policy_artifact"), "{mutation}: {error}");
    }
}

#[test]
fn typed_policy_counts_payload_identity_and_selector_require_concrete_evidence() {
    let fixture = Fixture::new(CellScriptEdition::Edition2027);
    for mutation in ["input-count", "output-count", "payload-schema", "selector-content", "selector-label", "unknown", "wrapper"] {
        let mut changed = fixture.clone();
        match mutation {
            "input-count" => changed.policy_mut().variants[0].input_count = 1,
            "output-count" => changed.policy_mut().variants[0].output_count = 2,
            "payload-schema" => changed.policy_mut().variants[0].payload_schema_hash = "unbound-params".into(),
            "selector-label" => changed.policy_mut().selector_node_id = "caller-chosen-label".into(),
            "selector-content" => {
                let id = changed.policy_mut().selector_node_id.clone();
                let node = changed.record.typed_semantics.foundation.provenance.nodes.iter_mut().find(|node| node.id == id).unwrap();
                let ValueProvenance::EntryWitness { field_path, .. } = &mut node.provenance else { panic!("selector root") };
                *field_path = "input_type.unauthenticated_tag".into();
                node.id = canonical_hash("cellscript-value-provenance-node-v1", &node.provenance).unwrap();
                let replacement = node.id.clone();
                changed.policy_mut().selector_node_id = replacement;
            }
            "unknown" => changed.policy_mut().unknown_selector = "accept".into(),
            "wrapper" => changed.record.typed_semantics.foundation.entry_contract.exact_entry = "action:mint".into(),
            _ => unreachable!(),
        }
        changed.rebind_policy_identity();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{mutation}: {error}");
    }
}

#[test]
fn interface_callable_signature_cannot_be_forged_after_outer_hash_rebinding() {
    let source = SOURCE.replace("require amount > 0", "require echo(amount) > 0") + "\npublic fn echo(value: u64) -> u64 { value }\n";
    let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, 0, declaration());
    baseline.assert_interface_inspection();
    assert!(baseline.record.typed_semantics.entries.iter().any(|entry| entry.name == "echo" && entry.kind == "helper"));
    for mutation in
        ["same-width-type", "name", "source", "mutable", "reference", "omit", "duplicate", "order", "output", "return", "kind"]
    {
        let mut changed = baseline.clone();
        let callable = changed.metadata["public_interface"]["callables"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|entry| entry["name"] == "mint")
            .unwrap();
        match mutation {
            "same-width-type" => callable["params"][1]["type"] = "Hash".into(),
            "name" => callable["params"][1]["name"] = "substituted".into(),
            "source" => callable["params"][1]["source"] = "default".into(),
            "mutable" => callable["params"][1]["mutable"] = true.into(),
            "reference" => callable["params"][1]["reference"] = true.into(),
            "omit" => {
                callable["params"].as_array_mut().unwrap().pop();
            }
            "duplicate" => {
                let param = callable["params"][1].clone();
                callable["params"].as_array_mut().unwrap().push(param);
            }
            "order" => callable["params"].as_array_mut().unwrap().reverse(),
            "output" => {
                callable["outputs"] = serde_json::json!([{
                    "name": "forged", "type": "Token", "source": "output", "mutable": false, "reference": false
                }])
            }
            "return" => callable["return_type"] = "u64".into(),
            "kind" => callable["kind"] = "function".into(),
            _ => unreachable!(),
        }
        changed.rebind_interface_identity();
        changed.check().unwrap();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{mutation}: {error}");
        assert!(
            error.message.starts_with("interface callable ") && error.message.ends_with("differs from checked entry"),
            "{mutation}: {error}"
        );
    }
    let mut changed = baseline.clone();
    let callable = changed.metadata["public_interface"]["callables"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "echo")
        .unwrap();
    callable["return_type"] = "BlockNumber".into();
    changed.rebind_interface_identity();
    changed.check().unwrap();
    let error = cellscript_artifact_checker::interface::inspect_bundle(
        &changed.artifact,
        &serde_json::to_vec(&changed.metadata).unwrap(),
        &serde_json::to_vec(&changed.record).unwrap(),
        &serde_json::to_vec(&changed.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert_eq!(error.message, "interface callable return type differs from checked entry");
}

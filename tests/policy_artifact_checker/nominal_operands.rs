#[test]
fn distinct_zero_width_constructor_fields_reject_swapped_nominal_operands_after_rebinding() {
    let source = r#"
module zero_width_constructor;
struct EmptyZ {}
struct EmptyA {}
struct Payload { z_empty: EmptyZ, a_empty: EmptyA, tail: u64, }
fn build() -> Payload { new Payload { tail: 37, z_empty: new EmptyZ {}, a_empty: new EmptyA {}, } }
action verify() {

    let value: Payload = build();
    require value.tail == 37;
}

"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        // This prototype belongs to the single-entry ABI. Keep policy-witness
        // helper-call admission separate from its typed constructor contract.
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { source_contracts: true, diagnostics, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let mut changed = Fixture {
            artifact: compiled.artifact_bytes,
            metadata: serde_json::to_value(compiled.metadata).unwrap(),
            record: compiled.verified_lowering_record.unwrap(),
            source_map: compiled.source_artifact_map.unwrap(),
        };
        changed.check().unwrap();
        let constructor = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "build").unwrap();
        let operation = constructor
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.opcode == "tuple" && operation.operands.len() == 3)
            .unwrap();
        assert_eq!(operation.operands[0].ty, "EmptyA");
        assert_eq!(operation.operands[1].ty, "EmptyZ");
        operation.operands.swap(0, 1);
        // Rebind all semantic layers so the checker must reject the tuple
        // operand substitution instead of a stale program identity.
        changed.rebind_layered_identities();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{diagnostics}: {error}");
        assert!(error.message.contains("'tuple'"), "{diagnostics}: {error}");
    }
}

#[test]
fn real_policy_bundle_and_unchanged_identity_rebinding_are_valid_in_edition_2027() {
    {
        let edition = CellScriptEdition::Edition2027;
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
        // Both current artifact binding and interface inspection reject losing
        // a declaration field, even when every outer digest is rebound.
        assert_eq!(changed.check().unwrap_err().code, CheckerRejectionCode::V2410MetadataBindingMismatch);
        let error = inspect(&changed).unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch);
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
        let canonical = cellscript::canonical_json::canonical_json_value(&layout);
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
module nominal_call_contract;
private struct Pair<T: fixed_value> { left: T, right: T, }
private struct Twin<T: fixed_value> { left: T, right: T, }
private fn inspect_pair(value: Pair<u64>) -> u64 { value.left }
action verify(witness pair: Pair<u64>, witness twin: Twin<u64>) {

    require inspect_pair(pair) == pair.left;
}
"#;
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions {
            source_contracts: true,
            diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
            target: Some("riscv64-elf".into()),
            ..CompileOptions::default()
        },
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
    changed.rebind_layered_identities();
    let error = changed.check().unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
    assert!(error.message.contains("invalid signature contract"), "{error}");
}

#[test]
fn private_generic_shapes_reject_rebound_layout_signature_and_phantom_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Shape;
    let source = format!("{SOURCE}\nprivate struct Pair<T: fixed_value> {{ left: T, right: T }}\nprivate enum Choice<T: fixed_value> {{ First(Pair<T>), Second([T; 2]) }}\nprivate struct Marker<phantom T> {{ value: u64 }}\nprivate fn first<T: fixed_value>(value: T) -> T {{ value }}")
        .replace("witness recipient: Address)", "witness recipient: Address, witness pair: Pair<u64>, witness choice: Choice<Hash>, witness marker: Marker<Hash>)")
        .replace("require amount > 0", "require amount > 0;\nrequire first::<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
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
        .replace("require amount > 0", "require amount > 0;\nrequire first::<u64>(pair.left) == pair.left");
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
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
    let baseline = Fixture::new_source_with(
        &source,
        CellScriptEdition::Edition2027,
        cellscript::CompilerDiagnostics::without_optimizations(),
        declaration(),
    );
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

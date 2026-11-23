#[test]
fn uninstantiated_generic_catalog_rejects_rebound_shape_and_bound_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!("{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic enum Choice<T: fixed_value> {{ First(T), Empty }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, diagnostics, declaration());
        baseline.assert_interface_inspection();
        assert!(baseline.record.typed_semantics.instantiations.is_empty());
        assert!(!baseline.record.typed_semantics.entries.iter().any(|entry| entry.name.contains("first")));
        for mutation in [
            "schema",
            "missing",
            "duplicate",
            "owner",
            "kind",
            "parameter",
            "constraint",
            "phantom",
            "field",
            "field-order",
            "ability",
            "variant",
            "return",
            "source",
            "type-bound",
            "depth-bound",
            "type-syntax",
        ] {
            let mut changed = baseline.clone();
            let catalog = changed.record.typed_semantics.generic_declarations.as_mut().unwrap();
            let pair = catalog.declarations.iter().position(|contract| contract.name == "Pair").unwrap();
            let function = catalog.declarations.iter().position(|contract| contract.name == "first").unwrap();
            let choice = catalog.declarations.iter().position(|contract| contract.name == "Choice").unwrap();
            match mutation {
                "schema" => catalog.schema.push('x'),
                "missing" => {
                    catalog.declarations.remove(pair);
                }
                "duplicate" => catalog.declarations.push(catalog.declarations[pair].clone()),
                "owner" => catalog.declarations[pair].module = "other_owner".into(),
                "kind" => catalog.declarations[pair].kind = "function".into(),
                "parameter" => catalog.declarations[pair].parameters[0].name = "Other".into(),
                "constraint" => {
                    catalog.declarations[pair].parameters[0].constraints.remove(0);
                }
                "phantom" => catalog.declarations[pair].parameters[0].phantom = true,
                "field" | "field-order" | "ability" | "type-bound" | "depth-bound" | "type-syntax" => {
                    let Declaration::Struct { fields, abilities } = &mut catalog.declarations[pair].declaration else {
                        unreachable!()
                    };
                    match mutation {
                        "field" => fields[0].name = "different".into(),
                        "field-order" => fields.swap(0, 1),
                        "ability" => {
                            abilities.remove(0);
                        }
                        "type-bound" => fields[0].ty = "T".repeat(513),
                        "depth-bound" => fields[0].ty = format!("{}T{}", "(".repeat(17), ")".repeat(17)),
                        "type-syntax" => fields[0].ty = "T;;".into(),
                        _ => unreachable!(),
                    }
                }
                "variant" => {
                    let Declaration::Enum { variants, .. } = &mut catalog.declarations[choice].declaration else { unreachable!() };
                    variants.swap(0, 1);
                }
                "return" | "source" => {
                    let Declaration::Function { params, return_type } = &mut catalog.declarations[function].declaration else {
                        unreachable!()
                    };
                    if mutation == "return" {
                        *return_type = "u64".into();
                    } else {
                        params[0].source = "witness".into();
                    }
                }
                _ => unreachable!(),
            }
            changed.rebind_policy_identity();
            let error = cellscript_artifact_checker::interface::inspect_bundle(
                &changed.artifact,
                &serde_json::to_vec(&changed.metadata).unwrap(),
                &serde_json::to_vec(&changed.record).unwrap(),
                &serde_json::to_vec(&changed.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(
                matches!(
                    error.code,
                    CheckerRejectionCode::V2410MetadataBindingMismatch | CheckerRejectionCode::V2419TypedSemanticsInvalid
                ),
                "opt={diagnostics} {mutation}: {error}"
            );
            assert!(error.message.contains("generic") || error.message.contains("nominal"), "opt={diagnostics} {mutation}: {error}");
        }
    }
}

#[test]
fn private_absent_template_declarations_reject_rebound_universal_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!(
        "{SOURCE}\nprivate struct Sealed<T: fixed_value> has copy, drop, store, fixed, serializable, non_linear {{ inner: T }}\nprivate fn select<T: fixed_value>(value: T) -> T {{ value }}"
    );
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, diagnostics, declaration());
        baseline.assert_interface_inspection();
        let inspected = cellscript_artifact_checker::interface::inspect_bundle(
            &baseline.artifact,
            &serde_json::to_vec(&baseline.metadata).unwrap(),
            &serde_json::to_vec(&baseline.record).unwrap(),
            &serde_json::to_vec(&baseline.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap();
        // The private templates stay declaration-only: no instantiation or
        // lowered entry exercises them at either standard or diagnostic builds.
        assert!(baseline
            .record
            .typed_semantics
            .instantiations
            .iter()
            .all(|instance| !matches!(instance.template.as_str(), "Sealed" | "select")));
        assert!(!baseline.record.typed_semantics.entries.iter().any(|entry| entry.name.contains("select")));
        inspected.validate_symbolic_declarations().unwrap();
        inspected.project_module_contract().unwrap();
        for mutation in ["boundary", "field-vec", "field-missing", "ability-order", "function-param", "function-return"] {
            let mut changed = baseline.clone();
            let catalog = changed.record.typed_semantics.generic_declarations.as_mut().unwrap();
            let sealed = catalog
                .declarations
                .iter()
                .position(|contract| contract.name == "Sealed" && contract.visibility == "private")
                .unwrap();
            let function =
                catalog.declarations.iter().position(|contract| contract.name == "select" && contract.kind == "function").unwrap();
            match mutation {
                "boundary" => {
                    catalog.declarations[sealed].parameters[0].constraints.retain(|ability| ability != "fixed");
                }
                "field-vec" | "field-missing" | "ability-order" => {
                    let Declaration::Struct { fields, abilities } = &mut catalog.declarations[sealed].declaration else {
                        unreachable!()
                    };
                    match mutation {
                        "field-vec" => fields[0].ty = "Vec<T>".into(),
                        "field-missing" => fields[0].ty = "Missing".into(),
                        _ => abilities.push("copy".into()),
                    }
                }
                "function-param" | "function-return" => {
                    let Declaration::Function { params, return_type } = &mut catalog.declarations[function].declaration else {
                        unreachable!()
                    };
                    if mutation == "function-param" {
                        params[0].ty = "Missing".into();
                    } else {
                        *return_type = "Missing".into();
                    }
                }
                _ => unreachable!(),
            }
            changed.rebind_policy_identity();
            let inspected = cellscript_artifact_checker::interface::inspect_bundle(
                &changed.artifact,
                &serde_json::to_vec(&changed.metadata).unwrap(),
                &serde_json::to_vec(&changed.record).unwrap(),
                &serde_json::to_vec(&changed.source_map).unwrap(),
                &CheckerBudgets::default(),
            );
            // Ability ordering and unresolved structural fields are owned by
            // the earlier catalog binding checks; the remaining mutations must
            // pass that layer and reject exactly at the universal sweep.
            if matches!(mutation, "ability-order" | "field-missing") {
                let error = inspected.unwrap_err();
                assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "opt={diagnostics} {mutation}: {error}");
                continue;
            }
            let error = inspected.unwrap().validate_symbolic_declarations().unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "opt={diagnostics} {mutation}: {error}");
            assert!(error.message.contains("universal"), "opt={diagnostics} {mutation}: {error}");
        }
    }
}

#[test]
fn nominal_catalog_rejects_rebound_scopes_shapes_and_public_omissions() {
    let source = format!("{SOURCE}\npublic struct Snapshot {{ z: u64, a: Hash }}\npublic enum Event {{ First(u64), Second(Hash) }}\npublic fn view(value: Snapshot) -> u64 {{ value.z }}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, diagnostics, declaration());
        baseline.assert_interface_inspection();
        assert!(baseline.metadata["public_interface"]["callables"].as_array().unwrap().iter().any(|entry| entry["name"] == "view"));
        for mutation in [
            "schema",
            "duplicate",
            "missing",
            "scope-missing",
            "own-rebound",
            "unknown-owner",
            "scope-duplicate",
            "scope-bound",
            "field-order",
            "variant-order",
            "type-syntax",
            "public-type-omitted",
            "public-callable-omitted",
            "public-type-added",
            "public-callable-duplicate",
        ] {
            let mut changed = baseline.clone();
            let catalog = changed.record.typed_semantics.nominal_declarations.as_mut().unwrap();
            let snapshot = catalog.declarations.iter().position(|contract| contract.name == "Snapshot").unwrap();
            let event = catalog.declarations.iter().position(|contract| contract.name == "Event").unwrap();
            let scope = catalog.scopes.iter().position(|scope| scope.module == "policy_artifact_checker").unwrap();
            match mutation {
                "schema" => catalog.schema.push('x'),
                "duplicate" => catalog.declarations.push(catalog.declarations[snapshot].clone()),
                "missing" => {
                    catalog.declarations.remove(snapshot);
                }
                "scope-missing" => {
                    catalog.scopes[scope].bindings.retain(|binding| binding.local_name != "Snapshot");
                }
                "own-rebound" => {
                    catalog.scopes[scope].bindings.iter_mut().find(|binding| binding.local_name == "Snapshot").unwrap().source_name =
                        "Token".into();
                }
                "unknown-owner" => catalog.scopes[scope].bindings[0].owner_module = "unknown".into(),
                "scope-duplicate" => catalog.scopes.push(catalog.scopes[scope].clone()),
                "scope-bound" => {
                    let binding = catalog.scopes[scope].bindings[0].clone();
                    catalog.scopes[scope].bindings = vec![binding; 257];
                }
                "field-order" => catalog.declarations[snapshot].fields.swap(0, 1),
                "variant-order" => catalog.declarations[event].variants.swap(0, 1),
                "type-syntax" => catalog.declarations[snapshot].fields[0].ty = "u64<,>".into(),
                "public-type-omitted" => {
                    changed.metadata["public_interface"]["types"].as_array_mut().unwrap().retain(|ty| ty["name"] != "Snapshot")
                }
                "public-callable-omitted" => changed.metadata["public_interface"]["callables"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|callable| callable["name"] != "view"),
                "public-type-added" => {
                    let mut ty = changed.metadata["public_interface"]["types"][0].clone();
                    ty["identity"] = "policy_artifact_checker::ZZExtra".into();
                    ty["name"] = "ZZExtra".into();
                    ty["kind"] = "struct".into();
                    ty["fields"] = serde_json::json!([]);
                    ty["variants"] = serde_json::json!([]);
                    ty["value_abilities"] = serde_json::json!([]);
                    let layout = serde_json::json!(["struct", [], []]);
                    let canonical = cellscript::canonical_json::canonical_json_value(&layout);
                    ty["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
                        &serde_json::to_vec(&canonical).unwrap(),
                    ))
                    .into();
                    changed.metadata["public_interface"]["types"].as_array_mut().unwrap().push(ty);
                }
                "public-callable-duplicate" => {
                    let callable = changed.metadata["public_interface"]["callables"][0].clone();
                    changed.metadata["public_interface"]["callables"].as_array_mut().unwrap().push(callable);
                }
                _ => unreachable!(),
            }
            if mutation.starts_with("public-") {
                // Recompute the aggregate builder digest as well as the outer
                // interface identity: source completeness must reject on its own.
                let callables = changed.metadata["public_interface"]["callables"].as_array().unwrap();
                let builder =
                    callables.iter().map(|callable| (&callable["identity"], &callable["builder_contract_hash"])).collect::<Vec<_>>();
                let canonical = cellscript::canonical_json::canonical_json_value(&serde_json::to_value(builder).unwrap());
                changed.metadata["public_interface"]["builder_contract_hash"] = cellscript_artifact_checker::hex_encode(
                    &cellscript_artifact_checker::ckb_blake2b256(&serde_json::to_vec(&canonical).unwrap()),
                )
                .into();
                changed.rebind_interface_identity();
            } else {
                changed.rebind_policy_identity();
            }
            let error = cellscript_artifact_checker::interface::inspect_bundle(
                &changed.artifact,
                &serde_json::to_vec(&changed.metadata).unwrap(),
                &serde_json::to_vec(&changed.record).unwrap(),
                &serde_json::to_vec(&changed.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(
                matches!(
                    error.code,
                    CheckerRejectionCode::V2410MetadataBindingMismatch | CheckerRejectionCode::V2419TypedSemanticsInvalid
                ),
                "{diagnostics} {mutation}: {error}"
            );
            if mutation.starts_with("public-") && mutation != "public-callable-duplicate" {
                assert!(error.message.contains("public declaration set"), "{diagnostics} {mutation}: {error}");
            }
        }
    }
}

#[test]
fn universal_abilities_reject_weakened_parameter_minima_without_any_concrete_instance() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!("{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}");
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, diagnostics, declaration());
        let inspect = |fixture: &Fixture| {
            cellscript_artifact_checker::interface::inspect_bundle(
                &fixture.artifact,
                &serde_json::to_vec(&fixture.metadata).unwrap(),
                &serde_json::to_vec(&fixture.record).unwrap(),
                &serde_json::to_vec(&fixture.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap()
        };
        assert!(baseline.record.typed_semantics.instantiations.is_empty());
        inspect(&baseline).validate_symbolic_declarations().unwrap();
        for missing in ["nominal", "generic", "both"] {
            let mut stripped = baseline.clone();
            if missing != "generic" {
                stripped.record.typed_semantics.nominal_declarations = None;
            }
            if missing != "nominal" {
                stripped.record.typed_semantics.generic_declarations = None;
            }
            stripped.rebind_policy_identity();
            let error = cellscript_artifact_checker::interface::inspect_bundle(
                &stripped.artifact,
                &serde_json::to_vec(&stripped.metadata).unwrap(),
                &serde_json::to_vec(&stripped.record).unwrap(),
                &serde_json::to_vec(&stripped.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .and_then(|inspection| inspection.validate_symbolic_declarations())
            .unwrap_err();
            assert!(
                error.message.contains("requires a bounded") || error.message.contains("defining contract"),
                "{diagnostics} {missing}: {error}"
            );
        }
        let mut changed = baseline.clone();
        let pair = changed
            .record
            .typed_semantics
            .generic_declarations
            .as_mut()
            .unwrap()
            .declarations
            .iter_mut()
            .find(|contract| contract.name == "Pair")
            .unwrap();
        pair.parameters[0].constraints.retain(|ability| ability != "copy");
        let pair =
            changed.metadata["public_interface"]["types"].as_array_mut().unwrap().iter_mut().find(|ty| ty["name"] == "Pair").unwrap();
        pair["type_parameters"][0]["constraints"].as_array_mut().unwrap().retain(|ability| ability != "copy");
        changed.rebind_interface_identity();
        // A consistent declaration-only record is inspectable. It must not
        // supply a universal copy guarantee when its parameter lacks copy.
        let error = inspect(&changed).validate_symbolic_declarations().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        assert!(error.message.contains("parameter minima"), "{diagnostics}: {error}");

        let mut recursive = baseline.clone();
        let pair = recursive
            .record
            .typed_semantics
            .generic_declarations
            .as_mut()
            .unwrap()
            .declarations
            .iter_mut()
            .find(|contract| contract.name == "Pair")
            .unwrap();
        let Declaration::Struct { fields, .. } = &mut pair.declaration else { unreachable!() };
        fields[0].ty = "Pair<T>".into();
        let pair = recursive.metadata["public_interface"]["types"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ty| ty["name"] == "Pair")
            .unwrap();
        pair["fields"][0]["type"] = "Pair<T>".into();
        let layout = serde_json::json!([pair["kind"], pair["fields"], pair["variants"]]);
        let canonical = cellscript::canonical_json::canonical_json_value(&layout);
        pair["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        recursive.rebind_interface_identity();
        let error = inspect(&recursive).validate_symbolic_declarations().unwrap_err();
        assert!(error.message.contains("recursive"), "{diagnostics}: {error}");
    }
}

#[test]
fn imported_nominal_layouts_reject_same_width_owner_and_alias_substitution() {
    use cellscript::{artifact::compile_sources_artifact, InMemorySource};
    let source = SOURCE.replace(
        "action burn(input token: Token) {  consume token; }",
        "action burn(input token: Token, witness values: Envelope) {  consume token; }",
    );
    let source = format!("{source}\nuse left_owner::Value as Left;\nuse right_owner::Value as Right;\npublic struct Envelope {{ left: Left, right: Right }}");
    let sources = [
        InMemorySource { path: "main.cell".into(), source, role: None },
        InMemorySource {
            path: "left.cell".into(),
            source: "module left_owner;\npublic struct Value { value: u64, }".into(),
            role: None,
        },
        InMemorySource {
            path: "right.cell".into(),
            source: "module right_owner;\npublic struct Value { value: u64, }".into(),
            role: None,
        },
    ];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_sources_artifact(
            &sources,
            "main.cell",
            CompileOptions {
                source_contracts: true,
                edition: CellScriptEdition::Edition2027,
                diagnostics,
                target: Some("riscv64-elf".into()),
                ..CompileOptions::default()
            },
            declaration(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let baseline = Fixture {
            artifact: compiled.artifact_bytes,
            metadata: serde_json::to_value(compiled.metadata).unwrap(),
            record: compiled.verified_lowering_record.unwrap(),
            source_map: compiled.source_artifact_map.unwrap(),
        };
        baseline.assert_interface_inspection();
        let catalog = baseline.record.typed_semantics.nominal_declarations.as_ref().unwrap();
        assert!(catalog
            .layout_bindings
            .iter()
            .any(|binding| binding.lowered_name == "Left" && binding.owner_module == "left_owner" && binding.source_name == "Value"));
        assert!(catalog.layout_bindings.iter().any(|binding| binding.lowered_name == "Right"
            && binding.owner_module == "right_owner"
            && binding.source_name == "Value"));
        for mutation in ["owner", "missing", "duplicate", "extra", "nested-owner"] {
            let mut changed = baseline.clone();
            let catalog = changed.record.typed_semantics.nominal_declarations.as_mut().unwrap();
            let left = catalog.layout_bindings.iter().position(|binding| binding.lowered_name == "Left").unwrap();
            match mutation {
                "owner" => catalog.layout_bindings[left].owner_module = "right_owner".into(),
                "missing" => {
                    catalog.layout_bindings.remove(left);
                }
                "duplicate" => catalog.layout_bindings.push(catalog.layout_bindings[left].clone()),
                "extra" => {
                    let mut binding = catalog.layout_bindings[left].clone();
                    binding.lowered_name = "Extra".into();
                    catalog.layout_bindings.push(binding);
                }
                "nested-owner" => {
                    let envelope = catalog.declarations.iter_mut().find(|declaration| declaration.name == "Envelope").unwrap();
                    envelope.fields[0].ty = "Right".into();
                }
                _ => unreachable!(),
            }
            changed.rebind_policy_identity();
            let error = changed.check().unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{diagnostics} {mutation}: {error}");
            assert!(error.message.contains("nominal"), "{diagnostics} {mutation}: {error}");
        }
    }
}

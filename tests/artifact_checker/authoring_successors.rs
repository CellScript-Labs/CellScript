use super::*;
use cellscript_artifact_checker::{InputDisposition, OutputOrigin};
use std::sync::OnceLock;

const SOURCE: &str = r#"// test-artifact: {"entry":"update","layout":"Note"}

module authoring_checker::branches;
resource Note has store, replace, relock { amount: u64, }
action update(
        input before: Note,
        witness choose: bool,
        output after: Note,
    ) {
        if choose {
            replace before -> after {
                data = same except { amount = before.amount + 1, };
                lock = same;
                type_script = same;
                capacity = same;
            }
        } else {
            replace before -> after {
                data = same except { amount = before.amount + 2, };
                lock = same;
                type_script = same;
                capacity = same;
            }
        }
    }
"#;

fn valid() -> Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| Fixture::from_source(SOURCE)).clone()
}

fn rebind_dispositions(fixture: &mut Fixture) {
    for disposition in &mut fixture.record.typed_semantics.foundation.dispositions {
        disposition.semantic_node_id = canonical_hash(
            "cellscript-semantic-node-disposition-v5",
            &(
                disposition.id.as_str(),
                disposition.entry_id.as_str(),
                disposition.input_role.as_deref(),
                disposition.output_role.as_deref(),
                &disposition.input,
                &disposition.output,
                &disposition.envelope,
                disposition.enforcement.as_str(),
                &disposition.execution,
                &disposition.lifecycle_execution,
                &disposition.kernel_execution,
            ),
        )
        .unwrap();
    }
    fixture.rebind_layered_identities();
}

#[test]
fn independent_checker_reconstructs_fresh_and_pool_script_authority() {
    for source in [include_str!("../fixtures/authoring/absolute-fresh.cell"), include_str!("../fixtures/authoring/absolute-pool.cell")]
    {
        for fixed in [false, true] {
            let source = if fixed {
                format!("// test-artifact: {{\"entry\":\"verify\",\"layout\":\"Token\"}}\n{source}")
            } else {
                source.into()
            };
            let fixture = Fixture::from_source(&source);
            for index in 0..fixture.record.typed_semantics.foundation.dispositions.len() {
                let mut forged = fixture.clone();
                let record = &mut forged.record.typed_semantics.foundation.dispositions[index];
                assert!(record.lifecycle_execution.is_some());
                assert_eq!(
                    record.envelope.type_script,
                    if fixed { "checked-type-group-member" } else { "unconstrained-by-this-relation" }
                );
                record.envelope.type_script =
                    if fixed { "unconstrained-by-this-relation" } else { "checked-type-group-member" }.into();
                rebind_dispositions(&mut forged);
                assert_eq!(forged.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
            }
        }
    }
}

#[test]
fn independent_checker_requires_unique_creation_to_match_its_declared_identity() {
    let source = "module identity_defaults; resource Token has store,create identity(field(amount)) {amount:u64,} action mint(witness recipient:Address)->Token {create_unique::<Token>() {amount:7,} with_lock(recipient)}";
    let mut fixture = Fixture::from_source(source);
    let operation = fixture
        .record
        .typed_semantics
        .entries
        .iter_mut()
        .flat_map(|entry| &mut entry.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| operation.opcode == "create-unique")
        .unwrap();
    let TypedSemanticOperationDetail::CreateUnique { pattern, identity } = &mut operation.detail else { panic!("unique create") };
    *identity = "script-args".into();
    pattern.identity = "script-args".into();
    for record in &mut fixture.record.typed_semantics.foundation.dispositions {
        if let Some(OutputOrigin::Fresh { identity_policy }) = &mut record.output {
            *identity_policy = "script-args".into();
            record.envelope.logical_identity = "create:script-args".into();
        }
    }
    for pattern in fixture.metadata["actions"][0]["create_set"].as_array_mut().unwrap() {
        if pattern["operation"] == "create_unique" {
            pattern["identity_policy"] = Value::String("script_args".into());
        }
    }
    rebind_dispositions(&mut fixture);
    let result =
        check_bundle_values(&fixture.artifact, &fixture.metadata, &fixture.record, &fixture.source_map, &CheckerBudgets::default());
    assert!(
        matches!(&result, Err(error) if error.code==CheckerRejectionCode::V2419TypedSemanticsInvalid && error.message.contains("declared Cell identity")),
        "{:?}",
        result.map(|_| ())
    );
}

#[test]
fn independent_checker_binds_advanced_kernel_results_and_identity_without_promoting_envelopes() {
    for source in [
        include_str!("../fixtures/authoring/branch-unique-retire.cell"),
        include_str!("../fixtures/authoring/receipt-claim-kernel.cell"),
        include_str!("../fixtures/authoring/settle-kernel.cell"),
    ] {
        let fixture = Fixture::from_source(source);
        let index = fixture
            .record
            .typed_semantics
            .foundation
            .dispositions
            .iter()
            .position(|record| record.kernel_execution.as_ref().is_some_and(|site| site.result_local.is_some()))
            .unwrap();
        for mutation in 0..4 {
            let mut forged = fixture.clone();
            let record = &mut forged.record.typed_semantics.foundation.dispositions[index];
            match mutation {
                0 => record.kernel_execution = None,
                1 => record.kernel_execution.as_mut().unwrap().result_local = Some(u32::MAX),
                2 => record.envelope.logical_identity = "application-identity-not-established-and-capacity-preserved".into(),
                _ => record.enforcement = "checked-runtime".into(),
            }
            rebind_dispositions(&mut forged);
            assert_eq!(forged.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid), "mutation {mutation}");
        }
    }
}

#[test]
fn independent_interface_inspection_rejects_rehashed_cell_policy_and_receipt_relabelling() {
    let inspect = |fixture: &Fixture| {
        cellscript_artifact_checker::interface::inspect_bundle(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
    };
    for source in [
        include_str!("../fixtures/authoring/branch-unique-retire.cell"),
        include_str!("../fixtures/authoring/receipt-claim-kernel.cell"),
    ] {
        let result =
            compile(source, CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() })
                .unwrap();
        let fixture = Fixture::from_result(result);
        inspect(&fixture).unwrap();
        let mut forged = fixture.clone();
        let types = forged.metadata["public_interface"]["types"].as_array_mut().unwrap();
        if let Some(receipt) = types.iter_mut().find(|ty| ty["kind"] == "receipt") {
            receipt["claim_output"] = Value::String("Other".into());
        } else {
            types.iter_mut().find(|ty| ty["name"] == "Token").unwrap()["identity_policy"] = Value::String("script-args".into());
        }
        let declared = serde_json::from_value(forged.metadata["public_interface"].clone()).unwrap();
        let hash = cellscript::interface::hash(&declared);
        forged.metadata["interface_hash"] = Value::String(hash.clone());
        forged.record.typed_semantics.interface_hash = hash;
        forged.rebind_layered_identities();
        let error = inspect(&forged).unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2410MetadataBindingMismatch, "{error}");
        assert!(error.message.contains("identity") || error.message.contains("claim result"), "{error}");
    }
    let mut forged = Fixture::from_source(include_str!("../fixtures/authoring/receipt-claim-kernel.cell"));
    let types = &forged.record.typed_semantics.types;
    assert_eq!(
        types.iter().find(|ty| ty.name == "Token").unwrap().layout_hash,
        types.iter().find(|ty| ty.name == "Other").unwrap().layout_hash,
        "equal physical layouts must still retain distinct receipt result identities"
    );
    forged.record.typed_semantics.types.iter_mut().find(|ty| ty.name == "Voucher").unwrap().claim_output = Some("Other".into());
    forged.rebind_layered_identities();
    assert_eq!(forged.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
}

#[test]
fn independent_checker_keeps_kernel_successors_distinct_from_complete_replacement() {
    use cellscript_artifact_checker::SuccessorExecutionContract;
    let fixture = Fixture::from_source(include_str!("../fixtures/authoring/kernel-group-successor.cell"));
    assert_eq!(fixture.record.typed_semantics.foundation.dispositions.len(), 1);
    assert_eq!(
        fixture.record.typed_semantics.foundation.dispositions[0].execution.as_ref().unwrap().contract,
        SuccessorExecutionContract::FixedGroupKernel
    );
    for mutation in 0..4 {
        let mut forged = fixture.clone();
        let disposition = &mut forged.record.typed_semantics.foundation.dispositions[0];
        match mutation {
            0 => disposition.envelope.capacity = "preserve-and-check".into(),
            1 => disposition.execution.as_mut().unwrap().contract = SuccessorExecutionContract::Complete,
            2 => disposition.execution = None,
            _ => disposition.execution.as_mut().unwrap().output_local = disposition.execution.as_ref().unwrap().input_local,
        }
        rebind_dispositions(&mut forged);
        assert_eq!(forged.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid), "mutation {mutation}");
    }
}

#[test]
fn independent_checker_binds_mixed_kernel_occurrences_and_rejects_rehashed_forgery() {
    for source in [
        include_str!("../fixtures/authoring/branch-retire-consume.cell"),
        include_str!("../fixtures/authoring/branch-fresh-kernel.cell"),
    ] {
        let fixture = Fixture::from_source(source);
        fixture.check().unwrap();
        let kernel = fixture
            .record
            .typed_semantics
            .foundation
            .dispositions
            .iter()
            .position(|record| record.kernel_execution.is_some())
            .unwrap();
        let mut missing = fixture.clone();
        missing.record.typed_semantics.foundation.dispositions.remove(kernel);
        rebind_dispositions(&mut missing);
        assert_eq!(missing.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
        let mut wrong_site = fixture.clone();
        wrong_site.record.typed_semantics.foundation.dispositions[kernel].kernel_execution.as_mut().unwrap().operation = u32::MAX;
        rebind_dispositions(&mut wrong_site);
        assert_eq!(wrong_site.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
        let mut stripped = fixture.clone();
        stripped.record.typed_semantics.foundation.dispositions[kernel].kernel_execution = None;
        rebind_dispositions(&mut stripped);
        assert_eq!(stripped.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
        let mut capacity = fixture.clone();
        capacity.record.typed_semantics.foundation.dispositions[kernel].envelope.capacity = "preserve-and-check".into();
        rebind_dispositions(&mut capacity);
        assert_eq!(capacity.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
        let mut overlap = fixture.clone();
        let record = &mut overlap.record.typed_semantics.foundation.dispositions[kernel];
        record.lifecycle_execution =
            fixture.record.typed_semantics.foundation.dispositions.iter().find_map(|record| record.lifecycle_execution.clone());
        rebind_dispositions(&mut overlap);
        assert_eq!(overlap.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid));
    }
}

#[test]
fn independent_checker_rejects_quantity_burn_relabelled_as_object_retirement() {
    let source="module quantity_forgery; resource Token has store,consume,burn {amount:u64,} action burn(input token:Token) {burn_amount(token,field=amount);}";
    let mut fixture = Fixture::from_source(source);
    let disposition = &mut fixture.record.typed_semantics.foundation.dispositions[0];
    assert!(matches!(&disposition.input,Some(InputDisposition::Consumed { operation,.. }) if operation=="burn-amount"));
    let previous = disposition.semantic_node_id.clone();
    disposition.input = Some(InputDisposition::Retired { absence_policy: "declared-destruction-policy".into() });
    disposition.enforcement = "checked-runtime".into();
    disposition.envelope.completeness = "declared-policy-bound".into();
    disposition.envelope.logical_identity = "retire-under-declared-policy".into();
    rebind_dispositions(&mut fixture);
    let current = fixture.record.typed_semantics.foundation.dispositions[0].semantic_node_id.clone();
    for mapping in &mut fixture.source_map.semantic_mappings {
        if mapping.semantic_node_id == previous {
            mapping.semantic_node_id.clone_from(&current);
        }
    }
    fixture.source_map.canonicalize();
    fixture.rebind_sidecars();
    let error =
        check_bundle_values(&fixture.artifact, &fixture.metadata, &fixture.record, &fixture.source_map, &CheckerBudgets::default())
            .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "{error}");
    assert!(error.message.contains("quantity destruction cannot establish an object-absence disposition"), "{error}");
    let unreachable="module unreachable_quantity; resource Token has store,consume,burn {amount:u64,} action close(input token:Token) {if true {destroy_singleton_type(token);} else {burn_amount(token,field=amount);}}";
    let result = cellscript::compile(
        unreachable,
        CompileOptions {
            target: Some("riscv64-elf".into()),
            diagnostics: cellscript::CompilerDiagnostics::without_optimizations(),
            ..Default::default()
        },
    )
    .unwrap();
    let fixture = Fixture::from_result(result);
    assert!(matches!(fixture.record.typed_semantics.foundation.dispositions[0].input, Some(InputDisposition::Retired { .. })));
}

fn branch_lifecycle_fixtures() -> Vec<Fixture> {
    [
        include_str!("../fixtures/authoring/branch-fresh.cell"),
        include_str!("../fixtures/authoring/branch-retire.cell"),
        include_str!("../fixtures/authoring/branch-pool.cell"),
        include_str!("../fixtures/authoring/branch-pool-many.cell"),
        include_str!("../fixtures/authoring/branch-continue-retire.cell"),
    ]
    .into_iter()
    .map(Fixture::from_source)
    .collect()
}

#[test]
fn independent_checker_binds_branch_local_fresh_retire_and_pool() {
    for fixture in branch_lifecycle_fixtures() {
        let dispositions = &fixture.record.typed_semantics.foundation.dispositions;
        assert!(dispositions.iter().all(|disposition| disposition.lifecycle_execution.is_some() || disposition.execution.is_some()));
        let metadata = serde_json::from_value(fixture.metadata.clone()).unwrap();
        let expanded = cellscript::semantic_expansion::render(&metadata).unwrap();
        assert!(expanded.contains("lifecycle-execution occurrence="));
        if dispositions
            .iter()
            .any(|disposition| disposition.lifecycle_execution.as_ref().is_some_and(|execution| execution.pool.is_some()))
        {
            assert!(expanded.contains("pool-accounting inputs="));
            assert!(expanded.contains("conserved-fields=amount"));
        }
        fixture.check().unwrap();
    }
}

#[test]
fn independent_checker_rejects_omitted_and_forged_lifecycle_sites_after_rehashing() {
    for fixture in branch_lifecycle_fixtures() {
        if fixture.record.typed_semantics.foundation.dispositions.iter().any(|disposition| disposition.execution.is_some()) {
            // Mixed successor/retirement paths are covered by the shared path
            // mutation below; their event records intentionally have two shapes.
            let mut missing = fixture;
            missing.record.typed_semantics.foundation.dispositions.retain(|disposition| disposition.lifecycle_execution.is_none());
            rebind_dispositions(&mut missing);
            assert_code(&missing, CheckerRejectionCode::V2419TypedSemanticsInvalid);
            continue;
        }
        let mut missing = fixture.clone();
        missing.record.typed_semantics.foundation.dispositions.remove(0);
        rebind_dispositions(&mut missing);
        assert_code(&missing, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        let mut forged = fixture.clone();
        let dispositions = &mut forged.record.typed_semantics.foundation.dispositions;
        let other = dispositions
            .iter()
            .skip(1)
            .find(|other| other.input_role == dispositions[0].input_role && other.output_role == dispositions[0].output_role)
            .unwrap()
            .lifecycle_execution
            .clone();
        dispositions[0].lifecycle_execution = other;
        rebind_dispositions(&mut forged);
        assert_code(&forged, CheckerRejectionCode::V2419TypedSemanticsInvalid);

        let mut stripped = fixture;
        stripped.record.typed_semantics.foundation.dispositions[0].lifecycle_execution = None;
        rebind_dispositions(&mut stripped);
        assert_code(&stripped, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn independent_checker_rejects_forged_pool_sums_after_rehashing() {
    let fixture = Fixture::from_source(include_str!("../fixtures/authoring/branch-pool.cell"));
    for mutation in ["field", "member", "missing"] {
        let mut changed = fixture.clone();
        for disposition in &mut changed.record.typed_semantics.foundation.dispositions {
            let pool = disposition.lifecycle_execution.as_mut().unwrap().pool.as_mut().unwrap();
            match mutation {
                "field" => pool.conserved_fields[0] = "nonexistent".into(),
                "member" => pool.inputs[0] = pool.outputs[0],
                "missing" => pool.conserved_fields.clear(),
                _ => unreachable!(),
            }
        }
        rebind_dispositions(&mut changed);
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn independent_checker_requires_a_guard_for_the_claimed_numeric_field() {
    let source = include_str!("../fixtures/authoring/branch-pool.cell")
        .replace("amount: u64,", "amount: u64, marker: u64,")
        .replace("amount = conserve,", "amount = conserve, marker { next = 1, },");
    let mut changed = Fixture::from_source(&source);
    for disposition in &mut changed.record.typed_semantics.foundation.dispositions {
        disposition.lifecycle_execution.as_mut().unwrap().pool.as_mut().unwrap().conserved_fields = vec!["marker".into()];
        match (&mut disposition.input, &mut disposition.output) {
            (Some(InputDisposition::Pooled { accounting_obligation, .. }), _)
            | (_, Some(OutputOrigin::PoolResult { accounting_obligation, .. })) => {
                *accounting_obligation = "checked-u128-field-sum-equality:marker".into();
            }
            _ => unreachable!(),
        }
    }
    rebind_dispositions(&mut changed);
    assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_admits_both_exclusive_successor_occurrences() {
    let fixture = valid();
    let relations = fixture
        .record
        .typed_semantics
        .foundation
        .dispositions
        .iter()
        .filter(|disposition| disposition.execution.is_some())
        .collect::<Vec<_>>();
    assert_eq!(relations.len(), 2);
    assert_eq!(relations[0].input_role, relations[1].input_role);
    assert_ne!(relations[0].execution.as_ref().unwrap().consume_block, relations[1].execution.as_ref().unwrap().consume_block);
    assert!(relations.iter().all(|relation| relation.envelope.logical_identity == "application-identity-not-established"));
    let metadata = serde_json::from_value(fixture.metadata.clone()).unwrap();
    let expanded = cellscript::semantic_expansion::render(&metadata).unwrap();
    assert!(expanded.contains("field amount value="), "{expanded}");
    assert!(expanded.contains("\"operator\":\"add\""), "{expanded}");
    assert!(expanded.contains("value-local"), "{expanded}");
    fixture.check().unwrap();
}

#[test]
fn independent_checker_rejects_omitted_accepting_branch_after_rehashing() {
    let mut fixture = valid();
    fixture.record.typed_semantics.foundation.dispositions.remove(0);
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_duplicate_execution_site_after_rehashing() {
    let mut fixture = valid();
    let dispositions = &mut fixture.record.typed_semantics.foundation.dispositions;
    let mut duplicate = dispositions[0].execution.clone().unwrap();
    duplicate.occurrence = dispositions[1].execution.as_ref().unwrap().occurrence;
    dispositions[1].execution = Some(duplicate);
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_unproven_business_identity_after_rehashing() {
    let mut fixture = valid();
    fixture.record.typed_semantics.foundation.dispositions[0].envelope.logical_identity = "preserve-and-check".into();
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_foreign_branch_success_site_after_rehashing() {
    let mut fixture = valid();
    let dispositions = &mut fixture.record.typed_semantics.foundation.dispositions;
    let foreign = dispositions[1].execution.as_ref().unwrap().success_block;
    dispositions[0].execution.as_mut().unwrap().success_block = foreign;
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_preservation_claim_with_erased_execution() {
    let mut fixture = valid();
    let dispositions = &mut fixture.record.typed_semantics.foundation.dispositions;
    dispositions.truncate(1);
    dispositions[0].execution = None;
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_invented_field_preservation_after_rehashing() {
    let mut fixture = valid();
    fixture.record.typed_semantics.foundation.dispositions[0].envelope.data_fields[0].treatment = "preserve".into();
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_native_execution_erasure_with_weaker_labels() {
    let mut fixture = valid();
    let dispositions = &mut fixture.record.typed_semantics.foundation.dispositions;
    dispositions.truncate(1);
    dispositions[0].execution = None;
    dispositions[0].envelope.capacity = "not-constrained".into();
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

#[test]
fn independent_checker_rejects_invented_successor_correspondence() {
    for (cardinality, correspondence) in
        [("many-to-many", "group-input[0]->group-output[0]"), ("one-input-to-one-output", "group-input[0]->group-output[7]")]
    {
        let mut fixture = valid();
        let envelope = &mut fixture.record.typed_semantics.foundation.dispositions[0].envelope;
        envelope.cardinality = cardinality.into();
        envelope.correspondence = correspondence.into();
        rebind_dispositions(&mut fixture);
        assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn independent_checker_rejects_forged_field_operand_after_rehashing() {
    let mut fixture = valid();
    fixture.record.typed_semantics.foundation.dispositions[0].execution.as_mut().unwrap().fields[0].value =
        cellscript_artifact_checker::TypedSemanticOperand {
            local: None,
            ty: "u64".into(),
            constant: Some(TypedSemanticConstant::U64("0".into())),
        };
    rebind_dispositions(&mut fixture);
    assert_code(&fixture, CheckerRejectionCode::V2419TypedSemanticsInvalid);
}

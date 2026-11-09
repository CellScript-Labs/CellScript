use super::*;
use std::sync::OnceLock;

const SOURCE: &str = r#"
module authoring_checker::branches;
resource Note has store, replace, relock { amount: u64, }
type_script Update on type_group<Note> {
    action update(
        input before: Note from group_input[0],
        witness choose: bool from group_witness.input_type,
        output after: Note from group_output[0],
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
}
"#;

fn valid() -> Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| Fixture::from_source(SOURCE)).clone()
}

fn rebind_dispositions(fixture: &mut Fixture) {
    for disposition in &mut fixture.record.typed_semantics.foundation.dispositions {
        disposition.semantic_node_id = canonical_hash(
            "cellscript-semantic-node-disposition-v2",
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
            ),
        )
        .unwrap();
    }
    fixture.rebind_layered_identities();
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

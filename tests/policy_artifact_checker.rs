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

fn fixed_external_codec(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::external_codec::CheckedFixedExternalCodec, CheckerError> {
    cellscript_artifact_checker::external_codec::check_fixed_external_codec(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        &CheckerBudgets::default(),
    )
}
fn external_fixture(source: &str, opt: u8) -> Fixture {
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    Fixture::new_source_with(source, CellScriptEdition::Edition2026, opt, selected)
}
const EXTERNAL_SOURCE: &str = r#"
module external_codec
resource Token has store, consume { amount: u64 }
action burn(input token: Token, witness value: u64) {
    verification require token.amount > 0 require value > 0 consume token
}
"#;
#[test]
fn fixed_external_codec_checks_complete_available_public_scalar_profile() {
    for opt in 0..=3 {
        for ty in ["u8", "u16", "u32", "u64", "i32"] {
            let source = if ty == "i32" {
                EXTERNAL_SOURCE
                    .replace("witness value: u64", "witness value: i32, witness zero: i32")
                    .replace("value > 0", "value > zero")
            } else {
                EXTERNAL_SOURCE.replace("witness value: u64", &format!("witness value: {ty}"))
            };
            let fixture = external_fixture(&source, opt);
            let checked = fixed_external_codec(&fixture).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["callables"][0]["availability"], "external-policy-entry");
            assert_eq!(record["public_layouts"][0]["width"], 8);
            assert!(checked.fields().is_some());
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            let entry = fixture.record.entries.iter().find(|entry| entry.name == "burn").unwrap();
            let address = fixture.record.blocks.iter().find(|block| block.id == entry.entry_block).unwrap().range.start + 24;
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            assert_eq!((original >> 20) & 31, 12);
            for (mask, bits) in [(31 << 20, 17 << 20), (0x1f << 7, (original.wrapping_add(16 << 7)) & (0x1f << 7))] {
                let mut changed = fixture.clone();
                changed.replace_machine_word(address, (original & !mask) | bits);
                changed.check().unwrap();
                fixed_cell_fields(&changed).unwrap();
                assert_eq!(fixed_external_codec(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
            }
        }
    }
}
#[test]
fn fixed_external_codec_does_not_grant_pruned_helpers_outputs_or_unsupported_layouts() {
    for opt in 0..=3 {
        for extra in [
            "public fn extra() { }",
            "public struct Extra { value: bool }",
            "public struct Extra { values: [u64; 2] }",
            "public enum Extra { A, B }",
        ] {
            let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\n{extra}"), opt);
            fixture.check().unwrap();
            assert_eq!(fixed_external_codec(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        for source in [
            "module output_external\nresource Token has store, consume { amount: u64 }\naction burn(witness recipient: Address) { verification create Token { amount: 7 } with_lock(recipient) }",
            "module multiple_external\nresource Token has store, consume { amount: u64 }\naction burn(input left: Token, input right: Token) { verification require left.amount > 0 require right.amount > 0 consume left consume right }",
        ] {
            let fixture = external_fixture(source, opt);
            assert_eq!(fixed_external_codec(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        let checked = fixed_external_codec(&fixture).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(record["callables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|callable| callable["availability"] == "declaration-only" && callable["entry"].is_null()));
    }
}
#[test]
fn fixed_external_codec_generic_instances_are_not_declaration_only() {
    for opt in 0..=3 {
        let declaration = "public struct Pair<T: fixed_value> { value: T }";
        let absent = external_fixture(&format!("{EXTERNAL_SOURCE}\n{declaration}"), opt);
        let checked = fixed_external_codec(&absent).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(record["public_layouts"].as_array().unwrap().iter().any(|layout| layout["availability"] == "declaration-only"));
        let instantiated =
            EXTERNAL_SOURCE.replace("require value > 0", "let pair: Pair<u64> = Pair<u64> { value: value } require pair.value > 0");
        let fixture = external_fixture(&format!("{instantiated}\n{declaration}"), opt);
        assert!(fixture
            .record
            .typed_semantics
            .instantiations
            .iter()
            .any(|instance| instance.kind == "struct" && instance.template == "Pair"));
        let error = fixed_external_codec(&fixture).unwrap_err();
        assert!(error.message.contains("generic concrete layout"), "{error:?}");
        let instantiated = EXTERNAL_SOURCE.replace("require value > 0", "require first<u64>(value) > 0");
        let fixture = external_fixture(&format!("{instantiated}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        if fixture
            .record
            .typed_semantics
            .instantiations
            .iter()
            .any(|instance| instance.kind == "function" && instance.template == "first")
        {
            let error = fixed_external_codec(&fixture).unwrap_err();
            assert!(error.message.contains("generic concrete instance"), "{error:?}");
        } else {
            // Inlining may erase the concrete helper and its instance record.
            // That leaves a symbolic declaration, never an external entry.
            assert!(fixture.record.typed_semantics.entries.iter().all(|entry| entry.kind != "function"));
            let checked = fixed_external_codec(&fixture).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert!(record["callables"].as_array().unwrap().iter().any(|callable| callable["declaration"] == "external_codec::first"
                && callable["availability"] == "declaration-only"
                && callable["entry"].is_null()));
        }
    }
}

#[test]
fn fixed_external_codec_binds_machine_bytes_without_claiming_predicate_equivalence() {
    for opt in 0..=3 {
        let required = external_fixture(EXTERNAL_SOURCE, opt);
        let other = external_fixture(&EXTERNAL_SOURCE.replace("token.amount > 0", "token.amount == 7"), opt);
        let required_checked = fixed_external_codec(&required).unwrap();
        let other_checked = fixed_external_codec(&other).unwrap();
        required_checked
            .parameters()
            .module_projection()
            .check_required_contracts(other_checked.parameters().module_projection())
            .unwrap();
        assert_ne!(required_checked.identity(), other_checked.identity());
        let narrowed = external_fixture(&EXTERNAL_SOURCE.replace("amount: u64", "amount: u32"), opt);
        let narrowed_checked = fixed_external_codec(&narrowed).unwrap();
        assert!(required_checked
            .parameters()
            .module_projection()
            .check_required_contracts(narrowed_checked.parameters().module_projection())
            .is_err());
    }
}

fn receipt_bundle(fixture: &Fixture) -> [Vec<u8>; 4] {
    [
        fixture.artifact.clone(),
        serde_json::to_vec(&fixture.metadata).unwrap(),
        serde_json::to_vec(&fixture.record).unwrap(),
        serde_json::to_vec(&fixture.source_map).unwrap(),
    ]
}
fn receipt_deployment(fixture: &Fixture, args: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let type_script =
        packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
    let tx = TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
                .build(),
        )
        .output(
            packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock).type_(Some(type_script.clone()).pack()).build(),
        )
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .build();
    let type_hash = fixture.metadata["target_profile"]["name"] == "ckb-type-hash";
    let selected = packed::Script::new_builder()
        .code_hash(if type_hash { type_script.calc_script_hash() } else { packed::CellOutput::calc_data_hash(&fixture.artifact) })
        .hash_type(if type_hash { 1u8 } else { 4u8 })
        .args(Bytes::from(args).pack())
        .build();
    (tx.data().raw().as_slice().to_vec(), selected.as_slice().to_vec())
}
fn fixed_policy_receipt(
    fixture: &Fixture,
    args: Vec<u8>,
) -> Result<cellscript_artifact_checker::fixed_policy_receipt::CheckedFixedPolicyReceipt, CheckerError> {
    let bytes = receipt_bundle(fixture);
    let (raw, script) = receipt_deployment(fixture, args);
    cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt(
        bytes.each_ref().map(Vec::as_slice),
        &raw,
        0,
        &script,
        &CheckerBudgets::default(),
    )
}
#[test]
fn fixed_policy_receipt_recomputes_complete_finite_fields_from_actual_inputs() {
    for opt in 0..=3 {
        for target in ["ckb", "ckb-type-hash"] {
            let mut declaration = declaration();
            declaration.actions.retain(|action| action.action == "burn");
            declaration.common_checks.clear();
            let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2026, opt, declaration, target);
            let bytes = receipt_bundle(&fixture);
            let (raw, script) = receipt_deployment(&fixture, vec![1, 2, 3]);
            let checked = fixed_policy_receipt(&fixture, vec![1, 2, 3]).unwrap();
            checked.check_unchanged_inputs(bytes.each_ref().map(Vec::as_slice), &raw, 0, &script).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["schema"], "cellscript-fixed-policy-interface-receipt-v1");
            assert_eq!(record["declared_interface"], fixture.metadata["public_interface"]);
            assert_eq!(
                record["entry_contract"],
                serde_json::to_value(&fixture.record.typed_semantics.foundation.entry_contract).unwrap()
            );
            assert_eq!(record["entry_contract"]["script_role"], "type");
            let origin = checked.target_origin().origin();
            assert_eq!(record["module_contract"], origin.codec().parameters().module_projection().identity());
            assert_eq!(record["external_codec"], origin.codec().identity());
            assert_eq!(record["code_origin"], origin.identity());
            assert_eq!(record["target_selection"], checked.target_origin().identity());
            for (index, input) in bytes.iter().enumerate() {
                assert_eq!(record["bundle_byte_lengths"][index], input.len());
                assert_eq!(record["bundle_byte_hashes"][index], code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(input)));
            }
            assert_eq!(record["artifact_report"], serde_json::to_value(checked.artifact_report()).unwrap());
            assert!(!checked.artifact_report().semantic_equivalence_claimed);
            assert_eq!(checked.artifact_report().chain_evidence, cellscript_artifact_checker::EvidenceState::NotProvided);
            assert_eq!(checked.artifact_report().ckb_vm_evidence, cellscript_artifact_checker::EvidenceState::NotExecuted);
            let mut material = b"cellscript-fixed-policy-interface-receipt-id-v1\0".to_vec();
            material.extend_from_slice(&checked.canonical_bytes().unwrap());
            assert_eq!(checked.identity(), code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(&material)));
            assert_eq!(checked.identity(), fixed_policy_receipt(&fixture, vec![1, 2, 3]).unwrap().identity());
            assert_ne!(checked.identity(), fixed_policy_receipt(&fixture, vec![1, 2, 4]).unwrap().identity());
        }
    }
}
#[test]
fn fixed_policy_receipts_compare_directionally_without_predicate_or_deployment_equivalence() {
    for opt in 0..=3 {
        let required = fixed_policy_receipt(&external_fixture(EXTERNAL_SOURCE, opt), vec![1]).unwrap();
        let other =
            fixed_policy_receipt(&external_fixture(&EXTERNAL_SOURCE.replace("token.amount > 0", "token.amount == 7"), opt), vec![2])
                .unwrap();
        required.check_required_contracts(&other).unwrap();
        assert_ne!(required.identity(), other.identity());
        assert_ne!(required.target_origin().origin().artifact_hash(), other.target_origin().origin().artifact_hash());
        for source in [
            EXTERNAL_SOURCE.replace("amount: u64", "amount: u32"),
            EXTERNAL_SOURCE.replace("witness value: u64", "witness value: u32"),
        ] {
            let narrowed = fixed_policy_receipt(&external_fixture(&source, opt), vec![1]).unwrap();
            assert!(required.check_required_contracts(&narrowed).is_err());
        }
        let source = format!("{EXTERNAL_SOURCE}\naction audit(input token: Token, witness value: u64) {{ verification require token.amount > 0 require value > 0 consume token }}");
        let mut declaration = declaration();
        declaration.actions.retain(|action| action.action == "burn");
        declaration.actions.push(ArtifactAction { tag: 55, action: "audit".into() });
        declaration.common_checks.clear();
        let extension =
            fixed_policy_receipt(&Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, declaration), vec![3])
                .unwrap();
        required.check_required_contracts(&extension).unwrap();
        assert!(extension.check_required_contracts(&required).is_err());
    }
}
#[test]
fn fixed_policy_receipt_admits_proven_constants_and_rejects_unproven_ones() {
    for opt in 0..=3 {
        // A constant inside the closed grammar is independently re-evaluated
        // and admitted through the finite external codec.
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LIMIT: u64 = 3 * 7"), opt);
        let checked = fixed_external_codec(&fixture).unwrap();
        let codec_record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(codec_record["constants"][0]["value"], "21");
        assert!(fixed_policy_receipt(&fixture, vec![1]).is_ok());
        // Constants outside the closed grammar keep no proven value and the
        // receipt fails closed exactly as before.
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LABEL: String = \"sealed\""), opt);
        let error = fixed_policy_receipt(&fixture, vec![1]).unwrap_err();
        assert!(error.message.contains("constant"), "{error:?}");
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic struct Pair<T: fixed_value> {{ value: T }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}"), opt);
        let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
        let record: Value = serde_json::from_slice(&checked.target_origin().origin().codec().canonical_bytes().unwrap()).unwrap();
        assert!(record["callables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["availability"] == "declaration-only" && entry["entry"].is_null()));
        assert!(record["public_layouts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["availability"] == "declaration-only" && entry["width"].is_null()));
    }
}
#[test]
fn fixed_policy_receipt_detects_postcheck_substitution_of_every_exact_input() {
    use cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt;
    for opt in 0..=3 {
        let fixture = external_fixture(EXTERNAL_SOURCE, opt);
        let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
        let original = receipt_bundle(&fixture);
        let (raw, script) = receipt_deployment(&fixture, vec![1]);
        for index in 0..4 {
            let mut changed = original.clone();
            changed[index].push(b'\n');
            assert!(checked
                .check_unchanged_inputs(changed.each_ref().map(Vec::as_slice), &raw, 0, &script)
                .unwrap_err()
                .message
                .contains("changed"));
        }
        let mut altered_raw = raw.clone();
        *altered_raw.last_mut().unwrap() ^= 1;
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &altered_raw, 0, &script).is_err());
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &raw, 1, &script).is_err());
        let (_, other_script) = receipt_deployment(&fixture, vec![2]);
        assert!(checked.check_unchanged_inputs(original.each_ref().map(Vec::as_slice), &raw, 0, &other_script).is_err());
        // A semantically identical valid metadata encoding is a distinct frozen tuple.
        let mut pretty = original.clone();
        pretty[1] = serde_json::to_vec_pretty(&fixture.metadata).unwrap();
        let other =
            check_fixed_policy_receipt(pretty.each_ref().map(Vec::as_slice), &raw, 0, &script, &CheckerBudgets::default()).unwrap();
        checked.check_required_contracts(&other).unwrap();
        assert_ne!(checked.identity(), other.identity());
    }
}
#[test]
fn fixed_policy_receipt_bounds_creation_and_substitution_checks_before_any_parser_or_hash() {
    use cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt;
    let fixture = external_fixture(EXTERNAL_SOURCE, 0);
    let checked = fixed_policy_receipt(&fixture, vec![1]).unwrap();
    let original = receipt_bundle(&fixture);
    let (raw, script) = receipt_deployment(&fixture, vec![1]);
    for field in ["artifact", "record", "source-map"] {
        let mut budgets = CheckerBudgets::default();
        match field {
            "artifact" => budgets.artifact_bytes = 0,
            "record" => budgets.record_bytes = 0,
            _ => budgets.source_map_bytes = 0,
        }
        assert_eq!(
            check_fixed_policy_receipt(original.each_ref().map(Vec::as_slice), &raw, 0, &script, &budgets).unwrap_err().code,
            CheckerRejectionCode::V2400BudgetExceeded
        );
    }
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    assert_eq!(
        check_fixed_policy_receipt([&oversized, &[], &[], &[]], &[], 0, &[], &CheckerBudgets::default()).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    assert_eq!(
        checked.check_unchanged_inputs([&oversized, &[], &[], &[]], &[], 0, &[]).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    let limit = vec![0; 4 * 1024 * 1024];
    for constructor in [true, false] {
        let bundle = [limit.as_slice(); 4];
        let error = if constructor {
            check_fixed_policy_receipt(bundle, &[0], 0, &[], &CheckerBudgets::default()).unwrap_err()
        } else {
            checked.check_unchanged_inputs(bundle, &[0], 0, &[]).unwrap_err()
        };
        assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    }
}

fn code_origin_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn code_origin(
    fixture: &Fixture,
    raw: &[u8],
    index: u32,
    script: &[u8],
) -> Result<cellscript_artifact_checker::code_origin::CheckedCodeCellOrigin, CheckerError> {
    cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        raw,
        index,
        script,
        &CheckerBudgets::default(),
    )
}
#[test]
fn code_cell_origin_recomputes_sdk_transaction_script_and_code_identities() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    for opt in 0..=3 {
        let fixture = external_fixture(EXTERNAL_SOURCE, opt);
        let lock =
            packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::from(vec![1, 2, 3]).pack()).build();
        let type_script =
            packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
        let input = packed::CellInput::new_builder()
            .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
            .build();
        let code = packed::CellOutput::new_builder()
            .capacity(1000000000000u64)
            .lock(lock.clone())
            .type_(Some(type_script.clone()).pack())
            .build();
        let other = packed::CellOutput::new_builder().capacity(100000000000u64).lock(lock.clone()).build();
        let tx = TransactionBuilder::default()
            .input(input.clone())
            .output(code.clone())
            .output_data(Bytes::from(fixture.artifact.clone()).pack())
            .output(other.clone())
            .output_data(Bytes::from(vec![1, 2, 3]).pack())
            .build();
        let raw = tx.data().raw();
        for hash_type in [0u8, 1, 2, 4] {
            let code_hash =
                if hash_type == 1 { type_script.calc_script_hash() } else { packed::CellOutput::calc_data_hash(&fixture.artifact) };
            let selected = packed::Script::new_builder()
                .code_hash(code_hash.clone())
                .hash_type(hash_type)
                .args(Bytes::from(vec![31, 32, 33]).pack())
                .build();
            let checked = code_origin(&fixture, raw.as_slice(), 0, selected.as_slice()).unwrap();
            assert_eq!(checked.transaction_hash(), code_origin_hex(tx.hash().as_slice()));
            assert_eq!(checked.artifact_hash(), code_origin_hex(packed::CellOutput::calc_data_hash(&fixture.artifact).as_slice()));
            assert_eq!(checked.type_script_hash(), Some(code_origin_hex(type_script.calc_script_hash().as_slice()).as_str()));
            assert_eq!(checked.selected_script_hash(), code_origin_hex(selected.calc_script_hash().as_slice()));
            assert_eq!(checked.output_index(), 0);
            assert_eq!(checked.selected_hash_type(), hash_type);
            let other_args = selected.clone().as_builder().args(Bytes::from(vec![31, 32, 34]).pack()).build();
            let changed = code_origin(&fixture, raw.as_slice(), 0, other_args.as_slice()).unwrap();
            assert_eq!(checked.transaction_hash(), changed.transaction_hash());
            assert_ne!(checked.selected_script_hash(), changed.selected_script_hash());
            assert_ne!(checked.identity(), changed.identity());
            let wrong = selected.clone().as_builder().code_hash([0u8; 32].pack()).build();
            assert!(code_origin(&fixture, raw.as_slice(), 0, wrong.as_slice()).is_err());
            assert!(code_origin(&fixture, raw.as_slice(), 1, selected.as_slice()).is_err());
            assert!(code_origin(&fixture, raw.as_slice(), 2, selected.as_slice()).is_err());
            let mut altered = fixture.artifact.clone();
            let last = altered.len() - 1;
            altered[last] ^= 1;
            let bad_code = TransactionBuilder::default()
                .input(input.clone())
                .output(code.clone())
                .output_data(Bytes::from(altered).pack())
                .build();
            assert!(code_origin(&fixture, bad_code.data().raw().as_slice(), 0, selected.as_slice()).is_err());
        }
    }
}
#[test]
fn code_cell_target_requires_the_actual_checked_profile_even_for_valid_byte_origins() {
    use cellscript_artifact_checker::code_origin::check_code_cell_target;
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    for opt in 0..=3 {
        for (target, accepted) in [("ckb", 4u8), ("ckb-type-hash", 1u8)] {
            let mut declaration = declaration();
            declaration.actions.retain(|action| action.action == "burn");
            declaration.common_checks.clear();
            let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2026, opt, declaration, target);
            let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
            let type_script =
                packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
            let tx = TransactionBuilder::default()
                .output(
                    packed::CellOutput::new_builder()
                        .capacity(1000000000000u64)
                        .lock(lock)
                        .type_(Some(type_script.clone()).pack())
                        .build(),
                )
                .output_data(Bytes::from(fixture.artifact.clone()).pack())
                .build();
            for hash_type in [0u8, 1, 2, 4] {
                let code_hash = if hash_type == 1 {
                    type_script.calc_script_hash()
                } else {
                    packed::CellOutput::calc_data_hash(&fixture.artifact)
                };
                let selected = packed::Script::new_builder()
                    .code_hash(code_hash)
                    .hash_type(hash_type)
                    .args(Bytes::from(vec![31, 32, 33]).pack())
                    .build();
                // All four are byte-bound to this exact SDK-built code Cell.
                let origin = code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice()).unwrap();
                let origin_id = origin.identity().to_owned();
                let result = check_code_cell_target(origin);
                if hash_type == accepted {
                    let checked = result.unwrap();
                    assert_eq!(checked.origin().identity(), origin_id);
                    assert_eq!(checked.runtime_contract().target_profile, target);
                    let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
                    assert_eq!(record["code_origin"], origin_id);
                    assert_eq!(record["runtime"], fixture.metadata["public_interface"]["runtime_contract"]);
                    assert_eq!(record["deployment_hash_type"], if accepted == 4 { "data2" } else { "type" });
                    let mut material = b"cellscript-code-cell-target-id-v1\0".to_vec();
                    material.extend_from_slice(&checked.canonical_bytes().unwrap());
                    assert_eq!(checked.identity(), code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(&material)));
                    // Exact args remain bound after profile checking.
                    let changed = selected.as_builder().args(Bytes::from(vec![31, 32, 34]).pack()).build();
                    let other =
                        check_code_cell_target(code_origin(&fixture, tx.data().raw().as_slice(), 0, changed.as_slice()).unwrap())
                            .unwrap();
                    assert_ne!(checked.identity(), other.identity());
                } else {
                    let error = result.unwrap_err();
                    assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    assert!(error.message.contains("selected Script hash type differs"), "{error:?}");
                }
            }
        }
    }
}

#[test]
fn code_cell_origin_rejects_noncanonical_and_unselected_transaction_structures() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, 3);
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let tx = TransactionBuilder::default()
        .output(code.clone())
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .output(code.clone())
        .output_data(Bytes::new().pack())
        .build();
    let raw = tx.data().raw();
    let selected = lock.clone().as_builder().code_hash(packed::CellOutput::calc_data_hash(&fixture.artifact)).build();
    code_origin(&fixture, raw.as_slice(), 0, selected.as_slice()).unwrap();
    for index in [0usize, 4, 8, 12, 16, 20, 24] {
        let mut malformed = raw.as_slice().to_vec();
        malformed[index] ^= 1;
        assert!(code_origin(&fixture, &malformed, 0, selected.as_slice()).is_err());
    }
    for length in [0, 1, 3, 4, 7, 27, raw.as_slice().len() - 1] {
        assert!(code_origin(&fixture, &raw.as_slice()[..length], 0, selected.as_slice()).is_err());
    }
    let mut trailing = raw.as_slice().to_vec();
    trailing.push(0);
    assert!(code_origin(&fixture, &trailing, 0, selected.as_slice()).is_err());
    let broken_lock = lock.as_builder().hash_type(3u8).build();
    let unselected = code.as_builder().lock(broken_lock).build();
    let bad = TransactionBuilder::default()
        .output(tx.outputs().get(0).unwrap())
        .output_data(Bytes::from(fixture.artifact.clone()).pack())
        .output(unselected)
        .output_data(Bytes::new().pack())
        .build();
    assert!(code_origin(&fixture, bad.data().raw().as_slice(), 0, selected.as_slice()).is_err());
    let wrong_cardinality = TransactionBuilder::default().output(tx.outputs().get(0).unwrap()).build();
    assert!(code_origin(&fixture, wrong_cardinality.data().raw().as_slice(), 0, selected.as_slice()).is_err());
}

#[test]
fn code_cell_origin_has_exact_script_count_and_preparse_byte_bounds() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, 3);
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let code_hash = packed::CellOutput::calc_data_hash(&fixture.artifact);
    let script = lock.clone().as_builder().code_hash(code_hash.clone()).args(Bytes::from(vec![0xCC; 4043]).pack()).build();
    assert_eq!(script.as_slice().len(), 4096);
    for count in [256, 257] {
        let mut tx = TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack());
        for _ in 1..count {
            tx = tx.output(code.clone()).output_data(Bytes::new().pack());
        }
        let tx = tx.build();
        let result = code_origin(&fixture, tx.data().raw().as_slice(), 0, script.as_slice());
        if count == 256 {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().message.contains("dynvec count"));
        }
    }
    let tx = TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack()).build();
    let too_long = script.as_builder().args(Bytes::from(vec![0xCC; 4044]).pack()).build();
    assert_eq!(too_long.as_slice().len(), 4097);
    assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, too_long.as_slice()).unwrap_err().message.contains("Script exceeds"));
    let selected = lock.as_builder().code_hash(code_hash).build();
    let budgets = CheckerBudgets { record_bytes: 1, ..CheckerBudgets::default() };
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
        ],
        tx.data().raw().as_slice(),
        0,
        selected.as_slice(),
        &budgets,
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    for budgets in [
        CheckerBudgets { artifact_bytes: 0, ..CheckerBudgets::default() },
        CheckerBudgets { record_bytes: 0, ..CheckerBudgets::default() },
        CheckerBudgets { source_map_bytes: 0, ..CheckerBudgets::default() },
    ] {
        // Malformed raw bytes must never reach a parser with an over-budget
        // source file, even when that file would be parsed in a later phase.
        let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
            [
                &fixture.artifact,
                &serde_json::to_vec(&fixture.metadata).unwrap(),
                &serde_json::to_vec(&fixture.record).unwrap(),
                &serde_json::to_vec(&fixture.source_map).unwrap(),
            ],
            &[0],
            0,
            &[],
            &budgets,
        )
        .unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    }
    let exactly = vec![b'?'; 4 * 1024 * 1024];
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [exactly.as_slice(); 4],
        &[1],
        0,
        &[],
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    let oversized = vec![b'?'; 4 * 1024 * 1024 + 1];
    let error = cellscript_artifact_checker::code_origin::check_code_cell_origin(
        [oversized.as_slice(), &[], &[], &[]],
        &[],
        0,
        &[],
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
}

#[test]
fn code_cell_origin_rejects_duplicate_and_overbound_dependency_input_sets() {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, 3);
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let selected = lock.as_builder().code_hash(packed::CellOutput::calc_data_hash(&fixture.artifact)).build();
    let base = || TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack());
    for (kind, maximum) in [("headers", 64usize), ("deps", 64), ("inputs", 256)] {
        for count in [maximum, maximum + 1] {
            let mut tx = base();
            for index in 0..count {
                let mut bytes = [7u8; 32];
                bytes[..4].copy_from_slice(&(index as u32).to_le_bytes());
                let point = packed::OutPoint::new_builder().tx_hash(bytes.pack()).index(3u32).build();
                tx = match kind {
                    "headers" => tx.header_dep(bytes.pack()),
                    "deps" => tx.cell_dep(packed::CellDep::new_builder().out_point(point).dep_type((index % 2) as u8).build()),
                    _ => tx.input(packed::CellInput::new_builder().previous_output(point).build()),
                };
            }
            let tx = tx.build();
            let result = code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice());
            if count == maximum {
                result.unwrap();
            } else {
                assert!(result.unwrap_err().message.contains("fixvec count/width"));
            }
        }
    }
    let point = packed::OutPoint::new_builder().tx_hash([8u8; 32].pack()).index(3u32).build();
    let dep = packed::CellDep::new_builder().out_point(point.clone()).dep_type(0u8).build();
    let input = packed::CellInput::new_builder().previous_output(point.clone()).build();
    let cases = [
        (base().cell_dep(dep.clone()).cell_dep(dep).build(), "duplicate or unsupported raw CellDep"),
        (base().input(input.clone()).input(input).build(), "duplicate input OutPoint"),
        (base().header_dep([8u8; 32].pack()).header_dep([8u8; 32].pack()).build(), "duplicate header dependency"),
        (base().cell_dep(packed::CellDep::new_builder().out_point(point).dep_type(2u8).build()).build(), "unsupported raw CellDep"),
    ];
    for (tx, message) in cases {
        assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice()).unwrap_err().message.contains(message));
    }
    let selected = selected.as_builder().hash_type(1u8).build();
    let tx = base().build();
    assert!(code_origin(&fixture, tx.data().raw().as_slice(), 0, selected.as_slice())
        .unwrap_err()
        .message
        .contains("no actual code Cell Type Script"));
}

fn fixed_cell_reads(fixture: &Fixture) -> Result<cellscript_artifact_checker::fixed_cell_reads::CheckedFixedCellReads, CheckerError> {
    cellscript_artifact_checker::fixed_cell_reads::check_fixed_cell_reads(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

fn fixed_cell_storage(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::fixed_cell_storage::CheckedFixedCellParameterStorage, CheckerError> {
    cellscript_artifact_checker::fixed_cell_storage::check_fixed_cell_parameter_storage(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

fn fixed_cell_fields(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::fixed_cell_fields::CheckedFixedCellScalarFields, CheckerError> {
    cellscript_artifact_checker::fixed_cell_fields::check_fixed_cell_scalar_fields(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

#[test]
fn fixed_cell_fields_decode_direct_unsigned_layouts_and_reject_rebound_bytes() {
    let mut coverage = Vec::new();
    let mut mutations = 0;
    for (members, predicate, widths) in [
        ("amount: u64", "require token.amount > 0", vec![8]),
        (
            "a: u8, b: u16, c: u32, d: u64",
            "require token.a == 42 require token.b == 513 require token.c == 67305985 require token.d == 123456789012345",
            vec![1, 2, 4, 8],
        ),
    ] {
        let source = SOURCE.replace("amount: u64 }", &format!("{members} }}"));
        // The unselected mint is removed because its resource initializer belongs
        // to the original amount layout, independent of field checker profiles.
        let source = source.split("action mint(").next().unwrap().to_owned()
            + &format!("action burn(input token: Token) {{ verification {predicate} consume token }}\n");
        let mut selected = declaration();
        selected.actions.retain(|action| action.action == "burn");
        selected.common_checks.clear();
        for opt in 0..=3 {
            let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, selected.clone());
            let checked = fixed_cell_fields(&fixture).unwrap();
            let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            let fields = value["fields"].as_array().unwrap();
            coverage.push(serde_json::json!({"opt":opt,"members":members,"sites":fields.len()}));
            let actual = fields.iter().map(|field| field["width"].as_u64().unwrap()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(actual, widths.iter().map(|width| *width as u64).collect());
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            for field in fields {
                let start = field["pointer_load"].as_u64().unwrap();
                let first = elf.instructions.iter().find(|instruction| instruction.address == start + 4).unwrap().word;
                let source = if first & 0x7f == 0x03 { start + 4 } else { start + 8 };
                let load = elf.instructions.iter().find(|instruction| instruction.address == source).unwrap().word;
                for (mask, bits) in [(31 << 15, 28 << 15), (0xfff << 20, (((load >> 20) + 1) & 0xfff) << 20)] {
                    let mut changed = fixture.clone();
                    changed.replace_machine_word(source, (load & !mask) | bits);
                    changed.check().unwrap();
                    fixed_cell_storage(&changed).unwrap();
                    assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                    mutations += 1;
                }
                if first & 0x7f != 0x03 {
                    let mut locations = vec![(start + 4, 0xfff << 20, 1 << 20), (start + 12, 31 << 20, 6 << 20)];
                    if field["width"].as_u64().unwrap() > 1 {
                        locations.push((start + 20, 63 << 20, 16 << 20));
                    }
                    for (address, mask, bits) in locations {
                        let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
                        let mut changed = fixture.clone();
                        changed.replace_machine_word(address, (original & !mask) | bits);
                        changed.check().unwrap();
                        fixed_cell_storage(&changed).unwrap();
                        assert_eq!(
                            fixed_cell_fields(&changed).unwrap_err().code,
                            CheckerRejectionCode::V2420TypedMachineBindingInvalid
                        );
                        mutations += 1;
                    }
                }
            }
        }
    }
    println!("{}", serde_json::json!({"field_coverage":coverage,"rebound_field_mutations":mutations}));
}

#[test]
fn fixed_cell_fields_reject_membership_helper_memory_and_register_substitutions() {
    let source = SOURCE.replace("verification consume token", "verification require token.amount > 0 consume token");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in 0..=3 {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, selected.clone());
        fixed_cell_fields(&fixture).unwrap();
        let blocks = fixture
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == "runtime:__cellscript_require_cell_membership")
            .collect::<Vec<_>>();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let syscall =
            elf.syscall_addresses.iter().copied().find(|address| blocks.iter().any(|block| block.range.contains(*address))).unwrap();
        let buffer = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < syscall
                    && instruction.word & 0x7f == 0x13
                    && ((instruction.word >> 7) & 31) == 10
                    && ((instruction.word >> 15) & 31) == 2
            })
            .unwrap();
        let store = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < buffer.address
                    && instruction.word & 0x7f == 0x23
                    && ((instruction.word >> 15) & 31) == 2
                    && (((instruction.word >> 25) << 5) | ((instruction.word >> 7) & 31)) == 0
            })
            .unwrap();
        let capacity = elf.instructions.iter().find(|instruction| instruction.address == store.address - 4).unwrap();
        for (address, word) in [
            (capacity.address, (capacity.word & !(0xfff << 20)) | (64 << 20)),
            (capacity.address, (capacity.word & !(31 << 7)) | (29 << 7)),
            (store.address, (store.word & !(31 << 15)) | (3 << 15)),
            (store.address, store.word & !(7 << 12)),
            (store.address, (store.word & !(31 << 7)) | (1 << 7)),
            (buffer.address, (buffer.word & !(31 << 15)) | (3 << 15)),
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, word);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn fixed_cell_fields_accept_rebound_native_u64_and_reject_unknown_field_profiles() {
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    let source = SOURCE.replace("verification consume token", "verification require token.amount == 7 consume token");
    for opt in 0..=3 {
        let mut fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, selected.clone());
        let bytes = fixed_cell_fields(&fixture).unwrap().canonical_bytes().unwrap();
        let record: Value = serde_json::from_slice(&bytes).unwrap();
        let field = &record["fields"][0];
        let load = field["pointer_load"].as_u64().unwrap() + 4;
        let end = field["decode_end"].as_u64().unwrap();
        let register = field["register"].as_u64().unwrap() as u32;
        // An alternate actual machine form, not a claim that this fixture's
        // compiler emitted LD. Rebind every outer identity after padding out
        // the original reconstruction, preserving addresses and its consumers.
        fixture.replace_machine_word(load, (29 << 15) | (3 << 12) | (register << 7) | 0x03);
        for address in (load + 4..end).step_by(4) {
            fixture.replace_machine_word(address, 0x00000013);
        }
        fixture.check().unwrap();
        let checked = fixed_cell_fields(&fixture).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["fields"][0]["native_load"], true);
        assert!(!checked.storage().reads().module_projection().artifact_report().semantic_equivalence_claimed);
        for replacement in
            [(8 << 20) | (29 << 15) | (3 << 12) | (register << 7) | 0x03, (29 << 15) | (4 << 12) | (register << 7) | 0x03]
        {
            let mut changed = fixture.clone();
            changed.replace_machine_word(load, replacement);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let signed = "module signed_cell_fields\nresource Token has store, consume { signed: i32 }\naction burn(input token: Token, witness zero: i32) { verification require token.signed < zero consume token }";
        let fixture = Fixture::new_source_with(signed, CellScriptEdition::Edition2026, opt, selected.clone());
        fixed_cell_storage(&fixture).unwrap();
        assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        // Nested Cell access remains outside the production executable surface;
        // do not weaken that boundary to manufacture a checker-positive bundle.
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("nested.cell");
        std::fs::write(&source, "module nested_cell_fields\nstruct Inner has copy, drop, store, fixed, serializable, non_linear { x: u64 }\nresource Token has store, consume { inner: Inner }\naction burn(input token: Token) { verification require token.inner.x > 0 consume token }").unwrap();
        let error = compile_path_with_executable_surface_policy(
            source.to_str().unwrap(),
            CompileOptions {
                source_contracts: true,
                edition: CellScriptEdition::Edition2026,
                opt_level: opt,
                target: Some("riscv64-elf".into()),
                ..CompileOptions::default()
            },
            Some(CompileEntryScope::Artifact(selected.clone())),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2105"));
    }
}

#[test]
fn fixed_cell_fields_check_inline_capacity_and_hash_observation_memory() {
    let source = SOURCE.replace("verification consume token", "verification require token.amount == 7 consume token");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in 0..=3 {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, selected.clone());
        fixed_cell_fields(&fixture).unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let site = fixture
            .record
            .syscall_sites
            .iter()
            .find(|site| {
                fixture.record.blocks.iter().any(|block| {
                    block.id == site.block_id
                        && block.owner_entry == "action:burn"
                        && elf.instructions.iter().any(|instruction| {
                            instruction.address < site.address
                                && block.range.contains(instruction.address)
                                && instruction.word == (15 << 7) | 0x13
                        })
                })
            })
            .unwrap();
        let block = fixture.record.blocks.iter().find(|block| block.id == site.block_id).unwrap();
        let definition = |register| {
            elf.instructions
                .iter()
                .rev()
                .find(|instruction| {
                    instruction.address < site.address
                        && block.range.contains(instruction.address)
                        && instruction.word & 0x7f == 0x13
                        && ((instruction.word >> 7) & 31) == register
                })
                .unwrap()
        };
        let buffer = definition(10);
        let size = definition(11);
        let field = definition(15);
        let index = definition(13);
        let capacity = elf
            .instructions
            .iter()
            .rev()
            .find(|instruction| {
                instruction.address < buffer.address
                    && block.range.contains(instruction.address)
                    && instruction.word & 0x7f == 0x13
                    && ((instruction.word >> 7) & 31) == 5
            })
            .unwrap();
        let store = elf.instructions.iter().find(|instruction| instruction.address == capacity.address + 4).unwrap();
        for (address, word) in [
            (capacity.address, (capacity.word & !(0xfff << 20)) | (64 << 20)),
            (store.address, store.word & !(7 << 12)),
            (buffer.address, (buffer.word & !(31 << 15)) | (3 << 15)),
            (size.address, (size.word & !(31 << 15)) | (3 << 15)),
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, word);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        // Independently certify alternate actual 32-byte observation writes.
        // This does not claim the original compiler emitted hash observations
        // here, or certify their downstream source/count predicate meaning.
        for hash_field in [3, 5] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(capacity.address, (capacity.word & !(0xfff << 20)) | (32 << 20));
            changed.replace_machine_word(field.address, (field.word & !(0xfff << 20)) | (hash_field << 20));
            changed.check().unwrap();
            fixed_cell_fields(&changed).unwrap();
        }
        let storage: Value = serde_json::from_slice(&fixed_cell_storage(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
        let cell_buffer = storage["cells"][0]["buffer_offset"].as_i64().unwrap();
        let delta = cell_buffer - i64::from((buffer.word as i32) >> 20);
        assert!((-2048..=2047).contains(&delta));
        for instruction in [
            (((delta as u32) & 0xfff) << 20) | (10 << 15) | (10 << 7) | 0x1b,
            (29 << 20) | (10 << 15) | (10 << 7) | 0x3b,
            (0x20 << 25) | (29 << 20) | (10 << 15) | (10 << 7) | 0x3b,
        ] {
            let mut changed = fixture.clone();
            changed.replace_machine_word(index.address, instruction);
            changed.check().unwrap();
            fixed_cell_storage(&changed).unwrap();
            assert_eq!(fixed_cell_fields(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let number = elf.instructions.iter().find(|instruction| instruction.address == site.address - 4).unwrap();
        assert_eq!(number.word & 0x7f, 0x13);
        let mut changed = fixture.clone();
        changed.replace_machine_word(number.address, (number.word & !0x7f) | 0x1b);
        changed.check().unwrap();
        fixed_cell_storage(&changed).unwrap();
        fixed_cell_fields(&changed).unwrap();
    }
}

#[test]
fn fixed_cell_fields_reject_unproved_owner_syscalls_after_native_materialization() {
    let source = SOURCE.replace("verification consume token", "verification require token.amount == 7 consume token");
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "burn");
    selected.common_checks.clear();
    for opt in 0..=3 {
        let mut fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, selected.clone());
        let checked = fixed_cell_fields(&fixture).unwrap();
        let fields: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let storage: Value = serde_json::from_slice(&checked.storage().canonical_bytes().unwrap()).unwrap();
        let load = fields["fields"][0]["pointer_load"].as_u64().unwrap() + 4;
        let end = fields["fields"][0]["decode_end"].as_u64().unwrap();
        let register = fields["fields"][0]["register"].as_u64().unwrap() as u32;
        let size = storage["cells"][0]["size_offset"].as_u64().unwrap() as u32;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let helper = fixture
            .record
            .blocks
            .iter()
            .filter(|block| block.owner_entry == "runtime:__cellscript_require_cell_membership")
            .collect::<Vec<_>>();
        let syscall =
            elf.syscall_addresses.iter().copied().find(|address| helper.iter().any(|block| block.range.contains(*address))).unwrap();
        let word = |address| elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
        fixture.replace_machine_word(load, (29 << 15) | (3 << 12) | (register << 7) | 0x03);
        for address in (load + 4..end).step_by(4) {
            fixture.replace_machine_word(address, 0x00000013);
        }
        for (index, instruction) in [
            (29 << 15) | (10 << 7) | 0x13,
            (size << 20) | (2 << 15) | (11 << 7) | 0x13,
            word(syscall - 8),
            word(syscall - 4),
            0x00000073,
        ]
        .into_iter()
        .enumerate()
        {
            fixture.replace_machine_word(load + 4 + 4 * index as u64, instruction);
        }
        let mut site = fixture.record.syscall_sites.iter().find(|site| site.address == syscall).unwrap().clone();
        site.address = load + 20;
        site.block_id = fixture.record.blocks.iter().find(|block| block.range.contains(site.address)).unwrap().id.clone();
        fixture.record.syscall_sites.push(site);
        fixture.record.syscall_sites.sort_by_key(|site| site.address);
        fixture.rebind_sidecars();
        fixture.check().unwrap();
        fixed_cell_storage(&fixture).unwrap();
        assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_fields_enforce_256_sites_without_raising_variant_or_read_limits() {
    for extra in [false, true] {
        let mut source =
            "module field_site_bounds\nresource Token has store, consume { a: u8, b: u16, c: u32, d: u64, e: u8 }\n".to_owned();
        let mut selected = declaration();
        selected.actions.clear();
        selected.common_checks.clear();
        for index in 0..64 {
            let name = format!("burn{index}");
            selected.actions.push(ArtifactAction { tag: index, action: name.clone() });
            let tail = if extra && index == 0 { "require token.e == 17" } else { "" };
            source.push_str(&format!("action {name}(input token: Token) {{ verification require token.a == 42 require token.b == 513 require token.c == 67305985 require token.d == 123456789012345 {tail} consume token }}\n"));
        }
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, 0, selected);
        fixed_cell_storage(&fixture).unwrap();
        if extra {
            assert_eq!(fixed_cell_fields(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        } else {
            let record: Value = serde_json::from_slice(&fixed_cell_fields(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["fields"].as_array().unwrap().len(), 256);
        }
    }
}

#[test]
fn fixed_cell_parameter_storage_binds_source_ids_and_actual_pointer_reception() {
    for opt in 0..=3 {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, declaration());
        let checked = fixed_cell_storage(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(value["cells"].as_array().unwrap().len(), 1);
        assert_eq!(value["cells"][0]["parameter"], "token");
        assert_eq!(value["cells"][0]["pointer_offset"], value["cells"][0]["source_id"].as_u64().unwrap() * 8);
        assert_eq!(value["read_gates"], checked.reads().identity());
        assert_eq!(value["parameter_decoder"], checked.parameters().identity());
        let cell = &value["cells"][0];
        let receiver = cell["receiver_start"].as_u64().unwrap();
        let spill = cell["spill_address"].as_u64().unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, mask, bits) in [
            (receiver, 31 << 15, 3 << 15),
            (receiver, 31 << 7, 6 << 7),
            (receiver + 4, 31 << 15, 3 << 15),
            (receiver + 4, 31 << 20, 6 << 20),
            (spill, 31 << 20, 11 << 20),
        ] {
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, (original & !mask) | bits);
            changed.check().unwrap();
            fixed_cell_reads(&changed).expect("read gates alone do not attest parameter storage");
            assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
        let mut changed = fixture.clone();
        let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.id == "action:burn").unwrap();
        let local_id = entry.cell_bindings[0].local_id.unwrap();
        entry.locals.iter_mut().find(|local| local.id == local_id).unwrap().source_id += 1;
        changed.rebind_policy_identity();
        changed.check().unwrap();
        fixed_cell_reads(&changed).unwrap();
        assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_parameter_storage_rejects_overlapping_argument_spills_and_output_only() {
    let source = SOURCE.replace(
        "action burn(input token: Token) { verification consume token }",
        "action burn(input token: Token, witness n: u64) { verification require n > 0 consume token }",
    );
    for opt in 0..=3 {
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, declaration());
        let checked = fixed_cell_storage(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let spill = value["cells"][0]["spill_address"].as_u64().unwrap();
        let pointer = value["cells"][0]["pointer_offset"].as_u64().unwrap() as u32;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let second = elf.instructions.iter().find(|instruction| instruction.address == spill + 4).unwrap().word;
        assert_eq!((second >> 20) & 31, 11);
        let mut changed = fixture.clone();
        changed
            .replace_machine_word(spill + 4, (second & !((0x7f << 25) | (31 << 7))) | ((pointer >> 5) << 25) | ((pointer & 31) << 7));
        changed.check().unwrap();
        fixed_cell_reads(&changed).unwrap();
        assert_eq!(fixed_cell_storage(&changed).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);

        let mut output = declaration();
        output.actions.retain(|action| action.action == "mint");
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, output);
        fixed_cell_reads(&fixture).unwrap();
        assert_eq!(fixed_cell_storage(&fixture).unwrap_err().code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
    }
}

#[test]
fn fixed_cell_reads_check_actual_source_size_and_error_gates() {
    for opt in 0..=3 {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, declaration());
        let checked = fixed_cell_reads(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert!(!value["reads"].as_array().unwrap().is_empty());
        assert_eq!(value["reads"][0]["width_bytes"], 8);
        assert_eq!(value["reads"][0]["capacity_bytes"], 512);
        assert_eq!(
            value["bundle_hashes"][0],
            cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&fixture.artifact))
        );
        assert!(!checked.module_projection().artifact_report().semantic_equivalence_claimed);
    }
}

#[test]
fn fixed_cell_reads_reject_rebound_machine_setup_and_length_mutations() {
    for opt in 0..=3 {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, declaration());
        let value: Value = serde_json::from_slice(&fixed_cell_reads(&fixture).unwrap().canonical_bytes().unwrap()).unwrap();
        let read = &value["reads"][0];
        let start = read["setup_start"].as_u64().unwrap();
        let syscall = read["syscall_address"].as_u64().unwrap();
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, mask, bits) in [
            (start, 0xfff0_0000, 513 << 20),                                          // capacity
            (start + 4, 31 << 15, 3 << 15),                                           // size initializer base
            (start + 8, 31 << 15, 3 << 15),                                           // buffer base
            (start + 12, 31 << 15, 3 << 15),                                          // length pointer base
            (start + 16, 0xfff0_0000, 1 << 20),                                       // nonzero data byte offset
            (start + 20, 0xfff0_0000, 1 << 20),                                       // other output ordinal
            (syscall - 12, 0xfff0_0000, 3 << 20),                                     // other Cell source
            (syscall - 4, 0xfff0_0000, ((2093u32.wrapping_sub(4096)) & 0xfff) << 20), // another syscall
            (syscall + 4, 7 << 12, 1 << 12),                                          // inverted status test
            (syscall + 12, 31 << 15, 3 << 15),                                        // length guard reads another pointer
            (syscall + 16, 0xfff0_0000, 9 << 20),                                     // wrong exact length
            (syscall + 20, 31 << 20, 12 << 20),                                       // comparison with wrong register
            (syscall + 24, 7 << 12, 1 << 12),                                         // inverted length test
        ] {
            let original = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let mut changed = fixture.clone();
            changed.replace_machine_word(address, (original & !mask) | bits);
            changed.check().expect("ordinary inspection does not certify these Cell read arguments");
            let error = fixed_cell_reads(&changed).unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid);
        }
    }
}

#[test]
fn fixed_cell_reads_reject_missing_source_contracts_and_noncanonical_boolean_layouts() {
    let source = "module bool_read\nresource Token has store, consume { amount: bool }\naction mint(witness recipient: Address) { verification create Token { amount: true } with_lock(recipient) }\n";
    let mut selected = declaration();
    selected.actions.retain(|action| action.action == "mint");
    selected.common_checks.clear();
    let fixture = Fixture::new_source_with(source, CellScriptEdition::Edition2026, 0, selected);
    assert!(fixed_cell_reads(&fixture).unwrap_err().message.contains("boolean canonicality"));
    let mut missing = Fixture::new(CellScriptEdition::Edition2026);
    missing.record.typed_semantics.nominal_declarations = None;
    missing.rebind_policy_identity();
    assert!(fixed_cell_reads(&missing).is_err());
}

#[test]
fn fixed_cell_reads_have_real_read_count_and_instruction_budget_limits() {
    for count in [64, 65] {
        let mut source = "module read_count\nresource Token has store, consume { amount: u64 }\n".to_string();
        for index in 0..64 {
            let input = if count == 65 && index == 0 { "input before: Token, " } else { "" };
            let consume = if input.is_empty() { "" } else { "require before.amount == 7\nconsume before\n" };
            source.push_str(&format!("action read_{index}({input}witness recipient: Address) {{ verification\n{consume}create Token {{ amount: 7 }} with_lock(recipient) }}\n"));
        }
        let selected = ArtifactDeclaration {
            name: "ReadCount".into(),
            context: ArtifactContext::TypeGroup { resource: "Token".into() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: (0..64).map(|index| ArtifactAction { tag: index, action: format!("read_{index}") }).collect(),
            common_checks: Vec::new(),
        };
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, 3, selected);
        if count == 64 {
            let checked = fixed_cell_reads(&fixture).unwrap();
            let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(value["reads"].as_array().unwrap().len(), 64);
            let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
            for read in value["reads"].as_array().unwrap() {
                assert!(elf.syscall_addresses.contains(&read["syscall_address"].as_u64().unwrap()));
            }
        } else {
            assert!(fixed_cell_reads(&fixture).unwrap_err().message.contains("more than 64"));
        }
    }
    let fixture = Fixture::new(CellScriptEdition::Edition2026);
    let error = cellscript_artifact_checker::fixed_cell_reads::check_fixed_cell_reads(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets { instructions: 1, ..Default::default() },
    )
    .unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
}

#[test]
fn fixed_policy_parameter_decoders_bind_actual_offsets_and_source_contracts() {
    let mut identity = None;
    for opt in 0..=3 {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, declaration());
        let checked = cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(value["variants"]["10"]["action"], "mint");
        assert_eq!(value["variants"]["10"]["parameters"][0]["width_bytes"], 8);
        assert_eq!(value["variants"]["10"]["parameters"][1]["payload_offset"], 8);
        assert_eq!(value["variants"]["10"]["parameters"][1]["transport"], "fixed-byte-pointer");
        assert_eq!(value["variants"]["40"]["parameters"][0]["transport"], "runtime-bound-null");
        assert!(!checked.module_projection().artifact_report().semantic_equivalence_claimed);
        if let Some(previous) = &identity {
            assert_eq!(previous, checked.identity())
        } else {
            identity = Some(checked.identity().to_owned())
        }
    }
}

#[test]
fn fixed_decoder_rejects_rebound_parameter_instructions_that_ordinary_inspection_does_not_certify() {
    for opt in 0..=3 {
        let fixture = Fixture::new_with(CellScriptEdition::Edition2026, opt, declaration());
        let params = fixture
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_exact_size_ok_")))
            .unwrap()
            .range
            .start;
        let size_ok = fixture
            .record
            .blocks
            .iter()
            .find(|block| block.machine_label.as_deref().is_some_and(|label| label.starts_with(".Lentry_witness_size_ok_")))
            .unwrap()
            .range
            .start;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        for (address, kind) in [(params + 4, 0), (params + 8, 1), (params + 16, 2), (size_ok + 4, 3)] {
            let mut changed = fixture.clone();
            let word = elf.instructions.iter().find(|instruction| instruction.address == address).unwrap().word;
            let modified = match kind {
                0 => (word & 0x000f_ffff) | (17 << 20), // duplicate next byte instead of byte zero
                1 => (word & !(31 << 7)) | (11 << 7),   // overwrite the following ABI argument
                2 => (word & 0x000f_ffff) | (16 << 20), // shift second byte by sixteen instead of eight
                _ => (word & 0x000f_ffff) | (68 << 20), // accept a different witness magic
            };
            changed.replace_machine_word(address, modified);
            changed.check().expect("ordinary inspection does not grant a decoder certificate");
            let error = cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
                &changed.artifact,
                &serde_json::to_vec(&changed.metadata).unwrap(),
                &serde_json::to_vec(&changed.record).unwrap(),
                &serde_json::to_vec(&changed.source_map).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2420TypedMachineBindingInvalid, "O{opt} mutation {kind}: {error}");
        }
    }
}

#[test]
fn fixed_decoder_evidence_never_downgrades_unsupported_codec_or_stack_profiles() {
    for source in [
        SOURCE.replace("witness recipient: Address)", "witness recipient: Address, witness flag: bool)"),
        STACK_ARGS_SOURCE.to_owned(),
    ] {
        let mut artifact = declaration();
        artifact.actions.retain(|action| action.action == "mint");
        artifact.common_checks.clear();
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, 0, artifact);
        assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
            &fixture.artifact,
            &serde_json::to_vec(&fixture.metadata).unwrap(),
            &serde_json::to_vec(&fixture.record).unwrap(),
            &serde_json::to_vec(&fixture.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .is_err());
    }
}

#[test]
fn fixed_decoder_checks_signed_extension_small_aggregates_and_shared_variants() {
    let source = SOURCE.replace("witness amount: u64, witness recipient: Address", "witness amount: u64, witness recipient: Address, witness signed: i32, witness bytes: [u8; 4], witness pair: (u16, u8)")
        + "\naction other(witness amount: u64, witness recipient: Address, witness signed: i32, witness bytes: [u8; 4], witness pair: (u16, u8)) { verification require amount > 0 create Token { amount: amount } with_lock(recipient) }\n";
    for opt in 0..=3 {
        let mut declaration = declaration();
        declaration.actions.push(ArtifactAction { tag: 11, action: "other".into() });
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, opt, declaration);
        let checked = certify_fixed_decoder(&fixture).unwrap();
        let value: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        let params = &value["variants"]["10"]["parameters"];
        assert_eq!(params[2]["width_bytes"], 4);
        assert_eq!(params[3]["width_bytes"], 4);
        assert_eq!(params[4]["width_bytes"], 3);
        assert_eq!(value["variants"]["11"]["action"], "other");
        assert!(fixture.record.entries.iter().any(|entry| entry.name.starts_with(".Lpolicy_shared_decoder_")));
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let signed = elf
            .instructions
            .iter()
            .find(|instruction| {
                let word = instruction.word;
                word & 0x707f == 0x5013 && word >> 20 == 0x420 && (word >> 7) & 31 == 13
            })
            .expect("signed a3 must be extended from 32 bits");
        let mut changed = fixture.clone();
        changed.replace_machine_word(signed.address, signed.word & !(0x400 << 20));
        changed.check().unwrap();
        assert!(certify_fixed_decoder(&changed).is_err(), "unsigned substitute cannot certify i32 at O{opt}");
    }
}

#[test]
fn fixed_decoder_profile_limits_are_exact_and_never_raise_checker_budgets() {
    let mut single = declaration();
    single.actions.retain(|action| action.action == "mint");
    single.common_checks.clear();
    for (padding, accepted) in [(1496, true), (1497, false)] {
        let source =
            SOURCE.replace("witness recipient: Address)", &format!("witness recipient: Address, witness padding: [u8; {padding}])"));
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, 0, single.clone());
        assert_eq!(certify_fixed_decoder(&fixture).is_ok(), accepted, "padding={padding}");
    }
    for (extra, accepted) in [(5, true), (6, false)] {
        let parameters = (0..extra).map(|index| format!(", witness p{index}: u8")).collect::<String>();
        let source = SOURCE.replace("witness recipient: Address)", &format!("witness recipient: Address{parameters})"));
        let fixture = Fixture::new_source_with(&source, CellScriptEdition::Edition2026, 0, single.clone());
        assert_eq!(certify_fixed_decoder(&fixture).is_ok(), accepted, "extra={extra}");
    }
    let fixture = Fixture::new_with(CellScriptEdition::Edition2026, 0, declaration());
    let budgets = CheckerBudgets { instructions: 1, ..CheckerBudgets::default() };
    assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &budgets,
    )
    .is_err());
    assert!(cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &vec![b' '; 4 * 1024 * 1024 + 1],
        b"{}",
        b"{}",
        &CheckerBudgets::default(),
    )
    .is_err());
}

fn certify_fixed_decoder(
    fixture: &Fixture,
) -> Result<cellscript_artifact_checker::entry_codec::CheckedFixedPolicyParameterDecoders, CheckerError> {
    cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders(
        &fixture.artifact,
        &serde_json::to_vec(&fixture.metadata).unwrap(),
        &serde_json::to_vec(&fixture.record).unwrap(),
        &serde_json::to_vec(&fixture.source_map).unwrap(),
        &CheckerBudgets::default(),
    )
}

impl Fixture {
    fn new(edition: CellScriptEdition) -> Self {
        Self::new_with(edition, 0, declaration())
    }

    fn new_with(edition: CellScriptEdition, opt_level: u8, declaration: ArtifactDeclaration) -> Self {
        Self::new_source_with(SOURCE, edition, opt_level, declaration)
    }

    fn new_source_with(source_text: &str, edition: CellScriptEdition, opt_level: u8, declaration: ArtifactDeclaration) -> Self {
        Self::new_source_with_target(source_text, edition, opt_level, declaration, "ckb")
    }

    fn new_source_with_target(
        source_text: &str,
        edition: CellScriptEdition,
        opt_level: u8,
        declaration: ArtifactDeclaration,
        target: &str,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("main.cell");
        std::fs::write(&source, source_text).unwrap();
        let result = compile_path_with_executable_surface_policy(
            source.to_str().unwrap(),
            CompileOptions {
                source_contracts: true,
                target_profile: Some(target.into()),
                edition,
                opt_level,
                target: Some("riscv64-elf".to_string()),
                ..CompileOptions::default()
            },
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
fn distinct_zero_width_constructor_fields_reject_swapped_nominal_operands_after_rebinding() {
    let source = r#"
module zero_width_constructor
struct EmptyZ {}
struct EmptyA {}
struct Payload { z_empty: EmptyZ, a_empty: EmptyA, tail: u64 }
fn build() -> Payload { Payload { tail: 37, z_empty: EmptyZ {}, a_empty: EmptyA {} } }
action verify() {
    verification
    let value: Payload = build()
    require value.tail == 37
}

"#;
    for opt_level in 0..=3 {
        // This prototype belongs to the single-entry ABI. Keep policy-witness
        // helper-call admission separate from its typed constructor contract.
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { source_contracts: true, opt_level, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
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
        // Operations do not change the entry-selection or declared-layout
        // identities. Preserve those and rebind only the mutated typed record
        // and outer sidecars, including their bundle identity.
        changed.record.typed_semantics_hash = canonical_hash(TYPED_SEMANTICS_SCHEMA, &changed.record.typed_semantics).unwrap();
        changed.metadata["typed_semantics"] = serde_json::to_value(&changed.record.typed_semantics).unwrap();
        changed.metadata["typed_semantics_hash"] = changed.record.typed_semantics_hash.clone().into();
        changed.rebind_sidecars();
        let error = changed.check().unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "O{opt_level}: {error}");
        assert!(error.message.contains("'tuple'"), "O{opt_level}: {error}");
    }
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
        CompileOptions { source_contracts: true, opt_level: 0, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
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
fn uninstantiated_generic_catalog_rejects_rebound_shape_and_bound_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!("{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}\npublic enum Choice<T: fixed_value> {{ First(T), Empty }}\npublic fn first<T: fixed_value>(value: T) -> T {{ value }}");
    for opt_level in 0..=3 {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt_level, declaration());
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
                "opt={opt_level} {mutation}: {error}"
            );
            assert!(error.message.contains("generic") || error.message.contains("nominal"), "opt={opt_level} {mutation}: {error}");
        }
    }
}

#[test]
fn private_absent_template_declarations_reject_rebound_universal_mutations() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!(
        "{SOURCE}\nprivate struct Sealed<T: fixed_value> has copy, drop, store, fixed, serializable, non_linear {{ inner: T }}\nprivate fn select<T: fixed_value>(value: T) -> T {{ value }}"
    );
    for opt_level in 0..=3 {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt_level, declaration());
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
        // lowered entry exercises them at any optimization level.
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
                assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "opt={opt_level} {mutation}: {error}");
                continue;
            }
            let error = inspected.unwrap().validate_symbolic_declarations().unwrap_err();
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "opt={opt_level} {mutation}: {error}");
            assert!(error.message.contains("universal"), "opt={opt_level} {mutation}: {error}");
        }
    }
}

#[test]
fn nominal_catalog_rejects_rebound_scopes_shapes_and_public_omissions() {
    let source = format!("{SOURCE}\npublic struct Snapshot {{ z: u64, a: Hash }}\npublic enum Event {{ First(u64), Second(Hash) }}\npublic fn view(value: Snapshot) -> u64 {{ value.z }}");
    for opt_level in 0..=3 {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt_level, declaration());
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
                    let canonical = cellscript::package::registry::canonical_json_value(&layout);
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
                let canonical = cellscript::package::registry::canonical_json_value(&serde_json::to_value(builder).unwrap());
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
                "O{opt_level} {mutation}: {error}"
            );
            if mutation.starts_with("public-") && mutation != "public-callable-duplicate" {
                assert!(error.message.contains("public declaration set"), "O{opt_level} {mutation}: {error}");
            }
        }
    }
}

#[test]
fn universal_abilities_reject_weakened_parameter_minima_without_any_concrete_instance() {
    use cellscript_artifact_checker::TypedSemanticGenericDeclaration as Declaration;
    let source = format!("{SOURCE}\npublic struct Pair<T: fixed_value> {{ left: T, right: T }}");
    for opt_level in 0..=3 {
        let baseline = Fixture::new_source_with(&source, CellScriptEdition::Edition2027, opt_level, declaration());
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
                "O{opt_level} {missing}: {error}"
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
        assert!(error.message.contains("parameter minima"), "O{opt_level}: {error}");

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
        let canonical = cellscript::package::registry::canonical_json_value(&layout);
        pair["layout_identity"] = cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(
            &serde_json::to_vec(&canonical).unwrap(),
        ))
        .into();
        recursive.rebind_interface_identity();
        let error = inspect(&recursive).validate_symbolic_declarations().unwrap_err();
        assert!(error.message.contains("recursive"), "O{opt_level}: {error}");
    }
}

#[test]
fn imported_nominal_layouts_reject_same_width_owner_and_alias_substitution() {
    use cellscript::{artifact::compile_sources_artifact, InMemorySource};
    let source = SOURCE.replace(
        "action burn(input token: Token) { verification consume token }",
        "action burn(input token: Token, witness values: Envelope) { verification consume token }",
    );
    let source = format!("{source}\nuse left_owner::Value as Left\nuse right_owner::Value as Right\npublic struct Envelope {{ left: Left, right: Right }}");
    let sources = [
        InMemorySource { path: "main.cell".into(), source, role: None },
        InMemorySource {
            path: "left.cell".into(),
            source: "module left_owner\npublic struct Value { value: u64 }".into(),
            role: None,
        },
        InMemorySource {
            path: "right.cell".into(),
            source: "module right_owner\npublic struct Value { value: u64 }".into(),
            role: None,
        },
    ];
    for opt_level in 0..=3 {
        let compiled = compile_sources_artifact(
            &sources,
            "main.cell",
            CompileOptions {
                source_contracts: true,
                edition: CellScriptEdition::Edition2027,
                opt_level,
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
            assert_eq!(error.code, CheckerRejectionCode::V2419TypedSemanticsInvalid, "O{opt_level} {mutation}: {error}");
            assert!(error.message.contains("nominal"), "O{opt_level} {mutation}: {error}");
        }
    }
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

fn fixed_result_fixture(opt_level: u8) -> Fixture {
    let source = r#"
module checked_result
struct Pair { left: u64, right: u64 }
fn build(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, value: Pair) -> Pair {
    Pair { left: value.right + a + b + c + d, right: value.left + e + f + g + h }
}
action verify(witness value: Pair, witness expected: Pair) {
    verification
    let result = build(1, 2, 3, 4, 5, 6, 7, 8, value)
    require result.left == expected.left
    require result.right == expected.right
}
"#;
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions { source_contracts: true, opt_level, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let fixture = Fixture {
        artifact: compiled.artifact_bytes,
        metadata: serde_json::to_value(compiled.metadata).unwrap(),
        record: compiled.verified_lowering_record.unwrap(),
        source_map: compiled.source_artifact_map.unwrap(),
    };
    fixture.check().unwrap();
    fixture
}

#[test]
fn fixed_result_contracts_reject_rebound_placement_extent_and_coverage_mutations() {
    for opt_level in 0..=3 {
        let valid = fixed_result_fixture(opt_level);
        for mutation in 0..19 {
            let mut changed = valid.clone();
            let helper = changed.record.entries.iter().position(|entry| entry.name == "build").unwrap();
            let caller = changed.record.entries.iter().position(|entry| entry.name == "verify").unwrap();
            match mutation {
                0 => changed.record.entries[helper].fixed_result_abi = None,
                1 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().hidden_argument_index -= 1,
                2 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().width_bytes -= 1,
                3 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().saved_pointer_offset += 8,
                4 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().type_name = "u64".into(),
                5 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_ranges.clear(),
                6 => changed.record.entries[caller].fixed_result_calls.clear(),
                7 => changed.record.entries[caller].fixed_result_calls[0].hidden_argument_index += 1,
                8 => changed.record.entries[caller].fixed_result_calls[0].outgoing_stack_bytes += 16,
                9 => changed.record.entries[caller].fixed_result_calls[0].buffer_offset = 0,
                10 => {
                    changed.record.entries[caller].fixed_result_calls[0].buffer_offset =
                        changed.record.entries[caller].frame_size_bytes
                }
                11 => changed.record.entries[caller].fixed_result_calls[0].buffers[0].width_bytes = u32::MAX,
                12 => changed.record.entries[caller].fixed_result_calls[0].receive_range.end = u64::MAX - 3,
                13 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().schema = "unknown-result-abi".into(),
                14 => {
                    changed.record.entries[caller].fixed_result_calls[0].pointer_slot_offset =
                        changed.record.entries[caller].fixed_result_calls[0].buffer_offset
                }
                15 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources.clear(),
                16 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].base_local = u64::MAX,
                17 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].field_offset = 8,
                18 => changed.record.entries[helper].fixed_result_abi.as_mut().unwrap().copy_sources[0].source_local = u64::MAX,
                _ => unreachable!(),
            }
            changed.rebind_sidecars();
            let error = changed.check().expect_err("rebound fixed result contract must fail independently");
            assert_eq!(error.code, CheckerRejectionCode::V2407AbiOrStackInvalid, "O{opt_level}, mutation {mutation}: {error}");
            assert!(error.message.contains("fixed struct result ABI"), "{error}");
        }
    }
}

#[test]
fn fixed_result_machine_mutations_reject_after_elf_and_sidecars_are_rebound() {
    for opt_level in 0..=3 {
        let valid = fixed_result_fixture(opt_level);
        let helper = valid.record.entries.iter().find(|entry| entry.name == "build").unwrap();
        let abi = helper.fixed_result_abi.as_ref().unwrap();
        let call = &valid.record.entries.iter().find(|entry| entry.name == "verify").unwrap().fixed_result_calls[0];
        let elf = parse_elf(&valid.artifact, CheckerBudgets::default().instructions).unwrap();
        let word_in = |range: cellscript_artifact_checker::MachineRange, opcode: u32, rd: Option<u32>| {
            *elf.instructions
                .iter()
                .find(|instruction| {
                    range.contains(instruction.address)
                        && instruction.word & 0x7f == opcode
                        && rd.is_none_or(|rd| (instruction.word >> 7) & 31 == rd)
                })
                .unwrap()
        };
        let save_load = word_in(abi.save_range, 0x03, Some(5));
        let save_store = word_in(abi.save_range, 0x23, None);
        let copy_load = word_in(abi.copy_ranges[0], 0x03, Some(11));
        let source_load = word_in(abi.copy_sources[0].range, 0x03, Some(10));
        let copy_length = word_in(abi.copy_ranges[0], 0x13, Some(12));
        let outgoing_store = word_in(call.setup_range, 0x23, None);
        let receive_pointer = word_in(call.receive_range, 0x13, Some(5));
        let cases = [
            (source_load.address, source_load.word ^ (8 << 20)),
            (source_load.address, (source_load.word & !(31 << 7)) | (11 << 7)),
            (save_load.address, save_load.word ^ (8 << 20)),
            (save_store.address, save_store.word ^ (8 << 7)),
            (copy_load.address, copy_load.word ^ (8 << 20)),
            (copy_load.address, (copy_load.word & !(31 << 7)) | (10 << 7)),
            (copy_length.address, copy_length.word ^ (1 << 20)),
            (outgoing_store.address, outgoing_store.word ^ (8 << 7)),
            (receive_pointer.address, (receive_pointer.word & !(31 << 15)) | (10 << 15)),
        ];
        for (address, word) in cases {
            let mut changed = valid.clone();
            changed.replace_machine_word(address, word);
            let error = changed.check().expect_err("machine ABI mutation must fail after complete hash rebinding");
            assert_eq!(error.code, CheckerRejectionCode::V2407AbiOrStackInvalid, "O{opt_level}, address {address:#x}: {error}");
            assert!(error.message.contains("fixed struct result ABI"), "{error}");
        }
    }
}

#[derive(Clone)]
struct DependencySnapshot {
    out_point: [u8; 36],
    output: Vec<u8>,
    data: Vec<u8>,
}
fn dependency_inputs(snapshots: &[DependencySnapshot]) -> Vec<cellscript_artifact_checker::code_origin::SuppliedDependencyCell<'_>> {
    snapshots
        .iter()
        .map(|cell| cellscript_artifact_checker::code_origin::SuppliedDependencyCell {
            out_point: cell.out_point,
            output: &cell.output,
            data: &cell.data,
        })
        .collect()
}
fn direct_dependency_fixture(
    opt: u8,
) -> (cellscript_artifact_checker::code_origin::CheckedTargetCodeCellOrigin, Vec<u8>, Vec<DependencySnapshot>) {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(EXTERNAL_SOURCE, opt);
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let code = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock.clone()).build();
    let selected = lock
        .clone()
        .as_builder()
        .code_hash(packed::CellOutput::calc_data_hash(&fixture.artifact))
        .hash_type(4u8)
        .args(Bytes::from(vec![17]).pack())
        .build();
    let creation =
        TransactionBuilder::default().output(code.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack()).build();
    let origin = code_origin(&fixture, creation.data().raw().as_slice(), 0, selected.as_slice()).unwrap();
    let target = cellscript_artifact_checker::code_origin::check_code_cell_target(origin).unwrap();
    let point = packed::OutPoint::new_builder().tx_hash(creation.hash()).index(0u32).build();
    let other_point = packed::OutPoint::new_builder().tx_hash([12u8; 32].pack()).index(7u32).build();
    let raw = TransactionBuilder::default()
        .cell_dep(packed::CellDep::new_builder().out_point(point.clone()).dep_type(0u8).build())
        .cell_dep(packed::CellDep::new_builder().out_point(other_point.clone()).dep_type(0u8).build())
        .output(packed::CellOutput::new_builder().capacity(200000000000u64).lock(lock.clone()).build())
        .output_data(Bytes::new().pack())
        .build()
        .data()
        .raw()
        .as_slice()
        .to_vec();
    // Supply another order deliberately; this is not an assertion about VM
    // resolution order or syscall indices.
    let cells = vec![
        DependencySnapshot {
            out_point: other_point.as_slice().try_into().unwrap(),
            output: packed::CellOutput::new_builder().capacity(100000000000u64).lock(lock).build().as_slice().to_vec(),
            data: vec![1, 2, 3],
        },
        DependencySnapshot {
            out_point: point.as_slice().try_into().unwrap(),
            output: code.as_slice().to_vec(),
            data: fixture.artifact,
        },
    ];
    (target, raw, cells)
}
#[test]
fn direct_code_dependency_binds_actual_outpoints_outputs_and_independent_orders() {
    use cellscript_artifact_checker::code_origin::check_direct_code_dependency;
    use ckb_testtool::ckb_types::{packed, prelude::*};
    for opt in 0..=3 {
        let (target, raw, cells) = direct_dependency_fixture(opt);
        let inputs = dependency_inputs(&cells);
        let proof = check_direct_code_dependency(&target, &raw, &inputs, &CheckerBudgets::default()).unwrap();
        assert_eq!(proof.raw_dependency_index(), 0);
        assert_eq!(proof.supplied_cell_index(), 1);
        let sdk = packed::RawTransaction::from_slice(&raw).unwrap();
        assert_eq!(proof.transaction_hash(), code_origin_hex(sdk.calc_tx_hash().as_slice()));
        let bytes = proof.canonical_bytes().unwrap();
        let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(record["target_origin"], target.identity());
        assert_eq!(record["supplied_cells"][1]["out_point"], code_origin_hex(&cells[1].out_point));
        assert_eq!(record["supplied_cells"][1]["data_hash"], target.origin().artifact_hash());
        for key in ["consensus_claimed", "vm_execution_claimed", "authorization_claimed"] {
            assert_eq!(record[key], false);
        }
        let mut material = b"cellscript-direct-code-dependency-id-v1\0".to_vec();
        material.extend_from_slice(&bytes);
        assert_eq!(proof.identity(), cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&material)));
        proof.check_unchanged_inputs(&raw, &inputs, &CheckerBudgets::default()).unwrap();
        let mut reordered = cells.clone();
        reordered.swap(0, 1);
        let next = check_direct_code_dependency(&target, &raw, &dependency_inputs(&reordered), &CheckerBudgets::default()).unwrap();
        assert_eq!(next.raw_dependency_index(), 0);
        assert_eq!(next.supplied_cell_index(), 0);
        assert_ne!(proof.identity(), next.identity());
        assert!(proof.check_unchanged_inputs(&raw, &dependency_inputs(&reordered), &CheckerBudgets::default()).is_err());
        let mut changed_raw = raw.clone();
        *changed_raw.last_mut().unwrap() ^= 1;
        assert!(proof.check_unchanged_inputs(&changed_raw, &inputs, &CheckerBudgets::default()).is_err());
    }
}
#[test]
fn direct_code_dependency_rejects_missing_copied_duplicate_and_unselected_cells() {
    use cellscript_artifact_checker::code_origin::check_direct_code_dependency;
    use ckb_testtool::ckb_types::{packed, prelude::*};
    for opt in 0..=3 {
        let (target, raw, cells) = direct_dependency_fixture(opt);
        for axis in 0..12 {
            let mut changed = cells.clone();
            let mut tx = packed::RawTransaction::from_slice(&raw).unwrap();
            match axis {
                0 => {
                    changed.pop();
                }
                1 => changed[1].out_point[0] ^= 1,
                2 => changed[0].out_point = changed[1].out_point,
                3 => changed[0].output = vec![0],
                4 => changed[0].data = changed[1].data.clone(),
                5 => *changed[1].data.last_mut().unwrap() ^= 1,
                6 => {
                    let output = packed::CellOutput::from_slice(&changed[1].output).unwrap();
                    changed[1].output = output.as_builder().capacity(1u64).build().as_slice().to_vec();
                }
                7 => {
                    let output = packed::CellOutput::from_slice(&changed[1].output).unwrap();
                    let lock = output.lock().as_builder().args([1u8].pack()).build();
                    changed[1].output = output.as_builder().lock(lock).build().as_slice().to_vec();
                }
                8 => {
                    let mut deps: Vec<_> = tx.cell_deps().into_iter().collect();
                    deps[0] = deps[0].clone().as_builder().dep_type(1u8).build();
                    tx = tx.as_builder().cell_deps(deps.pack()).build();
                }
                9 => {
                    let deps: Vec<_> = tx.cell_deps().into_iter().collect();
                    tx = tx.as_builder().cell_deps(vec![deps[0].clone(), deps[0].clone()].pack()).build();
                }
                10 => {
                    let deps: Vec<_> = tx.cell_deps().into_iter().collect();
                    let wrong = deps[0].out_point().as_builder().tx_hash([0x34u8; 32].pack()).build();
                    changed[1].out_point = wrong.as_slice().try_into().unwrap();
                    tx = tx
                        .as_builder()
                        .cell_deps(vec![deps[0].clone().as_builder().out_point(wrong).build(), deps[1].clone()].pack())
                        .build();
                }
                11 => {
                    let output = packed::CellOutput::from_slice(&changed[1].output).unwrap();
                    let code_type = output.lock();
                    changed[1].output = output.as_builder().type_(Some(code_type).pack()).build().as_slice().to_vec();
                }
                _ => unreachable!(),
            }
            assert!(
                check_direct_code_dependency(&target, tx.as_slice(), &dependency_inputs(&changed), &CheckerBudgets::default())
                    .is_err(),
                "O{opt} axis {axis}"
            );
        }
    }
}
#[test]
fn direct_code_dependency_preflights_all_snapshot_inputs_before_parsing() {
    use cellscript_artifact_checker::code_origin::check_direct_code_dependency;
    let (target, raw, cells) = direct_dependency_fixture(0);
    let proof = check_direct_code_dependency(&target, &raw, &dependency_inputs(&cells), &CheckerBudgets::default()).unwrap();
    for count in [0, 65] {
        let oversized = vec![cells[0].clone(); count];
        let err = check_direct_code_dependency(&target, &[0], &dependency_inputs(&oversized), &CheckerBudgets::default()).unwrap_err();
        assert!(err.message.contains("1..=64"));
    }
    let large = vec![0u8; 4 * 1024 * 1024 + 1];
    let limit = vec![0u8; 4 * 1024 * 1024];
    for constructor in [false, true] {
        for axis in 0..5 {
            let mut changed = cells.clone();
            changed[0].output = vec![0];
            let mut budgets = CheckerBudgets::default();
            let input_raw = if axis == 0 { large.as_slice() } else { &[0] };
            match axis {
                1 => changed[1].output = large.clone(),
                2 => changed[1].data = large.clone(),
                3 => {
                    changed = vec![cells[0].clone(); 4];
                    for cell in &mut changed {
                        cell.data = limit.clone();
                    }
                }
                4 => budgets.record_bytes = 0,
                _ => {}
            }
            let err = if constructor {
                check_direct_code_dependency(&target, input_raw, &dependency_inputs(&changed), &budgets).unwrap_err()
            } else {
                proof.check_unchanged_inputs(input_raw, &dependency_inputs(&changed), &budgets).unwrap_err()
            };
            assert_eq!(err.code, CheckerRejectionCode::V2400BudgetExceeded, "constructor={constructor},axis={axis}: {err}");
        }
    }
}

#[test]
fn direct_code_dependency_rejects_actual_checked_type_targets_without_history() {
    use cellscript_artifact_checker::code_origin::{check_code_cell_target, check_direct_code_dependency, SuppliedDependencyCell};
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    for opt in 0..=3 {
        let mut contract = declaration();
        contract.actions.retain(|action| action.action == "burn");
        contract.common_checks.clear();
        let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2026, opt, contract, "ckb-type-hash");
        let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
        let code_type = lock.clone().as_builder().hash_type(1u8).build();
        let output =
            packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock).type_(Some(code_type.clone()).pack()).build();
        let creation =
            TransactionBuilder::default().output(output.clone()).output_data(Bytes::from(fixture.artifact.clone()).pack()).build();
        let script = code_type.clone().as_builder().code_hash(code_type.calc_script_hash()).build();
        let origin = code_origin(&fixture, creation.data().raw().as_slice(), 0, script.as_slice()).unwrap();
        let target = check_code_cell_target(origin).unwrap();
        let inputs = [SuppliedDependencyCell { out_point: [0; 36], output: output.as_slice(), data: &fixture.artifact }];
        let error = check_direct_code_dependency(&target, &[0], &inputs, &CheckerBudgets::default()).unwrap_err();
        assert!(error.message.contains("Type history"));
    }
}

#[test]
fn direct_code_dependency_binds_all_64_cells_including_the_final_snapshot() {
    use cellscript_artifact_checker::code_origin::check_direct_code_dependency;
    use ckb_testtool::ckb_types::{core::TransactionBuilder, packed, prelude::*};
    let (target, _raw, mut cells) = direct_dependency_fixture(0);
    for index in 0..62u32 {
        let point = packed::OutPoint::new_builder().tx_hash([21u8; 32].pack()).index(index).build();
        cells.push(DependencySnapshot {
            out_point: point.as_slice().try_into().unwrap(),
            output: cells[0].output.clone(),
            data: index.to_le_bytes().to_vec(),
        });
    }
    let mut builder = TransactionBuilder::default();
    for cell in cells.iter().rev() {
        let point = packed::OutPoint::from_slice(&cell.out_point).unwrap();
        builder = builder.cell_dep(packed::CellDep::new_builder().out_point(point).dep_type(0u8).build());
    }
    let raw = builder.build().data().raw().as_slice().to_vec();
    let proof = check_direct_code_dependency(&target, &raw, &dependency_inputs(&cells), &CheckerBudgets::default()).unwrap();
    assert_eq!(proof.raw_dependency_index(), 62);
    assert_eq!(proof.supplied_cell_index(), 1);
    let record: serde_json::Value = serde_json::from_slice(&proof.canonical_bytes().unwrap()).unwrap();
    assert_eq!(record["supplied_cells"].as_array().unwrap().len(), 64);
    proof.check_unchanged_inputs(&raw, &dependency_inputs(&cells), &CheckerBudgets::default()).unwrap();
    cells[63].output = vec![0];
    assert!(check_direct_code_dependency(&target, &raw, &dependency_inputs(&cells), &CheckerBudgets::default()).is_err());
    assert!(proof.check_unchanged_inputs(&raw, &dependency_inputs(&cells), &CheckerBudgets::default()).is_err());
}

struct TypeGroupFixture {
    receipt: cellscript_artifact_checker::fixed_policy_receipt::CheckedFixedPolicyReceipt,
    transaction: ckb_testtool::ckb_types::packed::Transaction,
    dependencies: Vec<DependencySnapshot>,
    inputs: Vec<DependencySnapshot>,
}
fn supplied_inputs(cells: &[DependencySnapshot]) -> Vec<cellscript_artifact_checker::code_origin::SuppliedInputCell<'_>> {
    cells
        .iter()
        .map(|cell| cellscript_artifact_checker::code_origin::SuppliedInputCell {
            out_point: cell.out_point,
            output: &cell.output,
            data: &cell.data,
        })
        .collect()
}
fn type_group_fixture(opt: u8) -> TypeGroupFixture {
    let mut args = b"CSARGv1\0".to_vec();
    args.extend_from_slice(&7u64.to_le_bytes());
    type_group_fixture_with(EXTERNAL_SOURCE, opt, args)
}
fn type_group_fixture_with(source: &str, opt: u8, args: Vec<u8>) -> TypeGroupFixture {
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let fixture = external_fixture(source, opt);
    let (creation, script) = receipt_deployment(&fixture, vec![17]);
    let receipt = fixed_policy_receipt(&fixture, vec![17]).unwrap();
    let raw = packed::RawTransaction::from_slice(&creation).unwrap();
    let selected = packed::Script::from_slice(&script).unwrap();
    let code_point = packed::OutPoint::new_builder().tx_hash(raw.calc_tx_hash()).index(0u32).build();
    let policy = encode_policy_witness_bundle(&[PolicyWitnessRecord {
        role: PolicyScriptRole::Type,
        script_hash: selected.calc_script_hash().as_slice().try_into().unwrap(),
        tag: 40,
        args,
    }])
    .unwrap();
    let witness = packed::WitnessArgs::new_builder()
        .lock(Some(Bytes::from(vec![0; 65])).pack())
        .input_type(Some(Bytes::from(policy)).pack())
        .build();
    let lock = raw.outputs().get(0).unwrap().lock();
    let foreign_point = packed::OutPoint::new_builder().tx_hash([88u8; 32].pack()).index(2u32).build();
    let selected_point = packed::OutPoint::new_builder().tx_hash([89u8; 32].pack()).index(3u32).build();
    let transaction = TransactionBuilder::default()
        .cell_dep(packed::CellDep::new_builder().out_point(code_point.clone()).dep_type(0u8).build())
        .input(packed::CellInput::new_builder().previous_output(foreign_point.clone()).build())
        .input(packed::CellInput::new_builder().previous_output(selected_point.clone()).build())
        .output(packed::CellOutput::new_builder().capacity(200000000000u64).lock(lock.clone()).build())
        .output_data(Bytes::new().pack())
        .witness(Bytes::from(vec![0xff]).pack()) // Other groups' witnesses are opaque.
        .witness(witness.as_bytes().pack())
        .witness(Bytes::from(vec![3, 2, 1]).pack())
        .build().data();
    TypeGroupFixture {
        receipt,
        transaction,
        dependencies: vec![DependencySnapshot {
            out_point: code_point.as_slice().try_into().unwrap(),
            output: raw.outputs().get(0).unwrap().as_slice().to_vec(),
            data: fixture.artifact,
        }],
        // Supplied list index 0 corresponds to raw input/witness index 1.
        inputs: vec![
            DependencySnapshot {
                out_point: selected_point.as_slice().try_into().unwrap(),
                output: packed::CellOutput::new_builder()
                    .capacity(200000000000u64)
                    .lock(lock.clone())
                    .type_(Some(selected).pack())
                    .build()
                    .as_slice()
                    .to_vec(),
                data: 1u64.to_le_bytes().to_vec(),
            },
            DependencySnapshot {
                out_point: foreign_point.as_slice().try_into().unwrap(),
                output: packed::CellOutput::new_builder().capacity(200000000000u64).lock(lock).build().as_slice().to_vec(),
                data: vec![5],
            },
        ],
    }
}
fn check_type_group(f: &TypeGroupFixture) -> Result<cellscript_artifact_checker::code_origin::CheckedDirectTypeGroup, CheckerError> {
    use cellscript_artifact_checker::code_origin::{
        check_direct_code_dependency, check_direct_type_group, SuppliedTypeGroupTransaction,
    };
    use ckb_testtool::ckb_types::prelude::*;
    let deps = dependency_inputs(&f.dependencies);
    let inputs = supplied_inputs(&f.inputs);
    let dep = check_direct_code_dependency(f.receipt.target_origin(), f.transaction.raw().as_slice(), &deps, &Default::default())?;
    check_direct_type_group(
        &f.receipt,
        dep,
        &SuppliedTypeGroupTransaction { full_transaction: f.transaction.as_slice(), dependencies: &deps, inputs: &inputs },
        &Default::default(),
    )
}
#[test]
fn direct_type_group_binds_actual_role_full_script_raw_indices_and_complete_witness_bytes() {
    use cellscript_artifact_checker::code_origin::SuppliedTypeGroupTransaction;
    use ckb_testtool::ckb_types::{bytes::Bytes, prelude::*};
    for opt in 0..=3 {
        let f = type_group_fixture(opt);
        let proof = check_type_group(&f).unwrap();
        assert_eq!(proof.group_inputs(), &[1]);
        assert!(proof.group_outputs().is_empty());
        assert_eq!(proof.witness_index(), 1);
        assert_eq!(proof.selected_tag(), 40);
        let bytes = proof.canonical_bytes().unwrap();
        let record: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(record["full_transaction_hash"], code_origin_hex(f.transaction.calc_witness_hash().as_slice()));
        assert_eq!(record["fixed_receipt"], f.receipt.identity());
        assert_eq!(record["witness_source"], "group-input[0]");
        for flag in ["consensus_claimed", "vm_execution_claimed", "authorization_claimed", "signatures_verified"] {
            assert_eq!(record[flag], false);
        }
        let mut material = b"cellscript-direct-type-group-id-v1\0".to_vec();
        material.extend_from_slice(&bytes);
        assert_eq!(proof.identity(), code_origin_hex(&cellscript_artifact_checker::ckb_blake2b256(&material)));
        let deps = dependency_inputs(&f.dependencies);
        let inputs = supplied_inputs(&f.inputs);
        proof
            .check_unchanged_inputs(
                &SuppliedTypeGroupTransaction { full_transaction: f.transaction.as_slice(), dependencies: &deps, inputs: &inputs },
                &Default::default(),
            )
            .unwrap();
        // A changed Lock signature or extra/foreign witness preserves raw tx
        // hash but must invalidate this complete byte snapshot.
        for index in [0, 1, 2] {
            let mut witnesses = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
            let mut data = witnesses[index].raw_data().to_vec();
            *data.last_mut().unwrap() ^= 1;
            witnesses[index] = Bytes::from(data).pack();
            let changed = f.transaction.clone().as_builder().witnesses(witnesses.pack()).build();
            assert_eq!(changed.raw().calc_tx_hash(), f.transaction.raw().calc_tx_hash());
            assert!(proof
                .check_unchanged_inputs(
                    &SuppliedTypeGroupTransaction { full_transaction: changed.as_slice(), dependencies: &deps, inputs: &inputs },
                    &Default::default()
                )
                .is_err());
        }
        let witness =
            ckb_testtool::ckb_types::packed::WitnessArgs::from_slice(f.transaction.witnesses().get(1).unwrap().raw_data().as_ref())
                .unwrap();
        let mut lock_bytes = witness.lock().to_opt().unwrap().raw_data().to_vec();
        lock_bytes[0] = 1;
        let witness = witness.as_builder().lock(Some(Bytes::from(lock_bytes)).pack()).build();
        let mut witnesses = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
        witnesses[1] = witness.as_bytes().pack();
        let signed_bytes_changed = f.transaction.clone().as_builder().witnesses(witnesses.pack()).build();
        assert!(proof
            .check_unchanged_inputs(
                &SuppliedTypeGroupTransaction {
                    full_transaction: signed_bytes_changed.as_slice(),
                    dependencies: &deps,
                    inputs: &inputs
                },
                &Default::default()
            )
            .is_err());
        // Rechecking new opaque signature bytes can produce a new host snapshot;
        // that successful constructor must not claim a signature was verified.
        let changed_fixture = TypeGroupFixture {
            receipt: f.receipt,
            transaction: signed_bytes_changed,
            dependencies: f.dependencies.clone(),
            inputs: f.inputs.clone(),
        };
        let changed_proof = check_type_group(&changed_fixture).unwrap();
        assert_ne!(changed_proof.identity(), proof.identity());
        assert_eq!(serde_json::from_slice::<Value>(&changed_proof.canonical_bytes().unwrap()).unwrap()["signatures_verified"], false);
        let mut reordered = f.inputs.clone();
        reordered.swap(0, 1);
        assert!(proof
            .check_unchanged_inputs(
                &SuppliedTypeGroupTransaction {
                    full_transaction: f.transaction.as_slice(),
                    dependencies: &deps,
                    inputs: &supplied_inputs(&reordered)
                },
                &Default::default()
            )
            .is_err());
    }
}
#[test]
fn direct_type_group_rejects_missing_duplicate_foreign_role_and_changed_args_inputs() {
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for opt in 0..=3 {
        for axis in 0..8 {
            let mut f = type_group_fixture(opt);
            match axis {
                0 => {
                    f.inputs.pop();
                }
                1 => {
                    f.inputs.push(f.inputs[0].clone());
                }
                2 => {
                    f.inputs[1].out_point = f.inputs[0].out_point;
                }
                3 => {
                    f.inputs[1].output = vec![0];
                }
                4 => {
                    f.inputs[0].out_point[0] ^= 1;
                }
                5..=7 => {
                    let output = packed::CellOutput::from_slice(&f.inputs[0].output).unwrap();
                    let selected = output.type_().to_opt().unwrap();
                    let changed = match axis {
                        5 => output.as_builder().type_(Option::<packed::Script>::None.pack()).lock(selected).build(),
                        6 => output
                            .as_builder()
                            .type_(Some(selected.as_builder().args(Bytes::from(vec![18]).pack()).build()).pack())
                            .build(),
                        _ => {
                            f.inputs[1].output = output.as_slice().to_vec();
                            output
                        }
                    };
                    f.inputs[0].output = changed.as_slice().to_vec();
                }
                _ => unreachable!(),
            }
            assert!(check_type_group(&f).is_err(), "O{opt} axis {axis}");
        }
    }
}
#[test]
fn direct_type_group_rejects_unknown_ambiguous_misplaced_or_wrong_width_policy_records() {
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for opt in 0..=3 {
        for axis in 0..10 {
            let mut f = type_group_fixture(opt);
            let mut records = cellscript::policy_witness::decode_policy_witness_bundle(
                packed::WitnessArgs::from_slice(f.transaction.witnesses().get(1).unwrap().raw_data().as_ref())
                    .unwrap()
                    .input_type()
                    .to_opt()
                    .unwrap()
                    .raw_data()
                    .as_ref(),
            )
            .unwrap();
            match axis {
                0 => records[0].role = PolicyScriptRole::Lock,
                1 => records[0].script_hash[0] ^= 1,
                2 => records[0].tag = 41,
                3 => {
                    records[0].args.pop();
                }
                4 => records[0].args.push(0),
                5 => records[0].args = vec![],
                6 => records.push(PolicyWitnessRecord { role: PolicyScriptRole::Lock, script_hash: [0; 32], tag: 1, args: vec![] }),
                _ => {}
            }
            let mut bundle = encode_policy_witness_bundle(&records).unwrap();
            if axis == 6 {
                // All records, including the unselected first member, validate.
                bundle[8 + 12 + 20] = 2;
            }
            if axis == 7 {
                bundle[8] ^= 1;
            }
            let witness = if axis == 8 {
                packed::WitnessArgs::new_builder().output_type(Some(Bytes::from(bundle)).pack()).build()
            } else {
                packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bundle)).pack()).build()
            };
            let mut witnesses = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
            witnesses[1] = witness.as_bytes().pack();
            if axis == 9 {
                witnesses.swap(0, 1);
            }
            f.transaction = f.transaction.as_builder().witnesses(witnesses.pack()).build();
            assert!(check_type_group(&f).is_err(), "O{opt} axis {axis}");
        }
    }
}

#[test]
fn direct_type_group_preflights_complete_shared_inputs_and_rejects_substitution() {
    use cellscript_artifact_checker::code_origin::{
        check_direct_code_dependency, check_direct_type_group, SuppliedTypeGroupTransaction,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let f = type_group_fixture(0);
    let deps = dependency_inputs(&f.dependencies);
    let inputs = supplied_inputs(&f.inputs);
    let dependency = || {
        check_direct_code_dependency(f.receipt.target_origin(), f.transaction.raw().as_slice(), &deps, &Default::default()).unwrap()
    };
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    for axis in 0..7 {
        let mut changed = f.inputs.clone();
        let full: &[u8] = if axis == 0 { &oversized } else { &[0] };
        let mut budgets = CheckerBudgets::default();
        match axis {
            1 => changed[1].output = oversized.clone(),
            2 => changed[1].data = oversized.clone(),
            3 => changed = vec![changed[0].clone(); 257],
            4 => {
                changed = vec![changed[0].clone(); 4];
                for cell in &mut changed {
                    cell.data = vec![0; 4 * 1024 * 1024];
                }
            }
            5 => budgets.record_bytes = 1,
            6 => budgets.artifact_bytes = 0,
            _ => {}
        }
        let error = check_direct_type_group(
            &f.receipt,
            dependency(),
            &SuppliedTypeGroupTransaction { full_transaction: full, dependencies: &deps, inputs: &supplied_inputs(&changed) },
            &budgets,
        )
        .unwrap_err();
        assert!(
            if axis == 3 { error.message.contains("counts") } else { error.code == CheckerRejectionCode::V2400BudgetExceeded },
            "axis {axis}: {error}"
        );
        let proof = check_type_group(&f).unwrap();
        let error = proof
            .check_unchanged_inputs(
                &SuppliedTypeGroupTransaction { full_transaction: full, dependencies: &deps, inputs: &supplied_inputs(&changed) },
                &budgets,
            )
            .unwrap_err();
        assert!(
            if axis == 3 { error.message.contains("counts") } else { error.code == CheckerRejectionCode::V2400BudgetExceeded },
            "recheck axis {axis}: {error}"
        );
    }
    let proof = check_type_group(&f).unwrap();
    for dependency_changed in [false, true] {
        let mut changed_deps = f.dependencies.clone();
        let mut changed_inputs = f.inputs.clone();
        if dependency_changed {
            changed_deps[0].data[0] ^= 1;
        } else {
            changed_inputs[1].data[0] ^= 1;
        }
        assert!(proof
            .check_unchanged_inputs(
                &SuppliedTypeGroupTransaction {
                    full_transaction: f.transaction.as_slice(),
                    dependencies: &dependency_inputs(&changed_deps),
                    inputs: &supplied_inputs(&changed_inputs)
                },
                &Default::default()
            )
            .is_err());
    }
    let foreign = fixed_policy_receipt(&external_fixture(EXTERNAL_SOURCE, 0), vec![18]).unwrap();
    assert!(check_direct_type_group(
        &foreign,
        dependency(),
        &SuppliedTypeGroupTransaction { full_transaction: &[0], dependencies: &deps, inputs: &inputs },
        &Default::default()
    )
    .unwrap_err()
    .message
    .contains("different actual target"));
    // Full Transaction/Witness Bytes are strict even when an unselected item is
    // opaque. A changed inner Bytes count cannot acquire canonical evidence.
    let mut malformed = f.transaction.as_slice().to_vec();
    let extra = f.transaction.witnesses().get(2).unwrap().as_slice().to_vec();
    let position = malformed.windows(extra.len()).rposition(|bytes| bytes == extra).unwrap();
    malformed[position] ^= 1;
    assert!(check_direct_type_group(
        &f.receipt,
        dependency(),
        &SuppliedTypeGroupTransaction { full_transaction: &malformed, dependencies: &deps, inputs: &inputs },
        &Default::default()
    )
    .is_err());
    let mut trailing = f.transaction.as_slice().to_vec();
    trailing.push(0);
    assert!(check_direct_type_group(
        &f.receipt,
        dependency(),
        &SuppliedTypeGroupTransaction { full_transaction: &trailing, dependencies: &deps, inputs: &inputs },
        &Default::default()
    )
    .is_err());
    for count in [0, 65] {
        let extra = (0..count)
            .map(|_| cellscript_artifact_checker::code_origin::SuppliedDependencyCell {
                out_point: f.dependencies[0].out_point,
                output: &f.dependencies[0].output,
                data: &f.dependencies[0].data,
            })
            .collect::<Vec<_>>();
        assert!(check_direct_type_group(
            &f.receipt,
            dependency(),
            &SuppliedTypeGroupTransaction { full_transaction: &[0], dependencies: &extra, inputs: &inputs },
            &Default::default()
        )
        .unwrap_err()
        .message
        .contains("counts"));
    }
    let many = f.transaction.clone().as_builder().witnesses(vec![Bytes::new().pack(); 257].pack()).build();
    assert!(check_direct_type_group(
        &f.receipt,
        dependency(),
        &SuppliedTypeGroupTransaction { full_transaction: many.as_slice(), dependencies: &deps, inputs: &inputs },
        &Default::default()
    )
    .unwrap_err()
    .message
    .contains("count exceeds"));
    let original_witness = packed::WitnessArgs::from_slice(f.transaction.witnesses().get(1).unwrap().raw_data().as_ref()).unwrap();
    let large_witness = original_witness.as_builder().lock(Some(Bytes::from(vec![0; 4096])).pack()).build();
    let mut witnesses = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[1] = large_witness.as_bytes().pack();
    let large = f.transaction.clone().as_builder().witnesses(witnesses.pack()).build();
    assert!(check_direct_type_group(
        &f.receipt,
        dependency(),
        &SuppliedTypeGroupTransaction { full_transaction: large.as_slice(), dependencies: &deps, inputs: &inputs },
        &Default::default()
    )
    .unwrap_err()
    .message
    .contains("WitnessArgs exceeds"));
    // Output-only appearance is located by global output index, but the finite
    // receipt's burn action requires one input and zero outputs; no admission.
    let output = packed::CellOutput::from_slice(&f.inputs[0].output).unwrap();
    let raw = f
        .transaction
        .raw()
        .as_builder()
        .inputs(Vec::<packed::CellInput>::new().pack())
        .outputs(vec![f.transaction.raw().outputs().get(0).unwrap(), output].pack())
        .outputs_data(vec![Bytes::new().pack(), Bytes::from(1u64.to_le_bytes().to_vec()).pack()].pack())
        .build();
    let output_only = f.transaction.clone().as_builder().raw(raw).build();
    assert!(check_direct_type_group(
        &f.receipt,
        check_direct_code_dependency(f.receipt.target_origin(), output_only.raw().as_slice(), &deps, &Default::default()).unwrap(),
        &SuppliedTypeGroupTransaction { full_transaction: output_only.as_slice(), dependencies: &deps, inputs: &[] },
        &Default::default()
    )
    .unwrap_err()
    .message
    .contains("cardinality"));
}

#[test]
fn direct_type_group_checks_all_256_inputs_and_all_eight_policy_records() {
    use cellscript::policy_witness::{
        decode_policy_witness_bundle, encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for opt in 0..=3 {
        let mut f = type_group_fixture(opt);
        let foreign = f.inputs[1].clone();
        let selected = f.inputs[0].clone();
        let mut raw_inputs = Vec::new();
        f.inputs = vec![selected.clone()];
        for index in 0..255u32 {
            let point = packed::OutPoint::new_builder().tx_hash([90u8; 32].pack()).index(index).build();
            raw_inputs.push(packed::CellInput::new_builder().previous_output(point.clone()).build());
            f.inputs.push(DependencySnapshot {
                out_point: point.as_slice().try_into().unwrap(),
                output: foreign.output.clone(),
                data: foreign.data.clone(),
            });
        }
        raw_inputs.push(
            packed::CellInput::new_builder().previous_output(packed::OutPoint::from_slice(&selected.out_point).unwrap()).build(),
        );
        let witness = packed::WitnessArgs::from_slice(f.transaction.witnesses().get(1).unwrap().raw_data().as_ref()).unwrap();
        let selected_record =
            decode_policy_witness_bundle(witness.input_type().to_opt().unwrap().raw_data().as_ref()).unwrap().pop().unwrap();
        let mut records = (0..7u8)
            .map(|index| PolicyWitnessRecord {
                role: PolicyScriptRole::Lock,
                script_hash: [index; 32],
                tag: index as u32,
                args: vec![],
            })
            .collect::<Vec<_>>();
        records.push(selected_record);
        let bundle = encode_policy_witness_bundle(&records).unwrap();
        let witness = witness.as_builder().input_type(Some(Bytes::from(bundle.clone())).pack()).build();
        let mut witnesses = vec![Bytes::from(vec![1]).pack(); 256];
        witnesses[255] = witness.as_bytes().pack();
        let raw = f.transaction.raw().as_builder().inputs(raw_inputs.pack()).build();
        f.transaction = f.transaction.as_builder().raw(raw).witnesses(witnesses.pack()).build();
        let proof = check_type_group(&f).unwrap();
        assert_eq!(proof.group_inputs(), &[255]);
        assert_eq!(proof.witness_index(), 255);
        let saved = f.inputs[255].output.clone();
        f.inputs[255].output = vec![0];
        assert!(check_type_group(&f).is_err(), "malformed final unselected input O{opt}");
        f.inputs[255].output = saved;
        // Final record participates in strict role/key/length checking, even
        // though the selected group is a distinct independently checked key.
        let mut broken = bundle.clone();
        let vector = &bundle[8..];
        let last = u32::from_le_bytes(vector[32..36].try_into().unwrap()) as usize;
        broken[8 + last + 20] = 2;
        let bad_witness = witness.clone().as_builder().input_type(Some(Bytes::from(broken)).pack()).build();
        let mut changed = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
        changed[255] = bad_witness.as_bytes().pack();
        f.transaction = f.transaction.as_builder().witnesses(changed.pack()).build();
        assert!(check_type_group(&f).is_err());
        // Bounds are checked before walking a claimed ninth record.
        let mut ninth = bundle;
        ninth[12..16].copy_from_slice(&40u32.to_le_bytes());
        let witness = witness.as_builder().input_type(Some(Bytes::from(ninth)).pack()).build();
        let mut changed = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
        changed[255] = witness.as_bytes().pack();
        f.transaction = f.transaction.as_builder().witnesses(changed.pack()).build();
        assert!(check_type_group(&f).is_err());
    }
}

#[test]
fn direct_type_group_uses_checked_scalar_widths_and_exact_empty_argument_case() {
    use cellscript::policy_witness::{decode_policy_witness_bundle, encode_policy_witness_bundle};
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for opt in 0..=3 {
        for (ty, width) in [("u8", 1), ("u16", 2), ("u32", 4), ("u64", 8), ("unit", 0)] {
            let source = if width == 0 {
                EXTERNAL_SOURCE.replace(", witness value: u64", "").replace(" require value > 0", "")
            } else {
                EXTERNAL_SOURCE.replace("witness value: u64", &format!("witness value: {ty}"))
            };
            let args = if width == 0 {
                vec![]
            } else {
                let mut args = b"CSARGv1\0".to_vec();
                args.extend_from_slice(&7u64.to_le_bytes()[..width]);
                args
            };
            let mut f = type_group_fixture_with(&source, opt, args);
            check_type_group(&f).unwrap();
            let witness = packed::WitnessArgs::from_slice(f.transaction.witnesses().get(1).unwrap().raw_data().as_ref()).unwrap();
            let mut records = decode_policy_witness_bundle(witness.input_type().to_opt().unwrap().raw_data().as_ref()).unwrap();
            if width == 0 {
                records[0].args = b"CSARGv1\0".to_vec();
            } else {
                records[0].args.push(0);
            }
            let witness =
                witness.as_builder().input_type(Some(Bytes::from(encode_policy_witness_bundle(&records).unwrap())).pack()).build();
            let mut witnesses = f.transaction.witnesses().into_iter().collect::<Vec<_>>();
            witnesses[1] = witness.as_bytes().pack();
            f.transaction = f.transaction.as_builder().witnesses(witnesses.pack()).build();
            assert!(check_type_group(&f).is_err(), "O{opt} {ty} wrong checked args length");
        }
    }
}

#[test]
fn direct_type_group_rejects_actual_receipt_bundle_byte_substitution() {
    use cellscript_artifact_checker::code_origin::{
        check_direct_code_dependency, check_direct_type_group, SuppliedTypeGroupTransaction,
    };
    use ckb_testtool::ckb_types::prelude::*;
    let f = type_group_fixture(0);
    let fixture = external_fixture(EXTERNAL_SOURCE, 0);
    let mut bundle = receipt_bundle(&fixture);
    // Both are actual independently checked bundles; JSON whitespace retains
    // API shape/code bytes but changes bound machine-byte provenance identities.
    bundle[1] = serde_json::to_vec_pretty(&fixture.metadata).unwrap();
    let (creation, script) = receipt_deployment(&fixture, vec![17]);
    let second = cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt(
        bundle.each_ref().map(Vec::as_slice),
        &creation,
        0,
        &script,
        &Default::default(),
    )
    .unwrap();
    assert_ne!(f.receipt.target_origin().identity(), second.target_origin().identity());
    assert_eq!(f.receipt.target_origin().origin().artifact_hash(), second.target_origin().origin().artifact_hash());
    assert_eq!(f.receipt.entry_contract(), second.entry_contract());
    assert_ne!(f.receipt.identity(), second.identity());
    let deps = dependency_inputs(&f.dependencies);
    let inputs = supplied_inputs(&f.inputs);
    let dep =
        check_direct_code_dependency(f.receipt.target_origin(), f.transaction.raw().as_slice(), &deps, &Default::default()).unwrap();
    let error = check_direct_type_group(
        &second,
        dep,
        &SuppliedTypeGroupTransaction { full_transaction: f.transaction.as_slice(), dependencies: &deps, inputs: &inputs },
        &Default::default(),
    )
    .unwrap_err();
    assert!(error.message.contains("different actual target"));
    check_type_group(&f).unwrap();
}

#[test]
fn nested_public_layouts_admit_end_to_end_with_certified_frame_copies() {
    const NESTED_SOURCE: &str = r#"
module nested_codec
resource Token has store, consume { amount: u64 }
public struct Inner { first: u32, second: u32 }
public struct Outer { meta: Inner, count: u64 }
action burn(input token: Token, witness value: u64) {
    verification
    let outer: Outer = Outer { meta: Inner { first: 1, second: 2 }, count: 3 }
    require token.amount > 0
    require outer.count > 0
    require value > 0
    consume token
}
"#;
    for opt in 0..=3 {
        let fixture = external_fixture(NESTED_SOURCE, opt);
        let checked = fixed_external_codec(&fixture).unwrap();
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["profile"], "policy-unit-scalars-nested-unsigned-cell-v1");
        let outer = record["public_layouts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|layout| layout["declaration"].as_str().is_some_and(|identity| identity.contains("Outer")))
            .unwrap();
        assert_eq!(outer["width"], 16);
        // A same-width nominal substitute keeps every width but changes the
        // declaration, so the codec identity distinguishes owners.
        let swapped = fixed_external_codec(&external_fixture(
            &NESTED_SOURCE
                .replace("public struct Inner", "public struct Substitute")
                .replace("meta: Inner", "meta: Substitute")
                .replace("Inner { first: 1", "Substitute { first: 1"),
            opt,
        ))
        .unwrap();
        assert_ne!(checked.identity(), swapped.identity());
        // Machine-word mutations at the certified copy sites reject: the
        // destination offset escapes the frame, the length becomes unbounded,
        // or the call target leaves the certified helper.
        let memcpy_entry = fixture.record.entries.iter().find(|entry| entry.name == "__cellscript_memcpy_fixed").unwrap();
        let memcpy_start = fixture.record.blocks.iter().find(|block| block.id == memcpy_entry.entry_block).unwrap().range.start;
        let burn = fixture.record.entries.iter().find(|entry| entry.name == "burn").unwrap();
        let burn_start = fixture.record.blocks.iter().find(|block| block.id == burn.entry_block).unwrap().range.start;
        let elf = parse_elf(&fixture.artifact, CheckerBudgets::default().instructions).unwrap();
        let mut sites = Vec::new();
        for instruction in &elf.instructions {
            if matches!(instruction.word & 0x7f, 0x6f | 0x67)
                && ((instruction.word >> 7) & 31) == 1
                && instruction.address >= burn_start
                && elf.control_flow.iter().any(|flow| flow.address == instruction.address && flow.target == memcpy_start)
            {
                sites.push(instruction.address);
            }
        }
        assert!(!sites.is_empty(), "opt={opt}");
        for site in sites {
            for mutation in ["destination", "length"] {
                let mut changed = fixture.clone();
                let argument = if mutation == "destination" { site - 8 } else { site - 12 };
                let original = elf.instructions.iter().find(|item| item.address == argument).unwrap().word;
                changed.replace_machine_word(argument, (original & !(0xfff << 20)) | (0x7ffu32 << 20));
                changed.check().unwrap();
                assert_eq!(
                    fixed_external_codec(&changed).unwrap_err().code,
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    "opt={opt} site={site} {mutation}"
                );
            }
        }
    }
}

#[test]
fn constant_value_catalog_rejects_rebound_mutations() {
    use cellscript_artifact_checker::{ConstantExpression, ConstantValueContract};
    for opt in 0..=3 {
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LIMIT: u64 = 3 * 7"), opt);
        fixed_external_codec(&fixture).unwrap();
        for mutation in ["value", "expression", "div-zero", "deep", "missing-catalog", "undeclared"] {
            let mut changed = fixture.clone();
            if mutation == "missing-catalog" {
                changed.record.typed_semantics.constant_values = None;
            } else {
                let catalog = changed.record.typed_semantics.constant_values.as_mut().unwrap();
                match mutation {
                    "value" => catalog.values[0].value = "22".into(),
                    "expression" => {
                        let ConstantExpression::Binary { right, .. } = &mut catalog.values[0].expression else { unreachable!() };
                        **right = ConstantExpression::Literal { value: "8".into() };
                    }
                    "div-zero" => {
                        let ConstantExpression::Binary { op, right, .. } = &mut catalog.values[0].expression else { unreachable!() };
                        *op = "div".into();
                        **right = ConstantExpression::Literal { value: "0".into() };
                    }
                    "deep" => {
                        // Twenty nested additions evaluate to the recorded
                        // value but exceed the evaluation depth budget.
                        let mut expression = ConstantExpression::Literal { value: "1".into() };
                        for _ in 0..20 {
                            expression = ConstantExpression::Binary {
                                op: "add".into(),
                                left: Box::new(expression),
                                right: Box::new(ConstantExpression::Literal { value: "1".into() }),
                            };
                        }
                        catalog.values[0].expression = expression;
                        catalog.values[0].value = "21".into();
                    }
                    "undeclared" => catalog.values.push(ConstantValueContract {
                        module: catalog.values[0].module.clone(),
                        name: "OTHER".into(),
                        ty: "u64".into(),
                        value: "1".into(),
                        expression: ConstantExpression::Literal { value: "1".into() },
                    }),
                    _ => unreachable!(),
                }
            }
            changed.rebind_policy_identity();
            let error = fixed_external_codec(&changed).unwrap_err();
            assert!(error.message.contains("constant"), "opt={opt} {mutation}: {error}");
        }
    }
}

#[test]
fn nominal_catalog_rejects_reserved_open_handle_class_names() {
    for opt_level in 0..=3 {
        let baseline = Fixture::new_source_with(
            &format!("{SOURCE}\npublic struct Snapshot {{ z: u64 }}"),
            CellScriptEdition::Edition2027,
            opt_level,
            declaration(),
        );
        baseline.assert_interface_inspection();
        let mut changed = baseline.clone();
        let catalog = changed.record.typed_semantics.nominal_declarations.as_mut().unwrap();
        catalog.declarations.iter_mut().find(|declaration| declaration.name == "Snapshot").unwrap().name = "ScriptHandle".into();
        changed.rebind_policy_identity();
        let error = cellscript_artifact_checker::interface::inspect_bundle(
            &changed.artifact,
            &serde_json::to_vec(&changed.metadata).unwrap(),
            &serde_json::to_vec(&changed.record).unwrap(),
            &serde_json::to_vec(&changed.source_map).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert!(error.message.contains("reserved open-handle class name"), "opt={opt_level}: {error}");
    }
}

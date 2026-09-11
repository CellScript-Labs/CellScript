use cellscript::{compile, strip_vm_abi_trailer, CompileOptions, EntryWitnessArg, NEXT_EDITION};
use cellscript_ckb_adapter::{place_entry_witness_payload_before_signing, EntryWitnessPlacementAbi};
use ckb_testtool::ckb_hash::blake2b_256;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, compile_cellscript_source_to_elf, execute_cellscript_script};

const FIXED_WIDTH_OPENING: &str = r#"
module committed

struct State {
    counter: u64,
    owner: Address,
}

lock reveal(witness expected: Commitment<State>, witness opening: Opening<State>) -> bool {
    verification
        let state = commitment::open(expected, opening)
        return state.counter > 0
}
"#;

#[test]
fn committed_state_surface_round_trips_formatter_and_lsp() {
    let tokens = cellscript::lexer::lex(FIXED_WIDTH_OPENING).unwrap();
    let module = cellscript::parser::parse(&tokens).unwrap();
    let formatted = cellscript::fmt::format_default(&module).unwrap();
    assert!(formatted.contains("Commitment<State>"));
    assert!(formatted.contains("Opening<State>"));
    assert!(formatted.contains("commitment::open(expected, opening)"));
    compile(&formatted, CompileOptions::default()).expect("formatted committed-state source");

    let uri = "file:///committed-state.cell".to_string();
    let mut lsp = cellscript::lsp::LspServer::new();
    lsp.open_document(uri.clone(), FIXED_WIDTH_OPENING.to_string());
    let diagnostics = lsp.get_diagnostics(&uri);
    assert!(diagnostics.is_empty(), "unexpected committed-state LSP diagnostics: {diagnostics:?}");
    let edits = lsp.format_document(&uri);
    assert!(edits.iter().any(|edit| edit.new_text.contains("Opening<State>")), "missing committed-state LSP formatting edit");
}

#[test]
fn fixed_width_opening_lowers_to_hash_compare_then_materialize() {
    let result = compile(FIXED_WIDTH_OPENING, CompileOptions::default()).unwrap();
    let lock = result.metadata.locks.iter().find(|lock| lock.name == "reveal").unwrap();
    let expected = lock.params.iter().find(|param| param.name == "expected").unwrap();
    assert_eq!(expected.fixed_byte_len, Some(32));
    assert!(!expected.schema_pointer_abi);
    let opening = lock.params.iter().find(|param| param.name == "opening").unwrap();
    assert!(opening.schema_pointer_abi);
    assert_eq!(opening.fixed_byte_len, None);
    let opening_plan =
        lock.proof_plan.iter().find(|plan| plan.category == "committed-state-opening").expect("typed opening ProofPlan record");
    assert_eq!(opening_plan.evidence_tier.as_str(), "checked-runtime");
    assert!(opening_plan.coverage.iter().any(|item| item == "mismatch-error:73"));

    let commitment = [0x11; 32];
    let opening_bytes = vec![0x22; 40];
    let encoded = lock
        .entry_witness_args(&[EntryWitnessArg::Bytes(commitment.to_vec()), EntryWitnessArg::Bytes(opening_bytes.clone())])
        .unwrap();
    let mut expected_encoding = b"CSARGv1\0".to_vec();
    expected_encoding.extend_from_slice(&commitment);
    expected_encoding.extend_from_slice(&(opening_bytes.len() as u32).to_le_bytes());
    expected_encoding.extend_from_slice(&opening_bytes);
    assert_eq!(encoded, expected_encoding);

    let assembly = String::from_utf8(result.artifact_bytes).unwrap();

    assert!(assembly.contains("# cellscript committed-state: validate explicit fixed-width opening"));
    assert!(assembly.contains("call __ckb_hash_blake2b_var"));
    assert!(assembly.contains("call __cellscript_memcmp_fixed"));
    assert!(assembly.contains("commitment-opening-mismatch"));

    let compare = assembly.find("call __cellscript_memcmp_fixed").unwrap();
    let verified = assembly.find("commitment_opening_verified").unwrap();
    let materialize = assembly.find("validated typed opening").unwrap();
    assert!(compare < verified && verified < materialize, "opening became typed before commitment validation");
}

#[test]
fn packed_commitment_constructor_preserves_nominal_domain() {
    let source = r#"
module committed

struct State { counter: u64 }

lock same(witness value: State) -> bool {
    verification
        let left = commitment::commit(value)
        let right = commitment::commit(value)
        return left == right
}
"#;
    let result = compile(source, CompileOptions::default()).unwrap();
    let plans = &result.metadata.locks[0].proof_plan;
    assert_eq!(plans.iter().filter(|plan| plan.category == "committed-state-commit").count(), 2);
    let assembly = String::from_utf8(result.artifact_bytes).unwrap();
    assert_eq!(assembly.matches("call __ckb_hash_blake2b_var").count(), 2);
    assert!(assembly.contains("CellScript Generated Assembly"));
}

#[test]
fn exact_opening_executes_and_tampering_returns_stable_error_in_ckb_vm() {
    let source = FIXED_WIDTH_OPENING
        .replace("lock reveal", "action reveal")
        .replace(" -> bool", " -> u64")
        .replace("return state.counter > 0", "return 0");
    let elf = compile_cellscript_source_to_elf(&source, "reveal", None);

    let mut opening = 7u64.to_le_bytes().to_vec();
    opening.extend_from_slice(&[0x42; 32]);
    let mut preimage = b"CellScriptPackedHashV0\0State\0".to_vec();
    preimage.extend_from_slice(&(opening.len() as u32).to_le_bytes());
    preimage.extend_from_slice(&opening);
    let commitment = blake2b_256(&preimage);

    let run = |commitment: [u8; 32], opening: &[u8]| {
        let mut payload = b"CSARGv1\0".to_vec();
        payload.extend_from_slice(&commitment);
        payload.extend_from_slice(&(opening.len() as u32).to_le_bytes());
        payload.extend_from_slice(opening);
        let base = packed::WitnessArgs::new_builder().build();
        let witness =
            place_entry_witness_payload_before_signing(&base, EntryWitnessPlacementAbi::WitnessArgsInputTypeV2, Bytes::from(payload))
                .unwrap();
        let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![witness.as_bytes()];
        execute_cellscript_script(&elf, &fixture).exit_code
    };

    assert_eq!(run(commitment, &opening), 0);
    opening[0] ^= 1;
    assert_eq!(run(commitment, &opening), 73);
}

#[test]
fn committed_substate_vectors_pin_domain_width_bytes_and_ckb_hash() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/committed_substate_vectors.json")).unwrap();
    assert_eq!(fixture["schema"], "cellscript-committed-substate-v1");
    for vector in fixture["vectors"].as_array().unwrap() {
        let packed = hex::decode(vector["packed_value_hex"].as_str().unwrap()).unwrap();
        let width = vector["packed_width"].as_u64().unwrap() as usize;
        assert_eq!(packed.len(), width);

        let mut preimage = b"CellScriptPackedHashV0\0".to_vec();
        preimage.extend_from_slice(vector["type_name"].as_str().unwrap().as_bytes());
        preimage.push(0);
        preimage.extend_from_slice(&(width as u32).to_le_bytes());
        preimage.extend_from_slice(&packed);
        assert_eq!(hex::encode(&preimage), vector["preimage_hex"].as_str().unwrap());
        assert_eq!(hex::encode(blake2b_256(&preimage)), vector["commitment_hex"].as_str().unwrap());
    }
}

#[test]
fn successor_cell_uses_one_evaluated_typed_commitment_value() {
    let source = r#"
module committed_successor

struct State { counter: u64 }

resource Vault has store, create, consume {
    state_commitment: Commitment<State>,
}

action rotate(
    input before: Vault,
    witness opening: Opening<State>,
    witness next_state: State,
    witness owner: Address,
) -> after: Vault {
    verification
        let current = commitment::open(before.state_commitment, opening)
        require next_state.counter == current.counter + 1
        let successor = commitment::commit(next_state)
        consume before
        create after = Vault { state_commitment: successor } with_lock(owner)
}
"#;
    let result = compile(source, CompileOptions::default()).unwrap();
    let action = result.metadata.actions.iter().find(|action| action.name == "rotate").unwrap();
    assert!(action.fail_closed_runtime_features.is_empty(), "{:?}", action.fail_closed_runtime_features);
    assert_eq!(action.create_set.len(), 1);
    assert_eq!(action.create_set[0].fields, vec!["state_commitment"]);
    let recommit = action
        .proof_plan
        .iter()
        .find(|plan| plan.category == "committed-state-commit")
        .expect("successor commitment ProofPlan record");
    assert_eq!(recommit.input_output_relation_checks, vec!["successor-field:after.state_commitment=single-evaluated-commitment"]);
    let assembly = String::from_utf8(result.artifact_bytes).unwrap();
    assert_eq!(assembly.matches("# cellscript committed-state: validate explicit fixed-width opening").count(), 1);
    assert_eq!(assembly.matches("call __ckb_hash_blake2b_var").count(), 2);
}

#[test]
fn maximum_fixed_width_opening_resource_profile_is_exact_and_bounded() {
    let source = r#"
module committed_resource

struct S { bytes: [u8; 451] }

action reveal(witness expected: Commitment<S>, witness opening: Opening<S>) -> u64 {
    verification
        let value = commitment::open(expected, opening)
        require value.bytes[0] == 17
        require value.bytes[450] == 23
        require commitment::commit(value) == expected
        return 0
}
"#;
    let result = compile(
        source,
        CompileOptions {
            edition: NEXT_EDITION,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
    )
    .expect("maximum fixed-width committed-state fixture");
    let mut opening = vec![0u8; 451];
    opening[0] = 17;
    opening[450] = 23;
    let mut preimage = b"CellScriptPackedHashV0\0S\0".to_vec();
    preimage.extend_from_slice(&(opening.len() as u32).to_le_bytes());
    preimage.extend_from_slice(&opening);
    assert_eq!(preimage.len(), 480);
    let commitment = blake2b_256(&preimage);
    let action = result.metadata.actions.iter().find(|action| action.name == "reveal").unwrap();
    let payload = action.entry_witness_args(&[EntryWitnessArg::Bytes(commitment.to_vec()), EntryWitnessArg::Bytes(opening)]).unwrap();
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build();
    let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![witness.as_bytes()];
    let execution = execute_cellscript_script(strip_vm_abi_trailer(&result.artifact_bytes), &fixture);
    assert_eq!(execution.exit_code, 0, "maximum opening failed: {:?}", execution.captured_debug);

    let max_stack_frame_bytes =
        result.verified_lowering_record.as_ref().unwrap().entries.iter().map(|entry| entry.frame_size_bytes).max().unwrap();
    let actual = serde_json::json!({
        "cycles": execution.cycles,
        "elf_bytes": strip_vm_abi_trailer(&result.artifact_bytes).len(),
        "max_stack_frame_bytes": max_stack_frame_bytes,
        "witness_bytes": execution.witness_bytes,
        "transaction_bytes": execution.transaction_bytes,
        "dependency_bytes": execution.dependency_bytes,
    });
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/committed_substate_resource_budget.json")).unwrap();
    assert_eq!(manifest["maximum_shape"]["scratch_bytes_including_digest"], 512);
    let identities = serde_json::json!({
        "artifact_hash": format!("0x{}", result.metadata.artifact_hash.as_deref().unwrap()),
        "lowering_record_hash": format!("0x{}", result.metadata.verified_artifact.lowering_record_hash.as_deref().unwrap()),
        "source_map_hash": format!("0x{}", result.metadata.verified_artifact.source_map_hash.as_deref().unwrap()),
        "verified_bundle_id": format!("0x{}", result.metadata.verified_artifact.verified_bundle_id.as_deref().unwrap()),
        "raw_transaction_hash": execution.raw_transaction_hash,
        "serialized_transaction_hash": execution.serialized_transaction_hash,
    });
    assert_eq!(identities, manifest["identities"], "recorded committed-state identities are stale: {identities}");
    assert_eq!(actual, manifest["measured"], "recorded committed-state resource measurement is stale: {actual}");
    for field in ["cycles", "elf_bytes", "max_stack_frame_bytes", "witness_bytes", "transaction_bytes", "dependency_bytes"] {
        assert!(actual[field].as_u64().unwrap() <= manifest["budgets"][field].as_u64().unwrap(), "{field} exceeded budget");
    }
}

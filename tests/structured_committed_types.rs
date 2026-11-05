//! Retained committed value types preserve the packed domain, authentication
//! before materialization, one-shot witness ownership and independent admission.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_types::{bytes::Bytes, packed, prelude::*},
};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/committed_source_identity_current.json")).unwrap()
}

#[test]
fn committed_source_matches_current_executable_interface_and_semantic_identity() {
    for case in cases()["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let result = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
        result.validate().unwrap();
        assert_eq!(hex::encode(Sha256::digest(source.as_bytes())), case["source_sha256"]);
        assert_eq!(hex::encode(Sha256::digest(&result.artifact_bytes)), case["elf_sha256"], "{}", case["name"]);
        assert_eq!(result.metadata.interface_hash, case["interface_hash"], "{}", case["name"]);
        assert_eq!(result.metadata.typed_semantics_hash, case["typed_semantics_hash"], "{}", case["name"]);
    }
}

fn execute(result: &cellscript::CompileResult, values: &[EntryWitnessArg]) -> i64 {
    let params = &result.metadata.actions[0].params;
    let payload = cellscript::encode_entry_witness_args_for_params(params, values).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code
}

fn digest(domain: &str, value: &[u8]) -> Vec<u8> {
    let mut preimage = b"CellScriptPackedHashV0\0".to_vec();
    preimage.extend_from_slice(domain.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&(value.len() as u32).to_le_bytes());
    preimage.extend_from_slice(value);
    blake2b_256(&preimage).to_vec()
}

#[test]
fn committed_source_authenticates_record_and_scalar_openings_in_both_profiles() {
    for case in cases()["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        {
            let edition = CellScriptEdition::Edition2027;
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_with_executable_surface_policy(
                    source,
                    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                result.validate().unwrap();
                let mut value = 7u64.to_le_bytes().to_vec();
                if case["name"] == "construct" {
                    assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(value)]), 0);
                    continue;
                }
                let domain = if case["name"] == "record" {
                    value.extend_from_slice(&[0x42; 32]);
                    "State"
                } else {
                    "u64"
                };
                let expected = digest(domain, &value);
                let params = &result.metadata.actions[0].params;
                assert_eq!(params[0].fixed_byte_len, Some(32));
                assert!(!params[0].schema_pointer_abi);
                assert!(params[1].schema_pointer_abi && params[1].schema_length_abi);
                assert_eq!(params[1].fixed_byte_len, None);
                assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(expected.clone()), EntryWitnessArg::Bytes(value.clone())]), 0);
                value[0] ^= 1;
                assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(expected), EntryWitnessArg::Bytes(value)]), 73);
            }
        }
    }
}

#[test]
fn structural_array_and_tuple_openings_materialize_only_after_authentication() {
    for (ty, domain, value, predicate) in [
        ("[u8; 16]", "[u8; 16]", vec![7u8; 16], "value[0] == 7"),
        ("(u64, [u8; 2])", "(u64, [u8; 2])", [7u64.to_le_bytes().to_vec(), vec![8u8; 2]].concat(), "value.0 == 7"),
    ] {
        let source = format!("module committed_aggregate\naction verify(witness expected: Commitment<{ty}>, witness opening: Opening<{ty}>) -> u64 {{ verification let value = commitment::open(expected, opening) require {predicate} return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/{ty}: {error}"));
                result.validate().unwrap();
                let expected = digest(domain, &value);
                assert_eq!(
                    execute(&result, &[EntryWitnessArg::Bytes(expected.clone()), EntryWitnessArg::Bytes(value.clone())]),
                    0,
                    "{ty}"
                );
                let mut changed = value.clone();
                changed[0] ^= 1;
                assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(expected), EntryWitnessArg::Bytes(changed)]), 73, "{ty}");
            }
        }
    }
}

#[test]
fn hash_and_address_domains_survive_typed_wire_case_and_aggregate_materialization() {
    for (ty, value) in [("Hash", vec![7; 32]), ("Address", vec![8; 32]), ("[Hash; 1]", vec![9; 32]), ("(Hash, ())", vec![10; 32])] {
        let source = format!("module committed_case\naction verify(witness expected: Commitment<{ty}>, witness opening: Opening<{ty}>) -> u64 {{ verification let value = commitment::open(expected, opening) let actual = commitment::commit(value) require actual == expected return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{ty}/{edition}/{diagnostics}: {error:?}"));
                result.validate().unwrap();
                assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(digest(ty, &value)), EntryWitnessArg::Bytes(value.clone())]), 0);
                assert_eq!(
                    execute(
                        &result,
                        &[EntryWitnessArg::Bytes(digest(&ty.to_lowercase(), &value)), EntryWitnessArg::Bytes(value.clone())]
                    ),
                    73
                );
            }
        }
    }
}

#[test]
fn ordinary_commitment_results_own_their_digest_across_later_helper_calls() {
    let source = r#"
module committed_results
fn build(value: u64) -> Commitment<u64> { let result = commitment::commit(value) return result }
fn forward(value: Commitment<u64>) -> Commitment<u64> { let retained = value return retained }
action verify(witness opening: Opening<u64>) -> u64 {
    verification
    let first = build(7)
    let second = build(9)
    require first != second
    let retained = forward(first)
    let value = commitment::open(retained, opening)
    require value == 7
    return 0
}
"#;
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("{edition}/{diagnostics}: {error:?}"));
            result.validate().unwrap();
            assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(7u64.to_le_bytes().to_vec())]), 0);
            assert_eq!(execute(&result, &[EntryWitnessArg::Bytes(9u64.to_le_bytes().to_vec())]), 73);
        }
    }
}

#[test]
fn committed_argument_roles_and_legacy_ir_cannot_supply_type_evidence() {
    for ty in ["Commitment<16>", "Opening<16>", "Commitment<u64, u64>", "Opening<>"] {
        let source =
            format!("module invalid_committed\nfn inspect(value: {ty}) {{}}\naction verify() -> u64 {{ verification return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for target in [None, Some("riscv64-elf".into())] {
                let error = cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).unwrap_err();
                assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111), "{ty}: {error}");
            }
        }
    }
    let fixture = cases();
    let result = cellscript::compile(fixture["cases"][0]["source"].as_str().unwrap(), CompileOptions::default()).unwrap();
    for name in ["Commitment<State>", "Opening<State>", "Wrapper<Opening<State>>"] {
        let mut ir = cellscript::ir::generate(&result.ast).unwrap();
        let action = ir
            .items
            .iter_mut()
            .find_map(|item| match item {
                cellscript::ir::IrItem::Action(action) => Some(action),
                _ => None,
            })
            .unwrap();
        action.params[0].ty = cellscript::ir::IrType::Named(name.into());
        assert_eq!(
            cellscript::executable_surface::validate_ir_module(&ir).unwrap_err().code,
            Some(cellscript::error::DiagnosticCode::E2105)
        );
    }
}

#[test]
fn reserved_selection_and_unresolved_template_diagnostics_survive_structuring() {
    for (ty, expected) in [("ExactCellSelection<Token>", "constructed locally"), ("Vault<u64>", "has no local template available")] {
        let source = format!("module invalid_application\nresource Token has store {{ value: u64 }}\nfn inspect(value: {ty}) {{}}\naction verify() -> u64 {{ verification return 0 }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for target in [None, Some("riscv64-elf".into())] {
                let error = cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).unwrap_err();
                assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111));
                assert!(error.message.contains(expected), "{ty}: {error}");
            }
        }
    }
}

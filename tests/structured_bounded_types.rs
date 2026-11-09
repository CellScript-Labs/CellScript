//! Bounded family migration preserves source ownership, machine transport and
//! the independently verified runtime contract, including unavailable shapes.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn cases() -> Value {
    serde_json::from_str(include_str!("fixtures/bounded_source_identity_current.json")).unwrap()
}

#[test]
fn bounded_source_matches_current_executable_and_contract_identity() {
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

#[test]
fn bounded_runtime_and_fail_closed_shapes_execute_in_edition_2027_and_both_profiles() {
    for case in cases()["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        let unavailable = case["name"].as_str().unwrap().contains("FAIL_CLOSED");
        {
            let edition = CellScriptEdition::Edition2027;
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let options = CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() };
                if unavailable {
                    assert!(cellscript::compile_with_executable_surface_policy(
                        source,
                        options.clone(),
                        ExecutableSurfacePolicy::DenyFailClosed
                    )
                    .is_err());
                }
                let result = cellscript::compile(source, options).unwrap_or_else(|error| panic!("{edition}/{diagnostics}: {error}"));
                result.validate().unwrap();
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                if !unavailable && case["name"] == "BOUNDED_CREATE_RUNTIME_ENTRY" {
                    let owner = ckb_script_runner::deterministic_always_success_lock_hash();
                    let mut element = owner.to_vec();
                    element.extend_from_slice(&42u64.to_le_bytes());
                    let plan = cellscript::encode_bounded_output_plan_v1(&[element], 40, 2).unwrap();
                    let payload = result
                        .metadata
                        .actions
                        .iter()
                        .find(|action| action.name == "verify")
                        .unwrap()
                        .entry_witness_args(&[cellscript::EntryWitnessArg::Bytes(plan)])
                        .unwrap();
                    fixture.current_type_script_input_indices.clear();
                    fixture.outputs[0].data = Bytes::copy_from_slice(&42u64.to_le_bytes());
                    fixture.witnesses =
                        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                } else {
                    fixture.inputs[0].data = Bytes::copy_from_slice(&42u64.to_le_bytes());
                    fixture.current_type_script_input_indices = vec![0];
                }
                let executed = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(
                    executed.exit_code,
                    if unavailable { 24 } else { 0 },
                    "{edition}/{diagnostics}: {}: {executed:?}",
                    case["name"]
                );
            }
        }
    }
}

#[test]
fn bounded_source_rejects_wrong_argument_roles_bounds_and_element_ownership() {
    for ty in [
        "BoundedCellSet<Token, 0>",
        "BoundedCellSet<Token, 1025>",
        "BoundedCellSet<Token, 340282366920938463463374607431768211455>",
        "BoundedCellSet<2, 2>",
        "BoundedCellSet<Token, u64>",
        "BoundedCellSet<Token>",
        "BoundedCellSet<Token, 2, 2>",
        "BoundedCellSet<Plan, 2>",
        "BoundedCellSet<&Token, 2>",
        "BoundedList<Token, 2>",
        "BoundedList<&u64, 2>",
        "BoundedList<Vec<u8>, 2>",
    ] {
        let source = format!("module wrong_bounded;\nresource Token has consume {{ amount: u64, }}\nstruct Plan {{ amount: u64, }}\nfn inspect(value: {ty}) {{}}\naction verify() -> u64 {{  return 0; }}");
        {
            let edition = CellScriptEdition::Edition2027;
            for target in [None, Some("riscv64-elf".into())] {
                let error = cellscript::compile(&source, CompileOptions { edition, target, ..Default::default() }).unwrap_err();
                assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111), "{ty}: {error}");
            }
        }
    }
}

#[test]
fn bounded_ir_rejects_legacy_names_including_nested_wrappers() {
    let fixture = cases();
    let result = cellscript::compile(fixture["cases"][0]["source"].as_str().unwrap(), CompileOptions::default()).unwrap();
    let original = cellscript::ir::generate(&result.ast).unwrap();
    for ty in [
        cellscript::ir::IrType::Named("BoundedCellSet<Token, 2>".into()),
        cellscript::ir::IrType::Array(Box::new(cellscript::ir::IrType::Named("BoundedList<Plan, 2>".into())), 1),
        cellscript::ir::IrType::Named("Wrapper<BoundedCellSet<Token, 2>>".into()),
    ] {
        let mut module = original.clone();
        let action = module
            .items
            .iter_mut()
            .find_map(|item| match item {
                cellscript::ir::IrItem::Action(action) => Some(action),
                _ => None,
            })
            .unwrap();
        action.params[0].ty = ty;
        let error = cellscript::executable_surface::validate_ir_module(&module).unwrap_err();
        assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2105));
    }
}

#[test]
fn bounded_non_nominal_plans_keep_their_element_tree_and_fail_closed_contract() {
    let source = "module bounded_tuple_plan;\nresource Token has create { amount: u64, }\naction verify(witness plans: BoundedList<(u64, [u8; 2]), 2>) -> u64 {  create_each plan in plans { require plan.0 > 0; create Token { amount: plan.0, } } return 0; }";
    {
        let edition = CellScriptEdition::Edition2027;
        let result = cellscript::compile(source, CompileOptions { edition, ..Default::default() }).unwrap();
        let ir = cellscript::ir::generate(&result.ast).unwrap();
        let action = ir
            .items
            .iter()
            .find_map(|item| match item {
                cellscript::ir::IrItem::Action(action) => Some(action),
                _ => None,
            })
            .unwrap();
        let cellscript::ir::IrType::Bounded(collection) = &action.params[0].ty else { panic!("missing bounded family") };
        assert_eq!(
            collection.element_type,
            cellscript::ir::IrType::Tuple(vec![
                cellscript::ir::IrType::U64,
                cellscript::ir::IrType::Array(Box::new(cellscript::ir::IrType::U8), 2)
            ])
        );
        assert!(action.body.bounded_collection_ops[0].runtime_contract.is_none());
        assert_eq!(result.metadata.runtime.collection_instantiations[0].element_width_bytes, 10);
        let rejected = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { edition, ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert_eq!(rejected.code, Some(cellscript::error::DiagnosticCode::E2105));
    }
}

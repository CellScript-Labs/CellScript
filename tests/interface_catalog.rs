//! All actual candidate bundles are checked before any catalog evidence exists.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use cellscript_artifact_checker::interface::{check_module_catalog, ModuleBundle};
use cellscript_artifact_checker::{CheckerBudgets, CheckerRejectionCode};
use serde_json::Value;

const SOURCE: &str = "module catalog_coherence\npublic struct Value has copy, drop, store { amount: u64 }\naction verify(witness amount: u64) { verification require amount > 0 }\n";
fn compile(source: &str, opt_level: u8) -> [Vec<u8>; 4] {
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { source_contracts: true, opt_level, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    [
        compiled.artifact_bytes,
        serde_json::to_vec(&compiled.metadata).unwrap(),
        serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
    ]
}
fn borrow(bundle: &[Vec<u8>; 4]) -> ModuleBundle<'_> {
    std::array::from_fn(|index| bundle[index].as_slice())
}

#[test]
fn every_actual_candidate_is_inspected_and_byte_frozen() {
    let required = compile(SOURCE, 0);
    let added = compile(&format!("{SOURCE}public struct Extra has copy, drop, store {{ flag: u8 }}\n"), 3);
    let candidates = [borrow(&required), borrow(&added)];
    let catalog = check_module_catalog(borrow(&required), &candidates, &CheckerBudgets::default()).unwrap();
    assert_eq!(catalog.candidates().len(), 2);
    assert!(!catalog.required().artifact_report().semantic_equivalence_claimed);
    assert!(catalog.candidates().iter().all(|candidate| !candidate.artifact_report().semantic_equivalence_claimed));
    catalog.check_unchanged_inputs(borrow(&required), &candidates).unwrap();
    assert!(catalog.check_unchanged_inputs(borrow(&required), &[candidates[1], candidates[0]]).is_err());
    assert!(catalog.check_unchanged_inputs(borrow(&added), &candidates).is_err());
    assert!(catalog.check_unchanged_inputs(borrow(&required), &candidates[..1]).is_err());
    let mut changed = added.clone();
    changed[1].push(b' ');
    // JSON whitespace does not change the API, but it changes frozen input bytes.
    check_module_catalog(borrow(&required), &[borrow(&changed)], &CheckerBudgets::default()).unwrap();
    assert!(catalog.check_unchanged_inputs(borrow(&required), &[candidates[0], borrow(&changed)]).is_err());
    let value: Value = serde_json::from_slice(&catalog.canonical_bytes().unwrap()).unwrap();
    assert_eq!(value["schema"], "cellscript-checked-module-catalog-v1");
    assert_eq!(value["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(
        catalog.identity(),
        check_module_catalog(borrow(&required), &candidates, &CheckerBudgets::default()).unwrap().identity()
    );
}

#[test]
fn an_invalid_unselected_candidate_never_receives_partial_coherence_evidence() {
    let required = compile(SOURCE, 0);
    for field in 0..4 {
        let mut bad = required.clone();
        bad[field] = b"invalid input".to_vec();
        for candidates in [[borrow(&required), borrow(&bad)], [borrow(&bad), borrow(&required)]] {
            assert!(check_module_catalog(borrow(&required), &candidates, &CheckerBudgets::default()).is_err(), "field={field}");
        }
    }
    let broken_layout = compile(&SOURCE.replace("amount: u64 }", "amount: u32 }"), 0);
    assert!(check_module_catalog(borrow(&required), &[borrow(&required), borrow(&broken_layout)], &CheckerBudgets::default()).is_err());
    let missing_evidence =
        cellscript::compile(SOURCE, CompileOptions { target: Some("riscv64-elf".into()), ..CompileOptions::default() }).unwrap();
    let bundle = [
        missing_evidence.artifact_bytes,
        serde_json::to_vec(&missing_evidence.metadata).unwrap(),
        serde_json::to_vec(missing_evidence.verified_lowering_record.as_ref().unwrap()).unwrap(),
        serde_json::to_vec(missing_evidence.source_artifact_map.as_ref().unwrap()).unwrap(),
    ];
    assert!(check_module_catalog(borrow(&required), &[borrow(&required), borrow(&bundle)], &CheckerBudgets::default()).is_err());
}

#[test]
fn all_bundle_bytes_and_counts_are_bounded_before_any_parser_runs() {
    let empty = [&[][..]; 4];
    for candidates in [vec![], vec![empty; 33]] {
        assert_eq!(
            check_module_catalog(empty, &candidates, &CheckerBudgets::default()).unwrap_err().code,
            CheckerRejectionCode::V2400BudgetExceeded
        );
    }
    let large = vec![0; 4 * 1024 * 1024];
    // Individually bounded, but the total includes required AND all candidates.
    let tuple = [&large[..], &large[..], &[][..], &[][..]];
    assert_eq!(
        check_module_catalog(tuple, &[tuple, tuple], &CheckerBudgets::default()).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    let exact_tuple = [&large[..]; 4];
    let direct =
        cellscript_artifact_checker::interface::project_bundle(empty[0], empty[1], empty[2], empty[3], &CheckerBudgets::default())
            .unwrap_err();
    let exact = check_module_catalog(empty, &[exact_tuple], &CheckerBudgets::default()).unwrap_err();
    assert_eq!(exact.code, direct.code);
    assert_eq!(exact.message, direct.message, "exactly 16 MiB reaches the required bundle parser");
    let one_byte = [0u8];
    assert_eq!(
        check_module_catalog(
            empty,
            &[exact_tuple, [&one_byte, &[], &[], &[]]],
            &CheckerBudgets { instructions: u64::MAX, ..CheckerBudgets::default() }
        )
        .unwrap_err()
        .code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    assert_eq!(
        check_module_catalog(empty, &[[&oversized, &[], &[], &[]]], &CheckerBudgets::default()).unwrap_err().code,
        CheckerRejectionCode::V2400BudgetExceeded
    );
    let valid = compile(SOURCE, 0);
    assert!(check_module_catalog(borrow(&valid), &[borrow(&valid)], &CheckerBudgets { instructions: 1, ..CheckerBudgets::default() })
        .is_err());
}

#[test]
fn all_bounded_cardinalities_allow_reused_code_without_claiming_duplicate_receipts() {
    let bundle = compile(SOURCE, 0);
    for count in 1..=32 {
        // One checked code bundle may back distinct concrete Script args later.
        // This layer cannot classify duplicate/conflicting deployment receipts.
        let candidates = vec![borrow(&bundle); count];
        let catalog = check_module_catalog(borrow(&bundle), &candidates, &CheckerBudgets::default()).unwrap();
        assert_eq!(catalog.candidates().len(), count);
        catalog.check_unchanged_inputs(borrow(&bundle), &candidates).unwrap();
    }
}

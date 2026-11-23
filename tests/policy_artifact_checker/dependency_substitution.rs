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
    opt: cellscript::CompilerDiagnostics,
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
                "{opt} axis {axis}"
            );
        }
    }
}
#[test]
fn direct_code_dependency_preflights_all_snapshot_inputs_before_parsing() {
    use cellscript_artifact_checker::code_origin::check_direct_code_dependency;
    let (target, raw, cells) = direct_dependency_fixture(cellscript::CompilerDiagnostics::without_optimizations());
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut contract = declaration();
        contract.actions.retain(|action| action.action == "burn");
        contract.common_checks.clear();
        let fixture = Fixture::new_source_with_target(EXTERNAL_SOURCE, CellScriptEdition::Edition2027, opt, contract, "ckb-type-hash");
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
    let (target, _raw, mut cells) = direct_dependency_fixture(cellscript::CompilerDiagnostics::without_optimizations());
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

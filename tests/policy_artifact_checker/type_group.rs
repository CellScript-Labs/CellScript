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
fn type_group_fixture(opt: cellscript::CompilerDiagnostics) -> TypeGroupFixture {
    let mut args = b"CSARGv1\0".to_vec();
    args.extend_from_slice(&7u64.to_le_bytes());
    type_group_fixture_with(EXTERNAL_SOURCE, opt, args)
}
fn type_group_fixture_with(source: &str, opt: cellscript::CompilerDiagnostics, args: Vec<u8>) -> TypeGroupFixture {
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
            assert!(check_type_group(&f).is_err(), "{opt} axis {axis}");
        }
    }
}
#[test]
fn direct_type_group_rejects_unknown_ambiguous_misplaced_or_wrong_width_policy_records() {
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
            assert!(check_type_group(&f).is_err(), "{opt} axis {axis}");
        }
    }
}

#[test]
fn direct_type_group_preflights_complete_shared_inputs_and_rejects_substitution() {
    use cellscript_artifact_checker::code_origin::{
        check_direct_code_dependency, check_direct_type_group, SuppliedTypeGroupTransaction,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let f = type_group_fixture(cellscript::CompilerDiagnostics::without_optimizations());
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
    let foreign =
        fixed_policy_receipt(&external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::without_optimizations()), vec![18])
            .unwrap();
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
        assert!(check_type_group(&f).is_err(), "malformed final unselected input {opt}");
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (ty, width) in [("u8", 1), ("u16", 2), ("u32", 4), ("u64", 8), ("unit", 0)] {
            let source = if width == 0 {
                EXTERNAL_SOURCE.replace(", witness value: u64", "").replace(" require value > 0;", "")
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
            assert!(check_type_group(&f).is_err(), "{opt} {ty} wrong checked args length");
        }
    }
}

#[test]
fn direct_type_group_rejects_actual_receipt_bundle_byte_substitution() {
    use cellscript_artifact_checker::code_origin::{
        check_direct_code_dependency, check_direct_type_group, SuppliedTypeGroupTransaction,
    };
    use ckb_testtool::ckb_types::prelude::*;
    let f = type_group_fixture(cellscript::CompilerDiagnostics::without_optimizations());
    let fixture = external_fixture(EXTERNAL_SOURCE, cellscript::CompilerDiagnostics::without_optimizations());
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
fn certified_frame_copies_pin_the_bounded_copy_helper_store_sequence() {
    const NESTED_SOURCE: &str = r#"
module nested_codec_copy_lifetime;
resource Token has store, consume { amount: u64 }
public struct Inner { first: u32, second: u32 }
public struct Outer { meta: Inner, count: u64 }
action burn(input token: Token, witness value: u64) {
    let outer: Outer = new Outer { meta: new Inner { first: 1, second: 2 }, count: 3 };
    require token.amount > 0;
    require outer.count > 0;
    require value > 0;
    consume token;
}
"#;
    let fixture = external_fixture(NESTED_SOURCE, cellscript::CompilerDiagnostics::default());
    fixed_external_codec(&fixture).unwrap();
    let memcpy_entry = fixture.record.entries.iter().find(|entry| entry.name == "__cellscript_memcpy_fixed").unwrap();
    let memcpy_start = fixture.record.blocks.iter().find(|block| block.id == memcpy_entry.entry_block).unwrap().range.start;
    // The helper's only store is `sb t0, 0(a1)`. Widening it to an unbounded
    // `sd t0, 0(a1)` keeps every structural frame rule (contiguous range, no
    // calls, no RA/SP writes, one return) while writing past the certified
    // [destination, destination + length) window on every iteration. The
    // generic bundle check has no fixed-result copy in this artifact, so the
    // field certificate itself must pin the exact store sequence.
    let mut changed = fixture.clone();
    changed.replace_machine_word(memcpy_start + 8, 0x00553023);
    changed.check().unwrap();
    let error = fixed_external_codec(&changed).unwrap_err();
    assert!(error.to_string().contains("bounded byte copy helper changed"), "accepted widened helper store: {error}");
    let mut restored = changed.clone();
    restored.replace_machine_word(memcpy_start + 8, 0x00558023);
    fixed_external_codec(&restored).unwrap();
}

#[test]
fn nested_public_layouts_admit_end_to_end_with_certified_frame_copies() {
    const NESTED_SOURCE: &str = r#"
module nested_codec;
resource Token has store, consume { amount: u64, }
public struct Inner { first: u32, second: u32, }
public struct Outer { meta: Inner, count: u64, }
action burn(input token: Token, witness value: u64) {

    let outer: Outer = new Outer { meta: new Inner { first: 1, second: 2, }, count: 3, };
    require token.amount > 0;
    require outer.count > 0;
    require value > 0;
    consume token;
}
"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let fixture = external_fixture(&format!("{EXTERNAL_SOURCE}\npublic const LIMIT: u64 = 3 * 7;"), opt);
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
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let baseline = Fixture::new_source_with(
            &format!("{SOURCE}\npublic struct Snapshot {{ z: u64 }}"),
            CellScriptEdition::Edition2027,
            diagnostics,
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
        assert!(error.message.contains("reserved open-handle class name"), "opt={diagnostics}: {error}");
    }
}

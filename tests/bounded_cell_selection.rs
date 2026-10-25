//! Host contract evidence only; VM, checker and builder integration remain required.
#[path = "support/cell_selection_reference.rs"]
mod reference;

#[path = "support/cell_selection_transaction.rs"]
mod transaction;

use reference::*;

fn contract() -> Contract {
    Contract {
        roles: vec![
            Role {
                type_hash: [1; 32],
                lock_hash: [11; 32],
                schema: [21; 32],
                width: 16,
                key_offset: 0,
                key_width: 8,
                inputs: Count { min: 0, max: 32 },
                outputs: Count { min: 0, max: 32 },
                access: Access::LocalType,
                correspondence: Correspondence::Preserve,
            },
            Role {
                type_hash: [2; 32],
                lock_hash: [12; 32],
                schema: [22; 32],
                width: 24,
                key_offset: 8,
                key_width: 8,
                inputs: Count { min: 0, max: 32 },
                outputs: Count { min: 0, max: 32 },
                access: Access::Observe,
                correspondence: Correspondence::Preserve,
            },
        ],
        input_scan: 64,
        output_scan: 64,
        selected_per_side: 32,
        aggregate_bytes: 32768,
    }
}

fn current() -> CurrentScript {
    CurrentScript { role: ScriptRole::Type, hash: [1; 32] }
}

fn cell(contract: &Contract, role: usize, key: u64) -> Cell {
    let role = &contract.roles[role];
    let mut data = vec![0; role.width];
    data[role.key_offset..role.key_offset + 8].copy_from_slice(&key.to_le_bytes());
    Cell { type_hash: Some(role.type_hash), lock_hash: role.lock_hash, data }
}

fn fee() -> Cell {
    Cell { type_hash: None, lock_hash: [11; 32], data: vec![] }
}

fn proposal(selection: &Selection) -> Vec<Proposed> {
    selection.cells.iter().map(|cell| Proposed { side: cell.side, index: cell.index, role: cell.role }).collect()
}

#[test]
fn mixed_roles_keep_nominal_schema_and_actual_global_and_group_coordinates() {
    let contract = contract();
    let inputs = vec![fee(), cell(&contract, 1, 9), cell(&contract, 0, 9), cell(&contract, 0, 3)];
    let outputs = vec![cell(&contract, 0, 3), fee(), cell(&contract, 1, 9), cell(&contract, 0, 9)];
    let selected = select(&contract, current(), &inputs, &outputs).unwrap();
    assert_eq!(
        selected.cells.iter().map(|c| (c.side, c.index, c.group_index, c.role)).collect::<Vec<_>>(),
        vec![
            (Side::Input, 1, None, 1),
            (Side::Input, 2, Some(0), 0),
            (Side::Input, 3, Some(1), 0),
            (Side::Output, 0, Some(0), 0),
            (Side::Output, 2, None, 1),
            (Side::Output, 3, Some(1), 0),
        ]
    );
    assert_eq!(
        selected.pairs,
        vec![
            Pair { role: 0, inputs: vec![2], outputs: vec![3] },
            Pair { role: 0, inputs: vec![3], outputs: vec![0] },
            Pair { role: 1, inputs: vec![1], outputs: vec![2] },
        ]
    );
    for selected_cell in &selected.cells {
        assert_eq!(selected_cell.schema, contract.roles[selected_cell.role].schema);
        assert_eq!(selected_cell.access, contract.roles[selected_cell.role].access);
    }
    check_proposal(&selected, &inputs, &outputs, &proposal(&selected)).unwrap();
}

#[test]
fn full_membership_cannot_be_replaced_by_a_witness_index_list() {
    let contract = contract();
    let cells = vec![cell(&contract, 0, 1), fee(), cell(&contract, 1, 2)];
    let selected = select(&contract, current(), &cells, &cells).unwrap();
    let valid = proposal(&selected);
    for index in 0..valid.len() {
        let mut omitted = valid.clone();
        omitted.remove(index);
        assert_eq!(check_proposal(&selected, &cells, &cells, &omitted), Err(Error::ProposalMismatch));
        let mut duplicate = valid.clone();
        duplicate.push(valid[index]);
        assert_eq!(check_proposal(&selected, &cells, &cells, &duplicate), Err(Error::DuplicateIndex));
        let mut wrong_role = valid.clone();
        wrong_role[index].role ^= 1;
        assert_eq!(check_proposal(&selected, &cells, &cells, &wrong_role), Err(Error::ProposalMismatch));
    }
    let mut inserted_fee = valid.clone();
    inserted_fee.insert(1, Proposed { side: Side::Input, index: 1, role: 0 });
    assert_eq!(check_proposal(&selected, &cells, &cells, &inserted_fee), Err(Error::ProposalMismatch));
    let mut wrong_source = valid.clone();
    wrong_source[0].side = Side::Output;
    assert_eq!(check_proposal(&selected, &cells, &cells, &wrong_source), Err(Error::DuplicateIndex));
    let mut wrong_index = valid.clone();
    wrong_index[0].index = usize::MAX;
    assert_eq!(check_proposal(&selected, &cells, &cells, &wrong_index), Err(Error::ProposalIndex));
    let mut reordered = valid.clone();
    reordered.swap(0, 1);
    assert_eq!(check_proposal(&selected, &cells, &cells, &reordered), Err(Error::ProposalMismatch));
    assert_eq!(check_proposal(&selected, &cells, &cells, &vec![valid[0]; 65]), Err(Error::ProposalBound));
}

#[test]
fn protected_type_with_wrong_custody_or_layout_is_never_treated_as_change() {
    let contract = contract();
    let input = cell(&contract, 0, 1);
    let mut changed = input.clone();
    changed.lock_hash = [99; 32];
    assert_eq!(select(&contract, current(), std::slice::from_ref(&input), &[changed]), Err(Error::LockIdentity));
    let mut malformed = input.clone();
    malformed.data.push(0);
    assert_eq!(select(&contract, current(), std::slice::from_ref(&input), &[malformed]), Err(Error::Layout));
    let mut wrong_role = input.clone();
    wrong_role.type_hash = Some(contract.roles[1].type_hash);
    wrong_role.lock_hash = contract.roles[1].lock_hash;
    assert_eq!(select(&contract, current(), std::slice::from_ref(&input), &[wrong_role]), Err(Error::Layout));
    let mut erased_type = input.clone();
    erased_type.type_hash = None;
    assert_eq!(select(&contract, current(), &[input], &[erased_type]), Err(Error::Correspondence));
}

#[test]
fn duplicate_keys_overlap_and_shared_successors_reject() {
    let contract = contract();
    let first = cell(&contract, 0, 1);
    let second = cell(&contract, 0, 2);
    assert_eq!(select(&contract, current(), &[first.clone(), first.clone()], std::slice::from_ref(&first)), Err(Error::DuplicateKey));
    assert_eq!(select(&contract, current(), std::slice::from_ref(&first), &[first.clone(), first.clone()]), Err(Error::DuplicateKey));
    assert_eq!(
        select(&contract, current(), &[first.clone(), second.clone()], std::slice::from_ref(&first)),
        Err(Error::Correspondence)
    );
    assert_eq!(
        select(&contract, current(), std::slice::from_ref(&first), &[first.clone(), second.clone()]),
        Err(Error::Correspondence)
    );
    assert_eq!(select(&contract, current(), &[first], &[second]), Err(Error::Correspondence));
    let mut overlapping = contract.clone();
    overlapping.roles[1].type_hash = overlapping.roles[0].type_hash;
    assert_eq!(select(&overlapping, current(), &[], &[]), Err(Error::AmbiguousRole));
}

#[test]
fn identical_widths_and_keys_cannot_discharge_another_roles_obligation() {
    let mut contract = contract();
    contract.roles[1].width = contract.roles[0].width;
    contract.roles[1].key_offset = contract.roles[0].key_offset;
    contract.roles[1].lock_hash = contract.roles[0].lock_hash;
    let first = cell(&contract, 0, 7);
    let other_role = cell(&contract, 1, 7);
    assert_eq!(first.data, other_role.data);
    assert_eq!(first.lock_hash, other_role.lock_hash);
    assert_ne!(contract.roles[0].schema, contract.roles[1].schema);
    assert_eq!(select(&contract, current(), &[first], &[other_role]), Err(Error::Correspondence));
}

#[test]
fn scan_selection_bytes_counts_and_late_invalid_cells_have_independent_bounds() {
    let mut contract = contract();
    assert!(select(&contract, current(), &[], &[]).is_ok());
    let maximum = (0..32).map(|key| cell(&contract, (key % 2) as usize, key)).collect::<Vec<_>>();
    let mut scanned = vec![fee(); 32];
    scanned.extend(maximum.clone());
    assert_eq!(select(&contract, current(), &scanned, &scanned).unwrap().cells.len(), 64);
    let mut over_scan = scanned.clone();
    over_scan.push(fee());
    assert_eq!(select(&contract, current(), &over_scan, &scanned), Err(Error::ScanBound));
    assert_eq!(select(&contract, current(), &scanned, &over_scan), Err(Error::ScanBound));
    let mut over_selected = maximum.clone();
    over_selected.push(cell(&contract, 0, 99));
    assert_eq!(select(&contract, current(), &over_selected, &maximum), Err(Error::SelectedBound));
    let mut late = scanned.clone();
    late[63].lock_hash = [99; 32];
    assert_eq!(select(&contract, current(), &scanned, &late), Err(Error::LockIdentity));
    contract.aggregate_bytes = maximum.iter().map(|cell| cell.data.len()).sum::<usize>() * 2;
    assert!(select(&contract, current(), &scanned, &scanned).is_ok());
    contract.aggregate_bytes -= 1;
    assert_eq!(select(&contract, current(), &scanned, &scanned), Err(Error::ByteBound));
    contract.aggregate_bytes = 32768;
    contract.roles[0].inputs.min = 1;
    assert_eq!(select(&contract, current(), &[], &[]), Err(Error::Cardinality));
    contract.roles[0].inputs.min = 0;
    contract.roles[1].outputs.max = 15;
    assert_eq!(select(&contract, current(), &scanned, &scanned), Err(Error::Cardinality));
}

#[test]
fn maximum_catalog_and_encoded_bytes_are_accepted_without_weakening_nominal_roles() {
    let mut contract = contract();
    let prototype = contract.roles[0].clone();
    contract.roles = (0..8)
        .map(|index| Role {
            type_hash: [index + 1; 32],
            schema: [index + 21; 32],
            width: 512,
            key_offset: 480,
            key_width: 32,
            access: Access::Observe,
            ..prototype.clone()
        })
        .collect();
    let mut cells = vec![fee(); 32];
    cells.extend((0..32).map(|key| cell(&contract, key as usize % 8, key)));
    let selected = select(&contract, current(), &cells, &cells).unwrap();
    assert_eq!(selected.cells.len(), 64);
    assert_eq!(selected.pairs.len(), 32);
    assert_eq!(cells.iter().map(|cell| cell.data.len()).sum::<usize>() * 2, 32768);
    let mut truncated = cells.clone();
    truncated.last_mut().unwrap().data.pop();
    assert_eq!(select(&contract, current(), &cells, &truncated), Err(Error::Layout));
    contract.roles.push(prototype);
    assert_eq!(select(&contract, current(), &[], &[]), Err(Error::Contract));
}

#[test]
fn create_burn_and_merge_are_explicit_and_do_not_infer_conservation() {
    let mut contract = contract();
    let first = cell(&contract, 0, 1);
    let second = cell(&contract, 0, 2);
    contract.roles[0].correspondence = Correspondence::Create;
    assert!(select(&contract, current(), &[], std::slice::from_ref(&first)).is_ok());
    assert_eq!(select(&contract, current(), std::slice::from_ref(&first), std::slice::from_ref(&first)), Err(Error::Correspondence));
    assert_eq!(select(&contract, current(), &[], &[]), Err(Error::Correspondence));
    contract.roles[0].correspondence = Correspondence::Burn;
    assert!(select(&contract, current(), std::slice::from_ref(&first), &[]).is_ok());
    assert_eq!(select(&contract, current(), std::slice::from_ref(&first), std::slice::from_ref(&first)), Err(Error::Correspondence));
    contract.roles[0].correspondence = Correspondence::Merge { target_key: 9u64.to_le_bytes().to_vec() };
    let target = cell(&contract, 0, 9);
    let selected = select(&contract, current(), &[first.clone(), second.clone()], std::slice::from_ref(&target)).unwrap();
    assert_eq!(selected.pairs, vec![Pair { role: 0, inputs: vec![0, 1], outputs: vec![0] }]);
    assert_eq!(select(&contract, current(), std::slice::from_ref(&first), &[target]), Err(Error::Correspondence));
    assert_eq!(select(&contract, current(), &[first.clone(), second], &[first]), Err(Error::Correspondence));
}

#[test]
fn foreign_observation_never_becomes_local_type_authority_or_peer_success() {
    let mut contract = contract();
    let lock = CurrentScript { role: ScriptRole::Lock, hash: contract.roles[0].lock_hash };
    assert_eq!(select(&contract, lock, &[], &[]), Err(Error::Authority));
    contract.roles[0].access = Access::Observe;
    let cells = vec![fee(), cell(&contract, 0, 1), cell(&contract, 1, 2)];
    let selected = select(&contract, lock, &cells, &cells).unwrap();
    assert_eq!(selected.cells[0].group_index, Some(1));
    assert!(selected.cells.iter().all(|cell| cell.access == Access::Observe));
    assert!(selected.cells.iter().filter(|cell| cell.side == Side::Output).all(|cell| cell.group_index.is_none()));
    contract.roles[1].access = Access::LocalType;
    assert_eq!(select(&contract, current(), &cells, &cells), Err(Error::Authority));
    // A successful observation carries no executed-peer receipt or granted lifecycle resource.
}

#[test]
fn contract_rejects_invalid_shapes_before_examining_cells() {
    for mutate in [
        |c: &mut Contract| c.input_scan = 65,
        |c: &mut Contract| c.output_scan = 0,
        |c: &mut Contract| c.selected_per_side = 33,
        |c: &mut Contract| c.aggregate_bytes = 32769,
        |c: &mut Contract| c.roles[0].width = 513,
        |c: &mut Contract| c.roles[0].key_width = 33,
        |c: &mut Contract| c.roles[0].key_offset = usize::MAX,
        |c: &mut Contract| c.roles[0].inputs.min = 33,
        |c: &mut Contract| c.roles[0].correspondence = Correspondence::Merge { target_key: vec![0] },
        |c: &mut Contract| c.roles.clear(),
    ] {
        let mut contract = contract();
        mutate(&mut contract);
        assert_eq!(select(&contract, current(), &[], &[]), Err(Error::Contract));
    }
}

fn permutations(items: &[Cell]) -> Vec<Vec<Cell>> {
    if items.is_empty() {
        return vec![vec![]];
    }
    let mut result = Vec::new();
    for index in 0..items.len() {
        let mut remaining = items.to_vec();
        let first = remaining.remove(index);
        for mut tail in permutations(&remaining) {
            tail.insert(0, first.clone());
            result.push(tail);
        }
    }
    result
}

#[test]
fn every_transaction_permutation_preserves_logical_pairs_after_global_reindexing() {
    let contract = contract();
    let cells = vec![fee(), cell(&contract, 0, 9), cell(&contract, 0, 3), cell(&contract, 1, 9)];
    let permutations = permutations(&cells);
    assert_eq!(permutations.len(), 24);
    for inputs in &permutations {
        for outputs in &permutations {
            let selection = select(&contract, current(), inputs, outputs).unwrap();
            assert_eq!(selection.pairs.len(), 3);
            let mut used_outputs = Vec::new();
            for pair in &selection.pairs {
                let role = &contract.roles[pair.role];
                let key = role.key_offset..role.key_offset + role.key_width;
                assert_eq!(inputs[pair.inputs[0]].data[key.clone()], outputs[pair.outputs[0]].data[key]);
                assert!(!used_outputs.contains(&pair.outputs[0]));
                used_outputs.push(pair.outputs[0]);
            }
            check_proposal(&selection, inputs, outputs, &proposal(&selection)).unwrap();
        }
    }
}

//! Differential checks against the independent fixed-layout host model.
use super::*;
use cellscript_artifact_checker::cell_selection::{self as checked, transaction as host};
use cellscript_artifact_checker::{TypedSemanticField, TypedSemanticType};

fn typed(contract: &Contract) -> (checked::Contract, Vec<TypedSemanticType>) {
    let roles = contract
        .roles
        .iter()
        .enumerate()
        .map(|(index, role)| checked::Role {
            type_name: format!("Role{index}"),
            type_hash: role.type_hash,
            lock_hash: role.lock_hash,
            identity_field: "id".into(),
            inputs: (role.inputs.min as u32, role.inputs.max as u32),
            outputs: (role.outputs.min as u32, role.outputs.max as u32),
            local_type: role.access == Access::LocalType,
            correspondence: match &role.correspondence {
                Correspondence::Preserve => checked::Correspondence::Preserve,
                Correspondence::Create => checked::Correspondence::Create,
                Correspondence::Burn => checked::Correspondence::Burn,
                Correspondence::Merge { target_key } => checked::Correspondence::Merge(target_key.clone()),
            },
        })
        .collect();
    let types = contract
        .roles
        .iter()
        .enumerate()
        .map(|(index, role)| TypedSemanticType {
            name: format!("Role{index}"),
            kind: "resource".into(),
            encoded_size: Some(role.width as u32),
            fields: (0..role.width / 8)
                .map(|field| TypedSemanticField {
                    name: if field * 8 == role.key_offset { "id".into() } else { format!("field{field}") },
                    ty: "u64".into(),
                    offset: (field * 8) as u32,
                    width_bytes: Some(8),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
        .collect();
    (
        checked::Contract {
            profile: checked::PROFILE.into(),
            names: None,
            roles,
            input_scan: contract.input_scan as u32,
            output_scan: contract.output_scan as u32,
            selected_per_side: contract.selected_per_side as u32,
            aggregate_bytes: contract.aggregate_bytes as u32,
        },
        types,
    )
}

fn compare(contract: &Contract, current: CurrentScript, inputs: &[Cell], outputs: &[Cell]) {
    let expected = select(contract, current, inputs, outputs);
    let (policy, types) = typed(contract);
    let views = |cells: &[Cell]| cells.iter().map(|cell| (cell.type_hash, cell.lock_hash)).collect::<Vec<_>>();
    let input_ids = views(inputs);
    let output_ids = views(outputs);
    let input = input_ids
        .iter()
        .zip(inputs)
        .map(|((type_hash, lock_hash), cell)| host::Cell { type_hash: *type_hash, lock_hash: *lock_hash, data: &cell.data })
        .collect::<Vec<_>>();
    let output = output_ids
        .iter()
        .zip(outputs)
        .map(|((type_hash, lock_hash), cell)| host::Cell { type_hash: *type_hash, lock_hash: *lock_hash, data: &cell.data })
        .collect::<Vec<_>>();
    let actual = host::select(
        &policy,
        &types,
        host::CurrentScript { hash: current.hash, is_type: current.role == ScriptRole::Type },
        &input,
        &output,
    );
    match (expected, actual) {
        (Ok(expected), Ok(actual)) => {
            assert_eq!(
                actual.aggregate_bytes as usize,
                inputs
                    .iter()
                    .chain(outputs)
                    .filter(|cell| contract.roles.iter().any(|role| Some(role.type_hash) == cell.type_hash))
                    .map(|cell| cell.data.len())
                    .sum::<usize>()
            );
            let expected_cells = expected
                .cells
                .iter()
                .map(|cell| (cell.side == Side::Input, cell.index, cell.group_index, cell.role, cell.key.clone()))
                .collect::<Vec<_>>();
            let actual_cells = actual
                .cells
                .iter()
                .map(|cell| {
                    (
                        cell.side == host::Side::Input,
                        cell.index as usize,
                        cell.group_index.map(|i| i as usize),
                        cell.role as usize,
                        cell.key.clone(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_cells, expected_cells);
            let expected_pairs =
                expected.pairs.iter().map(|pair| (pair.role, pair.inputs.clone(), pair.outputs.clone())).collect::<Vec<_>>();
            let actual_pairs = actual
                .pairs
                .iter()
                .map(|pair| {
                    (
                        pair.role as usize,
                        pair.inputs.iter().map(|i| *i as usize).collect(),
                        pair.outputs.iter().map(|i| *i as usize).collect(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_pairs, expected_pairs);
        }
        (Err(expected), Err(actual)) => {
            let error = match expected {
                Error::Authority => host::Error::Authority,
                Error::ScanBound => host::Error::ScanBound,
                Error::LockIdentity => host::Error::Custody,
                Error::Layout => host::Error::Layout,
                Error::SelectedBound => host::Error::SelectedBound,
                Error::ByteBound => host::Error::ByteBound,
                Error::DuplicateKey => host::Error::DuplicateIdentity,
                Error::Cardinality => host::Error::Cardinality,
                Error::Correspondence => host::Error::Correspondence,
                other => panic!("unexpected reference error {other:?}"),
            };
            assert_eq!(actual, error);
        }
        (expected, actual) => panic!("reference={expected:?}, host={actual:?}"),
    }
}

#[test]
fn host_reconstruction_matches_independent_membership_and_correspondence_model() {
    for count in [0, 1, 2, 8, 16, 32] {
        let policy = contract();
        let mut inputs = vec![fee()];
        inputs.extend((0..count).map(|id| cell(&policy, id as usize % 2, id)));
        let mut outputs = inputs.clone();
        outputs.reverse();
        compare(&policy, current(), &inputs, &outputs);
        let mut lock_policy = policy.clone();
        lock_policy.roles.iter_mut().for_each(|role| role.access = Access::Observe);
        compare(&lock_policy, CurrentScript { role: ScriptRole::Lock, hash: [11; 32] }, &inputs, &outputs);
        for mutation in 0..7 {
            let mut altered = outputs.clone();
            let mut contract = policy.clone();
            match mutation {
                0 => altered.push(cell(&policy, 0, 42)),
                1 => altered.push(cell(&policy, 0, 0)),
                2 => {
                    let mut wrong = cell(&policy, 1, 42);
                    wrong.lock_hash = [99; 32];
                    altered.push(wrong);
                }
                3 => {
                    let mut wrong = cell(&policy, 1, 42);
                    wrong.data.push(0);
                    altered.push(wrong);
                }
                4 => contract.selected_per_side = 1,
                5 => contract.aggregate_bytes = 16,
                6 => contract.output_scan = 1,
                _ => unreachable!(),
            }
            compare(&contract, current(), &inputs, &altered);
        }
    }
    for correspondence in
        [Correspondence::Create, Correspondence::Burn, Correspondence::Merge { target_key: 9u64.to_le_bytes().to_vec() }]
    {
        let mut policy = contract();
        policy.roles[0].correspondence = correspondence;
        for inputs in [vec![], vec![cell(&policy, 0, 1)], vec![cell(&policy, 0, 1), cell(&policy, 0, 2)]] {
            for outputs in [vec![], vec![cell(&policy, 0, 9)], vec![cell(&policy, 0, 1), cell(&policy, 0, 2)]] {
                compare(&policy, current(), &inputs, &outputs);
            }
        }
    }
}

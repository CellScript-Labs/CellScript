//! Full-fill exchange over exact roles, with independently executing asset Types.
use super::*;

const SOURCE: &str = include_str!("../../../../../examples/language/batches/mixed_settlement.cell");
const PEER: &str = include_str!("../../../../../tests/fixtures/mixed_settlement_asset.cell");

fn compile_settlement(opt_level: u8, named: bool) -> crate::CompileResult {
    let mut source =
        if named { SOURCE } else { include_str!("../../../../../tests/fixtures/mixed_settlement_positional.cell") }.to_string();
    for (placeholder, tag) in [("11", 1), ("22", 2), ("33", 3), ("44", 4)] {
        source = source.replace(&placeholder.repeat(32), &hex::encode(hash(&script(&code_identity(tag), 0))));
    }
    let ast = crate::frontend::parse(&source, crate::NEXT_EDITION).unwrap();
    let formatted = crate::fmt::format_default(&ast).unwrap();
    assert_eq!(crate::fmt::format_default(&crate::frontend::parse(&formatted, crate::NEXT_EDITION).unwrap()).unwrap(), formatted);
    let compiled = crate::compile(
        &source,
        crate::CompileOptions { edition: crate::NEXT_EDITION, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap_or_else(|error| panic!("settlement O{opt_level}: {error}"));
    for blocker in ["cell-data-view-artifact-proof", "exact-role-selection-artifact-proof"] {
        assert!(compiled.metadata.runtime.fail_closed_runtime_features.iter().any(|feature| feature == blocker));
    }
    compiled
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    None,
    Debit,
    Credit,
    Owner,
    Terms,
    AlreadySettled,
    ZeroAmount,
    Insufficient,
    Overflow,
    Duplicate,
    Missing,
    Unmatched,
    Custody,
    Capacity,
    Schema,
    BasePeer,
    QuotePeer,
    LockReject,
    CrossAsset,
    PeerInflation,
    IdentityOverflow,
}

fn order_bytes(id: u64, give: u64, take: u64, settled: bool) -> Vec<u8> {
    [
        id.to_le_bytes().to_vec(),
        vec![0x51; 32],
        vec![0x72; 32],
        give.to_le_bytes().to_vec(),
        take.to_le_bytes().to_vec(),
        vec![u8::from(settled)],
    ]
    .concat()
}
fn balance_bytes(id: u64, owner: u8, amount: u64) -> Vec<u8> {
    [id.to_le_bytes().to_vec(), vec![owner; 32], amount.to_le_bytes().to_vec()].concat()
}

#[derive(Debug, serde::Serialize)]
struct Observation {
    cycles: u64,
    transaction_bytes: usize,
    witness_bytes: usize,
    settlement_elf_bytes: usize,
    asset_elf_bytes: usize,
    output_capacity: u64,
    occupied_output_capacity: u64,
    declared_frames: std::collections::BTreeMap<String, u32>,
    observed_stack_bytes: Option<u64>,
    observed_heap_bytes: Option<u64>,
    #[serde(skip)]
    transaction: Vec<u8>,
}

fn deploy_fixture(context: &mut Context, tag: u8, elf: Vec<u8>) {
    let point = packed::OutPoint::new_builder().tx_hash([0xd0 + tag; 32]).index(0u32).build();
    context.create_cell_with_out_point(
        point,
        packed::CellOutput::new_builder().capacity(100_000_000_000u64).type_(Some(code_identity(tag)).pack()).build(),
        Bytes::from(elf),
    );
}

fn run_settlement(
    compiled: &crate::CompileResult,
    peer: &crate::CompileResult,
    count: usize,
    mutation: Mutation,
    reverse: bool,
) -> std::result::Result<Observation, String> {
    let mut context = Context::new_with_deterministic_rng();
    let lock = script(&code_identity(4), 0);
    deploy_fixture(
        &mut context,
        1,
        if matches!(mutation, Mutation::PeerInflation) {
            exit_program(0)
        } else {
            crate::strip_vm_abi_trailer(&compiled.artifact_bytes).to_vec()
        },
    );
    for tag in [2, 3] {
        let reject = matches!((tag, mutation), (2, Mutation::BasePeer) | (3, Mutation::QuotePeer));
        deploy_fixture(
            &mut context,
            tag,
            if reject { exit_program(71) } else { crate::strip_vm_abi_trailer(&peer.artifact_bytes).to_vec() },
        );
    }
    deploy_fixture(&mut context, 4, exit_program(if matches!(mutation, Mutation::LockReject) { 72 } else { 0 }));
    let cell = |tag: Option<u8>| {
        packed::CellOutput::new_builder()
            .capacity(400_000_000_000u64)
            .lock(lock.clone())
            .type_(tag.map(|tag| script(&code_identity(tag), 0)).pack())
            .build()
    };
    // Unrelated fee/change data deliberately resembles a balance. No protected
    // Type, so it must never contribute to a settlement or conservation total.
    let mut inputs = vec![(cell(None), balance_bytes(0, 0x51, u64::MAX))];
    let mut outputs = vec![(cell(None), balance_bytes(1, 0x72, 0))];
    for ordinal in 0..count {
        let id = ordinal as u64;
        let late = ordinal + 1 == count;
        let give = if late && matches!(mutation, Mutation::ZeroAmount) { 0 } else { id * 3 + 7 };
        let take = id * 5 + 11;
        inputs.push((
            cell(Some(1)),
            order_bytes(
                if late && matches!(mutation, Mutation::IdentityOverflow) { u64::MAX } else { id },
                give,
                take,
                late && matches!(mutation, Mutation::AlreadySettled),
            ),
        ));
        outputs.push((
            cell(Some(1)),
            order_bytes(
                if late && matches!(mutation, Mutation::IdentityOverflow) { u64::MAX } else { id },
                give + u64::from(late && matches!(mutation, Mutation::Terms)),
                take,
                true,
            ),
        ));
        for (tag, quantity, from, to) in [(2, give, 0x51, 0x72), (3, take, 0x72, 0x51)] {
            let target = late && tag == 3;
            let debit = if target && matches!(mutation, Mutation::Insufficient) { quantity.saturating_sub(1) } else { 1000 + id };
            let credit = if target && matches!(mutation, Mutation::Overflow) { u64::MAX } else { 100 + id };
            let mut debit_id = id * 2;
            let mut credit_id = debit_id + 1;
            if target && matches!(mutation, Mutation::Unmatched) {
                debit_id += 100;
                credit_id += 100;
            }
            inputs.push((cell(Some(tag)), balance_bytes(debit_id, from, debit)));
            inputs.push((cell(Some(tag)), balance_bytes(credit_id, to, credit)));
            let outgoing_debit = debit.saturating_sub(quantity) + u64::from(target && matches!(mutation, Mutation::Debit));
            let outgoing_credit =
                credit.saturating_add(quantity) + u64::from(target && matches!(mutation, Mutation::Credit | Mutation::PeerInflation));
            let output_tag = if late && matches!(mutation, Mutation::CrossAsset) { 5 - tag } else { tag };
            outputs.push((cell(Some(output_tag)), balance_bytes(debit_id, from, outgoing_debit)));
            let output_id = if target && matches!(mutation, Mutation::Duplicate) { debit_id } else { credit_id };
            let owner = if target && matches!(mutation, Mutation::Owner) { from } else { to };
            let mut output = cell(Some(output_tag));
            if target && matches!(mutation, Mutation::Custody) {
                output = output.as_builder().lock(script(&code_identity(4), 1)).build();
            }
            if target && matches!(mutation, Mutation::Capacity) {
                output = output.as_builder().capacity(1u64).build();
            }
            let mut data = balance_bytes(output_id, owner, outgoing_credit);
            if target && matches!(mutation, Mutation::Schema) {
                data.push(0);
            }
            if !(target && matches!(mutation, Mutation::Missing)) {
                outputs.push((output, data));
            }
        }
    }
    if reverse {
        outputs.reverse();
    }
    let mut tx = TransactionBuilder::default();
    for (index, (cell, data)) in inputs.into_iter().enumerate() {
        let previous_output = packed::OutPoint::new_builder().tx_hash([0x90; 32]).index(index as u32).build();
        context.create_cell_with_out_point(previous_output.clone(), cell, Bytes::from(data));
        tx = tx.input(packed::CellInput::new_builder().previous_output(previous_output).build());
    }
    for (cell, data) in outputs {
        tx = tx.output(cell).output_data(Bytes::from(data).pack());
    }
    let witness = compiled.metadata.actions[0].entry_witness_args(&[]).unwrap();
    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(witness)).pack()).build();
    tx = tx.witness(Bytes::new().pack()).witness(witness.as_bytes().pack());
    let tx = context.complete_tx(tx.build());
    let cycles = context.verify_tx(&tx, 20_000_000).map_err(|error| format!("{error:?}"))?;
    let transaction = tx.data().as_slice().to_vec();
    let occupied_output_capacity = tx
        .outputs()
        .into_iter()
        .zip(tx.outputs_data())
        .map(|(cell, data)| {
            cell.occupied_capacity(ckb_testtool::ckb_types::core::Capacity::bytes(data.raw_data().len()).unwrap()).unwrap().as_u64()
        })
        .sum();
    let declared_frames = compiled
        .verified_lowering_record
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .filter(|entry| ["settle", "full_fill", "transfer_leg"].iter().any(|name| entry.name.ends_with(name)))
        .map(|entry| (entry.name.clone(), entry.frame_size_bytes))
        .collect();
    Ok(Observation {
        cycles,
        transaction_bytes: transaction.len(),
        transaction,
        witness_bytes: tx.witnesses().into_iter().map(|witness| witness.raw_data().len()).sum(),
        settlement_elf_bytes: crate::strip_vm_abi_trailer(&compiled.artifact_bytes).len(),
        asset_elf_bytes: crate::strip_vm_abi_trailer(&peer.artifact_bytes).len(),
        output_capacity: tx.outputs().into_iter().map(|cell| u64::from(cell.capacity())).sum(),
        occupied_output_capacity,
        declared_frames,
        observed_stack_bytes: None,
        observed_heap_bytes: None,
    })
}

#[test]
fn mixed_settlement_business_transaction_checks_each_asset_and_every_group() {
    let peer = crate::compile(PEER, crate::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    for opt in [0, 3] {
        let compiled = compile_settlement(opt, true);
        let positional = (opt == 3).then(|| compile_settlement(opt, false));
        for count in [1, 3, 6] {
            let forward = run_settlement(&compiled, &peer, count, Mutation::None, false).unwrap();
            let reversed = run_settlement(&compiled, &peer, count, Mutation::None, true).unwrap();
            assert_eq!(forward.cycles, reversed.cycles);
            eprintln!("[mixed settlement] O{opt} orders={count} {}", serde_json::to_string(&reversed).unwrap());
            if let Some(positional) = &positional {
                let legacy = run_settlement(positional, &peer, count, Mutation::None, true).unwrap();
                assert_eq!(legacy.transaction, reversed.transaction, "authoring form changed canonical transaction bytes");
                eprintln!("[mixed settlement positional] O{opt} orders={count} {}", serde_json::to_string(&legacy).unwrap());
            }
        }
        let too_many = run_settlement(&compiled, &peer, 7, Mutation::None, true).unwrap_err();
        assert!(too_many.contains("error code 21 "), "over-limit: {too_many}");
        for mutation in [
            Mutation::Debit,
            Mutation::Credit,
            Mutation::Owner,
            Mutation::Terms,
            Mutation::AlreadySettled,
            Mutation::ZeroAmount,
            Mutation::Insufficient,
            Mutation::Overflow,
            Mutation::Duplicate,
            Mutation::Missing,
            Mutation::Unmatched,
            Mutation::Custody,
            Mutation::Capacity,
            Mutation::Schema,
            Mutation::BasePeer,
            Mutation::QuotePeer,
            Mutation::LockReject,
            Mutation::CrossAsset,
            Mutation::PeerInflation,
            Mutation::IdentityOverflow,
        ] {
            let error = run_settlement(&compiled, &peer, 6, mutation, true).unwrap_err();
            let code = match mutation {
                Mutation::BasePeer | Mutation::QuotePeer => 71,
                Mutation::LockReject => 72,
                Mutation::Duplicate => 21,
                Mutation::Missing => 7,
                Mutation::Custody => 41,
                Mutation::Schema => 4,
                _ => 5,
            };
            assert!(error.contains(&format!("error code {code} ")), "O{opt} {mutation:?}: {error}");
            if let Some(positional) = &positional {
                let legacy = run_settlement(positional, &peer, 6, mutation, true).unwrap_err();
                assert!(legacy.contains(&format!("error code {code} ")), "positional {mutation:?}: {legacy}");
            }
        }
    }
}

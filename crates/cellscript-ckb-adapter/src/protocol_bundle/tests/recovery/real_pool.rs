//! Client smoke over exact transactions emitted by the authentic pool harness.
//! This does not execute another VM or claim new node evidence.

use super::*;

fn number(data: &[u8]) -> u128 {
    u128::from_le_bytes(data[..16].try_into().unwrap())
}
fn encoding(data: &[u8]) -> AmountEncoding {
    match data.len() {
        16 => AmountEncoding::U128Le,
        80 => AmountEncoding::PoolShares { pool: data[16..48].try_into().unwrap(), controller_code: data[48..80].try_into().unwrap() },
        length => panic!("unsupported actual financial payload length {length}"),
    }
}

#[test]
#[ignore = "requires fresh target/finance-protocol-evidence/pool/report.json from finance_protocol"]
fn accepted_pool_lp_cases_bind_share_tail_and_economic_limits() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let report: Value =
        serde_json::from_slice(&fs::read(root.join("target/finance-protocol-evidence/pool/report.json")).unwrap()).unwrap();
    for name in ["initialise-authentic-pool-and-lp", "lp-transfer-and-split-without-pool-spend", "withdraw-220"] {
        let case = report["cases"].as_array().unwrap().iter().find(|case| case["case"] == name).unwrap();
        assert_eq!(case["outcome"], "accepted");
        let transaction = ckb_types::packed::Transaction::from_slice(&hex::decode(case["transaction"].as_str().unwrap()).unwrap())
            .unwrap()
            .into_view();
        assert_eq!(hex::encode(transaction.hash().as_slice()), case["transaction_hash"]);
        assert_eq!(hex::encode(ckb_hash::blake2b_256(transaction.data().as_slice())), case["serialized_transaction_hash"]);
        let inputs = case["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| ProtocolBundleLiveCellObservation {
                out_point: OutPoint::from_slice(&hex::decode(cell["out_point"].as_str().unwrap()).unwrap()).unwrap(),
                output: CellOutput::from_slice(&hex::decode(cell["output"].as_str().unwrap()).unwrap()).unwrap(),
                data: Bytes::from(hex::decode(cell["data"].as_str().unwrap()).unwrap()),
            })
            .collect::<Vec<_>>();
        let outputs = transaction
            .outputs()
            .into_iter()
            .zip(transaction.outputs_data())
            .map(|(output, data)| (output, data.raw_data()))
            .collect::<Vec<_>>();
        let share_input = inputs.iter().filter(|cell| cell.data.len() == 80).max_by_key(|cell| number(&cell.data));
        let owner = share_input.map(|cell| cell.output.lock()).unwrap_or_else(|| {
            outputs.iter().filter(|(_, data)| data.len() == 80).max_by_key(|(_, data)| number(data)).unwrap().0.lock()
        });
        let recipient = if name.contains("transfer") {
            outputs
                .iter()
                .filter(|(output, data)| data.len() == 80 && output.lock() != owner)
                .max_by_key(|(_, data)| number(data))
                .unwrap()
                .0
                .lock()
        } else {
            owner.clone()
        };
        let total = |asset: &Script, holder: &Script, before: bool| -> u128 {
            if before {
                inputs
                    .iter()
                    .filter(|cell| cell.output.lock() == *holder && cell.output.type_().to_opt().as_ref() == Some(asset))
                    .map(|cell| number(&cell.data))
                    .sum()
            } else {
                outputs
                    .iter()
                    .filter(|(output, _)| output.lock() == *holder && output.type_().to_opt().as_ref() == Some(asset))
                    .map(|(_, data)| number(data))
                    .sum()
            }
        };
        let mut debits: Vec<AssetAmountBound> = Vec::new();
        for input in inputs.iter().filter(|cell| cell.output.lock() == owner && cell.output.type_().to_opt().is_some()) {
            let asset = input.output.type_().to_opt().unwrap();
            if debits.iter().any(|bound| bound.asset == asset) {
                continue;
            }
            debits.push(AssetAmountBound {
                amount: total(&asset, &owner, true).saturating_sub(total(&asset, &owner, false)),
                asset,
                encoding: encoding(&input.data),
            });
        }
        let mut credits: Vec<AssetAmountBound> = Vec::new();
        for (output, data) in outputs.iter().filter(|(output, _)| output.lock() == recipient && output.type_().to_opt().is_some()) {
            let asset = output.type_().to_opt().unwrap();
            let received = total(&asset, &recipient, false).saturating_sub(total(&asset, &recipient, true));
            if received == 0 || credits.iter().any(|bound| bound.asset == asset) {
                continue;
            }
            credits.push(AssetAmountBound { asset, encoding: encoding(data), amount: received });
        }
        let subject = inputs.iter().find_map(|cell| cell.output.type_().to_opt()).unwrap();
        let parameters = || FinanceIntentParameters {
            operation_id: H256::from([0x61; 32]),
            operation: name.into(),
            owner: owner.clone(),
            recipient: recipient.clone(),
            debits: debits.clone(),
            credits: credits.clone(),
            subjects: vec![RecoverySubject::Type(subject.clone())],
            maximum_fee_shannons: case["fee"].as_u64().unwrap(),
            maximum_capacity_debit_shannons: u64::MAX,
            maximum_occupied_capacity_shannons: u64::MAX,
        };
        let intent = FinanceIntent::new(parameters()).unwrap();
        intent.verify_candidate(&transaction, &inputs).unwrap_or_else(|error| panic!("{name}: {error}"));
        if let Some(index) = inputs.iter().position(|cell| cell.output.lock() == owner && cell.data.len() == 80) {
            let mut hostile = inputs.clone();
            let mut changed = hostile[index].data.to_vec();
            changed[79] ^= 1;
            hostile[index].data = Bytes::from(changed);
            assert!(intent.verify_candidate(&transaction, &hostile).is_err(), "{name}: altered LP tail");
            let mut hostile = inputs.clone();
            let mut changed = hostile[index].data.to_vec();
            let excessive = number(&changed) + 1;
            changed[..16].copy_from_slice(&excessive.to_le_bytes());
            hostile[index].data = Bytes::from(changed);
            assert!(intent.verify_candidate(&transaction, &hostile).is_err(), "{name}: excess LP debit");
        }
        let credit_index = outputs
            .iter()
            .position(|(output, _)| {
                output.lock() == recipient && credits.iter().any(|bound| output.type_().to_opt().as_ref() == Some(&bound.asset))
            })
            .unwrap();
        let mut hostile_outputs = outputs.iter().map(|(output, _)| output.clone()).collect::<Vec<_>>();
        hostile_outputs[credit_index] = hostile_outputs[credit_index]
            .clone()
            .as_builder()
            .lock(Script::new_builder().code_hash([0x99; 32].pack()).hash_type(4u8).build())
            .build();
        let hostile = transaction.as_advanced_builder().set_outputs(hostile_outputs).build();
        assert!(intent.verify_candidate(&hostile, &inputs).is_err(), "{name}: wrong payout recipient");
        if name.contains("initialise") {
            let lp_index = outputs.iter().position(|(output, data)| output.lock() == recipient && data.len() == 80).unwrap();
            for tail_change in [false, true] {
                let mut data = outputs[lp_index].1.to_vec();
                if tail_change {
                    data[79] ^= 1;
                } else {
                    data[..16].copy_from_slice(&(number(&outputs[lp_index].1) - 1).to_le_bytes());
                }
                let mut all_data = outputs.iter().map(|(_, data)| data.clone().pack()).collect::<Vec<_>>();
                all_data[lp_index] = Bytes::from(data).pack();
                let hostile = transaction.as_advanced_builder().set_outputs_data(all_data).build();
                assert!(intent.verify_candidate(&hostile, &inputs).is_err(), "{name}: altered minted LP bytes");
            }
        }
    }
}

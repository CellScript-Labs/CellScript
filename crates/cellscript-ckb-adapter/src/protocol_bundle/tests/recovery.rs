//! Host policy tests. Node observations below are synthetic and are never
//! reported as VM execution, node acceptance or financial protocol acceptance.

use super::*;
use crate::recovery::{
    ProtocolBundleRecovery, RecoveryCandidate, RecoveryInclusion, RecoveryObservation, RecoveryPolicy, RecoveryState, RecoverySubject,
    UnsignedAttemptFailure,
};

#[path = "../../../../../examples/finance_client/recovery.rs"]
mod finance_client;
use finance_client::FinanceJournal;
use finance_client::{AmountEncoding, AssetAmountBound, FinanceIntent, FinanceIntentParameters, FinanceRecovery};
mod real_pool;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_JOURNAL: AtomicU64 = AtomicU64::new(0);

#[test]
fn one_sided_placement_and_refund_intents_keep_explicit_owner_spend_authority() {
    let placement = Fixture::new(51, 900, 100, 1_000_000_000, false);
    let mut parameters = placement.intent_parameters(AmountEncoding::U128Le);
    parameters.operation = "place-escrow-order".into();
    parameters.credits.clear();
    let debit_only = FinanceIntent::new(parameters).unwrap();
    debit_only.verify_candidate(&placement.transaction, &placement.cells).unwrap();
    let excessive = Fixture::new(52, 899, 101, 1_000_000_000, false);
    assert!(debit_only.verify_candidate(&excessive.transaction, &excessive.cells).is_err());

    // The input belongs to the escrow/controller; the refund recipient signs
    // only unrelated untyped funding. A credit never authorizes its assets.
    let refund_parameters = || {
        let mut parameters = placement.intent_parameters(AmountEncoding::U128Le);
        parameters.operation = "refund-escrow-order".into();
        parameters.debits.clear();
        parameters
    };
    let unauthorized_owner = FinanceIntent::new(refund_parameters()).unwrap();
    assert!(unauthorized_owner.verify_candidate(&placement.transaction, &placement.cells).is_err());
    let mut parameters = refund_parameters();
    parameters.owner = parameters.recipient.clone();
    let credit_only = FinanceIntent::new(parameters).unwrap();
    credit_only.verify_candidate(&placement.transaction, &placement.cells).unwrap();
    let insufficient = Fixture::new(53, 901, 99, 1_000_000_000, false);
    assert!(credit_only.verify_candidate(&insufficient.transaction, &insufficient.cells).is_err());
    let mut parameters = refund_parameters();
    parameters.credits.clear();
    assert!(FinanceIntent::new(parameters).is_err());
}

#[test]
fn capacity_refunds_only_credit_untyped_or_explicitly_admitted_owner_assets() {
    let fixture = Fixture::new(54, 900, 100, 1_000_000_000, false);
    let owner = fixture.inputs[0].0.lock();
    let asset = fixture.inputs[0].0.type_().to_opt().unwrap();
    let mut outputs = fixture.transaction.outputs().into_iter().collect::<Vec<_>>();
    outputs[0] = outputs[0].clone().as_builder().capacity(50_000_000_000u64).build();
    outputs.push(CellOutput::new_builder().capacity(20_000_000_000u64).lock(owner).build());
    let mut data = fixture.transaction.outputs_data().into_iter().collect::<Vec<_>>();
    data.push(Bytes::from_static(b"untyped-owner-refund-data-is-allowed").pack());
    let candidate = |outputs: Vec<CellOutput>, data: Vec<ckb_types::packed::Bytes>| {
        fixture.transaction.as_advanced_builder().set_outputs(outputs).set_outputs_data(data).build()
    };
    let intent = fixture.intent();
    intent.verify_candidate(&candidate(outputs.clone(), data.clone()), &fixture.cells).unwrap();
    outputs[2] = outputs[2].clone().as_builder().type_(Some(asset.clone()).pack()).build();
    data[2] = Bytes::copy_from_slice(&0u128.to_le_bytes()).pack();
    intent.verify_candidate(&candidate(outputs.clone(), data.clone()), &fixture.cells).unwrap();
    let unknown =
        Script::new_builder().code_hash([0xf1; 32]).hash_type(4u8).args(Bytes::from_static(b"reject-future-spend").pack()).build();
    outputs[2] = outputs[2].clone().as_builder().type_(Some(unknown.clone()).pack()).build();
    assert!(outputs[2].occupied_capacity(ckb_types::core::Capacity::bytes(16).unwrap()).unwrap() <= outputs[2].capacity().unpack());
    assert!(intent
        .verify_candidate(&candidate(outputs.clone(), data.clone()), &fixture.cells)
        .unwrap_err()
        .to_string()
        .contains("undeclared asset Type"));

    let mut parameters = fixture.intent_parameters(AmountEncoding::U128Le);
    assert_ne!(parameters.owner, parameters.recipient);
    let credit_asset = asset.as_builder().args(Bytes::from(vec![0xe2; 32]).pack()).build();
    outputs[2] = outputs[2].clone().as_builder().type_(Some(credit_asset.clone()).pack()).build();
    parameters.credits.push(AssetAmountBound { asset: credit_asset, encoding: AmountEncoding::U128Le, amount: 0 });
    let explicit_credit = FinanceIntent::new(parameters).unwrap();
    explicit_credit.verify_candidate(&candidate(outputs.clone(), data.clone()), &fixture.cells).unwrap();
    data[2] = Bytes::copy_from_slice(&0u64.to_le_bytes()).pack();
    assert!(explicit_credit
        .verify_candidate(&candidate(outputs, data), &fixture.cells)
        .unwrap_err()
        .to_string()
        .contains("canonical u128"));
}

struct TestJournal {
    directory: PathBuf,
    journal: FinanceJournal,
}
impl TestJournal {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "cellscript-finance-recovery-{}-{}",
            std::process::id(),
            NEXT_JOURNAL.fetch_add(1, Ordering::Relaxed)
        ));
        let journal = FinanceJournal::new(&directory).unwrap();
        Self { directory, journal }
    }
    fn file(&self, suffix: &str) -> PathBuf {
        fs::read_dir(&self.directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.file_name().unwrap().to_string_lossy().ends_with(suffix))
            .unwrap()
    }
}
impl Drop for TestJournal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct Fixture {
    transaction: TransactionView,
    materialization: ProtocolBundleMaterializationEvidence,
    inputs: Vec<(CellOutput, Bytes)>,
    cells: Vec<ProtocolBundleLiveCellObservation>,
    live: ProtocolBundleLiveResolutionEvidence,
    dependencies: ProtocolBundleDependencyResolutionEvidence,
}

impl Fixture {
    fn new(nonce: u8, change: u128, received: u128, fee: u64, wrong_recipient: bool) -> Self {
        Self::with_owner_capacity(nonce, change, received, fee, wrong_recipient, 70_000_000_000)
    }

    fn with_owner_capacity(nonce: u8, change: u128, received: u128, fee: u64, wrong_recipient: bool, owner_capacity: u64) -> Self {
        let mut report = report();
        let auth_code = Bytes::from_static(b"recovery-auth-code");
        let token_code = Bytes::from_static(b"recovery-token-code");
        bind_artifact_to_code_data(&mut report, 0, &auth_code);
        bind_artifact_to_code_data(&mut report, 1, &token_code);
        report["bundle"]["transaction"]["inputs"][0]["out_point"]["tx_hash"] =
            Value::String(format!("0x{}", hex::encode([nonce; 32])));
        report["bundle"]["transaction"]["inputs"][0]["data"] = Value::String(format!("0x{}", hex::encode(1_000u128.to_le_bytes())));
        let mut owner_output = report["bundle"]["transaction"]["outputs"][0].clone();
        owner_output["capacity"] = Value::from(owner_capacity);
        owner_output["data"] = Value::String(format!("0x{}", hex::encode(change.to_le_bytes())));
        let mut recipient_output = owner_output.clone();
        recipient_output["capacity"] = Value::from(120_000_000_000u64 - owner_capacity - fee);
        recipient_output["lock"] = script(if wrong_recipient { "8" } else { "9" });
        recipient_output["data"] = Value::String(format!("0x{}", hex::encode(received.to_le_bytes())));
        report["bundle"]["transaction"]["outputs"] = serde_json::json!([owner_output, recipient_output]);
        rebind_bundle_hash(&mut report);
        let (transaction, materialization) = materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
        let lock: ScriptWire = serde_json::from_value(report["bundle"]["transaction"]["inputs"][0]["lock"].clone()).unwrap();
        let token: ScriptWire = serde_json::from_value(report["bundle"]["transaction"]["inputs"][0]["type"].clone()).unwrap();
        let output = CellOutput::new_builder()
            .capacity(120_000_000_000u64)
            .lock(parse_script("input.lock", &lock).unwrap())
            .type_(Some(parse_script("input.type", &token).unwrap()).pack())
            .build();
        let inputs = vec![(output.clone(), Bytes::copy_from_slice(&1_000u128.to_le_bytes()))];
        let cells = vec![ProtocolBundleLiveCellObservation {
            out_point: transaction.inputs().get(0).unwrap().previous_output(),
            output: output.clone(),
            data: inputs[0].1.clone(),
        }];
        let live = protocol_bundle_live_resolution_evidence(
            &transaction,
            &materialization,
            &materialization.network_chain_id,
            &materialization.network_genesis_hash,
            &inputs,
        )
        .unwrap();
        let code_output = CellOutput::new_builder().capacity(10_000_000_000u64).lock(output.lock()).build();
        let observations = [auth_code, token_code]
            .into_iter()
            .enumerate()
            .map(|(index, data)| ProtocolBundleCellDepObservation {
                transaction_cell_dep_index: index as u32,
                root: ProtocolBundleLiveCellObservation {
                    out_point: transaction.cell_deps().get(index).unwrap().out_point(),
                    output: code_output.clone(),
                    data,
                },
                dep_group_members: vec![],
            })
            .collect::<Vec<_>>();
        let dependencies =
            protocol_bundle_dependency_resolution_evidence(&transaction, &materialization, &live, &observations).unwrap();
        Self { transaction, materialization, inputs, cells, live, dependencies }
    }

    fn candidate(&self) -> RecoveryCandidate<'_> {
        RecoveryCandidate {
            transaction: &self.transaction,
            materialization: &self.materialization,
            resolved_inputs: &self.inputs,
            live_resolution: &self.live,
            dependency_resolution: &self.dependencies,
        }
    }

    fn intent(&self) -> FinanceIntent {
        self.intent_encoding(AmountEncoding::U128Le)
    }

    fn intent_encoding(&self, encoding: AmountEncoding) -> FinanceIntent {
        FinanceIntent::new(self.intent_parameters(encoding)).unwrap()
    }

    fn intent_parameters(&self, encoding: AmountEncoding) -> FinanceIntentParameters {
        let asset = self.inputs[0].0.type_().to_opt().unwrap();
        let recipient: ScriptWire = serde_json::from_value(script("9")).unwrap();
        FinanceIntentParameters {
            operation_id: H256::from([0x77; 32]),
            operation: "test-transfer".into(),
            owner: self.inputs[0].0.lock(),
            recipient: parse_script("recipient", &recipient).unwrap(),
            debits: vec![AssetAmountBound { asset: asset.clone(), encoding, amount: 100 }],
            credits: vec![AssetAmountBound { asset: asset.clone(), encoding, amount: 100 }],
            subjects: vec![RecoverySubject::Type(asset)],
            maximum_fee_shannons: 1_000_000_000,
            maximum_capacity_debit_shannons: 60_000_000_000,
            maximum_occupied_capacity_shannons: 100_000_000_000,
        }
    }

    fn client(&self, maximum_rebuilds: u32, maximum_observations: u32) -> FinanceRecovery {
        FinanceRecovery::new(
            self.intent(),
            self.materialization.network_chain_id.clone(),
            H256::from([0; 32]),
            RecoveryPolicy { maximum_rebuilds, maximum_observations, required_confirmations: 2 },
        )
        .unwrap()
    }

    fn signed(
        &self,
    ) -> (TransactionView, ProtocolBundleSignedTransactionEvidence, ProtocolBundleTxPoolEvidence, ProtocolBundleSubmissionEvidence)
    {
        let prepared =
            protocol_bundle_ready_to_sign_evidence(&self.transaction, &self.materialization, &self.live, &self.dependencies).unwrap();
        let witness = WitnessArgs::from_slice(self.transaction.witnesses().get(0).unwrap().raw_data().as_ref())
            .unwrap()
            .as_builder()
            .lock(Some(Bytes::from(vec![0xab; 65])).pack())
            .build();
        let signed = self.transaction.as_advanced_builder().set_witnesses(vec![witness.as_bytes().pack()]).build();
        let signing = protocol_bundle_signed_transaction_evidence(&self.transaction, &signed, &prepared, 1).unwrap();
        let dry_run = protocol_bundle_signed_dry_run_evidence(
            &signed,
            &self.materialization,
            &signing,
            &EstimateCycles { cycles: 1_000u64.into() },
        )
        .unwrap();
        let pool = protocol_bundle_tx_pool_evidence(
            &signed,
            &self.materialization,
            &signing,
            &dry_run,
            &EntryCompleted { cycles: 1_000u64.into(), fee: self.materialization.fee_shannons.into() },
        )
        .unwrap();
        let hash = H256::from_slice(signed.hash().as_slice()).unwrap();
        let submission = protocol_bundle_submission_evidence(&signed, &signing, &pool, &hash).unwrap();
        (signed, signing, pool, submission)
    }
}

#[test]
fn stale_unsigned_candidates_rebuild_with_identical_economic_limits_and_fresh_signing() {
    let first = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let second = Fixture::new(2, 900, 100, 1_000_000_000, false);
    let mut client = first.client(1, 8);
    client.prepare(first.candidate(), &first.cells).unwrap();
    client.unsigned_failure(UnsignedAttemptFailure::StaleQuote).unwrap();
    client.prepare(second.candidate(), &second.cells).unwrap();
    assert_eq!(client.recovery().rebuilds(), 1);
    assert_ne!(first.transaction.hash(), second.transaction.hash());
    let (old_signed, old_signing, _, _) = first.signed();
    assert!(client.bind_signed(&old_signed, &old_signing).is_err(), "new bytes require a new signature");
    let (signed, signing, _, _) = second.signed();
    client.bind_signed(&signed, &signing).unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Signed);
    let (_, _, old_pool, _) = first.signed();
    let journal = TestJournal::new();
    assert!(client.begin_submission(&old_pool, &journal.journal).is_err(), "new signed bytes cannot reuse old tx-pool evidence");
    assert!(client.prepare(first.candidate(), &first.cells).is_err());
    let replay = Fixture::new(2, 900, 100, 1_000_000_000, false);
    assert_eq!(
        replay.transaction.data().as_slice(),
        second.transaction.data().as_slice(),
        "identical rebuild inputs produce identical bytes"
    );
}

#[test]
fn actual_amount_recipient_fee_and_resolved_input_mutations_cannot_relax_intent() {
    let first = Fixture::new(1, 900, 100, 1_000_000_000, false);
    for hostile in [
        Fixture::new(2, 899, 101, 1_000_000_000, false),
        Fixture::new(2, 901, 99, 1_000_000_000, false),
        Fixture::new(2, 900, 100, 1_000_000_001, false),
        Fixture::new(2, 900, 100, 1_000_000_000, true),
        Fixture::with_owner_capacity(2, 900, 100, 1_000_000_000, false, 50_000_000_000),
    ] {
        let mut client = first.client(2, 8);
        assert!(client.prepare(hostile.candidate(), &hostile.cells).is_err());
        assert_eq!(client.recovery().state(), RecoveryState::AwaitingCandidate);
    }
    let mut client = first.client(1, 8);
    let mut cells = first.cells.clone();
    cells[0].data = Bytes::copy_from_slice(&900u128.to_le_bytes());
    assert!(client.prepare(first.candidate(), &cells).is_err(), "economic checks cannot use another input observation");
    cells[0].out_point = OutPoint::new_builder().index(3u32).build();
    assert!(client.prepare(first.candidate(), &cells).is_err());
}

#[test]
fn immutable_subjects_limits_and_actual_capacity_are_bound_by_adapter() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let intent = fixture.intent().recovery_intent("ckb-testnet".into(), H256::from([0; 32])).unwrap();
    let policy = RecoveryPolicy { maximum_rebuilds: 1, maximum_observations: 8, required_confirmations: 2 };
    let mut tracker = ProtocolBundleRecovery::new(intent.clone(), policy).unwrap();
    tracker.discovery_unavailable("indexer unavailable".into()).unwrap();
    assert_eq!(tracker.state(), RecoveryState::DiscoveryUnavailable);
    assert_eq!(tracker.discovery_unavailable_reason(), Some("indexer unavailable"));
    let changed = crate::recovery::RecoveryIntent::new(
        "ckb-testnet".into(),
        H256::from([0; 32]),
        intent.commitment().clone(),
        vec![RecoverySubject::Lock(fixture.inputs[0].0.lock())],
        2_000_000_000,
        100_000_000_000,
    )
    .unwrap();
    assert!(tracker.bind_candidate(&changed, fixture.candidate()).is_err());
    let mut materialization = fixture.materialization.clone();
    materialization.occupied_output_capacity_shannons = 0;
    assert!(tracker.bind_candidate(&intent, RecoveryCandidate { materialization: &materialization, ..fixture.candidate() }).is_err());
    tracker.bind_candidate(&intent, fixture.candidate()).unwrap();
    tracker.unsigned_failure(UnsignedAttemptFailure::StaleInput).unwrap();
    tracker.bind_candidate(&intent, fixture.candidate()).unwrap();
    tracker.unsigned_failure(UnsignedAttemptFailure::StaleInput).unwrap();
    assert!(tracker.bind_candidate(&intent, fixture.candidate()).unwrap_err().to_string().contains("rebuild limit"));
}

#[test]
fn explicit_amount_profiles_and_full_asset_identities_are_enforced() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let intent = fixture.intent_encoding(AmountEncoding::U64Le);
    assert!(intent.verify_candidate(&fixture.transaction, &fixture.cells).is_err(), "u128 bytes cannot silently become u64 amounts");
    let transaction = fixture
        .transaction
        .as_advanced_builder()
        .set_outputs_data(vec![
            Bytes::copy_from_slice(&900u64.to_le_bytes()).pack(),
            Bytes::copy_from_slice(&100u64.to_le_bytes()).pack(),
        ])
        .build();
    let mut cells = fixture.cells.clone();
    cells[0].data = Bytes::copy_from_slice(&1_000u64.to_le_bytes());
    intent.verify_candidate(&transaction, &cells).unwrap();
    let asset = cells[0].output.type_().to_opt().unwrap();
    let substituted = asset.as_builder().args(Bytes::from_static(b"other-asset-identity").pack()).build();
    cells[0].output = cells[0].output.clone().as_builder().type_(Some(substituted).pack()).build();
    assert!(intent.verify_candidate(&transaction, &cells).unwrap_err().to_string().contains("undeclared owner asset"));
}

#[test]
fn a_recipient_credit_does_not_authorize_spending_the_owners_credit_only_asset() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let owner = fixture.inputs[0].0.lock();
    let asset_b =
        fixture.inputs[0].0.type_().to_opt().unwrap().as_builder().args(Bytes::from_static(b"distinct-asset-b").pack()).build();
    let point = OutPoint::new_builder().tx_hash([0x91; 32].pack()).index(0u32).build();
    let extra_owner_input =
        CellOutput::new_builder().capacity(30_000_000_000u64).lock(owner.clone()).type_(Some(asset_b.clone()).pack()).build();
    let recipient = fixture.transaction.outputs().get(1).unwrap().lock();
    assert_ne!(owner, recipient);
    let attacker = Script::new_builder().code_hash([0x88; 32].pack()).hash_type(4u8).build();
    let transaction = fixture
        .transaction
        .as_advanced_builder()
        .input(CellInput::new_builder().previous_output(point.clone()).build())
        .output(CellOutput::new_builder().capacity(15_000_000_000u64).lock(recipient).type_(Some(asset_b.clone()).pack()).build())
        .output_data(Bytes::copy_from_slice(&100u128.to_le_bytes()).pack())
        .output(CellOutput::new_builder().capacity(15_000_000_000u64).lock(attacker).type_(Some(asset_b.clone()).pack()).build())
        .output_data(Bytes::copy_from_slice(&900u128.to_le_bytes()).pack())
        .build();
    let mut cells = fixture.cells.clone();
    cells.push(ProtocolBundleLiveCellObservation {
        out_point: point,
        output: extra_owner_input,
        data: Bytes::copy_from_slice(&1_000u128.to_le_bytes()),
    });
    let parameters = || {
        let mut parameters = fixture.intent_parameters(AmountEncoding::U128Le);
        parameters.credits = vec![AssetAmountBound { asset: asset_b.clone(), encoding: AmountEncoding::U128Le, amount: 100 }];
        parameters.maximum_capacity_debit_shannons = 100_000_000_000;
        parameters
    };
    let error = FinanceIntent::new(parameters()).unwrap().verify_candidate(&transaction, &cells).unwrap_err();
    assert!(error.to_string().contains("explicit debit limit"), "{error}");
    let mut declared = parameters();
    declared.debits.push(AssetAmountBound { asset: asset_b, encoding: AmountEncoding::U128Le, amount: 0 });
    let error = FinanceIntent::new(declared).unwrap().verify_candidate(&transaction, &cells).unwrap_err();
    assert!(error.to_string().contains("maximum spend"), "{error}");
}

#[test]
fn timeout_pending_unknown_and_unavailable_never_create_another_economic_attempt() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let next = Fixture::new(2, 900, 100, 1_000_000_000, false);
    let mut client = fixture.client(4, 8);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, _) = fixture.signed();
    let hash = H256::from_slice(signed.hash().as_slice()).unwrap();
    client.bind_signed(&signed, &signing).unwrap();
    let journal = TestJournal::new();
    client.begin_submission(&pool, &journal.journal).unwrap();
    let (_, _, _, unrelated_submission) = next.signed();
    assert!(client.submitted(&unrelated_submission).is_err());
    client.submission_unknown().unwrap();
    for (observation, state) in [
        (RecoveryObservation::Unknown, RecoveryState::SubmissionUnknown),
        (RecoveryObservation::Pending, RecoveryState::Pending),
        (RecoveryObservation::Unavailable("RPC timeout".into()), RecoveryState::ObservationUnavailable),
    ] {
        client.observe(&hash, observation.clone()).unwrap();
        assert_eq!(client.recovery().state(), state);
        assert_eq!(client.recovery().last_observation(), Some(&observation));
        assert!(client.prepare(next.candidate(), &next.cells).is_err());
    }
    assert!(client.observe(&H256::from([9; 32]), RecoveryObservation::Pending).is_err());
    client
        .observe(
            &hash,
            RecoveryObservation::Rejected {
                stage: crate::recovery::RecoveryRejectionStage::TransactionStatus,
                reason: "node rejected original transaction".into(),
            },
        )
        .unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Rejected);
    assert!(client.prepare(next.candidate(), &next.cells).is_err());
}

#[test]
fn committed_original_confirms_with_bound_depth_and_reorgs_remain_observations() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let mut client = fixture.client(1, 8);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, submission) = fixture.signed();
    let hash = H256::from_slice(signed.hash().as_slice()).unwrap();
    client.bind_signed(&signed, &signing).unwrap();
    let journal = TestJournal::new();
    client.begin_submission(&pool, &journal.journal).unwrap();
    client.submission_unknown().unwrap();
    client
        .observe(
            &hash,
            RecoveryObservation::Included(RecoveryInclusion {
                block_hash: H256::from([3; 32]),
                block_number: 10,
                transaction_index: 0,
            }),
        )
        .unwrap();
    let confirmation = |block: u8, reorgs: u32, depth: u64| {
        protocol_bundle_confirmation_evidence(
            &submission,
            &fixture.materialization,
            "ckb-testnet",
            &format!("0x{}", "0".repeat(64)),
            &format!("0x{}", hex::encode([block; 32])),
            10,
            0,
            &format!("0x{}", "4".repeat(64)),
            11,
            depth,
            reorgs,
        )
        .unwrap()
    };
    assert!(client.confirmed(&confirmation(3, 0, 1)).is_err(), "caller cannot lower original confirmation depth");
    client.confirmed(&confirmation(3, 0, 2)).unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Confirmed);
    assert!(client.cancel().is_err());
    client
        .observe(
            &hash,
            RecoveryObservation::Included(RecoveryInclusion {
                block_hash: H256::from([5; 32]),
                block_number: 10,
                transaction_index: 0,
            }),
        )
        .unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Reorged);
    assert_eq!(client.recovery().reorgs_observed(), 1);
    assert!(client.confirmed(&confirmation(3, 0, 2)).is_err());
    client.confirmed(&confirmation(5, 1, 2)).unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Confirmed);
}

#[test]
fn rejected_signer_and_preflight_never_grant_submission_or_rebuild_rights() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    for failure in [UnsignedAttemptFailure::SignerRejected, UnsignedAttemptFailure::PreflightRejected] {
        let mut client = fixture.client(1, 8);
        client.prepare(fixture.candidate(), &fixture.cells).unwrap();
        client.unsigned_failure(failure).unwrap();
        assert_eq!(client.recovery().state(), RecoveryState::Rejected);
        assert_eq!(client.recovery().last_unsigned_failure(), Some(failure));
        let (signed, signing, pool, _) = fixture.signed();
        assert!(client.bind_signed(&signed, &signing).is_err());
        assert!(client.begin_submission(&pool, &TestJournal::new().journal).is_err());
        assert!(client.prepare(fixture.candidate(), &fixture.cells).is_err());
        assert_eq!(client.recovery().rebuilds(), 0);
    }
}

#[test]
fn cancelled_signing_and_bounded_observation_do_not_authorize_a_retry() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let mut client = fixture.client(1, 2);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    client.unsigned_failure(UnsignedAttemptFailure::RpcUnavailable).unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Prepared);
    client.unsigned_failure(UnsignedAttemptFailure::SignerCancelled).unwrap();
    assert_eq!(client.recovery().state(), RecoveryState::Cancelled);
    let (signed, signing, pool, submission) = fixture.signed();
    assert!(client.bind_signed(&signed, &signing).is_err());
    let mut client = fixture.client(1, 2);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    client.bind_signed(&signed, &signing).unwrap();
    let journal = TestJournal::new();
    client.begin_submission(&pool, &journal.journal).unwrap();
    client.submitted(&submission).unwrap();
    client.cancel().unwrap();
    assert!(client.recovery().cancellation_requested());
    assert_eq!(client.recovery().state(), RecoveryState::CancellationPendingObservation);
    let hash = H256::from_slice(signed.hash().as_slice()).unwrap();
    client.observe(&hash, RecoveryObservation::Pending).unwrap();
    client.observe(&hash, RecoveryObservation::Unknown).unwrap();
    assert!(client.observe(&hash, RecoveryObservation::Unknown).is_err());
    assert_eq!(client.recovery().state(), RecoveryState::ObservationLimitReached);
    assert!(client.prepare(fixture.candidate(), &fixture.cells).is_err());
}

fn recovery_policy() -> RecoveryPolicy {
    RecoveryPolicy { maximum_rebuilds: 1, maximum_observations: 8, required_confirmations: 2 }
}

#[test]
fn durable_original_submission_survives_a_separate_process_restart() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let journal = TestJournal::new();
    let mut client = fixture.client(1, 8);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, _) = fixture.signed();
    client.bind_signed(&signed, &signing).unwrap();
    client.begin_submission(&pool, &journal.journal).unwrap();
    // Simulate process loss while the original send could be in flight. The
    // next process never receives this tracker or any checked receipt object.
    drop(client);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "protocol_bundle::tests::recovery::journal_restart_child", "--ignored", "--test-threads=1"])
        .env("CELLSCRIPT_RECOVERY_CHILD_JOURNAL", &journal.directory)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert_eq!(
        fs::read_to_string(journal.directory.join("child-observed-original")).unwrap(),
        format!("0x{}", hex::encode(signed.hash().as_slice()))
    );
    let mut another = fixture.client(1, 8);
    let second = Fixture::new(2, 900, 100, 1_000_000_000, false);
    another.prepare(second.candidate(), &second.cells).unwrap();
    let (signed, signing, pool, _) = second.signed();
    another.bind_signed(&signed, &signing).unwrap();
    assert!(
        another.begin_submission(&pool, &journal.journal).is_err(),
        "same durable intent cannot authorize a different economic transaction"
    );
    assert_eq!(another.recovery().state(), RecoveryState::Signed);
    let mut weakened = second.intent_parameters(AmountEncoding::U128Le);
    weakened.credits[0].amount = 99;
    let mut weakened =
        FinanceRecovery::new(FinanceIntent::new(weakened).unwrap(), "ckb-testnet".into(), H256::from([0; 32]), recovery_policy())
            .unwrap();
    weakened.prepare(second.candidate(), &second.cells).unwrap();
    weakened.bind_signed(&signed, &signing).unwrap();
    assert!(
        weakened.begin_submission(&pool, &journal.journal).is_err(),
        "weakened limits cannot select a new journal slot for the same operation ID"
    );
}

#[test]
#[ignore = "entry point invoked in a separate process by the journal restart test"]
fn journal_restart_child() {
    let directory = PathBuf::from(std::env::var_os("CELLSCRIPT_RECOVERY_CHILD_JOURNAL").expect("parent journal path"));
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let journal = FinanceJournal::new(&directory).unwrap();
    let mut observer = match FinanceRecovery::start_or_resume(
        fixture.intent(),
        "ckb-testnet".into(),
        H256::from([0; 32]),
        recovery_policy(),
        &journal,
    )
    .unwrap()
    {
        finance_client::FinanceRecoverySession::ObservationOnly(observer) => observer,
        finance_client::FinanceRecoverySession::Fresh(_) => panic!("restart must not regain signing or submission rights"),
    };
    assert_eq!(observer.state(), RecoveryState::SubmissionUnknown);
    let original_hash = observer.original_transaction_hash().clone();
    assert!(observer.observe("wrong-chain", &H256::from([0; 32]), &original_hash, RecoveryObservation::Unknown).is_err());
    observer.observe("ckb-testnet", &H256::from([0; 32]), &original_hash, RecoveryObservation::Unknown).unwrap();
    assert_eq!(observer.state(), RecoveryState::SubmissionUnknown, "NotFound does not permit a new economic attempt");
    observer.observe("ckb-testnet", &H256::from([0; 32]), &original_hash, RecoveryObservation::Pending).unwrap();
    observer
        .observe(
            "ckb-testnet",
            &H256::from([0; 32]),
            &original_hash,
            RecoveryObservation::Included(RecoveryInclusion {
                block_hash: H256::from([3; 32]),
                block_number: 10,
                transaction_index: 0,
            }),
        )
        .unwrap();
    assert_eq!(observer.state(), RecoveryState::Included);
    let confirmed = observer
        .observe_confirmation(
            "ckb-testnet",
            &H256::from([0; 32]),
            &original_hash,
            RecoveryInclusion { block_hash: H256::from([3; 32]), block_number: 10, transaction_index: 0 },
            H256::from([4; 32]),
            11,
        )
        .unwrap();
    assert!(confirmed.required_depth_observed);
    assert_eq!(confirmed.required_confirmation_count, 2);
    assert_eq!(observer.state(), RecoveryState::Confirmed);
    observer.observe("ckb-testnet", &H256::from([0; 32]), &original_hash, RecoveryObservation::Unknown).unwrap();
    assert_eq!(observer.state(), RecoveryState::Reorged);
    assert_eq!(observer.reorgs_observed(), 1);
    assert_eq!(observer.identity().raw_transaction_hash(), format!("0x{}", hex::encode(original_hash.as_bytes())));
    fs::write(directory.join("child-observed-original"), format!("0x{}", hex::encode(original_hash.as_bytes()))).unwrap();
}

#[test]
fn journal_aware_restart_never_requests_a_second_signature() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let journal = TestJournal::new();
    let signer_calls = std::cell::Cell::new(0);
    let signer = || {
        signer_calls.set(signer_calls.get() + 1);
        fixture.signed()
    };
    let mut client = match FinanceRecovery::start_or_resume(
        fixture.intent(),
        "ckb-testnet".into(),
        H256::from([0; 32]),
        recovery_policy(),
        &journal.journal,
    )
    .unwrap()
    {
        finance_client::FinanceRecoverySession::Fresh(client) => client,
        finance_client::FinanceRecoverySession::ObservationOnly(_) => panic!("new operation"),
    };
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, _) = signer();
    client.bind_signed(&signed, &signing).unwrap();
    client.begin_submission(&pool, &journal.journal).unwrap();
    drop(client);
    match FinanceRecovery::start_or_resume(
        fixture.intent(),
        "ckb-testnet".into(),
        H256::from([0; 32]),
        recovery_policy(),
        &journal.journal,
    )
    .unwrap()
    {
        finance_client::FinanceRecoverySession::ObservationOnly(mut observer) => {
            let hash = observer.original_transaction_hash().clone();
            observer.observe("ckb-testnet", &H256::from([0; 32]), &hash, RecoveryObservation::Unknown).unwrap();
        }
        finance_client::FinanceRecoverySession::Fresh(_) => {
            signer();
            panic!("existing operation cannot request a fresh signature");
        }
    }
    assert_eq!(signer_calls.get(), 1);
    let mut weakened = fixture.intent_parameters(AmountEncoding::U128Le);
    weakened.credits[0].amount = 99;
    assert!(FinanceRecovery::start_or_resume(
        FinanceIntent::new(weakened).unwrap(),
        "ckb-testnet".into(),
        H256::from([0; 32]),
        recovery_policy(),
        &journal.journal
    )
    .is_err());
    assert!(FinanceRecovery::start_or_resume(
        fixture.intent(),
        "ckb-mainnet".into(),
        H256::from([0; 32]),
        recovery_policy(),
        &journal.journal
    )
    .is_err());
}

#[test]
fn persistence_failure_partial_records_and_orphan_temporaries_fail_closed() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let journal = TestJournal::new();
    let mut client = fixture.client(1, 8);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, _) = fixture.signed();
    client.bind_signed(&signed, &signing).unwrap();
    let key = FinanceJournal::operation_key(&fixture.intent());
    let reference = journal.directory.join(format!("{key}.reference.json"));
    fs::create_dir(&reference).unwrap();
    assert!(client.begin_submission(&pool, &journal.journal).is_err());
    assert_eq!(client.recovery().state(), RecoveryState::Signed, "no submission phase before durable journal completion");
    assert!(journal
        .journal
        .restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), recovery_policy(), 0)
        .is_err());
    fs::remove_dir(&reference).unwrap();
    fs::write(journal.directory.join(".pending-crash"), b"{partial").unwrap();
    client.begin_submission(&pool, &journal.journal).unwrap();
    let record_path = journal.file(".journal.json");
    let original = fs::read(&record_path).unwrap();
    fs::write(&record_path, b"{partial").unwrap();
    assert!(journal
        .journal
        .restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), recovery_policy(), 0)
        .is_err());
    fs::write(&record_path, original).unwrap();
    journal.journal.restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), recovery_policy(), 0).unwrap();
}

#[test]
fn journal_schema_intent_chain_attempt_policy_and_actual_bytes_must_match() {
    let fixture = Fixture::new(1, 900, 100, 1_000_000_000, false);
    let journal = TestJournal::new();
    let mut client = fixture.client(1, 8);
    client.prepare(fixture.candidate(), &fixture.cells).unwrap();
    let (signed, signing, pool, _) = fixture.signed();
    client.bind_signed(&signed, &signing).unwrap();
    client.begin_submission(&pool, &journal.journal).unwrap();
    let restore =
        || journal.journal.restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), recovery_policy(), 0);
    assert!(journal
        .journal
        .restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), recovery_policy(), 1)
        .is_err());
    assert!(journal
        .journal
        .restore_observation_only(&fixture.intent(), "ckb-mainnet", H256::from([0; 32]), recovery_policy(), 0)
        .is_err());
    let wrong_policy = RecoveryPolicy { required_confirmations: 1, ..recovery_policy() };
    assert!(journal.journal.restore_observation_only(&fixture.intent(), "ckb-testnet", H256::from([0; 32]), wrong_policy, 0).is_err());
    let path = journal.file(".journal.json");
    let original = fs::read(&path).unwrap();
    let reference_path = journal.file(".reference.json");
    let original_reference = fs::read(&reference_path).unwrap();
    for mutate in [0, 1, 2, 3, 4, 5] {
        let mut value: Value = serde_json::from_slice(&original).unwrap();
        match mutate {
            0 => value["schema"] = Value::String("future-schema".into()),
            1 => value["payload"]["identity"]["tx_pool_accepted"] = Value::Bool(true),
            2 => value["payload"]["intent"]["credits"][0]["amount"] = Value::String("99".into()),
            3 => value["payload"]["signed_transaction"] = Value::String("0x00".into()),
            4 => value["payload"]["identity"]["attempt"] = Value::from(1u32),
            _ => value["payload"]["identity"]["chain_id"] = Value::String("ckb-mainnet".into()),
        }
        if mutate >= 2 {
            // Rebind integrity hashes to reach the financial/schema/identity
            // checks rather than merely testing a stale payload checksum.
            let payload_hash =
                format!("0x{}", cellscript_artifact_checker::canonical_hash(
                "cellscript-finance-submission-journal-payload-v1", &value["payload"],
            ).unwrap());
            value["payload_hash"] = Value::String(payload_hash.clone());
            let mut reference: Value = serde_json::from_slice(&original_reference).unwrap();
            reference["payload_hash"] = Value::String(payload_hash);
            fs::write(&reference_path, serde_json::to_vec(&reference).unwrap()).unwrap();
        }
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(restore().is_err());
        fs::write(&reference_path, &original_reference).unwrap();
    }
    fs::write(&path, original).unwrap();
    let observer = restore().unwrap();
    assert_eq!(observer.observations(), 0);
    assert_eq!(observer.reorgs_observed(), 0);
    assert!(observer.last_observation().is_none());
}

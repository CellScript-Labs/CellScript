//! Exact signed, persistent transactions for the fungible business inventory.

#![cfg(not(feature = "wasm"))]

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{
    encode_policy_witness_bundle, place_policy_witness_bundle_before_signing, PolicyScriptRole, PolicyWitnessRecord,
};
use ckb_sdk::{
    constants::MultisigScript,
    traits::SecpCkbRawKeySigner,
    types::ScriptGroup,
    unlock::{MultisigConfig, ScriptSigner, SecpMultisigScriptSigner},
    SECP256K1,
};
use ckb_testtool::{
    ckb_hash::blake2b_256,
    ckb_types::{
        bytes::Bytes,
        core::{DepType, ScriptHashType, TransactionBuilder, TransactionView},
        packed,
        prelude::*,
        H160,
    },
    context::Context,
};
use secp256k1::{PublicKey, SecretKey};
use std::collections::HashMap;

const SOURCE: &str = include_str!("fixtures/fungible_scenarios.cell");
const TOKEN_CAPACITY: u64 = 100_000_000_000;
const FEE: u64 = 100_000_000;
const MAX_CYCLES: u64 = 100_000_000;

fn compile_policy() -> CompileResult {
    let compiled = compile_artifact(
        SOURCE,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            opt_level: 2,
            target: Some("riscv64-elf".to_string()),
            ..Default::default()
        },
        ArtifactDeclaration {
            name: "FungibleScenarioPolicy".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: [(0, "mint"), (17, "transfer"), (33, "split"), (255, "merge"), (u32::MAX, "burn")]
                .into_iter()
                .map(|(tag, action)| ArtifactAction { tag, action: action.to_string() })
                .collect(),
            common_checks: Vec::new(),
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("fungible scenario policy: {error}"));
    compiled.validate().expect("independent fungible scenario artifact validation");
    compiled
}

fn key(byte: u8) -> SecretKey {
    SecretKey::from_slice(&[byte; 32]).expect("fixed test-only secret key")
}

#[derive(Clone, Copy)]
enum Actor {
    FeePayer,
    Issuer,
    Alice,
    Bob,
}

impl Actor {
    fn keys(self) -> Vec<SecretKey> {
        let bytes = match self {
            Self::FeePayer => [0x71, 0x72],
            Self::Issuer => [0x11, 0x22],
            Self::Alice => [0x33, 0x44],
            Self::Bob => [0x55, 0x66],
        };
        bytes.into_iter().map(key).collect()
    }

    fn config(self) -> MultisigConfig {
        let ids = self
            .keys()
            .iter()
            .map(|secret| {
                let public_key = PublicKey::from_secret_key(&SECP256K1, secret);
                H160::from_slice(&blake2b_256(public_key.serialize())[..20]).unwrap()
            })
            .collect();
        MultisigConfig::new_with(MultisigScript::V2, ids, 0, 2).expect("canonical 2-of-2 multisig-v2")
    }

    fn lock(self) -> packed::Script {
        (&self.config()).into()
    }

    fn from_lock(lock: &packed::Script) -> Self {
        [Self::FeePayer, Self::Issuer, Self::Alice, Self::Bob]
            .into_iter()
            .find(|actor| actor.lock() == *lock)
            .expect("known scenario Lock")
    }
}

#[derive(Clone, Copy)]
enum Action {
    Mint { issuer_input: u64, amount: u64 },
    Transfer,
    Split { left_amount: u64 },
    Merge,
    Burn,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Mint { .. } => "mint",
            Self::Transfer => "transfer",
            Self::Split { .. } => "split",
            Self::Merge => "merge",
            Self::Burn => "burn",
        }
    }

    fn args(self, recipient: Actor) -> Vec<EntryWitnessArg> {
        let recipient = EntryWitnessArg::Address(recipient.lock().calc_script_hash().unpack());
        match self {
            Self::Mint { issuer_input, amount } => vec![EntryWitnessArg::U64(issuer_input), EntryWitnessArg::U64(amount), recipient],
            Self::Split { left_amount } => vec![EntryWitnessArg::U64(left_amount), recipient],
            Self::Transfer | Self::Merge => vec![recipient],
            Self::Burn => Vec::new(),
        }
    }
}

struct SigningGroup {
    actor: Actor,
    group: ScriptGroup,
}

struct Pending {
    unsigned: TransactionView,
    signed: TransactionView,
    groups: Vec<SigningGroup>,
}

impl Pending {
    fn sign(&self, transaction: &TransactionView, include_target: bool) -> TransactionView {
        let mut signed = transaction.clone();
        for signing in &self.groups {
            if !include_target && signing.group.input_indices[0] == 1 {
                continue;
            }
            let signer = SecpMultisigScriptSigner::new(
                Box::new(SecpCkbRawKeySigner::new_with_secret_keys(signing.actor.keys())),
                signing.actor.config(),
            );
            signed = signer.sign_tx(&signed, &signing.group).expect("sign completed scenario transaction");
        }
        signed
    }
}

struct Lifecycle<'a> {
    compiled: &'a CompileResult,
    context: Context,
    policy: packed::Script,
    secp_data: packed::OutPoint,
    live: HashMap<packed::OutPoint, (packed::CellOutput, Bytes)>,
    funding: packed::OutPoint,
    issuer: packed::OutPoint,
    attacker: packed::OutPoint,
}

fn plain_cell(actor: Actor, capacity: u64) -> packed::CellOutput {
    packed::CellOutput::new_builder().capacity::<packed::Uint64>(capacity.pack()).lock(actor.lock()).build()
}

impl<'a> Lifecycle<'a> {
    fn new(compiled: &'a CompileResult) -> Self {
        let mut context = Context::new_with_deterministic_rng();
        let multisig = ckb_system_scripts_v0_6_0::BUNDLED_CELL
            .get("specs/cells/secp256k1_blake160_multisig_all")
            .expect("pinned bundled multisig-v2");
        context.deploy_cell(Bytes::copy_from_slice(&multisig));
        let secp = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_data").unwrap();
        let secp_data = context.deploy_cell(Bytes::copy_from_slice(&secp));
        let elf = Bytes::copy_from_slice(strip_vm_abi_trailer(&compiled.artifact_bytes));
        let code = context.deploy_cell(elf.clone());
        let policy = context
            .build_script_with_hash_type(&code, ScriptHashType::Data2, Actor::Issuer.lock().calc_script_hash().as_bytes())
            .unwrap();
        assert_eq!(policy.code_hash(), packed::CellOutput::calc_data_hash(&elf));

        let mut live = HashMap::new();
        let mut genesis_index = 0_u32;
        let mut genesis = |actor, capacity| {
            let output = plain_cell(actor, capacity);
            let mut preimage = b"cellscript-fungible-lifecycle-genesis-v1".to_vec();
            preimage.extend_from_slice(&genesis_index.to_le_bytes());
            let out_point = packed::OutPoint::new_builder().tx_hash(blake2b_256(&preimage).pack()).index(genesis_index).build();
            genesis_index += 1;
            context.create_cell_with_out_point(out_point.clone(), output.clone(), Bytes::new());
            live.insert(out_point.clone(), (output, Bytes::new()));
            out_point
        };
        let funding = genesis(Actor::FeePayer, 1_000 * TOKEN_CAPACITY);
        let issuer = genesis(Actor::Issuer, 10 * TOKEN_CAPACITY);
        let attacker = genesis(Actor::Bob, 10 * TOKEN_CAPACITY);
        Self { compiled, context, policy, secp_data, live, funding, issuer, attacker }
    }

    fn prepare(&mut self, action: Action, consumed: &[packed::OutPoint], amounts: &[u64], recipient: Actor) -> Pending {
        let inputs = std::iter::once(self.funding.clone()).chain(consumed.iter().cloned()).collect::<Vec<_>>();
        let total: u64 = inputs.iter().map(|input| u64::from(self.live[input].0.capacity())).sum();
        let mut outputs = vec![plain_cell(Actor::FeePayer, 0)];
        let mut data = vec![Bytes::new()];
        for amount in amounts {
            outputs.push(plain_cell(recipient, TOKEN_CAPACITY).as_builder().type_(Some(self.policy.clone()).pack()).build());
            data.push(Bytes::copy_from_slice(&amount.to_le_bytes()));
        }
        if matches!(action, Action::Mint { .. }) {
            let authority = &self.live[&consumed[0]].0;
            let capacity = u64::from(authority.capacity()) - TOKEN_CAPACITY;
            outputs.push(authority.clone().as_builder().capacity::<packed::Uint64>(capacity.pack()).build());
            data.push(Bytes::new());
        }
        let allocated: u64 = outputs.iter().skip(1).map(|output| u64::from(output.capacity())).sum();
        outputs[0] = plain_cell(Actor::FeePayer, total.checked_sub(allocated + FEE).expect("funded scenario transaction"));

        let mut groups: Vec<SigningGroup> = Vec::new();
        for (index, out_point) in inputs.iter().enumerate() {
            let lock = self.live[out_point].0.lock();
            if let Some(existing) = groups.iter_mut().find(|signing| signing.group.script == lock) {
                existing.group.input_indices.push(index);
            } else {
                let mut group = ScriptGroup::from_lock_script(&lock);
                group.input_indices.push(index);
                groups.push(SigningGroup { actor: Actor::from_lock(&lock), group });
            }
        }
        let mut witnesses = vec![packed::WitnessArgs::default(); inputs.len()];
        for signing in &groups {
            witnesses[signing.group.input_indices[0]] = signing.actor.config().placeholder_witness();
        }
        let selected = encode_policy_action_record(
            &self.compiled.metadata,
            &self.policy.calc_script_hash().unpack(),
            action.name(),
            &action.args(recipient),
        )
        .unwrap();
        let bundle = encode_policy_witness_bundle(&[
            PolicyWitnessRecord {
                role: PolicyScriptRole::Type,
                script_hash: selected.script_hash,
                tag: selected.tag,
                args: selected.args,
            },
            PolicyWitnessRecord {
                role: PolicyScriptRole::Lock,
                script_hash: Actor::FeePayer.lock().calc_script_hash().unpack(),
                tag: 900,
                args: Vec::new(),
            },
        ])
        .unwrap();
        witnesses[1] = place_policy_witness_bundle_before_signing(&witnesses[1], &bundle).unwrap();
        let transaction = TransactionBuilder::default()
            .inputs(inputs.into_iter().map(|out_point| packed::CellInput::new_builder().previous_output(out_point).build()))
            .outputs(outputs)
            .outputs_data(data.pack())
            .witnesses(witnesses.into_iter().map(|witness| witness.as_bytes().pack()))
            .cell_dep(packed::CellDep::new_builder().out_point(self.secp_data.clone()).dep_type(DepType::Code).build())
            .build();
        let unsigned = self.context.complete_tx(transaction);
        let mut pending = Pending { signed: unsigned.clone(), unsigned, groups };
        pending.signed = pending.sign(&pending.unsigned, true);
        pending
    }

    fn check_live(&self, transaction: &TransactionView) -> Result<(), String> {
        for input in transaction.inputs() {
            if !self.live.contains_key(&input.previous_output()) {
                return Err("non-live local input".to_string());
            }
        }
        Ok(())
    }

    fn commit(&mut self, transaction: &TransactionView) -> Result<Vec<packed::OutPoint>, String> {
        self.check_live(transaction)?;
        self.context.verify_tx(transaction, MAX_CYCLES).map_err(|error| format!("{error:?}"))?;
        for input in transaction.inputs() {
            self.live.remove(&input.previous_output()).unwrap();
        }
        let mut out_points = Vec::new();
        for (index, output) in transaction.outputs().into_iter().enumerate() {
            let data = transaction.outputs_data().get(index).unwrap().raw_data();
            let out_point = packed::OutPoint::new(transaction.hash(), index as u32);
            self.context.create_cell_with_out_point(out_point.clone(), output.clone(), data.clone());
            self.live.insert(out_point.clone(), (output, data));
            out_points.push(out_point);
        }
        self.funding = out_points[0].clone();
        Ok(out_points)
    }

    fn reject(&self, transaction: &TransactionView) -> String {
        self.check_live(transaction).expect("negative scenario inputs are live");
        format!("{:?}", self.context.verify_tx(transaction, MAX_CYCLES).expect_err("scenario must reject"))
    }
}

fn transaction_identity(transaction: &TransactionView) -> (String, String) {
    (
        format!("0x{}", hex::encode(transaction.hash().as_slice())),
        format!("0x{}", hex::encode(blake2b_256(transaction.data().as_slice()))),
    )
}

fn case_record(
    scenario: &str,
    outcome: &str,
    rejection_stage: &str,
    expected_exit_code: Option<i64>,
    transaction: &TransactionView,
) -> serde_json::Value {
    let (raw_transaction_hash, serialized_transaction_hash) = transaction_identity(transaction);
    serde_json::json!({
        "scenario": scenario,
        "outcome": outcome,
        "rejection_stage": rejection_stage,
        "expected_exit_code": expected_exit_code,
        "raw_transaction_hash": raw_transaction_hash,
        "serialized_transaction_hash": serialized_transaction_hash,
    })
}

fn rejection_exit_code(detail: &str) -> i64 {
    for marker in ["error code ", "error code: "] {
        if let Some(start) = detail.find(marker).map(|index| index + marker.len()) {
            let digits =
                detail[start..].chars().take_while(|character| character.is_ascii_digit() || *character == '-').collect::<String>();
            if let Ok(code) = digits.parse() {
                return code;
            }
        }
    }
    panic!("rejection did not expose an exact script exit code: {detail}");
}

#[test]
fn fungible_business_inventory_scenarios_are_exact() {
    let compiled = compile_policy();
    let mut cases = Vec::new();
    let mut lifecycle = Lifecycle::new(&compiled);

    let mint = lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 12 }, &[lifecycle.issuer.clone()], &[12], Actor::Alice);
    cases.push(case_record("authorized_mint", "positive", "ckb-vm", Some(0), &mint.signed));
    let replay = mint.signed.clone();
    let outputs = lifecycle.commit(&mint.signed).expect("scenario mint");
    lifecycle.issuer = outputs[2].clone();
    assert!(lifecycle.check_live(&replay).unwrap_err().contains("non-live"));
    cases.push(case_record("replay", "adversarial", "local-live-set", None, &replay));

    let minted = outputs[1].clone();
    let transfer = lifecycle.prepare(Action::Transfer, std::slice::from_ref(&minted), &[12], Actor::Bob);
    cases.push(case_record("transfer", "positive", "ckb-vm", Some(0), &transfer.signed));
    let missing = lifecycle.prepare(Action::Transfer, std::slice::from_ref(&minted), &[], Actor::Bob);
    let exit = rejection_exit_code(&lifecycle.reject(&missing.signed));
    cases.push(case_record("missing_or_extra_cell", "adversarial", "ckb-vm", Some(exit), &missing.signed));
    let wrong_amount = lifecycle.prepare(Action::Transfer, std::slice::from_ref(&minted), &[13], Actor::Bob);
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_amount.signed));
    cases.push(case_record("wrong_amount", "adversarial", "ckb-vm", Some(exit), &wrong_amount.signed));
    let unsigned_owner = transfer.sign(&transfer.unsigned, false);
    let exit = rejection_exit_code(&lifecycle.reject(&unsigned_owner));
    cases.push(case_record("wrong_authority", "adversarial", "ckb-vm", Some(exit), &unsigned_owner));

    let outputs = lifecycle.commit(&transfer.signed).expect("scenario transfer");
    let split = lifecycle.prepare(Action::Split { left_amount: 5 }, &outputs[1..2], &[5, 7], Actor::Bob);
    cases.push(case_record("bounded_split", "positive", "ckb-vm", Some(0), &split.signed));
    let outputs = lifecycle.commit(&split.signed).expect("scenario split");
    let merge = lifecycle.prepare(Action::Merge, &outputs[1..3], &[12], Actor::Bob);
    cases.push(case_record("bounded_merge", "positive", "ckb-vm", Some(0), &merge.signed));
    let outputs = lifecycle.commit(&merge.signed).expect("scenario merge");
    let burn = lifecycle.prepare(Action::Burn, &outputs[1..2], &[], Actor::Bob);
    cases.push(case_record("burn", "positive", "ckb-vm", Some(0), &burn.signed));
    lifecycle.commit(&burn.signed).expect("scenario burn");

    let wrong_identity =
        lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 7 }, &[lifecycle.attacker.clone()], &[7], Actor::Alice);
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_identity.signed));
    cases.push(case_record("wrong_identity", "adversarial", "ckb-vm", Some(exit), &wrong_identity.signed));

    let mut overflow_lifecycle = Lifecycle::new(&compiled);
    let maximum = overflow_lifecycle.prepare(
        Action::Mint { issuer_input: 1, amount: u64::MAX },
        &[overflow_lifecycle.issuer.clone()],
        &[u64::MAX],
        Actor::Alice,
    );
    let outputs = overflow_lifecycle.commit(&maximum.signed).expect("maximum token mint");
    overflow_lifecycle.issuer = outputs[2].clone();
    let maximum_token = outputs[1].clone();
    let one = overflow_lifecycle.prepare(
        Action::Mint { issuer_input: 1, amount: 1 },
        &[overflow_lifecycle.issuer.clone()],
        &[1],
        Actor::Alice,
    );
    let outputs = overflow_lifecycle.commit(&one.signed).expect("one token mint");
    overflow_lifecycle.issuer = outputs[2].clone();
    let overflow_inputs = [maximum_token, outputs[1].clone()];
    let overflow = overflow_lifecycle.prepare(Action::Merge, &overflow_inputs, &[0], Actor::Alice);
    let exit = rejection_exit_code(&overflow_lifecycle.reject(&overflow.signed));
    cases.push(case_record("overflow", "adversarial", "ckb-vm", Some(exit), &overflow.signed));

    cases.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let actual = serde_json::json!({
        "schema": "cellscript-fungible-scenarios-v1",
        "source_file": "tests/fixtures/fungible_scenarios.cell",
        "artifact_identities": {
            "artifact_hash": format!("0x{}", compiled.metadata.artifact_hash.as_deref().expect("artifact hash")),
            "lowering_record_hash": format!("0x{}", compiled.metadata.verified_artifact.lowering_record_hash.as_deref().expect("lowering hash")),
            "source_map_hash": format!("0x{}", compiled.metadata.verified_artifact.source_map_hash.as_deref().expect("source-map hash")),
            "verified_bundle_id": format!("0x{}", compiled.metadata.verified_artifact.verified_bundle_id.as_deref().expect("bundle id")),
        },
        "cases": cases,
    });
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/fungible_scenarios.json")).expect("fungible scenario fixture");
    assert_eq!(actual, fixture, "recorded fungible scenario identities are stale: {actual}");

    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/business_scenario_evidence.json")).expect("business scenario evidence JSON");
    let family = &manifest["families"]["fungible_asset"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("fungible scenario records");
    assert_eq!(records.len(), 11, "fungible inventory requires five positive and six adversarial records");
    for case in actual["cases"].as_array().expect("executed fungible cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/fungible_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([actual["artifact_identities"]["artifact_hash"]]));
    }
}

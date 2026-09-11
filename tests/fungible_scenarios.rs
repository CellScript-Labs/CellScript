//! Exact signed, persistent transactions for the fungible business inventory.

#![cfg(not(feature = "wasm"))]

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    compile_with_executable_surface_policy, strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg,
    ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{
    encode_policy_witness_bundle, place_policy_witness_bundle_before_signing, PolicyScriptRole, PolicyWitnessRecord,
};
use ckb_sdk::{
    constants::MultisigScript,
    traits::SecpCkbRawKeySigner,
    types::ScriptGroup,
    unlock::{generate_message, MultisigConfig, ScriptSigner, SecpMultisigScriptSigner},
    util::serialize_signature,
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
use secp256k1::{Message, PublicKey, SecretKey};
use std::collections::HashMap;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script};

const SOURCE: &str = include_str!("fixtures/fungible_scenarios.cell");
const NFT_SOURCE: &str = include_str!("fixtures/nft_scenarios.cell");
const NFT_CAPACITY_SOURCE: &str = include_str!("fixtures/nft_capacity_scenario.cell");
const TOKEN_CAPACITY: u64 = 100_000_000_000;
const FEE: u64 = 100_000_000;
const MAX_CYCLES: u64 = 100_000_000;
const SIGNATURE_OFFSET: usize = 4 + 2 * 20;

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

fn compile_nft_policy() -> CompileResult {
    let compiled = compile_artifact(
        NFT_SOURCE,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            opt_level: 2,
            target: Some("riscv64-elf".to_string()),
            ..Default::default()
        },
        ArtifactDeclaration {
            name: "NftScenarioPolicy".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Nft".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: [(0, "mint"), (17, "update"), (33, "transfer"), (u32::MAX, "burn")]
                .into_iter()
                .map(|(tag, action)| ArtifactAction { tag, action: action.to_string() })
                .collect(),
            common_checks: Vec::new(),
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("NFT scenario policy: {error}"));
    compiled.validate().expect("independent NFT scenario artifact validation");
    compiled
}

fn compile_nft_capacity() -> CompileResult {
    let compiled = compile_with_executable_surface_policy(
        NFT_CAPACITY_SOURCE,
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..Default::default()
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect("NFT capacity artifact compiles");
    compiled.validate().expect("independent NFT capacity artifact validation");
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

#[derive(Clone, Copy)]
enum NftAction {
    Mint { issuer_input: u64, identity: u64 },
    Update,
    Transfer,
    Burn,
}

impl NftAction {
    fn name(self) -> &'static str {
        match self {
            Self::Mint { .. } => "mint",
            Self::Update => "update",
            Self::Transfer => "transfer",
            Self::Burn => "burn",
        }
    }

    fn args(self, recipient: Actor) -> Vec<EntryWitnessArg> {
        let recipient = EntryWitnessArg::Address(recipient.lock().calc_script_hash().unpack());
        match self {
            Self::Mint { issuer_input, identity } => {
                vec![EntryWitnessArg::U64(issuer_input), EntryWitnessArg::U64(identity), recipient]
            }
            Self::Update => vec![recipient],
            Self::Transfer => vec![recipient],
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
        let target_keys = include_target.then(|| self.target().actor.keys());
        self.sign_with_target_keys(transaction, target_keys)
    }

    fn sign_with_target_keys(&self, transaction: &TransactionView, target_keys: Option<Vec<SecretKey>>) -> TransactionView {
        let mut signed = transaction.clone();
        for signing in &self.groups {
            let keys = if signing.group.input_indices[0] == 1 {
                match &target_keys {
                    Some(keys) => keys.clone(),
                    None => continue,
                }
            } else {
                signing.actor.keys()
            };
            let signer =
                SecpMultisigScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(keys)), signing.actor.config());
            signed = signer.sign_tx(&signed, &signing.group).expect("sign completed scenario transaction");
        }
        signed
    }

    fn target(&self) -> &SigningGroup {
        self.groups.iter().find(|signing| signing.group.input_indices[0] == 1).expect("scenario target Lock group")
    }

    fn message(&self, transaction: &TransactionView) -> Bytes {
        let target = self.target();
        let zero_lock = target.actor.config().placeholder_witness().lock().to_opt().unwrap().raw_data();
        generate_message(transaction, &target.group, zero_lock).expect("canonical owning Lock group message")
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

    fn prepare_nft(
        &mut self,
        action: NftAction,
        consumed: &[packed::OutPoint],
        states: &[u64],
        output_owner: Actor,
        declared_owner: Actor,
        output_capacity: u64,
        native_output: bool,
    ) -> Pending {
        let inputs = std::iter::once(self.funding.clone()).chain(consumed.iter().cloned()).collect::<Vec<_>>();
        let total: u64 = inputs.iter().map(|input| u64::from(self.live[input].0.capacity())).sum();
        let mut outputs = vec![plain_cell(Actor::FeePayer, 0)];
        let mut data = vec![Bytes::new()];
        for state in states {
            let mut builder = plain_cell(output_owner, output_capacity).as_builder();
            if native_output {
                builder = builder.type_(Some(self.policy.clone()).pack());
            }
            outputs.push(builder.build());
            data.push(Bytes::copy_from_slice(&state.to_le_bytes()));
        }
        if matches!(action, NftAction::Mint { .. }) {
            let authority = &self.live[&consumed[0]].0;
            let capacity = u64::from(authority.capacity()) - output_capacity;
            outputs.push(authority.clone().as_builder().capacity::<packed::Uint64>(capacity.pack()).build());
            data.push(Bytes::new());
        }
        let allocated: u64 = outputs.iter().skip(1).map(|output| u64::from(output.capacity())).sum();
        outputs[0] = plain_cell(Actor::FeePayer, total.checked_sub(allocated + FEE).expect("funded NFT transaction"));

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
            &action.args(declared_owner),
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
                tag: 901,
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

fn replace_witness(
    transaction: &TransactionView,
    index: usize,
    change: impl FnOnce(packed::WitnessArgs) -> packed::WitnessArgs,
) -> TransactionView {
    let current = packed::WitnessArgs::from_slice(transaction.witnesses().get(index).unwrap().raw_data().as_ref()).unwrap();
    let mut witnesses = transaction.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[index] = change(current).as_bytes().pack();
    transaction.as_advanced_builder().set_witnesses(witnesses).build()
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

#[test]
fn authorization_business_inventory_scenarios_are_exact() {
    let compiled = compile_policy();
    let mut lifecycle = Lifecycle::new(&compiled);
    let mut cases = Vec::new();
    let valid = lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 10 }, &[lifecycle.issuer.clone()], &[10], Actor::Alice);
    lifecycle.context.verify_tx(&valid.signed, MAX_CYCLES).expect("fully signed standard multisig transaction");
    assert_eq!(valid.target().actor.config().threshold(), 2);
    assert_eq!(valid.target().actor.config().sighash_addresses().len(), 2);
    assert_eq!(lifecycle.policy.hash_type(), ScriptHashType::Data2.into());
    assert_eq!(lifecycle.policy.args().raw_data(), Actor::Issuer.lock().calc_script_hash().as_bytes());
    for scenario in ["standard_lock", "multisig_threshold", "issuer_authority", "exact_script_identity"] {
        cases.push(case_record(scenario, "positive", "ckb-vm", Some(0), &valid.signed));
    }

    let mut outputs = valid.signed.outputs().into_iter().collect::<Vec<_>>();
    let changed_capacity = u64::from(outputs[0].capacity()) - 1;
    outputs[0] = outputs[0].clone().as_builder().capacity::<packed::Uint64>(changed_capacity.pack()).build();
    let post_signing_mutation = valid.signed.as_advanced_builder().set_outputs(outputs).build();
    let exit = rejection_exit_code(&lifecycle.reject(&post_signing_mutation));
    cases.push(case_record("post_signing_mutation", "adversarial", "ckb-vm", Some(exit), &post_signing_mutation));

    let partial_signature = valid.sign_with_target_keys(&valid.unsigned, Some(vec![Actor::Issuer.keys()[0]]));
    let exit = rejection_exit_code(&lifecycle.reject(&partial_signature));
    cases.push(case_record("partial_signature", "adversarial", "ckb-vm", Some(exit), &partial_signature));

    let message = Message::from_digest(valid.message(&valid.signed).as_ref().try_into().unwrap());
    let wrong_signature = serialize_signature(&SECP256K1.sign_ecdsa_recoverable(&message, &key(0x7f)));
    let wrong_key = replace_witness(&valid.signed, 1, |witness| {
        let mut lock = witness.lock().to_opt().unwrap().raw_data().to_vec();
        lock[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 65].copy_from_slice(&wrong_signature);
        witness.as_builder().lock(Some(Bytes::from(lock)).pack()).build()
    });
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_key));
    cases.push(case_record("wrong_key", "adversarial", "ckb-vm", Some(exit), &wrong_key));

    let alternate = lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 11 }, &[lifecycle.issuer.clone()], &[11], Actor::Alice);
    let valid_owner_witness = valid.signed.witnesses().get(1).unwrap().raw_data();
    let wrong_domain = replace_witness(&alternate.signed, 1, |_| packed::WitnessArgs::from_slice(&valid_owner_witness).unwrap());
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_domain));
    cases.push(case_record("wrong_domain", "adversarial", "ckb-vm", Some(exit), &wrong_domain));

    let replay = valid.signed.clone();
    let outputs = lifecycle.commit(&valid.signed).expect("authorization seed mint");
    lifecycle.issuer = outputs[2].clone();
    assert!(lifecycle.check_live(&replay).unwrap_err().contains("non-live"));
    cases.push(case_record("replay", "adversarial", "local-live-set", None, &replay));

    let alice_transfer = lifecycle.prepare(Action::Transfer, &outputs[1..2], &[10], Actor::Alice);
    let alice_witness = alice_transfer.signed.witnesses().get(1).unwrap().raw_data();
    let mut copied_lifecycle = Lifecycle::new(&compiled);
    let bob_mint =
        copied_lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 10 }, &[copied_lifecycle.issuer.clone()], &[10], Actor::Bob);
    let outputs = copied_lifecycle.commit(&bob_mint.signed).expect("copied-owner seed mint");
    copied_lifecycle.issuer = outputs[2].clone();
    let bob_transfer = copied_lifecycle.prepare(Action::Transfer, &outputs[1..2], &[10], Actor::Alice);
    let copied_owner = replace_witness(&bob_transfer.signed, 1, |_| packed::WitnessArgs::from_slice(&alice_witness).unwrap());
    let exit = rejection_exit_code(&copied_lifecycle.reject(&copied_owner));
    cases.push(case_record("copied_owner", "adversarial", "ckb-vm", Some(exit), &copied_owner));

    cases.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let actual = serde_json::json!({
        "schema": "cellscript-authorization-scenarios-v1",
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
        serde_json::from_str(include_str!("fixtures/authorization_scenarios.json")).expect("authorization scenario fixture");
    assert_eq!(actual, fixture, "recorded authorization scenario identities are stale: {actual}");

    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/business_scenario_evidence.json")).expect("business scenario evidence JSON");
    let family = &manifest["families"]["authorization"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("authorization scenario records");
    assert_eq!(records.len(), 10, "authorization inventory requires four positive and six adversarial records");
    for case in actual["cases"].as_array().expect("executed authorization cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/authorization_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([actual["artifact_identities"]["artifact_hash"]]));
    }
}

#[test]
fn nft_business_inventory_scenarios_are_exact() {
    let compiled = compile_nft_policy();
    let capacity_compiled = compile_nft_capacity();
    let mut lifecycle = Lifecycle::new(&compiled);
    let mut cases = Vec::new();

    let mint = lifecycle.prepare_nft(
        NftAction::Mint { issuer_input: 1, identity: 7 },
        &[lifecycle.issuer.clone()],
        &[7],
        Actor::Alice,
        Actor::Alice,
        TOKEN_CAPACITY,
        true,
    );
    cases.push(case_record("mint_unique", "positive", "ckb-vm", Some(0), &mint.signed));
    let duplicate = lifecycle.prepare_nft(
        NftAction::Mint { issuer_input: 1, identity: 7 },
        &[lifecycle.issuer.clone()],
        &[7, 7],
        Actor::Alice,
        Actor::Alice,
        TOKEN_CAPACITY,
        true,
    );
    let exit = rejection_exit_code(&lifecycle.reject(&duplicate.signed));
    cases.push(case_record("duplicate_identity", "adversarial", "ckb-vm", Some(exit), &duplicate.signed));
    let outputs = lifecycle.commit(&mint.signed).expect("NFT mint");
    lifecycle.issuer = outputs[2].clone();

    let nft = outputs[1].clone();
    let update =
        lifecycle.prepare_nft(NftAction::Update, std::slice::from_ref(&nft), &[8], Actor::Alice, Actor::Alice, TOKEN_CAPACITY, true);
    cases.push(case_record("metadata_update", "positive", "ckb-vm", Some(0), &update.signed));
    let stale =
        lifecycle.prepare_nft(NftAction::Update, std::slice::from_ref(&nft), &[7], Actor::Alice, Actor::Alice, TOKEN_CAPACITY, true);
    let exit = rejection_exit_code(&lifecycle.reject(&stale.signed));
    cases.push(case_record("stale_state", "adversarial", "ckb-vm", Some(exit), &stale.signed));
    let wrong_lock =
        lifecycle.prepare_nft(NftAction::Update, std::slice::from_ref(&nft), &[8], Actor::Bob, Actor::Alice, TOKEN_CAPACITY, true);
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_lock.signed));
    cases.push(case_record("wrong_lock", "adversarial", "ckb-vm", Some(exit), &wrong_lock.signed));
    let unauthorized = update.sign(&update.unsigned, false);
    let exit = rejection_exit_code(&lifecycle.reject(&unauthorized));
    cases.push(case_record("unauthorized_update", "adversarial", "ckb-vm", Some(exit), &unauthorized));
    let outputs = lifecycle.commit(&update.signed).expect("NFT metadata update");

    let nft = outputs[1].clone();
    let transfer =
        lifecycle.prepare_nft(NftAction::Transfer, std::slice::from_ref(&nft), &[8], Actor::Bob, Actor::Bob, TOKEN_CAPACITY, true);
    cases.push(case_record("ownership_transfer", "positive", "ckb-vm", Some(0), &transfer.signed));
    let outputs = lifecycle.commit(&transfer.signed).expect("NFT ownership transfer");

    let nft = outputs[1].clone();
    let wrong_type =
        lifecycle.prepare_nft(NftAction::Transfer, std::slice::from_ref(&nft), &[8], Actor::Bob, Actor::Bob, TOKEN_CAPACITY, false);
    let exit = rejection_exit_code(&lifecycle.reject(&wrong_type.signed));
    cases.push(case_record("wrong_type", "adversarial", "ckb-vm", Some(exit), &wrong_type.signed));
    let mut capacity_fixture = build_simple_fixture(Bytes::from_static(b"nft-capacity"), 1, 1);
    capacity_fixture.current_type_script_input_indices = vec![0];
    let nft_data = 8_u64.to_le_bytes();
    capacity_fixture.inputs[0].data = Bytes::copy_from_slice(&nft_data);
    capacity_fixture.outputs[0].data = Bytes::copy_from_slice(&nft_data);
    capacity_fixture.inputs[0].capacity = TOKEN_CAPACITY;
    capacity_fixture.outputs[0].capacity = TOKEN_CAPACITY + 1_000_000_000;
    let capacity_execution = execute_cellscript_script(strip_vm_abi_trailer(&capacity_compiled.artifact_bytes), &capacity_fixture);
    assert_eq!(capacity_execution.exit_code, 0, "NFT capacity adjustment: {:?}", capacity_execution.captured_debug);
    cases.push(serde_json::json!({
        "scenario": "capacity_adjustment",
        "outcome": "positive",
        "rejection_stage": "ckb-vm",
        "expected_exit_code": 0,
        "raw_transaction_hash": capacity_execution.raw_transaction_hash,
        "serialized_transaction_hash": capacity_execution.serialized_transaction_hash,
    }));

    let burn = lifecycle.prepare_nft(NftAction::Burn, std::slice::from_ref(&nft), &[], Actor::Bob, Actor::Bob, TOKEN_CAPACITY, true);
    cases.push(case_record("burn", "positive", "ckb-vm", Some(0), &burn.signed));
    lifecycle.commit(&burn.signed).expect("NFT burn");

    cases.sort_by(|left, right| left["scenario"].as_str().cmp(&right["scenario"].as_str()));
    let actual = serde_json::json!({
        "schema": "cellscript-nft-scenarios-v1",
        "source_files": ["tests/fixtures/nft_scenarios.cell", "tests/fixtures/nft_capacity_scenario.cell"],
        "artifact_identities": {
            "policy": {
                "artifact_hash": format!("0x{}", compiled.metadata.artifact_hash.as_deref().expect("artifact hash")),
                "lowering_record_hash": format!("0x{}", compiled.metadata.verified_artifact.lowering_record_hash.as_deref().expect("lowering hash")),
                "source_map_hash": format!("0x{}", compiled.metadata.verified_artifact.source_map_hash.as_deref().expect("source-map hash")),
                "verified_bundle_id": format!("0x{}", compiled.metadata.verified_artifact.verified_bundle_id.as_deref().expect("bundle id")),
            },
            "capacity": {
                "artifact_hash": format!("0x{}", capacity_compiled.metadata.artifact_hash.as_deref().expect("capacity artifact hash")),
                "lowering_record_hash": format!("0x{}", capacity_compiled.metadata.verified_artifact.lowering_record_hash.as_deref().expect("capacity lowering hash")),
                "source_map_hash": format!("0x{}", capacity_compiled.metadata.verified_artifact.source_map_hash.as_deref().expect("capacity source-map hash")),
                "verified_bundle_id": format!("0x{}", capacity_compiled.metadata.verified_artifact.verified_bundle_id.as_deref().expect("capacity bundle id")),
            },
        },
        "cases": cases,
    });
    let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/nft_scenarios.json")).expect("NFT scenario fixture");
    assert_eq!(actual, fixture, "recorded NFT scenario identities are stale: {actual}");

    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/business_scenario_evidence.json")).expect("business scenario evidence JSON");
    let family = &manifest["families"]["nft_dob"];
    assert_eq!(family["coverage_status"], "exact-artifact-fixtures");
    assert_eq!(family["gaps"], serde_json::json!([]));
    let records = family["records"].as_array().expect("NFT scenario records");
    assert_eq!(records.len(), 10, "NFT inventory requires five positive and five adversarial records");
    for case in actual["cases"].as_array().expect("executed NFT cases") {
        let scenario = case["scenario"].as_str().unwrap();
        let record = records
            .iter()
            .find(|record| record["scenario"] == scenario && record["outcome"] == case["outcome"])
            .unwrap_or_else(|| panic!("missing exact business-scenario record for {scenario}"));
        let artifact = if scenario == "capacity_adjustment" {
            &actual["artifact_identities"]["capacity"]["artifact_hash"]
        } else {
            &actual["artifact_identities"]["policy"]["artifact_hash"]
        };
        assert_eq!(record["status"], "exact-artifact-fixture");
        assert_eq!(record["fixture"], "tests/fixtures/nft_scenarios.json");
        assert_eq!(record["raw_transaction_hash"], case["raw_transaction_hash"]);
        assert_eq!(record["serialized_transaction_hash"], case["serialized_transaction_hash"]);
        assert_eq!(record["artifact_hashes"], serde_json::json!([artifact]));
    }
}

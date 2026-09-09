//! Issuer-authorized, persistent Token lifecycle in real CKB-VM. Positive Token
//! Cells are outputs of earlier verified transactions, never pre-created phase
//! fixtures. A local live-Cell map supplies bookkeeping, not node admission or
//! chain confirmation evidence. The workspace SDK (including local changes) is
//! used as-is; this is not clean-source production or semantic-equivalence proof.
//! Signatures authenticate the owning Lock group's witnesses, not every other
//! group's witness. All merge inputs deliberately have the same owner.

#![cfg(not(feature = "wasm"))]

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    strip_vm_abi_trailer, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use cellscript_ckb_adapter::policy_witness::{
    decode_policy_witness_bundle, encode_policy_witness_bundle, place_policy_witness_bundle_before_signing, PolicyScriptRole,
    PolicyWitnessRecord,
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
        core::{Capacity, DepType, ScriptHashType, TransactionBuilder, TransactionView},
        packed,
        prelude::*,
        H160,
    },
    context::Context,
};
use secp256k1::{Message, PublicKey, SecretKey};
use std::collections::{HashMap, HashSet};

#[path = "support/cryptographic_resource_budgets.rs"]
mod cryptographic_resource_budgets;

use cryptographic_resource_budgets::{assert_metrics, ResourceMetrics};

const SOURCE: &str = r#"
module issuer_authorized_token
resource Token has store, consume { amount: u64 }
action mint(witness issuer_input: u64, witness amount: u64, witness recipient: Address) {
    verification
    require amount > 0
    let issuer = ckb::cell_type_args32(source::group_output(0))
    ckb::require_cell_lock_hash(source::input(issuer_input), issuer)
    create Token { amount: amount } with_lock(recipient)
}
action transfer(input before: Token, witness recipient: Address) {
    verification
    require before.amount > 0
    let amount = before.amount
    consume before
    create Token { amount: amount } with_lock(recipient)
}
action merge(input left: Token, input right: Token, witness recipient: Address) {
    verification
    require left.amount > 0
    require right.amount > 0
    require left.amount <= 18446744073709551615 - right.amount
    let amount = left.amount + right.amount
    consume left
    consume right
    create Token { amount: amount } with_lock(recipient)
}
action burn(input before: Token) {
    verification
    require before.amount > 0
    consume before
}
"#;

const TOKEN_CAPACITY: u64 = 100_000_000_000;
const FEE: u64 = 100_000_000;
const MAX_CYCLES: u64 = 100_000_000;
const SIGNATURE_OFFSET: usize = 4 + 2 * 20;
const SOURCE_VIEW_SHIFT: u64 = 1 << 32;

fn compile_policy(edition: CellScriptEdition, opt_level: u8) -> CompileResult {
    let compiled = compile_artifact(
        SOURCE,
        CompileOptions { edition, opt_level, target: Some("riscv64-elf".to_string()), ..Default::default() },
        ArtifactDeclaration {
            name: "IssuerAuthorizedToken".to_string(),
            context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
            dispatch: ArtifactDispatch::PolicyWitnessV1,
            actions: [0, 17, 255, u32::MAX]
                .into_iter()
                .zip(["mint", "transfer", "merge", "burn"])
                .map(|(tag, action)| ArtifactAction { tag, action: action.to_string() })
                .collect(),
            common_checks: Vec::new(),
        },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("issuer policy {edition:?}/opt{opt_level}: {error}"));
    compiled.validate().expect("independent bounded artifact validation");
    compiled
}

fn key(byte: u8) -> SecretKey {
    SecretKey::from_slice(&[byte; 32]).expect("fixed test-only secret key")
}

#[derive(Clone, Copy, Debug)]
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
            .expect("every lifecycle input has a real known multisig Lock")
    }
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Mint { issuer_input: u64, amount: u64 },
    Transfer,
    Merge,
    Burn,
}

impl Action {
    fn name(self) -> &'static str {
        match self {
            Self::Mint { .. } => "mint",
            Self::Transfer => "transfer",
            Self::Merge => "merge",
            Self::Burn => "burn",
        }
    }

    fn args(self, recipient: Actor) -> Vec<EntryWitnessArg> {
        let recipient = EntryWitnessArg::Address(recipient.lock().calc_script_hash().unpack());
        match self {
            Self::Mint { issuer_input, amount } => vec![EntryWitnessArg::U64(issuer_input), EntryWitnessArg::U64(amount), recipient],
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
    fn sign(&self, transaction: &TransactionView, target_keys: Option<Vec<SecretKey>>) -> TransactionView {
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
            signed = signer.sign_tx(&signed, &signing.group).expect("sign completed transaction and placed policy witness");
        }
        signed
    }

    fn target(&self) -> &SigningGroup {
        self.groups.iter().find(|signing| signing.group.input_indices[0] == 1).unwrap()
    }

    fn resign(&self, transaction: &TransactionView) -> TransactionView {
        let mut unsigned = transaction.clone();
        for signing in &self.groups {
            unsigned = replace_witness(&unsigned, signing.group.input_indices[0], |witness| {
                witness.as_builder().lock(signing.actor.config().placeholder_witness().lock()).build()
            });
        }
        self.sign(&unsigned, Some(self.target().actor.keys()))
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
    committed: usize,
    max_cycles: u64,
    max_tx_bytes: usize,
    max_witness_bytes: usize,
    max_occupied_bytes: u64,
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
        assert_eq!(policy.args().raw_data(), Actor::Issuer.lock().calc_script_hash().as_bytes());

        // Only ordinary funding/authority Cells exist initially. No Token is
        // inserted here or in any negative test to stand in for a prior phase.
        let mut live = HashMap::new();
        let mut genesis = |actor, capacity| {
            let output = plain_cell(actor, capacity);
            let out_point = context.create_cell(output.clone(), Bytes::new());
            live.insert(out_point.clone(), (output, Bytes::new()));
            out_point
        };
        let funding = genesis(Actor::FeePayer, 1_000 * TOKEN_CAPACITY);
        let issuer = genesis(Actor::Issuer, 10 * TOKEN_CAPACITY);
        let attacker = genesis(Actor::Bob, 10 * TOKEN_CAPACITY);
        Self {
            compiled,
            context,
            policy,
            secp_data,
            live,
            funding,
            issuer,
            attacker,
            committed: 0,
            max_cycles: 0,
            max_tx_bytes: 0,
            max_witness_bytes: 0,
            max_occupied_bytes: 0,
        }
    }

    fn prepare(&mut self, action: Action, consumed: &[packed::OutPoint], amounts: &[u64], recipient: Actor) -> Pending {
        let inputs = std::iter::once(self.funding.clone()).chain(consumed.iter().cloned()).collect::<Vec<_>>();
        let total: u64 =
            inputs.iter().map(|input| u64::from(self.live.get(input).expect("input must still be live").0.capacity())).sum();
        let mut outputs = vec![plain_cell(Actor::FeePayer, 0)];
        let mut data = vec![Bytes::new()];
        for amount in amounts {
            outputs.push(plain_cell(recipient, TOKEN_CAPACITY).as_builder().type_(Some(self.policy.clone()).pack()).build());
            data.push(Bytes::copy_from_slice(&amount.to_le_bytes()));
        }
        if matches!(action, Action::Mint { .. }) {
            let authority = &self.live.get(&consumed[0]).unwrap().0;
            let capacity = u64::from(authority.capacity()) - TOKEN_CAPACITY;
            outputs.push(authority.clone().as_builder().capacity::<packed::Uint64>(capacity.pack()).build());
            data.push(Bytes::new());
        }
        let allocated: u64 = outputs.iter().skip(1).map(|output| u64::from(output.capacity())).sum();
        outputs[0] = plain_cell(Actor::FeePayer, total.checked_sub(allocated + FEE).expect("funded test transaction"));

        let mut groups: Vec<SigningGroup> = Vec::new();
        for (index, out_point) in inputs.iter().enumerate() {
            let lock = self.live.get(out_point).unwrap().0.lock();
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
        let records = [
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
        ];
        let bundle = encode_policy_witness_bundle(&records).unwrap();
        witnesses[1] = place_policy_witness_bundle_before_signing(&witnesses[1], &bundle).unwrap();
        let transaction = TransactionBuilder::default()
            .inputs(inputs.into_iter().map(|out_point| packed::CellInput::new_builder().previous_output(out_point).build()))
            .outputs(outputs)
            .outputs_data(data.pack())
            .witnesses(witnesses.into_iter().map(|witness| witness.as_bytes().pack()))
            .cell_dep(packed::CellDep::new_builder().out_point(self.secp_data.clone()).dep_type(DepType::Code).build())
            .build();
        // The complete transaction and all witness bytes precede signing.
        let unsigned = self.context.complete_tx(transaction);
        let mut pending = Pending { signed: unsigned.clone(), unsigned, groups };
        pending.signed = pending.sign(&pending.unsigned, Some(pending.target().actor.keys()));
        pending
    }

    fn check_live(&self, transaction: &TransactionView) -> Result<(), String> {
        let mut seen = HashSet::new();
        for input in transaction.inputs() {
            let out_point = input.previous_output();
            if !seen.insert(out_point.clone()) || !self.live.contains_key(&out_point) {
                return Err(format!("non-live or duplicate local input: {out_point}"));
            }
        }
        Ok(())
    }

    fn commit(&mut self, transaction: &TransactionView) -> Result<Vec<packed::OutPoint>, String> {
        self.check_live(transaction)?;
        let cycles = self.context.verify_tx(transaction, MAX_CYCLES).map_err(|error| format!("{error:?}"))?;
        let input_capacity: u64 =
            transaction.inputs().into_iter().map(|input| u64::from(self.live[&input.previous_output()].0.capacity())).sum();
        let output_capacity: u64 = transaction.outputs().into_iter().map(|output| u64::from(output.capacity())).sum();
        assert_eq!(input_capacity - output_capacity, FEE, "local capacity accounting, not full node admission");
        for input in transaction.inputs() {
            self.live.remove(&input.previous_output()).unwrap();
        }
        let mut out_points = Vec::new();
        for (index, output) in transaction.outputs().into_iter().enumerate() {
            let data = transaction.outputs_data().get(index).unwrap().raw_data();
            let occupied = output.occupied_capacity(Capacity::bytes(data.len()).unwrap()).unwrap();
            assert!(u64::from(output.capacity()) >= occupied.as_u64());
            self.max_occupied_bytes = self.max_occupied_bytes.max(occupied.as_u64() / 100_000_000);
            if let Some(type_script) = output.type_().to_opt() {
                assert_eq!(type_script, self.policy, "all Token Cells retain the identical full persistent policy Script");
            }
            let out_point = packed::OutPoint::new(transaction.hash(), index as u32);
            assert!(!self.live.contains_key(&out_point));
            self.context.create_cell_with_out_point(out_point.clone(), output.clone(), data.clone());
            assert_eq!(self.context.get_cell(&out_point), Some((output.clone(), data.clone())));
            self.live.insert(out_point.clone(), (output, data));
            out_points.push(out_point);
        }
        self.funding = out_points[0].clone();
        self.committed += 1;
        self.max_cycles = self.max_cycles.max(cycles);
        self.max_tx_bytes = self.max_tx_bytes.max(transaction.data().as_slice().len());
        self.max_witness_bytes =
            self.max_witness_bytes.max(transaction.witnesses().into_iter().map(|witness| witness.raw_data().len()).sum());
        Ok(out_points)
    }

    fn assert_rejects(&self, transaction: &TransactionView, script_kind: &str) -> String {
        self.check_live(transaction).expect("negative exercises live inputs, not replay rejection");
        let error = self.context.verify_tx(transaction, MAX_CYCLES).expect_err("invalid signed transaction must reject");
        let detail = format!("{error:?}");
        assert!(detail.contains("ValidationFailure"), "expected Script failure: {detail}");
        let origin = if script_kind == "Lock" {
            detail.contains("Inputs[1].Lock") || detail.contains("Inputs(1, Lock)")
        } else {
            ["Inputs[1].Type", "Inputs(1, Type)", "Outputs[1].Type", "Outputs(1, Type)"].iter().any(|origin| detail.contains(origin))
        };
        assert!(origin, "rejection must reach actual {script_kind} at Token/authority index1: {detail}");
        detail
    }

    fn assert_type_exit(&self, transaction: &TransactionView, code: u64) {
        let detail = self.assert_rejects(transaction, "Type");
        assert!(
            detail.contains(&format!("error code {code}")) || detail.contains(&format!("error code: {code}")),
            "expected exact Type failure {code}: {detail}"
        );
    }

    fn assert_authorization(&self, pending: &Pending) {
        // Fee payer stays correctly signed. Only the issuer/owner credentials
        // at input1 are absent, incomplete, or cryptographically wrong.
        self.assert_rejects(&pending.sign(&pending.unsigned, None), "Lock");
        self.assert_rejects(&pending.sign(&pending.unsigned, Some(vec![pending.target().actor.keys()[0]])), "Lock");
        let message = Message::from_digest(pending.message(&pending.signed).as_ref().try_into().unwrap());
        let wrong_signature = serialize_signature(&SECP256K1.sign_ecdsa_recoverable(&message, &key(0x7f)));
        let wrong = replace_witness(&pending.signed, 1, |witness| {
            let mut lock = witness.lock().to_opt().unwrap().raw_data().to_vec();
            lock[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 65].copy_from_slice(&wrong_signature);
            witness.as_builder().lock(Some(Bytes::from(lock)).pack()).build()
        });
        self.assert_rejects(&wrong, "Lock");

        // An opaque sibling record is policy-valid before and after mutation.
        // This isolates authentication of the actual owned witness bytes.
        let changed = replace_witness(&pending.signed, 1, |witness| {
            let mut records = decode_policy_witness_bundle(&witness.input_type().to_opt().unwrap().raw_data()).unwrap();
            records.iter_mut().find(|record| record.role == PolicyScriptRole::Lock).unwrap().tag += 1;
            witness.as_builder().input_type(Some(Bytes::from(encode_policy_witness_bundle(&records).unwrap())).pack()).build()
        });
        assert_eq!(changed.hash(), pending.signed.hash(), "witness mutation does not change raw transaction hash");
        assert_ne!(pending.message(&changed), pending.message(&pending.signed));
        self.assert_rejects(&changed, "Lock");
        self.context.verify_tx(&pending.resign(&changed), MAX_CYCLES).expect("policy-valid witness change succeeds after re-signing");
    }
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
fn signed_persistent_policy_executes_six_transactions_with_live_prior_outputs() {
    let multisig_dependency_bytes = ckb_system_scripts_v0_6_0::BUNDLED_CELL
        .get("specs/cells/secp256k1_blake160_multisig_all")
        .expect("pinned bundled multisig-v2")
        .len()
        + ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_data").expect("pinned secp data").len();
    let mut measured = ResourceMetrics {
        cycles: 0,
        elf_bytes: 0,
        max_stack_frame_bytes: 0,
        witness_bytes: 0,
        transaction_bytes: 0,
        dependency_bytes: multisig_dependency_bytes,
    };
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            let compiled = compile_policy(edition, opt_level);
            measured.elf_bytes = measured.elf_bytes.max(strip_vm_abi_trailer(&compiled.artifact_bytes).len());
            measured.max_stack_frame_bytes = measured.max_stack_frame_bytes.max(
                compiled
                    .verified_lowering_record
                    .iter()
                    .flat_map(|record| &record.entries)
                    .map(|entry| entry.frame_size_bytes)
                    .max()
                    .expect("policy lowering record contains an entry"),
            );
            let mut lifecycle = Lifecycle::new(&compiled);
            let original_policy = lifecycle.policy.clone();
            let mut minted = Vec::new();
            for amount in [7, 5] {
                let action = Action::Mint { issuer_input: 1, amount };
                let consumed = [lifecycle.issuer.clone()];
                let valid = lifecycle.prepare(action, &consumed, &[amount], Actor::Alice);
                lifecycle.assert_authorization(&valid);
                let wrong_amount = lifecycle.prepare(action, &consumed, &[amount + 1], Actor::Alice);
                lifecycle.assert_rejects(&wrong_amount.signed, "Type");
                let extra_output = lifecycle.prepare(action, &consumed, &[amount, amount], Actor::Alice);
                lifecycle.assert_rejects(&extra_output.signed, "Type");
                let outputs =
                    lifecycle.commit(&valid.signed).unwrap_or_else(|error| panic!("{edition:?}/opt{opt_level} mint: {error}"));
                minted.push(outputs[1].clone());
                lifecycle.issuer = outputs[2].clone();
                let count = lifecycle.live.len();
                assert!(lifecycle.commit(&valid.signed).unwrap_err().contains("non-live"), "local ledger rejects replay");
                assert_eq!(lifecycle.live.len(), count);
            }
            let mut transferred = Vec::new();
            for (token, amount) in minted.into_iter().zip([7, 5]) {
                let consumed = [token];
                let valid = lifecycle.prepare(Action::Transfer, &consumed, &[amount], Actor::Bob);
                lifecycle.assert_authorization(&valid);
                for outputs in [vec![amount + 1], Vec::new(), vec![amount, amount]] {
                    let invalid = lifecycle.prepare(Action::Transfer, &consumed, &outputs, Actor::Bob);
                    lifecycle.assert_rejects(&invalid.signed, "Type");
                }
                let outputs =
                    lifecycle.commit(&valid.signed).unwrap_or_else(|error| panic!("{edition:?}/opt{opt_level} transfer: {error}"));
                transferred.push(outputs[1].clone());
            }
            let merge = lifecycle.prepare(Action::Merge, &transferred, &[12], Actor::Bob);
            lifecycle.assert_authorization(&merge);
            assert_eq!(merge.target().group.input_indices, vec![1, 2], "one real owner Lock authenticates both merge inputs");
            for amounts in [vec![11], vec![13], Vec::new(), vec![12, 12]] {
                let invalid = lifecycle.prepare(Action::Merge, &transferred, &amounts, Actor::Bob);
                lifecycle.assert_rejects(&invalid.signed, "Type");
            }
            let missing_input = lifecycle.prepare(Action::Merge, &transferred[..1], &[12], Actor::Bob);
            lifecycle.assert_rejects(&missing_input.signed, "Type");
            let outputs = lifecycle.commit(&merge.signed).unwrap_or_else(|error| panic!("{edition:?}/opt{opt_level} merge: {error}"));
            let consumed = [outputs[1].clone()];
            let burn = lifecycle.prepare(Action::Burn, &consumed, &[], Actor::Bob);
            lifecycle.assert_authorization(&burn);
            let surviving = lifecycle.prepare(Action::Burn, &consumed, &[12], Actor::Bob);
            lifecycle.assert_rejects(&surviving.signed, "Type");
            lifecycle.commit(&burn.signed).unwrap_or_else(|error| panic!("{edition:?}/opt{opt_level} burn: {error}"));
            assert_eq!(lifecycle.committed, 6);
            assert_eq!(lifecycle.policy, original_policy);
            assert!(lifecycle.live.values().all(|(output, _)| output.type_().to_opt().is_none()), "no live Token remains after burn");
            measured.cycles = measured.cycles.max(lifecycle.max_cycles);
            measured.witness_bytes = measured.witness_bytes.max(lifecycle.max_witness_bytes);
            measured.transaction_bytes = measured.transaction_bytes.max(lifecycle.max_tx_bytes);
            eprintln!(
                "lifecycle {edition:?}/opt{opt_level}: {} committed txs, max cycles={}, max serialized tx bytes={}, max output occupied bytes={}",
                lifecycle.committed, lifecycle.max_cycles, lifecycle.max_tx_bytes, lifecycle.max_occupied_bytes
            );
        }
    }
    eprintln!("standard-multisig-lifecycle={}", serde_json::to_string(&measured).expect("serialize multisig measurements"));
    assert_metrics("standard-multisig-lifecycle", &measured);
}

#[test]
fn canonical_issuer_full_lock_hash_and_checked_merge_are_enforced() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            let compiled = compile_policy(edition, opt_level);
            let mut lifecycle = Lifecycle::new(&compiled);
            assert_eq!(Actor::Issuer.lock().code_hash(), Actor::Bob.lock().code_hash());
            assert_eq!(Actor::Issuer.lock().hash_type(), Actor::Bob.lock().hash_type());
            assert_ne!(Actor::Issuer.lock().args(), Actor::Bob.lock().args());
            let mint = Action::Mint { issuer_input: 1, amount: 7 };
            let wrong_issuer = lifecycle.prepare(mint, &[lifecycle.attacker.clone()], &[7], Actor::Alice);
            lifecycle.assert_type_exit(&wrong_issuer.signed, 47);
            // Reject indexes whose high bits would carry into the encoded view
            // tag. Otherwise this Input request is later decoded as Output #2,
            // allowing an issuer-locked output to counterfeit input authority.
            let carried = lifecycle.prepare(
                Action::Mint { issuer_input: SOURCE_VIEW_SHIFT + 2, amount: 7 },
                &[lifecycle.attacker.clone()],
                &[7],
                Actor::Alice,
            );
            let mut outputs = carried.unsigned.outputs().into_iter().collect::<Vec<_>>();
            outputs[2] = outputs[2].clone().as_builder().lock(Actor::Issuer.lock()).build();
            let carried = carried.unsigned.as_advanced_builder().set_outputs(outputs).build();
            lifecycle.assert_type_exit(&wrong_issuer.resign(&carried), 44);
            // Canonical policy args never change. An issuer Cell present only
            // in CellDeps is not a consumed and authorized Input.
            let dep_only = wrong_issuer
                .unsigned
                .as_advanced_builder()
                .cell_dep(packed::CellDep::new_builder().out_point(lifecycle.issuer.clone()).dep_type(DepType::Code).build())
                .build();
            lifecycle.assert_type_exit(&wrong_issuer.resign(&dep_only), 47);
            for issuer_input in [0, 99] {
                let invalid =
                    lifecycle.prepare(Action::Mint { issuer_input, amount: 7 }, &[lifecycle.issuer.clone()], &[7], Actor::Alice);
                lifecycle.assert_type_exit(&invalid.signed, if issuer_input == 0 { 47 } else { 44 });
            }
            let zero = lifecycle.prepare(Action::Mint { issuer_input: 1, amount: 0 }, &[lifecycle.issuer.clone()], &[0], Actor::Alice);
            lifecycle.assert_type_exit(&zero.signed, 5);
            let mut tokens = Vec::new();
            for amount in [u64::MAX, 1] {
                let valid =
                    lifecycle.prepare(Action::Mint { issuer_input: 1, amount }, &[lifecycle.issuer.clone()], &[amount], Actor::Alice);
                let outputs =
                    lifecycle.commit(&valid.signed).unwrap_or_else(|error| panic!("{edition:?}/opt{opt_level} large mint: {error}"));
                tokens.push(outputs[1].clone());
                lifecycle.issuer = outputs[2].clone();
            }
            let overflow = lifecycle.prepare(Action::Merge, &tokens, &[0], Actor::Alice);
            lifecycle.assert_type_exit(&overflow.signed, 5);
            assert_eq!(lifecycle.committed, 2, "rejected candidates never advance the local ledger");
        }
    }
}

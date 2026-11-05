//! Spending authorization for one persistent Type policy, not issuer-authorized
//! minting or a complete token lifecycle. The consumed Token is guarded by the
//! real bundled multisig-v2 Lock. Policy records occupy that Lock group's first
//! witness before the SDK computes its canonical transaction signing message.
//! No claim is made that another Lock group's witnesses are authenticated.

#![cfg(not(feature = "wasm"))]

use cellscript::{
    artifact::{
        compile_artifact, encode_policy_action_record, ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch,
    },
    strip_vm_abi_trailer, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy, CURRENT_EDITION,
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
        core::{DepType, TransactionBuilder, TransactionView},
        packed,
        prelude::*,
        H160,
    },
    context::Context,
};
use secp256k1::{Message, PublicKey, SecretKey};
use std::sync::OnceLock;

const SOURCE: &str = r#"
module policy_spending_authorization
resource Token has store, consume { amount: u64 }
action transfer(input before: Token, witness recipient: Address, witness memo: u64) {
    require before.amount == 7
    require memo > 0
    let amount = before.amount
    consume before
    create Token { amount: amount } with_lock(recipient)
}
action burn(input before: Token) {
    require before.amount == 7
    consume before
}
"#;

const TRANSFER_TAG: u32 = 17;
const BURN_TAG: u32 = u32::MAX;
const SIGNED_TX_MAX_CYCLES: u64 = 70_000_000;

fn options() -> CompileOptions {
    CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() }
}

fn policy() -> &'static CompileResult {
    static POLICY: OnceLock<CompileResult> = OnceLock::new();
    POLICY.get_or_init(|| {
        compile_artifact(
            SOURCE,
            options(),
            ArtifactDeclaration {
                name: "SpendingPolicy".to_string(),
                context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
                dispatch: ArtifactDispatch::PolicyWitnessV1,
                actions: vec![
                    ArtifactAction { tag: TRANSFER_TAG, action: "transfer".to_string() },
                    ArtifactAction { tag: BURN_TAG, action: "burn".to_string() },
                ],
                common_checks: Vec::new(),
            },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .expect("spending policy compiles without deferred authorization primitives")
    })
}

fn key(byte: u8) -> SecretKey {
    SecretKey::from_slice(&[byte; 32]).expect("fixed test-only secret key")
}

fn signer_id(secret_key: &SecretKey) -> H160 {
    let public_key = PublicKey::from_secret_key(&SECP256K1, secret_key);
    H160::from_slice(&blake2b_256(public_key.serialize())[..20]).expect("20-byte signer id")
}

fn config(first: u8, second: u8) -> MultisigConfig {
    MultisigConfig::new_with(MultisigScript::V2, vec![signer_id(&key(first)), signer_id(&key(second))], 0, 2)
        .expect("canonical test 2-of-2 multisig-v2 config")
}

struct Fixture {
    context: Context,
    unsigned: TransactionView,
    signed: TransactionView,
    owner: MultisigConfig,
    owner_group: ScriptGroup,
    policy_hash: [u8; 32],
}

impl Fixture {
    fn sign(&self, transaction: &TransactionView, keys: Vec<SecretKey>) -> TransactionView {
        let signer = SecpMultisigScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(keys)), self.owner.clone());
        signer.sign_tx(transaction, &self.owner_group).expect("SDK signs the canonical group transaction message")
    }

    fn resign(&self, transaction: &TransactionView) -> TransactionView {
        let unsigned =
            replace_witness(transaction, |witness| witness.as_builder().lock(self.owner.placeholder_witness().lock()).build());
        self.sign(&unsigned, vec![key(0x11), key(0x22)])
    }

    fn message(&self, transaction: &TransactionView) -> Bytes {
        let zero_lock = self.owner.placeholder_witness().lock().to_opt().unwrap().raw_data();
        generate_message(transaction, &self.owner_group, zero_lock).expect("SDK canonical transaction signing message")
    }

    fn assert_owner_lock_rejects(&self, transaction: &TransactionView) {
        let error = self.context.verify_tx(transaction, SIGNED_TX_MAX_CYCLES).expect_err("owner authorization must reject");
        let detail = format!("{error:?}");
        assert!(
            detail.contains("Inputs[1].Lock") || detail.contains("Inputs(1, Lock)"),
            "rejection must come from the actual owner Lock, not the policy's Type rules: {detail}"
        );
        assert!(detail.contains("ValidationFailure"), "expected a real Lock validation failure: {detail}");
    }
}

fn fixture(action: &str) -> Fixture {
    let compiled = policy();
    compiled.validate().expect("checked Type-policy artifact");
    let owner = config(0x11, 0x22);
    let recipient = config(0x33, 0x44);
    let owner_lock: packed::Script = (&owner).into();
    let recipient_lock: packed::Script = (&recipient).into();
    let mut context = Context::new_with_deterministic_rng();
    let multisig = ckb_system_scripts_v0_6_0::BUNDLED_CELL
        .get("specs/cells/secp256k1_blake160_multisig_all")
        .expect("pinned bundled multisig-v2 Script");
    context.deploy_cell(Bytes::copy_from_slice(&multisig));
    let secp_data = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_data").expect("pinned secp256k1 data");
    let secp_data_out_point = context.deploy_cell(Bytes::copy_from_slice(&secp_data));
    let foreign = cellscript::compile("module authorization_foreign\nlock allow() -> bool { true }", options()).unwrap();
    let foreign_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&foreign.artifact_bytes)));
    let foreign_lock = context.build_script(&foreign_code, Bytes::new()).unwrap();
    let policy_code = context.deploy_cell(Bytes::copy_from_slice(strip_vm_abi_trailer(&compiled.artifact_bytes)));
    let policy_script = context.build_script(&policy_code, Bytes::from_static(b"persistent-spending-policy")).unwrap();
    let policy_hash: [u8; 32] = policy_script.calc_script_hash().unpack();

    let funding = context.create_cell(
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(foreign_lock.clone()).build(),
        Bytes::new(),
    );
    let token = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(owner_lock.clone())
            .type_(Some(policy_script.clone()).pack())
            .build(),
        Bytes::copy_from_slice(&7u64.to_le_bytes()),
    );
    let values = if action == "transfer" {
        vec![EntryWitnessArg::Address(recipient_lock.calc_script_hash().unpack()), EntryWitnessArg::U64(1)]
    } else {
        Vec::new()
    };
    let selected = encode_policy_action_record(&compiled.metadata, &policy_hash, action, &values).unwrap();
    let records = vec![
        PolicyWitnessRecord {
            role: PolicyScriptRole::Type,
            script_hash: selected.script_hash,
            tag: selected.tag,
            args: selected.args,
        },
        // This opaque, non-selected record shares the authenticated witness
        // slot. Its unknown tag is not interpreted by the Token Type policy.
        PolicyWitnessRecord {
            role: PolicyScriptRole::Lock,
            script_hash: foreign_lock.calc_script_hash().unpack(),
            tag: 900,
            args: Vec::new(),
        },
    ];
    let bundle = encode_policy_witness_bundle(&records).expect("independent adapter codec encodes both records");
    let unsigned_witness = place_policy_witness_bundle_before_signing(&owner.placeholder_witness(), &bundle)
        .expect("place complete policy bundle before signing; preserve lock placeholder");
    let mut transaction = TransactionBuilder::default()
        .inputs([
            packed::CellInput::new_builder().previous_output(funding).build(),
            packed::CellInput::new_builder().previous_output(token).build(),
        ])
        .output(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>((if action == "transfer" { 90_000_000_000u64 } else { 190_000_000_000u64 }).pack())
                .lock(foreign_lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .cell_dep(packed::CellDep::new_builder().out_point(secp_data_out_point).dep_type(DepType::Code).build())
        .witnesses([Bytes::from_static(b"unrelated-input-zero"), unsigned_witness.as_bytes()].pack());
    if action == "transfer" {
        transaction = transaction
            .output(
                packed::CellOutput::new_builder()
                    .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                    .lock(recipient_lock)
                    .type_(Some(policy_script).pack())
                    .build(),
            )
            .output_data(Bytes::copy_from_slice(&7u64.to_le_bytes()).pack());
    }
    let unsigned = context.complete_tx(transaction.build());
    let mut owner_group = ScriptGroup::from_lock_script(&owner_lock);
    owner_group.input_indices.push(1);
    let mut fixture = Fixture { context, signed: unsigned.clone(), unsigned, owner, owner_group, policy_hash };
    fixture.signed = fixture.sign(&fixture.unsigned, vec![key(0x11), key(0x22)]);
    fixture
}

fn replace_witness(transaction: &TransactionView, change: impl FnOnce(packed::WitnessArgs) -> packed::WitnessArgs) -> TransactionView {
    let current = packed::WitnessArgs::from_slice(transaction.witnesses().get(1).unwrap().raw_data().as_ref()).unwrap();
    let mut witnesses = transaction.witnesses().into_iter().collect::<Vec<_>>();
    witnesses[1] = change(current).as_bytes().pack();
    transaction.as_advanced_builder().set_witnesses(witnesses).build()
}

#[test]
fn same_type_policy_transfer_and_burn_require_real_owner_signatures() {
    for action in ["transfer", "burn"] {
        let fixture = fixture(action);
        let signed_witness = packed::WitnessArgs::from_slice(fixture.signed.witnesses().get(1).unwrap().raw_data().as_ref()).unwrap();
        let lock = signed_witness.lock().to_opt().unwrap().raw_data();
        let signature_offset = 4 + 2 * 20;
        assert_eq!(&lock[..4], &[0, 0, 2, 2]);
        assert!(lock[signature_offset..].iter().any(|byte| *byte != 0));
        fixture.context.verify_tx(&fixture.signed, SIGNED_TX_MAX_CYCLES).expect("real owner Lock and selected Type action pass");
        fixture.assert_owner_lock_rejects(&fixture.unsigned);
        let only_first_signer = fixture.sign(&fixture.unsigned, vec![key(0x11)]);
        fixture.assert_owner_lock_rejects(&only_first_signer);

        // The recipient's valid key signs the real transaction message, but it
        // is not a member of the consumed Cell's ownership Lock configuration.
        let message = Message::from_digest(fixture.message(&fixture.signed).as_ref().try_into().unwrap());
        let wrong_signature = serialize_signature(&SECP256K1.sign_ecdsa_recoverable(&message, &key(0x33)));
        let wrong_key = replace_witness(&fixture.signed, |witness| {
            let mut lock = witness.lock().to_opt().unwrap().raw_data().to_vec();
            lock[signature_offset..signature_offset + 65].copy_from_slice(&wrong_signature);
            witness.as_builder().lock(Some(Bytes::from(lock)).pack()).build()
        });
        fixture.assert_owner_lock_rejects(&wrong_key);
    }
}

#[test]
fn signing_commits_selector_args_and_every_sibling_record_in_the_owned_witness() {
    let fixture = fixture("transfer");
    fixture.context.verify_tx(&fixture.signed, SIGNED_TX_MAX_CYCLES).expect("untampered signed policy");
    let signed_message = fixture.message(&fixture.signed);
    for mutation in ["selector", "args", "sibling-record"] {
        let tampered = replace_witness(&fixture.signed, |witness| {
            let mut records = decode_policy_witness_bundle(&witness.input_type().to_opt().unwrap().raw_data()).unwrap();
            if mutation == "sibling-record" {
                records.iter_mut().find(|record| record.script_hash != fixture.policy_hash).unwrap().tag += 1;
            } else {
                let selected = records.iter_mut().find(|record| record.script_hash == fixture.policy_hash).unwrap();
                if mutation == "selector" {
                    selected.tag = BURN_TAG;
                } else {
                    // Memo1 and memo2 both satisfy the Type policy. Rejection
                    // must therefore come from the signature, not its predicate.
                    let length = selected.args.len();
                    selected.args[length - 8..].copy_from_slice(&2u64.to_le_bytes());
                }
            }
            let bundle = encode_policy_witness_bundle(&records).unwrap();
            witness.as_builder().input_type(Some(Bytes::from(bundle)).pack()).build()
        });
        assert_eq!(fixture.signed.hash(), tampered.hash(), "only witness bytes change; the raw transaction hash does not");
        assert_ne!(signed_message, fixture.message(&tampered), "canonical signing message must bind {mutation}");
        fixture.assert_owner_lock_rejects(&tampered);
        if mutation != "selector" {
            let resigned = fixture.resign(&tampered);
            fixture
                .context
                .verify_tx(&resigned, SIGNED_TX_MAX_CYCLES)
                .expect("same policy-valid modification passes after re-signing");
        }
    }
}

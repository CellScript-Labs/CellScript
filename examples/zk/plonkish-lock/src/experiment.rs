use std::{env, fs, path::Path};

use anyhow::{anyhow, bail, ensure, Context as _, Result};
use ark_bn254::Fr;
use ark_ff::{BigInteger, PrimeField};
use cellscript_zk_lock_plonkish_experiment::{context_scalar, hash, CONTEXT_MISMATCH, DEMO_DIGEST, VERIFICATION_FAILED};
use ckb_testtool::{
    ckb_types::{
        bytes::Bytes,
        core::{ScriptHashType, TransactionBuilder, TransactionView},
        packed::{CellDep, CellInput, CellOutput, OutPoint, WitnessArgs},
        prelude::*,
    },
    context::Context,
};
mod artifacts;
use artifacts::Scheme;
use serde_json::{json, Value};

const CYCLES: u64 = 1_000_000_000;
const CAPACITY: u64 = 1_000 * 100_000_000;

fn digest_bytes() -> Result<[u8; 32]> {
    let digest = DEMO_DIGEST.parse::<Fr>().map_err(|_| anyhow!("invalid fixture digest"))?;
    digest.into_bigint().to_bytes_le().try_into().map_err(|_| anyhow!("unexpected field width"))
}

fn tx_context(tx: &TransactionView) -> Result<[u8; 32]> {
    let hash = tx.hash();
    Ok(context_scalar(hash.as_slice().try_into().context("unexpected raw transaction hash width")?))
}

struct Fixture {
    vm: Context,
    original: TransactionView,
    changed: TransactionView,
    key: Vec<u8>,
    elf: Vec<u8>,
}

impl Fixture {
    fn new(path: &Path, scheme: Scheme) -> Result<Self> {
        let key = scheme.key(&fs::read(path.join("vk.json"))?, 2)?;
        let elf = fs::read(path.join("lock.elf"))?;
        let mut vm = Context::new_with_deterministic_rng();
        let code_dep = vm.deploy_cell(Bytes::copy_from_slice(&elf));
        let key_dep = vm.deploy_cell(Bytes::copy_from_slice(&key));
        let args = [hash(&key), hash(&digest_bytes()?)].concat();
        let lock = vm.build_script_with_hash_type(&code_dep, ScriptHashType::Data2, args.into()).context("missing lock code Cell")?;
        let previous = OutPoint::new_builder().tx_hash([3; 32]).index(0u32).build();
        vm.create_cell_with_out_point(previous.clone(), CellOutput::new_builder().capacity(CAPACITY).lock(lock).build(), Bytes::new());
        let original = TransactionBuilder::default()
            .cell_dep(CellDep::new_builder().out_point(code_dep).build())
            .cell_dep(CellDep::new_builder().out_point(key_dep).build())
            .input(CellInput::new_builder().previous_output(previous).build())
            .output(CellOutput::new_builder().capacity(CAPACITY).build())
            .output_data(Bytes::from_static(&[1]).pack())
            .build();
        let changed = original.as_advanced_builder().set_outputs_data(vec![Bytes::from_static(&[2]).pack()]).build();
        ensure!(tx_context(&original)? != tx_context(&changed)?, "fixture changed transaction does not change context");
        Ok(Self { vm, original, changed, key, elf })
    }
}

fn witness_input(path: &Path, name: &str, tx: &TransactionView) -> Result<()> {
    let input = json!({
        "context": Fr::from_le_bytes_mod_order(&tx_context(tx)?).to_string(),
        "digest": DEMO_DIGEST,
        "preimage": "42",
    });
    fs::write(path.join(format!("{name}.input.json")), serde_json::to_vec(&input)?)?;
    Ok(())
}

fn checked_proof(path: &Path, name: &str, tx: &TransactionView, scheme: Scheme) -> Result<Vec<u8>> {
    let inputs: [String; 2] = serde_json::from_slice(&fs::read(path.join(format!("{name}.public.json")))?)?;
    ensure!(
        inputs == [Fr::from_le_bytes_mod_order(&tx_context(tx)?).to_string(), DEMO_DIGEST.to_string()],
        "proof public inputs do not match the fixture"
    );
    scheme.proof(&fs::read(path.join(format!("{name}.proof.json")))?)
}

fn attach(tx: &TransactionView, proof: &[u8], context: &[u8; 32]) -> Result<(TransactionView, usize)> {
    let mut lock = proof.to_vec();
    lock.extend_from_slice(&2u32.to_le_bytes());
    lock.extend_from_slice(context);
    lock.extend_from_slice(&digest_bytes()?);
    let witness = WitnessArgs::new_builder().lock(Some(Bytes::from(lock)).pack()).build().as_bytes();
    let size = witness.len();
    Ok((tx.as_advanced_builder().set_witnesses(vec![witness.pack()]).build(), size))
}

fn rejected(vm: &Context, tx: &TransactionView, expected: i8) -> Result<()> {
    let error = vm.verify_tx(tx, CYCLES).err().context("negative control was accepted by CKB-VM")?;
    let marker = format!("error code {expected} ");
    let diagnostic = error.to_string();
    ensure!(diagnostic.contains(&marker) || diagnostic.ends_with(marker.trim_end()), "expected exit {expected}, got {diagnostic}");
    Ok(())
}

fn verify(path: &Path, scheme: Scheme, fixture: Fixture) -> Result<Value> {
    let original_context = tx_context(&fixture.original)?;
    let changed_context = tx_context(&fixture.changed)?;
    let original_proof = checked_proof(path, "original", &fixture.original, scheme)?;
    let changed_proof = checked_proof(path, "changed", &fixture.changed, scheme)?;
    let (original, witness_bytes) = attach(&fixture.original, &original_proof, &original_context)?;
    let (replay, _) = attach(&fixture.changed, &original_proof, &original_context)?;
    let (rebound, _) = attach(&fixture.changed, &original_proof, &changed_context)?;
    let (reproved, _) = attach(&fixture.changed, &changed_proof, &changed_context)?;

    let original_cycles = fixture.vm.verify_tx(&original, CYCLES).map_err(|err| anyhow!("original proof: {err}"))?;
    rejected(&fixture.vm, &replay, CONTEXT_MISMATCH)?;
    rejected(&fixture.vm, &rebound, VERIFICATION_FAILED)?;
    let changed_cycles = fixture.vm.verify_tx(&reproved, CYCLES).map_err(|err| anyhow!("changed proof: {err}"))?;

    Ok(json!({
        "proof_system": scheme.name(),
        "lock_elf_bytes": fixture.elf.len(),
        "lock_ckb_data_hash": hex::encode(hash(&fixture.elf)),
        "verification_key_bytes": fixture.key.len(),
        "verification_key_ckb_data_hash": hex::encode(hash(&fixture.key)),
        "proving_key_file_bytes": fs::metadata(path.join("fixture.zkey"))?.len(),
        "proof_bytes": original_proof.len(),
        "public_input_bytes": 64,
        "witness_args_bytes": witness_bytes,
        "transaction_bytes": original.data().as_slice().len(),
        "original_transaction_hash": hex::encode(original.hash().as_slice()),
        "changed_transaction_hash": hex::encode(reproved.hash().as_slice()),
        "original_cycles": original_cycles,
        "changed_cycles": changed_cycles,
        "stale_context_exit": CONTEXT_MISMATCH,
        "fresh_context_stale_proof_exit": VERIFICATION_FAILED,
    }))
}

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let command = args.next().context("expected prepare or verify")?;
    let root = args.next().context("expected experiment directory")?;
    ensure!(args.next().is_none(), "unexpected argument");
    let mut reports = Vec::new();
    for scheme in [Scheme::Groth16, Scheme::Plonk] {
        let path = Path::new(&root).join(scheme.name());
        let fixture = Fixture::new(&path, scheme).with_context(|| format!("prepare {} fixture", scheme.name()))?;
        if command == "prepare" {
            witness_input(&path, "original", &fixture.original)?;
            witness_input(&path, "changed", &fixture.changed)?;
        } else if command == "verify" {
            reports.push(verify(&path, scheme, fixture).with_context(|| format!("verify {} fixture", scheme.name()))?);
        } else {
            bail!("expected prepare or verify");
        }
    }
    if command == "verify" {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "ckb_testtool": "1.1.1", "harness_cycle_limit": CYCLES, "observations": reports }))?
        );
    }
    Ok(())
}

use std::{fs, path::Path, process::Command};

use anyhow::{anyhow, ensure, Context as _, Result};
use cellscript_zk_private_counter::{self as counter, wire, CounterCircuit, Witness};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{
        bytes::Bytes,
        core::{ScriptHashType, TransactionBuilder, TransactionView},
        packed::{CellDep, CellInput, CellOutput, OutPoint, Script, WitnessArgs},
        prelude::*,
    },
    context::Context,
};
use serde_json::{json, Value};

use crate::{artifacts::Scheme, counter_proof, r1cs};

const SCHEMES: [Scheme; 2] = [Scheme::Groth16, Scheme::Plonk];
const CASES: [&str; 3] = ["increment-0-1", "increment-1-2", "changed-raw-tx"];
const CYCLES: u64 = 250_000_000;
const CAPACITY: u64 = 100_000_000_000;
const SECRET: [u8; 32] = [0x23; 32];

fn run_cellc(command: &mut Command) -> Result<Vec<u8>> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "{command:?} failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn compile_parent(dir: &Path, scheme: Scheme, cellc: &Path, key: &[u8], child: &[u8]) -> Result<()> {
    let package = dir.join("parent");
    fs::create_dir_all(package.join("src"))?;
    let child_hash = counter::hash(child);
    let profile = match scheme {
        Scheme::Groth16 => "groth16-bn254-counter-hex4-v1",
        Scheme::Plonk => "plonk-kzg-bn254-compact-counter-hex4-v1",
    };
    let manifest = include_str!("../counter/Cell.toml.in")
        .replace("@SCHEME@", scheme.name())
        .replace("@PROFILE@", profile)
        .replace("@KEY@", &hex::encode(counter::hash(key)))
        .replace("@DOMAIN@", &hex::encode(counter::domain()))
        .replace("@ACTION@", &hex::encode(counter::action()))
        .replace("@CHILD_HASH@", &hex::encode(child_hash));
    fs::write(package.join("Cell.toml"), manifest)?;
    fs::write(package.join("src/main.cell"), include_str!("../counter/main.cell"))?;
    run_cellc(Command::new(cellc).current_dir(&package).args(["lock", "--json"]))?;
    let output = run_cellc(Command::new(cellc).current_dir(&package).args([
        "build",
        "--entry-action",
        "increment",
        "--deny-fail-closed",
        "--frozen",
        "--json",
    ]))?;
    let build: Value = serde_json::from_slice(&output)?;
    let artifact = package.join(build["artifact"].as_str().context("missing CellScript build artifact")?);
    let check = run_cellc(Command::new(cellc).arg("verify-artifact").arg(&artifact).args([
        "--deny-fail-closed",
        "--expect-target-profile",
        "ckb",
        "--json",
    ]))?;
    fs::write(dir.join("parent-build.json"), output)?;
    fs::write(dir.join("parent-check.json"), check)?;
    fs::copy(artifact, dir.join("parent.elf"))?;
    Ok(())
}

fn dep(out_point: OutPoint) -> CellDep {
    CellDep::new_builder().out_point(out_point).build()
}

struct Update {
    tx: TransactionView,
    circuit: CounterCircuit,
}

struct Fixture {
    vm: Context,
    creation: TransactionView,
    updates: [Update; 3],
    extra: OutPoint,
    proof_key: Vec<u8>,
    child: Vec<u8>,
    parent: Vec<u8>,
}

impl Fixture {
    fn new(work: &Path, scheme: Scheme, cellc: Option<&Path>) -> Result<Self> {
        let dir = work.join(scheme.name());
        let child = fs::read(dir.join("child.elf"))?;
        let proof_key = scheme.key(&fs::read(dir.join("vk.json"))?, wire::PUBLIC_INPUT_COUNT as u32)?;
        if let Some(cellc) = cellc {
            compile_parent(&dir, scheme, cellc, &proof_key, &child)?;
        }
        let parent = fs::read(dir.join("parent.elf"))?;
        let lifecycle = fs::read(work.join("lifecycle.elf"))?;
        let mut vm = Context::new_with_deterministic_rng();
        let child_dep = vm.deploy_cell(Bytes::copy_from_slice(&child));
        let key_dep = vm.deploy_cell(Bytes::copy_from_slice(&proof_key));
        let parent_dep = vm.deploy_cell(Bytes::copy_from_slice(&parent));
        let lifecycle_dep = vm.deploy_cell(Bytes::from(lifecycle));
        let lock_dep = vm.deploy_cell(ALWAYS_SUCCESS.clone());
        // A resolvable, unused dependency changes the raw hash without changing the state transition.
        let extra = vm.deploy_cell(Bytes::from_static(b"additional unused CellDep"));
        let lock = vm.build_script(&lock_dep, Bytes::new()).context("missing fixture lock")?;
        let funding = CellOutput::new_builder().capacity(CAPACITY).lock(lock).build();
        let funding_out = OutPoint::new_builder().tx_hash([3; 32]).index(0u32).build();
        vm.create_cell_with_out_point(funding_out.clone(), funding.clone(), Bytes::new());
        let input = CellInput::new_builder().previous_output(funding_out).build();
        let mut type_id_preimage = input.as_slice().to_vec();
        type_id_preimage.extend(0u64.to_le_bytes());
        let args = [counter::hash(&type_id_preimage), counter::hash(&parent)].concat();
        let script = vm
            .build_script_with_hash_type(&lifecycle_dep, ScriptHashType::Data2, args.into())
            .context("missing lifecycle Type Script")?;
        let cell = funding.as_builder().type_(Some(script.clone()).pack()).build();
        let owner = counter::owner(&SECRET);
        let data = |n| Bytes::copy_from_slice(&counter::state(&owner, n));
        let creation =
            vm.complete_tx(TransactionBuilder::default().input(input).output(cell.clone()).output_data(data(0).pack()).build());
        let deps = vec![dep(child_dep), dep(key_dep), dep(parent_dep)];
        let mut increment = |old: u64, previous_tx: &TransactionView| {
            let previous = OutPoint::new_builder().tx_hash(previous_tx.hash()).index(0u32).build();
            vm.create_cell_with_out_point(previous.clone(), cell.clone(), data(old));
            let tx = vm.complete_tx(
                TransactionBuilder::default()
                    .input(CellInput::new_builder().previous_output(previous.clone()).build())
                    .output(cell.clone())
                    .output_data(data(old + 1).pack())
                    .set_cell_deps(deps.clone())
                    .build(),
            );
            update(&script, &previous, old, tx)
        };
        let first = increment(0, &creation)?;
        let second = increment(1, &first.tx)?;
        let changed = first.tx.as_advanced_builder().cell_dep(dep(extra.clone())).build();
        let previous = first.tx.inputs().get(0).context("missing counter input")?.previous_output();
        let changed = update(&script, &previous, 0, changed)?;
        ensure!(first.tx.hash() != changed.tx.hash(), "raw transaction mutation must change the statement");
        Ok(Self { vm, creation, updates: [first, second, changed], extra, proof_key, child, parent })
    }
}

fn update(script: &Script, previous: &OutPoint, old: u64, tx: TransactionView) -> Result<Update> {
    let owner = counter::owner(&SECRET);
    let statement = wire::Statement {
        domain: counter::domain(),
        action: counter::action(),
        script_hash: script.calc_script_hash().as_slice().try_into()?,
        old_data_hash: counter::hash(&counter::state(&owner, old)),
        new_data_hash: counter::hash(&counter::state(&owner, old + 1)),
        input_transaction_hash: previous.tx_hash().as_slice().try_into()?,
        input_output_index: previous.index().unpack(),
        transaction_hash: tx.hash().as_slice().try_into()?,
    };
    Ok(Update {
        tx,
        circuit: CounterCircuit { statement, witness: Witness { secret: SECRET, old_counter: old, new_counter: old + 1 } },
    })
}

pub fn prepare(work: &Path, cellc: &Path) -> Result<()> {
    let cellc = cellc.canonicalize().context("find the cellc binary")?;
    for scheme in SCHEMES {
        let fixture = Fixture::new(work, scheme, Some(&cellc)).with_context(|| format!("prepare {}", scheme.name()))?;
        for (name, update) in CASES.into_iter().zip(fixture.updates) {
            let dir = work.join(scheme.name());
            let cs = r1cs::synthesize(update.circuit)?;
            r1cs::write_witness(&dir.join(format!("{name}.wtns")), &cs)?;
        }
    }
    Ok(())
}

fn attach(tx: &TransactionView, proof: &[u8]) -> (TransactionView, usize) {
    let payload = [b"CSARGv1\0".as_slice(), proof].concat();
    attach_payload(tx, &payload)
}

fn attach_payload(tx: &TransactionView, payload: &[u8]) -> (TransactionView, usize) {
    let witness = WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(payload)).pack()).build().as_bytes();
    let size = witness.len();
    (tx.as_advanced_builder().set_witnesses(vec![witness.pack()]).build(), size)
}

fn accept(vm: &Context, tx: &TransactionView, case: &str) -> Result<u64> {
    vm.verify_tx(tx, CYCLES).map_err(|error| anyhow!("{case}: {error}"))
}

fn reject(vm: &Context, tx: &TransactionView, case: &str, code: i8, rows: &mut Vec<Value>) -> Result<()> {
    let error = vm.verify_tx(tx, CYCLES).err().with_context(|| format!("{case} was accepted"))?;
    let message = error.to_string();
    let expected = format!("error code {code}");
    ensure!(message.contains(&format!("{expected} ")) || message.ends_with(&expected), "{case}: expected {code}, got {message}");
    rows.push(json!({"case": case, "exit_code": code}));
    Ok(())
}

fn substitute(tx: &TransactionView, index: usize, outpoint: &OutPoint) -> TransactionView {
    let mut deps = tx.cell_deps().into_iter().collect::<Vec<_>>();
    deps[index] = dep(outpoint.clone());
    tx.as_advanced_builder().set_cell_deps(deps).build()
}

fn verify_scheme(work: &Path, scheme: Scheme, cellc: &Path) -> Result<Value> {
    let dir = work.join(scheme.name());
    let fixture = Fixture::new(work, scheme, None)?;
    let mut rows = vec![json!({"case": "create", "cycles": accept(&fixture.vm, &fixture.creation, "create")?})];
    let mut proofs = Vec::new();
    for (case, update) in CASES.into_iter().zip(&fixture.updates) {
        let inputs: Vec<String> = serde_json::from_slice(&fs::read(dir.join(format!("{case}.public.json")))?)?;
        let expected = counter::public_inputs(&update.circuit.statement).iter().map(ToString::to_string).collect::<Vec<_>>();
        ensure!(inputs == expected, "{case}: decoded proof inputs differ from the exact live statement");
        let proof = scheme.proof(&fs::read(dir.join(format!("{case}.proof.json")))?)?;
        let proof = match scheme {
            Scheme::Groth16 => proof,
            Scheme::Plonk => counter_proof::compress(proof.as_slice().try_into()?).context("compress PLONK proof")?.to_vec(),
        };
        let encoded = run_cellc(Command::new(cellc).arg("entry-witness").arg(dir.join("parent")).args([
            "--action",
            "increment",
            "--arg",
            &hex::encode(&proof),
            "--json",
        ]))?;
        let encoded: Value = serde_json::from_slice(&encoded)?;
        let payload = hex::decode(encoded["witness_hex"].as_str().context("missing encoded witness")?)?;
        let (tx, witness_bytes) = attach_payload(&update.tx, &payload);
        ensure!(tx.hash() == update.tx.hash(), "{case}: setting witness changed the raw transaction hash");
        rows.push(json!({
            "case": case,
            "cycles": accept(&fixture.vm, &tx, case)?,
            "raw_transaction_hash": hex::encode(tx.hash().as_slice()),
            "transaction_bytes": tx.data().as_slice().len(),
            "witness_args_bytes": witness_bytes,
            "proof_bytes": proof.len(),
        }));
        proofs.push(proof);
    }

    let vm = &fixture.vm;
    let [first, second, changed] = &fixture.updates;
    let stale = &proofs[0];
    reject(vm, &attach(&changed.tx, stale).0, "changed-raw-tx-stale-proof", 1, &mut rows)?;
    reject(vm, &attach(&second.tx, stale).0, "successor-stale-proof", 1, &mut rows)?;
    let mut wrong_magic = [b"CSARGv1\0".as_slice(), stale].concat();
    wrong_magic[0] ^= 1;
    reject(vm, &attach_payload(&first.tx, &wrong_magic).0, "wrong-witness-magic", 25, &mut rows)?;
    let mut corrupt = stale.clone();
    corrupt[0] ^= 1;
    reject(vm, &attach(&first.tx, &corrupt).0, "corrupt-proof", 1, &mut rows)?;
    reject(vm, &attach(&first.tx, &stale[..stale.len() - 1]).0, "truncated-proof", 25, &mut rows)?;
    if matches!(scheme, Scheme::Plonk) {
        let legacy = scheme.proof(&fs::read(dir.join("increment-0-1.proof.json"))?)?;
        reject(vm, &attach(&first.tx, &legacy).0, "uncompressed-proof", 25, &mut rows)?;
    }
    let first = attach(&first.tx, stale).0;
    reject(vm, &substitute(&first, 0, &fixture.extra), "substituted-child", 41, &mut rows)?;
    reject(vm, &substitute(&first, 1, &fixture.extra), "substituted-key", 41, &mut rows)?;
    let skip = first
        .as_advanced_builder()
        .set_outputs_data(vec![Bytes::copy_from_slice(&counter::state(&counter::owner(&SECRET), 2)).pack()])
        .build();
    reject(vm, &skip, "skipped-counter", 1, &mut rows)?;
    let change_owner =
        first.as_advanced_builder().set_outputs_data(vec![Bytes::copy_from_slice(&counter::state(&[7; 32], 1)).pack()]).build();
    reject(vm, &change_owner, "changed-owner", 92, &mut rows)?;

    let build: Value = serde_json::from_slice(&fs::read(dir.join("parent-build.json"))?)?;
    Ok(json!({
        "proof_system": scheme.name(),
        "proof_policy": build["constraints"]["ckb"]["trusted_external_verifiers"][0]["proof_policy"],
        "rows": rows,
        "compiler_version": build["compiler_version"],
        "parent_elf_bytes": fixture.parent.len(),
        "parent_ckb_data_hash": hex::encode(counter::hash(&fixture.parent)),
        "child_elf_bytes": fixture.child.len(),
        "child_ckb_data_hash": hex::encode(counter::hash(&fixture.child)),
        "verification_key_bytes": fixture.proof_key.len(),
        "verification_key_ckb_data_hash": hex::encode(counter::hash(&fixture.proof_key)),
        "proving_key_file_bytes": fs::metadata(dir.join("fixture.zkey"))?.len(),
    }))
}

pub fn verify(work: &Path, cellc: &Path) -> Result<Value> {
    let circuit: Value = serde_json::from_slice(&fs::read(work.join("circuit.json"))?)?;
    let rows = SCHEMES
        .into_iter()
        .map(|scheme| verify_scheme(work, scheme, cellc).with_context(|| format!("verify {}", scheme.name())))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "schema": "cellscript-counter-plonkish-comparison-v1",
        "witness": "CSARGv1 fixed proof in WitnessArgs.input_type",
        "status": "passed",
        "circuit": circuit,
        "statement_bytes": wire::STATEMENT_BYTES,
        "public_inputs": wire::PUBLIC_INPUT_COUNT,
        "lifecycle_elf_bytes": fs::metadata(work.join("lifecycle.elf"))?.len(),
        "vm_version": 2,
        "ckb_vm": "0.24.15",
        "ckb_testtool": "1.1.1",
        "max_cycles": CYCLES,
        "rows": rows,
    }))
}

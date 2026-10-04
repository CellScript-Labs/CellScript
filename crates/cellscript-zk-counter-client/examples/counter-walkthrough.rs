//! Local CKB-VM deployment using the supported counter client.
use anyhow::{ensure, Context as _, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_counter_client::{self as client, Deployment, PreparedIncrement};
use cellscript_zk_private_counter::{self as counter, package, wire, CounterCircuit, Witness};
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};
use serde_json::json;
use std::{fs, path::Path, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!((1..=2).contains(&args.len()), "usage: counter-walkthrough NEW_OUTPUT_DIR [SETUP_PACKAGE]");
    let output = Path::new(&args[0]);
    fs::create_dir(output).context("choose a new output directory; existing runs are never overwritten")?;
    let started = Instant::now();
    // Intentionally public tutorial parameters and owner secret. Never use these for custody.
    let mut rng = StdRng::seed_from_u64(0x434f554e544552);
    let (manifest, pk) = if let Some(path) = args.get(1) {
        package::load_package(Path::new(path))?
    } else {
        println!("1/5 Generating PUBLIC TEST setup (not admissible for production)");
        let (pk, _) = counter::setup(&mut rng)?;
        let manifest = package::write_package(&output.join("public-test-setup"), &pk, package::SetupKind::PublicTestSeed)?;
        (manifest, pk)
    };
    let setup_ms = started.elapsed().as_millis();
    let key = counter::serialize(&pk.vk)?;
    let mut context = Context::new_with_deterministic_rng();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let child = fs::read(
        root.join("contracts/zk-transition-verifier/target/riscv64imac-unknown-none-elf/release/cellscript-zk-transition-verifier"),
    )?;
    let lifecycle = fs::read(
        root.join("contracts/zk-private-counter/script/target/riscv64imac-unknown-none-elf/release/cellscript-counter-lifecycle"),
    )?;
    let child_out = context.deploy_cell(Bytes::from(child.clone()));
    let key_out = context.deploy_cell(Bytes::from(key.clone()));

    println!("2/5 Compile the named, exact-verifier CellScript parent");
    let deployment = Deployment {
        chain_id: "ckb-testnet".into(),
        genesis_hash: hex::encode([0x11; 32]),
        child_tx_hash: hex::encode(child_out.tx_hash().as_slice()),
        child_index: child_out.index().unpack(),
        child_data_hash: hex::encode(counter::hash(&child)),
        verification_key_hash: hex::encode(counter::hash(&key)),
    };
    let cellscript::zk_client::TransitionParent { compiled, handle, source, deploy } = client::parent(&deployment, &child, &key)?;
    fs::write(output.join("deployment.json"), serde_json::to_vec_pretty(&deployment)?)?;
    let parent = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    fs::write(output.join("parent.cell"), source)?;
    fs::write(output.join("parent.elf"), parent)?;
    fs::write(output.join("parent-metadata.json"), serde_json::to_vec_pretty(&compiled.metadata)?)?;
    fs::write(output.join("named-deployment.json"), serde_json::to_vec_pretty(&deploy)?)?;
    fs::write(output.join("exact-handle.bin"), &handle)?;
    let parent_out = context.deploy_cell(Bytes::copy_from_slice(parent));
    let lifecycle_out = context.deploy_cell(Bytes::from(lifecycle));
    let lock_out = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_out, Bytes::new()).context("local lock")?;
    let funding = packed::CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock).build();
    let funding_out = context.create_cell(funding.clone(), Bytes::new());
    let input = packed::CellInput::new_builder().previous_output(funding_out).build();
    let script =
        context.build_script(&lifecycle_out, Bytes::from(client::type_args(&input, 0, parent))).context("local lifecycle script")?;
    let cell = funding.as_builder().type_(Some(script.clone()).pack()).build();
    let secret = [0x23; 32];
    let owner = counter::owner(&secret);
    let data = |n| Bytes::copy_from_slice(&counter::state(&owner, n));

    println!("3/5 Verify creation of counter 0 in CKB-VM");
    let creation =
        context.complete_tx(TransactionBuilder::default().input(input).output(cell.clone()).output_data(data(0).pack()).build());
    let creation_cycles = context.verify_tx(&creation, 250_000_000)?;
    let mut previous = packed::OutPoint::new_builder().tx_hash(creation.hash()).index(0u32).build();
    context.create_cell_with_out_point(previous.clone(), cell.clone(), data(0));
    let deps = vec![dep(child_out), dep(key_out), dep(parent_out)];
    let mut rows = vec![json!({"case":"create", "cycles":creation_cycles})];
    let mut previous_proof: Option<Vec<u8>> = None;

    println!("4/5 Finalize transaction -> derive statement -> prove -> insert witness -> verify");
    for old in 0..2 {
        // complete_tx adds dependencies BEFORE computing the statement and proof.
        let transaction = context.complete_tx(
            TransactionBuilder::default()
                .input(packed::CellInput::new_builder().previous_output(previous.clone()).build())
                .output(cell.clone())
                .output_data(data(old + 1).pack())
                .set_cell_deps(deps.clone())
                .build(),
        );
        let mut prepared = PreparedIncrement::new(transaction.clone(), &[(previous.clone(), cell.clone(), data(old))], &script)?;
        ensure!(prepared.check_signed(&transaction, &transaction).is_err(), "unproved transaction must not pass signing checks");
        let statement = prepared.statement().clone();
        let proving = Instant::now();
        let proof = counter::prove(
            &pk,
            CounterCircuit { statement: statement.clone(), witness: Witness { secret, old_counter: old, new_counter: old + 1 } },
            &mut rng,
        )?;
        let proving_ms = proving.elapsed().as_millis();
        counter::verify(&key, &statement, &proof)?;
        let with_proof = |bytes: &[u8]| {
            transaction.as_advanced_builder().set_witnesses(vec![unchecked_witness(&compiled, bytes, &handle).pack()]).build()
        };
        if let Some(stale) = &previous_proof {
            let error = context.verify_tx(&with_proof(stale), 250_000_000).expect_err("a previous proof must reject");
            ensure!(error.to_string().contains("error code 79"), "unexpected replay failure: {error}");
            rows.push(json!({"case":"replay-old-proof", "expected_error":79, "error":error.to_string()}));
        }
        let signed = prepared.attach_proof(&compiled, &handle, &key, &proof)?; // Fixture Lock needs no signature.
        prepared.check_signed(&signed, &signed)?;
        let forged = signed.as_advanced_builder().set_witnesses(vec![unchecked_witness(&compiled, &[0; 128], &handle).pack()]).build();
        ensure!(prepared.check_signed(&forged, &forged).is_err(), "replacing both comparison inputs must not bypass proof binding");
        let changed = signed.as_advanced_builder().version(1u32).build();
        ensure!(prepared.check_signed(&signed, &changed).is_err(), "raw mutation must require reproving");
        ensure!(signed.hash() == transaction.hash(), "installing witnesses must preserve the raw transaction hash");
        let cycles = context.verify_tx(&signed, 250_000_000)?;
        println!("    {old} -> {}: {cycles} cycles, {proving_ms} ms proving", old + 1);
        rows.push(json!({"case":format!("increment-{old}-{}",old+1), "cycles":cycles, "proving_ms":proving_ms,
            "proof_bytes":proof.len(), "transaction_bytes":signed.data().as_slice().len()}));
        if old == 0 {
            let mut corrupt = proof;
            corrupt[0] ^= 1;
            ensure!(prepared.attach_proof(&compiled, &handle, &key, &corrupt).is_err(), "native verifier must reject corruption");
            ensure!(prepared.check_signed(&signed, &signed).is_err(), "failed proof replacement must invalidate signing");
            let mut wrong_key = key.clone();
            wrong_key[0] ^= 1;
            ensure!(
                prepared.attach_proof(&compiled, &handle, &wrong_key, &proof).unwrap_err().to_string().contains("VK differs"),
                "VK mismatch needs a precise diagnosis"
            );
            let mut wrong_handle = handle.clone();
            wrong_handle[0] ^= 1;
            ensure!(
                prepared
                    .attach_proof(&compiled, &wrong_handle, &key, &proof)
                    .unwrap_err()
                    .to_string()
                    .contains("exact handle differs"),
                "handle mismatch needs a precise diagnosis"
            );
            let recovered = prepared.attach_proof(&compiled, &handle, &key, &proof)?;
            prepared.check_signed(&recovered, &recovered)?;

            let error = context.verify_tx(&with_proof(&corrupt), 250_000_000).expect_err("corrupt proof must reject");
            ensure!(error.to_string().contains("error code 79"), "unexpected corrupt-proof failure: {error}");
            rows.push(json!({"case":"corrupt-proof", "expected_error":79, "error":error.to_string()}));
            let request = wire::Request { verification_key: counter::hash(&key), proof, statement: statement.clone() };
            fs::write(output.join("statement.bin"), statement.encode())?;
            fs::write(output.join("proof.bin"), proof)?;
            fs::write(output.join("transaction.bin"), signed.data().as_slice())?;
            // Public bridge data for the TypeScript example. No secret or proving key is included.
            fs::write(
                output.join("bridge.json"),
                serde_json::to_vec_pretty(&json!({
                    "scope":"local scheduler fixture; not live-chain evidence",
                    "input_data":hex::encode(data(old)), "input_output":hex::encode(cell.as_slice()),
                    "entry_witness":hex::encode(signed.witnesses().get(0).unwrap().raw_data()),
                    "statement": {"domain":hex::encode(statement.domain),"action":hex::encode(statement.action),
                        "scriptHash":hex::encode(statement.script_hash),"oldDataHash":hex::encode(statement.old_data_hash),
                        "newDataHash":hex::encode(statement.new_data_hash),"inputTransactionHash":hex::encode(statement.input_transaction_hash),
                        "inputOutputIndex":statement.input_output_index,"transactionHash":hex::encode(statement.transaction_hash)},
                    "statement_bytes":hex::encode(statement.encode()), "proof":hex::encode(proof),
                    "public_inputs":hex::encode(statement.public_inputs()),"verification_key_hash":hex::encode(counter::hash(&key)),
                    "handle":hex::encode(&handle),"request":hex::encode(request.encode())
                }))?,
            )?;
        }
        previous = packed::OutPoint::new_builder().tx_hash(signed.hash()).index(0u32).build();
        context.create_cell_with_out_point(previous.clone(), cell.clone(), data(old + 1));
        previous_proof = Some(proof.to_vec());
    }
    println!("5/5 Corrupt proof and replay rejected; public artifacts saved to {}", output.display());
    fs::write(
        output.join("walkthrough-report.json"),
        serde_json::to_vec_pretty(&json!({
            "schema":"cellscript-zk-walkthrough-v1","status":"passed","evidence_level":"CKB-VM scheduler",
            "setup_kind":manifest.setup_kind,"setup_or_load_ms":setup_ms,"total_ms":started.elapsed().as_millis(),
            "circuit_sha256":manifest.circuit.r1cs_sha256,"verification_key_data_hash":manifest.verification_key_data_hash,
            "public_test_secret":true,"public_deployment":false,"rows":rows
        }))?,
    )?;
    Ok(())
}

fn dep(out: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(out).build()
}
// Negative test helper deliberately bypasses the client's native proof verification.
fn unchecked_witness(compiled: &cellscript::CompileResult, proof: &[u8], handle: &[u8]) -> Bytes {
    let payload = compiled.metadata.actions[0]
        .entry_witness_args(&[cellscript::EntryWitnessArg::Bytes(proof.to_vec()), cellscript::EntryWitnessArg::Bytes(handle.to_vec())])
        .unwrap();
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

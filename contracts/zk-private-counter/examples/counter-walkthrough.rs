//! A readable application flow; the shared test harness supplies local deployment plumbing.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;

use anyhow::{ensure, Context as _, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
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
    let child = support::child_bytes();
    let lifecycle = support::lifecycle_bytes();
    let child_out = context.deploy_cell(Bytes::from(child.clone()));
    let key_out = context.deploy_cell(Bytes::from(key.clone()));

    println!("2/5 Compile the named, exact-verifier CellScript parent");
    let (compiled, handle, source, deploy) = support::parent_with_package(&child, &key, &child_out, "ckb-testnet", [0x11; 32]);
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
        context.build_script(&lifecycle_out, Bytes::from(support::type_args(&input, 0, parent))).context("local lifecycle script")?;
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
    let deps = vec![support::dep(child_out), support::dep(key_out), support::dep(parent_out)];
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
        let statement = support::statement(&script, &previous, &data(old), &data(old + 1), &transaction);
        let proving = Instant::now();
        let proof = counter::prove(
            &pk,
            CounterCircuit { statement: statement.clone(), witness: Witness { secret, old_counter: old, new_counter: old + 1 } },
            &mut rng,
        )?;
        let proving_ms = proving.elapsed().as_millis();
        counter::verify(&key, &statement, &proof)?;
        let with_proof = |bytes: &[u8]| {
            transaction.as_advanced_builder().set_witnesses(vec![support::witness(&compiled, bytes, &handle).pack()]).build()
        };
        if let Some(stale) = &previous_proof {
            let error = context.verify_tx(&with_proof(stale), 250_000_000).expect_err("a previous proof must reject");
            ensure!(error.to_string().contains("error code 79"), "unexpected replay failure: {error}");
            rows.push(json!({"case":"replay-old-proof", "expected_error":79, "error":error.to_string()}));
        }
        let signed = with_proof(&proof); // The fixture Lock is always-success: no wallet signature.
        ensure!(signed.hash() == transaction.hash(), "installing witnesses must preserve the raw transaction hash");
        let cycles = context.verify_tx(&signed, 250_000_000)?;
        println!("    {old} -> {}: {cycles} cycles, {proving_ms} ms proving", old + 1);
        rows.push(json!({"case":format!("increment-{old}-{}",old+1), "cycles":cycles, "proving_ms":proving_ms,
            "proof_bytes":proof.len(), "transaction_bytes":signed.data().as_slice().len()}));
        if old == 0 {
            let mut corrupt = proof;
            corrupt[0] ^= 1;
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
                    "scope":"local scheduler fixture; host-derived statement; no RPC or wallet",
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

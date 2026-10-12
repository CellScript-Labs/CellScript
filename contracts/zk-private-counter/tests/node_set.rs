//! Focused multi-counter node evidence. A prior receipt authenticates only the
//! pinned CKB executable; new application evidence is recorded independently.
#[allow(dead_code)]
#[path = "../../../crates/cellscript-tools/src/ckb_devnet.rs"]
mod devnet;
#[allow(dead_code)] // Shared single-counter fixture helpers include unused legacy paths.
mod support;
use anyhow::{ensure, Context, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, CounterCircuit, Witness};
use ckb_types::{
    bytes::Bytes,
    core::{TransactionBuilder, TransactionView},
    packed,
    prelude::*,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn rpc_tx(tx: &TransactionView) -> Value {
    serde_json::to_value(ckb_jsonrpc_types::Transaction::from(tx.data())).unwrap()
}
fn point(value: &Value) -> packed::OutPoint {
    serde_json::from_value::<ckb_jsonrpc_types::OutPoint>(value.clone()).unwrap().into()
}
fn tip(node: &devnet::CkbDevnet) -> Result<()> {
    let expected = node.rpc("get_tip_header", vec![])?;
    for _ in 0..80 {
        if node.rpc("tx_pool_info", vec![])?["tip_hash"] == expected["hash"] {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    anyhow::bail!("pool did not reach chain tip")
}
fn commit(node: &devnet::CkbDevnet, tx: &TransactionView) -> Result<Value> {
    tip(node)?;
    let hash = node.rpc("send_transaction", vec![rpc_tx(tx), json!("passthrough")])?;
    for _ in 0..80 {
        let status = node.rpc("get_transaction", vec![hash.clone()])?;
        if status["tx_status"]["status"] == "committed" {
            tip(node)?;
            return Ok(status["tx_status"].clone());
        }
        ensure!(status["tx_status"]["status"] != "rejected", "{status}");
        node.rpc("generate_block", vec![])?;
        std::thread::sleep(Duration::from_millis(100));
    }
    anyhow::bail!("transaction did not commit: {hash}")
}

#[test]
#[ignore = "explicit pinned local-node multi-counter conflict/recovery; no gates"]
fn counter_set_node_conflict_and_recovery() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize()?;
    let repo = PathBuf::from(std::env::var_os("CELLSCRIPT_CKB_REPO").context("pinned CKB repository")?);
    let receipt_bytes = fs::read(std::env::var_os("CELLSCRIPT_SET_CKB_RECEIPT").context("CKB binary provenance receipt")?)?;
    let receipt: Value = serde_json::from_slice(&receipt_bytes)?;
    let provenance = &receipt["ckb_runtime_provenance"];
    let bin = PathBuf::from(provenance["binary_path"].as_str().context("binary path")?);
    ensure!(format!("0x{}", package::sha256(&fs::read(&bin)?)) == provenance["binary_sha256"], "CKB binary changed");
    let pin: Value = serde_json::from_slice(&fs::read(root.join("scripts/ckb_acceptance_pin.json"))?)?;
    let git = |args: &[&str]| -> Result<String> {
        let output = Command::new("git").arg("-C").arg(&repo).args(args).output()?;
        ensure!(output.status.success(), "CKB source identity unavailable");
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    };
    ensure!(git(&["rev-parse", "HEAD"])? == pin["revision"], "unpinned CKB source");
    ensure!(git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty(), "dirty CKB source");
    ensure!(provenance["revision"] == pin["revision"] && provenance["repo_dirty"] == false, "invalid runtime provenance");
    let run = root.join("target/counter-set-node").join(format!(
        "{}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs(),
        std::process::id()
    ));
    let mut node = devnet::CkbDevnet::new(repo, bin, run.clone())?;
    node.start()?;
    let genesis = node.get_block_by_number(0)?;
    let always = devnet::always_success_dep(genesis["transactions"][0]["hash"].as_str().unwrap());
    let lock: packed::Script = serde_json::from_value::<ckb_jsonrpc_types::Script>(devnet::always_success_lock("0x"))?.into();
    let child = support::child_bytes();
    let lifecycle_path =
        root.join("contracts/zk-private-counter/script/target/riscv64imac-unknown-none-elf/release/cellscript-counter-set");
    let lifecycle = fs::read(&lifecycle_path)?;
    let mut rng = StdRng::seed_from_u64(0x53455401);
    let pk = counter::setup(&mut rng)?.0;
    let vk = counter::serialize(&pk.vk)?;
    let child_deploy = devnet::deploy_code(&mut node, "set-child", &child, &always)?;
    let vk_deploy = devnet::deploy_code(&mut node, "set-vk", &vk, &always)?;
    let genesis_hash = hex::decode(genesis["header"]["hash"].as_str().unwrap().trim_start_matches("0x"))?.try_into().unwrap();
    let (compiled, handle) = support::parent(&child, &vk, &point(&child_deploy["cell_dep"]["out_point"]), genesis_hash);
    let parent = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    let parent_deploy = devnet::deploy_code(&mut node, "set-parent", parent, &always)?;
    let lifecycle_deploy = devnet::deploy_code(&mut node, "set-lifecycle", &lifecycle, &always)?;
    let deps = [&child_deploy["cell_dep"], &vk_deploy["cell_dep"], &parent_deploy["cell_dep"], &lifecycle_deploy["cell_dep"], &always]
        .map(|d| support::dep(point(&d["out_point"])))
        .to_vec();
    fs::write(run.join("parent.elf"), parent)?;
    fs::write(run.join("verification-key.bin"), &vk)?;
    let mut rows = Vec::new();
    for count in [1usize, 2, 4] {
        let funding = node.collect_spendable((count as u64 * 1000 + 100) * devnet::SHANNONS)?;
        let inputs = devnet::funding_cells(&funding)
            .iter()
            .map(|c| {
                packed::CellInput::new_builder()
                    .previous_output(point(&devnet::out_point(c["tx_hash"].as_str().unwrap(), c["index"].as_u64().unwrap())))
                    .build()
            })
            .collect::<Vec<_>>();
        let mut preimage = inputs[0].as_slice().to_vec();
        preimage.extend(0u64.to_le_bytes());
        let family = counter::hash(&preimage);
        let scripts = (0..count)
            .map(|lane| {
                let mut args = b"CSZKSET1".to_vec();
                args.extend(family);
                args.push(lane as u8);
                args.extend(counter::hash(parent));
                packed::Script::new_builder()
                    .code_hash(counter::hash(&lifecycle))
                    .hash_type(4u8)
                    .args(Bytes::from(args).pack())
                    .build()
            })
            .collect::<Vec<_>>();
        let cells = scripts
            .iter()
            .map(|script| {
                packed::CellOutput::new_builder()
                    .capacity(devnet::STATE_CAPACITY)
                    .lock(lock.clone())
                    .type_(Some(script.clone()).pack())
                    .build()
            })
            .collect::<Vec<_>>();
        let data = |lane: usize, n: u64| Bytes::from(counter::state(&counter::owner(&[0x21 + lane as u8; 32]), n).to_vec());
        let change_capacity = funding["total_capacity"].as_u64().unwrap() - count as u64 * devnet::STATE_CAPACITY - 100_000;
        let change = packed::CellOutput::new_builder().capacity(change_capacity).lock(lock.clone()).build();
        let mut create = TransactionBuilder::default().set_inputs(inputs).set_cell_deps(deps.clone());
        for (i, cell) in cells.iter().enumerate() {
            create = create.output(cell.clone()).output_data(data(i, 0).pack());
        }
        let create = create.output(change.clone()).output_data(Bytes::new().pack()).build();
        let creation_dry = node.dry_run(&rpc_tx(&create))?;
        let creation_commit = commit(&node, &create)?;
        let build = |previous: &TransactionView, n: u64, fee: u64| {
            let mut tx = TransactionBuilder::default().set_cell_deps(deps.clone());
            for (i, cell) in cells.iter().enumerate() {
                tx = tx
                    .input(
                        packed::CellInput::new_builder()
                            .previous_output(packed::OutPoint::new_builder().tx_hash(previous.hash()).index(i as u32).build())
                            .build(),
                    )
                    .output(cell.clone())
                    .output_data(data(i, n + 1).pack());
            }
            let old_change = previous.outputs().get(count).unwrap();
            let capacity: u64 = old_change.capacity().unpack();
            tx.input(
                packed::CellInput::new_builder()
                    .previous_output(packed::OutPoint::new_builder().tx_hash(previous.hash()).index(count as u32).build())
                    .build(),
            )
            .output(old_change.as_builder().capacity(capacity - fee).build())
            .output_data(Bytes::new().pack())
            .build()
        };
        let mut prove = |tx: TransactionView, n: u64| -> Result<TransactionView> {
            let mut witnesses = Vec::new();
            for (i, script) in scripts.iter().enumerate() {
                let proof = counter::prove(
                    &pk,
                    CounterCircuit {
                        statement: support::statement(
                            script,
                            &tx.inputs().get(i).unwrap().previous_output(),
                            &data(i, n),
                            &data(i, n + 1),
                            &tx,
                        ),
                        witness: Witness { secret: [0x21 + i as u8; 32], old_counter: n, new_counter: n + 1 },
                    },
                    &mut rng,
                )?;
                witnesses.push(support::witness(&compiled, &proof, &handle).pack());
            }
            Ok(tx.as_advanced_builder().set_witnesses(witnesses).build())
        };
        let first = prove(build(&create, 0, 100_000), 0)?;
        let competing = prove(build(&create, 0, 200_000), 0)?;
        let dry_first = node.dry_run(&rpc_tx(&first))?;
        let dry_competing = node.dry_run(&rpc_tx(&competing))?;
        let committed = commit(&node, &first)?;
        for i in 0..count {
            node.wait_dead_cell(&format!("0x{}", hex::encode(create.hash().as_slice())), i as u64)?;
            let live = node.wait_live_cell(&format!("0x{}", hex::encode(first.hash().as_slice())), i as u64)?;
            ensure!(live["cell"]["data"]["content"] == format!("0x{}", hex::encode(data(i, 1))), "resolved successor data changed");
        }
        let conflict = node
            .rpc("send_transaction", vec![rpc_tx(&competing), json!("passthrough")])
            .expect_err("spent protected inputs must reject")
            .to_string();
        // The pinned node returns Resolve(Unknown) for an already committed,
        // spent output (pool conflicts may use Dead). Both candidates passed
        // dry-run against these exact live inputs before the first committed.
        let first_spent = hex::encode(competing.inputs().get(0).unwrap().previous_output().as_slice());
        ensure!(
            (conflict.contains("Resolve(Unknown(") || conflict.contains("Resolve(Dead(")) && conflict.contains(&first_spent),
            "conflict did not resolve the exact spent protected input: {conflict}"
        );
        let rebuilt = build(&first, 1, 100_000);
        let replay = rebuilt.clone().as_advanced_builder().set_witnesses(first.witnesses().into_iter().collect()).build();
        let rejected = node.dry_run_rejects(&rpc_tx(&replay), "same-proof-successor-replay", None, None, Some(79))?;
        let recovered = prove(rebuilt, 1)?;
        let recovery_dry = node.dry_run(&rpc_tx(&recovered))?;
        let recovery_commit = commit(&node, &recovered)?;
        for dry in [&dry_first, &dry_competing, &recovery_dry] {
            ensure!(
                u64::from_str_radix(dry["cycles"].as_str().unwrap().trim_start_matches("0x"), 16)? <= 500_000_000,
                "frozen cycle ceiling"
            );
        }
        for tx in [&first, &competing, &recovered] {
            ensure!(tx.data().as_slice().len() <= 16_384, "frozen transaction ceiling");
        }
        rows.push(json!({"protected_inputs":count,"creation_dry_run":creation_dry,"creation_commit":creation_commit,
            "first_dry_run":dry_first,"competing_dry_run_before_spend":dry_competing,"first_commit":committed,
            "conflicting_spend_rejection":conflict,"successor_replay_rejection":rejected,"recovery_dry_run":recovery_dry,"recovery_commit":recovery_commit,
            "transactions":([create,first,competing,recovered].iter().map(|tx|json!({"hash":hex::encode(tx.hash().as_slice()),"bytes":hex::encode(tx.data().as_slice()),"size":tx.data().as_slice().len()})).collect::<Vec<_>>())}));
    }
    node.stop();
    let source = devnet::provenance(
        &root,
        &[
            PathBuf::from("src"),
            PathBuf::from("contracts/zk-private-counter/src"),
            PathBuf::from("contracts/zk-private-counter/tests/node_set.rs"),
            PathBuf::from("contracts/zk-private-counter/tests/support/mod.rs"),
            PathBuf::from("contracts/zk-private-counter/script/src/counter_set.rs"),
            PathBuf::from("crates/cellscript-tools/src/ckb_devnet.rs"),
        ],
        &BTreeMap::from([("parent".into(), run.join("parent.elf")), ("lifecycle".into(), lifecycle_path)]),
    )?;
    let report = json!({"schema":"cellscript-counter-set-node-v1","status":"passed","source":source,"runtime_provenance":provenance,
        "compiled_test_source_sha256":package::sha256(include_bytes!("node_set.rs")),
        "test_executable_sha256":package::sha256(&fs::read(std::env::current_exe()?)?),
        "runtime_receipt_sha256":package::sha256(&receipt_bytes),"prior_receipt_scope":"authenticates pinned CKB binary only; not acceptance of new application",
        "genesis_hash":genesis["header"]["hash"],"setup":"public deterministic test seed; not production","fee_lock":"always success",
        "admission":"normal send_transaction, passthrough outputs validator, consensus and pool verification enabled",
        "deployments":[child_deploy,vk_deploy,parent_deploy,lifecycle_deploy],"rows":rows,"ci_or_gates_run":false,"production_admitted":false});
    fs::write(run.join("evidence.json"), serde_json::to_vec_pretty(&report)?)?;
    fs::write(root.join("target/counter-set/node-evidence.json"), serde_json::to_vec_pretty(&report)?)?;
    eprintln!("node evidence: {}", run.display());
    Ok(())
}

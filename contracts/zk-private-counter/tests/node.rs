//! Explicit local-node acceptance, separately invoked with a pinned CKB build.
// Reuse the repository's maintained node lifecycle/RPC harness.
#[path = "support/ccc.rs"]
mod ccc;
#[path = "support/ccc_migration.rs"]
mod ccc_migration;
#[allow(dead_code)]
#[path = "../../../crates/cellscript-tools/src/ckb_devnet.rs"]
mod devnet;
mod support;
use ark_std::rand::{rngs::OsRng, rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package, CounterCircuit, Witness};
use ckb_testtool::ckb_types::{
    bytes::Bytes,
    core::{ScriptHashType, TransactionBuilder, TransactionView},
    packed,
    prelude::*,
};
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
use support::*;
fn rpc_tx(tx: &TransactionView) -> Value {
    serde_json::to_value(ckb_jsonrpc_types::Transaction::from(tx.data())).unwrap()
}
fn point(value: &Value) -> packed::OutPoint {
    serde_json::from_value::<ckb_jsonrpc_types::OutPoint>(value.clone()).unwrap().into()
}
fn commit(node: &devnet::CkbDevnet, tx: &TransactionView) -> Value {
    // Normal tx-pool admission, including fee policy and script verification.
    let hash = node.rpc("send_transaction", vec![rpc_tx(tx), json!("passthrough")]).unwrap();
    for _ in 0..80 {
        let result = node.rpc("get_transaction", vec![hash.clone()]).unwrap();
        if result["tx_status"]["status"] == "committed" {
            return result["tx_status"].clone();
        }
        assert_ne!(result["tx_status"]["status"], "rejected", "{result}");
        node.rpc("generate_block", vec![]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("transaction did not commit: {hash}")
}
#[test]
#[ignore = "requires CELLSCRIPT_CKB_REPO and a passing pinned CKB acceptance receipt; starts a local node"]
fn counter_node_acceptance() {
    let repo = PathBuf::from(std::env::var_os("CELLSCRIPT_CKB_REPO").expect("pinned CKB repo"));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let acceptance_root = root.join("target/ckb-cellscript-acceptance");
    let index: Value = serde_json::from_slice(&std::fs::read(acceptance_root.join("latest-production.json")).unwrap()).unwrap();
    assert_eq!(index["status"], "passed");
    let relative = std::path::Path::new(index["report"]["path"].as_str().unwrap());
    assert!(relative.components().all(|c| matches!(c, std::path::Component::Normal(_))));
    let report_bytes = std::fs::read(acceptance_root.join(relative)).unwrap();
    assert_eq!(package::sha256(&report_bytes), index["report"]["sha256"]);
    let acceptance: Value = serde_json::from_slice(&report_bytes).unwrap();
    let source_provenance = &acceptance["source_provenance"];
    let source_head = Command::new("git").arg("-C").arg(&root).args(["rev-parse", "HEAD"]).output().unwrap();
    assert!(source_head.status.success(), "CellScript source identity is unavailable");
    assert_eq!(source_provenance["repo_commit"], String::from_utf8(source_head.stdout).unwrap().trim());
    assert_eq!(source_provenance["git_dirty"], false, "node acceptance requires a clean-source acceptance receipt");
    let source_status =
        Command::new("git").arg("-C").arg(&root).args(["status", "--porcelain", "--untracked-files=no"]).output().unwrap();
    assert!(source_status.status.success() && source_status.stdout.is_empty(), "CellScript source changed after acceptance");
    let provenance = &acceptance["ckb_runtime_provenance"];
    let bin = std::env::var_os("CELLSCRIPT_COUNTER_CKB_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(provenance["binary_path"].as_str().unwrap()));
    assert_eq!(
        format!("0x{}", package::sha256(&std::fs::read(&bin).unwrap())),
        provenance["binary_sha256"],
        "CKB binary differs from the fresh-build acceptance receipt"
    );
    let pin: Value = serde_json::from_slice(&std::fs::read(root.join("scripts/ckb_acceptance_pin.json")).unwrap()).unwrap();
    let head = Command::new("git").arg("-C").arg(&repo).args(["rev-parse", "HEAD"]).output().unwrap();
    let revision = String::from_utf8(head.stdout).unwrap().trim().to_owned();
    assert_eq!(pin["revision"], revision, "CKB revision is not pinned");
    assert_eq!(provenance["revision"], revision);
    assert_eq!(provenance["repo_dirty"], false);
    let status = Command::new("git").arg("-C").arg(&repo).args(["status", "--porcelain", "--untracked-files=no"]).output().unwrap();
    assert!(status.status.success() && status.stdout.is_empty(), "CKB source is dirty");
    let version = Command::new(&bin).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert!(String::from_utf8_lossy(&version.stdout).contains(&format!("{} ({}", pin["version"].as_str().unwrap(), &revision[..7])));
    // Keep the fallback package alive for both native proving and CCC child
    // processes. Passing its path explicitly avoids making test fixture setup
    // depend on a process-global environment mutation.
    let fallback_package = tempfile::tempdir().unwrap();
    let package_directory = std::env::var_os("CELLSCRIPT_COUNTER_PACKAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback_package.path().join("test-setup"));
    let (manifest, pk) = if std::env::var_os("CELLSCRIPT_COUNTER_PACKAGE").is_some() {
        package::load_package(&package_directory).unwrap()
    } else {
        let (pk, _) = counter::setup(&mut StdRng::seed_from_u64(0x43534b43544e)).unwrap();
        let manifest = package::write_package(&package_directory, &pk, package::SetupKind::PublicTestSeed).unwrap();
        (manifest, pk)
    };
    let key = counter::serialize(&pk.vk).unwrap();
    let run = root.join("target/counter-node").join(format!(
        "{}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        std::process::id()
    ));
    let mut node = devnet::CkbDevnet::new(repo, bin.clone(), run.clone()).unwrap();
    if std::env::var_os("CELLSCRIPT_COUNTER_CCC").is_some() {
        let config = node.ckb_dir.join("ckb.toml");
        let text = std::fs::read_to_string(&config).unwrap().replace("\"IntegrationTest\"", "\"IntegrationTest\", \"Indexer\"");
        std::fs::write(config, text).unwrap();
    }
    node.start().unwrap();
    let genesis = node.get_block_by_number(0).unwrap();
    let always_dep = devnet::always_success_dep(genesis["transactions"][0]["hash"].as_str().unwrap());
    let genesis_hash = hex::decode(genesis["header"]["hash"].as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap();
    let child = child_bytes();
    let lifecycle = lifecycle_bytes();
    let child_deploy = devnet::deploy_code(&mut node, "zk-child", &child, &always_dep).unwrap();
    let vk_deploy = devnet::deploy_code(&mut node, "counter-vk", &key, &always_dep).unwrap();
    let (compiled, handle) = parent(&child, &key, &point(&child_deploy["cell_dep"]["out_point"]), genesis_hash);
    let parent_bytes = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    let parent_deploy = devnet::deploy_code(&mut node, "counter-parent", parent_bytes, &always_dep).unwrap();
    let lifecycle_deploy = devnet::deploy_code(&mut node, "counter-lifecycle", &lifecycle, &always_dep).unwrap();
    let deps = vec![
        dep(point(&child_deploy["cell_dep"]["out_point"])),
        dep(point(&vk_deploy["cell_dep"]["out_point"])),
        dep(point(&parent_deploy["cell_dep"]["out_point"])),
        dep(point(&lifecycle_deploy["cell_dep"]["out_point"])),
        dep(point(&always_dep["out_point"])),
    ];
    let lock: packed::Script = serde_json::from_value::<ckb_jsonrpc_types::Script>(devnet::always_success_lock("0x")).unwrap().into();
    let funding = node.collect_spendable(1100 * devnet::SHANNONS).unwrap();
    let inputs = funding_inputs(&funding);
    let args = type_args(&inputs[0], 0, parent_bytes);
    let script = packed::Script::new_builder()
        .code_hash(counter::hash(&lifecycle))
        .hash_type(ScriptHashType::Data2)
        .args(Bytes::from(args).pack())
        .build();
    let cell = packed::CellOutput::new_builder()
        .capacity(devnet::STATE_CAPACITY)
        .lock(lock.clone())
        .type_(Some(script.clone()).pack())
        .build();
    let change = packed::CellOutput::new_builder()
        .capacity(funding["total_capacity"].as_u64().unwrap() - devnet::STATE_CAPACITY - 100_000)
        .lock(lock.clone())
        .build();
    let secret = [0x23; 32];
    let owner = counter::owner(&secret);
    let data = |n| Bytes::copy_from_slice(&counter::state(&owner, n));
    let create = TransactionBuilder::default()
        .set_inputs(inputs)
        .output(cell.clone())
        .output_data(data(0).pack())
        .output(change)
        .output_data(Bytes::new().pack())
        .set_cell_deps(deps.clone())
        .build();
    let dry = node.dry_run(&rpc_tx(&create)).unwrap();
    let committed = commit(&node, &create);
    let mut rows = vec![json!({"case":"create","dry_run":dry,"commit":committed,"tx_hash":hex::encode(create.hash().as_slice())})];
    let mut previous = packed::OutPoint::new_builder().tx_hash(create.hash()).index(0u32).build();
    let mut old_proof: Option<Vec<u8>> = None;
    for n in 0..2 {
        let funding = node.collect_spendable(100 * devnet::SHANNONS).unwrap();
        let mut inputs = vec![packed::CellInput::new_builder().previous_output(previous.clone()).build()];
        inputs.extend(funding_inputs(&funding));
        let change = packed::CellOutput::new_builder()
            .capacity(funding["total_capacity"].as_u64().unwrap() - 100_000)
            .lock(lock.clone())
            .build();
        let tx = TransactionBuilder::default()
            .set_inputs(inputs)
            .output(cell.clone())
            .output_data(data(n + 1).pack())
            .output(change)
            .output_data(Bytes::new().pack())
            .set_cell_deps(deps.clone())
            .build();
        let proof = counter::prove(
            &pk,
            CounterCircuit {
                statement: statement(&script, &previous, &data(n), &data(n + 1), &tx),
                witness: Witness { secret, old_counter: n, new_counter: n + 1 },
            },
            &mut OsRng,
        )
        .unwrap();
        let with_proof = |p: &[u8]| tx.as_advanced_builder().set_witnesses(vec![witness(&compiled, p, &handle).pack()]).build();
        if let Some(old) = &old_proof {
            rows.push(node.dry_run_rejects(&rpc_tx(&with_proof(old)), "successor-replay", None, None, Some(79)).unwrap());
        }
        let tx = with_proof(&proof);
        let skipped = tx.as_advanced_builder().set_outputs_data(vec![data(n + 2).pack(), Bytes::new().pack()]).build();
        rows.push(node.dry_run_rejects(&rpc_tx(&skipped), "skip-counter", None, None, Some(79)).unwrap());
        let dry = node.dry_run(&rpc_tx(&tx)).unwrap();
        let cycles = u64::from_str_radix(dry["cycles"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
        assert!(cycles < 250_000_000);
        let committed = commit(&node, &tx);
        let tx_hash = format!("0x{}", hex::encode(tx.hash().as_slice()));
        node.assert_live_cell(
            &tx_hash,
            0,
            "successor",
            Some(devnet::STATE_CAPACITY),
            Some(&devnet::always_success_lock("0x")),
            None,
            Some(&data(n + 1)),
        )
        .unwrap();
        node.wait_dead_cell(&format!("0x{}", hex::encode(previous.tx_hash().as_slice())), 0).unwrap();
        rows.push(json!({"case":format!("update-{n}-{}",n+1),"cycles":cycles,"commit":committed,"tx_hash":tx_hash,"bytes":tx.data().as_slice().len()}));
        previous = packed::OutPoint::new_builder().tx_hash(tx.hash()).index(0u32).build();
        old_proof = Some(proof.to_vec());
    }
    if std::env::var_os("CELLSCRIPT_COUNTER_CCC").is_some() {
        rows.push(ccc::run(
            &mut node,
            &root,
            &run,
            &genesis,
            &[child_deploy.clone(), vk_deploy.clone(), parent_deploy.clone(), lifecycle_deploy.clone()],
            &previous,
            &compiled,
            &handle,
            &key,
            &package_directory,
        ));
        rows.push(ccc_migration::run(
            &mut node,
            &root,
            &run,
            &genesis,
            &child_deploy,
            &vk_deploy,
            &parent_deploy,
            &compiled,
            &handle,
            &package_directory,
        ));
    }
    node.stop();
    let report = json!({"schema":"cellscript-counter-node-evidence-v1","status":"passed","circuit_sha256":manifest.circuit.r1cs_sha256,"verification_key_data_hash":manifest.verification_key_data_hash,
        "setup_kind":manifest.setup_kind,"source_provenance":source_provenance,"ckb_revision":revision,"ckb_version":String::from_utf8_lossy(&version.stdout).trim(),"ckb_binary_sha256":package::sha256(&std::fs::read(bin).unwrap()),
        "admission":"send_transaction with passthrough outputs validator; consensus and pool verification enabled","production_admitted":false,"network":"local integration chain","rows":rows,
        "deployments":[child_deploy,vk_deploy,parent_deploy,lifecycle_deploy]});
    std::fs::write(run.join("evidence.json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    std::fs::write(root.join("contracts/zk-private-counter/target/node-evidence.json"), serde_json::to_vec_pretty(&report).unwrap())
        .unwrap();
}
fn funding_inputs(funding: &Value) -> Vec<packed::CellInput> {
    devnet::funding_cells(funding)
        .iter()
        .map(|cell| {
            packed::CellInput::new_builder()
                .previous_output(point(&devnet::out_point(cell["tx_hash"].as_str().unwrap(), cell["index"].as_u64().unwrap())))
                .build()
        })
        .collect()
}

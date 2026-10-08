//! Paired instance creation plus the shipped CCC migration client on the pinned node.
use super::{commit, devnet, funding_inputs, point, support};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use cellscript_zk_private_counter::{self as counter, package};
use ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, time::Duration};

#[allow(clippy::too_many_arguments)] // The existing node owns these exact deployed artifacts.
pub fn run(
    node: &mut devnet::CkbDevnet,
    root: &Path,
    run: &Path,
    genesis: &Value,
    child_deploy: &Value,
    old_key_deploy: &Value,
    old_parent_deploy: &Value,
    old_compiled: &cellscript::CompileResult,
    old_handle: &[u8],
    old_package: &Path,
) -> Value {
    let directory = run.join("ccc-migration");
    fs::create_dir(&directory).unwrap();
    let (old_setup, _) = package::load_package(old_package).unwrap();
    let (new_package, new_setup, key) = if let Some(path) = std::env::var_os("CELLSCRIPT_COUNTER_MIGRATION_PACKAGE") {
        let path = std::path::PathBuf::from(path);
        let (manifest, key) = package::load_package(&path).unwrap();
        (path, manifest, key)
    } else {
        let path = directory.join("public-test-setup");
        let (key, _) = counter::setup(&mut StdRng::seed_from_u64(0x4343434d494752)).unwrap();
        let manifest = package::write_package(&path, &key, package::SetupKind::PublicTestSeed).unwrap();
        (path, manifest, key)
    };
    let vk = counter::serialize(&key.vk).unwrap();
    assert_ne!(old_setup.verification_key_data_hash, new_setup.verification_key_data_hash);
    let always_dep = devnet::always_success_dep(genesis["transactions"][0]["hash"].as_str().unwrap());
    let child = support::child_bytes();
    let genesis_hash = hex::decode(genesis["header"]["hash"].as_str().unwrap().trim_start_matches("0x")).unwrap().try_into().unwrap();
    let (new_compiled, new_handle) = support::parent(&child, &vk, &point(&child_deploy["cell_dep"]["out_point"]), genesis_hash);
    let key_deploy = devnet::deploy_code(node, "migration-vk", &vk, &always_dep).unwrap();
    let parent_deploy =
        devnet::deploy_code(node, "migration-parent", cellscript::strip_vm_abi_trailer(&new_compiled.artifact_bytes), &always_dep)
            .unwrap();
    let load_script = |name: &str| {
        fs::read(root.join("contracts/zk-private-counter/script/target/riscv64imac-unknown-none-elf/release").join(name)).unwrap()
    };
    let lifecycle = load_script("cellscript-counter-migratable-lifecycle");
    let guard = load_script("cellscript-counter-config");
    let lifecycle_deploy = devnet::deploy_code(node, "migratable-lifecycle", &lifecycle, &always_dep).unwrap();
    let guard_deploy = devnet::deploy_code(node, "migration-config", &guard, &always_dep).unwrap();
    let funding = node.collect_spendable(2100 * devnet::SHANNONS).unwrap();
    let inputs = funding_inputs(&funding);
    let type_id = |index: u64| {
        let mut bytes = inputs[0].as_slice().to_vec();
        bytes.extend(index.to_le_bytes());
        counter::hash(&bytes)
    };
    let parent_hashes = [
        counter::hash(cellscript::strip_vm_abi_trailer(&old_compiled.artifact_bytes)),
        counter::hash(cellscript::strip_vm_abi_trailer(&new_compiled.artifact_bytes)),
    ];
    let mut args = type_id(1).to_vec();
    args.extend(type_id(0));
    args.extend(counter::hash(&lifecycle));
    args.extend(parent_hashes[0]);
    args.extend(parent_hashes[1]);
    let config = packed::Script::new_builder().code_hash(counter::hash(&guard)).hash_type(4u8).args(Bytes::from(args).pack()).build();
    let mut args = type_id(0).to_vec();
    args.extend(config.calc_script_hash().as_slice());
    let script =
        packed::Script::new_builder().code_hash(counter::hash(&lifecycle)).hash_type(4u8).args(Bytes::from(args).pack()).build();
    let lock: packed::Script = serde_json::from_value::<ckb_jsonrpc_types::Script>(devnet::always_success_lock("0x")).unwrap().into();
    let state = packed::CellOutput::new_builder()
        .capacity(devnet::STATE_CAPACITY)
        .lock(lock.clone())
        .type_(Some(script.clone()).pack())
        .build();
    let config_cell = state.clone().as_builder().type_(Some(config.clone()).pack()).build();
    let data = Bytes::copy_from_slice(&counter::state(&counter::owner(&[0x23; 32]), 0));
    let mut config_data = b"CSZKCFG1\0".to_vec();
    config_data.extend(parent_hashes[0]);
    let create = TransactionBuilder::default()
        .set_inputs(inputs)
        .output(state)
        .output_data(data.pack())
        .output(config_cell)
        .output_data(Bytes::from(config_data).pack())
        .output(
            packed::CellOutput::new_builder()
                .capacity(funding["total_capacity"].as_u64().unwrap() - 2 * devnet::STATE_CAPACITY - 100_000)
                .lock(lock)
                .build(),
        )
        .output_data(Bytes::new().pack())
        .set_cell_deps(
            [&lifecycle_deploy["cell_dep"]["out_point"], &guard_deploy["cell_dep"]["out_point"], &always_dep["out_point"]]
                .map(|point_value| support::dep(point(point_value)))
                .to_vec(),
        )
        .build();
    let creation_dry_run = node.dry_run(&super::rpc_tx(&create)).unwrap();
    let creation_commit = commit(node, &create);
    let code = |value: &Value| {
        let p = point(&value["cell_dep"]["out_point"]);
        json!({"tx_hash":hex::encode(p.tx_hash().as_slice()),"index":Unpack::<u32>::unpack(&p.index()),"data_hash":value["data_hash"]})
    };
    let child_point = point(&child_deploy["cell_dep"]["out_point"]);
    let versions: Vec<_> = [(&old_setup, old_key_deploy, old_parent_deploy), (&new_setup, &key_deploy, &parent_deploy)].into_iter().map(|(setup, key_deploy, parent_deploy)| json!({
        "verifier":{"chain_id":"ckb-testnet","genesis_hash":hex::encode(genesis_hash),"child_tx_hash":hex::encode(child_point.tx_hash().as_slice()),"child_index":Unpack::<u32>::unpack(&child_point.index()),"child_data_hash":child_deploy["data_hash"],"verification_key_hash":setup.verification_key_data_hash},
        "parent":code(parent_deploy),"key":code(key_deploy),"setup":setup,
    })).collect();
    let manifest = json!({"schema":"cellscript-counter-migration-v1","chain_id":"ckb-testnet","genesis_hash":hex::encode(genesis_hash),"lifecycle":code(&lifecycle_deploy),"config_guard":code(&guard_deploy),"instance":{"counter":hex::encode(script.as_slice()),"configuration":hex::encode(config.as_slice())},"versions":versions});
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
    fs::write(directory.join("manifest.json"), &manifest_bytes).unwrap();
    for (i, compiled) in [old_compiled, &new_compiled].into_iter().enumerate() {
        let metadata = directory.join(format!("parent-{i}-metadata.json"));
        fs::write(&metadata, serde_json::to_vec_pretty(&compiled.metadata).unwrap()).unwrap();
        let status = Command::new(root.join("target/debug/cellc"))
            .args(["gen-builder", "--target", "typescript", "--metadata"])
            .arg(metadata)
            .arg("--output")
            .arg(directory.join(format!("sdk-{i}")))
            .status()
            .unwrap();
        assert!(status.success());
    }
    fs::write(directory.join("owner.bin"), [0x23; 32]).unwrap(); // Public fixture secret.
    let secp_type: packed::Script =
        serde_json::from_value::<ckb_jsonrpc_types::Script>(genesis["transactions"][0]["outputs"][1]["type"].clone()).unwrap().into();
    let prover = root.join("contracts/zk-private-counter/target/release/cellscript-zk-private-counter");
    let initial = |index: u32| json!({"txHash":format!("0x{}",hex::encode(create.hash().as_slice())),"index":index});
    let configuration = json!({"rpcUrl":node.rpc_url,"directory":directory,"manifestDigest":format!("0x{}",hex::encode(counter::hash(&manifest_bytes))),
        "handles":[format!("0x{}",hex::encode(old_handle)),format!("0x{}",hex::encode(new_handle))],
        "proverExecutable":prover,"proverSha256":package::sha256(&fs::read(&prover).unwrap()),"setupPackages":[old_package,new_package],"ownerSecretFile":directory.join("owner.bin"),
        "counter":initial(0),"configuration":initial(1),"genesisHash":genesis["header"]["hash"],
        "secpCodeHash":format!("0x{}",hex::encode(secp_type.calc_script_hash().as_slice())),"secpDep":{"outPoint":{"txHash":genesis["transactions"][1]["hash"],"index":0},"depType":"depGroup"},
        "lockDeps":[{"outPoint":{"txHash":always_dep["out_point"]["tx_hash"],"index":always_dep["out_point"]["index"]},"depType":"code"}]});
    fs::write(directory.join("config.json"), serde_json::to_vec_pretty(&configuration).unwrap()).unwrap();
    let mut child = Command::new("node")
        .arg("--experimental-strip-types")
        .arg(root.join("examples/zk/node-migration.ts"))
        .arg(directory.join("config.json"))
        .spawn()
        .unwrap();
    let mut completed = false;
    for _ in 0..240 {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "CCC migration client failed");
            completed = true;
            break;
        }
        node.rpc("generate_block", vec![]).unwrap();
        std::thread::sleep(Duration::from_secs(1));
    }
    if !completed {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("CCC migration client timed out");
    }
    let client: Value = serde_json::from_slice(&fs::read(directory.join("ccc-migration-report.json")).unwrap()).unwrap();
    assert_eq!(client["status"], "passed");
    json!({"case":"CCC authorized migration","status":"passed","creation_dry_run":creation_dry_run,"creation_commit":creation_commit,"setup_kinds":[old_setup.setup_kind,new_setup.setup_kind],"manifest_data_hash":hex::encode(counter::hash(&manifest_bytes)),"client":client})
}

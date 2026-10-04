//! Live-node integration of the shipped CCC client, including a real fee signature.
use super::{commit, devnet, funding_inputs, point};
use cellscript_zk_private_counter::{self as counter, package};
use ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command, time::Duration};

#[allow(clippy::too_many_arguments)]
pub fn run(
    node: &mut devnet::CkbDevnet,
    root: &Path,
    run: &Path,
    genesis: &Value,
    deployments: &[Value; 4],
    previous: &packed::OutPoint,
    compiled: &cellscript::CompileResult,
    handle: &[u8],
    key: &[u8],
) -> Value {
    let directory = run.join("ccc");
    fs::create_dir(&directory).unwrap();
    let always_dep = devnet::always_success_dep(genesis["transactions"][0]["hash"].as_str().unwrap());
    let secp_type: packed::Script =
        serde_json::from_value::<ckb_jsonrpc_types::Script>(genesis["transactions"][0]["outputs"][1]["type"].clone()).unwrap().into();
    let wallet = k256::SecretKey::from_slice(&[0x45; 32]).unwrap();
    let pubkey = wallet.public_key().to_encoded_point(true);
    let args = &counter::hash(pubkey.as_bytes())[..20];
    let lock = packed::Script::new_builder()
        .code_hash(secp_type.calc_script_hash())
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(args).pack())
        .build();
    let funding = node.collect_spendable(1000 * devnet::SHANNONS).unwrap();
    let tx = TransactionBuilder::default()
        .set_inputs(funding_inputs(&funding))
        .output(packed::CellOutput::new_builder().capacity(funding["total_capacity"].as_u64().unwrap() - 100_000).lock(lock).build())
        .output_data(Bytes::new().pack())
        .cell_dep(super::support::dep(point(&always_dep["out_point"])))
        .build();
    commit(node, &tx);
    fs::write(directory.join("parent-metadata.json"), serde_json::to_vec_pretty(&compiled.metadata).unwrap()).unwrap();
    let status = Command::new(root.join("target/debug/cellc"))
        .args(["gen-builder", "--target", "typescript", "--metadata"])
        .arg(directory.join("parent-metadata.json"))
        .arg("--output")
        .arg(directory.join("sdk"))
        .status()
        .unwrap();
    assert!(status.success());
    fs::write(directory.join("owner.bin"), [0x23; 32]).unwrap(); // Public fixture secret only.
    let code = |value: &Value| json!({"outPoint":{"txHash":value["cell_dep"]["out_point"]["tx_hash"], "index":value["cell_dep"]["out_point"]["index"]},"dataHash":value["data_hash"]});
    let configuration = json!({"rpcUrl":node.rpc_url,"sdkDirectory":directory.join("sdk"),
        "proverExecutable":root.join("contracts/zk-private-counter/target/release/cellscript-zk-private-counter"),
        "setupPackage":std::env::var("CELLSCRIPT_COUNTER_PACKAGE").expect("CCC test requires a setup package"),
        "ownerSecretFile":directory.join("owner.bin"), "counter":{"txHash":format!("0x{}",hex::encode(previous.tx_hash().as_slice())),"index":0},
        "secpCodeHash":format!("0x{}",hex::encode(secp_type.calc_script_hash().as_slice())),
        "secpDep":{"outPoint":{"txHash":genesis["transactions"][1]["hash"],"index":0},"depType":"depGroup"},
        "deployment":{"genesisHash":genesis["header"]["hash"],"child":code(&deployments[0]),"verificationKey":code(&deployments[1]),"parent":code(&deployments[2]),"lifecycle":code(&deployments[3]),"handle":format!("0x{}",hex::encode(handle)),
            "lockDeps":[{"outPoint":{"txHash":always_dep["out_point"]["tx_hash"],"index":always_dep["out_point"]["index"]},"depType":"code"}]},
        "verification_key_data_hash":hex::encode(counter::hash(key)), "circuit_package_lock":package::sha256(include_bytes!("../../Cargo.lock"))});
    fs::write(directory.join("config.json"), serde_json::to_vec_pretty(&configuration).unwrap()).unwrap();
    let mut child = Command::new("node")
        .args(["--experimental-strip-types"])
        .arg(root.join("examples/zk/node-client.ts"))
        .arg(directory.join("config.json"))
        .spawn()
        .unwrap();
    let mut completed = false;
    for _ in 0..180 {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "CCC node client failed");
            completed = true;
            break;
        }
        node.rpc("generate_block", vec![]).unwrap();
        std::thread::sleep(Duration::from_secs(1));
    }
    if !completed {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("CCC node client timed out");
    }
    serde_json::from_slice(&fs::read(directory.join("ccc-report.json")).unwrap()).unwrap()
}

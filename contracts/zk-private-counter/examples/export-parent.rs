//! Export a deployment-specific parent and exact handle after the child is deployed.
// The scheduler/node suites use these same builder routines.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;
use anyhow::{ensure, Result};
use cellscript_zk_private_counter::{self as counter, package};
use ckb_testtool::ckb_types::{packed, prelude::*};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 7,
        "usage: export-parent CHAIN_ID GENESIS_HASH CHILD_TX_HASH CHILD_INDEX PACKAGE OUTPUT_DIR EXPECTED_CHILD_HASH"
    );
    let hash = |s: &str| -> Result<[u8; 32]> {
        hex::decode(s.trim_start_matches("0x"))?.try_into().map_err(|_| anyhow::anyhow!("hash must be 32 bytes"))
    };
    let (manifest, pk) = package::load_package(std::path::Path::new(&args[4]))?;
    let child = support::child_bytes();
    ensure!(counter::hash(&child) == hash(&args[6])?, "deployed child hash differs from built child");
    let out = packed::OutPoint::new_builder().tx_hash(hash(&args[2])?).index(args[3].parse::<u32>()?).build();
    let (compiled, handle) = support::parent_for_network(&child, &counter::serialize(&pk.vk)?, &out, &args[0], hash(&args[1])?);
    let output = std::path::Path::new(&args[5]);
    std::fs::create_dir(output)?;
    let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    std::fs::write(output.join("parent.elf"), elf)?;
    std::fs::write(output.join("parent-metadata.json"), serde_json::to_vec_pretty(&compiled.metadata)?)?;
    std::fs::write(output.join("exact-handle.bin"), &handle)?;
    std::fs::write(
        output.join("binding.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
        "schema":"cellscript-counter-parent-binding-v1","network":args[0],"genesis_hash":args[1],"child_tx_hash":args[2],"child_index":args[3],
        "child_data_hash":hex::encode(counter::hash(&child)),"parent_data_hash":hex::encode(counter::hash(elf)),
        "handle_hash":hex::encode(counter::hash(&handle)),"verification_key_data_hash":manifest.verification_key_data_hash,
        "circuit_sha256":manifest.circuit.r1cs_sha256,"chain_deployment_verified":false}))?,
    )?;
    Ok(())
}

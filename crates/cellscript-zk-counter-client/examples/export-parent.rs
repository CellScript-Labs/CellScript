//! Checked manifest-driven parent export. Supplied coordinates are not a live-chain receipt.
use anyhow::{ensure, Result};
use cellscript_zk_counter_client::{parent, Deployment};
use cellscript_zk_private_counter::{self as counter, package};
use std::{fs, path::Path};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 4, "usage: export-parent DEPLOYMENT_JSON CHILD_ELF SETUP_PACKAGE NEW_OUTPUT_DIR");
    let deployment: Deployment = serde_json::from_slice(&fs::read(&args[0])?)?;
    let (_, pk) = package::load_package(Path::new(&args[2]))?;
    let result = parent(&deployment, &fs::read(&args[1])?, &counter::serialize(&pk.vk)?)?;
    let output = Path::new(&args[3]);
    fs::create_dir(output)?;
    fs::write(output.join("parent.elf"), cellscript::strip_vm_abi_trailer(&result.compiled.artifact_bytes))?;
    fs::write(output.join("parent.cell"), result.source)?;
    fs::write(output.join("parent-metadata.json"), serde_json::to_vec_pretty(&result.compiled.metadata)?)?;
    fs::write(output.join("named-deployment.json"), serde_json::to_vec_pretty(&result.deploy)?)?;
    fs::write(output.join("exact-handle.bin"), result.handle)?;
    fs::write(output.join("deployment.json"), serde_json::to_vec_pretty(&deployment)?)?;
    Ok(())
}

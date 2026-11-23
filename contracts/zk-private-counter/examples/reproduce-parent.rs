//! Deterministic parent build for an explicit, non-live deployment fixture.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;
use anyhow::{Context, Result};
use ark_std::rand::{rngs::StdRng, SeedableRng};
use ckb_types::{packed, prelude::*};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
fn main() -> Result<()> {
    let output = PathBuf::from(std::env::args_os().nth(1).context("output path is required")?);
    let (_, verification_key) =
        cellscript_zk_private_counter::setup(&mut StdRng::seed_from_u64(0x4d494752415445)).context("deterministic test setup")?;
    let key = cellscript_zk_private_counter::serialize(&verification_key).context("encode verification key")?;
    let out = packed::OutPoint::new_builder().tx_hash([0x22; 32]).index(0u32).build();
    let compiled = support::compile_parent(&support::read_child()?, &key, &out, [0x11; 32])?.compiled;
    let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    let metadata = serde_json::to_vec(&compiled.metadata)?;
    let lowering = serde_json::to_vec(compiled.verified_lowering_record.as_ref().context("parent lowering record is missing")?)?;
    let source_map = serde_json::to_vec(compiled.source_artifact_map.as_ref().context("parent source map is missing")?)?;
    let report = serde_json::json!({"schema":"cellscript-zk-parent-reproduction-v1","scope":"two fresh compiler processes; fixed synthetic deployment; not live chain evidence","rust":"1.97.1","profile":cellscript_zk_private_counter::wire::PROFILE,"parent_sha256":hex::encode(Sha256::digest(elf)),"parent_data_hash":hex::encode(cellscript_zk_private_counter::hash(elf)),"parent_bytes":elf.len(),"full_artifact_sha256":hex::encode(Sha256::digest(&compiled.artifact_bytes)),"metadata_sha256":hex::encode(Sha256::digest(metadata)),"lowering_sha256":hex::encode(Sha256::digest(lowering)),"source_map_sha256":hex::encode(Sha256::digest(source_map))});
    let report_bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(&output, elf).with_context(|| format!("write parent artifact {}", output.display()))?;
    let report_path = output.with_extension("json");
    std::fs::write(&report_path, report_bytes).with_context(|| format!("write parent report {}", report_path.display()))?;
    Ok(())
}

//! Deterministic parent build for an explicit, non-live deployment fixture.
#[allow(dead_code)]
#[path = "../tests/support/mod.rs"]
mod support;
use ckb_types::{packed, prelude::*};
use sha2::{Digest, Sha256};
fn main() {
    let output = std::env::args().nth(1).expect("output path");
    let key = include_bytes!("../../../docs/reports/0.32/private-counter/verification-key.bin");
    let out = packed::OutPoint::new_builder().tx_hash([0x22; 32]).index(0u32).build();
    let (compiled, _) = support::parent(&support::child_bytes(), key, &out, [0x11; 32]);
    let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    std::fs::write(&output, elf).unwrap();
    let metadata = serde_json::to_vec(&compiled.metadata).unwrap();
    let lowering = serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap();
    let source_map = serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap();
    let report = serde_json::json!({"schema":"cellscript-zk-parent-reproduction-v1","scope":"two fresh compiler processes; fixed synthetic deployment; not live chain evidence","rust":"1.97.1","profile":cellscript_zk_private_counter::wire::PROFILE,"parent_sha256":hex::encode(Sha256::digest(elf)),"parent_data_hash":hex::encode(cellscript_zk_private_counter::hash(elf)),"parent_bytes":elf.len(),"full_artifact_sha256":hex::encode(Sha256::digest(&compiled.artifact_bytes)),"metadata_sha256":hex::encode(Sha256::digest(metadata)),"lowering_sha256":hex::encode(Sha256::digest(lowering)),"source_map_sha256":hex::encode(Sha256::digest(source_map))});
    std::fs::write(std::path::Path::new(&output).with_extension("json"), serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

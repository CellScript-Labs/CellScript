//! Native deployment builder for the exact ZK transition profile.
//! This constructs an exact binding; live-chain observation remains the client's job.
use crate::protocol_bundle::*;
use crate::script_handle::{build_exact_script_handle, ExactScriptHandleReceiptInput};
use crate::{compile_with_zk_deploy, CompileOptions};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionParentManifest {
    pub chain_id: String,
    pub genesis_hash: [u8; 32],
    pub child_tx_hash: [u8; 32],
    pub child_index: u32,
    pub child_data_hash: [u8; 32],
    pub verification_key_hash: [u8; 32],
    pub package_coordinate: String,
    pub lock_node_id: String,
    pub module_name: String,
    pub action_name: String,
    pub policy: String,
    pub domain: [u8; 32],
    pub action: [u8; 32],
}

pub struct TransitionParent {
    pub compiled: crate::CompileResult,
    pub handle: Vec<u8>,
    pub source: String,
    pub deploy: crate::package::CkbDeployConfig,
}

pub fn ckb_hash(bytes: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new().hash_length(32).personal(b"ckb-default-hash").hash(bytes).as_bytes().try_into().expect("32-byte hash")
}

fn literal(bytes: &[u8]) -> String {
    format!("Hash::from_bytes(b\"{}\")", bytes.iter().map(|b| format!("\\x{b:02x}")).collect::<String>())
}

/// Validate declared code/VK identities before producing source, metadata and handle.
/// No on-chain deployment, setup admission or cryptographic review is implied.
pub fn compile_transition_parent(manifest: &TransitionParentManifest, child: &[u8], key: &[u8]) -> Result<TransitionParent> {
    for (field, name) in [("module_name", &manifest.module_name), ("action_name", &manifest.action_name), ("policy", &manifest.policy)]
    {
        ensure!(
            !name.is_empty()
                && name.len() <= 64
                && name.bytes().enumerate().all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())),
            "ZK {field}: expected a CellScript identifier"
        );
    }
    for (field, expected, observed) in [
        ("child_data_hash", manifest.child_data_hash, ckb_hash(child)),
        ("verification_key_hash", manifest.verification_key_hash, ckb_hash(key)),
    ] {
        ensure!(
            expected == observed,
            "ZK {field}: expected {}, observed {}; select the matching deployment/package",
            hex::encode(expected),
            hex::encode(observed)
        );
    }
    let code_hash = hex::encode(ckb_hash(child));
    let profile = hex::encode(cellscript_artifact_checker::zk::PROFILE_ID);
    let entry = ProtocolEntryIdentity { kind: ProtocolEntryKind::Action, name: "verify".into() };
    let deployment = ProtocolDeploymentIdentity {
        network: ProtocolNetworkIdentity {
            chain_id: manifest.chain_id.clone(),
            genesis_hash: format!("0x{}", hex::encode(manifest.genesis_hash)),
        },
        artifact_hash: code_hash.clone(),
        script: ProtocolScriptIdentity { code_hash: format!("0x{code_hash}"), hash_type: "data".into(), args: "0x".into() },
        code_cell_dep: ProtocolCellDep {
            out_point: ProtocolOutPoint { tx_hash: format!("0x{}", hex::encode(manifest.child_tx_hash)), index: manifest.child_index },
            dep_type: ProtocolDepType::Code,
        },
    };
    let (receipt, handle) = build_exact_script_handle(ExactScriptHandleReceiptInput {
        package_coordinate: &manifest.package_coordinate,
        lock_node_id: &manifest.lock_node_id,
        entry: &entry,
        script_role: ProtocolScriptRole::SpawnedVerifier,
        interface_hash: &profile,
        typed_semantics_hash: &profile,
        artifact_hash: &code_hash,
        target_profile_hash: &profile,
        runtime_abi_hash: &profile,
        verified_bundle_id: &profile,
        deployment: &deployment,
    })?;
    let deploy = crate::package::CkbDeployConfig {
        cell_deps: vec![crate::package::CkbCellDepConfig {
            name: Some("counter_verifier".into()),
            tx_hash: Some(format!("0x{}", hex::encode(manifest.child_tx_hash))),
            index: Some(manifest.child_index),
            data_hash: Some(code_hash),
            dep_type: Some("code".into()),
            ..Default::default()
        }],
        zk_verifiers: vec![crate::zk_package::NamedZkVerifier {
            policy: manifest.policy.clone(),
            dependency: "counter_verifier".into(),
            verification_key_hash: hex::encode(ckb_hash(key)),
            receipt,
        }],
        ..Default::default()
    };
    let handle = hex::decode(handle.encoded.trim_start_matches("0x"))?;
    let source = format!(
        r#"module {module_name}
action {action_name}(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("{policy}", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        literal(&ckb_hash(&handle)),
        literal(&ckb_hash(key)).replace("Hash::", "VerificationKeyCommitment::"),
        literal(&manifest.domain),
        literal(&manifest.action),
        module_name = manifest.module_name,
        action_name = manifest.action_name,
        policy = manifest.policy
    );
    let compiled = compile_with_zk_deploy(
        &source,
        CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        &deploy,
    )?;
    compiled.validate()?;
    Ok(TransitionParent { compiled, handle, source, deploy })
}

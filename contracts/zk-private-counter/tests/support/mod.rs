use cellscript::protocol_bundle::*;
use cellscript::script_handle::{build_exact_script_handle, ExactScriptHandleReceiptInput};
use cellscript::{compile_with_zk_deploy, CompileOptions, EntryWitnessArg};
use cellscript_zk_private_counter::{self as counter, wire};
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
fn literal(bytes: &[u8]) -> String {
    format!("Hash::from_bytes(b\"{}\")", bytes.iter().map(|b| format!("\\x{b:02x}")).collect::<String>())
}
pub fn child_bytes() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../zk-transition-verifier/target/riscv64imac-unknown-none-elf/release/cellscript-zk-transition-verifier"),
    )
    .unwrap()
}
pub fn lifecycle_bytes() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("script/target/riscv64imac-unknown-none-elf/release/cellscript-counter-lifecycle"),
    )
    .unwrap()
}
pub fn parent(child: &[u8], key: &[u8], out: &packed::OutPoint, genesis: [u8; 32]) -> (cellscript::CompileResult, Vec<u8>) {
    parent_for_network(child, key, out, "ckb-testnet", genesis)
}
pub fn parent_for_network(
    child: &[u8],
    key: &[u8],
    out: &packed::OutPoint,
    chain_id: &str,
    genesis: [u8; 32],
) -> (cellscript::CompileResult, Vec<u8>) {
    let (compiled, handle, _, _) = parent_with_package(child, key, out, chain_id, genesis);
    (compiled, handle)
}
pub fn parent_with_package(
    child: &[u8],
    key: &[u8],
    out: &packed::OutPoint,
    chain_id: &str,
    genesis: [u8; 32],
) -> (cellscript::CompileResult, Vec<u8>, String, cellscript::package::CkbDeployConfig) {
    let code_hash = hex::encode(counter::hash(child));
    let profile = hex::encode(wire::PROFILE_ID);
    let entry = ProtocolEntryIdentity { kind: ProtocolEntryKind::Action, name: "verify".into() };
    let deployment = ProtocolDeploymentIdentity {
        network: ProtocolNetworkIdentity { chain_id: chain_id.into(), genesis_hash: format!("0x{}", hex::encode(genesis)) },
        artifact_hash: code_hash.clone(),
        script: ProtocolScriptIdentity { code_hash: format!("0x{code_hash}"), hash_type: "data".into(), args: "0x".into() },
        code_cell_dep: ProtocolCellDep {
            out_point: ProtocolOutPoint {
                tx_hash: format!("0x{}", hex::encode(out.tx_hash().as_slice())),
                index: out.index().unpack(),
            },
            dep_type: ProtocolDepType::Code,
        },
    };
    let (receipt, handle) = build_exact_script_handle(ExactScriptHandleReceiptInput {
        package_coordinate: "test/counter-verifier@0.32.0",
        lock_node_id: "counter-verifier-v1",
        entry: &entry,
        script_role: ProtocolScriptRole::SpawnedVerifier,
        interface_hash: &profile,
        typed_semantics_hash: &profile,
        artifact_hash: &code_hash,
        target_profile_hash: &profile,
        runtime_abi_hash: &profile,
        verified_bundle_id: &profile,
        deployment: &deployment,
    })
    .unwrap();
    let deploy = cellscript::package::CkbDeployConfig {
        cell_deps: vec![cellscript::package::CkbCellDepConfig {
            name: Some("counter_verifier".into()),
            tx_hash: Some(format!("0x{}", hex::encode(out.tx_hash().as_slice()))),
            index: Some(out.index().unpack()),
            data_hash: Some(code_hash),
            dep_type: Some("code".into()),
            ..Default::default()
        }],
        zk_verifiers: vec![cellscript::zk_package::NamedZkVerifier {
            policy: "private_counter".into(),
            dependency: "counter_verifier".into(),
            verification_key_hash: hex::encode(counter::hash(key)),
            receipt,
        }],
        ..Default::default()
    };
    let handle = hex::decode(handle.encoded.trim_start_matches("0x")).unwrap();
    let source = format!(
        r#"module private_counter
action increment(witness proof: ZkTransitionProof, witness verifier: ExactScriptHandle) -> u64 {{
    verification
    zk::require_valid("private_counter", proof, ckb::cell_dep(0), verifier, {}, {}, {}, {})
    return 0
}}
"#,
        literal(&counter::hash(&handle)),
        literal(&counter::hash(key)).replace("Hash::", "VerificationKeyCommitment::"),
        literal(&counter::domain()),
        literal(&counter::action())
    );
    let compiled = compile_with_zk_deploy(
        &source,
        CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        &deploy,
    )
    .unwrap();
    compiled.validate().unwrap();
    (compiled, handle, source, deploy)
}
pub fn dep(out: packed::OutPoint) -> packed::CellDep {
    packed::CellDep::new_builder().out_point(out).build()
}
pub fn type_args(input: &packed::CellInput, index: u64, parent: &[u8]) -> Vec<u8> {
    let mut preimage = input.as_slice().to_vec();
    preimage.extend(index.to_le_bytes());
    let mut args = counter::hash(&preimage).to_vec();
    args.extend(counter::hash(parent));
    args
}
pub fn statement(script: &packed::Script, input: &packed::OutPoint, old: &[u8], new: &[u8], tx: &TransactionView) -> wire::Statement {
    wire::Statement {
        domain: counter::domain(),
        action: counter::action(),
        script_hash: script.calc_script_hash().as_slice().try_into().unwrap(),
        old_data_hash: counter::hash(old),
        new_data_hash: counter::hash(new),
        input_transaction_hash: input.tx_hash().as_slice().try_into().unwrap(),
        input_output_index: input.index().unpack(),
        transaction_hash: tx.hash().as_slice().try_into().unwrap(),
    }
}
pub fn witness(compiled: &cellscript::CompileResult, proof: &[u8], handle: &[u8]) -> Bytes {
    let payload = compiled.metadata.actions[0]
        .entry_witness_args(&[EntryWitnessArg::Bytes(proof.to_vec()), EntryWitnessArg::Bytes(handle.to_vec())])
        .unwrap();
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

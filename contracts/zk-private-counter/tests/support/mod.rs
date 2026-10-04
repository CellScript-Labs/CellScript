use cellscript::EntryWitnessArg;
use cellscript_zk_private_counter::{self as counter, wire};
use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
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
    let result = cellscript::zk_client::compile_transition_parent(
        &cellscript::zk_client::TransitionParentManifest {
            chain_id: chain_id.into(),
            genesis_hash: genesis,
            child_tx_hash: out.tx_hash().as_slice().try_into().unwrap(),
            child_index: out.index().unpack(),
            child_data_hash: counter::hash(child),
            verification_key_hash: counter::hash(key),
            package_coordinate: "test/counter-verifier@0.32.0".into(),
            lock_node_id: "counter-verifier-v1".into(),
            module_name: "private_counter".into(),
            action_name: "increment".into(),
            policy: "private_counter".into(),
            domain: counter::domain(),
            action: counter::action(),
        },
        child,
        key,
    )
    .unwrap();
    (result.compiled, result.handle, result.source, result.deploy)
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

//! Application integration of the general generated Lock codec. The original
//! checked archive remains the byte authority; no application source is rebuilt.
use super::*;
use cellscript::EntryWitnessArg as A;
use std::{collections::BTreeMap, process::Command};

#[test]
#[ignore = "requires completed order archive and the existing local TypeScript compiler"]
fn archived_order_lock_sdk_matches_native_and_actual_transaction_bytes() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = repository.join("target/finance-protocol-evidence/orders");
    let archive = load_archive(&directory).unwrap();
    let program = archive.programs().iter().find(|program| program.name == "order").unwrap();
    let metadata: cellscript::CompileMetadata = serde_json::from_value(program.metadata.clone()).unwrap();
    let (index, record) =
        archive.cases().iter().enumerate().find(|(_, case)| case["case"] == "first-partial-fill-with-donation").unwrap();
    let (context, tx, _) = archive.load_case(index).unwrap();
    assert_eq!(record["outcome"], "accepted");
    let actor = context.get_cell(&tx.inputs().get(2).unwrap().previous_output()).unwrap().0.lock().calc_script_hash();
    let requested = context.get_cell(&tx.inputs().get(3).unwrap().previous_output()).unwrap().0.type_().to_opt().unwrap();
    let args = [
        A::U64(1),
        A::U64(0),
        A::U64(0),
        A::U128(2),
        A::U128(7),
        A::Hash(actor.unpack()),
        A::Hash(requested.args().raw_data().as_ref().try_into().unwrap()),
    ];
    let native = metadata.locks.iter().find(|entry| entry.name == "verify").unwrap().entry_witness_args(&args).unwrap();
    let witness = packed::WitnessArgs::from_slice(&tx.witnesses().get(0).unwrap().raw_data()).unwrap();
    assert_eq!(native.as_slice(), witness.input_type().to_opt().unwrap().raw_data().as_ref());

    let data = tx.outputs_data().get(0).unwrap().raw_data();
    let schema = metadata.types.iter().find(|ty| ty.name == "OrderState").unwrap();
    let fields = schema
        .fields
        .iter()
        .map(|field| {
            let start = field.offset;
            let end = start + field.encoded_size.unwrap();
            (field.name.clone(), data[start..end].to_vec())
        })
        .collect::<BTreeMap<_, _>>();
    let borrowed = fields.iter().map(|(name, bytes)| (name.clone(), bytes.as_slice())).collect();
    let native_data = cellscript::fixed_cell_builder::encode_fixed_cell_data(&metadata, "OrderState", &borrowed).unwrap();
    assert_eq!(native_data.as_slice(), data.as_ref());

    let output = repository.join("target/general-sdk-evidence/order-sdk");
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--metadata"])
        .arg(directory.join("order.metadata.json"))
        .args(["--target", "typescript", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    let request = json!({"payload":hex::encode(&native),"data":hex::encode(&native_data),
        "args":["1","0","0","2","7",format!("0x{}",hex::encode(actor.as_slice())),format!("0x{}",hex::encode(requested.args().raw_data()))],
        "fields":fields.iter().map(|(name,bytes)|(name,hex::encode(bytes))).collect::<BTreeMap<_,_>>()});
    std::fs::write(output.join("actual.json"), serde_json::to_vec(&request).unwrap()).unwrap();
    std::fs::write(output.join("test/actual-transaction.test.mjs"), include_str!("finance_lock_sdk_parity.mjs")).unwrap();
    let tsc = repository.join("website/node_modules/typescript/bin/tsc");
    assert!(tsc.is_file(), "install the repository's declared local website TypeScript dependency before this explicit check");
    let checked = Command::new("node").arg(tsc).args(["-p", "tsconfig.json"]).current_dir(&output).output().unwrap();
    assert!(
        checked.status.success(),
        "TypeScript: {}{}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    let tested = Command::new("node")
        .args(["--test", "test/builder.test.mjs", "test/actual-transaction.test.mjs"])
        .current_dir(&output)
        .output()
        .unwrap();
    assert!(tested.status.success(), "Node: {}{}", String::from_utf8_lossy(&tested.stdout), String::from_utf8_lossy(&tested.stderr));
    let evidence = json!({"schema":"cellscript-actual-lock-sdk-parity-v1","archive_id":archive.identity(),"case_id":record["case_id"],
        "program_elf_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&program.elf)),
        "raw_transaction_hash":hex::encode(tx.hash().as_slice()),"native_payload_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&native)),
        "generated_source_hash":hex::encode(cellscript_artifact_checker::ckb_blake2b256(&std::fs::read(output.join("src/index.ts")).unwrap())),
        "strict_typescript_declarations":"passed","actual_payload_equals_native_and_generated":true,"actual_cell_data_equals_native_and_generated":true,
        "node_test_output":String::from_utf8_lossy(&tested.stdout),"new_vm_execution":false,"node_acceptance":null});
    std::fs::write(output.join("parity.json"), serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}

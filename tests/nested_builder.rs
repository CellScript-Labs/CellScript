#![cfg(feature = "cli")]
use cellscript::nested_builder::{
    encode_nested_cell_data, encode_nested_values, nested_cell_contracts, nested_value_contract, NestedCellFieldInput,
    NestedElementInput,
};
use std::collections::BTreeMap;
use std::process::Command;

const SOURCE: &str = r#"
module nested_builder
struct Record { id: u8, payload: Vec<u8> }
struct Wide { amount: u128, digest: Hash, key: [u8; 17], payload: Vec<u8> }
resource Document has store { id: u64, entries: NestedValues<Record, 32, 256, 4096> }
resource Packet has store { id: u64, payload: NestedValues<Vec<u8>, 32, 256, 4096>, memo: NestedValues<Vec<u8>, 1, 2, 15> }
action inspect(
    witness bytes: NestedValues<Vec<u8>, 32, 256, 4096>,
    witness records: NestedValues<Record, 32, 256, 4096>,
    witness small: NestedValues<Vec<u8>, 1, 2, 15>,
    witness empty: NestedValues<Vec<u8>, 0, 0, 4>,
    witness wide: NestedValues<Wide, 2, 8, 256>
) -> u64 { verification return 0 }
"#;

type OwnedItems = Vec<(Vec<Vec<u8>>, Vec<u8>)>;

#[path = "support/nested_codec_oracle.rs"]
mod codec_oracle;

fn generated_package(source_text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("nested.cell");
    std::fs::write(&source, source_text).unwrap();
    let package = temp.path().join("sdk");
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--target", "typescript", "--target-profile", "ckb"])
        .arg(&source)
        .arg("--output")
        .arg(&package)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    (temp, package)
}

fn native(metadata: &cellscript::CompileMetadata, ty: &str, items: &OwnedItems) -> cellscript::error::Result<Vec<u8>> {
    let fixed = items.iter().map(|(fields, _)| fields.iter().map(Vec::as_slice).collect::<Vec<_>>()).collect::<Vec<_>>();
    let input = items
        .iter()
        .zip(&fixed)
        .map(|((_, payload), fields)| NestedElementInput { fixed_fields: fields, payload })
        .collect::<Vec<_>>();
    encode_nested_values(metadata, ty, &input)
}

#[test]
fn named_data_shapes_keep_native_and_generated_builder_bytes_and_limits() {
    let concrete = SOURCE.replace("verification", "");
    let mut named = concrete.clone();
    let mut declarations = String::new();
    for (name, element, items, payload, total) in [
        ("Messages", "Vec<u8>", 32, 256, 4096),
        ("Entries", "Record", 32, 256, 4096),
        ("Small", "Vec<u8>", 1, 2, 15),
        ("Empty", "Vec<u8>", 0, 0, 4),
        ("WideEntries", "Wide", 2, 8, 256),
    ] {
        named = named.replace(&format!("NestedValues<{element}, {items}, {payload}, {total}>"), name);
        declarations.push_str(&format!("data_shape {name} {{ element: {element}, limits {{ items: {items}, payload_bytes: {payload}, encoded_bytes: {total} }} }}\n"));
    }
    named = named.replace("struct Record", &format!("{declarations}struct Record"));
    let metadata = cellscript::compile_metadata(&named, cellscript::NEXT_EDITION, None).unwrap();
    let baseline = cellscript::compile_metadata(&concrete, cellscript::NEXT_EDITION, None).unwrap();
    assert_eq!(serde_json::to_value(&metadata.actions[0].params).unwrap(), serde_json::to_value(&baseline.actions[0].params).unwrap());
    assert_eq!(
        serde_json::to_value(&metadata.molecule_schema_manifest).unwrap(),
        serde_json::to_value(&baseline.molecule_schema_manifest).unwrap()
    );
    assert_eq!(
        serde_json::to_value(nested_cell_contracts(&metadata).unwrap()).unwrap(),
        serde_json::to_value(nested_cell_contracts(&baseline).unwrap()).unwrap()
    );
    let params = &metadata.actions[0].params;
    let maximum = (0..32).map(|index| (vec![], vec![index as u8; if index < 31 { 120 } else { 116 }])).collect::<OwnedItems>();
    let vectors = vec![
        (&params[0].ty, vec![], true),
        (&params[0].ty, vec![(vec![], vec![1]), (vec![], vec![])], true),
        (&params[0].ty, maximum, true),
        (&params[0].ty, vec![(vec![], vec![]); 33], false),
        (&params[0].ty, vec![(vec![], vec![0; 257])], false),
        (&params[1].ty, vec![(vec![vec![9]], vec![1, 2, 3])], true),
        (&params[1].ty, vec![(vec![vec![9, 10]], vec![1])], false),
        (&params[2].ty, vec![(vec![], vec![1, 2])], true),
        (&params[2].ty, vec![(vec![], vec![1, 2, 3])], false),
        (&params[3].ty, vec![], true),
    ];
    let mut fixtures = Vec::new();
    for (ty, items, accepted) in vectors {
        let actual = native(&metadata, ty, &items);
        assert_eq!(actual.is_ok(), accepted, "{ty}: {actual:?}");
        let expected = native(&baseline, ty, &items);
        assert_eq!(actual.as_ref().ok(), expected.as_ref().ok());
        fixtures.push(serde_json::json!({"type": ty, "items": items.iter().map(|(fields, payload)| serde_json::json!({"fixedFields": fields, "payload": payload})).collect::<Vec<_>>(), "wire": actual.ok().map(hex::encode)}));
    }
    let temp = tempfile::tempdir().unwrap();
    let metadata_path = temp.path().join("named.meta.json");
    std::fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let package = temp.path().join("sdk");
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--target", "typescript", "--metadata"])
        .arg(&metadata_path)
        .arg("--output")
        .arg(&package)
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    std::fs::write(package.join("vectors.json"), serde_json::to_vec(&fixtures).unwrap()).unwrap();
    let check = package.join("check.mjs");
    std::fs::write(&check, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { encodeNestedValues } from './src/index.ts';
for (const vector of JSON.parse(fs.readFileSync(new URL('./vectors.json', import.meta.url), 'utf8'))) {
  const items = vector.items.map(item => ({ fixedFields: item.fixedFields.map(value => Uint8Array.from(value)), payload: Uint8Array.from(item.payload) }));
  if (vector.wire === null) assert.throws(() => encodeNestedValues(vector.type, items));
  else assert.equal(Buffer.from(encodeNestedValues(vector.type, items)).toString('hex'), vector.wire);
}
"#).unwrap();
    let checked = Command::new("node").arg("--experimental-strip-types").arg(check).output().unwrap();
    assert!(checked.status.success(), "{}\n{}", String::from_utf8_lossy(&checked.stdout), String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn nested_source_schema_describes_wire_shape_and_binds_declared_bounds() {
    let compile = |source: &str| cellscript::compile_metadata(source, cellscript::CURRENT_EDITION, None).unwrap();
    let metadata = compile(SOURCE);
    let schema = metadata.types.iter().find(|ty| ty.name == "Document").unwrap().molecule_schema.as_ref().unwrap();
    assert!(schema.schema.contains("vector DocumentEntriesNestedN32P256B4096 <Record>;"), "{}", schema.schema);
    assert!(schema.schema.contains("table Record {"), "{}", schema.schema);
    assert!(!schema.schema.contains("entries: DocumentEntriesBytes"));
    let byte_metadata = compile(&SOURCE.replace("entries: NestedValues<Record,", "entries: NestedValues<Vec<u8>,"));
    let byte_schema = byte_metadata.types.iter().find(|ty| ty.name == "Document").unwrap().molecule_schema.as_ref().unwrap();
    assert!(byte_schema.schema.contains("vector DocumentEntriesNestedN32P256B4096 <DocumentEntriesNestedItemVec>;"));
    assert!(byte_schema.schema.contains("vector DocumentEntriesNestedItemVec <byte>;"));
    for changed in [
        SOURCE.replace("32, 256, 4096", "31, 256, 4096"),
        SOURCE.replace("32, 256, 4096", "32, 255, 4096"),
        SOURCE.replace("32, 256, 4096", "32, 256, 4095"),
    ] {
        let changed = compile(&changed);
        let changed_schema = changed.types.iter().find(|ty| ty.name == "Document").unwrap().molecule_schema.as_ref().unwrap();
        assert_ne!(schema.schema_hash, changed_schema.schema_hash);
        assert_ne!(metadata.molecule_schema_manifest.manifest_hash, changed.molecule_schema_manifest.manifest_hash);
    }
}

#[test]
fn native_and_generated_typescript_nested_codecs_match_independent_vectors_and_limits() {
    let metadata = cellscript::compile_metadata(SOURCE, cellscript::CURRENT_EDITION, None).unwrap();
    let params = &metadata.actions[0].params;
    let bytes_ty = &params[0].ty;
    let record_ty = &params[1].ty;
    let small_ty = &params[2].ty;
    let empty_ty = &params[3].ty;
    let wide_ty = &params[4].ty;
    let bytes = vec![(vec![], vec![0xab]), (vec![], vec![])];
    let record = vec![(vec![vec![7]], vec![0xab])];
    assert_eq!(hex::encode(native(&metadata, bytes_ty, &bytes).unwrap()), "150000000c0000001100000001000000ab00000000");
    assert_eq!(hex::encode(native(&metadata, record_ty, &record).unwrap()), "1a00000008000000120000000c0000000d0000000701000000ab");

    let maximum = (0..32).map(|index| (vec![], vec![index as u8; if index < 31 { 120 } else { 116 }])).collect::<OwnedItems>();
    assert_eq!(native(&metadata, bytes_ty, &maximum).unwrap().len(), 4096);
    let mut too_large = maximum.clone();
    too_large.last_mut().unwrap().1.push(1);
    let record_maximum =
        (0..32).map(|index| (vec![vec![index as u8]], vec![index as u8; if index < 31 { 107 } else { 103 }])).collect::<OwnedItems>();
    assert_eq!(native(&metadata, record_ty, &record_maximum).unwrap().len(), 4096);
    let mut record_too_large = record_maximum.clone();
    record_too_large.last_mut().unwrap().1.push(1);
    let vectors = vec![
        (bytes_ty, vec![], true),
        (record_ty, vec![], true),
        (bytes_ty, bytes, true),
        (record_ty, record, true),
        (record_ty, vec![(vec![vec![0]], vec![]), (vec![vec![255]], vec![1, 2, 3])], true),
        (bytes_ty, vec![(vec![], vec![0xa5; 256])], true),
        (record_ty, vec![(vec![vec![0xff]], vec![0xa5; 256])], true),
        (bytes_ty, maximum, true),
        (record_ty, record_maximum, true),
        (empty_ty, vec![], true),
        (small_ty, vec![(vec![], vec![1, 2])], true),
        (bytes_ty, too_large, false),
        (record_ty, record_too_large, false),
        (record_ty, vec![(vec![vec![1]], vec![]); 33], false),
        (record_ty, vec![(vec![vec![1]], vec![0; 257])], false),
        (bytes_ty, vec![(vec![], vec![]); 33], false),
        (bytes_ty, vec![(vec![], vec![0; 257])], false),
        (record_ty, vec![(vec![], vec![1])], false),
        (record_ty, vec![(vec![vec![1, 2]], vec![1])], false),
        (bytes_ty, vec![(vec![vec![1]], vec![])], false),
        (small_ty, vec![(vec![], vec![1, 2, 3])], false),
        (empty_ty, vec![(vec![], vec![])], false),
        (
            wide_ty,
            vec![(vec![(u128::from(u64::MAX) + 2).to_le_bytes().to_vec(), vec![0xaa; 32], (3..20).collect()], vec![0xab])],
            true,
        ),
    ];
    let mut fixtures = Vec::new();
    for (ty, items, success) in vectors {
        let result = native(&metadata, ty, &items);
        assert_eq!(result.is_ok(), success, "{ty}: {result:?}");
        let wire = result.ok();
        if let Some(wire) = &wire {
            let (widths, count, payload, total): (&[usize], _, _, _) = if ty == record_ty {
                (&[1], 32, 256, 4096)
            } else if ty == wide_ty {
                (&[16, 32, 17], 2, 8, 256)
            } else if ty == small_ty {
                (&[], 1, 2, 15)
            } else if ty == empty_ty {
                (&[], 0, 0, 4)
            } else {
                assert_eq!(ty, bytes_ty);
                (&[], 32, 256, 4096)
            };
            let decoded = codec_oracle::decode(wire, widths, count, payload, total).unwrap();
            assert_eq!(decoded, items, "independent round trip for {ty}");
            assert_eq!(native(&metadata, ty, &decoded).unwrap(), *wire);
            let contract = nested_value_contract(&metadata, ty).unwrap();
            contract.check(wire).unwrap();
            let mut trailing = wire.clone();
            trailing.push(0);
            let mut rebound_size = trailing.clone();
            rebound_size[..4].copy_from_slice(&(trailing.len() as u32).to_le_bytes());
            let mut malformed = vec![wire[..wire.len() - 1].to_vec(), trailing, rebound_size];
            if wire.len() > 4 {
                let mut bad_offset = wire.clone();
                bad_offset[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
                malformed.push(bad_offset);
            }
            for bytes in malformed {
                assert!(codec_oracle::decode(&bytes, widths, count, payload, total).is_none(), "oracle: {ty}");
                assert!(contract.check(&bytes).is_err(), "contract: {ty}");
            }
        }
        fixtures.push(serde_json::json!({"type": ty, "items": items.iter().map(|(fields,payload)| serde_json::json!({"fixedFields":fields,"payload":payload})).collect::<Vec<_>>(), "wire": wire.map(hex::encode)}));
    }
    assert!(native(&metadata, "NestedValues<Vec<u8>, 31, 256, 4096>", &vec![]).is_err());
    let (_temp, package) = generated_package(SOURCE);
    std::fs::write(package.join("vectors.json"), serde_json::to_vec(&fixtures).unwrap()).unwrap();
    let check = package.join("check.mjs");
    std::fs::write(&check, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { encodeNestedValues } from './src/index.ts';
const vectors = JSON.parse(fs.readFileSync(new URL('./vectors.json', import.meta.url)));
for (const vector of vectors) {
  const items = vector.items.map(item => ({ fixedFields: item.fixedFields.map(field => Uint8Array.from(field)), payload: Uint8Array.from(item.payload) }));
  if (vector.wire === null) assert.throws(() => encodeNestedValues(vector.type, items));
  else assert.equal(Buffer.from(encodeNestedValues(vector.type, items)).toString('hex'), vector.wire);
}

assert.throws(() => encodeNestedValues('__proto__', []));
assert.throws(() => encodeNestedValues('NestedValues<Vec<u8>, 31, 256, 4096>', []));
assert.throws(() => encodeNestedValues(vectors[0].type, [{fixedFields: [], payload: [1]}]));
assert.throws(() => encodeNestedValues(vectors[0].type, new Array(1)));
assert.throws(() => encodeNestedValues(vectors[1].type, [{fixedFields: new Array(1), payload: new Uint8Array()}]));
"#).unwrap();
    let checked = Command::new("node").arg("--experimental-strip-types").arg(check).output().unwrap();
    assert!(checked.status.success(), "{}\n{}", String::from_utf8_lossy(&checked.stdout), String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn complete_nested_cell_data_agrees_across_builders_and_actual_capacity() {
    use cellscript_ckb_adapter::{build_action_transaction, CellOutputWithData, ResolvedActionTx};
    use ckb_types::{bytes::Bytes, core::Capacity, packed, prelude::*};
    let metadata = cellscript::compile_metadata(SOURCE, cellscript::CURRENT_EDITION, None).unwrap();
    let id = 42u64.to_le_bytes();
    let records = [NestedElementInput { fixed_fields: &[&[7]], payload: &[0xab] }];
    let mut document = BTreeMap::from([
        ("entries".into(), NestedCellFieldInput::Nested(&records)),
        ("id".into(), NestedCellFieldInput::FixedBytes(&id)),
    ]);
    let data = encode_nested_cell_data(&metadata, "Document", &document).unwrap();
    assert_eq!(
        hex::encode(&data),
        concat!("2e0000000c000000140000002a00000000000000", "1a00000008000000120000000c0000000d0000000701000000ab")
    );
    assert!(encode_nested_cell_data(&metadata, "Record", &document).is_err());
    document.insert("extra".into(), NestedCellFieldInput::FixedBytes(&[0]));
    assert!(encode_nested_cell_data(&metadata, "Document", &document).is_err());
    document.remove("extra");
    document.remove("id");
    assert!(encode_nested_cell_data(&metadata, "Document", &document).is_err());
    document.insert("id".into(), NestedCellFieldInput::FixedBytes(&[42]));
    assert!(encode_nested_cell_data(&metadata, "Document", &document).is_err());
    document.insert("id".into(), NestedCellFieldInput::FixedBytes(&id));
    document.insert("entries".into(), NestedCellFieldInput::FixedBytes(&data[20..]));
    assert!(encode_nested_cell_data(&metadata, "Document", &document).is_err());

    let payloads = (0..32).map(|index| vec![index as u8; if index < 31 { 120 } else { 116 }]).collect::<Vec<_>>();
    let items = payloads.iter().map(|payload| NestedElementInput { fixed_fields: &[], payload }).collect::<Vec<_>>();
    let packet = BTreeMap::from([
        ("memo".into(), NestedCellFieldInput::Nested(&[])),
        ("payload".into(), NestedCellFieldInput::Nested(&items)),
        ("id".into(), NestedCellFieldInput::FixedBytes(&id)),
    ]);
    let maximum = encode_nested_cell_data(&metadata, "Packet", &packet).unwrap();
    assert_eq!(maximum.len(), 4124); // Whole Cell includes headers, id and empty memo.
    let contracts = nested_cell_contracts(&metadata).unwrap();
    assert_eq!(contracts["Packet"].max_encoded_bytes, 4135);
    assert_eq!(
        contracts["Document"].schema_hash,
        metadata.types.iter().find(|ty| ty.name == "Document").unwrap().molecule_schema.as_ref().unwrap().schema_hash
    );

    let (_temp, package) = generated_package(SOURCE);
    std::fs::write(
        package.join("cells.json"),
        serde_json::to_vec(&serde_json::json!({"document":hex::encode(&data),"maximum":hex::encode(&maximum)})).unwrap(),
    )
    .unwrap();
    let check = package.join("cells.mjs");
    std::fs::write(&check, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { encodeNestedCellData, nestedCellContracts } from './src/index.ts';
const expected = JSON.parse(fs.readFileSync(new URL('./cells.json', import.meta.url)));
const id = Uint8Array.from([42,0,0,0,0,0,0,0]);
const entries = [{fixedFields: [Uint8Array.of(7)], payload: Uint8Array.of(171)}];
assert.equal(Buffer.from(encodeNestedCellData('Document', {entries, id})).toString('hex'), expected.document);
const payload = Array.from({length:32}, (_, i) => ({fixedFields: [], payload: new Uint8Array(i < 31 ? 120 : 116).fill(i)}));
assert.equal(Buffer.from(encodeNestedCellData('Packet', {memo: [], payload, id})).toString('hex'), expected.maximum);
assert.equal(nestedCellContracts.Packet.max_encoded_bytes, 4135);
for (const fields of [{entries}, {entries,id:Uint8Array.of(42)}, {entries,id,extra:0}, {entries:new Uint8Array(),id}, {entries,id:[42]}, Object.assign(Object.create({id}),{entries})]) {
  assert.throws(() => encodeNestedCellData('Document', fields));
}
assert.throws(() => encodeNestedCellData('Record', {entries,id}));
assert.throws(() => encodeNestedCellData('__proto__', {}));
"#).unwrap();
    let checked = Command::new("node").arg("--experimental-strip-types").arg(check).output().unwrap();
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));

    // The normal adapter computes capacity from actual scripts and complete
    // data, without a separate codec-specific formula or transaction serializer.
    for bytes in [data, maximum] {
        let lock =
            packed::Script::new_builder().code_hash([1u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![2; 20]).pack()).build();
        let type_script =
            packed::Script::new_builder().code_hash([3u8; 32].pack()).hash_type(2u8).args(Bytes::from(vec![4; 32]).pack()).build();
        let output = packed::CellOutput::new_builder().lock(lock).type_(Some(type_script).pack()).build();
        let occupied = output.occupied_capacity(Capacity::bytes(bytes.len()).unwrap()).unwrap().as_u64();
        let mut resolved = ResolvedActionTx {
            metadata_hash: "fixture".into(),
            artifact_hash: None,
            action_selector: "inspect".into(),
            inputs: vec![],
            outputs: vec![CellOutputWithData {
                output: output.as_builder().capacity(occupied).build(),
                data: Bytes::from(bytes.clone()),
            }],
            witnesses: vec![],
            cell_deps: vec![],
            header_deps: vec![],
            lineage: vec![],
            fee_shannons: 0,
        };
        let (transaction, evidence) = build_action_transaction(&resolved).unwrap();
        assert_eq!(transaction.outputs_data().get(0).unwrap().raw_data().as_ref(), bytes.as_slice());
        assert_eq!(evidence.occupied_capacity_shannons, occupied);
        resolved.outputs[0].output = resolved.outputs[0].output.clone().as_builder().capacity(occupied - 1).build();
        assert!(build_action_transaction(&resolved).unwrap_err().to_string().contains("below occupied capacity"));
    }
}

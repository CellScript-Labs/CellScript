#![cfg(feature = "cli")]
use cellscript::fixed_cell_builder::{encode_fixed_cell_data, fixed_cell_contracts};
use std::{collections::BTreeMap, process::Command};

const SOURCE: &str = r#"
module fixed_builder
resource Packet has store {
    flag: bool, small: u8, half: u16, word: u32, signed: i32,
    large: u64, wide: u128, asset: Hash, owner: Address, bytes: [u8; 3]
}
struct Plain { id: u64 }
resource Aggregate has store { value: Plain }
resource Dynamic has store { value: Vec<u8> }
resource __proto__ has store { value: u8 }
action inspect() -> u64 { verification return 0 }
"#;

#[test]
fn fixed_cell_codecs_match_source_order_and_reject_noncanonical_or_unsupported_inputs() {
    let metadata = cellscript::compile_metadata(SOURCE, cellscript::CellScriptEdition::Edition2026, None).unwrap();
    let fields = BTreeMap::from([
        ("flag".into(), vec![1]),
        ("small".into(), vec![255]),
        ("half".into(), u16::MAX.to_le_bytes().to_vec()),
        ("word".into(), u32::MAX.to_le_bytes().to_vec()),
        ("signed".into(), i32::MIN.to_le_bytes().to_vec()),
        ("large".into(), u64::MAX.to_le_bytes().to_vec()),
        ("wide".into(), u128::MAX.to_le_bytes().to_vec()),
        ("asset".into(), vec![0x5a; 32]),
        ("owner".into(), vec![0xa5; 32]),
        ("bytes".into(), vec![1, 2, 3]),
    ]);
    let encode = |fields: &BTreeMap<String, Vec<u8>>| {
        encode_fixed_cell_data(&metadata, "Packet", &fields.iter().map(|(name, bytes)| (name.clone(), bytes.as_slice())).collect())
    };
    let expected = [
        vec![1, 255],
        u16::MAX.to_le_bytes().to_vec(),
        u32::MAX.to_le_bytes().to_vec(),
        i32::MIN.to_le_bytes().to_vec(),
        u64::MAX.to_le_bytes().to_vec(),
        u128::MAX.to_le_bytes().to_vec(),
        vec![0x5a; 32],
        vec![0xa5; 32],
        vec![1, 2, 3],
    ]
    .concat();
    assert_eq!(encode(&fields).unwrap(), expected);
    let contracts = fixed_cell_contracts(&metadata).unwrap();
    assert_eq!(contracts.keys().collect::<Vec<_>>(), vec!["Packet", "__proto__"]);
    for name in fields.keys() {
        let mut short = fields.clone();
        short.get_mut(name).unwrap().pop();
        assert!(encode(&short).is_err(), "short {name}");
        short.remove(name);
        assert!(encode(&short).is_err(), "missing {name}");
    }
    let mut extra = fields.clone();
    extra.insert("unexpected".into(), vec![]);
    assert!(encode(&extra).is_err());
    let mut boolean = fields.clone();
    boolean.insert("flag".into(), vec![2]);
    assert!(encode(&boolean).is_err());
    for ty in ["Plain", "Aggregate", "Dynamic", "toString", "Unknown"] {
        assert!(encode_fixed_cell_data(&metadata, ty, &BTreeMap::new()).is_err());
    }
    for mutation in 0..6 {
        let mut changed = metadata.clone();
        let packet = changed.types.iter_mut().find(|definition| definition.name == "Packet").unwrap();
        match mutation {
            0 => packet.fields[1].offset += 1,
            1 => packet.fields[1].name = "flag".into(),
            2 => packet.fields[1].encoded_size = Some(2),
            3 => packet.encoded_size = Some(1),
            4 => packet.molecule_schema.as_mut().unwrap().layout = "molecule-table-v1".into(),
            5 => packet.fields[1].fixed_width = false,
            _ => unreachable!(),
        }
        assert!(fixed_cell_contracts(&changed).is_err(), "metadata mutation {mutation}");
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::write(root.join("source.cell"), SOURCE).unwrap();
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--target", "typescript", "--target-profile", "ckb"])
        .arg(root.join("source.cell"))
        .arg("--output")
        .arg(root.join("sdk"))
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    std::fs::write(
        root.join("vectors.json"),
        serde_json::to_vec(&serde_json::json!({
            "fields":fields,"expected":expected,"contracts":contracts,
        }))
        .unwrap(),
    )
    .unwrap();
    let script = root.join("check.mjs");
    std::fs::write(
        &script,
        r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as sdk from './sdk/src/index.ts';
const v = JSON.parse(fs.readFileSync(new URL('./vectors.json', import.meta.url)));
assert.deepEqual(sdk.fixedCellContracts, v.contracts);
assert.deepEqual([...sdk.encodeFixedCellData('__proto__', {value:Uint8Array.of(7)})], [7]);
const fields = Object.fromEntries(Object.entries(v.fields).reverse().map(([name, bytes]) => [name, Uint8Array.from(bytes)]));
const encode = input => sdk.encodeFixedCellData('Packet', input);
assert.deepEqual([...encode(fields)], v.expected);
for (const name of Object.keys(fields)) {
  assert.throws(() => encode({...fields, [name]: fields[name].subarray(1)}));
  const missing = {...fields}; delete missing[name]; assert.throws(() => encode(missing));
}
assert.throws(() => encode({...fields, flag: Uint8Array.of(2)}));
assert.throws(() => encode({...fields, bytes: [1, 2, 3]}));
assert.throws(() => encode({...fields, extra: new Uint8Array()}));
assert.throws(() => encode({...fields, [Symbol('extra')]: 1}));
assert.throws(() => encode(Object.create(fields)));
for (const type of ['Plain','Aggregate','Dynamic','toString','Unknown']) assert.throws(() => sdk.encodeFixedCellData(type, fields));
// A later field getter cannot change a field already validated and copied.
const snapshot = {...fields, flag: Uint8Array.of(1)};
Object.defineProperty(snapshot, 'asset', {enumerable:true, get() { snapshot.flag[0] = 2; return fields.asset; }});
assert.deepEqual([...encode(snapshot)], v.expected);
"#,
    )
    .unwrap();
    let result = Command::new("node").arg("--experimental-strip-types").arg(script).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}

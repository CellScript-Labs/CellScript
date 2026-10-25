//! Complete variable-payload migration through the source-derived codecs,
//! generated SDK, ProtocolBundle adapter and all required Script groups.
use super::support::{bind_deployment_lines, compile, compile_at, context, identity, participants, reference};
use super::*;
use cellscript::nested_builder::encode_nested_values;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

struct Case {
    input: ProtocolBundleInput,
    vector: Value,
    count: usize,
    reverse: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    Bytes,
    Records,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Message {
    #[serde(rename = "fixedFields")]
    fixed_fields: Vec<Vec<u8>>,
    payload: Vec<u8>,
}

fn message(shape: Shape, revision: u64, payload: Vec<u8>) -> Message {
    Message { fixed_fields: if shape == Shape::Records { vec![revision.to_le_bytes().to_vec()] } else { vec![] }, payload }
}

pub(super) fn package_sources(source: &str, shape: Shape) -> (String, String) {
    let schemas = source.find(if shape == Shape::Records { "struct Message" } else { "data_shape Messages" }).unwrap();
    let data_shape = source.find("data_shape Messages").unwrap();
    let resource = source.find("resource Document").unwrap();
    let catalog = source.find("cell_roles Migration").unwrap();
    let helpers = source.find(if shape == Shape::Records { "fn same_payload" } else { "fn same_message" }).unwrap();
    let action = source.find("action migrate").unwrap();
    let library = format!(
        "module shared::rules\n{}\n{}",
        source[schemas..catalog].replace("resource ", "public resource ").replace("struct ", "public struct "),
        source[helpers..action].replace("fn ", "public fn ")
    );
    let imports = if shape == Shape::Records { "use shared::rules::Message\n" } else { "" };
    let application = format!(
        "{}\nuse shared::rules::Document\nuse shared::rules::Registry\nuse shared::rules::migrate_document\n{imports}{}\n{}\n{}",
        &source[..schemas],
        &source[data_shape..resource],
        &source[catalog..helpers],
        &source[action..]
    );
    (application, library)
}

fn artifacts(root: &Path, shape: Shape, explicit: bool, opt_level: u8, cross_package: bool) -> (Vec<CompileResult>, Vec<Value>) {
    let mut source = match (shape, explicit) {
        (Shape::Bytes, false) => include_str!("../../examples/language/batches/nested_migration.cell"),
        (Shape::Bytes, true) => include_str!("../fixtures/nested_migration_explicit.cell"),
        (Shape::Records, false) => include_str!("../../examples/language/batches/record_migration.cell"),
        (Shape::Records, true) => include_str!("../fixtures/record_migration_explicit.cell"),
    }
    .to_string();
    for (placeholder, tag) in [("11", 1), ("22", 2), ("33", 3)] {
        let hash = cellscript::script_handle::ckb_script_identity_hash(&identity(tag)).unwrap();
        source = source.replace(&placeholder.repeat(32), &hash[2..]);
    }
    let formatted = cellscript::fmt::format_default(&cellscript::frontend::parse(&source, NEXT_EDITION).unwrap()).unwrap();
    assert_eq!(formatted, cellscript::fmt::format_default(&cellscript::frontend::parse(&formatted, NEXT_EDITION).unwrap()).unwrap());
    let main = if cross_package {
        assert!(!explicit);
        let (application, library) = package_sources(&source, shape);
        super::support::compile_package(root, &application, &library, opt_level).unwrap()
    } else {
        compile_at(&source, opt_level)
    };
    let compiled = vec![
        main,
        compile(include_str!("../fixtures/nested_migration_registry.cell")),
        compile("module auth\nlock authorize() -> bool { return true }"),
    ];
    let artifacts = participants(
        root,
        &[("migration", "action", "migrate"), ("registry", "action", "verify"), ("auth", "lock", "authorize")],
        &compiled,
    );
    (compiled, artifacts)
}

/// Independent canonical Molecule table/DynVec construction. Field order is
/// taken from this fixture's declared schema, not compiler offsets or encoders.
fn molecule_fields(fields: &[Vec<u8>]) -> Vec<u8> {
    if fields.is_empty() {
        return 4u32.to_le_bytes().to_vec();
    }
    let header = 4 * (1 + fields.len());
    let total = header + fields.iter().map(Vec::len).sum::<usize>();
    let mut wire = (total as u32).to_le_bytes().to_vec();
    let mut offset = header;
    for field in fields {
        wire.extend((offset as u32).to_le_bytes());
        offset += field.len();
    }
    for field in fields {
        wire.extend(field);
    }
    wire
}

fn message_oracle(messages: &[Message]) -> Vec<u8> {
    molecule_fields(
        &messages
            .iter()
            .map(|message| {
                let bytes = [(message.payload.len() as u32).to_le_bytes().to_vec(), message.payload.clone()].concat();
                if message.fixed_fields.is_empty() {
                    bytes
                } else {
                    let mut fields = message.fixed_fields.clone();
                    fields.push(bytes);
                    molecule_fields(&fields)
                }
            })
            .collect::<Vec<_>>(),
    )
}

fn document(metadata: &cellscript::CompileMetadata, id: u64, revision: u64, messages: &[Message]) -> Vec<u8> {
    let fields = messages.iter().map(|message| message.fixed_fields.iter().map(Vec::as_slice).collect::<Vec<_>>()).collect::<Vec<_>>();
    let items = messages
        .iter()
        .zip(&fields)
        .map(|(message, fields)| Item { fixed_fields: fields, payload: &message.payload })
        .collect::<Vec<_>>();
    let actual = encode_nested_cell_data(
        metadata,
        "Document",
        &BTreeMap::from([
            ("messages".into(), Field::Nested(&items)),
            ("id".into(), Field::FixedBytes(&id.to_le_bytes())),
            ("revision".into(), Field::FixedBytes(&revision.to_le_bytes())),
        ]),
    )
    .unwrap();
    assert_eq!(actual, molecule_fields(&[message_oracle(messages), id.to_le_bytes().to_vec(), revision.to_le_bytes().to_vec()]));
    actual
}

fn case(
    compiled: &[CompileResult],
    artifacts: &[Value],
    shape: Shape,
    count: usize,
    reverse: bool,
    payload_override: Option<usize>,
) -> Case {
    let metadata = &compiled[0].metadata;
    let cell = |tag: Option<u8>, data: Vec<u8>| {
        json!({
            "cell_commitment": format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&data))),
            "capacity": 400_000_000_000u64, "lock": identity(3), "type": tag.map(identity), "data": format!("0x{}", hex::encode(data))
        })
    };
    let maximum = count == 8 || (shape == Shape::Records && count == 7);
    let payload_bytes = payload_override.unwrap_or(if maximum {
        if shape == Shape::Records && count == 8 {
            240
        } else {
            256
        }
    } else {
        2
    });
    let revision = if shape == Shape::Records { 7u64 } else { 4 };
    let note = message(shape, revision + 1, vec![0xab; payload_bytes]);
    let note_fields = note.fixed_fields.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let note_wire = encode_nested_values(
        metadata,
        &metadata.actions[0].params[0].ty,
        &[Item { fixed_fields: &note_fields, payload: &note.payload }],
    )
    .unwrap();
    assert_eq!(note_wire, message_oracle(std::slice::from_ref(&note)));
    let mut inputs = vec![cell(None, vec![])];
    let mut outputs = vec![cell(None, vec![])];
    let mut documents = Vec::new();
    for index in 0..count {
        let old = (0..if maximum {
            7
        } else if count == 1 {
            0
        } else {
            index % 3 + 1
        })
            .map(|item| {
                message(shape, item as u64 + 1, vec![(index + item + 1) as u8; if maximum { payload_bytes } else { item * 9 }])
            })
            .collect::<Vec<_>>();
        let mut updated = old.clone();
        updated.push(note.clone());
        let before = document(metadata, index as u64, revision, &old);
        let after = document(metadata, index as u64, revision + 1, &updated);
        inputs.push(cell(Some(1), before.clone()));
        outputs.push(cell(Some(1), after.clone()));
        documents.push(json!({"inputIndex": index + 1, "outputIndex": if reverse {count-index} else {index+1},
            "id": hex::encode((index as u64).to_le_bytes()), "beforeRevision": hex::encode(revision.to_le_bytes()),
            "afterRevision": hex::encode((revision + 1).to_le_bytes()), "beforeMessages": old, "afterMessages": updated,
            "beforeWire": hex::encode(before), "afterWire": hex::encode(after)}));
    }
    inputs.push(cell(Some(2), [7u64.to_le_bytes(), revision.to_le_bytes()].concat()));
    outputs.push(cell(Some(2), [7u64.to_le_bytes(), (revision + 1).to_le_bytes()].concat()));
    if reverse {
        outputs.reverse();
    }
    for (index, input) in inputs.iter_mut().enumerate() {
        input["out_point"] = json!({"tx_hash": format!("0x{}", "90".repeat(32)), "index": index});
        input["since"] = json!(0);
    }
    let mut witnesses = vec![json!({}); inputs.len()];
    let mut claims = Vec::new();
    for (artifact, index, args) in [(0, 1, vec![cellscript::EntryWitnessArg::Bytes(note_wire.clone())]), (1, count + 1, vec![])] {
        let bytes = compiled[artifact].metadata.actions[0].entry_witness_args(&args).unwrap();
        let commitment = format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&bytes)));
        witnesses[index] = json!({"input_type": commitment, "input_type_bytes": format!("0x{}", hex::encode(bytes))});
        claims.push(json!({"artifact": if artifact == 0 {"migration"} else {"registry"}, "name": "entry", "index": index,
            "field": "input_type", "ownership": "exclusive-write", "abi": "cellscript-entry-witness-v1", "value_commitment": commitment}));
    }
    let mut input: ProtocolBundleInput = serde_json::from_value(json!({
        "schema": "cellscript-protocol-bundle-input-v1", "network": artifacts[0]["deployment"]["network"],
        "artifacts": artifacts, "witnesses": claims,
        "roles": [
            {"artifact":"migration", "name":"documents", "location":"input", "index":1, "ownership":"shared-read", "expected_type":identity(1)},
            {"artifact":"registry", "name":"registry", "location":"input", "index":count+1, "ownership":"shared-read", "expected_type":identity(2)},
            {"artifact":"auth", "name":"authorization", "location":"input", "index":0, "ownership":"shared-read", "expected_lock":identity(3)}],
        "transaction": {"version":0, "inputs":inputs, "outputs":outputs, "witnesses":witnesses,
            "cell_deps":artifacts.iter().map(|a|a["deployment"]["code_cell_dep"].clone()).collect::<Vec<_>>(), "header_deps":[],
            "fee_policy_hash":format!("0x{}", "de".repeat(32)), "change_policy_hash":format!("0x{}", "ef".repeat(32))}
    })).unwrap();
    bind_deployment_lines(&mut input, compiled, 3);
    Case {
        input,
        count,
        reverse,
        vector: json!({"documents":documents, "note":note,
        "noteType":metadata.actions[0].params[0].ty, "noteWire":hex::encode(note_wire)}),
    }
}

fn check_sdk(root: &Path, vectors: &[Value]) -> Vec<Value> {
    std::fs::write(root.join("vectors.json"), serde_json::to_vec(vectors).unwrap()).unwrap();
    let script = root.join("migration-check.mjs");
    std::fs::write(&script, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as sdk from './migration-sdk/src/index.ts';
const bytes = hex => new Uint8Array(Buffer.from(hex, 'hex'));
const hex = value => Buffer.from(value).toString('hex');
const items = values => values.map(value => ({fixedFields:value.fixedFields.map(field => Uint8Array.from(field)), payload:Uint8Array.from(value.payload)}));
const vectors = JSON.parse(fs.readFileSync(new URL('./vectors.json', import.meta.url)));
const generated = vectors.map(vector => {
  const noteWire = sdk.encodeNestedValues(vector.noteType, items([vector.note]));
  assert.equal(hex(noteWire), vector.noteWire);
  const plan = sdk.planMigrate({release_note: noteWire});
  assert.equal(plan.canSubmit, false);
  for (const blocker of ['nested-values-artifact-proof', 'cell-data-view-artifact-proof', 'exact-role-selection-artifact-proof']) {
    assert.ok(plan.failClosedRuntimeFeatures.includes(blocker));
  }
  const documents = vector.documents.map(d => {
    const encode = (messages, revision) => sdk.encodeNestedCellData('Document', {messages:items(messages), id:bytes(d.id), revision:bytes(revision)});
    const before = encode(d.beforeMessages, d.beforeRevision);
    const after = encode(d.afterMessages, d.afterRevision);
    assert.equal(hex(before), d.beforeWire);
    assert.equal(hex(after), d.afterWire);
    return {inputIndex:d.inputIndex, outputIndex:d.outputIndex, beforeWire:hex(before), afterWire:hex(after)};
  });
  assert.throws(() => sdk.encodeNestedValues(vector.noteType, items([{...vector.note, payload:new Array(257).fill(0)}])));
  assert.throws(() => sdk.encodeNestedValues(vector.noteType, items(new Array(9).fill(vector.note))));
  return {noteWire:hex(noteWire), documents};
});
fs.writeFileSync(new URL('./generated.json', import.meta.url), JSON.stringify(generated));
"#).unwrap();
    let output = Command::new("node").arg("--experimental-strip-types").arg(script).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&std::fs::read(root.join("generated.json")).unwrap()).unwrap()
}

fn change_record_revision(cell: &mut cellscript::protocol_bundle::ProtocolCellSlot, ordinal: usize, revision: u64) {
    let mut bytes = hex::decode(&cell.data.as_ref().unwrap()[2..]).unwrap();
    let offset = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    let messages = offset(4);
    let record = messages + offset(messages + 4 * (ordinal + 1));
    let field = record + offset(record + 4);
    bytes[field..field + 8].copy_from_slice(&revision.to_le_bytes());
    cell.data = Some(format!("0x{}", hex::encode(bytes)));
}

fn assert_business_rejection(input: &ProtocolBundleInput, root: &Path, compiled: &[CompileResult]) {
    let report = check_protocol_bundle(input, root).unwrap();
    assert_eq!(report.status, "ok");
    let (tx, _) = cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
    let tx = packed::Transaction::from_slice(tx.data().as_slice()).unwrap().into_view();
    let error = context(input, compiled, None).verify_tx(&tx, 10_000_000).unwrap_err();
    assert!(format!("{error:?}").contains("error code 5"), "business predicate: {error:?}");
}

#[test]
fn nested_migration_bundle_uses_identical_sdk_codec_and_transaction_bytes() {
    for opt_level in [0, 3] {
        let named = run_pipeline(Shape::Bytes, false, opt_level, false);
        let explicit = run_pipeline(Shape::Bytes, true, opt_level, false);
        assert_eq!(named, explicit, "named/explicit complete byte-list transaction bytes O{opt_level}");
    }
}

#[test]
fn record_migration_bundle_preserves_identities_and_named_explicit_authoring() {
    for opt_level in [0, 3] {
        let named = run_pipeline(Shape::Records, false, opt_level, false);
        let explicit = run_pipeline(Shape::Records, true, opt_level, false);
        assert_eq!(named, explicit, "named/explicit complete transaction bytes O{opt_level}");
    }
}

#[test]
fn nested_migration_cross_package_helpers_preserve_codec_and_transaction_outcomes() {
    for shape in [Shape::Bytes, Shape::Records] {
        for opt in [0, 3] {
            let local = run_pipeline(shape, false, opt, false);
            let packaged = run_pipeline(shape, false, opt, true);
            assert_eq!(packaged, local, "{shape:?} O{opt}: package extraction changed transaction bytes");
        }
    }
}

#[test]
fn nested_migration_cross_package_rejects_bound_and_source_substitution() {
    for (shape, source) in [
        (Shape::Bytes, include_str!("../../examples/language/batches/nested_migration.cell")),
        (Shape::Records, include_str!("../../examples/language/batches/record_migration.cell")),
    ] {
        let (application, library) = package_sources(source, shape);
        for invalid in [
            application.replace("payload_bytes: 256", "payload_bytes: 255"),
            application.replace("migrate_document(selected.documents.input(i)", "migrate_document(selected.registry.input(0)"),
            application.replace("migrate_document(selected.documents.input(i)", "migrate_document(selected.documents.successor(i)"),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let error = super::support::compile_package(temp.path(), &invalid, &library, 3).unwrap_err();
            assert!(
                std::iter::once(&error).chain(error.related.iter()).any(|diagnostic| diagnostic.message.contains("type mismatch")),
                "{shape:?}: {error:?}"
            );
        }
    }
}

fn run_pipeline(shape: Shape, explicit: bool, opt_level: u8, cross_package: bool) -> Vec<Vec<u8>> {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (compiled, artifacts) = artifacts(root, shape, explicit, opt_level, cross_package);
    let frames = compiled[0]
        .verified_lowering_record
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .filter(|entry| entry.id == "action:migrate" || entry.id.starts_with("helper:"))
        .map(|entry| (entry.id.clone(), entry.frame_size_bytes))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(frames.len(), if shape == Shape::Records { 4 } else { 3 });
    eprintln!(
        "migration resources: {}",
        json!({"declared_frames":frames,
        "elf_bytes":compiled.iter().map(|result| result.artifact_bytes.len()).collect::<Vec<_>>()})
    );
    let counts = if shape == Shape::Records { vec![1, 3, 7, 8] } else { vec![1, 3, 8] };
    let cases = counts
        .into_iter()
        .flat_map(|count| [false, true].map(|reverse| case(&compiled, &artifacts, shape, count, reverse, None)))
        .collect::<Vec<_>>();
    let mut transactions = Vec::new();
    let generated = check_sdk(root, &cases.iter().map(|case| case.vector.clone()).collect::<Vec<_>>());
    assert_eq!(cases.len(), generated.len());
    if shape == Shape::Records {
        // Each value is within its declared limits, but all eight full-width
        // histories together exceed the transaction's aggregate byte bound.
        let over = case(&compiled, &artifacts, shape, 8, false, Some(256));
        let error = check_protocol_bundle(&over.input, root).unwrap_err();
        assert!(error.to_string().contains("aggregate byte"), "{error}");
        let tx = packed::Transaction::from_slice(&reference(&over.input)).unwrap().into_view();
        let error = context(&over.input, &compiled, None).verify_tx(&tx, 10_000_000).unwrap_err();
        assert!(format!("{error:?}").contains("error code 21"), "aggregate maximum: {error:?}");
    }
    for (case, sdk) in cases.iter().zip(generated) {
        let input = &case.input;
        let mut rebuilt = input.clone();
        for document in sdk["documents"].as_array().unwrap() {
            rebuilt.transaction.inputs[document["inputIndex"].as_u64().unwrap() as usize].data =
                Some(format!("0x{}", document["beforeWire"].as_str().unwrap()));
            rebuilt.transaction.outputs[document["outputIndex"].as_u64().unwrap() as usize].data =
                Some(format!("0x{}", document["afterWire"].as_str().unwrap()));
        }
        let payload = compiled[0].metadata.actions[0]
            .entry_witness_args(&[cellscript::EntryWitnessArg::Bytes(hex::decode(sdk["noteWire"].as_str().unwrap()).unwrap())])
            .unwrap();
        rebuilt.transaction.witnesses[1].input_type_bytes = Some(format!("0x{}", hex::encode(payload)));
        assert_eq!(serde_json::to_value(&rebuilt).unwrap(), serde_json::to_value(input).unwrap());
        let report = check_protocol_bundle(&rebuilt, root).unwrap();
        assert_eq!(report.status, "ok", "{report:?}");
        let (tx, evidence) =
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(tx.data().as_slice(), reference(input));
        transactions.push(tx.data().as_slice().to_vec());
        assert_eq!(evidence.script_groups.len(), 3);
        assert_eq!(evidence.exact_role_selections["migration"][0].cells.len(), 2 * (case.count + 1));
        assert_eq!(evidence.ckb_vm_execution, "not-executed");
        let vm_tx = packed::Transaction::from_slice(tx.data().as_slice()).unwrap().into_view();
        let fixture = context(input, &compiled, None);
        let cycles = fixture.verify_tx(&vm_tx, 10_000_000).unwrap();
        if opt_level == 3 && !explicit {
            let measure = if cross_package { super::resources::measure_package } else { super::resources::measure };
            measure(&format!("{shape:?}-{}-{}-valid", case.count, case.reverse), &fixture, input, &compiled, &vm_tx, &[0, 0, 0]);
        }
        eprintln!("migration bundle: shape={shape:?} explicit={explicit} O{opt_level} documents={} reverse={} cycles={cycles} tx_bytes={} witness_bytes={} occupied_output={} protected_bytes={} elf_bytes={}",
            case.count, case.reverse, tx.data().as_slice().len(), tx.witnesses().into_iter().map(|w|w.raw_data().len()).sum::<usize>(),
            evidence.occupied_output_capacity_shannons, evidence.exact_role_selections["migration"][0].aggregate_bytes, compiled[0].artifact_bytes.len());
        if case.count != 8 || case.reverse {
            continue;
        }
        std::fs::write(root.join("checked.json"), serde_json::to_vec(&report).unwrap()).unwrap();
        if shape == Shape::Records {
            assert_eq!(evidence.exact_role_selections["migration"][0].aggregate_bytes, 32768);
            let mut wrong_note_identity = input.clone();
            change_record_revision(&mut wrong_note_identity.transaction.outputs[8], 7, 7);
            assert_business_rejection(&wrong_note_identity, root, &compiled);
            let mut duplicate_history = input.clone();
            change_record_revision(&mut duplicate_history.transaction.inputs[8], 1, 1);
            change_record_revision(&mut duplicate_history.transaction.outputs[8], 1, 1);
            assert_business_rejection(&duplicate_history, root, &compiled);
        }
        for peer in [1, 2] {
            let error = context(input, &compiled, Some(peer)).verify_tx(&vm_tx, 10_000_000).unwrap_err();
            assert!(format!("{error:?}").contains("ValidationFailure"), "peer {peer}: {error:?}");
        }
        let mut malformed = input.clone();
        let mut bytes = hex::decode(&malformed.transaction.outputs[8].data.as_ref().unwrap()[2..]).unwrap();
        bytes[4] ^= 1;
        malformed.transaction.outputs[8].data = Some(format!("0x{}", hex::encode(bytes)));
        assert!(check_protocol_bundle(&malformed, root).unwrap_err().to_string().contains("nested data"));
        let mut rebound = serde_json::to_value(&report).unwrap();
        rebound["bundle"]["transaction"]["outputs"][8]["data"] = json!(malformed.transaction.outputs[8].data);
        rebind(&mut rebound);
        let error = cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rebound).unwrap()).unwrap_err();
        assert!(error.to_string().contains("exact-role"), "{error:#}");
        let mut rebound = serde_json::to_value(&report).unwrap();
        rebound["bundle"]["transaction"]["outputs"][8]["capacity"] = json!(1);
        rebind(&mut rebound);
        assert!(cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rebound).unwrap()).is_err());
        let mut bad = input.clone();
        let mut bytes = hex::decode(&bad.transaction.outputs[8].data.as_ref().unwrap()[2..]).unwrap();
        let id_offset = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        bytes[id_offset - 1] ^= 1; // Last byte of the appended note; same canonical shape.
        bad.transaction.outputs[8].data = Some(format!("0x{}", hex::encode(bytes)));
        let bad_report = check_protocol_bundle(&bad, root).unwrap();
        assert_eq!(bad_report.status, "ok");
        let (bad_tx, _) =
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&bad_report).unwrap()).unwrap();
        let bad_vm = packed::Transaction::from_slice(bad_tx.data().as_slice()).unwrap().into_view();
        let error = fixture.verify_tx(&bad_vm, 10_000_000).unwrap_err();
        assert!(format!("{error:?}").contains("error code 5"), "late note: {error:?}");
        if opt_level == 3 && !explicit {
            let measure = if cross_package { super::resources::measure_package } else { super::resources::measure };
            measure(&format!("{shape:?}-8-false-late-note"), &fixture, &bad, &compiled, &bad_vm, &[5, 0, 0]);
            if !cross_package {
                super::resources::scan_limits(&format!("{shape:?}"), 8, input, &compiled, root, &bad, &[5, 0, 0]);
            }
        }
        let live_inputs =
            vm_tx.inputs().into_iter().map(|cell| fixture.get_cell(&cell.previous_output()).unwrap()).collect::<Vec<_>>();
        let resolve = |cells: &[_]| {
            cellscript_ckb_adapter::protocol_bundle_live_resolution_evidence(
                &tx,
                &evidence,
                &input.network.chain_id,
                &input.network.genesis_hash,
                cells,
            )
        };
        resolve(&live_inputs).unwrap();
        let mut changed = live_inputs;
        changed[8].1 = Bytes::from_static(b"substituted history");
        assert!(resolve(&changed).is_err());
    }
    let script = root.join("binding-check.mjs");
    std::fs::write(&script, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const report = JSON.parse(fs.readFileSync(new URL('./checked.json', import.meta.url)));
for (const a of report.bundle.artifacts) {
  const sdk = await import(`./${a.id}-sdk/src/index.ts`);
  const d = a.deployment;
  const binding = sdk.bindProtocolBundleArtifact({id:a.id, packageCoordinate:a.package_coordinate,
    lockNodeId:a.lock_node_id, entry:a.entry, scriptRole:a.script_role,
    deployment:{network:{chainId:d.network.chain_id, genesisHash:d.network.genesis_hash}, artifactHash:d.artifact_hash,
      script:{codeHash:d.script.code_hash, hashType:d.script.hash_type, args:d.script.args},
      codeCellDep:{outPoint:{txHash:d.code_cell_dep.out_point.tx_hash, index:d.code_cell_dep.out_point.index}, depType:d.code_cell_dep.dep_type}}});
  assert.equal(sdk.exactScriptHandleFromCheckedBundle(report, binding).artifact.id, a.id);
  if (a.id !== 'migration') continue;
  assert.deepEqual(binding.exactRoleContracts, a.exact_role_contracts);
  assert.equal(binding.exactRoleContracts.sites[0].contract.roles.length, 2);
  assert.equal(sdk.nestedCellContracts.Document.schema_hash, a.schema_contracts.find(s => s.type_name === 'Document').schema_hash);
  const changed = structuredClone(report);
  const field = changed.bundle.artifacts.find(a => a.id === 'migration').exact_role_contracts.types.find(t => t.name === 'Document').fields.find(f => f.name === 'messages');
  const original = field.ty;
  field.ty = field.ty.replace('256', '255');
  assert.notEqual(field.ty, original);
  assert.throws(() => sdk.exactScriptHandleFromCheckedBundle(changed, binding), /exact-role catalog/);
}
"#).unwrap();
    let checked = Command::new("node").arg("--experimental-strip-types").arg(script).output().unwrap();
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
    transactions
}

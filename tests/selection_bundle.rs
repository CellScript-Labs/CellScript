//! Final-transaction role reconstruction. These are offline builder/serializer
//! checks plus complete-program all-group VM fixtures; stable source admission
//! remains a separate obligation.
#![cfg(feature = "cli")]
use cellscript::nested_builder::{encode_nested_cell_data, NestedCellFieldInput as Field, NestedElementInput as Item};
use cellscript::protocol_bundle::{check_protocol_bundle, ProtocolBundleInput, ProtocolScriptIdentity};
use cellscript::{compile, CompileOptions, CompileResult, NEXT_EDITION};
use cellscript_artifact_checker::cell_selection::transaction::{project, Error};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, process::Command};

#[path = "selection_bundle/collections.rs"]
mod collections;

#[cfg(feature = "lsp")]
#[path = "selection_bundle/editor.rs"]
mod editor;

#[allow(dead_code)]
#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;

// Shared scheduler diagnostics also expose APIs for other resource suites.
#[allow(dead_code)]
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[allow(dead_code)]
#[path = "support/cost_trace.rs"]
mod cost_trace;
#[path = "selection_bundle/migration.rs"]
mod migration;
#[path = "selection_bundle/resources.rs"]
mod resources;
#[path = "selection_bundle/settlement.rs"]
mod settlement;
#[path = "selection_bundle/support.rs"]
mod support;

fn compile_source(source: &str) -> CompileResult {
    compile(source, CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() }).unwrap()
}

fn save(root: &Path, name: &str, result: &CompileResult) {
    std::fs::write(root.join(format!("{name}.elf")), &result.artifact_bytes).unwrap();
    for (suffix, bytes) in [
        ("meta", serde_json::to_vec(&result.metadata).unwrap()),
        ("lowering", cellscript_artifact_checker::canonical_bytes(result.verified_lowering_record.as_ref().unwrap()).unwrap()),
        ("sourcemap", cellscript_artifact_checker::canonical_bytes(result.source_artifact_map.as_ref().unwrap()).unwrap()),
    ] {
        std::fs::write(root.join(format!("{name}.elf.{suffix}.json")), bytes).unwrap();
    }
}

fn script(result: &CompileResult) -> Value {
    json!({"code_hash": format!("0x{}", hex::encode(result.artifact_hash)), "hash_type": "data2", "args": "0x"})
}

fn script_hash(script: &Value) -> String {
    cellscript::script_handle::ckb_script_identity_hash(&serde_json::from_value::<ProtocolScriptIdentity>(script.clone()).unwrap())
        .unwrap()
}

fn source(type_hash: &str, lock_hash: &str) -> String {
    format!(
        r#"
module selection_bundle
data_shape Messages {{ element: Vec<u8>, limits {{ items: 8, payload_bytes: 256, encoded_bytes: 2120 }} }}
resource Document has store {{ messages: Messages, id: u64 }}
cell_roles Settlement {{
    limits {{ input_scan: 32, output_scan: 32, selected_per_side: 8, aggregate_bytes: 32768 }}
    documents: Document {{
        type_hash: "{type_hash}", lock_hash: "{lock_hash}", identity: id,
        authority: observe, correspondence: preserve, inputs: 1..=8, outputs: 1..=8
    }}
}}
action inspect() -> u64 {{
    let selected = ckb::select_cell_roles<Settlement>()
    require selected.documents.input_count() == selected.documents.output_count()
    return 0
}}
"#
    )
}

fn rebind(report: &mut Value) {
    report["bundle_hash"] =
        json!(cellscript_artifact_checker::canonical_hash(report["bundle"]["schema"].as_str().unwrap(), &report["bundle"]).unwrap());
}

#[test]
fn nested_role_catalog_binds_native_builder_and_materialized_transaction() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let peer = compile_source("module peer\naction verify() -> u64 { return 0 }");
    let auth = compile_source("module auth\nlock authorize() -> bool { return true }");
    let selected = compile_source(&source(&script_hash(&script(&peer)), &script_hash(&script(&auth))));
    let network = json!({"chain_id": "ckb-testnet", "genesis_hash": format!("0x{}", "00".repeat(32))});
    let mut artifacts = vec![];
    for (id, entry, kind, result, dep) in [
        ("selected", "inspect", "action", &selected, 1u8),
        ("peer", "verify", "action", &peer, 2),
        ("auth", "authorize", "lock", &auth, 3),
    ] {
        save(root, id, result);
        artifacts.push(json!({
            "id": id, "package_coordinate": format!("example/{id}@1.0.0"), "lock_node_id": format!("{id}-node"),
            "entry": {"kind": kind, "name": entry}, "script_role": if kind == "lock" {"lock"} else {"type"},
            "files": {"artifact": format!("{id}.elf"), "metadata": format!("{id}.elf.meta.json"),
                "lowering_record": format!("{id}.elf.lowering.json"), "source_map": format!("{id}.elf.sourcemap.json")},
            "deployment": {"network": network, "artifact_hash": hex::encode(result.artifact_hash), "script": script(result),
                "code_cell_dep": {"out_point": {"tx_hash": format!("0x{}", hex::encode([dep; 32])), "index": 0}, "dep_type": "code"}}
        }));
    }
    let generated = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--metadata"])
        .arg(root.join("selected.elf.meta.json"))
        .args(["--target", "typescript", "--action", "inspect", "--output"])
        .arg(root.join("sdk"))
        .output()
        .unwrap();
    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
    artifacts[0]["files"]["builder_manifest"] = json!("sdk/cellscript-builder-manifest.json");
    let generated_peer = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["gen-builder", "--metadata"])
        .arg(root.join("peer.elf.meta.json"))
        .args(["--target", "typescript", "--action", "verify", "--output"])
        .arg(root.join("peer-sdk"))
        .output()
        .unwrap();
    assert!(generated_peer.status.success(), "{}", String::from_utf8_lossy(&generated_peer.stderr));
    artifacts[1]["files"]["builder_manifest"] = json!("peer-sdk/cellscript-builder-manifest.json");
    let manifest_path = root.join("sdk/cellscript-builder-manifest.json");
    let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    let catalog = project(&selected.metadata.typed_semantics, "action", "inspect").unwrap().unwrap();
    assert_eq!(manifest["exact_role_contracts"]["action:inspect"], serde_json::to_value(&catalog).unwrap());
    assert_eq!(manifest["protocol_bundle_contract"]["schema"], "cellscript-protocol-bundle-v2");
    let encode = |id: u64, payload: &[u8]| {
        encode_nested_cell_data(
            &selected.metadata,
            "Document",
            &BTreeMap::from([
                ("messages".into(), Field::Nested(&[Item { fixed_fields: &[], payload }])),
                ("id".into(), Field::FixedBytes(&id.to_le_bytes())),
            ]),
        )
        .unwrap()
    };
    let cell = |id: u8, result: &CompileResult, data: Vec<u8>| {
        json!({
            "cell_commitment": format!("0x{}", hex::encode([id;32])), "capacity": 100_000_000_000u64,
            "lock": script(&auth), "type": script(result), "data": format!("0x{}", hex::encode(data))
        })
    };
    let mut inputs = vec![cell(10, &selected, vec![]), cell(11, &peer, encode(7, &[1, 2, 3])), cell(12, &peer, encode(8, &[4]))];
    for (index, input) in inputs.iter_mut().enumerate() {
        input["out_point"] = json!({"tx_hash": format!("0x{}", "ab".repeat(32)), "index": index});
        input["since"] = json!(0);
    }
    let input: ProtocolBundleInput = serde_json::from_value(json!({
        "schema": "cellscript-protocol-bundle-input-v1", "network": network, "artifacts": artifacts,
        "roles": [
            {"artifact": "auth", "name": "authorization", "location": "input", "index": 0, "ownership": "shared-read", "expected_lock": script(&auth)},
            {"artifact": "selected", "name": "selected-input", "location": "input", "index": 0, "ownership": "shared-read", "expected_type": script(&selected)},
            {"artifact": "peer", "name": "document-input", "location": "input", "index": 1, "ownership": "shared-read", "expected_type": script(&peer)}
        ],
        "transaction": {"version": 0, "inputs": inputs,
            "outputs": [cell(13, &peer, encode(8, &[9])), cell(14, &selected, vec![]), cell(15, &peer, encode(7, &[8,7]))],
            "witnesses": [{},{},{}], "cell_deps": artifacts.iter().map(|a| a["deployment"]["code_cell_dep"].clone()).collect::<Vec<_>>(),
            "header_deps": [], "fee_policy_hash": format!("0x{}", "de".repeat(32)), "change_policy_hash": format!("0x{}", "ef".repeat(32))}
    })).unwrap();
    let report = check_protocol_bundle(&input, root).unwrap();
    assert_eq!(report.status, "ok", "{report:?}");
    assert_eq!(report.schema, "cellscript-protocol-bundle-report-v2");
    let report = serde_json::to_value(report).unwrap();
    let (tx, materialized) =
        cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(tx.outputs().len(), 3);
    assert_eq!(serde_json::to_value(&materialized.exact_role_selections).unwrap(), report["evidence"]["exact_role_selections"]);
    assert_eq!(materialized.exact_role_selections["selected"][0].pairs[0].outputs, [2]);
    assert_eq!(materialized.ckb_vm_execution, "not-executed");
    std::fs::write(root.join("checked.json"), serde_json::to_vec(&report).unwrap()).unwrap();
    let js = root.join("check.mjs");
    std::fs::write(&js, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as builder from './sdk/src/index.ts';
const report = JSON.parse(fs.readFileSync(new URL('./checked.json', import.meta.url)));
const artifact = report.bundle.artifacts.find(a => a.id === 'selected');
const d = artifact.deployment;
const binding = builder.bindProtocolBundleArtifact({
  id: artifact.id, packageCoordinate: artifact.package_coordinate, lockNodeId: artifact.lock_node_id,
  entry: artifact.entry, scriptRole: artifact.script_role,
  deployment: { network: { chainId: d.network.chain_id, genesisHash: d.network.genesis_hash },
    artifactHash: d.artifact_hash, script: { codeHash: d.script.code_hash, hashType: d.script.hash_type, args: d.script.args },
    codeCellDep: { outPoint: { txHash: d.code_cell_dep.out_point.tx_hash, index: d.code_cell_dep.out_point.index }, depType: d.code_cell_dep.dep_type } }
});
assert.deepEqual(binding.exactRoleContracts, artifact.exact_role_contracts);
assert.equal(builder.exactScriptHandleFromCheckedBundle(report, binding).artifact.id, 'selected');
for (const mutation of ['downgrade', 'remove', 'bounds', 'evidence']) {
  const bad = structuredClone(report);
  const a = bad.bundle.artifacts.find(a => a.id === 'selected');
  if (mutation === 'downgrade') { bad.schema = builder.PROTOCOL_BUNDLE_REPORT_SCHEMA; bad.bundle.schema = builder.PROTOCOL_BUNDLE_SCHEMA; delete a.exact_role_contracts; delete bad.evidence.exact_role_selections; }
  if (mutation === 'remove') delete a.exact_role_contracts;
  if (mutation === 'bounds') a.exact_role_contracts.sites[0].contract.aggregate_bytes--;
  if (mutation === 'evidence') delete bad.evidence.exact_role_selections.selected;
  assert.throws(() => builder.exactScriptHandleFromCheckedBundle(bad, binding), /exact-role catalog/);
}
"#).unwrap();
    let checked = Command::new("node").arg("--experimental-strip-types").arg(js).output().unwrap();
    assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
    for mutation in 0..8 {
        let mut hostile = report.clone();
        match mutation {
            0 => hostile["bundle"]["transaction"]["outputs"][0]["data"] = json!(format!("0x{}", hex::encode(encode(99, &[9])))),
            1 => hostile["bundle"]["transaction"]["outputs"][0]["lock"]["args"] = json!("0x01"),
            2 => {
                hostile["bundle"]["transaction"]["inputs"][1].as_object_mut().unwrap().remove("data");
            }
            3 => hostile["evidence"]["exact_role_selections"]["selected"][0]["pairs"][0]["outputs"][0] = json!(0),
            4 => hostile["bundle"]["transaction"]["outputs"][0]["data"] = json!("0x00000000"),
            5 => {
                hostile["schema"] = json!("cellscript-protocol-bundle-report-v1");
                hostile["bundle"]["schema"] = json!("cellscript-protocol-bundle-v1");
            }
            6 => {
                hostile["bundle"]["artifacts"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|a| a["id"] == "selected")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("exact_role_contracts");
            }
            7 => {
                hostile["evidence"].as_object_mut().unwrap().remove("exact_role_selections");
            }
            _ => unreachable!(),
        }
        rebind(&mut hostile);
        assert!(
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&hostile).unwrap()).is_err(),
            "mutation {mutation}"
        );
    }
    let mut hostile_input = input.clone();
    hostile_input.transaction.outputs[0].data = Some(format!("0x{}", hex::encode(encode(99, &[9]))));
    let error = check_protocol_bundle(&hostile_input, root).unwrap_err().to_string();
    assert!(error.contains("correspondence"), "{error}");
    let mut hostile_manifest = manifest.clone();
    hostile_manifest["exact_role_contracts"]["action:inspect"]["sites"][0]["contract"]["aggregate_bytes"] = json!(1);
    std::fs::write(manifest_path, serde_json::to_vec(&hostile_manifest).unwrap()).unwrap();
    let error = check_protocol_bundle(&input, root).unwrap_err().to_string();
    assert!(error.contains("builder exact-role contracts"), "{error}");
}

#[test]
fn projection_checks_selected_entry_and_rejects_unbound_conditional_paths() {
    let source = source(&format!("0x{}", "11".repeat(32)), &format!("0x{}", "22".repeat(32)));
    let compiled = compile_source(&source);
    let typed = &compiled.metadata.typed_semantics;
    assert_eq!(project(typed, "action", "inspect").unwrap().unwrap().sites.len(), 1);
    let conditional = source
        .replace("action inspect() -> u64 {", "action inspect(witness choose: bool) -> u64 { if choose {")
        .replace("    return 0", "    }\n    return 0");
    let compiled = compile_source(&conditional);
    assert_eq!(project(&compiled.metadata.typed_semantics, "action", "inspect"), Err(Error::Conditional));
    let source = format!("{source}\naction unused() -> u64 {{ return 0 }}");
    let compiled = compile_source(&source);
    assert!(project(&compiled.metadata.typed_semantics, "action", "unused").unwrap().is_none());
}

#[test]
fn selection_projection_preserves_entry_scope_and_ignores_unreachable_sites() {
    let source = source(&format!("0x{}", "11".repeat(32)), &format!("0x{}", "22".repeat(32)));
    let helper = source
        .replace("action inspect()", "fn inspect_roles()")
        .replace(
            "require selected.documents.input_count() == selected.documents.output_count()",
            "let count = selected.documents.input_count()",
        )
        .replace("return 0", "return count");
    let error = compile(
        &format!("{helper}\naction inspect() -> u64 {{ return inspect_roles() }}"),
        CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("requires an action or lock entry"), "{error}");
    let with_helper = source.replace("return 0", "return identity(selected.documents.input_count())");
    let compiled = compile_source(&format!("{with_helper}\nfn identity(count: u64) -> u64 {{ return count }}"));
    let catalog = project(&compiled.metadata.typed_semantics, "action", "inspect").unwrap().unwrap();
    assert_eq!(catalog.sites.len(), 1);
    assert!(catalog.sites[0].entry.contains("inspect"));
    // Projection must not promote an unreachable CFG block to an obligation.
    // This isolates reachability; machine/source validation is tested separately.
    let mut record = compile_source(&source).metadata.typed_semantics;
    let entry = record.entries.iter_mut().find(|entry| entry.name == "inspect").unwrap();
    let operation = entry
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .find(|op| matches!(op.detail, cellscript_artifact_checker::TypedSemanticOperationDetail::ExactRoleSelection { .. }))
        .unwrap()
        .clone();
    entry.blocks.push(cellscript_artifact_checker::TypedSemanticBlock {
        id: 999,
        operations: vec![operation],
        successors: vec![],
        terminator: "return".into(),
        runtime_error: None,
    });
    assert_eq!(project(&record, "action", "inspect").unwrap().unwrap().sites.len(), 1);
}

#[test]
fn nested_reconstruction_checks_maximum_aggregate_and_unused_late_payloads() {
    use cellscript_artifact_checker::cell_selection::transaction::{Cell, CurrentScript};
    let compiled = compile_source(&source(&format!("0x{}", "11".repeat(32)), &format!("0x{}", "22".repeat(32))));
    let mut catalog = project(&compiled.metadata.typed_semantics, "action", "inspect").unwrap().unwrap();
    let encode = |id: u64, count| {
        let items = (0..count).map(|_| Item { fixed_fields: &[], payload: &[7; 256] }).collect::<Vec<_>>();
        encode_nested_cell_data(
            &compiled.metadata,
            "Document",
            &BTreeMap::from([("messages".into(), Field::Nested(&items)), ("id".into(), Field::FixedBytes(&id.to_le_bytes()))]),
        )
        .unwrap()
    };
    let inputs = (0..8).map(|id| encode(id, 7)).collect::<Vec<_>>();
    let mut outputs = (0..8).rev().map(|id| encode(id, 8)).collect::<Vec<_>>();
    fn views(bytes: &[Vec<u8>]) -> Vec<Cell<'_>> {
        bytes.iter().map(|data| Cell { type_hash: Some([0x11; 32]), lock_hash: [0x22; 32], data }).collect()
    }
    let current = CurrentScript { hash: [0x33; 32], is_type: true };
    let actual = catalog.verify(current, &views(&inputs), &views(&outputs)).unwrap();
    let bytes = actual[0].aggregate_bytes;
    assert_eq!(actual[0].cells.len(), 16);
    assert_eq!(actual[0].pairs[0].outputs, [7]);
    catalog.sites[0].contract.aggregate_bytes = bytes;
    assert!(catalog.verify(current, &views(&inputs), &views(&outputs)).is_ok());
    catalog.sites[0].contract.aggregate_bytes -= 1;
    assert_eq!(catalog.verify(current, &views(&inputs), &views(&outputs)), Err(Error::ByteBound));
    catalog.sites[0].contract.aggregate_bytes = 32768;
    // The action reads only role counts. Corrupt the last nested element's
    // length while preserving complete table size and all fixed identities.
    let last = outputs.last_mut().unwrap();
    let vector = u32::from_le_bytes(last[4..8].try_into().unwrap()) as usize;
    let offset = u32::from_le_bytes(last[vector + 32..vector + 36].try_into().unwrap()) as usize;
    last[vector + offset..vector + offset + 4].copy_from_slice(&257u32.to_le_bytes());
    assert_eq!(catalog.verify(current, &views(&inputs), &views(&outputs)), Err(Error::Codec));
}

#[test]
fn selection_projection_limits_do_not_restrict_existing_non_selection_modules() {
    let mut source = String::from("module large_schema\naction inspect() -> u64 { return 0 }\n");
    for index in 0..513 {
        source.push_str(&format!("resource Record{index} has store {{ id: u64 }}\n"));
    }
    let metadata = cellscript::compile_metadata(&source, NEXT_EDITION, None).unwrap();
    assert_eq!(metadata.typed_semantics.types.len(), 513);
    assert!(project(&metadata.typed_semantics, "action", "inspect").unwrap().is_none());
}

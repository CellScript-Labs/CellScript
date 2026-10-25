//! Full-transaction aggregation and duplicate detection over actual Cell data.
//! Allocation/source machine admission remains an independent obligation.
use super::support::{bind_deployment_lines, compile_at, context, identity, participants, reference};
use super::*;
use ckb_testtool::ckb_types::{packed, prelude::*};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct Note {
    asset: [u8; 32],
    serial: u64,
    amount: u64,
}

impl Note {
    fn bytes(&self) -> Vec<u8> {
        [self.asset.to_vec(), self.serial.to_le_bytes().to_vec(), self.amount.to_le_bytes().to_vec()].concat()
    }

    fn sdk_vector(&self, metadata: &cellscript::CompileMetadata) -> Value {
        let fields = BTreeMap::from([
            ("asset".to_string(), self.asset.to_vec()),
            ("serial".to_string(), self.serial.to_le_bytes().to_vec()),
            ("amount".to_string(), self.amount.to_le_bytes().to_vec()),
        ]);
        let borrowed = fields.iter().map(|(name, bytes)| (name.clone(), bytes.as_slice())).collect();
        let native = cellscript::fixed_cell_builder::encode_fixed_cell_data(metadata, "Note", &borrowed).unwrap();
        assert_eq!(native, self.bytes());
        json!({"fields":fields.into_iter().map(|(name, bytes)|(name,hex::encode(bytes))).collect::<BTreeMap<_,_>>(),
            "wire":hex::encode(native)})
    }
}

fn sdk_data(root: &Path, metadata: &cellscript::CompileMetadata, before: &[Note], after: &[Note]) -> Value {
    let vectors = json!({"before":before.iter().map(|note|note.sdk_vector(metadata)).collect::<Vec<_>>(),
        "after":after.iter().map(|note|note.sdk_vector(metadata)).collect::<Vec<_>>(),
        "action_features":metadata.actions[0].fail_closed_runtime_features,
        "allocated":metadata.runtime.fail_closed_runtime_features.iter().any(|feature|feature == "allocated-values-artifact-proof")});
    std::fs::write(root.join("fixed-vectors.json"), serde_json::to_vec(&vectors).unwrap()).unwrap();
    let script = root.join("fixed-check.mjs");
    std::fs::write(&script, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as sdk from './transfer-sdk/src/index.ts';
const vectors = JSON.parse(fs.readFileSync(new URL('./fixed-vectors.json', import.meta.url)));
const encode = vector => {
  const fields = Object.fromEntries(Object.entries(vector.fields).reverse().map(([name, hex]) => [name, new Uint8Array(Buffer.from(hex, 'hex'))]));
  const wire = Buffer.from(sdk.encodeFixedCellData('Note', fields)).toString('hex');
  assert.equal(wire, vector.wire);
  assert.throws(() => sdk.encodeFixedCellData('Note', {...fields, amount: fields.amount.subarray(1)}));
  assert.throws(() => sdk.encodeFixedCellData('Note', {...fields, unknown: new Uint8Array()}));
  return wire;
};
const plan = sdk.planTransfer({});
assert.equal(plan.canSubmit, false);
assert.deepEqual(plan.failClosedRuntimeFeatures, vectors.action_features);
if (vectors.allocated) assert.ok(plan.failClosedRuntimeFeatures.includes('cell-data-view-artifact-proof'));
assert.equal(plan.failClosedRuntimeFeatures.includes('allocated-values-artifact-proof'), vectors.allocated);
fs.writeFileSync(new URL('./fixed-generated.json', import.meta.url), JSON.stringify({before:vectors.before.map(encode), after:vectors.after.map(encode)}));
"#).unwrap();
    let output = Command::new("node").arg("--experimental-strip-types").arg(script).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&std::fs::read(root.join("fixed-generated.json")).unwrap()).unwrap()
}

fn model(before: &[Note], after: &[Note]) -> bool {
    let summarize = |notes: &[Note]| {
        if !(1..=32).contains(&notes.len()) {
            return None;
        }
        let mut serials = BTreeSet::new();
        let mut amounts = BTreeMap::<[u8; 32], u128>::new();
        for note in notes {
            if note.amount == 0 || !serials.insert(note.serial) {
                return None;
            }
            *amounts.entry(note.asset).or_default() += u128::from(note.amount);
        }
        Some(amounts)
    };
    match (summarize(before), summarize(after)) {
        (Some(before), Some(after)) => before == after,
        _ => false,
    }
}

fn input(compiled: &[CompileResult], artifacts: &[Value], before: &[Note], after: &[Note]) -> ProtocolBundleInput {
    let cell = |note: Option<&Note>| {
        let bytes = note.map_or_else(Vec::new, Note::bytes);
        json!({"cell_commitment":format!("0x{}",hex::encode(cellscript::ckb_blake2b256(&bytes))),
            "capacity":400_000_000_000u64,"lock":identity(2),"type":note.map(|_|identity(1)),
            "data":format!("0x{}",hex::encode(bytes))})
    };
    // Unrelated fee/change Cells make the first Type-group input nonzero.
    let mut inputs = vec![cell(None)];
    let mut outputs = vec![cell(None)];
    inputs.extend(before.iter().map(|note| cell(Some(note))));
    outputs.extend(after.iter().map(|note| cell(Some(note))));
    // Splitting notes may add outputs. Supply capacity through the untyped fee
    // input so that token conservation, rather than CKB funding, is tested.
    inputs[0]["capacity"] = json!(400_000_000_000u64 * (1 + after.len().saturating_sub(before.len()) as u64));
    for (index, cell) in inputs.iter_mut().enumerate() {
        cell["out_point"] = json!({"tx_hash":format!("0x{}","90".repeat(32)),"index":index});
        cell["since"] = json!(0);
    }
    let payload = compiled[0].metadata.actions[0].entry_witness_args(&[]).unwrap();
    let commitment = format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&payload)));
    let mut witnesses = vec![json!({}); inputs.len()];
    witnesses[1] = json!({"input_type":commitment,"input_type_bytes":format!("0x{}",hex::encode(payload))});
    let mut result: ProtocolBundleInput = serde_json::from_value(json!({
        "schema":"cellscript-protocol-bundle-input-v1","network":artifacts[0]["deployment"]["network"],
        "artifacts":artifacts,
        "roles":[{"artifact":"transfer","name":"group-root","location":"input","index":1,
            "ownership":"shared-read","expected_type":identity(1)},
            {"artifact":"auth","name":"group-root","location":"input","index":0,
            "ownership":"shared-read","expected_lock":identity(2)}],
        "witnesses":[{"artifact":"transfer","name":"entry","index":1,"field":"input_type",
            "ownership":"exclusive-write","abi":"cellscript-entry-witness-v1","value_commitment":commitment}],
        "transaction":{"version":0,"inputs":inputs,"outputs":outputs,"witnesses":witnesses,
            "cell_deps":artifacts.iter().map(|a|a["deployment"]["code_cell_dep"].clone()).collect::<Vec<_>>(),
            "header_deps":[],"fee_policy_hash":format!("0x{}","de".repeat(32)),
            "change_policy_hash":format!("0x{}","ef".repeat(32))}
    }))
    .unwrap();
    bind_deployment_lines(&mut result, compiled, 2);
    result
}

fn verify(
    root: &Path,
    compiled: &[CompileResult],
    artifacts: &[Value],
    before: &[Note],
    after: &[Note],
    resource: Option<&str>,
) -> (Vec<u8>, Option<Value>) {
    let mut candidate = input(compiled, artifacts, before, after);
    let generated = sdk_data(root, &compiled[0].metadata, before, after);
    for (cells, key) in [(&mut candidate.transaction.inputs, "before"), (&mut candidate.transaction.outputs, "after")] {
        let wires = generated[key].as_array().unwrap();
        assert_eq!(cells.len(), wires.len() + 1);
        for (cell, wire) in cells.iter_mut().skip(1).zip(wires) {
            let wire = wire.as_str().unwrap();
            assert_eq!(cell.data.as_deref().unwrap(), format!("0x{wire}"));
            // Use the SDK result in the actual candidate, resolver and VM.
            cell.data = Some(format!("0x{wire}"));
        }
    }
    let report = check_protocol_bundle(&candidate, root).unwrap();
    let (tx, materialized) =
        cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(tx.data().as_slice(), reference(&candidate));
    assert_eq!(materialized.script_groups.len(), 2);
    let tx = packed::Transaction::from_slice(tx.data().as_slice()).unwrap().into_view();
    let fixture = context(&candidate, compiled, None);
    let result = fixture.verify_tx(&tx, 20_000_000);
    if model(before, after) {
        assert!(result.is_ok(), "{before:?} -> {after:?}: {result:?}");
        // The complete transaction requires its spending Lock even though the
        // Type's conservation check has already succeeded on identical bytes.
        let error = context(&candidate, compiled, Some(1)).verify_tx(&tx, 20_000_000).unwrap_err();
        assert!(format!("{error:?}").contains("ValidationFailure"), "{error:?}");
    } else {
        let error = result.unwrap_err();
        assert!(format!("{error:?}").contains("error code 5"), "{before:?} -> {after:?}: {error:?}");
    }
    let observed = resource.map(|key| {
        super::resources::observe(key, &fixture, &candidate, compiled, &tx, &[if model(before, after) { 0 } else { 5 }, 0])
    });
    (tx.data().as_slice().to_vec(), observed)
}

struct Case {
    name: String,
    before: Vec<Note>,
    after: Vec<Note>,
    resource: bool,
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for count in [1, 8, 32] {
        for assets in [1, 4, 32] {
            let before = (0..count)
                .map(|i| Note { asset: [(i % assets + 1) as u8; 32], serial: i as u64, amount: u64::MAX })
                .collect::<Vec<_>>();
            let mut after = before.clone();
            after.reverse();
            cases.push(Case {
                name: format!("{count}-{assets}-valid"),
                before: before.clone(),
                after: after.clone(),
                resource: assets == 1 || (count != 1 && assets == 32),
            });
            after.last_mut().unwrap().amount -= 1;
            cases.push(Case { name: format!("{count}-{assets}-amount"), before: before.clone(), after, resource: false });
            let mut after = before.clone();
            after.last_mut().unwrap().asset = [99; 32];
            cases.push(Case { name: format!("{count}-{assets}-asset"), before: before.clone(), after, resource: false });
            if count > 1 {
                let mut duplicate = before.clone();
                duplicate.last_mut().unwrap().serial = before[0].serial;
                cases.push(Case {
                    name: format!("{count}-{assets}-duplicate-output"),
                    before: before.clone(),
                    after: duplicate.clone(),
                    resource: count == 32 && assets == 32,
                });
                cases.push(Case {
                    name: format!("{count}-{assets}-duplicate-input"),
                    before: duplicate,
                    after: before.clone(),
                    resource: count == 32 && assets == 32,
                });
            }
            if count == 32 && assets == 32 {
                let mut after = before.clone();
                after.reverse();
                after[0].amount -= 1; // Last sorted key, last input-key comparison.
                cases.push(Case { name: "32-last-key-amount".into(), before, after, resource: true });
            }
        }
    }
    let before = vec![Note { asset: [7; 32], serial: 1, amount: 30 }];
    let after = vec![Note { asset: [7; 32], serial: 2, amount: 11 }, Note { asset: [7; 32], serial: 3, amount: 19 }];
    cases.push(Case { name: "split".into(), before: before.clone(), after: after.clone(), resource: true });
    cases.push(Case { name: "merge".into(), before: after, after: before.clone(), resource: true });
    let mut zero = before;
    zero[0].amount = 0;
    cases.push(Case { name: "zero".into(), before: zero.clone(), after: zero, resource: true });
    let over = (0..33).map(|i| Note { asset: [7; 32], serial: i, amount: 1 }).collect::<Vec<_>>();
    cases.push(Case { name: "over33".into(), before: over.clone(), after: over, resource: true });
    cases
}

#[test]
fn allocated_multi_asset_transfer_matches_independent_model_and_all_group_execution() {
    let programs = [
        ("arena", include_str!("../../examples/language/batches/multi_asset_transfer.cell")),
        ("stack", include_str!("../fixtures/multi_asset_transfer_stack.cell")),
    ];
    let cases = cases();
    let mut observations = BTreeMap::new();
    for opt in [0, 3] {
        let mut canonical = BTreeMap::new();
        for (name, source) in programs {
            let ast = cellscript::frontend::parse(source, NEXT_EDITION).unwrap();
            let formatted = cellscript::fmt::format_default(&ast).unwrap();
            let formatted_ast = cellscript::frontend::parse(&formatted, NEXT_EDITION).unwrap();
            assert_eq!(cellscript::fmt::format_default(&formatted_ast).unwrap(), formatted);
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            let compiled = vec![compile_at(source, opt), compile_at("module auth\nlock authorize() -> bool { return true }", opt)];
            assert_eq!(
                compiled[0].metadata.runtime.fail_closed_runtime_features.iter().any(|s| s == "allocated-values-artifact-proof"),
                name == "arena"
            );
            assert!(compiled[0].metadata.runtime.fail_closed_runtime_features.iter().any(|s| s == "cell-data-view-artifact-proof"));
            let artifacts = participants(root, &[("transfer", "action", "transfer"), ("auth", "lock", "authorize")], &compiled);
            for case in &cases {
                let key = format!("{name}-{}", case.name);
                let (bytes, observed) = verify(
                    root,
                    &compiled,
                    &artifacts,
                    &case.before,
                    &case.after,
                    (opt == 3 && case.resource).then_some(key.as_str()),
                );
                if name == "arena" {
                    canonical.insert(&case.name, bytes);
                } else {
                    assert_eq!(&bytes, &canonical[&case.name], "O{opt}/{}: program variants changed transaction bytes", case.name);
                }
                if let Some(observed) = observed {
                    observations.insert(key, observed);
                }
            }
        }
    }
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/collection_transfer_resources.json")).unwrap();
    assert_eq!(fixture["schema"], "cellscript-collection-transfer-resources-v1");
    for (key, row) in &observations {
        let profile = key.split('-').next().unwrap();
        super::resources::check_ceiling(key, row, &fixture["ceilings"][profile]);
    }
    assert_eq!(serde_json::to_value(observations).unwrap(), fixture["observations"], "resource identity drift requires review");
}

#[test]
fn allocated_transfer_preallocation_policy_preserves_business_outcomes_and_measures_cost() {
    use sha2::{Digest, Sha256};
    let actual_counts = include_str!("../../examples/language/batches/multi_asset_transfer.cell");
    // This is a lowering-policy experiment, not new declaration syntax. Keep
    // the complete program and all its rejects; only the four capacities vary.
    assert_eq!(actual_counts.matches(".reserve(inputs)").count(), 2);
    assert_eq!(actual_counts.matches(".reserve(outputs)").count(), 2);
    let declared_maxima = actual_counts.replace(".reserve(inputs)", ".reserve(32)").replace(".reserve(outputs)", ".reserve(32)");
    let cases = cases();
    let frozen: Value = serde_json::from_str(include_str!("../fixtures/collection_transfer_resources.json")).unwrap();
    let mut observations = BTreeMap::new();
    let mut ceiling_exceedances = BTreeMap::new();
    for opt in [0, 3] {
        let mut canonical = BTreeMap::new();
        for (policy, source) in [("actual-counts", actual_counts), ("declared-maxima", declared_maxima.as_str())] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            let compiled = vec![compile_at(source, opt), compile_at("module auth\nlock authorize() -> bool { return true }", opt)];
            let artifacts = participants(root, &[("transfer", "action", "transfer"), ("auth", "lock", "authorize")], &compiled);
            for case in &cases {
                let key = format!("{policy}-{}", case.name);
                let (bytes, row) = verify(
                    root,
                    &compiled,
                    &artifacts,
                    &case.before,
                    &case.after,
                    (opt == 3 && case.resource).then_some(key.as_str()),
                );
                if policy == "actual-counts" {
                    canonical.insert(&case.name, bytes);
                } else {
                    assert_eq!(&bytes, &canonical[&case.name], "O{opt}/{}: reservation policy changed transaction bytes", case.name);
                }
                if let Some(mut row) = row {
                    if policy == "actual-counts" {
                        super::resources::check_ceiling(&key, &row, &frozen["ceilings"]["arena"]);
                        row["key"] = json!(format!("arena-{}", case.name));
                        assert_eq!(row, frozen["observations"][format!("arena-{}", case.name)]);
                        row["key"] = json!(key);
                    } else {
                        // A candidate policy may fail existing acceptance. Keep
                        // those failures in the experiment, never raise limits.
                        let ceiling = &frozen["ceilings"]["arena"];
                        let mut exceeded = Vec::new();
                        for metric in
                            ["transaction_cycles", "transaction_bytes", "raw_witness_args_bytes", "occupied_output_capacity_shannons"]
                        {
                            if let Some(actual) = row[metric].as_u64() {
                                let limit = ceiling[metric].as_u64().unwrap();
                                if actual > limit {
                                    exceeded.push(json!({"metric":metric,"actual":actual,"limit":limit}));
                                }
                            }
                        }
                        for group in row["groups"].as_array().unwrap() {
                            let id = group["artifact"].as_str().unwrap();
                            for metric in ["cycles", "observed_stack_bytes", "touched_guest_page_bytes", "elf_bytes"] {
                                let actual = group[metric].as_u64().unwrap();
                                let limit = ceiling["groups"][id][metric].as_u64().unwrap();
                                if actual > limit {
                                    exceeded.push(json!({"artifact":id,"metric":metric,"actual":actual,"limit":limit}));
                                }
                            }
                        }
                        ceiling_exceedances.insert(key.clone(), exceeded);
                    }
                    observations.insert(key, row);
                }
            }
        }
    }
    assert_eq!(cases.len(), 44);
    assert_eq!(observations.len(), 24);
    let report = json!({
        "schema":"cellscript-collection-preallocation-comparison-v1",
        "scope":"two-reservation-policies-same-complete-transfer-program",
        "source_sha256": {
            "actual-counts":hex::encode(Sha256::digest(actual_counts)),
            "declared-maxima":hex::encode(Sha256::digest(&declared_maxima)),
        },
        "cases_per_policy_per_optimization":44,"optimization_levels":[0,3],
        "observations":observations,
        "candidate_ceiling_exceedances":ceiling_exceedances,
        "limitations":["no new source syntax or stable API decision", "finite business corpus, not a global worst-case proof"]
    });
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/collection-preallocation-comparison.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

#[test]
fn frozen_transfer_resources_bind_sources_and_complete_case_inventory() {
    use sha2::{Digest, Sha256};
    let fixture: Value = serde_json::from_str(include_str!("../fixtures/collection_transfer_resources.json")).unwrap();
    assert_eq!(fixture["schema"], "cellscript-collection-transfer-resources-v1");
    let sources = [
        (
            "examples/language/batches/multi_asset_transfer.cell",
            include_bytes!("../../examples/language/batches/multi_asset_transfer.cell").as_slice(),
        ),
        ("tests/fixtures/multi_asset_transfer_stack.cell", include_bytes!("../fixtures/multi_asset_transfer_stack.cell").as_slice()),
    ];
    assert_eq!(fixture["sources"].as_object().unwrap().len(), sources.len());
    for (path, bytes) in sources {
        assert_eq!(fixture["sources"][path], hex::encode(Sha256::digest(bytes)), "{path}");
    }
    let cases = cases();
    assert_eq!(cases.len(), 44);
    assert_eq!(cases.iter().map(|case| &case.name).collect::<BTreeSet<_>>().len(), cases.len());
    let required = ["arena", "stack"]
        .into_iter()
        .flat_map(|program| cases.iter().filter(|case| case.resource).map(move |case| format!("{program}-{}", case.name)))
        .collect::<BTreeSet<_>>();
    assert_eq!(required.len(), 24);
    assert_eq!(fixture["observations"].as_object().unwrap().keys().cloned().collect::<BTreeSet<_>>(), required);
    assert_eq!(
        fixture["ceilings"].as_object().unwrap().keys().map(String::as_str).collect::<BTreeSet<_>>(),
        BTreeSet::from(["arena", "stack"])
    );
    for case in cases.iter().filter(|case| case.resource) {
        let arena = &fixture["observations"][format!("arena-{}", case.name)];
        let stack = &fixture["observations"][format!("stack-{}", case.name)];
        assert_eq!(arena["transaction_sha256"], stack["transaction_sha256"]);
        assert_eq!(arena["transaction_bytes"], stack["transaction_bytes"]);
    }
}

#[test]
fn allocated_cross_package_generic_records_preserve_copies_after_owner_drop() {
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let library = "module shared::rules\npublic struct Pair<T: fixed_value> { left: T, right: T }\npublic fn first<T: fixed_value>(pair: Pair<T>) -> T { return pair.left }";
    for (ty, left, right, width) in [
        ("u64", "11", "23", 16),
        ("u128", "18446744073709551617", "36893488147419103233", 32),
        ("[u8; 17]", "b\"abcdefghijklmnopq\"", "b\"abcdefghijklmnopr\"", 34),
    ] {
        let source = format!(
            r#"module application
use shared::rules::Pair
use shared::rules::first
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<Pair<{ty}>> {{ items: 4 }}
}}
fn copied(view: &Work) -> Pair<{ty}> {{ return view.values.get(0) }}
action inspect() -> u64 {{
    let mut work = Work::new()
    work.values.push(Pair<{ty}> {{ left: {left}, right: {right} }})
    let value = copied(&work)
    work.drop()
    require first<{ty}>(value) == {left}
    require value.right == {right}
    return 0
}}"#
        );
        for opt in [0, 3] {
            for reject in [false, true] {
                let temp = tempfile::tempdir().unwrap();
                // Change only the assertion after the imported helper returns;
                // both large integer words and the final array byte must matter.
                let application = if reject {
                    source.replace(&format!("first<{ty}>(value) == {left}"), &format!("first<{ty}>(value) == {right}"))
                } else {
                    source.clone()
                };
                let compiled = super::support::compile_package(temp.path(), &application, library, opt).unwrap();
                let root = temp.path().join("application");
                let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
                    .current_dir(&root)
                    .args(["explain", "collections", "--json"])
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
                let report: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(report["owners"][0]["members"][0]["element_bytes"], width);
                assert!(compiled
                    .metadata
                    .runtime
                    .fail_closed_runtime_features
                    .iter()
                    .any(|feature| feature == "allocated-values-artifact-proof"));
                let mut fixture = super::ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.current_type_script_input_indices = vec![0];
                let payload = compiled.metadata.actions[0].entry_witness_args(&[]).unwrap();
                fixture.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let result = super::ckb_script_runner::execute_cellscript_script(
                    cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes),
                    &fixture,
                );
                assert_eq!(result.exit_code, if reject { 5 } else { 0 }, "{ty} O{opt}: {result:?}");
                let checked = cellscript_artifact_checker::check_bundle(
                    &compiled.artifact_bytes,
                    &serde_json::to_vec(&compiled.metadata).unwrap(),
                    &cellscript_artifact_checker::canonical_bytes(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
                    &cellscript_artifact_checker::canonical_bytes(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
                    &cellscript_artifact_checker::CheckerBudgets::default(),
                )
                .unwrap();
                for state in [
                    checked.binding_verification,
                    checked.structural_verification,
                    checked.lowering_record_verification,
                    checked.typed_semantics_verification,
                ] {
                    assert_eq!(state, cellscript_artifact_checker::EvidenceState::Verified);
                }
                assert!(!checked.semantic_equivalence_claimed);
            }
        }
    }
}

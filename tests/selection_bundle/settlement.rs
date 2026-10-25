//! The real settlement program through generated catalog bindings, the existing
//! packed transaction adapter and every required CKB-VM Script group.
use super::support::{bind_deployment_lines, compile_at, context, identity, participants, reference};
use super::*;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

const IDS: [&str; 4] = ["settlement", "base", "quote", "auth"];

#[test]
fn mixed_settlement_cross_package_helpers_preserve_transaction_and_authority() {
    let source = include_str!("../../examples/language/batches/mixed_settlement.cell");
    let schemas = source.find("resource Order").unwrap();
    let catalog = source.find("cell_roles Settlement").unwrap();
    let helpers = source.find("#[effect(ReadOnly)]").unwrap();
    let action = source.find("action settle").unwrap();
    let library = format!(
        "module shared::rules\n{}\n{}",
        source[schemas..catalog].replace("resource ", "public resource "),
        source[helpers..action].replace("fn ", "public fn ")
    );
    let mut application = format!(
        "{}\nuse shared::rules::Order\nuse shared::rules::Balance\nuse shared::rules::full_fill\nuse shared::rules::transfer_leg\n{}\n{}",
        &source[..schemas], &source[catalog..helpers], &source[action..]
    );
    for (placeholder, tag) in [("11", 1), ("22", 2), ("33", 3), ("44", 4)] {
        let hash = cellscript::script_handle::ckb_script_identity_hash(&identity(tag)).unwrap();
        application = application.replace(&placeholder.repeat(32), &hash[2..]);
    }
    let baseline = tempfile::tempdir().unwrap();
    let (baseline_compiled, baseline_artifacts) = artifacts(baseline.path());
    for opt in [0, 3] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let main = super::support::compile_package(root, &application, &library, opt).unwrap();
        let asset = include_str!("../fixtures/mixed_settlement_asset.cell");
        let compiled = vec![
            main,
            super::support::compile_at(asset, opt),
            super::support::compile_at(asset, opt),
            super::support::compile_at("module auth\nlock authorize() -> bool { return true }", opt),
        ];
        let artifacts = participants(
            root,
            &[
                ("settlement", "action", "settle"),
                ("base", "action", "verify"),
                ("quote", "action", "verify"),
                ("auth", "lock", "authorize"),
            ],
            &compiled,
        );
        for count in [1, 3, 6] {
            for reverse in [false, true] {
                let input = input(&compiled, &artifacts, count, reverse);
                let report = check_protocol_bundle(&input, root).unwrap();
                assert_eq!(report.status, "ok", "{report:?}");
                let (tx, materialized) =
                    cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
                assert_eq!(tx.data().as_slice(), reference(&input));
                let baseline_input = self::input(&baseline_compiled, &baseline_artifacts, count, reverse);
                assert_eq!(tx.data().as_slice(), reference(&baseline_input));
                assert_eq!(materialized.script_groups.len(), 4);
                let fixture = context(&input, &compiled, None);
                fixture.verify_tx(&tx, 20_000_000).unwrap();
                if opt == 3 {
                    super::resources::measure_package(
                        &format!("Settlement-{count}-{reverse}-valid"),
                        &fixture,
                        &input,
                        &compiled,
                        &tx,
                        &[0, 0, 0, 0],
                    );
                }
                for peer in [1, 2, 3] {
                    let error = context(&input, &compiled, Some(peer)).verify_tx(&tx, 20_000_000).unwrap_err();
                    assert!(format!("{error:?}").contains("error code 5"), "O{opt}/{count}/{reverse} peer {peer}: {error:?}");
                }
                if count == 6 && !reverse {
                    for bad_order in [false, true] {
                        let mut bad = input.clone();
                        if bad_order {
                            bad.transaction.outputs[1].data = Some(format!("0x{}", hex::encode(order(0, false))));
                        } else {
                            // Keep the base Type's total conserved, but violate
                            // the order's transfer quantity. The imported helper
                            // must reject even when the foreign asset accepts.
                            bad.transaction.outputs[2].data = Some(format!("0x{}", hex::encode(balance(0, 0x51, 999))));
                            bad.transaction.outputs[3].data = Some(format!("0x{}", hex::encode(balance(1, 0x72, 101))));
                        }
                        let report = check_protocol_bundle(&bad, root).unwrap();
                        assert_eq!(report.status, "ok");
                        let (tx, _) =
                            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
                        let error = fixture.verify_tx(&tx, 20_000_000).unwrap_err();
                        assert!(format!("{error:?}").contains("error code 5"), "imported helper: {error:?}");
                    }
                }
            }
        }
    }
}

fn artifacts(root: &Path) -> (Vec<CompileResult>, Vec<Value>) {
    artifacts_at(root, true, 3)
}

fn artifacts_at(root: &Path, named: bool, opt_level: u8) -> (Vec<CompileResult>, Vec<Value>) {
    let mut source = if named {
        include_str!("../../examples/language/batches/mixed_settlement.cell")
    } else {
        include_str!("../fixtures/mixed_settlement_positional.cell")
    }
    .to_string();
    for (placeholder, tag) in [("11", 1), ("22", 2), ("33", 3), ("44", 4)] {
        let hash = cellscript::script_handle::ckb_script_identity_hash(&identity(tag)).unwrap();
        source = source.replace(&placeholder.repeat(32), &hash[2..]);
    }
    let asset = include_str!("../fixtures/mixed_settlement_asset.cell");
    let compiled = vec![
        compile_at(&source, opt_level),
        compile_at(asset, opt_level),
        compile_at(asset, opt_level),
        compile_at("module auth\nlock authorize() -> bool { return true }", opt_level),
    ];
    let artifacts = participants(
        root,
        &[
            ("settlement", "action", "settle"),
            ("base", "action", "verify"),
            ("quote", "action", "verify"),
            ("auth", "lock", "authorize"),
        ],
        &compiled,
    );
    (compiled, artifacts)
}

fn balance(id: u64, owner: u8, amount: u64) -> Vec<u8> {
    [id.to_le_bytes().to_vec(), vec![owner; 32], amount.to_le_bytes().to_vec()].concat()
}

fn order(id: u64, settled: bool) -> Vec<u8> {
    [
        id.to_le_bytes().to_vec(),
        vec![0x51; 32],
        vec![0x72; 32],
        (id * 3 + 7).to_le_bytes().to_vec(),
        (id * 5 + 11).to_le_bytes().to_vec(),
        vec![u8::from(settled)],
    ]
    .concat()
}

fn input(compiled: &[CompileResult], artifacts: &[Value], count: usize, reverse: bool) -> ProtocolBundleInput {
    let cell = |tag: Option<u8>, data: Vec<u8>| {
        json!({"cell_commitment": format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&data))),
            "capacity": 400_000_000_000u64, "lock": identity(4), "type": tag.map(identity),
            "data": format!("0x{}", hex::encode(data))})
    };
    let mut inputs = vec![cell(None, balance(0, 0x51, u64::MAX))];
    let mut outputs = vec![cell(None, balance(1, 0x72, 0))];
    for id in 0..count as u64 {
        inputs.push(cell(Some(1), order(id, false)));
        outputs.push(cell(Some(1), order(id, true)));
        for (tag, quantity, from, to) in [(2, id * 3 + 7, 0x51, 0x72), (3, id * 5 + 11, 0x72, 0x51)] {
            inputs.push(cell(Some(tag), balance(id * 2, from, 1000 + id)));
            inputs.push(cell(Some(tag), balance(id * 2 + 1, to, 100 + id)));
            outputs.push(cell(Some(tag), balance(id * 2, from, 1000 + id - quantity)));
            outputs.push(cell(Some(tag), balance(id * 2 + 1, to, 100 + id + quantity)));
        }
    }
    if reverse {
        outputs.reverse();
    }
    for (index, cell) in inputs.iter_mut().enumerate() {
        cell["out_point"] = json!({"tx_hash": format!("0x{}", "90".repeat(32)), "index": index});
        cell["since"] = json!(0);
    }
    let mut witnesses = vec![json!({}); inputs.len()];
    let mut claims = Vec::new();
    for (artifact, index) in [(0, 1), (1, 2), (2, 4)] {
        let bytes = compiled[artifact].metadata.actions[0].entry_witness_args(&[]).unwrap();
        let commitment = format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&bytes)));
        witnesses[index] = json!({"input_type": commitment, "input_type_bytes": format!("0x{}", hex::encode(bytes))});
        claims.push(json!({"artifact": IDS[artifact], "name": "entry", "index": index, "field": "input_type",
            "ownership": "exclusive-write", "abi": "cellscript-entry-witness-v1", "value_commitment": commitment}));
    }
    let roles = [(0, 1), (1, 2), (2, 4), (3, 0)].map(|(artifact, index)| {
        let mut role =
            json!({"artifact": IDS[artifact], "name": "group-root", "location": "input", "index": index, "ownership": "shared-read"});
        role[if artifact == 3 { "expected_lock" } else { "expected_type" }] =
            serde_json::to_value(identity(artifact as u8 + 1)).unwrap();
        role
    });
    let mut input: ProtocolBundleInput = serde_json::from_value(json!({
        "schema": "cellscript-protocol-bundle-input-v1", "network": artifacts[0]["deployment"]["network"],
        "artifacts": artifacts, "roles": roles, "witnesses": claims,
        "transaction": {"version": 0, "inputs": inputs, "outputs": outputs, "witnesses": witnesses,
            "cell_deps": artifacts.iter().map(|a| a["deployment"]["code_cell_dep"].clone()).collect::<Vec<_>>(),
            "header_deps": [], "fee_policy_hash": format!("0x{}", "de".repeat(32)), "change_policy_hash": format!("0x{}", "ef".repeat(32))}
    })).unwrap();
    bind_deployment_lines(&mut input, compiled, 4);
    input
}

#[test]
fn mixed_settlement_bundle_materializes_canonical_bytes_and_executes_every_group() {
    for opt_level in [0, 3] {
        let named = run_bundle(true, opt_level);
        let positional = run_bundle(false, opt_level);
        assert_eq!(named, positional, "named/positional complete settlement transaction bytes O{opt_level}");
    }
}

fn run_bundle(named: bool, opt_level: u8) -> Vec<Vec<u8>> {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (compiled, artifacts) = artifacts_at(root, named, opt_level);
    let mut transactions = Vec::new();
    for count in [1, 3, 6] {
        for reverse in [false, true] {
            let input = input(&compiled, &artifacts, count, reverse);
            let report = check_protocol_bundle(&input, root).unwrap();
            assert_eq!(report.status, "ok", "{report:?}");
            let report = serde_json::to_value(report).unwrap();
            let (tx, materialized) =
                cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
            assert_eq!(tx.data().as_slice(), reference(&input));
            transactions.push(tx.data().as_slice().to_vec());
            assert_eq!(materialized.script_groups.len(), 4);
            assert_eq!(materialized.exact_role_selections["settlement"][0].cells.len(), count * 10);
            assert_eq!(materialized.ckb_vm_execution, "not-executed");
            let vm_tx = packed::Transaction::from_slice(tx.data().as_slice()).unwrap().into_view();
            let fixture = context(&input, &compiled, None);
            let cycles = fixture.verify_tx(&vm_tx, 20_000_000).unwrap();
            if opt_level == 3 {
                if named {
                    super::resources::measure(
                        &format!("Settlement-{count}-{reverse}-valid"),
                        &fixture,
                        &input,
                        &compiled,
                        &vm_tx,
                        &[0, 0, 0, 0],
                    );
                } else {
                    super::resources::observe(
                        &format!("Settlement-{count}-{reverse}-positional"),
                        &fixture,
                        &input,
                        &compiled,
                        &vm_tx,
                        &[0, 0, 0, 0],
                    );
                }
            }
            eprintln!(
                "mixed settlement bundle: named={named} O{opt_level} count={count} reverse={reverse} bytes={} cycles={cycles}",
                tx.data().as_slice().len()
            );
            if count != 6 || reverse {
                continue;
            }
            // Exercise resolver checks with actual local fixture Cells. These
            // API results are not published as live-node evidence.
            let live_inputs =
                vm_tx.inputs().into_iter().map(|cell| fixture.get_cell(&cell.previous_output()).unwrap()).collect::<Vec<_>>();
            let resolve = |candidate: &_, cells: &[_]| {
                cellscript_ckb_adapter::protocol_bundle_live_resolution_evidence(
                    candidate,
                    &materialized,
                    &input.network.chain_id,
                    &input.network.genesis_hash,
                    cells,
                )
            };
            let live = resolve(&tx, &live_inputs).unwrap();
            let mut missing_admission = input.clone();
            missing_admission.deployment_lines.pop();
            assert!(check_protocol_bundle(&missing_admission, root).unwrap_err().to_string().contains("deployment-line admission"));
            assert!(resolve(&tx, &live_inputs[..live_inputs.len() - 1]).is_err());
            let mut changed_inputs = live_inputs.clone();
            changed_inputs.swap(1, 2);
            assert!(resolve(&tx, &changed_inputs).is_err());
            let mut changed_inputs = live_inputs.clone();
            changed_inputs.last_mut().unwrap().1 = Bytes::from(balance(11, 0x51, 1));
            assert!(resolve(&tx, &changed_inputs).is_err());
            let mut witnesses = tx.witnesses().into_iter().collect::<Vec<_>>();
            witnesses[1] = Bytes::from_static(b"post-build substitution").pack();
            let changed_tx = tx.clone().as_advanced_builder().set_witnesses(witnesses).build();
            assert!(resolve(&changed_tx, &live_inputs).is_err());
            let dependencies = vm_tx
                .cell_deps()
                .into_iter()
                .enumerate()
                .map(|(index, dep)| {
                    let (output, data) = fixture.get_cell(&dep.out_point()).unwrap();
                    cellscript_ckb_adapter::ProtocolBundleCellDepObservation {
                        transaction_cell_dep_index: index as u32,
                        root: cellscript_ckb_adapter::ProtocolBundleLiveCellObservation { out_point: dep.out_point(), output, data },
                        dep_group_members: vec![],
                    }
                })
                .collect::<Vec<_>>();
            let resolve_deps =
                |deps: &[_]| cellscript_ckb_adapter::protocol_bundle_dependency_resolution_evidence(&tx, &materialized, &live, deps);
            let resolved = resolve_deps(&dependencies).unwrap();
            assert_eq!(resolved.cell_deps.len(), 4);
            for index in 0..dependencies.len() {
                let mut changed = dependencies.clone();
                changed[index].root.data = Bytes::from_static(b"substituted code or admission");
                assert!(resolve_deps(&changed).is_err(), "dependency {index}");
                changed.remove(index);
                assert!(resolve_deps(&changed).is_err(), "missing dependency {index}");
            }
            for peer in [1, 2, 3] {
                let error = context(&input, &compiled, Some(peer)).verify_tx(&vm_tx, 20_000_000).unwrap_err();
                assert!(format!("{error:?}").contains("ValidationFailure"), "required group {}: {error:?}", IDS[peer]);
            }
            // Exact-role reconstruction does not establish the package's
            // transfer formula. A late amount change retains all identities
            // and correspondence, but the actual transaction must reject.
            let mut bad = input.clone();
            let data = bad.transaction.outputs.last_mut().unwrap().data.as_mut().unwrap();
            let mut bytes = hex::decode(&data[2..]).unwrap();
            bytes[40] ^= 1;
            *data = format!("0x{}", hex::encode(bytes));
            let bad_report = check_protocol_bundle(&bad, root).unwrap();
            assert_eq!(bad_report.status, "ok");
            let (bad_tx, _) =
                cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&bad_report).unwrap()).unwrap();
            let bad_vm_tx = packed::Transaction::from_slice(bad_tx.data().as_slice()).unwrap().into_view();
            assert!(context(&bad, &compiled, None).verify_tx(&bad_vm_tx, 20_000_000).is_err());
            if named && opt_level == 3 {
                super::resources::measure("Settlement-6-false-late-amount", &fixture, &bad, &compiled, &bad_vm_tx, &[5, 0, 5, 0]);
                super::resources::scan_limits("Settlement", 6, &input, &compiled, root, &bad, &[5, 0, 5, 0]);
            }
            std::fs::write(root.join("checked.json"), serde_json::to_vec(&report).unwrap()).unwrap();
            for mutation in 0..4 {
                let mut hostile = report.clone();
                match mutation {
                    0 => {
                        hostile["bundle"]["transaction"]["outputs"][30]["data"] =
                            json!(format!("0x{}", hex::encode(balance(99, 0x51, 100))))
                    }
                    1 => hostile["bundle"]["transaction"]["outputs"][30]["lock"]["args"] = json!("0x01"),
                    2 => hostile["evidence"]["exact_role_selections"]["settlement"][0]["pairs"][0]["outputs"][0] = json!(0),
                    3 => hostile["bundle"]["transaction"]["witnesses"][1]["input_type_bytes"] = json!("0x00"),
                    _ => unreachable!(),
                }
                rebind(&mut hostile);
                assert!(
                    cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&hostile).unwrap()).is_err(),
                    "mutation {mutation}"
                );
            }
        }
    }
    let js = root.join("sdk-check.mjs");
    std::fs::write(&js, r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
const report = JSON.parse(fs.readFileSync(new URL('./checked.json', import.meta.url)));
for (const a of report.bundle.artifacts) {
  const sdk = await import(`./${a.id}-sdk/src/index.ts`);
  const d = a.deployment;
  const binding = sdk.bindProtocolBundleArtifact({ id: a.id, packageCoordinate: a.package_coordinate,
    lockNodeId: a.lock_node_id, entry: a.entry, scriptRole: a.script_role,
    deployment: { network: {chainId: d.network.chain_id, genesisHash: d.network.genesis_hash},
      artifactHash: d.artifact_hash, script: {codeHash: d.script.code_hash, hashType: d.script.hash_type, args: d.script.args},
      codeCellDep: {outPoint: {txHash: d.code_cell_dep.out_point.tx_hash, index: d.code_cell_dep.out_point.index}, depType: d.code_cell_dep.dep_type} }});
  assert.equal(sdk.exactScriptHandleFromCheckedBundle(report, binding).artifact.id, a.id);
  if (a.id === 'settlement') {
    const plan = sdk.planSettle({});
    assert.equal(plan.canSubmit, false);
    for (const blocker of ['exact-role-selection-artifact-proof', 'cell-data-view-artifact-proof']) {
      assert.ok(plan.failClosedRuntimeFeatures.includes(blocker));
    }
    assert.deepEqual(binding.exactRoleContracts, a.exact_role_contracts);
    assert.equal(binding.exactRoleContracts.sites[0].contract.roles.length, 3);
    const bad = structuredClone(report);
    bad.bundle.artifacts.find(a => a.id === 'settlement').exact_role_contracts.sites[0].contract.roles.reverse();
    assert.throws(() => sdk.exactScriptHandleFromCheckedBundle(bad, binding), /exact-role catalog/);
  }
}
"#).unwrap();
    let result = Command::new("node").arg("--experimental-strip-types").arg(js).output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    transactions
}

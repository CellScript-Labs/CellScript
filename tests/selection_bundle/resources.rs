//! Exact-transaction resource observations. Never add separately replayed
//! rejected groups into a purported executed transaction cycle total.
use super::*;
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{
        core::{Capacity, TransactionView},
        prelude::*,
    },
    context::Context,
};
use sha2::{Digest, Sha256};

pub(super) fn measure(
    key: &str,
    context: &Context,
    input: &ProtocolBundleInput,
    compiled: &[CompileResult],
    tx: &TransactionView,
    exits: &[i8],
) {
    let row = observe(key, context, input, compiled, tx, exits);
    check_frozen(key, &row);
}

/// Library extraction may change source/artifact identities, but it must not
/// silently enlarge the original program's measured resource allowance.
pub(super) fn measure_package(
    key: &str,
    context: &Context,
    input: &ProtocolBundleInput,
    compiled: &[CompileResult],
    tx: &TransactionView,
    exits: &[i8],
) {
    let row = observe(&format!("{key}-package"), context, input, compiled, tx, exits);
    let profile = key.split('-').take(2).collect::<Vec<_>>().join("-");
    check_ceiling(key, &row, &frozen()["ceilings"][&profile]);
}

pub(super) fn observe(
    key: &str,
    context: &Context,
    input: &ProtocolBundleInput,
    compiled: &[CompileResult],
    tx: &TransactionView,
    exits: &[i8],
) -> Value {
    // Derive required groups from actual resolved inputs and packed outputs,
    // independently of the participant list we are about to measure.
    let mut groups = BTreeMap::new();
    for cell in tx.inputs() {
        let (output, _) = context.get_cell(&cell.previous_output()).unwrap();
        groups.insert((0, output.lock().as_slice().to_vec()), output.lock());
        if let Some(script) = output.type_().to_opt() {
            groups.insert((1, script.as_slice().to_vec()), script);
        }
    }
    for output in tx.outputs() {
        if let Some(script) = output.type_().to_opt() {
            groups.insert((1, script.as_slice().to_vec()), script);
        }
    }
    assert_eq!(groups.len(), input.artifacts.len());
    assert_eq!(compiled.len(), exits.len());
    let valid = exits.iter().all(|exit| *exit == 0);
    let outcome = context.verify_tx(tx, 20_000_000);
    if valid {
        assert!(outcome.is_ok(), "{key}: {outcome:?}");
    } else {
        assert!(outcome.is_err(), "{key}: invalid transaction accepted");
    }
    let mut measured = Vec::new();
    let mut group_sum = 0;
    for ((artifact, result), exit) in input.artifacts.iter().zip(compiled).zip(exits) {
        let lock = artifact.script_role == cellscript::protocol_bundle::ProtocolScriptRole::Lock;
        let script = super::support::packed_script(&artifact.deployment.script);
        assert_eq!(groups.remove(&(if lock { 0 } else { 1 }, script.as_slice().to_vec())), Some(script.clone()));
        let trace = super::cost_trace::measure(
            context,
            tx,
            &script,
            if lock { ScriptGroupType::Lock } else { ScriptGroupType::Type },
            20_000_000,
        );
        assert_eq!(trace.status, "measured", "{key}/{}: {:?}", artifact.id, trace.unavailable_reason);
        assert_eq!(trace.exit_code, Some(*exit), "{key}/{}", artifact.id);
        assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
        // These complete programs have no EXEC/SPAWN. Do not silently drop a
        // newly introduced process or ELF generation from the resource scope.
        assert_eq!(trace.vms.len(), 1);
        let vm = &trace.vms[0];
        assert_eq!(vm.program_sha256, hex::encode(Sha256::digest(&result.artifact_bytes)));
        assert!(vm.observed_stack_bytes > 0);
        assert!(vm.touched_guest_page_bytes > 0);
        group_sum += trace.authoritative_cycles.unwrap();
        let frames = result
            .verified_lowering_record
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .map(|entry| (entry.id.clone(), entry.frame_size_bytes))
            .collect::<BTreeMap<_, _>>();
        measured.push(json!({
            "artifact":artifact.id,"role":if lock {"lock"} else {"type"},
            "script_hash":hex::encode(script.calc_script_hash().as_slice()),
            "elf_sha256":vm.program_sha256,"elf_bytes":result.artifact_bytes.len(),
            "exit_code":exit,"cycles":trace.authoritative_cycles,
            "scheduler_and_loading_cycles":trace.scheduler_and_loading_cycles,
            "instruction_cycles":vm.pcs.values().map(|pc|pc.instruction_cycles).sum::<u64>(),
            "synchronous_syscall_cycles":vm.pcs.values().map(|pc|pc.synchronous_syscall_cycles).sum::<u64>(),
            "declared_frames":frames,"vm_version":vm.vm_version,"isa":vm.isa,
            "observed_stack_bytes":vm.observed_stack_bytes,
            "touched_guest_page_bytes":vm.touched_guest_page_bytes,
            "observed_heap_bytes":null
        }));
    }
    assert!(groups.is_empty());
    let transaction_cycles = outcome.ok();
    if valid {
        assert_eq!(transaction_cycles, Some(group_sum));
    }
    let occupied: u64 = tx
        .outputs()
        .into_iter()
        .zip(tx.outputs_data())
        .map(|(cell, data)| cell.occupied_capacity(Capacity::bytes(data.raw_data().len()).unwrap()).unwrap().as_u64())
        .sum();
    let row = json!({
        "key":key,"transaction_sha256":hex::encode(Sha256::digest(tx.data().as_slice())),
        "transaction_bytes":tx.data().as_slice().len(),
        "raw_witness_args_bytes":tx.witnesses().into_iter().map(|w|w.raw_data().len()).sum::<usize>(),
        "occupied_output_capacity_shannons":occupied,"transaction_cycles":transaction_cycles,
        "groups":measured
    });
    eprintln!("selection-resource {row}");
    row
}

fn frozen() -> Value {
    serde_json::from_str(include_str!("../fixtures/selection_business_resources.json")).unwrap()
}

fn check_frozen(key: &str, row: &Value) {
    let fixture = frozen();
    assert_eq!(fixture["schema"], "cellscript-selection-business-resources-v1");
    let profile = key.split('-').take(2).collect::<Vec<_>>().join("-");
    let ceiling = &fixture["ceilings"][&profile];
    check_ceiling(key, row, ceiling);
    // Numeric ceilings are separate from exact observations: an intentional
    // artifact/fixture refresh must not silently raise previously frozen limits.
    // The full transaction SHA includes witnesses, unlike the raw tx hash.
    if refresh_observations("selection_business_resources.json", &json!({key: row})) {
        return;
    }
    assert_eq!(row, &fixture["observations"][key], "{key}: resource/identity drift requires review");
}

/// Explicit measurement refresh only. Callers must check the original ceilings
/// first; this replaces existing observations, never limits or case inventory.
pub(super) fn refresh_observations(file: &str, rows: &Value) -> bool {
    if std::env::var("CELLSCRIPT_UPDATE_SELECTION_RESOURCES").as_deref() != Ok("1") {
        return false;
    }
    static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITE.lock().unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(file);
    let mut fixture: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (key, row) in rows.as_object().unwrap() {
        *fixture["observations"].get_mut(key).expect("refresh requires an existing measured case") = row.clone();
    }
    let mut bytes = serde_json::to_vec_pretty(&fixture).unwrap();
    bytes.push(b'\n');
    std::fs::write(path, bytes).unwrap();
    true
}

pub(super) fn check_ceiling(key: &str, row: &Value, ceiling: &Value) {
    for metric in ["transaction_cycles", "transaction_bytes", "raw_witness_args_bytes", "occupied_output_capacity_shannons"] {
        if metric == "transaction_cycles" && row[metric].is_null() {
            continue;
        }
        assert!(row[metric].as_u64().unwrap() <= ceiling[metric].as_u64().unwrap(), "{key}: exceeded {metric}");
    }
    for group in row["groups"].as_array().unwrap() {
        let id = group["artifact"].as_str().unwrap();
        for metric in ["cycles", "observed_stack_bytes", "touched_guest_page_bytes", "elf_bytes"] {
            assert!(
                group[metric].as_u64().unwrap() <= ceiling["groups"][id][metric].as_u64().unwrap(),
                "{key}/{id}: exceeded {metric}"
            );
        }
    }
}

/// Fill the declared transaction scan domain with unrelated fee/change Cells.
/// Maximum selected membership alone does not exercise the maximum global scan.
pub(super) fn scan_limits(
    name: &str,
    count: usize,
    base: &ProtocolBundleInput,
    compiled: &[CompileResult],
    root: &Path,
    late: &ProtocolBundleInput,
    late_exits: &[i8],
) {
    // The late candidate changes output business data only. Keep its input
    // resolution, witnesses and identities identical before adding scan work.
    let mut restored = late.clone();
    restored.transaction.outputs = base.transaction.outputs.clone();
    assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(base).unwrap());
    assert_eq!(late.transaction.outputs.len(), base.transaction.outputs.len());
    assert_eq!(late_exits.len(), compiled.len());
    assert_eq!(late_exits[0], 5);
    let mut full = base.clone();
    let untyped_input = base.transaction.inputs.iter().find(|cell| cell.r#type.is_none()).unwrap().clone();
    let untyped_output = base.transaction.outputs.iter().find(|cell| cell.r#type.is_none()).unwrap().clone();
    let add_input = |input: &mut ProtocolBundleInput| {
        let mut cell = untyped_input.clone();
        cell.out_point.as_mut().unwrap().index = input.transaction.inputs.len() as u32;
        input.transaction.inputs.push(cell);
        input.transaction.witnesses.push(serde_json::from_value(json!({})).unwrap());
    };
    while full.transaction.inputs.len() < 32 {
        add_input(&mut full);
    }
    while full.transaction.outputs.len() < 32 {
        full.transaction.outputs.push(untyped_output.clone());
    }
    let report = check_protocol_bundle(&full, root).unwrap();
    let original = check_protocol_bundle(base, root).unwrap();
    assert_eq!(
        serde_json::to_value(&report.evidence.exact_role_selections).unwrap(),
        serde_json::to_value(&original.evidence.exact_role_selections).unwrap()
    );
    let (tx, _) = cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
    assert_eq!(tx.data().as_slice(), super::support::reference(&full));
    let tx = ckb_testtool::ckb_types::packed::Transaction::from_slice(tx.data().as_slice()).unwrap().into_view();
    measure(
        &format!("{name}Scan-{count}-valid"),
        &super::support::context(&full, compiled, None),
        &full,
        compiled,
        &tx,
        &vec![0; compiled.len()],
    );
    for output in [false, true] {
        let mut over = full.clone();
        if output {
            over.transaction.outputs.push(untyped_output.clone());
            // Keep the unrelated capacity balanced so that the Script scan
            // bound, rather than funding, is the rejection under test.
            over.transaction.inputs[0].capacity += untyped_output.capacity;
        } else {
            add_input(&mut over);
        }
        let error = check_protocol_bundle(&over, root).unwrap_err();
        assert!(error.to_string().contains("scan bound"), "{error}");
        // The rejected candidate has no adapter report. Independently construct
        // its exact bytes to test the on-chain end probe as well as host bounds.
        let tx = ckb_testtool::ckb_types::packed::Transaction::from_slice(&super::support::reference(&over)).unwrap().into_view();
        let mut exits = vec![0; compiled.len()];
        exits[0] = 21;
        measure(
            &format!("{name}Scan-{count}-{}33", if output { "output" } else { "input" }),
            &super::support::context(&over, compiled, None),
            &over,
            compiled,
            &tx,
            &exits,
        );
    }
    // Compose the maximum scan with a canonical but semantically invalid last
    // transfer/note. A fast bounds failure cannot stand in for this late path.
    let mut bad = full.clone();
    bad.transaction.outputs[..late.transaction.outputs.len()].clone_from_slice(&late.transaction.outputs);
    let bad_report = check_protocol_bundle(&bad, root).unwrap();
    assert_eq!(
        serde_json::to_value(&bad_report.evidence.exact_role_selections).unwrap(),
        serde_json::to_value(&report.evidence.exact_role_selections).unwrap()
    );
    let (bad_tx, _) = cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&bad_report).unwrap()).unwrap();
    assert_eq!(bad_tx.data().as_slice(), super::support::reference(&bad));
    let bad_tx = ckb_testtool::ckb_types::packed::Transaction::from_slice(bad_tx.data().as_slice()).unwrap().into_view();
    measure(
        &format!("{name}Scan-{count}-late-business"),
        &super::support::context(&bad, compiled, None),
        &bad,
        compiled,
        &bad_tx,
        late_exits,
    );
}

#[test]
fn frozen_business_resources_bind_sources_and_complete_case_inventory() {
    let fixture = frozen();
    let sources = [
        (
            "examples/language/batches/mixed_settlement.cell",
            include_bytes!("../../examples/language/batches/mixed_settlement.cell").as_slice(),
        ),
        (
            "examples/language/batches/nested_migration.cell",
            include_bytes!("../../examples/language/batches/nested_migration.cell").as_slice(),
        ),
        (
            "examples/language/batches/record_migration.cell",
            include_bytes!("../../examples/language/batches/record_migration.cell").as_slice(),
        ),
        ("tests/fixtures/mixed_settlement_asset.cell", include_bytes!("../fixtures/mixed_settlement_asset.cell").as_slice()),
        ("tests/fixtures/nested_migration_registry.cell", include_bytes!("../fixtures/nested_migration_registry.cell").as_slice()),
    ];
    assert_eq!(fixture["sources"].as_object().unwrap().len(), sources.len());
    for (path, bytes) in sources {
        assert_eq!(fixture["sources"][path], hex::encode(Sha256::digest(bytes)), "{path}");
    }
    let mut required = std::collections::BTreeSet::new();
    let mut profiles = std::collections::BTreeSet::new();
    for (name, counts, maximum, late) in [
        ("Settlement", vec![1, 3, 6], 6, "late-amount"),
        ("Bytes", vec![1, 3, 8], 8, "late-note"),
        ("Records", vec![1, 3, 7, 8], 8, "late-note"),
    ] {
        for count in counts {
            profiles.insert(format!("{name}-{count}"));
            for reverse in [false, true] {
                required.insert(format!("{name}-{count}-{reverse}-valid"));
            }
        }
        required.insert(format!("{name}-{maximum}-false-{late}"));
        required.insert(format!("{name}Scan-{maximum}-late-business"));
        profiles.insert(format!("{name}Scan-{maximum}"));
        for outcome in ["valid", "input33", "output33"] {
            required.insert(format!("{name}Scan-{maximum}-{outcome}"));
        }
    }
    assert_eq!(fixture["observations"].as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>(), required);
    assert_eq!(fixture["ceilings"].as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>(), profiles);
}

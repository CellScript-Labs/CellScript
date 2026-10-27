//! Freeze maximum-size sorted construction, duplicate compaction, late disorder
//! and shared-budget exhaustion with the same authoritative transaction fixture.
use super::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub(super) fn observe(
    compiled: &cellscript::CompileResult,
    source: &str,
    arguments: &[cellscript::EntryWitnessArg],
    expected: &ckb_script_runner::CkbScriptExecutionResult,
) -> Value {
    let payload = compiled.metadata.actions[0].entry_witness_args(arguments).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    let elf = cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes);
    let (result, trace) = ckb_script_runner::execute_cellscript_script_with_observer(
        elf,
        &fixture,
        |tx, _| tx,
        |context, tx, script| cost_trace::measure(context, tx, script, ckb_testtool::ckb_script::ScriptGroupType::Type, 10_000_000),
    );
    assert_eq!(result.exit_code, expected.exit_code);
    assert_eq!(result.serialized_transaction_hash, expected.serialized_transaction_hash);
    assert_eq!(result.cycles, expected.cycles);
    assert_eq!(trace.status, "measured", "{:?}", trace.unavailable_reason);
    assert_eq!(trace.exit_code.map(i64::from), Some(expected.exit_code));
    assert_eq!(trace.authoritative_cycles, trace.stepped_cycles);
    assert_eq!(trace.vms.len(), 1);
    let vm = &trace.vms[0];
    assert_eq!(vm.program_sha256, hex::encode(Sha256::digest(elf)));
    assert!(vm.observed_stack_bytes > 0);
    assert!(vm.touched_guest_page_bytes > 0);
    json!({
        "source_sha256":hex::encode(Sha256::digest(source)),
        "elf_sha256":vm.program_sha256,"elf_bytes":elf.len(),
        "exit_code":result.exit_code,"type_group_cycles":trace.authoritative_cycles,
        "transaction_cycles":(result.exit_code == 0).then_some(result.cycles),
        "transaction_bytes":result.transaction_bytes,"witness_bytes":result.witness_bytes,
        "transaction_hash":result.serialized_transaction_hash,
        "observed_stack_bytes":vm.observed_stack_bytes,"touched_guest_page_bytes":vm.touched_guest_page_bytes,
        "observed_heap_bytes":null,"declared_arena_bytes":8192,"declared_allocation_events":2,
        "vm_version":vm.vm_version,"isa":vm.isa,
        "declared_frames":compiled.verified_lowering_record.as_ref().unwrap().entries.iter()
            .map(|entry|(entry.id.clone(),entry.frame_size_bytes)).collect::<BTreeMap<_,_>>()
    })
}

pub(super) fn check_sorted(observations: BTreeMap<String, Value>) {
    let keys = [false, true]
        .into_iter()
        .flat_map(|precharge| (0..4).map(move |pattern| format!("precharge-{precharge}-pattern-{pattern}")))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(observations.keys().cloned().collect::<std::collections::BTreeSet<_>>(), keys);
    let report = json!({
        "schema":"cellscript-sorted-collection-resources-v1",
        "scope":"O3 1024-input OrderedMap<u16,u8> construction; independent Type-group replay over the exact transaction; finite corpus, not global worst-case proof",
        "stack_baseline":"entry-sp-after-argv-before-first-instruction-per-vm-generation",
        "guest_memory_scope":"distinct touched guest pages including ELF, argv and syscall buffers; not heap accounting",
        "heap_unavailable_reason":"separate heap accounting is not instrumented; declared arena bytes are not a measured heap peak",
        "rejected_transaction_cycles_unavailable_reason":"transaction rejection API has no cycle total; Type-group rejection cycles are measured separately",
        "observations":observations
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/sorted-collection-resources.json");
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    let frozen: Value = serde_json::from_str(include_str!("../fixtures/sorted_collection_resources.json")).unwrap();
    for row in observations.values() {
        for metric in [
            "type_group_cycles",
            "elf_bytes",
            "observed_stack_bytes",
            "touched_guest_page_bytes",
            "transaction_bytes",
            "witness_bytes",
        ] {
            let limit = frozen["ceilings"][metric].as_u64().expect("missing finite resource ceiling");
            assert!(row[metric].as_u64().unwrap() <= limit, "{metric} exceeds frozen ceiling");
        }
    }
    assert_eq!(report, frozen["report"], "sorted-construction identity/resource drift requires review");
}

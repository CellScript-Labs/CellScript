//! Separate diagnostic companion to the unchanged frozen cost report.

use cellscript::{
    compile_with_executable_surface_policy, CellScriptEdition, CompileOptions, CompileResult, EntryWitnessArg, ExecutableSurfacePolicy,
};
use ckb_testtool::{
    ckb_script::ScriptGroupType,
    ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*},
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf};

thread_local! {
    static SCALAR_RANGES: std::cell::RefCell<BTreeMap<String, Vec<Value>>> = const { std::cell::RefCell::new(BTreeMap::new()) };
    // Keep large per-PC traces out of the frozen helper's intermediate summary
    // file. Attach them once when writing this companion's final report.
    static TRACES: std::cell::RefCell<BTreeMap<String, Value>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;
#[path = "support/cost_expanded.rs"]
mod cost_expanded;
#[path = "support/cost_measurement.rs"]
mod cost_measurement;
#[path = "support/cost_memory.rs"]
mod cost_memory;
#[path = "support/cost_provenance.rs"]
// This companion uses source/VM provenance, but does not rebuild Rust binaries.
#[allow(dead_code)]
mod cost_provenance;
#[path = "support/cost_scalar.rs"]
mod cost_scalar;
#[path = "support/cost_stack.rs"]
mod cost_stack;
#[path = "support/cost_trace.rs"]
mod cost_trace;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn options() -> CompileOptions {
    CompileOptions {
        edition: CellScriptEdition::Edition2027,
        opt_level: 3,
        target: Some("riscv64-elf".into()),
        target_profile: Some("ckb".into()),
        ..Default::default()
    }
}

fn compile_cellscript(source: &str) -> CompileResult {
    let result = compile_with_executable_surface_policy(source, options(), ExecutableSurfacePolicy::DenyFailClosed)
        .expect("compile frozen fixture");
    let record = result.verified_lowering_record.as_ref().unwrap();
    let ranges = record
        .blocks
        .iter()
        .map(|block| {
            let entry = record.entries.iter().find(|entry| entry.id == block.owner_entry).unwrap();
            json!({"kind":entry.name,"start":block.range.start,"end":block.range.end})
        })
        .collect();
    SCALAR_RANGES.with(|all| {
        all.borrow_mut().insert(cost_provenance::sha256(cellscript::strip_vm_abi_trailer(&result.artifact_bytes)), ranges)
    });
    result
}

fn witness_for(result: &CompileResult, args: &[EntryWitnessArg]) -> Bytes {
    let payload = result.metadata.actions[0].entry_witness_args(args).expect("encode declared arguments");
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

fn measured_run<F>(
    name: &str,
    elf: &[u8],
    fixture: &ckb_script_runner::CkbVmFixture,
    transform: F,
    runs: &mut Vec<Value>,
) -> ckb_script_runner::CkbScriptExecutionResult
where
    F: FnOnce(TransactionView, packed::Script) -> TransactionView,
{
    let (oracle, (group, trace, transaction_bytes, witness_bytes)) =
        ckb_script_runner::execute_cellscript_script_with_observer(elf, fixture, transform, |context, tx, script| {
            (
                cost_measurement::measure_group(context, tx, script, ScriptGroupType::Type, 10_000_000),
                cost_trace::measure(context, tx, script, ScriptGroupType::Type, 10_000_000),
                tx.data().as_bytes().len(),
                tx.witnesses().into_iter().map(|witness| witness.raw_data().len()).sum::<usize>(),
            )
        });
    assert_eq!(trace.status, "measured", "{name}: {:?}", trace.unavailable_reason);
    assert_eq!(trace.exit_code.map(i64::from), Some(oracle.exit_code));
    group.required_cycles();
    let parsed = cellscript_artifact_checker::parse_elf(elf, 1_000_000).expect("bounded compiler ELF");
    let text_bytes = parsed.text.size;
    let rodata_bytes = parsed.rodata.size;
    let other_sections: u64 = parsed
        .sections
        .iter()
        .filter(|section| section.name != ".text" && section.name != ".rodata")
        .map(|section| section.size)
        .sum();
    let program_headers = u16::from_le_bytes(elf[56..58].try_into().unwrap()) as u64 * 56;
    let section_headers = u16::from_le_bytes(elf[60..62].try_into().unwrap()) as u64 * 64;
    let headers = 64 + program_headers + section_headers;
    let padding = (elf.len() as u64).checked_sub(text_bytes + rodata_bytes + other_sections + headers).expect("ELF file accounting");
    let compressed_sites = parsed
        .instructions
        .iter()
        .filter(|instruction| {
            if instruction.address >= parsed.entry && instruction.address < parsed.entry + 20 {
                return false;
            }
            let word = instruction.word;
            let rd = (word >> 7) & 31;
            let rs1 = (word >> 15) & 31;
            let immediate = (word as i32) >> 20;
            match word & 0x707f {
                0x13 => rd != 0 && (-32..=31).contains(&immediate) && (rs1 == 0 || (rs1 == rd && immediate != 0)),
                0x3003 => rd != 0 && rs1 == 2 && (0..=504).contains(&immediate) && immediate % 8 == 0,
                0x3023 => {
                    let offset = (((word >> 25) << 5) | ((word >> 7) & 31)) as i32;
                    rs1 == 2 && (0..=504).contains(&offset) && offset % 8 == 0
                }
                0x67 => rd == 0 && rs1 != 0 && immediate == 0,
                _ => false,
            }
        })
        .count();
    TRACES.with(|all| assert!(all.borrow_mut().insert(name.to_string(), serde_json::to_value(trace).unwrap()).is_none()));
    runs.push(json!({
        "name":name, "elf_sha256":cost_provenance::sha256(elf),
        "raw_transaction_hash":oracle.raw_transaction_hash, "serialized_transaction_hash":oracle.serialized_transaction_hash,
        "transaction":cost_measurement::CycleMeasurement::transaction(oracle.exit_code,oracle.cycles),
        "group":group,
        "transaction_bytes":transaction_bytes, "witness_bytes":witness_bytes,
        "compressed_site_survey":{"grammar":"c.li/c.addi/c.ldsp/c.sdsp/c.jr; excludes fixed 20-byte entry trampoline",
            "sites":compressed_sites,"local_instruction_byte_opportunity":compressed_sites*2,
            "whole_elf_or_cycle_saving_claimed":false},
        "elf_bytes":{"total":elf.len(),"text":text_bytes,"rodata":rodata_bytes,"other_sections":other_sections,"headers":headers,"padding":padding},
    }));
    oracle
}

#[test]
fn frozen_scalar_and_policy_corpus_has_reconciled_execution_attribution() {
    let mut runs = Vec::new();
    let scalar = cost_scalar::measure(&mut runs);
    let policy = cost_expanded::measure(&mut runs);
    for run in &mut runs {
        run["trace"] = TRACES.with(|all| all.borrow_mut().remove(run["name"].as_str().unwrap()).unwrap());
        let ranges = policy
            .iter()
            .find(|row| row["elf_sha256"] == run["elf_sha256"])
            .map(|row| row["policy_code_ranges"].as_array().unwrap().clone())
            .or_else(|| SCALAR_RANGES.with(|all| all.borrow().get(run["elf_sha256"].as_str().unwrap()).cloned()))
            .unwrap();
        let mut ranges: Vec<_> = ranges
            .iter()
            .map(|range| (range["start"].as_u64().unwrap(), range["end"].as_u64().unwrap(), range["kind"].as_str().unwrap()))
            .collect();
        ranges.sort_unstable();
        assert!(ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0), "shared text must be counted once");
        let mut classes: BTreeMap<String, [u64; 3]> = BTreeMap::new();
        for vm in run["trace"]["vms"].as_array().unwrap() {
            for (pc, cost) in vm["pcs"].as_object().unwrap() {
                let pc = pc.parse::<u64>().unwrap();
                let index = ranges.partition_point(|range| range.0 <= pc);
                let kind = index
                    .checked_sub(1)
                    .and_then(|index| ranges.get(index))
                    .filter(|range| pc < range.1)
                    .map_or("unmapped-trampoline-or-runtime", |range| range.2);
                let class = classes.entry(kind.into()).or_default();
                class[0] += cost["attempts"].as_u64().unwrap();
                class[1] += cost["instruction_cycles"].as_u64().unwrap();
                class[2] += cost["synchronous_syscall_cycles"].as_u64().unwrap();
            }
        }
        run["execution_classes"] = serde_json::to_value(classes).unwrap();
    }
    let report = json!({
        "schema":"cellscript-cost-attribution-v1", "compiler_version":env!("CARGO_PKG_VERSION"),
        "source":cost_provenance::capture_source(&repo_root()), "vm_configuration":cost_provenance::vm_configuration(),
        "cargo_lock_sha256":cost_provenance::sha256(&fs::read(repo_root().join("Cargo.lock")).unwrap()),
        "scope":"frozen-nine-scalar-and-41-expanded-policy-fixtures",
        "scalar":scalar,"policy":policy,"runs":runs,
        "execution_class_columns":["decoder_instruction_attempts","instruction_cycles","synchronous_syscall_cycles"],
        "trace_stack_scope":"observed per VM/generation from entry SP; distinct from scalar/policy static bounds",
        "unavailable":["host-syscall-memory-traffic","whole-transaction-trace","unexercised-paths"],
    });
    let output = repo_root().join("target/cellscript-cost/execution-attribution.json");
    fs::create_dir_all(output.parent().unwrap()).unwrap();
    fs::write(output, serde_json::to_vec(&report).unwrap()).unwrap();
}

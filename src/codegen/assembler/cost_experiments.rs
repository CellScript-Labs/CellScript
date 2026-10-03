//! Bounded experiments, never selected by the production emitter.
//! Use the actual immediate planner, ELF layout and pinned VM cost model.

use super::*;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use serde_json::{json, Value};

#[path = "../../../tests/support/cost_measurement.rs"]
#[allow(dead_code)]
mod measurement;

fn run(lines: &[String]) -> (usize, u64, i8) {
    let elf = assemble_elf_internal(lines).expect("experimental assembly");
    let size = elf.len();
    type Machine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;
    let core = <<Machine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 10_000_000);
    let mut machine = Machine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&Bytes::from(elf), std::iter::empty::<std::result::Result<Bytes, ckb_vm::Error>>()).unwrap();
    let exit = machine.run().expect("experimental VM execution");
    (size, machine.machine.cycles(), exit)
}

fn tree(tags: &[(u32, usize)], label: &str, lines: &mut Vec<String>) {
    lines.push(format!("{label}:"));
    if tags.is_empty() {
        lines.push("j unknown".into());
        return;
    }
    let mid = tags.len() / 2;
    let (tag, index) = tags[mid];
    lines.extend([format!("li t1, {tag}"), format!("beq t0, t1, selected_{index}")]);
    if tags.len() == 1 {
        lines.push("j unknown".into());
        return;
    }
    lines.extend([format!("bltu t0, t1, {label}_left"), format!("j {label}_right")]);
    tree(&tags[..mid], &format!("{label}_left"), lines);
    tree(&tags[mid + 1..], &format!("{label}_right"), lines);
}

fn dispatch(tags: &[u32], selected: u32, balanced: bool) -> (usize, u64, i8) {
    let mut lines = vec![".section .text".into(), ".global entry".into(), "entry:".into(), format!("li t0, {selected}")];
    if balanced {
        tree(&tags.iter().copied().enumerate().map(|(i, t)| (t, i)).collect::<Vec<_>>(), "dispatch", &mut lines);
    } else {
        for (index, tag) in tags.iter().enumerate() {
            lines.extend([format!("li t1, {tag}"), format!("beq t0, t1, selected_{index}")]);
        }
        lines.push("j unknown".into());
    }
    for index in 0..tags.len() {
        lines.extend([format!("selected_{index}:"), format!("li a0, {index}"), "ret".into()]);
    }
    lines.extend(["unknown:".into(), "li a0, 100".into(), "ret".into()]);
    run(&lines)
}

fn saved_selector(tags: &[u32], selected: u32, saved: bool, common_failure: bool) -> (usize, u64, i8) {
    let mut lines = vec![
        ".section .text".into(),
        ".global entry".into(),
        "entry:".into(),
        "addi sp, sp, -16".into(),
        format!("li t0, {selected}"),
        "sd t0, 0(sp)".into(),
    ];
    for (index, tag) in tags.iter().enumerate() {
        lines.extend([
            format!("li t1, {tag}"),
            format!("beq t0, t1, {}", if saved { format!("save_{index}") } else { "common".into() }),
        ]);
    }
    lines.push("j unknown".into());
    if saved {
        for index in 0..tags.len() {
            lines.extend([format!("save_{index}:"), format!("li t0, {index}"), "sd t0, 0(sp)".into(), "j common".into()]);
        }
    }
    lines.extend(["common:".into(), "li t0, 99".into(), "li t1, 98".into()]);
    if common_failure {
        lines.push("j common_failed".into());
    }
    lines.push("ld t0, 0(sp)".into());
    for (index, tag) in tags.iter().enumerate() {
        lines.extend([format!("li t1, {}", if saved { index as u32 } else { *tag }), format!("beq t0, t1, action_{index}")]);
    }
    lines.push("j unknown".into());
    for index in 0..tags.len() {
        lines.extend([format!("action_{index}:"), format!("li a0, {index}"), "j done".into()]);
    }
    lines.extend([
        "common_failed:".into(),
        "li a0, 101".into(),
        "j done".into(),
        "unknown:".into(),
        "li a0, 100".into(),
        "done:".into(),
        "addi sp, sp, 16".into(),
        "ret".into(),
    ]);
    run(&lines)
}

#[test]
fn saved_selector_measures_persistence_and_unknown_before_common_failure() {
    let mut rows = Vec::new();
    for count in [1, 2, 4, 8, 16, 32, 64] {
        for shape in ["dense", "sparse", "high"] {
            let tags: Vec<u32> = (0..count)
                .map(|i| match shape {
                    "dense" => i as u32,
                    "sparse" => (i as u32 + 1) * 7919,
                    _ => u32::MAX - (count - 1 - i) as u32,
                })
                .collect();
            for (position, tag) in tags.iter().copied().chain(std::iter::once(if shape == "dense" { u32::MAX } else { 0 })).enumerate()
            {
                for common_failure in [false, true] {
                    let baseline = saved_selector(&tags, tag, false, common_failure);
                    let saved = saved_selector(&tags, tag, true, common_failure);
                    let expected = if position == count {
                        100
                    } else if common_failure {
                        101
                    } else {
                        position as i8
                    };
                    assert_eq!((baseline.2, saved.2), (expected, expected));
                    rows.push(json!({"actions":count,"shape":shape,"position":position,"common_failure":common_failure,
                        "baseline":{"elf_bytes":baseline.0,"cycles":baseline.1},"saved":{"elf_bytes":saved.0,"cycles":saved.1}}));
                }
            }
        }
    }
    assert!(rows.iter().any(|r| r["saved"]["elf_bytes"].as_u64() > r["baseline"]["elf_bytes"].as_u64()));
    assert!(rows.iter().any(|r| r["saved"]["cycles"].as_u64() > r["baseline"]["cycles"].as_u64()));
    write(
        "saved-selector-experiment.json",
        json!({"schema":"cellscript-saved-selector-experiment-v1",
        "scope":"isolated two-pass routing with caller-owned selector and clobbering common-check surrogate",
        "decision":"retain-current-dispatch","reason":"saving an ordinal adds per-target code and early-path work; does not dominate existing dispatch under per-row no-regression policy",
        "rows":rows}),
    );
}

#[test]
fn measured_dispatch_frontier_retains_first_action_tradeoffs() {
    let mut rows = Vec::new();
    for count in [1, 2, 4, 8, 16, 32, 64] {
        for shape in ["dense", "sparse", "high"] {
            let tags: Vec<u32> = (0..count)
                .map(|i| match shape {
                    "dense" => i as u32,
                    "sparse" => (i as u32 + 1) * 7919,
                    _ => u32::MAX - (count - 1 - i) as u32,
                })
                .collect();
            for (index, selected) in
                tags.iter().copied().chain(std::iter::once(if shape == "dense" { u32::MAX } else { 0 })).enumerate()
            {
                let linear = dispatch(&tags, selected, false);
                let balanced = dispatch(&tags, selected, true);
                let expected = if index == count { 100 } else { index as i8 };
                assert_eq!(linear.2, expected);
                assert_eq!(balanced.2, expected);
                rows.push(json!({"actions":count,"shape":shape,"position":index,"unknown":index==count,
                    "linear":{"elf_bytes":linear.0,"cycles":linear.1}, "tree":{"elf_bytes":balanced.0,"cycles":balanced.1}}));
            }
        }
    }
    assert!(rows.iter().any(|r| r["tree"]["cycles"].as_u64() < r["linear"]["cycles"].as_u64()));
    assert!(rows.iter().any(|r| r["position"] == 0 && r["tree"]["cycles"].as_u64() > r["linear"]["cycles"].as_u64()));
    write(
        "dispatch-experiment.json",
        json!({"schema":"cellscript-dispatch-experiment-v1",
        "scope":"isolated tag routing, exact selected index and unknown rejection; excludes decoder/common-check costs",
        "decision":"retain-linear", "reason":"balanced routing trades first-action cycles and ELF bytes for late-action cycles; no uniformly dominating threshold",
        "rows":rows}),
    );
}

fn copying(width: usize, word: bool, offset: usize) -> (usize, u64, i8) {
    let mut lines = vec![
        ".section .text".into(),
        ".global entry".into(),
        "entry:".into(),
        "addi sp, sp, -512".into(),
        "la a0, source".into(),
        format!("addi a0, a0, {offset}"),
        "mv a1, sp".into(),
        format!("li a2, {width}"),
    ];
    if word {
        lines.extend([
            "li t1, 8".into(),
            "word_loop:".into(),
            "bltu a2, t1, byte_start".into(),
            "ld t0, 0(a0)".into(),
            "sd t0, 0(a1)".into(),
            "addi a0, a0, 8".into(),
            "addi a1, a1, 8".into(),
            "addi a2, a2, -8".into(),
            "j word_loop".into(),
        ]);
    }
    lines.extend([
        "byte_start:".into(),
        "beqz a2, copied".into(),
        "byte_loop:".into(),
        "lbu t0, 0(a0)".into(),
        "sb t0, 0(a1)".into(),
        "addi a0, a0, 1".into(),
        "addi a1, a1, 1".into(),
        "addi a2, a2, -1".into(),
        "bnez a2, byte_loop".into(),
        "copied:".into(),
        "la t1, source".into(),
        format!("addi t1, t1, {offset}"),
        "mv t2, sp".into(),
        format!("li t3, {width}"),
        "beqz t3, passed".into(),
        "verify_loop:".into(),
        "lbu t4, 0(t1)".into(),
        "lbu t5, 0(t2)".into(),
        "bne t4, t5, failed".into(),
        "addi t1, t1, 1".into(),
        "addi t2, t2, 1".into(),
        "addi t3, t3, -1".into(),
        "bnez t3, verify_loop".into(),
        "passed:".into(),
        "li a0, 0".into(),
        "addi sp, sp, 512".into(),
        "ret".into(),
        "failed:".into(),
        "li a0, 1".into(),
        "addi sp, sp, 512".into(),
        "ret".into(),
        ".section .rodata".into(),
        ".align 3".into(),
        "source:".into(),
    ]);
    for index in 0..520 {
        lines.push(format!(".byte {}", index % 251));
    }
    run(&lines)
}

#[test]
fn measured_word_copy_frontier_keeps_small_and_unaligned_cases() {
    let mut rows = Vec::new();
    for width in [0, 1, 7, 8, 9, 16, 31, 32, 33, 64, 128, 256] {
        for offset in 0..8 {
            let byte = copying(width, false, offset);
            let word = copying(width, true, offset);
            assert_eq!((byte.2, word.2), (0, 0));
            rows.push(json!({"width":width,"source_alignment_offset":offset,
                "byte":{"elf_bytes":byte.0,"cycles":byte.1},"word":{"elf_bytes":word.0,"cycles":word.1}}));
        }
    }
    assert!(rows.iter().any(|r| r["word"]["cycles"].as_u64() < r["byte"]["cycles"].as_u64()));
    assert!(rows.iter().any(|r| r["width"] == 0 && r["word"]["cycles"].as_u64() > r["byte"]["cycles"].as_u64()));
    write(
        "word-copy-experiment.json",
        json!({"schema":"cellscript-word-copy-experiment-v1",
        "scope":"disjoint source/destination synthetic VM copy; includes independent byte comparison; not transaction speedup",
        "decision":"reject-unconditional-shared-helper-replacement", "reason":"word loop grows helper and regresses small copies; overlap semantics require a separately proven specialization",
        "rows":rows}),
    );
}

fn write(name: &str, mut report: Value) {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources: BTreeMap<_, _> = [
        "Cargo.lock",
        "rust-toolchain.toml",
        "src/codegen/assembler.rs",
        "src/codegen/assembler/immediate.rs",
        "src/codegen/assembler/cost_experiments.rs",
    ]
    .into_iter()
    .map(|path| (path, hex::encode(Sha256::digest(std::fs::read(root.join(path)).unwrap()))))
    .collect();
    report["source_sha256"] = serde_json::to_value(sources).unwrap();
    report["vm_configuration"] =
        json!({"ckb_vm":"0.24.14","ckb_script":"1.1.0","version":2,"isa":"IMC|B|MOP","cycle_limit":10_000_000});
    let output = root.join("target/cellscript-cost").join(name);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}

/// Lower-bound staged-loader probe. It measures the real scheduler/syscall
/// charges for reading the header, optional-field length prefixes and selected
/// field. It omits canonical decoding deliberately: any admitted design must
/// still add those checks, so this cannot establish its acceptance behavior.
#[test]
fn staged_witness_loading_measures_syscall_tradeoff_with_optional_fields() {
    use ckb_testtool::{
        builtin::ALWAYS_SUCCESS,
        ckb_script::ScriptGroupType,
        ckb_types::{
            bytes::Bytes,
            core::TransactionBuilder,
            packed::{CellInput, CellOutput, WitnessArgs},
            prelude::*,
        },
        context::Context,
    };
    let mut rows = Vec::new();
    for width in [8usize, 32, 128] {
        for optional in [0usize, 64, 1024, 1800] {
            let witness = WitnessArgs::new_builder()
                .lock(Some(Bytes::from(vec![1; optional])).pack())
                .input_type(Some(Bytes::from(vec![2; width])).pack())
                .output_type(Some(Bytes::from(vec![3; optional])).pack())
                .build()
                .as_bytes();
            assert!(witness.len() <= 4096);
            for staged in [false, true] {
                let frame = if staged { 160 } else { 4112 };
                let reads = if staged {
                    vec![(0, 16), (16, 4), (20 + optional, width + 4), (24 + optional + width, 4)]
                } else {
                    vec![(0, 4096)]
                };
                let mut lines = vec![
                    ".section .text".into(),
                    ".global entry".into(),
                    "entry:".into(),
                    format!("li t0, {frame}"),
                    "sub sp, sp, t0".into(),
                ];
                for (offset, capacity) in &reads {
                    lines.extend([
                        format!("li t0, {capacity}"),
                        "sd t0, 0(sp)".into(),
                        "addi a0, sp, 8".into(),
                        "mv a1, sp".into(),
                        format!("li a2, {offset}"),
                        "li a3, 0".into(),
                        format!("li a4, {}", crate::ckb_abi::source::GROUP_INPUT),
                        "li a7, 2074".into(),
                        "ecall".into(),
                        "bnez a0, failed".into(),
                        "ld t0, 0(sp)".into(),
                        format!("li t1, {}", witness.len() - offset),
                        "bne t0, t1, failed".into(),
                    ]);
                }
                lines.extend([
                    "li a0, 0".into(),
                    "j done".into(),
                    "failed:".into(),
                    "li a0, 1".into(),
                    "done:".into(),
                    format!("li t0, {frame}"),
                    "add sp, sp, t0".into(),
                    "ret".into(),
                ]);
                let elf = assemble_elf_internal(&lines).unwrap();
                let mut context = Context::default();
                let code = context.deploy_cell(Bytes::from(elf.clone()));
                let always = context.deploy_cell(ALWAYS_SUCCESS.clone());
                let lock = context.build_script(&always, Bytes::new()).unwrap();
                let script = context.build_script(&code, Bytes::new()).unwrap();
                let cell =
                    CellOutput::new_builder().capacity(100_000_000_000u64).lock(lock).type_(Some(script.clone()).pack()).build();
                let previous = context.create_cell(cell.clone(), Bytes::new());
                let tx = context.complete_tx(
                    TransactionBuilder::default()
                        .input(CellInput::new_builder().previous_output(previous).build())
                        .output(cell)
                        .output_data(Bytes::new().pack())
                        .witness(witness.pack())
                        .build(),
                );
                context.verify_tx(&tx, 10_000_000).unwrap();
                let group = measurement::measure_group(&context, &tx, &script, ScriptGroupType::Type, 10_000_000);
                rows.push(json!({"width":width,"optional_field_bytes_each":optional,"witness_bytes":witness.len(),"staged":staged,
                    "elf_bytes":elf.len(),"static_frame_bytes":frame,"load_calls":reads.len(),"group_cycles":group.required_cycles()}));
            }
        }
    }
    write(
        "staged-loader-experiment.json",
        json!({"schema":"cellscript-staged-loader-experiment-v1",
        "scope":"real CKB scheduler and LOAD_WITNESS calls; known field offsets and lengths; excludes canonical decoder work",
        "decision":"retain-full-bounded-load","reason":"staging trades reserved stack and transferred bytes against extra syscalls; does not establish canonical runtime offset/late-record validation",
        "rows":rows}),
    );
}

#[test]
fn isolated_compressed_probe_executes_but_current_checker_rejects_it() {
    let lines: Vec<String> =
        [".section .text", ".global entry", "entry:", "li a0, 0", "ret"].into_iter().map(str::to_string).collect();
    let mut elf = assemble_elf_internal(&lines).unwrap();
    let parsed = cellscript_artifact_checker::parse_elf(&elf, 100).unwrap();
    let instructions = &parsed.instructions;
    let pair = instructions.windows(2).find(|pair| pair[0].word == 0x00000513 && pair[1].word == 0x00008067).unwrap();
    let offset = (parsed.text.offset + pair[0].address - parsed.text.address) as usize;
    // Exact RV64 C.LI a0,0 and C.JR ra. Retain the old file layout: this
    // probes decoding/control flow, not production relaxation or ELF savings.
    elf[offset..offset + 4].copy_from_slice(&[0x01, 0x45, 0x82, 0x80]);
    assert!(cellscript_artifact_checker::parse_elf(&elf, 100).is_err());
    type Machine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;
    let core = <<Machine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 10_000_000);
    let mut machine = Machine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    let size = elf.len();
    machine.load_program(&Bytes::from(elf), std::iter::empty::<std::result::Result<Bytes, ckb_vm::Error>>()).unwrap();
    assert_eq!(machine.run().unwrap(), 0);
    let baseline = run(&lines);
    write(
        "compressed-probe.json",
        json!({"schema":"cellscript-compressed-probe-v1",
        "scope":"two exact compressed instructions inside unchanged ELF layout; independent checker deliberately rejects mixed width",
        "baseline":{"elf_bytes":baseline.0,"cycles":baseline.1},"mixed":{"elf_bytes":size,"cycles":machine.machine.cycles()},
        "decision":"defer-production-support","blocker":"assembler sizing/relaxation, four-byte checker ranges and source maps require a coordinated versioned migration"}),
    );
}

fn borrowed_adapter(width: usize, offset: usize, borrowed: bool) -> (usize, u64, i8) {
    let frame = if borrowed { 32 } else { (width + 32 + 15) & !15 };
    let mut lines: Vec<String> = [".section .text", ".global entry", "entry:", "addi sp, sp, -352", "sd ra, 344(sp)", "la t0, source"]
        .into_iter()
        .map(str::to_string)
        .collect();
    lines.extend([
        format!("addi t1, sp, {}", 32 + offset),
        format!("li t2, {width}"),
        "beqz t2, parent_ready".into(),
        "fill_parent:".into(),
        "lbu t3, 0(t0)".into(),
        "sb t3, 0(t1)".into(),
        "addi t0, t0, 1".into(),
        "addi t1, t1, 1".into(),
        "addi t2, t2, -1".into(),
        "bnez t2, fill_parent".into(),
        "parent_ready:".into(),
        format!("addi a0, sp, {}", 32 + offset),
        "call adapter".into(),
        "ld ra, 344(sp)".into(),
        "addi sp, sp, 352".into(),
        "ret".into(),
        "adapter:".into(),
        format!("addi sp, sp, -{frame}"),
        "sd ra, 16(sp)".into(),
    ]);
    if !borrowed {
        lines.extend([
            "mv t0, a0".into(),
            "addi t1, sp, 32".into(),
            format!("li t2, {width}"),
            "beqz t2, private_ready".into(),
            "copy_private:".into(),
            "lbu t3, 0(t0)".into(),
            "sb t3, 0(t1)".into(),
            "addi t0, t0, 1".into(),
            "addi t1, t1, 1".into(),
            "addi t2, t2, -1".into(),
            "bnez t2, copy_private".into(),
            "private_ready:".into(),
            "addi a0, sp, 32".into(),
        ]);
    }
    // The parent stays live. Durable pointer state is owned by the adapter,
    // and the outgoing argument and nested frame live below that storage.
    lines.extend([
        "sd a0, 0(sp)".into(),
        "li t0, 777".into(),
        "addi sp, sp, -16".into(),
        "sd t0, 0(sp)".into(),
        "call nested".into(),
        "addi sp, sp, 16".into(),
        "ld t0, 0(sp)".into(),
        "la t1, source".into(),
        format!("li t2, {width}"),
        "beqz t2, verified".into(),
        "verify_span:".into(),
        "lbu t3, 0(t0)".into(),
        "lbu t4, 0(t1)".into(),
        "bne t3, t4, failed".into(),
        "addi t0, t0, 1".into(),
        "addi t1, t1, 1".into(),
        "addi t2, t2, -1".into(),
        "bnez t2, verify_span".into(),
        "verified:".into(),
        "li a0, 0".into(),
        "j restore".into(),
        "failed:".into(),
        "li a0, 1".into(),
        "restore:".into(),
        "ld ra, 16(sp)".into(),
        format!("addi sp, sp, {frame}"),
        "ret".into(),
        "nested:".into(),
        "addi sp, sp, -48".into(),
        "li t0, 11".into(),
        "sd t0, 0(sp)".into(),
        "li a0, 12".into(),
        "li t1, 13".into(),
        "addi sp, sp, 48".into(),
        "ret".into(),
        ".section .rodata".into(),
        "source:".into(),
    ]);
    for index in 0..256 {
        lines.push(format!(".byte {}", index % 251));
    }
    run(&lines)
}

#[test]
fn borrowed_span_probe_keeps_parent_live_across_nested_and_outgoing_frames() {
    let mut rows = Vec::new();
    for width in [0, 1, 8, 32, 128, 256] {
        for offset in [0, 1, 7] {
            let private = borrowed_adapter(width, offset, false);
            let borrowed = borrowed_adapter(width, offset, true);
            assert_eq!((private.2, borrowed.2), (0, 0));
            rows.push(json!({"width":width,"parent_alignment_offset":offset,
                "private":{"elf_bytes":private.0,"cycles":private.1,"static_call_chain_bytes":352+((width+32+15)&!15)+16+48},
                "borrowed":{"elf_bytes":borrowed.0,"cycles":borrowed.1,"static_call_chain_bytes":352+32+16+48}}));
        }
    }
    write(
        "borrowed-span-experiment.json",
        json!({"schema":"cellscript-borrowed-span-experiment-v1",
        "scope":"read-only adapter with live caller buffer, clobbering nested callee and separate outgoing reservation; not a canonical WitnessArgs decoder",
        "decision":"defer-production-borrowing","blocker":"general adapter/checker span lifetime and alias contracts are absent; this fixture cannot authorize arbitrary callees or overlapping normalization",
        "rows":rows}),
    );
}

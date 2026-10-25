use super::*;
use ckb_testtool::{
    ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*},
    context::Context,
};
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::*, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, Syscalls, TraceMachine, WXorXMemory, ISA_B,
    ISA_IMC, ISA_MOP,
};
use std::sync::{Arc, Mutex};

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

fn program(source: u64, bound: Option<usize>, expected: usize, offset: usize) -> Vec<u8> {
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.emit(".section .text");
    generator.emit(".global entry");
    generator.emit_label("entry");
    generator.emit_large_addi("sp", "sp", -((offset + 64) as i64));
    generator.emit(format!("li t0, {source}"));
    generator.emit_stack_store("t0", offset);
    generator.emit_cell_source_count_scan(
        CellScanFrame { source: offset, index: offset + 8, length: offset + 16, buffer: offset + 24 },
        bound,
        "invalid",
        "over_bound",
    );
    generator.emit(format!("li t0, {expected}"));
    generator.emit("bne a0, t0, wrong_count");
    for (label, status) in [("success", 0), ("invalid", 44), ("over_bound", 21), ("wrong_count", 127)] {
        generator.emit_label(label);
        generator.emit(format!("li a0, {status}"));
        generator.emit(format!("li a7, {}", ckb_abi::syscall::EXIT));
        generator.emit("ecall");
        generator.emit(format!("j {label}"));
    }
    assemble_generated_elf(&generator.assembly).unwrap()
}

#[derive(Clone, Copy)]
struct Fault {
    index: usize,
    status: u64,
    length: u64,
}

/// Fault injection complements real CKB execution below; it is deliberately
/// not the evidence for CKB source/group semantics.
struct CapacityProbe {
    source: u64,
    count: usize,
    fault: Option<Fault>,
    probes: Arc<Mutex<Vec<usize>>>,
}

impl Syscalls<Core> for CapacityProbe {
    fn initialize(&mut self, _: &mut Core) -> std::result::Result<(), Error> {
        Ok(())
    }

    fn ecall(&mut self, machine: &mut Core) -> std::result::Result<bool, Error> {
        if machine.registers()[A7] != ckb_abi::syscall::LOAD_CELL_BY_FIELD {
            return Ok(false);
        }
        assert_eq!(machine.registers()[A4], self.source);
        assert_eq!(machine.registers()[A5], ckb_abi::cell_field::CAPACITY);
        assert_eq!(machine.registers()[A2], 0);
        let index = machine.registers()[A3] as usize;
        self.probes.lock().unwrap().push(index);
        let size_address = machine.registers()[A1];
        let buffer_address = machine.registers()[A0];
        assert_eq!(machine.memory_mut().load64(&size_address)?, 8);
        let (status, length) = match self.fault.filter(|fault| fault.index == index) {
            Some(fault) => (fault.status, fault.length),
            None if index == self.count => (ckb_abi::syscall_error::INDEX_OUT_OF_BOUND, 8),
            None => {
                assert!(index < self.count, "scan went past its first end probe");
                (0, 8)
            }
        };
        machine.memory_mut().store64(&size_address, &length)?;
        machine.memory_mut().store64(&buffer_address, &100_000_000_000)?;
        // No accidental reliance on temporaries surviving a syscall.
        for reg in [T0, T1, T2, T3, T4, T5, T6, A1, A2, A3, A4, A5, A6, A7] {
            machine.set_register(reg, 0xfeed);
        }
        machine.set_register(A0, status);
        Ok(true)
    }
}

fn run(source: u64, bound: Option<usize>, count: usize, fault: Option<Fault>, offset: usize) -> (i8, Vec<usize>, u64) {
    let probes = Arc::new(Mutex::new(Vec::new()));
    let syscall = CapacityProbe { source, count, fault, probes: probes.clone() };
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 100_000);
    let mut machine = TraceMachine::new(
        DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).syscall(Box::new(syscall)).build(),
    );
    machine
        .load_program(&Bytes::from(program(source, bound, count, offset)), std::iter::empty::<std::result::Result<Bytes, Error>>())
        .unwrap();
    let status = machine.run().expect("complete bounded scan must not trap");
    let observed = probes.lock().unwrap().clone();
    (status, observed, machine.machine.cycles())
}

#[test]
fn bounded_cell_scan_probes_every_index_and_the_exact_limit() {
    for source in [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT] {
        for bound in 1..=64 {
            for count in [0, bound - 1, bound, bound + 1] {
                let (status, probes, _) = run(source, Some(bound), count, None, 0);
                assert_eq!(status, if count > bound { 21 } else { 0 }, "source={source} bound={bound} count={count}");
                assert_eq!(probes, (0..=count.min(bound)).collect::<Vec<_>>());
            }
        }
    }
}

#[test]
fn bounded_cell_scan_rejects_non_end_errors_and_inexact_capacity_fields() {
    for index in [0, 31, 64] {
        for status in [2, 3, 4, 255, u64::MAX] {
            let result = run(CKB_SOURCE_INPUT, Some(64), 64, Some(Fault { index, status, length: 8 }), 0);
            assert_eq!(result.0, 44);
            assert_eq!(result.1, (0..=index).collect::<Vec<_>>());
        }
        for length in [0, 7, 9, u64::MAX] {
            let result = run(CKB_SOURCE_OUTPUT, Some(64), 64, Some(Fault { index, status: 0, length }), 0);
            assert_eq!(result.0, 44);
            assert_eq!(result.1, (0..=index).collect::<Vec<_>>());
        }
    }
}

#[test]
fn bounded_cell_scan_preserves_state_with_large_frame_offsets_and_unbounded_legacy_count() {
    for source in [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT, CKB_SOURCE_CELL_DEP, CKB_SOURCE_GROUP_INPUT, CKB_SOURCE_GROUP_OUTPUT] {
        let bounded = run(source, Some(64), 64, None, 4096);
        assert_eq!(bounded.0, 0);
        assert_eq!(bounded.1, (0..=64).collect::<Vec<_>>());
        let legacy = run(source, None, 65, None, 4096);
        assert_eq!(legacy.0, 0);
        assert_eq!(legacy.1, (0..=65).collect::<Vec<_>>());
    }
}

fn verify_transaction(source: u64, bound: usize, count: usize) -> std::result::Result<u64, String> {
    let mut context = Context::new_with_deterministic_rng();
    let code = context.deploy_cell(Bytes::from(program(source, Some(bound), count, 0)));
    let type_script = context.build_script(&code, Bytes::new()).unwrap();
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    for line in [".section .text", ".global entry", "entry:", "li a0, 0", "li a7, 93", "ecall"] {
        generator.emit(line);
    }
    let lock_code = context.deploy_cell(Bytes::from(assemble_generated_elf(&generator.assembly).unwrap()));
    let lock = context.build_script(&lock_code, Bytes::new()).unwrap();
    let inputs = if source == CKB_SOURCE_INPUT { count } else { 1 };
    let outputs = if source == CKB_SOURCE_OUTPUT { count } else { 1 };
    let mut transaction = TransactionBuilder::default();
    for index in 0..inputs {
        // Only the first Cell is in the executing Type group. The rest lack
        // Type entirely and still consume whole-transaction scan budget.
        let cell = packed::CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(lock.clone())
            .type_((index == 0).then(|| type_script.clone()).pack())
            .build();
        let out_point = context.create_cell(cell, Bytes::new());
        transaction = transaction.input(packed::CellInput::new_builder().previous_output(out_point).build());
    }
    for index in 0..outputs {
        let cell = packed::CellOutput::new_builder()
            .capacity(100_000_000_000u64)
            .lock(lock.clone())
            .type_((index == 0).then(|| type_script.clone()).pack())
            .build();
        transaction = transaction.output(cell).output_data(Bytes::new().pack());
    }
    let transaction = context.complete_tx(transaction.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

#[test]
fn bounded_cell_scan_real_ckb_counts_untyped_cells_and_distinguishes_global_sources() {
    for source in [CKB_SOURCE_INPUT, CKB_SOURCE_OUTPUT] {
        for bound in [1, 32, 64] {
            for count in [0, bound, bound + 1] {
                let result = verify_transaction(source, bound, count);
                if count <= bound {
                    assert!(result.is_ok(), "source={source} bound={bound} count={count}: {result:?}");
                } else {
                    let error = result.unwrap_err();
                    assert!(error.contains("ValidationFailure: see error code 21 "), "{error}");
                }
            }
        }
    }
}

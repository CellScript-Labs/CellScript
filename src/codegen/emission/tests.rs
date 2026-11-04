use super::*;
use crate::codegen::value_arena::WriteGuard;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::S0, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC,
    ISA_MOP,
};

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

#[test]
fn guarded_store_widths_enforce_both_edges_and_preserve_live_registers_in_vm() {
    const START: usize = 4096;
    const END: usize = START + 32;
    for op in [StoreOp::Byte, StoreOp::Half, StoreOp::Word, StoreOp::Double] {
        let width = op.width();
        for target in [START - 1, START, END - width, END - width + 1] {
            for offset in [0, -4096, 4096] {
                for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()] {
                    let allowed = (START..=END - width).contains(&target);
                    let mut g = CodeGenerator::new(CodegenOptions { diagnostics, ..Default::default() });
                    g.emit(".section .text");
                    g.emit(".global entry");
                    g.emit_label("entry");
                    g.emit_stack_adjustment(-8192);
                    g.emit("mv s0, sp");
                    g.emit(format!("li t0, {}", 0xa5a5_a5a5_a5a5_a5a5u64 as i64));
                    for slot in (START - 16..END + 16).step_by(8) {
                        g.emit_stack_store("t0", slot);
                    }
                    g.emit_large_addi("t4", "sp", target as i64 - offset);
                    g.emit("li t6, 4660");
                    g.emit("li t5, 22136");
                    g.emit("li a7, 13398");
                    g.emit("li a6, 8608480567731124087");
                    g.allocated_write_guard = Some(WriteGuard { start: START, end: END, failure: "outside".into() });
                    // x16 aliases a6, which is first in the guard's scratch
                    // candidates. Its source value must survive scratch choice.
                    g.emit_store(op, "x16", "t4", offset);
                    g.allocated_write_guard = None;
                    for (register, expected) in [("t6", 4660), ("t5", 22136), ("a7", 13398)] {
                        g.emit(format!("li t0, {expected}"));
                        g.emit(format!("bne {register}, t0, clobbered"));
                    }
                    g.emit("li a0, 0");
                    g.emit("j done");
                    g.emit_label("clobbered");
                    g.emit("li a0, 99");
                    g.emit("j done");
                    g.emit_label("outside");
                    g.emit(format!("li a0, {}", CellScriptRuntimeError::CollectionBoundsInvalid.code()));
                    g.emit_label("done");
                    g.emit("li a7, 93");
                    g.emit("ecall");
                    assert!(g.emission_error.is_none(), "{:?}", g.emission_error);
                    if diagnostics.is_standard() {
                        eliminate_immediate_stack_reloads(&mut g.assembly);
                    }
                    let elf = assemble_generated_elf(&g.assembly).unwrap();
                    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 1_000_000);
                    let mut machine =
                        TraceMachine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
                    machine
                        .load_program(&ckb_vm::Bytes::from(elf), std::iter::empty::<std::result::Result<ckb_vm::Bytes, Error>>())
                        .unwrap();
                    let status = machine.run().unwrap();
                    assert_eq!(
                        status,
                        if allowed { 0 } else { CellScriptRuntimeError::CollectionBoundsInvalid.code() as i8 },
                        "{op:?} target={target} offset={offset} {diagnostics}"
                    );
                    let base = machine.machine.registers()[S0];
                    let memory = machine.machine.memory_mut().load_bytes(base + (START - 16) as u64, 64).unwrap();
                    let mut expected = vec![165u8; 64];
                    if allowed {
                        expected[target - (START - 16)..target - (START - 16) + width].fill(119);
                    }
                    assert_eq!(memory.as_ref(), expected, "guard must reject before any out-of-region write");
                }
            }
        }
    }
}

#[test]
fn structured_operands_reject_unrepresentable_immediates_and_bad_registers() {
    for kind in 0..3 {
        let mut g = CodeGenerator::new(CodegenOptions::default());
        match kind {
            0 => g.emit_load(LoadOp::Double, "t0", "t1", u64::MAX),
            1 => g.emit_store(StoreOp::Double, "t0", "t1", u64::MAX),
            _ => g.emit_add_immediate("t0", "t1", u64::MAX),
        }
        assert!(g.assembly.is_empty());
        assert!(g.emission_error.unwrap().contains("signed 64-bit"));
    }
    let mut g = CodeGenerator::new(CodegenOptions::default());
    g.emit_load(LoadOp::Double, "a8", "sp", 0);
    assert!(g.assembly.is_empty());
    assert!(g.emission_error.unwrap().contains("register operand"));
}

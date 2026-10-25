use super::*;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::S0, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC,
    ISA_MOP,
};

#[path = "../../../tests/support/value_arena_reference.rs"]
pub(super) mod reference;

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;
const SLOTS: usize = 8;

#[derive(Clone, Copy, Debug)]
enum Op {
    Allocate(usize, u64),
    Resize(usize, usize, u64),
    Release(usize),
    Fill(usize, u8),
    Shift(usize, usize, usize),
}

fn run(arena: Arena, operations: &[Op], corrupt: Option<(usize, u64)>) -> (u64, usize) {
    let mut expected = reference::Arena::new(arena.bytes, arena.allocations);
    let mut slots = [None; SLOTS];
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    let end = arena.offset + STATE + arena.bytes;
    let frame = end + 16;
    assert!(arena.offset >= SLOTS * 8 + 16);
    generator.emit(".section .text");
    generator.emit(".global entry");
    generator.emit_label("entry");
    generator.emit_large_addi("sp", "sp", -(frame as i64));
    generator.emit("mv s0, sp");
    // Dirty storage proves initialization, including padding, is real.
    generator.emit("mv t0, sp");
    generator.emit(format!("li t1, {frame}"));
    generator.emit("li t2, 165");
    generator.emit_label("dirty_frame");
    generator.emit("sb t2, 0(t0)");
    generator.emit("addi t0, t0, 1");
    generator.emit("addi t1, t1, -1");
    generator.emit("bnez t1, dirty_frame");
    for slot in 0..SLOTS {
        generator.emit_stack_store("zero", slot * 8);
    }
    generator.emit_value_arena_init(arena);
    let mut failed = false;
    for &op in operations {
        let result = match op {
            Op::Allocate(slot, size) => {
                generator.emit(format!("li a0, {}", size as i64));
                generator.emit_value_arena_allocate(arena);
                generator.emit_stack_store("a0", slot * 8);
                expected.allocate(size).map(|pointer| slots[slot] = pointer)
            }
            Op::Resize(dst, src, size) => {
                generator.emit_stack_load("a0", src * 8);
                generator.emit(format!("li a1, {}", size as i64));
                generator.emit_value_arena_resize(arena);
                generator.emit_stack_store("a0", dst * 8);
                expected.resize(slots[src], size).map(|pointer| slots[dst] = pointer)
            }
            Op::Release(slot) => {
                generator.emit_stack_load("a0", slot * 8);
                generator.emit_value_arena_release(arena);
                expected.release(slots[slot])
            }
            Op::Fill(slot, byte) => {
                let length = expected.fill(slots[slot], byte);
                generator.emit_stack_load("t0", slot * 8);
                generator.emit(format!("li t1, {length}"));
                generator.emit(format!("li t2, {byte}"));
                let label = generator.fresh_label("fill_payload");
                generator.emit_label(&label);
                generator.emit("sb t2, 0(t0)");
                generator.emit("addi t0, t0, 1");
                generator.emit("addi t1, t1, -1");
                generator.emit(format!("bnez t1, {label}"));
                Ok(())
            }
            Op::Shift(dst, src, offset) => {
                generator.emit_stack_load("a0", src * 8);
                generator.emit_large_addi("a0", "a0", offset as i64);
                generator.emit_stack_store("a0", dst * 8);
                slots[dst] = Some(slots[src].unwrap() + offset);
                Ok(())
            }
        };
        if result.is_err() {
            failed = true;
            break;
        }
    }
    if let Some((offset, value)) = corrupt {
        assert!(!failed);
        generator.emit(format!("li t0, {}", value as i64));
        generator.emit_stack_store("t0", arena.offset + offset);
        generator.emit_stack_load("a0", 0);
        generator.emit_value_arena_release(arena);
        failed = true;
    }
    generator.emit("li a0, 0");
    generator.emit("li a7, 93");
    generator.emit("ecall");
    generator.emit_process_failure_helper();
    let elf = assemble_generated_elf(&generator.assembly).unwrap();
    let elf_len = elf.len();
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 20_000_000);
    let mut machine = TraceMachine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&ckb_vm::Bytes::from(elf), std::iter::empty::<std::result::Result<ckb_vm::Bytes, Error>>()).unwrap();
    assert_eq!(machine.run().expect("arena must terminate without a memory trap"), if failed { 21 } else { 0 }, "{operations:?}");
    let base = machine.machine.registers()[S0];
    let bytes = machine.machine.memory_mut().load_bytes(base, frame as u64).unwrap();
    let word = |offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
    for (slot, pointer) in slots.iter().enumerate() {
        assert_eq!(word(slot * 8), pointer.map_or(0, |offset| base + (arena.offset + STATE + offset) as u64), "slot {slot}");
    }
    assert!(bytes[SLOTS * 8..arena.offset].iter().all(|byte| *byte == 165), "prefix guard changed");
    assert!(bytes[end..].iter().all(|byte| *byte == 165), "suffix guard changed");
    if corrupt.is_none() {
        assert_eq!(word(arena.offset), expected.used as u64);
        assert_eq!(word(arena.offset + 8), expected.records.len() as u64);
        for (ordinal, record) in expected.records.iter().enumerate() {
            let payload = arena.offset + STATE + record.offset;
            assert_eq!(word(payload - 32), record.data.len() as u64);
            assert_eq!(word(payload - 24), record.requested as u64);
            assert_eq!(word(payload - 16), u64::from(record.live));
            assert_eq!(word(payload - 8), ordinal as u64 + 1);
            assert_eq!(&bytes[payload..payload + record.data.len()], record.data);
        }
        assert!(bytes[arena.offset + STATE + expected.used..end].iter().all(|byte| *byte == 165), "uncharged storage changed");
    }
    (machine.machine.cycles(), elf_len)
}

fn prefixes(arena: Arena, operations: &[Op]) {
    for length in 0..=operations.len() {
        run(arena, &operations[..length], None);
    }
}

#[test]
fn value_arena_shared_growth_shrink_release_and_padding_match_model() {
    use Op::*;
    for offset in [80, 4096, 65536] {
        prefixes(
            Arena::new(offset, 4096, 32).unwrap(),
            &[
                Allocate(0, 0),
                Release(0),
                Resize(0, 0, 17),
                Fill(0, 0xab),
                Allocate(1, 31),
                Fill(1, 0xcd),
                Resize(2, 0, 65),
                Resize(3, 1, 1),
                Fill(2, 0xef),
                Release(2),
                Resize(3, 3, 0),
                Allocate(4, 16),
            ],
        );
    }
}

#[test]
fn value_arena_maximum_bytes_events_and_exhaustion_match_model() {
    use Op::*;
    for size in [0, 1, 15, 16, 17, 31, 32, 33, 65504, 65505, u64::MAX] {
        run(Arena::new(80, 65536, 256).unwrap(), &[Allocate(0, size)], None);
    }
    for sequence in [
        vec![Allocate(0, 32), Fill(0, 73), Resize(1, 0, 1)],
        vec![Allocate(0, 32), Release(0), Allocate(1, 1)],
        vec![Allocate(0, 17), Fill(0, 91), Resize(1, 0, u64::MAX)],
    ] {
        prefixes(Arena::new(80, 64, 1).unwrap(), &sequence);
    }
    let mut events = Vec::new();
    for _ in 0..256 {
        events.push(Allocate(0, 1));
        events.push(Release(0));
    }
    let arena = Arena::new(4096, 16384, 256).unwrap();
    let (cycles, elf_bytes) = run(arena, &events, None);
    eprintln!("arena 256 allocation/release events: cycles={cycles}, elf={elf_bytes}");
    events.push(Allocate(1, 1));
    run(arena, &events, None);
}

#[test]
fn value_arena_stale_interior_and_foreign_pointers_fail() {
    use Op::*;
    for suffix in [
        vec![Release(0), Release(0)],
        vec![Resize(1, 0, 16), Resize(2, 0, 32)],
        vec![Resize(1, 0, 0), Release(0)],
        vec![Shift(1, 0, 1), Release(1)],
        vec![Shift(1, 0, 16), Release(1)],
        vec![Shift(1, 0, 4096), Resize(2, 1, 16)],
    ] {
        let mut sequence = vec![Allocate(0, 64), Fill(0, 81)];
        sequence.extend(suffix);
        prefixes(Arena::new(4096, 4096, 32).unwrap(), &sequence);
    }
}

#[test]
fn value_arena_corrupt_state_and_headers_fail_before_release() {
    for (offset, value) in [
        (0, 1),
        (0, 4097),
        (0, u64::MAX),
        (8, 0),
        (8, 33),
        (32, 0),
        (32, 17),
        (32, u64::MAX),
        (40, 0),
        (40, 17),
        (40, u64::MAX),
        (48, 0),
        (48, 2),
        (48, u64::MAX),
        (56, 0),
        (56, 2),
    ] {
        run(Arena::new(4096, 4096, 32).unwrap(), &[Op::Allocate(0, 1)], Some((offset, value)));
    }
}

#[test]
fn value_arena_static_bounds_reject_overflow_and_misalignment() {
    for (offset, bytes, events) in
        [(1, 64, 1), (0, 63, 1), (0, 65, 1), (0, 65552, 1), (0, 64, 0), (0, 64, 257), (usize::MAX - 15, 64, 1), (1024 * 1024, 64, 1)]
    {
        assert!(Arena::new(offset, bytes, events).is_err());
    }
    assert!(Arena::new(1024 * 1024 - 32 - 65536, 65536, 256).is_ok());
}

#[test]
fn value_arena_generated_sequences_match_model() {
    use Op::*;
    for seed in 1..=24u64 {
        let mut state = seed;
        let mut operations = Vec::new();
        for _ in 0..40 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let slot = (state >> 32) as usize % SLOTS;
            let size = (state >> 8) % 97;
            match state % 4 {
                0 => operations.push(Allocate(slot, size)),
                1 | 2 => operations.push(Resize(slot, slot, size)),
                _ => {
                    operations.push(Release(slot));
                    operations.push(Allocate(slot, 0));
                }
            }
        }
        run(Arena::new(4096, 4096, 32).unwrap(), &operations, None);
    }
}

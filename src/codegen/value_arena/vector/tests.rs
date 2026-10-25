use super::*;
use crate::codegen::value_arena::tests::reference;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::S0, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC,
    ISA_MOP,
};

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;
const SOURCE: usize = 0;
const OUTPUT: usize = 64;

#[derive(Clone, Copy, Debug)]
enum Op {
    Push(usize, u8, usize),
    Reserve(usize, u64),
    Get(usize, u64),
    Remove(usize, u64),
    Set(usize, u64, u8),
    Insert(usize, u64, u8),
    Drop(usize),
}

fn run(width: usize, elements: usize, bytes: usize, events: usize, operations: &[Op]) -> (i8, u64, usize) {
    let arena = Arena::new(4096, bytes, events).unwrap();
    let vectors = [Vector::new(arena, 256, width, elements).unwrap(), Vector::new(arena, 288, width, elements).unwrap()];
    let mut model = reference::Arena::new(bytes, events);
    let mut expected = [reference::Vector::new(width, elements), reference::Vector::new(width, elements)];
    let mut output = vec![165; width];
    let end = arena.offset + STATE + arena.bytes;
    let frame = end + 16;
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.emit(".section .text");
    generator.emit(".global entry");
    generator.emit_label("entry");
    generator.emit_large_addi("sp", "sp", -(frame as i64));
    generator.emit("mv s0, sp");
    generator.emit("mv t0, sp");
    generator.emit(format!("li t1, {frame}"));
    generator.emit("li t2, 165");
    generator.emit_label("dirty_frame");
    generator.emit("sb t2, 0(t0)");
    generator.emit("addi t0, t0, 1");
    generator.emit("addi t1, t1, -1");
    generator.emit("bnez t1, dirty_frame");
    generator.emit_value_arena_init(arena);
    for vector in vectors {
        generator.emit_value_vector_init(vector);
    }
    let mut failed = false;
    for &operation in operations {
        let result = match operation {
            Op::Push(index, byte, count) => {
                assert!(count > 0);
                generator.emit(format!("li t0, {byte}"));
                for offset in SOURCE..SOURCE + width {
                    generator.emit_stack_store_byte("t0", offset);
                }
                generator.emit(format!("li s1, {count}"));
                let repeat = generator.fresh_label("push_repeated");
                generator.emit_label(&repeat);
                generator.emit_value_vector_push(vectors[index], SOURCE).unwrap();
                generator.emit("addi s1, s1, -1");
                generator.emit(format!("bnez s1, {repeat}"));
                (0..count).try_for_each(|_| expected[index].push(&mut model, byte))
            }
            Op::Reserve(index, capacity) => {
                generator.emit(format!("li a1, {}", capacity as i64));
                generator.emit_value_vector_reserve(vectors[index]);
                expected[index].reserve(&mut model, capacity)
            }
            Op::Get(index, element) => {
                generator.emit(format!("li a1, {}", element as i64));
                generator.emit_value_vector_get(vectors[index], OUTPUT).unwrap();
                expected[index].get(element).map(|value| output = value)
            }
            Op::Remove(index, element) => {
                generator.emit(format!("li a1, {}", element as i64));
                generator.emit_value_vector_remove(vectors[index]);
                expected[index].remove(&mut model, element)
            }
            Op::Drop(index) => {
                generator.emit_value_vector_drop(vectors[index]);
                expected[index].drop(&mut model)
            }
            Op::Set(index, element, byte) | Op::Insert(index, element, byte) => {
                generator.emit(format!("li t0, {byte}"));
                for offset in SOURCE..SOURCE + width {
                    generator.emit_stack_store_byte("t0", offset);
                }
                generator.emit(format!("li a1, {}", element as i64));
                if matches!(operation, Op::Set(..)) {
                    generator.emit_value_vector_set(vectors[index], SOURCE).unwrap();
                    expected[index].set(&mut model, element, byte)
                } else {
                    generator.emit_value_vector_insert(vectors[index], SOURCE).unwrap();
                    expected[index].insert(&mut model, element, byte)
                }
            }
        };
        if result.is_err() {
            failed = true;
            break;
        }
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
    let status = machine.run().unwrap();
    assert_eq!(status, if failed { 21 } else { 0 }, "{operations:?}");
    let base = machine.machine.registers()[S0];
    let memory = machine.machine.memory_mut().load_bytes(base, frame as u64).unwrap();
    let word = |offset| u64::from_le_bytes(memory[offset..offset + 8].try_into().unwrap());
    for (vector, expected) in vectors.iter().zip(expected.iter()) {
        assert_eq!(word(vector.offset), expected.pointer.map_or(0, |offset| base + (arena.offset + STATE + offset) as u64));
        assert_eq!(word(vector.offset + 8), expected.values.len() as u64);
        assert_eq!(word(vector.offset + 16), expected.capacity as u64);
        assert_eq!(word(vector.offset + 24), u64::from(expected.live));
    }
    assert_eq!(&memory[OUTPUT..OUTPUT + width], output);
    for range in [width..OUTPUT, OUTPUT + width..256, 320..arena.offset, end..frame] {
        assert!(memory[range].iter().all(|byte| *byte == 165), "guard changed");
    }
    assert_eq!(word(arena.offset), model.used as u64);
    assert_eq!(word(arena.offset + 8), model.records.len() as u64);
    for (ordinal, record) in model.records.iter().enumerate() {
        let payload = arena.offset + STATE + record.offset;
        assert_eq!(word(payload - 32), record.data.len() as u64);
        assert_eq!(word(payload - 24), record.requested as u64);
        assert_eq!(word(payload - 16), u64::from(record.live));
        assert_eq!(word(payload - 8), ordinal as u64 + 1);
        assert_eq!(&memory[payload..payload + record.data.len()], record.data);
    }
    assert!(memory[arena.offset + STATE + model.used..end].iter().all(|byte| *byte == 165));
    (status, machine.machine.cycles(), elf_len)
}

#[test]
fn allocated_vector_shared_arena_growth_and_stable_removal_match_model() {
    use Op::*;
    let operations = [
        Reserve(0, 0),
        Push(0, 1, 1),
        Push(1, 2, 1),
        Reserve(0, 3),
        Push(0, 3, 1),
        Push(0, 4, 1),
        Push(0, 5, 1),
        Get(0, 0),
        Get(0, 3),
        Remove(0, 1),
        Get(0, 1),
        Remove(0, 2),
        Remove(0, 0),
        Remove(0, 0),
        Push(0, 9, 1),
        Get(1, 0),
        Drop(0),
        Drop(1),
    ];
    for width in [1, 3, 8, 16, 32, 64] {
        for length in 0..=operations.len() {
            assert_eq!(run(width, 8, 4096, 32, &operations[..length]).0, 0);
        }
    }
}

#[test]
fn allocated_vector_empty_limit_and_dropped_owner_fail() {
    use Op::*;
    for operations in [
        vec![Get(0, 0)],
        vec![Remove(0, 0)],
        vec![Reserve(0, u64::MAX)],
        vec![Push(0, 7, 9)],
        vec![Push(0, 7, 8), Get(0, 8)],
        vec![Push(0, 7, 8), Remove(0, u64::MAX)],
        vec![Drop(0), Drop(0)],
        vec![Push(0, 7, 1), Drop(0), Get(0, 0)],
        vec![Drop(0), Push(0, 1, 1)],
        vec![Drop(0), Reserve(0, 0)],
    ] {
        assert_eq!(run(8, 8, 4096, 32, &operations).0, 21);
    }
}

#[test]
fn allocated_vector_exhaustion_preserves_owner_and_initialized_elements() {
    use Op::*;
    assert_eq!(run(8, 8, 128, 32, &[Push(0, 5, 1), Push(0, 6, 1), Push(0, 7, 1)]).0, 21);
    assert_eq!(run(8, 8, 4096, 1, &[Push(0, 5, 1), Push(0, 6, 1)]).0, 21);
    assert_eq!(run(8, 8, 128, 32, &[Reserve(0, 8), Push(0, 9, 8), Push(1, 1, 1)]).0, 21);
    assert_eq!(run(8, 8, 128, 32, &[Reserve(0, 8), Drop(0), Push(1, 1, 1)]).0, 21);
}

#[test]
fn allocated_vector_maximum_and_one_over_limit_match_model() {
    use Op::*;
    for width in [1, 8] {
        let operations = [Push(0, 87, 1024), Get(0, 1023), Remove(0, 511), Push(0, 91, 1)];
        // Geometric growth consumes all retired spans as well as current storage.
        let (status, cycles, elf) = run(width, 1024, 65536, 32, &operations);
        assert_eq!(status, 0);
        eprintln!("allocated vector width={width}, 1024 elements: cycles={cycles}, elf={elf}");
    }
    assert_eq!(run(32, 1024, 65536, 32, &[Reserve(0, 1024), Push(0, 87, 1024), Get(0, 1023)]).0, 0);
    assert_eq!(run(1, 1024, 65536, 32, &[Push(0, 87, 1025)]).0, 21);
    assert_eq!(run(32, 1024, 65536, 32, &[Push(0, 87, 1024)]).0, 21);
}

#[test]
fn allocated_vector_layout_and_copy_buffers_must_be_disjoint() {
    let arena = Arena::new(4096, 4096, 32).unwrap();
    for (offset, width, elements) in
        [(4096, 8, 8), (4080, 8, 8), (0, 0, 8), (0, 65, 8), (0, 8, 0), (0, 8, 1025), (0, 64, 64), (usize::MAX - 15, 8, 8)]
    {
        assert!(Vector::new(arena, offset, width, elements).is_err());
    }
    let vector = Vector::new(arena, 256, 64, 8).unwrap();
    for offset in [200, 256, 4090, 4096, usize::MAX] {
        assert!(vector.buffer(offset).is_err());
    }
    assert!(vector.buffer(0).is_ok());
}

#[test]
fn allocated_vector_insert_update_and_remove_preserve_order() {
    use Op::*;
    let operations = [
        Insert(0, 0, 1),
        Insert(0, 0, 2),
        Insert(0, 1, 3),
        Insert(0, 3, 4),
        Set(0, 1, 9),
        Get(0, 0),
        Get(0, 1),
        Get(0, 2),
        Get(0, 3),
        Remove(0, 1),
        Get(0, 1),
    ];
    for width in [1, 3, 8, 32, 64] {
        for length in 0..=operations.len() {
            assert_eq!(run(width, 8, 4096, 32, &operations[..length]).0, 0);
        }
    }
    for operations in
        [vec![Insert(0, 1, 3)], vec![Set(0, 0, 3)], vec![Push(0, 1, 8), Insert(0, 0, 9)], vec![Push(0, 1, 1), Insert(0, u64::MAX, 9)]]
    {
        assert_eq!(run(8, 8, 4096, 32, &operations).0, 21);
    }
}

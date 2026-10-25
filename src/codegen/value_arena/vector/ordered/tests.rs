use super::*;
use crate::codegen::value_arena::tests::reference;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::S0, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, Error, SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC,
    ISA_MOP,
};
use std::collections::BTreeMap;

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

#[derive(Clone, Debug)]
enum Op {
    Put(Vec<u8>),
    Get(Vec<u8>),
    Remove(Vec<u8>),
    Find(Vec<u8>),
    Iterate(usize),
    Reserve(usize),
    Batch(usize, bool),
}

fn canonical_key(ordered: Ordered, entry: &[u8]) -> Vec<u8> {
    let mut key = entry[..ordered.key_bytes].to_vec();
    if matches!(ordered.order, KeyOrder::UnsignedLittleEndian) {
        key.reverse();
    }
    key
}

fn put_model(
    ordered: Ordered,
    map: &mut BTreeMap<Vec<u8>, Vec<u8>>,
    vector: &mut reference::Vector,
    arena: &mut reference::Arena,
    entry: &[u8],
) -> std::result::Result<bool, ()> {
    let key = canonical_key(ordered, entry);
    let ordinal = map.range(..key.clone()).count();
    let inserted = !map.contains_key(&key);
    if inserted {
        vector.insert_value(arena, ordinal as u64, entry.to_vec())?;
    } else {
        vector.set_value(arena, ordinal as u64, entry.to_vec())?;
    }
    map.insert(key, entry.to_vec());
    Ok(inserted)
}

fn run(key_bytes: usize, value_bytes: usize, order: KeyOrder, limit: usize, operations: &[Op]) -> (i8, u64, usize) {
    let measured = measure(key_bytes, value_bytes, order, limit, operations);
    (measured.status, measured.cycles, measured.elf_bytes)
}

#[derive(Debug)]
struct Measured {
    status: i8,
    cycles: u64,
    elf_bytes: usize,
    charged_bytes: u64,
    allocations: u64,
}

fn measure(key_bytes: usize, value_bytes: usize, order: KeyOrder, limit: usize, operations: &[Op]) -> Measured {
    let arena = Arena::new(4096, 65536, 256).unwrap();
    let vector = Vector::new(arena, 256, key_bytes + value_bytes, limit).unwrap();
    let ordered = Ordered::new(vector, key_bytes, order).unwrap();
    let mut model = reference::Arena::new(arena.bytes, arena.allocations);
    let mut expected = reference::Vector::new(vector.width, limit);
    let mut map = BTreeMap::new();
    let mut output = vec![165; vector.width];
    let mut result = 0;
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
    generator.emit_stack_store("zero", 128);
    generator.emit_value_arena_init(arena);
    generator.emit_value_vector_init(vector);
    let mut failed = false;
    for op in operations {
        if let Op::Put(entry) | Op::Get(entry) | Op::Remove(entry) | Op::Find(entry) = op {
            assert_eq!(entry.len(), vector.width);
            for (offset, byte) in entry.iter().enumerate() {
                generator.emit(format!("li t0, {byte}"));
                generator.emit_stack_store_byte("t0", offset);
            }
        }
        let operation = match op {
            Op::Put(entry) => {
                generator.emit_value_ordered_put(ordered, 0).unwrap();
                put_model(ordered, &mut map, &mut expected, &mut model, entry)
            }
            Op::Get(entry) => {
                generator.emit_value_ordered_get(ordered, 0, 64).unwrap();
                let found = map.get(&canonical_key(ordered, entry));
                if let Some(value) = found {
                    output = value.clone();
                }
                Ok(found.is_some())
            }
            Op::Find(entry) => {
                generator.emit_value_ordered_find(ordered, 0).unwrap();
                Ok(map.contains_key(&canonical_key(ordered, entry)))
            }
            Op::Remove(entry) => {
                generator.emit_value_ordered_remove(ordered, 0).unwrap();
                let key = canonical_key(ordered, entry);
                if map.contains_key(&key) {
                    let ordinal = map.range(..key.clone()).count();
                    expected.remove(&mut model, ordinal as u64).unwrap();
                    map.remove(&key);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Op::Iterate(index) => {
                generator.emit(format!("li a1, {index}"));
                generator.emit_value_vector_get(vector, 64).unwrap();
                generator.emit("li a0, 1");
                let value = map.values().nth(*index).cloned();
                value
                    .map(|value| {
                        output = value;
                        true
                    })
                    .ok_or(())
            }
            Op::Reserve(capacity) => {
                generator.emit(format!("li a1, {capacity}"));
                generator.emit_value_vector_reserve(vector);
                generator.emit("li a0, 1");
                expected.reserve(&mut model, *capacity as u64).map(|_| true)
            }
            Op::Batch(count, descending) => {
                assert_eq!(key_bytes, 8);
                assert!(*count > 0);
                generator.emit("li t0, 7");
                for offset in key_bytes..vector.width {
                    generator.emit_stack_store_byte("t0", offset);
                }
                generator.emit(format!("li s1, {count}"));
                generator.emit(format!("li s2, {}", if *descending { *count - 1 } else { 0 }));
                let repeat = generator.fresh_label("ordered_batch");
                generator.emit_label(&repeat);
                generator.emit_stack_store("s2", 0);
                generator.emit_value_ordered_put(ordered, 0).unwrap();
                generator.emit(format!("addi s2, s2, {}", if *descending { -1 } else { 1 }));
                generator.emit("addi s1, s1, -1");
                generator.emit(format!("bnez s1, {repeat}"));
                (0..*count).try_fold(false, |_, index| {
                    let key = if *descending { *count - index - 1 } else { index };
                    let mut entry = (key as u64).to_le_bytes().to_vec();
                    entry.resize(vector.width, 7);
                    put_model(ordered, &mut map, &mut expected, &mut model, &entry)
                })
            }
        };
        generator.emit_stack_store("a0", 128);
        match operation {
            Ok(value) => result = u64::from(value),
            Err(()) => {
                failed = true;
                break;
            }
        }
    }
    generator.emit("li a0, 0");
    generator.emit("li a7, 93");
    generator.emit("ecall");
    generator.emit_process_failure_helper();
    let elf = assemble_generated_elf(&generator.assembly).unwrap();
    let elf_len = elf.len();
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 150_000_000);
    let mut machine = TraceMachine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&ckb_vm::Bytes::from(elf), std::iter::empty::<std::result::Result<ckb_vm::Bytes, Error>>()).unwrap();
    let status = machine.run().unwrap();
    assert_eq!(status, if failed { 21 } else { 0 }, "{operations:?}");
    let base = machine.machine.registers()[S0];
    let memory = machine.machine.memory_mut().load_bytes(base, frame as u64).unwrap();
    let word = |offset| u64::from_le_bytes(memory[offset..offset + 8].try_into().unwrap());
    assert_eq!(word(128), result);
    assert_eq!(word(vector.offset), expected.pointer.map_or(0, |offset| base + (arena.offset + STATE + offset) as u64));
    assert_eq!(word(vector.offset + 8), map.len() as u64);
    assert_eq!(word(vector.offset + 16), expected.capacity as u64);
    assert_eq!(word(vector.offset + 24), 1);
    assert_eq!(expected.values, map.values().cloned().collect::<Vec<_>>());
    assert_eq!(&memory[64..64 + vector.width], output);
    for range in [vector.width..64, 64 + vector.width..128, 136..256, 288..arena.offset, end..frame] {
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
    Measured {
        status,
        cycles: machine.machine.cycles(),
        elf_bytes: elf_len,
        charged_bytes: word(arena.offset),
        allocations: word(arena.offset + 8),
    }
}

#[test]
fn ordered_map_and_set_numeric_order_duplicate_updates_and_removal_match_btree() {
    use Op::*;
    for key_bytes in [1, 2, 4, 8] {
        for value_bytes in [0, 1, 8, 32] {
            let entry = |key: u64, value| {
                let mut bytes = key.to_le_bytes()[..key_bytes].to_vec();
                bytes.resize(key_bytes + value_bytes, value);
                bytes
            };
            let max = u64::MAX >> (64 - key_bytes * 8);
            let mut operations = vec![Get(entry(0, 0)), Remove(entry(0, 0)), Find(entry(0, 0))];
            for key in [max, 0, max / 2, 1] {
                operations.push(Put(entry(key, 7)));
            }
            operations.extend([
                Put(entry(max, 9)),
                Get(entry(max, 0)),
                Find(entry(max / 2, 0)),
                Iterate(0),
                Iterate(3),
                Remove(entry(0, 0)),
                Iterate(0),
                Put(entry(0, 3)),
                Remove(entry(2, 0)),
            ]);
            for length in 0..=operations.len() {
                assert_eq!(run(key_bytes, value_bytes, KeyOrder::UnsignedLittleEndian, 4, &operations[..length]).0, 0);
            }
            operations.push(Put(entry(2, 7)));
            assert_eq!(run(key_bytes, value_bytes, KeyOrder::UnsignedLittleEndian, 4, &operations).0, 21);
        }
    }
}

#[test]
fn ordered_byte_keys_compare_first_differing_byte_and_preserve_canonical_iteration() {
    use Op::*;
    for width in [1, 8, 32] {
        for values in [0, 32] {
            let mut entries = Vec::new();
            for (first, last) in [(0, 255), (255, 0), (128, 1), (128, 0)] {
                let mut entry = vec![128; width];
                entry[0] = first;
                entry[width - 1] = last;
                entry.resize(width + values, 7);
                entries.push(entry);
            }
            let operations =
                entries.iter().cloned().map(Put).chain((0..if width == 1 { 3 } else { 4 }).map(Iterate)).collect::<Vec<_>>();
            for length in 0..=operations.len() {
                assert_eq!(run(width, values, KeyOrder::LexicographicBytes, 4, &operations[..length]).0, 0);
            }
        }
    }
}

#[test]
fn ordered_reservation_cost_matrix_preserves_model_and_duplicate_semantics() {
    use Op::*;
    for count in [8, 32, 128, 1024] {
        for values in [0, 8] {
            for descending in [false, true] {
                let mut observations = Vec::new();
                for reserved in [false, true] {
                    let mut operations = Vec::new();
                    if reserved {
                        operations.push(Reserve(count));
                    }
                    operations.push(Batch(count, descending));
                    let before = measure(8, values, KeyOrder::UnsignedLittleEndian, count, &operations);
                    assert_eq!(before.status, 0);
                    operations.push(Batch(count, !descending));
                    let duplicate = measure(8, values, KeyOrder::UnsignedLittleEndian, count, &operations);
                    assert_eq!(duplicate.status, 0);
                    assert_eq!(duplicate.charged_bytes, before.charged_bytes);
                    assert_eq!(duplicate.allocations, before.allocations);
                    eprintln!("ordered-cost count={count} value_bytes={values} descending={descending} reserved={reserved} cycles={} duplicate_total_cycles={} charged_bytes={} allocations={} elf_bytes={}", before.cycles, duplicate.cycles, before.charged_bytes, before.allocations, before.elf_bytes);
                    observations.push(before);
                }
                assert!(observations[1].charged_bytes < observations[0].charged_bytes);
                assert_eq!(observations[1].allocations, 1);
                assert!(observations[1].cycles < observations[0].cycles);
            }
        }
    }
}

#[test]
fn ordered_map_maximum_ascending_and_adversarial_descending_insertions_match_btree() {
    use Op::*;
    for values in [0, 8] {
        for descending in [false, true] {
            let operations = [Reserve(1024), Batch(1024, descending), Iterate(0), Iterate(1023)];
            let (status, cycles, elf) = run(8, values, KeyOrder::UnsignedLittleEndian, 1024, &operations);
            assert_eq!(status, 0);
            eprintln!("ordered values={values} descending={descending} count=1024 cycles={cycles} elf={elf}");
        }
    }
    assert_eq!(run(8, 8, KeyOrder::UnsignedLittleEndian, 1024, &[Reserve(1024), Batch(1025, false)]).0, 21);
    assert_eq!(run(8, 32, KeyOrder::UnsignedLittleEndian, 1024, &[Batch(1024, false)]).0, 21);
}

#[test]
fn ordered_numeric_byte_boundaries_use_unsigned_value_order() {
    use Op::*;
    for width in [2, 4, 8] {
        let operations = [256u64, 1, 255, 257, 32768, 65535]
            .into_iter()
            .map(|key| Put(key.to_le_bytes()[..width].to_vec()))
            .chain((0..6).map(Iterate))
            .collect::<Vec<_>>();
        for length in 0..=operations.len() {
            assert_eq!(run(width, 0, KeyOrder::UnsignedLittleEndian, 6, &operations[..length]).0, 0);
        }
    }
}

#[test]
fn ordered_layout_rejects_unadmitted_keys_and_values() {
    let arena = Arena::new(4096, 4096, 32).unwrap();
    for (width, key, order) in [
        (8, 0, KeyOrder::LexicographicBytes),
        (40, 33, KeyOrder::LexicographicBytes),
        (8, 3, KeyOrder::UnsignedLittleEndian),
        (8, 16, KeyOrder::UnsignedLittleEndian),
        (8, 9, KeyOrder::LexicographicBytes),
        (64, 8, KeyOrder::UnsignedLittleEndian),
    ] {
        let vector = Vector::new(arena, 256, width, 8).unwrap();
        assert!(Ordered::new(vector, key, order).is_err());
    }
}

use super::*;
use crate::codegen::value_arena::tests::reference;
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, memory::Memory, registers::S0, CoreMachine, DefaultCoreMachine,
    DefaultMachineBuilder, DefaultMachineRunner, SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use std::collections::BTreeMap;

type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;

fn run(
    keys: usize,
    values: usize,
    order: KeyOrder,
    limit: usize,
    entries: &[Vec<u8>],
    repeated: bool,
    nonempty: bool,
    alias: bool,
) -> (u64, u64, usize) {
    let arena = Arena::new(4096, 65536, 256).unwrap();
    let vector = Vector::new(arena, 256, keys + values, limit).unwrap();
    let ordered = Ordered::new(vector, keys, order).unwrap();
    let data = entries.concat();
    let input = arena.offset + STATE + arena.bytes + 16;
    let frame = (input + data.len() + 31) / 16 * 16;
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
    generator.emit_value_vector_init(vector);
    if !data.is_empty() {
        let label = generator.const_data_label_for_bytes(data.clone());
        generator.emit(format!("la t3, {label}"));
        generator.emit_sp_addi("t0", input);
        generator.emit(format!("li t2, {}", data.len()));
        generator.emit_value_vector_copy();
    }
    let mut model = reference::Arena::new(arena.bytes, arena.allocations);
    let mut expected = reference::Vector::new(vector.width, limit);
    if nonempty {
        assert!(!entries.is_empty());
        for (offset, byte) in entries[0].iter().enumerate() {
            generator.emit(format!("li t0, {byte}"));
            generator.emit_stack_store_byte("t0", offset);
        }
        generator.emit_value_ordered_put(ordered, 0).unwrap();
        expected.insert_value(&mut model, 0, entries[0].clone()).unwrap();
    }
    generator.emit_sp_addi("a4", if alias { arena.offset } else { input });
    generator.emit_stack_store("a4", 144);
    let canonical = |entry: &[u8]| {
        let mut key = entry[..keys].to_vec();
        if matches!(order, KeyOrder::UnsignedLittleEndian) {
            key.reverse();
        }
        key
    };
    let mut map = BTreeMap::new();
    let mut previous = None;
    let mut valid = !nonempty && !alias;
    for entry in entries {
        assert_eq!(entry.len(), vector.width);
        let key = canonical(entry);
        if previous.as_ref().is_some_and(|previous| previous > &key) {
            valid = false;
        }
        previous = Some(key.clone());
        map.insert(key, entry.clone());
    }
    valid &= map.len() <= limit;
    if repeated {
        assert!(valid);
        generator.emit(format!("li a1, {}", entries.len()));
        generator.emit_value_vector_reserve(vector);
        expected.reserve(&mut model, entries.len() as u64).unwrap();
        generator.emit_sp_addi("s1", input);
        generator.emit(format!("li s2, {}", entries.len()));
        let done = generator.fresh_label("repeat_done");
        let repeat = generator.fresh_label("repeat_insert");
        generator.emit(format!("beqz s2, {done}"));
        generator.emit_label(&repeat);
        generator.emit_sp_addi("t0", 0);
        generator.emit("mv t3, s1");
        generator.emit(format!("li t2, {}", vector.width));
        generator.emit_value_vector_copy();
        generator.emit_value_ordered_put(ordered, 0).unwrap();
        generator.emit(format!("addi s1, s1, {}", vector.width));
        generator.emit("addi s2, s2, -1");
        generator.emit(format!("bnez s2, {repeat}"));
        generator.emit_label(&done);
    } else {
        generator.emit_value_ordered_load_sorted(ordered, 144, entries.len()).unwrap();
        if valid {
            expected.reserve(&mut model, map.len() as u64).unwrap();
        }
    }
    if valid {
        for entry in map.values() {
            expected.insert_value(&mut model, expected.values.len() as u64, entry.clone()).unwrap();
        }
    }
    generator.emit("li a0, 0");
    generator.emit("li a7, 93");
    generator.emit("ecall");
    generator.emit_process_failure_helper();
    generator.emit_const_data_pool();
    let elf = assemble_generated_elf(&generator.assembly).unwrap();
    let elf_bytes = elf.len();
    let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 150_000_000);
    let mut machine = TraceMachine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&ckb_vm::Bytes::from(elf), std::iter::empty::<std::result::Result<ckb_vm::Bytes, ckb_vm::Error>>()).unwrap();
    assert_eq!(machine.run().unwrap(), if valid { 0 } else { 21 });
    let base = machine.machine.registers()[S0];
    let memory = machine.machine.memory_mut().load_bytes(base, frame as u64).unwrap();
    let word = |offset| u64::from_le_bytes(memory[offset..offset + 8].try_into().unwrap());
    assert_eq!(word(vector.offset), expected.pointer.map_or(0, |offset| base + (arena.offset + STATE + offset) as u64));
    assert_eq!(word(vector.offset + 8), expected.values.len() as u64);
    assert_eq!(word(vector.offset + 16), expected.capacity as u64);
    assert_eq!(word(vector.offset + 24), 1);
    assert_eq!(word(arena.offset), model.used as u64);
    assert_eq!(word(arena.offset + 8), model.records.len() as u64);
    for record in &model.records {
        let payload = arena.offset + STATE + record.offset;
        assert_eq!(&memory[payload..payload + record.data.len()], record.data);
    }
    assert_eq!(&memory[input..input + data.len()], data);
    for range in [vector.width..144, 152..256, 288..4096, arena.offset + STATE + model.used..input, input + data.len()..frame] {
        assert!(memory[range].iter().all(|byte| *byte == 165));
    }
    (machine.machine.cycles(), word(arena.offset), elf_bytes)
}

#[test]
fn sorted_batches_match_model_for_keys_duplicates_and_failures() {
    for (keys, order) in [(1, KeyOrder::UnsignedLittleEndian), (8, KeyOrder::UnsignedLittleEndian), (32, KeyOrder::LexicographicBytes)]
    {
        for values in [0, 8, 32] {
            for identifiers in
                [vec![], vec![1], vec![1, 2, 3, 4], vec![1, 1, 3, 3], vec![1, 1, 1, 1], vec![1, 3, 2, 4], vec![4, 3, 2, 1]]
            {
                let entries: Vec<_> = identifiers
                    .iter()
                    .enumerate()
                    .map(|(index, id)| {
                        let mut entry = vec![0; keys];
                        entry[0] = *id;
                        entry.resize(keys + values, index as u8 + 7);
                        entry
                    })
                    .collect();
                run(keys, values, order, 4, &entries, false, false, false);
                if !entries.is_empty() {
                    run(keys, values, order, 4, &entries, false, true, false);
                    run(keys, values, order, 4, &entries, false, false, true);
                }
            }
        }
    }
    let entries: Vec<_> = (0..5u64).map(|key| key.to_le_bytes().to_vec()).collect();
    run(8, 0, KeyOrder::UnsignedLittleEndian, 4, &entries, false, false, false);
}

#[test]
fn sorted_batches_measure_linear_construction_against_repeated_insertion() {
    for count in [8, 32, 128, 1024] {
        for values in [0, 8] {
            let entries: Vec<_> = (0..count)
                .map(|key| {
                    let mut entry = (key as u64).to_le_bytes().to_vec();
                    entry.resize(8 + values, 7);
                    entry
                })
                .collect();
            let repeated = run(8, values, KeyOrder::UnsignedLittleEndian, count, &entries, true, false, false);
            let bulk = run(8, values, KeyOrder::UnsignedLittleEndian, count, &entries, false, false, false);
            assert!(bulk.0 < repeated.0);
            assert_eq!(bulk.1, repeated.1);
            eprintln!("sorted-batch count={count} values={values} repeated={repeated:?} bulk={bulk:?}");
        }
    }
}

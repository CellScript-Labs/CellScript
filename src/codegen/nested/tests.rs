use super::*;
use cellscript_artifact_checker::bounded_nested::{MAX_ITEMS, MAX_PAYLOAD_BYTES, MAX_TOTAL_BYTES, PROFILE};
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};

fn contract(element: ElementShape) -> NestedValueContract {
    NestedValueContract {
        profile: PROFILE.into(),
        element,
        max_items: MAX_ITEMS,
        max_payload_bytes: MAX_PAYLOAD_BYTES,
        max_total_bytes: MAX_TOTAL_BYTES,
    }
}

fn bytes(payload: &[u8]) -> Vec<u8> {
    let mut value = (payload.len() as u32).to_le_bytes().to_vec();
    value.extend_from_slice(payload);
    value
}

// Independent test fixture writer, not used by either validator. Literal
// upstream-format examples below keep this writer from defining the format.
fn table(fields: &[Vec<u8>]) -> Vec<u8> {
    let header = 4 * (fields.len() + 1);
    let mut value = ((header + fields.iter().map(Vec::len).sum::<usize>()) as u32).to_le_bytes().to_vec();
    let mut offset = header;
    for field in fields {
        value.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += field.len();
    }
    for field in fields {
        value.extend_from_slice(field);
    }
    value
}

fn run(contract: &NestedValueContract, input: &[u8], alignment: usize) -> (i8, u64, usize) {
    let expected_count = contract.check(input).ok().map(|value| value.elements().len()).unwrap_or(0);
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    generator.emit(".section .text");
    generator.emit(".global entry");
    generator.emit_label("entry");
    generator.emit("la a0, input");
    generator.emit(format!("li a1, {}", input.len()));
    for register in ["a6", "a7", "t4", "t5", "t6"] {
        generator.emit(format!("li {register}, 99"));
    }
    generator.emit_bounded_nested_validation(contract, "invalid").unwrap();
    generator.emit("la t0, input");
    generator.emit("bne a0, t0, abi_corrupted");
    generator.emit(format!("li t0, {}", input.len()));
    generator.emit("bne a1, t0, abi_corrupted");
    generator.emit(format!("li t0, {expected_count}"));
    generator.emit("bne a2, t0, abi_corrupted");
    generator.emit("li t0, 99");
    for register in ["a6", "a7", "t4", "t5", "t6"] {
        generator.emit(format!("bne {register}, t0, abi_corrupted"));
    }
    generator.emit("li a0, 0");
    generator.emit("ret");
    generator.emit_label("invalid");
    generator.emit(format!("li a0, {}", CellScriptRuntimeError::DynamicFieldBoundsInvalid.code()));
    generator.emit("ret");
    generator.emit_label("abi_corrupted");
    generator.emit("li a0, 127");
    generator.emit("ret");
    generator.emit(".section .rodata");
    generator.emit(".align 3");
    for _ in 0..alignment {
        generator.emit(".byte 0");
    }
    generator.emit_label("input");
    for byte in input {
        generator.emit(format!(".byte {byte}"));
    }
    // An empty slice still needs an addressable data symbol for the minimum
    // length rejection. No read of this sentinel belongs to the slice.
    generator.emit(".byte 165");
    execute(&generator.assembly)
}

fn execute(lines: &[String]) -> (i8, u64, usize) {
    let elf = assemble_generated_elf(lines).unwrap();
    let elf_bytes = elf.len();
    type Machine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;
    let core = <<Machine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 100_000);
    let mut machine = Machine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&Bytes::from(elf), std::iter::empty::<std::result::Result<Bytes, ckb_vm::Error>>()).unwrap();
    let exit = machine.run().expect("bounded decoder must terminate without a VM trap");
    (exit, machine.machine.cycles(), elf_bytes)
}

fn agree(contract: &NestedValueContract, input: &[u8]) {
    let valid = contract.check(input).is_ok();
    let (exit, cycles, _) = run(contract, input, 3);
    assert_eq!(exit, if valid { 0 } else { 16 }, "VM/independent host disagree on {}", hex::encode(input));
    assert!(cycles < 20_000, "bounded decoder exceeds local test ceiling: {cycles}");
}

#[test]
fn bounded_nested_machine_literal_vectors_and_unaligned_slices() {
    for (shape, wire, count) in [
        (ElementShape::Bytes {}, "04000000", 0),
        (ElementShape::Bytes {}, "1f0000001000000014000000190000000000000001000000ab020000000102", 3),
        (
            ElementShape::Record { fixed_field_widths: vec![1, 2] },
            "2100000008000000190000001000000011000000130000007f341202000000aabb",
            1,
        ),
    ] {
        let c = contract(shape);
        let input = hex::decode(wire).unwrap();
        assert_eq!(c.check(&input).unwrap().elements().len(), count);
        for alignment in 0..8 {
            assert_eq!(run(&c, &input, alignment).0, 0);
        }
    }
}

#[test]
fn bounded_nested_machine_truncation_rebound_sizes_and_offsets() {
    for (c, input) in [
        (contract(ElementShape::Bytes {}), table(&[bytes(&[1]), bytes(&[2, 3]), bytes(&[4, 5, 6])])),
        (
            contract(ElementShape::Record { fixed_field_widths: vec![1, 2] }),
            table(&[table(&[vec![1], vec![2, 3], bytes(&[])]), table(&[vec![4], vec![5, 6], bytes(&[7, 8])])]),
        ),
    ] {
        assert_eq!(run(&c, &input, 3).0, 0);
        for end in 0..input.len() {
            let mut truncated = input[..end].to_vec();
            assert!(c.check(&truncated).is_err());
            agree(&c, &truncated);
            if end >= 8 {
                truncated[..4].copy_from_slice(&(end as u32).to_le_bytes());
                assert!(c.check(&truncated).is_err());
                agree(&c, &truncated);
            }
        }
        // Includes offset/count/total changes and ordinary payload changes.
        // Both validators must agree; payload content is not authorization.
        for offset in 0..input.len().saturating_sub(3) {
            for word in [0, 4, 8, 12, input.len() as u32, u32::MAX] {
                let mut mutation = input.clone();
                mutation[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
                agree(&c, &mutation);
            }
        }
        let mut trailing = input.clone();
        trailing.push(0);
        agree(&c, &trailing);
        let size = trailing.len() as u32;
        trailing[..4].copy_from_slice(&size.to_le_bytes());
        assert!(c.check(&trailing).is_err());
        agree(&c, &trailing);
    }
}

#[test]
fn bounded_nested_machine_maximum_and_late_invalid_resource_evidence() {
    let c = contract(ElementShape::Bytes {});
    let count_max = table(&vec![bytes(&[]); MAX_ITEMS as usize]);
    agree(&c, &count_max);
    agree(&c, &table(&vec![bytes(&[]); MAX_ITEMS as usize + 1]));
    agree(&c, &table(&[bytes(&vec![0; MAX_PAYLOAD_BYTES as usize + 1])]));
    let mut payloads = vec![bytes(&[0xaa; 256]); 15];
    payloads.push(bytes(&[0xbb; 124]));
    let maximum = table(&payloads);
    assert_eq!(maximum.len(), MAX_TOTAL_BYTES as usize);
    let valid = run(&c, &maximum, 1);
    assert_eq!(valid.0, 0);
    payloads[15][0] = 123; // final element's claimed length; outer total remains exact
    let late_invalid = table(&payloads);
    let invalid = run(&c, &late_invalid, 1);
    assert_eq!(invalid.0, 16);
    eprintln!(
        "[nested-codec primitive] max bytes={} valid_cycles={} late_invalid_cycles={} ELF={} stack=0 heap=0 (excluding caller)",
        maximum.len(),
        valid.1,
        invalid.1,
        valid.2
    );
    assert!(valid.1 < 20_000 && invalid.1 < 20_000 && valid.2 <= 8192);
    payloads[15] = bytes(&[0xbb; 125]);
    agree(&c, &table(&payloads));

    let c = contract(ElementShape::Record { fixed_field_widths: vec![256; 8] });
    let mut fields = vec![vec![0x11; 256]; 8];
    fields.push(bytes(&[0x22; 256]));
    let maximum_record = table(&[table(&fields)]);
    assert_eq!(run(&c, &maximum_record, 7).0, 0);
    agree(&c, &maximum_record);
    // Most iterations for the record shape, not just the largest record.
    let c = contract(ElementShape::Record { fixed_field_widths: vec![1] });
    let records = table(&vec![table(&[vec![0x44], bytes(&[])]); 32]);
    assert_eq!(run(&c, &records, 5).0, 0);
    agree(&c, &records);

    // Joint count/total maxima: scanning the maximum byte count alone need
    // not maximize work, since payload bytes are borrowed without copying.
    for shape in [ElementShape::Bytes {}, ElementShape::Record { fixed_field_widths: vec![1; 8] }] {
        let c = contract(shape);
        let make_record = |payload: Vec<u8>| match &c.element {
            ElementShape::Bytes {} => bytes(&payload),
            ElementShape::Record { fixed_field_widths } => {
                let mut fields = vec![vec![0x55]; fixed_field_widths.len()];
                fields.push(bytes(&payload));
                table(&fields)
            }
        };
        let minimum = table(&vec![make_record(Vec::new()); 32]).len();
        let available = MAX_TOTAL_BYTES as usize - minimum;
        let mut items =
            (0..32).map(|index| make_record(vec![0x66; available / 32 + usize::from(index < available % 32)])).collect::<Vec<_>>();
        let maximum = table(&items);
        assert_eq!(maximum.len(), MAX_TOTAL_BYTES as usize);
        let valid = run(&c, &maximum, 1);
        assert_eq!(valid.0, 0);
        // Change only the final Bytes length after every preceding item has
        // been traversed. Keep both outer and record total lengths exact.
        let payload_word = match &c.element {
            ElementShape::Bytes {} => 0,
            ElementShape::Record { fixed_field_widths } => (fixed_field_widths.len() + 2) * 4 + fixed_field_widths.len(),
        };
        items[31][payload_word] -= 1;
        let invalid = run(&c, &table(&items), 1);
        assert_eq!(invalid.0, 16);
        eprintln!(
            "[nested-codec joint maxima] {:?}: items=32 bytes=4096 valid_cycles={} late_invalid_cycles={} ELF={} stack=0 heap=0 (excluding caller)",
            c.element, valid.1, invalid.1, valid.2
        );
        assert!(valid.1 < 20_000 && invalid.1 < 20_000 && valid.2 <= 8192);
    }
}

#[test]
fn bounded_nested_machine_narrow_contracts_and_overflow_reject_before_load() {
    let mut c = contract(ElementShape::Bytes {});
    c.max_items = 0;
    c.max_payload_bytes = 0;
    c.max_total_bytes = 4;
    agree(&c, &table(&[]));
    agree(&c, &table(&[bytes(&[])]));
    c.max_total_bytes = 64;
    agree(&c, &table(&[bytes(&[])]));
    c.max_items = 1;
    agree(&c, &table(&[bytes(&[])]));
    agree(&c, &table(&[bytes(&[0])]));
    for (pointer, length) in [(u64::MAX - 1, 4), (0, 0), (u64::MAX, 4097)] {
        let mut generator = CodeGenerator::new(CodegenOptions::default());
        generator.emit(".section .text");
        generator.emit(".global entry");
        generator.emit_label("entry");
        generator.emit(format!("li a0, {}", pointer as i64));
        generator.emit(format!("li a1, {length}"));
        generator.emit_bounded_nested_validation(&c, "invalid").unwrap();
        generator.emit("li a0, 127");
        generator.emit("ret");
        generator.emit_label("invalid");
        generator.emit("li a0, 16");
        generator.emit("ret");
        assert_eq!(execute(&generator.assembly).0, 16);
    }
    c.max_items = MAX_ITEMS + 1;
    let mut generator = CodeGenerator::new(CodegenOptions::default());
    assert!(generator.emit_bounded_nested_validation(&c, "invalid").is_err());
    assert!(generator.assembly.is_empty(), "invalid contracts cannot leave partial machine code");
}

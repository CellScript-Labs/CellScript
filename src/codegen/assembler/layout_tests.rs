use super::*;

fn lines(source: &str) -> Vec<String> {
    source.lines().map(str::to_owned).collect()
}

fn empty_ir() -> IrModule {
    IrModule {
        name: "assembler_layout".to_string(),
        items: vec![],
        entry_selection: Default::default(),
        external_type_defs: vec![],
        external_callable_abis: vec![],
        enum_fixed_sizes: Default::default(),
        enum_layouts: Default::default(),
        generic_contracts: Default::default(),
        source_type_origins: Default::default(),
        nominal_identities: Default::default(),
        source_callable_targets: Default::default(),
    }
}

fn decode(bytes: &[u8]) -> cellscript_artifact_checker::ParsedElf {
    cellscript_artifact_checker::parse_elf(bytes, 1_000_000).expect("independent ELF decoding")
}

#[cfg(feature = "vm-runner")]
fn execute(bytes: Vec<u8>) -> i8 {
    use ckb_vm::{
        cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
        SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
    };
    type Machine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;
    let core = <<Machine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 5_000_000);
    let mut machine = Machine::new(DefaultMachineBuilder::new(core).instruction_cycle_func(Box::new(estimate_cycles)).build());
    machine.load_program(&Bytes::from(bytes), std::iter::empty::<std::result::Result<Bytes, ckb_vm::Error>>()).unwrap();
    machine.run().unwrap()
}

#[test]
fn all_admitted_instruction_forms_reject_missing_and_extra_operands() {
    for &(mnemonic, fixture) in SUPPORTED_INTERNAL_ASSEMBLER_MNEMONICS {
        parse_instruction(fixture).unwrap_or_else(|error| panic!("valid {mnemonic}: {error}"));
        let extra = if fixture == mnemonic { format!("{fixture} extra") } else { format!("{fixture}, extra") };
        let error = parse_instruction(&extra).expect_err("extra operands must never be ignored");
        assert!(error.message.contains("expects exactly"), "{extra}: {error}");
        if fixture != mnemonic {
            let args = fixture.split_once(char::is_whitespace).unwrap().1;
            let missing = args.rsplit_once(',').map_or_else(|| mnemonic.to_string(), |(prefix, _)| format!("{mnemonic} {prefix}"));
            assert!(parse_instruction(&missing).is_err(), "missing operand: {missing}");
        }
    }
    for malformed in ["addi a0, a1, 1, extra", "ret extra", "ecall extra", "add a0,,a1", "li a0,1,"] {
        let source = lines(&format!(".section .text\n.global entry\nentry:\n{malformed}\nret"));
        let error = assemble_generated_elf(&source).unwrap_err();
        assert_eq!(error.details.as_ref().unwrap()["generated_assembly"]["line"], 4);
    }
}

#[test]
fn memory_operands_must_consume_the_complete_parenthesized_form() {
    for operand in ["0(sp)junk", "0(sp) (t0)", "0(sp))", "0((sp)", "0)sp(", "0(sp", "(sp)"] {
        assert!(parse_memory_operand(operand).is_err(), "{operand}");
        assert!(memory_operand_offset_and_base(operand).is_none(), "{operand}");
    }
    assert_eq!(parse_memory_operand(" -2048 ( x2 ) ").unwrap(), (-2048, 2));
    assert_eq!(memory_operand_offset_and_base(" -2048 ( x2 ) "), Some((-2048, "x2")));
}

#[test]
fn assembler_shift_fields_reject_negative_and_over_width_values() {
    for (mnemonic, maximum) in [("slli", 63), ("srli", 63), ("srai", 63), ("rori", 63), ("roriw", 31)] {
        for amount in [-1, maximum + 1] {
            let source = lines(&format!(".section .text\n.global entry\nentry:\n{mnemonic} a0, a1, {amount}\nret"));
            let error = assemble_generated_elf(&source).expect_err("invalid machine shift must reject");
            assert!(error.message.contains("shift amount"), "{mnemonic}: {error}");
            assert_eq!(error.details.as_ref().unwrap()["generated_assembly"]["line"], 4);
        }
        for amount in [0, maximum] {
            assemble_generated_elf(&lines(&format!(".section .text\n.global entry\nentry:\n{mnemonic} a0, a1, {amount}\nret")))
                .expect("inclusive machine shift boundary");
        }
    }
}

#[test]
fn mulhu_and_rotate_encodings_keep_their_independent_riscv_bit_fields() {
    // Constants come from the RISC-V opcode formats, rather than this encoder.
    let bytes =
        assemble_generated_elf(&lines(".section .text\n.global entry\nentry:\nmulhu a0,a1,a2\nrori a0,a1,63\nroriw a0,a1,31\nret"))
            .unwrap();
    let elf = decode(&bytes);
    let words = elf.instructions.iter().filter(|word| word.address >= ELF_BASE_ADDR + 20).map(|word| word.word).collect::<Vec<_>>();
    assert_eq!(words, [0x02c5_b533, 0x63f5_d513, 0x61f5_d51b, 0x0000_8067]);
    assert!(parse_instruction("sgt a0,a1,a2").is_ok());
    assert!(parse_instruction("sgtu a0,a1,a2").is_err(), "do not advertise an unimplemented alias");
}

#[test]
fn text_alignment_includes_the_entry_prefix_and_preserves_label_positions() {
    for before_alignment in [false, true] {
        let placement = if before_alignment { "entry:\n.align 3" } else { ".align 3\nentry:" };
        let source = lines(&format!(".section .text\n.global entry\n{placement}\nli a0,0\nret\n.section .rodata\n.word 305419896"));
        let (bytes, evidence) = assemble_generated_elf_with_evidence(&source, &BTreeMap::new(), &empty_ir()).unwrap();
        let elf = decode(&bytes);
        assert_eq!(elf.text.size, 32);
        assert_eq!(elf.rodata.address, ELF_BASE_ADDR + 32);
        assert_eq!(evidence.text_end, elf.text.range().end);
        assert_eq!(evidence.symbols["entry"], ELF_BASE_ADDR + if before_alignment { 20 } else { 24 });
        assert_eq!(elf.instructions.iter().find(|word| word.address == ELF_BASE_ADDR + 20).unwrap().word, 0x0000_0013);
        assert_eq!(&bytes[elf.rodata.offset as usize..elf.rodata.offset as usize + 4], &305419896u32.to_le_bytes());
        #[cfg(feature = "vm-runner")]
        assert_eq!(execute(bytes), 0, "both label placements execute whole NOP padding safely");
    }
}

#[test]
fn alignment_powers_and_read_only_data_use_their_real_virtual_addresses() {
    for exponent in [0, 1, 2, 3, 4, 5, 8, 16] {
        let source = lines(&format!(
            ".section .text\n.global entry\n.align {exponent}\nentry:\nli a0,0\nret\n.section .rodata\n.byte 7\n.align {exponent}\ndata:\n.word 42"
        ));
        let plan = MachineLayoutPlan::build(&source).unwrap();
        let alignment = 1u64 << exponent;
        assert_eq!(plan.parsed.symbol_address("entry", &plan.layout).unwrap() % alignment, 0);
        assert_eq!(plan.parsed.symbol_address("data", &plan.layout).unwrap() % alignment, 0);
        let elf = decode(&encode_elf(&plan).unwrap());
        assert_eq!(elf.text.size, START_TRAMPOLINE_SIZE as u64 + plan.parsed.text_size as u64);
        assert!(elf.text.range().end <= elf.rodata.address);
    }
    let error = assemble_generated_elf(&lines(".section .text\n.global entry\n.byte 1\n.align 3\nentry:\nret")).unwrap_err();
    assert!(error.message.contains("four-byte NOP padding"), "{error}");
}

#[test]
fn consecutive_alignment_and_unaligned_data_keep_exact_padding_extents() {
    let source = lines(
        ".section .text\n.global entry\n.align 3\nbefore:\n.align 4\nentry:\nli a0,0\nret\n.section .rodata\n.byte 7\n.align 4\n.align 4\ndata:\n.word 42",
    );
    let plan = MachineLayoutPlan::build(&source).unwrap();
    assert_eq!(plan.parsed.symbol_address("before", &plan.layout).unwrap(), ELF_BASE_ADDR + 24);
    assert_eq!(plan.parsed.symbol_address("entry", &plan.layout).unwrap(), ELF_BASE_ADDR + 32);
    assert_eq!(plan.parsed.symbol_address("data", &plan.layout).unwrap(), plan.layout.rodata_base + 16);
    let bytes = encode_elf(&plan).unwrap();
    let elf = decode(&bytes);
    assert_eq!(elf.text.size, 40);
    let start = elf.rodata.offset as usize;
    assert_eq!(bytes[start], 7);
    assert!(bytes[start + 1..start + 16].iter().all(|byte| *byte == 0));
    assert_eq!(&bytes[start + 16..start + 20], &42u32.to_le_bytes());
}

#[test]
fn alignment_relaxation_rechecks_both_the_1018_and_1023_instruction_examples() {
    for (fillers, requires_long) in [(1018, true), (1023, false)] {
        let mut source = lines(".section .text\n.global entry\nentry:\nbeqz zero,next\nnext:");
        source.extend((0..fillers).map(|_| "addi t0,t0,0".to_string()));
        source.extend(lines("beqz zero,far\nj far\n.align 13\nfar:\nli a0,0\nret"));
        let plan = MachineLayoutPlan::build(&source).unwrap();
        let second = plan.parsed.text_ops.iter().position(|op| op.provenance.text == "beqz zero,far").unwrap();
        assert_eq!(plan.parsed.relaxed_text_branches.contains(&second), requires_long);
        assert_eq!(plan.parsed.symbol_address("far", &plan.layout).unwrap(), ELF_BASE_ADDR + 8192);
        if requires_long {
            assert_eq!(START_TRAMPOLINE_SIZE + plan.parsed.text_layouts[second].offset, 4096);
        }
        let bytes = encode_elf(&plan).unwrap();
        let elf = decode(&bytes);
        assert!(elf.control_flow.iter().any(|edge| edge.target == ELF_BASE_ADDR + 8192));
        #[cfg(feature = "vm-runner")]
        assert_eq!(execute(bytes), 0);
    }
}

#[test]
fn final_backward_and_register_branch_layouts_are_in_range() {
    for branch in ["beq a0,a1,back", "bne a0,a1,back", "blt a0,a1,back", "bge a0,a1,back", "bltu a0,a1,back", "bgeu a0,a1,back"] {
        let mut source = lines(".section .text\n.global entry\nentry:\nj finish\nback:\nli a0,0\n.align 13");
        source.push(branch.to_string());
        source.extend(lines("finish:\nli a0,0\nret"));
        let plan = MachineLayoutPlan::build(&source).unwrap();
        assert_eq!(plan.parsed.relaxed_text_branches.len(), 1);
        let bytes = encode_elf(&plan).unwrap();
        decode(&bytes);
        #[cfg(feature = "vm-runner")]
        assert_eq!(execute(bytes), 0, "the aligned backward loop is intentionally skipped");
    }
}

#[test]
fn frozen_layout_mismatches_and_size_overflows_return_errors() {
    let source = lines(".section .text\n.global entry\nentry:\nli a0,0\nret");
    let mut plan = MachineLayoutPlan::build(&source).unwrap();
    plan.parsed.text_layouts.last_mut().unwrap().offset += 4;
    let error = encode_elf(&plan).unwrap_err();
    assert!(error.message.contains("layout mismatch"), "{error}");
    assert!(checked_layout_add(isize::MAX as usize, 1).is_err());
    assert!(checked_align_up(isize::MAX as usize, 16).is_err());
    assert!(checked_align_up(0, 0).is_err());
    assert!(checked_align_up(0, 3).is_err());
    assert!(SectionLayout::for_text_user_size(isize::MAX as usize, 8).is_err());
    assert!(op_size(&AsmOp::Align(8), usize::MAX, SectionKind::Text, 0, BranchSizeMode::Conservative).is_err());
    let invalid_layout = SectionLayout { text_base: 16, text_user_base: 20, rodata_base: 8 };
    assert!(invalid_layout.rodata_offset().is_err());
    plan.parsed.symbols.insert("overflow".to_string(), SymbolDef { section: SectionKind::Rodata, offset: 16, line: 1 });
    let invalid_layout = SectionLayout { text_base: 0, text_user_base: 20, rodata_base: u64::MAX - 8 };
    assert!(plan.parsed.symbol_address("overflow", &invalid_layout).is_err());
}

#[test]
fn calls_cannot_disappear_from_the_cfg_by_targeting_data_or_an_empty_tail() {
    for source in [
        ".section .text\n.global entry\nentry:\ncall target\nret\n.section .rodata\ntarget:\n.word 0",
        ".section .text\n.global entry\nentry:\ncall target\nret\ntarget:",
    ] {
        let error = assemble_generated_elf(&lines(source)).unwrap_err();
        assert!(error.message.contains("call target 'target' does not start a text machine block"), "{error}");
        assert_eq!(error.details.as_ref().unwrap()["generated_assembly"]["line"], 4);
    }
}

#[test]
fn combined_generation_keeps_unchanged_small_elf_bytes_and_evidence() {
    let source = lines(".section .text\n.global entry\nentry:\nli a0,7\nret\n.section .rodata\n.word 42");
    let (bytes, evidence) = assemble_generated_elf_with_evidence(&source, &BTreeMap::new(), &empty_ir()).unwrap();
    let elf = decode(&bytes);
    let start = elf.text.offset as usize;
    let expected = [0x0000_0097u32, 0x0140_80e7, 0x0000_08b7, 0x05d8_8893, 0x0000_0073, 0x0070_0513, 0x0000_8067];
    let expected = expected.into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
    assert_eq!(&bytes[start..start + expected.len()], expected);
    assert_eq!(evidence.text_start, ELF_BASE_ADDR + 20);
    assert_eq!(evidence.text_end, elf.text.range().end);
    assert_eq!(evidence.symbols["entry"], ELF_BASE_ADDR + 20);
    assert_eq!(elf.rodata.address, ELF_BASE_ADDR + 32);
    assert_eq!(bytes, assemble_generated_elf(&source).unwrap());
    assert_eq!(evidence.blocks.len(), evidence.edges.len() + 1);
    #[cfg(feature = "vm-runner")]
    assert_eq!(execute(bytes), 7);
}

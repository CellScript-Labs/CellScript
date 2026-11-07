use super::*;

fn value(word: u32, left: u64, right: u64) -> u64 {
    decode(word)
        .unwrap()
        .constant_result(0x10000, |register| match register {
            10 => Some(left),
            11 => Some(right),
            _ => None,
        })
        .unwrap()
}

#[test]
fn word_operations_truncate_then_sign_extend_including_unsigned_division_and_rotation() {
    // Independent instruction words: all destinations/left operands are a0,
    // register right operands are a1. Immediate shift amounts are in the word.
    for (word, left, right, expected) in [
        (0x0005051b, 0x80000000, 0, 0xffffffff80000000),          // addiw 0
        (0x01f5151b, 1, 0, 0xffffffff80000000),                   // slliw 31
        (0x0005551b, 0x80000000, 0, 0xffffffff80000000),          // srliw 0
        (0x0015551b, 0xffffffff, 0, 0x7fffffff),                  // srliw 1
        (0x4015551b, 0x80000000, 0, 0xffffffffc0000000),          // sraiw 1
        (0x6015551b, 1, 0, 0xffffffff80000000),                   // roriw 1
        (0x00b5053b, 0x7fffffff, 1, 0xffffffff80000000),          // addw
        (0x40b5053b, 0x80000000, 1, 0x7fffffff),                  // subw
        (0x00b5153b, 1, 63, 0xffffffff80000000),                  // sllw masks rs2 to 5 bits
        (0x00b5553b, 0xffffffff, 33, 0x7fffffff),                 // srlw masks rs2
        (0x40b5553b, 0x80000000, 1, 0xffffffffc0000000),          // sraw
        (0x02b5053b, 0x40000000, 2, 0xffffffff80000000),          // mulw
        (0x02b5453b, 0x80000000, 0xffffffff, 0xffffffff80000000), // divw overflow
        (0x02b5553b, 0xffffffff, 1, u64::MAX),                    // divuw still sign-extends
        (0x02b5653b, 0x80000000, 0, 0xffffffff80000000),          // remw by zero
        (0x02b5753b, 0xffffffff, 0, u64::MAX),                    // remuw by zero
    ] {
        assert_eq!(value(word, left, right), expected, "{word:#010x}");
    }
}

#[test]
fn xlen_extrema_and_shift_masks_follow_rv64_integer_semantics() {
    for (word, left, right, expected) in [
        (0x03f51513, 1, 0, 1 << 63),                    // slli 63
        (0x03f55513, u64::MAX, 0, 1),                   // srli 63
        (0x43f55513, 1 << 63, 0, u64::MAX),             // srai 63
        (0x63f55513, 1, 0, 2),                          // rori 63
        (0x00b51533, 1, 64, 1),                         // sll masks to six bits
        (0x02b51533, u64::MAX, u64::MAX, 0),            // mulh signed
        (0x02b52533, u64::MAX, u64::MAX, u64::MAX),     // mulhsu
        (0x02b53533, u64::MAX, u64::MAX, u64::MAX - 1), // mulhu
        (0x02b54533, 1 << 63, u64::MAX, 1 << 63),       // div signed overflow
        (0x02b55533, 17, 0, u64::MAX),                  // divu by zero
        (0x02b56533, 17, 0, 17),                        // rem by zero
        (0x02b57533, u64::MAX, 3, 0),                   // remu
    ] {
        assert_eq!(value(word, left, right), expected, "{word:#010x}");
    }
    assert_eq!(decode(0x80000537).unwrap().constant_result(0, |_| None), Some(0xffffffff80000000));
    assert_eq!(decode(0xfffff517).unwrap().constant_result(0x10000, |_| None), Some(0xf000));
}

#[test]
fn every_admitted_alu_encoding_has_real_register_effects() {
    // Enumerate independent architectural funct combinations, including the
    // deliberately narrow SLTIU/SEQZ and Zbb immediate surface.
    for (opcode, highs, functions) in [
        (0x33, vec![0, 1], vec![0, 1, 2, 3, 4, 5, 6, 7]),
        (0x33, vec![0x20], vec![0, 5]),
        (0x3b, vec![0], vec![0, 1, 5]),
        (0x3b, vec![1], vec![0, 4, 5, 6, 7]),
        (0x3b, vec![0x20], vec![0, 5]),
    ] {
        for high in highs {
            for &function in &functions {
                for rd in 0..32 {
                    let word = high << 25 | 11 << 20 | 10 << 15 | function << 12 | rd << 7 | opcode;
                    let instruction = decode(word).unwrap();
                    let effects = instruction.effects();
                    assert_eq!(effects.reads, (1 << 10) | (1 << 11));
                    assert_eq!(effects.writes, if rd == 0 { 0 } else { 1 << rd });
                    assert!(effects.memory.is_none());
                    assert_eq!(effects.control, ControlFlow::Fallthrough);
                }
            }
        }
    }
    for word in [
        0x00150513, 0x00153513, 0xfff54513, 0xfff56513, 0xfff57513, 0x03f51513, 0x03f55513, 0x43f55513, 0x63f55513, 0x0015051b,
        0x01f5151b, 0x01f5551b, 0x41f5551b, 0x61f5551b,
    ] {
        let effects = decode(word).unwrap().effects();
        assert_eq!(effects.reads, 1 << 10);
        assert_eq!(effects.writes, 1 << 10);
    }
}

#[test]
fn stores_and_branches_never_treat_immediate_bits_as_destination_registers() {
    for width in 0..=3 {
        for low in 0..32 {
            let word = 11 << 20 | 10 << 15 | width << 12 | low << 7 | 0x23;
            let effect = decode(word).unwrap().effects();
            assert_eq!(effect.writes, 0);
            assert_eq!(effect.reads, (1 << 10) | (1 << 11));
            assert_eq!(effect.memory.unwrap().bytes, Some(1 << width));
        }
    }
    for function in [0, 1, 4, 5, 6, 7] {
        let effect = decode(11 << 20 | 10 << 15 | function << 12 | 1 << 7 | 0x63).unwrap().effects();
        assert_eq!(effect.writes, 0);
        assert_eq!(effect.control, ControlFlow::Conditional { offset: 2048 });
    }
    for (word, width) in [(0x00053583, 8), (0x00054583, 1)] {
        let effect = decode(word).unwrap().effects();
        assert_eq!(effect.reads, 1 << 10);
        assert_eq!(effect.writes, 1 << 11);
        assert_eq!(effect.memory.unwrap().bytes, Some(width));
    }
}

#[test]
fn jump_dependencies_and_syscall_effects_do_not_assume_call_memory_permissions() {
    let call = decode(0x004080e7).unwrap();
    assert_eq!(call.effects().reads, 1 << 1);
    assert_eq!(call.effects().writes, 1 << 1);
    assert_eq!(call.effects().control, ControlFlow::Indirect { base: 1, offset: 4 });
    assert!(call.is_call());
    let ret = decode(0x00008067).unwrap();
    assert_eq!(ret.effects().reads, 1 << 1);
    assert_eq!(ret.effects().writes, 0);
    assert!(!ret.is_call());
    assert_eq!(ret.constant_result(0x10000, |_| None), Some(0));
    assert_eq!(decode(0x7ff00013).unwrap().constant_result(0, |_| None), Some(0));
    assert_eq!(decode(0xffdff06f).unwrap().effects().control, ControlFlow::Direct { offset: -4 });
    assert_eq!(decode(0xfe000ee3).unwrap().effects().control, ControlFlow::Conditional { offset: -4 });
    let syscall = decode(0x00000073).unwrap().effects();
    assert_eq!(syscall.writes, 1 << 10);
    assert_eq!(syscall.memory.unwrap().access, MemoryAccess::Syscall);
}

#[test]
fn neighboring_reserved_or_unadmitted_forms_fail_closed() {
    for word in [
        0x00000000, 0x00008082, // invalid and compressed
        0x0205151b, 0x0205551b, 0x4205551b, 0x6205551b, // reserved word shift bit5
        0x04051513, 0x66055513, 0x6315551b, // reserved XLEN/Zbb neighbors
        0x00253513, 0x00052513, // arbitrary SLTIU and SLTI
        0x0205153b, 0x0205253b, 0x0205353b, // missing MULH word forms
        0x00052583, 0x00050583, 0x00055583, // non-admitted loads
        0x00004023, 0x00002063, 0x00003063, // reserved stores/branches
        0x0000000f, 0x0000002f, 0x00000053, 0x00100073, 0x00101073, // fence, A, FP, ebreak, CSR
    ] {
        assert!(decode(word).is_none(), "{word:#010x}");
    }
}

#[test]
fn constant_profiles_keep_initialization_and_instruction_policy_separate() {
    assert_eq!(constant_step(0x00150513, 10, None, ConstantInstructions::LogicalShifts), None);
    assert_eq!(constant_step(0xfff00513, 10, None, ConstantInstructions::LeftShift), Some(u64::MAX));
    assert_eq!(constant_step(0x00155513, 10, Some(u64::MAX), ConstantInstructions::LeftShift), None);
    assert_eq!(constant_step(0x00155513, 10, Some(u64::MAX), ConstantInstructions::LogicalShifts), Some(u64::MAX >> 1));
    assert_eq!(constant_step(0x0005051b, 10, Some(0x80000000), ConstantInstructions::LeftShift), Some(0xffffffff80000000));
}

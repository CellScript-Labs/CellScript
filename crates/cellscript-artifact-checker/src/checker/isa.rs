//! Independent RV64 word predicates used by artifact machine validation.
//! These decode instruction words without compiler or assembler dependencies.

pub(crate) fn is_addi(word: u32, rd: u32, rs1: u32, immediate: i32) -> bool {
    word & 0x7f == 0x13
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word as i32) >> 20 == immediate
}

pub(super) fn is_lui(word: u32, rd: u32, immediate: u32) -> bool {
    word & 0x7f == 0x37 && (word >> 7) & 0x1f == rd && word & 0xffff_f000 == immediate
}

pub(crate) fn is_ld(word: u32, rd: u32, rs1: u32, immediate: i32) -> bool {
    word & 0x7f == 0x03
        && (word >> 12) & 0x7 == 0x3
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word as i32) >> 20 == immediate
}

pub(crate) fn is_sd(word: u32, rs2: u32, rs1: u32, immediate: i32) -> bool {
    let decoded = (((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f);
    let decoded = ((decoded as i32) << 20) >> 20;
    word & 0x7f == 0x23
        && (word >> 12) & 0x7 == 0x3
        && (word >> 20) & 0x1f == rs2
        && (word >> 15) & 0x1f == rs1
        && decoded == immediate
}

pub(crate) fn is_sub(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0x20
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(super) fn is_add(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(super) fn is_mul(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0x01
        && (word >> 12) & 0x7 == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(crate) fn is_or(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 6
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(super) fn is_and(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 7
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(crate) fn is_sltu(word: u32, rd: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x33
        && (word >> 25) & 0x7f == 0
        && (word >> 12) & 0x7 == 3
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x1f == rs2
}

pub(crate) fn is_lbu(word: u32, rd: u32, rs1: u32, immediate: i32) -> bool {
    word & 0x7f == 0x03
        && (word >> 12) & 0x7 == 4
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word as i32) >> 20 == immediate
}

fn is_store(word: u32, function: u32, rs2: u32, rs1: u32, immediate: i32) -> bool {
    let decoded = (((word >> 25) & 0x7f) << 5) | ((word >> 7) & 0x1f);
    let decoded = ((decoded as i32) << 20) >> 20;
    word & 0x7f == 0x23
        && (word >> 12) & 0x7 == function
        && (word >> 20) & 0x1f == rs2
        && (word >> 15) & 0x1f == rs1
        && decoded == immediate
}

pub(super) fn is_sb(word: u32, rs2: u32, rs1: u32, immediate: i32) -> bool {
    is_store(word, 0, rs2, rs1, immediate)
}

pub(super) fn is_sw(word: u32, rs2: u32, rs1: u32, immediate: i32) -> bool {
    is_store(word, 2, rs2, rs1, immediate)
}

pub(super) fn is_srli(word: u32, rd: u32, rs1: u32, shift: u32) -> bool {
    word & 0x7f == 0x13
        && (word >> 12) & 0x7 == 5
        && (word >> 26) & 0x3f == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x3f == shift
}

pub(crate) fn is_slli(word: u32, rd: u32, rs1: u32, shift: u32) -> bool {
    word & 0x7f == 0x13
        && (word >> 12) & 0x7 == 1
        && (word >> 26) & 0x3f == 0
        && (word >> 7) & 0x1f == rd
        && (word >> 15) & 0x1f == rs1
        && (word >> 20) & 0x3f == shift
}

pub(crate) fn is_auipc(word: u32, rd: u32) -> bool {
    word & 0x7f == 0x17 && (word >> 7) & 0x1f == rd
}

pub(crate) fn is_jalr_call(word: u32) -> bool {
    word & 0x7f == 0x67 && (word >> 7) & 0x1f == 1 && (word >> 12) & 0x7 == 0 && (word >> 15) & 0x1f == 1
}

pub(crate) fn is_beq(word: u32, rs1: u32, rs2: u32) -> bool {
    is_branch(word, 0, rs1, rs2)
}

pub(crate) fn is_bne(word: u32, rs1: u32, rs2: u32) -> bool {
    is_branch(word, 1, rs1, rs2)
}

pub(super) fn is_bltu(word: u32, rs1: u32, rs2: u32) -> bool {
    is_branch(word, 6, rs1, rs2)
}

pub(super) fn is_bgeu(word: u32, rs1: u32, rs2: u32) -> bool {
    is_branch(word, 7, rs1, rs2)
}

fn is_branch(word: u32, function: u32, rs1: u32, rs2: u32) -> bool {
    word & 0x7f == 0x63 && (word >> 12) & 0x7 == function && (word >> 15) & 0x1f == rs1 && (word >> 20) & 0x1f == rs2
}

pub(crate) fn is_jal_zero(word: u32) -> bool {
    word & 0x7f == 0x6f && (word >> 7) & 0x1f == 0
}

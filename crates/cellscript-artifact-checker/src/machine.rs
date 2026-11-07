//! Independent decoding and effects for the checker's finite, four-byte RV64 surface.
//! This module never reads compiler assembly, plans, or producer effect summaries.

mod constants;
#[cfg(test)]
mod tests;

pub(crate) use constants::{constant_step, ConstantInstructions};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    Xlen,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alu {
    Add,
    Sub,
    ShiftLeft,
    LessSigned,
    LessUnsigned,
    Xor,
    ShiftRight,
    ShiftArithmetic,
    Or,
    And,
    Multiply,
    MultiplyHighSigned,
    MultiplyHighSignedUnsigned,
    MultiplyHighUnsigned,
    DivideSigned,
    DivideUnsigned,
    RemainderSigned,
    RemainderUnsigned,
    RotateRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Register(u8),
    Immediate(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    Equal,
    NotEqual,
    LessSigned,
    GreaterEqualSigned,
    LessUnsigned,
    GreaterEqualUnsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Alu { op: Alu, width: Width, rd: u8, rs1: u8, right: Operand },
    Lui { rd: u8, value: i64 },
    Auipc { rd: u8, offset: i64 },
    Load { rd: u8, base: u8, offset: i64, bytes: u8 },
    Store { source: u8, base: u8, offset: i64, bytes: u8 },
    Branch { condition: Condition, left: u8, right: u8, offset: i64 },
    Jal { rd: u8, offset: i64 },
    Jalr { rd: u8, base: u8, offset: i64 },
    Ecall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccess {
    Read,
    Write,
    /// The syscall-specific contract must prove actual addresses and extents.
    Syscall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryEffect {
    pub access: MemoryAccess,
    pub base: Option<u8>,
    pub offset: i64,
    pub bytes: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlFlow {
    Fallthrough,
    Conditional { offset: i64 },
    Direct { offset: i64 },
    Indirect { base: u8, offset: i64 },
    Syscall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effects {
    /// Hardware register dependencies. Reading x0 is not a mutable fact.
    pub reads: u32,
    /// Actual register writes; x0 is always excluded.
    pub writes: u32,
    pub memory: Option<MemoryEffect>,
    pub control: ControlFlow,
}

impl Effects {
    pub fn reads_register(self, register: u32) -> bool {
        register < 32 && self.reads & (1 << register) != 0
    }

    pub fn writes_register(self, register: u32) -> bool {
        register < 32 && self.writes & (1 << register) != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    operation: Operation,
}

fn bit(register: u8) -> u32 {
    if register == 0 {
        0
    } else {
        1 << register
    }
}

/// Decode only the existing admitted surface, not all instructions in the target ISA.
/// Compressed, reserved, atomic, CSR, floating-point, and fence encodings return None.
pub fn decode(word: u32) -> Option<Instruction> {
    use Alu::*;
    let opcode = word & 0x7f;
    let rd = (word >> 7 & 31) as u8;
    let rs1 = (word >> 15 & 31) as u8;
    let rs2 = (word >> 20 & 31) as u8;
    let function = word >> 12 & 7;
    let high = word >> 25;
    let immediate = i64::from(word as i32 >> 20);
    let upper = i64::from((word & 0xfffff000) as i32);
    let alu = |op, width, right| Operation::Alu { op, width, rd, rs1, right };
    let operation = match opcode {
        0x03 if matches!(function, 3 | 4) => {
            Operation::Load { rd, base: rs1, offset: immediate, bytes: if function == 3 { 8 } else { 1 } }
        }
        0x13 | 0x1b => {
            let width = if opcode == 0x1b { Width::Word } else { Width::Xlen };
            let op = match (opcode, function) {
                (_, 0) => Add,
                (0x13, 3) if word >> 20 == 1 => LessUnsigned,
                (0x13, 4) => Xor,
                (0x13, 6) => Or,
                (0x13, 7) => And,
                (0x13, 1) if word >> 26 == 0 => ShiftLeft,
                (0x1b, 1) if high == 0 => ShiftLeft,
                (0x13, 5) => match word >> 26 {
                    0 => ShiftRight,
                    0x10 => ShiftArithmetic,
                    0x18 => RotateRight,
                    _ => return None,
                },
                (0x1b, 5) => match high {
                    0 => ShiftRight,
                    0x20 => ShiftArithmetic,
                    0x30 => RotateRight,
                    _ => return None,
                },
                _ => return None,
            };
            let value = if matches!(op, ShiftLeft | ShiftRight | ShiftArithmetic | RotateRight) {
                i64::from(word >> 20 & if width == Width::Word { 31 } else { 63 })
            } else {
                immediate
            };
            alu(op, width, Operand::Immediate(value))
        }
        0x17 => Operation::Auipc { rd, offset: upper },
        0x37 => Operation::Lui { rd, value: upper },
        0x23 if function <= 3 => {
            let offset = (((word >> 25) << 5) | (word >> 7 & 31)) as i32;
            Operation::Store { source: rs2, base: rs1, offset: i64::from(offset << 20 >> 20), bytes: 1 << function }
        }
        0x33 | 0x3b => {
            let width = if opcode == 0x3b { Width::Word } else { Width::Xlen };
            let op = match (high, function) {
                (0, 0) => Add,
                (0, 1) => ShiftLeft,
                (0, 2) if width == Width::Xlen => LessSigned,
                (0, 3) if width == Width::Xlen => LessUnsigned,
                (0, 4) if width == Width::Xlen => Xor,
                (0, 5) => ShiftRight,
                (0, 6) if width == Width::Xlen => Or,
                (0, 7) if width == Width::Xlen => And,
                (0x20, 0) => Sub,
                (0x20, 5) => ShiftArithmetic,
                (1, 0) => Multiply,
                (1, 1) if width == Width::Xlen => MultiplyHighSigned,
                (1, 2) if width == Width::Xlen => MultiplyHighSignedUnsigned,
                (1, 3) if width == Width::Xlen => MultiplyHighUnsigned,
                (1, 4) => DivideSigned,
                (1, 5) => DivideUnsigned,
                (1, 6) => RemainderSigned,
                (1, 7) => RemainderUnsigned,
                _ => return None,
            };
            alu(op, width, Operand::Register(rs2))
        }
        0x63 => {
            let condition = match function {
                0 => Condition::Equal,
                1 => Condition::NotEqual,
                4 => Condition::LessSigned,
                5 => Condition::GreaterEqualSigned,
                6 => Condition::LessUnsigned,
                7 => Condition::GreaterEqualUnsigned,
                _ => return None,
            };
            let offset = ((word >> 31 & 1) << 12) | ((word >> 7 & 1) << 11) | ((word >> 25 & 63) << 5) | ((word >> 8 & 15) << 1);
            Operation::Branch { condition, left: rs1, right: rs2, offset: i64::from((offset as i32) << 19 >> 19) }
        }
        0x6f if matches!(rd, 0 | 1) => {
            let offset = ((word >> 31 & 1) << 20) | ((word >> 12 & 255) << 12) | ((word >> 20 & 1) << 11) | ((word >> 21 & 1023) << 1);
            Operation::Jal { rd, offset: i64::from((offset as i32) << 11 >> 11) }
        }
        0x67 if function == 0 => Operation::Jalr { rd, base: rs1, offset: immediate },
        0x73 if word == 0x00000073 => Operation::Ecall,
        _ => return None,
    };
    Some(Instruction { operation })
}

impl Instruction {
    pub const BYTE_LENGTH: u64 = 4;

    pub fn operation(self) -> Operation {
        self.operation
    }

    pub fn effects(self) -> Effects {
        let mut effects = Effects { reads: 0, writes: 0, memory: None, control: ControlFlow::Fallthrough };
        match self.operation {
            Operation::Alu { rd, rs1, right, .. } => {
                effects.reads = bit(rs1)
                    | match right {
                        Operand::Register(register) => bit(register),
                        Operand::Immediate(_) => 0,
                    };
                effects.writes = bit(rd);
            }
            Operation::Lui { rd, .. } | Operation::Auipc { rd, .. } => effects.writes = bit(rd),
            Operation::Load { rd, base, offset, bytes } => {
                effects.reads = bit(base);
                effects.writes = bit(rd);
                effects.memory = Some(MemoryEffect { access: MemoryAccess::Read, base: Some(base), offset, bytes: Some(bytes) });
            }
            Operation::Store { source, base, offset, bytes } => {
                effects.reads = bit(base) | bit(source);
                effects.memory = Some(MemoryEffect { access: MemoryAccess::Write, base: Some(base), offset, bytes: Some(bytes) });
            }
            Operation::Branch { left, right, offset, .. } => {
                effects.reads = bit(left) | bit(right);
                effects.control = ControlFlow::Conditional { offset };
            }
            Operation::Jal { rd, offset } => {
                effects.writes = bit(rd);
                effects.control = ControlFlow::Direct { offset };
            }
            Operation::Jalr { rd, base, offset } => {
                effects.reads = bit(base);
                effects.writes = bit(rd);
                effects.control = ControlFlow::Indirect { base, offset };
            }
            Operation::Ecall => {
                // Register transport follows this CKB profile. Actual memory and
                // whether execution returns require the separately checked syscall.
                effects.reads = (10..=17).fold(0, |mask, register| mask | (1 << register));
                effects.writes = bit(10);
                effects.memory = Some(MemoryEffect { access: MemoryAccess::Syscall, base: None, offset: 0, bytes: None });
                effects.control = ControlFlow::Syscall;
            }
        }
        effects
    }

    pub fn destination(self) -> Option<u8> {
        match self.operation {
            Operation::Alu { rd, .. }
            | Operation::Lui { rd, .. }
            | Operation::Auipc { rd, .. }
            | Operation::Load { rd, .. }
            | Operation::Jal { rd, .. }
            | Operation::Jalr { rd, .. } => Some(rd),
            Operation::Store { .. } | Operation::Branch { .. } => None,
            Operation::Ecall => Some(10),
        }
    }

    /// ABI havoc for an opaque call is separate from the instruction's link write.
    pub fn is_call(self) -> bool {
        matches!(self.operation, Operation::Jal { rd: 1, .. } | Operation::Jalr { rd: 1, .. })
    }

    pub fn constant_result(self, pc: u64, mut register: impl FnMut(u8) -> Option<u64>) -> Option<u64> {
        if self.destination() == Some(0) {
            return Some(0);
        }
        let mut read = |index| if index == 0 { Some(0) } else { register(index) };
        match self.operation {
            Operation::Lui { value, .. } => Some(value as u64),
            Operation::Auipc { offset, .. } => Some(pc.wrapping_add_signed(offset)),
            Operation::Jal { .. } | Operation::Jalr { .. } => Some(pc.wrapping_add(Self::BYTE_LENGTH)),
            Operation::Alu { op, width, rs1, right, .. } => {
                let left = read(rs1)?;
                let right = match right {
                    Operand::Register(index) => read(index)?,
                    Operand::Immediate(value) => value as u64,
                };
                Some(constants::evaluate(op, width, left, right))
            }
            Operation::Load { .. } | Operation::Store { .. } | Operation::Branch { .. } | Operation::Ecall => None,
        }
    }
}

pub fn caller_saved(register: u32) -> bool {
    matches!(register, 1 | 5..=7 | 10..=17 | 28..=31)
}

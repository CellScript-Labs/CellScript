use super::{decode, Alu, Operand, Operation, Width};

#[derive(Clone, Copy)]
pub(crate) enum ConstantInstructions {
    LeftShift,
    LogicalShifts,
}

/// Evaluate a register-local constant program without expanding a consumer's
/// allowed instruction policy or trusting an incoming register value.
pub(crate) fn constant_step(word: u32, register: u32, previous: Option<u64>, allowed: ConstantInstructions) -> Option<u64> {
    if register == 0 {
        return None;
    }
    let instruction = decode(word)?;
    if instruction.destination()? as u32 != register {
        return None;
    }
    match instruction.operation {
        Operation::Lui { value, .. } => Some(value as u64),
        Operation::Alu { op, width, rs1, right: Operand::Immediate(value), .. } => {
            if !matches!((op, width), (Alu::Add, _) | (Alu::ShiftLeft, Width::Xlen))
                && !(matches!(allowed, ConstantInstructions::LogicalShifts) && op == Alu::ShiftRight && width == Width::Xlen)
            {
                return None;
            }
            let source = if rs1 == 0 {
                0
            } else if u32::from(rs1) == register {
                previous?
            } else {
                return None;
            };
            Some(evaluate(op, width, source, value as u64))
        }
        _ => None,
    }
}

pub(super) fn evaluate(op: Alu, width: Width, left: u64, right: u64) -> u64 {
    use Alu::*;
    if width == Width::Word {
        let (left, right) = (left as u32, right as u32);
        let value = match op {
            Add => left.wrapping_add(right),
            Sub => left.wrapping_sub(right),
            ShiftLeft => left.wrapping_shl(right & 31),
            ShiftRight => left.wrapping_shr(right & 31),
            ShiftArithmetic => (left as i32).wrapping_shr(right & 31) as u32,
            RotateRight => left.rotate_right(right & 31),
            Multiply => left.wrapping_mul(right),
            DivideSigned => {
                if right == 0 {
                    u32::MAX
                } else {
                    (left as i32).wrapping_div(right as i32) as u32
                }
            }
            DivideUnsigned => left.checked_div(right).unwrap_or(u32::MAX),
            RemainderSigned => {
                if right == 0 {
                    left
                } else {
                    (left as i32).wrapping_rem(right as i32) as u32
                }
            }
            RemainderUnsigned => {
                if right == 0 {
                    left
                } else {
                    left % right
                }
            }
            // These operations have no admitted OP-32 encoding.
            LessSigned | LessUnsigned | Xor | Or | And | MultiplyHighSigned | MultiplyHighSignedUnsigned | MultiplyHighUnsigned => {
                unreachable!("decoder produced an unavailable word operation")
            }
        };
        return value as i32 as i64 as u64;
    }
    match op {
        Add => left.wrapping_add(right),
        Sub => left.wrapping_sub(right),
        ShiftLeft => left.wrapping_shl((right & 63) as u32),
        LessSigned => u64::from((left as i64) < (right as i64)),
        LessUnsigned => u64::from(left < right),
        Xor => left ^ right,
        ShiftRight => left.wrapping_shr((right & 63) as u32),
        ShiftArithmetic => (left as i64).wrapping_shr((right & 63) as u32) as u64,
        Or => left | right,
        And => left & right,
        Multiply => left.wrapping_mul(right),
        MultiplyHighSigned => (((left as i64 as i128) * (right as i64 as i128)) >> 64) as u64,
        MultiplyHighSignedUnsigned => (((left as i64 as i128) * i128::from(right)) >> 64) as u64,
        MultiplyHighUnsigned => ((u128::from(left) * u128::from(right)) >> 64) as u64,
        DivideSigned => {
            if right == 0 {
                u64::MAX
            } else {
                (left as i64).wrapping_div(right as i64) as u64
            }
        }
        DivideUnsigned => left.checked_div(right).unwrap_or(u64::MAX),
        RemainderSigned => {
            if right == 0 {
                left
            } else {
                (left as i64).wrapping_rem(right as i64) as u64
            }
        }
        RemainderUnsigned => {
            if right == 0 {
                left
            } else {
                left % right
            }
        }
        RotateRight => left.rotate_right((right & 63) as u32),
    }
}

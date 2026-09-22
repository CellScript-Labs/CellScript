//! One RV64 immediate plan shared by layout, relaxation and emission.
//!
//! All supported steps cost one cycle in the pinned CKB-VM model. Equal-size
//! plans therefore also tie on cycles; enum order followed by signed operand
//! order supplies the final stable ordering. The entry trampoline does not use
//! this planner and keeps its fixed reservation.

use super::{li_bits, li_fits_lui_addi_rv64, li_form, split_hi_lo, CompileError, LiForm, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Step {
    AddFromZero(i64),
    LoadUpper(i64),
    ShiftLeft(u8),
    Add(i64),
}

pub(super) fn plan(literal: i128) -> Result<Vec<Step>> {
    let bits = li_bits(literal)?;
    let fallback = legacy(literal, bits)?;
    let candidate = recursive(bits as i64);
    // Keep a correctness fallback even if a future candidate generator emits
    // an invalid immediate, shift or reconstructed value.
    let valid = evaluate(&candidate) == Some(bits);
    if valid && (candidate.len(), &candidate) < (fallback.len(), &fallback) {
        Ok(candidate)
    } else if evaluate(&fallback) == Some(bits) {
        Ok(fallback)
    } else {
        Err(CompileError::new("invalid RV64 immediate plan", crate::error::Span::default()))
    }
}

fn legacy(literal: i128, bits: u64) -> Result<Vec<Step>> {
    Ok(match li_form(literal) {
        LiForm::Addi => vec![Step::AddFromZero(literal as i64)],
        LiForm::Lui => vec![Step::LoadUpper((literal as i64) >> 12)],
        LiForm::LuiAddi => {
            let (high, low) = split_hi_lo(literal as i64)?;
            vec![Step::LoadUpper(high), Step::Add(low)]
        }
        LiForm::Large => {
            let bytes = bits.to_be_bytes();
            let mut steps = vec![Step::AddFromZero(i64::from(bytes[0]))];
            for byte in &bytes[1..] {
                steps.extend([Step::ShiftLeft(8), Step::Add(i64::from(*byte))]);
            }
            steps
        }
    })
}

fn recursive(value: i64) -> Vec<Step> {
    if (-2048..=2047).contains(&value) {
        return vec![Step::AddFromZero(value)];
    }
    let mut short = None;
    if li_fits_lui_addi_rv64(value) {
        let (high, low) = split_hi_lo(value).expect("validated LUI/ADDI range");
        let mut steps = vec![Step::LoadUpper(high)];
        if low != 0 {
            steps.push(Step::Add(low));
        }
        short = Some(steps);
    }
    // Sign-extend the low twelve bits before shifting the remaining value.
    // i128 subtraction avoids overflow for either signed RV64 endpoint.
    let low = (value << 52) >> 52;
    let high = ((i128::from(value) - i128::from(low)) >> 12) as i64;
    let extra = high.trailing_zeros().min(51);
    let mut steps = recursive(high >> extra);
    steps.push(Step::ShiftLeft((12 + extra) as u8));
    if low != 0 {
        steps.push(Step::Add(low));
    }
    if let Some(short) = short
        && (short.len(), &short) <= (steps.len(), &steps)
    {
        return short;
    }
    steps
}

fn evaluate(steps: &[Step]) -> Option<u64> {
    let mut value: Option<u64> = None;
    for step in steps {
        value = Some(match *step {
            Step::AddFromZero(imm) if (-2048..=2047).contains(&imm) => imm as u64,
            Step::LoadUpper(imm) if (-0x80000..=0x7ffff).contains(&imm) => (imm << 12) as u64,
            Step::ShiftLeft(shift) if (1..=63).contains(&shift) => value? << shift,
            Step::Add(imm) if (-2048..=2047).contains(&imm) => value?.wrapping_add_signed(imm),
            _ => return None,
        });
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_boundaries_sparse_patterns_and_full_width_samples_never_grow() {
        let mut samples = vec![0, 1, u64::MAX, i64::MIN as u64, i64::MAX as u64];
        for bit in 0..64 {
            let value = 1u64 << bit;
            samples.extend([value, value.wrapping_sub(1), value.wrapping_add(1), !value]);
        }
        let mut state = 0x6d75_a6e7_c4a4_ec17u64;
        for _ in 0..100_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            samples.push(state);
        }
        for bits in samples {
            for literal in [i128::from(bits), i128::from(bits as i64)] {
                let steps = plan(literal).unwrap();
                assert_eq!(evaluate(&steps), Some(bits), "{literal}");
                assert!(steps.len() <= legacy(literal, bits).unwrap().len());
                assert_eq!(steps, plan(literal).unwrap());
            }
        }
        assert!(plan(1i128 << 56).unwrap().len() <= 2);
        assert_eq!(plan(i128::from(u64::MAX)).unwrap(), vec![Step::AddFromZero(-1)]);
        assert!(plan(i128::from(u64::MAX) + 1).is_err());
        assert!(plan(i128::from(i64::MIN) - 1).is_err());
    }
}

//! One RV64 immediate plan shared by layout, relaxation and emission.
//!
//! All supported steps cost one cycle in the pinned CKB-VM model. Equal-size
//! plans therefore also tie on cycles. Existing constructions use enum order
//! followed by signed operand order; the low-mask candidate replaces them only
//! on a strict improvement. The entry trampoline does not use this planner and
//! keeps its fixed reservation.
//!
//! The recursive construction needs at most eight steps and includes every
//! legacy short form. The retired expansion is only a test reference; selected
//! plans must pass validation in every build profile or compilation fails.

use super::{li_bits, li_fits_lui_addi_rv64, split_hi_lo, CompileError, Result};
#[cfg(test)]
use super::{li_form, LiForm};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Step {
    AddFromZero(i64),
    LoadUpper(i64),
    ShiftLeft(u8),
    Add(i64),
    ShiftRight(u8),
}

pub(super) fn plan(literal: i128) -> Result<Vec<Step>> {
    let bits = li_bits(literal)?;
    let mut best = validate_plan(recursive(bits as i64), bits)?;
    // A nonempty low-bit mask can be built from all ones and a logical right
    // shift. Keep the 0.31 plan on ties, including its one-instruction cases.
    // Excluding both endpoints also excludes zero/64-bit shifts and overflow.
    if bits != 0 && bits != u64::MAX && bits & (bits + 1) == 0 {
        let mask = vec![Step::AddFromZero(-1), Step::ShiftRight(bits.leading_zeros() as u8)];
        if mask.len() < best.len() {
            best = validate_plan(mask, bits)?;
        }
    }
    Ok(best)
}

fn validate_plan(steps: Vec<Step>, bits: u64) -> Result<Vec<Step>> {
    // This is a release-build check, not a debug assertion. A broken planner
    // must reject compilation rather than hide the defect behind a fallback.
    if evaluate(&steps) != Some(bits) {
        return Err(CompileError::new("invalid RV64 immediate plan", crate::error::Span::default()));
    }
    Ok(steps)
}

// Frozen pre-0.31 expansion for differential and cost tests only.
#[cfg(test)]
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
    // high is nonzero here and |high >> extra| < |value|, so recursion ends.
    // For |value| <= 2^B, |high| <= 2^(B-12): starting at B=63, at most
    // three levels reach B=27, which fits LUI/ADDI. Each level adds at most
    // two steps, giving 3*2+2=8; short-form selection can only improve this.
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
            Step::ShiftRight(shift) if (1..=63).contains(&shift) => value? >> shift,
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
        let mut samples = vec![
            0,
            1,
            u64::MAX,
            i64::MIN as u64,
            i64::MAX as u64,
            0x1234_5678_9abc_def0,
            0x9e37_79b9_7f4a_7c15,
            0x5555_5555_5555_5555,
            0xaaaa_aaaa_aaaa_aaaa,
        ];
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
            let candidate = recursive(bits as i64);
            assert_eq!(evaluate(&candidate), Some(bits), "recursive reconstruction: {bits:#018x}");
            assert!(candidate.len() <= 8, "recursive bound: {bits:#018x}");
            for literal in [i128::from(bits), i128::from(bits as i64)] {
                let fallback = legacy(literal, bits).unwrap();
                let steps = plan(literal).unwrap();
                assert_eq!(evaluate(&steps), Some(bits), "{literal}");
                assert!(
                    (candidate.len(), &candidate) <= (fallback.len(), &fallback),
                    "retired fallback cannot improve either length or tie-breaking: {literal}"
                );
                assert!(steps.len() <= candidate.len(), "must not grow relative to 0.31");
                // Reproduce the retired selection policy, including ties and
                // strict mask improvement, to pin the returned Step sequence.
                let mut previous = std::cmp::min((candidate.len(), candidate.clone()), (fallback.len(), fallback)).1;
                if bits != 0 && bits != u64::MAX && bits & (bits + 1) == 0 && previous.len() > 2 {
                    previous = vec![Step::AddFromZero(-1), Step::ShiftRight(bits.leading_zeros() as u8)];
                }
                assert_eq!(steps, previous, "retirement must preserve output: {literal}");
                assert_eq!(steps, plan(literal).unwrap());
            }
        }
        assert_eq!(recursive(0x1234_5678_9abc_def0).len(), 8);
        assert!(plan(1i128 << 56).unwrap().len() <= 2);
        assert_eq!(plan(i128::from(u64::MAX)).unwrap(), vec![Step::AddFromZero(-1)]);
        assert!(plan(i128::from(u64::MAX) + 1).is_err());
        assert!(plan(i128::from(i64::MIN) - 1).is_err());
    }

    #[test]
    fn invalid_generated_plans_fail_closed() {
        use Step::*;

        let invalid = [
            (vec![], 0),
            (vec![Add(1)], 1),
            (vec![ShiftLeft(1)], 0),
            (vec![ShiftRight(1)], 0),
            (vec![AddFromZero(1)], 2),
            (vec![AddFromZero(-2049)], (-2049i64) as u64),
            (vec![AddFromZero(2048)], 2048),
            (vec![LoadUpper(-0x80001)], (-0x8000_1000i64) as u64),
            (vec![LoadUpper(0x80000)], 0x8000_0000),
            (vec![AddFromZero(0), Add(-2049)], (-2049i64) as u64),
            (vec![AddFromZero(0), Add(2048)], 2048),
            (vec![AddFromZero(1), ShiftLeft(0)], 1),
            (vec![AddFromZero(1), ShiftLeft(64)], 0),
            (vec![AddFromZero(-1), ShiftRight(0)], u64::MAX),
            (vec![AddFromZero(-1), ShiftRight(64)], 0),
            // A later overwrite must not hide an earlier malformed step.
            (vec![AddFromZero(2048), AddFromZero(1)], 1),
        ];
        for (steps, bits) in invalid {
            let error = validate_plan(steps.clone(), bits).expect_err("invalid generated plan must reject compilation");
            assert!(error.message.contains("invalid RV64 immediate plan"), "{steps:?}: {error:?}");
        }
    }

    #[test]
    fn low_masks_shorten_only_when_the_existing_plan_needs_more_than_two_steps() {
        for width in 1..64 {
            let bits = (1u64 << width) - 1;
            let previous = recursive(bits as i64);
            let actual = plan(i128::from(bits)).unwrap();
            assert_eq!(evaluate(&actual), Some(bits));
            if width >= 31 {
                assert_eq!(previous.len(), 3);
                assert_eq!(actual, vec![Step::AddFromZero(-1), Step::ShiftRight(64 - width)]);
            } else {
                assert_eq!(actual, previous, "preserve shorter and equal-size plans");
            }
        }
        for value in [0, -1, -2, i128::from(i64::MIN), i128::from(u64::MAX)] {
            assert!(!plan(value).unwrap().iter().any(|step| matches!(step, Step::ShiftRight(_))));
        }
        assert_eq!(evaluate(&[Step::AddFromZero(-1), Step::ShiftRight(0)]), None);
        assert_eq!(evaluate(&[Step::AddFromZero(-1), Step::ShiftRight(64)]), None);
    }
}

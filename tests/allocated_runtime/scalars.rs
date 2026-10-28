use super::*;

fn input(ty: &str, value: u64) -> cellscript::EntryWitnessArg {
    use cellscript::EntryWitnessArg as A;
    match ty {
        "u8" => A::U8(value as u8),
        "u16" => A::U16(value as u16),
        "u32" => A::U32(value as u32),
        "i32" => A::I32(value as i32),
        "u64" => A::U64(value),
        _ => unreachable!(),
    }
}

#[test]
fn allocated_scalar_definitions_execute_against_integer_models_and_guards() {
    for (ty, bits) in [("u8", 8), ("u16", 16), ("u32", 32), ("i32", 32), ("u64", 64)] {
        let source = allocated_scalar_definitions::source(ty);
        let mask = u64::MAX >> (64 - bits);
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, opt);
            for (raw, shift) in [(17u64, 0), (mask, bits - 1), (1u64 << (bits - 1), 1)] {
                let a = if ty == "i32" { raw as i32 as i64 as u64 } else { raw };
                let b = 3;
                let quotient = if ty == "i32" { (a as i64 / b as i64) as u64 } else { a / b };
                let remainder = if ty == "i32" { (a as i64 % b as i64) as u64 } else { a % b };
                let right = if ty == "i32" { ((a as i64) >> shift) as u64 } else { a >> shift };
                let expected = [
                    a,
                    b,
                    a.wrapping_add(b),
                    a.wrapping_sub(b),
                    a.wrapping_mul(b),
                    quotient,
                    remainder,
                    a & b,
                    a | b,
                    a ^ b,
                    a.wrapping_shl(shift),
                    right,
                    a.wrapping_neg(),
                ];
                let bytes =
                    expected.into_iter().flat_map(|value| (value & mask).to_le_bytes()[..bits as usize / 8].to_vec()).collect();
                let args = [
                    input(ty, a),
                    input(ty, b),
                    cellscript::EntryWitnessArg::Bool(shift == 0),
                    cellscript::EntryWitnessArg::U64(u64::from(shift)),
                    cellscript::EntryWitnessArg::Bytes(bytes),
                ];
                let result = execute_compiled(&compiled, &args);
                assert_eq!(result.exit_code, 0, "{ty}/O{opt}/a={a}/shift={shift}: {result:?}");
                for (divisor, shift, code) in [(0, 0, 20), (b, bits, 65), (b, u32::MAX, 65)] {
                    let mut invalid = args.clone();
                    invalid[1] = input(ty, divisor);
                    invalid[3] = cellscript::EntryWitnessArg::U64(u64::from(shift));
                    let result = execute_compiled(&compiled, &invalid);
                    assert_eq!(result.exit_code, code, "{ty}/O{opt}/divisor={divisor}/shift={shift}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn allocated_signed_literals_and_array_indices_keep_their_type() {
    let source = r#"module signed_literal_transport
const SEVEN: i32 = 7
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<i32> { items: 4 }
}

fn divide(value: i32) -> i32 { return value / SEVEN }
action inspect(witness input: [i32; 2], witness index: u64) -> u64 {
    let mut work = Work::new()
    let mut legacy: Vec<i32> = Vec::with_capacity(2)
    let mut literal: i32 = 7
    let cast = 7 as i32
    literal = -cast
    legacy.push(input[0])
    legacy.push(input[1])
    work.values.push(input[0])
    work.values.push(input[index])
    if literal >= 0 { return 61 }
    if (cast != SEVEN) { return 62 }
    if legacy[0] >= 0 { return 63 }
    if legacy[index] >= 0 { return 64 }
    if work.values.get(0) >= 0 { return 65 }
    if work.values.get(1) >= 0 { return 66 }
    if divide(input[0]) != -(1 as i32) { return 67 }
    return 0
}"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_checked(source, opt);
        for index in [0, 1] {
            let result = execute_compiled(
                &compiled,
                &[
                    cellscript::EntryWitnessArg::Bytes([(-7i32).to_le_bytes(), i32::MIN.to_le_bytes()].concat()),
                    cellscript::EntryWitnessArg::U64(index),
                ],
            );
            assert_eq!(result.exit_code, 0, "O{opt}/index={index}: {result:?}");
        }
    }
}

#[test]
fn allocated_scalar_comparisons_match_signed_unsigned_and_boolean_models() {
    use cellscript::EntryWitnessArg as A;
    for (ty, bits) in [("u8", 8), ("u16", 16), ("u32", 32), ("i32", 32), ("u64", 64), ("bool", 1)] {
        let source = allocated_scalar_definitions::comparison_source(ty);
        let max = u64::MAX >> (64 - bits);
        let high = 1u64 << (bits - 1);
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let compiled = compile_checked(&source, opt);
            for (a, b) in [(0, 0), (0, 1), (1, 0), (max, max), (max, 0), (0, max), (high, high - 1), (high - 1, high)] {
                let ordering = if ty == "i32" { (a as i32).cmp(&(b as i32)) } else { a.cmp(&b) };
                let predicates =
                    [ordering.is_eq(), !ordering.is_eq(), ordering.is_lt(), ordering.is_le(), ordering.is_gt(), ordering.is_ge()];
                let count = if ty == "bool" { 2 } else { 6 };
                let expected = predicates[..count].iter().enumerate().fold(0, |mask, (bit, value)| mask | (u64::from(*value) << bit));
                let args = if ty == "bool" {
                    [A::Bool(a != 0), A::Bool(b != 0), A::U64(expected)]
                } else {
                    [input(ty, a), input(ty, b), A::U64(expected)]
                };
                let result = execute_compiled(&compiled, &args);
                assert_eq!(result.exit_code, 0, "{ty}/O{opt}/{a}/{b}: {result:?}");
            }
        }
    }
}

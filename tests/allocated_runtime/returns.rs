use super::*;

#[test]
fn collection_helpers_receive_wide_literal_defaults_in_registers_and_on_stack() {
    for (ty, value) in [
        ("u128", "1512366075204170929049582354406559215"),
        ("[u8; 9]", "b\"abcdefghi\""),
        ("[u8; 32]", "b\"0123456789abcdef0123456789ABCDEF\""),
    ] {
        for padding in [0, 8] {
            let params = (0..padding).map(|i| format!("p{i}: u64, ")).collect::<String>();
            let args = "0, ".repeat(padding);
            let source = format!(
                r#"module collection_literal_defaults
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: OrderedMap<u64, {ty}> {{ items: 2 }}
}}
fn lookup({params}owner: &Work, key: u64, fallback: {ty}) -> {ty} {{
    return owner.values.get_or(key, fallback)
}}
action inspect(witness key: u64) -> u64 {{
    let mut work = Work::new()
    work.values.insert(7, {value})
    let result = lookup({args}&work, key, {value})
    require result == {value}
    return 0
}}"#
            );
            for opt_level in [0, 3] {
                let compiled = compile_checked(&source, opt_level);
                for key in [7, 9] {
                    let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::U64(key)]);
                    assert_eq!(result.exit_code, 0, "{ty}/padding={padding}/O{opt_level}/key={key}: {result:?}");
                }
            }
        }
    }
}

#[test]
fn allocated_tuple_returns_keep_whole_values_and_snapshots() {
    let source = r#"module tuple_return_values
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}

fn make(value: u64) -> (u64, u64) { return (value, 7) }
fn echo(value: (u64, u64)) -> (u64, u64) { return value }
fn total(value: (u64, u64)) -> u64 { return value.0 + value.1 }
action inspect(witness input: u64) -> u64 {
    let mut value = make(input)
    let saved = value
    value.0 += 1
    let forwarded = echo(saved)
    let mut work = Work::new()
    work.values.push(total(saved))
    work.values.push(total(forwarded))
    if saved.0 != input { return 61 }
    if saved.1 != 7 { return 62 }
    if forwarded.0 != input { return 63 }
    if forwarded.1 != 7 { return 64 }
    if value.0 != input + 1 { return 65 }
    if value.1 != 7 { return 66 }
    require work.values.get(0) == input + 7
    require work.values.get(1) == input + 7
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        for input in [1, 29] {
            let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::U64(input)]);
            assert_eq!(result.exit_code, 0, "O{opt}/input={input}: {result:?}");
        }
    }
}

#[test]
fn allocated_tuple_returns_copy_packed_and_wide_fields_before_other_calls() {
    for width in [1, 7, 8, 9, 32, 1024] {
        let source = format!(
            r#"module tuple_return_bytes
struct Count {{ value: u64 }}
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    values: Vec<u64> {{ items: 4 }}
}}
fn make(bytes: [u8; {width}], number: u64) -> ([u8; {width}], u64) {{
    return (bytes, number)
}}
fn count(value: u64) -> Count {{ return Count {{ value: value }} }}
fn forward(value: ([u8; {width}], u64), change: bool) -> ([u8; {width}], u64) {{
    let mut local = value
    if change {{ local.1 += 1 }}
    return local
}}
action inspect(witness bytes: [u8; {width}], witness other: [u8; {width}], witness change: bool) -> u64 {{
    let before = count(53)
    let mut original = make(bytes, 17)
    let saved = original
    let forwarded = forward(saved, change)
    let second = make(other, 29)
    let after = count(59)
    original.0 = other
    original.1 = 41
    let mut work = Work::new()
    work.values.push(saved.1)
    work.values.push(forwarded.1)
    if saved.0 != bytes {{ return 61 }}
    if forwarded.0 != bytes {{ return 62 }}
    if second.0 != other {{ return 63 }}
    if original.0 != other {{ return 64 }}
    if original.1 != 41 {{ return 65 }}
    if work.values.get(0) != 17 {{ return 66 }}
    if change {{ require work.values.get(1) == 18 }} else {{ require work.values.get(1) == 17 }}
    require second.1 == 29
    require before.value == 53
    require after.value == 59
    return 0
}}"#
        );
        for opt in [0, 3] {
            let compiled = compile_checked(&source, opt);
            for change in [false, true] {
                let result = execute_compiled(
                    &compiled,
                    &[
                        cellscript::EntryWitnessArg::Bytes((0..width).map(|i| (i % 251) as u8).collect()),
                        cellscript::EntryWitnessArg::Bytes(vec![253; width]),
                        cellscript::EntryWitnessArg::Bool(change),
                    ],
                );
                assert_eq!(result.exit_code, 0, "width={width}/O{opt}/change={change}: {result:?}");
            }
        }
    }
}

#[test]
fn allocated_tuple_returns_preserve_all_registers_and_large_field_offsets() {
    let source = r#"module tuple_return_registers
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn make(a: [u8; 1024], b: [u8; 1024], c: [u8; 9], n: u128) -> ([u8; 1024], [u8; 1024], [u8; 9], u128, u64, u32, u16, u8) {
    return (a, b, c, n, 17, 19, 23, 29)
}
fn forward(value: ([u8; 1024], [u8; 1024], [u8; 9], u128, u64, u32, u16, u8)) -> ([u8; 1024], [u8; 1024], [u8; 9], u128, u64, u32, u16, u8) {
    return value
}
action inspect(witness a: [u8; 1024], witness b: [u8; 1024], witness c: [u8; 9], witness n: u128) -> u64 {
    let first = make(a, b, c, n)
    let saved = forward(first)
    let second = make(b, a, c, n + 1)
    let mut work = Work::new()
    work.values.push(saved.4)
    if saved.0 != a { return 61 }
    if saved.1 != b { return 62 }
    if saved.2 != c { return 63 }
    if saved.3 != n { return 64 }
    if saved.4 != 17 { return 65 }
    if saved.5 != 19 { return 66 }
    if saved.6 != 23 { return 67 }
    if saved.7 != 29 { return 68 }
    require second.0 == b
    require second.1 == a
    require second.3 == n + 1
    require work.values.get(0) == 17
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        let result = execute_compiled(
            &compiled,
            &[
                cellscript::EntryWitnessArg::Bytes(vec![17; 1024]),
                cellscript::EntryWitnessArg::Bytes(vec![29; 1024]),
                cellscript::EntryWitnessArg::Bytes(vec![41; 9]),
                cellscript::EntryWitnessArg::U128((1u128 << 97) + 53),
            ],
        );
        assert_eq!(result.exit_code, 0, "O{opt}: {result:?}");
    }
}

#[test]
fn allocated_tuple_returns_preserve_small_nested_and_empty_values() {
    let source = r#"module small_tuple_returns
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u8> { items: 4 }
}
fn small(value: u8) -> (u8, bool, (u16, u8)) { return (value, true, (513, 7)) }
fn forward(value: (u8, bool, (u16, u8))) -> (u8, bool, (u16, u8)) { return value }
fn empty(value: u64) -> ((), u64, ()) { return ((), value, ()) }
fn signed(value: i32) -> (i32, bool, u16) { return (value, false, 1025) }
action inspect(witness input: u8, witness signed_input: i32) -> u64 {
    let mut local = small(input)
    let saved = forward(local)
    local.2.1 = 11
    let ignored = empty(19)
    let negative = signed(signed_input)
    let mut work = Work::new()
    work.values.push(saved.0)
    work.values.push(saved.2.1)
    if saved.0 != input { return 61 }
    if !saved.1 { return 62 }
    if saved.2.0 != 513 { return 63 }
    if saved.2.1 != 7 { return 64 }
    if local.2.1 != 11 { return 65 }
    if ignored.1 != 19 { return 66 }
    if negative.0 != signed_input { return 67 }
    if negative.1 { return 68 }
    if negative.2 != 1025 { return 69 }
    require work.values.get(0) == input
    require work.values.get(1) == 7
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::U8(251), cellscript::EntryWitnessArg::I32(-7)]);
        assert_eq!(result.exit_code, 0, "O{opt}: {result:?}");
    }
}

#[test]
fn allocated_tuple_returns_reject_register_overflow() {
    let source = r#"module tuple_register_overflow
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn make() -> (u64, u64, u64, u64, u64, u64, u64, u64, u64) {
    return (1, 2, 3, 4, 5, 6, 7, 8, 9)
}
action inspect() -> u64 {
    let values = make()
    let mut work = Work::new()
    work.values.push(values.0)
    require values.0 == 1
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        let result = execute_compiled(&compiled, &[]);
        assert_eq!(
            result.exit_code,
            cellscript::runtime_errors::CellScriptRuntimeError::FixedByteComparisonUnresolved.code() as i64,
            "O{opt}: {result:?}"
        );
    }
}

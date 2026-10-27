use super::*;

#[test]
fn allocated_projection_mutations_preserve_nested_record_values() {
    let source = r#"module nested_record_updates
struct Inner { amount: u64, code: [u8; 8] }
struct Record { head: u64, inner: Inner, tail: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<Record> { items: 4 }
}
action inspect(witness input: Record, witness next: [u8; 8]) -> u64 {
    let mut record = input
    let original = record
    let original_inner = record.inner
    record.inner.amount = 3
    let changed = record
    let mut work = Work::new()
    work.values.push(record)
    record.inner.code = next
    record.inner.amount += 1
    if original.inner.amount != 2 { return 61 }
    if original_inner.amount != 2 { return 62 }
    if changed.inner.amount != 3 { return 63 }
    let saved = work.values.get(0)
    if saved.inner.amount != 3 { return 64 }
    require saved.inner.code == input.inner.code
    require changed.inner.code == input.inner.code
    require record.inner.amount == 4
    require record.inner.code == next
    require record.head == input.head
    require record.tail == input.tail
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        let result = execute_compiled(
            &compiled,
            &[
                cellscript::EntryWitnessArg::Bytes(
                    [1u64.to_le_bytes().to_vec(), 2u64.to_le_bytes().to_vec(), vec![17; 8], 9u64.to_le_bytes().to_vec()].concat(),
                ),
                cellscript::EntryWitnessArg::Bytes(vec![29; 8]),
            ],
        );
        assert_eq!(result.exit_code, 0, "O{opt}: {result:?}");
    }
}

#[test]
fn allocated_projection_mutations_preserve_tuple_copies() {
    let source = r#"module tuple_updates
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn total(value: (u64, u64)) -> u64 { return value.0 + value.1 }
action inspect() -> u64 {
    let mut value = (1, 7)
    let original = value
    value.0 = 2
    let changed = value
    let mut work = Work::new()
    work.values.push(total(value))
    value.0 += 1
    if original.0 != 1 { return 61 }
    if changed.0 != 2 { return 62 }
    if total(changed) != 9 { return 63 }
    if work.values.get(0) != 9 { return 64 }
    require value.0 == 3
    require value.1 == 7
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        assert_eq!(execute_compiled(&compiled, &[]).exit_code, 0, "O{opt}");
    }
}

#[test]
fn allocated_projection_mutations_preserve_array_copies_and_sorted_inputs() {
    for element in ["u8", "u64"] {
        let source = format!(
            r#"module array_updates
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    keys: OrderedSet<{element}> {{ items: 4 }}
}}
action inspect() -> u64 {{
    let mut values: [{element}; 4] = [1, 3, 5, 7]
    let original = values
    values[1] = 4
    let changed = values
    let mut work = Work::new()
    work.keys.load_sorted(values)
    values[1] = 6
    if original[1] != 3 {{ return 61 }}
    if changed[1] != 4 {{ return 62 }}
    if !work.keys.contains(4) {{ return 63 }}
    require values[1] == 6
    require values[0] == 1
    require values[2] == 5
    require values[3] == 7
    return 0
}}"#
        );
        for opt in [0, 3] {
            let compiled = compile_checked(&source, opt);
            assert_eq!(execute_compiled(&compiled, &[]).exit_code, 0, "{element}/O{opt}");
        }
    }
}

#[test]
fn allocated_projection_mutations_follow_mixed_paths_and_loop_backedges() {
    let source = r#"module mixed_projection_updates
struct Record { head: u64, pair: ([u8; 8], u64), tail: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    keys: OrderedSet<u8> { items: 8 }
}
action inspect(witness change: bool) -> u64 {
    let mut record = Record { head: 13, pair: ([1, 3, 5, 7, 9, 11, 13, 15], 17), tail: 19 }
    if record.pair.0[1] != 3 { return 51 }
    let original = record
    if original.pair.0[1] != 3 { return 52 }
    if change { record.pair.0[1] = 4 }
    let changed = record
    let mut work = Work::new()
    work.keys.load_sorted(record.pair.0)
    record.pair.0[1] = 6
    record.pair.1 += 1
    if original.pair.0[1] != 3 { return 61 }
    if changed.pair.1 != 17 { return 62 }
    if record.pair.0[1] != 6 { return 63 }
    if record.pair.1 != 18 { return 64 }
    if record.head != 13 { return 65 }
    if record.tail != 19 { return 66 }
    if change {
        if changed.pair.0[1] != 4 { return 67 }
        if !work.keys.contains(4) { return 68 }
    } else {
        if changed.pair.0[1] != 3 { return 69 }
        if !work.keys.contains(3) { return 70 }
    }
    let mut values = [1, 3, 5, 7]
    let mut pair = (1, 7)
    for round in 0..2 {
        let snapshot = values
        let before = values[1]
        let (left, right) = pair
        let matched = match pair { (a, b) => a + b }
        let mut sum = 0
        for value in values { sum += value }
        if sum != 16 + round { return 71 }
        if matched != 8 + round { return 72 }
        values[1] += 1
        pair.0 += 1
        if snapshot[1] != 3 + round { return 73 }
        if before != 3 + round { return 74 }
        if left != 1 + round { return 75 }
        if right != 7 { return 76 }
    }
    require values[1] == 5
    require pair.0 == 3
    return 0
}"#;
    for opt in [0, 3] {
        let compiled = compile_checked(source, opt);
        for change in [false, true] {
            let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::Bool(change)]);
            assert_eq!(result.exit_code, 0, "O{opt}/change={change}: {result:?}");
        }
    }
}

#[test]
fn allocated_projection_mutations_reject_invalid_lvalues() {
    let template = r#"module projection_rejections
action inspect(witness index: u64) -> u64 {
    let mut values: [u8; 4] = [1, 3, 5, 7]
    UPDATE
    return 0
}"#;
    for (update, expected) in [
        ("values[4] = 9", "array index 4 is out of bounds"),
        ("values[index] = 9", "compile-time constant index"),
        ("values[0] = 256", "u8"),
    ] {
        let error = cellscript::compile(
            &template.replace("UPDATE", update),
            CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() },
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected), "{update}: {error}");
    }
    let immutable = template.replace("let mut values", "let values").replace("UPDATE", "values[0] = 9");
    let error =
        cellscript::compile(&immutable, CompileOptions { edition: CellScriptEdition::Edition2027, ..Default::default() }).unwrap_err();
    assert!(error.to_string().contains("not mutable"), "{error}");
}

#[test]
fn allocated_projection_mutations_accept_short_tuple_return_fields_on_all_paths() {
    for width in 1..=8 {
        let source = format!(
            r#"module short_return_fields
collections Work {{ limits {{ bytes: 512, allocations: 4 }}
    keys: OrderedSet<u8> {{ items: 8 }}
    values: Vec<[u8; {width}]> {{ items: 4 }}
}}
fn make(value: [u8; {width}]) -> ([u8; {width}], u64) {{ return (value, 7) }}
action inspect(witness input: [u8; {width}], witness first: bool) -> u64 {{
    let returned = make(input)
    let mut work = Work::new()
    if first {{ work.values.push(returned.0) }}
    work.keys.load_sorted(returned.0)
    work.values.push(returned.0)
    require returned.1 == 7
    require work.values.get(0) == input
    require work.keys.len() == {width}
    require work.keys.contains({width})
    return 0
}}"#
        );
        for opt in [0, 3] {
            let compiled = compile_checked(&source, opt);
            for first in [false, true] {
                let result = execute_compiled(
                    &compiled,
                    &[cellscript::EntryWitnessArg::Bytes((1..=width as u8).collect()), cellscript::EntryWitnessArg::Bool(first)],
                );
                assert_eq!(result.exit_code, 0, "{width}/O{opt}/first={first}: {result:?}");
            }
        }
    }
}

#[test]
fn allocated_projection_mutations_preserve_packed_and_maximum_arrays() {
    for count in [1, 8, 9, 32, 1024] {
        let source = format!(
            r#"module array_update_boundaries
collections Work {{ limits {{ bytes: 2048, allocations: 2 }}
    keys: OrderedSet<u8> {{ items: {count} }}
}}
action inspect(witness input: [u8; {count}]) -> u64 {{
    let mut values = input
    let original = values
    values[{last}] = 18
    let changed = values
    let mut work = Work::new()
    work.keys.load_sorted(values)
    values[0] = 16
    require original[{last}] == 17
    require changed[{last}] == 18
    require changed[0] == {saved_first}
    require values[0] == 16
    require values[{last}] == {current_last}
    require work.keys.contains(18)
    require work.keys.len() == {distinct}
    return 0
}}"#,
            last = count - 1,
            saved_first = if count == 1 { 18 } else { 17 },
            current_last = if count == 1 { 16 } else { 18 },
            distinct = if count == 1 { 1 } else { 2 },
        );
        for opt in [0, 3] {
            let compiled = compile_checked(&source, opt);
            let result = execute_compiled(&compiled, &[cellscript::EntryWitnessArg::Bytes(vec![17; count])]);
            assert_eq!(result.exit_code, 0, "{count}/O{opt}: {result:?}");
        }
    }
}

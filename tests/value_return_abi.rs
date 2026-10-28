//! Execute fixed ordinary struct and bounded byte-value returns with caller-owned result storage.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn u128_arguments_cross_register_stack_and_hidden_result_boundaries() {
    const SOURCE: &str = r#"module wide_arguments
struct Totals { total: u128, first: u128 }
fn sum_wide(a: u128, b: u128, c: u128, d: u128, e: u128) -> Totals {
    let left = a + b
    let right = c + d
    let subtotal = left + right
    return Totals { total: subtotal + e, first: a }
}
action verify(witness a: u128, witness b: u128, witness c: u128, witness d: u128, witness e: u128, witness expected: u128) {
    verification
    let result = sum_wide(a, b, c, d, e)
    require result.total == expected
    require result.first == a
}
"#;
    let mut measurements = Vec::new();
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile_with_executable_surface_policy(
            SOURCE,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        cellscript_artifact_checker::check_bundle_values(
            &compiled.artifact_bytes,
            &serde_json::to_value(&compiled.metadata).unwrap(),
            compiled.verified_lowering_record.as_ref().unwrap(),
            compiled.source_artifact_map.as_ref().unwrap(),
            &Default::default(),
        )
        .unwrap();
        let helper_frame = compiled
            .verified_lowering_record
            .as_ref()
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.name == "sum_wide")
            .unwrap()
            .frame_size_bytes;
        let mut cases = Vec::new();
        for values in
            [[0u128; 5], [(1u128 << 100) + 5, 7, (1u128 << 90) + 9, 11, 13], [u128::MAX, 0, 0, 0, 0], [u128::MAX, 1, 0, 0, 0]]
        {
            let total = values.iter().try_fold(0u128, |sum, value| sum.checked_add(*value));
            let mut args = values.into_iter().map(EntryWitnessArg::U128).collect::<Vec<_>>();
            args.push(EntryWitnessArg::U128(total.unwrap_or(0)));
            let payload = compiled.metadata.actions[0].entry_witness_args(&args).unwrap();
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.current_type_script_input_indices = vec![0];
            fixture.witnesses =
                vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
            let execution =
                ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
            let expected = if total.is_some() {
                0
            } else {
                cellscript::runtime_errors::CellScriptRuntimeError::AggregateAmountMismatch.code() as i64
            };
            assert_eq!(execution.exit_code, expected, "{diagnostics}: {execution:?}");
            if let Some(total) = total {
                cases.push(serde_json::json!({"total":total.to_string(),"cycles":execution.cycles}));
            }
        }
        measurements.push(serde_json::json!({"diagnostics":diagnostics.to_string(),"helper_frame_bytes":helper_frame,
            "elf_bytes":compiled.artifact_bytes.len(),"cases":cases}));
    }
    let report = serde_json::json!({"schema":"cellscript-wide-argument-measurements-v1", "compiler":cellscript::VERSION,
        "source_hash":hex::encode(cellscript::ckb_blake2b256(SOURCE.as_bytes())), "measurements":measurements});
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/wide-argument-measurements.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn fixed_result_source_field_uses_its_declared_offset() {
    let source = r#"
module field_result
struct Pair { left: u64, right: u64 }
struct Envelope { prefix: u64, value: Pair }
fn select(value: Envelope) -> Pair { value.value }
action verify(witness value: Envelope, witness expected: Pair) {
    verification
    let result = select(value)
    require result.left == expected.left
    require result.right == expected.right
}

"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let value = [99u64.to_le_bytes(), 11u64.to_le_bytes(), 23u64.to_le_bytes()].concat();
        let expected = [11u64.to_le_bytes(), 23u64.to_le_bytes()].concat();
        execute(source, diagnostics, &[EntryWitnessArg::Bytes(value), EntryWitnessArg::Bytes(expected)], 0);
    }
}

#[test]
fn fixed_byte_results_survive_forwarding_and_later_calls() {
    for (ty, width) in [
        ("[u8; 1]", 1),
        ("[u8; 3]", 3),
        ("[u8; 8]", 8),
        ("[u8; 9]", 9),
        ("[u8; 64]", 64),
        ("Hash", 32),
        ("Address", 32),
        ("ScriptHash", 32),
        ("SighashAllDigest", 32),
    ] {
        let source = format!(
            r#"
module byte_results
fn copy(value: {ty}) -> {ty} {{ let snapshot = value
return snapshot }}
fn relay(value: {ty}) -> {ty} {{ return copy(value) }}
action verify(witness first: {ty}, witness second: {ty}) {{
    verification
    let retained = relay(first)
    let later = relay(second)
    require retained == first
    require later == second
}}
"#
        );
        for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            execute(&source, opt, &[EntryWitnessArg::Bytes(vec![17; width]), EntryWitnessArg::Bytes(vec![29; width])], 0);
        }
    }
    let source = "module byte_literal\nfn value() -> [u8; 9] { return b\"constant!\" }\naction verify() { verification\nrequire value() == b\"constant!\" }";
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, opt, &[], 0);
    }
}

#[test]
fn short_byte_results_and_fields_preserve_register_and_stack_arguments() {
    let source = r#"
module short_arguments
struct Packet { prefix: u64, bytes: [u8; 3] }
fn word(value: [u8; 3]) -> [u8; 3] { value }
fn stacked(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, value: [u8; 3]) -> [u8; 3] {
    if a + b + c + d + e + f + g + h != 36 { return b"bad" }
    return word(value)
}
action verify(witness packet: Packet) {
    verification
    require stacked(1, 2, 3, 4, 5, 6, 7, 8, packet.bytes) == b"yes"
    require stacked(1, 2, 3, 4, 5, 6, 7, 8, word(b"yes")) == b"yes"
}
"#;
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (bytes, expected) in [(b"yes", 0), (b"no!", 5)] {
            execute(source, opt, &[EntryWitnessArg::Bytes([99u64.to_le_bytes().as_slice(), bytes].concat())], expected);
        }
    }
}

fn execute(source: &str, diagnostics: cellscript::CompilerDiagnostics, args: &[EntryWitnessArg], expected: i64) {
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { target: Some("riscv64-elf".into()), diagnostics, ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_or_else(|error| panic!("{diagnostics}: {error}\n{source}"));
    cellscript_artifact_checker::check_bundle_values(
        &compiled.artifact_bytes,
        &serde_json::to_value(&compiled.metadata).unwrap(),
        compiled.verified_lowering_record.as_ref().unwrap(),
        compiled.source_artifact_map.as_ref().unwrap(),
        &cellscript_artifact_checker::CheckerBudgets::default(),
    )
    .expect("fixed struct results must preserve the existing checked bundle boundary");
    let payload = compiled.metadata.actions[0].entry_witness_args(args).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    let result = ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
    assert_eq!(result.exit_code, expected, "{diagnostics}: {:?}", result.captured_debug);
}

#[test]
fn fixed_struct_return_storage_survives_nested_calls_and_both_branches() {
    for (ty, width) in [("u8", 1), ("u64", 8), ("u128", 16), ("Hash", 32), ("[u8; 260]", 260)] {
        let source = format!(
            r#"
module value_result
public struct Pair<T: fixed_value> {{ left: T, right: T }}
public fn swap<T: fixed_value>(value: Pair<T>) -> Pair<T> {{
    Pair<T> {{ left: value.right, right: value.left }}
}}
public fn relay<T: fixed_value>(value: Pair<T>, exchange: bool) -> Pair<T> {{
    if exchange {{ return swap<T>(value) }}
    return value
}}
action verify(witness input: Pair<{ty}>, witness expected: Pair<{ty}>, witness exchange: bool) {{
    verification
    let first: Pair<{ty}> = relay<{ty}>(input, exchange)
    let second: Pair<{ty}> = swap<{ty}>(input)
    require first.left == expected.left
    require first.right == expected.right
    require second.left == input.right
    require second.right == input.left
}}
"#
        );
        let left = vec![0x11; width];
        let right = vec![0x23; width];
        let input = [left.clone(), right.clone()].concat();
        let swapped = [right, left].concat();
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            for exchange in [false, true] {
                let expected = if exchange { swapped.clone() } else { input.clone() };
                let args = [EntryWitnessArg::Bytes(input.clone()), EntryWitnessArg::Bytes(expected), EntryWitnessArg::Bool(exchange)];
                execute(&source, diagnostics, &args, 0);
                let mut wrong = args.clone();
                let EntryWitnessArg::Bytes(bytes) = &mut wrong[1] else { unreachable!() };
                bytes[0] ^= 1;
                execute(&source, diagnostics, &wrong, 5);
            }
        }
    }
}

#[test]
fn fixed_struct_return_hidden_pointer_uses_outgoing_stack_without_overwriting_arguments() {
    let source = r#"
module stacked_value_result
struct Pair { left: u64, right: u64 }
fn calculate(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, value: Pair) -> Pair {
    Pair { left: value.right + a + b + c + d, right: value.left + e + f + g + h }
}
action verify(witness value: Pair, witness expected: Pair) {
    verification
    let result: Pair = calculate(1, 2, 3, 4, 5, 6, 7, 8, value)
    require result.left == expected.left
    require result.right == expected.right
}
"#;
    let args = [
        EntryWitnessArg::Bytes([11u64.to_le_bytes(), 23u64.to_le_bytes()].concat()),
        EntryWitnessArg::Bytes([33u64.to_le_bytes(), 37u64.to_le_bytes()].concat()),
    ];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &args, 0);
    }
}

#[test]
fn zero_width_struct_return_does_not_dereference_an_empty_buffer() {
    let source = r#"
module empty_value_result
struct Empty {}
fn make() -> Empty { Empty {} }
fn relay() -> Empty { make() }
action verify() { verification let value = relay() require true }
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &[], 0);
    }
}

#[test]
fn fixed_result_helper_restores_its_frame_after_nested_outgoing_stack_calls() {
    let source = r#"
module nested_stack_result
struct Pair { left: u64, right: u64 }
fn calculate(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64, value: Pair) -> Pair {
    Pair { left: value.right + a + b + c + d, right: value.left + e + f + g + h }
}
fn relay(value: Pair) -> Pair {
    let first = calculate(1, 2, 3, 4, 5, 6, 7, 8, value)
    calculate(8, 7, 6, 5, 4, 3, 2, 1, first)
}
action verify(witness value: Pair, witness expected: Pair) {
    verification
    let result = relay(value)
    require result.left == expected.left
    require result.right == expected.right
}
"#;
    let args = [
        EntryWitnessArg::Bytes([11u64.to_le_bytes(), 23u64.to_le_bytes()].concat()),
        EntryWitnessArg::Bytes([63u64.to_le_bytes(), 43u64.to_le_bytes()].concat()),
    ];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &args, 0);
    }
}

#[test]
fn constructed_result_uses_declaration_order_with_reordered_initializers() {
    let source = r#"
module result_field_order
fn build(left: u64, right: u64) -> Pair {
    Pair { right: right, left: left }
}
struct Pair { left: u64, right: u64 }
action verify(witness left: u64, witness right: u64) {
    verification
    let result: Pair = build(left, right)
    require result.left == left
    require result.right == right
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23)], 0);
    }
}

#[test]
fn fixed_struct_result_preserves_nested_values_and_zero_width_field_order() {
    let source = r#"
module nested_result_fields
struct EmptyZ {}
struct EmptyA {}
struct Inner { left: u64, right: u64 }
struct Payload { z_empty: EmptyZ, a_empty: EmptyA, inner: Inner, tail: u64 }
fn build(left: u64, right: u64) -> Payload {
    Payload {
        tail: 37,
        a_empty: EmptyA {},
        inner: Inner { right: right, left: left },
        z_empty: EmptyZ {}
    }
}
fn relay(left: u64, right: u64) -> Payload { build(left, right) }
action verify(witness left: u64, witness right: u64, witness expected: u64) {
    verification
    let result: Payload = relay(left, right)
    require result.inner.left == left
    require result.inner.right == right
    require result.tail == expected
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23), EntryWitnessArg::U64(37)], 0);
        execute(source, diagnostics, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23), EntryWitnessArg::U64(38)], 5);
    }
}

#[test]
fn mutable_wide_parameters_keep_the_callers_values() {
    let source = r#"module mutable_wide_arguments
struct Pair { left: u128, right: u128 }
fn bump_number(mut value: u128) -> u128 { value += 1 return value }
fn bump_pair(mut value: Pair) -> Pair {
    value.left += 1
    value.right += 2
    return value
}
action verify(witness input: Pair) {
    verification
    let before = input
    let changed = bump_pair(input)
    let number = bump_number(input.left)
    require input.left == 11 && input.right == 23
    require before.left == 11 && before.right == 23
    require changed.left == 12 && changed.right == 25
    require number == 12
}
"#;
    let args = [EntryWitnessArg::Bytes([11u128.to_le_bytes(), 23u128.to_le_bytes()].concat())];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &args, 0);
    }
}

#[test]
fn script_hash_bytes_keep_the_full_value_through_helper_results() {
    let source = r#"module script_hash_bytes
fn serialized(value: ScriptHash) -> [u8; 32] { return value.0 }
action verify(witness first: ScriptHash, witness second: ScriptHash) {
    verification
    let copied = serialized(first)
    let later = serialized(second)
    require copied == b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    require later == b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"
}
"#;
    let args = [EntryWitnessArg::Bytes(vec![65; 32]), EntryWitnessArg::Bytes(vec![66; 32])];
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        execute(source, diagnostics, &args, 0);
    }
}

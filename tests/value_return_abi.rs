//! Execute fixed ordinary struct returns with caller-owned result storage.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

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
    for opt_level in 0..=3 {
        let value = [99u64.to_le_bytes(), 11u64.to_le_bytes(), 23u64.to_le_bytes()].concat();
        let expected = [11u64.to_le_bytes(), 23u64.to_le_bytes()].concat();
        execute(source, opt_level, &[EntryWitnessArg::Bytes(value), EntryWitnessArg::Bytes(expected)], 0);
    }
}

fn execute(source: &str, opt_level: u8, args: &[EntryWitnessArg], expected: i64) {
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
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
    assert_eq!(result.exit_code, expected, "O{opt_level}: {:?}", result.captured_debug);
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
        for opt_level in 0..=3 {
            for exchange in [false, true] {
                let expected = if exchange { swapped.clone() } else { input.clone() };
                let args = [EntryWitnessArg::Bytes(input.clone()), EntryWitnessArg::Bytes(expected), EntryWitnessArg::Bool(exchange)];
                execute(&source, opt_level, &args, 0);
                let mut wrong = args.clone();
                let EntryWitnessArg::Bytes(bytes) = &mut wrong[1] else { unreachable!() };
                bytes[0] ^= 1;
                execute(&source, opt_level, &wrong, 5);
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
    for opt_level in 0..=3 {
        execute(source, opt_level, &args, 0);
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
    for opt_level in 0..=3 {
        execute(source, opt_level, &[], 0);
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
    for opt_level in 0..=3 {
        execute(source, opt_level, &args, 0);
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
    for opt_level in 0..=3 {
        execute(source, opt_level, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23)], 0);
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
    for opt_level in 0..=3 {
        execute(source, opt_level, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23), EntryWitnessArg::U64(37)], 0);
        execute(source, opt_level, &[EntryWitnessArg::U64(11), EntryWitnessArg::U64(23), EntryWitnessArg::U64(38)], 5);
    }
}

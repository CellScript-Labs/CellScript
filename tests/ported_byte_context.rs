//! Bounded Cell-data commitments and explicit fixed-byte identity checks.

use cellscript::{CompileOptions, CompileResult, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_hash::blake2b_256;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

use ckb_script_runner::{build_simple_fixture, execute_cellscript_script, execute_cellscript_script_with_transaction_transform};

fn compile(source: &str, diagnostics: CompilerDiagnostics) -> CompileResult {
    let compiled = cellscript::compile_with_executable_surface_policy(
        source,
        CompileOptions { diagnostics, target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .expect("bounded context program must compile");
    compiled.validate().expect("bounded context program must pass independent bundle checking");
    compiled
}

fn witness(compiled: &CompileResult, expected: [u8; 32]) -> Bytes {
    let payload = compiled.metadata.actions[0].entry_witness_args(&[EntryWitnessArg::Hash(expected)]).unwrap();
    packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()
}

#[test]
fn full_data_commitments_hash_empty_data_and_enforce_the_declared_bound() {
    let source = r#"module bounded_data_commitment
action inspect(witness expected: Hash) -> u64 {
    require ckb::cell_data_size(source::input(0)) <= 512
    require ckb::cell_data_blake2b_span(source::input(0), 0, ckb::cell_data_size(source::input(0))) == expected
    return 0
}"#;
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(source, diagnostics);
        for length in [0, 1, 32, 128, 129, 511, 512, 513] {
            let data: Vec<u8> = (0..length).map(|index| (index % 251) as u8).collect();
            let expected = blake2b_256(&data);
            let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = Bytes::copy_from_slice(&data);
            fixture.witnesses = vec![witness(&compiled, expected)];
            let result = execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(result.exit_code == 0, length <= 512, "{diagnostics}, length={length}: {result:?}");
            if length <= 512 {
                let mut wrong = expected;
                wrong[31] ^= 1;
                fixture.witnesses = vec![witness(&compiled, wrong)];
                assert_ne!(execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code, 0);
            }
        }
        let mut empty = build_simple_fixture(Bytes::new(), 1, 1);
        empty.witnesses = vec![witness(&compiled, [0; 32])];
        assert_ne!(
            execute_cellscript_script(&compiled.artifact_bytes, &empty).exit_code,
            0,
            "empty bytes still have a raw Blake2b digest"
        );
    }
}

#[test]
fn explicit_hash_from_a_cell_slice_preserves_exact_bytes_and_offset() {
    let source = r#"module exact_data_identity
action inspect(witness expected: Hash) -> u64 {
    require Hash::from_bytes(ckb::cell_data_bytes32(source::input(0), 7)) == expected
    return 0
}"#;
    let expected = std::array::from_fn::<_, 32, _>(|index| (3 + index * 7) as u8);
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(source, diagnostics);
        for remaining in [0, 1, 31, 32, 33, 96] {
            let mut data = vec![0x88; 7];
            data.extend_from_slice(&expected[..remaining.min(32)]);
            data.resize(7 + remaining, 0xee);
            let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = Bytes::from(data);
            fixture.witnesses = vec![witness(&compiled, expected)];
            let result = execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(result.exit_code == 0, remaining >= 32, "{diagnostics}, remaining={remaining}: {result:?}");
            if remaining >= 32 {
                fixture.witnesses = vec![witness(&compiled, blake2b_256(expected))];
                assert_ne!(
                    execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code,
                    0,
                    "conversion must not hash the bytes"
                );
            }
        }
    }
}

#[test]
fn explicit_hash_from_lock_args_requires_exactly_the_declared_bytes() {
    let source = r#"module exact_lock_identity
action inspect(witness expected: Hash) -> u64 {
    require Hash::from_bytes(ckb::cell_lock_args32(source::output(0))) == expected
    return 0
}"#;
    let expected = [0x5a; 32];
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(source, diagnostics);
        for length in [0, 31, 32, 33, 96] {
            for matching in [false, true] {
                let mut fixture = build_simple_fixture(Bytes::new(), 1, 1);
                fixture.witnesses = vec![witness(&compiled, expected)];
                let args = Bytes::from(vec![if matching { 0x5a } else { 0x5b }; length]);
                let result = execute_cellscript_script_with_transaction_transform(&compiled.artifact_bytes, &fixture, |tx, _| {
                    let output = tx.outputs().get(0).unwrap();
                    let lock = output.lock().as_builder().args(args.pack()).build();
                    tx.as_advanced_builder().set_outputs(vec![output.as_builder().lock(lock).build()]).build()
                });
                assert_eq!(
                    result.exit_code == 0,
                    matching && length == 32,
                    "{diagnostics}, length={length}, matching={matching}: {result:?}"
                );
            }
        }
    }
}

//! Keep requested capacity, reported remaining length and published length separate.
use cellscript::{compile_with_executable_surface_policy, CellScriptEdition, CompileOptions, ExecutableSurfacePolicy};
use ckb_testtool::{ckb_hash::blake2b_256, ckb_types::bytes::Bytes};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn amount_words_reject_partial_reads() {
    for opt_level in [0, 3] {
        for (method, minimum) in [("amount_low", 8), ("amount_high", 16)] {
            let source = format!("module amount_bounds\naction inspect() -> u64 {{ let amount = xudt::{method}(source::input(0)) require amount == amount return 0 }}");
            let result = compile_with_executable_surface_policy(
                &source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            for length in [0, 1, 7, 8, 9, 15, 16, 17, 64] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.inputs[0].data = vec![0x5a; length].into();
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code == 0, length >= minimum, "O{opt_level}, {method}, length={length}: {execution:?}");
            }
        }
    }
}

#[test]
fn legacy_witness_projection_rejects_a_table_larger_than_the_loaded_buffer() {
    for opt_level in [0, 3] {
        let result = compile_with_executable_surface_policy(
            "module witness_bounds\naction inspect() -> u64 { let value = witness::lock(source::input(0)) require value == value return 0 }",
            CompileOptions { edition: CellScriptEdition::Edition2027, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed).unwrap();
        for payload in [492usize, 493, 600] {
            let mut witness = Vec::new();
            for word in [20 + payload as u32, 16, 20 + payload as u32, 20 + payload as u32, payload as u32] {
                witness.extend_from_slice(&word.to_le_bytes());
            }
            witness.extend(vec![0x5a; payload]);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.witnesses = vec![witness.into()];
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, payload <= 492, "O{opt_level}, payload={payload}: {execution:?}");
        }
    }
}

#[test]
fn fixed_cell_data_slice_requires_a_complete_slice_but_allows_a_suffix() {
    for opt_level in [0, 3] {
        for remaining in [0usize, 1, 16, 31, 32, 33, 512, 600] {
            let source = r#"module checked_slice
action inspect() -> u64 {
    let value = ckb::cell_data_hash_at(source::input(0), 7)
    require value == value
    return 0
}"#;
            let result = compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition: CellScriptEdition::Edition2027,
                    opt_level,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            assert!(result
                .verified_lowering_record
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .any(|entry| entry.name == "__ckb_cell_data_hash_at"));
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = vec![0x5a; 7 + remaining].into();
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, remaining >= 32, "O{opt_level}, remaining={remaining}: {execution:?}");
            if remaining < 32 {
                assert_eq!(execution.exit_code, 1);
            }
        }
    }
}

#[test]
fn bounded_cell_data_hash_rejects_reported_lengths_larger_than_its_buffer() {
    for opt_level in [0, 3] {
        let result = compile_with_executable_surface_policy(
            "module checked_data_hash\naction inspect() -> u64 { let hash = ckb::cell_data_hash(source::input(0)) require hash == hash return 0 }",
            CompileOptions { edition: CellScriptEdition::Edition2027, opt_level, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        ).unwrap();
        assert!(result.verified_lowering_record.as_ref().unwrap().entries.iter().any(|entry| entry.name == "__ckb_cell_data_hash"));
        for length in [0usize, 1, 511, 512, 513, 600] {
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = vec![0x5a; length].into();
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, length <= 512, "O{opt_level}, length={length}: {execution:?}");
            if length > 512 {
                assert_eq!(execution.exit_code, 3);
            }
        }
    }
}

#[test]
fn checked_cell_reads_preserve_live_views_and_collection_copies() {
    let data = vec![0x5au8; 40];
    let hash = blake2b_256(&data).iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
    let source = format!(
        r#"module composed_cell_reads
resource Token has store {{ amount: u64 }}
collections Work {{ limits {{ bytes: 256, allocations: 2 }}
    values: Vec<[u8; 32]> {{ items: 2 }}
}}
fn inspect(view: InputView<Token>, work: &Work) -> bool {{
    let saved = work.values.get(0)
    let slice = ckb::cell_data_hash_at(source::input(0), 8)
    let hash = ckb::cell_data_hash(source::input(0))
    return view.capacity > 0 && slice == Hash::from_bytes(b"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ")
        && hash == Hash::from_bytes(b"{hash}") && saved == b"0123456789abcdef0123456789ABCDEF"
}}
action check() -> u64 {{
    let mut work = Work::new()
    work.values.push(b"0123456789abcdef0123456789ABCDEF")
    require inspect(ckb::input<Token>(0), &work)
    return 0
}}"#
    );
    for opt_level in [0, 3] {
        let result = compile_with_executable_surface_policy(
            &source,
            CompileOptions {
                edition: CellScriptEdition::Edition2027,
                opt_level,
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.inputs[0].data = data.clone().into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
    }
}

//! Behavioral stack-layout regressions: nested hashing/fixed results and large
//! collection frames with stack arguments, copied results and live Cell views.
use cellscript::{compile_with_executable_surface_policy, CellScriptEdition, CompileOptions, ExecutableSurfacePolicy};
use ckb_testtool::{ckb_hash::blake2b_256, ckb_types::bytes::Bytes};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn incoming_stack_arguments_cross_the_signed_immediate_boundary() {
    let parameters = (0..270).map(|index| format!("p{index}: u64")).collect::<Vec<_>>().join(", ");
    let arguments = (0..270).map(|index| (index + 100).to_string()).collect::<Vec<_>>().join(", ");
    // The first eight ABI words are registers: words 263/264 straddle
    // FP+2040/2048, the signed 12-bit load-immediate boundary.
    let source = format!(
        "module large_incoming_args\nfn last({parameters}) -> u64 {{ return p0 + p263 + p264 + p269 }}\naction inspect() -> u64 {{ require last({arguments}) == 1196 return 0 }}"
    );
    for opt_level in [0, 3] {
        let compiled = compile_with_executable_surface_policy(
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
        let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
        assert_eq!(execution.exit_code, 0, "O{opt_level}: {execution:?}");
    }
}

#[test]
fn legacy_witness_frames_preserve_padding_truncation_and_field_boundaries() {
    for width in [0usize, 1, 32, 33, 492] {
        let payload = (0..width).map(|index| (index + 1) as u8).collect::<Vec<_>>();
        let mut expected = [0u8; 32];
        let count = width.min(32);
        expected[..count].copy_from_slice(&payload[..count]);
        let literal = expected.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
        let source = format!(
            r#"module legacy_witness_stack
action inspect() -> u64 {{
    let first = witness::lock(source::input(0))
    let second = witness::lock(source::input(0))
    require first == Hash::from_bytes(b"{literal}")
    require second == first
    return 0
}}"#
        );
        let mut witness = Vec::new();
        for word in [20 + width as u32, 16, 20 + width as u32, 20 + width as u32, width as u32] {
            witness.extend_from_slice(&word.to_le_bytes());
        }
        witness.extend_from_slice(&payload);
        for opt_level in [0, 3] {
            let compiled = compile_with_executable_surface_policy(
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
            let entry = compiled
                .verified_lowering_record
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.name == "__ckb_witness_lock")
                .unwrap();
            assert_eq!(entry.frame_size_bytes, 592);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.witnesses = vec![witness.clone().into()];
            assert_eq!(ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code, 0);
            let mut malformed = witness.clone();
            malformed[8..12].copy_from_slice(&15u32.to_le_bytes());
            fixture.witnesses = vec![malformed.into()];
            assert_ne!(ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture).exit_code, 0);
        }
    }
}

#[test]
fn compact_out_point_and_product_frames_preserve_success_and_rejection() {
    let source = r#"module stack_requirements
fn products(a: u128, b: u128) -> u64 {
    c256::require_product_eq(a, b, a, b)
    c256::require_sum2_products_eq(a, b, b, a, a, b, b, a)
    return 0
}
action inspect() -> u64 {
    let input = source::input(0)
    let hash = ckb::input_out_point_tx_hash(input)
    require ckb::input_out_point_index(input) == 0
    ckb::require_input_out_point_tx_hash(input, hash)
    ckb::require_input_out_point(input, hash, 0)
    let a: u128 = 170141183460469231731687303715884105727
    let b: u128 = 3
    require products(a, b) == 0
    return 0
}"#;
    for opt_level in [0, 3] {
        for (source, succeeds) in [
            (source.to_string(), true),
            (source.replace("require_input_out_point(input, hash, 0)", "require_input_out_point(input, hash, 1)"), false),
            (
                source.replace("require_input_out_point_tx_hash(input, hash)", "require_input_out_point_tx_hash(input, Hash::zero())"),
                false,
            ),
            (source.replace("require_product_eq(a, b, a, b)", "require_product_eq(a, b, a, a)"), false),
        ] {
            let compiled = compile_with_executable_surface_policy(
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
            let record = compiled.verified_lowering_record.as_ref().unwrap();
            for (name, size) in [
                ("__ckb_require_input_out_point_tx_hash", 64),
                ("__ckb_require_input_out_point", 80),
                ("__c256_require_u128_product_eq", 112),
                ("__c256_require_u128_sum2_products_eq", 272),
            ] {
                assert_eq!(record.entries.iter().find(|entry| entry.name == name).unwrap().frame_size_bytes, size);
            }
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            let execution = ckb_script_runner::execute_cellscript_script(&compiled.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code == 0, succeeds, "O{opt_level}: {execution:?}");
        }
    }
}

#[test]
fn compact_cell_data_hash_frames_preserve_nested_calls_and_fixed_results() {
    for width in [0, 1, 512] {
        let data = vec![0x5au8; width];
        let expected = blake2b_256(&data).iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
        let source = format!(
            r#"module stack_hash_frames
fn digest() -> Hash {{ return ckb::cell_data_hash(source::input(0)) }}
action inspect() -> u64 {{
    let saved = digest()
    let again = digest()
    require saved == Hash::from_bytes(b"{expected}")
    require saved == again
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
            let hash = result
                .verified_lowering_record
                .as_ref()
                .unwrap()
                .entries
                .iter()
                .find(|entry| entry.name == "__ckb_cell_data_hash")
                .unwrap();
            assert_eq!(hash.frame_size_bytes, 544);
            let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            fixture.inputs[0].data = data.clone().into();
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
            fixture.inputs[0].data = vec![0; 513].into();
            assert_ne!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
        }
    }
}

#[test]
fn large_owner_frames_keep_stack_arguments_copies_and_cell_views_disjoint() {
    let source = r#"module stack_composition
resource Token has store { amount: u64 }
collections Work { limits { bytes: 8192, allocations: 4 }
    values: OrderedMap<u64, [u8; 32]> { items: 2 }
}
fn lookup(p0: u64, p1: u64, p2: u64, p3: u64, p4: u64, p5: u64, p6: u64, p7: u64,
    work: &Work, key: u64) -> [u8; 32] {
    return work.values.get_or(key, b"0123456789abcdef0123456789ABCDEF")
}
fn hashes(view: InputView<Token>) -> bool {
    let lock = view.lock_hash
    let current = ckb::current_script_hash()
    let transaction = ckb::transaction_hash()
    return view.capacity > 0 && lock == view.lock_hash
        && ckb::script_hash(current) == view.type_hash
        && transaction == ckb::transaction_hash()
}
action inspect() -> u64 {
    let mut work = Work::new()
    work.values.insert(7, b"0123456789abcdef0123456789ABCDEF")
    let saved = lookup(0, 0, 0, 0, 0, 0, 0, 0, &work, 7)
    require hashes(ckb::input<Token>(0))
    require saved == b"0123456789abcdef0123456789ABCDEF"
    require lookup(0, 0, 0, 0, 0, 0, 0, 0, &work, 9) == saved
    return 0
}"#;
    for opt_level in [0, 3] {
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
        assert!(result.verified_lowering_record.as_ref().unwrap().entries.iter().any(|entry| entry.frame_size_bytes > 8192));
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.current_type_script_input_indices = vec![0];
        fixture.inputs[0].data = vec![0; 8].into();
        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
    }
}

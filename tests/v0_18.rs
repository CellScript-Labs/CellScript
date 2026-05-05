use cellscript::{compile, CkbScriptArgsValue, CkbScriptHashTypeValue, CkbScriptValue, CompileOptions};
use ckb_testtool::ckb_types::{
    bytes::Bytes,
    core::{ScriptHashType, TransactionBuilder},
    packed,
    prelude::*,
};
use ckb_testtool::context::Context;

const SCRIPT_REF_READ_PROGRAM: &str = r#"
module v018::script_ref_read

action inspect(
    expected_lock_hash: Hash,
    expected_type_hash: Hash,
    expected_lock_code_hash: Hash,
    expected_type_code_hash: Hash,
    expected_lock_args_hash: Hash,
    expected_type_args_hash: Hash
) -> u64
where
    let input = source::input(0)
    let lock_hash: Hash = ckb::cell_lock_hash(input)
    let type_hash: Hash = ckb::cell_type_hash(input)
    let lock_code_hash: Hash = ckb::cell_lock_code_hash(input)
    let type_code_hash: Hash = ckb::cell_type_code_hash(input)
    let lock_hash_type = ckb::cell_lock_hash_type(input)
    let type_hash_type = ckb::cell_type_hash_type(input)
    let lock_args_empty = ckb::cell_lock_args_empty(input)
    let type_args_empty = ckb::cell_type_args_empty(input)
    let lock_args_hash: Hash = ckb::cell_lock_args_hash(input)
    let type_args_hash: Hash = ckb::cell_type_args_hash(input)
    require lock_hash == expected_lock_hash
    require type_hash == expected_type_hash
    require lock_code_hash == expected_lock_code_hash
    require type_code_hash == expected_type_code_hash
    require lock_args_hash == expected_lock_args_hash
    require type_args_hash == expected_type_args_hash
    ckb::require_cell_lock_args_prefix_hash(input, expected_lock_args_hash)
    ckb::require_cell_type_args_prefix_hash(input, expected_type_args_hash)
    ckb::require_cell_lock_args_suffix_hash(input, expected_lock_args_hash)
    ckb::require_cell_type_args_suffix_hash(input, expected_type_args_hash)
    let lock_empty_flag = if lock_args_empty { 1 } else { 0 }
    let type_empty_flag = if type_args_empty { 1 } else { 0 }
    return lock_hash_type + type_hash_type + lock_empty_flag + type_empty_flag
"#;

const SCRIPT_REF_PROPERTY_PROGRAM: &str = r#"
module v018::script_ref_property

action inspect(
    expected_lock_code_hash: Hash,
    expected_type_code_hash: Hash,
    expected_lock_args_hash: Hash,
    expected_type_args_hash: Hash
) -> u64
where
    let input = source::input(0)
    let lock = input.lock
    let type_script = input.type
    let lock_code_hash: Hash = lock.code_hash
    let type_code_hash: Hash = type_script.code_hash
    let lock_args_hash: Hash = input.lock.args_hash
    let type_args_hash: Hash = input.type.args_hash
    let lock_args_empty = input.lock.args_empty
    let type_args_empty = input.type.args_empty
    require lock_code_hash == expected_lock_code_hash
    require type_code_hash == expected_type_code_hash
    require lock_args_hash == expected_lock_args_hash
    require type_args_hash == expected_type_args_hash
    let lock_empty_flag = if lock_args_empty { 1 } else { 0 }
    let type_empty_flag = if type_args_empty { 1 } else { 0 }
    return lock.hash_type + type_script.hash_type + lock_empty_flag + type_empty_flag
"#;

const SCRIPT_CONSTRUCTION_PROGRAM: &str = r#"
module v018::script_construction

action inspect() -> u64
where
    let input = source::group_input(0)
    let code_hash: Hash = ckb::cell_lock_code_hash(input)
    let hash_type = ckb::cell_lock_hash_type(input)
    let args = script::args(b"owner")
    let expected = script::new(code_hash, hash_type, args)
    let same_code_hash: Hash = expected.code_hash
    require same_code_hash == code_hash
    require expected.hash_type == hash_type
    require expected.args.len == 5
    script::require_cell_lock_matches(input, expected)
    return 0
"#;

const SCRIPT_LITERAL_CONSTRUCTION_PROGRAM: &str = r#"
module v018::script_literal_construction

action inspect() -> u64
where
    let code_hash: Hash = Hash::from_bytes(b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f\x20")
    let expected = script::new(code_hash, script::hash_type_data2(), script::args_empty())
    let same_code_hash: Hash = expected.code_hash
    require same_code_hash == code_hash
    require expected.args.is_empty == true
    return expected.hash_type + expected.args.len
"#;

const CELL_DATA_DECODE_PROGRAM: &str = r#"
module v018::cell_data_decode

action inspect() -> u64
where
    let input = source::group_input(0)
    let quantity = ckb::cell_data_u32_le(input, 0)
    let amount = ckb::cell_data_u64_le(input, 4)
    if quantity != 7 {
        return 90
    }
    if amount != 123456789 {
        return 91
    }
    return 0
"#;

const OUT_POINT_API_PROGRAM: &str = r#"
module v018::out_point_api

action inspect() -> u64
where
    let input = source::group_input(0)
    let tx_hash: Hash = ckb::input_out_point_tx_hash(input)
    let index = ckb::input_out_point_index(input)
    ckb::require_input_out_point_tx_hash(input, tx_hash)
    ckb::require_input_out_point(input, tx_hash, index)
    return 0
"#;

const OUT_POINT_OUTPUT_REJECT_PROGRAM: &str = r#"
module v018::out_point_output_reject

action inspect() -> u64
where
    let output = source::output(0)
    let tx_hash: Hash = ckb::input_out_point_tx_hash(output)
    ckb::require_input_out_point_tx_hash(output, tx_hash)
    return 0
"#;

const ALWAYS_SUCCESS_LOCK_PROGRAM: &str = r#"
module v018::always_success_lock

action always_success() -> u64
where
    return 0
"#;

fn compile_source_to_elf(source: &str, entry_action: &str) -> Vec<u8> {
    let result = compile(
        source,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .unwrap_or_else(|err| panic!("failed to compile {entry_action}: {}", err.message));
    assert!(result.metadata.actions.iter().any(|action| action.name == entry_action), "entry action {entry_action} not found");
    cellscript::strip_vm_abi_trailer(&result.artifact_bytes).to_vec()
}

#[test]
fn v0_18_script_ref_reads_lower_to_fail_closed_ckb_helpers() {
    let result = compile(
        SCRIPT_REF_READ_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 ScriptRef read program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__ckb_cell_lock_code_hash",
        "__ckb_cell_type_code_hash",
        "__ckb_cell_lock_hash",
        "__ckb_cell_type_hash",
        "__ckb_cell_lock_hash_type",
        "__ckb_cell_type_hash_type",
        "__ckb_cell_lock_args_empty",
        "__ckb_cell_type_args_empty",
        "__ckb_cell_lock_args_hash",
        "__ckb_cell_type_args_hash",
        "__ckb_require_cell_lock_args_prefix_hash",
        "__ckb_require_cell_type_args_prefix_hash",
        "__ckb_require_cell_lock_args_suffix_hash",
        "__ckb_require_cell_type_args_suffix_hash",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing helper {helper}:\n{assembly}");
    }
    assert!(
        assembly.contains("read-only ScriptRef Hash field")
            && assembly.contains("read-only ScriptRef scalar field")
            && assembly.contains("load SourceView ScriptRef hash field into addressable Hash"),
        "ScriptRef reads must be explicit runtime extraction helpers:\n{assembly}"
    );
    assert!(
        assembly.contains("first 32 bytes == expected hash") && assembly.contains("last 32 bytes == expected hash"),
        "ScriptArgs prefix/suffix requirements must be visible in generated helpers:\n{assembly}"
    );
    assert!(
        assembly.contains("scalar runtime helper status check (a1 == 0)"),
        "scalar ScriptRef reads must fail closed on helper status:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-source-view".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-source-cell-fields".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-ref-read".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-args-read".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    for operation in [
        "cell-lock-script-code-hash-read",
        "cell-type-script-code-hash-read",
        "cell-lock-hash-read",
        "cell-type-hash-read",
        "cell-lock-script-hash-type-read",
        "cell-type-script-hash-type-read",
        "cell-lock-script-args-empty-read",
        "cell-type-script-args-empty-read",
        "cell-lock-script-args-hash-read",
        "cell-type-script-args-hash-read",
        "cell-lock-script-prefix-hash-args-require",
        "cell-type-script-prefix-hash-args-require",
        "cell-lock-script-suffix-hash-args-require",
        "cell-type-script-suffix-hash-args-require",
    ] {
        assert!(accesses.contains(&(operation, "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    }

    let elf = compile(
        SCRIPT_REF_READ_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 ScriptRef read program should assemble to ELF");
    assert!(!elf.artifact_bytes.is_empty());
}

#[test]
fn v0_18_cell_data_le_decoders_lower_to_fail_closed_ckb_helpers() {
    let result = compile(
        CELL_DATA_DECODE_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("cell data LE decoder program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in ["__ckb_cell_data_u32_le", "__ckb_cell_data_u64_le"] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing helper {helper}:\n{assembly}");
    }
    assert!(
        assembly.contains("little-endian u32 read via LOAD_CELL_DATA")
            && assembly.contains("little-endian u64 read via LOAD_CELL_DATA")
            && assembly.contains("scalar runtime helper status check (a1 == 0)"),
        "cell data decoders must use fail-closed LOAD_CELL_DATA helpers:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-source-cell-fields".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-cell-data-decode".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    assert!(accesses.contains(&("cell-data-u32-le", "LOAD_CELL_DATA", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("cell-data-u64-le", "LOAD_CELL_DATA", "SourceView")), "{accesses:?}");
}

#[test]
fn v0_18_cell_data_le_decoders_run_in_ckb_vm() {
    let type_elf = compile_source_to_elf(CELL_DATA_DECODE_PROGRAM, "inspect");
    let always_success_elf = compile_source_to_elf(ALWAYS_SUCCESS_LOCK_PROGRAM, "always_success");

    fn run_with_data(type_elf: &[u8], always_success_elf: &[u8], data: Bytes) -> Result<u64, String> {
        let mut context = Context::new_with_deterministic_rng();
        let always_success_out_point = context.deploy_cell(Bytes::copy_from_slice(always_success_elf));
        let type_out_point = context.deploy_cell(Bytes::copy_from_slice(type_elf));
        let lock = context.build_script(&always_success_out_point, Bytes::default()).expect("always success lock");
        let type_script = context.build_script(&type_out_point, Bytes::default()).expect("type script under test");

        let input = context.create_cell(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                .lock(lock.clone())
                .type_(packed::ScriptOpt::from(type_script))
                .build(),
            data,
        );
        let output = packed::CellOutput::new_builder().capacity::<packed::Uint64>(100_000_000_000u64.pack()).lock(lock).build();
        let tx = TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(input).build())
            .output(output)
            .output_data(Bytes::default().pack())
            .witness(Bytes::default().pack())
            .build();
        let tx = context.complete_tx(tx);
        context.verify_tx(&tx, 10_000_000).map_err(|err| format!("{err:?}"))
    }

    let mut good = Vec::new();
    good.extend_from_slice(&7u32.to_le_bytes());
    good.extend_from_slice(&123_456_789u64.to_le_bytes());
    let pass_cycles = run_with_data(&type_elf, &always_success_elf, Bytes::from(good)).expect("LE data decoder pass");
    assert!(pass_cycles > 0);

    let reject = run_with_data(&type_elf, &always_success_elf, Bytes::from(vec![7, 0, 0, 0, 1, 2, 3]))
        .expect_err("short u64 data read must fail closed");
    assert!(reject.contains("error") || reject.contains("ValidationFailure"), "{reject}");
}

#[test]
fn v0_18_out_point_tx_hash_read_lowers_to_addressable_hash() {
    let result = compile(
        OUT_POINT_API_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("OutPoint API program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__ckb_input_out_point_tx_hash",
        "__ckb_input_out_point_index",
        "__ckb_require_input_out_point_tx_hash",
        "__ckb_require_input_out_point",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing helper {helper}:\n{assembly}");
    }
    assert!(
        assembly.contains("OutPoint full tx-hash read")
            && assembly.contains("load SourceView input OutPoint tx hash into addressable Hash")
            && assembly.contains("OutPoint full tx-hash + index requirement"),
        "OutPoint API must expose full tx-hash reads and binding checks:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-source-input-out-point".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-source-cell-fields".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    assert!(accesses.contains(&("input-out-point-tx-hash-read", "LOAD_INPUT_BY_FIELD", "SourceView")), "{accesses:?}");
    assert!(accesses.contains(&("input-out-point-require", "LOAD_INPUT_BY_FIELD", "SourceView")), "{accesses:?}");
}

#[test]
fn v0_18_out_point_tx_hash_read_runs_in_ckb_vm() {
    let type_elf = compile_source_to_elf(OUT_POINT_API_PROGRAM, "inspect");
    let reject_type_elf = compile_source_to_elf(OUT_POINT_OUTPUT_REJECT_PROGRAM, "inspect");
    let always_success_elf = compile_source_to_elf(ALWAYS_SUCCESS_LOCK_PROGRAM, "always_success");

    fn run(type_elf: &[u8], always_success_elf: &[u8]) -> Result<u64, String> {
        let mut context = Context::new_with_deterministic_rng();
        let always_success_out_point = context.deploy_cell(Bytes::copy_from_slice(always_success_elf));
        let type_out_point = context.deploy_cell(Bytes::copy_from_slice(type_elf));
        let lock = context.build_script(&always_success_out_point, Bytes::default()).expect("always success lock");
        let type_script = context.build_script(&type_out_point, Bytes::default()).expect("type script under test");

        let input = context.create_cell(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                .lock(lock.clone())
                .type_(packed::ScriptOpt::from(type_script.clone()))
                .build(),
            Bytes::default(),
        );
        let output = packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(lock)
            .type_(packed::ScriptOpt::from(type_script))
            .build();
        let tx = TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(input).build())
            .output(output)
            .output_data(Bytes::default().pack())
            .witness(Bytes::default().pack())
            .build();
        let tx = context.complete_tx(tx);
        context.verify_tx(&tx, 10_000_000).map_err(|err| format!("{err:?}"))
    }

    let pass_cycles = run(&type_elf, &always_success_elf).expect("OutPoint tx-hash read should pass for input SourceView");
    assert!(pass_cycles > 0);

    let reject =
        run(&reject_type_elf, &always_success_elf).expect_err("OutPoint tx-hash read must reject non-input SourceView at runtime");
    assert!(reject.contains("error") || reject.contains("ValidationFailure"), "{reject}");
}

#[test]
fn v0_18_script_ref_property_surface_lowers_to_same_helpers() {
    let result = compile(
        SCRIPT_REF_PROPERTY_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 ScriptRef property program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__ckb_cell_lock_code_hash",
        "__ckb_cell_type_code_hash",
        "__ckb_cell_lock_hash_type",
        "__ckb_cell_type_hash_type",
        "__ckb_cell_lock_args_empty",
        "__ckb_cell_type_args_empty",
        "__ckb_cell_lock_args_hash",
        "__ckb_cell_type_args_hash",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing property helper {helper}:\n{assembly}");
    }
    assert!(
        assembly.contains("read-only ScriptRef Hash field") && assembly.contains("read-only ScriptRef scalar field"),
        "property ScriptRef reads must reuse the runtime extraction helpers:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-script-ref-read".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-args-read".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str()))
        .collect::<Vec<_>>();
    for operation in [
        "cell-lock-script-code-hash-read",
        "cell-type-script-code-hash-read",
        "cell-lock-script-hash-type-read",
        "cell-type-script-hash-type-read",
        "cell-lock-script-args-empty-read",
        "cell-type-script-args-empty-read",
        "cell-lock-script-args-hash-read",
        "cell-type-script-args-hash-read",
    ] {
        assert!(accesses.contains(&(operation, "LOAD_CELL_BY_FIELD", "SourceView")), "{accesses:?}");
    }

    let elf = compile(
        SCRIPT_REF_PROPERTY_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 ScriptRef property program should assemble to ELF");
    assert!(!elf.artifact_bytes.is_empty());
}

#[test]
fn v0_18_script_construction_lowers_to_exact_script_requirements() {
    let result = compile(
        SCRIPT_CONSTRUCTION_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 Script construction program should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    for helper in [
        "__ckb_cell_lock_code_hash",
        "__ckb_cell_lock_hash_type",
        "__ckb_require_cell_lock_script_hash_type",
        "__ckb_require_cell_lock_args_exact",
    ] {
        assert!(assembly.contains(&format!(".global {helper}")), "missing constructed Script helper {helper}:\n{assembly}");
    }
    assert!(
        assembly.contains("Script arbitrary exact args requirement")
            && assembly.contains("validates Molecule packed::Script args Bytes exactly"),
        "constructed Script args must lower to arbitrary exact args verification:\n{assembly}"
    );

    let features = &result.metadata.runtime.ckb_runtime_features;
    assert!(features.contains(&"ckb-script-construction".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-identity-requirements".to_string()), "{features:?}");
    assert!(features.contains(&"ckb-script-args-requirements".to_string()), "{features:?}");

    let accesses = result
        .metadata
        .runtime
        .ckb_runtime_accesses
        .iter()
        .map(|access| (access.operation.as_str(), access.syscall.as_str(), access.source.as_str(), access.binding.as_str()))
        .collect::<Vec<_>>();
    assert!(
        accesses.contains(&(
            "cell-lock-script-exact-args-require",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "script::require_cell_lock_matches",
        )),
        "{accesses:?}"
    );

    let elf = compile(
        SCRIPT_CONSTRUCTION_PROGRAM,
        CompileOptions {
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("0.18 Script construction program should assemble to ELF");
    assert!(!elf.artifact_bytes.is_empty());
}

#[test]
fn v0_18_script_value_encoding_matches_ckb_types_packed_script() {
    let code_hash = [7u8; 32];
    let args = vec![1u8, 3, 5, 7, 9, 11, 13];
    let value = CkbScriptValue::new(code_hash, CkbScriptHashTypeValue::Data1, CkbScriptArgsValue::exact(args.clone()));
    let ckb_script = packed::Script::new_builder()
        .code_hash(code_hash.pack())
        .hash_type(ScriptHashType::Data1)
        .args(Bytes::from(args).pack())
        .build();

    assert_eq!(value.packed_bytes(), ckb_script.as_slice());
    let expected_hash: [u8; 32] = ckb_script.calc_script_hash().unpack();
    assert_eq!(value.hash(), expected_hash);
}

#[test]
fn v0_18_constructed_script_exact_args_runs_in_ckb_vm() {
    let type_elf = compile_source_to_elf(SCRIPT_CONSTRUCTION_PROGRAM, "inspect");
    let always_success_elf = compile_source_to_elf(ALWAYS_SUCCESS_LOCK_PROGRAM, "always_success");

    fn run_with_lock_args(type_elf: &[u8], always_success_elf: &[u8], lock_args: Vec<u8>) -> Result<u64, String> {
        let mut context = Context::new_with_deterministic_rng();
        let always_success_out_point = context.deploy_cell(Bytes::copy_from_slice(always_success_elf));
        let type_out_point = context.deploy_cell(Bytes::copy_from_slice(type_elf));
        let input_lock = context.build_script(&always_success_out_point, Bytes::from(lock_args)).expect("input lock script");
        let output_lock = context.build_script(&always_success_out_point, Bytes::default()).expect("output lock script");
        let type_script = context.build_script(&type_out_point, Bytes::default()).expect("type script under test");

        let input = context.create_cell(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(100_000_000_000u64.pack())
                .lock(input_lock)
                .type_(packed::ScriptOpt::from(type_script.clone()))
                .build(),
            Bytes::default(),
        );
        let output = packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(100_000_000_000u64.pack())
            .lock(output_lock)
            .type_(packed::ScriptOpt::from(type_script))
            .build();
        let tx = TransactionBuilder::default()
            .input(packed::CellInput::new_builder().previous_output(input).build())
            .output(output)
            .output_data(Bytes::default().pack())
            .witness(Bytes::default().pack())
            .build();
        let tx = context.complete_tx(tx);
        context.verify_tx(&tx, 10_000_000).map_err(|err| format!("{err:?}"))
    }

    let pass_cycles =
        run_with_lock_args(&type_elf, &always_success_elf, b"owner".to_vec()).expect("matching constructed Script args pass");
    assert!(pass_cycles > 0);

    let reject = run_with_lock_args(&type_elf, &always_success_elf, b"wrong".to_vec())
        .expect_err("mismatched constructed Script args must reject");
    assert!(reject.contains("error") || reject.contains("Script"), "{reject}");
}

#[test]
fn v0_18_script_ref_reads_reject_non_source_view_arguments() {
    let err = compile(
        r#"
module v018::bad_script_ref_read

action inspect(flag: bool) -> Hash
where
    return ckb::cell_lock_code_hash(flag)
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("ScriptRef reads must reject non-SourceView arguments");

    assert!(
        err.message.contains("cell_lock_code_hash expects a source view returned by source::*"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn v0_18_script_ref_property_rejects_unknown_script_field() {
    let err = compile(
        r#"
module v018::bad_script_ref_property

action inspect() -> Hash
where
    let input = source::group_input(0)
    return input.lock.owner_hash
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("ScriptRef property surface must reject unknown fields");

    assert!(
        err.message.contains("unknown ScriptRef field 'owner_hash'; expected code_hash, hash_type, args_empty, or args_hash"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn v0_18_script_args_prefix_suffix_require_hash_operands() {
    let err = compile(
        r#"
module v018::bad_script_args_hash

action inspect() -> u64
where
    let input = source::group_input(0)
    ckb::require_cell_lock_args_prefix_hash(input, 1)
    return 0
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("ScriptArgs prefix/suffix requirements must reject non-Hash expected operands");

    assert!(
        err.message.contains("require_cell_lock_args_prefix_hash expects (source_view: u64, expected_args_hash: Hash)"),
        "unexpected error: {}",
        err.message
    );
}

#[test]
fn v0_18_script_construction_accepts_literal_code_hashes() {
    let result = compile(
        SCRIPT_LITERAL_CONSTRUCTION_PROGRAM,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("literal Hash-backed Script construction should compile");

    let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
    assert!(
        assembly.contains(".byte 1\n    .byte 2\n    .byte 3\n    .byte 4"),
        "literal code_hash should be emitted as fixed bytes:\n{assembly}"
    );
}

#[test]
fn v0_18_script_construction_rejects_bad_operands() {
    let bad_hash_type = compile(
        r#"
module v018::bad_script_hash_type

action inspect(code_hash: Hash) -> u64
where
    let expected = script::new(code_hash, 3, script::args_empty())
    return expected.hash_type
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("script::new must reject unsupported literal hash_type values");
    assert!(bad_hash_type.message.contains("script hash_type must be one of"), "unexpected error: {}", bad_hash_type.message);

    let bad_args = compile(
        r#"
module v018::bad_script_args

action inspect(code_hash: Hash) -> u64
where
    let expected = script::new(code_hash, script::hash_type_data1(), script::args(1))
    return expected.hash_type
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("script::args must reject scalar non-byte payloads");
    assert!(bad_args.message.contains("script::args expects fixed bytes"), "unexpected error: {}", bad_args.message);

    let bad_hash_bytes = compile(
        r#"
module v018::bad_hash_bytes

action inspect() -> u64
where
    let code_hash: Hash = Hash::from_bytes(b"short")
    let expected = script::new(code_hash, script::hash_type_data1(), script::args_empty())
    return expected.hash_type
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.18".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect_err("Hash::from_bytes must reject non-32-byte payloads");
    assert!(
        bad_hash_bytes.message.contains("Hash::from_bytes expects exactly 32 bytes"),
        "unexpected error: {}",
        bad_hash_bytes.message
    );
}

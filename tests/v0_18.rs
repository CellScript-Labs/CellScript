use cellscript::{compile, CompileOptions};

const SCRIPT_REF_READ_PROGRAM: &str = r#"
module v018::script_ref_read

action inspect(
    expected_lock_code_hash: Hash,
    expected_type_code_hash: Hash,
    expected_lock_args_hash: Hash,
    expected_type_args_hash: Hash
) -> u64
where
    let input = source::group_input(0)
    let lock_code_hash: Hash = ckb::cell_lock_code_hash(input)
    let type_code_hash: Hash = ckb::cell_type_code_hash(input)
    let lock_hash_type = ckb::cell_lock_hash_type(input)
    let type_hash_type = ckb::cell_type_hash_type(input)
    let lock_args_empty = ckb::cell_lock_args_empty(input)
    let type_args_empty = ckb::cell_type_args_empty(input)
    let lock_args_hash: Hash = ckb::cell_lock_args_hash(input)
    let type_args_hash: Hash = ckb::cell_type_args_hash(input)
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

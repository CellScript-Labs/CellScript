use cellscript::{compile, CompileOptions};

#[test]
fn fixed_u64_le_lowers_fixed_byte_constants_and_parameters() {
    let result = compile(
        r#"
module cellscript::fixed_u64_le_words

struct SignaturePayload {
    pubkey: [u8; 32],
    signature: [u8; 64],
}

action from_const() -> u64
where
    return fixed_u64_le(b"ABCDEFGH", 0)

action from_param(witness payload: [u8; 16]) -> u64
where
    return fixed_u64_le(payload, 1)

action from_schema_field(witness sig: SignaturePayload) -> u64
where
    return fixed_u64_le(sig.pubkey, 0)
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("fixed_u64_le should compile for fixed byte constants and parameters");
    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");

    assert!(
        assembly.contains("# cellscript abi: fixed_u64_le word=0 offset=0 width=8"),
        "constant fixed_u64_le lowering should classify the 8-byte input exactly:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: fixed_u64_le word=1 offset=8 width=16"),
        "parameter fixed_u64_le lowering should classify the selected 8-byte window:\n{assembly}"
    );
    assert!(
        assembly.contains("lbu t1, 8(t2)") && assembly.contains("lbu t1, 15(t2)"),
        "parameter fixed_u64_le lowering should load the selected little-endian byte window:\n{assembly}"
    );
    let field_word_block = assembly
        .split("# cellscript abi: fixed_u64_le word=0 offset=0 width=32")
        .nth(1)
        .and_then(|after| after.split("sd a0").next())
        .expect("schema-field fixed_u64_le block should be present");
    let after_accumulator_init = field_word_block.split("li a0, 0").nth(1).expect("fixed_u64_le accumulator init should be present");
    assert!(
        !after_accumulator_init.contains("call __cellscript_require_"),
        "schema-field fixed_u64_le must not clobber its accumulator with per-byte bounds helper calls:\n{field_word_block}"
    );
    assert!(
        after_accumulator_init.contains("lbu t1, 0(t2)") && after_accumulator_init.contains("lbu t1, 7(t2)"),
        "schema-field fixed_u64_le should load all bytes from one checked base pointer:\n{field_word_block}"
    );
}

#[test]
fn fixed_u64_le_rejects_dynamic_index_oob_window_and_non_fixed_input() {
    let dynamic_index = compile(
        r#"
module cellscript::fixed_u64_le_dynamic_index

action bad(witness payload: [u8; 16], idx: u64) -> u64
where
    return fixed_u64_le(payload, idx)
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(
        dynamic_index.message.contains("fixed_u64_le word_index must be a static u64 literal or const"),
        "unexpected error: {}",
        dynamic_index.message
    );

    let out_of_bounds = compile(
        r#"
module cellscript::fixed_u64_le_out_of_bounds

action bad() -> u64
where
    return fixed_u64_le(b"SHORT", 0)
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(out_of_bounds.message.contains("fixed_u64_le requires 8 bytes"), "unexpected error: {}", out_of_bounds.message);

    let non_fixed_input = compile(
        r#"
module cellscript::fixed_u64_le_non_fixed

action bad(value: u64) -> u64
where
    return fixed_u64_le(value, 0)
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(
        non_fixed_input.message.contains("fixed_u64_le expects (bytes: Hash | Address | [u8; N], word_index: u64)"),
        "unexpected error: {}",
        non_fixed_input.message
    );
}

#[test]
fn generic_verifier_envelope_compiles_all_words_after_spawn_with_fd() {
    let result = compile(
        r#"
module cellscript::generic_verifier_envelope

action verify(message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> u64
where
    let (read_fd, write_fd) = pipe()
    let pid = spawn_with_fd("bounded_verifier_riscv", read_fd)
    pipe_write(write_fd, 0x435049305642534e)
    pipe_write(write_fd, 65536)
    pipe_write(write_fd, fixed_u64_le(message, 0))
    pipe_write(write_fd, fixed_u64_le(message, 1))
    pipe_write(write_fd, fixed_u64_le(message, 2))
    pipe_write(write_fd, fixed_u64_le(message, 3))
    pipe_write(write_fd, fixed_u64_le(pubkey, 0))
    pipe_write(write_fd, fixed_u64_le(pubkey, 1))
    pipe_write(write_fd, fixed_u64_le(pubkey, 2))
    pipe_write(write_fd, fixed_u64_le(pubkey, 3))
    pipe_write(write_fd, fixed_u64_le(signature, 0))
    pipe_write(write_fd, fixed_u64_le(signature, 1))
    pipe_write(write_fd, fixed_u64_le(signature, 2))
    pipe_write(write_fd, fixed_u64_le(signature, 3))
    pipe_write(write_fd, fixed_u64_le(signature, 4))
    pipe_write(write_fd, fixed_u64_le(signature, 5))
    pipe_write(write_fd, fixed_u64_le(signature, 6))
    pipe_write(write_fd, fixed_u64_le(signature, 7))
    close(write_fd)
    let status = wait(pid)
    return status
"#,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("generic verifier envelope should compile");
    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");

    assert_eq!(
        assembly.matches("\n    call __ckb_pipe_write").count(),
        18,
        "envelope should emit exactly the two header writes plus 16 fixed-byte word writes:\n{assembly}"
    );
    assert!(
        assembly.contains("# cellscript abi: fixed_u64_le word=7 offset=56 width=64"),
        "signature word extraction should remain a generic fixed-byte classification:\n{assembly}"
    );
    assert!(
        assembly.contains("call __ckb_spawn_with_fd1"),
        "envelope should delegate through the fd-inheriting spawn helper:\n{assembly}"
    );
}

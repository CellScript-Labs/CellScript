//! Execute the public generator bytes in real CKB syscall contexts. This is
//! focused VM evidence, not node acceptance or a production gate receipt.
use super::*;
use ckb_testtool::{
    builtin::ALWAYS_SUCCESS,
    ckb_types::{
        bytes::Bytes,
        core::{EpochNumberWithFraction, HeaderBuilder, ScriptHashType, TransactionBuilder},
        packed,
        prelude::*,
    },
    context::Context,
};

fn execute(body: &str, library: &str, headers: bool, input_type: bool) -> Result<u64, String> {
    let assembly = format!(
        ".section .text\n.global entry\nentry:\naddi sp, sp, -256\nsd ra, 248(sp)\nli s0, 1234567\n{body}\nli a0, 0\nld ra, 248(sp)\naddi sp, sp, 256\nret\n.Ltest_fail:\nli a0, 101\nli a7, 93\necall\nj .Ltest_fail\n{library}"
    );
    let elf = crate::codegen::assemble_public_test_assembly(&assembly).unwrap();
    let mut context = Context::default();
    let code = context.deploy_cell(Bytes::from(elf));
    let script = context.build_script_with_hash_type(&code, ScriptHashType::Data2, Bytes::new()).unwrap();
    let lock_code = context.deploy_cell(ALWAYS_SUCCESS.clone());
    let lock = context.build_script(&lock_code, Bytes::new()).unwrap();
    let capacity: packed::Uint64 = 100_000_000_000u64.pack();
    let since: packed::Uint64 = 42u64.pack();
    let output = packed::CellOutput::new_builder().capacity(capacity).lock(lock).type_(Some(script).pack()).build();
    let input_output =
        if input_type { output.clone() } else { output.clone().as_builder().type_(None::<packed::Script>.pack()).build() };
    let input = context.create_cell(input_output, Bytes::from_static(&[1, 2, 3, 4, 5, 6]));
    let mut builder = TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input).since(since).build())
        .output(output)
        .output_data(Bytes::from_static(&[1, 2, 3, 4, 5, 6]).pack())
        .witness(Bytes::from_static(&[9, 8, 7]).pack());
    if headers {
        let number: packed::Uint64 = 100u64.pack();
        let epoch: packed::Uint64 = EpochNumberWithFraction::new(42, 2, 7).full_value().pack();
        let header = HeaderBuilder::default().number(number).epoch(epoch).build();
        let hash = header.hash();
        context.insert_header(header);
        builder = builder.header_dep(hash);
    }
    let transaction = context.complete_tx(builder.build());
    context.verify_tx(&transaction, 10_000_000).map_err(|error| format!("{error:?}"))
}

fn check(body: &str, library: &str) {
    let result = execute(body, library, true, true);
    assert!(result.is_ok(), "{body}\n{result:?}");
    assert!(result.unwrap() > 0);
}

fn rejection(body: &str, library: &str, code: i8, headers: bool, input_type: bool) {
    let error = execute(body, library, headers, input_type).expect_err("the helper must terminate rather than return a value");
    let observed = ["error code ", "error code: "].iter().find_map(|marker| {
        let tail = error.split_once(marker)?.1;
        tail.chars().take_while(|ch| ch.is_ascii_digit() || *ch == '-').collect::<String>().parse::<i8>().ok()
    });
    assert!(error.contains("ValidationFailure"), "expected a policy rejection, got {error}");
    assert_eq!(observed, Some(code), "expected rejection {code}, got {error}");
}

#[test]
fn public_stdlib_unsigned_math_executes_zero_full_width_and_square_boundaries() {
    let library = StdLib::generate_assembly();
    const TOP_SQUARE: u64 = (u32::MAX as u64) * (u32::MAX as u64);
    let values = [
        0u64,
        1,
        2,
        3,
        4,
        8,
        9,
        15,
        16,
        17,
        (1 << 32) - 1,
        1 << 32,
        (1 << 63) - 1,
        1 << 63,
        1 << 62,
        TOP_SQUARE - 1,
        TOP_SQUARE,
        TOP_SQUARE + 1,
        u64::MAX - 1,
        u64::MAX,
    ];
    for &a in &values {
        for &b in &[0, 1, 1 << 63, u64::MAX] {
            for (name, expected) in [("min", a.min(b)), ("max", a.max(b)), ("abs_diff", a.abs_diff(b))] {
                check(&format!("li a0, {a}\nli a1, {b}\ncall __math_{name}\nli t0, {expected}\nbne a0, t0, .Ltest_fail\nli t0, 1234567\nbne s0, t0, .Ltest_fail"), &library);
            }
        }
        check(
            &format!(
                "li a0, {a}\ncall __math_isqrt\nli t0, {}\nbne a0, t0, .Ltest_fail\nli t0, 1234567\nbne s0, t0, .Ltest_fail",
                a.isqrt()
            ),
            &library,
        );
    }
}

#[test]
fn public_stdlib_raw_descriptors_and_bodies_share_the_register_contract() {
    let assembly = StdLib::generate_assembly();
    for call in RAW_SYSCALLS {
        let descriptor = StdLib::get_function(call.name).unwrap();
        assert_eq!(descriptor.return_type, call.result.ir_type());
        assert_eq!(descriptor.params, call.parameters.iter().map(|name| ((*name).to_string(), IrType::U64)).collect::<Vec<_>>());
        let body = assembly.split(&format!("__{}:\n", call.name)).nth(1).unwrap().split("\n\n").next().unwrap();
        let instructions = body.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')).collect::<Vec<_>>();
        assert_eq!(instructions, [format!("li a7, {}", call.number), "ecall".into(), "ret".into()]);
    }
}

#[test]
fn public_stdlib_raw_loads_execute_with_real_ckb_buffers_and_lengths() {
    let library = StdLib::generate_assembly();
    // Raw calls preserve the official register placement, including six-word
    // LOAD_CELL_BY_FIELD. All use actual transaction syscalls, not mock success.
    let cell_length = packed::CellOutput::new_builder().type_(Some(packed::Script::default()).pack()).build().as_bytes().len();
    for (name, args, length) in [
        ("load_tx_hash", "li a2, 0", 32),
        ("load_script_hash", "li a2, 0", 32),
        ("load_script", "li a2, 0", packed::Script::default().as_bytes().len()),
        ("load_header", "li a2, 0\nli a3, 0\nli a4, 4", packed::Header::TOTAL_SIZE),
        ("load_input", "li a2, 0\nli a3, 0\nli a4, 1", packed::CellInput::TOTAL_SIZE),
        ("load_cell", "li a2, 0\nli a3, 0\nli a4, 1", cell_length),
        ("load_witness", "li a2, 0\nli a3, 0\nli a4, 1", 3),
        ("load_cell_data", "li a2, 0\nli a3, 0\nli a4, 1", 6),
        ("load_cell_by_field", "li a2, 0\nli a3, 0\nli a4, 1\nli a5, 0", 8),
    ] {
        check(&format!("li t0, 208\nsd t0, 8(sp)\naddi a0, sp, 16\naddi a1, sp, 8\n{args}\ncall __syscall_{name}\nbnez a0, .Ltest_fail\nld t0, 8(sp)\nli t1, {length}\nbne t0, t1, .Ltest_fail"), &library);
    }
    // Read two bytes at offset one: returned length is the five-byte remainder,
    // while adjacent guards demonstrate that only the requested bytes are copied.
    check("li t0, 170\nsb t0, 16(sp)\nsb t0, 19(sp)\nli t0, 2\nsd t0, 8(sp)\naddi a0, sp, 17\naddi a1, sp, 8\nli a2, 1\nli a3, 0\nli a4, 1\ncall __syscall_load_cell_data\nbnez a0, .Ltest_fail\nld t0, 8(sp)\nli t1, 5\nbne t0, t1, .Ltest_fail\nlbu t0, 17(sp)\nli t1, 2\nbne t0, t1, .Ltest_fail\nlbu t0, 18(sp)\nli t1, 3\nbne t0, t1, .Ltest_fail\nlbu t0, 16(sp)\nli t1, 170\nbne t0, t1, .Ltest_fail\nlbu t0, 19(sp)\nbne t0, t1, .Ltest_fail", &library);
    // The raw interface returns INDEX_OUT_OF_BOUND rather than interpreting it
    // as loaded data or suppressing it. Its caller decides how to handle status.
    check("li t0, 8\nsd t0, 8(sp)\naddi a0, sp, 16\naddi a1, sp, 8\nli a2, 0\nli a3, 99\nli a4, 1\ncall __syscall_load_input\nli t0, 1\nbne a0, t0, .Ltest_fail", &library);
    check("call __syscall_current_cycles\nbeqz a0, .Ltest_fail", &library);
    check(
        "li t0, 79\nsb t0, 16(sp)\nli t0, 75\nsb t0, 17(sp)\nsb zero, 18(sp)\naddi a0, sp, 16\ncall __syscall_debug_print",
        &library,
    );
}

#[test]
fn public_stdlib_checked_context_reads_require_success_and_exact_size() {
    let library = StdLib::generate_assembly();
    for (name, expected) in [
        ("env_current_timepoint", 42),
        ("ckb_header_epoch_number", 42),
        ("ckb_header_epoch_start_block_number", 98),
        ("ckb_header_epoch_length", 7),
        ("ckb_input_since", 42),
    ] {
        check(&format!("call __{name}\nli t0, {expected}\nbne a0, t0, .Ltest_fail"), &library);
    }
    rejection("call __env_current_timepoint", &library, CellScriptRuntimeError::SyscallFailed.code() as i8, false, true);
    rejection("call __ckb_input_since", &library, CellScriptRuntimeError::SyscallFailed.code() as i8, true, false);
    // Exercise the shared scalar loader's exact-size rejection with a real
    // 36-byte OutPoint field; this test-only instance is not a new public ABI.
    let mut wrong_extent = String::new();
    StdLib::push_checked_u64_load(
        &mut wrong_extent,
        "__test_wrong_extent",
        "test",
        (ckb_abi::syscall::LOAD_INPUT_BY_FIELD, "LOAD_INPUT_BY_FIELD"),
        ckb_abi::source::INPUT,
        ckb_abi::input_field::OUT_POINT,
        true,
    );
    rejection("call __test_wrong_extent", &wrong_extent, CellScriptRuntimeError::ExactSizeMismatch.code() as i8, true, true);
    rejection("call __env_remaining_cycles\nj .Ltest_fail", &library, CellScriptRuntimeError::SyscallFailed.code() as i8, true, true);
}

#[test]
fn retired_public_collection_symbols_all_reject_before_returning() {
    let library = collections::Collections::generate_assembly();
    assert!(!library.contains("2101"), "there is no CKB allocation syscall in this retired interface");
    assert!(!library.contains("    ret"), "a retired helper must not return a fabricated value");
    for function in collections::Collections::functions() {
        rejection(
            &format!("call __{}\nj .Ltest_fail", function.name),
            &library,
            CellScriptRuntimeError::CollectionRuntimeUnsupported.code() as i8,
            true,
            true,
        );
    }
}

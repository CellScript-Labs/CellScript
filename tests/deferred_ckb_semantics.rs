//! Deferred CKB semantic helpers must never emit success-shaped stubs, and
//! the IPC read/inherit syscalls must prepare their full VM2 ABI.

use cellscript::{compile, compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};

fn asm_of(source: &str) -> String {
    let result = compile(source, CompileOptions { target: Some("riscv64-asm".into()), ..Default::default() }).unwrap();
    String::from_utf8(result.artifact_bytes.clone()).unwrap()
}

fn body_of<'a>(assembly: &'a str, symbol: &str) -> &'a str {
    let start = assembly.find(&format!("{symbol}:")).unwrap_or_else(|| panic!("{symbol} missing"));
    let rest = &assembly[start..];
    let end = rest.find("\n.global ").unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn since_constraint_helpers_abort_instead_of_accepting() {
    // H1 regression: these four helpers previously emitted `li a0, 0; ret`,
    // so a source-level timelock accepted immature spends unconditionally.
    let source = "module since\naction mint() -> u64 { verification\nrequire_maturity(100)\nrequire_time(1714000000)\nrequire_epoch_after(10, 0, 1)\nrequire_epoch_relative(10, 0, 1)\nreturn 0 }";
    let assembly = asm_of(source);
    for symbol in ["__ckb_require_maturity", "__ckb_require_time", "__ckb_require_epoch_after", "__ckb_require_epoch_relative"] {
        let body = body_of(&assembly, symbol);
        assert!(body.contains("li a0, 86"), "{symbol} must abort with runtime error 86:\n{body}");
        assert!(!body.contains("li a0, 0\n    ret"), "{symbol} must not keep the success stub:\n{body}");
    }
}

#[test]
fn occupied_capacity_floor_aborts_instead_of_returning_zero() {
    let source =
        "module cap\naction verify() -> u64 { verification\nlet floor = occupied_capacity(\"Token\")\nrequire floor >= 0\nreturn 0 }";
    let assembly = asm_of(source);
    let body = body_of(&assembly, "__ckb_occupied_capacity");
    assert!(body.contains("li a0, 87"), "occupied capacity floor must abort with runtime error 87:\n{body}");
}

#[test]
fn deferred_since_constraints_are_declared_and_rejected_under_production() {
    let source = "module since\naction mint() -> u64 { verification\nrequire_maturity(100)\nreturn 0 }";
    let metadata = cellscript::compile_metadata(source, cellscript::CellScriptEdition::Edition2026, None).unwrap();
    let features = metadata.actions.iter().flat_map(|action| action.fail_closed_runtime_features.iter()).collect::<Vec<_>>();
    assert!(features.iter().any(|feature| feature.as_str() == "ckb-since-constraints-deferred"), "features: {features:?}");

    let rejected = compile_with_executable_surface_policy(
        source,
        CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap_err();
    assert!(
        rejected.message.contains("ckb-since-constraints-deferred"),
        "production build must reject deferred since constraints: {}",
        rejected.message
    );
}

#[test]
fn pipe_read_prepares_the_full_vm2_read_abi() {
    // IPC ABI regression: a bare ecall passed unprepared a1/a2 and returned
    // the status word as the payload. The helper must stage an 8-byte buffer,
    // pass the length slot pointer in a2, check SUCCESS, require the full
    // 8-byte delivery, and extract the payload from the buffer.
    let source = "module ipc\naction verify() -> u64 { verification\nlet fd = pipe()\nlet mut echoed: u64 = 0\nechoed = pipe_read(fd.0)\nrequire echoed != 999\nclose(fd.0)\nclose(fd.1)\nreturn 0 }";
    let assembly = asm_of(source);
    let body = body_of(&assembly, "__ckb_pipe_read");
    assert!(body.contains("li a7, 2606"), "read syscall number:\n{body}");
    assert!(body.contains("li t0, 8"), "8-byte length staging:\n{body}");
    assert!(body.contains("addi a2, sp, 8"), "a2 must point at the length slot:\n{body}");
    assert!(body.contains("addi a1, sp, 16"), "a1 must point at the payload buffer:\n{body}");
    assert!(body.contains("bne t0, t1"), "delivered length must be checked:\n{body}");
    assert!(body.contains("ld a0, 16(sp)"), "payload must be extracted from the buffer:\n{body}");
}

#[test]
fn inherited_fd_prepares_buffer_and_selects_the_requested_index() {
    let source = "module ipc\naction verify() -> u64 { verification\nlet mut resolved: u64 = 0\nresolved = inherited_fd(0)\nrequire resolved != 999\nreturn 0 }";
    let assembly = asm_of(source);
    let body = body_of(&assembly, "__ckb_inherited_fd");
    assert!(body.contains("li a7, 2607"), "inherited fd syscall number:\n{body}");
    assert!(body.contains("addi a0, sp, 16"), "a0 must be the fd-list buffer:\n{body}");
    assert!(body.contains("addi a1, sp, 8"), "a1 must be the count slot:\n{body}");
    assert!(body.contains("bgeu t1, t0"), "index must be bounded by the returned count:\n{body}");
    assert!(body.contains("ld a0, 0(t2)"), "resolved fd must be loaded from the list:\n{body}");
}

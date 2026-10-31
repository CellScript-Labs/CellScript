//! Deferred CKB semantic helpers must never emit success-shaped stubs, and
//! the IPC read/inherit syscalls must prepare their full VM2 ABI.

use cellscript::{compile, compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

#[test]
fn bare_spawn_rejects_before_metadata_or_artifacts_in_every_policy() {
    use cellscript::{error::SourceDiagnosticCode, CellScriptEdition, CompilerDiagnostics};
    let fixtures = [
        "module spawn_failure\naction verify() -> u64 { verification spawn(\"hash_checker\") return 0 }",
        "module spawn_failure\naction verify() -> u64 { verification let child = spawn(\"hash_checker\") return child }",
        "module spawn_failure\nfn child() -> u64 { return spawn(\"hash_checker\") }\naction verify() -> u64 { verification return child() }",
        "module spawn_failure\nfn unused() -> u64 { return spawn(\"hash_checker\") }\naction verify() -> u64 { verification return 0 }",
        "module spawn_failure\naction verify() -> u64 { verification if false { spawn(\"hash_checker\") } return 0 }",
    ];
    for source in fixtures {
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let metadata = cellscript::compile_metadata_with_diagnostics(source, edition, None);
            assert!(metadata.metadata.is_none());
            assert!(metadata
                .diagnostics
                .iter()
                .any(|error| error.code.as_deref() == Some(SourceDiagnosticCode::BareSpawnUnsupported.code())));
            let ast = cellscript::frontend::parse(source, edition).unwrap();
            // The public IR API must not resurrect the removed helper when called without type checking.
            let error = cellscript::ir::generate(&ast).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E1021"));
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                for target in ["riscv64-asm", "riscv64-elf"] {
                    for policy in [ExecutableSurfacePolicy::AllowFailClosed, ExecutableSurfacePolicy::DenyFailClosed] {
                        let options = CompileOptions { edition, target: Some(target.into()), diagnostics, ..Default::default() };
                        let error = compile_with_executable_surface_policy(source, options, policy).unwrap_err();
                        assert_eq!(error.code.as_deref(), Some("E1021"), "{error}");
                        assert!(error.message.contains("manifest-bound CellDep spawn"), "{error}");
                        assert!(source[error.span.start..error.span.end].starts_with("spawn("), "{error}");
                    }
                }
            }
        }
    }
}

#[test]
fn resolved_user_spawn_remains_an_executable_source_call() {
    use cellscript::CompilerDiagnostics;
    let source =
        "module source_spawn\nfn spawn(value: u64) -> u64 { return value }\naction verify() -> u64 { verification return spawn(7) }";
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = compile_with_executable_surface_policy(
            source,
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
        assert!(result.metadata.actions[0].effects.is_empty());
        let fixture = ckb_script_runner::build_simple_fixture(ckb_testtool::bytes::Bytes::new(), 1, 1);
        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 7);
    }
}

#[test]
fn unregistered_runtime_helpers_reject_before_any_artifact_format_is_returned() {
    let source =
        cellscript::frontend::parse("module closure\naction main() { verification require true }", cellscript::CURRENT_EDITION)
            .unwrap();
    for helper in ["__ckb_unregistered_helper", "__ckb_spawn"] {
        let mut ir = cellscript::ir::generate(&source).unwrap();
        let cellscript::ir::IrItem::Action(action) = &mut ir.items[0] else { panic!("action") };
        action.body.blocks[0]
            .instructions
            .insert(0, cellscript::ir::IrInstruction::Call { dest: None, func: helper.into(), args: vec![] });
        for format in [cellscript::ArtifactFormat::RiscvAssembly, cellscript::ArtifactFormat::RiscvElf] {
            let error = cellscript::codegen::CodeGenerator::new(Default::default()).generate(&ir, format).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E2200"));
        }
    }
}

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

#[test]
fn retired_bare_spawn_metadata_is_not_an_admitted_fail_closed_feature() {
    let mut metadata = cellscript::compile_metadata(
        "module current\naction verify() { verification require true }",
        cellscript::CURRENT_EDITION,
        None,
    )
    .unwrap();
    metadata.runtime.fail_closed_runtime_features.push("ckb-bare-spawn-deferred".into());
    for policy in [ExecutableSurfacePolicy::AllowFailClosed, ExecutableSurfacePolicy::DenyFailClosed] {
        assert_eq!(cellscript::validate_executable_surface(&metadata, policy).unwrap_err().code.as_deref(), Some("E2105"));
    }
    assert!(cellscript::runtime_errors::CellScriptRuntimeError::from_code(88).is_none());
}

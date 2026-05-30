use camino::Utf8Path;
use cellscript::{compile, CompileOptions};
use tempfile::tempdir;

const SPAWN_WITH_FD_SOURCE: &str = r#"
module cellscript::v0_16_spawn_with_fd

action delegate(value: u64) -> u64
where
    let (read_fd, write_fd) = pipe()
    let pid = spawn_with_fd("novaseal_btc_verifier_riscv", read_fd)
    pipe_write(write_fd, value)
    close(write_fd)
    let status = wait(pid)
    return status
"#;

#[test]
fn v0_16_spawn_with_fd_exposes_spawn_target_metadata() {
    let result =
        compile(SPAWN_WITH_FD_SOURCE, CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() })
            .expect("spawn_with_fd should compile");

    let delegate = result.metadata.actions.iter().find(|action| action.name == "delegate").expect("delegate metadata");
    assert!(delegate.ckb_runtime_accesses.iter().any(|access| {
        access.operation == "spawn-with-fd"
            && access.syscall == "SPAWN"
            && access.source == "CellDep"
            && access.binding.starts_with("spawn-target-tag:0x")
    }));
    assert!(delegate.verifier_obligations.iter().any(|obligation| {
        obligation.category == "spawn-target"
            && obligation.feature.starts_with("spawn-target:CellDep#0@0x")
            && obligation.status == "runtime-required"
    }));
}

#[test]
fn strict_0_16_accepts_manifest_bound_spawn_with_fd_target_cell_dep() {
    let dir = tempdir().unwrap();
    let root = Utf8Path::from_path(dir.path()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
name = "novaseal_spawn_with_fd_bound"
version = "0.1.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "novaseal_btc_verifier_riscv"
out_point = "0x4444444444444444444444444444444444444444444444444444444444444444:0"
dep_type = "code"
hash_type = "data1"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), SPAWN_WITH_FD_SOURCE).unwrap();

    let result = cellscript::compile_path(
        root,
        CompileOptions {
            target_profile: Some("ckb".to_string()),
            primitive_compat: Some("0.16".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("manifest-bound spawn_with_fd should satisfy strict 0.16");

    let spawn_plan =
        result.metadata.runtime.proof_plan.iter().find(|plan| plan.category == "spawn-target").expect("spawn target ProofPlan record");
    assert_eq!(spawn_plan.status, "builder-required");
    assert_eq!(spawn_plan.codegen_coverage_status, "builder-required");
    assert!(spawn_plan.detail.contains("novaseal_btc_verifier_riscv"), "{spawn_plan:#?}");
    assert!(result.metadata.runtime.builder_assumptions.iter().any(|assumption| {
        assumption.kind == "spawn_target_cell_dep_binding"
            && assumption.proof_plan_status == "builder-required"
            && assumption.required_cell_deps.iter().any(|dep| dep.contains("CellDep#0:name=novaseal_btc_verifier_riscv:dep_type=code"))
    }));
}

#[test]
fn v0_16_spawn_with_fd_emits_vm2_spawnargs_with_single_inherited_fd() {
    let result = compile(
        SPAWN_WITH_FD_SOURCE,
        CompileOptions {
            target: Some("riscv64-asm".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        },
    )
    .expect("spawn_with_fd should compile to assembly");
    let assembly = String::from_utf8(result.artifact_bytes).expect("assembly should be utf-8");

    assert!(assembly.contains("call __ckb_spawn_with_fd1"), "delegate should call spawn_with_fd helper:\n{assembly}");
    assert!(
        assembly.contains("# cellscript abi: __ckb_spawn_with_fd1 returns status in a1; fail closed on nonzero"),
        "spawn_with_fd call site should be status checked:\n{assembly}"
    );

    let helper =
        assembly.split_once("__ckb_spawn_with_fd1:").map(|(_, after)| after).expect("spawn_with_fd helper body should be present");
    let helper = helper.split_once(".global ").map(|(body, _)| body).unwrap_or(helper);
    assert!(
        helper.contains("spawn_with_fd resolves the static target to CellDep#0 with no argv and one inherited fd from a1")
            && helper.contains("sd a1, 8(sp)")
            && helper.contains("sd zero, 16(sp)")
            && helper.contains("li a7, 2601")
            && helper.contains("ecall"),
        "spawn_with_fd helper should build a one-fd, zero-terminated SpawnArgs inherited_fds list:\n{helper}"
    );
}

#[test]
fn v0_16_spawn_with_fd_requires_static_target_and_open_fd() {
    let dynamic_target = compile(
        r#"
module cellscript::dynamic_spawn_with_fd

action delegate(target: String, fd: u64) -> u64
where
    return spawn_with_fd(target, fd)
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(
        dynamic_target.message.contains("spawn target must be a static script reference"),
        "unexpected error: {}",
        dynamic_target.message
    );

    let closed_fd = compile(
        r#"
module cellscript::closed_spawn_with_fd

action delegate() -> u64
where
    let (read_fd, write_fd) = pipe()
    close(read_fd)
    let pid = spawn_with_fd("novaseal_btc_verifier", read_fd)
    close(write_fd)
    return pid
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(
        closed_fd.message.contains("spawn_with_fd uses a Spawn/IPC file descriptor after close"),
        "unexpected error: {}",
        closed_fd.message
    );

    let transferred_fd = compile(
        r#"
module cellscript::transferred_spawn_with_fd

action delegate() -> u64
where
    let (read_fd, write_fd) = pipe()
    let pid = spawn_with_fd("novaseal_btc_verifier", read_fd)
    close(read_fd)
    close(write_fd)
    return pid
"#,
        CompileOptions { target_profile: Some("ckb".to_string()), ..CompileOptions::default() },
    )
    .unwrap_err();
    assert!(transferred_fd.message.contains("transferred to a spawned process"), "unexpected error: {}", transferred_fd.message);
}

// CLI surface behavior; included in the original cli integration target.

#[test]
fn cellc_top_level_help_shows_commands_and_direct_source_mode() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--help").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cellc <COMMAND> [OPTIONS]"), "unexpected help: {stdout}");
    assert!(stdout.contains("Direct source mode:"), "unexpected help: {stdout}");
    assert!(stdout.contains("build"), "unexpected help: {stdout}");
    assert!(stdout.contains("verify-artifact"), "unexpected help: {stdout}");
    assert!(stdout.contains("tx"), "unexpected help: {stdout}");
    assert!(stdout.contains("protocol"), "unexpected help: {stdout}");
    assert!(stdout.contains("deploy"), "unexpected help: {stdout}");
    assert!(!stdout.contains("validate-tx"), "legacy tx alias should be hidden from top-level help: {stdout}");
    assert!(!stdout.contains("deploy-plan"), "legacy deploy alias should be hidden from top-level help: {stdout}");
    assert!(stdout.contains("--explain <CODE>"), "unexpected help: {stdout}");
    assert!(stdout.contains("--json"), "unexpected help: {stdout}");
    assert!(!stdout.contains("--message-format"), "deprecated flag should be hidden: {stdout}");
    assert!(stdout.contains("--color <WHEN>"), "unexpected help: {stdout}");
    assert!(stdout.contains("Run `cellc <command> --help`"), "unexpected help: {stdout}");
}

#[test]
fn cellc_short_and_long_version_flags_share_one_output() {
    let short = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("-V").output().unwrap();
    let long = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--version").output().unwrap();
    assert!(short.status.success());
    assert!(long.status.success());
    assert_eq!(short.stdout, long.stdout);
    assert_eq!(String::from_utf8_lossy(&long.stdout).lines().count(), 1);
}

#[test]
fn cellc_list_reports_package_commands_without_direct_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--list").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installed cellc commands"), "unexpected list: {stdout}");
    assert!(stdout.contains("build"), "unexpected list: {stdout}");
    assert!(stdout.contains("tx"), "unexpected list: {stdout}");
    assert!(stdout.contains("protocol"), "unexpected list: {stdout}");
    assert!(stdout.contains("deploy"), "unexpected list: {stdout}");
    assert!(stdout.contains("registry"), "unexpected list: {stdout}");
    assert!(stdout.contains("receipt"), "unexpected list: {stdout}");
    assert!(stdout.contains("sign-receipt"), "unexpected list: {stdout}");
    assert!(stdout.contains("verify-receipt"), "unexpected list: {stdout}");
    assert!(stdout.contains("certify"), "unexpected list: {stdout}");
    assert!(!stdout.contains("validate-tx"), "legacy tx alias should be hidden from command list: {stdout}");
    assert!(!stdout.contains("deploy-plan"), "legacy deploy alias should be hidden from command list: {stdout}");
    assert!(!stdout.contains("registry-verify"), "legacy registry alias should be hidden from command list: {stdout}");
    assert!(!stdout.contains("package-verify"), "legacy package alias should be hidden from command list: {stdout}");
    assert!(!stdout.contains("--target-profile"), "unexpected direct flag in command list: {stdout}");
}

#[test]
fn cellc_help_subcommand_delegates_to_package_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["help", "build"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Compile the current package"), "unexpected help: {stdout}");
    assert!(stdout.contains("Usage: cellc build [OPTIONS]"), "unexpected help: {stdout}");
}

#[test]
fn cellc_help_reports_canonical_nested_021_groups() {
    let explain = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["help", "explain"]).output().unwrap();
    assert!(explain.status.success(), "{}", String::from_utf8_lossy(&explain.stderr));
    let explain_stdout = String::from_utf8_lossy(&explain.stdout);
    assert!(explain_stdout.contains("profile"), "unexpected explain help: {explain_stdout}");
    assert!(explain_stdout.contains("proof"), "unexpected explain help: {explain_stdout}");
    assert!(explain_stdout.contains("assumptions"), "unexpected explain help: {explain_stdout}");
    assert!(explain_stdout.contains("generics"), "unexpected explain help: {explain_stdout}");
    assert!(explain_stdout.contains("graph"), "unexpected explain help: {explain_stdout}");

    let tx = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["help", "tx"]).output().unwrap();
    assert!(tx.status.success(), "{}", String::from_utf8_lossy(&tx.stderr));
    let tx_stdout = String::from_utf8_lossy(&tx.stdout);
    assert!(tx_stdout.contains("Validate, solve, and trace transaction evidence"), "unexpected tx help: {tx_stdout}");
    assert!(tx_stdout.contains("validate"), "unexpected tx help: {tx_stdout}");
    assert!(tx_stdout.contains("solve"), "unexpected tx help: {tx_stdout}");
    assert!(tx_stdout.contains("trace"), "unexpected tx help: {tx_stdout}");

    let deploy = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["help", "deploy"]).output().unwrap();
    assert!(deploy.status.success(), "{}", String::from_utf8_lossy(&deploy.stderr));
    let deploy_stdout = String::from_utf8_lossy(&deploy.stdout);
    assert!(deploy_stdout.contains("Plan, verify, diff, and lock deployment evidence"), "unexpected deploy help: {deploy_stdout}");
    assert!(deploy_stdout.contains("plan"), "unexpected deploy help: {deploy_stdout}");
    assert!(deploy_stdout.contains("verify"), "unexpected deploy help: {deploy_stdout}");
    assert!(deploy_stdout.contains("lock-deps"), "unexpected deploy help: {deploy_stdout}");
}

#[test]
fn cellc_command_typo_wins_over_an_ambiguous_extensionless_file() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("buil"), "not CellScript").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(temp.path()).arg("buil").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no such command or input: `buil`"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("similar name exists: `build`"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_structured_failures_expose_usage_and_io_exit_categories() {
    let usage = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["--json", "--not-a-cellc-option"]).output().unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&usage.stderr));
    let usage_json: serde_json::Value = serde_json::from_slice(&usage.stdout).unwrap();
    assert_eq!(usage_json["category"], "usage");
    assert_eq!(usage_json["exit_code"], 2);

    let missing = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["--json", "missing.cell"]).output().unwrap();
    assert_eq!(missing.status.code(), Some(74));
    assert!(missing.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&missing.stderr));
    let missing_json: serde_json::Value = serde_json::from_slice(&missing.stdout).unwrap();
    assert_eq!(missing_json["category"], "io");
    assert_eq!(missing_json["exit_code"], 74);
    assert_eq!(missing_json["diagnostics"][0]["code"], "CLI0005");
    assert_eq!(missing_json["diagnostics"][0]["file"], "missing.cell");
}

#[test]
fn cellc_empty_directory_error_suggests_init_or_source_input() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(temp.path()).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[CLI0006]: Cell.toml not found"), "unexpected stderr: {stderr}");
    assert!(!stderr.contains("line 0"), "unexpected no-span line marker: {stderr}");
    assert!(stderr.contains("cellc init"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("pass a .cell file"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_input_failures_have_typed_codes_and_path_hints() {
    let root = tempfile::tempdir().unwrap();
    for (args, code, category, hint) in [
        (vec!["unsupported.txt"], "CLI0004", "usage", "pass a .cell file, package directory, or Cell.toml"),
        (vec!["Cell.toml not found.txt"], "CLI0004", "usage", "pass a .cell file, package directory, or Cell.toml"),
        (vec!["missing.cell"], "CLI0005", "io", "check the path"),
        (vec!["."], "CLI0006", "io", "cellc init"),
    ] {
        let json = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root.path()).arg("--json").args(&args).output().unwrap();
        assert!(!json.status.success());
        assert!(json.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(value["category"], category);
        assert_eq!(value["diagnostics"][0]["code"], code);
        let human = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root.path()).args(&args).output().unwrap();
        assert!(!human.status.success());
        let stderr = String::from_utf8_lossy(&human.stderr);
        assert!(stderr.contains(hint), "{code}: {stderr}");
        if code == "CLI0004" {
            assert!(!stderr.contains("cellc init"), "input text must not select a manifest hint: {stderr}");
        }
    }
}

#[test]
fn cellc_direct_parse_error_prints_source_context() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("bad.cell");
    std::fs::write(
        &input,
        r#"module demo::bad

resource Token {
    amount:
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("--parse").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: expected type"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("-->"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("bad.cell:4:12") || stderr.contains("bad.cell:4:13"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("4 |     amount:"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("^ expected type"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_check_production_distinguishes_fixed_and_dynamic_helper_outputs() {
    // A dynamic vector returned by a helper is not an admitted output value.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Fingerprint {
    digest: Vec<u64>,
}

fn pass_digest(digest: Vec<u64>) -> Vec<u64> {
    return digest
}

action issue(digest: Vec<u64>) -> Fingerprint {
    verification
        let dynamic_digest = pass_digest(digest)
        let token = create Fingerprint {
            digest: dynamic_digest
        }
        return token
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[E2105]"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("stopped before codegen"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("output-verification-incomplete"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("fail-closed"), "unexpected stderr: {}", stderr);

    // The corresponding fixed-byte helper result is now fully checked.
    let source_path = root.join("src/main.cell");
    let fixed_source = std::fs::read_to_string(&source_path).unwrap().replace("Vec<u64>", "Hash");
    std::fs::write(&source_path, fixed_source).unwrap();
    let accepted =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["check", "--production", "--json"]).output().unwrap();
    assert!(accepted.status.success(), "fixed Hash helper output must pass: {}", String::from_utf8_lossy(&accepted.stderr));
    let report: serde_json::Value = serde_json::from_slice(&accepted.stdout).unwrap();
    let target = &report["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 0);
    assert_eq!(target["checked_transaction_runtime_input_requirements"], 1);
    assert_eq!(target["fail_closed_runtime_features"], serde_json::json!([]));
}

#[test]
fn cellc_explain_subcommand_reports_runtime_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("explain").arg("E0018").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["code"], 18);
    assert_eq!(summary["ecode"], "E0018");
    assert_eq!(summary["name"], "fixed-byte-comparison-unresolved");
    assert!(summary["description"].as_str().unwrap().contains("fixed-byte verifier comparison"));
    assert!(summary["hint"].as_str().unwrap().contains("schema-backed"));
}

#[test]
fn cellc_explain_subcommand_reports_compiler_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "E2202", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["domain"], "compiler");
    assert_eq!(summary["code"], "E2202");
    assert_eq!(summary["name"], "instruction-encoding");
    assert!(summary["description"].as_str().unwrap().contains("RISC-V instruction"));
    assert!(summary["hint"].as_str().unwrap().contains("immediate range"));
}

#[test]
fn cellc_explain_subcommand_reports_cli_input_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "CLI0006", "--json"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["domain"], "cli");
    assert_eq!(summary["code"], "CLI0006");
    assert_eq!(summary["name"], "manifest-not-found");
    assert!(summary["hint"].as_str().unwrap().contains("cellc init"));
}

#[test]
fn cellc_explain_subcommand_reports_public_interface_breaking_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "E2501", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["domain"], "compiler");
    assert_eq!(summary["code"], "E2501");
    assert_eq!(summary["name"], "public-interface-breaking");
    assert!(summary["description"].as_str().unwrap().contains("public interface"));
    assert!(summary["hint"].as_str().unwrap().contains("compatibility dimension"));
}

#[test]
fn cellc_explain_subcommand_reports_package_compiler_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "E2600", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["domain"], "compiler");
    assert_eq!(summary["code"], "E2600");
    assert_eq!(summary["name"], "package-compiler-incompatible");
    assert!(summary["description"].as_str().unwrap().contains("compiler requirement"));
    assert!(summary["hint"].as_str().unwrap().contains("Select a package version"));
}

#[test]
fn cellc_explain_subcommand_reports_package_instance_conflict() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "E2601", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["domain"], "compiler");
    assert_eq!(summary["code"], "E2601");
    assert_eq!(summary["name"], "package-instance-conflict");
    assert!(summary["description"].as_str().unwrap().contains("package coordinate"));
    assert!(summary["hint"].as_str().unwrap().contains("repin the lockfile"));
}

#[test]
fn cellc_check_xudt_group_amount_conserved_lowers_to_runtime_helper() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_xudt_package(root, xudt_conserved_source_with_runtime_helper());

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap()).unwrap();
    let proof_plan = metadata["runtime"]["proof_plan"].as_array().expect("proof_plan array");
    let aggregate =
        proof_plan.iter().find(|plan| plan["category"] == "aggregate-invariant").expect("aggregate-invariant ProofPlan record");

    assert_eq!(aggregate["status"], "checked-runtime", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["evidence_tier"], "checked-runtime", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["codegen_coverage_status"], "covered", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["on_chain_checked"], true);
    assert!(
        aggregate["builder_assumptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|assumption| assumption == "declared(runtime-helper-checked:xudt::require_group_amount_conserved)"),
        "missing checked helper assumption: {:?}",
        aggregate["builder_assumptions"]
    );

    let runtime_accesses = metadata["runtime"]["ckb_runtime_accesses"].as_array().expect("ckb_runtime_accesses array");
    assert!(
        runtime_accesses.iter().any(|access| {
            access["operation"] == "xudt-group-amount-conservation" && access["binding"] == "xudt::require_group_amount_conserved"
        }),
        "missing xudt conserved runtime access: {:?}",
        runtime_accesses
    );
}

#[test]
fn cellc_check_primitive_strict_017_rejects_stale_xudt_helper_gap() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_xudt_package(root, xudt_conserved_source_without_helper_call());

    // The invariant is runtime-helper-required; strict 0.17 mode must fail closed
    // because the matching runtime access is missing from generated code.
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("build")
        .arg("--primitive-strict")
        .arg("0.17")
        .output()
        .unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("0.17 CKB source strict check failed"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("PP0170"), "unexpected stderr: {stderr}");
    assert!(
        stderr.contains("0.17 strict mode requires matching runtime-helper-required:xudt::require_group_amount_conserved coverage"),
        "unexpected stderr: {stderr}"
    );
}

// CLI packages behavior; included in the original cli integration target.

#[test]
fn cellc_top_level_explain_matches_explain_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["--explain", "E0001"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CellScript runtime error E0001"), "unexpected explain output: {stdout}");
    assert!(stdout.contains("syscall-failed"), "unexpected explain output: {stdout}");
}

#[test]
fn cellc_unknown_bare_command_suggests_nearest_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("buil").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no such command or input: `buil`"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("similar name exists: `build`"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("pass a .cell file, package directory, or Cell.toml"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_source_snippet_uses_terminal_width_for_unicode_prefixes() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("unicode.cell");
    let bad_line = "        let 数量: u64 true";
    std::fs::write(
        &input,
        format!("module unicode_error\n\naction bad() -> bool {{\n    verification\n{bad_line}\n        return true\n}}\n"),
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("--parse").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let underline = stderr.lines().find(|line| line.contains("^ expected")).expect("unicode underline");
    let gutter_end = underline.find("| ").expect("snippet gutter") + 2;
    let caret = underline.find('^').expect("caret");
    let expected_offset = UnicodeWidthStr::width(&bad_line[..bad_line.find("true").unwrap()]);
    assert_eq!(caret - gutter_end, expected_offset, "unexpected underline: {underline}");
}

#[test]
fn cellc_parse_reports_multiple_recovered_syntax_errors() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("bad.cell");
    std::fs::write(
        &input,
        r#"module multi_parse_errors

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("--parse").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected '=', found 'true'"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("expected '=', found integer 1"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("5 |         let first: u64 true"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("6 |         let second: bool 1"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("aborting due to 2 diagnostics"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_direct_compile_reports_multiple_recovered_syntax_errors() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("bad.cell");
    std::fs::write(
        &input,
        r#"module multi_parse_errors

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("expected '=', found 'true'"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("expected '=', found integer 1"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("5 |         let first: u64 true"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("6 |         let second: bool 1"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("aborting due to 2 diagnostics"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_direct_json_reports_recovered_syntax_errors_on_stdout() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("bad.cell");
    std::fs::write(
        &input,
        r#"module multi_parse_errors

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--json").arg("--parse").arg(&input).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["diagnostic_count"], 2, "unexpected diagnostics: {stdout}");
    assert_eq!(stdout["error_count"], 2);
    assert_eq!(stdout["warning_count"], 0);
    let diagnostics = stdout["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found 'true'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found integer 1")));
    assert!(diagnostics.iter().all(|diagnostic| diagnostic["range"]["start"]["line"].as_u64().unwrap_or_default() > 0));
    let raw_stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!raw_stdout.contains("-->"), "JSON diagnostics should not include human source snippets: {raw_stdout}");
}

#[test]
fn cellc_direct_json_success_is_a_single_stdout_document() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("ok.cell");
    std::fs::write(&input, "module demo::ok\naction main() -> u64 {\n    verification\n        0\n}\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["--json", "--lex"]).arg(&input).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["mode"], "lex");
    assert!(stdout["token_count"].as_u64().unwrap_or_default() > 0);
    assert!(stdout["tokens"].is_array());
}

#[test]
fn cellc_direct_color_control_overrides_ansi_output() {
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

    let always = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--color=always").arg("--parse").arg(&input).output().unwrap();
    assert!(!always.status.success(), "unexpected success: {}", String::from_utf8_lossy(&always.stdout));
    let always_stderr = String::from_utf8_lossy(&always.stderr);
    assert!(always_stderr.contains("\u{1b}["), "expected ANSI colour when forced: {always_stderr}");

    let never = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--color=never").arg("--parse").arg(&input).output().unwrap();
    assert!(!never.status.success(), "unexpected success: {}", String::from_utf8_lossy(&never.stdout));
    let never_stderr = String::from_utf8_lossy(&never.stderr);
    assert!(!never_stderr.contains("\u{1b}["), "unexpected ANSI colour when disabled: {never_stderr}");

    let no_color = Command::new(env!("CARGO_BIN_EXE_cellc")).env("NO_COLOR", "1").arg("--parse").arg(&input).output().unwrap();
    assert!(!no_color.status.success(), "unexpected success: {}", String::from_utf8_lossy(&no_color.stdout));
    let no_color_stderr = String::from_utf8_lossy(&no_color.stderr);
    assert!(!no_color_stderr.contains("\u{1b}["), "unexpected ANSI colour with NO_COLOR: {no_color_stderr}");
}

#[test]
fn cellc_check_multiple_diagnostics_prints_each_source_context() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("Cell.toml"),
        r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bad"
version = "0.1.0"
entry = "src/main.cell"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(temp.path().join("src")).unwrap();
    std::fs::write(
        temp.path().join("src/main.cell"),
        r#"module multi_errors

action bad_one() -> u64 {
    verification
        return true
}

action bad_two() -> bool {
    verification
        return 1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("check").current_dir(temp.path()).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("return type mismatch: expected U64, found Bool"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("return type mismatch: expected Bool, found U64"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("5 |         return true"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("10 |         return 1"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("aborting due to 2 diagnostics"), "unexpected stderr: {stderr}");
    assert!(!stderr.contains("2 diagnostics:\n  -"), "unexpected collapsed diagnostics: {stderr}");
}

#[test]
fn cellc_ls_idl_validate_bind_and_bundle_preserve_raw_digest() {
    let temp = tempfile::tempdir().unwrap();
    let idl = br#"{"idl_version":"0.1","name":"demo_lock","witness":[{"name":"signature","type":"secp256k1_sig","required":true}]}"#;
    let idl_path = temp.path().join("idl.json");
    let executable_path = temp.path().join("lock");
    let bound_path = temp.path().join("lock.ls-idl");
    let source_path = temp.path().join("lock.rs");
    std::fs::write(&idl_path, idl).unwrap();
    std::fs::write(&executable_path, b"\x7fELFdemo-lock").unwrap();
    std::fs::write(&source_path, b"fn main() {}").unwrap();

    let validate = cellc_command().args(["artifact", "ls-idl", "validate", "--idl"]).arg(&idl_path).arg("--json").output().unwrap();
    assert!(validate.status.success(), "stderr: {}", String::from_utf8_lossy(&validate.stderr));

    let bind = cellc_command()
        .args(["artifact", "ls-idl", "bind", "--idl"])
        .arg(&idl_path)
        .arg("--executable")
        .arg(&executable_path)
        .arg("--output")
        .arg(&bound_path)
        .arg("--json")
        .output()
        .unwrap();
    assert!(bind.status.success(), "stderr: {}", String::from_utf8_lossy(&bind.stderr));
    let bound = std::fs::read(&bound_path).unwrap();
    let digest: [u8; 32] = Sha256::digest(idl).into();
    assert_eq!(&bound[bound.len() - 32..], digest);

    let bundle = cellc_command()
        .args(["artifact", "ls-idl", "bundle", "--idl"])
        .arg(&idl_path)
        .arg("--executable")
        .arg(&bound_path)
        .arg("--source")
        .arg(&source_path)
        .args([
            "--namespace",
            "cellscript",
            "--name",
            "demo-ls-idl-lock",
            "--release",
            "0.1.0",
            "--language",
            "rust",
            "--hash-type",
            "data1",
            "--dep-type",
            "code",
            "--toolchain",
            "rustc-1.97.1",
            "--source-revision",
            "0123456789abcdef0123456789abcdef01234567",
            "--output",
            "artifact.bundle.json",
            "--artifact-manifest-output",
            "Artifact.toml",
            "--json",
        ])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(bundle.status.success(), "stderr: {}", String::from_utf8_lossy(&bundle.stderr));

    let publish = cellc_command()
        .args(["publish", "--artifact-manifest", "Artifact.toml", "--dry-run", "--json"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(publish.status.success(), "stderr: {}", String::from_utf8_lossy(&publish.stderr));
}

#[test]
fn cellc_writes_requested_output_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.s");
    let source = r#"
module test

action add(x: u64, y: u64) -> u64 {
    verification
        let z = x + y
        return z
}
"#;
    std::fs::write(&input, source).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(written.contains(".global add"));

    let metadata = std::fs::read_to_string(dir.path().join("sample.s.meta.json")).unwrap();
    assert!(metadata.contains("\"actions\""));
    assert!(metadata.contains("\"add\""));
    assert!(metadata.contains("\"scheduler_witness_abi\""));
    assert!(metadata.contains("\"scheduler_witness_hex\""));
    assert!(!metadata.contains("\"scheduler_witness_molecule_hex\""));
    assert!(metadata.contains("\"metadata_schema_version\""));
    assert!(metadata.contains("\"compiler_version\""));
    assert!(metadata.contains("\"artifact_hash\""));
    assert!(metadata.contains("\"artifact_size_bytes\""));
    assert!(metadata.contains("\"source_hash\""));
    assert!(metadata.contains("\"source_content_hash\""));
    assert!(metadata.contains("\"source_units\""));
    assert!(metadata.contains("\"target_profile\""));
    assert!(metadata.contains("\"target_chain\""));
    assert!(metadata.contains("\"constraints\""));
    assert!(metadata.contains("\"entry_abi\""));
    assert!(metadata.contains("\"artifact\""));
    assert!(metadata.contains("\"runtime_errors\""));
    assert!(metadata.contains("\"source_metadata_schema_version\""));
    assert!(metadata.contains("\"artifact_metadata_schema_version\""));
    assert!(metadata.contains("\"constraints_metadata_schema_version\""));
}

#[test]
fn cellc_direct_elf_build_writes_and_reports_verified_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.cell");
    let output = dir.path().join("sample.elf");
    std::fs::write(
        &input,
        r#"
module test

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg(&input)
        .args(["--target", "riscv64-elf", "--json", "-o"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let payload: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let lowering = dir.path().join("sample.elf.lowering.json");
    let source_map = dir.path().join("sample.elf.sourcemap.json");
    assert_eq!(payload["lowering_record"], lowering.to_string_lossy().as_ref());
    assert_eq!(payload["source_map"], source_map.to_string_lossy().as_ref());
    assert!(lowering.is_file());
    assert!(source_map.is_file());

    let verify = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("verify-artifact")
        .arg(&output)
        .args(["--expect-target-profile", "ckb", "--json"])
        .output()
        .unwrap();
    assert!(verify.status.success(), "{}", String::from_utf8_lossy(&verify.stderr));
}

#[test]
fn cellc_verify_ckb_fixtures_accepts_standard_manifest() {
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("compat").join("ckb_standard").join("manifest.json");

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-ckb-fixtures").arg(&manifest).arg("--json").output().unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["manifest_schema"], "cellscript-ckb-standard-compat-v0.16");
    assert_eq!(report["execution_level"], "MODEL");
    assert_eq!(report["ckb_vm_execution"], false);
    assert_eq!(report["issue_count"], 0);
    assert!(report["fixture_count"].as_u64().unwrap() >= 14);
    assert!(report["manifest_hash"].as_str().is_some_and(|hash| hash.len() == 64));
}

#[test]
fn cellc_verify_ckb_fixtures_accepts_ickb_claim_manifest() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("benchmarks")
        .join("ickb_diff")
        .join("claim_manifest.json");

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-ckb-fixtures").arg(&manifest).arg("--json").output().unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["manifest_schema"], "cellscript-ickb-claim-manifest-v1");
    assert_eq!(report["execution_level"], "DIFFERENTIAL_CKB_VM_MANIFEST");
    assert_eq!(report["ckb_vm_execution"], false);
    assert_eq!(report["committed_ckb_vm_evidence"], true);
    assert_eq!(report["evidence_execution_level"], "DIFFERENTIAL_CKB_VM_EXECUTED");
    assert_eq!(report["required_executable_gate"], "cargo test --locked -p cellscript --test ickb_diff");
    assert!(
        report["vm_execution_note"].as_str().is_some_and(|note| note.contains("does not execute CKB VM")),
        "{}",
        report["vm_execution_note"]
    );
    assert_eq!(report["issue_count"], 0);
    assert!(report["fixture_count"].as_u64().unwrap() >= 8);
}

#[test]
fn cellc_verify_ckb_fixtures_rejects_ickb_claim_without_matrix_row() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("benchmarks")
        .join("ickb_diff")
        .join("claim_manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["families"][0]["branches"][0]["required_scenarios"] =
        serde_json::json!(["differential: missing iCKB protocol branch original vs CellScript agree"]);

    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("claim_manifest.json");
    std::fs::write(&invalid, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    std::fs::copy(manifest_path.parent().unwrap().join("matrix.json"), dir.path().join("matrix.json")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-ckb-fixtures").arg(&invalid).arg("--json").output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    let issues = report["issues"].as_array().unwrap().iter().filter_map(|issue| issue.as_str()).collect::<Vec<_>>().join("\n");
    assert!(issues.contains("required scenario is missing"), "{issues}");
}

#[test]
fn cellc_verify_ckb_fixtures_rejects_tampered_ickb_execution_evidence() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("benchmarks")
        .join("ickb_diff")
        .join("claim_manifest.json");
    let matrix_path = manifest_path.parent().unwrap().join("matrix.json");
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let mut matrix: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&matrix_path).unwrap()).unwrap();

    let rows = matrix["rows"].as_array_mut().unwrap();
    let pass_row =
        rows.iter_mut().find(|row| row["result"].as_str() == Some("differential-agree-pass")).expect("at least one pass row");
    pass_row["execution"]["cellscript_cycles"] = serde_json::json!(0);

    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("claim_manifest.json");
    std::fs::write(&invalid, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    std::fs::write(dir.path().join("matrix.json"), serde_json::to_vec_pretty(&matrix).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-ckb-fixtures").arg(&invalid).arg("--json").output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    let issues = report["issues"].as_array().unwrap().iter().filter_map(|issue| issue.as_str()).collect::<Vec<_>>().join("\n");
    assert!(issues.contains("cellscript pass must consume cycles"), "{issues}");
}

#[test]
fn cellc_verify_ckb_fixtures_rejects_invalid_manifest_claim() {
    let manifest_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("compat").join("ckb_standard").join("manifest.json");
    let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    manifest["schema"] = serde_json::Value::String("wrong-schema".to_string());

    let dir = tempfile::tempdir().unwrap();
    let invalid = dir.path().join("invalid-fixture-manifest.json");
    std::fs::write(&invalid, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("verify-ckb-fixtures").arg(&invalid).arg("--json").output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "failed");
    let issues = report["issues"].as_array().unwrap().iter().filter_map(|issue| issue.as_str()).collect::<Vec<_>>().join("\n");
    assert!(issues.contains("manifest schema must be cellscript-ckb-standard-compat-v0.16"), "{issues}");
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(report["diagnostics"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("CKB fixture manifest failed verification")));
}

#[test]
fn cellc_top_level_accepts_primitive_strict_for_kernel_effect_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("strict.cell");
    let output = dir.path().join("strict.s");
    std::fs::write(
        &input,
        r#"
module test

resource Token has store, consume, burn {
    amount: u64,
}

action burn(token: Token) {
    verification
        destroy token
}
"#,
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg(&input)
        .arg("--primitive-strict")
        .arg("0.15")
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();

    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    assert!(output.exists());
}

#[test]
fn cellc_top_level_primitive_strict_rejects_legacy_capabilities() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("legacy.cell");
    std::fs::write(
        &input,
        r#"
module test

resource Token has store, destroy {
    amount: u64,
}

action burn(token: Token) {
    verification
        destroy token
}
"#,
    )
    .unwrap();

    let run = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("--primitive-strict").arg("0.15").output().unwrap();

    assert!(!run.status.success(), "legacy capability should fail strict mode");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains("CS0151"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("legacy capability 'destroy'"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("consume + burn"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_compiles_bundled_examples_to_requested_outputs() {
    let fixture = fixtures::examples();
    let examples_dir = fixture.path();
    let output_dir = tempfile::tempdir().unwrap();

    for example in [
        "amm_pool.cell",
        "atomic_swap.cell",
        "launch.cell",
        "multi_phase_dao.cell",
        "multisig.cell",
        "nft.cell",
        "timelock.cell",
        "token.cell",
        "vesting.cell",
    ] {
        let input = examples_dir.join(example);
        let output = output_dir.path().join(example.replace(".cell", ".s"));

        let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&input).arg("-o").arg(&output).status().unwrap();
        assert!(status.success(), "cellc failed for {}", example);

        let written = std::fs::read_to_string(&output).unwrap();
        assert!(written.contains(".section .text"), "missing text section for {}", example);
        assert!(!written.trim().is_empty(), "empty output for {}", example);
    }
}

#[test]
fn cellc_compiles_package_with_local_path_dependency() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("token.cell"),
        r#"
module dep::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();

    let app_entry = app_root.join("src").join("main.cell");
    std::fs::write(
        &app_entry,
        r#"
module app::main

use dep::token::Token

action pass_through(token: Token) -> Token {
    verification
        token
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = app_root.join("build").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(written.contains(".global pass_through"));
    assert!(!app_entry.with_extension("s").exists());
}

#[test]
fn cellc_rejects_registry_dependency_without_namespace() {
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

[dependencies]
remote = "1.2.3"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("lock").output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("registry dependency 'remote' requires a namespace"), "unexpected stderr: {}", stderr);
    assert!(!root.join("Cell.lock").exists());
}

#[test]
fn cellc_init_accepts_phase1_namespace_flag() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("init")
        .arg("amm_pool")
        .arg("--namespace")
        .arg("cellscript")
        .current_dir(temp.path())
        .output()
        .unwrap();

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let manifest = std::fs::read_to_string(temp.path().join("Cell.toml")).unwrap();
    assert!(manifest.contains("namespace = \"cellscript\""), "manifest: {}", manifest);
}

#[test]
fn cellc_infers_creation_effects_from_path_dependency_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("token.cell"),
        r#"
module dep::token

resource Token {
    amount: u64
}

action issue(amount: u64) -> Token {
    verification
        let out = create Token {
            amount: amount
        }
        return out
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

use dep::token::Token
use dep::token::issue

action wrapper(amount: u64) -> Token {
    verification
        return issue(amount)
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata_path = app_root.join("effects.json");
    let metadata = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&app_root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata.status.success(), "{}", String::from_utf8_lossy(&metadata.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(
        metadata["actions"].as_array().unwrap().iter().find(|action| action["name"] == "wrapper").unwrap()["effects"]["lifecycle_ops"],
        serde_json::json!(["create"])
    );

    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

use dep::token::Token

action wrapper(amount: u64) -> Token {
    verification
        return dep::token::issue(amount)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let metadata_path = app_root.join("effects.json");
    let metadata = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&app_root)
        .arg("metadata")
        .arg("--output")
        .arg(&metadata_path)
        .output()
        .unwrap();
    assert!(metadata.status.success(), "{}", String::from_utf8_lossy(&metadata.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    assert_eq!(
        metadata["actions"].as_array().unwrap().iter().find(|action| action["name"] == "wrapper").unwrap()["effects"]["lifecycle_ops"],
        serde_json::json!(["create"])
    );
}

#[test]
fn cellc_compiles_external_dependency_function_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("math.cell"),
        r#"
module dep::math

fn add_one(x: u64) -> u64 {
    return x + 1
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

action run(x: u64) -> u64 {
    verification
        return dep::math::add_one(x)
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let artifact = std::fs::read_to_string(app_root.join("build").join("main.s")).unwrap();
    assert!(artifact.contains("call __cellscript.ext.6465703a3a6d617468.add_one"), "external call was not lowered:\n{}", artifact);
    assert!(artifact.contains("__cellscript.ext.6465703a3a6d617468.add_one:"), "external helper body was not merged:\n{}", artifact);
    assert!(!artifact.contains("call dep::math::add_one"), "qualified label leaked into assembly:\n{}", artifact);
}

#[test]
fn cellc_enforces_package_visibility_across_path_dependencies() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();
    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("math.cell"),
        r#"
module dep::math

public(package) fn package_secret(x: u64) -> u64 {
    return x + 1
}

public fn exported(x: u64) -> u64 {
    return x + 2
}
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();

    let app_source = app_root.join("src").join("main.cell");
    std::fs::write(
        &app_source,
        r#"
module app::main

use dep::math::package_secret as hidden

action run(x: u64) -> u64 {
    verification
        return hidden(x)
}
"#,
    )
    .unwrap();
    lock_package(&app_root);
    let aliased = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(!aliased.status.success(), "package-private alias unexpectedly compiled");
    let stderr = String::from_utf8_lossy(&aliased.stderr);
    assert!(stderr.contains("package_secret") && stderr.contains("public(package)"), "unexpected stderr: {stderr}");

    std::fs::write(
        &app_source,
        r#"
module app::main

action run(x: u64) -> u64 {
    verification
        return dep::math::package_secret(x)
}
"#,
    )
    .unwrap();
    lock_package(&app_root);
    let qualified = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(!qualified.status.success(), "qualified package-private call unexpectedly compiled");

    std::fs::write(
        app_root.join("src").join("internal.cell"),
        r#"
module app::internal

public(package) fn same_package(x: u64) -> u64 {
    return x + 3
}
"#,
    )
    .unwrap();
    std::fs::write(
        &app_source,
        r#"
module app::main

use app::internal::same_package
use dep::math::exported

action run(x: u64) -> u64 {
    verification
        return same_package(x) + exported(x)
}
"#,
    )
    .unwrap();
    lock_package(&app_root);
    let allowed = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(allowed.status.success(), "stderr: {}", String::from_utf8_lossy(&allowed.stderr));
}

#[test]
fn cellc_compiles_aliased_external_dependency_function_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("math.cell"),
        r#"
module dep::math

fn add_one(x: u64) -> u64 {
    return x + 1
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

use dep::math::add_one as plus_one
use dep::math::add_one as inc

action run(x: u64) -> u64 {
    verification
        return plus_one(x) + inc(x)
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let artifact = std::fs::read_to_string(app_root.join("build").join("main.s")).unwrap();
    assert!(
        artifact.contains("call __cellscript.ext.6465703a3a6d617468.add_one"),
        "aliased external call was not lowered:\n{}",
        artifact
    );
    assert_eq!(
        artifact.matches("__cellscript.ext.6465703a3a6d617468.add_one:").count(),
        1,
        "aliases must share one defining helper body"
    );
    assert!(!artifact.contains("plus_one:"), "a source alias must not create a second physical helper:\n{}", artifact);
    assert!(!artifact.contains("call inc"), "duplicate alias did not reuse the canonical imported label:\n{}", artifact);
    assert!(!artifact.contains("inc:"), "duplicate alias emitted a second helper body:\n{}", artifact);
    assert!(!artifact.contains("call add_one"), "alias call fell back to the dependency basename:\n{}", artifact);
}

#[test]
fn cellc_compiles_same_basename_external_dependency_function_calls_without_collision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_a_root = root.join("dep_a_pkg");
    let dep_b_root = root.join("dep_b_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_a_root.join("src")).unwrap();
    std::fs::create_dir_all(dep_b_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    for (dep_root, package, module, delta) in
        [(&dep_a_root, "dep_a_pkg", "dep_a::math", 1_u64), (&dep_b_root, "dep_b_pkg", "dep_b::math", 2_u64)]
    {
        std::fs::write(
            dep_root.join("Cell.toml"),
            format!(
                r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "{package}"
version = "0.1.0"
"#
            ),
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("math.cell"),
            format!(
                r#"
module {module}

fn add_one(x: u64) -> u64 {{
    return x + {delta}
}}
"#
            ),
        )
        .unwrap();
    }

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_a_pkg = { path = "../dep_a_pkg" }
dep_b_pkg = { path = "../dep_b_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

action run(x: u64) -> u64 {
    verification
        return dep_a::math::add_one(x) + dep_b::math::add_one(x)
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let artifact = std::fs::read_to_string(app_root.join("build").join("main.s")).unwrap();
    assert!(artifact.contains("call __cellscript.ext.6465705f613a3a6d617468.add_one"), "dep_a call was not lowered:\n{}", artifact);
    assert!(artifact.contains("call __cellscript.ext.6465705f623a3a6d617468.add_one"), "dep_b call was not lowered:\n{}", artifact);
    assert!(
        artifact.contains("__cellscript.ext.6465705f613a3a6d617468.add_one:")
            && artifact.contains("__cellscript.ext.6465705f623a3a6d617468.add_one:"),
        "same-basename external helpers were not both merged:\n{}",
        artifact
    );
    assert!(!artifact.contains("call dep_a::math::add_one"), "dep_a qualified label leaked into assembly:\n{}", artifact);
    assert!(!artifact.contains("call dep_b::math::add_one"), "dep_b qualified label leaked into assembly:\n{}", artifact);
}

#[test]
fn cellc_compiles_transitive_external_dependency_function_calls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let dep_root = root.join("dep_pkg");
    let app_root = root.join("app_pkg");

    std::fs::create_dir_all(dep_root.join("src")).unwrap();
    std::fs::create_dir_all(app_root.join("src")).unwrap();

    std::fs::write(
        dep_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        dep_root.join("src").join("math.cell"),
        r#"
module dep::math

fn add_one(x: u64) -> u64 {
    return x + 1
}

fn add_two(x: u64) -> u64 {
    return add_one(x) + 1
}
"#,
    )
    .unwrap();

    std::fs::write(
        app_root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
    )
    .unwrap();
    std::fs::write(
        app_root.join("src").join("main.cell"),
        r#"
module app::main

action run(x: u64) -> u64 {
    verification
        return dep::math::add_two(x)
}
"#,
    )
    .unwrap();

    lock_package(&app_root);
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(&app_root).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let artifact = std::fs::read_to_string(app_root.join("build").join("main.s")).unwrap();
    assert!(artifact.contains("call __cellscript.ext.6465703a3a6d617468.add_two"), "outer helper call was not lowered:\n{}", artifact);
    assert!(
        artifact.contains("call __cellscript.ext.6465703a3a6d617468.add_one"),
        "transitive helper call was not lowered:\n{}",
        artifact
    );
    assert!(
        artifact.contains("__cellscript.ext.6465703a3a6d617468.add_two:")
            && artifact.contains("__cellscript.ext.6465703a3a6d617468.add_one:"),
        "transitive external helpers were not merged:\n{}",
        artifact
    );
}

#[test]
fn cellc_uses_manifest_build_out_dir_for_package_input() {
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

[build]
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(!root.join("build").join("main.s").exists());
}

#[test]
fn cellc_cli_target_overrides_manifest_build_target() {
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

[build]
target = "riscv64-elf"
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.s");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).arg("--target").arg("riscv64-asm").status().unwrap();

    assert!(status.success());

    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains(".section .text"));
    assert!(!written.trim().is_empty());
}

#[test]
fn cellc_uses_manifest_build_target_by_default() {
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

[build]
target = "riscv64-elf"
out_dir = "artifacts"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = root.join("artifacts").join("main.elf");
    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(root).status().unwrap();

    assert!(status.success());

    let written = std::fs::read(&output).unwrap();
    assert!(written.starts_with(b"\x7fELF"));
    assert!(!root.join("artifacts").join("main.s").exists());
}

#[test]
fn cellc_build_and_check_subcommands_use_package_flow() {
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

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").status().unwrap();
    assert!(check.success());

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").status().unwrap();
    assert!(build.success());

    let output = root.join("build").join("main.s");
    let written = std::fs::read_to_string(output).unwrap();
    assert!(written.contains(".section .text"));
    let metadata = std::fs::read_to_string(root.join("build").join("main.s.meta.json")).unwrap();
    assert!(metadata.contains("\"module\": \"demo::main\""));
    assert!(metadata.contains("\"scheduler_witness_abi\""));
    assert!(metadata.contains("\"scheduler_witness_hex\""));
    assert!(!metadata.contains("\"scheduler_witness_molecule_hex\""));

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["artifact_format"], "RISC-V assembly");
    assert_eq!(stdout["compilation_policy"], cellscript::COMPILATION_POLICY);
    assert!(stdout.get("opt_level").is_none());
    assert_eq!(stdout["target_profile"], "ckb");
    assert_eq!(stdout["policy_verified"], false);
    assert_eq!(stdout["runtime_required_verifier_obligations"], 0);
    assert_eq!(stdout["fail_closed_verifier_obligations"], 0);
    assert!(stdout["artifact"].as_str().unwrap().ends_with("build/main.s"));
    assert!(stdout["metadata"].as_str().unwrap().ends_with("build/main.s.meta.json"));
    assert!(stdout["artifact_hash"].as_str().unwrap().len() == 64);
    assert!(stdout["source_content_hash"].as_str().unwrap().len() == 64);
    assert_eq!(stdout["constraints"]["target_profile"], "ckb");
    assert_eq!(stdout["constraints"]["status"], "warn");
    assert!(stdout["constraints"]["artifact"]["artifact_size_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn cellc_rejects_incompatible_package_compiler_before_source_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
edition = "2027"
name = "future-package"
version = "1.0.0"
cellscript_version = ">=999.0.0"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), "this source must never reach the parser").unwrap();

    let output = cellc_command().current_dir(root).args(["check", "--json"]).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2600");
    assert_eq!(report["package"], "future-package");
    assert_eq!(report["compiler_requirement"], ">=999.0.0");
    assert_eq!(report["phase"], "manifest-before-source");
    assert!(!report.to_string().contains("parser"), "unexpected parse diagnostic: {report}");
}

#[test]
fn cellc_json_reports_all_incompatible_dependency_packages_and_incoming_edges() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for package in ["future_a", "future_b"] {
        let package_root = root.join(package);
        std::fs::create_dir_all(package_root.join("src")).unwrap();
        std::fs::write(
            package_root.join("Cell.toml"),
            format!(
                r#"
[package]
edition = "2027"
name = "{package}"
version = "1.0.0"
cellscript_version = ">=999.0.0"
"#
            ),
        )
        .unwrap();
        std::fs::write(package_root.join("src/lib.cell"), "invalid source must not be parsed").unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.first]
package = "future_a"
path = "future_a"

[dependencies.second]
package = "future_b"
path = "future_b"
"#,
    )
    .unwrap();

    let output = cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2600");
    assert_eq!(report["phase"], "dependency-compatibility-preflight");
    let incompatible = report["incompatible_packages"].as_array().unwrap();
    assert_eq!(incompatible.len(), 2);
    assert_eq!(incompatible[0]["package"], "future_a");
    assert_eq!(incompatible[0]["incoming_edge"]["alias"], "first");
    assert_eq!(incompatible[1]["package"], "future_b");
    assert_eq!(incompatible[1]["incoming_edge"]["alias"], "second");
    assert!(!root.join("Cell.lock").exists());
}

#[test]
fn cellc_json_reports_package_instance_conflicts_before_writing_a_lock() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for path in ["published", "substitute"] {
        let package_root = root.join(path);
        std::fs::create_dir_all(package_root.join("src")).unwrap();
        std::fs::write(
            package_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "shared"
version = "1.0.0"
namespace = "cellscript"
"#,
        )
        .unwrap();
        std::fs::write(package_root.join("src/lib.cell"), format!("module {path};\n")).unwrap();
    }
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app"
version = "1.0.0"

[dependencies.first]
package = "shared"
path = "published"

[dependencies.second]
package = "shared"
path = "substitute"
"#,
    )
    .unwrap();

    let output = cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();

    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2601");
    assert_eq!(report["coordinate"]["namespace"], "cellscript");
    assert_eq!(report["coordinate"]["name"], "shared");
    assert_eq!(report["conflict_kind"], "source");
    let incoming = report["incoming_edges"].as_array().unwrap();
    assert_eq!(incoming.len(), 2);
    assert_eq!(incoming[0]["alias"], "first");
    assert_eq!(incoming[1]["alias"], "second");
    assert!(!root.join("Cell.lock").exists());
}

#[test]
fn cellc_check_json_reports_multiple_compile_diagnostics() {
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

action bad() -> bool {
    verification
        let first: u64 = true
        let second: bool = 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    let diagnostics = stdout["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2, "unexpected diagnostics: {stdout}");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected U64, found Bool")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected Bool, found U64")));
    assert_eq!(stdout["diagnostic_count"], 2);
    assert_eq!(stdout["error_count"], 2);
    assert_eq!(stdout["warning_count"], 0);
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.get("range").is_some()));
    assert!(diagnostics.iter().all(|diagnostic| diagnostic["range"]["start"]["line"].as_u64().unwrap_or_default() > 0));
}

#[test]
fn cellc_check_json_reports_multiple_parse_diagnostics() {
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

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["diagnostic_count"], 2);
    assert_eq!(stdout["error_count"], 2);
    assert_eq!(stdout["warning_count"], 0);
    let diagnostics = stdout["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2, "unexpected diagnostics: {stdout}");
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found 'true'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found integer 1")));
    assert!(diagnostics.iter().all(|diagnostic| diagnostic["range"]["start"]["line"].as_u64().unwrap_or_default() > 0));
}

#[test]
fn cellc_check_json_reports_diagnostics_on_stdout() {
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

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["check", "--json"]).output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["diagnostic_count"], 2);
    assert_eq!(stdout["error_count"], 2);
    assert_eq!(stdout["warning_count"], 0);
    let diagnostics = stdout["diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found 'true'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic["message"].as_str().unwrap().contains("expected '=', found integer 1")));
    assert!(diagnostics.iter().all(|diagnostic| diagnostic["range"]["start"]["line"].as_u64().unwrap_or_default() > 0));
}

#[test]
fn cellc_json_is_global_for_package_commands_and_build_failures_use_stdout() {
    let temp = tempfile::tempdir().unwrap();
    for args in [["build", "--json"], ["--json", "build"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(temp.path()).args(args).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
        let diagnostic: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(diagnostic["status"], "failed");
        assert_eq!(diagnostic["diagnostic_count"], 1);
    }
}

#[test]
fn cellc_message_format_json_remains_a_hidden_stdout_compatibility_alias() {
    let temp = tempfile::tempdir().unwrap();
    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(temp.path()).args(["build", "--message-format=json"]).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stderr.is_empty(), "unexpected stderr: {}", String::from_utf8_lossy(&output.stderr));
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(diagnostic["status"], "failed");
}

#[test]
fn cellc_check_json_reports_multiple_ir_diagnostics() {
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

resource Token has store { amount: u64 }

action issue_one(witness handle: ExactScriptHandle, witness expected: Hash) -> u64 {
    let input = ckb::input<Token>(0)
    ckb::require_cell_lock_exact_handle(input, handle, expected)
    return 0
}

action issue_two(witness handle: ExactScriptHandle, witness expected: Hash) -> u64 {
    let input = ckb::input<Token>(0)
    ckb::require_cell_type_exact_handle(input, handle, expected)
    return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["diagnostic_count"], 2);
    assert_eq!(stdout["error_count"], 2);
    assert_eq!(stdout["warning_count"], 0);
    let diagnostics = stdout["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2, "unexpected diagnostics: {stdout}");
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic["message"].as_str().unwrap().contains("exact Script handle handle_hash must be a compile-time Hash literal")
            && diagnostic["range"]["start"]["line"].as_u64().unwrap_or_default() > 0
    }));
    assert_ne!(diagnostics[0]["range"]["start"]["line"], diagnostics[1]["range"]["start"]["line"]);
}

#[test]
fn cellc_check_accepts_pure_ckb_target_profile() {
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

action add(x: u64, y: u64) -> u64 {
    verification
        return x + y
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("check")
        .arg("--target-profile")
        .arg("ckb")
        .arg("--json")
        .output()
        .unwrap();

    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    let checked_targets = stdout["checked_targets"].as_array().unwrap();
    assert_eq!(checked_targets.len(), 1);
    assert_eq!(checked_targets[0]["target_profile"], "ckb");
    assert_eq!(checked_targets[0]["compiled_target_profile"], "ckb");
    assert!(checked_targets[0]["target_profile_policy_violations"].as_array().unwrap().is_empty());
}

#[test]
fn cellc_check_accepts_ckb_profile_timepoint() {
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

action now() -> u64 {
    verification
        return env::current_timepoint()
}
"#,
    )
    .unwrap();

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--target-profile").arg("ckb").output().unwrap();

    assert!(output.status.success(), "check should succeed with timepoint: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn cellc_check_production_rejects_fail_closed_runtime_paths() {
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
}

#[test]
fn cellc_production_rejects_bounded_collection_consensus_gaps_before_codegen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bounded-gap"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module bounded::gap

struct Plan { amount: u64 }
resource DynamicToken has store, consume {
    amount: u64
    memo: String
}
resource Token has store, create { amount: u64 }

action batch(input inputs: BoundedCellSet<DynamicToken, 2>, witness plans: BoundedList<Plan, 2>) -> u64 {
    verification
        consume_each token in inputs {
            require false
            require token.amount > 0
        }
        create_each plan in plans {
            require false
            create Token { amount: plan.amount }
        }
        return 0
}
"#,
    )
    .unwrap();

    let check =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").arg("--json").output().unwrap();
    assert!(!check.status.success(), "unexpected success: {}", String::from_utf8_lossy(&check.stdout));
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2105");
    assert_eq!(report["phase"], "pre-codegen");
    assert_eq!(report["consensus_gaps"].as_array().map(Vec::len), Some(2));
    assert_eq!(report["consensus_gaps"][0]["operation"], "consume_each:inputs:BoundedCellSet<DynamicToken, 2>");
    assert_eq!(report["consensus_gaps"][0]["missing_enforcement"], "gap:runtime-helper-required");
    assert_eq!(report["consensus_gaps"][1]["missing_enforcement"], "gap:builder-evidence-required");
    assert!(report["consensus_gaps"][0]["remediation"].as_str().unwrap().contains("fixed-arity"));

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--production").output().unwrap();
    assert!(!build.status.success(), "unexpected success: {}", String::from_utf8_lossy(&build.stdout));
    let stderr = String::from_utf8_lossy(&build.stderr);
    assert!(stderr.contains("error[E2105]"), "unexpected stderr: {stderr}");
    assert!(stderr.contains("create_each") || stderr.contains("consume_each"), "unexpected stderr: {stderr}");
    assert!(!root.join("build").join("main.s").exists());
    assert!(!root.join("build").join("main.s.meta.json").exists());
}

#[test]
fn cellc_errors_include_compiler_ecode_when_surface_is_rejected_before_codegen() {
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

action append_schema_vec(items: Vec<Address>, owner: Address) -> u64 {
    verification
        let mut values = items
        values.push(owner)
        return values.len()
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--production").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[E2105]"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("cellc explain E2105"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("collection-push"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("stopped before codegen"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_can_reject_runtime_required_obligations() {
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

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 1, "unexpected stdout: {}", stdout);
    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value
            .as_str()
            .is_some_and(|summary| { summary.contains("create-output:Fingerprint") && summary.contains("(runtime-required)") })),
        "unexpected runtime-required transaction runtime input summaries: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("check policy failed"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required verifier obligations"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input requirements"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input blockers"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input blocker classes"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("create-output:Fingerprint"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_reports_transaction_invariant_checked_subconditions() {
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

resource Token has store {
    amount: u64
    owner: Address
}

receipt VestingGrant has store {
    state: u8
    beneficiary: Address
    total_amount: u64
    claimed_amount: u64
    cliff_timepoint: u64
    end_timepoint: u64
}

flow VestingGrant.state {
    Granted -> Claimable;
    Granted -> FullyClaimed;
    Claimable -> FullyClaimed;
}

action claim_vested(grant: VestingGrant) -> (tokens: Token, updated_grant: VestingGrant) {
    transition grant.state: Claimable -> updated_grant.state: FullyClaimed
    verification
        let now = env::current_timepoint()

        require now >= grant.cliff_timepoint, "cliff not reached"
        require grant.state < VestingGrant::FullyClaimed, "already fully claimed"

        let vested_total = grant.total_amount
        let claimable = vested_total - grant.claimed_amount
        require claimable > 0, "nothing to claim"

        consume grant

        let new_state: u8 = if vested_total == grant.total_amount { VestingGrant::FullyClaimed } else { VestingGrant::Claimable }

        create tokens = Token {
            amount: claimable,
            owner: grant.beneficiary
        } with_lock(grant.beneficiary)

        create updated_grant = VestingGrant {
            state: new_state,
            beneficiary: grant.beneficiary,
            total_amount: grant.total_amount,
            claimed_amount: grant.claimed_amount + claimable,
            cliff_timepoint: grant.cliff_timepoint,
            end_timepoint: grant.end_timepoint
        } with_lock(grant.beneficiary)
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_invariants"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_invariant_checked_subconditions"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["transaction_runtime_input_requirements"], 5, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["checked_transaction_runtime_input_requirements"], 5, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 0, "unexpected stdout: {}", stdout);
    let summaries = target["runtime_required_transaction_invariant_checked_subcondition_summaries"]
        .as_array()
        .expect("transaction invariant summaries array");
    assert!(summaries.is_empty(), "claim guards should be checked-runtime now: {}", stdout);
    let runtime_inputs =
        target["transaction_runtime_input_requirement_summaries"].as_array().expect("transaction runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("consume-input:VestingGrant:grant:consume-input-data=Input:grant.data")
                && summary.contains("consume-load-cell-input")
        })),
        "unexpected transaction runtime input summaries: {}",
        stdout
    );
    let checked_runtime_inputs = target["checked_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("checked transaction runtime input summaries array");
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("consume-input:VestingGrant:grant:consume-input-data=Input:grant.data")
                && summary.contains("consume-load-cell-input")
                && summary.contains("(checked-runtime)")
                && !summary.contains("blocker=")
                && !summary.contains("blocker_class=")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("create-output:Token:tokens:create-output-fields=Output:tokens.fields")
                && summary.contains("create-output-field-verifier")
                && summary.contains("(checked-runtime)")
                && !summary.contains("blocker=")
                && !summary.contains("blocker_class=")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("create-output:VestingGrant:updated_grant:create-output-lock=Output:updated_grant.lock_hash")
                && summary.contains("create-output-lock-hash-32[32]")
                && summary.contains("(checked-runtime)")
                && !summary.contains("blocker=")
                && !summary.contains("blocker_class=")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    let runtime_required_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(runtime_required_inputs.is_empty(), "claim input requirements should be checked-runtime now: {}", stdout);
    let runtime_input_blockers = target["runtime_required_transaction_runtime_input_blocker_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input blocker summaries array");
    assert!(runtime_input_blockers.is_empty(), "claim blockers should be checked-runtime now: {}", stdout);
    let runtime_input_blocker_classes = target["runtime_required_transaction_runtime_input_blocker_class_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input blocker class summaries array");
    assert!(runtime_input_blocker_classes.is_empty(), "claim blocker classes should be checked-runtime now: {}", stdout);

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(
        output.status.success(),
        "checked obligations should satisfy deny-runtime-obligations: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cellc_check_reports_resource_conservation_blocker_class() {
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

resource Token has store {
    amount: u64
}

action withdraw(token: Token, fee: u64) -> Token {
    verification
        let amount = token.amount
        let remaining = amount - fee
        consume token
        let out = create Token {
            amount: remaining
        }
        return out
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 1, "unexpected stdout: {}", stdout);

    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("resource-conservation:Token:resource-conservation-proof=Transaction:Token.input-output-conservation")
                && summary.contains("resource-conservation-consume-create-accounting")
                && summary.contains("(runtime-required)")
                && summary.contains("blocker=resource conservation is not fully lowered for this consumed-input/created-output shape")
                && summary.contains("blocker_class=resource-conservation-proof-gap")
        })),
        "unexpected runtime-required transaction runtime input summaries: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("resource-conservation:Token"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("runtime-required transaction runtime input blocker classes"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("resource-conservation-proof-gap"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_reports_explicit_output_binding_without_mutable_state_blockers() {
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

shared Ledger has store {
    balance: u128,
    owner: Address,
}

action credit(ledger_before: Ledger, delta: u128) -> ledger_after: Ledger {
    verification
        require ledger_after.owner == ledger_before.owner
        require ledger_after.balance == ledger_before.balance + delta
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 0, "unexpected stdout: {}", stdout);

    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(runtime_inputs.is_empty(), "unexpected runtime-required transaction runtime input summaries: {}", stdout);

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(
        output.status.success(),
        "explicit output requirements should not report mutable-state runtime blockers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cellc_check_reports_dynamic_output_blocker_class() {
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

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 1, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 1, "unexpected stdout: {}", stdout);

    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(
        runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("create-output:Fingerprint")
                && summary.contains("(runtime-required)")
                && summary.contains("blocker=create output field verifier is incomplete")
                && summary.contains("blocker_class=create-output-verification-gap")
        })),
        "unexpected runtime-required transaction runtime input summaries: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("create-output:Fingerprint"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("create-output-verification-gap"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("create output field verifier is incomplete"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_check_rejects_cell_backed_vec_with_source_aware_guidance() {
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

resource NFT {
    token_id: u64
    owner: Address
}

action batch_mint(owner: Address) -> Vec<NFT> {
    verification
        let mut nfts = Vec::new()
        let nft = create NFT {
            token_id: 1,
            owner: owner
        }
        nfts.push(nft)
        return nfts
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(!json_output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&json_output.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["diagnostic_count"], 1);
    let diagnostics = stdout["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic["message"].as_str().is_some_and(|message| {
                message.contains("type 'Vec<NFT>' cannot store a cell-backed resource")
                    && message.contains("use a source-aware BoundedCellSet<T, N> with explicit ownership")
            })
        }),
        "unexpected diagnostics: {}",
        stdout
    );
}

#[test]
fn cellc_check_accepts_u128_mutable_state_transition_with_u64_delta() {
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

shared Ledger has store {
    balance: u128,
    owner: Address,
}

action credit(ledger_before: Ledger, delta: u64) -> ledger_after: Ledger {
    verification
        require ledger_after.owner == ledger_before.owner
        require ledger_after.balance == ledger_before.balance + delta
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 0, "unexpected stdout: {}", stdout);

    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(runtime_inputs.is_empty(), "unexpected runtime-required transaction runtime input summaries: {}", stdout);

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn cellc_check_rejects_undeclared_flow_edge() {
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
    // All three states are declared in the flow, but only Live -> Filled and the
    // Filled <-> Cancelled edges exist. `cancel` uses Live -> Cancelled, which is
    // not a declared edge, so the static flow-edge membership check must reject it.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
    Filled -> Cancelled;
    Cancelled -> Filled;
}

action cancel(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Cancelled
    verification
        require input.amount == output.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("action 'cancel' transition 'Offer.state Live -> Cancelled' is not declared in the flow"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cellc_check_accepts_declared_cyclic_flow_edge() {
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
    // Declared cyclic edges (Open <-> Closed) must be accepted by the static flow
    // membership validator; the cycle is a legitimate state machine, not an error.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Pool has store {
    state: u8
    reserve: u64
}

flow Pool.state {
    Open -> Closed;
    Closed -> Open;
}

action close(pool_before: Pool) -> pool_after: Pool {
    transition pool_before.state: Open -> pool_after.state: Closed
    verification
        require pool_after.reserve == pool_before.reserve
}

action reopen(pool_before: Pool) -> pool_after: Pool {
    transition pool_before.state: Closed -> pool_after.state: Open
    verification
        require pool_after.reserve == pool_before.reserve
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn cellc_check_accepts_declared_linear_flow_edge() {
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
    // A linear (acyclic) state machine must be accepted when every action uses a
    // declared edge. This is the positive counterpart to the undeclared-edge test.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
    Filled -> Cancelled;
    Cancelled -> Filled;
}

action fill(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        require input.amount == output.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn cellc_check_rejects_flow_create_missing_state_field() {
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

resource Offer has store, create {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action seed(recipient: Address) -> output: Offer {
    verification
        create output = Offer { amount: 0 } with_lock(recipient)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("create of flow type 'Offer' must set its state field"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_check_rejects_initial_flow_create_non_static_state() {
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
    // The initial create of a flow-typed cell uses a runtime-derived state value,
    // which the static flow-state contract forbids.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store, create {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action seed(dynamic_state: u8, recipient: Address) -> output: Offer {
    verification
        create output = Offer { state: dynamic_state, amount: 0 } with_lock(recipient)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("initial create of flow type 'Offer' must use a statically known declared state"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cellc_check_rejects_flow_state_index_out_of_range() {
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
    // A statically-known state index that falls outside the declared state set
    // (two states: Live=0, Filled=1) must be rejected by the flow-state contract.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store, create {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
}

action seed(recipient: Address) -> output: Offer {
    verification
        create output = Offer { state: 99, amount: 0 } with_lock(recipient)
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("flow state index 99 is out of range for 'Offer' with 2 states"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_check_rejects_duplicate_flow_edge() {
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
    // Declaring the same edge twice is a static error: the flow block must declare a
    // set of distinct transitions, not a multiset.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store {
    state: u8
    amount: u64
}

flow Offer.state {
    Live -> Filled;
    Live -> Filled;
    Filled -> Cancelled;
}

action fill(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        require input.amount == output.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("duplicate state transition 'Live -> Filled'"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_check_rejects_transition_on_type_without_flow_block() {
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
    // An action declares a state-annotated transition, but the target type has no
    // `flow` block. The compiler must reject it rather than silently accepting the
    // transition as a plain consume/create.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Offer has store {
    state: u8
    amount: u64
}

action fill(input: Offer) -> output: Offer {
    transition input.state: Live -> output.state: Filled
    verification
        require input.amount == output.amount
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("type 'Offer' has no declared flow"), "unexpected stderr: {stderr}");
}

#[test]
fn cellc_check_rejects_aggregate_invariant_scope_mismatch() {
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

[build]
target_profile = "ckb"
"#,
    )
    .unwrap();
    // The invariant declares scope: transaction but the aggregate reads group-scoped
    // endpoints (group_inputs/group_outputs). The aggregate scope must match the
    // enclosing invariant scope.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::main

resource Token has store, create, consume {
    amount: u128,
}

invariant wrong_scope_conservation {
    trigger: type_group
    scope: transaction
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

action transfer(input: Token) -> output: Token {
    verification
        xudt::require_group_amount_conserved()
        preserve output from input {
            amount
        }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aggregate invariant scope 'group' must match enclosing invariant scope 'transaction'"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cellc_check_reports_claim_source_predicate_blocker_class() {
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

resource Token has store {
    amount: u64
}

resource VestingReceipt has store {
    amount: u64
    beneficiary: Address
    cliff_timepoint: u64
}

action redeem_after_cliff(receipt: VestingReceipt) -> Token {
    verification
        let now = env::current_timepoint()
        require now >= receipt.cliff_timepoint, "cliff not reached"

        consume receipt

        create Token {
            amount: receipt.amount
        } with_lock(receipt.beneficiary)
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["transaction_runtime_input_requirements"], 3, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_requirements"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["checked_transaction_runtime_input_requirements"], 3, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blockers"], 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_transaction_runtime_input_blocker_classes"], 0, "unexpected stdout: {}", stdout);

    let runtime_inputs = target["runtime_required_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("runtime-required transaction runtime input summaries array");
    assert!(runtime_inputs.is_empty(), "unexpected runtime-required transaction runtime input summaries: {}", stdout);

    let checked_runtime_inputs = target["checked_transaction_runtime_input_requirement_summaries"]
        .as_array()
        .expect("checked transaction runtime input summaries array");
    assert!(
        checked_runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| {
            summary.contains("consume-input:VestingReceipt:receipt:consume-input-data=Input:receipt.data")
                && summary.contains("consume-load-cell-input")
                && summary.contains("(checked-runtime)")
        })),
        "unexpected checked transaction runtime input summaries: {}",
        stdout
    );
    // Using consume instead of claim, so only consume-input runtime requirements are present.
    // The checked_transaction_runtime_input_requirements count is 3:
    // 1. consume-input:VestingReceipt
    // 2. create-output:Token (fields)
    // 3. create-output:Token (lock_hash)
}

#[test]
fn cellc_check_reports_pool_invariant_policy_families() {
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

resource Token has store {
    symbol: [u8; 8]
    amount: u64
}

receipt LPReceipt has store {
    pool_id: Hash
    lp_amount: u64
    provider: Address
}

shared Pool has store {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

action seed_pool(token_a: Token, token_b: Token, fee_rate_bps: u16, provider: Address) -> (Pool, LPReceipt) {
    verification
        require token_a.symbol != token_b.symbol, "same token"
        require token_a.amount > 0 && token_b.amount > 0, "zero liquidity"
        require fee_rate_bps <= 10000, "fee too high"
        require token_a.type_hash() != token_b.type_hash(), "same token type"

        let initial_lp: u64 = token_a.amount
        consume token_a
        consume token_b

        let pool = create Pool {
            token_a_symbol: token_a.symbol,
            token_b_symbol: token_b.symbol,
            reserve_a: token_a.amount,
            reserve_b: token_b.amount,
            total_lp: initial_lp,
            fee_rate_bps: fee_rate_bps
        }

        let receipt = create LPReceipt {
            pool_id: pool.type_hash(),
            lp_amount: initial_lp,
            provider: provider
        } with_lock(provider)

        (pool, receipt)
}
"#,
    )
    .unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert!(target["checked_pool_invariant_families"].as_u64().unwrap() > 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_pool_invariant_families"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["pool_runtime_input_requirements"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    let runtime_inputs = target["pool_runtime_input_requirement_summaries"].as_array().expect("runtime input summaries array");
    assert!(runtime_inputs.is_empty(), "checked seed_pool identity should leave no Pool runtime inputs: {}", stdout);

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(
        output.status.success(),
        "checked seed_pool identity should satisfy deny-runtime-obligations: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cellc_check_reports_amm_pool_without_runtime_blockers() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let amm_source = std::fs::read_to_string(manifest_dir.join("examples").join("amm_pool.cell"))
        .unwrap()
        .replace("use cellscript::fungible_token::Token", "resource Token has store {\n    symbol: [u8; 8]\n    amount: u64\n}");

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
    std::fs::write(root.join("src").join("main.cell"), amm_source).unwrap();

    let json_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--json").output().unwrap();
    assert!(json_output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&json_output.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    let target = &stdout["checked_targets"][0];
    assert_eq!(target["checked_pool_invariant_families"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_pool_invariant_families"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    assert_eq!(target["runtime_required_pool_invariant_blocker_classes"].as_u64().unwrap(), 0, "unexpected stdout: {}", stdout);
    let blocker_classes = target["runtime_required_pool_invariant_blocker_class_summaries"]
        .as_array()
        .expect("runtime-required Pool invariant blocker class summaries array");
    assert!(blocker_classes.is_empty(), "AMM pool admission should not leave runtime-required blockers: {}", stdout);
    let runtime_inputs = target["pool_runtime_input_requirement_summaries"].as_array().expect("runtime input summaries array");
    assert!(
        !runtime_inputs.iter().any(|value| value.as_str().is_some_and(|summary| { summary.contains("reserve-conservation=") })),
        "AMM reserve-conservation should not appear in Pool runtime input summaries: {}",
        stdout
    );
    assert_eq!(
        target["runtime_required_transaction_runtime_input_requirements"].as_u64().unwrap(),
        0,
        "unexpected stdout: {}",
        stdout
    );
    assert_eq!(
        target["runtime_required_transaction_runtime_input_blocker_classes"].as_u64().unwrap(),
        0,
        "unexpected stdout: {}",
        stdout
    );

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").arg("--deny-runtime-obligations").output().unwrap();
    assert!(
        output.status.success(),
        "full AMM policy should satisfy deny-runtime-obligations: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cellc_check_uses_manifest_policy_defaults() {
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

[policy]
production = true
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

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error[E2105]"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("stopped before codegen"), "unexpected stderr: {}", stderr);
    assert!(stderr.contains("output-verification-incomplete"), "unexpected stderr: {}", stderr);
}

#[test]
fn cellc_build_production_accepts_closed_u128_modulo_lowering() {
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

fn mod128(left: u128, right: u128) -> u128 {
    return left % right
}

action remainder(left: u128, right: u128) -> u128 {
    verification
        return mod128(left, right)
}
"#,
    )
    .unwrap();

    let metadata = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("metadata").output().unwrap();
    assert!(metadata.status.success(), "metadata-only analysis failed: {}", String::from_utf8_lossy(&metadata.stderr));
    assert!(!String::from_utf8_lossy(&metadata.stdout).contains("u128-modulo"));

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--production").output().unwrap();
    assert!(output.status.success(), "unexpected failure: {}", String::from_utf8_lossy(&output.stderr));
    assert!(root.join("build").join("main.s").exists());
    assert!(root.join("build").join("main.s.meta.json").exists());
}

#[test]
fn cellc_doc_subcommand_generates_markdown_docs() {
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

action ping() -> u64 {
    verification
        1
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("doc")
        .arg("--format")
        .arg("markdown")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["format"], "markdown");
    assert!(summary["output"].as_str().unwrap().ends_with("docs/cellscript-api.md"));
    assert!(summary["output_size_bytes"].as_u64().unwrap() > 0);

    let docs = std::fs::read_to_string(root.join("docs").join("cellscript-api.md")).unwrap();
    assert!(docs.contains("## Module `demo::main`"));
    assert!(docs.contains("### action `ping`"));
    assert!(docs.contains("## Lowering Audit Report"));
    assert!(docs.contains("### Verifier Obligations"));
}

#[test]
fn cellc_init_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("demo_pkg");

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("init").arg("demo").arg(&root).arg("--lib").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["kind"], "library");
    assert_eq!(summary["package"], "demo");
    assert!(summary["manifest"].as_str().unwrap().ends_with("demo_pkg/Cell.toml"));
    assert_eq!(summary["entry"], "src/lib.cell");
    assert!(root.join("Cell.toml").exists());
    assert!(root.join("src").join("lib.cell").exists());
    assert!(!root.join("src").join("main.cell").exists());

    let manifest: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest["package"]["entry"].as_str(), Some("src/lib.cell"));
}

#[test]
fn cellc_new_subcommand_supports_json_summary_and_vcs_none() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("demo_pkg");

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("new")
        .arg("demo")
        .arg("--path")
        .arg(&root)
        .arg("--lib")
        .arg("--vcs")
        .arg("none")
        .arg("--json")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["command"], "new");
    assert_eq!(summary["kind"], "library");
    assert_eq!(summary["package"], "demo");
    assert_eq!(summary["vcs"], "none");
    assert_eq!(summary["git_initialized"], false);
    assert!(summary["manifest"].as_str().unwrap().ends_with("demo_pkg/Cell.toml"));
    assert_eq!(summary["entry"], "src/lib.cell");
    assert!(root.join("Cell.toml").exists());
    assert!(root.join("src").join("lib.cell").exists());
    assert!(!root.join("src").join("main.cell").exists());
    assert!(!root.join(".git").exists());

    let manifest: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest["package"]["entry"].as_str(), Some("src/lib.cell"));
}

#[test]
fn cellc_new_subcommand_initializes_git_by_default() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("git_pkg");

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).arg("new").arg("git_demo").arg("--path").arg(&root).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["command"], "new");
    assert_eq!(summary["kind"], "binary");
    assert_eq!(summary["package"], "git_demo");
    assert_eq!(summary["vcs"], "git");
    assert_eq!(summary["git_initialized"], true);
    assert_eq!(summary["entry"], "src/main.cell");
    assert!(root.join(".git").exists());
    assert!(root.join("src").join("main.cell").exists());
}

#[test]
fn cellc_interface_defaults_to_compact_human_output_and_keeps_complete_json() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("api.cell");
    std::fs::write(
        &source,
        "module api\npublic struct Pair<T: fixed_value> { left: T, right: T }\npublic fn first<T: fixed_value>(pair: Pair<T>) -> T { pair.left }\n",
    )
    .unwrap();

    let human = cellc_command().args(["interface", source.to_str().unwrap()]).output().unwrap();
    assert!(human.status.success(), "{}", String::from_utf8_lossy(&human.stderr));
    let human = String::from_utf8(human.stdout).unwrap();
    assert!(human.contains("public struct Pair<T: fixed_value>"), "{human}");
    assert!(human.contains("public function first<T: fixed_value>"), "{human}");
    assert!(!human.trim_start().starts_with('{'), "{human}");

    let json = cellc_command().args(["--json", "interface", source.to_str().unwrap()]).output().unwrap();
    assert!(json.status.success(), "{}", String::from_utf8_lossy(&json.stderr));
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(
        json["interface"]["types"][0]["type_parameters"][0]["constraints"],
        serde_json::json!(["copy", "drop", "store", "fixed", "serializable", "non_linear"])
    );
}

#[test]
fn cellc_clean_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("target")).unwrap();
    std::fs::create_dir_all(root.join(".cell").join("cache")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("clean").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["removed"], 2);
    assert_eq!(summary["removed_paths"].as_array().unwrap().len(), 2);
    assert!(!root.join("target").exists());
    assert!(!root.join(".cell").join("cache").exists());
}

#[test]
fn cellc_info_subcommand_supports_json_summary() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
authors = ["Audit Bot"]
description = "demo package"
license = "MIT"
entry = "src/main.cell"

[dependencies]
math = "1"

[policy]
deny_fail_closed = true
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("info").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["manifest"], "Cell.toml");
    assert_eq!(summary["package"]["name"], "demo");
    assert_eq!(summary["package"]["authors"][0], "Audit Bot");
    assert_eq!(summary["dependencies"]["math"], "1");
    assert_eq!(summary["policy"]["deny_fail_closed"], true);
}

#[test]
fn cellc_add_and_remove_subcommands_honor_dev_path_and_json() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("math/src")).unwrap();
    std::fs::create_dir_all(root.join("contracts")).unwrap();
    std::fs::create_dir_all(root.join("shared")).unwrap();
    std::fs::write(root.join("src/main.cell"), "module demo;\n").unwrap();
    std::fs::write(
        root.join("math/Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"math\"\nversion = \"0.1.0\"\nentry = \"src/lib.cell\"\n",
    )
    .unwrap();
    std::fs::write(root.join("math/src/lib.cell"), "module math;\n").unwrap();

    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
entry = "src/main.cell"
source_roots = ["contracts", "shared"]

[build]
target = "riscv64-elf"
target_profile = "ckb"
out_dir = "artifacts"
"#,
    )
    .unwrap();

    let add_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("add")
        .arg("--dev")
        .arg("--path")
        .arg("math")
        .arg("--json")
        .arg("math")
        .output()
        .unwrap();
    assert!(
        add_output.status.success(),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&add_output.stdout),
        String::from_utf8_lossy(&add_output.stderr)
    );

    let add_summary: serde_json::Value = serde_json::from_slice(&add_output.stdout).unwrap();
    assert_eq!(add_summary["status"], "ok");
    assert_eq!(add_summary["target"], "dev-dependencies");
    assert_eq!(add_summary["added"][0], "math");
    assert_eq!(add_summary["dependency"]["path"], "math");

    let manifest: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest["package"]["source_roots"].as_array().unwrap().len(), 2);
    assert_eq!(manifest["build"]["target"].as_str().unwrap(), "riscv64-elf");
    assert_eq!(manifest["build"]["target_profile"].as_str().unwrap(), "ckb");
    assert_eq!(manifest["build"]["out_dir"].as_str().unwrap(), "artifacts");
    assert_eq!(manifest["dev_dependencies"]["math"]["path"].as_str().unwrap(), "math");
    assert!(manifest.get("dependencies").and_then(|value| value.get("math")).is_none());

    let remove_output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("remove")
        .arg("--dev")
        .arg("--json")
        .arg("math")
        .output()
        .unwrap();
    assert!(remove_output.status.success(), "stderr: {}", String::from_utf8_lossy(&remove_output.stderr));

    let remove_summary: serde_json::Value = serde_json::from_slice(&remove_output.stdout).unwrap();
    assert_eq!(remove_summary["status"], "ok");
    assert_eq!(remove_summary["target"], "dev-dependencies");
    assert_eq!(remove_summary["removed"][0], "math");
    assert!(remove_summary["missing"].as_array().unwrap().is_empty());

    let manifest_after: toml::Value = std::fs::read_to_string(root.join("Cell.toml")).unwrap().parse().unwrap();
    assert_eq!(manifest_after["package"]["source_roots"].as_array().unwrap().len(), 2);
    assert_eq!(manifest_after["build"]["target"].as_str().unwrap(), "riscv64-elf");
    assert_eq!(manifest_after["build"]["target_profile"].as_str().unwrap(), "ckb");
    assert_eq!(manifest_after["build"]["out_dir"].as_str().unwrap(), "artifacts");
    assert!(manifest_after.get("dev_dependencies").and_then(|value| value.get("math")).is_none());
}

#[test]
fn cellc_build_uses_authoritative_lock_and_frozen_is_offline_and_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("math/src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[dependencies.math]
path = "math"
version = "^1.2.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        r#"
module demo::main

action ping(value: u64) -> u64 {
    verification
        value
}
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("math/Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "math"
version = "1.2.3"
"#,
    )
    .unwrap();
    std::fs::write(root.join("math/src/lib.cell"), "module math;\n").unwrap();

    let missing = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").output().unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("Cell.lock is missing"));

    let lock = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("lock").arg("--json").output().unwrap();
    assert!(lock.status.success(), "stderr: {}", String::from_utf8_lossy(&lock.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&lock.stdout).unwrap();
    assert_eq!(summary["schema"], cellscript::package::Lockfile::CURRENT_SCHEMA);
    assert_eq!(summary["resolver_model"], cellscript::package::Lockfile::CURRENT_RESOLVER_MODEL);
    assert_eq!(summary["dependency_nodes"], 1);

    let build = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--locked").output().unwrap();
    assert!(build.status.success(), "stderr: {}", String::from_utf8_lossy(&build.stderr));
    let before_frozen = std::fs::read(root.join("Cell.lock")).unwrap();
    let frozen =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--frozen").arg("--offline").output().unwrap();
    assert!(frozen.status.success(), "stderr: {}", String::from_utf8_lossy(&frozen.stderr));
    assert_eq!(std::fs::read(root.join("Cell.lock")).unwrap(), before_frozen);

    std::fs::write(root.join("math/src/lib.cell"), "module math;\n// source drift\n").unwrap();
    let drift = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--locked").output().unwrap();
    assert!(!drift.status.success());
    assert!(String::from_utf8_lossy(&drift.stderr).contains("source hash mismatch"));

    let plan_path = root.join("upgrade-plan.json");
    let update =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("update").arg("--output").arg(&plan_path).output().unwrap();
    assert!(update.status.success(), "stderr: {}", String::from_utf8_lossy(&update.stderr));
    assert_eq!(std::fs::read(root.join("Cell.lock")).unwrap(), before_frozen);
    let apply = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("update")
        .arg("--apply-plan")
        .arg(&plan_path)
        .arg("--acknowledge")
        .arg("UPG3001")
        .output()
        .unwrap();
    assert!(apply.status.success(), "stderr: {}", String::from_utf8_lossy(&apply.stderr));
    let rebuilt = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("build").arg("--locked").output().unwrap();
    assert!(rebuilt.status.success(), "stderr: {}", String::from_utf8_lossy(&rebuilt.stderr));
}

#[test]
fn cellc_explicit_lock_migrates_v3_compiler_requirement_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
edition = "2027"
name = "legacy-lock"
version = "1.0.0"
cellscript_version = ">=0.20, >=0.33.0-alpha.1"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), "module legacy_lock;\n\naction ping() -> u64 {\n    verification\n        1\n}\n")
        .unwrap();
    std::fs::write(
        root.join("Cell.lock"),
        r#"version = 3
schema = "cellscript-lock-v0.24-graph-v1"
resolver_model = "legacy"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "legacy-lock"
version = "1.0.0"

[root]
manifest_digest = "legacy"

[dependencies]
"#,
    )
    .unwrap();

    let frozen = cellc_command().current_dir(root).args(["check", "--frozen", "--offline"]).output().unwrap();
    assert!(!frozen.status.success(), "legacy lock unexpectedly admitted");
    assert!(String::from_utf8_lossy(&frozen.stderr).contains("unsupported Cell.lock version 3"));

    let migrated = cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();
    assert!(migrated.status.success(), "stderr: {}", String::from_utf8_lossy(&migrated.stderr));
    let lockfile: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    assert_eq!(lockfile.version, cellscript::package::Lockfile::CURRENT_VERSION);
    assert_eq!(lockfile.schema, cellscript::package::Lockfile::CURRENT_SCHEMA);
    assert_eq!(lockfile.package.compiler_requirement, ">=0.20, >=0.33.0-alpha.1");
    assert_eq!(lockfile.package.resolver_compiler_version, cellscript::VERSION);

    let frozen = cellc_command().current_dir(root).args(["check", "--frozen", "--offline"]).output().unwrap();
    assert!(frozen.status.success(), "stderr: {}", String::from_utf8_lossy(&frozen.stderr));
}

#[test]
fn cellc_explicit_lock_migrates_v4_to_the_declared_resolver_model() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
edition = "2027"
name = "v4-lock"
version = "1.0.0"
cellscript_version = ">=0.20, >=0.33.0-alpha.1"
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), "module v4_lock;\n\naction ping() -> u64 {\n    verification\n        1\n}\n").unwrap();
    std::fs::write(
        root.join("Cell.lock"),
        format!(
            r#"version = 4
schema = "cellscript-lock-v0.30-compiler-requirement-v1"

[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "v4-lock"
version = "1.0.0"
compiler_requirement = ">=0.20, >=0.33.0-alpha.1"
resolver_compiler_version = "{}"

[root]
manifest_digest = "legacy"

[dependencies]
"#,
            cellscript::VERSION
        ),
    )
    .unwrap();

    let frozen = cellc_command().current_dir(root).args(["check", "--frozen", "--offline"]).output().unwrap();
    assert!(!frozen.status.success(), "v4 lock unexpectedly admitted");
    assert!(String::from_utf8_lossy(&frozen.stderr).contains("unsupported Cell.lock version 4"));

    let migrated = cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();
    assert!(migrated.status.success(), "stderr: {}", String::from_utf8_lossy(&migrated.stderr));
    let report: serde_json::Value = serde_json::from_slice(&migrated.stdout).unwrap();
    assert_eq!(report["resolver_model"], cellscript::package::Lockfile::CURRENT_RESOLVER_MODEL);
    let lockfile: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    assert_eq!(lockfile.version, 5);
    assert_eq!(lockfile.resolver_model, "single-package-coordinate-v1");

    let frozen = cellc_command().current_dir(root).args(["check", "--frozen", "--offline"]).output().unwrap();
    assert!(frozen.status.success(), "stderr: {}", String::from_utf8_lossy(&frozen.stderr));
}

#[test]
fn cellc_expand_uses_the_manifest_edition_and_emits_the_semantic_foundation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "semantic-preview"
version = "0.1.0"
"#,
    )
    .unwrap();
    let source_path = root.join("src").join("main.cell");
    std::fs::write(
        &source_path,
        r#"
module semantic_preview

action main(witness value: u64) -> u64 {
    verification
        return value
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "expand"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let foundation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(foundation["schema"], cellscript_artifact_checker::SEMANTIC_FOUNDATION_SCHEMA);
    assert_eq!(foundation["entry_contract"]["dispatch"]["kind"], "single-entry");
    assert!(foundation["identities"]["core_semantic_id"].as_str().is_some_and(|id| !id.is_empty()));

    std::fs::write(
        source_path,
        r#"
module semantic_preview

action main(value: u64) -> u64 {
    return value
}
"#,
    )
    .unwrap();
    let authoring = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "expand"]).output().unwrap();
    assert!(authoring.status.success(), "stderr: {}", String::from_utf8_lossy(&authoring.stderr));
    let authoring_foundation: serde_json::Value = serde_json::from_slice(&authoring.stdout).unwrap();
    assert_eq!(authoring_foundation["entry_contract"]["dispatch"]["kind"], "single-entry");
    assert_eq!(authoring_foundation["identities"]["core_semantic_id"], foundation["identities"]["core_semantic_id"]);
}

#[test]
fn cellc_expand_compiles_native_edition_2027_type_script_surface() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "native-semantic-preview"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module native_semantic_preview

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

type_script TokenTransfer on type_group<Token> {
    entry transfer(
        input token: Token from group_input[0],
        witness recipient: Address from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        verify {
            enforce token.amount > 0
        }

        replace token -> next {
            data {
                owner = same
                amount = same
            }
            identity = same
            type_script = same
            lock_script = exact_hash(recipient)
            capacity = same
            cardinality = one_to_one
        }
    }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "expand"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let foundation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(foundation["entry_contract"]["trigger"], "type-group<Token>");
    assert_eq!(foundation["entry_contract"]["exact_entry"], "action:transfer");
    assert_eq!(foundation["dispositions"][0]["input"]["kind"], "successor");
    assert_eq!(foundation["dispositions"][0]["envelope"]["completeness"], "exhaustive");
    let enforced = foundation["claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|claim| claim["category"] == "entry-condition")
        .expect("native enforce must emit an executable semantic claim");
    assert_eq!(enforced["statement"], "require token.amount > 0");
    assert_eq!(enforced["enforcement"], "checked-runtime");
    assert_eq!(enforced["on_chain_checked"], true);
    assert_eq!(enforced["evidence_reference"], "typed-entry:action:transfer:block:0:branch-condition");
    assert!(enforced["execution"]["condition_node_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(enforced["execution"]["failure_error_code"], 5);

    let format = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").output().unwrap();
    assert!(format.status.success(), "stderr: {}", String::from_utf8_lossy(&format.stderr));
    let checked = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["fmt", "--check"]).output().unwrap();
    assert!(checked.status.success(), "stderr: {}", String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn cellc_expand_compiles_native_edition_2027_lock_script_surface() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();

    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "native-lock-preview"
version = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module native_lock_preview

resource Vault has store {
    owner: Address,
}

lock_script VaultOwner on lock_group {
    entry unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        verify {
            enforce vault.owner == owner
            enforce claimed_owner == owner
        }
    }
}
"#,
    )
    .unwrap();

    let parsed = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "--parse"]).output().unwrap();
    assert!(parsed.status.success(), "stderr: {}", String::from_utf8_lossy(&parsed.stderr));

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["--json", "expand"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let foundation: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(foundation["entry_contract"]["script_role"], "lock");
    assert_eq!(foundation["entry_contract"]["trigger"], "lock-group");
    assert_eq!(foundation["entry_contract"]["exact_entry"], "lock:unlock");
    assert_eq!(foundation["dispositions"][0]["input"]["kind"], "authorization-only");
    assert_eq!(foundation["dispositions"][0]["input"]["disposition_owner"], "type-script-or-explicit-transaction-policy");
    let enforced =
        foundation["claims"].as_array().unwrap().iter().filter(|claim| claim["category"] == "entry-condition").collect::<Vec<_>>();
    assert_eq!(enforced.len(), 2);
    assert_eq!(enforced[0]["statement"], "require vault.owner == owner");
    assert_eq!(enforced[1]["statement"], "require claimed_owner == owner");
    assert!(enforced.iter().all(|claim| claim["execution"]["failure_error_code"] == 5));

    let format = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").output().unwrap();
    assert!(format.status.success(), "stderr: {}", String::from_utf8_lossy(&format.stderr));
    let checked = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["fmt", "--check"]).output().unwrap();
    assert!(checked.status.success(), "stderr: {}", String::from_utf8_lossy(&checked.stderr));
}

#[test]
fn cellc_multi_phase_dao_rejects_undeclared_state_transition() {
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
    // The flow declares Draft -> Active and Active -> {Executed, Defeated}, but the
    // action tries Draft -> Executed, which is not a declared edge. The static flow
    // membership validator must reject it.
    std::fs::write(
        root.join("src").join("main.cell"),
        r#"
module demo::dao_bad

receipt Proposal has store, create, consume {
    proposal_id: Hash,
    state: u8,
    proposer: Address,
}

flow Proposal.state {
    Draft -> Active;
    Active -> Executed;
    Active -> Defeated;
}

action fast_track(proposal_before: Proposal) -> proposal_after: Proposal {
    transition proposal_before.state: Draft -> proposal_after.state: Executed
    verification
        require proposal_after.proposer == proposal_before.proposer
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("check").output().unwrap();
    assert!(!output.status.success(), "unexpected success: {}", String::from_utf8_lossy(&output.stdout));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("action 'fast_track' transition 'Proposal.state Draft -> Executed' is not declared in the flow"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn cellc_cross_module_launch_composition_distributes_correctly() {
    // The bundled launch example composes token minting with AMM seeding and a 4-way
    // distribution; compile it from the repository root so cross-module stdlib imports
    // (cellscript::fungible_token, cellscript::amm_pool) resolve, then assert the full
    // audit lifecycle and the eight-output distribution shape.
    let fixture = fixtures::examples();
    let launch_path = fixture.path().join("launch");

    let lock_before = std::fs::read(launch_path.join("Cell.lock")).expect("bundled launch package must carry a tracked lockfile");
    let build = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(&launch_path)
        .arg("build")
        .arg("--frozen")
        .arg("--offline")
        .output()
        .unwrap();
    assert!(build.status.success(), "build failed: {}", String::from_utf8_lossy(&build.stderr));

    let metadata: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(launch_path.join("build").join("main.s.meta.json")).unwrap()).unwrap();
    // Clear only this test's copied output before the audit path runs.
    let _ = std::fs::remove_dir_all(launch_path.join("build"));
    // The launch_token action must expose eight create outputs (auth + 4 distributions +
    // pool + lp_receipt + change).
    let launch_action =
        metadata["actions"].as_array().unwrap().iter().find(|action| action["name"] == "launch_token").expect("launch_token action");
    assert_eq!(
        launch_action["create_set"].as_array().map(|create_set| create_set.len()).unwrap_or(0),
        8,
        "expected launch_token with eight create outputs: {launch_action}"
    );

    let audit_dir = tempfile::tempdir().unwrap();
    let audit = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .arg("audit-bundle")
        .arg(&launch_path)
        .args(["--output"])
        .arg(audit_dir.path())
        .arg("--json")
        .output()
        .unwrap();
    assert!(audit.status.success(), "{}", String::from_utf8_lossy(&audit.stderr));
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(audit_dir.path().join("audit-bundle.json")).unwrap()).unwrap();
    assert_eq!(bundle["protocol_graph"]["schema"], "cellscript-protocol-graph-v0.22");
    assert_eq!(
        std::fs::read(launch_path.join("Cell.lock")).unwrap(),
        lock_before,
        "frozen/offline build and audit must not rewrite the tracked dependency graph"
    );
}

#[test]
fn cellc_scheduler_plan_consumes_shared_touch_hints() {
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

shared Ledger has store {
    balance: u64,
}

action credit(ledger_before: Ledger, delta: u64) -> ledger_after: Ledger {
    verification
        require ledger_after.balance == ledger_before.balance + delta

}
action debit(ledger_before: Ledger, delta: u64) -> ledger_after: Ledger {
    verification
        require ledger_after.balance == ledger_before.balance - delta

}
action read_only(value: u64) -> u64 {
    verification
        return value
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("scheduler-plan")
        .arg("--target-profile")
        .arg("ckb")
        .output()
        .unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["policy"], "cellscript-scheduler-hints-v1");
    assert_eq!(stdout["action_count"], 3);
    assert_eq!(stdout["conflict_count"], 1);
    assert_eq!(stdout["conflicts"][0]["left"], "credit");
    assert_eq!(stdout["conflicts"][0]["right"], "debit");
    assert_eq!(stdout["conflicts"][0]["policy"], "must-not-run-in-parallel");
    assert_eq!(stdout["serial_required_actions"][0], "credit");
    assert_eq!(stdout["serial_required_actions"][1], "debit");
    assert!(stdout["estimated_cycles"]["total"].as_u64().unwrap() > 0);
    let read_only = stdout["actions"].as_array().unwrap().iter().find(|action| action["action"] == "read_only").unwrap();
    assert_eq!(read_only["admission"], "parallel-candidate");
}

#[test]
fn cellc_ckb_hash_emits_default_blake2b_vector() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("ckb-hash").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["algorithm"], "blake2b-256");
    assert_eq!(stdout["personalization"], "ckb-default-hash");
    assert_eq!(stdout["input_bytes"], 0);
    assert_eq!(stdout["hash"], "44f4c69744d5f8c55d642062949dcae49bc4e7ef43d388c5a12f42b5633d163e");

    let text = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("ckb-hash").arg("--hex").arg("00").output().unwrap();
    assert!(text.status.success(), "stderr: {}", String::from_utf8_lossy(&text.stderr));
    assert_eq!(String::from_utf8_lossy(&text.stdout).trim().len(), 64);
}

#[test]
fn cellc_ckb_std_compat_reports_runtime_boundary() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("ckb-std-compat").arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["schema"], "cellscript-ckb-std-compat-report-v0.19");
    assert_eq!(report["runtime_policy"], "inline");
    assert_eq!(report["compiler_core_dependency"], "no-ckb-std");
    assert_eq!(report["test_evidence"]["compat_tests"], "tests/ckb_std_compat.rs");
    assert_eq!(report["test_evidence"]["packed_transaction_materialization"], true);
    assert_eq!(report["test_evidence"]["script_construction_api"], true);
    assert_eq!(report["ckb_std_refs"]["type_id"], "ckb_std::type_id");
    assert_eq!(report["inline_abi"]["fields"]["cell_occupied_capacity"], 6);
    assert_eq!(report["witness_args_policy"]["entry_payload_abi"], "cellscript-entry-witness-v1");
    assert_eq!(report["witness_args_policy"]["placement_abi"], "cellscript-witnessargs-input-type-v2");
    assert_eq!(report["witness_args_policy"]["default_action_payload_field"], "input_type");
    assert_eq!(report["witness_args_policy"]["runtime_source"], "group-input-0-then-group-output-0");
    assert_eq!(report["witness_args_policy"]["raw_v1_compatible"], false);
    assert_eq!(report["witness_args_policy"]["final_witness_args_owner"], "adapter");
    assert_eq!(report["witness_args_policy"]["lock_signature_policy"], "explicit-adapter-owned-do-not-overwrite");
    assert_eq!(report["adapter_boundary"]["transaction_realizer"], "ckb-sdk-rust-or-CCC-adapter");
    assert_eq!(report["adapter_boundary"]["compiler_core_uses_ckb_sdk_rust"], false);
    assert_eq!(report["adapter_boundary"]["script_construction"]["packed_type"], "ckb_types::packed::Script");
    assert_eq!(report["adapter_boundary"]["script_construction"]["evidence_schema"], "cellscript-ckb-script-evidence-v0.19");
    assert!(report["adapter_boundary"]["script_construction"]["supports"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "script_ref_readback")));
    assert!(report["adapter_boundary"]["script_construction"]["supports"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "explicit_cell_dep_binding")));
    assert!(report["non_goals"].as_array().is_some_and(|items| items.iter().any(|item| item == "does-not-execute-ckb-vm")));
}

#[test]
fn cellc_rejects_removed_optimization_controls() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("strategy.cell");
    std::fs::write(&source, "module strategy\naction inspect() -> u64 { verification return 20 }\n").unwrap();
    for argument in ["-O0", "-O1", "-O2", "-O3", "--opt=3", "--opt-level=3", "--optimization-level=3", "opt-report"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg(argument).arg(&source).output().unwrap();
        assert!(!output.status.success(), "removed control accepted: {argument}");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(!help.contains("--opt"));
    assert!(!help.contains("opt-report"));
}

#[test]
fn cellc_release_and_normal_builds_use_the_same_compilation_strategy() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    cellscript::package::PackageManager::new(root).init("one-strategy").unwrap();
    std::fs::write(root.join("src/main.cell"),
        "module one_strategy\nfn add(value: u64) -> u64 { return value + 1 }\naction inspect(witness value: u64) -> u64 { verification require add(value) == 5 return 0 }\n").unwrap();
    lock_package(root);
    let mut artifacts = Vec::new();
    for release in [false, true] {
        let mut command = cellc_command();
        command.current_dir(root).args(["build", "--target", "riscv64-elf", "--json"]);
        if release {
            command.arg("--release");
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(report["compilation_policy"], cellscript::COMPILATION_POLICY);
        assert!(report.get("opt_level").is_none());
        artifacts.push(std::fs::read(root.join(report["artifact"].as_str().unwrap())).unwrap());
    }
    assert_eq!(artifacts[0], artifacts[1]);
}

#[test]
fn cellc_fmt_subcommand_formats_sources() {
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
    let source_path = root.join("src").join("main.cell");
    std::fs::write(&source_path, "module demo::main\naction ping(x:u64)->u64{\nverification\nx\n}\n").unwrap();

    let dirty_check =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").arg("--check").arg("--json").output().unwrap();
    assert!(!dirty_check.status.success(), "unexpected success: {}", String::from_utf8_lossy(&dirty_check.stdout));
    let stdout: serde_json::Value = serde_json::from_slice(&dirty_check.stdout).unwrap();
    assert_eq!(stdout["status"], "failed");
    assert_eq!(stdout["mode"], "check");
    assert_eq!(stdout["changed"], 1);
    assert!(stdout["changed_files"][0].as_str().unwrap().ends_with("src/main.cell"));

    let status = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").status().unwrap();
    assert!(status.success());

    let formatted = std::fs::read_to_string(&source_path).unwrap();
    assert!(formatted.contains("action ping(x: u64) -> u64 {\n    verification"));

    let check = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("fmt").arg("--check").arg("--json").output().unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let stdout: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(stdout["status"], "ok");
    assert_eq!(stdout["mode"], "check");
    assert_eq!(stdout["changed"], 0);
}

#[test]
fn inspection_fixtures_use_the_current_schemas_and_reject_retired_build_selection() {
    let graph: cellscript::package::inspection::ResolveGraph =
        serde_json::from_str(include_str!("../fixtures/resolve_graph_v1.json")).unwrap();
    let plan: cellscript::package::inspection::BuildPlan =
        serde_json::from_str(include_str!("../fixtures/build_plan_v2.json")).unwrap();
    let upgrade: cellscript::package::upgrade::UpgradePlan =
        serde_json::from_str(include_str!("../fixtures/upgrade_plan_v1.json")).unwrap();
    assert_eq!(graph.schema, cellscript::package::inspection::RESOLVE_GRAPH_SCHEMA);
    assert_eq!(plan.schema, cellscript::package::inspection::BUILD_PLAN_SCHEMA);
    assert_eq!(plan.schema_version, 2);
    assert!(
        serde_json::from_str::<cellscript::package::inspection::BuildPlan>(include_str!("../fixtures/build_plan_v1.json")).is_err()
    );
    assert_eq!(upgrade.schema, cellscript::package::upgrade::UPGRADE_PLAN_SCHEMA);
}

#[test]
fn cellc_update_plan_is_deterministic_read_only_and_applies_only_explicitly() {
    let dir = tempfile::tempdir().unwrap();
    let package = write_workspace_member(dir.path(), "app", "app", &[]);
    lock_package(&package);
    let lock_before = std::fs::read(package.join("Cell.lock")).unwrap();
    let plan_path = package.join("upgrade-plan.json");

    let protected_output =
        cellc_command().current_dir(&package).args(["update-plan", "--offline", "--output", "Cell.lock", "--json"]).output().unwrap();
    assert!(!protected_output.status.success());
    let protected_report: serde_json::Value = serde_json::from_slice(&protected_output.stdout).unwrap();
    assert_eq!(protected_report["diagnostics"][0]["code"], "E2800");
    assert_eq!(std::fs::read(package.join("Cell.lock")).unwrap(), lock_before);

    let plan_once = cellc_command()
        .current_dir(&package)
        .args(["update-plan", "--offline", "--output", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(plan_once.status.success(), "stdout: {}", String::from_utf8_lossy(&plan_once.stdout));
    let first_bytes = std::fs::read(&plan_path).unwrap();
    assert_eq!(std::fs::read(package.join("Cell.lock")).unwrap(), lock_before);
    assert!(!package.join(".cell").exists());

    let plan_twice = cellc_command()
        .current_dir(&package)
        .args(["update", "--offline", "--output", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(plan_twice.status.success(), "stdout: {}", String::from_utf8_lossy(&plan_twice.stdout));
    assert_eq!(std::fs::read(&plan_path).unwrap(), first_bytes);
    assert_eq!(std::fs::read(package.join("Cell.lock")).unwrap(), lock_before);
    assert!(!package.join(".cell").exists());

    let plan: serde_json::Value = serde_json::from_slice(&first_bytes).unwrap();
    assert_eq!(plan["schema"], "cellscript-upgrade-plan-v1");
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["resolve_graph_schema"], "cellscript-resolve-graph-v1");
    assert_eq!(plan["build_plan_schema"], "cellscript-build-plan-v2");
    assert_eq!(plan["apply_status"], "ready");
    assert_eq!(plan["reverse_dependents"][0]["build_unit_schema"], "cellscript-build-plan-v2");
    assert!(plan["reverse_dependents"][0]["old_build_unit_id"].as_str().unwrap().starts_with("build-unit:"));
    assert!(plan["reverse_dependents"][0]["new_build_unit_id"].as_str().unwrap().starts_with("build-unit:"));
    assert_eq!(plan["reverse_dependents"][0]["old_build_unit"]["id"], plan["reverse_dependents"][0]["old_build_unit_id"]);
    assert_eq!(plan["reverse_dependents"][0]["new_build_unit"]["id"], plan["reverse_dependents"][0]["new_build_unit_id"]);
    assert!(plan["reverse_dependents"][0]["old_protocol_bundle_input_hash"].as_str().unwrap().starts_with("sha256:"));
    assert!(plan["reverse_dependents"][0]["new_protocol_bundle_input_hash"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(
        plan["policy"].as_array().unwrap().iter().map(|dimension| dimension["dimension"].as_str().unwrap()).collect::<Vec<_>>(),
        [
            "source_semver",
            "source_api",
            "serialized_layout",
            "runtime_abi",
            "effects_capabilities",
            "builder",
            "deployment",
            "upgrade_authorization"
        ]
    );
    assert_eq!(plan["mutates_deployed_manifest"], false);
    assert_eq!(plan["performs_deployment"], false);

    let apply = cellc_command()
        .current_dir(&package)
        .args(["update", "--apply-plan", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(apply.status.success(), "stdout: {}", String::from_utf8_lossy(&apply.stdout));
    let applied: serde_json::Value = serde_json::from_slice(&apply.stdout).unwrap();
    assert_eq!(applied["status"], "applied");
    assert_eq!(std::fs::read_to_string(package.join("Cell.lock")).unwrap(), plan["locks"][0]["new_lock_content"]);
    assert!(!package.join("Deployed.toml").exists());
}

#[test]
fn cellc_update_apply_rejects_stale_and_hash_tampered_plans() {
    let dir = tempfile::tempdir().unwrap();
    let package = write_workspace_member(dir.path(), "app", "app", &[]);
    lock_package(&package);
    let plan_path = package.join("upgrade-plan.json");
    let generated = cellc_command()
        .current_dir(&package)
        .args(["update-plan", "--offline", "--output", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(generated.status.success(), "stdout: {}", String::from_utf8_lossy(&generated.stdout));

    let mut current = std::fs::read_to_string(package.join("Cell.lock")).unwrap();
    current.push('\n');
    std::fs::write(package.join("Cell.lock"), &current).unwrap();
    let stale = cellc_command()
        .current_dir(&package)
        .args(["update", "--apply-plan", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!stale.status.success());
    let stale_report: serde_json::Value = serde_json::from_slice(&stale.stdout).unwrap();
    assert_eq!(stale_report["diagnostics"][0]["code"], "E2802");
    assert!(stale_report["diagnostics"][0]["message"].as_str().unwrap().contains("stale upgrade plan"));
    assert_eq!(std::fs::read_to_string(package.join("Cell.lock")).unwrap(), current);

    let mut tampered: serde_json::Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    tampered["new_graph_hash"] = serde_json::Value::String("sha256:tampered".to_string());
    std::fs::write(&plan_path, serde_json::to_vec_pretty(&tampered).unwrap()).unwrap();
    let invalid = cellc_command()
        .current_dir(&package)
        .args(["update", "--apply-plan", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let invalid_report: serde_json::Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(invalid_report["diagnostics"][0]["code"], "E2802");
    assert!(invalid_report["diagnostics"][0]["message"].as_str().unwrap().contains("plan hash mismatch"));
}

#[test]
fn cellc_package_scoped_update_preserves_unrelated_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let left = write_workspace_member(root, "left", "left", &[]);
    let right = write_workspace_member(root, "right", "right", &[]);
    let app = write_workspace_member(root, "app", "app", &[("left", "../left"), ("right", "../right")]);
    lock_package(&app);
    let old_lock_bytes = std::fs::read(app.join("Cell.lock")).unwrap();
    let old: cellscript::package::Lockfile = toml::from_str(std::str::from_utf8(&old_lock_bytes).unwrap()).unwrap();
    let old_right = old
        .dependencies
        .iter()
        .find(|(_, dependency)| dependency.name == "right")
        .map(|(node, dependency)| (node.clone(), serde_json::to_value(dependency).unwrap()))
        .unwrap();

    let left_manifest = std::fs::read_to_string(left.join("Cell.toml")).unwrap().replace("version = \"1.0.0\"", "version = \"1.1.0\"");
    std::fs::write(left.join("Cell.toml"), left_manifest).unwrap();
    let plan_path = app.join("left-upgrade.json");
    let output = cellc_command()
        .current_dir(&app)
        .args(["update-plan", "--package", "left", "--offline", "--output", plan_path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "stdout: {}", String::from_utf8_lossy(&output.stdout));
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let changes = plan["locks"][0]["node_changes"].as_array().unwrap();
    assert!(changes.iter().any(|change| change["coordinate"] == "left" && change["classification"] == "upgraded"));
    assert!(!changes.iter().any(|change| change["coordinate"] == "right"));
    let new_lock: cellscript::package::Lockfile = serde_json::from_value(plan["locks"][0]["new_lock"].clone()).unwrap();
    let new_right = new_lock.dependencies.get(&old_right.0).unwrap();
    assert_eq!(serde_json::to_value(new_right).unwrap(), old_right.1);
    assert_eq!(std::fs::read(app.join("Cell.lock")).unwrap(), old_lock_bytes);

    let unacknowledged =
        cellc_command().current_dir(&app).args(["update", "--apply-plan", plan_path.to_str().unwrap(), "--json"]).output().unwrap();
    assert!(!unacknowledged.status.success());
    let report: serde_json::Value = serde_json::from_slice(&unacknowledged.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["code"], "E2801");
    assert_eq!(std::fs::read(app.join("Cell.lock")).unwrap(), old_lock_bytes);

    let applied = cellc_command()
        .current_dir(&app)
        .args(["update", "--apply-plan", plan_path.to_str().unwrap(), "--acknowledge", "UPG3001", "--json"])
        .output()
        .unwrap();
    assert!(applied.status.success(), "stdout: {}", String::from_utf8_lossy(&applied.stdout));
    assert_eq!(std::fs::read_to_string(app.join("Cell.lock")).unwrap(), plan["locks"][0]["new_lock_content"]);
    assert!(right.join("Cell.toml").exists());
}

#[test]
fn cellc_add_rejects_self_name_dependency() {
    // `cellc add` (manifest-mutating, distinct from `cellc install`) shares the
    // same self-dep hazard and must also be fail-closed.
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

[dependencies]
"#,
    )
    .unwrap();

    let add = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .current_dir(root)
        .arg("add")
        .arg("demo")
        .arg("--path")
        .arg("./src")
        .output()
        .unwrap();

    assert!(!add.status.success(), "self name add must be rejected");
    let stderr = String::from_utf8_lossy(&add.stderr);
    assert!(stderr.contains("refusing to add self-dependency"), "expected self-dep refusal, got: {stderr}");
}

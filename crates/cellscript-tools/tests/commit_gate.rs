#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let fixture = Self(std::env::temp_dir().join(format!("cellscript-commit-gate-{}-{nonce}", std::process::id())));
        fixture.write(".gitignore", "target/\nfake-bin/\ncargo.log\n");
        fixture.write("Cargo.toml", "[package]\nname = 'fixture'\n");
        fixture.write("README.md", "# Fixture\n");
        fixture.write("src/lib.rs", "pub fn baseline() {}\n");
        fixture.write("crates/member/Cargo.toml", "[package]\nname = 'member'\n");
        fixture.write("crates/member/src/lib.rs", "pub fn baseline() {}\n");
        fixture.write(
            "scripts/cellscript_gate.sh",
            &fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/cellscript_gate.sh")).unwrap(),
        );
        fixture.write(
            "fake-bin/cargo",
            "#!/usr/bin/env bash\nprintf '%s\\n' \"$*\" >>\"$GATE_TEST_LOG\"\nif [[ ${GATE_FAIL_CHECK:-0} == 1 && $1 == check ]]; then exit 17; fi\n",
        );
        fs::set_permissions(fixture.0.join("fake-bin/cargo"), fs::Permissions::from_mode(0o755)).unwrap();
        fixture.git(&["init", "-q"]);
        fixture.git(&["add", "."]);
        fixture.git(&[
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.name=Gate Test",
            "-c",
            "user.email=gate@example.invalid",
            "commit",
            "-qm",
            "baseline",
        ]);
        fixture
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn git(&self, args: &[&str]) {
        let output = Command::new("git").current_dir(&self.0).args(args).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    fn gate(&self, args: &[&str], fail_check: bool) -> Output {
        let mut paths = vec![self.0.join("fake-bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
        Command::new("bash")
            .arg(self.0.join("scripts/cellscript_gate.sh"))
            .args(args)
            .current_dir(&self.0)
            .env("PATH", std::env::join_paths(paths).unwrap())
            .env("GATE_TEST_LOG", self.0.join("cargo.log"))
            .env("GATE_FAIL_CHECK", if fail_check { "1" } else { "0" })
            // Keep report directory setup inside this fixture even inside CI.
            .env("CELLSCRIPT_BACKEND_SHAPE_REPORT", self.0.join("target/shape.json"))
            .env("CELLSCRIPT_MOLECULE_SCHEMA_MANIFEST_REPORT", self.0.join("target/schema.json"))
            .env("CELLSCRIPT_COST_CORPUS_REPORT", self.0.join("target/cost.json"))
            .env("CELLSCRIPT_MULTI_SCRIPT_COST_REPORT", self.0.join("target/multi.json"))
            .output()
            .unwrap()
    }

    fn calls(&self) -> String {
        fs::read_to_string(self.0.join("cargo.log")).unwrap_or_default()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn default_documentation_gate_has_no_build_test_or_vm_work() {
    let fixture = Fixture::new();
    fixture.write("README.md", "# Updated documentation\n");
    fixture.write("docs/new file.md", "# Untracked document\n");
    let output = fixture.gate(&[], false);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fixture.calls();
    assert_eq!(calls.lines().count(), 3, "{calls}");
    for check in ["check-source-policy", "check-doc-status", "check-markdown-links"] {
        assert!(calls.contains(check), "{calls}");
    }
    assert!(String::from_utf8_lossy(&output.stdout).contains("CellScript commit gate passed."));
}

#[test]
fn rust_checks_include_staged_changes_reverted_in_the_worktree_and_untracked_members() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "pub fn staged_change() {}\n");
    fixture.git(&["add", "src/lib.rs"]);
    fixture.write("src/lib.rs", "pub fn baseline() {}\n");
    fixture.write("crates/member/src/new file.rs", "pub fn new_file() {}\n");
    let output = fixture.gate(&["commit"], false);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fixture.calls();
    for manifest in ["Cargo.toml", "crates/member/Cargo.toml"] {
        let expected = format!("check --locked --manifest-path {manifest} --all-targets");
        assert_eq!(calls.lines().filter(|line| *line == expected).count(), 1, "{calls}");
    }
    assert!(!calls.lines().any(|line| line.starts_with("test ") || line.starts_with("build ")));
}

#[test]
fn shared_lock_changes_check_workspace_once_and_preserve_wasm_feature() {
    let fixture = Fixture::new();
    fixture.write("Cargo.lock", "# Changed workspace lock\n");
    fixture.write("rust-toolchain.toml", "[toolchain]\nchannel = '1.97.1'\n");
    let output = fixture.gate(&["commit"], false);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let calls = fixture.calls();
    assert_eq!(calls.lines().filter(|line| line.starts_with("check ")).count(), 7, "{calls}");
    assert!(calls.contains("check --locked --manifest-path crates/cellscript-wasm/Cargo.toml --all-targets --features wasm"));
}

#[test]
fn staged_whitespace_fails_even_when_worktree_is_fixed() {
    let fixture = Fixture::new();
    fixture.write("README.md", "Bad staged whitespace \n");
    fixture.git(&["add", "README.md"]);
    fixture.write("README.md", "Fixed worktree\n");
    let output = fixture.gate(&["commit"], false);
    assert!(!output.status.success());
    assert!(fixture.calls().is_empty());
}

#[test]
fn malformed_shell_and_host_check_failures_are_not_suppressed() {
    let fixture = Fixture::new();
    fixture.write("scripts/broken file.sh", "if then\n");
    assert!(!fixture.gate(&["commit"], false).status.success());
    fs::remove_file(fixture.0.join("scripts/broken file.sh")).unwrap();
    fixture.write("src/lib.rs", "pub fn changed() {}\n");
    assert_eq!(fixture.gate(&["commit"], true).status.code(), Some(17));
}

#[test]
fn commit_gate_rejects_unknown_arguments() {
    let fixture = Fixture::new();
    assert_eq!(fixture.gate(&["commit", "--skip-checks"], false).status.code(), Some(2));
    assert!(fixture.calls().is_empty());
}

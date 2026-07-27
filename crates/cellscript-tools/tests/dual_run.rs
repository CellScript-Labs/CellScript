use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("CellScript repository root must exist")
}

fn run(root: &Path, program: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", program.display()))
}

fn assert_matches_python_at(root: &Path, python_script: &str, rust_subcommand: &str) {
    let python = run(root, Path::new("python3"), &[python_script]);
    let rust = run(
        root,
        Path::new(env!("CARGO_BIN_EXE_cellscript-tools")),
        &["--root", root.to_str().expect("UTF-8 repository path"), rust_subcommand],
    );

    assert_eq!(
        rust.status.code(),
        python.status.code(),
        "exit code mismatch\npython stderr:\n{}\nrust stderr:\n{}",
        String::from_utf8_lossy(&python.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stdout,
        python.stdout,
        "stdout mismatch\npython stderr:\n{}\nrust stderr:\n{}",
        String::from_utf8_lossy(&python.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
}

fn assert_matches_python(python_script: &str, rust_subcommand: &str) {
    assert_matches_python_at(&repo_root(), python_script, rust_subcommand);
}

struct TestRepo {
    path: PathBuf,
}

impl TestRepo {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system clock must follow Unix epoch").as_nanos();
        let path = std::env::temp_dir().join(format!("cellscript-tools-test-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).expect("test repository root must be creatable");
        Self { path }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file must have a parent")).expect("fixture parent must be creatable");
        fs::write(path, contents).expect("fixture file must be writable");
    }

    fn copy_from_repo(&self, relative: &str) {
        let destination = self.path.join(relative);
        fs::create_dir_all(destination.parent().expect("fixture file must have a parent")).expect("fixture parent must be creatable");
        fs::copy(repo_root().join(relative), destination).expect("fixture file must be copied");
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let expected_parent = std::env::temp_dir();
        let safe_name =
            self.path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("cellscript-tools-test-"));
        if self.path.parent() == Some(expected_parent.as_path()) && safe_name {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

const EXPECTED_SKILLS: &[&str] = &[
    "cellscript-builder-deployment",
    "cellscript-ckb-model",
    "cellscript-diagnostics",
    "cellscript-language-basics",
    "cellscript-metadata-audit",
    "cellscript-package-cli",
];

fn skill_document(name: &str) -> String {
    format!("---\nname: {name}\nreferences:\n  - docs/wiki/Current.md\ncommands:\n  - cellc check\n---\n# {name}\n")
}

fn skill_pack_fixture() -> TestRepo {
    let fixture = TestRepo::new("skill-pack");
    fixture.copy_from_repo("scripts/check_cellscript_skill_pack.py");
    fixture.write("src/cli/commands.rs", "ClapCommand::new(\"check\")\n");
    fixture.write("docs/wiki/Current.md", "# Current\n");
    for skill in EXPECTED_SKILLS {
        fixture.write(&format!("docs/skills/{skill}/SKILL.md"), &skill_document(skill));
    }
    fixture
}

#[test]
fn skill_pack_output_and_exit_code_match_python() {
    assert_matches_python("scripts/check_cellscript_skill_pack.py", "check-skill-pack");
}

#[test]
fn tooling_release_output_and_exit_code_match_python() {
    assert_matches_python("scripts/validate_cellscript_tooling_release.py", "validate-tooling-release");
}

#[test]
fn skill_pack_failure_and_encoding_paths_match_python() {
    let fixture = skill_pack_fixture();
    let script = "scripts/check_cellscript_skill_pack.py";
    assert_matches_python_at(&fixture.path, script, "check-skill-pack");

    let first = EXPECTED_SKILLS[0];
    fixture.write(
        &format!("docs/skills/{first}/SKILL.md"),
        &skill_document(first).replace("references:\n  - docs/wiki/Current.md", "references: docs/wiki/Current.md"),
    );
    assert_matches_python_at(&fixture.path, script, "check-skill-pack");

    fixture.write(&format!("docs/skills/{first}/SKILL.md"), &skill_document(first));
    fixture.write("docs/skills/cellscript-雪/SKILL.md", &skill_document("cellscript-雪"));
    assert_matches_python_at(&fixture.path, script, "check-skill-pack");

    fixture.write(&format!("docs/skills/{first}/SKILL.md"), "name: malformed\n");
    assert_matches_python_at(&fixture.path, script, "check-skill-pack");
}

#[cfg(unix)]
fn tooling_release_fixture() -> TestRepo {
    use std::os::unix::fs::symlink;

    let source_root = repo_root();
    let fixture = TestRepo::new("tooling-release");
    for entry in fs::read_dir(&source_root).expect("repository root must be readable") {
        let entry = entry.expect("repository entry must be readable");
        if entry.file_name() == "scripts" {
            continue;
        }
        symlink(entry.path(), fixture.path.join(entry.file_name())).expect("fixture symlink must be creatable");
    }
    fixture.copy_from_repo("scripts/validate_cellscript_tooling_release.py");
    for script in ["cellscript_gate.sh", "cellscript_ckb_release_gate.sh", "ckb_cellscript_acceptance.sh"] {
        symlink(source_root.join("scripts").join(script), fixture.path.join("scripts").join(script))
            .expect("script fixture symlink must be creatable");
    }
    fixture
}

#[test]
#[cfg(unix)]
fn tooling_release_python_bytecode_failure_paths_match_python() {
    use std::os::unix::fs::symlink;

    let fixture = tooling_release_fixture();
    let script = "scripts/validate_cellscript_tooling_release.py";
    assert_matches_python_at(&fixture.path, script, "validate-tooling-release");

    let fixture_gitignore = fixture.path.join(".gitignore");
    fs::remove_file(&fixture_gitignore).expect("fixture .gitignore symlink must be removable");
    let gitignore =
        fs::read_to_string(repo_root().join(".gitignore")).expect("repository .gitignore must be readable").replace("*.py[cod]\n", "");
    fs::write(&fixture_gitignore, gitignore).expect("fixture .gitignore must be writable");
    assert_matches_python_at(&fixture.path, script, "validate-tooling-release");

    fs::remove_file(&fixture_gitignore).expect("fixture .gitignore must be removable");
    symlink(repo_root().join(".gitignore"), &fixture_gitignore).expect("fixture .gitignore symlink must be restorable");

    let fixture_manifest = fixture.path.join("Cargo.toml");
    fs::remove_file(&fixture_manifest).expect("fixture Cargo.toml symlink must be removable");
    let manifest = fs::read_to_string(repo_root().join("Cargo.toml"))
        .expect("repository Cargo.toml must be readable")
        .replace("    \"scripts/__pycache__/\",\n", "");
    fs::write(&fixture_manifest, manifest).expect("fixture Cargo.toml must be writable");
    assert_matches_python_at(&fixture.path, script, "validate-tooling-release");
}

use std::{path::PathBuf, process::Command};

mod common;

#[test]
fn syntax_combo_quick_matrix_is_cargo_test_visible() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("bash")
        .arg("scripts/cellscript_syntax_combo_audit.sh")
        .arg("quick")
        .current_dir(&manifest_dir)
        .env("CELLC_BIN", common::cellc_bin())
        .output()
        .expect("syntax combo quick runner should start");

    assert!(
        output.status.success(),
        "syntax combo quick runner failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

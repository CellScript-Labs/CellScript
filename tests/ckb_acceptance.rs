#![cfg(feature = "ckb-acceptance")]

use std::{path::PathBuf, process::Command};

#[test]
fn ckb_acceptance_script_is_cargo_test_visible_when_feature_enabled() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let ckb_repo = std::env::var("CKB_REPO").expect("CKB_REPO must be set when running --features ckb-acceptance");
    let output = Command::new("bash")
        .arg("scripts/ckb_cellscript_acceptance.sh")
        .arg("--production")
        .arg("--stateful-scenarios")
        .current_dir(&manifest_dir)
        .env("CKB_REPO", ckb_repo)
        .output()
        .expect("CKB acceptance runner should start");

    assert!(
        output.status.success(),
        "CKB acceptance runner failed\nstatus: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

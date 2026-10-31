//! Declared source hashes and independently checked nominal compatibility are distinct.
#![cfg(feature = "cli")]
mod common;

use camino::Utf8Path;
use cellscript::{CompileOptions, ExecutableSurfacePolicy};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Output,
};

fn package(root: &Path, name: &str, source: &str, dependencies: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n{dependencies}")).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
}

fn bundle(root: &Path, alias: &str, owner: &str) -> (PathBuf, String) {
    let source = format!("module client\nuse peer::{owner} as {alias}\npublic struct Wrapper has copy, drop, store, fixed, serializable, non_linear {{ value: {alias} }}\npublic action verify(witness item: Wrapper) {{ verification require item.value.amount > 0 }}\n");
    package(root, "client", &source, "[dependencies]\npeer = { path = \"../peer\" }\n");
    let output = common::cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stdout));
    let compiled = cellscript::compile_path_with_executable_surface_policy(
        Utf8Path::from_path(root).unwrap(),
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
        None,
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let path = root.join("out/main.elf");
    let artifact = Utf8Path::from_path(&path).unwrap();
    compiled.write_to_path(artifact).unwrap();
    compiled.write_metadata_to_path(&compiled.default_metadata_path(artifact)).unwrap();
    compiled.write_verified_artifact_sidecars(artifact).unwrap().unwrap();
    (path, compiled.metadata.interface_hash)
}

fn diff(old: &Path, new: &Path) -> Output {
    common::cellc_command()
        .args(["interface-diff", "--checked", "--old"])
        .arg(old)
        .arg("--new")
        .arg(new)
        .arg("--json")
        .output()
        .unwrap()
}

fn first(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("expected one complete JSON result: {error}: {}", String::from_utf8_lossy(&output.stdout)))
}

#[test]
fn checked_diff_accepts_alias_renames_but_rejects_a_distinct_same_width_owner() {
    let directory = tempfile::tempdir().unwrap();
    package(&directory.path().join("peer"), "peer", "module peer\npublic struct Payload has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\npublic struct Other has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\n", "");
    let (left, left_hash) = bundle(&directory.path().join("left"), "Left", "Payload");
    let (right, right_hash) = bundle(&directory.path().join("right"), "Right", "Payload");
    let (other, _) = bundle(&directory.path().join("other"), "Right", "Other");
    assert_ne!(left_hash, right_hash, "the existing declaration digest is preserved");
    assert_eq!(std::fs::read(&left).unwrap(), std::fs::read(&right).unwrap());
    let accepted = diff(&left, &right);
    assert!(accepted.status.success(), "{}", String::from_utf8_lossy(&accepted.stdout));
    let report = first(&accepted);
    assert_eq!(report["schema"], "cellscript-checked-interface-compatibility-v1");
    assert_eq!(report["compatible"], true);
    assert_eq!(report["required_contract_hash"], report["candidate_contract_hash"]);
    let rejected = diff(&left, &other);
    assert!(!rejected.status.success());
    assert_eq!(first(&rejected)["compatible"], false);
    assert_eq!(first(&rejected)["status"], "failed");
    assert_eq!(first(&rejected)["diagnostics"][0]["code"], "E2501");

    let globally_json = common::cellc_command()
        .args(["--message-format=json", "interface-diff", "--checked", "--old"])
        .arg(&left)
        .arg("--new")
        .arg(&other)
        .output()
        .unwrap();
    assert!(!globally_json.status.success());
    assert_eq!(first(&globally_json)["compatible"], false);
    for (name, artifact) in [("old", &left), ("new", &other)] {
        let metadata: Value = serde_json::from_slice(
            &std::fs::read(cellscript::default_metadata_path_for_artifact(Utf8Path::from_path(artifact).unwrap())).unwrap(),
        )
        .unwrap();
        std::fs::write(
            directory.path().join(format!("{name}.json")),
            serde_json::to_vec(
                &serde_json::json!({"interface":metadata["public_interface"],"interface_hash":metadata["interface_hash"]}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    let declared = common::cellc_command()
        .args(["interface-diff", "--old"])
        .arg(directory.path().join("old.json"))
        .arg("--new")
        .arg(directory.path().join("new.json"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(!declared.status.success());
    assert_eq!(first(&declared)["schema"], "cellscript-interface-compatibility-v1");
    assert_eq!(first(&declared)["compatible"], false);
    assert_eq!(first(&declared)["diagnostics"][0]["code"], "E2501");

    // Equal declarations cannot make a modified executable independently checked.
    let mut bytes = std::fs::read(&right).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&right, bytes).unwrap();
    let malformed = diff(&left, &right);
    assert!(!malformed.status.success());
    assert_ne!(first(&malformed)["schema"], "cellscript-checked-interface-compatibility-v1");
}

#[test]
fn checked_diff_requires_bounded_complete_bundles() {
    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("missing.elf");
    std::fs::write(&artifact, b"not a complete bundle").unwrap();
    assert!(!diff(&artifact, &artifact).status.success());
    let oversized = directory.path().join("large.elf");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(cellscript_artifact_checker::interface::MAX_PROJECTION_FILE_BYTES as u64 + 1).unwrap();
    let rejected = diff(&oversized, &oversized);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stdout).contains("byte budget"));
}

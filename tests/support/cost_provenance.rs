use std::{fs, path::Path, process::Command};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn output(program: &str, args: &[&str], root: &Path) -> String {
    let result = Command::new(program).args(args).current_dir(root).output().expect("cost provenance command");
    assert!(result.status.success(), "cost provenance command failed: {program} {args:?}");
    String::from_utf8(result.stdout).expect("cost provenance UTF-8").trim().to_string()
}

fn untracked_source_hash(root: &Path) -> String {
    let paths = output("git", &["ls-files", "--others", "--exclude-standard", "-z"], root);
    let mut paths: Vec<_> = paths.split('\0').filter(|path| !path.is_empty()).collect();
    paths.sort_unstable();
    let mut hash = Sha256::new();
    for path in paths {
        let bytes = fs::read(root.join(path)).expect("untracked source file");
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    hex::encode(hash.finalize())
}

pub fn capture_source(root: &Path) -> Value {
    let tracked_diff = output("git", &["diff", "--submodule=diff", "HEAD", "--", "."], root);
    let fixtures: serde_json::Map<String, Value> = [
        "tests/cost_corpus.rs",
        "tests/business_corpus.rs",
        "tests/support/cost_growth.rs",
        "tests/support/cost_scalar.rs",
        "tests/support/cost_memory.rs",
        "tests/support/cost_measurement.rs",
        "tests/support/cost_stack.rs",
        "tests/fixtures/cost_corpus/expanded_fixtures.json",
        "tests/fixtures/cost_corpus/growth_budgets.json",
        "tests/fixtures/cost_corpus/expanded_budgets.json",
        "tests/fixtures/cost_corpus/scalar_budgets.json",
        "tests/fixtures/cost_corpus/multi_script_budgets.json",
        "tests/fixtures/cost_corpus/pool_merge.cell",
        "tests/fixtures/cost_corpus/schema_roll.cell",
        "tests/fixtures/cost_corpus/nft_lock.cell",
        "tests/fixtures/cost_corpus/Cargo.toml",
        "tests/fixtures/cost_corpus/src/common.rs",
        "tests/fixtures/cost_corpus/src/pool_merge.rs",
        "tests/fixtures/cost_corpus/src/schema_roll.rs",
        "tests/fixtures/cost_corpus/src/nft_lock.rs",
        "tests/fixtures/capability_anchor_order.cell",
        "tests/fixtures/capability_anchor_policy.cell",
        "tests/fixtures/capability_anchor_token.cell",
        "tests/fixtures/capability_anchor_authorization.cell",
    ]
    .into_iter()
    .map(|path| (path.to_string(), json!(sha256(&fs::read(root.join(path)).expect("cost fixture source")))))
    .collect();
    json!({
        "compiler_version": env!("CARGO_PKG_VERSION"),
        "source_commit": output("git", &["rev-parse", "HEAD"], root),
        "source_dirty": !output("git", &["status", "--porcelain", "--untracked-files=all"], root).is_empty(),
        "tracked_diff_sha256": sha256(tracked_diff.as_bytes()),
        "untracked_source_sha256": untracked_source_hash(root),
        "cargo_lock_sha256": sha256(&fs::read(root.join("Cargo.lock")).expect("compiler lock")),
        "rust_toolchain": output("rustc", &["-vV"], root),
        "measurement_fixture_sha256": fixtures,
    })
}

pub fn vm_configuration() -> Value {
    json!({
        "ckb_testtool":"1.1.1", "ckb_script":"1.1.0", "ckb_vm":"0.24.14",
        "consensus":"ConsensusBuilder::default with CKB2021/CKB2023 new_dev_default",
        "tip_number":0, "verification_environment":"submit", "group_cycle_limit":10_000_000,
        "group_scope":"scheduler total including spawned VMs",
        "stack_scope":"static single-VM call chain; external EXEC/SPAWN unknown"
    })
}

pub fn capture(root: &Path, strip: &Path) -> Value {
    let mut provenance = capture_source(root);
    let profile = json!({
        "rust_reference_lock_sha256": sha256(&fs::read(root.join("tests/fixtures/cost_corpus/Cargo.lock")).expect("Rust reference lock")),
        "strip_version": output(strip.to_str().expect("strip path"), &["--version"], root),
        "strip_sha256": sha256(&fs::read(strip).expect("strip executable")),
        "target": "riscv64-elf",
        "target_profile": "ckb",
        "edition": "2027",
        "opt_level": 3,
        "rust_reference_profile": {"opt_level":"z", "lto":"thin", "codegen_units":1, "panic":"abort", "target":"riscv64imac-unknown-none-elf"},
        "vm_configuration": vm_configuration(),
    });
    provenance.as_object_mut().unwrap().extend(profile.as_object().unwrap().clone());
    provenance
}

#[test]
fn security_audit_coverage_requires_docs_tcb_and_live_negative_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let agreement_docs = temp.path().join(AGREEMENT_ROOT).join("docs");
    let core_docs = temp.path().join(CORE_ROOT).join("docs");
    let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
    std::fs::create_dir_all(&agreement_docs).unwrap();
    std::fs::create_dir_all(&core_docs).unwrap();
    std::fs::create_dir_all(&riscv_src).unwrap();
    std::fs::write(
        agreement_docs.join("SECURITY.md"),
        "## Implemented Guards\npublic/shared CellDep\npublic BTC SPV\nRWA legal/registry review evidence\nexternal BIP340\n## Not Implemented\n## Risk Posture\n",
    )
    .unwrap();
    std::fs::write(
        agreement_docs.join("AUDIT_STATUS.md"),
        "## Claim Classification\n## Fixture Honesty\nexternal production attestations, public BTC SPV evidence, and RWA legal/registry review evidence still required\n## Production Statement Boundary\n",
    )
    .unwrap();
    std::fs::write(core_docs.join("RISCV_VERIFIER_SHELL.md"), "## Unsafe Boundary\nsyscall register ABI only\n").unwrap();
    std::fs::write(
        riscv_src.join("main.rs"),
        "// SAFETY: test syscall boundary\nunsafe {\n}\n// SAFETY: second syscall boundary\nunsafe {\n}\n",
    )
    .unwrap();
    let repo_commit = ensure_clean_fixture_repo(temp.path());
    let tcb_source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
    let tcb_files = json_array_strings(&tcb_source, "/files").into_iter().map(|path| json!({ "path": path })).collect::<Vec<_>>();

    let core_security = json!({ "status": "passed" });
    let invariant_matrix = json!({ "status": "passed" });
    let live_evidence = json!({
        "checks": {
            "negative_cases_rejected": true,
            "valid_originate_repay_claim_live": true,
        }
    });
    let tcb = json!({
        "status": "passed_local_review_external_attestation_required",
        "repo_commit": repo_commit,
        "source_inventory": {
            "source_tree_sha256": json_pointer_str(&tcb_source, "/sha256"),
            "total_files": json_pointer_i64(&tcb_source, "/file_count"),
            "files": tcb_files,
            "unsafe_hits": [
                { "path": "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier_riscv/src/main.rs" }
            ],
            "review_hits": [],
        },
        "local_review_gates": [
            { "name": "reference_bip340_vectors", "status": "passed" }
        ],
    });
    let attestation_templates = json!({ "status": "passed" });

    let passed =
        validate_security_audit_coverage(temp.path(), &core_security, &invariant_matrix, &live_evidence, &tcb, &attestation_templates)
            .unwrap();
    let mut failed_tcb = tcb.clone();
    failed_tcb["source_inventory"]["review_hits"] = json!([{ "path": "todo.rs", "line": 1 }]);
    let failed = validate_security_audit_coverage(
        temp.path(),
        &core_security,
        &invariant_matrix,
        &live_evidence,
        &failed_tcb,
        &attestation_templates,
    )
    .unwrap();

    assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));
    assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed, "/checks/tcb_review_hits_empty"));
}

#[test]
fn bip340_tcb_source_inventory_must_match_current_safe_tree() {
    let temp = tempfile::tempdir().unwrap();
    let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
    std::fs::create_dir_all(&riscv_src).unwrap();
    std::fs::write(riscv_src.join("main.rs"), "fn main() {}\n").unwrap();
    let repo_commit = ensure_clean_fixture_repo(temp.path());
    let source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
    let files = json_array_strings(&source, "/files").into_iter().map(|path| json!({ "path": path })).collect::<Vec<_>>();
    let tcb = json!({
        "repo_commit": repo_commit,
        "source_inventory": {
            "source_tree_sha256": json_pointer_str(&source, "/sha256"),
            "total_files": json_pointer_i64(&source, "/file_count"),
            "files": files,
        },
    });

    let passed = validate_bip340_tcb_source_inventory(temp.path(), &tcb).unwrap();
    assert_eq!(json_pointer_str(&passed, "/status"), Some("passed"));

    let mut stale_hash = tcb.clone();
    stale_hash["source_inventory"]["source_tree_sha256"] = json!(format!("0x{}", "00".repeat(32)));
    let failed_hash = validate_bip340_tcb_source_inventory(temp.path(), &stale_hash).unwrap();
    assert_eq!(json_pointer_str(&failed_hash, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_hash, "/checks/source_tree_sha256_matches_current"));

    let mut stale_commit = tcb.clone();
    stale_commit["repo_commit"] = json!("0000000000000000000000000000000000000000");
    let failed_commit = validate_bip340_tcb_source_inventory(temp.path(), &stale_commit).unwrap();
    assert_eq!(json_pointer_str(&failed_commit, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_commit, "/checks/repo_commit_matches_current_head"));

    let mut missing_commit = tcb.clone();
    missing_commit.as_object_mut().unwrap().remove("repo_commit");
    let failed_missing_commit = validate_bip340_tcb_source_inventory(temp.path(), &missing_commit).unwrap();
    assert_eq!(json_pointer_str(&failed_missing_commit, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_missing_commit, "/checks/repo_commit_present"));

    let mut missing_file = tcb;
    missing_file["source_inventory"]["files"] = json!([]);
    let failed_files = validate_bip340_tcb_source_inventory(temp.path(), &missing_file).unwrap();
    assert_eq!(json_pointer_str(&failed_files, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_files, "/checks/source_tree_file_list_matches_current"));
}

#[cfg(unix)]
#[test]
fn bip340_tcb_source_inventory_rejects_symlinked_source() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let riscv_src = temp.path().join(CORE_ROOT).join("verifier/novaseal_btc_verifier_riscv/src");
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(&riscv_src).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(riscv_src.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(outside.join("linked.rs"), "pub fn outside() {}\n").unwrap();
    symlink(outside.join("linked.rs"), riscv_src.join("linked.rs")).unwrap();
    let source = source_tree_hash_with_options(temp.path(), BIP340_TCB_SOURCE_HASH_PATHS, true).unwrap();
    let tcb = json!({
        "source_inventory": {
            "source_tree_sha256": json_pointer_str(&source, "/sha256"),
            "total_files": json_pointer_i64(&source, "/file_count"),
            "files": json_array_strings(&source, "/files")
                .into_iter()
                .map(|path| json!({ "path": path }))
                .collect::<Vec<_>>(),
        },
    });

    let failed = validate_bip340_tcb_source_inventory(temp.path(), &tcb).unwrap();

    assert_eq!(json_pointer_str(&failed, "/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed, "/checks/current_source_tree_valid"));
    assert!(!json_pointer_bool(&failed, "/checks/source_tree_invalid_paths_empty"));
}

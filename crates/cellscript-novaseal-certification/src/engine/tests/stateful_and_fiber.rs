#[test]
fn stateful_acceptance_requires_profile_and_business_coverage() {
    let mut report = json!({
        "status": "passed",
        "blocker_count": 0,
        "live_devnet_rpc_executed": true,
        "stateful_lifecycle_executed": true,
        "profile_coverage": { "status": "passed" },
        "business_scenario_coverage": { "status": "passed" },
    });

    assert!(stateful_acceptance_passed(&report));
    assert!(stateful_local_acceptance_passed(&report));

    report["business_scenario_coverage"]["status"] = Value::String("failed".to_string());
    assert!(!stateful_acceptance_passed(&report));
    assert!(!stateful_local_acceptance_passed(&report));

    report["business_scenario_coverage"]["status"] = Value::String("passed".to_string());
    report["status"] = Value::String("local_devnet_passed_external_endpoint_required".to_string());
    report["blocker_count"] = json!(1);
    report["local_blocker_count"] = json!(0);
    report["acceptance_blocker_count"] = json!(1);
    assert!(!stateful_acceptance_passed(&report));
    assert!(stateful_local_acceptance_passed(&report));

    report["local_blocker_count"] = json!(1);
    assert!(!stateful_local_acceptance_passed(&report));
}

#[test]
fn live_acceptance_blockers_include_unrun_devnet_and_external_endpoint_gaps() {
    let scenarios = vec![json!({
        "name": "fiber_candidate_settlement",
        "status": "ready_to_wire_live_devnet",
        "live_devnet_rpc_executed": false,
        "stateful_lifecycle_executed": false,
    })];
    let external_endpoint_coverage = json!({
        "status": "external_required",
        "btc": {"status": "external_required"},
        "fiber": {"status": "external_required"},
    });

    let blockers = stateful_live_acceptance_blockers(&scenarios, false, false, &external_endpoint_coverage);

    assert_eq!(blockers.len(), 4);
    assert_eq!(json_pointer_str(&blockers[0], "/scenario"), Some("fiber_candidate_settlement"));
    assert!(!json_pointer_bool(&blockers[0], "/live_devnet_rpc_executed"));
    assert!(blockers.iter().any(|blocker| json_pointer_str(blocker, "/dimension") == Some("profile_coverage")));
    assert!(blockers.iter().any(|blocker| json_pointer_str(blocker, "/dimension") == Some("business_scenario_coverage")));
    let endpoint =
        blockers.iter().find(|blocker| json_pointer_str(blocker, "/dimension") == Some("external_endpoint_coverage")).unwrap();
    assert_eq!(json_pointer_str(endpoint, "/btc_status"), Some("external_required"));
    assert_eq!(json_pointer_str(endpoint, "/fiber_status"), Some("external_required"));
}

#[test]
fn stateful_acceptance_status_stays_pending_when_external_endpoint_is_required() {
    let external_endpoint_coverage = json!({
        "status": "external_required",
        "btc": {"status": "external_required"},
        "fiber": {"status": "passed"},
    });

    let status = stateful_acceptance_status(0, 1, true, true, true, &external_endpoint_coverage);

    assert_eq!(status, "local_devnet_passed_external_endpoint_required");

    let status = stateful_acceptance_status(0, 0, true, true, true, &external_endpoint_coverage);

    assert_eq!(status, "local_devnet_passed_external_endpoint_required");
}

fn write_fiber_workflow_fixture_files(repo_root: &Path, fiber_repo: &Path, suite: &str) {
    let evidence_file = fiber_repo.join(format!("tests/bruno/e2e/{suite}/step.bru"));
    let stdout_log = repo_root.join(format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stdout"));
    let stderr_log = repo_root.join(format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stderr"));
    std::fs::create_dir_all(evidence_file.parent().unwrap()).unwrap();
    std::fs::create_dir_all(stdout_log.parent().unwrap()).unwrap();
    std::fs::create_dir_all(stderr_log.parent().unwrap()).unwrap();
    std::fs::write(evidence_file, "meta { name: step }\n").unwrap();
    std::fs::write(stdout_log, "Bruno suite passed\n").unwrap();
    std::fs::write(stderr_log, "").unwrap();
}

fn git_available() -> bool {
    std::process::Command::new("git").arg("--version").output().is_ok()
}

fn run_fixture_git(fiber_repo: &Path, args: &[&str]) {
    let output = std::process::Command::new("git").current_dir(fiber_repo).args(args).output().unwrap();
    assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
}

fn run_fixture_git_with_identity(fiber_repo: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(fiber_repo)
        .args(["-c", "user.name=CellScript Test", "-c", "user.email=cellscript@example.invalid", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
}

fn fixture_git_stdout(fiber_repo: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git").current_dir(fiber_repo).args(args).output().unwrap();
    assert!(output.status.success(), "git {} failed: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn ensure_clean_fixture_repo(repo: &Path) -> String {
    let init = std::process::Command::new("git").arg("init").arg("--initial-branch=main").arg(repo).output().unwrap();
    if !init.status.success() {
        let fallback = std::process::Command::new("git").arg("init").arg(repo).output().unwrap();
        assert!(fallback.status.success(), "git init failed: {}", String::from_utf8_lossy(&fallback.stderr));
        run_fixture_git(repo, &["checkout", "-B", "main"]);
    }
    run_fixture_git(repo, &["add", "."]);
    run_fixture_git_with_identity(repo, &["commit", "-m", "fixture"]);
    let status = fixture_git_stdout(repo, &["status", "--porcelain"]);
    assert!(status.is_empty(), "fixture repo should be clean after commit, got status:\n{status}");
    fixture_git_stdout(repo, &["rev-parse", "HEAD"])
}

fn ensure_clean_fiber_fixture_repo(fiber_repo: &Path) -> String {
    if fiber_repo.join(".git").exists() {
        let status = fixture_git_stdout(fiber_repo, &["status", "--porcelain"]);
        assert!(status.is_empty(), "fixture Fiber repo should stay clean, got status:\n{status}");
        return fixture_git_stdout(fiber_repo, &["rev-parse", "HEAD"]);
    }

    let init = std::process::Command::new("git").arg("init").arg("--initial-branch=develop").arg(fiber_repo).output().unwrap();
    if !init.status.success() {
        let fallback = std::process::Command::new("git").arg("init").arg(fiber_repo).output().unwrap();
        assert!(fallback.status.success(), "git init failed: {}", String::from_utf8_lossy(&fallback.stderr));
        run_fixture_git(fiber_repo, &["checkout", "-B", "develop"]);
    }
    run_fixture_git(fiber_repo, &["remote", "add", "origin", EXPECTED_FIBER_REPO_ORIGIN]);
    run_fixture_git(fiber_repo, &["add", "."]);
    run_fixture_git_with_identity(fiber_repo, &["commit", "-m", "fiber fixture"]);
    let status = fixture_git_stdout(fiber_repo, &["status", "--porcelain"]);
    assert!(status.is_empty(), "fixture Fiber repo should be clean after commit, got status:\n{status}");
    fixture_git_stdout(fiber_repo, &["rev-parse", "HEAD"])
}

fn fiber_workflow_fixture(
    repo_root: &Path,
    fiber_repo: &Path,
    suite: &str,
    mapped_profiles: &[&str],
    fiber_repo_info: &Value,
) -> Value {
    write_fiber_workflow_fixture_files(repo_root, fiber_repo, suite);
    let mut execution = json!({
        "status": "passed",
        "started_node": true,
        "command": ["npm", "exec", "--", "@usebruno/cli", "run", format!("e2e/{suite}"), "-r", "--env", "test"],
        "returncode": 0,
        "duration_seconds": 1.0,
        "stdout_log": format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stdout"),
        "stderr_log": format!("target/novaseal-fiber-node-experiments/{suite}/bruno.stderr"),
        "fiber_repo": fiber_repo_info,
    });
    if suite == "cross-chain-hub" {
        let bruno_cwd = "target/novaseal-fiber-node-experiments/cross-chain-hub/bruno-worktree";
        let patch = "e2e/cross-chain-hub/10-node1-add-fiber-invoice.bru";
        let patch_file = repo_root.join(bruno_cwd).join(patch);
        std::fs::create_dir_all(patch_file.parent().unwrap()).unwrap();
        std::fs::write(patch_file, "meta { name: patched }\n").unwrap();
        execution["bruno_cwd"] = Value::String(bruno_cwd.to_string());
        execution["bruno_compatibility_patches"] = json!([patch]);
    }
    json!({
        "suite": suite,
        "status": "passed",
        "present": true,
        "mapped_profiles": mapped_profiles,
        "expected_terms": {
            "term-a": true,
            "term-b": true,
        },
        "rpc_methods": ["open_channel"],
        "evidence_files": [format!("tests/bruno/e2e/{suite}/step.bru")],
        "execution": execution,
    })
}

fn complete_fiber_node_execution_report(repo_root: &Path, fiber_repo: &Path) -> Value {
    for (suite, _) in EXPECTED_FIBER_WORKFLOWS {
        write_fiber_workflow_fixture_files(repo_root, fiber_repo, suite);
    }
    let commit = ensure_clean_fiber_fixture_repo(fiber_repo);
    let fiber_repo_info = json!({
        "path": fiber_repo.display().to_string(),
        "origin": EXPECTED_FIBER_REPO_ORIGIN,
        "branch": "develop",
        "commit": commit,
        "dirty": false,
    });
    let workflows = EXPECTED_FIBER_WORKFLOWS
        .iter()
        .map(|(suite, profiles)| fiber_workflow_fixture(repo_root, fiber_repo, suite, profiles, &fiber_repo_info))
        .collect::<Vec<_>>();
    json!({
        "schema": EXPECTED_FIBER_NODE_EXECUTION_SCHEMA,
        "status": "passed",
        "fiber_repo": fiber_repo_info,
        "devnet_contract": {
            "runnable_devnet_contract_present": true,
        },
        "workflow_coverage": {
            "required_count": EXPECTED_FIBER_WORKFLOWS.len(),
            "present_count": EXPECTED_FIBER_WORKFLOWS.len(),
            "executed_count": EXPECTED_FIBER_WORKFLOWS.len(),
            "passed_execution_count": EXPECTED_FIBER_WORKFLOWS.len(),
            "all_required_workflows_present": true,
            "all_required_workflows_executed_passed": true,
            "partial_execution_passed": false,
        },
        "profiles_covered": EXPECTED_FIBER_NODE_PROFILES,
        "workflows": workflows,
    })
}

#[test]
fn fiber_node_execution_requires_exact_suite_profile_and_execution_contract() {
    if !git_available() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().join("cellscript");
    let fiber_repo = temp.path().join("fiber");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::create_dir_all(&fiber_repo).unwrap();

    let passed = fiber_node_execution_summary(&repo_root, Some(&complete_fiber_node_execution_report(&repo_root, &fiber_repo)));
    assert!(json_pointer_bool(&passed, "/all_required_workflows_executed_passed"));
    assert!(json_pointer_bool(&passed, "/checks/workflow_suites_exact"));
    assert!(json_pointer_bool(&passed, "/checks/profiles_covered_exact"));
    assert!(json_pointer_bool(&passed, "/checks/fiber_repo_exists"));
    assert!(json_pointer_bool(&passed, "/checks/fiber_repo_git_provenance_verified"));
    assert!(json_pointer_bool(&passed, "/checks/fiber_repo_current_checkout_matches_report"));
    assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/origin_matches_report"));
    assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/commit_matches_report"));
    assert!(json_pointer_bool(&passed, "/fiber_repo_git_provenance/checks/clean_tree"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_logs_exist"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_started_node"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_command_exact"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_returncode_zero"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/open-use-close-a-channel/execution_duration_positive"));
    assert!(json_pointer_bool(&passed, "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"));
    assert!(json_pointer_bool(&passed, "/checks/reported_partial_execution_semantics"));

    let drift_fiber_repo = temp.path().join("fiber-drift");
    std::fs::create_dir_all(&drift_fiber_repo).unwrap();
    let archived_report = complete_fiber_node_execution_report(&repo_root, &drift_fiber_repo);
    std::fs::write(drift_fiber_repo.join("post-run-change.txt"), "checkout advanced after archived run\n").unwrap();
    run_fixture_git(&drift_fiber_repo, &["add", "post-run-change.txt"]);
    run_fixture_git_with_identity(&drift_fiber_repo, &["commit", "-m", "advance checkout after archived run"]);
    let drifted_checkout = fiber_node_execution_summary(&repo_root, Some(&archived_report));
    assert!(json_pointer_bool(&drifted_checkout, "/all_required_workflows_executed_passed"));
    assert!(json_pointer_bool(&drifted_checkout, "/checks/fiber_repo_git_provenance_verified"));
    assert!(!json_pointer_bool(&drifted_checkout, "/checks/fiber_repo_current_checkout_matches_report"));
    assert!(!json_pointer_bool(&drifted_checkout, "/fiber_repo_git_provenance/verified"));
    assert!(!json_pointer_bool(&drifted_checkout, "/fiber_repo_git_provenance/checks/commit_matches_report"));

    let mut contradictory_partial = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    contradictory_partial["workflow_coverage"]["partial_execution_passed"] = json!(true);
    let failed_contradictory_partial = fiber_node_execution_summary(&repo_root, Some(&contradictory_partial));
    assert!(!json_pointer_bool(&failed_contradictory_partial, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_contradictory_partial, "/checks/reported_partial_execution_semantics"));

    let mut extra_suite = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    let mut unexpected_suite = extra_suite["workflows"][0].clone();
    unexpected_suite["suite"] = json!("unexpected-suite");
    unexpected_suite["mapped_profiles"] = json!([EXPECTED_FIBER_CANDIDATE_PROFILE]);
    extra_suite["workflows"].as_array_mut().unwrap().push(unexpected_suite);
    let failed_extra_suite = fiber_node_execution_summary(&repo_root, Some(&extra_suite));
    assert!(!json_pointer_bool(&failed_extra_suite, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_extra_suite, "/checks/workflow_suites_exact"));

    let mut wrong_profile = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    wrong_profile["workflows"][0]["mapped_profiles"] = json!([EXPECTED_FUNGIBLE_XUDT_PROFILE]);
    let failed_profile = fiber_node_execution_summary(&repo_root, Some(&wrong_profile));
    assert!(!json_pointer_bool(&failed_profile, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_profile, "/workflow_checks/open-use-close-a-channel/mapped_profiles_exact"));

    let mut dirty_repo = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    dirty_repo["fiber_repo"]["dirty"] = Value::Bool(true);
    let failed_dirty_repo = fiber_node_execution_summary(&repo_root, Some(&dirty_repo));
    assert!(!json_pointer_bool(&failed_dirty_repo, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_dirty_repo, "/checks/clean_expected_fiber_repo"));
    assert!(!json_pointer_bool(&failed_dirty_repo, "/checks/fiber_repo_git_provenance_verified"));
    assert!(!json_pointer_bool(&failed_dirty_repo, "/fiber_repo_git_provenance/checks/dirty_matches_report"));

    let mut forged_commit = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    forged_commit["fiber_repo"]["commit"] = json!("0123456789abcdef0123456789abcdef01234567");
    let failed_forged_commit = fiber_node_execution_summary(&repo_root, Some(&forged_commit));
    assert!(!json_pointer_bool(&failed_forged_commit, "/all_required_workflows_executed_passed"));
    assert!(json_pointer_bool(&failed_forged_commit, "/checks/fiber_repo_git_provenance_verified"));
    assert!(!json_pointer_bool(
        &failed_forged_commit,
        "/workflow_checks/open-use-close-a-channel/execution_fiber_repo_matches_report"
    ));
    assert!(!json_pointer_bool(&failed_forged_commit, "/fiber_repo_git_provenance/checks/commit_matches_report"));

    let mut stale_execution_commit = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    stale_execution_commit["workflows"][0]["execution"]["fiber_repo"]["commit"] = json!("0123456789abcdef0123456789abcdef01234567");
    let failed_stale_execution_commit = fiber_node_execution_summary(&repo_root, Some(&stale_execution_commit));
    assert!(!json_pointer_bool(&failed_stale_execution_commit, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(
        &failed_stale_execution_commit,
        "/workflow_checks/open-use-close-a-channel/execution_fiber_repo_matches_report"
    ));

    let mut missing_execution_repo = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    missing_execution_repo["workflows"][0]["execution"].as_object_mut().unwrap().remove("fiber_repo");
    let failed_missing_execution_repo = fiber_node_execution_summary(&repo_root, Some(&missing_execution_repo));
    assert!(!json_pointer_bool(&failed_missing_execution_repo, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(
        &failed_missing_execution_repo,
        "/workflow_checks/open-use-close-a-channel/execution_fiber_repo_matches_report"
    ));

    let mut missing_logs = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    missing_logs["workflows"][0]["execution"]["stdout_log"] = Value::String(String::new());
    let failed_logs = fiber_node_execution_summary(&repo_root, Some(&missing_logs));
    assert!(!json_pointer_bool(&failed_logs, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_logs, "/workflow_checks/open-use-close-a-channel/execution_logs_present"));

    let mut assumed_nodes_running = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    assumed_nodes_running["workflows"][0]["execution"]["started_node"] = Value::Bool(false);
    let failed_assumed_nodes_running = fiber_node_execution_summary(&repo_root, Some(&assumed_nodes_running));
    assert!(!json_pointer_bool(&failed_assumed_nodes_running, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_assumed_nodes_running, "/workflow_checks/open-use-close-a-channel/execution_started_node"));

    let mut wrong_command = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    wrong_command["workflows"][0]["execution"]["command"] =
        json!(["npm", "exec", "--", "@usebruno/cli", "run", "e2e/invoice-ops", "-r", "--env", "test"]);
    let failed_wrong_command = fiber_node_execution_summary(&repo_root, Some(&wrong_command));
    assert!(!json_pointer_bool(&failed_wrong_command, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_wrong_command, "/workflow_checks/open-use-close-a-channel/execution_command_exact"));

    let mut nonzero_returncode = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    nonzero_returncode["workflows"][0]["execution"]["returncode"] = json!(1);
    let failed_nonzero_returncode = fiber_node_execution_summary(&repo_root, Some(&nonzero_returncode));
    assert!(!json_pointer_bool(&failed_nonzero_returncode, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_nonzero_returncode, "/workflow_checks/open-use-close-a-channel/execution_returncode_zero"));

    let mut zero_duration = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    zero_duration["workflows"][0]["execution"]["duration_seconds"] = json!(0.0);
    let failed_zero_duration = fiber_node_execution_summary(&repo_root, Some(&zero_duration));
    assert!(!json_pointer_bool(&failed_zero_duration, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_zero_duration, "/workflow_checks/open-use-close-a-channel/execution_duration_positive"));

    let mut missing_evidence_file = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    missing_evidence_file["workflows"][0]["evidence_files"] = json!(["tests/bruno/e2e/open-use-close-a-channel/missing.bru"]);
    let failed_evidence = fiber_node_execution_summary(&repo_root, Some(&missing_evidence_file));
    assert!(!json_pointer_bool(&failed_evidence, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_evidence, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));

    let mut missing_patch_file = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    let cross_chain_hub = missing_patch_file["workflows"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|workflow| json_pointer_str(workflow, "/suite") == Some("cross-chain-hub"))
        .unwrap();
    cross_chain_hub["execution"]["bruno_compatibility_patches"] = json!(["e2e/cross-chain-hub/missing-patch.bru"]);
    let failed_patch = fiber_node_execution_summary(&repo_root, Some(&missing_patch_file));
    assert!(!json_pointer_bool(&failed_patch, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(&failed_patch, "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"));

    let mut unsafe_empty_patch_metadata = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
    unsafe_empty_patch_metadata["workflows"][0]["execution"]["bruno_cwd"] = Value::String("/tmp/fiber/tests/bruno".to_string());
    unsafe_empty_patch_metadata["workflows"][0]["execution"]["bruno_compatibility_patches"] = json!([]);
    let failed_unsafe_empty_patch_metadata = fiber_node_execution_summary(&repo_root, Some(&unsafe_empty_patch_metadata));
    assert!(!json_pointer_bool(&failed_unsafe_empty_patch_metadata, "/all_required_workflows_executed_passed"));
    assert!(!json_pointer_bool(
        &failed_unsafe_empty_patch_metadata,
        "/workflow_checks/open-use-close-a-channel/bruno_compatibility_patch_files_exist"
    ));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;

        let mut symlink_evidence = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        let outside = temp.path().join("outside-evidence.bru");
        std::fs::write(&outside, "meta { name: outside }\n").unwrap();
        let rel_path = "tests/bruno/e2e/open-use-close-a-channel/symlink.bru";
        let symlink_path = fiber_repo.join(rel_path);
        std::fs::create_dir_all(symlink_path.parent().unwrap()).unwrap();
        symlink(&outside, &symlink_path).unwrap();
        symlink_evidence["workflows"][0]["evidence_files"] = json!([rel_path]);

        let failed_symlink = fiber_node_execution_summary(&repo_root, Some(&symlink_evidence));
        assert!(!json_pointer_bool(&failed_symlink, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(&failed_symlink, "/workflow_checks/open-use-close-a-channel/evidence_files_exist"));
        std::fs::remove_file(&symlink_path).unwrap();

        let mut symlink_bruno_root = complete_fiber_node_execution_report(&repo_root, &fiber_repo);
        let cross_chain_hub = symlink_bruno_root["workflows"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|workflow| json_pointer_str(workflow, "/suite") == Some("cross-chain-hub"))
            .unwrap();
        let bruno_cwd = json_pointer_str(cross_chain_hub, "/execution/bruno_cwd").unwrap().to_string();
        let patch = json_pointer_str(cross_chain_hub, "/execution/bruno_compatibility_patches/0").unwrap().to_string();
        let bruno_root = repo_root.join(&bruno_cwd);
        std::fs::remove_dir_all(&bruno_root).unwrap();
        let outside_bruno_root = temp.path().join("outside-bruno-root");
        let outside_patch = outside_bruno_root.join(&patch);
        std::fs::create_dir_all(outside_patch.parent().unwrap()).unwrap();
        std::fs::write(outside_patch, "meta { name: outside-patched }\n").unwrap();
        symlink(&outside_bruno_root, &bruno_root).unwrap();

        let failed_symlink_bruno_root = fiber_node_execution_summary(&repo_root, Some(&symlink_bruno_root));
        assert!(!json_pointer_bool(&failed_symlink_bruno_root, "/all_required_workflows_executed_passed"));
        assert!(!json_pointer_bool(
            &failed_symlink_bruno_root,
            "/workflow_checks/cross-chain-hub/bruno_compatibility_patch_files_exist"
        ));
    }
}

#[test]
fn fiber_candidate_profile_docs_match_live_execution_boundary() {
    let repo_root = unit_resource_root();
    let report = validate_fiber_candidate_profile_package(&repo_root).unwrap();
    assert_eq!(json_pointer_str(&report, "/status"), Some("passed"));
    assert!(json_pointer_bool(&report, "/checks/docs_fiber_execution_claim_current"));
}

#[test]
fn external_endpoint_coverage_requires_public_btc_spv_evidence() {
    let ckb_live = json!({"required_live_checks_passed": true});
    let fiber = json!({
        "all_required_workflows_executed_passed": true,
        "checks": {
            "fiber_repo_git_provenance_verified": true,
        },
        "workflow_coverage": {
            "all_required_workflows_executed": true,
            "all_required_workflows_executed_passed": true,
        },
        "fiber_repo": {
            "origin": EXPECTED_FIBER_REPO_ORIGIN,
            "commit": "0123456789abcdef0123456789abcdef01234567",
            "dirty": false,
        },
        "fiber_repo_git_provenance": {
            "verified": true,
        },
    });
    let btc_missing = json!({
        "status": "external_required",
        "reason": "missing public BTC SPV evidence",
        "required_report": PUBLIC_BTC_SPV_EVIDENCE,
        "template": PUBLIC_BTC_SPV_EVIDENCE_TEMPLATE,
    });

    let coverage = external_endpoint_coverage_summary(&btc_missing, &fiber, &ckb_live, &ckb_live, &ckb_live, &ckb_live);

    assert_eq!(json_pointer_str(&coverage, "/status"), Some("external_required"));
    assert_eq!(json_pointer_str(&coverage, "/btc/status"), Some("external_required"));
    assert_eq!(json_pointer_str(&coverage, "/fiber/status"), Some("passed"));
    assert!(!json_pointer_bool(&coverage, "/production_complete"));
    assert!(json_pointer_bool(&coverage, "/checks/btc_ckb_lifecycle_passed"));
    assert!(!json_pointer_bool(&coverage, "/checks/public_btc_spv_evidence_passed"));
    assert!(json_pointer_bool(&coverage, "/checks/fiber_git_provenance_verified"));
    assert!(json_pointer_bool(&coverage, "/checks/fiber_node_workflows_passed"));

    let ckb_missing = json!({"required_live_checks_passed": false});
    let failed_local_btc = external_endpoint_coverage_summary(&btc_missing, &fiber, &ckb_missing, &ckb_live, &ckb_live, &ckb_live);
    assert_eq!(json_pointer_str(&failed_local_btc, "/status"), Some("failed"));
    assert_eq!(json_pointer_str(&failed_local_btc, "/btc/status"), Some("failed"));
    assert!(!json_pointer_bool(&failed_local_btc, "/checks/btc_ckb_lifecycle_passed"));
    assert!(!json_pointer_bool(&failed_local_btc, "/production_complete"));

    let mut missing_fiber_provenance = fiber;
    missing_fiber_provenance["checks"]["fiber_repo_git_provenance_verified"] = Value::Bool(false);
    let failed_fiber =
        external_endpoint_coverage_summary(&btc_missing, &missing_fiber_provenance, &ckb_live, &ckb_live, &ckb_live, &ckb_live);
    assert_eq!(json_pointer_str(&failed_fiber, "/status"), Some("external_required"));
    assert_eq!(json_pointer_str(&failed_fiber, "/fiber/status"), Some("external_required"));
    assert!(!json_pointer_bool(&failed_fiber, "/checks/fiber_git_provenance_verified"));
    assert!(!json_pointer_bool(&failed_fiber, "/checks/fiber_node_workflows_passed"));
}

#[test]
fn all_profile_production_matrix_keeps_btc_and_fiber_evidence_separate() {
    let profile_certification = json!({
        "local_checks": {
            "conformance_gate_passed": true,
        },
        "planned_profile_packages": {
            "btc_tx_commitment": { "status": "passed" },
            "btc_utxo_seal": { "status": "passed" },
            "dual_seal": { "status": "passed" },
            "fiber_candidate": { "status": "passed" },
            "fungible_xudt": { "status": "passed" },
            "rwa_receipt": { "status": "passed" }
        },
    });
    let stateful_acceptance = json!({
        "profile_coverage": {
            "checks": {
                "core_profile_live_stateful": true,
                "agreement_profile_live_stateful": true,
            }
        },
        "business_scenario_coverage": {
            "checks": {
                "btc_transaction_commitment_transition_live": true,
                "btc_utxo_seal_closure_live": true,
                "dual_seal_finality_live": true,
                "fiber_candidate_path_live": true,
                "fungible_xudt_value_flow_live": true,
                "rwa_receipt_lifecycle_live": true,
            }
        },
        "external_endpoint_coverage": {
            "fiber": { "status": "passed" }
        },
    });
    let passed = json!({"status": "passed"});
    let btc_missing = json!({"status": "external_required"});

    let matrix =
        build_profile_production_completeness(&profile_certification, &stateful_acceptance, &passed, &passed, &btc_missing, &passed);
    let profile_status = |profile: &str| {
        matrix
            .get("profiles")
            .and_then(Value::as_array)
            .and_then(|profiles| profiles.iter().find(|row| json_pointer_str(row, "/profile") == Some(profile)))
            .and_then(|row| json_pointer_str(row, "/status"))
            .unwrap_or("missing")
    };

    assert_eq!(json_pointer_str(&matrix, "/status"), Some("external_required"));
    assert!(json_pointer_bool(&matrix, "/local_complete"));
    assert!(!json_pointer_bool(&matrix, "/production_complete"));
    assert_eq!(profile_status("fiber-candidate-profile-v0"), "passed");
    assert_eq!(profile_status("btc-transaction-commitment-profile-v0"), "external_required");
    assert_eq!(profile_status("btc-utxo-seal-profile-v0"), "external_required");
    assert_eq!(profile_status("dual-seal-profile-v0"), "external_required");
    assert_eq!(profile_status("rwa-receipt-profile-v0"), "passed");
    assert!(json_array_strings(&matrix, "/missing_external_evidence").iter().any(|item| item == "public_btc_spv_evidence"));
}

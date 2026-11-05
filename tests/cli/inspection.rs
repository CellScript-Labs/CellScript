// CLI inspection behavior; included in the original cli integration target.

#[test]
fn cellc_explain_profile_canonical_group_matches_legacy_alias() {
    let canonical = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "profile", "ckb", "--json"]).output().unwrap();
    assert!(canonical.status.success(), "{}", String::from_utf8_lossy(&canonical.stderr));

    let legacy = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain-profile", "ckb", "--json"]).output().unwrap();
    assert!(legacy.status.success(), "{}", String::from_utf8_lossy(&legacy.stderr));

    assert_eq!(canonical.stdout, legacy.stdout);
}

#[test]
fn cellc_explain_graph_reports_cyclic_protocol_view() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    std::fs::write(&input, protocol_graph_pool_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "graph"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph["schema"], "cellscript-protocol-graph-v0.22");
    assert_eq!(graph["derivation"], "derived-from-compile-metadata");
    assert_eq!(graph["consensus_checked"], false);
    assert_eq!(graph["cycle_detected"], true);
    assert!(graph["self_loop_count"].as_u64().unwrap() >= 1, "expected self-loop graph: {graph}");
    assert!(graph["edges"].as_array().unwrap().iter().any(|edge| {
        edge["action_name"] == "swap"
            && edge["source_vertex"] == "Pool"
            && edge["target_vertex"] == "Pool"
            && edge["derivation"] == "type-pattern"
    }));
    assert!(graph["role_lints"].as_array().unwrap().iter().any(|lint| lint["code"] == "PG-ROLE-MISSING"));
}

#[test]
fn cellc_explain_graph_attributes_explicit_protocol_role_without_authorization_claim() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("role.cell");
    std::fs::write(&input, protocol_graph_explicit_role_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "graph"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let edge = graph["edges"].as_array().unwrap().iter().find(|edge| edge["action_name"] == "claim").expect("claim graph edge");
    assert_eq!(edge["role"], "participant");
    assert_eq!(edge["role_source"], "verification-predicate");
    assert_eq!(edge["role_strength"], "explicit");
    assert_eq!(edge["role_status"], "attributed");
    assert_eq!(edge["role_evidence_tier"], "metadata-only");
    assert_eq!(edge["authorization_proven"], false);
    assert_eq!(edge["role_conflict"], false);
    assert_eq!(edge["role_source_used"]["actor_binding"], "participant");
    assert_eq!(edge["role_source_used"]["actor_source"], "witness");
    assert_eq!(graph["role_model"]["authorization_proven"], false);
    assert_eq!(graph["role_warning_count"], 0);
}

#[test]
fn cellc_explain_graph_lints_weak_field_role_without_overclaiming_authority() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("weak-role.cell");
    std::fs::write(&input, protocol_graph_weak_field_role_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "graph"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let edge = graph["edges"].as_array().unwrap().first().expect("Vault graph edge");
    assert_eq!(edge["role"], "owner");
    assert_eq!(edge["role_source"], "field-name");
    assert_eq!(edge["role_status"], "weak-field-inference");
    assert_eq!(edge["authorization_proven"], false);
    assert!(edge["role_warnings"].as_array().unwrap().iter().any(|warning| warning["code"] == "PG-ROLE-WEAK-FIELD"));
}

#[test]
fn cellc_explain_graph_resolves_conflicting_role_sources_deterministically() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("conflicting-role.cell");
    std::fs::write(&input, protocol_graph_conflicting_role_source()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "graph"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));

    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let edge = graph["edges"].as_array().unwrap().iter().find(|edge| edge["action_name"] == "refund").expect("refund graph edge");
    assert_eq!(edge["role"], "participant");
    assert_eq!(edge["role_source"], "verification-predicate");
    assert_eq!(edge["role_status"], "conflicting");
    assert_eq!(edge["role_conflict"], true);
    assert!(edge["role_candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|candidate| { candidate["role"] == "initiator" && candidate["source"] == "witness-binding" }));
    assert!(graph["role_lints"].as_array().unwrap().iter().any(|warning| warning["code"] == "PG-ROLE-CONFLICT"));
}

#[test]
fn cellc_explain_graph_mermaid_and_legacy_alias_match_json() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("pool.cell");
    std::fs::write(&input, protocol_graph_pool_source()).unwrap();

    let canonical = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "graph"]).arg(&input).arg("--json").output().unwrap();
    assert!(canonical.status.success(), "{}", String::from_utf8_lossy(&canonical.stderr));

    let legacy = Command::new(env!("CARGO_BIN_EXE_cellc")).arg("explain-graph").arg(&input).arg("--json").output().unwrap();
    assert!(legacy.status.success(), "{}", String::from_utf8_lossy(&legacy.stderr));
    assert_eq!(canonical.stdout, legacy.stdout);

    let mermaid = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args(["explain", "graph"])
        .arg(&input)
        .args(["--format", "mermaid"])
        .output()
        .unwrap();
    assert!(mermaid.status.success(), "{}", String::from_utf8_lossy(&mermaid.stderr));
    let mermaid_stdout = String::from_utf8_lossy(&mermaid.stdout);
    assert!(mermaid_stdout.contains("flowchart LR"), "unexpected mermaid output: {mermaid_stdout}");
    assert!(mermaid_stdout.contains("Pool"), "unexpected mermaid output: {mermaid_stdout}");
    assert!(mermaid_stdout.contains("swap"), "unexpected mermaid output: {mermaid_stdout}");
}

#[test]
fn cellc_explain_profile_reports_ckb_v0_14_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "profile", "ckb", "--json"]).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(summary["profile"], "ckb");
    assert_eq!(summary["witness_abi"], "ckb-molecule-witness-args-input-type-v2+cellscript-entry-witness-v1");
    assert_eq!(summary["lock_args_abi"], "ckb-script-args-typed-fixed-bytes");
    assert_eq!(summary["source_encoding"], "ckb-source-group-high-bit");
    assert_eq!(summary["spawn_ipc_abi"], "ckb-vm-v2-spawn-ipc-syscalls-2601-2608");
    assert_eq!(summary["since_abi"], "ckb-since-rfc0017-typed-v1");
    assert_eq!(summary["cell_dep_abi"], "ckb-cell-dep-outpoint-and-dep-group");
    assert_eq!(summary["script_ref_abi"], "ckb-script-code-hash-hash-type-args");
    assert_eq!(summary["output_data_abi"], "ckb-outputs-and-outputs-data-index-aligned");
    assert_eq!(summary["capacity_floor_abi"], "ckb-output-capacity-floor-shannons");
    assert_eq!(summary["type_id_abi"], "ckb-type-id-v1");
    let boundaries = summary["boundaries"].as_array().unwrap();
    assert!(
        boundaries.iter().any(|boundary| boundary.as_str().unwrap_or_default().contains("outputs and outputs_data are index-aligned")),
        "missing outputs_data boundary: {boundaries:?}"
    );
    assert!(
        boundaries.iter().any(|boundary| boundary.as_str().unwrap_or_default().contains("lock_args parameters are typed script args")),
        "missing lock_args boundary: {boundaries:?}"
    );
    assert!(
        boundaries.iter().any(|boundary| boundary.as_str().unwrap_or_default().contains("capacity floors are declared")),
        "missing capacity floor boundary: {boundaries:?}"
    );
}

#[test]
fn cellc_explain_proof_reports_covenant_proof_plan() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("token.cell");
    std::fs::write(
        &input,
        r#"
module test

resource Token has store, replace, relock, consume {
    amount: u64,
}

action transfer_token(token: Token, to: Address) -> next_token: Token {
    verification
        std::lifecycle::transfer(token, next_token, to) {
            amount
        }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "proof"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let proof_plan = summary["proof_plan"].as_array().expect("proof_plan array");
    // std::lifecycle::transfer decomposes into consume + create proof plan records.
    let consume_plan = proof_plan
        .iter()
        .find(|plan| plan["feature"].as_str().is_some_and(|feature| feature.starts_with("consume-input:Token")))
        .expect("consume-input ProofPlan record");
    let create_plan = proof_plan
        .iter()
        .find(|plan| plan["feature"].as_str().is_some_and(|feature| feature.starts_with("create-output:Token")))
        .expect("create-output ProofPlan record");

    assert_eq!(summary["status"], "ok");
    assert_eq!(summary["proof_plan_summary"]["record_count"].as_u64().unwrap(), proof_plan.len() as u64);
    assert!(summary["proof_plan_summary"]["macro_provenance_count"].as_u64().unwrap() > 0);
    assert_eq!(consume_plan["trigger"], "explicit_entry");
    assert_eq!(consume_plan["scope"], "transaction");
    assert_eq!(create_plan["trigger"], "explicit_entry");
    assert_eq!(create_plan["scope"], "transaction");
    assert!(consume_plan["reads"].as_array().unwrap().iter().any(|read| read == "input"));
    assert!(create_plan["reads"].as_array().unwrap().iter().any(|read| read == "output"));
    assert!(consume_plan["coverage"].as_array().unwrap().iter().any(|coverage| {
        coverage.as_str().is_some_and(|coverage| coverage.contains("transaction-scoped relation over explicit input/output views"))
    }));
    assert!(create_plan["coverage"].as_array().unwrap().iter().any(|coverage| {
        coverage.as_str().is_some_and(|coverage| coverage.contains("transaction-scoped relation over explicit input/output views"))
    }));
}

#[test]
fn cellc_explain_proof_human_reports_macro_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("token.cell");
    std::fs::write(
        &input,
        r#"
module test

resource Token has store, replace, relock, consume {
    amount: u64,
}

action transfer_token(token: Token, to: Address) -> next_token: Token {
    verification
        std::lifecycle::transfer(token, next_token, to) {
            amount
        }
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "proof"]).arg(&input).output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Summary:"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("macro_provenance_records:"), "unexpected stdout: {}", stdout);
    assert!(stdout.contains("macro_provenance:"), "unexpected stdout: {}", stdout);
    // std::lifecycle::transfer decomposes; check for consume/create provenance instead of transfer.
    assert!(
        stdout.contains("macro_expansion:create=create-output") || stdout.contains("consume-input"),
        "unexpected stdout: {}",
        stdout
    );
}

#[test]
fn cellc_explain_proof_reports_declared_invariant() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("token.cell");
    std::fs::write(
        &input,
        r#"
module test

invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount
    assert_conserved(Token.amount, scope = group)
    assert_invariant(true, "token amount is conserved")
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "proof"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let proof_plan = summary["proof_plan"].as_array().expect("proof_plan array");
    let declared =
        proof_plan.iter().find(|plan| plan["origin"] == "invariant:token_conservation").expect("declared invariant ProofPlan record");

    assert_eq!(summary["status"], "ok");
    assert!(summary["proof_plan_summary"]["runtime_required_count"].as_u64().unwrap() > 0);
    assert!(summary["proof_plan_summary"]["metadata_only_gap_count"].as_u64().unwrap() > 0);
    assert_eq!(summary["proof_plan_summary"]["has_runtime_required_gaps"], true);
    assert_eq!(declared["category"], "declared-invariant");
    assert_eq!(declared["trigger"], "type_group");
    assert_eq!(declared["scope"], "group");
    assert_eq!(declared["codegen_coverage_status"], "gap:metadata-only");
    assert_eq!(declared["on_chain_checked"], false);
    assert!(declared["input_output_relation_checks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|check| check == "assert_conserved:Token.amount=metadata-only"));
}

#[test]
fn cellc_explain_proof_warns_for_lock_group_transaction_scope() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("token.cell");
    std::fs::write(
        &input,
        r#"
module test

invariant lock_scans_transaction {
    trigger: lock_group
    scope: transaction
    reads: inputs<Token>.amount, outputs<Token>.amount
    assert_sum(outputs<Token>.amount) <= assert_sum(inputs<Token>.amount)
}

resource Token {
    amount: u64,
}

action run() -> u64 {
    verification
        return 0
}
"#,
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "proof"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let proof_plan = summary["proof_plan"].as_array().expect("proof_plan array");
    let declared = proof_plan
        .iter()
        .find(|plan| plan["origin"] == "invariant:lock_scans_transaction")
        .expect("lock-group transaction invariant ProofPlan record");

    assert_eq!(declared["trigger"], "lock_group");
    assert_eq!(declared["scope"], "transaction");
    assert!(declared["coverage"].as_array().unwrap().iter().any(|coverage| {
        coverage.as_str().is_some_and(|coverage| coverage.contains("only inputs sharing this lock script trigger the verifier"))
    }));
    assert!(declared["diagnostics"].as_array().unwrap().iter().any(|diagnostic| {
        diagnostic["severity"] == "warning"
            && diagnostic["message"].as_str().is_some_and(|message| message.contains("do not imply type-group conservation"))
    }));
}

#[test]
fn cellc_explain_proof_summary_reports_fail_closed_diagnostics() {
    let fixture = fixtures::examples();
    let input = fixture.path().join("language/ckb/witness_source.cell");

    let output = Command::new(env!("CARGO_BIN_EXE_cellc")).args(["explain", "proof"]).arg(&input).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let proof_summary = &summary["proof_plan_summary"];
    assert!(proof_summary["fail_closed_count"].as_u64().unwrap() > 0, "unexpected summary: {}", summary);
    assert!(proof_summary["diagnostic_error_count"].as_u64().unwrap() > 0, "unexpected summary: {}", summary);
    assert_eq!(proof_summary["has_fail_closed_gaps"], true);
    assert_eq!(proof_summary["has_blocking_diagnostics"], true);
}

#[test]
fn cellc_explain_collections_reports_preallocation_and_growth_without_writing_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"budgets\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.cell"), include_str!("../../examples/language/batches/multi_asset_transfer.cell")).unwrap();
    let output = cellc_command().current_dir(root).args(["explain", "collections", "--json"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], "cellscript-collection-budget-report-v1");
    assert_eq!(report["experimental"], true);
    let owners = report["owners"].as_array().unwrap();
    assert_eq!(owners.len(), 1);
    let owner = &owners[0];
    assert_eq!(owner["name"], "Totals");
    assert_eq!(owner["reserve_all_once"]["cost"]["bytes"], 3712);
    assert_eq!(owner["reserve_all_once"]["cost"]["allocations"], 4);
    assert_eq!(owner["reserve_all_once"]["fits"], true);
    assert_eq!(owner["grow_all_incrementally"]["cost"]["bytes"], 7840);
    assert_eq!(owner["grow_all_incrementally"]["cost"]["allocations"], 24);
    assert_eq!(owner["grow_all_incrementally"]["bytes_shortfall"], 3744);
    assert_eq!(owner["grow_all_incrementally"]["allocations_shortfall"], 20);
    assert_eq!(owner["grow_all_incrementally"]["fits"], false);
    assert_eq!(owner["frame_region_bytes"], 4384);
    assert_eq!(owner["members"][0]["element_bytes"], 48);
    assert_eq!(owner["members"][0]["reserve_once"]["bytes"], 1568);
    assert_eq!(owner["members"][0]["incremental"]["bytes"], 3216);
    assert_eq!(owner["members"][2]["reserve_once"]["bytes"], 288);
    assert_eq!(owner["members"][2]["incremental"]["bytes"], 704);
    let human = cellc_command().current_dir(root).args(["explain", "collections"]).output().unwrap();
    assert!(human.status.success(), "{}", String::from_utf8_lossy(&human.stderr));
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.contains("3712 bytes / 4 allocations — fits"));
    assert!(text.contains("7840 bytes / 24 allocations — does not fit"));
    assert!(text.contains("retired bytes remain charged"));
    assert!(text.contains("not a program-path or worst-case proof"));
    let files = std::fs::read_dir(root).unwrap().map(|entry| entry.unwrap().file_name()).collect::<std::collections::BTreeSet<_>>();
    assert_eq!(files, [std::ffi::OsString::from("Cell.toml"), std::ffi::OsString::from("src")].into_iter().collect());
}

#[test]
fn cellc_explain_collections_resolves_unused_imported_records_and_shared_shortfalls() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nname = \"budgets\"\nversion = \"0.1.0\"\nedition = \"2027\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/values.cell"), "module values\npublic struct Pair<T: fixed_value> { left: T, right: T }").unwrap();
    let source = r#"module budgets
use values::Pair
struct Row { pair: Pair<u64>, tag: u8 }
collections Work { limits { bytes: 256, allocations: 2 }
    rows: Vec<Row> { items: 3 }
    by_key: OrderedMap<Hash, Row> { items: 2 }
}
action inspect() -> u64 { return 0 }
"#;
    std::fs::write(root.join("src/main.cell"), source).unwrap();
    let run = || cellc_command().current_dir(root).args(["explain", "collections", "--json"]).output().unwrap();
    let output = run();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let owner = &report["owners"][0];
    assert_eq!(owner["members"][0]["element_bytes"], 17);
    assert_eq!(owner["members"][1]["element_bytes"], 49);
    assert_eq!(owner["reserve_all_once"]["cost"]["bytes"], 240);
    assert_eq!(owner["reserve_all_once"]["fits"], true);
    assert_eq!(owner["grow_all_incrementally"]["cost"]["bytes"], 480);
    assert_eq!(owner["grow_all_incrementally"]["cost"]["allocations"], 5);
    std::fs::write(root.join("src/main.cell"), source.replace("bytes: 256, allocations: 2", "bytes: 160, allocations: 1")).unwrap();
    let output = run();
    assert!(output.status.success(), "a shared shortfall is a report, not invalid source");
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["owners"][0]["reserve_all_once"]["fits"], false);
    assert_eq!(report["owners"][0]["reserve_all_once"]["bytes_shortfall"], 80);
    assert_eq!(report["owners"][0]["reserve_all_once"]["allocations_shortfall"], 1);
    std::fs::write(root.join("src/main.cell"), source.replace("Vec<Row>", "Vec<Unknown>")).unwrap();
    assert!(!run().status.success());
    // A separate empty project must not retain the imported generic template.
    std::fs::remove_file(root.join("src/values.cell")).unwrap();
    std::fs::write(root.join("src/main.cell"), "module empty\naction inspect() -> u64 { return 0 }").unwrap();
    let output = run();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["owners"], serde_json::json!([]));
}

#[test]
fn cellc_explain_generics_reports_checked_vec_instantiations() {
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

action address_helpers(owner: Address, candidate: Address) -> bool {
    verification
        let mut owners = Vec::with_capacity(2)
        owners.push(owner)
        owners.insert(0, candidate)
        owners.swap(0, 1)
        let removed = owners.remove(1)
        owners.push(removed)
        owners.truncate(1)
        owners.set(0, owner)

        if owners.contains(owner) {
            return owners.first() == owner
        }

        false

}
action hash_helpers(first: Hash, second: Hash) -> bool {
    verification
        let mut keys = Vec::new()
        keys.push(first)
        keys.push(second)
        let popped = keys.pop()
        keys.push(popped)
        keys.swap(0, 1)
        keys.reverse()

        if keys.first() == first {
            return keys.last() == second
        }

        false
}
"#,
    )
    .unwrap();

    let json_output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["explain", "generics", "--json"]).output().unwrap();
    assert!(json_output.status.success(), "stderr: {}", String::from_utf8_lossy(&json_output.stderr));
    let summary: serde_json::Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(summary["status"], "ok");
    assert!(summary["count"].as_u64().unwrap() >= 2);
    let instantiations = summary["collection_instantiations"].as_array().unwrap();

    let address = instantiations
        .iter()
        .find(|instantiation| instantiation["collection_ty"] == "Vec<Address>")
        .expect("Vec<Address> instantiation should be explained");
    assert_eq!(address["scope_kind"], "action");
    assert_eq!(address["scope_name"], "address_helpers");
    assert_eq!(address["element_ty"], "Address");
    assert_eq!(address["element_width_bytes"], 32);
    assert_eq!(address["max_elements"], 8);
    assert_eq!(address["backing"], "stack-fixed-buffer:256");
    assert_eq!(address["status"], "checked-runtime");
    let address_helpers = address["helpers"].as_array().unwrap();
    for helper in ["contains", "index", "insert", "push", "remove", "set", "swap", "truncate", "with_capacity"] {
        assert!(
            address_helpers.iter().any(|value| value.as_str() == Some(helper)),
            "missing Address helper {helper}: {address_helpers:?}"
        );
    }
    assert!(
        !address_helpers.iter().any(|value| value.as_str() == Some("new")),
        "Vec<Address> was constructed with Vec::with_capacity, not Vec::new: {address_helpers:?}"
    );

    let hash = instantiations
        .iter()
        .find(|instantiation| instantiation["collection_ty"] == "Vec<Hash>")
        .expect("Vec<Hash> instantiation should be explained");
    assert_eq!(hash["scope_kind"], "action");
    assert_eq!(hash["scope_name"], "hash_helpers");
    assert_eq!(hash["element_ty"], "Hash");
    assert_eq!(hash["element_width_bytes"], 32);
    assert_eq!(hash["max_elements"], 8);
    assert_eq!(hash["backing"], "stack-fixed-buffer:256");
    assert_eq!(hash["status"], "checked-runtime");
    let hash_helpers = hash["helpers"].as_array().unwrap();
    for helper in ["index", "new", "pop", "push", "reverse", "swap"] {
        assert!(hash_helpers.iter().any(|value| value.as_str() == Some(helper)), "missing Hash helper {helper}: {hash_helpers:?}");
    }

    let text_output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["explain", "generics"]).output().unwrap();
    assert!(text_output.status.success(), "stderr: {}", String::from_utf8_lossy(&text_output.stderr));
    let stdout = String::from_utf8_lossy(&text_output.stdout);
    assert!(stdout.contains("Checked bounded generic collection instantiations"), "{}", stdout);
    assert!(stdout.contains("Vec<Address> -> Address"), "{}", stdout);
    assert!(stdout.contains("Vec<Hash> -> Hash"), "{}", stdout);
    assert!(stdout.contains("with_capacity"), "{}", stdout);
}

#[test]
fn cellc_explain_proof_plan_distinguishes_three_coverage_states() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write_xudt_package(root, xudt_conserved_source_with_runtime_helper());

    let output =
        Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).args(["explain", "proof"]).arg("--json").output().unwrap();
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let proof_plan = summary["proof_plan"].as_array().expect("proof_plan array");
    let aggregate =
        proof_plan.iter().find(|plan| plan["category"] == "aggregate-invariant").expect("aggregate-invariant ProofPlan record");

    // The same builder-assumption label surface must expose the checked-runtime
    // state distinctly from the runtime-required and metadata-only labels used by
    // the other two coverage branches.
    assert_eq!(aggregate["status"], "checked-runtime", "unexpected metadata: {:?}", aggregate);
    assert_eq!(aggregate["codegen_coverage_status"], "covered", "unexpected metadata: {:?}", aggregate);
    let assumptions = aggregate["builder_assumptions"].as_array().unwrap();
    assert!(
        assumptions.iter().any(|assumption| {
            let text = assumption.as_str().unwrap_or("");
            text.contains("runtime-helper-checked:xudt::require_group_amount_conserved")
        }),
        "expected checked helper label in assumptions: {assumptions:?}"
    );
    // The three label prefixes must be the canonical, distinguishable strings that
    // metadata consumers (registry, audit bundles) key off.
    let label = |prefix: &str| assumptions.iter().any(|assumption| assumption.as_str().is_some_and(|text| text.starts_with(prefix)));
    assert!(label("declared(runtime-helper-checked:"));
    // The other two branches use these prefixes; they must not collide with checked.
    let all_assumption_text = assumptions.iter().map(|assumption| assumption.as_str().unwrap_or("")).collect::<Vec<_>>().join("\n");
    assert!(!all_assumption_text.contains("declared(runtime-helper-required:"));
    assert!(!all_assumption_text.contains("declared(metadata-only invariant"));
}

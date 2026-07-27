//! Local NovaSeal BIP340 runtime-verifier TCB review bundle.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::crypto::sha256_hex;
use crate::shared::{python_json_pretty, python_path};

fn load(root: &Path, path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({ "missing": true, "path": path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/") }));
    }
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("failed to decode {}", path.display()))
}

fn collect_source(root: &Path, directory: &Path, files: &mut Vec<PathBuf>, invalid: &mut Vec<String>) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            invalid.push(path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"));
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name();
            if ["target", "build", ".git", "__pycache__"].iter().any(|skip| name == *skip) {
                continue;
            }
            collect_source(root, &path, files, invalid)?;
        } else if metadata.is_file() {
            let name = entry.file_name();
            if path.extension().and_then(|value| value.to_str()) == Some("rs")
                || ["Cargo.toml", "Cargo.lock", "README.md"].iter().any(|allowed| name == *allowed)
            {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn source_inventory(root: &Path, verifier_dirs: &[PathBuf]) -> Result<Value> {
    let mut files = Vec::new();
    let mut invalid = Vec::new();
    for directory in verifier_dirs {
        collect_source(root, directory, &mut files, &mut invalid)?;
    }
    files.sort();
    invalid.sort();
    let mut rows = Vec::new();
    let mut tree = Sha256::new();
    let mut unsafe_hits = Vec::new();
    let mut review_hits = Vec::new();
    let mut total_lines = 0_usize;
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let bytes = fs::read(&path)?;
        let digest = sha256_hex(&bytes);
        let text = String::from_utf8_lossy(&bytes);
        let lines = text.matches('\n').count() + usize::from(!text.ends_with('\n'));
        total_lines += lines;
        rows.push(json!({ "path": relative, "sha256": format!("0x{digest}"), "lines": lines }));
        tree.update(relative.as_bytes());
        tree.update([0]);
        tree.update(hex::decode(&digest)?);
        for (index, line) in text.lines().enumerate() {
            let stripped = line.trim();
            if stripped.contains("unsafe") {
                unsafe_hits.push(json!({ "path": relative, "line": index + 1, "text": stripped }));
            }
            if ["TODO", "todo!", "unimplemented!", "panic!"].iter().any(|token| stripped.contains(token)) {
                review_hits.push(json!({ "path": relative, "line": index + 1, "text": stripped }));
            }
        }
    }
    Ok(json!({
        "source_tree_sha256": format!("0x{}", hex::encode(tree.finalize())),
        "files": rows,
        "total_files": rows.len(),
        "total_lines": total_lines,
        "valid": invalid.is_empty(),
        "invalid_paths": invalid,
        "unsafe_hits": unsafe_hits,
        "review_hits": review_hits
    }))
}

fn gate(name: &str, passed: bool, evidence: &str, detail: Value) -> Value {
    json!({ "name": name, "status": if passed { "passed" } else { "failed" }, "evidence": evidence, "detail": detail })
}

fn bool_at(value: &Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(Value::as_bool) == Some(true)
}

fn equal_at(value: &Value, left: &str, right: &str) -> bool {
    value.pointer(left) == value.pointer(right)
}

fn git_commit(root: &Path) -> Option<String> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).current_dir(root).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn run(root: &Path, output: Option<&Path>, pretty: bool) -> Result<i32> {
    let core = root.join("proposals/novaseal/v0-mvp-skeleton");
    let target = root.join("target");
    let report_paths = [
        ("reference_vectors", core.join("target/novaseal-btc-verifier-vectors.json")),
        ("ipc_vectors", core.join("target/novaseal-btc-verifier-ipc-vectors.json")),
        ("shell_report", core.join("target/novaseal-btc-verifier-shell-report.json")),
        ("riscv_artifact", core.join("target/novaseal-riscv-shell-artifact.json")),
        ("child_verifier_ckb_vm", core.join("target/novaseal-ckb-vm-child-verifier-report.json")),
        ("parent_lock_ckb_vm", core.join("target/novaseal-parent-lock-ckb-vm-report.json")),
        ("combined_tx_ckb_vm", core.join("target/novaseal-combined-tx-report.json")),
        ("core_live_devnet", target.join("novaseal-devnet-stateful-live.json")),
        ("agreement_live_devnet", target.join("novaseal-agreement-devnet-stateful-live.json")),
    ];
    let mut reports = serde_json::Map::new();
    for (name, path) in report_paths {
        reports.insert(name.to_owned(), load(root, &path)?);
    }
    let reports = Value::Object(reports);
    let vectors = reports.pointer("/reference_vectors/summary").cloned().unwrap_or_else(|| json!({}));
    let ipc = reports.pointer("/ipc_vectors/summary").cloned().unwrap_or_else(|| json!({}));
    let shell = reports.pointer("/shell_report/summary").cloned().unwrap_or_else(|| json!({}));
    let artifact = reports.get("riscv_artifact").cloned().unwrap_or_else(|| json!({}));
    let child = reports.pointer("/child_verifier_ckb_vm/summary").cloned().unwrap_or_else(|| json!({}));
    let parent = reports.pointer("/parent_lock_ckb_vm/summary").cloned().unwrap_or_else(|| json!({}));
    let combined = reports.pointer("/combined_tx_ckb_vm/summary").cloned().unwrap_or_else(|| json!({}));
    let core_live = reports.get("core_live_devnet").cloned().unwrap_or_else(|| json!({}));
    let agreement_live = reports.get("agreement_live_devnet").cloned().unwrap_or_else(|| json!({}));
    let artifact_hash = artifact
        .pointer("/staged_release_elf/sha256")
        .and_then(Value::as_str)
        .map(|value| if value.starts_with("0x") { value.to_owned() } else { format!("0x{value}") })
        .map(Value::String)
        .unwrap_or(Value::Null);
    let gates = vec![
        gate(
            "reference_bip340_vectors",
            vectors.get("positive_self_verified").and_then(Value::as_u64).unwrap_or(0) > 0
                && equal_at(&vectors, "/positive_self_verified", "/positive_vectors")
                && equal_at(&vectors, "/negative_self_rejected", "/negative_vectors"),
            "target/novaseal-btc-verifier-vectors.json",
            vectors,
        ),
        gate(
            "fixed_ipc_vectors",
            ipc.get("expected_accept").and_then(Value::as_u64).unwrap_or(0) > 0
                && ipc.get("expected_reject").and_then(Value::as_u64).unwrap_or(0) > 0
                && ipc.get("total_vectors").and_then(Value::as_u64).unwrap_or(0)
                    == ipc.get("expected_accept").and_then(Value::as_u64).unwrap_or(0)
                        + ipc.get("expected_reject").and_then(Value::as_u64).unwrap_or(0),
            "target/novaseal-btc-verifier-ipc-vectors.json",
            ipc,
        ),
        gate(
            "riscv_shell_spawn_word_report",
            bool_at(&shell, "/all_expected_matched") && equal_at(&shell, "/matched_expected", "/total_vectors"),
            "target/novaseal-btc-verifier-shell-report.json",
            shell,
        ),
        gate(
            "riscv_artifact_preflight",
            bool_at(&artifact, "/staged_matches_release")
                && bool_at(&artifact, "/status/preflight_passed")
                && bool_at(&artifact, "/status/ready_for_ckb_vm_dry_run"),
            "target/novaseal-riscv-shell-artifact.json",
            json!({
                "artifact_hash": artifact_hash,
                "size_bytes": artifact.pointer("/staged_release_elf/size_bytes").cloned().unwrap_or(Value::Null),
                "production_ready_claim": artifact.pointer("/status/production_ready").cloned().unwrap_or(Value::Null)
            }),
        ),
        gate(
            "child_verifier_ckb_vm",
            bool_at(&child, "/child_verifier_ckb_vm_executed")
                && equal_at(&child, "/matched_expected", "/total_cases")
                && child.get("mismatched").and_then(Value::as_u64) == Some(0),
            "target/novaseal-ckb-vm-child-verifier-report.json",
            child,
        ),
        gate(
            "parent_lock_spawn_ckb_vm",
            bool_at(&parent, "/parent_spawn_executed")
                && bool_at(&parent, "/child_verifier_ckb_vm_executed")
                && bool_at(&parent, "/full_transaction_verifier_matched_expected")
                && equal_at(&parent, "/matched_expected", "/total_cases"),
            "target/novaseal-parent-lock-ckb-vm-report.json",
            parent,
        ),
        gate(
            "combined_lock_type_node_stack",
            ((bool_at(&combined, "/ckb_node_verification_stack_executed")
                && equal_at(&combined, "/node_stack_matched_expected", "/total_cases"))
                || (bool_at(&combined, "/combined_full_transaction_executed")
                    && equal_at(&combined, "/matched_expected", "/total_cases")
                    && bool_at(&combined, "/lock_and_type_script_groups_present")))
                && bool_at(&combined, "/child_spawn_target_cell_dep0_modelled"),
            "target/novaseal-combined-tx-report.json",
            combined,
        ),
        gate(
            "live_local_devnet_core_and_agreement",
            core_live.get("status").and_then(Value::as_str) == Some("passed")
                && bool_at(&core_live, "/live_devnet_rpc_executed")
                && agreement_live.get("status").and_then(Value::as_str) == Some("passed")
                && bool_at(&agreement_live, "/live_devnet_rpc_executed"),
            "target/novaseal-devnet-stateful-live.json + target/novaseal-agreement-devnet-stateful-live.json",
            json!({
                "core_status": core_live.get("status").cloned().unwrap_or(Value::Null),
                "agreement_status": agreement_live.get("status").cloned().unwrap_or(Value::Null),
                "core_verifier_data_hash": core_live.pointer("/artifacts/verifier/data_hash").cloned().unwrap_or(Value::Null),
                "agreement_verifier_data_hash": agreement_live.pointer("/artifacts/verifier/data_hash").cloned().unwrap_or(Value::Null)
            }),
        ),
    ];
    let verifier_dirs = [
        core.join("verifier/novaseal_btc_verifier_core"),
        core.join("verifier/novaseal_btc_verifier_riscv"),
        core.join("verifier/novaseal_btc_verifier"),
    ];
    let inventory = source_inventory(root, &verifier_dirs)?;
    let passed = gates.iter().all(|gate| gate["status"] == "passed") && inventory["valid"] == true;
    let report = json!({
        "schema": "novaseal-bip340-tcb-review-v0.1",
        "status": if passed { "passed_local_review_external_attestation_required" } else { "failed" },
        "repo_commit": git_commit(root),
        "verifier_id": "btc.bip340.v0",
        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
        "runtime_artifact": {
            "name": "cellscript_btc_bip340_verifier_riscv",
            "role": "runtime_verifier",
            "artifact_hash": artifact_hash,
            "artifact_hash_algorithm": "sha256",
            "size_bytes": artifact.pointer("/staged_release_elf/size_bytes").cloned().unwrap_or(Value::Null)
        },
        "local_review_gates": gates,
        "source_inventory": inventory,
        "tcb_boundary": {
            "included": ["BIP340 verifier core", "RISC-V spawn/pipe/wait shell", "IPC envelope parser", "artifact hash used by NovaSeal manifests"],
            "excluded": ["NovaSeal .cell protocol code", "CKB node implementation", "test harness Rust used only to construct evidence", "wallet UI implementation"]
        },
        "external_review": {
            "required_for_production": true,
            "attestation_file": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json",
            "template": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            "status": "missing_attestation"
        }
    });
    let default_output = target.join("novaseal-bip340-tcb-review.json");
    let output = python_path(output.unwrap_or(&default_output));
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    fs::write(&output, format!("{}\n", python_json_pretty(&report)?))?;
    if pretty {
        println!(
            "wrote {} status={} artifact={} local_gates={}",
            output.display(),
            report["status"].as_str().unwrap_or("failed"),
            report.pointer("/runtime_artifact/artifact_hash").and_then(Value::as_str).unwrap_or("None"),
            report["local_review_gates"].as_array().map_or(0, Vec::len)
        );
    }
    Ok(if passed { 0 } else { 1 })
}

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::shared::stable_json_pretty;

fn read_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("failed to read {}", path.display()))?)
        .with_context(|| format!("failed to parse {} as JSON", path.display()))
}

fn scalar(value: Option<&Value>) -> String {
    match value {
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Null) | None => "unknown".into(),
        Some(value) => value.to_string(),
    }
}

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if !condition {
        bail!(message.into());
    }
    Ok(())
}

pub fn novaseal_summary(report_path: &Path) -> Result<()> {
    let report = read_json(report_path)?;
    println!(
        "{}\t{}\t{}\t{}\t{}\t{}",
        scalar(report.get("status")),
        scalar(report.get("live_devnet_rpc_executed")),
        scalar(report.get("local_blocker_count")),
        scalar(report.get("acceptance_blocker_count")),
        scalar(report.get("blocker_count")),
        scalar(report.pointer("/external_endpoint_coverage/status")),
    );
    Ok(())
}

pub fn fiber_report_binding(compatibility_path: &Path, acceptance_path: &Path, expected_revision: &str) -> Result<()> {
    let compatibility = read_json(compatibility_path)?;
    let acceptance = read_json(acceptance_path)?;
    require(
        compatibility.pointer("/binding/fiber_revision").and_then(Value::as_str) == Some(expected_revision),
        "compatibility report Fiber revision does not match the pinned checkout",
    )?;
    require(
        compatibility.get("binding_fingerprint") == acceptance.get("binding_fingerprint"),
        "acceptance report is not bound to compatibility.json",
    )?;
    require(
        matches!(
            compatibility.get("status").and_then(Value::as_str),
            Some("LocalNodeAdvertised" | "ChannelReady" | "TopologyCertified")
        ),
        "full acceptance requires at least LocalNodeAdvertised compatibility evidence",
    )
}

pub fn ecosystem_reuse_contracts(compatibility_path: &Path, action_path: &Path) -> Result<()> {
    let compatibility = read_json(compatibility_path)?;
    let action = read_json(action_path)?;
    require(compatibility["status"] == "ok", "CKB compatibility status must be ok")?;
    require(compatibility["schema"] == "cellscript-ckb-std-compat-report-v0.19", "CKB compatibility schema drift")?;
    require(
        compatibility.pointer("/inline_abi/syscalls/load_cell_by_field") == Some(&json!(2081)),
        "load_cell_by_field syscall drift",
    )?;
    require(compatibility.pointer("/inline_abi/syscalls/load_witness") == Some(&json!(2074)), "load_witness syscall drift")?;
    require(compatibility.pointer("/inline_abi/sources/group_input") == Some(&json!((1_u64 << 56) | 1)), "group_input source drift")?;
    require(
        compatibility.pointer("/inline_abi/sources/group_output") == Some(&json!((1_u64 << 56) | 2)),
        "group_output source drift",
    )?;
    require(
        compatibility.pointer("/witness_args_policy/entry_payload_abi") == Some(&json!("cellscript-entry-witness-v1")),
        "entry witness ABI drift",
    )?;
    require(
        compatibility.pointer("/witness_args_policy/final_witness_args_owner") == Some(&json!("adapter")),
        "WitnessArgs ownership drift",
    )?;
    require(
        compatibility.pointer("/adapter_boundary/compiler_core_uses_ckb_sdk_rust") == Some(&json!(false)),
        "compiler core SDK boundary drift",
    )?;
    require(
        compatibility.pointer("/test_evidence/script_construction_api") == Some(&json!(true)),
        "script construction evidence missing",
    )?;
    require(
        compatibility.pointer("/adapter_boundary/script_construction/packed_type") == Some(&json!("ckb_types::packed::Script")),
        "packed Script type drift",
    )?;
    require(
        compatibility.pointer("/adapter_boundary/script_construction/evidence_schema")
            == Some(&json!("cellscript-ckb-script-evidence-v0.19")),
        "script evidence schema drift",
    )?;
    let supports = compatibility
        .pointer("/adapter_boundary/script_construction/supports")
        .and_then(Value::as_array)
        .context("compatibility supports must be an array")?;
    for required in ["args_exact_prefix_suffix", "script_ref_readback", "explicit_cell_dep_binding"] {
        require(supports.iter().any(|value| value == required), format!("missing adapter support {required}"))?;
    }

    require(action["status"] == "ok", "action build status must be ok")?;
    require(action["policy"] == "cellscript-action-builder-plan-v1", "action build policy drift")?;
    require(action["headless"] == true, "action build must remain headless")?;
    require(action["ui_scope"] == "none", "action build UI scope drift")?;
    require(action.pointer("/transaction_draft/state") == Some(&json!("ActionPlan")), "transaction draft state drift")?;
    require(action.pointer("/transaction_draft/can_submit") == Some(&json!(false)), "unmaterialized action must not submit")?;
    require(
        action.pointer("/transaction_draft/requires_packed_materialization") == Some(&json!(true)),
        "packed materialization must remain required",
    )?;
    for (field, expected) in [
        ("transaction", "ckb_types::packed::Transaction"),
        ("script", "ckb_types::packed::Script"),
        ("out_point", "ckb_types::packed::OutPoint"),
    ] {
        require(
            action.pointer(&format!("/transaction_draft/packed_materialization/{field}")) == Some(&json!(expected)),
            format!("packed materialization {field} drift"),
        )?;
    }
    require(
        action.pointer("/adapter_contract/schema") == Some(&json!("cellscript-ckb-adapter-contract-v0.19")),
        "adapter contract schema drift",
    )?;
    require(
        action.pointer("/adapter_contract/witness_policy/default_action_payload_field") == Some(&json!("input_type")),
        "default action payload field drift",
    )?;
    require(
        action.pointer("/adapter_contract/witness_policy/lock_signature_policy")
            == Some(&json!("explicit-adapter-owned-do-not-overwrite")),
        "lock signature policy drift",
    )?;
    let required_fields = action
        .pointer("/adapter_contract/resolved_tx_required_fields")
        .and_then(Value::as_array)
        .context("resolved_tx_required_fields must be an array")?;
    for required in ["outputs_data", "cell_deps", "lineage"] {
        require(required_fields.iter().any(|value| value == required), format!("resolved transaction field missing: {required}"))?;
    }
    require(
        action.pointer("/adapter_contract/acceptance_report_template/schema")
            == Some(&json!("cellscript-ckb-action-acceptance-report-v0.19")),
        "adapter acceptance template schema drift",
    )
}

fn collect_entries<'a>(metadata: &'a Value, group: &str, field: &str) -> impl Iterator<Item = &'a Value> {
    metadata[group].as_array().into_iter().flatten().flat_map(move |entry| entry[field].as_array().into_iter().flatten())
}

pub fn scope_014(out_dir: &Path, metadata_paths: &[PathBuf]) -> Result<()> {
    require(
        metadata_paths.len() == 7,
        format!("0.14 scope metadata oracle failed: expected 7 v0.14 language metadata files, got {}", metadata_paths.len()),
    )?;
    let mut features = BTreeSet::new();
    let mut operations = BTreeSet::new();
    let mut purposes = BTreeSet::new();
    let mut capacity_types = BTreeSet::new();
    let mut has_type_id_plan = false;
    let mut has_output_data_binding = false;
    let mut names = Vec::new();
    for path in metadata_paths {
        let metadata = read_json(path).map_err(|error| anyhow::anyhow!("0.14 scope metadata oracle failed: {error:#}"))?;
        names.push(path.file_name().context("metadata path has no file name")?.to_string_lossy().into_owned());
        let profile = &metadata["target_profile"];
        for (field, expected) in [
            ("name", "ckb"),
            ("source_encoding", "ckb-source-group-high-bit"),
            ("witness_abi", "ckb-molecule-witness-args-input-type-v2+cellscript-entry-witness-v1"),
            ("spawn_ipc_abi", "ckb-vm-v2-spawn-ipc-syscalls-2601-2608"),
            ("output_data_abi", "ckb-outputs-and-outputs-data-index-aligned"),
            ("type_id_abi", "ckb-type-id-v1"),
        ] {
            require(
                profile[field] == expected,
                format!("0.14 scope metadata oracle failed: {} target profile {field} drift", path.display()),
            )?;
        }
        require(
            metadata["artifact_hash"].as_str().is_some_and(|value| !value.is_empty()),
            format!("{} missing artifact hash", path.display()),
        )?;
        require(metadata["artifact_size_bytes"].as_u64().unwrap_or(0) > 0, format!("{} missing artifact size", path.display()))?;
        let ckb = metadata.pointer("/constraints/ckb").and_then(Value::as_object).context("metadata missing constraints.ckb")?;
        let abi = ckb.get("profile_abi_contract").context("metadata missing profile_abi_contract")?;
        require(abi["witness_abi"] == profile["witness_abi"], format!("{} profile ABI witness drift", path.display()))?;
        require(abi["output_data_abi"] == profile["output_data_abi"], format!("{} profile ABI output_data drift", path.display()))?;
        for value in metadata.pointer("/runtime/ckb_runtime_features").and_then(Value::as_array).into_iter().flatten() {
            if let Some(value) = value.as_str() {
                features.insert(value.to_owned());
            }
        }
        let runtime_accesses = metadata.pointer("/runtime/ckb_runtime_accesses").and_then(Value::as_array).into_iter().flatten();
        for access in runtime_accesses.chain(collect_entries(&metadata, "actions", "ckb_runtime_accesses")).chain(collect_entries(
            &metadata,
            "locks",
            "ckb_runtime_accesses",
        )) {
            if let Some(value) = access["operation"].as_str() {
                operations.insert(value.to_owned());
            }
        }
        for reference in ckb.get("script_references").and_then(Value::as_array).into_iter().flatten() {
            if let Some(purpose) = reference["purpose"].as_str() {
                purposes.insert(purpose.to_owned());
                if purpose == "spawn-target" {
                    require(
                        reference["dep_source"] == "CellDep-or-DepGroup",
                        format!("{} spawn target dep_source overclaimed", path.display()),
                    )?;
                    require(
                        reference["status"] == "runtime-required-builder-resolved",
                        format!("{} spawn target status drift", path.display()),
                    )?;
                    require(
                        reference["code_hash"].is_null() && reference["hash_type"].is_null() && reference["args"].is_null(),
                        format!("{} spawn target must remain builder-resolved", path.display()),
                    )?;
                }
            }
        }
        for floor in ckb.get("declared_capacity_floors").and_then(Value::as_array).into_iter().flatten() {
            if let Some(kind) = floor["type_name"].as_str() {
                capacity_types.insert(kind.to_owned());
            }
            require(floor["source"] == "dsl-with_capacity_floor", format!("{} capacity floor source drift", path.display()))?;
            require(floor["shannons"].as_u64().unwrap_or(0) > 0, format!("{} non-positive capacity floor", path.display()))?;
        }
        for create in collect_entries(&metadata, "actions", "create_set").chain(collect_entries(&metadata, "locks", "create_set")) {
            has_type_id_plan |= !create["ckb_type_id"].is_null();
            has_output_data_binding |= !create["ckb_output_data"].is_null();
        }
    }
    for required in [
        "ckb-spawn-ipc",
        "ckb-source-view",
        "ckb-witness-args",
        "ckb-lock-args",
        "ckb-sighash-all",
        "ckb-declarative-since",
        "ckb-declarative-capacity",
        "ckb-blake2b",
    ] {
        require(features.contains(required), format!("0.14 scope metadata oracle failed: missing runtime feature {required}"))?;
    }
    for required in [
        "spawn",
        "wait",
        "pipe",
        "pipe-write",
        "pipe-read",
        "close-fd",
        "source-group-input",
        "witness-lock",
        "lock-args",
        "sighash-all",
        "require-maturity",
        "require-time",
        "require-epoch-after",
        "require-epoch-relative",
        "occupied-capacity",
        "hash-blake2b",
    ] {
        require(operations.contains(required), format!("0.14 scope metadata oracle failed: missing runtime operation {required}"))?;
    }
    require(purposes.contains("spawn-target"), "0.14 scope metadata oracle failed: missing spawn target script-reference obligation")?;
    require(
        purposes.contains("type-id-create-output"),
        "0.14 scope metadata oracle failed: missing TYPE_ID create script-reference obligation",
    )?;
    require(capacity_types.contains("TimedToken"), "0.14 scope metadata oracle failed: missing TimedToken capacity floor")?;
    require(has_type_id_plan, "0.14 scope metadata oracle failed: missing TYPE_ID output plan in language examples")?;
    require(has_output_data_binding, "0.14 scope metadata oracle failed: missing outputs_data binding in language examples")?;
    let report = json!({
        "status": "passed",
        "metadata_files": names,
        "features": features,
        "operations": operations,
        "script_reference_purposes": purposes,
        "capacity_floor_types": capacity_types,
    });
    let report_path = out_dir.join("cellscript-0-14-scope-audit-report.json");
    fs::write(&report_path, format!("{}\n", stable_json_pretty(&report)?))?;
    println!("valid CellScript 0.14 scope audit: {}", report_path.display());
    Ok(())
}

pub fn cellfabric_bridge(envelope_path: &Path, summary_path: &Path) -> Result<()> {
    let envelope = read_json(envelope_path)?;
    let summary = read_json(summary_path)?;
    let source = &envelope["source"];
    for (condition, message) in [
        (envelope["schema"] == "cellscript-cellfabric-intent-envelope-v0.20", "envelope schema mismatch"),
        (envelope["status"] == "requires-runtime-binding", "envelope status mismatch"),
        (summary["schema"] == "cellscript-cellfabric-intent-envelope-v0.20", "summary schema mismatch"),
        (summary["import_status"] == "requires-runtime-binding", "import status mismatch"),
        (summary["status"] == "submitted-and-soft-confirmed-non-final", "flow status mismatch"),
        (summary["action_plan_hash_hex"] == source["action_plan_hash"], "action_plan_hash mismatch"),
        (summary["chain_id"] == source["target_profile"], "chain_id mismatch"),
        (summary["app_namespace"] == source["module"], "app_namespace mismatch"),
        (summary["action"] == source["action"], "action mismatch"),
        (summary["payload_format"] == "cellscript-action-plan-json-v1", "payload format mismatch"),
        (summary["requires_signature"] == true, "summary must require signature"),
        (summary["submitted"] == true, "summary must claim gateway submission"),
        (summary["soft_confirmed"] == true, "summary must claim soft confirmation"),
        (summary["l1_final"] == false, "summary must not claim L1 finality"),
        (summary["gateway_status"] == "Indexed", "gateway status mismatch"),
        (summary.pointer("/ledger_status/status/SoftConfirmed/non_final") == Some(&json!(true)), "ledger status mismatch"),
        (summary["bundle_intent_count"] == 1, "bundle must contain one intent"),
        (summary["excluded_conflict_count"] == 0, "unexpected excluded conflicts"),
        (summary["receipt_non_final"] == true, "receipt must remain non-final"),
        (summary["soft_confirmation_confidence"] == "unsigned-non-final-receipt", "unexpected soft confirmation confidence label"),
        (summary["settlement_requires_external_builder"] == true, "CellScript settlement must require external runtime builder"),
    ] {
        require(condition, message)?;
    }
    for field in ["intent_id", "bundle_id"] {
        let value = summary[field].as_str().unwrap_or_default();
        require(value.starts_with("0x") && value.len() == 66, format!("{field} must be 0x-prefixed 32-byte hash"))?;
    }
    println!("valid CellScript -> CellFabric bridge flow summary");
    Ok(())
}

pub fn rust_toolchain_channel(root: &Path) -> Result<()> {
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(root.join("rust-toolchain.toml"))?)?;
    println!("{}", manifest["toolchain"]["channel"].as_str().context("rust-toolchain.toml is missing toolchain.channel")?);
    Ok(())
}

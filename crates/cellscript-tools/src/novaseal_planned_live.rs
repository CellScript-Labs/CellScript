use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::shared::{stable_json_pretty, stable_json_spaced};

#[derive(Clone, Copy)]
pub(crate) struct Contract {
    pub(crate) profile: &'static str,
    pub(crate) output: &'static str,
    pub(crate) source: &'static str,
    pub(crate) source_actions: &'static [&'static str],
    pub(crate) lifecycle_action: &'static str,
    pub(crate) tx_hashes: &'static [(&'static str, &'static str)],
    pub(crate) live_checks: &'static [(&'static str, &'static str)],
    pub(crate) negative_cases: &'static [(&'static str, &'static str)],
}

const FUNGIBLE: Contract = Contract {
    profile: "fungible-xudt",
    output: "target/novaseal-fungible-xudt-devnet-stateful-live.json",
    source: "proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell",
    source_actions: &["issue_xudt", "transfer_xudt", "settle_xudt", "nova_fungible_xudt_lifecycle"],
    lifecycle_action: "nova_fungible_xudt_lifecycle",
    tx_hashes: &[("issue", "/issue/commit/tx_hash"), ("transfer", "/transfer/commit/tx_hash"), ("settle", "/settle/commit/tx_hash")],
    live_checks: &[
        ("issue_balance_live", "/issue/balance_live"),
        ("issue_receipt_live", "/issue/receipt_live"),
        ("transfer_old_balance_not_live", "/transfer/old_balance_not_live"),
        ("transfer_sender_balance_live", "/transfer/sender_balance_live"),
        ("transfer_receiver_balance_live", "/transfer/receiver_balance_live"),
        ("transfer_receipt_live", "/transfer/receipt_live"),
        ("transfer_amount_conserved", "/transfer/amount_conserved"),
        ("settle_old_balance_not_live", "/settle/old_balance_not_live"),
        ("settlement_receipt_live", "/settle/settlement_receipt_live"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_holder_signature_rejected", "wrong_holder_signature_dry_run"),
        ("transfer_amount_mismatch_rejected", "transfer_amount_mismatch_dry_run"),
        ("settle_wrong_holder_signature_rejected", "settle_wrong_holder_signature_dry_run"),
    ],
};

const RWA: Contract = Contract {
    profile: "rwa-receipt",
    output: "target/novaseal-rwa-receipt-devnet-stateful-live.json",
    source: "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
    source_actions: &["materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt", "nova_rwa_receipt_lifecycle"],
    lifecycle_action: "nova_rwa_receipt_lifecycle",
    tx_hashes: &[
        ("materialize", "/materialize/commit/tx_hash"),
        ("claim", "/claim/commit/tx_hash"),
        ("settle", "/settle/commit/tx_hash"),
    ],
    live_checks: &[
        ("materialized_receipt_live", "/materialize/receipt_live"),
        ("materialized_audit_event_live", "/materialize/audit_event_live"),
        ("claim_old_receipt_not_live", "/claim/old_receipt_not_live"),
        ("claimed_receipt_live", "/claim/claimed_receipt_live"),
        ("claim_event_live", "/claim/claim_event_live"),
        ("settle_old_claim_not_live", "/settle/old_claim_not_live"),
        ("settlement_receipt_live", "/settle/settlement_receipt_live"),
        ("settlement_event_live", "/settle/settlement_event_live"),
        ("amount_conserved", "/settle/amount_conserved"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_holder_claim_rejected", "wrong_holder_claim_dry_run"),
        ("wrong_issuer_settlement_rejected", "wrong_issuer_settlement_dry_run"),
        ("amount_mutation_rejected", "amount_mutation_dry_run"),
    ],
};

const BTC_TX: Contract = Contract {
    profile: "btc-transaction-commitment",
    output: "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json",
    source: "proposals/novaseal/btc-transaction-commitment-profile-v0/src/nova_btc_transaction_commitment_type.cell",
    source_actions: &["commit_btc_transaction_transition", "nova_btc_transaction_commitment_lifecycle"],
    lifecycle_action: "nova_btc_transaction_commitment_lifecycle",
    tx_hashes: &[("commit_transaction", "/commit_transaction/commit/tx_hash")],
    live_checks: &[
        ("old_state_not_live", "/commit_transaction/old_state_not_live"),
        ("new_state_live", "/commit_transaction/new_state_live"),
        ("receipt_live", "/commit_transaction/receipt_live"),
        ("btc_tx_tuple_bound", "/commit_transaction/btc_tx_tuple_bound"),
        ("transition_commitment_bound", "/commit_transaction/transition_commitment_bound"),
        ("public_btc_verification_executed", "/commit_transaction/public_btc_verification_executed"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_committer_signature_rejected", "wrong_committer_signature_dry_run"),
        ("zero_btc_txid_rejected", "zero_btc_txid_dry_run"),
        ("transition_hash_mismatch_rejected", "transition_hash_mismatch_dry_run"),
    ],
};

const BTC_UTXO: Contract = Contract {
    profile: "btc-utxo-seal",
    output: "target/novaseal-btc-utxo-seal-devnet-stateful-live.json",
    source: "proposals/novaseal/btc-utxo-seal-profile-v0/src/nova_btc_utxo_seal_type.cell",
    source_actions: &["close_btc_utxo_seal", "nova_btc_utxo_seal_lifecycle"],
    lifecycle_action: "nova_btc_utxo_seal_lifecycle",
    tx_hashes: &[("close_utxo_seal", "/close_utxo_seal/commit/tx_hash")],
    live_checks: &[
        ("old_state_not_live", "/close_utxo_seal/old_state_not_live"),
        ("new_state_live", "/close_utxo_seal/new_state_live"),
        ("receipt_live", "/close_utxo_seal/receipt_live"),
        ("sealed_utxo_tuple_bound", "/close_utxo_seal/sealed_utxo_tuple_bound"),
        ("spend_tuple_bound", "/close_utxo_seal/spend_tuple_bound"),
        ("public_btc_spend_verification_executed", "/close_utxo_seal/public_btc_spend_verification_executed"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_owner_signature_rejected", "wrong_owner_signature_dry_run"),
        ("utxo_commitment_mismatch_rejected", "utxo_commitment_mismatch_dry_run"),
        ("zero_spend_txid_rejected", "zero_spend_txid_dry_run"),
    ],
};

const DUAL: Contract = Contract {
    profile: "dual-seal",
    output: "target/novaseal-dual-seal-devnet-stateful-live.json",
    source: "proposals/novaseal/dual-seal-profile-v0/src/nova_dual_seal_type.cell",
    source_actions: &["finalize_dual_seal", "nova_dual_seal_lifecycle"],
    lifecycle_action: "nova_dual_seal_lifecycle",
    tx_hashes: &[("finalize_dual_seal", "/finalize_dual_seal/commit/tx_hash")],
    live_checks: &[
        ("old_state_not_live", "/finalize_dual_seal/old_state_not_live"),
        ("receipt_live", "/finalize_dual_seal/receipt_live"),
        ("btc_closure_bound", "/finalize_dual_seal/btc_closure_bound"),
        ("ckb_maturity_executed", "/finalize_dual_seal/ckb_maturity_executed"),
        ("dual_authority_executed", "/finalize_dual_seal/dual_authority_executed"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_btc_owner_signature_rejected", "wrong_btc_owner_signature_dry_run"),
        ("wrong_ckb_authority_signature_rejected", "wrong_ckb_authority_signature_dry_run"),
        ("btc_closure_commitment_missing_rejected", "btc_closure_commitment_missing_dry_run"),
    ],
};

const FIBER: Contract = Contract {
    profile: "fiber-candidate",
    output: "target/novaseal-fiber-candidate-devnet-stateful-live.json",
    source: "proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell",
    source_actions: &["settle_fiber_candidate", "nova_fiber_candidate_lifecycle"],
    lifecycle_action: "nova_fiber_candidate_lifecycle",
    tx_hashes: &[("settle_fiber_candidate", "/settle_fiber_candidate/commit/tx_hash")],
    live_checks: &[
        ("old_candidate_not_live", "/settle_fiber_candidate/old_candidate_not_live"),
        ("new_candidate_live", "/settle_fiber_candidate/new_candidate_live"),
        ("receipt_live", "/settle_fiber_candidate/receipt_live"),
        ("balance_commitment_progressed", "/settle_fiber_candidate/balance_commitment_progressed"),
        ("fiber_execution_executed", "/settle_fiber_candidate/fiber_execution_executed"),
        ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
    ],
    negative_cases: &[
        ("wrong_operator_signature_rejected", "wrong_operator_signature_dry_run"),
        ("balance_commitment_replay_rejected", "balance_commitment_replay_dry_run"),
    ],
};

fn contract(profile: &str) -> Result<Contract> {
    match profile {
        "fungible-xudt" => Ok(FUNGIBLE),
        "rwa-receipt" => Ok(RWA),
        "btc-transaction-commitment" => Ok(BTC_TX),
        "btc-utxo-seal" => Ok(BTC_UTXO),
        "dual-seal" => Ok(DUAL),
        "fiber-candidate" => Ok(FIBER),
        _ => bail!("unsupported planned profile {profile}"),
    }
}

fn rows(rows: &[(&str, &str)], pointer_name: &str) -> Vec<Value> {
    rows.iter().map(|(name, pointer)| json!({"name": name, (pointer_name): pointer})).collect()
}

pub(crate) fn lifecycle_type(data_hash: &str) -> Value {
    json!({"code_hash": data_hash, "hash_type": "data2", "args": "0x"})
}

pub(crate) fn contract_report_header(
    contract: Contract,
    scenario: &str,
    root: &Path,
    ckb_repo: &Path,
    ckb_bin: &Path,
    run_dir: &Path,
) -> Value {
    json!({
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": scenario,
        "repo_root": root.display().to_string(),
        "ckb_repo": ckb_repo.display().to_string(),
        "ckb_bin": ckb_bin.display().to_string(),
        "run_dir": run_dir.display().to_string(),
        "expected_tx_hashes": rows(contract.tx_hashes, "pointer"),
        "required_live_checks": rows(contract.live_checks, "pointer"),
        "required_negative_cases": rows(contract.negative_cases, "key"),
    })
}

fn not_run(contract: Contract) -> Value {
    let negative: BTreeMap<_, _> = contract
        .negative_cases
        .iter()
        .map(|(_, key)| ((*key).to_owned(), json!({"status": "not_run", "matched_expected": false})))
        .collect();
    json!({
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "not_run",
        "live_devnet_rpc_executed": false,
        "stateful_lifecycle_executed": false,
        "artifact_contract": {
            "source": contract.source,
            "source_actions": contract.source_actions,
            "lifecycle_action": contract.lifecycle_action,
            "stable_lifecycle_artifact_required": true,
            "dispatcher_required": false,
            "dispatcher_gap": Value::Null,
        },
        "expected_tx_hashes": rows(contract.tx_hashes, "pointer"),
        "required_live_checks": rows(contract.live_checks, "pointer"),
        "required_negative_cases": rows(contract.negative_cases, "key"),
        "provenance": {"repo_commit": Value::Null, "source_tree": Value::Null, "artifacts": Value::Null},
        "negative_cases": negative,
        "next_engineering_step": "Replace this contract report with profile-specific live CKB devnet transaction evidence, including fresh source/artifact provenance.",
    })
}

fn render(value: &Value, pretty: bool) -> Result<String> {
    if pretty {
        stable_json_pretty(value)
    } else {
        stable_json_spaced(value)
    }
}

fn prepare(root: &Path, contract: Contract) -> Result<Value> {
    let output = root
        .join("target/novaseal-planned-profile-artifacts")
        .join(contract.profile)
        .join(format!("{}.elf", contract.lifecycle_action));
    fs::create_dir_all(output.parent().context("artifact output has no parent")?)?;
    let args = [
        "run",
        "--quiet",
        "--bin",
        "cellc",
        "--",
        contract.source,
        "--target-profile",
        "ckb",
        "--target",
        "riscv64-elf",
        "--entry-action",
        contract.lifecycle_action,
        "-o",
        output.to_str().context("artifact path is not UTF-8")?,
    ];
    let completed = Command::new("cargo").args(args).current_dir(root).output()?;
    let command: Vec<_> = std::iter::once("cargo").chain(args).collect();
    let mut report = json!({
        "schema": "novaseal-planned-profile-artifact-prep-v0.1", "profile": contract.profile,
        "source": contract.source, "lifecycle_action": contract.lifecycle_action,
        "artifact": output.to_string_lossy(), "status": if completed.status.success() { "passed" } else { "failed" }, "command": command,
    });
    if completed.status.success() {
        report["size_bytes"] = json!(fs::metadata(output)?.len());
    } else {
        report["stderr"] = json!(String::from_utf8_lossy(&completed.stderr));
        report["stdout"] = json!(String::from_utf8_lossy(&completed.stdout));
    }
    Ok(report)
}

pub(crate) fn compile_contract(root: &Path, contract: Contract, output: &Path) -> Result<()> {
    fs::create_dir_all(output.parent().context("lifecycle artifact path has no parent")?)?;
    let status = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--locked",
            "--bin",
            "cellc",
            "--",
            contract.source,
            "--target-profile",
            "ckb",
            "--target",
            "riscv64-elf",
            "--entry-action",
            contract.lifecycle_action,
            "-o",
            output.to_str().context("lifecycle artifact path is not UTF-8")?,
        ])
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("failed to compile {} lifecycle", contract.profile);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    profile: &str,
    output: Option<&Path>,
    ckb_repo: Option<&Path>,
    ckb_bin: Option<&Path>,
    run_dir: Option<&Path>,
    pretty: bool,
    keep_node: bool,
    list_contract: bool,
    prepare_artifacts: bool,
    live: bool,
) -> Result<i32> {
    let contract = contract(profile)?;
    if prepare_artifacts {
        let report = prepare(root, contract)?;
        println!("{}", render(&report, pretty)?);
        return Ok(if report["status"] == "passed" { 0 } else { 1 });
    }
    let mut report = not_run(contract);
    if list_contract {
        println!("{}", render(&report, pretty)?);
        return Ok(1);
    }
    if live {
        report = match profile {
            "fungible-xudt" => crate::novaseal_planned_fungible::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?,
            "rwa-receipt" => crate::novaseal_planned_rwa::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?,
            "btc-transaction-commitment" => {
                crate::novaseal_planned_btc_tx::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?
            }
            "btc-utxo-seal" => crate::novaseal_planned_btc_utxo::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?,
            "dual-seal" => crate::novaseal_planned_dual::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?,
            "fiber-candidate" => crate::novaseal_planned_fiber::run(root, ckb_repo, ckb_bin, run_dir, contract, keep_node)?,
            _ => bail!("{profile} Rust live runner is not wired yet; refusing to emit synthetic devnet evidence"),
        };
    }
    let output = match output {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join(contract.output),
    };
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    fs::write(&output, format!("{}\n", render(&report, pretty)?))?;
    println!("wrote {} status={} profile={profile}", output.display(), report["status"].as_str().unwrap_or("failed"));
    Ok(if report["status"] == "passed" { 0 } else { 1 })
}

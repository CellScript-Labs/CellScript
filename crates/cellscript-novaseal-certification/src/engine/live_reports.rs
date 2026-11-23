use super::provenance::provenance_summary;
use super::{
    json_pointer_bool, json_pointer_bool_opt, json_pointer_str, negative_case_matched, tx_hash_value_is_real, AGREEMENT_MANIFEST,
    CORE_MANIFEST, VERIFIER_ROOT,
};
use crate::error::Result;
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn live_core_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let transition = report.get("transition").cloned().unwrap_or(Value::Null);
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            CORE_MANIFEST,
            "proposals/novaseal/v0-mvp-skeleton/src",
            "proposals/novaseal/v0-mvp-skeleton/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_core_live.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "bootstrap_tx_hash": json_pointer_str(report, "/bootstrap/commit/tx_hash"),
        "bootstrap_state_cell_live": json_pointer_bool_opt(report, "/bootstrap/state_cell_live"),
        "transition_tx_hash": json_pointer_str(&transition, "/commit/tx_hash"),
        "old_state_not_live": json_pointer_bool_opt(&transition, "/old_state_not_live"),
        "new_state_live": json_pointer_bool_opt(&transition, "/new_state_live"),
        "receipt_live": json_pointer_bool_opt(&transition, "/receipt_live"),
        "wrong_signature_rejected": negative_case_matched(report, "wrong_signature_dry_run"),
        "post_negative_state_still_live": json_pointer_bool_opt(report, "/negative_cases/post_negative_state_still_live"),
    }))
}

pub(super) fn live_agreement_summary(repo_root: &Path, report: Option<&Value>) -> Result<Value> {
    let Some(report) = report else {
        return Ok(json!({"present": false}));
    };
    if report.get("_invalid_json").is_some() {
        return Ok(json!({"present": true, "valid_json": false, "error": report.get("_invalid_json")}));
    }
    let provenance = provenance_summary(
        report,
        repo_root,
        &[
            AGREEMENT_MANIFEST,
            "proposals/novaseal/agreement-profile-v0/src",
            "proposals/novaseal/agreement-profile-v0/schemas",
            VERIFIER_ROOT,
            "crates/cellscript-tools/src/novaseal_agreement_live.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ],
    )?;
    Ok(json!({
        "present": true,
        "valid_json": true,
        "status": json_pointer_str(report, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool(report, "/live_devnet_rpc_executed"),
        "stateful_lifecycle_executed": json_pointer_bool(report, "/stateful_lifecycle_executed"),
        "provenance": provenance,
        "provenance_freshness_matched": json_pointer_bool(&provenance, "/freshness_matched"),
        "originate_tx_hash": json_pointer_str(report, "/originate/commit/tx_hash"),
        "repay_tx_hash": json_pointer_str(report, "/repay/commit/tx_hash"),
        "claim_originate_tx_hash": json_pointer_str(report, "/claim_originate/commit/tx_hash"),
        "claim_tx_hash": json_pointer_str(report, "/claim/commit/tx_hash"),
        "origin_active_live": json_pointer_bool_opt(report, "/originate/active_live"),
        "origin_principal_payout_live": json_pointer_bool_opt(report, "/originate/principal_payout_live"),
        "origin_receipt_live": json_pointer_bool_opt(report, "/originate/receipt_live"),
        "claim_origin_active_live": json_pointer_bool_opt(report, "/claim_originate/active_live"),
        "claim_origin_principal_payout_live": json_pointer_bool_opt(report, "/claim_originate/principal_payout_live"),
        "claim_origin_receipt_live": json_pointer_bool_opt(report, "/claim_originate/receipt_live"),
        "repay_old_active_not_live": json_pointer_bool_opt(report, "/repay/old_active_not_live"),
        "repay_closed_live": json_pointer_bool_opt(report, "/repay/closed_live"),
        "repay_lender_repayment_live": json_pointer_bool_opt(report, "/repay/lender_repayment_live"),
        "repay_borrower_collateral_return_live": json_pointer_bool_opt(report, "/repay/borrower_collateral_return_live"),
        "repay_receipt_live": json_pointer_bool_opt(report, "/repay/receipt_live"),
        "claim_old_active_not_live": json_pointer_bool_opt(report, "/claim/old_active_not_live"),
        "claim_closed_live": json_pointer_bool_opt(report, "/claim/closed_live"),
        "claim_lender_default_claim_live": json_pointer_bool_opt(report, "/claim/lender_default_claim_live"),
        "claim_receipt_live": json_pointer_bool_opt(report, "/claim/receipt_live"),
        "wrong_lender_signature_rejected": negative_case_matched(report, "wrong_lender_signature_dry_run"),
        "non_ckb_asset_kind_rejected": negative_case_matched(report, "non_ckb_asset_kind_dry_run"),
        "wrong_borrower_signature_rejected": negative_case_matched(report, "wrong_borrower_signature_dry_run"),
        "repay_payout_capacity_short_rejected": negative_case_matched(report, "repay_payout_capacity_short_dry_run"),
        "repay_payout_lock_args_mismatch_rejected": negative_case_matched(report, "repay_payout_lock_args_mismatch_dry_run"),
        "repay_wrong_payout_amount_rejected": negative_case_matched(report, "repay_wrong_payout_amount_dry_run"),
        "early_claim_rejected": negative_case_matched(report, "early_claim_dry_run"),
        "wrong_lender_claim_signature_rejected": negative_case_matched(report, "wrong_lender_claim_signature_dry_run"),
        "post_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_negative_active_still_live"),
        "post_claim_negative_active_still_live": json_pointer_bool_opt(report, "/negative_cases/post_claim_negative_active_still_live"),
    }))
}

fn summary_tx_hashes_real(summary: &Value, fields: &[&str]) -> bool {
    fields.iter().all(|field| summary.get(*field).is_some_and(tx_hash_value_is_real))
}

pub(super) fn core_live_summary_passed(live_core: &Value) -> bool {
    json_pointer_str(live_core, "/status") == Some("passed")
        && json_pointer_bool(live_core, "/live_devnet_rpc_executed")
        && json_pointer_bool(live_core, "/stateful_lifecycle_executed")
        && json_pointer_bool(live_core, "/provenance_freshness_matched")
        && summary_tx_hashes_real(live_core, &["bootstrap_tx_hash", "transition_tx_hash"])
        && json_pointer_bool(live_core, "/bootstrap_state_cell_live")
        && json_pointer_bool(live_core, "/old_state_not_live")
        && json_pointer_bool(live_core, "/new_state_live")
        && json_pointer_bool(live_core, "/receipt_live")
        && json_pointer_bool(live_core, "/wrong_signature_rejected")
        && json_pointer_bool(live_core, "/post_negative_state_still_live")
}

pub(super) fn agreement_live_summary_passed(live_agreement: &Value, agreement_conformance: &Value) -> bool {
    json_pointer_str(live_agreement, "/status") == Some("passed")
        && json_pointer_bool(live_agreement, "/live_devnet_rpc_executed")
        && json_pointer_bool(live_agreement, "/stateful_lifecycle_executed")
        && json_pointer_bool(live_agreement, "/provenance_freshness_matched")
        && summary_tx_hashes_real(live_agreement, &["originate_tx_hash", "repay_tx_hash", "claim_originate_tx_hash", "claim_tx_hash"])
        && [
            "origin_active_live",
            "origin_principal_payout_live",
            "origin_receipt_live",
            "claim_origin_active_live",
            "claim_origin_principal_payout_live",
            "claim_origin_receipt_live",
            "repay_old_active_not_live",
            "repay_closed_live",
            "repay_lender_repayment_live",
            "repay_borrower_collateral_return_live",
            "repay_receipt_live",
            "claim_old_active_not_live",
            "claim_closed_live",
            "claim_lender_default_claim_live",
            "claim_receipt_live",
            "wrong_lender_signature_rejected",
            "non_ckb_asset_kind_rejected",
            "wrong_borrower_signature_rejected",
            "repay_payout_capacity_short_rejected",
            "repay_payout_lock_args_mismatch_rejected",
            "repay_wrong_payout_amount_rejected",
            "early_claim_rejected",
            "wrong_lender_claim_signature_rejected",
            "post_negative_active_still_live",
            "post_claim_negative_active_still_live",
        ]
        .iter()
        .all(|key| json_pointer_bool(live_agreement, &format!("/{key}")))
        && json_pointer_str(agreement_conformance, "/status") == Some("passed")
}

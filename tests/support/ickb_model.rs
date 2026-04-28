//! iCKB benchmark fixture model verifier.
//!
//! This module deliberately verifies model-level JSON fixtures only. It does not
//! execute CKB VM and must not be used as production equivalence evidence.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const AR0: u128 = 10_000_000_000_000_000;
const SOFT_CAP: u128 = 10_000_000_000_000;
const MIN_DEPOSIT: u128 = 100_000_000_000;
const MAX_DEPOSIT: u128 = 100_000_000_000_000;
const ICKB_XUDT_BINDING: &str = "ickb_logic_hash+owner_mode_input_type";

pub const POSITIVE_FIXTURES: [&str; 6] = [
    "valid_deposit_phase_1.json",
    "valid_deposit_phase_2.json",
    "valid_ickb_transfer.json",
    "valid_withdrawal_redeem.json",
    "valid_limit_order_fulfillment.json",
    "valid_owned_owner_unlock.json",
];

pub const NEGATIVE_FIXTURES: [&str; 16] = [
    "amount_deflation_exact_equality.json",
    "amount_inflation.json",
    "capacity_violation.json",
    "cell_dep_substitution.json",
    "duplicate_receipt_double_mint.json",
    "forged_receipt.json",
    "limit_order_underpayment.json",
    "limit_order_wrong_asset.json",
    "missing_header_dep.json",
    "owned_owner_relative_distance_mismatch.json",
    "redeem_before_maturity.json",
    "script_role_confusion.json",
    "witness_malformation.json",
    "wrong_accumulated_rate.json",
    "wrong_owner.json",
    "wrong_xudt_binding.json",
];

pub fn verify_fixture_root(root: &Path) -> Value {
    let positive_dir = root.join("ickb_positive");
    let negative_dir = root.join("ickb_negative");
    let mut issues = Vec::<String>::new();
    let mut rows = Vec::<Value>::new();

    verify_fixture_set(&positive_dir, &POSITIVE_FIXTURES, true, &mut rows, &mut issues);
    verify_fixture_set(&negative_dir, &NEGATIVE_FIXTURES, false, &mut rows, &mut issues);

    serde_json::json!({
        "schema": "cellscript-ickb-fixture-verification-v0.17",
        "root": root.display().to_string(),
        "execution_level": "MODEL",
        "ckb_vm_execution": false,
        "positive_fixture_count": rows.iter().filter(|row| row["expected"] == "pass").count(),
        "negative_fixture_count": rows.iter().filter(|row| row["expected"] == "fail").count(),
        "fixture_count": rows.len(),
        "status": if issues.is_empty() { "ok" } else { "failed" },
        "issue_count": issues.len(),
        "issues": issues,
        "fixtures": rows,
        "vm_execution_note": "This test-only verifier validates iCKB-style model fixtures only; it does not execute original iCKB scripts or generated CellScript artifacts in CKB VM.",
    })
}

fn verify_fixture_set(dir: &Path, files: &[&str], expected_pass: bool, rows: &mut Vec<Value>, issues: &mut Vec<String>) {
    for file in files {
        match verify_fixture_file(dir, file, expected_pass) {
            Ok(row) => rows.push(row),
            Err(issue) => issues.push(issue),
        }
    }
}

fn verify_fixture_file(dir: &Path, file: &str, expected_pass: bool) -> std::result::Result<Value, String> {
    let path = dir.join(file);
    let content = std::fs::read_to_string(&path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let fixture: Value = serde_json::from_str(&content).map_err(|err| format!("failed to parse {}: {err}", path.display()))?;
    let expected = fixture["expected"].as_str().ok_or_else(|| format!("{file} missing expected field"))?;
    let model_level_only = fixture["model_level_only"].as_bool().unwrap_or(false);
    if !model_level_only {
        return Err(format!("{file} must set model_level_only=true"));
    }
    if expected_pass && expected != "pass" {
        return Err(format!("{file} is in positive fixture set but expected={expected}"));
    }
    if !expected_pass && expected != "fail" {
        return Err(format!("{file} is in negative fixture set but expected={expected}"));
    }

    let verdict = evaluate_fixture(&fixture);
    if expected_pass {
        verdict.map_err(|reason| format!("{file} should pass model verifier, got {reason}"))?;
        Ok(fixture_row(file, &fixture, expected, "pass", None))
    } else {
        let expected_reason = fixture["expected_reason"].as_str().ok_or_else(|| format!("{file} missing expected_reason"))?;
        let actual_reason = match verdict {
            Ok(()) => return Err(format!("{file} should fail model verifier but passed")),
            Err(reason) => reason,
        };
        if actual_reason != expected_reason {
            return Err(format!("{file} expected reason {expected_reason}, got {actual_reason}"));
        }
        Ok(fixture_row(file, &fixture, expected, "fail", Some(actual_reason)))
    }
}

fn fixture_row(file: &str, fixture: &Value, expected: &str, model_status: &str, reason: Option<String>) -> Value {
    serde_json::json!({
        "file": file,
        "scenario": fixture["scenario"].as_str().unwrap_or("unknown"),
        "expected": expected,
        "model_status": model_status,
        "reason": reason,
        "execution_level": "MODEL",
        "ckb_vm_execution": false,
        "model_level_only": fixture["model_level_only"].as_bool().unwrap_or(false),
    })
}

pub fn evaluate_fixture(fixture: &Value) -> std::result::Result<(), String> {
    let data = fixture.get("data").ok_or("missing data")?;
    match str_field(fixture, "scenario")? {
        "deposit_phase_1" => evaluate_deposit_phase_1(data),
        "deposit_phase_2" => evaluate_ickb_accounting(data),
        "transfer" => evaluate_transfer(data),
        "withdrawal" => {
            evaluate_ickb_accounting(data)?;
            if u64_field(data, "current_epoch")? < u64_field(data, "maturity_epoch")? {
                return Err("immature_redeem".to_string());
            }
            Ok(())
        }
        "redeem" => {
            if u64_field(data, "current_epoch")? < u64_field(data, "maturity_epoch")? {
                return Err("immature_redeem".to_string());
            }
            if str_field(data, "owner")? != str_field(data, "claimed_owner")? {
                return Err("wrong_owner".to_string());
            }
            Ok(())
        }
        "limit_order" => evaluate_limit_order(data),
        "owned_owner" => evaluate_owned_owner(data),
        "script_role" => {
            if bool_field(data, "lock_is_ickb_logic")? && bool_field(data, "type_is_ickb_logic")? {
                Err("script_role_confusion".to_string())
            } else {
                Ok(())
            }
        }
        "witness" => {
            if str_field(data, "witness_shape")? != "valid" {
                Err("witness_malformation".to_string())
            } else {
                Ok(())
            }
        }
        "cell_dep" => require_deps(data, &["ickb_logic", "xudt", "dao"]),
        other => Err(format!("unknown scenario {other}")),
    }
}

fn evaluate_deposit_phase_1(data: &Value) -> std::result::Result<(), String> {
    require_deps(data, &["ickb_logic", "dao"])?;
    let deposit_amount = u128_field(data, "deposit_amount")?;
    if !(MIN_DEPOSIT..=MAX_DEPOSIT).contains(&deposit_amount) {
        return Err("capacity_violation".to_string());
    }
    if u64_field(data, "output_cell_count")? > 64 {
        return Err("output_cell_limit".to_string());
    }

    let mut accounting = BTreeMap::<u128, (u128, u128)>::new();
    for deposit in array_field(data, "output_deposits")? {
        let amount = u128_field(deposit, "amount")?;
        if !(MIN_DEPOSIT..=MAX_DEPOSIT).contains(&amount) {
            return Err("capacity_violation".to_string());
        }
        accounting.entry(amount).or_default().0 += 1;
    }
    for receipt in array_field(data, "output_receipts")? {
        let quantity = u128_field(receipt, "quantity")?;
        if quantity == 0 {
            return Err("empty_receipt".to_string());
        }
        accounting.entry(u128_field(receipt, "amount")?).or_default().1 += quantity;
    }

    if accounting.into_values().any(|(deposited, receipted)| deposited != receipted) {
        return Err("receipt_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_ickb_accounting(data: &Value) -> std::result::Result<(), String> {
    if !bool_field(data, "header_deps_present")? {
        return Err("missing_header_dep".to_string());
    }
    if data.get("accumulated_rate_matches_header").is_some_and(|value| value.as_bool() == Some(false)) {
        return Err("wrong_accumulated_rate".to_string());
    }
    require_deps(data, &["ickb_logic", "xudt", "dao"])?;
    if str_field(data, "xudt_binding")? != ICKB_XUDT_BINDING {
        return Err("wrong_xudt_binding".to_string());
    }

    let mut seen_receipts = BTreeSet::new();
    let receipt_total = array_field(data, "input_receipts")?.into_iter().try_fold(0u128, |total, receipt| {
        let id = str_field(receipt, "id")?;
        if !seen_receipts.insert(id.to_string()) {
            return Err("duplicate_receipt".to_string());
        }
        Ok(total + u128_field(receipt, "quantity")? * discounted_ickb_value(receipt)?)
    })?;
    let deposit_total = array_field(data, "input_deposits")?
        .into_iter()
        .map(discounted_ickb_value)
        .try_fold(0u128, |total, value| value.map(|value| total + value))?;

    let left = u128_field(data, "input_udt")? + receipt_total;
    let right = u128_field(data, "output_udt")? + deposit_total;
    if left != right {
        return Err("amount_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_transfer(data: &Value) -> std::result::Result<(), String> {
    if str_field(data, "input_xudt_binding")? != ICKB_XUDT_BINDING || str_field(data, "output_xudt_binding")? != ICKB_XUDT_BINDING {
        return Err("wrong_xudt_binding".to_string());
    }
    if u128_field(data, "input_udt")? != u128_field(data, "output_udt")? {
        return Err("amount_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_limit_order(data: &Value) -> std::result::Result<(), String> {
    if str_field(data, "input_udt_type_hash")? != str_field(data, "output_udt_type_hash")? {
        return Err("wrong_asset".to_string());
    }
    let ckb_mul = u128_field(data, "ckb_multiplier")?;
    let udt_mul = u128_field(data, "udt_multiplier")?;
    let old_value = u128_field(data, "input_ckb")? * ckb_mul + u128_field(data, "input_udt")? * udt_mul;
    let new_value = u128_field(data, "output_ckb")? * ckb_mul + u128_field(data, "output_udt")? * udt_mul;
    if new_value < old_value {
        return Err("limit_order_underpayment".to_string());
    }
    if u128_field(data, "output_ckb")? > 0
        && u128_field(data, "input_ckb")? < u128_field(data, "output_ckb")? + u128_field(data, "ckb_min_match")?
    {
        return Err("insufficient_match".to_string());
    }
    Ok(())
}

fn evaluate_owned_owner(data: &Value) -> std::result::Result<(), String> {
    if !bool_field(data, "empty_args")? {
        return Err("not_empty_args".to_string());
    }
    if !bool_field(data, "withdrawal_request")? {
        return Err("not_withdrawal_request".to_string());
    }
    if str_field(data, "owner")? != str_field(data, "claimed_owner")? {
        return Err("wrong_owner".to_string());
    }
    if u64_field(data, "owned_index")? != u64_field(data, "owner_cell_owned_index")? {
        return Err("owned_owner_mismatch".to_string());
    }
    if i32_field(data, "owner_relative_distance")? != i32_field(data, "owner_cell_relative_distance")? {
        return Err("relative_distance_mismatch".to_string());
    }
    Ok(())
}

fn discounted_ickb_value(cell: &Value) -> std::result::Result<u128, String> {
    let amount = u128_field(cell, "amount")?;
    let ar_m = u128_field(cell, "accumulated_rate")?;
    let raw = amount * AR0 / ar_m;
    if raw > SOFT_CAP {
        Ok(raw - (raw - SOFT_CAP) / 10)
    } else {
        Ok(raw)
    }
}

fn require_deps(data: &Value, required: &[&str]) -> std::result::Result<(), String> {
    let deps = array_field(data, "cell_deps")?
        .into_iter()
        .map(|value| value.as_str().ok_or("cell_dep entry must be string"))
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    if required.iter().all(|dep| deps.contains(dep)) {
        Ok(())
    } else {
        Err("cell_dep_substitution".to_string())
    }
}

fn array_field<'a>(value: &'a Value, key: &str) -> std::result::Result<Vec<&'a Value>, String> {
    Ok(value[key].as_array().ok_or_else(|| format!("missing array field {key}"))?.iter().collect())
}

fn u128_field(value: &Value, key: &str) -> std::result::Result<u128, String> {
    value[key].as_u64().map(u128::from).ok_or_else(|| format!("missing u128-compatible field {key}: {value:#?}"))
}

fn u64_field(value: &Value, key: &str) -> std::result::Result<u64, String> {
    value[key].as_u64().ok_or_else(|| format!("missing u64 field {key}: {value:#?}"))
}

fn i32_field(value: &Value, key: &str) -> std::result::Result<i32, String> {
    value[key].as_i64().and_then(|value| i32::try_from(value).ok()).ok_or_else(|| format!("missing i32 field {key}: {value:#?}"))
}

fn bool_field(value: &Value, key: &str) -> std::result::Result<bool, String> {
    value[key].as_bool().ok_or_else(|| format!("missing bool field {key}: {value:#?}"))
}

fn str_field<'a>(value: &'a Value, key: &str) -> std::result::Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("missing string field {key}: {value:#?}"))
}

use camino::Utf8PathBuf;
use cellscript::{compile_file, CompileOptions};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const AR0: u128 = 10_000_000_000_000_000;
const SOFT_CAP: u128 = 10_000_000_000_000;
const MIN_DEPOSIT: u128 = 100_000_000_000;
const MAX_DEPOSIT: u128 = 100_000_000_000_000;
const ICKB_XUDT_BINDING: &str = "ickb_logic_hash+owner_mode_input_type";
const EXPECTED_LIMITATION_IDS: [&str; 8] = [
    "aggregate-invariant-lowering",
    "header-dep-accumulated-rate",
    "limit-order-metapoint-script-role",
    "limit-order-wide-valuation",
    "oversize-discount-arithmetic",
    "owned-owner-signed-relative-index",
    "script-role-xudt-args",
    "u128-ordering-bounds",
];

const POSITIVE_FIXTURES: [&str; 6] = [
    "valid_deposit_phase_1.json",
    "valid_deposit_phase_2.json",
    "valid_ickb_transfer.json",
    "valid_withdrawal_redeem.json",
    "valid_limit_order_fulfillment.json",
    "valid_owned_owner_unlock.json",
];

const NEGATIVE_FIXTURES: [&str; 15] = [
    "amount_deflation_exact_equality.json",
    "amount_inflation.json",
    "capacity_violation.json",
    "cell_dep_substitution.json",
    "duplicate_receipt_double_mint.json",
    "forged_receipt.json",
    "limit_order_underpayment.json",
    "limit_order_wrong_asset.json",
    "missing_header_dep.json",
    "redeem_before_maturity.json",
    "script_role_confusion.json",
    "witness_malformation.json",
    "wrong_accumulated_rate.json",
    "wrong_owner.json",
    "wrong_xudt_binding.json",
];

#[test]
fn ickb_benchmark_specs_compile_and_expose_expected_entries() {
    let cases = [
        (
            "ickb_logic.cell",
            ["deposit_phase_1", "mint_from_receipt", "transfer_ickb", "request_withdrawal", "redeem_mature"].as_slice(),
        ),
        ("limit_order.cell", ["mint_order", "fulfill_ckb_to_udt", "fulfill_udt_to_ckb", "cancel_order"].as_slice()),
        ("owned_owner.cell", ["mint_owned_owner", "melt_owned_owner"].as_slice()),
    ];

    for (file, actions) in cases {
        let result = compile_file(
            example_path(file),
            CompileOptions {
                target: Some("riscv64-asm".to_string()),
                target_profile: Some("ckb".to_string()),
                ..CompileOptions::default()
            },
        )
        .unwrap_or_else(|err| panic!("{file} should compile: {}", err.message));
        let assembly = std::str::from_utf8(&result.artifact_bytes).expect("assembly utf-8");
        assert!(assembly.contains(".section .text"), "{file} emitted no text section");

        let emitted_actions = result.metadata.actions.iter().map(|action| action.name.as_str()).collect::<BTreeSet<_>>();
        for action in actions {
            assert!(emitted_actions.contains(action), "{file} missing action {action}: {emitted_actions:?}");
            assert!(assembly.contains(&format!(".global {action}")), "{file} missing global symbol for {action}");
        }
    }
}

#[test]
fn ickb_positive_fixtures_pass_model_verifier() {
    for fixture_name in POSITIVE_FIXTURES {
        let fixture = read_fixture("ickb_positive", fixture_name);
        assert_eq!(fixture["expected"], "pass", "{fixture_name}");
        evaluate_fixture(&fixture).unwrap_or_else(|reason| panic!("{fixture_name} should pass, failed with {reason}"));
        assert_eq!(fixture["model_level_only"], true, "{fixture_name} must be labelled honestly");
    }
}

#[test]
fn ickb_negative_fixtures_fail_for_expected_invariant() {
    for fixture_name in NEGATIVE_FIXTURES {
        let fixture = read_fixture("ickb_negative", fixture_name);
        assert_eq!(fixture["expected"], "fail", "{fixture_name}");
        let expected_reason = fixture["expected_reason"].as_str().expect("expected_reason");
        let actual_reason = evaluate_fixture(&fixture).expect_err("negative fixture should fail");
        assert_eq!(actual_reason, expected_reason, "{fixture_name}");
        assert_eq!(fixture["model_level_only"], true, "{fixture_name} must be labelled honestly");
    }
}

#[test]
fn ickb_diff_matrix_is_partial_and_consistent_with_model_fixtures() {
    let matrix = read_fixture("ickb_diff", "matrix.json");
    assert_eq!(matrix["schema"], "cellscript-ickb-diff-matrix-v0");
    assert_eq!(matrix["mode"], "MODEL_LEVEL_ONLY");
    let rows = matrix["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 9);
    for row in rows {
        assert!(row["result"].as_str().is_some_and(|result| result.starts_with("model-")), "{row:#?}");
        assert!(row["original_ickb_expected"].as_str().is_some(), "{row:#?}");
        assert!(row["cellscript_expected"].as_str().is_some(), "{row:#?}");
    }
}

#[test]
fn ickb_limitations_manifest_matches_source_markers_and_model_fixtures() {
    let manifest = read_example_json("limitations.json");
    assert_eq!(manifest["schema"], "cellscript-ickb-benchmark-limitations-v0");
    assert_eq!(manifest["status"], "model_level_only_known_gaps");

    let limitations = manifest["limitations"].as_array().expect("limitations");
    let manifest_ids = limitations.iter().map(|entry| str_field(entry, "id").to_string()).collect::<BTreeSet<_>>();
    let expected_ids = EXPECTED_LIMITATION_IDS.iter().map(|id| (*id).to_string()).collect::<BTreeSet<_>>();
    assert_eq!(manifest_ids, expected_ids);

    let source_files = ["README.md", "ickb_logic.cell", "limit_order.cell", "owned_owner.cell"];
    let mut source_marker_ids = BTreeSet::new();
    for file in source_files {
        let source = std::fs::read_to_string(example_path(file)).unwrap_or_else(|err| panic!("failed to read {file}: {err}"));
        assert!(!source.contains("TODO(ickb-benchmark)"), "{file} still has stale TODO markers");
        if file.ends_with(".cell") {
            source_marker_ids.extend(ickb_limitation_ids(&source));
        }
    }
    assert_eq!(source_marker_ids, expected_ids);

    for entry in limitations {
        let id = str_field(entry, "id");
        assert!(!str_field(entry, "requires_compiler_feature").is_empty(), "{id} must name the missing compiler/runtime feature");
        assert!(!str_field(entry, "production_readiness_impact").is_empty(), "{id} must state production-readiness impact");
        for source_file in array_field(entry, "source_files") {
            let source_file = source_file.as_str().expect("source file");
            let source = std::fs::read_to_string(example_path(source_file))
                .unwrap_or_else(|err| panic!("failed to read limitation source {source_file}: {err}"));
            assert!(source.contains(&format!("LIMITATION(ickb-benchmark:{id})")), "{id} missing source marker in {source_file}");
        }
        let fixtures = array_field(entry, "model_fixtures");
        assert!(!fixtures.is_empty(), "{id} must cite at least one model fixture");
        for fixture in fixtures {
            let fixture = fixture.as_str().expect("fixture path");
            let payload = read_fixture_path(fixture);
            assert_eq!(payload["model_level_only"], true, "{id} fixture {fixture} must stay honestly labelled");
        }
    }
}

fn example_path(file: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("ickb_benchmark").join(file)
}

fn read_example_json(file: &str) -> Value {
    let path = example_path(file);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn read_fixture(dir: &str, file: &str) -> Value {
    let path = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("benchmarks").join(dir).join(file);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn read_fixture_path(path: &str) -> Value {
    let (dir, file) = path.split_once('/').unwrap_or_else(|| panic!("fixture path must be dir/file: {path}"));
    read_fixture(dir, file)
}

fn ickb_limitation_ids(source: &str) -> Vec<String> {
    let prefix = "LIMITATION(ickb-benchmark:";
    source
        .match_indices(prefix)
        .map(|(index, _)| {
            let rest = &source[index + prefix.len()..];
            rest.split_once(')')
                .map(|(id, _)| id.to_string())
                .unwrap_or_else(|| panic!("unterminated iCKB limitation marker near {rest}"))
        })
        .collect()
}

fn evaluate_fixture(fixture: &Value) -> Result<(), String> {
    let data = &fixture["data"];
    match fixture["scenario"].as_str().expect("scenario") {
        "deposit_phase_1" => evaluate_deposit_phase_1(data),
        "deposit_phase_2" => evaluate_ickb_accounting(data),
        "transfer" => evaluate_transfer(data),
        "withdrawal" => {
            evaluate_ickb_accounting(data)?;
            if u64_field(data, "current_epoch") < u64_field(data, "maturity_epoch") {
                return Err("immature_redeem".to_string());
            }
            Ok(())
        }
        "redeem" => {
            if u64_field(data, "current_epoch") < u64_field(data, "maturity_epoch") {
                return Err("immature_redeem".to_string());
            }
            if str_field(data, "owner") != str_field(data, "claimed_owner") {
                return Err("wrong_owner".to_string());
            }
            Ok(())
        }
        "limit_order" => evaluate_limit_order(data),
        "owned_owner" => evaluate_owned_owner(data),
        "script_role" => {
            if bool_field(data, "lock_is_ickb_logic") && bool_field(data, "type_is_ickb_logic") {
                Err("script_role_confusion".to_string())
            } else {
                Ok(())
            }
        }
        "witness" => {
            if str_field(data, "witness_shape") != "valid" {
                Err("witness_malformation".to_string())
            } else {
                Ok(())
            }
        }
        "cell_dep" => require_deps(data, &["ickb_logic", "xudt", "dao"]),
        other => panic!("unknown scenario {other}"),
    }
}

fn evaluate_deposit_phase_1(data: &Value) -> Result<(), String> {
    require_deps(data, &["ickb_logic", "dao"])?;
    if u128_field(data, "deposit_amount") < MIN_DEPOSIT || u128_field(data, "deposit_amount") > MAX_DEPOSIT {
        return Err("capacity_violation".to_string());
    }
    if u64_field(data, "output_cell_count") > 64 {
        return Err("output_cell_limit".to_string());
    }

    let mut accounting = BTreeMap::<u128, (u128, u128)>::new();
    for deposit in array_field(data, "output_deposits") {
        let amount = u128_field(deposit, "amount");
        if !(MIN_DEPOSIT..=MAX_DEPOSIT).contains(&amount) {
            return Err("capacity_violation".to_string());
        }
        accounting.entry(amount).or_default().0 += 1;
    }
    for receipt in array_field(data, "output_receipts") {
        let quantity = u128_field(receipt, "quantity");
        if quantity == 0 {
            return Err("empty_receipt".to_string());
        }
        accounting.entry(u128_field(receipt, "amount")).or_default().1 += quantity;
    }

    if accounting.into_values().any(|(deposited, receipted)| deposited != receipted) {
        return Err("receipt_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_ickb_accounting(data: &Value) -> Result<(), String> {
    if !bool_field(data, "header_deps_present") {
        return Err("missing_header_dep".to_string());
    }
    if data.get("accumulated_rate_matches_header").is_some_and(|value| value.as_bool() == Some(false)) {
        return Err("wrong_accumulated_rate".to_string());
    }
    require_deps(data, &["ickb_logic", "xudt", "dao"])?;
    if str_field(data, "xudt_binding") != ICKB_XUDT_BINDING {
        return Err("wrong_xudt_binding".to_string());
    }

    let mut seen_receipts = BTreeSet::new();
    let receipt_total = array_field(data, "input_receipts").into_iter().try_fold(0u128, |total, receipt| {
        let id = str_field(receipt, "id");
        if !seen_receipts.insert(id.to_string()) {
            return Err("duplicate_receipt".to_string());
        }
        Ok(total + u128_field(receipt, "quantity") * discounted_ickb_value(receipt))
    })?;
    let deposit_total = array_field(data, "input_deposits").into_iter().map(discounted_ickb_value).sum::<u128>();

    let left = u128_field(data, "input_udt") + receipt_total;
    let right = u128_field(data, "output_udt") + deposit_total;
    if left != right {
        return Err("amount_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_transfer(data: &Value) -> Result<(), String> {
    if str_field(data, "input_xudt_binding") != ICKB_XUDT_BINDING || str_field(data, "output_xudt_binding") != ICKB_XUDT_BINDING {
        return Err("wrong_xudt_binding".to_string());
    }
    if u128_field(data, "input_udt") != u128_field(data, "output_udt") {
        return Err("amount_mismatch".to_string());
    }
    Ok(())
}

fn evaluate_limit_order(data: &Value) -> Result<(), String> {
    if str_field(data, "input_udt_type_hash") != str_field(data, "output_udt_type_hash") {
        return Err("wrong_asset".to_string());
    }
    let ckb_mul = u128_field(data, "ckb_multiplier");
    let udt_mul = u128_field(data, "udt_multiplier");
    let old_value = u128_field(data, "input_ckb") * ckb_mul + u128_field(data, "input_udt") * udt_mul;
    let new_value = u128_field(data, "output_ckb") * ckb_mul + u128_field(data, "output_udt") * udt_mul;
    if new_value < old_value {
        return Err("limit_order_underpayment".to_string());
    }
    if u128_field(data, "output_ckb") > 0
        && u128_field(data, "input_ckb") < u128_field(data, "output_ckb") + u128_field(data, "ckb_min_match")
    {
        return Err("insufficient_match".to_string());
    }
    Ok(())
}

fn evaluate_owned_owner(data: &Value) -> Result<(), String> {
    if !bool_field(data, "empty_args") {
        return Err("not_empty_args".to_string());
    }
    if !bool_field(data, "withdrawal_request") {
        return Err("not_withdrawal_request".to_string());
    }
    if str_field(data, "owner") != str_field(data, "claimed_owner") {
        return Err("wrong_owner".to_string());
    }
    if u64_field(data, "owned_index") != u64_field(data, "owner_cell_owned_index") {
        return Err("owned_owner_mismatch".to_string());
    }
    Ok(())
}

fn discounted_ickb_value(cell: &Value) -> u128 {
    let amount = u128_field(cell, "amount");
    let ar_m = u128_field(cell, "accumulated_rate");
    let raw = amount * AR0 / ar_m;
    if raw > SOFT_CAP {
        raw - (raw - SOFT_CAP) / 10
    } else {
        raw
    }
}

fn require_deps(data: &Value, required: &[&str]) -> Result<(), String> {
    let deps = array_field(data, "cell_deps").into_iter().map(|value| value.as_str().expect("dep string")).collect::<BTreeSet<_>>();
    if required.iter().all(|dep| deps.contains(dep)) {
        Ok(())
    } else {
        Err("cell_dep_substitution".to_string())
    }
}

fn array_field<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value[key].as_array().unwrap_or_else(|| panic!("missing array field {key}")).iter().collect()
}

fn u128_field(value: &Value, key: &str) -> u128 {
    value[key].as_u64().unwrap_or_else(|| panic!("missing u128-compatible field {key}: {value:#?}")) as u128
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or_else(|| panic!("missing u64 field {key}: {value:#?}"))
}

fn bool_field(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or_else(|| panic!("missing bool field {key}: {value:#?}"))
}

fn str_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_else(|| panic!("missing string field {key}: {value:#?}"))
}

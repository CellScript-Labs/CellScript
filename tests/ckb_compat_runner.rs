use camino::Utf8PathBuf;
use serde_json::Value;
use std::collections::BTreeSet;

#[test]
fn ckb_standard_compat_runner_executes_declared_fixture_verdicts() {
    let manifest = read_json("manifest.json");
    assert_eq!(manifest["schema"], "cellscript-ckb-standard-compat-v0.16");

    let mut executed = Vec::new();
    for suite in manifest["suites"].as_array().expect("manifest suites") {
        let suite_name = suite["name"].as_str().expect("suite name");
        let accepted = names(suite, "accepted_fixtures");
        let rejected = names(suite, "rejected_fixtures");
        let files = suite["fixture_files"].as_object().expect("fixture_files");

        for fixture_name in accepted.iter().chain(rejected.iter()) {
            let file = files[*fixture_name].as_str().unwrap_or_else(|| panic!("missing fixture file for {fixture_name}"));
            let fixture = read_json(file);
            assert_eq!(fixture["suite"], suite_name, "{file}");
            assert_eq!(fixture["fixture_name"], *fixture_name, "{file}");
            let verdict = run_fixture_model(&fixture).unwrap_or_else(|err| panic!("{file} runner error: {err}"));

            if accepted.contains(fixture_name) {
                assert_eq!(verdict.exit_code, 0, "{file}");
                assert_eq!(fixture["status"], "accepted", "{file}");
            } else {
                assert_ne!(verdict.exit_code, 0, "{file}");
                assert_eq!(fixture["status"], "rejected", "{file}");
                assert!(verdict.reason.is_some(), "{file} rejection must be tied to a named reason");
            }
            executed.push(format!("{suite_name}:{fixture_name}"));
        }
    }

    assert!(executed.len() >= 14, "expected all CKB compat fixtures to run, got {executed:?}");
}

#[derive(Debug, PartialEq, Eq)]
struct FixtureVerdict {
    exit_code: i64,
    reason: Option<String>,
}

fn run_fixture_model(fixture: &Value) -> Result<FixtureVerdict, String> {
    let shape = &fixture["transaction_shape"];
    require_array(shape, "inputs")?;
    require_array(shape, "outputs")?;
    require_array(shape, "cell_deps")?;
    let expected = fixture["expected_behavior"].as_object().ok_or("missing expected_behavior")?;
    let exit_code = expected.get("script_exit_code").and_then(Value::as_i64).ok_or("missing script_exit_code")?;
    let reason = expected.get("rejection_reason").and_then(Value::as_str).map(str::to_string);

    if fixture["status"] == "rejected" && reason.is_none() {
        return Err("rejected fixture lacks expected_behavior.rejection_reason".to_string());
    }
    if fixture["status"] == "accepted" && exit_code != 0 {
        return Err("accepted fixture has non-zero expected exit code".to_string());
    }
    if fixture["status"] == "rejected" && exit_code == 0 {
        return Err("rejected fixture has zero expected exit code".to_string());
    }

    let metadata = fixture["metadata_expectation"].as_object().ok_or("missing metadata_expectation")?;
    let proof_plan = metadata.get("proof_plan").and_then(Value::as_object).ok_or("missing proof_plan expectation")?;
    for key in ["trigger", "scope", "reads", "coverage", "on_chain_checked"] {
        if !proof_plan.contains_key(key) {
            return Err(format!("proof_plan expectation missing {key}"));
        }
    }
    if !metadata.contains_key("codegen_coverage_status") {
        return Err("metadata expectation missing codegen_coverage_status".to_string());
    }
    if fixture.get("cycle_report").is_none() || fixture.get("capacity_report").is_none() {
        return Err("fixture must carry cycle and capacity reports".to_string());
    }

    let verdict = evaluate_fixture_semantics(fixture)?;
    if verdict.exit_code != exit_code {
        return Err(format!("fixture semantic model exit {} disagrees with expected exit {}", verdict.exit_code, exit_code));
    }
    if exit_code != 0 && verdict.reason.is_none() {
        return Err("semantic model rejected without a named invariant reason".to_string());
    }
    if let Some(expected_reason) = &reason {
        if verdict.reason.as_deref() != Some(expected_reason.as_str()) {
            return Err(format!(
                "fixture semantic model reason {:?} disagrees with expected reason {:?}",
                verdict.reason, expected_reason
            ));
        }
    }

    Ok(FixtureVerdict { exit_code, reason: reason.or(verdict.reason) })
}

fn evaluate_fixture_semantics(fixture: &Value) -> Result<FixtureVerdict, String> {
    let shape = &fixture["transaction_shape"];
    validate_capacity_report(fixture)?;
    match fixture["suite"].as_str().ok_or("missing suite")? {
        "sudt" => evaluate_amount_conservation(shape, "sudt-cell", "output_amount > input_amount; conservation violated"),
        "xudt" => {
            if any_cell_dep_name(shape, "lockup") || any_input_witness(shape, "lockup-active") {
                return Ok(reject("extension_policy_violated: lockup period not expired"));
            }
            evaluate_amount_conservation(shape, "xudt-cell", "output_amount > input_amount; conservation violated")
        }
        "acp" => {
            let first_input = first_cell(shape, "inputs")?;
            let first_output = first_cell(shape, "outputs")?;
            if cell_str(first_input, "witness").contains("wrong")
                || cell_str(first_input, "lock_script") != cell_str(first_output, "lock_script")
            {
                return Ok(reject("witness_lock_hash != args_owner_lock_hash"));
            }
            Ok(pass())
        }
        "cheque" => {
            let first_input = first_cell(shape, "inputs")?;
            let first_output = first_cell(shape, "outputs")?;
            if cell_str(first_input, "witness").contains("wrong") || cell_str(first_output, "lock_script").contains("wrong") {
                return Ok(reject("receiver_lock_hash != args_receiver_hash"));
            }
            Ok(pass())
        }
        "omnilock" => {
            if any_input_witness(shape, "invalid") {
                Ok(reject("auth_verification_failed: invalid_signature_or_wrong_method"))
            } else {
                Ok(pass())
            }
        }
        "nervosdao-since" => {
            if shape["header_deps"].as_array().into_iter().flatten().any(|header| header.as_str() == Some("mature-epoch-header")) {
                Ok(pass())
            } else {
                Ok(reject("since_not_mature: current_epoch < required_epoch"))
            }
        }
        "type-id" => {
            let type_id_outputs = shape["outputs"]
                .as_array()
                .expect("outputs")
                .iter()
                .filter(|output| cell_str(output, "type_script").starts_with("type-id-script"))
                .count();
            if type_id_outputs > 1 {
                Ok(reject("duplicate_type_id: at-most-one-input-and-one-output-per-type-id-group"))
            } else {
                Ok(pass())
            }
        }
        other => Err(format!("unsupported compat fixture suite {other}")),
    }
}

fn evaluate_amount_conservation(shape: &Value, cell_type: &str, reason: &str) -> Result<FixtureVerdict, String> {
    let input_sum = amount_sum(shape, "inputs", cell_type)?;
    let output_sum = amount_sum(shape, "outputs", cell_type)?;
    if output_sum > input_sum {
        Ok(reject(reason))
    } else {
        Ok(pass())
    }
}

fn amount_sum(shape: &Value, side: &str, cell_type: &str) -> Result<u128, String> {
    shape[side]
        .as_array()
        .ok_or_else(|| format!("missing transaction_shape.{side}"))?
        .iter()
        .filter(|cell| cell_str(cell, "type") == cell_type)
        .try_fold(0u128, |total, cell| Ok(total + little_endian_u128(cell_str(cell, "data"))?))
}

fn little_endian_u128(hex_value: &str) -> Result<u128, String> {
    let bytes = hex_value.strip_prefix("0x").unwrap_or(hex_value);
    if bytes.is_empty() {
        return Ok(0);
    }
    if bytes.len() % 2 != 0 {
        return Err(format!("odd-length hex amount {hex_value}"));
    }
    let raw = hex::decode(bytes).map_err(|err| format!("invalid hex amount {hex_value}: {err}"))?;
    if raw.len() > 16 {
        return Err(format!("amount data exceeds u128 width: {} bytes", raw.len()));
    }
    let mut padded = [0u8; 16];
    padded[..raw.len()].copy_from_slice(&raw);
    Ok(u128::from_le_bytes(padded))
}

fn validate_capacity_report(fixture: &Value) -> Result<(), String> {
    let reported = fixture["capacity_report"]["occupied_capacity_shannons"]
        .as_u64()
        .ok_or("capacity_report missing occupied_capacity_shannons")?;
    let output_capacity = fixture["transaction_shape"]["outputs"]
        .as_array()
        .ok_or("missing outputs")?
        .iter()
        .map(|output| output["capacity_shannons"].as_u64().ok_or("output missing capacity_shannons"))
        .try_fold(0u64, |total, value| value.map(|value| total.saturating_add(value)))?;
    if reported > output_capacity {
        return Err(format!("capacity report occupied capacity {} exceeds output capacity {}", reported, output_capacity));
    }
    Ok(())
}

fn pass() -> FixtureVerdict {
    FixtureVerdict { exit_code: 0, reason: None }
}

fn reject(reason: &str) -> FixtureVerdict {
    FixtureVerdict { exit_code: 1, reason: Some(reason.to_string()) }
}

fn first_cell<'a>(shape: &'a Value, side: &str) -> Result<&'a Value, String> {
    shape[side].as_array().and_then(|cells| cells.first()).ok_or_else(|| format!("missing first transaction_shape.{side} cell"))
}

fn cell_str<'a>(cell: &'a Value, field: &str) -> &'a str {
    cell[field].as_str().unwrap_or("")
}

fn any_cell_dep_name(shape: &Value, needle: &str) -> bool {
    shape["cell_deps"].as_array().into_iter().flatten().any(|dep| dep["name"].as_str().is_some_and(|name| name.contains(needle)))
}

fn any_input_witness(shape: &Value, needle: &str) -> bool {
    shape["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|input| input["witness"].as_str().is_some_and(|witness| witness.contains(needle)))
}

fn names<'a>(suite: &'a Value, key: &str) -> BTreeSet<&'a str> {
    suite[key].as_array().expect(key).iter().map(|value| value.as_str().expect("fixture name")).collect()
}

fn require_array(value: &Value, key: &str) -> Result<(), String> {
    value[key].as_array().map(|_| ()).ok_or_else(|| format!("missing transaction_shape.{key}"))
}

fn read_json(file: &str) -> Value {
    let path = fixture_dir().join(file);
    let content = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("failed to read {path}: {err}"));
    serde_json::from_str(&content).unwrap_or_else(|err| panic!("failed to parse {path}: {err}"))
}

fn fixture_dir() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("compat").join("ckb_standard")
}

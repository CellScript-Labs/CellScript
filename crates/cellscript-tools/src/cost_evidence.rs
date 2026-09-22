//! Cost-report v2 consumer. A success marker alone is never measurement evidence.

use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("read {}", path.display()))?)
        .with_context(|| format!("parse {}", path.display()))
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field].as_array().with_context(|| format!("missing array {field}"))
}

fn positive(value: &Value) -> Result<u64> {
    let number = value.as_u64().context("missing unsigned measurement")?;
    ensure!(number > 0, "zero is not available measurement evidence");
    Ok(number)
}

fn measurement(value: &Value, group: bool) -> Result<Option<u64>> {
    let expected_scope = if group { "script_group_scheduler_including_children" } else { "transaction_scripts" };
    ensure!(value["scope"] == expected_scope, "wrong measurement scope");
    if group {
        ensure!(matches!(value["group"]["role"].as_str(), Some("lock" | "type")), "missing group role");
        let hash = value["group"]["script_hash"].as_str().context("missing group hash")?;
        ensure!(hash.len() == 66 && hash.starts_with("0x") && hex::decode(&hash[2..]).is_ok(), "invalid group hash");
        let inputs = array(&value["group"], "input_indices")?;
        let outputs = array(&value["group"], "output_indices")?;
        ensure!(!inputs.is_empty() || !outputs.is_empty(), "empty Script group");
        ensure!(inputs.iter().chain(outputs).all(|index| index.as_u64().is_some()), "invalid group membership");
    } else {
        ensure!(value["group"].is_null(), "transaction scope has a selected group");
    }
    let observation = &value["observation"];
    match observation["status"].as_str() {
        Some("measured") => {
            let code = observation["exit_code"].as_i64().context("missing observed exit code")?;
            ensure!((-128..=127).contains(&code), "exit code outside CKB i8 range");
            ensure!(value["exit_category"] == if code == 0 { "success" } else { "nonzero_exit" }, "exit category mismatch");
            ensure!(group || code == 0, "ordinary verifier cannot measure rejected transaction cycles");
            Ok(Some(positive(&observation["cycles"])?))
        }
        Some("unavailable") => {
            ensure!(!group && value["exit_category"] == "transaction_rejected", "required group measurement unavailable");
            ensure!(observation["cycles"].is_null() && observation["exit_code"].is_null(), "unavailable cycles contain a placeholder");
            ensure!(!observation["reason"].as_str().unwrap_or("").is_empty(), "missing unavailability reason");
            Ok(None)
        }
        _ => bail!("unsupported measurement availability"),
    }
}

fn rows_with_budgets(root: &Path, report: &Value, field: &str, budget_file: &str) -> Result<()> {
    let rows = array(report, field)?;
    let budgets = read(&root.join("tests/fixtures/cost_corpus").join(budget_file))?;
    let budgets = budgets.as_object().context("budget map")?;
    ensure!(rows.len() == budgets.len(), "{field}: fixture count changed");
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = row["name"].as_str().context("fixture name")?;
        ensure!(seen.insert(name), "duplicate fixture {name}");
        let budget = budgets.get(name).with_context(|| format!("unbudgeted fixture {name}"))?;
        for metric in [
            "elf_bytes",
            "positive_cycles",
            "group_positive_cycles",
            "group_rejection_cycles",
            "max_stack_frame_bytes",
            "static_call_chain_stack_bound_bytes",
        ] {
            ensure!(positive(&row[metric])? <= positive(&budget[metric])?, "{name}: {metric} exceeds ceiling");
        }
        if !budget["witness_bytes"].is_null() {
            ensure!(row["witness_bytes"] == budget["witness_bytes"], "{name}: witness ABI changed");
        }
        for hash in ["source_sha256", "elf_sha256"] {
            let hash = row[hash].as_str().context("missing fixture hash")?;
            ensure!(hash.len() == 64 && hex::decode(hash).is_ok(), "invalid fixture hash");
        }
    }
    Ok(())
}

fn same_source(left: &Value, right: &Value) -> Result<()> {
    for field in [
        "compiler_version",
        "source_commit",
        "source_dirty",
        "tracked_diff_sha256",
        "untracked_source_sha256",
        "cargo_lock_sha256",
        "rust_toolchain",
        "vm_configuration",
    ] {
        ensure!(!left[field].is_null() && left[field] == right[field], "cost report source/configuration mismatch: {field}");
    }
    // The matched corpus deliberately uses Edition 2027 / -O3, while the
    // existing multi-Script anchor retains its default Edition 2026 profile.
    // Those profiles are explicit, rather than incorrectly equated here.
    for value in [left, right] {
        ensure!(matches!(value["edition"].as_str(), Some("2026" | "2027")), "missing cost-report edition");
        ensure!(value["opt_level"].as_u64().is_some_and(|level| level <= 3), "missing cost-report optimization profile");
        ensure!(value["target"] == "riscv64-elf" && value["target_profile"] == "ckb", "wrong cost-report target");
    }
    Ok(())
}

pub fn check(root: &Path, path: &Path, multi_path: &Path) -> Result<()> {
    let report = read(path)?;
    ensure!(report["schema"] == "cellscript-cost-corpus-v2" && report["status"] == "passed", "missing passing cost-report v2");
    ensure!(array(&report, "matched")?.len() == 3 && array(&report, "growth")?.len() == 18, "legacy corpus incomplete");
    let growth_budgets = read(&root.join("tests/fixtures/cost_corpus/growth_budgets.json"))?;
    let mut growth_names = BTreeSet::new();
    for row in array(&report, "growth")? {
        let name = row["name"].as_str().context("missing growth identity")?;
        ensure!(growth_names.insert(name) && growth_budgets[name].is_object(), "duplicate or unbudgeted growth row");
        for metric in ["elf_bytes", "positive_cycles", "max_stack_frame_bytes"] {
            ensure!(positive(&row[metric])? <= positive(&growth_budgets[name][metric])?, "growth {name} {metric} regression");
        }
        ensure!(row["witness_bytes"] == growth_budgets[name]["witness_bytes"], "growth witness ABI changed");
        ensure!(row["static_stack"]["status"] == "bounded", "required growth stack bound unavailable");
        positive(&row["static_stack"]["static_call_chain_stack_bound_bytes"])?;
    }
    let matched_names: BTreeSet<_> = array(&report, "matched")?.iter().filter_map(|row| row["name"].as_str()).collect();
    ensure!(matched_names == BTreeSet::from(["pool-merge", "schema-roll", "nft-lock"]), "matched identities changed");
    for row in array(&report, "matched")? {
        for (metric, reference, ceiling) in
            [("elf_bytes", "rust_elf_bytes", "elf_budget"), ("positive_cycles", "rust_positive_cycles", "cycle_budget")]
        {
            let observed = positive(&row[metric])?;
            ensure!(observed <= positive(&row[reference])? && observed <= positive(&row[ceiling])?, "matched sample regression");
        }
    }
    for field in
        ["source_commit", "cargo_lock_sha256", "rust_reference_lock_sha256", "rust_toolchain", "strip_sha256", "strip_version"]
    {
        ensure!(!report["provenance"][field].as_str().unwrap_or("").is_empty(), "missing provenance {field}");
    }
    ensure!(report["provenance"]["source_dirty"].is_boolean(), "missing dirty-state declaration");
    rows_with_budgets(root, &report, "expanded", "expanded_budgets.json")?;
    rows_with_budgets(root, &report, "scalar", "scalar_budgets.json")?;
    let mut names = BTreeSet::new();
    let executions = array(&report, "executions")?;
    ensure!(executions.len() >= 1800, "execution sweep incomplete");
    for execution in executions {
        let name = execution["name"].as_str().context("missing execution identity")?;
        ensure!(names.insert(name), "duplicate execution identity {name}");
        measurement(&execution["group"], true)?;
        let transaction = measurement(&execution["transaction"], false)?;
        let code = execution["oracle_exit_code"].as_i64().context("missing oracle outcome")?;
        ensure!(execution["group"]["observation"]["exit_code"].as_i64() == Some(code), "group/oracle mismatch");
        ensure!(transaction.is_some() == (code == 0), "transaction availability/oracle mismatch");
    }
    let multi = read(multi_path)?;
    ensure!(multi["schema"] == "cellscript-multi-script-cost-v2" && multi["status"] == "passed", "missing multi-Script v2 evidence");
    same_source(&report["provenance"], &multi["provenance"])?;
    let rows = array(&multi, "rows")?;
    let budgets = read(&root.join("tests/fixtures/cost_corpus/multi_script_budgets.json"))?;
    ensure!(rows.len() == 6, "multi-Script rejection sweep incomplete");
    let mut names = BTreeSet::new();
    for row in rows {
        let name = row["name"].as_str().context("missing multi-Script identity")?;
        ensure!(names.insert(name) && budgets[name].is_object(), "duplicate or unbudgeted multi-Script fixture");
        let groups = array(row, "groups")?;
        ensure!(groups.len() == 5, "canonical anchor must execute five Script groups");
        let mut total = 0;
        let mut group_keys = BTreeSet::new();
        let mut all_succeeded = true;
        for group in groups {
            let cycles = measurement(group, true)?.context("required group cycles")?;
            let key = format!("{}:{}:{}", group["group"]["role"], group["group"]["input_indices"], group["group"]["output_indices"]);
            ensure!(group_keys.insert(key.clone()), "duplicate Script group");
            all_succeeded &= group["exit_category"] == "success";
            ensure!(cycles <= positive(&budgets[name]["groups"][&key])?, "multi-Script group regression");
            total += cycles;
        }
        let transaction = measurement(&row["transaction"], false)?;
        ensure!(transaction.is_some() == all_succeeded, "multi-Script outcomes disagree with the transaction oracle");
        if let Some(cycles) = transaction {
            ensure!(total == cycles, "transaction total differs from group sum");
        }
        for metric in ["elf_bytes", "max_stack_frame_bytes", "transaction_bytes", "witness_bytes"] {
            ensure!(positive(&row[metric])? <= positive(&budgets[name][metric])?, "multi-Script {metric} regression");
        }
        let stacks = array(row, "static_stacks")?;
        ensure!(stacks.len() == 4, "missing artifact stack accounting");
        let mut stack_owners = BTreeSet::new();
        for stack in stacks {
            let artifact = stack["name"].as_str().context("missing stack owner")?;
            ensure!(stack_owners.insert(artifact), "duplicate artifact stack");
            ensure!(stack["bound"]["status"] == "bounded", "required stack bound unavailable");
            ensure!(
                positive(&stack["bound"]["static_call_chain_stack_bound_bytes"])?
                    <= positive(&budgets[name]["static_stacks"][artifact])?,
                "multi-Script stack regression"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn companion_reports_require_the_same_source_but_allow_declared_profiles() {
        let source = json!({
            "compiler_version":"0.30.0", "source_commit":"commit",
            "source_dirty":true, "tracked_diff_sha256":"tracked",
            "untracked_source_sha256":"untracked", "cargo_lock_sha256":"lock",
            "rust_toolchain":"rustc", "vm_configuration":{"limit":10_000_000},
            "edition":"2027", "opt_level":3, "target":"riscv64-elf", "target_profile":"ckb"
        });
        let mut companion = source.clone();
        companion["edition"] = json!("2026");
        companion["opt_level"] = json!(0);
        same_source(&source, &companion).unwrap();
        for field in [
            "compiler_version",
            "source_commit",
            "source_dirty",
            "tracked_diff_sha256",
            "untracked_source_sha256",
            "cargo_lock_sha256",
            "rust_toolchain",
            "vm_configuration",
        ] {
            let mut changed = companion.clone();
            changed[field] = json!("different");
            assert!(same_source(&source, &changed).is_err(), "accepted changed {field}");
            changed.as_object_mut().unwrap().remove(field);
            assert!(same_source(&source, &changed).is_err(), "accepted missing {field}");
        }
        for (field, invalid) in
            [("edition", json!("2028")), ("opt_level", json!(4)), ("target", json!("assembly")), ("target_profile", json!("unknown"))]
        {
            let mut changed = companion.clone();
            changed[field] = invalid;
            assert!(same_source(&source, &changed).is_err(), "accepted invalid {field}");
        }
    }

    #[test]
    fn unavailable_cycles_cannot_be_used_as_zero_or_as_a_required_measurement() {
        let mut value = json!({"scope":"transaction_scripts","group":null,"exit_category":"transaction_rejected", "observation":{"status":"unavailable","reason":"ordinary verifier rejected"}});
        assert_eq!(measurement(&value, false).unwrap(), None);
        value["observation"]["cycles"] = json!(0);
        assert!(measurement(&value, false).is_err());
        value["observation"] = json!({"status":"measured","cycles":0,"exit_code":0});
        value["exit_category"] = json!("success");
        assert!(measurement(&value, false).is_err());
        value["observation"]["cycles"] = json!(42);
        assert_eq!(measurement(&value, false).unwrap(), Some(42));
        value["exit_category"] = json!("vm_trap");
        assert!(measurement(&value, false).is_err());
    }
}

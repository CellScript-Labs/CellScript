//! Reproducible, per-metric comparison of two complete cost-report pairs.

use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn read(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("parse {}", path.display()))
}

fn named<'a>(value: &'a Value, field: &str) -> Result<BTreeMap<String, &'a Value>> {
    let mut rows = BTreeMap::new();
    for row in value[field].as_array().with_context(|| format!("missing {field}"))? {
        let name = row["name"].as_str().context("missing row name")?;
        ensure!(rows.insert(name.to_owned(), row).is_none(), "duplicate {field}/{name}");
    }
    Ok(rows)
}

#[derive(Default)]
struct Comparison {
    count: usize,
    regressions: Vec<Value>,
    scalar: Vec<Value>,
}

impl Comparison {
    fn metric(&mut self, name: &str, field: &str, before: &Value, after: &Value, exact: bool) -> Result<()> {
        let left = before.as_u64().with_context(|| format!("missing baseline {name}/{field}"))?;
        let right = after.as_u64().with_context(|| format!("missing candidate {name}/{field}"))?;
        self.count += 1;
        if if exact { left != right } else { right > left } {
            self.regressions.push(json!({"name":name,"metric":field,"before":left,"after":right,"exact":exact}));
        }
        Ok(())
    }

    fn observations(&mut self, name: &str, before: &Value, after: &Value) -> Result<()> {
        ensure!(before["scope"] == after["scope"], "{name}: measurement scope changed");
        ensure!(before["exit_category"] == after["exit_category"], "{name}: exit category changed");
        let left = &before["observation"];
        let right = &after["observation"];
        ensure!(left["status"] == right["status"] && left["exit_code"] == right["exit_code"], "{name}: outcome changed");
        if left["status"] == "measured" {
            self.metric(name, "cycles", &left["cycles"], &right["cycles"], false)?;
        }
        Ok(())
    }

    fn corpus(&mut self, before: &Value, after: &Value) -> Result<()> {
        for family in ["matched", "growth", "expanded", "scalar", "executions"] {
            let left = named(before, family)?;
            let right = named(after, family)?;
            ensure!(left.keys().eq(right.keys()), "{family}: fixture set changed");
            for (name, row) in left {
                let candidate = right[&name];
                ensure!(row["source_sha256"] == candidate["source_sha256"], "{name}: fixture source changed");
                for field in [
                    "elf_bytes",
                    "positive_cycles",
                    "group_positive_cycles",
                    "group_rejection_cycles",
                    "max_stack_frame_bytes",
                    "static_call_chain_stack_bound_bytes",
                    "witness_bytes",
                    "transaction_bytes",
                ] {
                    if !row[field].is_null() || !candidate[field].is_null() {
                        self.metric(
                            &name,
                            field,
                            &row[field],
                            &candidate[field],
                            matches!(field, "witness_bytes" | "transaction_bytes"),
                        )?;
                    }
                }
                if family == "matched" {
                    for field in ["rust_elf_bytes", "rust_positive_cycles", "elf_budget", "cycle_budget"] {
                        ensure!(row[field] == candidate[field], "{name}: Rust reference or budget changed: {field}");
                    }
                }
                if family == "growth" {
                    self.metric(
                        &name,
                        "static_stack",
                        &row["static_stack"]["static_call_chain_stack_bound_bytes"],
                        &candidate["static_stack"]["static_call_chain_stack_bound_bytes"],
                        false,
                    )?;
                }
                if family == "scalar" {
                    super::cost_evidence::static_memory(&row["static_memory"])?;
                    super::cost_evidence::static_memory(&candidate["static_memory"])?;
                    self.scalar.push(json!({"name":name,"source_sha256":row["source_sha256"],
                        "before_elf_sha256":row["elf_sha256"],"after_elf_sha256":candidate["elf_sha256"],
                        "before":row["static_memory"],"after":candidate["static_memory"]}));
                }
                if family == "executions" {
                    if name.split('/').nth(1) == Some("rust") {
                        ensure!(row["elf_sha256"] == candidate["elf_sha256"], "{name}: Rust reference ELF changed");
                    }
                    ensure!(row["oracle_exit_code"] == candidate["oracle_exit_code"], "{name}: oracle changed");
                    for scope in ["transaction", "group"] {
                        self.observations(&format!("{name}/{scope}"), &row[scope], &candidate[scope])?;
                    }
                    for field in ["role", "input_indices", "output_indices"] {
                        ensure!(row["group"]["group"][field] == candidate["group"]["group"][field], "{name}: group selection changed");
                    }
                }
            }
        }
        Ok(())
    }

    fn multi(&mut self, before: &Value, after: &Value) -> Result<()> {
        let left = named(before, "rows")?;
        let right = named(after, "rows")?;
        ensure!(left.keys().eq(right.keys()), "multi-Script fixture set changed");
        for (name, row) in left {
            let candidate = right[&name];
            for field in ["elf_bytes", "max_stack_frame_bytes", "transaction_bytes", "witness_bytes"] {
                self.metric(&name, field, &row[field], &candidate[field], matches!(field, "transaction_bytes" | "witness_bytes"))?;
            }
            self.observations(&name, &row["transaction"], &candidate["transaction"])?;
            let group_key = |group: &Value| {
                json!([group["group"]["role"], group["group"]["input_indices"], group["group"]["output_indices"]]).to_string()
            };
            let groups = |value: &Value| -> Result<BTreeMap<String, Value>> {
                let mut groups = BTreeMap::new();
                for group in value["groups"].as_array().context("missing groups")? {
                    ensure!(groups.insert(group_key(group), group.clone()).is_none(), "duplicate group");
                }
                Ok(groups)
            };
            let left_groups = groups(row)?;
            let right_groups = groups(candidate)?;
            ensure!(left_groups.keys().eq(right_groups.keys()), "{name}: group membership changed");
            for (key, group) in left_groups {
                self.observations(&format!("{name}/{key}"), &group, &right_groups[&key])?;
            }
            let stacks = named(row, "static_stacks")?;
            let candidate_stacks = named(candidate, "static_stacks")?;
            ensure!(stacks.keys().eq(candidate_stacks.keys()), "{name}: static stack set changed");
            for (key, stack) in stacks {
                self.metric(
                    &format!("{name}/{key}"),
                    "static_stack",
                    &stack["bound"]["static_call_chain_stack_bound_bytes"],
                    &candidate_stacks[&key]["bound"]["static_call_chain_stack_bound_bytes"],
                    false,
                )?;
            }
        }
        Ok(())
    }
}

pub fn run(root: &Path, before: &Path, before_multi: &Path, after: &Path, after_multi: &Path, output: &Path) -> Result<()> {
    // A failed comparison must not leave an earlier passing result in place.
    fs::write(output, b"{\"status\":\"not-generated\"}\n")?;
    super::cost_evidence::check(root, before, before_multi)?;
    super::cost_evidence::check(root, after, after_multi)?;
    let baseline = read(before)?;
    let candidate = read(after)?;
    for field in [
        "cargo_lock_sha256",
        "rust_reference_lock_sha256",
        "rust_toolchain",
        "strip_sha256",
        "strip_version",
        "target",
        "target_profile",
        "edition",
        "opt_level",
        "rust_reference_profile",
        "vm_configuration",
        "measurement_fixture_sha256",
    ] {
        ensure!(baseline["provenance"][field] == candidate["provenance"][field], "comparison configuration changed: {field}");
    }
    let mut comparison = Comparison::default();
    comparison.corpus(&baseline, &candidate)?;
    comparison.multi(&read(before_multi)?, &read(after_multi)?)?;
    let report = json!({"schema":"cellscript-cost-comparison-v2","baseline":baseline["provenance"],
        "candidate":candidate["provenance"],"comparisons":comparison.count,"regressions":comparison.regressions,
        "scalar_static_memory":comparison.scalar,"status":if comparison.regressions.is_empty(){"passed"}else{"failed"}});
    fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    ensure!(comparison.regressions.is_empty(), "cost regressions: see {}", output.display());
    println!("{} comparable metrics passed; static memory counts reported separately", comparison.count);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparison_rejects_missing_duplicate_or_changed_measurements() {
        assert!(named(&json!({"rows":[{"name":"a"},{"name":"a"}]}), "rows").is_err());
        let mut comparison = Comparison::default();
        assert!(comparison.metric("a", "cycles", &json!(8), &Value::Null, false).is_err());
        comparison.metric("a", "cycles", &json!(8), &json!(9), false).unwrap();
        comparison.metric("a", "witness", &json!(8), &json!(7), true).unwrap();
        assert_eq!(comparison.regressions.len(), 2);
        let measured = json!({"scope":"transaction_scripts","exit_category":"success","observation":{"status":"measured","exit_code":0,"cycles":8}});
        let mut changed = measured.clone();
        changed["observation"]["status"] = json!("unavailable");
        assert!(comparison.observations("a", &measured, &changed).is_err());
    }
}

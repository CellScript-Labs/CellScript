//! Explicit pilot freeze/independent check for the recovery experiment namespace.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn unique_map<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map without duplicate keys")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> std::result::Result<Self::Value, A::Error> {
            let mut output = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, T>()? {
                if output.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate limit key"));
                }
            }
            Ok(output)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

const SCHEMA: &str = "cellscript-finance-node-recovery-limits-v1";
const POLICY: &str =
    "quote/encoding/packing_ns=observed*3+1000000; live_resolution_ns=observed*3+25000000; whole_operation_ns=observed*3+1000000000; integer_metrics=ceil(observed*5/4); no-runtime-widening";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bound {
    observed: u64,
    upper: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: String,
    occurrence: usize,
    #[serde(deserialize_with = "unique_map")]
    metrics: BTreeMap<String, Bound>,
    #[serde(deserialize_with = "unique_map")]
    unavailable: BTreeMap<String, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Specification {
    schema: String,
    kind: String,
    policy: String,
    checker_source_hash: String,
    baseline_report: String,
    baseline_report_hash: String,
    profile_parameters: Value,
    ckb_binary_hash: String,
    compiler_binary_hash: String,
    application_binary_hash: String,
    compiler_version: String,
    #[serde(deserialize_with = "unique_map")]
    host_build_inputs: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    protocol_elf_hashes: BTreeMap<String, String>,
    #[serde(deserialize_with = "unique_map")]
    application_compiled_source_hashes: BTreeMap<String, String>,
    cases: Vec<Case>,
}
pub struct Limits {
    spec: Specification,
    hash: String,
}
pub fn source_hash() -> String {
    super::backend::prefixed(&ckb_hash::blake2b_256(include_str!("operations_limits.rs").as_bytes()))
}
fn upper(key: &str, observed: u64) -> Result<u64> {
    if key.ends_with("_ns") {
        observed
            .checked_mul(3)
            .and_then(|value| {
                value.checked_add(match key {
                    "whole_operation_ns" => 1_000_000_000,
                    "known_predecessor_live_resolution_ns" => 25_000_000,
                    _ => 1_000_000,
                })
            })
            .context("timing ceiling overflow")
    } else {
        observed.checked_mul(5).and_then(|value| value.checked_add(3)).map(|value| value / 4).context("integer ceiling overflow")
    }
}
impl Limits {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)?;
        if bytes.len() > 2 * 1024 * 1024 {
            bail!("recovery limits file too large");
        }
        let spec: Specification = serde_json::from_slice(&bytes)?;
        if spec.schema != SCHEMA
            || !matches!(spec.kind.as_str(), "recovery-experiment" | "full-financial-operations")
            || spec.policy != POLICY
            || spec.checker_source_hash != source_hash()
            || spec.baseline_report.is_empty()
            || spec.baseline_report_hash.len() != 66
        {
            bail!("recovery limits schema/policy/source/baseline differs");
        }
        if spec.cases.is_empty() {
            bail!("recovery limits have no cases");
        }
        if spec.host_build_inputs.keys().map(String::as_str).collect::<Vec<_>>()
            != ["compiled_cargo_lock_hash", "rustc", "sdk_head", "toolchain_contract_hash"]
        {
            bail!("recovery build input inventory differs");
        }
        for case in &spec.cases {
            if case.case.is_empty() || case.occurrence == 0 || case.metrics.is_empty() {
                bail!("recovery case inventory invalid");
            }
            for (key, bound) in &case.metrics {
                if bound.upper != upper(key, bound.observed)? {
                    bail!("recovery ceiling differs from explicit policy");
                }
            }
        }
        Ok(Self { spec, hash: super::backend::prefixed(&ckb_hash::blake2b_256(&bytes)) })
    }
    pub fn check(&self, report: &Value) -> Result<Value> {
        let samples = if self.spec.kind == "recovery-experiment" {
            &report["operational_recovery"]["samples"]
        } else {
            &report["whole_operation_samples"]
        };
        let samples = samples.as_array().context("operation samples missing")?;
        let mut violations = vec![];
        if self.spec.profile_parameters != report["profile_parameters"]
            || self.spec.ckb_binary_hash != report["ckb_binary_hash"]
            || self.spec.compiler_version != report["compiler_version"]
            || self.spec.compiler_binary_hash != report["compiler_binary_hash"]
            || self.spec.application_binary_hash != report["application_binary_hash"]
            || report["compiler_snapshot_unchanged"] != true
            || serde_json::to_value(&self.spec.protocol_elf_hashes)? != report["protocol_elf_hashes"]
            || serde_json::to_value(&self.spec.application_compiled_source_hashes)? != report["application_compiled_source_hashes"]
        {
            violations.push("exact profile/executable/source identities differ".to_owned());
        }
        if self.spec.host_build_inputs.iter().any(|(key, value)| report["host_context"][key] != *value) {
            violations.push("exact toolchain/lock/SDK build inputs differ".into());
        }
        if samples.len() != self.spec.cases.len() {
            violations.push("case inventory length differs".to_owned());
        }
        for (sample, case) in samples.iter().zip(&self.spec.cases) {
            if sample["case"] != case.case || sample["occurrence"] != case.occurrence {
                violations.push("case/occurrence ordering differs".to_owned());
            }
            let actual = sample["metrics"].as_object().context("sample metrics missing")?;
            if serde_json::to_value(&case.unavailable)? != sample["unavailable"] {
                violations.push(format!("{}#{} availability differs", case.case, case.occurrence));
            }
            if actual.len() != case.metrics.len() {
                violations.push(format!("{}#{} metric inventory differs", case.case, case.occurrence));
            }
            for (key, bound) in &case.metrics {
                match actual.get(key).and_then(Value::as_u64) {
                    Some(observed) if observed <= bound.upper => {}
                    Some(observed) => {
                        violations.push(format!("{}#{} {key}: {observed} > {}", case.case, case.occurrence, bound.upper))
                    }
                    None => violations.push(format!("{}#{} {key} unavailable or invalid", case.case, case.occurrence)),
                }
            }
        }
        let scope = if self.spec.kind == "recovery-experiment" {
            "finite recovery experiment; three serialized same-shape contention patterns and named controls; no universal latency or sustained network throughput claim"
        } else {
            "named whole-operation samples for the exact profile/case occurrence inventory; phase scopes and unavailable metrics retained in the report; single-host; no universal latency or network-capacity guarantee"
        };
        Ok(
            json!({"schema":SCHEMA,"kind":self.spec.kind,"status":if violations.is_empty(){"passed-measured-scope"}else{"failed"},"limits_hash":self.hash,
            "checker_source_hash":source_hash(),"checked_cases":samples.len(),"violations":violations,"automatic_widening":false,
            "scope":scope}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_policy_keeps_phases_and_whole_operation_separate() {
        assert_eq!(upper("quote_ns", 100).unwrap(), 1_000_300);
        assert_eq!(upper("whole_operation_ns", 100).unwrap(), 1_000_000_300);
        assert_eq!(upper("maximum_transaction_bytes", 101).unwrap(), 127);
        assert!(upper("quote_ns", u64::MAX).is_err());
    }
    #[test]
    fn unknown_fields_cannot_assert_a_pass() {
        assert!(serde_json::from_str::<Bound>(r#"{"observed":1,"upper":2,"passed":true}"#).is_err());
        assert!(serde_json::from_str::<Bound>(r#"{"observed":-1,"upper":2}"#).is_err());
        assert!(serde_json::from_str::<Case>(
            r#"{"case":"x","occurrence":1,"metrics":{"x":{"observed":1,"upper":2},"x":{"observed":1,"upper":2}}}"#
        )
        .is_err());
    }
    #[test]
    fn exceeded_missing_or_misbound_samples_fail() {
        let mut report = json!({"profile_parameters":{"name":"reference"},"ckb_binary_hash":"node","compiler_binary_hash":"compiler","application_binary_hash":"application","compiler_snapshot_unchanged":true,"compiler_version":"test","host_context":{},"protocol_elf_hashes":{},"application_compiled_source_hashes":{},
            "whole_operation_samples":[{"case":"case","occurrence":1,"metrics":{"quote_ns":10},"unavailable":{}}]});
        let spec=serde_json::from_value(json!({"schema":SCHEMA,"kind":"full-financial-operations","policy":POLICY,"checker_source_hash":source_hash(),
            "baseline_report":"pilot","baseline_report_hash":"0x00","profile_parameters":report["profile_parameters"],"ckb_binary_hash":"node","compiler_binary_hash":"compiler","application_binary_hash":"application","compiler_version":"test","host_build_inputs":{},
            "protocol_elf_hashes":{},"application_compiled_source_hashes":{},"cases":[{"case":"case","occurrence":1,"metrics":{"quote_ns":{"observed":10,"upper":1000030}},"unavailable":{}}]})).unwrap();
        let limits = Limits { spec, hash: "fixture".into() };
        assert_eq!(limits.check(&report).unwrap()["status"], "passed-measured-scope");
        assert_eq!(limits.check(&report).unwrap()["kind"], "full-financial-operations");
        assert!(!limits.check(&report).unwrap()["scope"].as_str().unwrap().contains("three serialized"));
        report["whole_operation_samples"][0]["metrics"]["quote_ns"] = json!(1000031);
        assert_eq!(limits.check(&report).unwrap()["status"], "failed");
        report["whole_operation_samples"][0]["metrics"] = json!({});
        assert_eq!(limits.check(&report).unwrap()["status"], "failed");
        report["whole_operation_samples"] = json!([]);
        assert_eq!(limits.check(&report).unwrap()["status"], "failed");
    }
}

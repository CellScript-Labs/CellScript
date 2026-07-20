use crate::descriptor::{canonical_hash_type, canonical_hex, ScriptIdentity};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberScriptConfig {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberCellDep {
    pub out_point: FiberOutPoint,
    pub dep_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberOutPoint {
    pub tx_hash: String,
    pub index: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberTypeIdScript {
    pub code_hash: String,
    pub hash_type: String,
    pub args: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberUdtDep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_dep: Option<FiberCellDep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_id: Option<FiberTypeIdScript>,
}

impl FiberUdtDep {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.cell_dep.is_some() == self.type_id.is_some() {
            anyhow::bail!("Fiber UDT dependency must contain exactly one of cell_dep and type_id");
        }
        if let Some(cell_dep) = &self.cell_dep {
            canonical_hex(&cell_dep.out_point.tx_hash, Some(32), "cell_dep.out_point.tx_hash")?;
            parse_hex_u32(&cell_dep.out_point.index)?;
            if !matches!(cell_dep.dep_type.as_str(), "code" | "dep_group") {
                anyhow::bail!("Fiber cell_dep.dep_type must be 'code' or 'dep_group'");
            }
        }
        if let Some(type_id) = &self.type_id {
            canonical_hex(&type_id.code_hash, Some(32), "type_id.code_hash")?;
            canonical_hex(&type_id.args, None, "type_id.args")?;
            canonical_hash_type(&type_id.hash_type)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiberUdtArgInfo {
    pub name: String,
    pub script: FiberScriptConfig,
    pub auto_accept_amount: Option<u128>,
    pub cell_deps: Vec<FiberUdtDep>,
}

impl FiberUdtArgInfo {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.name.trim().is_empty() {
            anyhow::bail!("Fiber UDT name must not be empty");
        }
        canonical_hex(&self.script.code_hash, Some(32), "script.code_hash")?;
        canonical_hash_type(&self.script.hash_type)?;
        let regex = Regex::new(&self.script.args)?;
        if !self.script.args.starts_with('^') || !self.script.args.ends_with('$') {
            anyhow::bail!("Fiber Script args matcher must be explicitly anchored with ^ and $");
        }
        if self.cell_deps.is_empty() {
            anyhow::bail!("Fiber UDT configuration must contain at least one live-verified CellDep");
        }
        for dependency in &self.cell_deps {
            dependency.validate()?;
        }
        if regex.is_match("") {
            anyhow::bail!("Fiber Script args matcher must reject the empty string");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactArgsMatcherEvidence {
    pub intended_args: String,
    pub matcher: String,
    pub intended_match: bool,
    pub empty_rejected: bool,
    pub prefix_rejected: bool,
    pub suffix_rejected: bool,
    pub neighbour_rejected: bool,
}

impl ExactArgsMatcherEvidence {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.intended_match && self.empty_rejected && self.prefix_rejected && self.suffix_rejected && self.neighbour_rejected {
            Ok(())
        } else {
            anyhow::bail!("anchored Fiber Script args matcher failed its closed positive/negative match set")
        }
    }
}

pub fn exact_args_matcher(args: &str) -> anyhow::Result<ExactArgsMatcherEvidence> {
    let intended_args = canonical_hex(args, None, "asset_script.args")?;
    let matcher = format!("^{}$", regex::escape(&intended_args));
    let regex = Regex::new(&matcher)?;
    let prefix = intended_args[..intended_args.len().saturating_sub(1)].to_string();
    let suffix = format!("{}00", intended_args);
    let neighbour = neighbouring_hex(&intended_args);
    let evidence = ExactArgsMatcherEvidence {
        intended_match: regex.is_match(&intended_args),
        empty_rejected: !regex.is_match(""),
        prefix_rejected: !regex.is_match(&prefix),
        suffix_rejected: !regex.is_match(&suffix),
        neighbour_rejected: !regex.is_match(&neighbour),
        intended_args,
        matcher,
    };
    evidence.validate()?;
    Ok(evidence)
}

pub fn build_fiber_udt_config(
    name: impl Into<String>,
    asset_script: &ScriptIdentity,
    auto_accept_amount: Option<u128>,
    cell_deps: Vec<FiberUdtDep>,
) -> anyhow::Result<(FiberUdtArgInfo, ExactArgsMatcherEvidence)> {
    let asset_script = asset_script.clone().canonicalized()?;
    validate_authority_args(&asset_script.args)?;
    let matcher = exact_args_matcher(&asset_script.args)?;
    let config = FiberUdtArgInfo {
        name: name.into(),
        script: FiberScriptConfig {
            code_hash: asset_script.code_hash,
            hash_type: asset_script.hash_type,
            args: matcher.matcher.clone(),
        },
        auto_accept_amount,
        cell_deps,
    };
    config.validate()?;
    Ok((config, matcher))
}

fn validate_authority_args(args: &str) -> anyhow::Result<()> {
    let args = canonical_hex(args, None, "asset_script.args")?;
    let bytes = hex::decode(&args[2..])?;
    if bytes.len() == 32 || (bytes.len() == 33 && bytes[0] == 1) {
        Ok(())
    } else {
        anyhow::bail!(
            "asset_script.args must be a 32-byte input Lock Script hash or 0x01 followed by a 32-byte input Type Script hash"
        )
    }
}

pub fn render_fiber_config_overlay(config: &FiberUdtArgInfo) -> anyhow::Result<String> {
    config.validate()?;
    let mut yaml = String::from("ckb:\n  udt_whitelist:\n");
    yaml.push_str(&format!("    - name: {}\n", json_string(&config.name)?));
    yaml.push_str("      script:\n");
    yaml.push_str(&format!("        code_hash: {}\n", json_string(&config.script.code_hash)?));
    yaml.push_str(&format!("        hash_type: {}\n", json_string(&config.script.hash_type)?));
    yaml.push_str(&format!("        args: {}\n", json_string(&config.script.args)?));
    match config.auto_accept_amount {
        Some(amount) => yaml.push_str(&format!("      auto_accept_amount: {amount}\n")),
        None => yaml.push_str("      auto_accept_amount: null\n"),
    }
    yaml.push_str("      cell_deps:\n");
    for dependency in &config.cell_deps {
        if let Some(cell_dep) = &dependency.cell_dep {
            yaml.push_str("        - cell_dep:\n");
            yaml.push_str("            out_point:\n");
            yaml.push_str(&format!("              tx_hash: {}\n", json_string(&cell_dep.out_point.tx_hash)?));
            yaml.push_str(&format!("              index: {}\n", json_string(&cell_dep.out_point.index)?));
            yaml.push_str(&format!("            dep_type: {}\n", json_string(&cell_dep.dep_type)?));
        } else if let Some(type_id) = &dependency.type_id {
            yaml.push_str("        - type_id:\n");
            yaml.push_str(&format!("            code_hash: {}\n", json_string(&type_id.code_hash)?));
            yaml.push_str(&format!("            hash_type: {}\n", json_string(&type_id.hash_type)?));
            yaml.push_str(&format!("            args: {}\n", json_string(&type_id.args)?));
        }
    }
    Ok(yaml)
}

/// Replaces only `ckb.udt_whitelist` in an ordinary Fiber YAML config.
///
/// The generated file remains a native Fiber configuration; the compatibility
/// report is evidence for this operation, not a runtime profile or semantic
/// interpreter.
pub fn materialize_fiber_config(base_yaml: &str, configs: &[FiberUdtArgInfo]) -> anyhow::Result<String> {
    if configs.is_empty() {
        anyhow::bail!("at least one verified Fiber UDT configuration is required");
    }
    for config in configs {
        config.validate()?;
    }
    let mut document: serde_yaml::Value = serde_yaml::from_str(base_yaml)?;
    let root = document.as_mapping_mut().ok_or_else(|| anyhow::anyhow!("Fiber config root must be a YAML mapping"))?;
    let ckb = root
        .get_mut(serde_yaml::Value::String("ckb".to_string()))
        .and_then(serde_yaml::Value::as_mapping_mut)
        .ok_or_else(|| anyhow::anyhow!("Fiber config must contain a ckb mapping"))?;
    ckb.insert(serde_yaml::Value::String("udt_whitelist".to_string()), serde_yaml::to_value(configs)?);
    Ok(serde_yaml::to_string(&document)?)
}

fn json_string(value: &str) -> anyhow::Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn parse_hex_u32(value: &str) -> anyhow::Result<u32> {
    let raw = value.strip_prefix("0x").ok_or_else(|| anyhow::anyhow!("out_point index must be 0x-prefixed"))?;
    u32::from_str_radix(raw, 16).map_err(Into::into)
}

fn neighbouring_hex(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    if let Some(last) = bytes.last_mut() {
        *last = if *last == b'0' { b'1' } else { b'0' };
    }
    String::from_utf8(bytes).unwrap_or_else(|_| "0x00".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direct_dep() -> FiberUdtDep {
        FiberUdtDep {
            cell_dep: Some(FiberCellDep {
                out_point: FiberOutPoint { tx_hash: format!("0x{}", "11".repeat(32)), index: "0x0".to_string() },
                dep_type: "code".to_string(),
            }),
            type_id: None,
        }
    }

    #[test]
    fn matcher_is_escaped_anchored_and_exact() {
        let evidence = exact_args_matcher("0x2e2a5b5d").unwrap();
        assert_eq!(evidence.matcher, r"^0x2e2a5b5d$");
        evidence.validate().unwrap();
        let regex = Regex::new(&evidence.matcher).unwrap();
        assert!(regex.is_match("0x2e2a5b5d"));
        assert!(!regex.is_match("prefix0x2e2a5b5dsuffix"));
    }

    #[test]
    fn dependency_requires_exactly_one_identity_form() {
        assert!(FiberUdtDep { cell_dep: None, type_id: None }.validate().is_err());
        let mut both = direct_dep();
        both.type_id = Some(FiberTypeIdScript {
            code_hash: format!("0x{}", "00".repeat(32)),
            hash_type: "type".to_string(),
            args: format!("0x{}", "22".repeat(32)),
        });
        assert!(both.validate().is_err());
    }

    #[test]
    fn overlay_is_deterministic_and_uses_exact_matcher() {
        let owner_args = format!("0x{}", "01".repeat(32));
        let script =
            ScriptIdentity { code_hash: format!("0x{}", "ab".repeat(32)), hash_type: "data2".to_string(), args: owner_args.clone() };
        let (config, _) = build_fiber_udt_config("sample::Asset", &script, Some(42), vec![direct_dep()]).unwrap();
        let first = render_fiber_config_overlay(&config).unwrap();
        let second = render_fiber_config_overlay(&config).unwrap();
        assert_eq!(first, second);
        assert!(first.contains(&format!("args: \"^{}$\"", owner_args)));
        assert!(first.contains("auto_accept_amount: 42"));
    }

    #[test]
    fn tagged_type_script_authority_is_accepted_and_other_tags_fail_closed() {
        let tagged = format!("0x01{}", "22".repeat(32));
        let script =
            ScriptIdentity { code_hash: format!("0x{}", "ab".repeat(32)), hash_type: "data2".to_string(), args: tagged.clone() };
        let (config, matcher) = build_fiber_udt_config("policy-asset", &script, None, vec![direct_dep()]).unwrap();
        assert_eq!(matcher.intended_args, tagged);
        assert_eq!(config.script.args, format!("^{tagged}$"));

        let invalid = ScriptIdentity {
            code_hash: format!("0x{}", "ab".repeat(32)),
            hash_type: "data2".to_string(),
            args: format!("0x02{}", "22".repeat(32)),
        };
        assert!(build_fiber_udt_config("invalid", &invalid, None, vec![direct_dep()]).is_err());
    }

    #[test]
    fn materialization_replaces_only_the_native_whitelist() {
        let owner_args = format!("0x{}", "01".repeat(32));
        let script = ScriptIdentity { code_hash: format!("0x{}", "ab".repeat(32)), hash_type: "data2".to_string(), args: owner_args };
        let (config, _) = build_fiber_udt_config("sample::Asset", &script, Some(42), vec![direct_dep()]).unwrap();
        let base = "fiber:\n  chain: dev.toml\nrpc:\n  listening_addr: 127.0.0.1:21714\nckb:\n  rpc_url: http://127.0.0.1:8114\n  udt_whitelist:\n    - name: stale\n";
        let rendered = materialize_fiber_config(base, &[config]).unwrap();
        let parsed: serde_yaml::Value = serde_yaml::from_str(&rendered).unwrap();
        assert_eq!(parsed["fiber"]["chain"].as_str(), Some("dev.toml"));
        assert_eq!(parsed["rpc"]["listening_addr"].as_str(), Some("127.0.0.1:21714"));
        assert_eq!(parsed["ckb"]["rpc_url"].as_str(), Some("http://127.0.0.1:8114"));
        assert_eq!(parsed["ckb"]["udt_whitelist"][0]["name"].as_str(), Some("sample::Asset"));
        assert_eq!(parsed["ckb"]["udt_whitelist"].as_sequence().map(Vec::len), Some(1));
    }
}

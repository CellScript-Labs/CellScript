use super::report_files::json_load;
use super::EXPECTED_NOVASEAL_CANONICAL_ENVELOPE;
use super::{json_pointer_bool_opt, json_pointer_str, normalize_hex, rel, sha256_file_hex, EXPECTED_CANONICAL_SCHEMA_FIELDS};
use crate::error::{Error, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

pub(super) fn live_verifier_facts(repo_root: &Path, rel_path: &str) -> Result<Value> {
    let payload = json_load(repo_root, rel_path)?;
    let verifier = payload.pointer("/artifacts/verifier").cloned().unwrap_or(Value::Null);
    let out_point = verifier.pointer("/cell_dep/out_point").cloned().unwrap_or(Value::Null);
    let index = json_pointer_str(&out_point, "/index")
        .and_then(|value| value.strip_prefix("0x").and_then(|hex| u64::from_str_radix(hex, 16).ok()))
        .or_else(|| out_point.get("index").and_then(Value::as_u64));
    Ok(json!({
        "status": json_pointer_str(&payload, "/status"),
        "live_devnet_rpc_executed": json_pointer_bool_opt(&payload, "/live_devnet_rpc_executed"),
        "name": json_pointer_str(&verifier, "/name"),
        "tx_hash": normalize_hex(json_pointer_str(&out_point, "/tx_hash")),
        "index": index,
        "dep_type": json_pointer_str(&verifier, "/cell_dep/dep_type"),
        "data_hash": normalize_hex(json_pointer_str(&verifier, "/data_hash")),
        "artifact_size_bytes": verifier.get("artifact_size_bytes").and_then(Value::as_u64),
    }))
}

pub(super) fn runtime_dep(manifest: &toml::Value) -> Result<toml::Value> {
    let deps = manifest
        .get("deploy")
        .and_then(toml::Value::as_table)
        .and_then(|deploy| deploy.get("ckb"))
        .and_then(toml::Value::as_table)
        .and_then(|ckb| ckb.get("cell_deps"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let matches = deps
        .into_iter()
        .filter(|dep| {
            toml_str(dep, "role") == Some("runtime_verifier") || toml_str(dep, "name") == Some("cellscript_btc_bip340_verifier_riscv")
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(Error::invalid(format!("expected exactly one NovaSeal runtime verifier dep, found {}", matches.len())));
    }
    Ok(matches[0].clone())
}

pub(super) fn expected_files(repo_root: &Path, root: &Path, names: &[&str]) -> Result<Value> {
    let canonical_repo_root = repo_root.canonicalize()?;
    let expected = names.iter().map(|name| (*name).to_string()).collect::<BTreeSet<_>>();
    let mut invalid = BTreeSet::new();
    let mut found = BTreeSet::new();
    if safe_directory_within_root(&canonical_repo_root, root)?.is_some() {
        for entry in std::fs::read_dir(root)? {
            let entry = entry?;
            let path = entry.path();
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                invalid.insert(name);
            } else if file_type.is_file() && safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
                found.insert(name);
            }
        }
    } else {
        invalid.insert(rel(repo_root, root));
    }
    let mut hashes = Map::new();
    for name in &expected {
        let path = root.join(name);
        if safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
            hashes.insert(name.clone(), Value::String(sha256_file_hex(&path)?));
        }
    }
    Ok(json!({
        "root": rel(repo_root, root),
        "expected": expected.iter().cloned().collect::<Vec<_>>(),
        "present": found.intersection(&expected).cloned().collect::<Vec<_>>(),
        "missing": expected.difference(&found).cloned().collect::<Vec<_>>(),
        "extra": found.difference(&expected).cloned().collect::<Vec<_>>(),
        "invalid": invalid.iter().cloned().collect::<Vec<_>>(),
        "hashes": hashes,
        "exact": found == expected && invalid.is_empty(),
    }))
}

pub(super) fn symlink_metadata_optional(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn safe_regular_file_within_root(canonical_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Ok(None);
    }
    let canonical_path = path.canonicalize()?;
    if canonical_path.starts_with(canonical_root) {
        Ok(Some(canonical_path))
    } else {
        Ok(None)
    }
}

pub(super) fn safe_directory_within_root(canonical_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(None);
    }
    let canonical_path = path.canonicalize()?;
    if canonical_path.starts_with(canonical_root) {
        Ok(Some(canonical_path))
    } else {
        Ok(None)
    }
}

pub(super) fn canonical_schema_lines(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = std::fs::read_to_string(path)?;
    Ok(source
        .lines()
        .filter_map(|raw| {
            let stripped = raw.split_once('#').map_or(raw, |(before, _)| before).trim();
            if stripped.is_empty() {
                return None;
            }
            if let Some((name, rest)) = stripped.split_once(':') {
                let rest = rest.split_whitespace().collect::<Vec<_>>().join(" ");
                if rest.is_empty() {
                    Some(format!("{}:", name.trim()))
                } else {
                    Some(format!("{}: {rest}", name.trim()))
                }
            } else {
                Some(stripped.split_whitespace().collect::<Vec<_>>().join(" "))
            }
        })
        .collect())
}

pub(super) fn canonical_schema_hash(path: &Path) -> Result<Option<String>> {
    let lines = canonical_schema_lines(path)?;
    if lines.is_empty() {
        return Ok(None);
    }
    let mut payload = lines.join("\n").into_bytes();
    payload.push(b'\n');
    Ok(Some(format!("0x{}", hex::encode(Sha256::digest(payload)))))
}

pub(super) fn canonical_schema_checks(path: &Path) -> Result<Map<String, Value>> {
    let lines = canonical_schema_lines(path)?;
    let mut expected_lines = vec![format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:")];
    expected_lines.extend(EXPECTED_CANONICAL_SCHEMA_FIELDS.iter().map(|(name, ty)| format!("{name}: {ty}")));
    let expected_set = expected_lines.iter().cloned().collect::<BTreeSet<_>>();
    let lines_set = lines.iter().cloned().collect::<BTreeSet<_>>();
    Ok([
        ("canonical_schema_file_present".to_string(), Value::Bool(path.exists())),
        (
            "canonical_schema_name".to_string(),
            Value::Bool(lines.first().is_some_and(|line| line == &format!("{EXPECTED_NOVASEAL_CANONICAL_ENVELOPE}:"))),
        ),
        ("canonical_schema_exact_field_order".to_string(), Value::Bool(lines == expected_lines)),
        ("canonical_schema_no_extra_fields".to_string(), Value::Bool(lines_set == expected_set)),
        ("canonical_schema_normalized_hash_present".to_string(), Value::Bool(canonical_schema_hash(path)?.is_some())),
    ]
    .into_iter()
    .collect())
}

pub(super) fn manifest_metadata(path: &Path) -> Result<toml::Value> {
    Ok(toml_value(path)?.get("metadata").cloned().unwrap_or_else(|| toml::Value::Table(Default::default())))
}

pub(super) fn toml_value(path: &Path) -> Result<toml::Value> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| Error::invalid(format!("failed to read TOML '{}': {}", path.display(), error)))?;
    toml::from_str(&source).map_err(|error| Error::invalid(format!("failed to parse TOML '{}': {}", path.display(), error)))
}

pub(super) fn toml_to_json(value: &toml::Value) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

pub(super) fn toml_str<'a>(value: &'a toml::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(toml::Value::as_str)
}

pub(super) fn read_cell_sources(src_root: &Path) -> Result<String> {
    if !src_root.is_dir() {
        return Ok(String::new());
    }
    let mut paths = std::fs::read_dir(src_root)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cell") && path.is_file())
        .collect::<Vec<_>>();
    paths.sort();
    let mut source = String::new();
    for path in paths {
        source.push_str(&std::fs::read_to_string(path)?);
        source.push('\n');
    }
    Ok(source)
}

use super::package_sources::symlink_metadata_optional;
use super::rel;
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(super) fn json_load(repo_root: &Path, rel_path: &str) -> Result<Value> {
    json_load_path(repo_root, &repo_root.join(rel_path))
}

pub(super) fn json_load_path(repo_root: &Path, path: &Path) -> Result<Value> {
    let Some(path) = safe_json_report_path(repo_root, path)? else {
        return Ok(json!({"missing": true, "path": rel(repo_root, path)}));
    };
    let bytes = std::fs::read(&path)?;
    serde_json::from_slice(&bytes).map_err(|error| Error::invalid(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

pub(super) fn json_load_path_optional(repo_root: &Path, path: &Path) -> Result<Option<Value>> {
    let Some(path) = safe_json_report_path(repo_root, path)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path)?;
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(value) if value.is_object() => Ok(Some(value)),
        Ok(_) => Ok(Some(json!({"_invalid_json": "top-level value is not an object"}))),
        Err(error) => Ok(Some(json!({"_invalid_json": error.to_string()}))),
    }
}

fn safe_json_report_path(repo_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let Some(metadata) = symlink_metadata_optional(path)? else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(Error::invalid(format!(
            "refusing to read JSON report '{}' because it is not a regular file within repository root '{}'",
            path.display(),
            repo_root.display()
        )));
    }
    let canonical_repo_root = repo_root.canonicalize()?;
    let canonical_path = path.canonicalize()?;
    if !canonical_path.starts_with(&canonical_repo_root) {
        return Err(Error::invalid(format!(
            "refusing to read JSON report '{}' because it resolves outside repository root '{}'",
            path.display(),
            repo_root.display()
        )));
    }
    Ok(Some(canonical_path))
}

pub(super) fn write_json_report(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| Error::invalid(format!("failed to serialize JSON report '{}': {}", path.display(), error)))?;
    std::fs::write(path, json + "\n")?;
    Ok(())
}

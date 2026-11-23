use super::package_sources::{safe_directory_within_root, safe_regular_file_within_root, symlink_metadata_optional};
use super::{json_pointer_bool, json_pointer_str, rel, sha256_file_hex};
use crate::error::Result;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(super) fn provenance_summary(report: &Value, repo_root: &Path, source_paths: &[&str]) -> Result<Value> {
    let provenance = report.get("provenance").cloned().unwrap_or(Value::Null);
    let recorded_source = provenance.get("source_tree").cloned().unwrap_or(Value::Null);
    let current_source = source_tree_hash(repo_root, source_paths)?;
    let recorded_source_hash = json_pointer_str(&recorded_source, "/sha256");
    let current_source_hash = json_pointer_str(&current_source, "/sha256");
    let recorded_source_valid =
        recorded_source.get("valid").and_then(Value::as_bool).unwrap_or_else(|| recorded_source_hash.is_some());
    let current_source_valid = json_pointer_bool(&current_source, "/valid");
    let source_hash_matches =
        recorded_source_valid && current_source_valid && recorded_source_hash.is_some() && recorded_source_hash == current_source_hash;
    let mut artifact_checks = Map::new();
    let recorded_artifacts = provenance.get("artifacts").cloned().unwrap_or(Value::Null);
    let canonical_repo_root = repo_root.canonicalize()?;
    for name in ["verifier", "lifecycle"] {
        let artifact = recorded_artifacts.get(name).cloned().unwrap_or(Value::Null);
        let raw_path = json_pointer_str(&artifact, "/path");
        let path = raw_path.map(|value| {
            let path = Path::new(value);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                repo_root.join(path)
            }
        });
        let canonical_path =
            path.as_ref().map(|path| safe_regular_file_within_root(&canonical_repo_root, path)).transpose()?.flatten();
        let current_sha = canonical_path.as_ref().map(|path| sha256_file_hex(path)).transpose()?;
        artifact_checks.insert(
            name.to_string(),
            json!({
                "present": artifact.is_object(),
                "path": raw_path,
                "exists": canonical_path.is_some(),
                "regular_file_within_repo": canonical_path.is_some(),
                "sha256_matches": current_sha.as_deref() == json_pointer_str(&artifact, "/sha256"),
                "recorded_sha256": json_pointer_str(&artifact, "/sha256"),
                "current_sha256": current_sha,
            }),
        );
    }
    let artifact_hashes_match = artifact_checks.values().all(|row| {
        json_pointer_bool(row, "/present") && json_pointer_bool(row, "/exists") && json_pointer_bool(row, "/sha256_matches")
    });
    let current_commit = git_commit(repo_root);
    let repo_commit_matches = json_pointer_str(&provenance, "/repo_commit") == current_commit.as_deref();
    Ok(json!({
        "present": provenance.is_object(),
        "freshness_matched": source_hash_matches && artifact_hashes_match && repo_commit_matches,
        "repo_commit": json_pointer_str(&provenance, "/repo_commit"),
        "current_repo_commit": current_commit,
        "repo_commit_matches": repo_commit_matches,
        "source_hash_matches": source_hash_matches,
        "recorded_source_valid": recorded_source_valid,
        "current_source_valid": current_source_valid,
        "recorded_source_hash": recorded_source_hash,
        "current_source_hash": current_source_hash,
        "recorded_file_count": recorded_source.get("file_count").and_then(Value::as_u64),
        "current_file_count": current_source.get("file_count").and_then(Value::as_u64),
        "current_source_invalid_paths": current_source.get("invalid_paths").cloned().unwrap_or(Value::Null),
        "artifact_hashes_match": artifact_hashes_match,
        "artifacts": artifact_checks,
    }))
}

pub(super) fn source_tree_hash(repo_root: &Path, paths: &[&str]) -> Result<Value> {
    source_tree_hash_with_options(repo_root, paths, false)
}

pub(super) fn source_tree_hash_with_options(repo_root: &Path, paths: &[&str], include_markdown: bool) -> Result<Value> {
    let canonical_repo_root = repo_root.canonicalize()?;
    let mut files = BTreeSet::new();
    let mut invalid_paths = BTreeSet::new();
    for raw_path in paths {
        let path = repo_root.join(raw_path);
        let Some(metadata) = symlink_metadata_optional(&path)? else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            invalid_paths.insert(rel(repo_root, &path));
        } else if metadata.is_file() {
            if safe_regular_file_within_root(&canonical_repo_root, &path)?.is_some() {
                files.insert(path);
            } else {
                invalid_paths.insert(rel(repo_root, &path));
            }
        } else if metadata.is_dir() {
            if safe_directory_within_root(&canonical_repo_root, &path)?.is_some() {
                collect_source_tree_files(
                    repo_root,
                    &canonical_repo_root,
                    &path,
                    &path,
                    include_markdown,
                    &mut files,
                    &mut invalid_paths,
                )?;
            } else {
                invalid_paths.insert(rel(repo_root, &path));
            }
        }
    }
    let mut hasher = Sha256::new();
    let mut rows = Vec::new();
    for path in files {
        let rel_path = rel(repo_root, &path);
        let digest = Sha256::digest(std::fs::read(&path)?);
        hasher.update(rel_path.as_bytes());
        hasher.update([0]);
        hasher.update(digest);
        rows.push(rel_path);
    }
    let invalid_rows = invalid_paths.into_iter().collect::<Vec<_>>();
    Ok(json!({
        "sha256": if invalid_rows.is_empty() { Value::String(format!("0x{}", hex::encode(hasher.finalize()))) } else { Value::Null },
        "files": rows,
        "file_count": rows.len(),
        "invalid_paths": invalid_rows,
        "valid": invalid_rows.is_empty(),
    }))
}

fn collect_source_tree_files(
    repo_root: &Path,
    canonical_repo_root: &Path,
    root: &Path,
    path: &Path,
    include_markdown: bool,
    files: &mut BTreeSet<PathBuf>,
    invalid_paths: &mut BTreeSet<String>,
) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let relative_parts = child.strip_prefix(root).unwrap_or(&child).components().map(|part| part.as_os_str().to_string_lossy());
        if relative_parts.clone().any(|part| matches!(part.as_ref(), "target" | "build" | ".git")) {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&child)?;
        if metadata.file_type().is_symlink() {
            invalid_paths.insert(rel(repo_root, &child));
        } else if metadata.is_dir() {
            if safe_directory_within_root(canonical_repo_root, &child)?.is_some() {
                collect_source_tree_files(repo_root, canonical_repo_root, root, &child, include_markdown, files, invalid_paths)?;
            } else {
                invalid_paths.insert(rel(repo_root, &child));
            }
        } else if metadata.is_file() && source_tree_file_allowed(&child, include_markdown) {
            if safe_regular_file_within_root(canonical_repo_root, &child)?.is_some() {
                files.insert(child);
            } else {
                invalid_paths.insert(rel(repo_root, &child));
            }
        }
    }
    Ok(())
}

fn source_tree_file_allowed(path: &Path, include_markdown: bool) -> bool {
    path.file_name().is_some_and(|name| name == "Cargo.lock")
        || (include_markdown && path.file_name().is_some_and(|name| name == "README.md"))
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "cell" | "schema" | "toml" | "py" | "json" | "rs"))
}

pub(super) fn git_commit(repo_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git").arg("rev-parse").arg("HEAD").current_dir(repo_root).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

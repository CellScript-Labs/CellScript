use super::{
    is_git_commit_hash, json_pointer_bool_opt, json_pointer_str, object_values_all_true, safe_relative_path,
    EXPECTED_FIBER_REPO_ORIGIN,
};
use serde_json::{json, Value};
use std::path::Path;

pub(super) fn fiber_repo_git_provenance(fiber_repo_path: Option<&Path>, report: &Value) -> Value {
    let Some(path) = fiber_repo_path else {
        return json!({
            "verified": false,
            "reason": "missing fiber repo path",
            "checks": {
                "git_commands_succeeded": false,
                "origin_matches_report": false,
                "branch_matches_report": false,
                "commit_matches_report": false,
                "dirty_matches_report": false,
                "origin_matches_expected": false,
                "commit_is_full_sha1": false,
                "clean_tree": false,
            }
        });
    };
    let origin = git_stdout(path, &["remote", "get-url", "origin"]);
    let branch = git_stdout(path, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let commit = git_stdout(path, &["rev-parse", "HEAD"]);
    let status = git_stdout(path, &["status", "--porcelain"]);
    let dirty = status.as_ref().map(|value| !value.trim().is_empty());
    let checks = json!({
        "git_commands_succeeded": origin.is_some() && branch.is_some() && commit.is_some() && dirty.is_some(),
        "origin_matches_report": origin.as_deref() == json_pointer_str(report, "/fiber_repo/origin"),
        "branch_matches_report": branch.as_deref() == json_pointer_str(report, "/fiber_repo/branch"),
        "commit_matches_report": commit.as_deref() == json_pointer_str(report, "/fiber_repo/commit"),
        "dirty_matches_report": dirty == json_pointer_bool_opt(report, "/fiber_repo/dirty"),
        "origin_matches_expected": origin.as_deref() == Some(EXPECTED_FIBER_REPO_ORIGIN),
        "commit_is_full_sha1": commit.as_deref().is_some_and(is_git_commit_hash),
        "clean_tree": dirty == Some(false),
    });
    json!({
        "verified": object_values_all_true(Some(&checks)),
        "path": path.display().to_string(),
        "origin": origin,
        "branch": branch,
        "commit": commit,
        "dirty": dirty,
        "checks": checks,
    })
}

fn git_stdout(repo: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["-c", "protocol.ext.allow=never", "-c", "protocol.file.allow=never", "-c", "core.fsmonitor=false"])
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) fn relative_file_exists(root: &Path, rel_path: Option<&str>, require_nonempty: bool) -> bool {
    let Some(rel_path) = rel_path else {
        return false;
    };
    if !safe_relative_path(rel_path) {
        return false;
    }
    let path = root.join(rel_path);
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(link_metadata) = std::fs::symlink_metadata(&path) else {
        return false;
    };
    if link_metadata.file_type().is_symlink() || !link_metadata.is_file() {
        return false;
    }
    let Ok(canonical_path) = path.canonicalize() else {
        return false;
    };
    if !canonical_path.starts_with(root) {
        return false;
    }
    !require_nonempty || link_metadata.len() > 0
}

pub(super) fn relative_file_array_all_exist(root: &Path, value: Option<&Value>, require_nonempty: bool) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|paths| !paths.is_empty() && paths.iter().all(|path| relative_file_exists(root, path.as_str(), require_nonempty)))
}

pub(super) fn bruno_compatibility_patch_contract(repo_root: &Path, bruno_cwd: Option<&str>, patches: Option<&Value>) -> bool {
    match (bruno_cwd, patches) {
        (None, None) => true,
        (Some(bruno_cwd), Some(patches)) if safe_relative_path(bruno_cwd) => {
            let Some(patches) = patches.as_array() else {
                return false;
            };
            if patches.is_empty() {
                return false;
            }
            let bruno_root = repo_root.join(bruno_cwd);
            let Ok(repo_root) = repo_root.canonicalize() else {
                return false;
            };
            let Ok(metadata) = std::fs::symlink_metadata(&bruno_root) else {
                return false;
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return false;
            }
            let Ok(bruno_root) = bruno_root.canonicalize() else {
                return false;
            };
            bruno_root.starts_with(repo_root) && patches.iter().all(|path| relative_file_exists(&bruno_root, path.as_str(), true))
        }
        _ => false,
    }
}

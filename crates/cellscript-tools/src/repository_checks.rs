//! Native repository-policy checks used by every gate mode.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use percent_encoding::percent_decode_str;
use regex::Regex;

fn tracked_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "--recurse-submodules", "-z"])
        .current_dir(root)
        .output()
        .context("failed to enumerate tracked repository and submodule files")?;
    if !output.status.success() {
        bail!("git ls-files --recurse-submodules failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| std::str::from_utf8(path).map(PathBuf::from).context("tracked path is not valid UTF-8"))
        .filter(|path| path.as_ref().is_ok_and(|path| root.join(path).is_file()))
        .collect()
}

fn forbidden_source_artifact(path: &Path) -> bool {
    let forbidden_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "py" | "pyi" | "pyc" | "pyo"));
    forbidden_extension
        || path.file_name().and_then(|name| name.to_str()) == Some(".DS_Store")
        || path.components().any(|component| matches!(component.as_os_str().to_str(), Some("__pycache__" | ".cap")))
}

fn active_tooling_source(path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()).is_some_and(|name| matches!(name, "package.json" | "Makefile" | "Justfile")) {
        return true;
    }
    path.extension().and_then(|extension| extension.to_str()).is_some_and(|extension| {
        matches!(extension, "rs" | "sh" | "bash" | "zsh" | "yml" | "yaml" | "toml" | "mjs" | "js" | "ts" | "tsx")
    })
}

pub fn check_source_policy(root: &Path) -> Result<()> {
    let retired_runtime_name = ["py", "thon"].concat();
    let mut forbidden = Vec::new();
    let mut runtime_residue = Vec::new();
    for relative in tracked_paths(root)? {
        if forbidden_source_artifact(&relative) {
            forbidden.push(relative.clone());
        }
        if active_tooling_source(&relative) {
            let path = root.join(&relative);
            let text =
                fs::read_to_string(&path).with_context(|| format!("failed to read active tooling source {}", path.display()))?;
            if text.to_ascii_lowercase().contains(&retired_runtime_name) {
                runtime_residue.push(relative);
            }
        }
    }
    if forbidden.is_empty() && runtime_residue.is_empty() {
        return Ok(());
    }
    eprintln!("Repository source-language policy failed:");
    for path in forbidden {
        eprintln!("  forbidden source or generated artifact: {}", path.display());
    }
    for path in runtime_residue {
        eprintln!("  retired runtime residue in active tooling source: {}", path.display());
    }
    bail!("repository source-language policy failed")
}

fn normalized_head(path: &Path, lines: usize) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(text.lines().take(lines).flat_map(str::split_whitespace).collect::<Vec<_>>().join(" "))
}

pub fn check_doc_status(root: &Path) -> Result<()> {
    let readme = fs::read_to_string(root.join("README.md"))?;
    let link_re = Regex::new(r"\]\((docs/CELLSCRIPT_[^)#]+\.md)(?:#[^)]+)?\)")?;
    let mut docs = link_re
        .captures_iter(&readme)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_owned()))
        .collect::<BTreeSet<_>>();
    let tracked = Command::new("git").args(["ls-files", "docs/CELLSCRIPT_*.md"]).current_dir(root).output();
    if let Ok(output) = tracked
        && output.status.success()
    {
        for relative in String::from_utf8_lossy(&output.stdout).lines() {
            if root.join(relative).is_file() {
                docs.insert(relative.to_owned());
            }
        }
    }
    for entry in fs::read_dir(root.join("docs"))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if entry.path().is_file() && name.starts_with("CELLSCRIPT_") && name.ends_with(".md") {
            docs.insert(format!("docs/{name}"));
        }
    }
    let stale_patterns = [
        "formal 0.19 headless Rust adapter crate",
        "0.19 scope compatibility contract",
        "Active 0.19 grammar-governance contract",
        "Proposed. Implementation gated",
        "**Status**: In progress",
    ];
    let mut failures = Vec::new();
    for relative in docs {
        let path = root.join(&relative);
        if !path.is_file() {
            failures.push(format!("README-linked CellScript doc is missing: {relative}"));
            continue;
        }
        let head = normalized_head(&path, 40)?;
        for pattern in stale_patterns {
            if head.contains(pattern) {
                failures.push(format!("{relative} has stale Status header pattern: {pattern}"));
            }
        }
    }
    for (relative, marker) in [
        ("docs/CELLSCRIPT_CKB_ADAPTER.md", "production contract for the current CellScript CKB profile"),
        ("docs/CELLSCRIPT_CKB_STD_COMPAT.md", "production compatibility contract for the current CellScript CKB profile"),
        ("docs/CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md", "Active grammar-governance contract"),
        ("docs/CELLSCRIPT_WEBSITE_PARADIGM_UPGRADE_RFC.md", "Implemented across the 0.20-0.21 line"),
    ] {
        if !normalized_head(&root.join(relative), 20)?.contains(marker) {
            failures.push(format!("{relative} Status header is missing freshness marker: {marker}"));
        }
    }
    if !failures.is_empty() {
        eprintln!("CellScript documentation Status freshness check failed:");
        for failure in failures {
            eprintln!("  - {failure}");
        }
        bail!("documentation status freshness check failed");
    }
    Ok(())
}

fn collect_markdown(path: &Path, output: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_file() {
        output.push(path.to_owned());
        return Ok(());
    }
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            let name = entry.file_name();
            if [".git", ".mavis", "dist", "node_modules", "target"].iter().any(|skip| name == *skip) {
                continue;
            }
            collect_markdown(&entry_path, output)?;
        } else if entry_path.extension().and_then(|value| value.to_str()) == Some("md") {
            output.push(entry_path);
        }
    }
    Ok(())
}

pub fn check_markdown_links(root: &Path) -> Result<()> {
    let starts = [
        root.join("README.md"),
        root.join("docs"),
        root.join("roadmap"),
        root.join("editors/vscode-cellscript/README.md"),
        root.join("editors/vscode-cellscript/docs"),
    ];
    let mut files = Vec::new();
    for start in starts {
        collect_markdown(&start, &mut files)?;
    }
    files.sort();
    let link_re = Regex::new(r#"(!?)\[[^\]]+\]\(([^)\s]+(?:\s+\"[^\"]*\")?)\)"#)?;
    let mut failures = Vec::new();
    for path in files {
        let text = fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
        for (index, line) in text.lines().enumerate() {
            for capture in link_re.captures_iter(line) {
                if capture.get(1).is_some_and(|marker| marker.as_str() == "!") {
                    continue;
                }
                let mut raw = capture[2].trim().to_owned();
                if raw.contains(' ') && !raw.starts_with('<') {
                    raw.truncate(raw.find(' ').unwrap_or(raw.len()));
                }
                raw = raw.trim_matches(['<', '>']).to_owned();
                let target = raw.split('#').next().unwrap_or("");
                if target.is_empty()
                    || target.starts_with("http://")
                    || target.starts_with("https://")
                    || target.starts_with("mailto:")
                    || target.starts_with("tel:")
                    || target.starts_with("app://")
                    || target.starts_with('/')
                {
                    continue;
                }
                let decoded = percent_decode_str(target).decode_utf8_lossy();
                let candidate = path.parent().unwrap_or(root).join(decoded.as_ref());
                if !candidate.exists() {
                    let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                    failures.push(format!("{relative}:{}: missing local markdown link target {raw}", index + 1));
                }
            }
        }
    }
    if !failures.is_empty() {
        eprintln!("Local markdown link check failed:");
        for failure in failures {
            eprintln!("  - {failure}");
        }
        bail!("local Markdown link check failed");
    }
    Ok(())
}

pub fn check_package_contents(path: &Path) -> Result<()> {
    let allowed_files = [
        ".cargo_vcs_info.json",
        "Cargo.lock",
        "Cargo.toml",
        "Cargo.toml.orig",
        "CHANGELOG.md",
        "CODING_STYLE.md",
        "LICENSE-MIT",
        "README.md",
    ];
    let allowed_dirs = ["assets", "examples", "roadmap", "scripts", "src", "tests"];
    let mut unexpected = Vec::new();
    let contents = fs::read_to_string(path)?;
    for raw in contents.lines() {
        let item = raw.trim();
        if item.is_empty() {
            continue;
        }
        let root = item.split('/').next().unwrap_or(item);
        if item.ends_with(".pyc")
            || item.ends_with(".pyo")
            || item.contains("__pycache__/")
            || (!item.contains('/') && !allowed_files.contains(&item))
            || (item.contains('/') && !allowed_dirs.contains(&root))
        {
            unexpected.push(item);
        }
    }
    if !unexpected.is_empty() {
        eprintln!("crates.io package includes repository-only files:");
        for item in unexpected {
            eprintln!("  {item}");
        }
        bail!("package contents check failed");
    }
    Ok(())
}

pub fn workspace_version(root: &Path) -> Result<String> {
    let manifest: toml::Value = fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .context("Cargo.toml package.version is missing")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_policy_recognizes_forbidden_artifact_paths() {
        assert!(forbidden_source_artifact(Path::new("scripts/legacy.py")));
        assert!(forbidden_source_artifact(Path::new("src/__pycache__/legacy.pyc")));
        assert!(forbidden_source_artifact(Path::new(".cap/logs/run.log")));
        assert!(!forbidden_source_artifact(Path::new("src/main.rs")));
    }

    #[test]
    fn active_tooling_source_scope_excludes_historical_prose() {
        assert!(active_tooling_source(Path::new("src/main.rs")));
        assert!(active_tooling_source(Path::new(".github/workflows/ci.yml")));
        assert!(active_tooling_source(Path::new("website/package.json")));
        assert!(!active_tooling_source(Path::new("docs/archive/history.md")));
    }
}

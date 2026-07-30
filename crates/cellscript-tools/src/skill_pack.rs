//! CellScript skill-pack validator used by the repository gate.
//!
//! Validates that the CellScript programming skill-pack stays fresh against
//! the current CLI: every expected skill directory exists, each `SKILL.md`
//! carries the required YAML front-matter, every referenced file exists and
//! stays inside the repo, and every `cellc` command token used in a skill is
//! present in the live CLI registry extracted from `src/cli/commands.rs`.
//!
//! Behavioural contract (must match the Python script byte-for-byte on stdout
//! and on exit code; stderr text is allowed to differ):
//! - always emits exactly one JSON document on stdout (pass or fail);
//! - exit 0 iff no failures; exit 1 if any failure was recorded;
//! - a structurally malformed `SKILL.md` (missing/unterminated/malformed
//!   front matter) is a hard error: the Python original raises an uncaught
//!   `ValueError` and dies without emitting JSON; this port mirrors that by
//!   returning an `Err` before any JSON is printed.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::json;
use std::sync::OnceLock;

use crate::shared::python_json_pretty;

/// The expected skill directory names, mirrored verbatim from
/// `EXPECTED_SKILLS` in the Python script. Order is irrelevant (Python uses a
/// `set`); we keep them sorted for readability.
const EXPECTED_SKILLS: &[&str] = &[
    "cellscript-ckb-model",
    "cellscript-diagnostics",
    "cellscript-language-basics",
    "cellscript-metadata-audit",
    "cellscript-builder-deployment",
    "cellscript-package-cli",
];

/// The single regex used to extract visible CLI command names from
/// `src/cli/commands.rs`. Verbatim from the Python script.
fn cli_command_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"ClapCommand::new\("([^"]+)"\)"#).expect("CLI command regex must compile"))
}

/// A parsed front-matter field. Mirrors the Python
/// `dict[str, list[str] | str]` without collapsing scalars into lists: the
/// validator deliberately rejects scalar `references` and `commands`.
enum FrontMatterValue {
    Scalar(String),
    List(Vec<String>),
}

#[derive(Default)]
struct FrontMatter {
    fields: std::collections::BTreeMap<String, FrontMatterValue>,
}

/// Hand-rolled YAML front-matter parser, byte-for-byte compatible with
/// `parse_front_matter()` in the Python script.
///
/// Semantics mirrored exactly:
/// - the file MUST start with `---\n` at byte 0 (no leading whitespace, no
///   CRLF tolerance);
/// - the closing `---\n` is found by splitting the text on `---\n` at most
///   twice and taking part `[1]`;
/// - a missing closing delimiter is `unterminated front matter`;
/// - a list item must begin with exactly `"  - "` (two spaces, hyphen, space)
///   and must immediately follow a list-head line (the `current_list` reset
///   happens at the top of each non-list line);
/// - tabs are NOT accepted as indentation;
/// - a scalar line must contain `:`; `key: value` (value non-empty) stores a
///   scalar, `key:` (value empty) starts a list.
fn parse_front_matter(text: &str, path: &Path) -> anyhow::Result<FrontMatter> {
    if !text.starts_with("---\n") {
        return Err(anyhow::anyhow!("{} is missing YAML-style front matter", path.display()));
    }
    // Python: text.split("---\n", 2)[1]. The same split semantics: split into
    // at most 3 parts on the literal delimiter. The first part is everything
    // before the opening `---\n` (empty, since the file starts with it); the
    // second part is the front matter; the third is the body.
    let parts: Vec<&str> = text.splitn(3, "---\n").collect();
    let header = parts.get(1).ok_or_else(|| anyhow::anyhow!("{} has unterminated front matter", path.display()))?;

    let mut fm = FrontMatter::default();
    let mut current_list: Option<String> = None;

    for raw_line in header.split('\n') {
        // Python uses `raw_line.rstrip()` which strips only trailing
        // whitespace (spaces and tabs and newlines, but splitlines already
        // removed newlines). Rust's `trim_end` matches that.
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }
        // List item: must be exactly two-space indent + `- `.
        if let Some(rest) = line.strip_prefix("  - ") {
            let key =
                current_list.as_ref().ok_or_else(|| anyhow::anyhow!("{} has a list item outside a list: {}", path.display(), line))?;
            let value = rest.trim().to_string();
            match fm.fields.get_mut(key) {
                Some(FrontMatterValue::List(values)) => values.push(value),
                _ => unreachable!("current_list always names a list field"),
            }
            continue;
        }
        // Any non-list line resets the current list context.
        current_list = None;
        let Some((key, value)) = line.split_once(':') else {
            return Err(anyhow::anyhow!("{} has malformed front matter line: {}", path.display(), line));
        };
        let key = key.trim().to_string();
        let value = value.trim();
        if !value.is_empty() {
            // Scalar: overwrite any prior list/scalar (Python dict assignment).
            fm.fields.insert(key, FrontMatterValue::Scalar(value.to_string()));
        } else {
            // List head: replace any prior scalar/list with a fresh list,
            // matching Python's `result[key] = []`.
            fm.fields.insert(key.clone(), FrontMatterValue::List(Vec::new()));
            current_list = Some(key);
        }
    }
    Ok(fm)
}

/// Return a field only when the front matter represented it as a YAML list.
fn list<'a>(fm: &'a FrontMatter, key: &str) -> Option<&'a [String]> {
    match fm.fields.get(key) {
        Some(FrontMatterValue::List(values)) => Some(values),
        _ => None,
    }
}

/// Collect every `cellc` command token known to the live CLI, plus the
/// top-level `cellc` binary name. Mirrors `visible_command_names()`.
fn visible_command_names(root: &Path) -> anyhow::Result<BTreeSet<String>> {
    let source = fs::read_to_string(root.join("src/cli/commands.rs"))
        .map_err(|e| anyhow::anyhow!("failed to read src/cli/commands.rs: {e}"))?;
    let mut names: BTreeSet<String> =
        cli_command_regex().captures_iter(&source).filter_map(|c| c.get(1).map(|m| m.as_str().to_string())).collect();
    names.insert("cellc".to_string());
    Ok(names)
}

/// Discover every `docs/skills/cellscript-*/SKILL.md` and return the sorted
/// list of (absolute_path, skill_dir_name) pairs. Mirrors
/// `sorted((repo_root / "docs/skills").glob("cellscript-*/SKILL.md"))`.
fn discover_skills(root: &Path) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let base = root.join("docs/skills");
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    if !base.is_dir() {
        return Ok(found);
    }
    for entry in fs::read_dir(&base)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        if !dir_name.starts_with("cellscript-") {
            continue;
        }
        let skill_md = entry.path().join("SKILL.md");
        if skill_md.is_file() {
            found.push((skill_md, dir_name));
        }
    }
    // Python's `sorted(glob)` sorts the absolute PathBufs lexically; mirror
    // that by sorting on the path.
    found.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(found)
}

/// Validate a single skill file, appending any failure messages to `failures`.
/// Mirrors `validate_skill()` line by line, including the exact error message
/// wording and the `{path}` / `{reference}` / `{command}` / `{part}`
/// interpolation.
fn validate_skill(skill_md: &Path, fm: &FrontMatter, root: &Path, command_names: &BTreeSet<String>, failures: &mut Vec<String>) {
    let path_str = skill_md.display().to_string();

    // name: present and non-empty.
    let name_is_missing = match fm.fields.get("name") {
        None => true,
        Some(FrontMatterValue::Scalar(value)) => value.trim().is_empty(),
        // Python applies `str(...)` before `strip()`. Both `[]` and every
        // non-empty list therefore count as a present name.
        Some(FrontMatterValue::List(_)) => false,
    };
    if name_is_missing {
        failures.push(format!("{path_str}: missing name"));
    }

    // references: non-empty list.
    let references = list(fm, "references").unwrap_or(&[]);
    if references.is_empty() {
        failures.push(format!("{path_str}: missing references list"));
    }
    let mut has_current_doc_or_example = false;
    for reference in references {
        // Strip any `#anchor` suffix before path checks.
        let ref_path = reference.split('#').next().unwrap_or(reference);
        if ref_path.starts_with("../") || ref_path.contains("/../") {
            failures.push(format!("{path_str}: reference escapes repo root: {reference}"));
            continue;
        }
        let full = root.join(ref_path);
        if !full.exists() {
            failures.push(format!("{path_str}: referenced file does not exist: {reference}"));
            continue;
        }
        if ref_path.starts_with("docs/wiki/") || ref_path.starts_with("docs/CELLSCRIPT_") || ref_path.starts_with("examples/") {
            has_current_doc_or_example = true;
        }
    }
    if !has_current_doc_or_example {
        failures.push(format!("{path_str}: references must include current docs/wiki, docs/CELLSCRIPT_*, or examples files"));
    }

    // commands: non-empty list.
    let commands = list(fm, "commands").unwrap_or(&[]);
    if commands.is_empty() {
        failures.push(format!("{path_str}: missing commands list"));
    }
    for command in commands {
        let mut parts = command.split_whitespace();
        let first = parts.next();
        if first != Some("cellc") {
            failures.push(format!("{path_str}: command must start with 'cellc': {command}"));
            continue;
        }
        for part in parts {
            if part.starts_with('-') || part.starts_with('<') {
                continue;
            }
            if !command_names.contains(part) {
                failures.push(format!("{path_str}: command token is not present in CLI registry: {command} ({part})"));
            }
        }
    }
}

/// Entry point. Returns the exit code the binary should propagate.
///
/// On a structurally malformed `SKILL.md` the Python original raises an
/// uncaught `ValueError` (no JSON emitted). This port mirrors that: the
/// `anyhow::Error` propagates and `main.rs` prints it to stderr and returns
/// exit code 1 without printing any JSON.
pub fn run(root: &Path) -> anyhow::Result<i32> {
    let skill_files = discover_skills(root)?;
    let found: BTreeSet<String> = skill_files.iter().map(|(_, name)| name.clone()).collect();
    let expected: BTreeSet<String> = EXPECTED_SKILLS.iter().map(|s| s.to_string()).collect();

    let mut failures: Vec<String> = Vec::new();

    // Directory-level failures: missing then extra, in that fixed order.
    let missing: Vec<&String> = expected.difference(&found).collect();
    if !missing.is_empty() {
        let joined = missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        failures.push(format!("missing skill directories: {joined}"));
    }
    let extra: Vec<&String> = found.difference(&expected).collect();
    if !extra.is_empty() {
        let joined = extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
        failures.push(format!("unexpected CellScript skill directories: {joined}"));
    }

    let command_names = visible_command_names(root)?;
    for (skill_md, _name) in &skill_files {
        let text = fs::read_to_string(skill_md)?;
        // A malformed file propagates as a hard error (no JSON emitted),
        // mirroring the Python uncaught ValueError.
        let fm = parse_front_matter(&text, skill_md)?;
        validate_skill(skill_md, &fm, root, &command_names, &mut failures);
    }

    let status = if failures.is_empty() { "passed" } else { "failed" };
    let skills_sorted: Vec<&String> = found.iter().collect::<Vec<_>>();
    let report = json!({
        "schema": "cellscript-skill-pack-freshness-v0.22",
        "status": status,
        "skills": skills_sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        "skill_count": skill_files.len(),
        "failures": failures,
    });
    // Python: `json.dumps(report, indent=2, sort_keys=True)` followed by
    // `print()`. `serde_json::to_string_pretty` matches `indent=2`. Keys are
    // already sorted alphabetically because `report` is built from a
    // `serde_json::Map` (BTreeMap-backed when the `preserve_order` feature is
    // off, which it is here). The trailing newline from Python's `print()` is
    // added by `println!`.
    println!("{}", python_json_pretty(&report)?);

    Ok(if failures.is_empty() { 0 } else { 1 })
}

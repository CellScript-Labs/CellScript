//! Shared helpers for the cellscript-tools binaries.
//!
//! These helpers preserve the historical report encodings and path semantics
//! so the native Rust tools remain compatible with existing evidence.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Resolve the CellScript repository root.
///
/// Mirrors the Python scripts' `Path(__file__).resolve().parents[1]` (the
/// parent of `scripts/`), but the Rust binary does not live under `scripts/`,
/// so resolution is performed by walking up from the current directory until a
/// `Cargo.toml` declaring `name = "cellscript"` is found.
///
/// `--root` overrides the walk and is canonicalised, matching
/// `Path(__file__).resolve()` in the Python scripts. This matters on platforms
/// such as macOS where `/var` resolves to `/private/var`.
pub fn resolve_repo_root(override_root: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(root) = override_root {
        return fs::canonicalize(root).map_err(|e| anyhow::anyhow!("failed to resolve repository root {}: {e}", root.display()));
    }
    let cwd = std::env::current_dir().map_err(|e| anyhow::anyhow!("failed to read current directory: {e}"))?;
    for dir in cwd.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && let Ok(text) = fs::read_to_string(&manifest)
            && text.lines().any(|line| line.trim() == "name = \"cellscript\"")
        {
            return Ok(dir.to_path_buf());
        }
    }
    anyhow::bail!(
        "could not locate the CellScript repository root \
         (no Cargo.toml with name = \"cellscript\" found by walking up from cwd); \
         pass --root <PATH> explicitly"
    )
}

/// Read a UTF-8 text file relative to the repo root.
///
/// Mirrors `read(path)` in the Python tooling scripts, which always reads
/// `(ROOT / path)` as UTF-8 and propagates `FileNotFoundError` on absence.
pub fn read_text(root: &Path, relative: &str) -> anyhow::Result<String> {
    let full = root.join(relative);
    fs::read_to_string(&full).map_err(|e| anyhow::anyhow!("failed to read {}: {e}", full.display()))
}

/// Substring containment check.
///
/// Mirrors `token in text` from the Python `require_contains` helper: a plain
/// substring match, not a line-based one. Tokens may contain embedded
/// newlines; the match is byte-for-byte on the original text.
pub fn contains(text: &str, token: &str) -> bool {
    text.contains(token)
}

/// Slice the text strictly between two marker substrings.
///
/// Mirrors the Python pattern
/// `text.split(start, 1)[1].split(end, 1)[0]`, returning the text after the
/// first `start` and before the first subsequent `end`.
///
/// Unlike the Python original, which raises `IndexError` when a marker is
/// missing, this surfaces a clean error message identifying the missing
/// marker. The dev/CI gate compares stdout and exit code only, so this is a
/// strictly-better diagnostic.
pub fn slice_between<'a>(text: &'a str, start: &str, end: &str) -> anyhow::Result<&'a str> {
    let after_start = text
        .split_once(start)
        .map(|(_, rest)| rest)
        .ok_or_else(|| anyhow::anyhow!("slice_between: start marker not found: {start:?}"))?;
    let before_end = after_start
        .split_once(end)
        .map(|(before, _)| before)
        .ok_or_else(|| anyhow::anyhow!("slice_between: end marker not found: {end:?}"))?;
    Ok(before_end)
}

/// Apply the lexical normalisation performed by Python's `pathlib.Path`:
/// collapse repeated separators and `.` components without resolving
/// symlinks or parent components.
pub fn python_path(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Render a JSON value like Python's
/// `json.dumps(value, indent=2, sort_keys=True)`.
pub fn python_json_pretty(value: &Value) -> anyhow::Result<String> {
    let json = serde_json::to_string_pretty(value)?;
    Ok(escape_json_non_ascii(&json))
}

/// Render a JSON value like Python's
/// `json.dumps(value, sort_keys=True, separators=(",", ":"))`.
pub fn python_json_compact(value: &Value) -> anyhow::Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(escape_json_non_ascii(&json))
}

/// Render a JSON value like Python's `json.dumps(value, sort_keys=True)`.
/// Python's default compact formatter keeps one space after commas and
/// colons; serde_json's compact formatter does not, so add those separators
/// while respecting string literals and escapes.
pub fn python_json_default(value: &Value) -> anyhow::Result<String> {
    let json = serde_json::to_string(value)?;
    let mut rendered = String::with_capacity(json.len() + json.len() / 8);
    let mut in_string = false;
    let mut escaped = false;
    for character in json.chars() {
        rendered.push(character);
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if matches!(character, ',' | ':') {
            rendered.push(' ');
        }
    }
    Ok(escape_json_non_ascii(&rendered))
}

/// Match Python's default `ensure_ascii=True` JSON behaviour. `serde_json`
/// emits non-ASCII Unicode directly, while Python writes UTF-16 `\u` escapes
/// (including surrogate pairs for non-BMP characters).
fn escape_json_non_ascii(json: &str) -> String {
    let mut escaped = String::with_capacity(json.len());
    for character in json.chars() {
        if character.is_ascii() {
            escaped.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                use std::fmt::Write as _;
                write!(escaped, "\\u{unit:04x}").expect("writing to String cannot fail");
            }
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn python_default_json_spacing_ignores_string_punctuation() {
        assert_eq!(python_json_default(&json!({"a": [1, 2], "b": "x,y:z\""})).unwrap(), r#"{"a": [1, 2], "b": "x,y:z\""}"#);
    }
}

//! Shared helpers for the cellscript-tools binaries.
//!
//! These helpers preserve stable report encodings and path semantics so native
//! tools remain compatible with existing evidence.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Resolve the CellScript repository root.
///
/// Resolution walks up from the current directory until a `Cargo.toml`
/// declaring `name = "cellscript"` is found.
///
/// `--root` overrides the walk and is canonicalised. This matters on platforms
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
/// The path is resolved beneath `root` and decoded as UTF-8.
pub fn read_text(root: &Path, relative: &str) -> anyhow::Result<String> {
    let full = root.join(relative);
    fs::read_to_string(&full).map_err(|e| anyhow::anyhow!("failed to read {}: {e}", full.display()))
}

/// Substring containment check.
///
/// This is a plain substring match, not a line-based one. Tokens may contain
/// embedded newlines; the match is byte-for-byte on the original text.
pub fn contains(text: &str, token: &str) -> bool {
    text.contains(token)
}

/// Slice the text strictly between two marker substrings.
///
/// Returns the text after the first `start` and before the first subsequent
/// `end`, with a diagnostic naming either missing marker.
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

/// Collapse repeated separators and `.` components without resolving symlinks
/// or parent components.
pub fn lexical_path(path: &Path) -> PathBuf {
    path.components().collect()
}

/// Render stable pretty JSON with sorted object keys and ASCII-only escapes.
pub fn stable_json_pretty(value: &Value) -> anyhow::Result<String> {
    let json = serde_json::to_string_pretty(value)?;
    Ok(escape_json_non_ascii(&json))
}

/// Render stable compact JSON with sorted object keys and ASCII-only escapes.
pub fn stable_json_compact(value: &Value) -> anyhow::Result<String> {
    let json = serde_json::to_string(value)?;
    Ok(escape_json_non_ascii(&json))
}

/// Render stable single-line JSON with one space after commas and colons.
pub fn stable_json_spaced(value: &Value) -> anyhow::Result<String> {
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

/// Escape non-ASCII text as UTF-16 `\u` units, including surrogate pairs for
/// non-BMP characters, so report bytes remain platform-independent.
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
    fn stable_spaced_json_spacing_ignores_string_punctuation() {
        assert_eq!(stable_json_spaced(&json!({"a": [1, 2], "b": "x,y:z\""})).unwrap(), r#"{"a": [1, 2], "b": "x,y:z\""}"#);
    }
}

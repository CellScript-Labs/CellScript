//! Shared helpers for the cellscript-tools binaries.
//!
//! These helpers mirror the behaviour of the in-tree Python scripts under
//! `scripts/`. Behavioural fidelity matters: the dev/CI gate runs both the
//! Python and Rust implementations and requires byte-identical stdout and a
//! matching exit code. See `scripts/dev/dual_run_tools.sh`.

use std::fs;
use std::path::{Path, PathBuf};

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

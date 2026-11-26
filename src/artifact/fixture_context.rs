//! Explicit test setup, compiled only by unit/integration test harnesses.
//! These comments have no compiler or package-manifest authority.
use serde::Deserialize;

pub(super) const MARKER: &str = "// test-artifact: ";

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct FixtureContext {
    pub entry: String,
    pub layout: String,
}

pub(super) fn parse(source: &str) -> Result<Option<FixtureContext>, String> {
    let mut rows = source
        .lines()
        .map(str::trim)
        .take_while(|line| line.is_empty() || line.starts_with("//"))
        .filter_map(|line| line.strip_prefix(MARKER));
    let Some(row) = rows.next() else { return Ok(None) };
    if rows.next().is_some() {
        return Err("test fixture contains multiple artifact declarations".into());
    }
    serde_json::from_str(row).map(Some).map_err(|error| format!("invalid test artifact declaration: {error}"))
}

use std::{path::PathBuf, process::Command};

pub fn cellc_bin() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_BIN_EXE_cellc"));
    let path = if path.is_absolute() { path } else { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path) };

    if path.file_name().and_then(|name| name.to_str()) == Some("cellc") {
        if let Some(debug_dir) = path.parent() {
            if let Some(candidate) = newest_hashed_cellc_bin(&debug_dir.join("deps")) {
                return candidate;
            }
        }
    }
    path
}

fn newest_hashed_cellc_bin(deps_dir: &std::path::Path) -> Option<PathBuf> {
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(deps_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        let file_name = path.file_name()?.to_str()?;
        if !file_name.starts_with("cellc-") || path.extension().is_some() {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if newest.as_ref().is_none_or(|(seen, _)| modified > *seen) {
            newest = Some((modified, path));
        }
    }
    newest.map(|(_, path)| path)
}

#[allow(dead_code)]
pub fn cellc_command() -> Command {
    Command::new(cellc_bin())
}

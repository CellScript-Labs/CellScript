use std::{path::PathBuf, process::Command};

pub fn cellc_bin() -> PathBuf {
    cargo_binary_path(PathBuf::from(env!("CARGO_BIN_EXE_cellc")), PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

pub(super) fn cargo_binary_path(path: PathBuf, manifest: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        manifest.join(path)
    }
}

pub fn cellc_command() -> Command {
    Command::new(cellc_bin())
}

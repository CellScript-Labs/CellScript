use std::{path::PathBuf, process::Command};

pub fn cellc_bin() -> PathBuf {
    cargo_binary_path(PathBuf::from(env!("CARGO_BIN_EXE_cellc")), PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn cargo_binary_path(path: PathBuf, manifest: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        manifest.join(path)
    }
}

pub fn cellc_command() -> Command {
    Command::new(cellc_bin())
}

#[cfg(test)]
mod binary_identity_tests {
    use super::*;

    #[test]
    fn cargo_identity_wins_over_neighboring_stale_feature_builds() {
        let root = tempfile::tempdir().unwrap();
        let deps = root.path().join("target/debug/deps");
        std::fs::create_dir_all(&deps).unwrap();
        let declared = root.path().join("target/debug/cellc");
        std::fs::write(&declared, "Cargo-selected cli+lsp build").unwrap();
        for name in ["cellc-stale-cli", "cellc-newer-no-default-features"] {
            std::fs::write(deps.join(name), "unrelated build").unwrap();
        }
        let selected = cargo_binary_path(declared.clone(), root.path().into());
        assert_eq!(selected, declared);
        assert_eq!(std::fs::read_to_string(selected).unwrap(), "Cargo-selected cli+lsp build");
        assert_eq!(cargo_binary_path("target/debug/cellc".into(), root.path().into()), declared);
    }
}

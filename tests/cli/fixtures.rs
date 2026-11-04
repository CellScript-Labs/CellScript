//! Each CLI test owns its mutable package, dependency tree, cache and outputs.
use std::path::Path;

pub(super) fn examples() -> tempfile::TempDir {
    let directory = tempfile::Builder::new().prefix("cellscript-cli-examples-").tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    copy_sources(&source, directory.path()).unwrap();
    directory
}

fn copy_sources(source: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if [".cell", ".git", "build", "target", "node_modules"].iter().any(|excluded| name == *excluded) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("fixture symlink: {}", entry.path().display())));
        }
        let destination = target.join(name);
        if kind.is_dir() {
            copy_sources(&entry.path(), &destination)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), destination)?;
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "fixture is not a regular file or directory"));
        }
    }
    Ok(())
}

#[test]
fn package_copies_keep_sources_caches_and_outputs_separate() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(source.path().join("src")).unwrap();
    std::fs::write(source.path().join("src/main.cell"), b"source").unwrap();
    for generated in ["build", ".cell", "target"] {
        std::fs::create_dir_all(source.path().join(generated)).unwrap();
        std::fs::write(source.path().join(generated).join("preserve"), b"original output").unwrap();
    }
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    copy_sources(source.path(), first.path()).unwrap();
    copy_sources(source.path(), second.path()).unwrap();
    for generated in ["build", ".cell", "target"] {
        assert!(!first.path().join(generated).exists());
        assert!(!second.path().join(generated).exists());
    }
    std::fs::write(first.path().join("src/main.cell"), b"edited").unwrap();
    std::fs::create_dir_all(first.path().join("build")).unwrap();
    std::fs::write(first.path().join("build/generated"), b"first result").unwrap();
    assert_eq!(std::fs::read(second.path().join("src/main.cell")).unwrap(), b"source");
    assert_eq!(std::fs::read(source.path().join("src/main.cell")).unwrap(), b"source");
    assert_eq!(std::fs::read(source.path().join("build/preserve")).unwrap(), b"original output");
    assert!(!second.path().join("build").exists());
}

#[cfg(unix)]
#[test]
fn package_copy_rejects_links_to_shared_inputs() {
    let source = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("shared.cell"), b"shared").unwrap();
    std::os::unix::fs::symlink(outside.path().join("shared.cell"), source.path().join("alias.cell")).unwrap();
    assert_eq!(copy_sources(source.path(), target.path()).unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(std::fs::read(outside.path().join("shared.cell")).unwrap(), b"shared");
}

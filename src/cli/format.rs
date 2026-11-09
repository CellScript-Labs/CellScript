//! Prepare every source rewrite before replacing any file. Replacements are
//! atomic per file; an I/O failure reports the already replaced prefix.
use crate::error::{CompileError, Result};
use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

struct Temporary(Utf8PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn unchanged(path: &Utf8PathBuf, original: &str) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err(CompileError::without_span(format!("format refuses a symlink, non-file or read-only source: {path}")));
    }
    if std::fs::read_to_string(path)? != original {
        return Err(CompileError::without_span(format!("source changed while formatting: {path}; no stale rewrite was applied")));
    }
    Ok(())
}

pub(super) fn write_sources(changes: &BTreeMap<Utf8PathBuf, (String, String)>) -> Result<()> {
    write_sources_with_replace(changes, |from, to| std::fs::rename(from, to))
}

fn write_sources_with_replace(
    changes: &BTreeMap<Utf8PathBuf, (String, String)>,
    mut replace: impl FnMut(&Utf8PathBuf, &Utf8PathBuf) -> std::io::Result<()>,
) -> Result<()> {
    for (path, (original, _)) in changes {
        unchanged(path, original)?;
    }
    let mut staged = Vec::new();
    for (path, (_, candidate)) in changes {
        let parent = path.parent().ok_or_else(|| CompileError::without_span("format source has no parent directory"))?;
        let nonce = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = Temporary(parent.join(format!(".cellscript-fmt-{}-{nonce}.tmp", std::process::id())));
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary.0)?;
        file.write_all(candidate.as_bytes())?;
        file.set_permissions(std::fs::metadata(path)?.permissions())?;
        file.sync_all()?;
        staged.push((path, temporary));
    }
    // Staging may take time. Check the complete source snapshot again before
    // the first replacement, and each file immediately before its replacement.
    for (path, (original, _)) in changes {
        unchanged(path, original)?;
    }
    for (written, (path, temporary)) in staged.iter().enumerate() {
        let result = unchanged(path, &changes[*path].0).and_then(|()| {
            replace(&temporary.0, path)?;
            Ok(())
        });
        if let Err(error) = result {
            return Err(CompileError::without_span(format!(
                "format stopped after replacing {written} file(s); package formatting was not atomic; failed at {path}: {error}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_second_source_prevents_writing_the_first_source() {
        let directory = tempfile::tempdir().unwrap();
        let a = Utf8PathBuf::from_path_buf(directory.path().join("a.cell")).unwrap();
        let b = Utf8PathBuf::from_path_buf(directory.path().join("b.cell")).unwrap();
        std::fs::write(&a, "first").unwrap();
        std::fs::write(&b, "newer edit").unwrap();
        let changes = BTreeMap::from([
            (a.clone(), ("first".into(), "formatted first".into())),
            (b.clone(), ("older edit".into(), "formatted second".into())),
        ]);
        assert!(write_sources(&changes).unwrap_err().message.contains("source changed"));
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "newer edit");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn replacement_failure_reports_the_written_prefix_and_cleans_staged_files() {
        let directory = tempfile::tempdir().unwrap();
        let a = Utf8PathBuf::from_path_buf(directory.path().join("a.cell")).unwrap();
        let b = Utf8PathBuf::from_path_buf(directory.path().join("b.cell")).unwrap();
        std::fs::write(&a, "first").unwrap();
        std::fs::write(&b, "second").unwrap();
        let changes = BTreeMap::from([
            (a.clone(), ("first".into(), "formatted first".into())),
            (b.clone(), ("second".into(), "formatted second".into())),
        ]);
        let error = write_sources_with_replace(&changes, |from, to| {
            if to == &b {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "injected rename failure"))
            } else {
                std::fs::rename(from, to)
            }
        })
        .unwrap_err();
        assert!(error.message.contains("after replacing 1 file(s)"), "{error}");
        assert!(error.message.contains(b.as_str()), "{error}");
        assert!(error.message.contains("injected rename failure"), "{error}");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "formatted first");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "second");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_late_source_edit_is_retained_and_reports_partial_progress() {
        let directory = tempfile::tempdir().unwrap();
        let a = Utf8PathBuf::from_path_buf(directory.path().join("a.cell")).unwrap();
        let b = Utf8PathBuf::from_path_buf(directory.path().join("b.cell")).unwrap();
        std::fs::write(&a, "first").unwrap();
        std::fs::write(&b, "second").unwrap();
        let changes = BTreeMap::from([
            (a.clone(), ("first".into(), "formatted first".into())),
            (b.clone(), ("second".into(), "formatted second".into())),
        ]);
        let mut replacements = 0;
        let error = write_sources_with_replace(&changes, |from, to| {
            replacements += 1;
            std::fs::rename(from, to)?;
            // Deterministically model another writer after the first rename.
            std::fs::write(&b, "newer edit")
        })
        .unwrap_err();
        assert_eq!(replacements, 1);
        assert!(error.message.contains("after replacing 1 file(s)"), "{error}");
        assert!(error.message.contains("source changed"), "{error}");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "formatted first");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "newer edit");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }
}

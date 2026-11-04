//! Checkout-only freshness guard. Package unit tests use their own bounded data.
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Component, Path};

#[test]
fn certification_unit_resources_match_current_proposal_inputs() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resources = repository.join("crates/cellscript-novaseal-certification/tests/resources");
    let index: serde_json::Value = serde_json::from_slice(&std::fs::read(resources.join("resource_index.json")).unwrap()).unwrap();
    assert_eq!(index["schema"], "cellscript-novaseal-unit-resources-v1");
    let mut previous = None;
    let mut expected = BTreeSet::new();
    for row in index["files"].as_array().unwrap() {
        let relative = row["path"].as_str().unwrap();
        let path = Path::new(relative);
        assert!(path.components().all(|part| matches!(part, Component::Normal(_))));
        assert!(path.starts_with("proposals/novaseal"));
        assert!(previous.is_none_or(|before| before < relative), "resource paths must be sorted and unique");
        previous = Some(relative);
        expected.insert(path.to_path_buf());
        let packaged = std::fs::read(resources.join(path)).unwrap();
        let checkout = std::fs::read(repository.join(path)).unwrap();
        assert_eq!(packaged, checkout, "unit resource differs from current proposal input: {relative}");
        assert_eq!(row["bytes"].as_u64(), Some(packaged.len() as u64), "{relative}");
        assert_eq!(row["sha256"].as_str(), Some(hex::encode(Sha256::digest(&packaged)).as_str()), "{relative}");
    }
    fn collect(directory: &Path, root: &Path, paths: &mut BTreeSet<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            assert!(!entry.file_type().unwrap().is_symlink(), "unit resources must be ordinary package files");
            if entry.file_type().unwrap().is_dir() {
                collect(&entry.path(), root, paths);
            } else {
                paths.insert(entry.path().strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut actual = BTreeSet::new();
    collect(&resources.join("proposals"), &resources, &mut actual);
    assert_eq!(actual, expected);
}

use cellscript_fiber_adapter::{check_path, check_path_for};
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("repository root")
}

#[test]
fn bounded_fiber_examples_compile_through_the_dedicated_entry() {
    let root = repository_root().join("examples/fiber");
    for (file, expected_type) in [
        ("ordinary_fungible.cell", "FiberToken"),
        ("fixed_supply.cell", "FixedAsset"),
        ("governed_supply_cap.cell", "GovernedUsd"),
        ("reserve_compliance.cell", "RegulatedUsd"),
        ("wrapped_bridge.cell", "WrappedAsset"),
        ("type_id_upgradeable.cell", "UpgradeableAsset"),
    ] {
        let checked = check_path(root.join(file)).unwrap_or_else(|error| panic!("{file} failed Fiber compatibility: {error:#}"));
        assert_eq!(checked.descriptor.selected_type, expected_type);
    }
}

#[test]
fn multi_asset_example_requires_and_honours_an_explicit_selection() {
    let source = repository_root().join("examples/fiber/multi_asset.cell");
    assert!(check_path(&source).is_err(), "ambiguous multi-asset package must fail without --asset");
    for selected in ["FiberUsd", "FiberEur"] {
        let checked = check_path_for(&source, selected).unwrap_or_else(|error| panic!("{selected} selection failed: {error:#}"));
        assert_eq!(checked.descriptor.selected_type, selected);
    }
    assert!(check_path_for(source, "MissingAsset").is_err());
}

#![cfg(all(feature = "cli", not(feature = "wasm")))]
mod common;
use camino::Utf8Path;
use cellscript::package::frozen_interface::{compile_module, freeze_module_catalog, EntrySelection, FrozenPackageModule};
use cellscript::CompileOptions;
use serde_json::Value;
use std::path::Path;

const SOURCE: &str = "module client\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\npublic action verify(witness value: Value) { verification require value.amount > 0 }\n";
fn package(root: &Path, name: &str, source: &str, extra: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), format!("[package]\nedition = \"2027\"\nname = \"{name}\"\nversion = \"1.0.0\"\n[environments.dev]\nchain_id = \"test-chain\"\ngenesis_hash = \"0x{}\"\n{extra}", "11".repeat(32))).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
}
fn lock(root: &Path) {
    let output = common::cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}
fn compile(root: &Path, opt_level: u8) -> FrozenPackageModule {
    compile_module(
        Utf8Path::from_path(root).unwrap(),
        "dev",
        CompileOptions { opt_level, ..CompileOptions::default() },
        EntrySelection::Action("verify".into()),
    )
    .unwrap()
}
fn context(value: &FrozenPackageModule) -> Value {
    serde_json::from_slice(&value.context_bytes().unwrap()).unwrap()
}

#[test]
fn native_catalog_owns_actual_source_snapshots_and_checked_bundles() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let candidate = directory.path().join("candidate");
    package(&original, "client", SOURCE, "");
    package(&candidate, "client", &format!("{SOURCE}public struct Extra has copy, drop, store {{ value: u8 }}\n"), "");
    lock(&original);
    lock(&candidate);
    for opt in 0..=3 {
        std::fs::write(original.join("src/main.cell"), SOURCE).unwrap();
        let required = compile(&original, opt);
        let required_context = required.context_identity().to_owned();
        let candidates = vec![compile(&original, opt), compile(&candidate, opt)];
        let source_contexts = candidates.iter().map(|candidate| candidate.context_identity().to_owned()).collect::<Vec<_>>();
        // A snapshot remains exact evidence of its stored sources, not a promise
        // about whichever bytes the path contains after compilation.
        std::fs::write(original.join("src/main.cell"), "module changed\naction deny() { verification require false }\n").unwrap();
        let catalog = freeze_module_catalog(required, candidates, &cellscript_artifact_checker::CheckerBudgets::default()).unwrap();
        assert_eq!(catalog.required().context_identity(), required_context);
        let record: Value = serde_json::from_slice(&catalog.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["required_source_context"], required_context);
        assert_eq!(record["candidate_source_contexts"], serde_json::to_value(source_contexts).unwrap());
        assert_eq!(record["checked_catalog"], catalog.evidence().identity());
        let bundles = catalog.candidates().iter().map(FrozenPackageModule::bundle).collect::<Vec<_>>();
        catalog.evidence().check_unchanged_inputs(catalog.required().bundle(), &bundles).unwrap();
        assert!(!catalog.required().projection().artifact_report().semantic_equivalence_claimed);
    }
}

#[test]
fn native_catalog_rejects_other_pinned_networks_missing_candidates_and_narrow_budgets() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    package(&original, "client", SOURCE, "");
    lock(&original);
    for (chain, genesis) in [("other-chain", "11"), ("test-chain", "22")] {
        let candidate = directory.path().join(format!("{chain}-{genesis}"));
        package(&candidate, "client", SOURCE, "");
        let manifest = candidate.join("Cell.toml");
        let text =
            std::fs::read_to_string(&manifest).unwrap().replace("test-chain", chain).replace(&"11".repeat(32), &genesis.repeat(32));
        std::fs::write(manifest, text).unwrap();
        lock(&candidate);
        let error = freeze_module_catalog(
            compile(&original, 0),
            vec![compile(&candidate, 0)],
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap_err();
        assert!(error.message.contains("conflicting pinned chain identities"));
    }
    assert!(freeze_module_catalog(compile(&original, 0), Vec::new(), &cellscript_artifact_checker::CheckerBudgets::default()).is_err());
    assert!(freeze_module_catalog(
        compile(&original, 0),
        vec![compile(&original, 0)],
        &cellscript_artifact_checker::CheckerBudgets { instructions: 1, ..Default::default() }
    )
    .is_err());
}

#[test]
fn frozen_context_binds_actual_sources_and_pinned_environment_without_lock_writes() {
    let directory = tempfile::tempdir().unwrap();
    package(directory.path(), "client", SOURCE, "");
    lock(directory.path());
    let lock_bytes = std::fs::read(directory.path().join("Cell.lock")).unwrap();
    let mut identity = None;
    for opt in 0..=3 {
        let compiled = compile(directory.path(), opt);
        let value = context(&compiled);
        assert_eq!(value["entry_module"], "client");
        assert_eq!(value["chain_id"], "test-chain");
        assert_eq!(value["network_genesis"], format!("0x{}", "11".repeat(32)));
        assert_eq!(value["modules"]["client"]["relative_path"], "src/main.cell");
        let package_id = value["modules"]["client"]["package"].as_str().unwrap();
        assert_eq!(value["packages"][package_id]["name"], "client");
        assert_eq!(value["packages"][package_id]["source"]["kind"], "root-snapshot");
        assert!(!String::from_utf8(compiled.context_bytes().unwrap()).unwrap().contains(directory.path().to_str().unwrap()));
        let bytes = compiled.bundle();
        cellscript_artifact_checker::check_bundle(
            bytes[0],
            bytes[1],
            bytes[2],
            bytes[3],
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap();
        assert_eq!(compiled.projection().module(), "client");
        assert!(!compiled.projection().artifact_report().semantic_equivalence_claimed);
        if let Some(previous) = &identity {
            assert_eq!(previous, compiled.context_identity());
        } else {
            identity = Some(compiled.context_identity().to_owned());
        }
        assert_eq!(std::fs::read(directory.path().join("Cell.lock")).unwrap(), lock_bytes);
    }
}

#[test]
fn portable_local_snapshot_uses_defining_sources_without_physical_directory_identity() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let moved = directory.path().join("moved");
    package(&original, "client", SOURCE, "");
    lock(&original);
    package(&moved, "client", SOURCE, "");
    std::fs::copy(original.join("Cell.lock"), moved.join("Cell.lock")).unwrap();
    let first = compile(&original, 1);
    let second = compile(&moved, 1);
    assert_eq!(first.context_identity(), second.context_identity());
    assert_eq!(first.context_bytes().unwrap(), second.context_bytes().unwrap());
    assert_eq!(first.projection().identity(), second.projection().identity());
}

#[test]
fn imported_aliases_bind_one_actual_locked_defining_package_and_detect_source_changes() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let client = directory.path().join("client");
    package(
        &library,
        "library",
        "module foreign\npublic struct Payload has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\n",
        "",
    );
    package(&client, "client", "module client\nuse foreign::Payload as Value\npublic action verify(witness value: Value) { verification require value.amount > 0 }\n", "[dependencies]\nleft = { package = \"library\", path = \"../library\" }\nright = { package = \"library\", path = \"../library\" }\n");
    lock(&client);
    let snapshot = compile(&client, 1);
    let value = context(&snapshot);
    assert_eq!(value["packages"].as_object().unwrap().len(), 2);
    assert_eq!(value["modules"].as_object().unwrap().len(), 2);
    let owner = value["modules"]["foreign"]["package"].as_str().unwrap();
    assert_eq!(value["packages"][owner]["name"], "library");
    assert_eq!(value["packages"][owner]["source"]["kind"], "local-snapshot");
    let old_lock = std::fs::read(client.join("Cell.lock")).unwrap();
    std::fs::write(
        library.join("src/main.cell"),
        "module foreign\npublic struct Payload has copy, drop, store, fixed, serializable, non_linear { amount: u32 }\n",
    )
    .unwrap();
    assert!(compile_module(Utf8Path::from_path(&client).unwrap(), "dev", CompileOptions::default(), EntrySelection::Default).is_err());
    assert_eq!(std::fs::read(client.join("Cell.lock")).unwrap(), old_lock);
}

#[test]
fn missing_or_stale_frozen_inputs_never_repin_or_create_contexts() {
    let directory = tempfile::tempdir().unwrap();
    package(directory.path(), "client", SOURCE, "");
    let root = Utf8Path::from_path(directory.path()).unwrap();
    assert!(compile_module(root, "dev", CompileOptions::default(), EntrySelection::Default).is_err());
    assert!(!directory.path().join("Cell.lock").exists());
    lock(directory.path());
    let pinned = std::fs::read(directory.path().join("Cell.lock")).unwrap();
    assert!(compile_module(root, "unknown", CompileOptions::default(), EntrySelection::Default).is_err());
    assert!(
        compile_module(root, "dev", CompileOptions { opt_level: 4, ..CompileOptions::default() }, EntrySelection::Default).is_err()
    );
    assert!(compile_module(
        root,
        "dev",
        CompileOptions { target: Some("riscv64-asm".into()), ..CompileOptions::default() },
        EntrySelection::Default
    )
    .is_err());
    assert!(compile_module(root, "dev", CompileOptions::default(), EntrySelection::Action("absent".into())).is_err());
    assert!(compile_module(root, "dev", CompileOptions::default(), EntrySelection::Artifact("undeclared".into())).is_err());
    std::fs::write(directory.path().join("src/main.cell"), SOURCE.replace("module client", "module impostor")).unwrap();
    assert!(compile_module(root, "dev", CompileOptions::default(), EntrySelection::Default).is_err());
    assert_eq!(std::fs::read(directory.path().join("Cell.lock")).unwrap(), pinned);
    std::fs::write(directory.path().join("Cell.lock"), vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    let error = compile_module(root, "dev", CompileOptions::default(), EntrySelection::Default).unwrap_err();
    assert!(error.message.contains("bounded Cell.lock"));
}

#[test]
fn source_preflight_rejects_large_deep_or_overpopulated_trees_before_source_hashing() {
    for case in ["large", "deep", "count"] {
        let directory = tempfile::tempdir().unwrap();
        package(directory.path(), "client", SOURCE, "");
        lock(directory.path());
        let pinned = std::fs::read(directory.path().join("Cell.lock")).unwrap();
        let expected = match case {
            "large" => {
                std::fs::write(directory.path().join("src/main.cell"), vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
                "4 MiB/file"
            }
            "deep" => {
                let path = directory.path().join("src").join(vec!["nested"; 18].join("/"));
                std::fs::create_dir_all(path).unwrap();
                "depth exceeds 16"
            }
            _ => {
                for index in 0..256 {
                    // Deliberately invalid source: the bound must reject before
                    // parsing/source hashing can turn this into another error.
                    std::fs::write(directory.path().join(format!("src/{index}.cell")), "invalid").unwrap();
                }
                "module count exceeds 256"
            }
        };
        let error =
            compile_module(Utf8Path::from_path(directory.path()).unwrap(), "dev", CompileOptions::default(), EntrySelection::Default)
                .unwrap_err();
        assert!(error.message.contains(expected), "{case}: {error}");
        assert_eq!(std::fs::read(directory.path().join("Cell.lock")).unwrap(), pinned);
    }
}

#[cfg(unix)]
#[test]
fn source_preflight_rejects_symlink_loops_and_linked_lockfiles() {
    let directory = tempfile::tempdir().unwrap();
    package(directory.path(), "client", SOURCE, "");
    lock(directory.path());
    let pinned = std::fs::read(directory.path().join("Cell.lock")).unwrap();
    let link = directory.path().join("src/loop");
    std::os::unix::fs::symlink(directory.path().join("src"), &link).unwrap();
    let root = Utf8Path::from_path(directory.path()).unwrap();
    let error = compile_module(root, "dev", CompileOptions::default(), EntrySelection::Default).unwrap_err();
    assert!(error.message.contains("symbolic links"), "{error}");
    std::fs::remove_file(link).unwrap();
    std::fs::rename(directory.path().join("Cell.lock"), directory.path().join("actual.lock")).unwrap();
    std::os::unix::fs::symlink(directory.path().join("actual.lock"), directory.path().join("Cell.lock")).unwrap();
    let error = compile_module(root, "dev", CompileOptions::default(), EntrySelection::Default).unwrap_err();
    assert!(error.message.contains("bounded Cell.lock"), "{error}");
    assert_eq!(std::fs::read(directory.path().join("actual.lock")).unwrap(), pinned);
}

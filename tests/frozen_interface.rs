#![cfg(all(feature = "cli", not(feature = "wasm")))]
mod common;
#[path = "support/instance_document.rs"]
mod instance_document;
#[path = "support/instance_pool.rs"]
mod instance_pool;
use camino::Utf8Path;
use cellscript::package::frozen_interface::{compile_module, freeze_module_catalog, EntrySelection, FrozenPackageModule};
use cellscript::CompileOptions;
use serde_json::Value;
use std::path::Path;

const SOURCE: &str = "module client\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { amount: u64 }\npublic action verify(witness value: Value) { verification require value.amount > 0 }\n";
fn package(root: &Path, name: &str, source: &str, extra: &str) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), format!("[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"{name}\"\nversion = \"1.0.0\"\n[environments.dev]\nchain_id = \"test-chain\"\ngenesis_hash = \"0x{}\"\n{extra}", "11".repeat(32))).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
}
fn lock(root: &Path) {
    let output = common::cellc_command().current_dir(root).args(["lock", "--json"]).output().unwrap();
    assert!(
        output.status.success(),
        "lock {}: stdout={} stderr={}",
        root.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn compile(root: &Path, diagnostics: cellscript::CompilerDiagnostics) -> FrozenPackageModule {
    compile_module(
        Utf8Path::from_path(root).unwrap(),
        "dev",
        CompileOptions { diagnostics, ..CompileOptions::default() },
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
            compile(&original, cellscript::CompilerDiagnostics::without_optimizations()),
            vec![compile(&candidate, cellscript::CompilerDiagnostics::without_optimizations())],
            &cellscript_artifact_checker::CheckerBudgets::default(),
        )
        .unwrap_err();
        assert!(error.message.contains("conflicting pinned chain identities"));
    }
    assert!(freeze_module_catalog(
        compile(&original, cellscript::CompilerDiagnostics::without_optimizations()),
        Vec::new(),
        &cellscript_artifact_checker::CheckerBudgets::default()
    )
    .is_err());
    assert!(freeze_module_catalog(
        compile(&original, cellscript::CompilerDiagnostics::without_optimizations()),
        vec![compile(&original, cellscript::CompilerDiagnostics::without_optimizations())],
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
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
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
    let first = compile(&original, cellscript::CompilerDiagnostics::default());
    let second = compile(&moved, cellscript::CompilerDiagnostics::default());
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
    let snapshot = compile(&client, cellscript::CompilerDiagnostics::default());
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
// Native all-member source/API/byte-origin closure, not deployment admission.
const CODE_SOURCE: &str = "module client\nresource Token has store, consume { amount: u64 }\npublic action burn(input token: Token, witness value: u64) { verification require token.amount > 0 require value > 0 consume token }\n";
fn code_package(root: &Path, source: &str) {
    use cellscript::artifact::{ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    package(root, "client", source, "");
    let manager = cellscript::package::PackageManager::new(root);
    let mut manifest = manager.read_manifest().unwrap();
    manifest.artifacts.push(ArtifactDeclaration {
        name: "code-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 40, action: "burn".into() }],
        common_checks: vec![],
    });
    manager.write_manifest(&manifest).unwrap();
    lock(root);
}
fn compile_code(root: &Path, opt: cellscript::CompilerDiagnostics) -> FrozenPackageModule {
    compile_module(
        Utf8Path::from_path(root).unwrap(),
        "dev",
        CompileOptions { diagnostics: opt, ..Default::default() },
        EntrySelection::Artifact("code-policy".into()),
    )
    .unwrap()
}
fn code_candidate(module: FrozenPackageModule, args: Vec<u8>) -> cellscript::package::frozen_interface::CodeCandidateInput {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let artifact = module.bundle()[0];
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let selected = lock
        .clone()
        .as_builder()
        .code_hash(packed::CellOutput::calc_data_hash(artifact))
        .hash_type(4u8)
        .args(Bytes::from(args).pack())
        .build();
    let tx = TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
                .build(),
        )
        .output(packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock).build())
        .output_data(Bytes::from(artifact.to_vec()).pack())
        .build();
    cellscript::package::frozen_interface::CodeCandidateInput {
        module,
        raw_transaction: tx.data().raw().as_slice().to_vec(),
        output_index: 0,
        selected_script: selected.as_slice().to_vec(),
    }
}

#[test]
fn native_code_catalog_binds_every_source_codec_and_actual_deployment_byte_tuple() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::CheckerBudgets;
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let changed = directory.path().join("changed");
    code_package(&original, CODE_SOURCE);
    // Equal public contracts do not imply equal predicates or source bytes.
    code_package(&changed, &CODE_SOURCE.replace("value > 0", "value == 7"));
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        std::fs::write(original.join("src/main.cell"), CODE_SOURCE).unwrap();
        let required = compile_code(&original, opt);
        let required_id = required.context_identity().to_owned();
        let inputs = vec![code_candidate(compile_code(&original, opt), vec![1]), code_candidate(compile_code(&changed, opt), vec![2])];
        let source_ids = inputs.iter().map(|input| input.module.context_identity().to_owned()).collect::<Vec<_>>();
        let raw = inputs[1].raw_transaction.clone();
        let script = inputs[1].selected_script.clone();
        // Ownership survives later filesystem edits; it does not bless them.
        std::fs::write(original.join("src/main.cell"), "module tampered\n").unwrap();
        let checked = freeze_code_catalog(required, inputs, &CheckerBudgets::default()).unwrap();
        assert_eq!(checked.required().context_identity(), required_id);
        assert_eq!(checked.candidates()[1].raw_transaction(), raw);
        assert_eq!(checked.candidates()[1].selected_script(), script);
        let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["schema"], "cellscript-frozen-code-catalog-v4");
        assert_eq!(record["candidate_source_contexts"], serde_json::to_value(source_ids).unwrap());
        assert_eq!(record["required_external_codec"], checked.required_codec().identity());
        assert_eq!(record["checked_modules"], checked.module_evidence().identity());
        for (index, candidate) in checked.candidates().iter().enumerate() {
            assert_eq!(record["candidate_code_origins"][index], candidate.origin().identity());
            assert_eq!(record["candidate_target_origins"][index], candidate.target_origin().identity());
            assert_eq!(record["candidate_receipts"][index], candidate.receipt().identity());
            candidate
                .receipt()
                .check_unchanged_inputs(candidate.module().bundle(), candidate.raw_transaction(), 0, candidate.selected_script())
                .unwrap();
            assert_eq!(candidate.origin().selected_hash_type(), 4);
            let origin = cellscript_artifact_checker::code_origin::check_code_cell_origin(
                candidate.module().bundle(),
                candidate.raw_transaction(),
                0,
                candidate.selected_script(),
                &CheckerBudgets::default(),
            )
            .unwrap();
            assert_eq!(origin.identity(), candidate.origin().identity());
            assert!(!candidate.module().projection().artifact_report().semantic_equivalence_claimed);
        }
        let bundles = checked.candidates().iter().map(|input| input.module().bundle()).collect::<Vec<_>>();
        checked.module_evidence().check_unchanged_inputs(checked.required().bundle(), &bundles).unwrap();
    }
}

#[test]
fn native_code_catalog_rejects_unselected_bad_origins_interfaces_and_networks() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::CheckerBudgets;
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let narrow = directory.path().join("narrow");
    let other = directory.path().join("other");
    let other_genesis = directory.path().join("other-genesis");
    code_package(&original, CODE_SOURCE);
    code_package(&narrow, &CODE_SOURCE.replace("amount: u64", "amount: u32"));
    code_package(&other, CODE_SOURCE);
    let manifest = other.join("Cell.toml");
    std::fs::write(&manifest, std::fs::read_to_string(&manifest).unwrap().replace("test-chain", "other-chain")).unwrap();
    lock(&other);
    code_package(&other_genesis, CODE_SOURCE);
    let manifest = other_genesis.join("Cell.toml");
    std::fs::write(&manifest, std::fs::read_to_string(&manifest).unwrap().replace(&"11".repeat(32), &"22".repeat(32))).unwrap();
    lock(&other_genesis);
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for case in ["raw", "script", "output", "data", "interface", "network", "genesis"] {
            let root = match case {
                "interface" => &narrow,
                "network" => &other,
                "genesis" => &other_genesis,
                _ => &original,
            };
            let mut bad = code_candidate(compile_code(root, opt), vec![2]);
            match case {
                "raw" => bad.raw_transaction.push(0),
                "script" => bad.selected_script[16] ^= 1,
                "output" => bad.output_index = 1,
                "data" => *bad.raw_transaction.last_mut().unwrap() ^= 1,
                _ => (),
            }
            let error = freeze_code_catalog(
                compile_code(&original, opt),
                vec![code_candidate(compile_code(&original, opt), vec![1]), bad],
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(!error.message.is_empty(), "case {case}, {opt}");
            if matches!(case, "network" | "genesis") {
                assert!(error.message.contains("conflicting pinned chain identities"));
            }
        }
    }
}

#[test]
fn native_code_catalog_requires_finite_receipts_for_unselected_and_final_members() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::{code_origin::check_code_cell_origin, CheckerBudgets};
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    let constant = directory.path().join("constant");
    code_package(&original, CODE_SOURCE);
    code_package(&constant, &format!("{CODE_SOURCE}\npublic const LABEL: String = \"sealed\""));
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let invalid = code_candidate(compile_code(&constant, opt), vec![2]);
        // A constant outside the proven-value grammar cannot construct its
        // finite receipt at all: the origin check itself fails closed.
        let origin_error = check_code_cell_origin(
            invalid.module.bundle(),
            &invalid.raw_transaction,
            0,
            &invalid.selected_script,
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert!(origin_error.message.contains("constant"), "{origin_error:?}");
        let error = freeze_code_catalog(
            compile_code(&original, opt),
            vec![code_candidate(compile_code(&original, opt), vec![1]), invalid],
            &CheckerBudgets::default(),
        )
        .unwrap_err();
        assert!(error.message.contains("constant"), "{error:?}");
    }
    let mut inputs = (0..31)
        .map(|index| code_candidate(compile_code(&original, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]))
        .collect::<Vec<_>>();
    inputs.push(code_candidate(compile_code(&constant, cellscript::CompilerDiagnostics::without_optimizations()), vec![31]));
    assert!(freeze_code_catalog(
        compile_code(&original, cellscript::CompilerDiagnostics::without_optimizations()),
        inputs,
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("constant"));
}

#[test]
fn native_code_catalog_rejects_wrong_vm_hash_types_in_unselected_members() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::{code_origin::check_code_cell_origin, CheckerBudgets};
    use ckb_testtool::ckb_types::{packed, prelude::*};
    let directory = tempfile::tempdir().unwrap();
    code_package(directory.path(), CODE_SOURCE);
    let root = directory.path();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for hash_type in [0u8, 2] {
            let mut bad = code_candidate(compile_code(root, opt), vec![2]);
            bad.selected_script = packed::Script::from_slice(&bad.selected_script)
                .unwrap()
                .as_builder()
                .hash_type(hash_type)
                .build()
                .as_slice()
                .to_vec();
            // Historical byte-origin evidence is valid; target selection is not.
            assert!(check_code_cell_origin(
                bad.module.bundle(),
                &bad.raw_transaction,
                bad.output_index,
                &bad.selected_script,
                &CheckerBudgets::default()
            )
            .is_ok());
            let error = freeze_code_catalog(
                compile_code(root, opt),
                vec![code_candidate(compile_code(root, opt), vec![1]), bad],
                &CheckerBudgets::default(),
            )
            .unwrap_err();
            assert!(error.message.contains("selected Script hash type differs"), "{opt}, hash_type={hash_type}: {error:?}");
        }
    }
    let mut inputs = (0..32)
        .map(|index| code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]))
        .collect::<Vec<_>>();
    inputs[31].selected_script =
        packed::Script::from_slice(&inputs[31].selected_script).unwrap().as_builder().hash_type(2u8).build().as_slice().to_vec();
    assert!(freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        inputs,
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("selected Script hash type differs"));
}

#[test]
fn native_code_catalog_bounds_all_inputs_before_parsing_and_returns_no_partial_catalog() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::CheckerBudgets;
    let directory = tempfile::tempdir().unwrap();
    code_package(directory.path(), CODE_SOURCE);
    let root = directory.path();
    assert!(freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        vec![],
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("1..=32"));
    let too_many = (0..33)
        .map(|index| code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]))
        .collect();
    assert!(freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        too_many,
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("1..=32"));
    for which in ["artifact", "record", "source-map", "large-raw", "large-script", "shared"] {
        let mut budgets = CheckerBudgets::default();
        let mut inputs = vec![code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![1])];
        // Malformed first member must not obscure a later shared-budget error.
        inputs[0].raw_transaction = vec![0];
        match which {
            "artifact" => budgets.artifact_bytes = 0,
            "record" => budgets.record_bytes = 0,
            "source-map" => budgets.source_map_bytes = 0,
            "large-raw" => inputs[0].raw_transaction = vec![0; 4 * 1024 * 1024 + 1],
            "large-script" => inputs[0].selected_script = vec![0; 4 * 1024 * 1024 + 1],
            "shared" => {
                for index in 2..=6 {
                    let mut input =
                        code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]);
                    input.raw_transaction = vec![0; 4 * 1024 * 1024];
                    inputs.push(input);
                }
            }
            _ => unreachable!(),
        }
        let error =
            freeze_code_catalog(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), inputs, &budgets)
                .unwrap_err();
        assert!(error.message.contains("shared 16 MiB or per-file/caller byte budgets"), "{which}: {error:?}");
    }
}

#[test]
fn native_code_catalog_checks_all_32_members_and_distinguishes_complete_script_args() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::CheckerBudgets;
    let directory = tempfile::tempdir().unwrap();
    code_package(directory.path(), CODE_SOURCE);
    let root = directory.path();
    let inputs = (0..32)
        .map(|index| code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]))
        .collect();
    let checked = freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        inputs,
        &CheckerBudgets::default(),
    )
    .unwrap();
    assert_eq!(checked.candidates().len(), 32);
    let first = checked.candidates()[0].origin();
    let second = checked.candidates()[1].origin();
    assert_eq!(first.transaction_hash(), second.transaction_hash());
    assert_eq!(first.output_index(), second.output_index());
    assert_ne!(first.selected_script_hash(), second.selected_script_hash());
    assert_ne!(first.identity(), second.identity());
    let mut inputs = (0..32)
        .map(|index| code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![index]))
        .collect::<Vec<_>>();
    inputs[31].output_index = 1;
    assert!(freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        inputs,
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("code output index is absent"));
    let duplicate = vec![
        code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![1]),
        code_candidate(compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()), vec![1]),
    ];
    assert!(freeze_code_catalog(
        compile_code(root, cellscript::CompilerDiagnostics::without_optimizations()),
        duplicate,
        &CheckerBudgets::default()
    )
    .unwrap_err()
    .message
    .contains("duplicate concrete Script/code deployment"));
}

fn owned_code_package(root: &Path, name: &str) {
    code_package(root, &CODE_SOURCE.replace("module client", "module foreign"));
    let manager = cellscript::package::PackageManager::new(root);
    let mut manifest = manager.read_manifest().unwrap();
    manifest.package.name = name.into();
    manager.write_manifest(&manifest).unwrap();
    lock(root);
}
fn consuming_package(root: &Path, library_name: &str, library_path: &str, aliases: &[&str]) {
    let dependencies = aliases
        .iter()
        .map(|alias| format!("{alias} = {{ package = \"{library_name}\", path = \"{library_path}\" }}\n"))
        .collect::<String>();
    package(root,"client","module client\nuse foreign::Token as Value\npublic action verify(input token: Value) { verification require token.amount > 0 consume token }\n",&format!("[dependencies]\n{dependencies}"));
    lock(root);
}
fn one_code_catalog(root: &Path, opt: cellscript::CompilerDiagnostics) -> cellscript::package::frozen_interface::FrozenCodeCatalog {
    cellscript::package::frozen_interface::freeze_code_catalog(
        compile_code(root, opt),
        vec![code_candidate(compile_code(root, opt), vec![1])],
        &cellscript_artifact_checker::CheckerBudgets::default(),
    )
    .unwrap()
}

#[test]
fn resolved_code_catalog_uses_actual_locked_defining_owner_across_dependency_aliases() {
    use cellscript::package::frozen_interface::resolve_code_catalog_source;
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    owned_code_package(&library, "library");
    consuming_package(&first, "library", "../library", &["left", "right"]);
    consuming_package(&second, "library", "../library", &["renamed"]);
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let first = resolve_code_catalog_source(compile(&first, opt), one_code_catalog(&library, opt)).unwrap();
        let second = resolve_code_catalog_source(compile(&second, opt), one_code_catalog(&library, opt)).unwrap();
        assert_eq!(first.source_owner_identity(), second.source_owner_identity());
        assert_ne!(first.consumer().context_identity(), second.consumer().context_identity());
        assert_ne!(first.identity(), second.identity());
        let consumer = context(first.consumer());
        let baseline = context(first.catalog().required());
        let owner: Value = serde_json::from_slice(&first.source_owner_bytes().unwrap()).unwrap();
        let actual = consumer["modules"]["foreign"]["package"].as_str().unwrap();
        assert_eq!(owner["defining_package"], actual);
        assert_eq!(consumer["packages"][actual]["source"]["kind"], "local-snapshot");
        let root = baseline["modules"]["foreign"]["package"].as_str().unwrap();
        assert_eq!(baseline["packages"][root]["source"]["kind"], "root-snapshot");
        assert_ne!(actual, root);
        assert_eq!(owner["required_module_contract"], first.catalog().required().projection().identity());
        let record: Value = serde_json::from_slice(&first.canonical_bytes().unwrap()).unwrap();
        assert_eq!(record["defining_owner"], first.source_owner_identity());
        assert_eq!(record["code_catalog"], first.catalog().identity());
        assert!(!first.consumer().projection().artifact_report().semantic_equivalence_claimed);
    }
}

#[test]
fn resolved_code_catalog_keeps_same_name_and_shape_from_different_owners_distinct() {
    use cellscript::package::frozen_interface::resolve_code_catalog_source;
    let directory = tempfile::tempdir().unwrap();
    let left = directory.path().join("left");
    let right = directory.path().join("right");
    let app_left = directory.path().join("app-left");
    let app_right = directory.path().join("app-right");
    owned_code_package(&left, "library-left");
    owned_code_package(&right, "library-right");
    consuming_package(&app_left, "library-left", "../left", &["library"]);
    consuming_package(&app_right, "library-right", "../right", &["library"]);
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let left_bound = resolve_code_catalog_source(compile(&app_left, opt), one_code_catalog(&left, opt)).unwrap();
        let right_bound = resolve_code_catalog_source(compile(&app_right, opt), one_code_catalog(&right, opt)).unwrap();
        assert_eq!(left_bound.catalog().required().projection().identity(), right_bound.catalog().required().projection().identity());
        assert_ne!(left_bound.source_owner_identity(), right_bound.source_owner_identity());
        assert!(resolve_code_catalog_source(compile(&app_left, opt), one_code_catalog(&right, opt))
            .unwrap_err()
            .message
            .contains("defining package differs"));
        assert!(resolve_code_catalog_source(compile(&app_right, opt), one_code_catalog(&left, opt))
            .unwrap_err()
            .message
            .contains("defining package differs"));
    }
}

#[test]
fn resolved_code_catalog_rejects_changed_definitions_versions_networks_and_absent_imports() {
    use cellscript::package::frozen_interface::resolve_code_catalog_source;
    for case in ["source", "version", "network", "genesis", "absent"] {
        let directory = tempfile::tempdir().unwrap();
        let library = directory.path().join("library");
        let app = directory.path().join("app");
        owned_code_package(&library, "library");
        let baseline = one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations());
        match case {
            "source" => std::fs::write(
                library.join("src/main.cell"),
                CODE_SOURCE.replace("module client", "module foreign").replace("amount: u64", "amount: u32"),
            )
            .unwrap(),
            "version" => {
                let manager = cellscript::package::PackageManager::new(&library);
                let mut manifest = manager.read_manifest().unwrap();
                manifest.package.version = "2.0.0".into();
                manager.write_manifest(&manifest).unwrap();
            }
            _ => (),
        }
        if case == "absent" {
            package(&app, "client", SOURCE, "");
            lock(&app);
        } else {
            consuming_package(&app, "library", "../library", &["library"]);
        }
        if matches!(case, "network" | "genesis") {
            let path = app.join("Cell.toml");
            let text = std::fs::read_to_string(&path).unwrap();
            let text = if case == "network" {
                text.replace("test-chain", "other-chain")
            } else {
                text.replace(&"11".repeat(32), &"22".repeat(32))
            };
            std::fs::write(path, text).unwrap();
            lock(&app);
        }
        let consumer = compile(&app, cellscript::CompilerDiagnostics::without_optimizations());
        let error = resolve_code_catalog_source(consumer, baseline).unwrap_err();
        let expected = match case {
            "source" | "version" => "defining package differs",
            "network" | "genesis" => "conflicting pinned chain identities",
            _ => "did not resolve the defining baseline module",
        };
        assert!(error.message.contains(expected), "{case}: {error:?}");
    }
}

fn source_owner_git(root: &Path, arguments: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=CellScript Tests", "-c", "user.email=tests@cellscript.dev", "-c", "commit.gpgsign=false"])
        .args(arguments)
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success(), "{arguments:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn resolved_code_catalog_preserves_transitive_git_pins_even_for_identical_source_bytes() {
    use cellscript::package::frozen_interface::resolve_code_catalog_source;
    let directory = tempfile::tempdir().unwrap();
    let auxiliary = directory.path().join("auxiliary");
    let library = directory.path().join("library");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    package(&auxiliary, "auxiliary", "module auxiliary\npublic struct Payload has copy, drop, store { amount: u64 }\n", "");
    source_owner_git(&auxiliary, &["init", "-b", "main"]);
    source_owner_git(&auxiliary, &["add", "."]);
    source_owner_git(&auxiliary, &["commit", "-m", "initial fixture"]);
    let first_revision = source_owner_git(&auxiliary, &["rev-parse", "HEAD"]);
    owned_code_package(&library, "library");
    std::fs::write(
        library.join("src/main.cell"),
        CODE_SOURCE.replace("module client", "module foreign\nuse auxiliary::Payload as Marker"),
    )
    .unwrap();
    let manager = cellscript::package::PackageManager::new(&library);
    let mut manifest = manager.read_manifest().unwrap();
    manifest.dependencies.insert(
        "auxiliary".into(),
        serde_json::from_value(serde_json::json!({"git": auxiliary.to_str().unwrap(), "branch": "main"})).unwrap(),
    );
    manager.write_manifest(&manifest).unwrap();
    lock(&library);
    consuming_package(&first, "library", "../library", &["library"]);
    let baseline = one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations());
    let checked = resolve_code_catalog_source(
        compile(&first, cellscript::CompilerDiagnostics::without_optimizations()),
        one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations()),
    )
    .unwrap();
    let first_context = context(checked.consumer());
    let first_package = first_context["modules"]["auxiliary"]["package"].as_str().unwrap();
    assert_eq!(first_context["packages"][first_package]["source"]["revision"], first_revision);
    let owner: Value = serde_json::from_slice(&checked.source_owner_bytes().unwrap()).unwrap();
    assert!(owner["source_closure_packages"].as_array().unwrap().iter().any(|identity| identity == first_package));
    // Only Git history changes. Source and manifest bytes remain byte-identical.
    source_owner_git(&auxiliary, &["commit", "--allow-empty", "-m", "second fixture pin"]);
    let second_revision = source_owner_git(&auxiliary, &["rev-parse", "HEAD"]);
    assert_ne!(first_revision, second_revision);
    consuming_package(&second, "library", "../library", &["library"]);
    let consumer = compile(&second, cellscript::CompilerDiagnostics::without_optimizations());
    let second_context = context(&consumer);
    let second_package = second_context["modules"]["auxiliary"]["package"].as_str().unwrap();
    assert_eq!(second_context["packages"][second_package]["source"]["revision"], second_revision);
    assert_eq!(first_context["packages"][first_package]["source_hash"], second_context["packages"][second_package]["source_hash"]);
    assert_eq!(
        first_context["packages"][first_package]["manifest_digest"],
        second_context["packages"][second_package]["manifest_digest"]
    );
    assert_ne!(first_package, second_package);
    let error = resolve_code_catalog_source(consumer, baseline).unwrap_err();
    assert!(error.message.contains("another transitive source origin or snapshot"), "{error:?}");
}

fn padded_source_owner_candidate(
    module: FrozenPackageModule,
    args: Vec<u8>,
    padding: usize,
) -> cellscript::package::frozen_interface::CodeCandidateInput {
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let mut input = code_candidate(module, args);
    let lock = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
    let cell = packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock).build();
    let tx = TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
                .build(),
        )
        .output(cell.clone())
        .output_data(Bytes::from(input.module.bundle()[0].to_vec()).pack())
        .output(cell)
        .output_data(Bytes::from(vec![0; padding]).pack())
        .build();
    input.raw_transaction = tx.data().raw().as_slice().to_vec();
    input
}

#[test]
fn resolved_code_catalog_counts_the_consumer_before_any_ownership_traversal() {
    use cellscript::package::frozen_interface::{freeze_code_catalog, resolve_code_catalog_source};
    use cellscript_artifact_checker::CheckerBudgets;
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let absent = directory.path().join("absent");
    owned_code_package(&library, "library");
    // Deliberately lacks the foreign defining module; budget error must win.
    package(&absent, "client", SOURCE, "");
    lock(&absent);
    let consumer = compile(&absent, cellscript::CompilerDiagnostics::without_optimizations());
    let consumer_bytes = consumer.bundle().iter().map(|bytes| bytes.len()).sum::<usize>();
    let required = compile_code(&library, cellscript::CompilerDiagnostics::without_optimizations());
    let mut inputs = (0..4)
        .map(|index| {
            padded_source_owner_candidate(
                compile_code(&library, cellscript::CompilerDiagnostics::without_optimizations()),
                vec![index],
                0,
            )
        })
        .collect::<Vec<_>>();
    let base = required.bundle().iter().map(|bytes| bytes.len()).sum::<usize>()
        + inputs
            .iter()
            .map(|input| {
                input.module.bundle().iter().map(|bytes| bytes.len()).sum::<usize>()
                    + input.raw_transaction.len()
                    + input.selected_script.len()
            })
            .sum::<usize>();
    let target = 16 * 1024 * 1024 - consumer_bytes / 2;
    let padding = target.checked_sub(base).unwrap();
    for (index, input) in inputs.iter_mut().enumerate() {
        let size = padding / 4 + usize::from(index < padding % 4);
        let replacement = padded_source_owner_candidate(
            compile_code(&library, cellscript::CompilerDiagnostics::without_optimizations()),
            vec![index as u8],
            size,
        );
        assert!(replacement.raw_transaction.len() <= 4 * 1024 * 1024);
        *input = replacement;
    }
    let catalog = freeze_code_catalog(required, inputs, &CheckerBudgets::default()).unwrap();
    let catalog_bytes = catalog.required().bundle().iter().map(|bytes| bytes.len()).sum::<usize>()
        + catalog
            .candidates()
            .iter()
            .map(|candidate| {
                candidate.module().bundle().iter().map(|bytes| bytes.len()).sum::<usize>()
                    + candidate.raw_transaction().len()
                    + candidate.selected_script().len()
            })
            .sum::<usize>();
    assert_eq!(catalog_bytes, target);
    assert!(catalog_bytes + consumer_bytes > 16 * 1024 * 1024);
    let error = resolve_code_catalog_source(consumer, catalog).unwrap_err();
    assert!(error.message.contains("shared 16 MiB"), "{error:?}");
}

#[test]
fn parsed_source_closure_matches_native_compilation_and_detects_repinning() {
    use cellscript::package::frozen_interface::freeze_package_sources;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    package(root, "client", SOURCE, "");
    lock(root);
    let pinned = std::fs::read(root.join("Cell.lock")).unwrap();
    let parsed = freeze_package_sources(Utf8Path::from_path(root).unwrap(), "dev").unwrap();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = compile(root, opt);
        assert_eq!(parsed.context_identity(), compiled.context_identity());
        assert_eq!(parsed.context_bytes().unwrap(), compiled.context_bytes().unwrap());
        parsed.check_unchanged().unwrap();
        assert_eq!(std::fs::read(root.join("Cell.lock")).unwrap(), pinned);
    }
    std::fs::write(root.join("src/main.cell"), SOURCE.replace("value.amount > 0", "value.amount > 7")).unwrap();
    assert!(parsed.check_unchanged().is_err());
    lock(root);
    let error = parsed.check_unchanged().unwrap_err();
    assert!(error.message.contains("frozen source closure changed"));
    let fresh = freeze_package_sources(Utf8Path::from_path(root).unwrap(), "dev").unwrap();
    assert_ne!(parsed.context_identity(), fresh.context_identity());
}

#[test]
fn source_catalog_binds_defining_owner_before_consumer_typechecking() {
    use cellscript::package::frozen_interface::{freeze_package_sources, resolve_code_catalog_source, resolve_source_catalog};
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let consumer = directory.path().join("consumer");
    owned_code_package(&library, "library");
    consuming_package(&consumer, "library", "../library", &["left", "right"]);
    let compiled = resolve_code_catalog_source(
        compile(&consumer, cellscript::CompilerDiagnostics::without_optimizations()),
        one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations()),
    )
    .unwrap();
    let root = Utf8Path::from_path(&consumer).unwrap();
    let parsed = resolve_source_catalog(
        freeze_package_sources(root, "dev").unwrap(),
        one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations()),
    )
    .unwrap();
    assert_eq!(parsed.source_owner_identity(), compiled.source_owner_identity());
    assert_eq!(parsed.source_owner_bytes().unwrap(), compiled.source_owner_bytes().unwrap());
    assert_eq!(parsed.sources().context_identity(), compiled.consumer().context_identity());
    assert_ne!(parsed.identity(), compiled.identity());
    let record: Value = serde_json::from_slice(&parsed.canonical_bytes().unwrap()).unwrap();
    assert_eq!(record["schema"], "cellscript-resolver-source-catalog-v1");
    assert_eq!(record["defining_owner"], parsed.source_owner_identity());
    assert_eq!(record["code_catalog"], parsed.catalog().identity());
    parsed.check_unchanged_sources().unwrap();
    // Parsed source ownership must be available before unknown consumer types
    // are resolved, and cannot itself cause those semantic types to be admitted.
    let path = consumer.join("src/main.cell");
    let source =
        std::fs::read_to_string(&path).unwrap().replace("input token: Value", "input token: Value, witness deferred: FutureHandle");
    std::fs::write(&path, source).unwrap();
    lock(&consumer);
    assert!(parsed.check_unchanged_sources().is_err());
    let untyped = resolve_source_catalog(
        freeze_package_sources(root, "dev").unwrap(),
        one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations()),
    )
    .unwrap();
    assert_eq!(untyped.source_owner_identity(), compiled.source_owner_identity());
    let error = compile_module(root, "dev", CompileOptions::default(), EntrySelection::Action("verify".into())).unwrap_err();
    assert!(
        error.message.contains("FutureHandle") || error.related.iter().any(|diagnostic| diagnostic.message.contains("FutureHandle")),
        "{error:?}"
    );
    untyped.check_unchanged_sources().unwrap();
}

#[test]
fn source_catalog_rechecks_actual_files_before_ownership_binding() {
    use cellscript::package::frozen_interface::{freeze_package_sources, resolve_source_catalog};
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let consumer = directory.path().join("consumer");
    owned_code_package(&library, "library");
    consuming_package(&consumer, "library", "../library", &["library"]);
    let root = Utf8Path::from_path(&consumer).unwrap();
    let sources = freeze_package_sources(root, "dev").unwrap();
    let catalog = one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations());
    std::fs::write(consumer.join("src/main.cell"), "module replaced\npublic action verify() { require true }\n").unwrap();
    lock(&consumer);
    let error = resolve_source_catalog(sources, catalog).unwrap_err();
    assert!(error.message.contains("frozen source closure changed"));
    let fresh = freeze_package_sources(root, "dev").unwrap();
    assert!(
        resolve_source_catalog(fresh, one_code_catalog(&library, cellscript::CompilerDiagnostics::without_optimizations())).is_ok()
    );
    // Only source ownership is certified. A module need not execute a peer to
    // have that actual dependency in its locked closure.
}

#[test]
fn parsed_source_closure_rejects_unlocked_unpinned_large_and_malformed_inputs() {
    use cellscript::package::frozen_interface::freeze_package_sources;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    package(root, "client", SOURCE, "");
    let path = Utf8Path::from_path(root).unwrap();
    assert!(freeze_package_sources(path, "dev").is_err());
    assert!(!root.join("Cell.lock").exists());
    lock(root);
    assert!(freeze_package_sources(path, "absent").is_err());
    let pinned = std::fs::read(root.join("Cell.lock")).unwrap();
    std::fs::write(root.join("src/main.cell"), "module client\npublic action bad(\n").unwrap();
    // Explicit lock hashes the intentionally malformed source; source capture
    // must then reject parsing rather than accepting a producer context label.
    lock(root);
    assert!(freeze_package_sources(path, "dev").is_err());
    std::fs::write(root.join("Cell.lock"), pinned).unwrap();
    std::fs::write(root.join("src/main.cell"), vec![b' '; 4 * 1024 * 1024 + 1]).unwrap();
    let error = freeze_package_sources(path, "dev").unwrap_err();
    assert!(error.message.contains("4 MiB/file"), "{error:?}");
}

#[test]
fn parsed_source_closure_rejects_even_identical_planned_lock_overrides() {
    use cellscript::package::frozen_interface::freeze_package_sources;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    package(root, "client", SOURCE, "");
    lock(root);
    let path = Utf8Path::from_path(root).unwrap();
    let parsed = freeze_package_sources(path, "dev").unwrap();
    let pinned: cellscript::package::Lockfile = toml::from_str(&std::fs::read_to_string(root.join("Cell.lock")).unwrap()).unwrap();
    cellscript::package::with_lockfile_override(root, pinned, || {
        let error = freeze_package_sources(path, "dev").unwrap_err();
        assert!(error.message.contains("planned lockfile override"));
        let error = parsed.check_unchanged().unwrap_err();
        assert!(error.message.contains("planned lockfile override"));
        Ok(())
    })
    .unwrap();
    parsed.check_unchanged().unwrap();
}

fn policy_hash(text: &str) -> [u8; 32] {
    let text = text.strip_prefix("0x").unwrap_or(text);
    assert_eq!(text.len(), 64);
    std::array::from_fn(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
}
fn policy_sources(
    library: &Path,
    candidate: &Path,
    consumer: &Path,
    opt: cellscript::CompilerDiagnostics,
    count: usize,
) -> cellscript::package::frozen_interface::ResolvedSourceCatalog {
    use cellscript::package::frozen_interface::{freeze_code_catalog, freeze_package_sources, resolve_source_catalog};
    let inputs = (0..count)
        .map(|index| code_candidate(compile_code(if index % 2 == 0 { library } else { candidate }, opt), vec![index as u8 + 1]))
        .collect();
    let catalog = freeze_code_catalog(compile_code(library, opt), inputs, &Default::default()).unwrap();
    resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(consumer).unwrap(), "dev").unwrap(), catalog).unwrap()
}
fn policy_records(
    sources: &cellscript::package::frozen_interface::ResolvedSourceCatalog,
    exact: bool,
) -> (cellscript_artifact_checker::open_handle_policy::PolicyHeader, Vec<cellscript_artifact_checker::open_handle_policy::PolicyMember>)
{
    use cellscript_artifact_checker::open_handle_policy::*;
    let catalog = sources.catalog();
    let runtime = catalog.required().projection().runtime_contract();
    let members = catalog
        .candidates()
        .iter()
        .map(|candidate| PolicyMember {
            status: MemberStatus::Active,
            hash_type: CodeHashType::Data2,
            admission_sequence: 4,
            deployment_sequence: 0,
            receipt: policy_hash(candidate.receipt().identity()),
            interface: policy_hash(candidate.module().projection().identity()),
            artifact: policy_hash(candidate.origin().artifact_hash()),
            script: policy_hash(candidate.origin().selected_script_hash()),
            code_hash: policy_hash(candidate.origin().artifact_hash()),
            code_tx_hash: policy_hash(candidate.origin().transaction_hash()),
            deployment_line: [0; 32],
            history_tip: [0; 32],
            code_output_index: candidate.origin().output_index(),
        })
        .collect::<Vec<_>>();
    let header = PolicyHeader {
        class: HandleClass::Script,
        role: ScriptRole::Type,
        mode: if exact { SelectionMode::Exact } else { SelectionMode::Compatible },
        member_count: members.len() as u8,
        sequence: 5,
        minimum_admission_sequence: 3,
        required_interface: policy_hash(catalog.required().projection().identity()),
        exact_receipt: if exact { members[0].receipt } else { [0; 32] },
        network_genesis: [0x11; 32],
        target_profile: policy_hash(
            &cellscript_artifact_checker::canonical_hash("cellscript-code-policy-target-id-v1", &runtime.target_profile).unwrap(),
        ),
        runtime_abi: policy_hash(
            &cellscript_artifact_checker::canonical_hash("cellscript-code-policy-runtime-id-v1", runtime).unwrap(),
        ),
    };
    (header, members)
}
fn policy_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let candidate = directory.path().join("candidate");
    let consumer = directory.path().join("consumer");
    owned_code_package(&library, "library");
    owned_code_package(&candidate, "library");
    let source = CODE_SOURCE.replace("module client", "module foreign").replace("value > 0", "value == 7");
    std::fs::write(candidate.join("src/main.cell"), source).unwrap();
    lock(&candidate);
    consuming_package(&consumer, "library", "../library", &["left", "right"]);
    (directory, library, candidate, consumer)
}
#[test]
fn code_policy_binds_actual_all_member_receipts_and_selects_exact_or_compatible() {
    use cellscript::package::frozen_interface::freeze_code_policy;
    use cellscript_artifact_checker::open_handle_policy::AuthorizationSet;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for exact in [false, true] {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let source_id = sources.identity().to_owned();
            let (header, members) = policy_records(&sources, exact);
            assert_ne!(members[0].artifact, members[1].artifact);
            let policy = AuthorizationSet::new(header, &members).unwrap();
            let root = policy.root();
            let checked = freeze_code_policy(sources, policy).unwrap();
            let record: Value = serde_json::from_slice(&checked.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["schema"], "cellscript-frozen-code-policy-bindings-v1");
            for field in ["source_version_floor", "source_receipts", "version_rule"] {
                assert!(record.get(field).is_none(), "artifact-only v1 unexpectedly contains {field}");
            }
            assert_eq!(record["source_catalog"], source_id);
            assert_eq!(record["policy_root"], cellscript_artifact_checker::hex_encode(&root));
            let mut material = b"cellscript-frozen-code-policy-bindings-id-v1\0".to_vec();
            material.extend_from_slice(&checked.canonical_bytes().unwrap());
            assert_eq!(
                checked.identity(),
                cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&material))
            );
            for (index, member) in members.iter().enumerate() {
                let witness = checked.policy().selection(&member.receipt).unwrap();
                let selection = checked.check_selection(&witness);
                if exact && index == 1 {
                    assert!(selection.unwrap_err().message.contains("ExactReceipt"));
                } else {
                    let selection = selection.unwrap();
                    assert_eq!(selection.membership().root(), root);
                    assert_eq!(policy_hash(selection.candidate().receipt().identity()), member.receipt);
                    let candidate = selection.candidate();
                    selection
                        .check_unchanged_inputs(
                            candidate.module().bundle(),
                            candidate.raw_transaction(),
                            candidate.origin().output_index(),
                            candidate.selected_script(),
                        )
                        .unwrap();
                    assert_eq!(record["candidate_indices"][selection.membership().index() as usize], index);
                }
            }
        }
    }
}
#[test]
fn code_policy_rejects_rebound_headers_and_every_unselected_member_identity() {
    use cellscript::package::frozen_interface::freeze_code_policy;
    use cellscript_artifact_checker::open_handle_policy::*;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for axis in 0..12 {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let (mut header, mut members) = policy_records(&sources, true);
            // Reconstruct a valid low-level tree: rejection must come from
            // actual host bindings rather than a stale outer Merkle root.
            match axis {
                0 => header.required_interface[0] ^= 1,
                1 => header.network_genesis[0] ^= 1,
                2 => header.target_profile[0] ^= 1,
                3 => header.runtime_abi[0] ^= 1,
                4 => header.role = ScriptRole::Lock,
                5 => members[1].receipt[0] ^= 1,
                6 => members[1].interface[0] ^= 1,
                7 => {
                    members[1].artifact[0] ^= 1;
                    members[1].code_hash = members[1].artifact;
                }
                8 => members[1].script[0] ^= 1,
                9 => members[1].code_tx_hash[0] ^= 1,
                10 => members[1].code_output_index += 1,
                11 => members[1].hash_type = CodeHashType::Data1,
                _ => unreachable!(),
            }
            let wire_only = AuthorizationSet::new(header, &members).unwrap();
            let error = freeze_code_policy(sources, wire_only).unwrap_err();
            assert!(error.message.contains("code policy"), "axis {axis} {opt}: {error}");
        }
    }
}
#[test]
fn code_policy_checks_final_member_and_rejects_missing_or_substituted_members() {
    use cellscript::package::frozen_interface::freeze_code_policy;
    use cellscript_artifact_checker::open_handle_policy::*;
    let (_directory, library, candidate, consumer) = policy_fixture();
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 32);
    let (header, members) = policy_records(&sources, false);
    let checked = freeze_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
    for member in &members {
        let witness = checked.policy().selection(&member.receipt).unwrap();
        let selected = checked.check_selection(&witness).unwrap();
        assert_eq!(policy_hash(selected.candidate().receipt().identity()), member.receipt);
    }
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 32);
    let (header, mut members) = policy_records(&sources, false);
    members[31].script[0] ^= 1;
    assert!(freeze_code_policy(sources, AuthorizationSet::new(header, &members).unwrap())
        .unwrap_err()
        .message
        .contains("complete Script"));
    for count in [1, 2] {
        let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
        let (mut header, mut members) = policy_records(&sources, false);
        if count == 1 {
            members.pop();
            header.member_count = 1;
        } else {
            members[1].receipt = [0x87; 32];
        }
        assert!(freeze_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).is_err());
    }
}
#[test]
fn code_policy_keeps_snapshot_status_floors_and_root_separate_from_host_byte_bindings() {
    use cellscript::package::frozen_interface::freeze_code_policy;
    use cellscript_artifact_checker::open_handle_policy::*;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for yanked in [false, true] {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let (header, mut members) = policy_records(&sources, false);
            if yanked {
                members[1].status = MemberStatus::Yanked;
            } else {
                members[1].admission_sequence = 2;
            }
            let checked = freeze_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
            let witness = checked.policy().selection(&members[1].receipt).unwrap();
            let error = checked.check_selection(&witness).unwrap_err();
            assert!(error.message.contains(if yanked { "Inactive" } else { "Sequence" }));
            let mut good = checked.policy().selection(&members[0].receipt).unwrap();
            good[8 + 28] ^= 1; // Change the header without authorizing a new root.
            assert!(checked.check_selection(&good).is_err());
            assert!(checked.check_selection(&good[..good.len() - 1]).is_err());
        }
    }
}
#[test]
fn code_policy_detects_later_receipt_input_and_real_source_substitution() {
    use cellscript::package::frozen_interface::freeze_code_policy;
    use cellscript_artifact_checker::open_handle_policy::*;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let original = CODE_SOURCE.replace("module client", "module foreign");
        std::fs::write(library.join("src/main.cell"), &original).unwrap();
        let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
        let (header, members) = policy_records(&sources, false);
        let checked = freeze_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let witness = checked.policy().selection(&members[0].receipt).unwrap();
        let selection = checked.check_selection(&witness).unwrap();
        let actual = selection.candidate();
        let mut metadata = actual.module().bundle()[1].to_vec();
        metadata.push(b'\n');
        let mut bundle = actual.module().bundle();
        bundle[1] = &metadata;
        assert!(selection
            .check_unchanged_inputs(bundle, actual.raw_transaction(), actual.origin().output_index(), actual.selected_script())
            .is_err());
        let mut raw = actual.raw_transaction().to_vec();
        *raw.last_mut().unwrap() ^= 1;
        assert!(selection
            .check_unchanged_inputs(actual.module().bundle(), &raw, actual.origin().output_index(), actual.selected_script())
            .is_err());
        let mut script = actual.selected_script().to_vec();
        *script.last_mut().unwrap() ^= 1;
        assert!(selection
            .check_unchanged_inputs(actual.module().bundle(), actual.raw_transaction(), actual.origin().output_index(), &script)
            .is_err());
        assert!(selection
            .check_unchanged_inputs(
                actual.module().bundle(),
                actual.raw_transaction(),
                actual.origin().output_index() + 1,
                actual.selected_script()
            )
            .is_err());
        std::fs::write(library.join("src/main.cell"), format!("{original}// changed after checking\n")).unwrap();
        assert!(checked.check_selection(&witness).is_err());
        assert!(selection
            .check_unchanged_inputs(
                actual.module().bundle(),
                actual.raw_transaction(),
                actual.origin().output_index(),
                actual.selected_script()
            )
            .is_err());
    }
}

#[test]
fn code_policy_rejects_actual_type_hash_receipts_without_authenticated_history() {
    use cellscript::package::frozen_interface::{
        freeze_code_catalog, freeze_code_policy, freeze_package_sources, resolve_source_catalog, CodeCandidateInput,
    };
    use cellscript_artifact_checker::open_handle_policy::*;
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};
    let (_directory, library, _candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let make_module = || {
            compile_module(
                Utf8Path::from_path(&library).unwrap(),
                "dev",
                CompileOptions { diagnostics: opt, target_profile: Some("ckb-type-hash".into()), ..Default::default() },
                EntrySelection::Artifact("code-policy".into()),
            )
            .unwrap()
        };
        let module = make_module();
        let lock_script = packed::Script::new_builder().code_hash([9u8; 32].pack()).hash_type(2u8).args(Bytes::new().pack()).build();
        let code_type =
            packed::Script::new_builder().code_hash([8u8; 32].pack()).hash_type(1u8).args(Bytes::from(vec![4; 32]).pack()).build();
        let selected = packed::Script::new_builder()
            .code_hash(code_type.calc_script_hash())
            .hash_type(1u8)
            .args(Bytes::from(vec![1]).pack())
            .build();
        let tx = TransactionBuilder::default()
            .input(
                packed::CellInput::new_builder()
                    .previous_output(packed::OutPoint::new_builder().tx_hash([7u8; 32].pack()).index(3u32).build())
                    .build(),
            )
            .output(
                packed::CellOutput::new_builder().capacity(1000000000000u64).lock(lock_script).type_(Some(code_type).pack()).build(),
            )
            .output_data(Bytes::from(module.bundle()[0].to_vec()).pack())
            .build();
        let catalog = freeze_code_catalog(
            make_module(),
            vec![CodeCandidateInput {
                module,
                raw_transaction: tx.data().raw().as_slice().to_vec(),
                output_index: 0,
                selected_script: selected.as_slice().to_vec(),
            }],
            &Default::default(),
        )
        .unwrap();
        let sources =
            resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(&consumer).unwrap(), "dev").unwrap(), catalog).unwrap();
        assert_eq!(sources.catalog().candidates()[0].origin().selected_hash_type(), 1);
        let (header, mut members) = policy_records(&sources, false);
        members[0].hash_type = CodeHashType::Type;
        members[0].code_hash = policy_hash(sources.catalog().candidates()[0].origin().type_script_hash().unwrap());
        members[0].deployment_sequence = 1;
        members[0].deployment_line = [0x55; 32];
        members[0].history_tip = [0x66; 32];
        let wire_only = AuthorizationSet::new(header, &members).unwrap();
        assert!(freeze_code_policy(sources, wire_only).unwrap_err().message.contains("Type-hash history"));
    }
}

#[test]
fn source_policy_binds_type_id_history_and_the_exact_final_dependency() {
    use cellscript::package::frozen_interface::{
        freeze_code_catalog, freeze_package_sources, freeze_source_code_policy_with_histories, resolve_source_catalog,
        CodeCandidateInput,
    };
    use cellscript_artifact_checker::code_origin::{check_type_code_history, SuppliedDependencyCell, TypeCodeHistoryStep};
    use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, CodeHashType};
    use ckb_testtool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed, prelude::*};

    let (_directory, library, _candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for count in [1usize, 2, 32] {
            let compile = || {
                compile_module(
                    Utf8Path::from_path(&library).unwrap(),
                    "dev",
                    CompileOptions { diagnostics: opt, target_profile: Some("ckb-type-hash".into()), ..Default::default() },
                    EntrySelection::Artifact("code-policy".into()),
                )
                .unwrap()
            };
            let module = compile();
            let first = packed::CellInput::new_builder()
                .previous_output(packed::OutPoint::new_builder().tx_hash([0x31; 32].pack()).index(2u32).build())
                .build();
            let mut type_code_hash = [0u8; 32];
            type_code_hash[25..].copy_from_slice(b"TYPE_ID");
            let mut seed = first.as_slice().to_vec();
            seed.extend_from_slice(&0u64.to_le_bytes());
            let code_type = packed::Script::new_builder()
                .code_hash(type_code_hash.pack())
                .hash_type(1u8)
                .args(Bytes::from(ckb_testtool::ckb_hash::blake2b_256(&seed).to_vec()).pack())
                .build();
            let custody = packed::Script::new_builder().code_hash([0x32; 32].pack()).hash_type(2u8).build();
            let output = packed::CellOutput::new_builder()
                .capacity(1000000000000u64)
                .lock(custody.clone())
                .type_(Some(code_type.clone()).pack())
                .build();
            let mut input = first;
            let mut history = Vec::new();
            for sequence in 0..count {
                let data = if sequence + 1 == count { module.bundle()[0].to_vec() } else { vec![sequence as u8; 8] };
                let tx =
                    TransactionBuilder::default().input(input).output(output.clone()).output_data(Bytes::from(data).pack()).build();
                input = packed::CellInput::new_builder()
                    .previous_output(packed::OutPoint::new_builder().tx_hash(tx.hash()).index(0u32).build())
                    .build();
                history.push(tx.data().raw().as_slice().to_vec());
            }
            let selected = packed::Script::new_builder()
                .code_hash(code_type.calc_script_hash())
                .hash_type(1u8)
                .args(Bytes::from(vec![1]).pack())
                .build();
            let catalog = freeze_code_catalog(
                compile(),
                vec![CodeCandidateInput {
                    module,
                    raw_transaction: history.last().unwrap().clone(),
                    output_index: 0,
                    selected_script: selected.as_slice().to_vec(),
                }],
                &Default::default(),
            )
            .unwrap();
            let sources =
                resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(&consumer).unwrap(), "dev").unwrap(), catalog)
                    .unwrap();
            let target = sources.catalog().candidates()[0].target_origin();
            let steps = history.iter().map(|raw| TypeCodeHistoryStep { raw_transaction: raw, output_index: 0 }).collect::<Vec<_>>();
            let proof = check_type_code_history(target, &steps, &Default::default()).unwrap();
            assert_eq!(proof.sequence(), (count - 1) as u64);
            assert_eq!(proof.deployment_line().as_slice(), code_type.calc_script_hash().as_slice());
            // Reusable lineage may follow a new instance only when an
            // independent receipt still selects the exact same code output.
            let new_script = selected.clone().as_builder().args(Bytes::from(vec![2, 3]).pack()).build();
            let selected_candidate = &sources.catalog().candidates()[0];
            let instance_receipt = cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt(
                selected_candidate.module().bundle(),
                history.last().unwrap(),
                0,
                new_script.as_slice(),
                &Default::default(),
            )
            .unwrap();
            let instance_history = proof.bind_instance(target, instance_receipt.target_origin()).unwrap();
            assert_eq!(instance_history.target_identity(), instance_receipt.target_origin().identity());
            assert_ne!(instance_history.target_identity(), proof.target_identity());
            assert_eq!(instance_history.canonical_bytes().unwrap(), proof.canonical_bytes().unwrap());
            assert!(proof.bind_instance(instance_receipt.target_origin(), target).is_err());
            let last = packed::RawTransaction::from_slice(history.last().unwrap()).unwrap();
            let moved = last
                .clone()
                .as_builder()
                .outputs(vec![output.clone(), output.clone()].pack())
                .outputs_data(vec![last.outputs_data().get(0).unwrap(), last.outputs_data().get(0).unwrap()].pack())
                .build();
            let moved_receipt = cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt(
                selected_candidate.module().bundle(),
                moved.as_slice(),
                1,
                new_script.as_slice(),
                &Default::default(),
            )
            .unwrap();
            assert!(proof.bind_instance(target, moved_receipt.target_origin()).is_err());
            let record: serde_json::Value = serde_json::from_slice(&proof.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["steps"].as_array().unwrap().len(), count);
            // A truncated lineage never becomes a fresh Type-ID genesis.
            if count > 1 {
                assert!(check_type_code_history(target, &steps[1..], &Default::default()).unwrap_err().message.contains("genesis"));
            }
            let duplicate = (0..33).map(|_| TypeCodeHistoryStep { raw_transaction: &history[0], output_index: 0 }).collect::<Vec<_>>();
            assert!(check_type_code_history(target, &duplicate, &Default::default()).is_err());
            let mut changed = history.clone();
            changed[0].push(0);
            let changed_steps =
                changed.iter().map(|raw| TypeCodeHistoryStep { raw_transaction: raw, output_index: 0 }).collect::<Vec<_>>();
            assert!(check_type_code_history(target, &changed_steps, &Default::default()).is_err());

            // Rebind canonical transaction bytes rather than merely corrupting
            // a digest: custody, Type-ID, predecessor, uniqueness and the final
            // artifact each have an independently enforced boundary.
            let last = packed::RawTransaction::from_slice(history.last().unwrap()).unwrap();
            let mutations = [
                last.clone()
                    .as_builder()
                    .outputs(
                        vec![output
                            .clone()
                            .as_builder()
                            .lock(custody.clone().as_builder().args(Bytes::from(vec![9]).pack()).build())
                            .build()]
                        .pack(),
                    )
                    .build(),
                last.clone()
                    .as_builder()
                    .outputs(
                        vec![output
                            .clone()
                            .as_builder()
                            .type_(Some(code_type.clone().as_builder().args(Bytes::from(vec![9; 32]).pack()).build()).pack())
                            .build()]
                        .pack(),
                    )
                    .build(),
                last.clone()
                    .as_builder()
                    .inputs(
                        vec![packed::CellInput::new_builder()
                            .previous_output(packed::OutPoint::new_builder().tx_hash([0x99; 32].pack()).build())
                            .build()]
                        .pack(),
                    )
                    .build(),
                last.clone()
                    .as_builder()
                    .outputs(vec![output.clone(), output.clone()].pack())
                    .outputs_data(vec![last.outputs_data().get(0).unwrap(), last.outputs_data().get(0).unwrap()].pack())
                    .build(),
                last.clone().as_builder().outputs_data(vec![Bytes::from(vec![0x99]).pack()].pack()).build(),
            ];
            for mutation in mutations {
                let mut changed = history.clone();
                *changed.last_mut().unwrap() = mutation.as_slice().to_vec();
                let refs = changed.iter().map(|raw| TypeCodeHistoryStep { raw_transaction: raw, output_index: 0 }).collect::<Vec<_>>();
                assert!(check_type_code_history(target, &refs, &Default::default()).is_err(), "opt={opt} count={count}");
            }
            let repeated = [
                TypeCodeHistoryStep { raw_transaction: &history[0], output_index: 0 },
                TypeCodeHistoryStep { raw_transaction: &history[0], output_index: 0 },
            ];
            assert!(check_type_code_history(target, &repeated, &Default::default()).is_err());

            let (header, mut members) = source_policy_records(&sources, false, 0);
            members[0].hash_type = CodeHashType::Type;
            members[0].code_hash = proof.deployment_line();
            members[0].deployment_line = proof.deployment_line();
            members[0].history_tip = proof.history_tip();
            members[0].deployment_sequence = proof.sequence();
            let policy =
                freeze_source_code_policy_with_histories(sources, AuthorizationSet::new(header, &members).unwrap(), vec![proof])
                    .unwrap();
            let witness = policy.policy().selection(&members[0].receipt).unwrap();
            let candidate = policy.sources().catalog().candidates().first().unwrap();
            let (raw, point, output_bytes, data) = final_code_dependency(candidate);
            let cells = [SuppliedDependencyCell { out_point: point, output: &output_bytes, data: &data }];
            let checked =
                policy.check_selection(&witness).unwrap().check_direct_dependency(&raw, &cells, &Default::default()).unwrap();
            checked.check_unchanged_inputs(&raw, &cells, &Default::default()).unwrap();
            let bad_data = [0x99];
            let bad_cells = [SuppliedDependencyCell { out_point: point, output: &output_bytes, data: &bad_data }];
            assert!(policy.check_selection(&witness).unwrap().check_direct_dependency(&raw, &bad_cells, &Default::default()).is_err());
            let mut other_point = point;
            other_point[0] ^= 1;
            let copied = [SuppliedDependencyCell { out_point: other_point, output: &output_bytes, data: &data }];
            assert!(policy.check_selection(&witness).unwrap().check_direct_dependency(&raw, &copied, &Default::default()).is_err());
        }
    }
}

fn source_policy_records(
    sources: &cellscript::package::frozen_interface::ResolvedSourceCatalog,
    exact: bool,
    selected: usize,
) -> (cellscript_artifact_checker::open_handle_policy::PolicyHeader, Vec<cellscript_artifact_checker::open_handle_policy::PolicyMember>)
{
    let (mut header, mut members) = policy_records(sources, exact);
    header.required_interface = policy_hash(sources.source_owner_identity());
    for (member, candidate) in members.iter_mut().zip(sources.catalog().candidates()) {
        member.receipt = policy_hash(candidate.source_receipt().identity());
    }
    if exact {
        header.exact_receipt = members[selected].receipt;
    }
    (header, members)
}
fn source_version(root: &Path, version: &str) {
    let manager = cellscript::package::PackageManager::new(root);
    let mut manifest = manager.read_manifest().unwrap();
    manifest.package.version = version.into();
    manager.write_manifest(&manifest).unwrap();
    lock(root);
}

#[test]
fn open_verifier_target_claim_rejects_unbound_or_conditional_delegation() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let app = directory.path().join("app");
    package(&library, "library", "module foreign\npublic action verify() { verification require true }\n", "");
    lock(&library);
    let membership = "let dep = ckb::cell_dep(0) ckb::require_cell_dep_open_verifier_handle_from_args(dep, handle)";
    let spawn = "ckb::spawn_wait_cell_dep_hex4(0, bytes, 1, 0, 0, 0)";
    let bodies = [
        spawn.to_string(),
        format!("{membership} {}", spawn.replace("hex4(0,", "hex4(1,")),
        format!("{spawn} {membership}"),
        format!("if ckb::cell_count(ckb::cell_dep(0)) == 3 {{ {membership} }} {spawn}"),
        format!("{membership} {spawn} {spawn}"),
    ];
    for (case, body) in bodies.iter().enumerate() {
        package(&app, "client", &format!("module client\nuse foreign::verify\npublic action accept(witness handle: VerifierHandle<foreign>) {{ verification let mut bytes = Vec::new() bytes.push(0 as u8) {body} }}\n"), "[dependencies]\nlibrary = { path = \"../library\" }\n");
        lock(&app);
        for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
            let error = compile_module(
                Utf8Path::from_path(&app).unwrap(),
                "dev",
                CompileOptions { diagnostics, ..Default::default() },
                EntrySelection::Action("accept".into()),
            )
            .unwrap_err();
            assert!(error.to_string().contains("spawn"), "case={case} {diagnostics}: {error}");
        }
    }
}

#[test]
fn open_lock_and_verifier_profiles_materialize_and_execute_identical_transactions() {
    use cellscript::package::frozen_interface::{
        freeze_code_catalog, freeze_package_sources, freeze_source_code_policy, resolve_source_catalog,
    };
    use cellscript::protocol_bundle::{
        check_protocol_bundle_with_open_handles, ProtocolBundleInput, ProtocolOpenHandleBinding, ProtocolOpenTransaction,
    };
    use cellscript_artifact_checker::code_origin::{SuppliedDependencyCell, SuppliedInputCell};
    use cellscript_artifact_checker::open_handle_policy::{AuthorizationSet, HandleClass, ScriptRole};
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let hex = |bytes: &[u8]| format!("0x{}", cellscript_artifact_checker::hex_encode(bytes));
    let script_json = |script: &packed::Script| serde_json::json!({"code_hash": hex(script.code_hash().as_slice()), "hash_type": "data2", "args": hex(&script.args().raw_data())});
    {
        let opt = cellscript::CompilerDiagnostics::default();
        for verifier in [false, true] {
            for consumer_lock in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let root = directory.path();
                let library = root.join("library");
                let app = root.join("app");
                let provider_body = if verifier {
                    "public action verify() { verification require ckb::cell_count(ckb::cell_dep(0)) == 3 }"
                } else {
                    "public lock verify(witness value: u64) -> bool { verification value == 7 }"
                };
                package(&library, "library", &format!("module foreign\n{provider_body}\n"), "");
                lock(&library);
                let class = if verifier { "VerifierHandle" } else { "ScriptHandle" };
                let helper = if verifier {
                    "require_cell_dep_open_verifier_handle_from_args"
                } else {
                    "require_cell_dep_open_script_handle_from_args"
                };
                let spawn = if verifier {
                    "let mut bytes = Vec::new() bytes.push(0 as u8) ckb::spawn_wait_cell_dep_hex4(0, bytes, 1, 0, 0, 0)"
                } else {
                    ""
                };
                let declaration = if consumer_lock { "lock" } else { "action" };
                let result = if consumer_lock { "-> bool" } else { "" };
                let tail = if consumer_lock { "true" } else { "" };
                package(&app, "client", &format!("module client\nuse foreign::verify\npublic {declaration} accept(witness handle: {class}<foreign>) {result} {{ verification let dep = ckb::cell_dep(0) ckb::{helper}(dep, handle) {spawn} {tail} }}\n"), "[dependencies]\nlibrary = { path = \"../library\" }\n");
                lock(&app);
                let build = |root: &Path, is_lock: bool, name: &str| {
                    compile_module(
                        Utf8Path::from_path(root).unwrap(),
                        "dev",
                        CompileOptions { diagnostics: opt, ..Default::default() },
                        if is_lock { EntrySelection::Lock(name.into()) } else { EntrySelection::Action(name.into()) },
                    )
                    .unwrap()
                };
                let catalog = freeze_code_catalog(
                    build(&library, !verifier, "verify"),
                    vec![code_candidate(build(&library, !verifier, "verify"), vec![1])],
                    &Default::default(),
                )
                .unwrap();
                let sources =
                    resolve_source_catalog(freeze_package_sources(Utf8Path::from_path(&app).unwrap(), "dev").unwrap(), catalog)
                        .unwrap();
                let (mut header, members) = source_policy_records(&sources, false, 0);
                header.class = if verifier { HandleClass::Verifier } else { HandleClass::Script };
                header.role = if verifier { ScriptRole::SpawnedVerifier } else { ScriptRole::Lock };
                let policy = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
                let selection = policy.policy().selection(&members[0].receipt).unwrap();
                let chosen = policy.check_selection(&selection).unwrap();
                let consumer = build(&app, consumer_lock, "accept");
                let (raw, point, code_output, code_data) = final_code_dependency(chosen.candidate());
                let provider_script = packed::Script::from_slice(chosen.candidate().selected_script()).unwrap();
                let consumer_script = packed::Script::new_builder()
                    .code_hash(packed::CellOutput::calc_data_hash(consumer.bundle()[0]))
                    .hash_type(4u8)
                    .args(Bytes::copy_from_slice(&policy.policy().root()).pack())
                    .build();
                let always = cellscript::compile(
                    "module always\nlock permit() -> bool { verification true }",
                    CompileOptions { target: Some("riscv64-elf".into()), target_profile: Some("ckb".into()), ..Default::default() },
                )
                .unwrap();
                let always_script = packed::Script::new_builder()
                    .code_hash(packed::CellOutput::calc_data_hash(&always.artifact_bytes))
                    .hash_type(4u8)
                    .build();
                let consumer_point = packed::OutPoint::new_builder().tx_hash([0x83u8; 32].pack()).build();
                let always_point = packed::OutPoint::new_builder().tx_hash([0x84u8; 32].pack()).build();
                let input_points = [0x81u8, 0x82].map(|byte| packed::OutPoint::new_builder().tx_hash([byte; 32].pack()).build());
                let input_outputs = [
                    packed::CellOutput::new_builder()
                        .capacity(200_000_000_000u64)
                        .lock(if consumer_lock { consumer_script.clone() } else { always_script.clone() })
                        .type_(if consumer_lock { None } else { Some(consumer_script.clone()) }.pack())
                        .build(),
                    packed::CellOutput::new_builder()
                        .capacity(200_000_000_000u64)
                        .lock(if verifier { always_script.clone() } else { provider_script.clone() })
                        .build(),
                ];
                let payloads = [
                    [b"CSARGv1\0".as_slice(), selection.as_slice()].concat(),
                    [b"CSARGv1\0".as_slice(), &7u64.to_le_bytes()].concat(),
                ];
                let witnesses = payloads
                    .iter()
                    .map(|p| {
                        packed::WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(p)).pack()).build().as_bytes().pack()
                    })
                    .collect::<Vec<_>>();
                let raw = packed::RawTransaction::from_slice(&raw).unwrap();
                let deps_raw = raw
                    .cell_deps()
                    .into_iter()
                    .chain([consumer_point.clone(), always_point.clone()].map(|p| packed::CellDep::new_builder().out_point(p).build()))
                    .collect::<Vec<_>>();
                let tx = packed::Transaction::new_builder()
                    .raw(
                        raw.as_builder()
                            .cell_deps(deps_raw.pack())
                            .inputs(
                                input_points
                                    .iter()
                                    .map(|p| packed::CellInput::new_builder().previous_output(p.clone()).build())
                                    .collect::<Vec<_>>()
                                    .pack(),
                            )
                            .build(),
                    )
                    .witnesses(witnesses.pack())
                    .build();
                let inputs = [0, 1].map(|i| SuppliedInputCell {
                    out_point: input_points[i].as_slice().try_into().unwrap(),
                    output: input_outputs[i].as_slice(),
                    data: &[],
                });
                let deps = [
                    SuppliedDependencyCell { out_point: point, output: &code_output, data: &code_data },
                    SuppliedDependencyCell {
                        out_point: consumer_point.as_slice().try_into().unwrap(),
                        output: &code_output,
                        data: consumer.bundle()[0],
                    },
                    SuppliedDependencyCell {
                        out_point: always_point.as_slice().try_into().unwrap(),
                        output: &code_output,
                        data: &always.artifact_bytes,
                    },
                ];
                let network = serde_json::json!({"chain_id": "test-chain", "genesis_hash": format!("0x{}", "11".repeat(32))});
                let dep_json = |p: &[u8]| serde_json::json!({"out_point": {"tx_hash": hex(&p[..32]), "index": u32::from_le_bytes(p[32..].try_into().unwrap())}, "dep_type": "code"});
                let mut artifacts = Vec::new();
                for (id, name, module, script, dep, role, kind) in [
                    (
                        "consumer",
                        "accept",
                        &consumer,
                        &consumer_script,
                        consumer_point.as_slice(),
                        if consumer_lock { "lock" } else { "type" },
                        declaration,
                    ),
                    (
                        "provider",
                        "verify",
                        chosen.candidate().module(),
                        &provider_script,
                        &point,
                        if verifier { "spawned-verifier" } else { "lock" },
                        if verifier { "action" } else { "lock" },
                    ),
                ] {
                    let files =
                        [format!("{id}.elf"), format!("{id}.meta.json"), format!("{id}.lowering.json"), format!("{id}.map.json")];
                    for (file, bytes) in files.iter().zip(module.bundle()) {
                        std::fs::write(root.join(file), bytes).unwrap();
                    }
                    let generated = common::cellc_command()
                        .args(["gen-builder", "--metadata"])
                        .arg(root.join(&files[1]))
                        .args(["--target", "typescript", "--output"])
                        .arg(root.join(format!("{id}-builder")))
                        .output()
                        .unwrap();
                    assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
                    artifacts.push(serde_json::json!({"id": id, "package_coordinate": id, "lock_node_id": id, "entry": {"kind": kind, "name": name}, "script_role": role,
                        "files": {"artifact": files[0], "metadata": files[1], "lowering_record": files[2], "source_map": files[3], "builder_manifest": format!("{id}-builder/cellscript-builder-manifest.json")},
                        "deployment": {"network": network, "artifact_hash": cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(module.bundle()[0])), "script": script_json(script), "code_cell_dep": dep_json(dep)}}));
                }
                let skeleton = serde_json::json!({"version": 0, "inputs": ([0, 1].map(|i| serde_json::json!({"cell_commitment": hex(&[i as u8 + 1; 32]), "capacity": 200_000_000_000u64, "lock": script_json(&input_outputs[i].lock()), "type": input_outputs[i].type_().to_opt().as_ref().map(script_json), "data": "0x", "out_point": {"tx_hash": hex(input_points[i].tx_hash().as_slice()), "index": 0}, "since": 0}))), "outputs": [], "witnesses": payloads.iter().map(|p| serde_json::json!({"input_type": hex(&cellscript_artifact_checker::ckb_blake2b256(p)), "input_type_bytes": hex(p)})).collect::<Vec<_>>(), "cell_deps": [dep_json(&point), dep_json(consumer_point.as_slice()), dep_json(always_point.as_slice())], "header_deps": [], "fee_policy_hash": hex(&[8; 32]), "change_policy_hash": hex(&[9; 32])});
                let mut roles = vec![
                    serde_json::json!({"artifact": "consumer", "name": "consumer-input", "location": "input", "index": 0, "ownership": "exclusive"}),
                ];
                roles[0][if consumer_lock { "expected_lock" } else { "expected_type" }] = script_json(&consumer_script);
                if !verifier {
                    roles.push(serde_json::json!({"artifact": "provider", "name": "provider-input", "location": "input", "index": 1, "ownership": "exclusive", "expected_lock": script_json(&provider_script)}));
                }
                let input: ProtocolBundleInput = serde_json::from_value(serde_json::json!({"schema": "cellscript-protocol-bundle-input-v1", "network": network, "artifacts": artifacts, "transaction": skeleton, "roles": roles})).unwrap();
                let bindings = [ProtocolOpenHandleBinding {
                    consumer: "consumer",
                    parameter: "handle",
                    consumer_module: &consumer,
                    policy: &policy,
                    selection: &selection,
                    participant: "provider",
                }];
                let supplied = ProtocolOpenTransaction {
                    full_transaction: tx.as_slice(),
                    inputs: &inputs,
                    dependencies: &deps,
                    bindings: &bindings,
                };
                let report = check_protocol_bundle_with_open_handles(&input, root, &supplied).unwrap();
                if !verifier {
                    // A deployed, independently compatible Lock with no input
                    // group cannot be represented as an executing participant.
                    let without_peer = input_outputs[1].clone().as_builder().lock(always_script.clone()).build();
                    let changed_inputs = [
                        SuppliedInputCell { out_point: inputs[0].out_point, output: inputs[0].output, data: inputs[0].data },
                        SuppliedInputCell { out_point: inputs[1].out_point, output: without_peer.as_slice(), data: inputs[1].data },
                    ];
                    let mut changed = input.clone();
                    changed.transaction.inputs[1].lock = serde_json::from_value(script_json(&always_script)).unwrap();
                    changed.roles.retain(|role| role.artifact != "provider");
                    let missing = ProtocolOpenTransaction { inputs: &changed_inputs, ..supplied };
                    assert!(check_protocol_bundle_with_open_handles(&changed, root, &missing).is_err());
                }
                let (materialized, _) =
                    cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
                assert_eq!(materialized.data().as_slice(), tx.as_slice());
                let mut context = ckb_testtool::context::Context::default();
                for cell in &deps {
                    context.create_cell_with_out_point(
                        packed::OutPoint::from_slice(&cell.out_point).unwrap(),
                        packed::CellOutput::from_slice(cell.output).unwrap(),
                        Bytes::copy_from_slice(cell.data),
                    );
                }
                for cell in &inputs {
                    context.create_cell_with_out_point(
                        packed::OutPoint::from_slice(&cell.out_point).unwrap(),
                        packed::CellOutput::from_slice(cell.output).unwrap(),
                        Bytes::copy_from_slice(cell.data),
                    );
                }
                let cycles = context.verify_tx(&materialized, 3_000_000).unwrap();
                eprintln!("open Lock/verifier {opt} verifier={verifier} consumer_lock={consumer_lock}: {cycles} cycles");
                assert_eq!(report.bundle.open_handles[0].verifier_invocation.is_some(), verifier);
                assert_eq!(report.bundle.open_handles[0].peer_input_indices, if verifier { vec![] } else { vec![1] });
                if verifier {
                    let lowering: cellscript_artifact_checker::VerifiedLoweringRecord =
                        serde_json::from_slice(consumer.bundle()[2]).unwrap();
                    assert!(lowering
                        .proof_records
                        .iter()
                        .any(|proof| proof.obligation == "spawn-target-and-checked-child-exit:spawn-delegation:checked-runtime"
                            && proof.evidence_tier == "checked-runtime"));
                    let failed = lowering
                        .blocks
                        .iter()
                        .find(|b| b.machine_label.as_deref().is_some_and(|l| l.starts_with(".Lexec_hex4_failed_")))
                        .unwrap()
                        .range
                        .start;
                    // Rebind every outer identity after turning a child failure
                    // into success. The standalone machine proof must reject.
                    let changed = rebound_open_machine_bundle(&consumer, failed, 0x00000513);
                    let error = cellscript_artifact_checker::check_bundle(
                        &changed[0],
                        &changed[1],
                        &changed[2],
                        &changed[3],
                        &Default::default(),
                    )
                    .unwrap_err();
                    assert_eq!(error.code, cellscript_artifact_checker::CheckerRejectionCode::V2420TypedMachineBindingInvalid);
                }
                // Rebinding a serialized report never changes the selected
                // role, group, dependency or inherited verifier context.
                for pointer in [
                    "/open_handles/0/code_dependency_index",
                    "/open_handles/0/peer_input_indices",
                    "/artifacts/1/script_role",
                    "/open_handles/0/claim_boundaries/participant_predicates_and_data",
                ] {
                    let mut changed = serde_json::to_value(&report).unwrap();
                    *changed["bundle"].pointer_mut(pointer).unwrap() = match pointer {
                        "/open_handles/0/code_dependency_index" => serde_json::json!(2),
                        "/open_handles/0/peer_input_indices" => serde_json::json!([0]),
                        "/artifacts/1/script_role" => serde_json::json!("type"),
                        _ => serde_json::json!("checked_by_current_script"),
                    };
                    changed["bundle_hash"] = serde_json::json!(cellscript_artifact_checker::canonical_hash(
                        "cellscript-protocol-bundle-v1",
                        &changed["bundle"]
                    )
                    .unwrap());
                    assert!(
                        cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&changed).unwrap()).is_err(),
                        "rebound {pointer}"
                    );
                }
                // A valid receipt and ABI do not make a false peer predicate
                // succeed. Execute the rejected request through every group.
                let mut rejected_input = input.clone();
                let (rejected_tx, rejected_report) = if verifier {
                    let extra_point = packed::OutPoint::new_builder().tx_hash([0x85u8; 32].pack()).build();
                    let extra_data = b"extra-dependency";
                    let extra_dep = packed::CellDep::new_builder().out_point(extra_point.clone()).build();
                    let extra = SuppliedDependencyCell {
                        out_point: extra_point.as_slice().try_into().unwrap(),
                        output: &code_output,
                        data: extra_data,
                    };
                    let rejected_deps = deps
                        .iter()
                        .chain(std::iter::once(&extra))
                        .map(|cell| SuppliedDependencyCell { out_point: cell.out_point, output: cell.output, data: cell.data })
                        .collect::<Vec<_>>();
                    let raw = tx
                        .raw()
                        .as_builder()
                        .cell_deps(tx.raw().cell_deps().into_iter().chain([extra_dep]).collect::<Vec<_>>().pack())
                        .build();
                    let rejected_tx = tx.clone().as_builder().raw(raw).build();
                    rejected_input.transaction.cell_deps.push(serde_json::from_value(dep_json(extra_point.as_slice())).unwrap());
                    let supplied =
                        ProtocolOpenTransaction { full_transaction: rejected_tx.as_slice(), dependencies: &rejected_deps, ..supplied };
                    let rejected_report = check_protocol_bundle_with_open_handles(&rejected_input, root, &supplied).unwrap();
                    context.create_cell_with_out_point(
                        extra_point,
                        packed::CellOutput::from_slice(&code_output).unwrap(),
                        Bytes::copy_from_slice(extra_data),
                    );
                    (rejected_tx, rejected_report)
                } else {
                    let bytes = [b"CSARGv1\0".as_slice(), &6u64.to_le_bytes()].concat();
                    let witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(&bytes)).pack()).build();
                    let rejected_tx = tx
                        .clone()
                        .as_builder()
                        .witnesses(vec![tx.witnesses().get(0).unwrap(), witness.as_bytes().pack()].pack())
                        .build();
                    rejected_input.transaction.witnesses[1].input_type =
                        Some(hex(&cellscript_artifact_checker::ckb_blake2b256(&bytes)));
                    rejected_input.transaction.witnesses[1].input_type_bytes = Some(hex(&bytes));
                    let supplied = ProtocolOpenTransaction { full_transaction: rejected_tx.as_slice(), ..supplied };
                    let rejected_report = check_protocol_bundle_with_open_handles(&rejected_input, root, &supplied).unwrap();
                    (rejected_tx, rejected_report)
                };
                let (rejected_materialized, _) =
                    cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rejected_report).unwrap())
                        .unwrap();
                assert_eq!(rejected_materialized.data().as_slice(), rejected_tx.as_slice());
                let error = context.verify_tx(&rejected_materialized, 3_000_000).unwrap_err();
                let expected_group = if !verifier {
                    "Inputs[1].Lock"
                } else if consumer_lock {
                    "Inputs[0].Lock"
                } else {
                    "Inputs[0].Type"
                };
                assert!(format!("{error:?}").contains(expected_group), "{error:?}");

                std::fs::write(root.join("open-report.json"), serde_json::to_vec(&report).unwrap()).unwrap();
                std::fs::write(root.join("bundle-input.json"), serde_json::to_vec(&input).unwrap()).unwrap();
                std::fs::write(root.join("transaction.bin"), tx.as_slice()).unwrap();
                std::fs::write(root.join("creation.bin"), chosen.candidate().raw_transaction()).unwrap();
                let provider_request = serde_json::json!({"package": "library", "environment": "dev", "entry_kind": if verifier { "action" } else { "lock" }, "entry": "verify"});
                let request = serde_json::json!({"schema": "cellscript-protocol-open-admission-input-v2", "transaction_file": "transaction.bin",
                    "inputs": inputs.iter().map(|c| serde_json::json!({"out_point": hex(&c.out_point), "output": hex(c.output), "data": hex(c.data)})).collect::<Vec<_>>(),
                    "dependencies": deps.iter().map(|c| serde_json::json!({"out_point": hex(&c.out_point), "output": hex(c.output), "data": hex(c.data)})).collect::<Vec<_>>(),
                    "bindings": [{"consumer": "consumer", "parameter": "handle", "participant": "provider",
                        "consumer_module": {"package": "app", "environment": "dev", "entry_kind": declaration, "entry": "accept"},
                        "required": provider_request, "candidates": [{"module": provider_request, "creation_transaction_file": "creation.bin", "output_index": 0, "selected_script": hex(chosen.candidate().selected_script())}],
                        "policy_header": hex(&policy.policy().header().encode().unwrap()), "policy_members": policy.policy().members().iter().map(|m| hex(&m.encode().unwrap())).collect::<Vec<_>>(), "selection": hex(&selection)}]});
                std::fs::write(root.join("open-input.json"), serde_json::to_vec(&request).unwrap()).unwrap();
                let cli = common::cellc_command()
                    .current_dir(root)
                    .args([
                        "protocol",
                        "bundle",
                        "check",
                        "bundle-input.json",
                        "--open-admission",
                        "open-input.json",
                        "--output",
                        "cli-report.json",
                        "--json",
                    ])
                    .output()
                    .unwrap();
                assert!(cli.status.success(), "{}", String::from_utf8_lossy(&cli.stderr));
                let cli_report: Value = serde_json::from_slice(&std::fs::read(root.join("cli-report.json")).unwrap()).unwrap();
                assert_eq!(cli_report, serde_json::to_value(&report).unwrap());
                let smoke = root.join("consumer-builder/open-witness.mjs");
                std::fs::write(&smoke, r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {bindProtocolBundleArtifact,openHandleWitnessFromCheckedBundle} from './src/index.ts';
const report=JSON.parse(readFileSync(new URL('../open-report.json',import.meta.url),'utf8'));
const a=report.bundle.artifacts.find(a=>a.id==='consumer'),d=a.deployment;
const bound=bindProtocolBundleArtifact({id:a.id,packageCoordinate:a.package_coordinate,lockNodeId:a.lock_node_id,entry:a.entry,scriptRole:a.script_role,
deployment:{network:{chainId:d.network.chain_id,genesisHash:d.network.genesis_hash},artifactHash:d.artifact_hash,
script:{codeHash:d.script.code_hash,hashType:d.script.hash_type,args:d.script.args},codeCellDep:{outPoint:{txHash:d.code_cell_dep.out_point.tx_hash,index:d.code_cell_dep.out_point.index},depType:d.code_cell_dep.dep_type}}});
const witness=openHandleWitnessFromCheckedBundle(report,bound,'handle');
assert.equal('0x'+Buffer.from(witness.witnessPayload).toString('hex'),report.bundle.transaction.witnesses[0].input_type_bytes);
"#).unwrap();
                let node = std::process::Command::new("node").args(["--experimental-strip-types"]).arg(smoke).output().unwrap();
                assert!(node.status.success(), "{}", String::from_utf8_lossy(&node.stderr));
            }
        }
    }
}

fn rebound_open_machine_bundle(module: &FrozenPackageModule, address: u64, word: u32) -> [Vec<u8>; 4] {
    use cellscript_artifact_checker::{
        canonical_hash, ckb_blake2b256, hex_encode, SourceArtifactMap, VerifiedLoweringRecord, LOWERING_RECORD_SCHEMA,
        SOURCE_MAP_SCHEMA,
    };
    let mut bundle = module.bundle().map(<[u8]>::to_vec);
    let mut metadata: Value = serde_json::from_slice(&bundle[1]).unwrap();
    let mut record: VerifiedLoweringRecord = serde_json::from_slice(&bundle[2]).unwrap();
    let mut source_map: SourceArtifactMap = serde_json::from_slice(&bundle[3]).unwrap();
    let elf = cellscript_artifact_checker::parse_elf(&bundle[0], 1_000_000).unwrap();
    let offset = (elf.text.offset + address - elf.text.address) as usize;
    bundle[0][offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    for block in &mut record.blocks {
        let start = (elf.text.offset + block.range.start - elf.text.address) as usize;
        let end = (elf.text.offset + block.range.end - elf.text.address) as usize;
        block.byte_digest = cellscript_artifact_checker::domain_hash_bytes("cellscript-machine-block-v1", &bundle[0][start..end]);
    }
    let artifact_hash = hex_encode(&ckb_blake2b256(&bundle[0]));
    record.artifact_hash.clone_from(&artifact_hash);
    source_map.artifact_hash.clone_from(&artifact_hash);
    metadata["artifact_hash"] = artifact_hash.clone().into();
    metadata["verified_artifact"]["deployable_artifact_id"] = artifact_hash.into();
    let record_hash = canonical_hash(LOWERING_RECORD_SCHEMA, &record).unwrap();
    source_map.lowering_record_hash = record_hash.clone();
    let source_map_hash = canonical_hash(SOURCE_MAP_SCHEMA, &source_map).unwrap();
    let verified_bundle_id = canonical_hash(
        "cellscript-verified-bundle-id-v1",
        &(
            record.artifact_hash.as_str(),
            record.typed_semantics_hash.as_str(),
            record.compatibility_profile_hash.as_str(),
            record_hash.as_str(),
            source_map_hash.as_str(),
            source_map.source_digest.as_str(),
        ),
    )
    .unwrap();
    metadata["verified_artifact"]["lowering_record_hash"] = record_hash.into();
    metadata["verified_artifact"]["source_map_hash"] = source_map_hash.into();
    metadata["verified_artifact"]["verified_bundle_id"] = verified_bundle_id.into();
    bundle[1] = serde_json::to_vec(&metadata).unwrap();
    bundle[2] = serde_json::to_vec(&record).unwrap();
    bundle[3] = serde_json::to_vec(&source_map).unwrap();
    bundle
}

#[test]
fn source_code_receipts_bind_actual_context_versions_and_change_wire_roots() {
    use cellscript::package::frozen_interface::freeze_source_code_policy;
    use cellscript_artifact_checker::open_handle_policy::AuthorizationSet;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let mut previous = None;
        for version in ["1.0.0", "1.0.1+source.a"] {
            source_version(&candidate, version);
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let actual = &sources.catalog().candidates()[1];
            let receipt = actual.source_receipt();
            assert_eq!(receipt.source_context_identity(), actual.module().context_identity());
            assert_eq!(receipt.defining_module(), "foreign");
            assert_eq!(receipt.package_name(), "library");
            assert_eq!(receipt.package_namespace(), None);
            assert_eq!(receipt.package_version().to_string(), version);
            assert_eq!(receipt.edition(), &cellscript::CellScriptEdition::Edition2027);
            assert_eq!(receipt.artifact_receipt_identity(), actual.receipt().identity());
            let bytes = receipt.canonical_bytes().unwrap();
            let record: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["schema"], "cellscript-frozen-source-code-receipt-v1");
            assert_eq!(record["package"]["version"], version);
            let mut oracle = b"cellscript-frozen-source-code-receipt-id-v1\0".to_vec();
            oracle.extend_from_slice(&bytes);
            assert_eq!(
                receipt.identity(),
                cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&oracle))
            );
            let catalog: Value = serde_json::from_slice(&sources.catalog().canonical_bytes().unwrap()).unwrap();
            assert_eq!(catalog["schema"], "cellscript-frozen-code-catalog-v4");
            for (index, candidate) in sources.catalog().candidates().iter().enumerate() {
                assert_eq!(catalog["candidate_source_receipts"][index], candidate.source_receipt().identity());
            }
            let source_id = receipt.identity().to_owned();
            let finite_id = actual.receipt().identity().to_owned();
            let bundle = actual.module().bundle().map(<[u8]>::to_vec);
            let (header, members) = source_policy_records(&sources, false, 1);
            let checked = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
            let root = checked.policy().root();
            let policy_bytes = checked.canonical_bytes().unwrap();
            let policy: Value = serde_json::from_slice(&policy_bytes).unwrap();
            assert_eq!(policy["schema"], "cellscript-frozen-code-policy-bindings-v2");
            assert_eq!(policy["source_version_floor"]["version"], "1.0.0");
            assert!(policy["source_receipts"].as_array().unwrap().iter().any(|id| id == &source_id));
            let mut oracle = b"cellscript-frozen-code-policy-bindings-id-v2\0".to_vec();
            oracle.extend_from_slice(&policy_bytes);
            assert_eq!(
                checked.identity(),
                cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&oracle))
            );
            for member in &members {
                let witness = checked.policy().selection(&member.receipt).unwrap();
                let selection = checked.check_selection(&witness).unwrap();
                assert_eq!(policy_hash(selection.candidate().source_receipt().identity()), member.receipt);
            }
            if let Some((old_source, old_finite, old_bundle, old_root)) = previous.take() {
                assert_ne!(source_id, old_source);
                assert_ne!(root, old_root);
                // Real manifest-only edits leave these compiler/artifact bytes
                // unchanged; the native source receipt closes that wire gap.
                assert_eq!(bundle, old_bundle);
                assert_eq!(finite_id, old_finite);
            }
            previous = Some((source_id, finite_id, bundle, root));
        }
    }
}

#[test]
fn source_code_policy_enforces_semver_precedence_for_both_selection_modes() {
    use cellscript::package::frozen_interface::freeze_source_code_policy;
    use cellscript_artifact_checker::open_handle_policy::AuthorizationSet;
    let (_directory, library, candidate, consumer) = policy_fixture();
    source_version(&library, "1.0.0+z");
    lock(&consumer);
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for (version, compatible_ok, exact_ok) in [
            ("0.9.9", false, false),
            ("1.0.0-alpha", false, false),
            ("1.0.0+a", true, true),
            ("1.1.0", true, true),
            ("1.1.0-alpha", false, true),
            ("2.0.0", false, true),
        ] {
            source_version(&candidate, version);
            for exact in [false, true] {
                let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
                let (header, members) = source_policy_records(&sources, exact, 1);
                let result = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap());
                let expected = if exact { exact_ok } else { compatible_ok };
                assert_eq!(result.is_ok(), expected, "{opt}, {version}, exact={exact}: {result:?}");
                if let Ok(checked) = result {
                    let witness = checked.policy().selection(&members[1].receipt).unwrap();
                    checked.check_selection(&witness).unwrap();
                    if exact {
                        let other = checked.policy().selection(&members[0].receipt).unwrap();
                        assert!(checked.check_selection(&other).unwrap_err().message.contains("ExactReceipt"));
                    }
                }
            }
        }
    }
    source_version(&library, "1.0.0-alpha");
    lock(&consumer);
    source_version(&candidate, "1.0.0");
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
    let (header, members) = source_policy_records(&sources, false, 1);
    assert!(freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap())
        .unwrap_err()
        .message
        .contains("prerelease"));
}

#[test]
fn source_code_policy_retains_unselectable_history_without_relaxing_coordinates() {
    use cellscript::package::frozen_interface::freeze_source_code_policy;
    use cellscript_artifact_checker::open_handle_policy::*;
    let (_directory, library, candidate, consumer) = policy_fixture();
    source_version(&candidate, "0.9.0");
    for state in 0..4 {
        let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
        let (header, mut members) = source_policy_records(&sources, state == 3, 0);
        match state {
            0 => members[1].status = MemberStatus::Yanked,
            1 => members[1].admission_sequence = 2,
            2 => members[1].admission_sequence = 6,
            _ => {}
        }
        if state == 2 {
            assert!(matches!(AuthorizationSet::new(header, &members), Err(PolicyError::Sequence)));
            continue;
        }
        let checked = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let historical = checked.policy().selection(&members[1].receipt).unwrap();
        assert!(checked.check_selection(&historical).is_err());
        let baseline = checked.policy().selection(&members[0].receipt).unwrap();
        checked.check_selection(&baseline).unwrap();
    }
    for coordinate in ["name", "namespace"] {
        let manager = cellscript::package::PackageManager::new(&candidate);
        let mut manifest = manager.read_manifest().unwrap();
        manifest.package.name = if coordinate == "name" { "other" } else { "library" }.into();
        manifest.package.namespace = if coordinate == "namespace" { Some("other".into()) } else { None };
        manager.write_manifest(&manifest).unwrap();
        lock(&candidate);
        let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
        let (header, mut members) = source_policy_records(&sources, true, 0);
        members[1].status = MemberStatus::Yanked;
        assert!(freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap())
            .unwrap_err()
            .message
            .contains("coordinate/edition"));
    }
}

#[test]
fn source_code_policy_rejects_artifact_only_bindings_and_stale_source_versions() {
    use cellscript::package::frozen_interface::freeze_source_code_policy;
    use cellscript_artifact_checker::open_handle_policy::AuthorizationSet;
    let (_directory, library, candidate, consumer) = policy_fixture();
    for axis in 0..3 {
        let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
        let (mut header, mut members) = source_policy_records(&sources, true, 0);
        match axis {
            0 => header.required_interface = policy_hash(sources.catalog().required().projection().identity()),
            1 => members[1].receipt = policy_hash(sources.catalog().candidates()[1].receipt().identity()),
            _ => members[1].receipt[0] ^= 1,
        }
        assert!(freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).is_err());
    }
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
    let (header, members) = source_policy_records(&sources, false, 0);
    let checked = freeze_source_code_policy(sources, AuthorizationSet::new(header.clone(), &members).unwrap()).unwrap();
    let witness = checked.policy().selection(&members[1].receipt).unwrap();
    source_version(&candidate, "1.1.0");
    // The private candidate is an immutable historical snapshot, not a live
    // filesystem promise. Selection rechecks the consumer's pinned closure.
    let retained = checked.check_selection(&witness).unwrap();
    assert_eq!(retained.candidate().source_receipt().package_version().to_string(), "1.0.0");
    let refreshed = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
    // A newly constructed valid Merkle tree using old source identities still
    // fails against actual newly captured candidate source facts.
    assert!(freeze_source_code_policy(refreshed, AuthorizationSet::new(header, &members).unwrap())
        .unwrap_err()
        .message
        .contains("receipt"));
}

#[test]
fn source_code_policy_checks_all_32_source_receipts_and_bounds_version_text() {
    use cellscript::package::frozen_interface::{freeze_code_catalog, freeze_source_code_policy};
    use cellscript_artifact_checker::open_handle_policy::AuthorizationSet;
    let (_directory, library, candidate, consumer) = policy_fixture();
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 32);
    let (header, members) = source_policy_records(&sources, false, 0);
    let checked = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
    for member in &members {
        let witness = checked.policy().selection(&member.receipt).unwrap();
        checked.check_selection(&witness).unwrap();
    }
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 32);
    let (header, mut members) = source_policy_records(&sources, true, 0);
    members[31].receipt[0] ^= 1;
    assert!(freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).is_err());
    source_version(&candidate, &format!("1.0.0+{}", "a".repeat(129)));
    let oversized = code_candidate(compile_code(&candidate, cellscript::CompilerDiagnostics::without_optimizations()), vec![1]);
    assert!(freeze_code_catalog(
        compile_code(&library, cellscript::CompilerDiagnostics::without_optimizations()),
        vec![oversized],
        &Default::default()
    )
    .unwrap_err()
    .message
    .contains("text limits"));
}

fn final_code_dependency(
    candidate: &cellscript::package::frozen_interface::FrozenCodeCandidate,
) -> (Vec<u8>, [u8; 36], Vec<u8>, Vec<u8>) {
    use ckb_testtool::ckb_types::{core::TransactionBuilder, packed, prelude::*};
    let creation = packed::RawTransaction::from_slice(candidate.raw_transaction()).unwrap();
    let output = creation.outputs().get(candidate.origin().output_index() as usize).unwrap();
    let point = packed::OutPoint::new_builder()
        .tx_hash(policy_hash(candidate.origin().transaction_hash()).pack())
        .index(candidate.origin().output_index())
        .build();
    let tx =
        TransactionBuilder::default().cell_dep(packed::CellDep::new_builder().out_point(point.clone()).dep_type(0u8).build()).build();
    (
        tx.data().raw().as_slice().to_vec(),
        point.as_slice().try_into().unwrap(),
        output.as_slice().to_vec(),
        candidate.module().bundle()[0].to_vec(),
    )
}
#[test]
fn source_dependency_binds_source_versions_membership_and_actual_final_raw_inputs() {
    use cellscript::package::frozen_interface::{freeze_source_code_policy, FrozenSourceCodeDependency, FrozenSourceCodePolicy};
    use cellscript_artifact_checker::{code_origin::SuppliedDependencyCell, open_handle_policy::AuthorizationSet};
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for exact in [false, true] {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let (header, members) = source_policy_records(&sources, exact, 1);
            let policy: FrozenSourceCodePolicy =
                freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
            let witness = policy.policy().selection(&members[1].receipt).unwrap();
            let selection = policy.check_selection(&witness).unwrap();
            let (raw, point, output, data) = final_code_dependency(selection.candidate());
            let inputs = [SuppliedDependencyCell { out_point: point, output: &output, data: &data }];
            let checked: FrozenSourceCodeDependency<'_> =
                selection.check_direct_dependency(&raw, &inputs, &Default::default()).unwrap();
            assert_eq!(policy_hash(checked.selection().candidate().source_receipt().identity()), members[1].receipt);
            let bytes = checked.canonical_bytes().unwrap();
            let record: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["schema"], "cellscript-frozen-source-code-dependency-v1");
            assert_eq!(record["source_policy"], policy.identity());
            assert_eq!(record["checked_dependency"], checked.dependency().identity());
            let mut material = b"cellscript-frozen-source-code-dependency-id-v1\0".to_vec();
            material.extend_from_slice(&bytes);
            assert_eq!(
                checked.identity(),
                cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&material))
            );
            checked.check_unchanged_inputs(&raw, &inputs, &Default::default()).unwrap();
            let mut changed = data.clone();
            *changed.last_mut().unwrap() ^= 1;
            let replaced = [SuppliedDependencyCell { out_point: point, output: &output, data: &changed }];
            assert!(checked.check_unchanged_inputs(&raw, &replaced, &Default::default()).is_err());
            assert!(policy.check_selection(&witness).unwrap().check_direct_dependency(&raw, &replaced, &Default::default()).is_err());
            let mut substituted = raw.clone();
            *substituted.last_mut().unwrap() ^= 1;
            assert!(checked.check_unchanged_inputs(&substituted, &inputs, &Default::default()).is_err());
        }
    }
}
#[test]
fn source_dependency_rechecks_consumer_sources_before_binding_and_later_materialization() {
    use cellscript::package::frozen_interface::freeze_source_code_policy;
    use cellscript_artifact_checker::{code_origin::SuppliedDependencyCell, open_handle_policy::AuthorizationSet};
    let (_directory, library, candidate, consumer) = policy_fixture();
    let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
    let (header, members) = source_policy_records(&sources, false, 0);
    let policy = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
    let witness = policy.policy().selection(&members[0].receipt).unwrap();
    let selected = policy.check_selection(&witness).unwrap();
    let (raw, point, output, data) = final_code_dependency(selected.candidate());
    let inputs = [SuppliedDependencyCell { out_point: point, output: &output, data: &data }];
    let checked = selected.check_direct_dependency(&raw, &inputs, &Default::default()).unwrap();
    let later = policy.check_selection(&witness).unwrap();
    let path = consumer.join("src/main.cell");
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{source}// changed consumer after selection\n")).unwrap();
    assert!(later.check_direct_dependency(&raw, &inputs, &Default::default()).is_err());
    assert!(checked.check_unchanged_inputs(&raw, &inputs, &Default::default()).is_err());
}

#[test]
fn source_type_group_binds_final_witnesses_and_rechecks_consumer_sources() {
    use cellscript::package::frozen_interface::{freeze_source_code_policy, FrozenSourceCodeTypeGroup};
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use cellscript_artifact_checker::{
        code_origin::{SuppliedDependencyCell, SuppliedInputCell, SuppliedTypeGroupTransaction},
        open_handle_policy::AuthorizationSet,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for exact in [false, true] {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let (header, members) = source_policy_records(&sources, exact, 1);
            let policy = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
            let membership = policy.policy().selection(&members[1].receipt).unwrap();
            let selection = policy.check_selection(&membership).unwrap();
            let (raw, point, code_output, data) = final_code_dependency(selection.candidate());
            let selected = packed::Script::from_slice(selection.candidate().selected_script()).unwrap();
            let input_point = packed::OutPoint::new_builder().tx_hash([91u8; 32].pack()).index(2u32).build();
            let lock_script = packed::CellOutput::from_slice(&code_output).unwrap().lock();
            let input_output = packed::CellOutput::new_builder()
                .capacity(200000000000u64)
                .lock(lock_script)
                .type_(Some(selected.clone()).pack())
                .build();
            let mut args = b"CSARGv1\0".to_vec();
            args.extend_from_slice(&7u64.to_le_bytes());
            let bundle = encode_policy_witness_bundle(&[PolicyWitnessRecord {
                role: PolicyScriptRole::Type,
                script_hash: selected.calc_script_hash().as_slice().try_into().unwrap(),
                tag: 40,
                args,
            }])
            .unwrap();
            let witness = packed::WitnessArgs::new_builder()
                .lock(Some(Bytes::from(vec![0; 65])).pack())
                .input_type(Some(Bytes::from(bundle)).pack())
                .build();
            let raw = packed::RawTransaction::from_slice(&raw)
                .unwrap()
                .as_builder()
                .inputs(vec![packed::CellInput::new_builder().previous_output(input_point.clone()).build()].pack())
                .build();
            let tx = packed::Transaction::new_builder().raw(raw.clone()).witnesses(vec![witness.as_bytes().pack()].pack()).build();
            let deps = [SuppliedDependencyCell { out_point: point, output: &code_output, data: &data }];
            let input_data = 1u64.to_le_bytes();
            let inputs = [SuppliedInputCell {
                out_point: input_point.as_slice().try_into().unwrap(),
                output: input_output.as_slice(),
                data: &input_data,
            }];
            let supplied = SuppliedTypeGroupTransaction { full_transaction: tx.as_slice(), dependencies: &deps, inputs: &inputs };
            let dependency = selection.check_direct_dependency(raw.as_slice(), &deps, &Default::default()).unwrap();
            let group: FrozenSourceCodeTypeGroup<'_> = dependency.check_type_group(&supplied, &Default::default()).unwrap();
            assert_eq!(group.group().group_inputs(), &[0]);
            assert_eq!(
                group.selection().candidate().source_receipt().identity(),
                policy.sources().catalog().candidates()[1].source_receipt().identity()
            );
            let bytes = group.canonical_bytes().unwrap();
            let record: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(record["source_policy"], policy.identity());
            assert_eq!(record["checked_type_group"], group.group().identity());
            let mut material = b"cellscript-frozen-source-code-type-group-id-v1\0".to_vec();
            material.extend_from_slice(&bytes);
            assert_eq!(
                group.identity(),
                cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(&material))
            );
            group.check_unchanged_inputs(&supplied, &Default::default()).unwrap();
            let changed = tx.clone().as_builder().witnesses(vec![Bytes::from(vec![0]).pack()].pack()).build();
            assert!(group
                .check_unchanged_inputs(
                    &SuppliedTypeGroupTransaction { full_transaction: changed.as_slice(), dependencies: &deps, inputs: &inputs },
                    &Default::default()
                )
                .is_err());
            let source_path = consumer.join("src/main.cell");
            let original = std::fs::read_to_string(&source_path).unwrap();
            let later = policy
                .check_selection(&membership)
                .unwrap()
                .check_direct_dependency(raw.as_slice(), &deps, &Default::default())
                .unwrap();
            std::fs::write(&source_path, format!("{original}\n// changed consumer\n")).unwrap();
            let source_error = policy.sources().check_unchanged_sources().unwrap_err();
            let changed_group = later.check_type_group(&supplied, &Default::default()).unwrap_err();
            let changed_recheck = group.check_unchanged_inputs(&supplied, &Default::default()).unwrap_err();
            assert_eq!(changed_group.message, source_error.message);
            assert_eq!(changed_recheck.message, source_error.message);
            std::fs::write(&source_path, original).unwrap();
        }
    }
}

#[test]
fn instance_selection_binds_private_sources_actual_code_and_concrete_script_group() {
    use cellscript::package::frozen_interface::{freeze_instance_policy, freeze_source_code_policy};
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use cellscript_artifact_checker::{
        ckb_blake2b256,
        code_origin::{SuppliedDependencyCell, SuppliedInputCell, SuppliedTypeGroupTransaction},
        instance_policy::{encode_selection, instance_script, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy},
        open_handle_policy::AuthorizationSet,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let (_directory, library, candidate, consumer) = policy_fixture();
    for opt in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        for exact_implementation in [false, true] {
            let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
            let (header, members) = source_policy_records(&sources, exact_implementation, 1);
            let implementation = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
            let inner = implementation.policy().selection(&members[1].receipt).unwrap();
            let policy = InstancePolicy {
                implementation_root: implementation.policy().root(),
                mode: InstanceMode::Constrained,
                args_length: 10,
                config_length: 1,
                constraints: vec![
                    ArgsConstraint::Config { offset: 0, length: 1, config_offset: 0 },
                    ArgsConstraint::Unsigned { offset: 1, width: 8, minimum: 1, maximum: u64::MAX },
                    ArgsConstraint::Any { offset: 9, length: 1 },
                ],
            };
            let root = policy.root().unwrap();
            let mut args = vec![0x41];
            args.extend_from_slice(&0x0123456789abcdefu64.to_le_bytes());
            args.push(0x51);
            let wire = encode_selection(&policy, &inner, &args).unwrap();
            let script = instance_script(&members[1].code_hash, members[1].hash_type, &args).unwrap();
            let sdk_script = packed::Script::new_builder()
                .code_hash(members[1].code_hash.pack())
                .hash_type(match members[1].hash_type {
                    cellscript_artifact_checker::open_handle_policy::CodeHashType::Data => 0u8,
                    cellscript_artifact_checker::open_handle_policy::CodeHashType::Type => 1,
                    cellscript_artifact_checker::open_handle_policy::CodeHashType::Data1 => 2,
                    cellscript_artifact_checker::open_handle_policy::CodeHashType::Data2 => 4,
                })
                .args(Bytes::copy_from_slice(&args).pack())
                .build();
            assert_eq!(sdk_script.as_slice(), script);
            let subject = ckb_blake2b256(&script);
            assert_eq!(subject.as_slice(), sdk_script.calc_script_hash().as_slice());
            let consumer_args = ConsumerArgs { policy_root: root, subject, config: vec![0x41] }.encode().unwrap();
            let frozen = freeze_instance_policy(implementation, policy.clone()).unwrap();
            let selection = frozen.check_selection(&consumer_args, &wire, &Default::default()).unwrap();
            assert_eq!(selection.membership().script(), script);
            assert_ne!(selection.source_receipt().identity(), selection.implementation().candidate().source_receipt().identity());
            selection.check_unchanged_selection(&consumer_args, &wire).unwrap();
            let mut other_args = args.clone();
            other_args[9] ^= 1;
            let other_wire = encode_selection(&policy, &inner, &other_args).unwrap();
            assert!(frozen.check_selection(&consumer_args, &other_wire, &Default::default()).is_err());
            assert!(selection.check_unchanged_selection(&consumer_args, &other_wire).is_err());
            let other_script = instance_script(&members[1].code_hash, members[1].hash_type, &other_args).unwrap();
            let other_consumer =
                ConsumerArgs { policy_root: root, subject: ckb_blake2b256(&other_script), config: vec![0x41] }.encode().unwrap();
            // Both instances are allowed independently, but retain different
            // private admission identities and cannot share a concrete group.
            let other = frozen.check_selection(&other_consumer, &other_wire, &Default::default()).unwrap();
            assert_ne!(selection.identity(), other.identity());
            let (raw, point, code_output, data) = final_code_dependency(selection.implementation().candidate());
            let input_point = packed::OutPoint::new_builder().tx_hash([91u8; 32].pack()).index(2u32).build();
            let input_output = packed::CellOutput::new_builder()
                .capacity(200000000000u64)
                .lock(packed::CellOutput::from_slice(&code_output).unwrap().lock())
                .type_(Some(sdk_script.clone()).pack())
                .build();
            let mut entry_args = b"CSARGv1\0".to_vec();
            entry_args.extend_from_slice(&7u64.to_le_bytes());
            let bundle = encode_policy_witness_bundle(&[PolicyWitnessRecord {
                role: PolicyScriptRole::Type,
                script_hash: subject,
                tag: 40,
                args: entry_args,
            }])
            .unwrap();
            let witness = packed::WitnessArgs::new_builder()
                .lock(Some(Bytes::from(vec![0; 65])).pack())
                .input_type(Some(Bytes::from(bundle)).pack())
                .build();
            let raw = packed::RawTransaction::from_slice(&raw)
                .unwrap()
                .as_builder()
                .inputs(vec![packed::CellInput::new_builder().previous_output(input_point.clone()).build()].pack())
                .build();
            let tx = packed::Transaction::new_builder().raw(raw.clone()).witnesses(vec![witness.as_bytes().pack()].pack()).build();
            let deps = [SuppliedDependencyCell { out_point: point, output: &code_output, data: &data }];
            let input_data = 1u64.to_le_bytes();
            let inputs = [SuppliedInputCell {
                out_point: input_point.as_slice().try_into().unwrap(),
                output: input_output.as_slice(),
                data: &input_data,
            }];
            let supplied = SuppliedTypeGroupTransaction { full_transaction: tx.as_slice(), dependencies: &deps, inputs: &inputs };
            let group = selection
                .check_direct_dependency(raw.as_slice(), &deps, &Default::default())
                .unwrap()
                .check_script_group(&supplied, &Default::default())
                .unwrap();
            assert_eq!(group.group().group_inputs(), &[0]);
            let record: Value = serde_json::from_slice(&group.canonical_bytes().unwrap()).unwrap();
            assert_eq!(record["schema"], "cellscript-frozen-instance-group-v1");
            assert_eq!(record["policy"], frozen.identity());
            assert_eq!(record["selection"], group.selection().identity());
            assert_eq!(record["checked_transaction"], group.group().identity());
            let mut material = b"cellscript-frozen-instance-group-id-v1\0".to_vec();
            material.extend_from_slice(&group.canonical_bytes().unwrap());
            assert_eq!(group.identity(), cellscript_artifact_checker::hex_encode(&ckb_blake2b256(&material)));
            group.check_unchanged_inputs(&supplied, &Default::default()).unwrap();
            assert!(other
                .check_direct_dependency(raw.as_slice(), &deps, &Default::default())
                .unwrap()
                .check_script_group(&supplied, &Default::default())
                .is_err());
            let changed_tx = tx.clone().as_builder().witnesses(vec![Bytes::from(vec![0]).pack()].pack()).build();
            assert!(group
                .check_unchanged_inputs(
                    &SuppliedTypeGroupTransaction { full_transaction: changed_tx.as_slice(), dependencies: &deps, inputs: &inputs },
                    &Default::default()
                )
                .is_err());
            let mut changed_data = data.clone();
            *changed_data.last_mut().unwrap() ^= 1;
            let changed_deps = [SuppliedDependencyCell { out_point: point, output: &code_output, data: &changed_data }];
            assert!(frozen
                .check_selection(&consumer_args, &wire, &Default::default())
                .unwrap()
                .check_direct_dependency(raw.as_slice(), &changed_deps, &Default::default())
                .is_err());
            let path = consumer.join("src/main.cell");
            let original = std::fs::read_to_string(&path).unwrap();
            std::fs::write(&path, format!("{original}\n// source changed after instance admission\n")).unwrap();
            assert!(group.check_unchanged_inputs(&supplied, &Default::default()).is_err());
            assert!(frozen.check_selection(&consumer_args, &wire, &Default::default()).is_err());
            std::fs::write(&path, original).unwrap();
        }
    }
}

#[test]
fn exact_instance_admission_preserves_the_original_private_receipt() {
    use cellscript::package::frozen_interface::{freeze_instance_policy, freeze_source_code_policy};
    use cellscript_artifact_checker::{
        ckb_blake2b256,
        instance_policy::{encode_selection, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy},
        open_handle_policy::AuthorizationSet,
    };
    use ckb_testtool::ckb_types::{packed, prelude::*};
    let (_directory, library, candidate, consumer) = policy_fixture();
    for exact_implementation in [false, true] {
        let sources = policy_sources(&library, &candidate, &consumer, cellscript::CompilerDiagnostics::without_optimizations(), 2);
        let (header, members) = source_policy_records(&sources, exact_implementation, 1);
        let implementation = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let original = &implementation.sources().catalog().candidates()[1];
        let script = original.selected_script().to_vec();
        let receipt_identity = original.receipt().identity().to_string();
        let source_identity = original.source_receipt().identity().to_string();
        let args = packed::Script::from_slice(&script).unwrap().args().raw_data();
        let policy = InstancePolicy {
            implementation_root: implementation.policy().root(),
            mode: InstanceMode::Exact,
            args_length: args.len() as u16,
            config_length: 0,
            constraints: args
                .chunks(32)
                .enumerate()
                .map(|(index, value)| ArgsConstraint::Bytes { offset: (index * 32) as u16, value: value.to_vec() })
                .collect(),
        };
        let consumer_args =
            ConsumerArgs { policy_root: policy.root().unwrap(), subject: ckb_blake2b256(&script), config: vec![] }.encode().unwrap();
        let inner = implementation.policy().selection(&members[1].receipt).unwrap();
        let wire = encode_selection(&policy, &inner, &args).unwrap();
        let frozen = freeze_instance_policy(implementation, policy).unwrap();
        let selected = frozen.check_selection(&consumer_args, &wire, &Default::default()).unwrap();
        assert_eq!(selected.membership().script(), script);
        assert_eq!(selected.receipt().identity(), receipt_identity);
        assert_eq!(selected.source_receipt().identity(), source_identity);
    }
}

#[test]
fn open_protocol_admission_binds_private_receipts_to_complete_transaction_bytes() {
    protocol_admission_binds_private_receipts_to_complete_transaction_bytes(false);
}

#[test]
fn instance_protocol_admission_binds_private_receipts_to_complete_transaction_bytes() {
    protocol_admission_binds_private_receipts_to_complete_transaction_bytes(true);
}

fn protocol_admission_binds_private_receipts_to_complete_transaction_bytes(instance: bool) {
    use cellscript::package::frozen_interface::{freeze_instance_policy, freeze_source_code_policy};
    use cellscript::policy_witness::{encode_policy_witness_bundle, PolicyScriptRole, PolicyWitnessRecord};
    use cellscript::protocol_bundle::{
        check_protocol_bundle, check_protocol_bundle_with_instance_handles, check_protocol_bundle_with_open_handles,
        ProtocolBundleInput, ProtocolInstanceHandleBinding, ProtocolInstanceTransaction, ProtocolOpenHandleBinding,
        ProtocolOpenTransaction,
    };
    use cellscript_artifact_checker::{
        code_origin::{check_protocol_transaction, SuppliedDependencyCell, SuppliedInputCell},
        instance_policy::{self, ArgsConstraint, ConsumerArgs, InstanceMode, InstancePolicy},
        open_handle_policy::AuthorizationSet,
    };
    use ckb_sdk::{
        traits::{OffchainTransactionDependencyProvider, SecpCkbRawKeySigner},
        types::{ScriptGroup, ScriptId},
        unlock::{ScriptSigner, ScriptUnlocker, SecpSighashScriptSigner, SecpSighashUnlocker},
        SECP256K1,
    };
    use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
    let signer_key = secp256k1::SecretKey::from_slice(&[0x51; 32]).unwrap();
    let public_key = secp256k1::PublicKey::from_secret_key(&SECP256K1, &signer_key);
    let owner = cellscript_artifact_checker::ckb_blake2b256(&public_key.serialize());
    let signer = || SecpSighashScriptSigner::new(Box::new(SecpCkbRawKeySigner::new_with_secret_keys(vec![signer_key])));
    let lock_code = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_blake160_sighash_all").unwrap();
    let secp_data = ckb_system_scripts_v0_6_0::BUNDLED_CELL.get("specs/cells/secp256k1_data").unwrap();
    let (directory, library, candidate, consumer) = policy_fixture();
    let (handle_type, helper) = if instance {
        ("InstanceScriptHandle", "require_instance_script_participant_from_args")
    } else {
        ("ScriptHandle", "require_open_script_participant_from_args")
    };
    std::fs::write(consumer.join("src/main.cell"), format!("module client\nuse foreign::Token\npublic action verify(witness handle: {handle_type}<foreign>) {{ verification let dep = ckb::cell_dep(0) ckb::{helper}(dep, handle, ckb::input<Token>(1)) }}\n")).unwrap();
    lock(&consumer);
    let hex = |bytes: &[u8]| format!("0x{}", cellscript_artifact_checker::hex_encode(bytes));
    let script_json = |script: &packed::Script| serde_json::json!({"code_hash": hex(script.code_hash().as_slice()), "hash_type": match script.hash_type().as_slice()[0] {4 => "data2", 2 => "data1", 1 => "type", _ => "data"}, "args": hex(&script.args().raw_data())});
    {
        let opt = cellscript::CompilerDiagnostics::default();
        let sources = policy_sources(&library, &candidate, &consumer, opt, 2);
        let (header, members) = source_policy_records(&sources, false, 1);
        let policy = freeze_source_code_policy(sources, AuthorizationSet::new(header, &members).unwrap()).unwrap();
        let (plain_policy, instance_policy) = if instance {
            let constraints = InstancePolicy {
                implementation_root: policy.policy().root(),
                mode: InstanceMode::Constrained,
                args_length: 16,
                config_length: 8,
                constraints: vec![
                    ArgsConstraint::Config { offset: 0, length: 8, config_offset: 0 },
                    ArgsConstraint::Unsigned { offset: 8, width: 8, minimum: 1, maximum: 100 },
                ],
            };
            (None, Some(freeze_instance_policy(policy, constraints).unwrap()))
        } else {
            (Some(policy), None)
        };
        let policy = plain_policy.as_ref().unwrap_or_else(|| instance_policy.as_ref().unwrap().implementation());
        let implementation_selection = policy.policy().selection(&members[1].receipt).unwrap();
        let chosen = policy.check_selection(&implementation_selection).unwrap();
        let consumer_module = compile(&consumer, opt);
        let (raw, point, code_output, code_data) = final_code_dependency(chosen.candidate());
        let mut provider_script = packed::Script::from_slice(chosen.candidate().selected_script()).unwrap();
        let (selection, consumer_args) = if let Some(policy) = &instance_policy {
            let args = [19u64.to_le_bytes(), 37u64.to_le_bytes()].concat();
            provider_script = provider_script.as_builder().args(Bytes::from(args.clone()).pack()).build();
            let consumer_args = ConsumerArgs {
                policy_root: policy.policy().root().unwrap(),
                subject: provider_script.calc_script_hash().as_slice().try_into().unwrap(),
                config: 19u64.to_le_bytes().to_vec(),
            }
            .encode()
            .unwrap();
            (
                instance_policy::encode_selection(policy.policy(), &implementation_selection, &args).unwrap().to_vec(),
                consumer_args.to_vec(),
            )
        } else {
            (implementation_selection.to_vec(), policy.policy().root().to_vec())
        };
        let selected_instance =
            instance_policy.as_ref().map(|policy| policy.check_selection(&consumer_args, &selection, &Default::default()).unwrap());
        let consumer_script = packed::Script::new_builder()
            .code_hash(packed::CellOutput::calc_data_hash(consumer_module.bundle()[0]))
            .hash_type(4u8)
            .args(Bytes::copy_from_slice(&consumer_args).pack())
            .build();
        let lock_script = packed::Script::new_builder()
            .code_hash(packed::CellOutput::calc_data_hash(&lock_code))
            .hash_type(4u8)
            .args(Bytes::copy_from_slice(&owner[..20]).pack())
            .build();
        let mut lock_group = ScriptGroup::from_lock_script(&lock_script);
        lock_group.input_indices = vec![0, 1];
        let consumer_point = packed::OutPoint::new_builder().tx_hash([0x83u8; 32].pack()).build();
        let lock_point = packed::OutPoint::new_builder().tx_hash([0x84u8; 32].pack()).build();
        let secp_point = packed::OutPoint::new_builder().tx_hash([0x85u8; 32].pack()).build();
        let consumer_code = packed::CellOutput::new_builder().capacity(200_000_000_000u64).lock(lock_script.clone()).build();
        let extra_deps = [consumer_point.clone(), lock_point.clone(), secp_point.clone()]
            .map(|point| packed::CellDep::new_builder().out_point(point).build());
        let input_points = [
            packed::OutPoint::new_builder().tx_hash([81u8; 32].pack()).build(),
            packed::OutPoint::new_builder().tx_hash([82u8; 32].pack()).build(),
        ];
        let input_outputs = [consumer_script.clone(), provider_script.clone()].map(|script| {
            packed::CellOutput::new_builder().capacity(200_000_000_000u64).lock(lock_script.clone()).type_(Some(script).pack()).build()
        });
        let input_data = [Vec::new(), 1u64.to_le_bytes().to_vec()];
        let payload = [b"CSARGv1\0".as_slice(), selection.as_slice()].concat();
        let provider_payload = encode_policy_witness_bundle(&[PolicyWitnessRecord {
            role: PolicyScriptRole::Type,
            script_hash: provider_script.calc_script_hash().as_slice().try_into().unwrap(),
            tag: 40,
            args: [b"CSARGv1\0".as_slice(), &7u64.to_le_bytes()].concat(),
        }])
        .unwrap();
        let witnesses = [payload.clone(), provider_payload]
            .map(|bytes| packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(bytes)).pack()).build());
        let raw = packed::RawTransaction::from_slice(&raw).unwrap();
        let raw_deps = raw.cell_deps().into_iter().chain(extra_deps).collect::<Vec<_>>();
        let raw = raw
            .as_builder()
            .cell_deps(raw_deps.pack())
            .inputs(
                input_points
                    .iter()
                    .map(|point| packed::CellInput::new_builder().previous_output(point.clone()).build())
                    .collect::<Vec<_>>()
                    .pack(),
            )
            .build();
        let tx = packed::Transaction::new_builder()
            .raw(raw)
            .witnesses(witnesses.iter().map(|w| w.as_bytes().pack()).collect::<Vec<_>>().pack())
            .build();
        let inputs = [0, 1].map(|index| SuppliedInputCell {
            out_point: input_points[index].as_slice().try_into().unwrap(),
            output: input_outputs[index].as_slice(),
            data: &input_data[index],
        });
        let deps = [
            SuppliedDependencyCell { out_point: point, output: &code_output, data: &code_data },
            SuppliedDependencyCell {
                out_point: consumer_point.as_slice().try_into().unwrap(),
                output: consumer_code.as_slice(),
                data: consumer_module.bundle()[0],
            },
            SuppliedDependencyCell {
                out_point: lock_point.as_slice().try_into().unwrap(),
                output: consumer_code.as_slice(),
                data: &lock_code,
            },
            SuppliedDependencyCell {
                out_point: secp_point.as_slice().try_into().unwrap(),
                output: consumer_code.as_slice(),
                data: &secp_data,
            },
        ];
        let network = serde_json::json!({"chain_id": "test-chain", "genesis_hash": format!("0x{}", "11".repeat(32))});
        let selected_dep = serde_json::json!({"out_point": {"tx_hash": hex(&point[..32]), "index": u32::from_le_bytes(point[32..].try_into().unwrap())}, "dep_type": "code"});
        let root = directory.path();
        let mut artifacts = Vec::new();
        for (id, action, module, script, dep) in [
            (
                "consumer",
                "verify",
                &consumer_module,
                &consumer_script,
                serde_json::json!({"out_point": {"tx_hash": format!("0x{}", "83".repeat(32)), "index": 0}, "dep_type": "code"}),
            ),
            ("provider", "burn", chosen.candidate().module(), &provider_script, selected_dep.clone()),
        ] {
            let files =
                [format!("{id}.elf"), format!("{id}.meta.json"), format!("{id}.lowering.json"), format!("{id}.sourcemap.json")];
            for (name, bytes) in files.iter().zip(module.bundle()) {
                std::fs::write(root.join(name), bytes).unwrap();
            }
            let generated = common::cellc_command()
                .args(["gen-builder", "--metadata"])
                .arg(root.join(&files[1]))
                .args(["--target", "typescript", "--action", action, "--output"])
                .arg(root.join(format!("{id}-builder")))
                .arg("--json")
                .output()
                .unwrap();
            assert!(generated.status.success(), "{}", String::from_utf8_lossy(&generated.stderr));
            artifacts.push(serde_json::json!({"id": id, "package_coordinate": id, "lock_node_id": id, "entry": {"kind": "action", "name": action}, "script_role": "type", "files": {"artifact": files[0], "metadata": files[1], "lowering_record": files[2], "source_map": files[3], "builder_manifest": format!("{id}-builder/cellscript-builder-manifest.json")}, "deployment": {"network": network, "artifact_hash": cellscript_artifact_checker::hex_encode(&cellscript_artifact_checker::ckb_blake2b256(module.bundle()[0])), "script": script_json(script), "code_cell_dep": dep}}));
        }
        let extra_deps_json = [consumer_point.clone(), lock_point.clone(), secp_point.clone()]
            .map(|p| serde_json::json!({"out_point": {"tx_hash": hex(p.tx_hash().as_slice()), "index": 0}, "dep_type": "code"}));
        let skeleton = serde_json::json!({"version": 0, "inputs": ([0, 1].map(|index| serde_json::json!({"cell_commitment": hex(&[index as u8 + 1; 32]), "capacity": 200_000_000_000u64, "lock": script_json(&lock_script), "type": script_json(if index == 0 {&consumer_script} else {&provider_script}), "data": hex(&input_data[index]), "out_point": {"tx_hash": hex(input_points[index].tx_hash().as_slice()), "index": 0}, "since": 0}))), "outputs": [], "witnesses": witnesses.iter().map(|w| {let bytes = w.input_type().to_opt().unwrap().raw_data(); serde_json::json!({"input_type": hex(&cellscript_artifact_checker::ckb_blake2b256(&bytes)), "input_type_bytes": hex(&bytes)})}).collect::<Vec<_>>(), "cell_deps": [selected_dep, extra_deps_json[0], extra_deps_json[1], extra_deps_json[2]], "header_deps": [], "fee_policy_hash": hex(&[8; 32]), "change_policy_hash": hex(&[9; 32])});
        check_protocol_transaction(tx.as_slice(), &inputs, &skeleton, &Default::default()).unwrap();
        let input: ProtocolBundleInput = serde_json::from_value(serde_json::json!({"schema": "cellscript-protocol-bundle-input-v1", "network": network, "artifacts": artifacts, "transaction": skeleton, "roles": [
            {"artifact": "consumer", "name": "consumer-input", "location": "input", "index": 0, "ownership": "exclusive", "expected_type": script_json(&consumer_script)},
            {"artifact": "provider", "name": "provider-input", "location": "input", "index": 1, "ownership": "exclusive", "expected_type": script_json(&provider_script)}
        ]})).unwrap();
        let bindings = [ProtocolOpenHandleBinding {
            consumer: "consumer",
            parameter: "handle",
            consumer_module: &consumer_module,
            policy,
            selection: &selection,
            participant: "provider",
        }];
        let instance_bindings = instance_policy.as_ref().map(|policy| {
            [ProtocolInstanceHandleBinding {
                consumer: "consumer",
                parameter: "handle",
                consumer_module: &consumer_module,
                policy,
                selection: &selection,
                participant: "provider",
            }]
        });
        let admit = |input: &ProtocolBundleInput, full_transaction: &[u8], missing: bool| {
            if let Some(bindings) = &instance_bindings {
                check_protocol_bundle_with_instance_handles(
                    input,
                    root,
                    &ProtocolInstanceTransaction {
                        full_transaction,
                        inputs: &inputs,
                        dependencies: &deps,
                        bindings: if missing { &[] } else { bindings },
                        open_bindings: &[],
                    },
                )
            } else {
                check_protocol_bundle_with_open_handles(
                    input,
                    root,
                    &ProtocolOpenTransaction {
                        full_transaction,
                        inputs: &inputs,
                        dependencies: &deps,
                        bindings: if missing { &[] } else { &bindings },
                    },
                )
            }
        };
        let report = admit(&input, tx.as_slice(), false).unwrap();
        assert_eq!(report.bundle.open_handles.len(), 1);
        // The CLI receives raw sources/transactions and recomputes private
        // proofs. Exported proof IDs or a producer's verified flag cannot enter.
        let module_request = |package: &str, kind: &str, entry: &str| {
            serde_json::json!({
                "package": package, "environment": "dev", "entry_kind": kind, "entry": entry,
            })
        };
        let mut candidate_requests = Vec::new();
        for (index, candidate) in policy.sources().catalog().candidates().iter().enumerate() {
            let file = format!("creation-{index}.molecule");
            std::fs::write(root.join(&file), candidate.raw_transaction()).unwrap();
            candidate_requests.push(serde_json::json!({
                "module": module_request(if index == 0 { "library" } else { "candidate" }, "artifact", "code-policy"),
                "creation_transaction_file": file, "output_index": candidate.origin().output_index(),
                "selected_script": hex(candidate.selected_script()),
            }));
        }
        std::fs::write(root.join("transaction.molecule"), tx.as_slice()).unwrap();
        std::fs::write(root.join("bundle-input.json"), serde_json::to_vec(&input).unwrap()).unwrap();
        let mut open_request = serde_json::json!({
            "schema": if instance { "cellscript-protocol-instance-admission-input-v2" } else { "cellscript-protocol-open-admission-input-v2" }, "transaction_file": "transaction.molecule",
            "inputs": inputs.iter().map(|cell| serde_json::json!({"out_point": hex(&cell.out_point), "output": hex(cell.output), "data": hex(cell.data)})).collect::<Vec<_>>(),
            "dependencies": deps.iter().map(|cell| serde_json::json!({"out_point": hex(&cell.out_point), "output": hex(cell.output), "data": hex(cell.data)})).collect::<Vec<_>>(),
            "bindings": [{
                "consumer": "consumer", "parameter": "handle", "participant": "provider",
                "consumer_module": module_request("consumer", "action", "verify"),
                "required": module_request("library", "artifact", "code-policy"), "candidates": candidate_requests,
                "policy_header": hex(&policy.policy().header().encode().unwrap()),
                "policy_members": policy.policy().members().iter().map(|member| hex(&member.encode().unwrap())).collect::<Vec<_>>(),
                "selection": hex(&selection),
            }],
        });
        if let Some(policy) = &instance_policy {
            open_request["bindings"][0]["instance_policy"] = serde_json::json!(hex(&policy.policy().encode().unwrap()));
        }
        std::fs::write(root.join("open-input.json"), serde_json::to_vec(&open_request).unwrap()).unwrap();
        let cli = common::cellc_command()
            .current_dir(root)
            .args([
                "protocol",
                "bundle",
                "check",
                "bundle-input.json",
                "--open-admission",
                "open-input.json",
                "--output",
                "open-cli-report.json",
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            cli.status.success(),
            "open admission CLI {opt}: {} {}",
            String::from_utf8_lossy(&cli.stdout),
            String::from_utf8_lossy(&cli.stderr)
        );
        let cli_report: Value = serde_json::from_slice(&std::fs::read(root.join("open-cli-report.json")).unwrap()).unwrap();
        assert_eq!(cli_report, serde_json::to_value(&report).unwrap(), "CLI/native private admission parity {opt}");
        if instance {
            let mut downgraded = open_request.clone();
            downgraded["schema"] = serde_json::json!("cellscript-protocol-open-admission-input-v2");
            std::fs::write(root.join("downgraded-input.json"), serde_json::to_vec(&downgraded).unwrap()).unwrap();
            assert!(
                cellscript::protocol_bundle::check_protocol_bundle_with_open_handles_file(
                    &root.join("bundle-input.json"),
                    &root.join("downgraded-input.json")
                )
                .is_err(),
                "old request schema cannot carry instance policy semantics"
            );
        }
        for pointer in
            ["/schema", "/bindings/0/policy_header", "/bindings/0/candidates/0/creation_transaction_file", "/inputs/0/out_point"]
        {
            let mut bad_request = open_request.clone();
            *bad_request.pointer_mut(pointer).unwrap() = serde_json::json!("invalid");
            std::fs::write(root.join("bad-open-input.json"), serde_json::to_vec(&bad_request).unwrap()).unwrap();
            assert!(
                cellscript::protocol_bundle::check_protocol_bundle_with_open_handles_file(
                    &root.join("bundle-input.json"),
                    &root.join("bad-open-input.json")
                )
                .is_err(),
                "malformed open input {pointer}"
            );
        }
        std::fs::write(root.join("open-report.json"), serde_json::to_vec(&report).unwrap()).unwrap();
        let smoke = root.join("consumer-builder/open-witness-smoke.mjs");
        let smoke_source = r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {bindProtocolBundleArtifact, encodeOpenHandleWitness, openHandleWitnessFromCheckedBundle, planVerify} from './src/index.ts';
const checked = JSON.parse(readFileSync(new URL('../open-report.json', import.meta.url), 'utf8'));
const consumer = checked.bundle.artifacts.find(value => value.id === 'consumer');
const d = consumer.deployment;
const artifact = bindProtocolBundleArtifact({id: consumer.id, packageCoordinate: consumer.package_coordinate, lockNodeId: consumer.lock_node_id,
  entry: consumer.entry, scriptRole: consumer.script_role, deployment: {network: {chainId:d.network.chain_id,genesisHash:d.network.genesis_hash},artifactHash:d.artifact_hash,
    script:{codeHash:d.script.code_hash,hashType:d.script.hash_type,args:d.script.args},codeCellDep:{outPoint:{txHash:d.code_cell_dep.out_point.tx_hash,index:d.code_cell_dep.out_point.index},depType:d.code_cell_dep.dep_type}}});
const admitted = openHandleWitnessFromCheckedBundle(checked, artifact, 'handle');
const expected = checked.bundle.transaction.witnesses[0].input_type_bytes;
assert.equal('0x'+Buffer.from(admitted.witnessPayload).toString('hex'), expected);
assert.equal('0x'+Buffer.from(encodeOpenHandleWitness(admitted.selection)).toString('hex'), expected);
assert.equal(planVerify({handle:admitted.selection}).params.handle, admitted.selection);
const width = (admitted.selection.length - 2) / 2;
for (const bytes of [new Uint8Array(width-1), new Uint8Array(width+1), new Uint8Array(width), '0x00']) assert.throws(() => encodeOpenHandleWitness(bytes));
const changed = structuredClone(checked);
changed.bundle.open_handles[0].authorization_root = '0x'+'00'.repeat(32);
assert.throws(() => openHandleWitnessFromCheckedBundle(changed, artifact, 'handle'));
assert.throws(() => openHandleWitnessFromCheckedBundle(checked, artifact, 'other'));
const duplicate = structuredClone(checked);
duplicate.bundle.open_handles.push(duplicate.bundle.open_handles[0]);
assert.throws(() => openHandleWitnessFromCheckedBundle(duplicate, artifact, 'handle'));
"#;
        let mut smoke_source = if instance {
            smoke_source
                .replace("encodeOpenHandleWitness", "encodeInstanceHandleWitness")
                .replace("openHandleWitnessFromCheckedBundle", "instanceHandleWitnessFromCheckedBundle")
        } else {
            smoke_source.to_string()
        };
        if instance {
            smoke_source.push_str(
                r#"
import {encodeOpenHandleWitness, openHandleWitnessFromCheckedBundle} from './src/index.ts';
assert.throws(() => encodeOpenHandleWitness(admitted.selection));
assert.throws(() => openHandleWitnessFromCheckedBundle(checked, artifact, 'handle'));
for (const field of ['schema', 'policy_root', 'consumer_args', 'args_schema', 'instance_mode']) {
  const changed = structuredClone(checked);
  changed.bundle.open_handles[0].instance[field] = 'substituted';
  assert.throws(() => instanceHandleWitnessFromCheckedBundle(changed, artifact, 'handle'), field);
}
const missingInstance = structuredClone(checked);
delete missingInstance.bundle.open_handles[0].instance;
assert.throws(() => instanceHandleWitnessFromCheckedBundle(missingInstance, artifact, 'handle'));
"#,
            );
        }
        std::fs::write(&smoke, smoke_source).unwrap();
        let node = std::process::Command::new("node").arg("--experimental-strip-types").arg(&smoke).output().unwrap();
        assert!(node.status.success(), "generated open witness {opt}: {}", String::from_utf8_lossy(&node.stderr));

        let (materialized, evidence) =
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&report).unwrap()).unwrap();
        assert_eq!(materialized.data().as_slice(), tx.as_slice(), "{opt}: existing SDK serializer must reproduce all admitted bytes");
        assert_eq!(evidence.serialized_transaction_hash, hex(&cellscript_artifact_checker::ckb_blake2b256(tx.as_slice())));
        let mut context = ckb_testtool::context::Context::default();
        for cell in &deps {
            context.create_cell_with_out_point(
                packed::OutPoint::from_slice(&cell.out_point).unwrap(),
                packed::CellOutput::from_slice(cell.output).unwrap(),
                Bytes::copy_from_slice(cell.data),
            );
        }
        for cell in &inputs {
            context.create_cell_with_out_point(
                packed::OutPoint::from_slice(&cell.out_point).unwrap(),
                packed::CellOutput::from_slice(cell.output).unwrap(),
                Bytes::copy_from_slice(cell.data),
            );
        }
        // Local observations exercise the adapter's byte-binding state machine;
        // they are not observations from a live network or consensus inclusion.
        let live_cells = inputs
            .iter()
            .map(|cell| (packed::CellOutput::from_slice(cell.output).unwrap(), Bytes::copy_from_slice(cell.data)))
            .collect::<Vec<_>>();
        let live = cellscript_ckb_adapter::protocol_bundle_live_resolution_evidence(
            &materialized,
            &evidence,
            "test-chain",
            &format!("0x{}", "11".repeat(32)),
            &live_cells,
        )
        .unwrap();
        let observations = deps[..2]
            .iter()
            .enumerate()
            .map(|(index, cell)| cellscript_ckb_adapter::ProtocolBundleCellDepObservation {
                transaction_cell_dep_index: index as u32,
                root: cellscript_ckb_adapter::ProtocolBundleLiveCellObservation {
                    out_point: packed::OutPoint::from_slice(&cell.out_point).unwrap(),
                    output: packed::CellOutput::from_slice(cell.output).unwrap(),
                    data: Bytes::copy_from_slice(cell.data),
                },
                dep_group_members: vec![],
            })
            .collect::<Vec<_>>();
        let resolved =
            cellscript_ckb_adapter::protocol_bundle_dependency_resolution_evidence(&materialized, &evidence, &live, &observations)
                .unwrap();
        let ready =
            cellscript_ckb_adapter::protocol_bundle_ready_to_sign_evidence(&materialized, &evidence, &live, &resolved).unwrap();
        let provider = OffchainTransactionDependencyProvider {
            tx_tip_num_map: Default::default(),
            txs: Default::default(),
            cells: input_points
                .iter()
                .zip(&live_cells)
                .map(|(point, cell)| ((point.tx_hash().unpack(), point.index().unpack()), cell.clone()))
                .collect(),
        };
        let unlockers = std::collections::HashMap::from([(
            ScriptId::new(lock_script.code_hash().unpack(), ckb_testtool::ckb_types::core::ScriptHashType::Data2),
            Box::new(SecpSighashUnlocker::new(signer())) as Box<dyn ScriptUnlocker>,
        )]);
        let (signed, signing) =
            cellscript_ckb_adapter::unlock_protocol_bundle_transaction(&materialized, &ready, &provider, &unlockers).unwrap();
        assert_eq!(signing.changed_lock_witness_indexes, vec![0]);
        assert!(signing.entry_witness_fields_preserved && signing.all_lock_groups_processed);
        assert!(context.verify_tx(&materialized, 3_000_000).is_err(), "unsigned transaction must not pass its real Lock group");
        let cycles = context.verify_tx(&signed, 3_000_000).unwrap();
        eprintln!("open admission signed all-group {opt}: cycles={cycles} tx={}", signing.signed_serialized_transaction_hash);
        // Even a field unused by the Type scripts is covered by the signature.
        let signed_witness = packed::WitnessArgs::from_slice(signed.witnesses().get(0).unwrap().raw_data().as_ref()).unwrap();
        let changed_witness = signed_witness.as_builder().output_type(Some(Bytes::from_static(b"after-signing")).pack()).build();
        let mut changed_witnesses = signed.witnesses().into_iter().collect::<Vec<_>>();
        changed_witnesses[0] = changed_witness.as_bytes().pack();
        let substituted = signed.as_advanced_builder().set_witnesses(changed_witnesses).build();
        let rejected = context.verify_tx(&substituted, 3_000_000).unwrap_err();
        assert!(
            format!("{rejected:?}").contains("Inputs[0].Lock"),
            "post-signing substitution must fail the actual Lock group: {rejected:?}"
        );
        assert!(
            cellscript_ckb_adapter::unlock_protocol_bundle_transaction(&substituted, &ready, &provider, &unlockers).is_err(),
            "old signing preparation cannot bind changed transaction bytes"
        );

        assert_eq!(
            report.bundle.open_handles[0].selected_receipt,
            selected_instance
                .as_ref()
                .map_or_else(|| chosen.candidate().source_receipt().identity(), |selection| selection.source_receipt().identity())
        );
        assert!(check_protocol_bundle(&input, root).is_err(), "a serialized report cannot manufacture a private receipt");
        let mut foreign_owner = input.clone();
        foreign_owner.roles[1].artifact = "consumer".into();
        assert!(admit(&foreign_owner, tx.as_slice(), false).is_err());
        let mut rebound_owner = serde_json::to_value(&report).unwrap();
        rebound_owner["bundle"]["roles"][1]["artifact"] = serde_json::json!("consumer");
        rebound_owner["bundle_hash"] =
            serde_json::json!(
                cellscript_artifact_checker::canonical_hash("cellscript-protocol-bundle-v1", &rebound_owner["bundle"]).unwrap()
            );
        assert!(cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rebound_owner).unwrap()).is_err());
        let mut observation_input = input.clone();
        let mut observation_role = observation_input.roles[1].clone();
        observation_role.artifact = "consumer".into();
        observation_role.name = "observed-peer".into();
        observation_role.ownership = cellscript::protocol_bundle::ProtocolRoleOwnership::SharedRead;
        observation_input.roles.push(observation_role);
        let observation_report = admit(&observation_input, tx.as_slice(), false).unwrap();
        assert_eq!(observation_report.status, "ok");
        let (observed_tx, _) =
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&observation_report).unwrap()).unwrap();
        assert_eq!(observed_tx.data().as_slice(), tx.as_slice(), "foreign read-only claims preserve the exact transaction");
        // A semantically rejected peer request still has a valid source receipt,
        // canonical codec and selection. Complete VM execution must reject it.
        let rejected_payload = encode_policy_witness_bundle(&[PolicyWitnessRecord {
            role: PolicyScriptRole::Type,
            script_hash: provider_script.calc_script_hash().as_slice().try_into().unwrap(),
            tag: 40,
            args: [b"CSARGv1\0".as_slice(), &6u64.to_le_bytes()].concat(),
        }])
        .unwrap();
        let rejected_witness =
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(rejected_payload.clone())).pack()).build();
        let rejected_tx =
            tx.clone().as_builder().witnesses(vec![witnesses[0].as_bytes().pack(), rejected_witness.as_bytes().pack()].pack()).build();
        let mut rejected_input = input.clone();
        rejected_input.transaction.witnesses[1].input_type =
            Some(hex(&cellscript_artifact_checker::ckb_blake2b256(&rejected_payload)));
        rejected_input.transaction.witnesses[1].input_type_bytes = Some(hex(&rejected_payload));
        let rejected_report = admit(&rejected_input, rejected_tx.as_slice(), false).unwrap();
        let (rejected_materialized, _) =
            cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rejected_report).unwrap()).unwrap();
        let rejected_signed = signer().sign_tx(&rejected_materialized, &lock_group).unwrap();
        let rejected = context.verify_tx(&rejected_signed, 3_000_000).unwrap_err();
        assert!(format!("{rejected:?}").contains("Inputs[1].Type"), "rejection must come from the selected peer: {rejected:?}");
        let mut report_mutations = vec![
            "/transaction/inputs/0/since",
            "/open_handles/0/witness_index",
            "/open_handles/0/selected_artifact_hash",
            "/open_handles/0/participant",
            "/open_handles/0/claim_boundaries/participant_predicates_and_data",
        ];
        if instance {
            report_mutations.extend([
                "/open_handles/0/instance/schema",
                "/open_handles/0/instance/policy_root",
                "/open_handles/0/instance/consumer_args",
                "/open_handles/0/instance/args_schema",
                "/open_handles/0/instance/instance_mode",
                "/open_handles/0/instance/implementation_receipt",
                "/open_handles/0/selected_script_hash",
            ]);
        }
        for pointer in report_mutations {
            let mut rebound = serde_json::to_value(&report).unwrap();
            let value = rebound["bundle"].pointer_mut(pointer).unwrap();
            *value = if pointer.ends_with("participant_predicates_and_data") {
                serde_json::json!("checked_by_current_script")
            } else if value.is_number() {
                serde_json::json!(99)
            } else {
                serde_json::json!("substituted")
            };
            rebound["bundle_hash"] =
                serde_json::json!(
                    cellscript_artifact_checker::canonical_hash("cellscript-protocol-bundle-v1", &rebound["bundle"]).unwrap()
                );
            assert!(
                cellscript_ckb_adapter::materialize_protocol_bundle_report(&serde_json::to_vec(&rebound).unwrap()).is_err(),
                "rebound report {opt}: {pointer}"
            );
        }

        for pointer in [
            "/inputs/0/capacity",
            "/inputs/1/type/args",
            "/inputs/0/out_point/index",
            "/inputs/0/since",
            "/inputs/1/data",
            "/cell_deps/0/dep_type",
            "/witnesses/0/input_type_bytes",
            "/witnesses/1/input_type",
            "/version",
        ] {
            let mut changed = skeleton.clone();
            let value = changed.pointer_mut(pointer).unwrap();
            *value = if value.is_number() { serde_json::json!(999) } else { serde_json::json!("0xff") };
            assert!(check_protocol_transaction(tx.as_slice(), &inputs, &changed, &Default::default()).is_err(), "{opt} {pointer}");
        }
        let mut malformed_since = skeleton.clone();
        malformed_since["inputs"][0]["since"] = serde_json::json!("zero");
        assert!(check_protocol_transaction(tx.as_slice(), &inputs, &malformed_since, &Default::default()).is_err());
        for pointer in ["/inputs", "/outputs", "/cell_deps", "/header_deps", "/witnesses"] {
            let mut changed = skeleton.clone();
            changed.pointer_mut(pointer).unwrap().as_array_mut().unwrap().push(serde_json::json!({}));
            assert!(
                check_protocol_transaction(tx.as_slice(), &inputs, &changed, &Default::default()).is_err(),
                "{opt} {pointer} cardinality"
            );
        }
        let mut changed = input.clone();
        changed.artifacts[0].deployment.script.args = hex(&[0; 32]);
        assert!(admit(&changed, tx.as_slice(), false).is_err());
        assert!(admit(&input, tx.as_slice(), true).is_err());
    }
}

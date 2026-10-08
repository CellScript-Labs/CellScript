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
    assert!(
        output.status.success(),
        "lock {}: stdout={} stderr={}",
        root.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
fn compile_code(root: &Path, opt: u8) -> FrozenPackageModule {
    compile_module(
        Utf8Path::from_path(root).unwrap(),
        "dev",
        CompileOptions { opt_level: opt, ..Default::default() },
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
    for opt in 0..=3 {
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
        assert_eq!(record["schema"], "cellscript-frozen-code-catalog-v2");
        assert_eq!(record["candidate_source_contexts"], serde_json::to_value(source_ids).unwrap());
        assert_eq!(record["required_external_codec"], checked.required_codec().identity());
        assert_eq!(record["checked_modules"], checked.module_evidence().identity());
        for (index, candidate) in checked.candidates().iter().enumerate() {
            assert_eq!(record["candidate_code_origins"][index], candidate.origin().identity());
            assert_eq!(record["candidate_target_origins"][index], candidate.target_origin().identity());
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
    for opt in 0..=3 {
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
            assert!(!error.message.is_empty(), "case {case}, O{opt}");
            if matches!(case, "network" | "genesis") {
                assert!(error.message.contains("conflicting pinned chain identities"));
            }
        }
    }
}

#[test]
fn native_code_catalog_rejects_wrong_vm_hash_types_in_unselected_members() {
    use cellscript::package::frozen_interface::freeze_code_catalog;
    use cellscript_artifact_checker::{code_origin::check_code_cell_origin, CheckerBudgets};
    use ckb_testtool::ckb_types::{packed, prelude::*};
    let directory = tempfile::tempdir().unwrap();
    code_package(directory.path(), CODE_SOURCE);
    let root = directory.path();
    for opt in 0..=3 {
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
            assert!(error.message.contains("selected Script hash type differs"), "O{opt}, hash_type={hash_type}: {error:?}");
        }
    }
    let mut inputs = (0..32).map(|index| code_candidate(compile_code(root, 0), vec![index])).collect::<Vec<_>>();
    inputs[31].selected_script =
        packed::Script::from_slice(&inputs[31].selected_script).unwrap().as_builder().hash_type(2u8).build().as_slice().to_vec();
    assert!(freeze_code_catalog(compile_code(root, 0), inputs, &CheckerBudgets::default())
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
    assert!(freeze_code_catalog(compile_code(root, 0), vec![], &CheckerBudgets::default()).unwrap_err().message.contains("1..=32"));
    let too_many = (0..33).map(|index| code_candidate(compile_code(root, 0), vec![index])).collect();
    assert!(freeze_code_catalog(compile_code(root, 0), too_many, &CheckerBudgets::default()).unwrap_err().message.contains("1..=32"));
    for which in ["artifact", "record", "source-map", "large-raw", "large-script", "shared"] {
        let mut budgets = CheckerBudgets::default();
        let mut inputs = vec![code_candidate(compile_code(root, 0), vec![1])];
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
                    let mut input = code_candidate(compile_code(root, 0), vec![index]);
                    input.raw_transaction = vec![0; 4 * 1024 * 1024];
                    inputs.push(input);
                }
            }
            _ => unreachable!(),
        }
        let error = freeze_code_catalog(compile_code(root, 0), inputs, &budgets).unwrap_err();
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
    let inputs = (0..32).map(|index| code_candidate(compile_code(root, 0), vec![index])).collect();
    let checked = freeze_code_catalog(compile_code(root, 0), inputs, &CheckerBudgets::default()).unwrap();
    assert_eq!(checked.candidates().len(), 32);
    let first = checked.candidates()[0].origin();
    let second = checked.candidates()[1].origin();
    assert_eq!(first.transaction_hash(), second.transaction_hash());
    assert_eq!(first.output_index(), second.output_index());
    assert_ne!(first.selected_script_hash(), second.selected_script_hash());
    assert_ne!(first.identity(), second.identity());
    let mut inputs = (0..32).map(|index| code_candidate(compile_code(root, 0), vec![index])).collect::<Vec<_>>();
    inputs[31].output_index = 1;
    assert!(freeze_code_catalog(compile_code(root, 0), inputs, &CheckerBudgets::default())
        .unwrap_err()
        .message
        .contains("code output index is absent"));
    let duplicate = vec![code_candidate(compile_code(root, 0), vec![1]), code_candidate(compile_code(root, 0), vec![1])];
    assert!(freeze_code_catalog(compile_code(root, 0), duplicate, &CheckerBudgets::default())
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
fn one_code_catalog(root: &Path, opt: u8) -> cellscript::package::frozen_interface::FrozenCodeCatalog {
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
    for opt in 0..=3 {
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
    for opt in 0..=3 {
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
        let baseline = one_code_catalog(&library, 0);
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
        let consumer = compile(&app, 0);
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
    let baseline = one_code_catalog(&library, 0);
    let checked = resolve_code_catalog_source(compile(&first, 0), one_code_catalog(&library, 0)).unwrap();
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
    let consumer = compile(&second, 0);
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
    let consumer = compile(&absent, 0);
    let consumer_bytes = consumer.bundle().iter().map(|bytes| bytes.len()).sum::<usize>();
    let required = compile_code(&library, 0);
    let mut inputs = (0..4).map(|index| padded_source_owner_candidate(compile_code(&library, 0), vec![index], 0)).collect::<Vec<_>>();
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
        let replacement = padded_source_owner_candidate(compile_code(&library, 0), vec![index as u8], size);
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
    for opt in 0..=3 {
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
    let compiled = resolve_code_catalog_source(compile(&consumer, 0), one_code_catalog(&library, 0)).unwrap();
    let root = Utf8Path::from_path(&consumer).unwrap();
    let parsed = resolve_source_catalog(freeze_package_sources(root, "dev").unwrap(), one_code_catalog(&library, 0)).unwrap();
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
    let untyped = resolve_source_catalog(freeze_package_sources(root, "dev").unwrap(), one_code_catalog(&library, 0)).unwrap();
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
    let catalog = one_code_catalog(&library, 0);
    std::fs::write(consumer.join("src/main.cell"), "module replaced\npublic action verify() { require true }\n").unwrap();
    lock(&consumer);
    let error = resolve_source_catalog(sources, catalog).unwrap_err();
    assert!(error.message.contains("frozen source closure changed"));
    let fresh = freeze_package_sources(root, "dev").unwrap();
    assert!(resolve_source_catalog(fresh, one_code_catalog(&library, 0)).is_ok());
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

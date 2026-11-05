// Compiler integration packages behavior; retains crate::tests identities.

    #[test]
    fn incremental_cache_separates_the_strategy_from_each_diagnostic_mask() {
        let mut identities = std::collections::BTreeSet::new();
        let passes = [
            crate::OptimizationPass::ConstantFolding,
            crate::OptimizationPass::Inlining,
            crate::OptimizationPass::DeadCode,
            crate::OptimizationPass::ExactCellEquality,
            crate::OptimizationPass::SourceByteEquality,
            crate::OptimizationPass::ExactReadHotCache,
            crate::OptimizationPass::StackReloads,
            crate::OptimizationPass::BranchFusion,
        ];
        for diagnostics in [crate::CompilerDiagnostics::default(), crate::CompilerDiagnostics::without_optimizations()]
            .into_iter()
            .chain(passes.map(|pass| crate::CompilerDiagnostics::default().without(pass)))
        {
            let options = CompileOptions { diagnostics, ..Default::default() };
            assert!(identities.insert(crate::incremental_cache_key(&[], &options)), "reused cache identity for {diagnostics}");
        }
    }

    #[test]
    fn compile_result_validation_rejects_metadata_source_hash_mismatch() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.source_hash = Some("00".repeat(32));

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("metadata source_hash"), "unexpected error: {}", err.message);
    }

    #[test]
    fn compile_file_source_content_hash_is_path_independent() {
        let left = tempdir().unwrap();
        let right = tempdir().unwrap();
        let left_root = Utf8Path::from_path(left.path()).unwrap();
        let right_root = Utf8Path::from_path(right.path()).unwrap();
        let left_source = left_root.join("left.cell");
        let right_source = right_root.join("right.cell");
        std::fs::write(&left_source, SIMPLE_PROGRAM).unwrap();
        std::fs::write(&right_source, SIMPLE_PROGRAM).unwrap();

        let left_result = compile_file(&left_source, CompileOptions::default()).unwrap();
        let right_result = compile_file(&right_source, CompileOptions::default()).unwrap();

        assert_ne!(
            left_result.metadata.source_hash, right_result.metadata.source_hash,
            "path-bound source set hash must change when the same source lives at a different path"
        );
        assert_eq!(
            left_result.metadata.source_content_hash, right_result.metadata.source_content_hash,
            "path-independent source content hash must stay stable across equivalent source locations"
        );
    }

    #[test]
    fn incremental_cache_key_partitions_primitive_and_debug_modes() {
        let release_elf = CompileOptions {
            diagnostics: crate::CompilerDiagnostics::default(),
            target: Some("riscv64-elf".to_string()),
            target_profile: Some("ckb".to_string()),
            ..CompileOptions::default()
        };
        let strict_release_elf = CompileOptions { primitive_compat: Some("0.16".to_string()), ..release_elf.clone() };
        let debug_release_elf = CompileOptions { debug: true, ..release_elf.clone() };

        let cache_units = vec![source_unit_from_bytes("<memory>", "entry", SIMPLE_PROGRAM.as_bytes())];
        let release_key = incremental_cache_key(&cache_units, &release_elf);

        assert_ne!(
            release_key,
            incremental_cache_key(&cache_units, &strict_release_elf),
            "primitive strict metadata must not reuse a non-strict cache entry"
        );
        assert_ne!(
            release_key,
            incremental_cache_key(&cache_units, &debug_release_elf),
            "debug artefacts must not reuse a non-debug cache entry"
        );
    }

    #[test]
    fn incremental_cache_pruning_is_bounded_and_ignores_unknown_entries() {
        let temp = tempfile::tempdir().unwrap();
        let cache = Utf8Path::from_path(temp.path()).unwrap().join(".cell/build/cache");
        std::fs::create_dir_all(&cache).unwrap();
        let keys = (0..5).map(|index| format!("{index:064x}")).collect::<Vec<_>>();
        for key in &keys {
            std::fs::create_dir(cache.join(key)).unwrap();
        }
        std::fs::create_dir(cache.join("operator-notes")).unwrap();

        prune_incremental_cache_entries(&cache, &keys[4], 2);

        let retained = keys.iter().filter(|key| cache.join(key).is_dir()).count();
        assert_eq!(retained, 2);
        assert!(cache.join(&keys[4]).is_dir());
        assert!(cache.join("operator-notes").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn incremental_cache_pruning_rejects_symlinked_managed_components() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let package = Utf8Path::from_path(temp.path()).unwrap().join("package");
        let outside = Utf8Path::from_path(temp.path()).unwrap().join("outside");
        std::fs::create_dir_all(package.join(".cell")).unwrap();
        std::fs::create_dir_all(outside.join("cache")).unwrap();
        let victim = outside.join("cache").join(format!("{:064x}", 1));
        std::fs::create_dir(&victim).unwrap();
        symlink(&outside, package.join(".cell/build")).unwrap();

        let cache = package.join(".cell/build/cache");
        assert!(!super::incremental_cache_components_are_safe(&cache));
        prune_incremental_cache_entries(&cache, &format!("{:064x}", 2), 1);
        assert!(victim.is_dir(), "cache retention must not cross a symlinked build directory");
    }

    #[cfg(unix)]
    #[test]
    fn incremental_cache_hit_does_not_follow_recency_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let source = root.join("main.cell");
        std::fs::write(&source, SIMPLE_PROGRAM).unwrap();

        let first = compile_file(&source, CompileOptions::default()).unwrap();
        assert!(!first.cache_hit);
        let cache = root.join(".cell/build/cache");
        let entry = std::fs::read_dir(&cache).unwrap().next().unwrap().unwrap().path();
        let recency = entry.join(".last_used");
        std::fs::remove_file(&recency).unwrap();
        let victim = root.join("outside.txt");
        std::fs::write(&victim, b"must survive").unwrap();
        symlink(&victim, &recency).unwrap();

        let second = compile_file(&source, CompileOptions::default()).unwrap();
        assert!(second.cache_hit);
        assert_eq!(std::fs::read(&victim).unwrap(), b"must survive");
        assert!(std::fs::symlink_metadata(&recency).unwrap().is_file());
    }

    #[cfg(unix)]
    #[test]
    fn incremental_cache_rejects_symlinked_payload_without_overwriting_target() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let source = root.join("main.cell");
        std::fs::write(&source, SIMPLE_PROGRAM).unwrap();

        let first = compile_file(&source, CompileOptions::default()).unwrap();
        assert!(!first.cache_hit);
        let cache = root.join(".cell/build/cache");
        let entry = std::fs::read_dir(&cache).unwrap().next().unwrap().unwrap().path();
        let artifact = entry.join("artifact");
        std::fs::remove_file(&artifact).unwrap();
        let victim = root.join("outside.txt");
        std::fs::write(&victim, b"must survive").unwrap();
        symlink(&victim, &artifact).unwrap();

        let second = compile_file(&source, CompileOptions::default()).unwrap();
        assert!(!second.cache_hit, "a cache payload symlink must invalidate the entry");
        assert_eq!(std::fs::read(&victim).unwrap(), b"must survive");
    }

    #[test]
    fn compile_result_validation_rejects_noncanonical_source_unit_hash() {
        let mut result = compile(SIMPLE_PROGRAM, CompileOptions::default()).unwrap();
        result.metadata.source_units[0].hash = result.metadata.source_units[0].hash.to_uppercase();

        let err = result.validate().unwrap_err();

        assert!(err.message.contains("expected 64 lowercase hex characters"), "unexpected error: {}", err.message);
    }

    #[test]
    fn load_modules_for_input_collects_package_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
source_roots = ["src", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/main.cell"), "module demo::main\naction ping() -> u64 {\n    verification\n        1\n}\n")
            .unwrap();
        std::fs::write(root.join("shared/types.cell"), "module demo::types\nstruct Pair { left: u64, right: u64 }\n").unwrap();

        let modules = load_modules_for_input(root).unwrap();
        assert_eq!(modules.len(), 2);
    }

    #[test]
    fn compile_file_loads_local_path_dependencies_from_cell_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("dep_pkg");
        let app_root = root.join("app_pkg");

        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();

        std::fs::write(
            dep_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("token.cell"),
            r#"
module dep::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
        )
        .unwrap();

        std::fs::write(
            app_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
        )
        .unwrap();
        let app_entry = app_root.join("src").join("main.cell");
        std::fs::write(
            &app_entry,
            r#"
module app::main

use dep::token::Token

action pass_through(token: Token) -> Token {
    verification
        token
}
"#,
        )
        .unwrap();

        lock_package_for_test(&app_root).unwrap();
        let result = compile_file(&app_entry, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
        assert!(result.metadata.source_hash.is_some());
        let roles = result.metadata.source_units.iter().map(|unit| unit.role.as_str()).collect::<Vec<_>>();
        assert!(roles.contains(&"entry"), "missing entry source unit: {:?}", result.metadata.source_units);
        assert!(roles.contains(&"dependency"), "missing dependency source unit: {:?}", result.metadata.source_units);
        assert_eq!(result.metadata.source_units.len(), 2);
    }

    #[test]
    fn package_interoperates_with_raw_temporal_dependency() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("legacy_time");
        let app_root = root.join("typed_app");
        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();
        std::fs::write(
            dep_root.join("Cell.toml"),
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"legacy_time\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("time.cell"),
            "module legacy::time\npublic fn legacy_epoch() -> u64 { return env::current_timepoint() }\n",
        )
        .unwrap();
        std::fs::write(
            app_root.join("Cell.toml"),
            r#"[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "typed_app"
version = "0.1.0"

[dependencies]
legacy_time = { path = "../legacy_time" }
"#,
        )
        .unwrap();
        let entry = app_root.join("src").join("main.cell");
        let source = r#"module typed::app
use legacy::time::legacy_epoch
action inspect() -> u64 { return legacy_epoch() }
"#;
        std::fs::write(&entry, source).unwrap();
        lock_package_for_test(&app_root).unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.edition, CellScriptEdition::Edition2027);
        assert!(result.metadata.runtime.ckb_runtime_features.contains(&"load-header-timepoint".to_string()));
        assert!(result.metadata.source_units.iter().any(|unit| unit.path.ends_with("legacy_time/src/time.cell")));
        let diagnostics = compile_path_metadata_with_diagnostics(&entry, CompileOptions::default());
        assert!(!diagnostics.has_errors());
        assert!(diagnostics.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("W3012")
                && diagnostic.file.as_ref().is_some_and(|path| path.ends_with("legacy_time/src/time.cell"))
        }));

        let report = compile_path_metadata_with_diagnostics_for_sources(
            &entry,
            &std::collections::BTreeMap::from([(
                entry.clone(),
                "module typed::app\nuse legacy::time::legacy_epoch\naction inspect() -> EpochNumber { return legacy_epoch() }\n"
                    .into(),
            )]),
            CompileOptions::default(),
        );
        assert!(report.has_errors(), "the raw u64 signature must not be retyped at the import boundary");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("EpochNumber") && diagnostic.message.contains("U64")),
            "{:?}",
            report.diagnostics.iter().map(|diagnostic| diagnostic.message.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn compile_file_monomorphizes_imported_generic_types_and_functions_in_their_owner_module() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("generic_dep");
        let app_root = root.join("generic_app");
        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();
        std::fs::write(
            dep_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "generic_dep"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("pairs.cell"),
            r#"
module generic_dep::pairs

public struct Pair<T: fixed_value> {
    left: T,
    right: T,
}

public fn swap<T: fixed_value>(pair: Pair<T>) -> Pair<T> {
    return Pair<T> { left: pair.right, right: pair.left }
}
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "generic_app"
version = "0.1.0"

[dependencies]
generic_dep = { path = "../generic_dep" }
"#,
        )
        .unwrap();
        let entry = app_root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module generic_app::main

use generic_dep::pairs::Pair as Duo
use generic_dep::pairs::swap as flip

action verify() -> u64 {
    verification
        let pair: Duo<u64> = Duo<u64> { left: 20, right: 22 }
        let swapped: Duo<u64> = flip<u64>(pair)
        return swapped.left + swapped.right
}
"#,
        )
        .unwrap();

        lock_package_for_test(&app_root).unwrap();
        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert!(!result.artifact_bytes.is_empty());
        assert!(result
            .metadata
            .generic_instantiations
            .iter()
            .any(|item| item.module == "generic_dep::pairs" && item.template == "swap"));
        let interface = &result.metadata.public_interface;
        assert!(interface.types.iter().all(|item| !item.name.contains("__mono__")));
        assert!(interface.callables.iter().all(|item| !item.name.contains("__mono__")));

        // Imported public generic declarations satisfy the same layout boundary.
        let dep_source = dep_root.join("src/pairs.cell");
        let source = std::fs::read_to_string(&dep_source)
            .unwrap()
            .replace("Pair<T: fixed_value> {", "Pair<T: copy> has copy {")
            .replace("swap<T: fixed_value>", "swap<T: copy>");
        std::fs::write(&dep_source, source).unwrap();
        lock_package_for_test(&app_root).unwrap();
        let error = compile_file(&entry, CompileOptions::default()).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2110"));
        assert!(error.message.contains("fixed value layout boundary"), "{error}");
    }

    #[test]
    fn compile_file_does_not_instantiate_unrelated_unpacked_sibling_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let entry = root.join("entry.cell");
        std::fs::write(
            &entry,
            r#"
module audit::entry

action verify() -> u64 {
    verification
        return 42
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("unrelated_rejection.cell"),
            r#"
module audit::unrelated

resource Token has consume {
    amount: u64,
}

enum Boxed<T> {
    Empty,
    Some(T),
}

fn hide(input: Token) -> Boxed<Token> {
    Boxed::Some<Token>(input)
}
"#,
        )
        .unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert!(!result.artifact_bytes.is_empty());
        assert_eq!(result.metadata.module, "audit::entry");
    }

    #[test]
    fn compile_file_metadata_lays_out_imported_nested_fixed_structs() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "nested_layout"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("schema.cell"),
            r#"
module nested_layout::schema

struct Core {
    amount: u64,
    owner: Hash,
}

struct Signed {
    core: Core,
    digest: Hash,
    receipt: Hash,
}
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module nested_layout::main

use nested_layout::schema::{Core, Signed}

action inspect(witness signed: Signed) -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        let signed = result.metadata.types.iter().find(|ty| ty.name == "Signed").expect("Signed metadata");
        let offsets = signed.fields.iter().map(|field| (field.name.as_str(), field.offset, field.encoded_size)).collect::<Vec<_>>();
        assert_eq!(offsets, vec![("core", 0, Some(40)), ("digest", 40, Some(32)), ("receipt", 72, Some(32))]);
    }

    #[test]
    fn resolve_input_path_accepts_package_root_and_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let expected: Utf8PathBuf = std::fs::canonicalize(&entry).unwrap().try_into().unwrap();
        assert_eq!(resolve_input_path(root).unwrap(), expected);
        assert_eq!(resolve_input_path(root.join("Cell.toml")).unwrap(), expected);
    }

    #[test]
    fn compile_path_accepts_package_root() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn default_output_path_for_package_input_uses_build_dir() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let expected = super::canonical_utf8_path(root).unwrap().join("build").join("main.s");
        let resolved = resolve_input_path(root).unwrap();
        assert_eq!(default_output_path_for_input(root, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);

        let manifest = root.join("Cell.toml");
        assert_eq!(default_output_path_for_input(&manifest, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);
    }

    #[test]
    fn default_output_path_for_package_input_uses_manifest_out_dir() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
out_dir = "artifacts"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let expected = super::canonical_utf8_path(root).unwrap().join("artifacts").join("main.s");
        let resolved = resolve_input_path(root).unwrap();
        assert_eq!(default_output_path_for_input(root, &resolved, ArtifactFormat::RiscvAssembly).unwrap(), expected);
    }

    #[test]
    fn compile_file_uses_manifest_build_target_by_default() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvElf);
        assert!(result.artifact_bytes.starts_with(b"\x7fELF"));
    }

    #[test]
    fn compile_file_uses_manifest_ckb_target_profile() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target_profile = "ckb"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let result = compile_file(&entry, CompileOptions::default()).unwrap();
        assert_eq!(result.metadata.target_profile.name.as_str(), "ckb");
        assert_eq!(result.metadata.target_profile.artifact_packaging.as_str(), "ckb-asm-sidecar");
        assert!(!result.metadata.runtime.vm_abi.embedded_in_artifact);
    }

    #[test]
    fn compile_file_explicit_target_overrides_manifest_build_target() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[build]
target = "riscv64-elf"
"#,
        )
        .unwrap();
        let entry = root.join("src").join("main.cell");
        std::fs::write(
            &entry,
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let result =
            compile_file(&entry, CompileOptions { target: Some("riscv64-asm".to_string()), ..CompileOptions::default() }).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_non_path_dependencies() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[dependencies]
token_std = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let err = lock_package_for_test(root).unwrap_err();
        assert!(err.message.contains("requires a namespace"));
        assert!(err.message.contains("token_std"));
    }

    #[test]
    fn compile_path_rejects_missing_path_dependency_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"

[dependencies]
token_std = { path = "../missing_dep" }
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let err = lock_package_for_test(root).unwrap_err();
        assert!(err.message.contains("not found at path"));
        assert!(err.message.contains("token_std"));
    }

    #[test]
    fn compile_path_ignores_examples_outside_package_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("examples")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();
        std::fs::write(root.join("examples").join("broken.cell"), "this is not valid cellscript").unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_supports_custom_entry_directory_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("helper.cell"),
            r#"
module demo::helper

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

use demo::helper::Token

action pass(token: Token) -> Token {
    verification
        token
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_path_dependency_cycles() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();
        let dep_root = root.join("dep_pkg");
        let app_root = root.join("app_pkg");

        std::fs::create_dir_all(dep_root.join("src")).unwrap();
        std::fs::create_dir_all(app_root.join("src")).unwrap();

        std::fs::write(
            dep_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "dep_pkg"
version = "0.1.0"

[dependencies]
app_pkg = { path = "../app_pkg" }
"#,
        )
        .unwrap();
        std::fs::write(
            dep_root.join("src").join("main.cell"),
            r#"
module dep::main

action dep_ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        std::fs::write(
            app_root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "app_pkg"
version = "0.1.0"

[dependencies]
dep_pkg = { path = "../dep_pkg" }
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("src").join("main.cell"),
            r#"
module app::main

action app_ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let err = lock_package_for_test(&app_root).unwrap_err();
        assert!(err.message.contains("Circular dependency detected"));
    }

    #[test]
    fn compile_path_supports_configured_source_roots_without_src() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("shared").join("token.cell"),
            r#"
module demo::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

use demo::token::Token

action pass(token: Token) -> Token {
    verification
        token
}
"#,
        )
        .unwrap();

        let result = compile_path(root, CompileOptions::default()).unwrap();
        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(!result.artifact_bytes.is_empty());
    }

    #[test]
    fn compile_path_rejects_missing_import_even_with_same_named_type_loaded() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "bad-import"
version = "0.1.0"
entry = "src/main.cell"
source_roots = ["src"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("other.cell"),
            r#"
module real::other

resource Token {
    value: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module app

use missing::module::Token

action pass(token: Token) -> Token {
    verification
        token
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("module 'missing::module' imported by 'app' not found"));
    }

    #[test]
    fn compile_path_rejects_invalid_unreferenced_package_module() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "unreferenced-bad"
version = "0.1.0"
entry = "src/main.cell"
source_roots = ["src"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module app

action ok() -> bool {
    verification
        true
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("helper.cell"),
            r#"
module helper

action broken() -> u64 {
    verification
        true
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.file.as_ref().is_some_and(|path| path.file_name() == Some("helper.cell")));
        assert!(err.message.contains("expected U64, found Bool"));
    }

    #[test]
    fn incremental_cache_invalidates_when_imported_cell_changes() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "cache-drift"
version = "0.1.0"
entry = "src/main.cell"
source_roots = ["src"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("types.cell"),
            r#"
module dep::types

resource Pair {
    value: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src").join("main.cell"),
            r#"
module app

use dep::types::Pair

action keep(pair: Pair) -> Pair {
    verification
        pair
}
"#,
        )
        .unwrap();

        let first = compile_path(root, CompileOptions::default()).unwrap();
        assert!(!first.cache_hit);

        std::fs::write(
            root.join("src").join("types.cell"),
            r#"
module dep::types

resource Pair {
    value: u64,
    nonce: u64
}
"#,
        )
        .unwrap();

        let second = compile_path(root, CompileOptions::default()).unwrap();
        assert!(!second.cache_hit);
    }

    #[test]
    fn compile_path_rejects_missing_configured_source_root() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("configured source root"));
        assert!(err.message.contains("shared"));
    }

    #[test]
    fn compile_path_rejects_duplicate_modules_across_source_roots() {
        let temp = tempdir().unwrap();
        let root = Utf8Path::from_path(temp.path()).unwrap();

        std::fs::create_dir_all(root.join("contracts")).unwrap();
        std::fs::create_dir_all(root.join("shared")).unwrap();
        std::fs::write(
            root.join("Cell.toml"),
            r#"
[package]
cellscript_version = ">=0.33.0-alpha.1"
edition = "2027"
name = "demo"
version = "0.1.0"
entry = "contracts/main.cell"
source_roots = ["contracts", "shared"]
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("main.cell"),
            r#"
module demo::main

action ping() -> u64 {
    verification
        1
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("contracts").join("token.cell"),
            r#"
module demo::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("shared").join("token.cell"),
            r#"
module demo::token

resource Token has store, replace, relock, consume, burn {
    amount: u64
}
"#,
        )
        .unwrap();

        let err = compile_path(root, CompileOptions::default()).unwrap_err();
        assert!(err.message.contains("duplicate module 'demo::token'"));
    }

    #[test]
    fn imported_type_validity_keeps_explicit_pure_helper_evidence() {
        let sources = vec![
            crate::InMemorySource {
                path: "src/main.cell".to_string(),
                role: Some("entry".to_string()),
                source: r#"
module validity::main

use validity::schema::{Token}

action mint(amount: u64) -> Token {
    verification
        return create Token { amount: amount }
}
"#
                .to_string(),
            },
            crate::InMemorySource {
                path: "src/schema.cell".to_string(),
                role: None,
                source: r#"
module validity::schema

const MIN_AMOUNT: u64 = 1

fn positive(value: u64) -> bool {
    return value > 0
}

resource Token has store, create {
    amount: u64

    validity
        require positive(amount) && amount >= MIN_AMOUNT
}
"#
                .to_string(),
            },
        ];
        let report = crate::compile_sources_metadata_with_diagnostics(&sources, "src/main.cell", crate::CURRENT_EDITION, None);
        assert!(report.diagnostics.is_empty(), "unexpected diagnostics: {:?}", report.diagnostics);
        let metadata = report.metadata.expect("imported validity metadata");
        let token = metadata.types.iter().find(|ty| ty.name == "Token").expect("imported Token metadata");
        let predicate = token.validity_predicates.first().expect("imported Token validity predicate");
        assert_eq!(predicate.evidence_tier, crate::EvidenceTier::CheckedRuntime);
        assert_eq!(predicate.create_path_status, "checked-runtime");
        assert!(predicate.dependencies.contains(&"helper:validity::schema::positive".to_string()));
        assert!(predicate.dependencies.contains(&"constant:validity::schema::MIN_AMOUNT".to_string()));
        assert!(
            metadata.functions.iter().any(|function| function.name.contains("positive")),
            "imported validity helper body was not retained"
        );
    }

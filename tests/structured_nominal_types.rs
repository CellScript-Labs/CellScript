//! Nominal equality follows the defining declaration, including inside ordinary
//! helper and vector arguments. Equal layouts from different owners stay distinct.
#![cfg(feature = "cli")]
mod common;

use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::bytes::Bytes;

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

fn has_reserved_type_error(error: &cellscript::error::CompileError) -> bool {
    (error.code == Some(cellscript::error::DiagnosticCode::E2111) && error.message.contains("reserved by the CellScript prelude"))
        || error.related.iter().any(has_reserved_type_error)
}

#[test]
fn builtin_nominal_collision_rejects_the_previously_false_accepting_program_before_artifact_construction() {
    for comparison in ["==", "!="] {
        let source = format!(
            "module builtin_collision;\nstruct BlockNumber {{ left: u64, right: u64, }}\nfn second(value: BlockNumber) -> u64 {{ return value.right; }}\naction verify() -> u64 {{  let value = new BlockNumber {{ left: 11, right: 23, }}; require second(value) {comparison} 23; return 0; }}"
        );
        let metadata = cellscript::compile_metadata_with_diagnostics(&source, CellScriptEdition::Edition2027, None);
        assert!(metadata.metadata.is_none());
        assert!(metadata.diagnostics.iter().any(has_reserved_type_error), "{comparison}: {:?}", metadata.diagnostics);
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let error = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { diagnostics, source_contracts, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_err();
                assert!(has_reserved_type_error(&error), "{comparison}/{diagnostics}/source-contracts={source_contracts}: {error:?}");
            }
        }
    }
}

#[test]
fn builtin_nominal_collision_rejects_unused_declarations_and_generic_template_names() {
    for declaration in [
        "struct EpochDuration { left: u64, right: u64, }",
        "resource BlockNumber has store { left: u64, right: u64, }",
        "shared BlockNumber has store { left: u64, right: u64, }",
        "receipt BlockNumber { left: u64, right: u64, }",
        "enum ScriptHash { First, Second, }",
        "struct ExactScriptHandle { left: u64, right: u64, }",
        "struct DeploymentLineHandle { left: u64, right: u64, }",
        "struct ZkTransitionProof { left: u64, right: u64, }",
        "struct BlockNumber<T: fixed_value> { value: T, }",
    ] {
        let source = format!("module unused_builtin_collision;\n{declaration}\naction verify() -> u64 {{  return 0; }}");
        let metadata = cellscript::compile_metadata_with_diagnostics(&source, CellScriptEdition::Edition2027, None);
        assert!(metadata.metadata.is_none(), "accepted {declaration}");
        assert!(metadata.diagnostics.iter().any(has_reserved_type_error), "{declaration}: {:?}", metadata.diagnostics);
    }
}

#[test]
fn builtin_nominal_collision_rejects_import_aliases_in_metadata_and_executable_paths() {
    for alias in ["BlockNumber", "ScriptHash", "ExactScriptHandle"] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cell.toml"),
            "[package]\nname = \"builtin_alias\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
        )
        .unwrap();
        std::fs::write(root.path().join("src/provider.cell"), "module provider;\npublic struct Pair { left: u64, right: u64, }")
            .unwrap();
        std::fs::write(
            root.path().join("src/main.cell"),
            format!("module builtin_alias;\nuse provider::Pair as {alias};\nfn second(value: {alias}) -> u64 {{ return value.right; }}\naction verify() -> u64 {{  let value = new {alias} {{ left: 11, right: 23 }}; require second(value) != 23; return 0; }}"),
        )
        .unwrap();
        let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
        let path = camino::Utf8Path::from_path(root.path()).unwrap();
        let metadata = cellscript::compile_path_metadata_with_diagnostics(path, CompileOptions::default());
        assert!(metadata.metadata.is_none());
        assert!(metadata.diagnostics.iter().any(has_reserved_type_error), "{alias}: {:?}", metadata.diagnostics);
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let error = cellscript::compile_path_with_executable_surface_policy(
                path,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_err();
            assert!(has_reserved_type_error(&error), "{alias}/{diagnostics}: {error:?}");
        }
    }
}

#[test]
fn builtin_nominal_collision_fix_preserves_builtin_and_ordinary_record_execution() {
    for (comparison, expected_exit) in [("==", 0), ("!=", 5)] {
        let source = format!(
            "module distinct_builtin_and_record;\nstruct MyBlockNumber {{ left: u64, right: u64, }}\nfn second(value: MyBlockNumber) -> u64 {{ return value.right; }}\nfn relay(value: BlockNumber) -> BlockNumber {{ return value; }}\naction verify(witness block: BlockNumber) -> u64 {{  require ckb::block_number_to_u64(relay(block)) == 42; let value = new MyBlockNumber {{ left: 11, right: 23, }}; require second(value) {comparison} 23; return 0; }}"
        );
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let result = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions { diagnostics, source_contracts, target: Some("riscv64-elf".into()), ..Default::default() },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{comparison}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                result.validate().unwrap();
                let payload = cellscript::encode_entry_witness_args_for_params(
                    &result.metadata.actions[0].params,
                    &[cellscript::EntryWitnessArg::U64(42)],
                )
                .unwrap();
                use ckb_testtool::ckb_types::{packed, prelude::*};
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                fixture.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(
                    execution.exit_code, expected_exit,
                    "{comparison}/{diagnostics}/source-contracts={source_contracts}: {execution:?}"
                );
            }
        }
    }
}

fn fixture(root: &std::path::Path, edition: CellScriptEdition, read_owner: &str, extra_body: &str) -> std::path::PathBuf {
    for owner in ["left", "right", "consumer"] {
        let package = root.join(owner);
        std::fs::create_dir_all(package.join("src")).unwrap();
        let dependencies =
            if owner == "consumer" { "[dependencies]\nleft = { path = \"../left\" }\nright = { path = \"../right\" }\n" } else { "" };
        std::fs::write(
            package.join("Cell.toml"),
            format!("[package]\nname = \"{owner}\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n{dependencies}"),
        )
        .unwrap();
        let source = if owner == "consumer" {
            format!(
                r#"
module consumer;
use left::Value as First;
use {read_owner}::Value as Alias;
use right::Value as Other;
enum Wrapped {{ Value(First), }}
fn inspect(value: Alias) -> u64 {{ return value.amount; }}
fn inspect_values(values: Vec<Alias>) -> u64 {{ return values[0].amount; }}
fn relay(value: Alias) -> First {{ let retained = value; return retained; }}
action verify() -> u64 {{

    let value: First = new First {{ amount: 42, tag: 7, }};
    require inspect(value) == 42;
    let copied: Alias = relay(value);
    require inspect(copied) == 42;
    {extra_body}
    let values: Vec<First> = [value];
    require inspect_values(values) == 42;
    return 0;
}}
"#
            )
        } else {
            format!("module {owner};\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64, tag: u32, }}")
        };
        std::fs::write(package.join("src/main.cell"), source).unwrap();
    }
    let app = root.join("consumer");
    let locked = common::cellc_command().current_dir(&app).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    app
}

#[test]
fn same_owner_nominal_aliases_execute_through_ordinary_and_vector_helper_parameters() {
    {
        let edition = CellScriptEdition::Edition2027;
        for (scenario, body) in [
        ("calls", ""),
        ("array", "let array: [First; 1] = [copied];\nrequire array[0].tag == 7;"),
        ("qualified", "let qualified: left::Value = copied;\nrequire qualified.tag == 7;"),
        ("vector literal", "let array: Vec<First> = [copied];\nrequire array[0].tag == 7;"),
        ("mutation", "let mut items: Vec<First> = [];\nitems.push(copied);\nitems.insert(1, copied);\nitems.set(0, copied);\nrequire items.contains(copied);\nrequire items[1].amount == 42;"),
        ("removal", "let mut items: Vec<First> = [value, copied];\nlet removed: Alias = items.remove(0);\nlet popped: Alias = items.pop();\nrequire removed.amount == 42;\nrequire popped.tag == 7;"),
        ("extension", "let mut items: Vec<First> = [];\nlet extra: [Alias; 1] = [copied];\nitems.extend_from_slice(extra);\nrequire items[0].tag == 7;"),
        ("aggregate", "let aggregate: (Alias, [First; 1]) = (value, [copied]);\nrequire aggregate.0.amount == 42;\nrequire aggregate.1[0].tag == 7;"),
        ("enum", "let wrapped = Wrapped::Value(copied);\nlet amount = match wrapped { Wrapped::Value(inner) => inner.amount };\nrequire amount == 42;"),
      ] {
        let root = tempfile::tempdir().unwrap();
        let app = fixture(root.path(), edition, "left", body);
        let app = camino::Utf8Path::from_path(&app).unwrap();
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    app,
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{scenario}: {edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                result.validate().unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                assert_eq!(execution.exit_code, 0, "{scenario}: {edition}/{diagnostics}/source-contracts={source_contracts}: {execution:?}");
            }
        }
      }
    }
}

#[test]
fn equal_layout_nominals_from_other_owners_reject_before_codegen() {
    fn mismatch(error: &cellscript::error::CompileError) -> bool {
        error.message.contains("type mismatch") || error.related.iter().any(mismatch)
    }
    {
        let edition = CellScriptEdition::Edition2027;
        let root = tempfile::tempdir().unwrap();
        let app = fixture(root.path(), edition, "right", "");
        let app = camino::Utf8Path::from_path(&app).unwrap();
        let metadata = cellscript::compile_path_metadata_with_diagnostics(app, CompileOptions::default());
        assert!(metadata.has_errors());
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let error = cellscript::compile_path_with_executable_surface_policy(
                app,
                CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_err();
            assert!(mismatch(&error), "{error:?}");
        }
    }
}

#[test]
fn specialization_identity_resolves_template_arguments_and_phantom_owners() {
    let declarations = r#"
struct Box<T: fixed_value> { value: T, }
struct Tagged<phantom T: fixed_value> { amount: u64, }
fn inspect_box(value: Box<Alias>) -> u64 { return value.value.amount; }
fn relay_box(value: Box<Alias>) -> Box<First> { let retained = value; return retained; }
"#;
    {
        let edition = CellScriptEdition::Edition2027;
        for (body, accepted) in [
            ("let boxed = new Box<First> { value: value, };\nlet boxed_copy: Box<Alias> = relay_box(boxed);\nrequire inspect_box(boxed_copy) == 42;\nlet nested: Box<Box<Alias>> = new Box<Box<First>> { value: boxed, };\nrequire nested.value.value.tag == 7;\nlet mut boxes: Vec<Box<Alias>> = [boxed];\nboxes.push(boxed_copy);\nrequire boxes[1].value.amount == 42;", true),
            ("let tagged = new Tagged<First> { amount: 42, };\nlet alias: Tagged<Alias> = tagged;\nrequire alias.amount == 42;", true),
            ("let tagged = new Tagged<First> { amount: 42, };\nlet other: Tagged<Other> = tagged;", false),
        ] {
            let root = tempfile::tempdir().unwrap();
            let app = fixture(root.path(), edition, "left", body);
            let source_path = app.join("src/main.cell");
            let source = std::fs::read_to_string(&source_path).unwrap().replace("enum Wrapped", &format!("{declarations}\nenum Wrapped"));
            std::fs::write(source_path, source).unwrap();
            let locked = common::cellc_command().current_dir(&app).args(["lock", "--json"]).output().unwrap();
            assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stdout));
            let app = camino::Utf8Path::from_path(&app).unwrap();
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                for source_contracts in [false, true] {
                    let result = cellscript::compile_path_with_executable_surface_policy(
                        app,
                        CompileOptions { edition, diagnostics, source_contracts, target: Some("riscv64-elf".into()), ..Default::default() },
                        None,
                        ExecutableSurfacePolicy::DenyFailClosed,
                    );
                    if accepted {
                        let result = result.unwrap_or_else(|error| panic!("{edition}/{diagnostics}: {error:?}"));
                        result.validate().unwrap();
                        let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
                    } else {
                        assert!(result.is_err(), "phantom arguments from different owners must remain distinct");
                    }
                }
            }
        }
    }
}

#[test]
fn commitment_aliases_preserve_exact_domain_spelling_and_imported_owners() {
    fn has_code(error: &cellscript::error::CompileError, code: cellscript::error::DiagnosticCode) -> bool {
        error.code == Some(code) || error.related.iter().any(|error| has_code(error, code))
    }
    use ckb_testtool::{
        ckb_hash::blake2b_256,
        ckb_types::{packed, prelude::*},
    };
    let cases = [
        ("use left::Value;\nuse left::relay_commitment;", "Value", "Value", "relay_commitment(expected)", true),
        ("use left::Value as First;\nuse left::Value as Alias;", "First", "Alias", "expected", false),
        ("use left::Value as Alias;\nuse left::relay_commitment;", "Alias", "Alias", "relay_commitment(expected)", false),
        ("use right::Value;\nuse left::relay_commitment;", "Value", "Value", "relay_commitment(expected)", false),
    ];
    {
        let edition = CellScriptEdition::Edition2027;
        for (imports, expected, opening, forwarded, accepted) in cases {
            let root = tempfile::tempdir().unwrap();
            let app = fixture(root.path(), edition, "left", "");
            let left = root.path().join("left/src/main.cell");
            let mut source = std::fs::read_to_string(&left).unwrap();
            source.push_str("\npublic fn relay_commitment(value: Commitment<Value>) -> Commitment<Value> { return value; }\n");
            std::fs::write(left, source).unwrap();
            std::fs::write(app.join("src/main.cell"), format!("module consumer;\n{imports}\naction verify(witness expected: Commitment<{expected}>, witness opening: Opening<{opening}>) -> u64 {{  let forwarded = {forwarded}; let value = commitment::open(forwarded, opening); require value.amount == 42; return 0; }}")).unwrap();
            let locked = common::cellc_command().current_dir(&app).args(["lock", "--json"]).output().unwrap();
            assert!(locked.status.success());
            let app = camino::Utf8Path::from_path(&app).unwrap();
            let metadata = cellscript::compile_path_metadata_with_diagnostics(app, CompileOptions::default());
            assert_eq!(!metadata.has_errors(), accepted, "{imports}");
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    app,
                    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                );
                if accepted {
                    let result = result.unwrap_or_else(|error| panic!("{imports}: {error:?}"));
                    result.validate().unwrap();
                    let value = [42u64.to_le_bytes().as_slice(), 7u32.to_le_bytes().as_slice()].concat();
                    let preimage = [b"CellScriptPackedHashV0\0Value\0".as_slice(), 12u32.to_le_bytes().as_slice(), &value].concat();
                    let payload = cellscript::encode_entry_witness_args_for_params(
                        &result.metadata.actions[0].params,
                        &[
                            cellscript::EntryWitnessArg::Bytes(blake2b_256(&preimage).to_vec()),
                            cellscript::EntryWitnessArg::Bytes(value),
                        ],
                    )
                    .unwrap();
                    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                    fixture.witnesses =
                        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                    assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
                } else {
                    let error = result.unwrap_err();
                    assert!(has_code(&error, cellscript::error::DiagnosticCode::E2111), "{imports}: {error:?}");
                    assert!(!has_code(&error, cellscript::error::DiagnosticCode::E2400), "must reject before artifact construction");
                }
            }
        }
    }
}

#[test]
fn external_template_requests_do_not_merge_equal_local_argument_names_from_different_owners() {
    {
        let edition = CellScriptEdition::Edition2027;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"scoped_requests\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
        for (file, source) in [
            ("main", "module scoped_requests;\nuse first::read as read_first;\nuse second::read as read_second;\naction verify() -> u64 {  require read_first() == 42; require read_second() == 9; return 0; }"),
            ("templates", "module templates;\npublic struct Box<T: fixed_value> { value: T, }"),
            ("first", "module first;\nuse left::Value as Arg;\nuse templates::Box;\npublic fn read() -> u64 { let boxed = new Box<Arg> { value: new Arg { left_amount: 42, tag: 7, }, }; return boxed.value.left_amount; }"),
            ("second", "module second;\nuse right::Value as Arg;\nuse templates::Box;\npublic fn read() -> u64 { let boxed = new Box<Arg> { value: new Arg { right_amount: 9, tag: 7, }, }; return boxed.value.right_amount; }"),
            ("left", "module left;\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { left_amount: u64, tag: u32, }"),
            ("right", "module right;\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { right_amount: u64, tag: u32, }"),
        ] { std::fs::write(root.path().join(format!("src/{file}.cell")), source).unwrap(); }
        let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success());
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    camino::Utf8Path::from_path(root.path()).unwrap(),
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                result.validate().unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
            }
        }
    }
}

#[test]
fn linked_template_labels_keep_owners_and_unambiguous_packed_domains() {
    use ckb_testtool::ckb_types::{packed, prelude::*};
    {
        let edition = CellScriptEdition::Edition2027;
        for committed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("src")).unwrap();
            std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"linked_templates\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
            let files = if committed {
                vec![
                    ("main", "module linked_templates;\nuse left::Value as Arg;\nuse first::make;\naction verify(witness opening: Opening<Arg>) -> u64 {  let expected = make(); let value = commitment::open(expected, opening); require value.amount == 42; return 0; }".to_string()),
                    ("left", "module left;\npublic struct Value has copy, drop, store, fixed, serializable, non_linear { amount: u64, tag: u32, }".into()),
                    ("templates", "module templates;\npublic struct Box<T: fixed_value> { value: T, }".into()),
                    ("first", "module first;\nuse left::Value as Arg;\nuse templates::Box;\npublic fn make() -> Commitment<Arg> { let boxed = new Box<Arg> { value: new Arg { amount: 42, tag: 7, }, }; return commitment::commit(boxed.value); }".into()),
                ]
            } else {
                vec![
                    ("main", "module linked_templates;\nuse first::read as read_first;\nuse second::read as read_second;\naction verify() -> u64 {  require read_first() == 42; require read_second() == 9; return 0; }".into()),
                    ("first", "module first;\npublic struct Box<T: fixed_value> { value: T, }\npublic fn read() -> u64 { let boxed = new Box<u64> { value: 42, }; return boxed.value; }".into()),
                    ("second", "module second;\npublic struct Box<T: fixed_value> { value: T, }\npublic fn read() -> u64 { let boxed = new Box<u64> { value: 9, }; return boxed.value; }".into()),
                ]
            };
            for (file, source) in files {
                std::fs::write(root.path().join(format!("src/{file}.cell")), source).unwrap();
            }
            let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
            assert!(locked.status.success());
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    camino::Utf8Path::from_path(root.path()).unwrap(),
                    CompileOptions { edition, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("committed={committed}/{edition}/{diagnostics}: {error:?}"));
                result.validate().unwrap();
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                if committed {
                    let value = [42u64.to_le_bytes().as_slice(), 7u32.to_le_bytes().as_slice()].concat();
                    let payload = cellscript::encode_entry_witness_args_for_params(
                        &result.metadata.actions[0].params,
                        &[cellscript::EntryWitnessArg::Bytes(value)],
                    )
                    .unwrap();
                    fixture.witnesses =
                        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                }
                assert_eq!(
                    ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code,
                    0,
                    "committed={committed}/{edition}/{diagnostics}"
                );
            }
        }
    }
}

#[test]
fn imported_signatures_cannot_capture_an_unrelated_requester_type_with_the_same_name() {
    for accepted in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), "[package]\nname = \"signature_owners\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n").unwrap();
        std::fs::write(root.path().join("src/left.cell"), "module left;\npublic struct Value { amount: u64, tag: u32, }\npublic fn inspect(value: Value) -> u64 { return value.amount; }").unwrap();
        std::fs::write(root.path().join("src/right.cell"), "module right;\npublic struct Value { amount: u64, tag: u32, }").unwrap();
        let (imports, ty) = if accepted { ("", "left::Value") } else { ("use right::Value;", "Value") };
        std::fs::write(root.path().join("src/main.cell"), format!("module signature_owners;\nuse left::inspect;\n{imports}\naction verify() -> u64 {{  let value: {ty} = new {ty} {{ amount: 42, tag: 7 }}; require inspect(value) == 42; return 0; }}")).unwrap();
        let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success());
        let path = camino::Utf8Path::from_path(root.path()).unwrap();
        assert_eq!(!cellscript::compile_path_metadata_with_diagnostics(path, CompileOptions::default()).has_errors(), accepted);
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_path_with_executable_surface_policy(
                path,
                CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
                None,
                ExecutableSurfacePolicy::DenyFailClosed,
            );
            if accepted {
                let result = result.unwrap_or_else(|error| panic!("{error:?}"));
                result.validate().unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
            } else {
                assert_ne!(
                    result.unwrap_err().code,
                    Some(cellscript::error::DiagnosticCode::E2400),
                    "wrong-owner arguments must fail before ELF admission"
                );
            }
        }
    }
}

#[test]
fn builtin_spelling_in_a_module_coordinate_is_not_a_builtin_type_alias() {
    use ckb_testtool::ckb_types::{packed, prelude::*};
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("Cell.toml"),
        "[package]\nname = \"qualified_case\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n",
    )
    .unwrap();
    for owner in ["Hash", "hash"] {
        std::fs::write(root.path().join(format!("src/{owner}.cell")), format!("module {owner};\npublic struct Value {{ amount: u64, tag: u32, }}\npublic fn inspect(value: Value) -> u64 {{ return value.amount; }}")).unwrap();
    }
    std::fs::write(root.path().join("src/main.cell"), "module qualified_case;\nuse Hash::Value as UpperValue;\nuse hash::Value as LowerValue;\nuse Hash::inspect as inspect_upper;\nuse hash::inspect as inspect_lower;\naction verify(witness first: UpperValue, witness second: LowerValue) -> u64 {  require inspect_upper(first) == 42; require inspect_lower(second) == 9; return 0; }").unwrap();
    let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success());
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(root.path()).unwrap(),
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_or_else(|error| panic!("{error:?}"));
        result.validate().unwrap();
        let values = [42u64, 9].map(|amount| {
            cellscript::EntryWitnessArg::Bytes([amount.to_le_bytes().as_slice(), 7u32.to_le_bytes().as_slice()].concat())
        });
        let payload = cellscript::encode_entry_witness_args_for_params(&result.metadata.actions[0].params, &values).unwrap();
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
    }
}

#[test]
fn prelude_option_results_keep_one_identity_across_modules_and_owned_register_returns() {
    {
        let edition = CellScriptEdition::Edition2027;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"option_calls\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
        std::fs::write(
            root.path().join("src/provider.cell"),
            r#"
module provider;
public fn maybe(amount: u64) -> Option<u64> { return Option::Some::<u64>(amount); }
public fn relay(value: Option<u64>) -> Option<u64> { return value; }
public fn small(amount: u32) -> Option<u32> { return Option::Some::<u32>(amount); }
"#,
        )
        .unwrap();
        std::fs::write(
            root.path().join("src/main.cell"),
            r#"
module option_calls;
use provider::maybe;
use provider::relay;
use provider::small;
fn inspect(value: Option<u64>) -> u64 {
    return match value { Option::Some(amount) => amount, Option::None => 0 };
}

fn inspect_small(value: Option<u32>) -> bool {
    return match value { Option::Some(amount) => amount == 7, Option::None => false };
}
action verify() -> u64 {

    let value: Option<u64> = maybe(42);
    let clobber: Option<u64> = maybe(9);
    require inspect(relay(value)) == 42;
    require inspect(clobber) == 9;
    let narrow: Option<u32> = small(7);
    require inspect_small(narrow);
    return 0;
}
"#,
        )
        .unwrap();
        let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stdout));
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    camino::Utf8Path::from_path(root.path()).unwrap(),
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                result.validate().unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
            }
        }
    }
}

#[test]
fn prelude_option_arguments_keep_their_nominal_owners_across_import_aliases() {
    {
        let edition = CellScriptEdition::Edition2027;
        for (owner, accepted) in [("left", true), ("right", false)] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("src")).unwrap();
            std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"option_owners\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
            for owner in ["left", "right"] {
                std::fs::write(
                    root.path().join(format!("src/{owner}.cell")),
                    format!(
                        "module {owner};\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64, }}"
                    ),
                )
                .unwrap();
            }
            std::fs::write(root.path().join("src/provider.cell"), "module provider;\nuse left::Value as Arg;\npublic fn inspect(value: Option<Arg>) -> u64 { return match value { Option::Some(inner) => inner.amount, Option::None => 0 }; }\n").unwrap();
            std::fs::write(root.path().join("src/main.cell"), format!("module option_owners;\nuse {owner}::Value as Alias;\nuse provider::inspect;\naction verify() -> u64 {{  let record: Alias = new Alias {{ amount: 42, }}; let value: Option<Alias> = Option::Some::<Alias>(record); require inspect(value) == 42; return 0; }}\n")).unwrap();
            let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
            assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stdout));
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                for source_contracts in [false, true] {
                    let result = cellscript::compile_path_with_executable_surface_policy(
                        camino::Utf8Path::from_path(root.path()).unwrap(),
                        CompileOptions {
                            edition,
                            diagnostics,
                            source_contracts,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                        None,
                        ExecutableSurfacePolicy::DenyFailClosed,
                    );
                    if accepted {
                        let result = result
                            .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                        result.validate().unwrap();
                        let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
                    } else {
                        assert_ne!(result.unwrap_err().code, Some(cellscript::error::DiagnosticCode::E2400));
                    }
                }
            }
        }
    }
    let source = "module option_user_enum;\nenum Maybe<T: fixed_value> { None, Some(T), }\nfn inspect(value: Option<u64>) -> u64 { return 0; }\naction verify() -> u64 {  let value: Maybe<u64> = Maybe::Some::<u64>(42); return inspect(value); }";
    let error = cellscript::compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap_err();
    assert_ne!(error.code, Some(cellscript::error::DiagnosticCode::E2400));
}

#[test]
fn inferred_prelude_results_retain_their_qualified_layout_and_instantiation_closure() {
    {
        let edition = CellScriptEdition::Edition2027;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"inferred_option\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
        std::fs::write(root.path().join("src/provider.cell"), "module provider;\npublic fn maybe(amount: u64) -> Option<u64> { return Option::Some::<u64>(amount); }\npublic fn relay(value: Option<u64>) -> Option<u64> { return value; }\npublic fn inspect(value: Option<u64>) -> u64 { return match value { Option::Some(amount) => amount, Option::None => 0 }; }\n").unwrap();
        std::fs::write(root.path().join("src/main.cell"), "module inferred_option;\nuse provider::maybe;\nuse provider::relay;\nuse provider::inspect;\naction verify() -> u64 {  let value = maybe(42); let other = maybe(9); require inspect(relay(value)) == 42; require inspect(other) == 9; return 0; }\n").unwrap();
        let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
        assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stdout));
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for source_contracts in [false, true] {
                let result = cellscript::compile_path_with_executable_surface_policy(
                    camino::Utf8Path::from_path(root.path()).unwrap(),
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    None,
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                result.validate().unwrap();
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
            }
        }
    }
}

#[test]
fn imported_generic_results_resolve_equivalent_argument_aliases_before_linking() {
    {
        let edition = CellScriptEdition::Edition2027;
        for (owner, accepted) in [("left", true), ("right", false)] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join("src")).unwrap();
            std::fs::write(root.path().join("Cell.toml"), format!("[package]\nname = \"generic_results\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"{edition}\"\n")).unwrap();
            for owner in ["left", "right"] {
                std::fs::write(root.path().join(format!("src/{owner}.cell")), format!("module {owner};\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64, }}\n")).unwrap();
            }
            std::fs::write(root.path().join("src/provider.cell"), "module provider;\nuse left::Value as Arg;\npublic struct Box<T: fixed_value> { value: T, }\npublic fn make(amount: u64) -> Box<Arg> { let value: Arg = new Arg { amount: amount, }; return new Box<Arg> { value: value, }; }\npublic fn relay(value: Box<Arg>) -> Box<Arg> { return value; }\n").unwrap();
            std::fs::write(root.path().join("src/main.cell"), format!("module generic_results;\nuse {owner}::Value as Alias;\nuse provider::Box as Imported;\nuse provider::make;\nuse provider::relay;\naction verify() -> u64 {{  let value: Imported<Alias> = make(42); let other: Imported<Alias> = make(9); let copied: Imported<Alias> = relay(value); require copied.value.amount == 42; require other.value.amount == 9; return 0; }}\n")).unwrap();
            let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
            assert!(locked.status.success(), "{}", String::from_utf8_lossy(&locked.stdout));
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                for source_contracts in [false, true] {
                    let result = cellscript::compile_path_with_executable_surface_policy(
                        camino::Utf8Path::from_path(root.path()).unwrap(),
                        CompileOptions {
                            edition,
                            diagnostics,
                            source_contracts,
                            target: Some("riscv64-elf".into()),
                            ..Default::default()
                        },
                        None,
                        ExecutableSurfacePolicy::DenyFailClosed,
                    );
                    if accepted {
                        let result = result
                            .unwrap_or_else(|error| panic!("{edition}/{diagnostics}/source-contracts={source_contracts}: {error:?}"));
                        result.validate().unwrap();
                        let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                        assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
                    } else {
                        assert_ne!(result.unwrap_err().code, Some(cellscript::error::DiagnosticCode::E2400));
                    }
                }
            }
        }
    }
}

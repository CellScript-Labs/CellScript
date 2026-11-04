//! Required alias evidence must not equate valid, equal-layout declarations
//! from another owner after all outer sidecar identities are rebound.
use super::*;

#[cfg(feature = "cli")]
use super::common;

#[test]
#[cfg(feature = "cli")]
fn prelude_option_never_erases_nominal_argument_owners_after_rebinding() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("Cell.toml"), "[package]\nname = \"option_argument_owners\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n").unwrap();
    for owner in ["left", "right"] {
        std::fs::write(
            root.path().join(format!("src/{owner}.cell")),
            format!("module {owner}\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64 }}"),
        )
        .unwrap();
    }
    std::fs::write(
        root.path().join("src/main.cell"),
        r#"
module option_argument_owners
use left::Value as First
use left::Value as Alias
use right::Value as Other
fn inspect(value: Option<Alias>) -> u64 {
    return match value { Option::Some(inner) => inner.amount, Option::None => 0 }
}
action verify(witness value: Option<First>, witness other: Option<Other>) -> u64 {
    verification
    require inspect(value) == 42
    return 0
}
"#,
    )
    .unwrap();
    let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success());
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(root.path()).unwrap(),
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        let mut changed = valid.clone();
        let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "verify").unwrap();
        let other = entry.params.iter().find(|param| param.name == "other").unwrap().clone();
        let call = entry
            .blocks
            .iter_mut()
            .flat_map(|block| &mut block.operations)
            .find(|operation| operation.call.as_ref().is_some_and(|call| call.target == "inspect"))
            .unwrap();
        call.operands[0].local = Some(other.binding_id);
        call.operands[0].ty = other.ty;
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
fn prelude_option_requires_its_closed_declaration_after_coherent_variant_rebinding() {
    fn rename_variant(value: &mut Value) -> usize {
        match value {
            Value::String(text) if text == "Some" => {
                *text = "Present".into();
                1
            }
            Value::Array(items) => items.iter_mut().map(rename_variant).sum(),
            Value::Object(fields) => fields.values_mut().map(rename_variant).sum(),
            _ => 0,
        }
    }
    let source = r#"
module option_contract
fn inspect(value: Option<u64>) -> u64 {
    return match value { Option::Some(amount) => amount, Option::None => 0 }
}
action verify() -> u64 {
    verification
    let value: Option<u64> = Option::Some<u64>(42)
    require inspect(value) == 42
    return 0
}
"#;
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_with_executable_surface_policy(
            source,
            CompileOptions { diagnostics, source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        valid.check().unwrap();
        let mut changed = valid.clone();
        let mut record = serde_json::to_value(&changed.record).unwrap();
        // The variant label is changed consistently in the declaration, layout,
        // constructor and match detail. Its tag and all machine bytes stay the
        // same: a shape-equivalent user enum must still not acquire Option's
        // prelude identity after the surrounding hashes are recomputed.
        assert!(rename_variant(&mut record) >= 3);
        assert!(rename_variant(&mut changed.metadata) >= 3);
        changed.record = serde_json::from_value(record).unwrap();
        changed.rebind_layered_identities();
        assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
    }
}

#[test]
#[cfg(feature = "cli")]
fn specialized_alias_calls_reject_distinct_argument_and_template_owners() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("Cell.toml"), "[package]\nname = \"specialized_calls\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n").unwrap();
    std::fs::write(
        root.path().join("src/main.cell"),
        r#"
module specialized_calls
use left::Value as First
use left::Value as Alias
use right::Value as Other
use right::Box as Foreign
struct Box<T: fixed_value> { value: T }
fn inspect(value: Box<Alias>) -> u64 { return value.value.amount }
action verify(witness value: Box<First>, witness other: Box<Other>, witness foreign: Foreign<First>) -> u64 {
    verification
    require inspect(value) == 42
    require other.value.tag == 7
    require foreign.value.amount == 9
    return 0
}
"#,
    )
    .unwrap();
    for owner in ["left", "right"] {
        std::fs::write(root.path().join(format!("src/{owner}.cell")), format!("module {owner}\npublic struct Value has copy, drop, store, fixed, serializable, non_linear {{ amount: u64, tag: u32 }}\npublic struct Box<T: fixed_value> {{ value: T }}\n")).unwrap();
    }
    let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success());
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(root.path()).unwrap(),
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let values = [(42u64, 1u32), (8, 7), (9, 2)].map(|(amount, tag)| {
            cellscript::EntryWitnessArg::Bytes([amount.to_le_bytes().as_slice(), tag.to_le_bytes().as_slice()].concat())
        });
        let payload = cellscript::encode_entry_witness_args_for_params(&compiled.metadata.actions[0].params, &values).unwrap();
        let valid = Fixture::from_result(compiled);
        valid.check().unwrap();
        use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
        let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
        fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
        assert_eq!(ckb_script_runner::execute_cellscript_script(&valid.artifact, &fixture).exit_code, 0);
        for replacement in ["other", "foreign"] {
            let mut changed = valid.clone();
            let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "verify").unwrap();
            let param = entry.params.iter().find(|param| param.name == replacement).unwrap().clone();
            let call = entry
                .blocks
                .iter_mut()
                .flat_map(|block| &mut block.operations)
                .find(|op| op.call.as_ref().is_some_and(|call| call.target == "inspect"))
                .unwrap();
            call.operands[0].local = Some(param.binding_id);
            call.operands[0].ty = param.ty;
            changed.rebind_layered_identities();
            assert_code(&changed, CheckerRejectionCode::V2419TypedSemanticsInvalid);
        }
    }
}

#[test]
#[cfg(feature = "cli")]
fn nominal_collection_operands_require_same_owner_evidence_after_rebinding() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("src")).unwrap();
    std::fs::write(root.path().join("Cell.toml"), "[package]\nname = \"nominal_operations\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\n").unwrap();
    std::fs::write(
        root.path().join("src/main.cell"),
        r#"
module nominal_operations
use left::Value as First
use left::Value as Alias
use right::Value as Other
enum Wrapped { Item(First) }
action verify(witness value: First, witness alias: Alias, witness other: Other) -> u64 {
    verification
    let mut values: Vec<First> = []
    values.push(alias)
    values.insert(0, alias)
    values.set(0, alias)
    require values.contains(alias)
    let extra: [Alias; 1] = [alias]
    values.extend_from_slice(extra)
    let removed: Alias = values.remove(0)
    let popped: Alias = values.pop()
    require removed.amount == popped.amount
    let aggregate: (First, [First; 1]) = (alias, [alias])
    require aggregate.0.amount == value.amount
    let wrapped = Wrapped::Item(alias)
    let amount = match wrapped { Wrapped::Item(item) => item.amount }
    require amount == value.amount
    require other.tag == 7
    return 0
}
"#,
    )
    .unwrap();
    for owner in ["left", "right"] {
        std::fs::write(
            root.path().join("src").join(format!("{owner}.cell")),
            format!("module {owner}\npublic struct Value {{ amount: u64, tag: u32 }}"),
        )
        .unwrap();
    }
    let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(root.path()).unwrap(),
            CompileOptions { diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        assert_eq!(valid.check(), Ok(()));
        for missing in [false, true] {
            let mut changed = valid.clone();
            if missing {
                changed.record.typed_semantics.nominal_declarations = None;
            } else {
                let catalog = changed.record.typed_semantics.nominal_declarations.as_mut().unwrap();
                let alias = catalog.layout_bindings.iter_mut().find(|binding| binding.lowered_name == "Alias").unwrap();
                alias.owner_module = "right".into();
            }
            changed.rebind_typed_semantics();
            assert_eq!(changed.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid), "{diagnostics}/missing={missing}");
        }
    }
}

#[test]
#[cfg(feature = "cli")]
fn ordinary_nominal_alias_calls_require_owner_evidence_and_reject_equal_layout_substitution() {
    use cellscript::InMemorySource;

    let sources = [
        InMemorySource {
            path: "main.cell".into(),
            source: r#"
module nominal_calls
use left::Value as First
use left::Value as Alias
use right::Value as Other
resource Token has store, consume { amount: u64 }
fn relay(value: Alias) -> First { let retained = value return retained }
action burn(input token: Token, witness value: First) {
    verification
    let copied: Alias = relay(value)
    require copied.amount > 0
    consume token
}
"#
            .into(),
            role: None,
        },
        InMemorySource {
            path: "left.cell".into(),
            source: "module left\npublic struct Value { amount: u64, tag: u32 }".into(),
            role: None,
        },
        InMemorySource {
            path: "right.cell".into(),
            source: "module right\npublic struct Value { amount: u64, tag: u32 }".into(),
            role: None,
        },
    ];
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("Cell.toml"),
        "[package]\nname = \"nominal_calls\"\nversion = \"0.1.0\"\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2026\"\n",
    )
    .unwrap();
    for source in &sources {
        std::fs::write(root.path().join("src").join(&source.path), &source.source).unwrap();
    }
    let locked = common::cellc_command().current_dir(root.path()).args(["lock", "--json"]).output().unwrap();
    assert!(locked.status.success(), "{} {}", String::from_utf8_lossy(&locked.stdout), String::from_utf8_lossy(&locked.stderr));
    for diagnostics in [cellscript::CompilerDiagnostics::default(), cellscript::CompilerDiagnostics::without_optimizations()] {
        let compiled = cellscript::compile_path_with_executable_surface_policy(
            camino::Utf8Path::from_path(root.path()).unwrap(),
            CompileOptions { diagnostics, source_contracts: false, target: Some("riscv64-elf".into()), ..Default::default() },
            None,
            cellscript::ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let valid = Fixture::from_result(compiled);
        for mutation in ["missing", "owner", "call-parameter", "return-owner"] {
            let mut changed = valid.clone();
            match mutation {
                "missing" => changed.record.typed_semantics.nominal_declarations = None,
                "owner" => {
                    let binding = changed
                        .record
                        .typed_semantics
                        .nominal_declarations
                        .as_mut()
                        .unwrap()
                        .layout_bindings
                        .iter_mut()
                        .find(|binding| binding.lowered_name == "Alias")
                        .unwrap();
                    binding.owner_module = "right".into();
                }
                "call-parameter" => {
                    let call = changed
                        .record
                        .typed_semantics
                        .entries
                        .iter_mut()
                        .flat_map(|entry| &mut entry.blocks)
                        .flat_map(|block| &mut block.operations)
                        .filter_map(|operation| operation.call.as_mut())
                        .find(|call| call.target == "relay")
                        .unwrap();
                    call.params[0] = "Other".into();
                }
                "return-owner" => {
                    let entry = changed.record.typed_semantics.entries.iter_mut().find(|entry| entry.name == "relay").unwrap();
                    entry.return_type = "Other".into();
                    changed.record.entries.iter_mut().find(|entry| entry.name == "relay").unwrap().return_type = "Other".into();
                }
                _ => unreachable!(),
            }
            changed.rebind_typed_semantics();
            assert_eq!(changed.check(), Err(CheckerRejectionCode::V2419TypedSemanticsInvalid), "{diagnostics}/{mutation}");
        }
    }
}

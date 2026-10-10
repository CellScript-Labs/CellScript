#![cfg(not(feature = "wasm"))]

//! Imported source closure for explicit policies. These are compiler/binding
//! checks, not token authorization or chain-acceptance evidence.

use cellscript::artifact::{
    compile_path_artifact_metadata, compile_sources_artifact, compile_sources_artifact_metadata, ArtifactAction, ArtifactContext,
    ArtifactDeclaration, ArtifactDispatch,
};
use cellscript::{
    compile_path_with_artifact_name, CellScriptEdition, CompileMetadata, CompileOptions, ExecutableSurfacePolicy, InMemorySource,
};
use cellscript_artifact_checker::{CellBindingMembership, CellBindingSource, EntryDispatchContract, TypedSemanticOperationDetail};

const ENTRY: &str = r#"
module policy_imports::main
use policy_imports::types::Token
use policy_imports::helpers::positive as is_positive
use policy_imports::helpers::imported_action

action mint(witness amount: u64, witness recipient: Address) {
    verification
        require is_positive(amount)
        create Token { amount: amount } with_lock(recipient)
}

action burn(input token: Token) {
    verification
        require is_positive(token.amount)
        consume token
}
"#;

const TYPES: &str = r#"
module policy_imports::types
resource Token has store, consume { amount: u64 }
"#;

const HELPERS: &str = r#"
module policy_imports::helpers
fn positive(value: u64) -> bool { return value > 0 }
action imported_action(witness marker: u64) {
    verification
        require marker == 77
}
"#;

fn sources() -> Vec<InMemorySource> {
    [("src/main.cell", ENTRY), ("src/types.cell", TYPES), ("src/helpers.cell", HELPERS)]
        .into_iter()
        .map(|(path, source)| InMemorySource { path: path.into(), source: source.into(), role: None })
        .collect()
}

fn declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        name: "ImportedTokenPolicy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 40, action: "burn".into() }, ArtifactAction { tag: 10, action: "mint".into() }],
        common_checks: Vec::new(),
    }
}

fn options(edition: CellScriptEdition) -> CompileOptions {
    CompileOptions {
        edition,
        opt_level: 0,
        target: Some("riscv64-elf".into()),
        target_profile: Some("ckb".into()),
        ..Default::default()
    }
}

fn assert_same_contract(full: &CompileMetadata, metadata: &CompileMetadata) {
    assert_eq!(full.typed_semantics, metadata.typed_semantics);
    assert_eq!(full.typed_semantics_hash, metadata.typed_semantics_hash);
    assert_eq!(full.runtime.policy_artifact, metadata.runtime.policy_artifact);
    assert_eq!(full.compatibility_profile, metadata.compatibility_profile);
    assert_eq!(serde_json::to_value(&full.actions).unwrap(), serde_json::to_value(&metadata.actions).unwrap());
}

fn assert_imported_contract(metadata: &CompileMetadata) {
    let typed = &metadata.typed_semantics;
    let EntryDispatchContract::PolicyWitnessV1(policy) = &typed.foundation.entry_contract.dispatch else {
        panic!("an imported policy must retain explicit dispatch");
    };
    assert_eq!(policy.resource, "Token");
    let resources = typed.types.iter().filter(|schema| schema.name == "Token").collect::<Vec<_>>();
    assert_eq!(resources.len(), 1, "the imported concrete resource resolves exactly once");
    assert_eq!(resources[0].layout_hash, policy.resource_layout_hash);
    assert_eq!(resources[0].fields.len(), 1);
    assert_eq!(resources[0].fields[0].name, "amount");
    assert_eq!(
        policy.variants.iter().map(|variant| (variant.tag, variant.entry_id.as_str())).collect::<Vec<_>>(),
        [(10, "action:mint"), (40, "action:burn")]
    );
    assert_eq!((policy.variants[0].input_count, policy.variants[0].output_count), (0, 1));
    assert_eq!((policy.variants[1].input_count, policy.variants[1].output_count), (1, 0));
    assert!(policy.common_checks.is_empty());
    let helper = typed
        .entries
        .iter()
        .find(|entry| entry.name == "is_positive")
        .expect("imported scalar helper must remain in the checked dependency closure");
    assert_eq!(helper.kind, "helper");
    assert!(helper.cell_bindings.is_empty(), "scalar helper must not acquire entry Cell bindings");
    assert!(!policy.variants.iter().any(|variant| variant.entry_id == helper.id || variant.entry_id.contains("imported_action")));
    for name in ["mint", "burn"] {
        let entry = typed.entries.iter().find(|entry| entry.name == name).unwrap();
        assert!(
            entry
                .blocks
                .iter()
                .flat_map(|block| &block.operations)
                .any(|operation| operation.call.as_ref().is_some_and(|call| call.target == "is_positive")),
            "{name} must call the retained imported helper"
        );
        assert!(entry.cell_bindings.iter().all(|binding| {
            binding.ty == "Token"
                && matches!(binding.source, CellBindingSource::GroupInput | CellBindingSource::GroupOutput)
                && binding.membership == CellBindingMembership::CurrentTypeGroup
        }));
    }
    assert!(metadata.runtime.fail_closed_runtime_features.is_empty());
}

#[test]
fn virtual_policy_imports_preserve_resource_helper_and_explicit_exports_in_both_editions() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let full = compile_sources_artifact(
            &sources(),
            "src/main.cell",
            options(edition),
            declaration(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_or_else(|error| panic!("{edition:?}: imported virtual policy failed: {error}"));
        let metadata = compile_sources_artifact_metadata(&sources(), "src/main.cell", options(edition), declaration()).unwrap();
        full.validate().unwrap();
        assert_same_contract(&full.metadata, &metadata);
        assert_imported_contract(&metadata);
    }
}

#[test]
fn package_and_virtual_policy_imports_share_the_same_resolved_contract() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let directory = tempfile::tempdir().unwrap();
        let root = camino::Utf8Path::from_path(directory.path()).unwrap();
        let manager = cellscript::package::PackageManager::new(directory.path());
        manager.init("policy_imports").unwrap();
        let mut manifest = manager.read_manifest().unwrap();
        manifest.package.edition = edition;
        manifest.artifacts = vec![declaration()];
        manager.write_manifest(&manifest).unwrap();
        for source in sources() {
            std::fs::write(root.join(source.path), source.source).unwrap();
        }
        let full =
            compile_path_with_artifact_name(root, options(edition), "ImportedTokenPolicy", ExecutableSurfacePolicy::DenyFailClosed)
                .unwrap_or_else(|error| panic!("{edition:?}: imported package policy failed: {error}"));
        let metadata = compile_path_artifact_metadata(root, options(edition), "ImportedTokenPolicy").unwrap();
        let virtual_metadata =
            compile_sources_artifact_metadata(&sources(), "src/main.cell", options(edition), declaration()).unwrap();
        full.validate().unwrap();
        assert_same_contract(&full.metadata, &metadata);
        assert_same_contract(&metadata, &virtual_metadata);
        assert_imported_contract(&metadata);
        assert!(!root.join("build").exists(), "library compilation must not write build products");
    }
}

#[test]
fn imported_scalar_arithmetic_and_cast_guards_are_retained_across_compile_modes() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        for opt_level in 0..=3 {
            for (body, operator, cast_guard) in [
                ("return value + 1 > value", "add", false),
                ("return 100 / value > 0", "div", false),
                ("return (value as u8) > 0", "le", true),
            ] {
                let mut sources = sources();
                sources[2].source = HELPERS.replace("return value > 0", body);
                let options = CompileOptions { opt_level, ..options(edition) };
                let full = compile_sources_artifact(
                    &sources,
                    "src/main.cell",
                    options.clone(),
                    declaration(),
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap_or_else(|error| panic!("{edition:?} opt{opt_level} {body}: {error}"));
                let metadata = compile_sources_artifact_metadata(&sources, "src/main.cell", options, declaration()).unwrap();
                full.validate().unwrap();
                assert_same_contract(&full.metadata, &metadata);
                assert_imported_contract(&metadata);
                let helper = metadata.typed_semantics.entries.iter().find(|entry| entry.name == "is_positive").unwrap();
                assert!(helper.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
                    matches!(&operation.detail, TypedSemanticOperationDetail::BinaryOperator { operator: actual } if actual == operator)
                }), "{edition:?} opt{opt_level} must retain {operator} in the imported helper");
                if cast_guard {
                    assert!(
                        helper.blocks.iter().any(|block| block.runtime_error.as_ref().is_some_and(|error| error.code == 20)),
                        "a narrowing cast must retain its explicit numeric failure block"
                    );
                }
            }
        }
    }
}

#[test]
fn imported_policy_resolution_and_declaration_errors_agree_across_compile_modes() {
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let mut ambiguous = sources();
        ambiguous[0].source =
            ENTRY.replace("use policy_imports::types::Token", "use policy_imports::types::Token\nuse policy_imports::other::Token");
        ambiguous.push(InMemorySource {
            path: "src/other.cell".into(),
            source: "module policy_imports::other\nresource Token has store, consume { amount: u64, nonce: u64 }\n".into(),
            role: None,
        });
        let cases = [
            (
                sources(),
                ArtifactDeclaration { context: ArtifactContext::TypeGroup { resource: "MisspelledToken".into() }, ..declaration() },
                "MisspelledToken",
            ),
            (
                sources(),
                ArtifactDeclaration { actions: vec![ArtifactAction { tag: 10, action: "missing_action".into() }], ..declaration() },
                "missing_action",
            ),
            (
                sources(),
                ArtifactDeclaration {
                    actions: vec![
                        ArtifactAction { tag: 10, action: "mint".into() },
                        ArtifactAction { tag: 10, action: "burn".into() },
                    ],
                    ..declaration()
                },
                "tag 10",
            ),
            (ambiguous, declaration(), "duplicate symbol 'Token'"),
        ];
        for (sources, declaration, expected) in cases {
            let full = compile_sources_artifact(
                &sources,
                "src/main.cell",
                options(edition),
                declaration.clone(),
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_err();
            let metadata = compile_sources_artifact_metadata(&sources, "src/main.cell", options(edition), declaration).unwrap_err();
            assert_eq!(full.message, metadata.message, "{edition:?}: rejection must not depend on machine-code generation");
            assert_eq!(full.code, metadata.code);
            assert!(full.message.contains(expected), "{edition:?}: expected {expected}: {}", full.message);
        }
    }
}

const TEMPLATE_A: &str = r#"
module qual::a
public struct Pair<T: fixed_value> has copy, drop, store, fixed, serializable, non_linear { left: T, right: T }
"#;

const TEMPLATE_A_DERIVED: &str = r#"
module qual::a
public struct Pair<T: fixed_value> { left: T, right: T }
"#;

const TEMPLATE_B: &str = r#"
module qual::b
public struct Pair<T: fixed_value> { amount: T }
"#;

const TEMPLATE_TOKEN: &str = r#"
module qual::types
resource Token has store, consume { amount: u64 }
"#;

const HOLDER_MAIN: &str = r#"
module qual::main
use qual::a::Pair
use qual::types::Token

public struct Holder<T: fixed_value> { first: T }
action verify(input token: Token, witness value: u64) {
    verification
    consume token
    let held: Holder<Pair<u64>> = Holder<Pair<u64>> { first: Pair<u64> { left: value, right: value } }
    require value > 0
}
"#;

const AMBIGUOUS_MAIN: &str = r#"
module qual::main
use qual::a::Pair
use qual::b::Pair as OtherPair
use qual::types::Token

public struct Holder<T: fixed_value> { first: T }
action verify(input token: Token, witness value: u64) {
    verification
    consume token
    let first: Holder<Pair<u64>> = Holder<Pair<u64>> { first: Pair<u64> { left: value, right: value } }
    let second: Holder<OtherPair<u64>> = Holder<OtherPair<u64>> { first: OtherPair<u64> { amount: value } }
    require value > 0
}
"#;

fn template_declaration() -> ArtifactDeclaration {
    ArtifactDeclaration {
        name: "qual-policy".into(),
        context: ArtifactContext::TypeGroup { resource: "Token".into() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 10, action: "verify".into() }],
        common_checks: Vec::new(),
    }
}

fn compile_template_case(main: &'static str, a: &'static str, b: Option<&'static str>, opt: u8) -> cellscript::CompileResult {
    let mut sources = vec![InMemorySource { path: "src/main.cell".into(), source: main.into(), role: None }];
    if let Some(b_source) = b {
        sources.push(InMemorySource { path: "src/b.cell".into(), source: b_source.into(), role: None });
    }
    sources.push(InMemorySource { path: "src/a.cell".into(), source: a.into(), role: None });
    sources.push(InMemorySource { path: "src/types.cell".into(), source: TEMPLATE_TOKEN.into(), role: None });
    compile_sources_artifact(
        &sources,
        "src/main.cell",
        CompileOptions {
            edition: CellScriptEdition::Edition2027,
            opt_level: opt,
            target: Some("riscv64-elf".into()),
            target_profile: Some("ckb".into()),
            source_contracts: true,
            ..Default::default()
        },
        template_declaration(),
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap()
}

fn project_template_case(compiled: &cellscript::CompileResult) -> cellscript_artifact_checker::interface::CheckedModuleProjection {
    cellscript_artifact_checker::interface::inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &cellscript_artifact_checker::CheckerBudgets::default(),
    )
    .unwrap()
    .project_module_contract()
    .unwrap()
}

/// An imported template application can serve as a generic argument: its
/// value-ability evidence derives from the owner's template definition, with
/// or without an explicit `has` clause, and the consumer's checks resolve it.
#[test]
fn imported_template_arguments_carry_value_ability_evidence() {
    for opt_level in 0..=3 {
        for a in [TEMPLATE_A, TEMPLATE_A_DERIVED] {
            let compiled = compile_template_case(HOLDER_MAIN, a, None, opt_level);
            let typed = &compiled.metadata.typed_semantics;
            assert!(typed.instantiations.iter().any(|instance| instance.module == "qual::a" && instance.template == "Pair"));
            assert!(typed
                .instantiations
                .iter()
                .any(|instance| instance.template == "Holder" && instance.type_arguments == ["Pair<u64>"]));
            project_template_case(&compiled);
        }
    }
}

/// Two owners may export the same template name. Their instantiations keep
/// distinct qualified identities in the checked projection, so a candidate
/// cannot silently swap which owner supplies an argument.
#[test]
fn same_named_imported_templates_resolve_through_qualified_identities() {
    for opt_level in 0..=3 {
        let compiled = compile_template_case(AMBIGUOUS_MAIN, TEMPLATE_A_DERIVED, Some(TEMPLATE_B), opt_level);
        let typed = &compiled.metadata.typed_semantics;
        assert_eq!(
            typed
                .instantiations
                .iter()
                .filter(|instance| instance.template == "Pair")
                .map(|instance| instance.module.as_str())
                .collect::<Vec<_>>(),
            ["qual::a", "qual::b"]
        );
        let projection = project_template_case(&compiled);
        let wire: serde_json::Value = serde_json::from_slice(&projection.canonical_bytes().unwrap()).unwrap();
        assert!(wire["contracts"].get("layout:qual::a::Pair<u64>").is_some());
        assert!(wire["contracts"].get("layout:qual::b::Pair<u64>").is_some());
        // Rebinding the local alias to the other owner keeps the retained
        // instance set but points each holder layout at the other owner's
        // qualified identity; directional matching rejects the substitution.
        let rebound = AMBIGUOUS_MAIN
            .replace("use qual::a::Pair", "use qual::a::Pair as OtherPair")
            .replace("use qual::b::Pair as OtherPair", "use qual::b::Pair")
            .replace("Pair<u64> { left: value, right: value }", "Pair<u64> { amount: value }")
            .replace("OtherPair<u64> { amount: value }", "OtherPair<u64> { left: value, right: value }");
        let candidate = compile_template_case(rebound.leak(), TEMPLATE_A_DERIVED, Some(TEMPLATE_B), opt_level);
        assert!(projection.check_required_contracts(&project_template_case(&candidate)).is_err(), "opt={opt_level}");
    }
}

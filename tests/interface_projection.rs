//! Directional contracts are derived from checked bytes, never producer flags.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use cellscript_artifact_checker::{
    interface::{inspect_bundle, CheckedModuleProjection},
    CheckerBudgets,
};

fn project(source: &str, opt_level: u8) -> CheckedModuleProjection {
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap()
    .project_module_contract()
    .unwrap()
}

const BASE: &str = r#"
module projection
private struct Inner has copy, drop, store, fixed, serializable, non_linear { value: u64 }
public struct Envelope has copy, drop, store, fixed, serializable, non_linear { nested: Inner, count: u64 }
public struct Box<T: fixed_value> { value: T }
public struct Marker<phantom T: copy> { tag: u64 }
public fn identity<T: fixed_value>(value: T) -> T { value }
public fn unused(value: u64) -> u64 { value }
public action verify(witness value: Envelope, witness box: Box<u64>) {
    verification
    require value.count > 0
    require identity<u64>(box.value) > 0
}
"#;

#[test]
fn directional_projection_preserves_required_contracts_and_allows_candidate_additions() {
    for opt in 0..=3 {
        let required = project(BASE, opt);
        let alternative = project(&BASE.replace("value.count > 0", "value.count > 1"), opt);
        assert_eq!(required.identity(), alternative.identity());
        assert_ne!(required.artifact_report().artifact_hash, alternative.artifact_report().artifact_hash);
        assert!(!required.artifact_report().semantic_equivalence_claimed);
        let candidate = project(
            &BASE
                .replace("value.count > 0", "value.count > 1")
                .replace("public fn unused", "public struct Extra { value: u32 }\npublic fn unused"),
            opt,
        );
        required.check_required_contracts(&candidate).unwrap();
        assert!(candidate.check_required_contracts(&required).is_err());
        assert_ne!(required.identity(), candidate.identity());
        assert_eq!(required.module(), "projection");
        let wire: serde_json::Value = serde_json::from_slice(&required.canonical_bytes().unwrap()).unwrap();
        assert!(wire["contracts"].get("nominal:projection::Inner").is_some());
        assert!(wire["contracts"].get("layout:projection::Box<u64>").is_some());
        assert_eq!(wire["contracts"]["effective:projection::verify"]["dispatch"]["kind"], "single-entry");
        // Retained source declaration does not invent an executable helper.
        assert!(wire["contracts"].get("callable:projection::unused").is_some());
    }
}

#[test]
fn projection_byte_limits_precede_json_or_elf_parsing() {
    use cellscript_artifact_checker::{interface::project_bundle, CheckerRejectionCode};
    let oversized = vec![0; 4 * 1024 * 1024 + 1];
    for index in 0..4 {
        let mut inputs: [&[u8]; 4] = [&[]; 4];
        inputs[index] = &oversized;
        let error = project_bundle(inputs[0], inputs[1], inputs[2], inputs[3], &CheckerBudgets::default()).unwrap_err();
        assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
    }
}

#[test]
fn coherent_incompatible_sources_reject_after_successful_independent_inspection() {
    let mutations = [
        ("nested-width", BASE.replace("value: u64 }\npublic struct Envelope", "value: u32 }\npublic struct Envelope")),
        (
            "nested-owner",
            BASE.replace(
                "private struct Inner",
                "private struct Other has copy, drop, store, fixed, serializable, non_linear { value: u64 }\nprivate struct Inner",
            )
            .replace("nested: Inner", "nested: Other"),
        ),
        ("field-order", BASE.replace("nested: Inner, count: u64", "count: u64, nested: Inner")),
        ("binder-constraint", BASE.replace("Marker<phantom T: copy>", "Marker<phantom T: copy + drop>")),
        (
            "parameter-name",
            BASE.replace("witness value: Envelope", "witness changed: Envelope").replace("value.count", "changed.count"),
        ),
        (
            "witness-type",
            BASE.replace("witness box: Box<u64>", "witness box: Box<u32>")
                .replace("identity<u64>(box.value)", "identity<u32>(box.value)"),
        ),
        ("public-omission", BASE.replace("public fn unused", "private fn unused")),
        ("module-owner", BASE.replace("module projection", "module other")),
    ];
    for opt in 0..=3 {
        let required = project(BASE, opt);
        for (mutation, source) in &mutations {
            let candidate = project(source, opt);
            assert!(required.check_required_contracts(&candidate).is_err(), "opt={opt} mutation={mutation}");
        }
    }
}

#[test]
fn optimizer_pruned_helpers_keep_declarations_without_inventing_execution() {
    let first = project(BASE, 0);
    let baseline: serde_json::Value = serde_json::from_slice(&first.canonical_bytes().unwrap()).unwrap();
    for opt in 1..=3 {
        let next = project(BASE, opt);
        let wire: serde_json::Value = serde_json::from_slice(&next.canonical_bytes().unwrap()).unwrap();
        for key in [
            "callable:projection::unused",
            "callable:projection::identity",
            "nominal:projection::Envelope",
            "nominal:projection::Box",
            "effective:projection::verify",
        ] {
            assert_eq!(baseline["contracts"][key], wire["contracts"][key], "opt={opt} key={key}");
        }
        if let Some(helper) = wire["contracts"].get("effective:projection::unused") {
            assert_eq!(helper["dispatch"]["kind"], "retained-helper");
        }
    }
    let optimized = project(BASE, 3);
    let wire: serde_json::Value = serde_json::from_slice(&optimized.canonical_bytes().unwrap()).unwrap();
    assert!(wire["contracts"].get("effective:projection::unused").is_none());
}

#[test]
fn inferred_effect_changes_reject_when_source_annotations_are_identical() {
    for opt in 0..=3 {
        let pure = project("module effects\npublic action verify() { verification require true }", opt);
        let read = project("module effects\npublic action verify() { verification let amount = ckb::cell_capacity(source::input(0)) require amount > 0 }", opt);
        let pure_wire: serde_json::Value = serde_json::from_slice(&pure.canonical_bytes().unwrap()).unwrap();
        let read_wire: serde_json::Value = serde_json::from_slice(&read.canonical_bytes().unwrap()).unwrap();
        assert_eq!(pure_wire["contracts"]["callable:effects::verify"], read_wire["contracts"]["callable:effects::verify"]);
        assert_eq!(pure_wire["contracts"]["effective:effects::verify"]["effect"], "Pure");
        assert_eq!(read_wire["contracts"]["effective:effects::verify"]["effect"], "ReadOnly");
        assert!(pure.check_required_contracts(&read).is_err());
        assert!(read.check_required_contracts(&pure).is_err());
    }
}

#[test]
fn policy_dispatch_tags_are_required_but_unrelated_candidate_variants_are_allowed() {
    use cellscript::artifact::{ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
    use cellscript::{compile_path_with_executable_surface_policy, CompileEntryScope};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.cell");
    let source = "module dispatch\nresource Token has store, consume { amount: u64 }\naction mint(witness amount: u64, witness recipient: Address) { verification require amount > 0 create Token { amount: amount } with_lock(recipient) }\naction burn(input token: Token) { verification consume token }\naction extra(input token: Token) { verification consume token }";
    std::fs::write(&path, source).unwrap();
    for opt_level in 0..=3 {
        let compile = |actions: Vec<ArtifactAction>| {
            let compiled = compile_path_with_executable_surface_policy(
                path.to_str().unwrap(),
                CompileOptions { target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
                Some(CompileEntryScope::Artifact(ArtifactDeclaration {
                    name: "token-policy".into(),
                    context: ArtifactContext::TypeGroup { resource: "Token".into() },
                    dispatch: ArtifactDispatch::PolicyWitnessV1,
                    actions,
                    common_checks: Vec::new(),
                })),
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            inspect_bundle(
                &compiled.artifact_bytes,
                &serde_json::to_vec(&compiled.metadata).unwrap(),
                &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
                &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap()
            .project_module_contract()
            .unwrap()
        };
        let actions = || vec![ArtifactAction { tag: 10, action: "mint".into() }, ArtifactAction { tag: 40, action: "burn".into() }];
        let required = compile(actions());
        let mut changed = actions();
        changed[0].tag = 11;
        assert!(required.check_required_contracts(&compile(changed)).is_err());
        let mut extra = actions();
        extra.push(ArtifactAction { tag: 50, action: "extra".into() });
        required.check_required_contracts(&compile(extra)).unwrap();
    }
}

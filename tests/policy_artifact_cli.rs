mod common;

use cellscript::artifact::{ArtifactAction, ArtifactContext, ArtifactDeclaration, ArtifactDispatch};
use cellscript::package::PackageManager;
use cellscript::{CompileMetadata, EntryWitnessArg, NEXT_EDITION};
use common::cellc_command;
use serde_json::Value;
use std::path::Path;

const SOURCE: &str = r#"
module policy_cli
resource Token has store, consume { amount: u64 }
action common() { require true }
action mint(witness amount: u64, witness recipient: Address) {
    require amount > 0
    create Token { amount: amount } with_lock(recipient)
}
action burn(input token: Token) { consume token }
"#;

fn fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let manager = PackageManager::new(directory.path());
    manager.init("policy_cli").unwrap();
    let mut manifest = manager.read_manifest().unwrap();
    manifest.package.edition = NEXT_EDITION;
    manifest.artifacts.push(ArtifactDeclaration {
        name: "token-policy".to_string(),
        context: ArtifactContext::TypeGroup { resource: "Token".to_string() },
        dispatch: ArtifactDispatch::PolicyWitnessV1,
        actions: vec![ArtifactAction { tag: 40, action: "burn".into() }, ArtifactAction { tag: 10, action: "mint".into() }],
        common_checks: vec!["common".to_string()],
    });
    manager.write_manifest(&manifest).unwrap();
    std::fs::write(directory.path().join("src/main.cell"), SOURCE).unwrap();
    success(directory.path(), &["lock", "--json"]);
    directory
}

fn success(root: &Path, args: &[&str]) -> Value {
    let output = cellc_command().current_dir(root).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn metadata(root: &Path, result: &Value) -> CompileMetadata {
    let path = root.join(result["metadata"].as_str().unwrap());
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn package_policy_build_and_check_preserve_default_cache_and_inner_entry_bytes() {
    let directory = fixture();
    let root = directory.path();
    let original = success(root, &["build", "--target", "riscv64-elf", "--json"]);
    let original_metadata = metadata(root, &original);
    assert!(original_metadata.runtime.policy_artifact.is_none());
    let original_artifact = std::fs::read(root.join(original["artifact"].as_str().unwrap())).unwrap();

    let policy = success(root, &["build", "--artifact", "token-policy", "--target", "riscv64-elf", "--json"]);
    assert_eq!(policy["policy_artifact"], "token-policy");
    assert_eq!(policy["cache_hit"], false);
    let policy_metadata = metadata(root, &policy);
    let declaration = &policy_metadata.runtime.policy_artifact.as_ref().unwrap().declaration;
    assert_eq!(declaration.name, "token-policy");
    assert_eq!(declaration.actions.iter().map(|variant| variant.tag).collect::<Vec<_>>(), [10, 40]);
    assert_eq!(declaration.common_checks, ["common"]);
    assert_eq!(policy_metadata.compatibility_profile.entry_witness_payload_abi, "cellscript-policy-witness-v1");
    let mint = policy_metadata.actions.iter().find(|action| action.name == "mint").unwrap();
    let values = [EntryWitnessArg::U64(7), EntryWitnessArg::Address([0x11; 32])];
    let mut expected = b"CSARGv1\0".to_vec();
    expected.extend_from_slice(&7u64.to_le_bytes());
    expected.extend_from_slice(&[0x11; 32]);
    assert_eq!(mint.entry_witness_args(&values).unwrap(), expected);

    let bytes_before_check = std::fs::read(root.join(policy["artifact"].as_str().unwrap())).unwrap();
    let scoped_metadata = cellscript::artifact::compile_path_artifact_metadata(
        camino::Utf8Path::from_path(root).unwrap(),
        cellscript::CompileOptions {
            diagnostics: cellscript::CompilerDiagnostics::default(),
            target: Some("riscv64-elf".to_string()),
            ..Default::default()
        },
        "token-policy",
    )
    .unwrap();
    assert_eq!(scoped_metadata.edition, NEXT_EDITION);
    assert_eq!(scoped_metadata.typed_semantics_hash, policy_metadata.typed_semantics_hash);
    assert_eq!(
        serde_json::to_value(&scoped_metadata.runtime.policy_artifact).unwrap(),
        serde_json::to_value(&policy_metadata.runtime.policy_artifact).unwrap()
    );
    assert_eq!(scoped_metadata.compatibility_profile, policy_metadata.compatibility_profile);
    assert_eq!(std::fs::read(root.join(policy["artifact"].as_str().unwrap())).unwrap(), bytes_before_check);
    assert_ne!(bytes_before_check, original_artifact);
    let checked = success(root, &["check", "--artifact", "token-policy", "--all-targets", "--json"]);
    assert_eq!(checked["policy_artifact"], "token-policy");
    assert_eq!(checked["checked_targets"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read(root.join(policy["artifact"].as_str().unwrap())).unwrap(), bytes_before_check);

    let restored = success(root, &["build", "--target", "riscv64-elf", "--json"]);
    assert!(metadata(root, &restored).runtime.policy_artifact.is_none());
    assert_eq!(std::fs::read(root.join(restored["artifact"].as_str().unwrap())).unwrap(), original_artifact);

    let single = success(root, &["build", "--entry-action", "mint", "--target", "riscv64-elf", "--json"]);
    let single = metadata(root, &single);
    assert!(single.runtime.policy_artifact.is_none());
    assert_eq!(single.compatibility_profile.entry_witness_payload_abi, "cellscript-entry-witness-v1");
    assert_eq!(single.actions.iter().find(|action| action.name == "mint").unwrap().entry_witness_args(&values).unwrap(), expected);
}

#[test]
fn policy_entry_witness_routes_exact_hash_and_tag_and_keeps_no_payload_empty() {
    let directory = fixture();
    let root = directory.path();
    let script_hash = "11".repeat(32);
    for (action, expected_tag, arguments) in
        [("burn", 40, Vec::new()), ("mint", 10, vec!["--arg", "7", "--arg", script_hash.as_str()])]
    {
        let mut args =
            vec!["entry-witness", "--artifact", "token-policy", "--action", action, "--script-hash", &script_hash, "--json"];
        args.extend(arguments);
        let result = success(root, &args);
        assert_eq!(result["abi"], "cellscript-policy-witness-v1");
        assert_eq!(result["placement_performed"], false);
        assert_eq!(result["script_hash_is_authentication"], false);
        let bytes = hex::decode(result["witness_hex"].as_str().unwrap()).unwrap();
        let records = cellscript::policy_witness::decode_policy_witness_bundle(&bytes).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].role, cellscript::policy_witness::PolicyScriptRole::Type);
        assert_eq!(records[0].script_hash, [0x11; 32]);
        assert_eq!(records[0].tag, expected_tag);
        if action == "burn" {
            assert!(records[0].args.is_empty());
        } else {
            assert_eq!(&records[0].args[..8], b"CSARGv1\0");
            assert_eq!(&records[0].args[8..16], &7u64.to_le_bytes());
            assert_eq!(&records[0].args[16..], &[0x11; 32]);
        }
    }
    for extra in [
        vec!["--action", "common", "--script-hash", &script_hash],
        vec!["--action", "burn", "--script-hash", "11"],
        vec!["--action", "burn"],
        vec!["--lock", "common", "--script-hash", &script_hash],
        vec!["--action", "burn", "--script-hash", &script_hash, "--arg", "7"],
    ] {
        let output = cellc_command()
            .current_dir(root)
            .args(["entry-witness", "--artifact", "token-policy", "--json"])
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success(), "invalid policy request must reject");
    }
    let legacy = success(root, &["entry-witness", "--action", "mint", "--arg", "7", "--arg", &script_hash, "--json"]);
    assert_eq!(legacy["abi"], "cellscript-entry-witness-v1");
    assert!(hex::decode(legacy["witness_hex"].as_str().unwrap()).unwrap().starts_with(b"CSARGv1\0"));
}

#[test]
fn policy_builder_exports_declared_variants_and_rejects_mismatched_metadata() {
    let directory = fixture();
    let root = directory.path();
    let result =
        success(root, &["gen-builder", "--artifact", "token-policy", "--target", "typescript", "--output", "builder", "--json"]);
    assert_eq!(result["action_count"], 2);
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("builder/cellscript-builder-manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["actions"].as_array().unwrap().iter().map(|action| action["name"].as_str().unwrap()).collect::<Vec<_>>(),
        ["mint", "burn"]
    );
    assert_eq!(manifest["actions"][0]["policy_tag"], 10);
    assert_eq!(manifest["actions"][1]["policy_tag"], 40);
    assert_eq!(manifest["actions"][1]["entry_witness_required"], false);
    assert_eq!(manifest["actions"][1]["policy_witness_required"], true);
    assert_eq!(manifest["runtime_contract"]["requires_policy_witness_bundle"], true);
    let index = std::fs::read_to_string(root.join("builder/src/index.ts")).unwrap();
    assert!(index.contains("policyWitness: policyWitnessRequest(action)"));
    assert!(!index.contains("export function planCommon"));
    assert!(index.contains("export function encodePolicyWitnessBundle"));
    for extra in [
        vec!["--artifact", "token-policy", "--action", "common"],
        vec!["--artifact", "other-policy", "--metadata", "builder/src/metadata.json"],
    ] {
        let output = cellc_command()
            .current_dir(root)
            .args(["gen-builder", "--target", "typescript", "--output", "invalid", "--json"])
            .args(extra)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!root.join("invalid").exists());
    }
    // Existing metadata input must retain policy dispatch automatically: there
    // is no opt-out flag that could reinterpret it as legacy entry metadata.
    let from_metadata = success(
        root,
        &["gen-builder", "--metadata", "builder/src/metadata.json", "--target", "typescript", "--output", "from-metadata", "--json"],
    );
    assert_eq!(from_metadata["action_count"], 2);
    let metadata_path = root.join("builder/src/metadata.json");
    let mut tampered: Value = serde_json::from_slice(&std::fs::read(&metadata_path).unwrap()).unwrap();
    tampered["runtime"]["policy_artifact"]["declaration"]["actions"][0]["tag"] = serde_json::json!(999);
    std::fs::write(root.join("tampered.json"), serde_json::to_vec(&tampered).unwrap()).unwrap();
    let rejected = cellc_command()
        .current_dir(root)
        .args(["gen-builder", "--metadata", "tampered.json", "--target", "typescript", "--output", "tampered-builder", "--json"])
        .output()
        .unwrap();
    assert!(!rejected.status.success(), "metadata tags must remain bound to the independently checked plan");
    assert!(!root.join("tampered-builder").exists());

    // Native backend-only tests do not require npm/node_modules. When the
    // website's declared TypeScript dependency is installed, also execute the
    // exact generated package's typecheck and Node test scripts, without any
    // network access or dependency installation from this test.
    let tsc = Path::new(env!("CARGO_MANIFEST_DIR")).join("website/node_modules/typescript/bin/tsc");
    if tsc.is_file() && std::process::Command::new("node").arg("--version").output().is_ok() {
        let checked = std::process::Command::new("node")
            .arg(tsc)
            .args(["-p", "tsconfig.json"])
            .current_dir(root.join("builder"))
            .output()
            .unwrap();
        assert!(
            checked.status.success(),
            "generated policy TypeScript: {}{}",
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
        let tested = std::process::Command::new("node")
            .args(["--test", "test/builder.test.mjs", "test/policy-witness.test.mjs"])
            .current_dir(root.join("builder"))
            .output()
            .unwrap();
        assert!(
            tested.status.success(),
            "generated policy runtime/golden tests: {}{}",
            String::from_utf8_lossy(&tested.stdout),
            String::from_utf8_lossy(&tested.stderr)
        );
        eprintln!("generated TypeScript checked; {}", String::from_utf8_lossy(&tested.stdout));
    } else {
        eprintln!("generated TypeScript execution not checked: install the website's declared Node/TypeScript dependencies");
    }
    success(root, &["gen-builder", "--target", "typescript", "--output", "builder", "--json"]);
    assert!(!root.join("builder/test/policy-witness.test.mjs").exists());
    assert!(!std::fs::read_to_string(root.join("builder/src/index.ts"))
        .unwrap()
        .contains("export function encodePolicyWitnessBundle"));
}

#[test]
fn unknown_policy_name_fails_without_creating_build_products() {
    let directory = fixture();
    for command in ["build", "check"] {
        let output = cellc_command()
            .current_dir(directory.path())
            .args([command, "--artifact", "undeclared-policy", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(response.to_string().contains("artifact 'undeclared-policy' is not declared"), "{response}");
    }
    assert!(!directory.path().join("build/main.elf").exists());
    assert!(!directory.path().join("build/main.s").exists());
}

#[test]
fn policy_metadata_and_expand_inspect_selected_contract_without_codegen() {
    let directory = fixture();
    let root = directory.path();
    let manager = PackageManager::new(root);
    let mut manifest = manager.read_manifest().unwrap();
    manifest.artifacts[0].common_checks = vec!["zeta_check".into(), "alpha_check".into()];
    manager.write_manifest(&manifest).unwrap();
    std::fs::write(
        root.join("src/main.cell"),
        format!("{SOURCE}\naction zeta_check() {{ require true }}\naction alpha_check() {{ require true }}\n"),
    )
    .unwrap();
    success(root, &["lock", "--json"]);

    let metadata = success(root, &["metadata", "--artifact", "token-policy", "--target", "riscv64-elf"]);
    assert_eq!(metadata["runtime"]["policy_artifact"]["declaration"]["name"], "token-policy");
    assert_eq!(metadata["edition"], "2027");
    assert!(metadata["artifact_hash"].is_null(), "inspection must not emit machine artifacts");
    let expected = &metadata["typed_semantics"]["foundation"];
    let dispatch = &expected["entry_contract"]["dispatch"];
    assert_eq!(dispatch["kind"], "policy-witness-v1");
    assert_eq!(dispatch["variants"].as_array().unwrap().len(), 2);
    assert_eq!(dispatch["variants"][0]["tag"], 10);
    assert_eq!(dispatch["variants"][0]["entry_id"], "action:mint");
    assert_eq!(dispatch["variants"][0]["input_count"], 0);
    assert_eq!(dispatch["variants"][0]["output_count"], 1);
    assert_eq!(dispatch["variants"][1]["tag"], 40);
    assert_eq!(dispatch["variants"][1]["entry_id"], "action:burn");
    assert_eq!(dispatch["variants"][1]["input_count"], 1);
    assert_eq!(dispatch["variants"][1]["output_count"], 0);
    assert_eq!(dispatch["common_checks"], serde_json::json!(["action:zeta_check", "action:alpha_check"]));
    for input in [".", "Cell.toml", "src/main.cell"] {
        let expanded = success(root, &["expand", input, "--artifact", "token-policy", "--target", "riscv64-elf", "--json"]);
        assert_eq!(&expanded, expected, "same selected foundation for {input}");
        let inspected = success(root, &["metadata", input, "--artifact", "token-policy", "--target", "riscv64-elf"]);
        assert_eq!(inspected["typed_semantics"]["foundation"], *expected);
        assert!(inspected["artifact_hash"].is_null());
    }

    let output =
        cellc_command().current_dir(root).args(["expand", "--artifact", "token-policy", "--target", "riscv64-elf"]).output().unwrap();
    assert!(output.status.success());
    let human = String::from_utf8(output.stdout).unwrap();
    let section = human.split("\npolicy ").nth(1).unwrap().split("\ntypes\n").next().unwrap();
    assert!(section.starts_with("token-policy schema=cellscript-policy-dispatch-v1 version=1 resource=Token"));
    assert!(section.contains("selector input_type.records[type,current-script-hash].tag"));
    assert!(section.contains("source=group-input[0]-or-output[0]-if-no-inputs"));
    assert!(section.contains("unknown=reject"));
    assert!(section.contains("limits records=1..8 witness-bytes<=4096"));
    let variant_lines = section.lines().filter(|line| line.trim_start().starts_with("tag ")).collect::<Vec<_>>();
    assert_eq!(variant_lines.len(), 2);
    for (line, variant) in variant_lines.iter().zip(dispatch["variants"].as_array().unwrap()) {
        assert!(line.contains(&format!(
            "tag {} -> {} payload-schema={}",
            variant["tag"],
            variant["entry_id"].as_str().unwrap(),
            variant["payload_schema_hash"].as_str().unwrap()
        )));
        assert!(line.contains(&format!("group-inputs={} group-outputs={}", variant["input_count"], variant["output_count"])));
    }
    assert!(section.find("1 -> action:zeta_check").unwrap() < section.find("2 -> action:alpha_check").unwrap());
    assert!(!variant_lines.iter().any(|line| line.contains("check") || line.contains("common")));

    for command in ["metadata", "expand"] {
        let output = cellc_command()
            .current_dir(root)
            .args([command, "--artifact", "unknown-policy", "--output", "missing-output.json", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(error.to_string().contains("artifact 'unknown-policy' is not declared"));
        assert!(!root.join("missing-output.json").exists());
    }
    assert!(!root.join("build").exists(), "inspection must not create ELF/ASM or sidecars");
    let default = success(root, &["expand", "--json"]);
    assert_eq!(default["entry_contract"]["dispatch"]["kind"], "single-entry");
    assert_ne!(default["entry_contract"], expected["entry_contract"]);
}

//! The transactional upgrade owns review coverage; a generic policy ack cannot
//! supply schema review or replace replay of the exact source/lock candidates.

use cellscript::package::upgrade::{
    apply_upgrade_plan, create_upgrade_plan, UpgradeOptions, UpgradePlan, UpgradeSchemaAcknowledgement,
};
use cellscript::schema_acknowledgement::{
    acknowledge_schema_change, build_schema_change_plan, build_schema_change_plans, verify_schema_acknowledgement,
    SchemaRelationSelector,
};
use cellscript::{CompileOptions, CompileResult};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const OLD: &str = r#"
module review::token;
resource Token has store, replace, relock { owner: Address, amount: u64, }
action update(input token: Token) -> next: Token {
    replace token -> next {
        data = same except { };
        lock = same;
        capacity = same;
        type_script = same;
    }
}
"#;

fn added_source(assignment: &str) -> String {
    OLD.replace("amount: u64, }", "amount: u64, approval_nonce: u64, }")
        .replace("same except { }", &format!("same except {{ {assignment} }}"))
}

struct Fixture {
    _directory: tempfile::TempDir,
    package: PathBuf,
    baseline: PathBuf,
    old_lock: Vec<u8>,
}

impl Fixture {
    fn new(candidate: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let package = directory.path().join("candidate");
        let baseline = directory.path().join("baseline");
        for root in [&package, &baseline] {
            std::fs::create_dir_all(root.join("src")).unwrap();
            std::fs::write(
                root.join("Cell.toml"),
                concat!(
                    "[package]\nname = \"token\"\nversion = \"1.0.0\"\nedition = \"2027\"\n",
                    "cellscript_version = \">=0.33.0-alpha.1\"\nentry = \"src/main.cell\"\n"
                ),
            )
            .unwrap();
            std::fs::write(root.join("src/main.cell"), OLD).unwrap();
        }
        let output = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(&package).arg("lock").output().unwrap();
        assert!(output.status.success(), "lock: {}", String::from_utf8_lossy(&output.stderr));
        let old_lock = std::fs::read(package.join("Cell.lock")).unwrap();
        std::fs::write(baseline.join("Cell.lock"), &old_lock).unwrap();
        std::fs::write(package.join("src/main.cell"), candidate).unwrap();
        Self { _directory: directory, package, baseline, old_lock }
    }

    fn options(&self) -> UpgradeOptions {
        UpgradeOptions {
            offline: true,
            schema_baselines: [("token".to_string(), self.baseline.clone())].into_iter().collect(),
            ..UpgradeOptions::default()
        }
    }

    fn plan(&self) -> UpgradePlan {
        create_upgrade_plan(&self.package, &self.options()).unwrap()
    }

    fn reviewed_options(&self, plan: &UpgradePlan) -> UpgradeOptions {
        UpgradeOptions {
            acknowledgements: plan.required_acknowledgements.iter().cloned().collect(),
            schema_acknowledgements: plan
                .schema_reviews
                .iter()
                .filter(|review| review.plan.requires_acknowledgement)
                .map(|review| UpgradeSchemaAcknowledgement {
                    subject: review.subject.clone(),
                    environment: review.environment.clone(),
                    receipt: acknowledge_schema_change(&review.plan, "maintainer", "reset nonce in every successor branch").unwrap(),
                })
                .collect(),
            ..self.options()
        }
    }

    fn assert_unchanged(&self) {
        assert_eq!(std::fs::read(self.package.join("Cell.lock")).unwrap(), self.old_lock);
        assert_eq!(std::fs::read(self.baseline.join("Cell.lock")).unwrap(), self.old_lock);
        assert!(!self.package.join("Deployed.toml").exists());
    }
}

#[test]
fn upgrade_requires_a_bound_receipt_and_applies_only_after_replay() {
    let fixture = Fixture::new(&added_source("approval_nonce = 0"));
    let plan = fixture.plan();
    assert_eq!(plan.schema, "cellscript-upgrade-plan-v2");
    assert_eq!(plan.schema_reviews.len(), 1);
    assert!(plan.schema_reviews[0].plan.state_migration_required);
    assert_eq!(plan.apply_status, "blocked");
    let generic = plan.required_acknowledgements.iter().cloned().chain(["UPG3201".to_string(), "SACK1001".to_string()]).collect();
    assert!(apply_upgrade_plan(&plan, &generic).is_err());
    fixture.assert_unchanged();

    let reviewed = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&plan)).unwrap();
    assert_eq!(reviewed.apply_status, "ready", "{:?}", reviewed.diagnostics);
    apply_upgrade_plan(&reviewed, &BTreeSet::new()).unwrap();
    assert_eq!(std::fs::read_to_string(fixture.package.join("Cell.lock")).unwrap(), reviewed.locks[0].new_lock_content);
    assert_eq!(std::fs::read(fixture.baseline.join("Cell.lock")).unwrap(), fixture.old_lock);
    assert!(reviewed.migration_artifacts_required.iter().any(|artifact| artifact == "token:state-migration-plan"));
    assert!(!reviewed.performs_deployment);
}

#[test]
fn an_implicit_new_field_blocks_both_receipt_creation_and_lock_apply() {
    let fixture = Fixture::new(&added_source(""));
    let plan = fixture.plan();
    assert!(plan.diagnostics.iter().any(|diagnostic| diagnostic.code == "SACK1001" && !diagnostic.acknowledgeable));
    assert!(acknowledge_schema_change(&plan.schema_reviews[0].plan, "maintainer", "reviewed").is_err());
    let acknowledgements = plan.diagnostics.iter().map(|diagnostic| diagnostic.code.clone()).collect();
    assert!(apply_upgrade_plan(&plan, &acknowledgements).is_err());
    fixture.assert_unchanged();
}

#[test]
fn missing_old_sources_are_never_a_first_baseline_or_generic_acknowledgement() {
    let fixture = Fixture::new(&added_source("approval_nonce = 0"));
    let plan = create_upgrade_plan(&fixture.package, &UpgradeOptions { offline: true, ..UpgradeOptions::default() }).unwrap();
    assert!(plan.diagnostics.iter().any(|diagnostic| diagnostic.code == "UPG3001" && !diagnostic.acknowledgeable));
    assert!(plan.schema_reviews.is_empty());
    assert!(apply_upgrade_plan(&plan, &["UPG3001".to_string()].into_iter().collect()).is_err());
    fixture.assert_unchanged();
}

#[test]
fn receipts_cannot_move_to_a_different_member_environment_or_assignment() {
    let fixture = Fixture::new(&added_source("approval_nonce = 0"));
    let plan = fixture.plan();
    let options = fixture.reviewed_options(&plan);
    for environment in [None, Some("other-chain".to_string())] {
        let mut wrong = options.clone();
        if environment.is_none() {
            wrong.schema_acknowledgements[0].subject = "unrelated".to_string();
        }
        wrong.schema_acknowledgements[0].environment = environment;
        assert!(create_upgrade_plan(&fixture.package, &wrong).is_err());
    }
    let mut duplicate = options.clone();
    duplicate.schema_acknowledgements.push(duplicate.schema_acknowledgements[0].clone());
    assert!(create_upgrade_plan(&fixture.package, &duplicate).is_err());
    std::fs::write(fixture.package.join("src/main.cell"), added_source("approval_nonce = 1")).unwrap();
    assert!(create_upgrade_plan(&fixture.package, &options).is_err());
    fixture.assert_unchanged();
}

#[test]
fn apply_rechecks_candidate_and_old_baseline_before_writing_any_lock() {
    for mutate_baseline in [false, true] {
        let fixture = Fixture::new(&added_source("approval_nonce = 0"));
        let plan = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&fixture.plan())).unwrap();
        let root = if mutate_baseline { &fixture.baseline } else { &fixture.package };
        let source = std::fs::read_to_string(root.join("src/main.cell")).unwrap();
        std::fs::write(root.join("src/main.cell"), format!("{source}\n// changed after review\n")).unwrap();
        let error = apply_upgrade_plan(&plan, &BTreeSet::new()).unwrap_err().to_string();
        assert!(error.contains("source") && error.contains("changed"), "{error}");
        fixture.assert_unchanged();
    }
}

#[test]
fn deleting_review_records_and_rehashing_the_plan_does_not_remove_coverage() {
    let fixture = Fixture::new(&added_source("approval_nonce = 0"));
    let mut plan = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&fixture.plan())).unwrap();
    plan.schema_reviews.clear();
    rehash(&mut plan);
    let error = apply_upgrade_plan(&plan, &BTreeSet::new()).unwrap_err().to_string();
    assert!(error.contains("schema review coverage"), "{error}");
    fixture.assert_unchanged();
}

#[test]
fn unchanged_schema_needs_no_receipt_but_retains_complete_review_coverage() {
    let fixture = Fixture::new(OLD);
    let plan = fixture.plan();
    assert_eq!(plan.schema_reviews.len(), 1);
    assert!(!plan.schema_reviews[0].plan.requires_acknowledgement);
    assert!(plan.schema_reviews[0].acknowledgement.is_none());
    apply_upgrade_plan(&plan, &plan.required_acknowledgements.iter().cloned().collect()).unwrap();
}

fn compile(source: &str) -> CompileResult {
    cellscript::compile(source, CompileOptions::default()).unwrap_or_else(|error| panic!("{error}\n{source}"))
}

fn selector() -> SchemaRelationSelector {
    SchemaRelationSelector { action: "update".to_string(), before: "token".to_string(), after: "next".to_string() }
}

#[test]
fn branch_family_reviews_every_occurrence_and_binds_the_branch_condition() {
    let branched = |added: bool, second: &str| {
        let source = if added { added_source("approval_nonce = 0") } else { OLD.to_string() };
        let start = source.find("    replace token -> next").unwrap();
        let body = source[start..source.rfind('}').unwrap()].to_string();
        let other = body.replace("approval_nonce = 0", second);
        source[..start].to_string() + "    if token.amount > 0 {\n" + &body + "    } else {\n" + &other + "    }\n}\n"
    };
    let old = compile(&branched(false, ""));
    let good = compile(&branched(true, "approval_nonce = 0"));
    let plan = build_schema_change_plan(&old, &good, selector()).unwrap();
    assert_eq!(plan.old_occurrences.len(), 2);
    assert_eq!(plan.new_occurrences.len(), 2);
    assert_ne!(plan.new_occurrences[0].path, plan.new_occurrences[1].path);
    let receipt = acknowledge_schema_change(&plan, "maintainer", "both branches reset").unwrap();
    let changed = compile(&branched(true, "approval_nonce = 0").replace("amount > 0", "amount > 1"));
    let changed_plan = build_schema_change_plan(&old, &changed, selector()).unwrap();
    assert!(verify_schema_acknowledgement(&changed_plan, &receipt).is_err());
    let implicit = compile(&branched(true, ""));
    let implicit_plan = build_schema_change_plan(&old, &implicit, selector()).unwrap();
    assert!(implicit_plan.blockers.iter().any(|blocker| blocker.code == "SACK1001"));
}

#[test]
fn relation_removal_and_renaming_do_not_erase_the_old_schema_expansion() {
    let old = compile(OLD);
    let renamed = compile(&added_source("approval_nonce = 0").replace("action update", "action changed"));
    let plans = build_schema_change_plans(&old, &renamed).unwrap();
    assert_eq!(plans.len(), 2);
    assert!(plans.iter().all(|plan| plan.requires_acknowledgement));
    assert!(plans.iter().any(|plan| plan.old_occurrences.is_empty()));
    assert!(plans.iter().any(|plan| plan.new_occurrences.is_empty()));
    for plan in plans {
        acknowledge_schema_change(&plan, "maintainer", "reviewed relation replacement").unwrap();
    }
}

#[test]
fn a_genuinely_new_resource_schema_is_the_only_first_schema_baseline() {
    let old = compile("module review::token;\naction harmless() -> u64 { 1 }\n");
    let new = compile(&added_source(""));
    let plans = build_schema_change_plans(&old, &new).unwrap();
    assert_eq!(plans.len(), 1);
    assert!(plans[0].old_resource_type.is_none());
    assert!(!plans[0].requires_acknowledgement);
    assert!(plans[0].blockers.is_empty());
}

#[test]
fn cli_creates_receipts_from_the_exact_upgrade_plan_review() {
    let fixture = Fixture::new(&added_source("approval_nonce = 0"));
    let plan = fixture.plan();
    let plan_path = fixture.package.join("upgrade.json");
    std::fs::write(&plan_path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args([
            "schema-ack",
            "--upgrade-plan",
            plan_path.to_str().unwrap(),
            "--subject",
            "token",
            "--action",
            "update",
            "--before",
            "token",
            "--after",
            "next",
            "--acknowledge-by",
            "maintainer",
            "--rationale",
            "nonce resets in the exact candidate",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{} {}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let receipt = serde_json::from_slice(&output.stdout).unwrap();
    verify_schema_acknowledgement(&plan.schema_reviews[0].plan, &receipt).unwrap();
    fixture.assert_unchanged();
}

fn rehash(plan: &mut UpgradePlan) {
    let mut value = serde_json::to_value(&*plan).unwrap();
    value["plan_hash"] = serde_json::Value::String(String::new());
    plan.plan_hash = format!("sha256:{}", hex::encode(Sha256::digest(serde_json::to_vec(&value).unwrap())));
}

#[test]
fn standalone_receipts_bind_helper_bodies_without_binding_source_whitespace() {
    let old = compile(&(OLD.to_string() + "\nfn reset() -> u64 { 0 }\n"));
    let source = added_source("approval_nonce = reset()") + "\nfn reset() -> u64 { 0 }\n";
    let candidate = compile(&source);
    let plan = build_schema_change_plan(&old, &candidate, selector()).unwrap();
    let receipt = acknowledge_schema_change(&plan, "maintainer", "helper resets nonce").unwrap();
    let changed = compile(&source.replace("fn reset() -> u64 { 0 }", "fn reset() -> u64 { 1 }"));
    let changed_plan = build_schema_change_plan(&old, &changed, selector()).unwrap();
    assert_ne!(changed_plan.new_source_identity, plan.new_source_identity);
    assert!(verify_schema_acknowledgement(&changed_plan, &receipt).is_err());
    let formatted = compile(&cellscript::fmt::format_default(&candidate.ast).unwrap());
    let formatted_plan = build_schema_change_plan(&old, &formatted, selector()).unwrap();
    assert_eq!(formatted_plan.plan_hash, plan.plan_hash);
}

fn package(root: &Path, name: &str, source: &str, dependency: bool) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    let mut manifest = format!("[package]\nname = \"{name}\"\nversion = \"1.0.0\"\nedition = \"2027\"\ncellscript_version = \">=0.33.0-alpha.1\"\nentry = \"src/main.cell\"\n");
    if dependency {
        manifest.push_str("\n[dependencies.library]\npath = \"../library\"\n");
    }
    std::fs::write(root.join("Cell.toml"), manifest).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
}

fn lock(root: &Path) {
    let result = Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(root).arg("lock").output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}

fn snapshot_package(from: &Path, to: &Path) {
    std::fs::create_dir_all(to.join("src")).unwrap();
    for relative in ["Cell.toml", "src/main.cell", "Cell.lock"] {
        if from.join(relative).exists() {
            std::fs::copy(from.join(relative), to.join(relative)).unwrap();
        }
    }
}

#[test]
fn unused_dependency_owned_actions_use_the_complete_old_snapshot_and_deduplicate_across_roots() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Cell.toml"), "[workspace]\nmembers = [\"first\", \"second\"]\n").unwrap();
    let library = root.join("library");
    package(&library, "library", OLD, false);
    let mut options = UpgradeOptions { offline: true, ..UpgradeOptions::default() };
    for name in ["first", "second"] {
        let app = root.join(name);
        package(&app, name, &format!("module {name};\naction run() -> u64 {{ 1 }}\n"), true);
        lock(&app);
        let baseline = directory.path().join("snapshot").join(name);
        snapshot_package(&app, &baseline);
        options.schema_baselines.insert(name.to_string(), baseline);
    }
    snapshot_package(&library, &directory.path().join("snapshot/library"));
    std::fs::write(library.join("src/main.cell"), added_source("approval_nonce = 0")).unwrap();
    let plan = create_upgrade_plan(&root, &options).unwrap();
    assert_eq!(plan.dependency_schema_impacts.len(), 1, "{:?}", plan.diagnostics);
    assert_eq!(plan.dependency_schema_impacts[0].coordinate, "library");
    assert_eq!(plan.dependency_schema_impacts[0].owners.len(), 2);
    assert_eq!(plan.schema_reviews.len(), 1);
    assert_eq!(plan.schema_reviews[0].subject, plan.dependency_schema_impacts[0].id);
    assert!(plan.schema_reviews[0].plan.requires_acknowledgement);
    assert!(plan.schema_reviews[0].plan.context.as_ref().unwrap().subject.starts_with("dependency:"));
    options.acknowledgements = plan.required_acknowledgements.iter().cloned().collect();
    options.schema_acknowledgements = plan
        .schema_reviews
        .iter()
        .map(|review| UpgradeSchemaAcknowledgement {
            subject: review.subject.clone(),
            environment: review.environment.clone(),
            receipt: acknowledge_schema_change(&review.plan, "maintainer", "reviewed dependency-owned reset").unwrap(),
        })
        .collect();
    let ready = create_upgrade_plan(&root, &options).unwrap();
    assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
    apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
    for transaction in &ready.locks {
        assert_eq!(std::fs::read_to_string(root.join(&transaction.lock_path)).unwrap(), transaction.new_lock_content);
    }
}

#[test]
fn dependency_review_covers_library_only_and_non_entry_modules() {
    for secondary_module in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let library = directory.path().join("library");
        let app = directory.path().join("app");
        package(&library, "library", OLD, false);
        let manifest = std::fs::read_to_string(library.join("Cell.toml")).unwrap().replace("entry = \"src/main.cell\"\n", "");
        std::fs::write(library.join("Cell.toml"), manifest).unwrap();
        let relation_file = if secondary_module { "src/relations.cell" } else { "src/lib.cell" };
        std::fs::rename(library.join("src/main.cell"), library.join(relation_file)).unwrap();
        if secondary_module {
            std::fs::write(library.join("src/lib.cell"), "module library;\npublic fn value() -> u64 { 1 }\n").unwrap();
        }
        package(&app, "app", "module app;\naction run() -> u64 { 1 }\n", true);
        lock(&app);
        let baseline = directory.path().join("snapshot/app");
        snapshot_package(&app, &baseline);
        let old_library = directory.path().join("snapshot/library");
        snapshot_package(&library, &old_library);
        for source in ["src/lib.cell", "src/relations.cell"] {
            if library.join(source).exists() {
                std::fs::copy(library.join(source), old_library.join(source)).unwrap();
            }
        }
        std::fs::write(library.join(relation_file), added_source("approval_nonce = 0")).unwrap();
        let mut options = UpgradeOptions {
            offline: true,
            schema_baselines: [("app".to_string(), baseline)].into_iter().collect(),
            ..UpgradeOptions::default()
        };
        let plan = create_upgrade_plan(&app, &options).unwrap();
        assert_eq!(plan.dependency_schema_impacts.len(), if secondary_module { 2 } else { 1 }, "{:?}", plan.diagnostics);
        assert_eq!(plan.schema_reviews.len(), 1, "{:?}", plan.diagnostics);
        assert!(plan.schema_reviews[0].plan.requires_acknowledgement);
        assert!(plan.schema_reviews[0].plan.state_migration_required);
        options.acknowledgements = plan.required_acknowledgements.iter().cloned().collect();
        options.schema_acknowledgements = plan
            .schema_reviews
            .iter()
            .map(|review| UpgradeSchemaAcknowledgement {
                subject: review.subject.clone(),
                environment: review.environment.clone(),
                receipt: acknowledge_schema_change(&review.plan, "maintainer", "review all library-owned modules").unwrap(),
            })
            .collect();
        let ready = create_upgrade_plan(&app, &options).unwrap();
        assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
        apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
    }
}

#[test]
fn losing_one_old_dependency_source_hard_blocks_graph_review() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let app = directory.path().join("app");
    package(&library, "library", OLD, false);
    package(&app, "app", "module app;\naction run() -> u64 { 1 }\n", true);
    lock(&app);
    let snapshot = directory.path().join("snapshot/app");
    snapshot_package(&app, &snapshot);
    std::fs::write(library.join("src/main.cell"), added_source("approval_nonce = 0")).unwrap();
    let options = UpgradeOptions {
        offline: true,
        schema_baselines: [("app".to_string(), snapshot)].into_iter().collect(),
        ..UpgradeOptions::default()
    };
    let plan = create_upgrade_plan(&app, &options).unwrap();
    assert!(plan.diagnostics.iter().any(|diagnostic| diagnostic.code == "UPG3001" && !diagnostic.acknowledgeable));
    assert_eq!(plan.apply_status, "blocked");
}

#[test]
fn imported_generic_layout_changes_are_visible_through_a_same_except_field() {
    for (payload, added_paths) in [
        ("Box<u64>", vec!["payload.nonce"]),
        ("(Box<u64>, Box<u32>)", vec!["payload.0.nonce", "payload.1.nonce"]),
        ("[Box<u64>; 2]", vec!["payload[].nonce"]),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let library = directory.path().join("library");
        let app = directory.path().join("app");
        package(&library, "library", "module library;\npublic struct Box<T: fixed_value> { value: T, }\n", false);
        let source = format!("module app;\nuse library::Box;\nresource Token has store, replace, relock {{ payload: {payload}, }}\naction update(input token: Token) -> next: Token {{ replace token -> next {{ data = same except {{ }}; lock = same; capacity = same; type_script = same; }} }}\n");
        package(&app, "app", &source, true);
        lock(&app);
        let old = cellscript::compile_path_with_entry_action(
            camino::Utf8Path::from_path(&app).unwrap(),
            CompileOptions::default(),
            "update",
        )
        .unwrap();
        std::fs::write(
            library.join("src/main.cell"),
            "module library;\npublic struct Box<T: fixed_value> { value: T, nonce: u64, }\n",
        )
        .unwrap();
        lock(&app);
        let new = cellscript::compile_path_with_entry_action(
            camino::Utf8Path::from_path(&app).unwrap(),
            CompileOptions::default(),
            "update",
        )
        .unwrap();
        let plan = build_schema_change_plan(&old, &new, selector()).unwrap();
        assert_ne!(plan.old_schema_identity, plan.new_schema_identity, "{payload}");
        assert!(plan.requires_acknowledgement, "{payload}");
        for path in added_paths {
            assert!(
                plan.field_changes.iter().any(|field| field.field == path && field.classification == "added"),
                "{payload}: {path}"
            );
            assert!(plan.blockers.iter().any(|blocker| blocker.code == "SACK1001" && blocker.field == path), "{payload}: {path}");
        }
    }
}

#[test]
fn imported_resource_aliases_and_other_cell_schema_kinds_cannot_disappear_from_review() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let app = directory.path().join("app");
    package(&library, "library", "module library;\npublic resource Token has store, replace, relock { amount: u64, }\n", false);
    let source = "module app;\nuse library::Token as Asset;\naction update(input token: Asset) -> next: Asset { replace token -> next { data = same except { }; lock = same; capacity = same; type_script = same; } }\n";
    package(&app, "app", source, true);
    lock(&app);
    let old =
        cellscript::compile_path_with_entry_action(camino::Utf8Path::from_path(&app).unwrap(), CompileOptions::default(), "update")
            .unwrap();
    std::fs::write(
        library.join("src/main.cell"),
        "module library;\npublic resource Token has store, replace, relock { amount: u64, nonce: u64, }\n",
    )
    .unwrap();
    std::fs::write(app.join("src/main.cell"), source.replace("same except { }", "same except { nonce = 0 }")).unwrap();
    lock(&app);
    let new =
        cellscript::compile_path_with_entry_action(camino::Utf8Path::from_path(&app).unwrap(), CompileOptions::default(), "update")
            .unwrap();
    let plan = build_schema_change_plan(&old, &new, selector()).unwrap();
    assert!(plan.requires_acknowledgement);
    assert!(plan.blockers.is_empty());
    for kind in ["shared", "receipt"] {
        let old_source = OLD.replace("resource Token", &format!("{kind} Token"));
        let new_source = added_source("approval_nonce = 0").replace("resource Token", &format!("{kind} Token"));
        let old = compile(&old_source);
        let new = compile(&new_source);
        let plan = build_schema_change_plan(&old, &new, selector()).unwrap();
        assert!(plan.requires_acknowledgement, "{kind}");
    }
}

#[test]
fn identical_source_changes_in_another_workspace_need_their_own_graph_bound_review() {
    let first = Fixture::new(&added_source("approval_nonce = 0"));
    let second = Fixture::new(&added_source("approval_nonce = 0"));
    let first_plan = first.plan();
    let mut options = second.options();
    options.schema_acknowledgements = first.reviewed_options(&first_plan).schema_acknowledgements;
    let error = create_upgrade_plan(&second.package, &options).unwrap_err().to_string();
    assert!(error.contains("stale") || error.contains("different"), "{error}");
    first.assert_unchanged();
    second.assert_unchanged();
}

#[test]
fn nested_relation_values_keep_the_inner_schema_review_and_its_blockers() {
    let source = r#"
module nested_schema_review;
resource Token has store, replace, relock { amount: u64, enabled: bool, }
type_script TokenUpdate on type_group<Token> {
    action update(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        output next_left: Token from group_output[0],
        output next_right: Token from group_output[1],
    ) {
        replace left -> next_left {
            data = same except {
                enabled = {
                    replace right -> next_right {
                        data = same except { };
                        lock = same;
                        type_script = same;
                        capacity = same;
                    }
                    true
                },
            };
            lock = same;
            type_script = same;
            capacity = same;
        }
    }
}
"#;
    let old = compile(source);
    let changed =
        source.replace("enabled: bool, }", "enabled: bool, nonce: u64, }").replace("enabled = {", "nonce = 0,\n enabled = {");
    let new = compile(&changed);
    let plans = build_schema_change_plans(&old, &new).unwrap();
    assert_eq!(plans.len(), 2);
    let outer = plans.iter().find(|plan| plan.selector.before == "left").unwrap();
    let inner = plans.iter().find(|plan| plan.selector.before == "right").unwrap();
    assert!(outer.blockers.is_empty());
    assert!(inner.blockers.iter().any(|blocker| blocker.field == "nonce" && blocker.code == "SACK1001"));
    assert_ne!(outer.new_occurrences[0].path, inner.new_occurrences[0].path);
}

fn environment_fixture(old: &[&str], new: &[&str], source: &str) -> Fixture {
    let mut fixture = Fixture::new(OLD);
    let manifest = std::fs::read_to_string(fixture.package.join("Cell.toml")).unwrap();
    let with_environments = |names: &[&str]| {
        let mut manifest = manifest.clone();
        for name in names {
            manifest
                .push_str(&format!("\n[environments.{name}]\nchain_id = \"test-chain\"\ngenesis_hash = \"0x{}\"\n", "11".repeat(32)));
        }
        manifest
    };
    std::fs::write(fixture.package.join("Cell.toml"), with_environments(old)).unwrap();
    lock(&fixture.package);
    snapshot_package(&fixture.package, &fixture.baseline);
    fixture.old_lock = std::fs::read(fixture.package.join("Cell.lock")).unwrap();
    std::fs::write(fixture.package.join("Cell.toml"), with_environments(new)).unwrap();
    std::fs::write(fixture.package.join("src/main.cell"), source).unwrap();
    fixture
}

#[test]
fn adding_an_environment_does_not_hide_schema_changes_in_existing_scopes() {
    use cellscript::package::upgrade::UpgradeEnvironmentTransition;
    let fixture = environment_fixture(&["production"], &["production", "staging"], &added_source("approval_nonce = 0"));
    let mut options = fixture.options();
    options.environment = Some("staging".to_string());
    let plan = create_upgrade_plan(&fixture.package, &options).unwrap();
    assert_eq!(plan.reverse_dependents.len(), 3, "{:?}", plan.diagnostics);
    let introduced = plan.reverse_dependents.iter().find(|impact| impact.environment.as_deref() == Some("staging")).unwrap();
    assert_eq!(introduced.environment_transition, UpgradeEnvironmentTransition::Introduced);
    assert_eq!(introduced.old.status, "absent-new-environment");
    assert_eq!(introduced.new.status, "compiled");
    let existing = plan.schema_reviews.iter().filter(|review| review.environment.as_deref() != Some("staging")).collect::<Vec<_>>();
    assert_eq!(existing.len(), 2);
    assert!(existing.iter().all(|review| review.plan.requires_acknowledgement));
    assert!(
        !plan
            .schema_reviews
            .iter()
            .find(|review| review.environment.as_deref() == Some("staging"))
            .unwrap()
            .plan
            .requires_acknowledgement
    );
    options.acknowledgements = plan.required_acknowledgements.iter().cloned().collect();
    options.schema_acknowledgements = fixture.reviewed_options(&plan).schema_acknowledgements;
    let ready = create_upgrade_plan(&fixture.package, &options).unwrap();
    assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
    apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
}

#[test]
fn removed_and_renamed_environments_keep_retirement_reviews_until_exact_acknowledgement() {
    use cellscript::package::upgrade::UpgradeEnvironmentTransition;
    for new in [vec![], vec!["renamed"]] {
        let fixture = environment_fixture(&["production"], &new, OLD);
        let plan = fixture.plan();
        let retired = plan.reverse_dependents.iter().find(|impact| impact.environment.as_deref() == Some("production")).unwrap();
        assert_eq!(retired.environment_transition, UpgradeEnvironmentTransition::Retired);
        assert_eq!(retired.old.status, "compiled");
        assert_eq!(retired.new.status, "absent-retired-environment");
        assert!(retired.dimensions.iter().all(|dimension| dimension.classification == "breaking"));
        let review = plan.schema_reviews.iter().find(|review| review.environment.as_deref() == Some("production")).unwrap();
        assert!(review.plan.requires_acknowledgement);
        assert!(!review.plan.old_occurrences.is_empty());
        assert!(review.plan.new_occurrences.is_empty());
        let ready = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&plan)).unwrap();
        assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
        assert!(!ready.locks[0].new_lock.environments.contains_key("production"));
        apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
    }
}

#[test]
fn deleting_a_retired_scope_from_a_rehashed_plan_cannot_bypass_its_review() {
    let fixture = environment_fixture(&["production"], &["renamed"], OLD);
    let original = fixture.plan();
    let mut plan = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&original)).unwrap();
    plan.reverse_dependents.retain(|impact| impact.environment.as_deref() != Some("production"));
    plan.schema_reviews.retain(|review| review.environment.as_deref() != Some("production"));
    rehash(&mut plan);
    let error = apply_upgrade_plan(&plan, &BTreeSet::new()).unwrap_err().to_string();
    assert!(error.contains("environment union coverage"), "{error}");
    fixture.assert_unchanged();
}

#[test]
fn build_unit_cache_inputs_bind_the_final_lock_after_primary_build_evidence_is_added() {
    use cellscript::package::{inspection, DependencyScope, ResolutionOptions};
    let fixture = Fixture::new(OLD);
    let plan = fixture.plan();
    assert!(plan.locks[0].old_lock.package_build.is_none());
    assert!(plan.locks[0].new_lock.package_build.is_some());
    let resolution =
        ResolutionOptions { scope: DependencyScope::Test, all_features: true, offline: true, ..ResolutionOptions::default() };
    // Compare with a physical final lock, independently of the in-memory
    // override path used by the producer. Restore the private fixture afterward.
    std::fs::write(fixture.package.join("Cell.lock"), &plan.locks[0].new_lock_content).unwrap();
    let expected = cellscript::package::with_resolution_options(resolution.clone(), || {
        let graph = inspection::resolve_graph(&fixture.package, None, &resolution)?;
        inspection::build_plan(&graph, &inspection::BuildPlanOptions::default())
    })
    .unwrap()
    .units
    .remove(0);
    std::fs::write(fixture.package.join("Cell.lock"), &fixture.old_lock).unwrap();
    let stored = plan.reverse_dependents[0].new_build_unit.as_ref().unwrap();
    assert_eq!(stored.id, expected.id);
    assert_eq!(stored.cache.source_set_hash, expected.cache.source_set_hash);
    assert_eq!(stored.cache.cache_key, expected.cache.cache_key);
    let mut tampered = plan;
    tampered.reverse_dependents[0].new_build_unit.as_mut().unwrap().cache.source_set_hash = "unbound".to_string();
    rehash(&mut tampered);
    let error = apply_upgrade_plan(&tampered, &BTreeSet::new()).unwrap_err().to_string();
    assert!(error.contains("build-unit inputs"), "{error}");
    fixture.assert_unchanged();
}

#[test]
fn apply_keeps_requested_online_provenance_while_replaying_only_locked_local_inputs() {
    let fixture = Fixture::new(OLD);
    let mut options = fixture.options();
    options.offline = false;
    let plan = create_upgrade_plan(&fixture.package, &options).unwrap();
    assert!(!plan.selection.offline);
    assert_eq!(plan.apply_status, "ready");
    apply_upgrade_plan(&plan, &BTreeSet::new()).unwrap();
    assert_eq!(std::fs::read_to_string(fixture.package.join("Cell.lock")).unwrap(), plan.locks[0].new_lock_content);
}

#[test]
fn environment_rename_reviews_dependency_owned_retirement_and_new_profile_separately() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("library");
    let app = directory.path().join("app");
    package(&library, "library", OLD, false);
    package(&app, "app", "module app;\naction run() -> u64 { 1 }\n", true);
    let environment =
        |name: &str| format!("\n[environments.{name}]\nchain_id = \"test-chain\"\ngenesis_hash = \"0x{}\"\n", "11".repeat(32));
    let library_manifest = std::fs::read_to_string(library.join("Cell.toml")).unwrap() + &environment("production");
    std::fs::write(library.join("Cell.toml"), library_manifest).unwrap();
    let app_manifest = std::fs::read_to_string(app.join("Cell.toml")).unwrap();
    std::fs::write(app.join("Cell.toml"), app_manifest.clone() + &environment("production")).unwrap();
    lock(&app);
    let baseline = directory.path().join("snapshot/app");
    snapshot_package(&app, &baseline);
    snapshot_package(&library, &directory.path().join("snapshot/library"));
    std::fs::write(app.join("Cell.toml"), app_manifest + &environment("staging")).unwrap();
    let mut options = UpgradeOptions {
        offline: true,
        schema_baselines: [("app".to_string(), baseline)].into_iter().collect(),
        ..UpgradeOptions::default()
    };
    let plan = create_upgrade_plan(&app, &options).unwrap();
    assert_eq!(plan.dependency_schema_impacts.len(), 2, "{:?}", plan.diagnostics);
    assert!(plan.dependency_schema_impacts.iter().any(|impact| impact.old.is_some() && impact.new.is_none()));
    assert!(plan.dependency_schema_impacts.iter().any(|impact| impact.old.is_none() && impact.new.is_some()));
    let retired = plan.schema_reviews.iter().find(|review| review.plan.requires_acknowledgement).unwrap();
    assert!(retired.subject.starts_with("dependency:"));
    assert!(!retired.plan.old_occurrences.is_empty());
    assert!(retired.plan.new_occurrences.is_empty());
    options.acknowledgements = plan.required_acknowledgements.iter().cloned().collect();
    options.schema_acknowledgements = plan
        .schema_reviews
        .iter()
        .filter(|review| review.plan.requires_acknowledgement)
        .map(|review| UpgradeSchemaAcknowledgement {
            subject: review.subject.clone(),
            environment: review.environment.clone(),
            receipt: acknowledge_schema_change(&review.plan, "maintainer", "review dependency profile retirement").unwrap(),
        })
        .collect();
    let ready = create_upgrade_plan(&app, &options).unwrap();
    assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
    apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
}

#[test]
fn module_namespace_changes_retire_the_old_endpoint_without_matching_new_nominal_ownership() {
    let fixture = Fixture::new(&OLD.replace("module review::token;", "module review::renamed;"));
    let plan = fixture.plan();
    assert_eq!(plan.schema_reviews.len(), 2, "{:?}", plan.diagnostics);
    let retired = plan.schema_reviews.iter().find(|review| review.plan.module == "review::token").unwrap();
    let fresh = plan.schema_reviews.iter().find(|review| review.plan.module == "review::renamed").unwrap();
    assert!(retired.plan.requires_acknowledgement && retired.plan.state_migration_required);
    assert!(!retired.plan.old_occurrences.is_empty() && retired.plan.new_occurrences.is_empty());
    assert!(!fresh.plan.requires_acknowledgement);
    assert!(fresh.plan.old_occurrences.is_empty() && !fresh.plan.new_occurrences.is_empty());
    for review in [&retired, &fresh] {
        assert_eq!(review.plan.old_module.as_deref(), Some("review::token"));
        assert_eq!(review.plan.new_module.as_deref(), Some("review::renamed"));
        assert!(review.plan.old_source_identity.is_some() && review.plan.new_source_identity.is_some());
    }
    assert_eq!(retired.plan.context, fresh.plan.context);
    assert!(apply_upgrade_plan(&plan, &plan.required_acknowledgements.iter().cloned().collect()).is_err());
    fixture.assert_unchanged();
    let path = fixture.package.join("namespace-review.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_cellc"))
        .args([
            "schema-ack",
            "--upgrade-plan",
            path.to_str().unwrap(),
            "--subject",
            "token",
            "--module",
            "review::token",
            "--action",
            "update",
            "--before",
            "token",
            "--after",
            "next",
            "--acknowledge-by",
            "maintainer",
            "--rationale",
            "retire the old namespace; no nominal continuity is inferred",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{} {}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    verify_schema_acknowledgement(&retired.plan, &serde_json::from_slice(&result.stdout).unwrap()).unwrap();
    let ready = create_upgrade_plan(&fixture.package, &fixture.reviewed_options(&plan)).unwrap();
    assert_eq!(ready.apply_status, "ready", "{:?}", ready.diagnostics);
    apply_upgrade_plan(&ready, &BTreeSet::new()).unwrap();
}

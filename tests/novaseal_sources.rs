use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use camino::Utf8PathBuf;
use cellscript::{compile_file_with_entry_action, compile_file_with_entry_lock, ArtifactFormat, CompileOptions};

const EXPECTED_TRACKED_NOVASEAL_CELL_SOURCES: &[&str] = &[
    "proposals/novaseal/agreement-profile-v0/harness/ckb_vm/always_success_lock.cell",
    "proposals/novaseal/agreement-profile-v0/src/nova_agreement_lifecycle_type.cell",
    "proposals/novaseal/agreement-profile-v0/src/nova_agreement_receipt_type.cell",
    "proposals/novaseal/agreement-profile-v0/src/nova_agreement_type.cell",
    "proposals/novaseal/btc-transaction-commitment-profile-v0/src/nova_btc_transaction_commitment_type.cell",
    "proposals/novaseal/btc-utxo-seal-profile-v0/src/nova_btc_utxo_seal_type.cell",
    "proposals/novaseal/dual-seal-profile-v0/src/nova_dual_seal_type.cell",
    "proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell",
    "proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell",
    "proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell",
    "proposals/novaseal/v0-mvp-skeleton/src/nova_btc_authority_lock.cell",
    "proposals/novaseal/v0-mvp-skeleton/src/nova_receipt_type.cell",
    "proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell",
    "proposals/novaseal/v0-mvp-skeleton/src/nova_state_type.cell",
];

const EXPECTED_EXECUTABLE_ENTRY_COUNT: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EntryKind {
    Action,
    Lock,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ExecutableEntry {
    path: String,
    kind: EntryKind,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ManifestEntryRef {
    manifest: String,
    path: String,
    action: Option<String>,
}

fn repo_root() -> Utf8PathBuf {
    Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_path(path: &str) -> Utf8PathBuf {
    repo_root().join(path)
}

fn tracked_novaseal_cell_sources() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "proposals/novaseal/**/*.cell"])
        .current_dir(repo_root())
        .output()
        .expect("git should be available for tracked NovaSeal source audit");
    assert!(output.status.success(), "git ls-files should succeed: {}", String::from_utf8_lossy(&output.stderr));

    let mut files =
        String::from_utf8(output.stdout).expect("git output should be utf-8").lines().map(str::to_owned).collect::<Vec<_>>();
    files.sort();
    files
}

fn tracked_novaseal_manifest_paths() -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "proposals/novaseal/*/Cell.toml"])
        .current_dir(repo_root())
        .output()
        .expect("git should be available for tracked NovaSeal manifest audit");
    assert!(output.status.success(), "git ls-files should succeed: {}", String::from_utf8_lossy(&output.stderr));

    let mut files =
        String::from_utf8(output.stdout).expect("git output should be utf-8").lines().map(str::to_owned).collect::<Vec<_>>();
    files.sort();
    files
}

fn declaration_name(line: &str, keyword: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(keyword)?.trim_start();
    let name = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_').collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn declared_executable_entries(path: &str) -> Vec<ExecutableEntry> {
    let source = std::fs::read_to_string(repo_path(path)).unwrap_or_else(|err| panic!("failed to read NovaSeal source {path}: {err}"));
    let mut entries = Vec::new();
    for line in source.lines() {
        if let Some(name) = declaration_name(line, "action ") {
            entries.push(ExecutableEntry { path: path.to_string(), kind: EntryKind::Action, name });
        }
        if let Some(name) = declaration_name(line, "lock ") {
            entries.push(ExecutableEntry { path: path.to_string(), kind: EntryKind::Lock, name });
        }
    }
    entries
}

fn all_declared_executable_entries() -> Vec<ExecutableEntry> {
    let mut entries =
        tracked_novaseal_cell_sources().into_iter().flat_map(|path| declared_executable_entries(&path)).collect::<Vec<_>>();
    entries.sort();
    entries
}

fn parse_manifest(manifest_path: &str) -> toml::Value {
    let manifest = std::fs::read_to_string(repo_path(manifest_path))
        .unwrap_or_else(|err| panic!("failed to read NovaSeal manifest {manifest_path}: {err}"));
    toml::from_str(&manifest).unwrap_or_else(|err| panic!("failed to parse {manifest_path}: {err}"))
}

fn manifest_package_root(manifest_path: &str) -> &str {
    manifest_path.strip_suffix("/Cell.toml").unwrap_or_else(|| panic!("NovaSeal manifest should end in /Cell.toml: {manifest_path}"))
}

fn manifest_entry_refs(manifest_path: &str) -> Vec<ManifestEntryRef> {
    let manifest = parse_manifest(manifest_path);
    let package_root = manifest_package_root(manifest_path);
    let metadata = manifest
        .get("metadata")
        .and_then(toml::Value::as_table)
        .unwrap_or_else(|| panic!("NovaSeal manifest should have [metadata]: {manifest_path}"));
    let mut refs = Vec::new();

    if let Some(entry) = manifest.get("package").and_then(|package| package.get("entry")).and_then(toml::Value::as_str) {
        refs.push(ManifestEntryRef { manifest: manifest_path.to_string(), path: format!("{package_root}/{entry}"), action: None });
    }

    for field in ["source_actions", "stateful_dispatcher"] {
        if let Some(raw) = metadata.get(field).and_then(toml::Value::as_str) {
            for item in raw.split(';').filter(|item| !item.trim().is_empty()) {
                let (path, action) = item
                    .trim()
                    .split_once(':')
                    .unwrap_or_else(|| panic!("{manifest_path} metadata.{field} entry should be file:action: {item}"));
                refs.push(ManifestEntryRef {
                    manifest: manifest_path.to_string(),
                    path: format!("{package_root}/{path}"),
                    action: Some(action.to_string()),
                });
            }
        }
    }

    refs.sort();
    refs.dedup();
    refs
}

fn declared_entries_by_file_and_name() -> BTreeMap<(String, String), BTreeSet<EntryKind>> {
    let mut entries = BTreeMap::<(String, String), BTreeSet<EntryKind>>::new();
    for entry in all_declared_executable_entries() {
        entries.entry((entry.path, entry.name)).or_default().insert(entry.kind);
    }
    entries
}

fn compile_options() -> CompileOptions {
    CompileOptions { target: Some("riscv64-asm".to_string()), target_profile: Some("ckb".to_string()), ..CompileOptions::default() }
}

#[test]
fn tracked_novaseal_cell_sources_are_complete() {
    let expected = EXPECTED_TRACKED_NOVASEAL_CELL_SOURCES.iter().map(|path| path.to_string()).collect::<Vec<_>>();
    assert_eq!(
        tracked_novaseal_cell_sources(),
        expected,
        "tracked NovaSeal .cell source set changed; update the executable-source audit deliberately"
    );
}

#[test]
fn novaseal_manifests_reference_tracked_source_entries() {
    let tracked_sources = tracked_novaseal_cell_sources().into_iter().collect::<BTreeSet<_>>();
    let declared_entries = declared_entries_by_file_and_name();
    let mut refs = Vec::new();

    for manifest in tracked_novaseal_manifest_paths() {
        refs.extend(manifest_entry_refs(&manifest));
    }

    assert!(!refs.is_empty(), "NovaSeal manifests should declare source entries");
    for entry_ref in refs {
        assert!(
            tracked_sources.contains(&entry_ref.path),
            "{} references untracked or missing NovaSeal source {}",
            entry_ref.manifest,
            entry_ref.path
        );

        if let Some(action) = entry_ref.action {
            assert!(
                declared_entries.contains_key(&(entry_ref.path.clone(), action.clone())),
                "{} references undeclared action {} in {}",
                entry_ref.manifest,
                action,
                entry_ref.path
            );
        }
    }
}

#[test]
fn all_novaseal_executable_entries_compile_for_ckb_profile() {
    let entries = all_declared_executable_entries();
    assert_eq!(
        entries.len(),
        EXPECTED_EXECUTABLE_ENTRY_COUNT,
        "NovaSeal executable entry set changed; update compile coverage deliberately"
    );

    for entry in entries {
        let path = repo_path(&entry.path);
        let result = match entry.kind {
            EntryKind::Action => compile_file_with_entry_action(&path, compile_options(), &entry.name),
            EntryKind::Lock => compile_file_with_entry_lock(&path, compile_options(), &entry.name),
        }
        .unwrap_or_else(|err| panic!("{} {} in {} should compile: {}", entry.kind.label(), entry.name, entry.path, err.message));

        assert_eq!(result.artifact_format, ArtifactFormat::RiscvAssembly);
        assert!(
            !result.artifact_bytes.is_empty(),
            "{} {} in {} should emit non-empty assembly",
            entry.kind.label(),
            entry.name,
            entry.path
        );
        assert_eq!(result.metadata.constraints.target_profile, "ckb");
        assert!(
            result.metadata.constraints.ckb.is_some(),
            "{} {} in {} should expose CKB constraints",
            entry.kind.label(),
            entry.name,
            entry.path
        );
    }
}

impl EntryKind {
    fn label(self) -> &'static str {
        match self {
            EntryKind::Action => "action",
            EntryKind::Lock => "lock",
        }
    }
}

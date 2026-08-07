//! NovaSeal runtime-verifier artifact and source pinning checks.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::crypto::{ckb_blake2b256, hex0x, sha256_hex};

fn git_files(cwd: &Path, pattern: &str) -> Result<Vec<String>> {
    let output = Command::new("git").args(["ls-files", pattern]).current_dir(cwd).output()?;
    if !output.status.success() {
        bail!("git ls-files failed in {}: {}", cwd.display(), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().filter(|line| !line.is_empty()).map(str::to_owned).collect())
}

fn collect_tree_files(
    root: &Path,
    directory: &Path,
    allowed_extensions: &[&str],
    allowed_names: &[&str],
    label: &str,
    files: &mut BTreeSet<PathBuf>,
    failures: &mut Vec<String>,
) -> Result<()> {
    let mut entries = fs::read_dir(directory)?.collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if metadata.file_type().is_symlink() {
            failures.push(format!("{relative} is a symlink inside the NovaSeal {label} source tree"));
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name();
            if ["target", "build", ".git"].iter().any(|skip| name == *skip) {
                continue;
            }
            collect_tree_files(root, &path, allowed_extensions, allowed_names, label, files, failures)?;
        } else if metadata.is_file()
            && (path.extension().and_then(|value| value.to_str()).is_some_and(|extension| allowed_extensions.contains(&extension))
                || entry.file_name().to_str().is_some_and(|name| allowed_names.contains(&name)))
        {
            files.insert(path);
        }
    }
    Ok(())
}

fn hash_files(root: &Path, files: impl IntoIterator<Item = PathBuf>) -> Result<String> {
    let mut digest = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        digest.update(relative.as_bytes());
        digest.update([0]);
        digest.update(Sha256::digest(fs::read(path)?));
    }
    Ok(format!("0x{}", hex::encode(digest.finalize())))
}

fn verifier_source_tree_hash(root: &Path, core_root: &Path, failures: &mut Vec<String>) -> Result<String> {
    let mut files = BTreeSet::new();
    for directory in [
        core_root.join("verifier/novaseal_btc_verifier_core"),
        core_root.join("verifier/novaseal_btc_verifier_riscv"),
        core_root.join("verifier/novaseal_btc_verifier"),
    ] {
        collect_tree_files(
            root,
            &directory,
            &["rs", "sh"],
            &["Cargo.toml", "Cargo.lock", "README.md"],
            "verifier TCB",
            &mut files,
            failures,
        )?;
    }
    hash_files(root, files)
}

fn profile_source_tree_hash(root: &Path, paths: &[&str], failures: &mut Vec<String>) -> Result<String> {
    let mut files = BTreeSet::new();
    for raw in paths {
        let path = root.join(raw);
        let metadata = fs::symlink_metadata(&path).with_context(|| format!("failed to inspect {}", path.display()))?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        if metadata.file_type().is_symlink() {
            failures.push(format!("{relative} is a symlink inside the NovaSeal profile source tree"));
        } else if metadata.is_file() {
            files.insert(path);
        } else if metadata.is_dir() {
            collect_tree_files(
                root,
                &path,
                &["cell", "schema", "toml", "py", "json", "rs"],
                &["Cargo.lock"],
                "profile",
                &mut files,
                failures,
            )?;
        }
    }
    hash_files(root, files)
}

fn load_json(path: &Path) -> Result<Value> {
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("failed to decode {}", path.display()))
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

pub fn run(root: &Path) -> Result<i32> {
    let core_root = root.join("proposals/novaseal/v0-mvp-skeleton");
    let release_elf =
        core_root.join("verifier/novaseal_btc_verifier_riscv/target/riscv64imac-unknown-none-elf/release/novaseal_btc_verifier_riscv");
    if !release_elf.is_file() {
        bail!("missing NovaSeal RISC-V verifier release ELF: {}", release_elf.display());
    }
    let artifact = fs::read(&release_elf)?;
    let artifact_hash = format!("0x{}", sha256_hex(&artifact));
    let data_hash = hex0x(&ckb_blake2b256(&artifact)?);
    let size_bytes = artifact.len();
    let mut failures = Vec::new();

    let mut manifests = BTreeSet::new();
    for tracked in git_files(root, "proposals/novaseal/**/Cell.toml")? {
        manifests.insert(root.join(tracked));
    }
    let novaseal_root = root.join("proposals/novaseal");
    if novaseal_root.is_dir() {
        for tracked in git_files(&novaseal_root, "**/Cell.toml")? {
            manifests.insert(novaseal_root.join(tracked));
        }
    }
    if manifests.is_empty() {
        failures.push("no tracked NovaSeal Cell.toml manifests found".to_owned());
    }
    for manifest_path in manifests {
        let manifest: toml::Value = toml::from_str(&fs::read_to_string(&manifest_path)?)?;
        let dependencies = manifest
            .get("deploy")
            .and_then(|value| value.get("ckb"))
            .and_then(|value| value.get("cell_deps"))
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let runtime_dependencies = dependencies
            .iter()
            .filter(|dependency| {
                dependency.get("role").and_then(toml::Value::as_str) == Some("runtime_verifier")
                    || dependency.get("name").and_then(toml::Value::as_str) == Some("cellscript_btc_bip340_verifier_riscv")
            })
            .collect::<Vec<_>>();
        if runtime_dependencies.is_empty() {
            failures.push(format!("{} has no NovaSeal runtime verifier CellDep", relative(root, &manifest_path)));
            continue;
        }
        for (index, dependency) in runtime_dependencies.iter().enumerate() {
            let actual_data = dependency.get("data_hash").and_then(toml::Value::as_str);
            if actual_data != Some(&data_hash) {
                failures.push(format!(
                    "{} runtime verifier #{index} data_hash {} != {data_hash}",
                    relative(root, &manifest_path),
                    actual_data.unwrap_or("None")
                ));
            }
            let actual_artifact = dependency.get("artifact_hash").and_then(toml::Value::as_str);
            if actual_artifact != Some(&artifact_hash) {
                failures.push(format!(
                    "{} runtime verifier #{index} artifact_hash {} != {artifact_hash}",
                    relative(root, &manifest_path),
                    actual_artifact.unwrap_or("None")
                ));
            }
        }
    }

    let source_tree_hash = verifier_source_tree_hash(root, &core_root, &mut failures)?;
    let public_template_path = core_root.join("proofs/public_shared_cell_dep_attestation.template.json");
    let public_template = load_json(&public_template_path)?;
    let public_hash = public_template.pointer("/runtime_verifier/artifact_hash").and_then(Value::as_str);
    if public_hash != Some(&artifact_hash) {
        failures.push(format!(
            "{} runtime_verifier.artifact_hash {} != {artifact_hash}",
            relative(root, &public_template_path),
            public_hash.unwrap_or("None")
        ));
    }
    let external_template_path = core_root.join("proofs/bip340_external_tcb_review_attestation.template.json");
    let external_template = load_json(&external_template_path)?;
    if external_template.get("artifact_hash").and_then(Value::as_str) != Some(&artifact_hash) {
        failures.push(format!(
            "{} artifact_hash {} != {artifact_hash}",
            relative(root, &external_template_path),
            external_template.get("artifact_hash").and_then(Value::as_str).unwrap_or("None")
        ));
    }
    if external_template.get("source_tree_sha256").and_then(Value::as_str) != Some(&source_tree_hash) {
        failures.push(format!(
            "{} source_tree_sha256 {} != {source_tree_hash}",
            relative(root, &external_template_path),
            external_template.get("source_tree_sha256").and_then(Value::as_str).unwrap_or("None")
        ));
    }

    let rwa_source_tree_hash = profile_source_tree_hash(
        root,
        &[
            "proposals/novaseal/rwa-receipt-profile-v0/Cell.toml",
            "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell",
            "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
            "proposals/novaseal/rwa-receipt-profile-v0/schemas",
            "proposals/novaseal/rwa-receipt-profile-v0/fixtures",
            "proposals/novaseal/rwa-receipt-profile-v0/proofs/invariant_matrix.json",
        ],
        &mut failures,
    )?;
    let rwa_template_path = root.join("proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.template.json");
    let rwa_template = load_json(&rwa_template_path)?;
    if rwa_template.get("profile_source_tree_sha256").and_then(Value::as_str) != Some(&rwa_source_tree_hash) {
        failures.push(format!(
            "{} profile_source_tree_sha256 {} != {rwa_source_tree_hash}",
            relative(root, &rwa_template_path),
            rwa_template.get("profile_source_tree_sha256").and_then(Value::as_str).unwrap_or("None")
        ));
    }

    let mapping_path = core_root.join("proofs/proofplan_mapping.json");
    let mapping = load_json(&mapping_path)?;
    let summary = mapping.pointer("/btc_verifier_riscv_shell_artifact/current_summary").unwrap_or(&Value::Null);
    if summary.get("staged_release_elf_sha256").and_then(Value::as_str) != artifact_hash.strip_prefix("0x") {
        failures.push(format!(
            "{} staged_release_elf_sha256 {} != {}",
            relative(root, &mapping_path),
            summary.get("staged_release_elf_sha256").and_then(Value::as_str).unwrap_or("None"),
            artifact_hash.strip_prefix("0x").unwrap_or(&artifact_hash)
        ));
    }
    if summary.get("staged_release_elf_size_bytes").and_then(Value::as_u64) != Some(size_bytes as u64) {
        failures.push(format!(
            "{} staged_release_elf_size_bytes {:?} != {size_bytes}",
            relative(root, &mapping_path),
            summary.get("staged_release_elf_size_bytes").unwrap_or(&Value::Null)
        ));
    }

    if !failures.is_empty() {
        eprintln!("NovaSeal verifier pinning check failed:");
        for failure in failures {
            eprintln!("  - {failure}");
        }
        return Ok(1);
    }
    println!(
        "NovaSeal verifier pinning check passed: artifact_hash={artifact_hash} data_hash={data_hash} \
source_tree_sha256={source_tree_hash} rwa_profile_source_tree_sha256={rwa_source_tree_hash} size_bytes={size_bytes}"
    );
    Ok(0)
}

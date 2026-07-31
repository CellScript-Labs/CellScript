//! Isolated source/build verifier used by the production Registry worker.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use camino::Utf8PathBuf;
use serde::Serialize;

const MAX_SNAPSHOT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug)]
struct Args {
    snapshot: PathBuf,
    namespace: String,
    name: String,
    version: String,
    source_hash: String,
    manifest_hash: String,
    compatibility_profile_hash: String,
}

#[derive(Serialize)]
struct VerificationOutput {
    status: &'static str,
    artifact_hash: String,
    metadata_hash: String,
    compiler_version: String,
    source_hash: String,
    manifest_hash: String,
    compatibility_profile_hash: String,
    artifact_format: String,
}

#[derive(Serialize)]
struct FailureOutput<'a> {
    status: &'static str,
    error_code: &'static str,
    message: &'a str,
}

fn main() -> ExitCode {
    match run() {
        Ok(output) => {
            if let Err(error) = serde_json::to_writer(std::io::stdout(), &output) {
                eprintln!("failed to serialize verifier output: {error}");
                return ExitCode::from(70);
            }
            println!();
            ExitCode::SUCCESS
        }
        Err(error) => {
            let message = error.to_string();
            let output = FailureOutput { status: "failed", error_code: "verification_failed", message: &message };
            let _ = serde_json::to_writer(std::io::stdout(), &output);
            println!();
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<VerificationOutput> {
    let args = parse_args()?;
    verify(args)
}

fn verify(args: Args) -> Result<VerificationOutput> {
    let metadata =
        fs::metadata(&args.snapshot).with_context(|| format!("failed to inspect source snapshot '{}'", args.snapshot.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SNAPSHOT_BYTES {
        bail!("source snapshot must be a non-empty regular file no larger than {MAX_SNAPSHOT_BYTES} bytes");
    }
    let snapshot =
        fs::read(&args.snapshot).with_context(|| format!("failed to read source snapshot '{}'", args.snapshot.display()))?;

    let work = unique_work_dir()?;
    let _cleanup = Cleanup(work.clone());
    cellscript::package::registry::materialize_generated_source_snapshot_bytes(
        &snapshot,
        &work,
        &args.namespace,
        &args.name,
        &args.version,
        &args.source_hash,
    )
    .context("source snapshot authentication failed")?;

    let package_manager = cellscript::package::PackageManager::new(&work);
    let manifest = package_manager.read_manifest().context("failed to read materialized Cell.toml")?;
    if manifest.package.namespace.as_deref() != Some(args.namespace.as_str())
        || manifest.package.name != args.name
        || manifest.package.version != args.version
    {
        bail!("materialized package identity does not match the verification job");
    }
    let manifest_hash = cellscript::package::registry::compute_package_manifest_hash(&manifest)
        .context("failed to compute canonical package manifest hash")?;
    require_matching_hash("manifest_hash", &manifest_hash, &args.manifest_hash)?;

    let compile_root = Utf8PathBuf::from_path_buf(work.clone())
        .map_err(|path| anyhow::anyhow!("verification work path is not valid UTF-8: {}", path.display()))?;
    let result = cellscript::compile_path(&compile_root, cellscript::CompileOptions::default())
        .context("CellScript package compilation failed")?;
    let compatibility_profile_bytes =
        serde_json::to_vec(&result.metadata.compatibility_profile).context("failed to serialize compatibility profile")?;
    let compatibility_profile_hash = hex::encode(cellscript::ckb_blake2b256(&compatibility_profile_bytes));
    require_matching_hash("compatibility_profile_hash", &compatibility_profile_hash, &args.compatibility_profile_hash)?;

    let artifact_hash = result.metadata.artifact_hash.clone().unwrap_or_else(|| hex::encode(result.artifact_hash));
    let metadata_bytes = serde_json::to_vec(&result.metadata).context("failed to serialize compile metadata")?;
    let metadata_hash = hex::encode(cellscript::ckb_blake2b256(&metadata_bytes));

    Ok(VerificationOutput {
        status: "passed",
        artifact_hash,
        metadata_hash,
        compiler_version: result.metadata.compiler_version,
        source_hash: args.source_hash,
        manifest_hash: args.manifest_hash,
        compatibility_profile_hash: args.compatibility_profile_hash,
        artifact_format: result.artifact_format.display_name().to_string(),
    })
}

fn parse_args() -> Result<Args> {
    let mut values = BTreeMap::new();
    let mut arguments = env::args().skip(1);
    while let Some(flag) = arguments.next() {
        if !flag.starts_with("--") {
            bail!("unexpected positional argument '{flag}'");
        }
        let value = arguments.next().with_context(|| format!("missing value for '{flag}'"))?;
        if values.insert(flag.clone(), value).is_some() {
            bail!("duplicate argument '{flag}'");
        }
    }
    let mut take = |name: &str| values.remove(name).with_context(|| format!("missing required argument '{name}'"));
    let args = Args {
        snapshot: PathBuf::from(take("--snapshot")?),
        namespace: take("--namespace")?,
        name: take("--name")?,
        version: take("--version")?,
        source_hash: take("--source-hash")?,
        manifest_hash: take("--manifest-hash")?,
        compatibility_profile_hash: take("--compatibility-profile-hash")?,
    };
    if let Some((unknown, _)) = values.into_iter().next() {
        bail!("unknown argument '{unknown}'");
    }
    Ok(args)
}

fn require_matching_hash(field: &str, actual: &str, expected: &str) -> Result<()> {
    let normalize = |value: &str| value.strip_prefix("0x").unwrap_or(value).to_ascii_lowercase();
    let actual = normalize(actual);
    let expected = normalize(expected);
    if actual.len() != 64 || expected.len() != 64 || actual != expected {
        bail!("{field} mismatch: compiled/materialized value does not match the signed Registry identity");
    }
    Ok(())
}

fn unique_work_dir() -> Result<PathBuf> {
    let root = env::temp_dir();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).context("system clock is before the Unix epoch")?.as_nanos();
    for attempt in 0..100_u32 {
        let candidate = root.join(format!("cellscript-registry-verify-{}-{timestamp}-{attempt}", std::process::id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    bail!("failed to allocate a unique verifier work directory")
}

struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.0.starts_with(env::temp_dir())
            && self.0.file_name().is_some_and(|name| name.to_string_lossy().starts_with("cellscript-registry-verify-"))
        {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use serde_json::json;

    use super::*;

    #[test]
    fn verifies_generated_snapshot_with_the_real_compiler() {
        let source_root = tempfile::tempdir().unwrap();
        fs::create_dir_all(source_root.path().join("src")).unwrap();
        fs::write(
            source_root.path().join("Cell.toml"),
            r#"[package]
edition = "2026"
name = "demo"
version = "1.2.3"
namespace = "cellscript"
entry = "src/main.cell"
"#,
        )
        .unwrap();
        fs::write(
            source_root.path().join("src/main.cell"),
            r#"module demo::main

action identity(value: u64) -> u64 {
    verification
        value
}
"#,
        )
        .unwrap();

        let source_hash = cellscript::package::registry::compute_source_hash(source_root.path()).unwrap();
        let manager = cellscript::package::PackageManager::new(source_root.path());
        let manifest = manager.read_manifest().unwrap();
        let manifest_hash = cellscript::package::registry::compute_package_manifest_hash(&manifest).unwrap();
        let compile_root = Utf8PathBuf::from_path_buf(source_root.path().to_path_buf()).unwrap();
        let result = cellscript::compile_path(&compile_root, cellscript::CompileOptions::default()).unwrap();
        let compatibility_profile_hash =
            hex::encode(cellscript::ckb_blake2b256(&serde_json::to_vec(&result.metadata.compatibility_profile).unwrap()));

        let mut files = Vec::new();
        for relative in ["Cell.toml", "src/main.cell"] {
            let content = fs::read(source_root.path().join(relative)).unwrap();
            files.push(json!({
                "path": relative,
                "blake2b256": hex::encode(cellscript::ckb_blake2b256(&content)),
                "content_base64": base64::engine::general_purpose::STANDARD.encode(content),
            }));
        }
        let snapshot = json!({
            "schema": "cellscript-source-snapshot-v1",
            "generated_by": cellscript::VERSION,
            "package": { "namespace": "cellscript", "name": "demo", "version": "1.2.3" },
            "files": files,
        });
        let snapshot_path = source_root.path().join("snapshot.json");
        fs::write(&snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();

        let output = verify(Args {
            snapshot: snapshot_path,
            namespace: "cellscript".to_string(),
            name: "demo".to_string(),
            version: "1.2.3".to_string(),
            source_hash: source_hash.clone(),
            manifest_hash: manifest_hash.clone(),
            compatibility_profile_hash: compatibility_profile_hash.clone(),
        })
        .unwrap();
        assert_eq!(output.status, "passed");
        assert_eq!(output.source_hash, source_hash);
        assert_eq!(output.manifest_hash, manifest_hash);
        assert_eq!(output.compatibility_profile_hash, compatibility_profile_hash);
        assert_eq!(output.artifact_hash.len(), 64);
        assert_eq!(output.metadata_hash.len(), 64);
    }
}

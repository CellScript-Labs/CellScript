//! Isolated source/build verifier used by the production Registry worker.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

const MAX_SNAPSHOT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug)]
struct Args {
    snapshot: PathBuf,
    namespace: String,
    name: String,
    version: String,
    source_hash: String,
    manifest_hash: String,
    profile: String,
    compatibility_profile_hash: Option<String>,
    artifact_hash: Option<String>,
    abi_hash: Option<String>,
    build_recipe_hash: Option<String>,
}

#[derive(Debug, Serialize)]
struct VerificationOutput {
    status: &'static str,
    verification_level: &'static str,
    artifact_hash: Option<String>,
    metadata_hash: String,
    compiler_version: Option<String>,
    source_hash: String,
    manifest_hash: String,
    compatibility_profile_hash: Option<String>,
    artifact_format: String,
}

#[derive(Debug, Deserialize)]
struct ArtifactBundle {
    schema: String,
    namespace: String,
    name: String,
    release: String,
    profile: String,
    manifest_json: String,
    objects: Vec<ArtifactBundleObject>,
}

#[derive(Debug, Deserialize)]
struct ArtifactBundleObject {
    role: String,
    content_base64: String,
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

    match args.profile.as_str() {
        "cellscript_source" => verify_cellscript_source(args, &snapshot),
        "ckb_executable" | "reproducible_build" | "copy_material" => verify_artifact_bundle(args, &snapshot),
        profile => bail!("unsupported artifact profile '{profile}'"),
    }
}

fn verify_cellscript_source(args: Args, snapshot: &[u8]) -> Result<VerificationOutput> {
    let compatibility_profile_expected =
        args.compatibility_profile_hash.as_deref().context("cellscript_source requires --compatibility-profile-hash")?;

    let work = unique_work_dir()?;
    let _cleanup = Cleanup(work.clone());
    cellscript::package::registry::materialize_generated_source_snapshot_bytes(
        snapshot,
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
    require_matching_hash("compatibility_profile_hash", &compatibility_profile_hash, compatibility_profile_expected)?;

    let artifact_hash = result.metadata.artifact_hash.clone().unwrap_or_else(|| hex::encode(result.artifact_hash));
    let metadata_bytes = serde_json::to_vec(&result.metadata).context("failed to serialize compile metadata")?;
    let metadata_hash = hex::encode(cellscript::ckb_blake2b256(&metadata_bytes));

    Ok(VerificationOutput {
        status: "passed",
        verification_level: "compiled",
        artifact_hash: Some(artifact_hash),
        metadata_hash,
        compiler_version: Some(result.metadata.compiler_version),
        source_hash: args.source_hash,
        manifest_hash: args.manifest_hash,
        compatibility_profile_hash: args.compatibility_profile_hash,
        artifact_format: result.artifact_format.display_name().to_string(),
    })
}

fn verify_artifact_bundle(args: Args, snapshot: &[u8]) -> Result<VerificationOutput> {
    let bundle: ArtifactBundle = serde_json::from_slice(snapshot).context("artifact bundle must be valid JSON")?;
    if bundle.schema != "cellscript-registry-bundle" {
        bail!("artifact bundle schema must be 'cellscript-registry-bundle'");
    }
    if bundle.namespace != args.namespace
        || bundle.name != args.name
        || bundle.release != args.version
        || bundle.profile != args.profile
    {
        bail!("artifact bundle identity does not match the verification job");
    }
    let manifest_hash = hex::encode(cellscript::ckb_blake2b256(bundle.manifest_json.as_bytes()));
    require_matching_hash("manifest_hash", &manifest_hash, &args.manifest_hash)?;
    let source = bundle_object(&bundle, "source")?;
    let source_hash = hex::encode(cellscript::ckb_blake2b256(&source));
    require_matching_hash("source_hash", &source_hash, &args.source_hash)?;

    let (artifact_hash, artifact_format, verification_level) = match args.profile.as_str() {
        "ckb_executable" => {
            let executable = bundle_object(&bundle, "executable")?;
            let actual_artifact_hash = hex::encode(cellscript::ckb_blake2b256(&executable));
            require_matching_hash(
                "artifact_hash",
                &actual_artifact_hash,
                args.artifact_hash.as_deref().context("ckb_executable requires --artifact-hash")?,
            )?;
            let abi = bundle_object(&bundle, "abi")?;
            let actual_abi_hash = hex::encode(cellscript::ckb_blake2b256(&abi));
            require_matching_hash(
                "abi_hash",
                &actual_abi_hash,
                args.abi_hash.as_deref().context("ckb_executable requires --abi-hash")?,
            )?;
            (Some(actual_artifact_hash), "ckb-vm-executable", "hash_bound")
        }
        "reproducible_build" => {
            let executable = bundle_object(&bundle, "executable")?;
            let actual_artifact_hash = hex::encode(cellscript::ckb_blake2b256(&executable));
            require_matching_hash(
                "artifact_hash",
                &actual_artifact_hash,
                args.artifact_hash.as_deref().context("reproducible_build requires --artifact-hash")?,
            )?;
            let recipe = bundle_object(&bundle, "build_recipe")?;
            let actual_recipe_hash = hex::encode(cellscript::ckb_blake2b256(&recipe));
            require_matching_hash(
                "build_recipe_hash",
                &actual_recipe_hash,
                args.build_recipe_hash.as_deref().context("reproducible_build requires --build-recipe-hash")?,
            )?;
            (Some(actual_artifact_hash), "reproducible-binary", "evidence_required")
        }
        "copy_material" => (None, "copy-material", "hash_bound"),
        _ => unreachable!("profile was checked before bundle verification"),
    };
    let metadata_hash = hex::encode(cellscript::ckb_blake2b256(snapshot));
    Ok(VerificationOutput {
        status: "passed",
        verification_level,
        artifact_hash,
        metadata_hash,
        compiler_version: None,
        source_hash: args.source_hash,
        manifest_hash: args.manifest_hash,
        compatibility_profile_hash: None,
        artifact_format: artifact_format.to_string(),
    })
}

fn bundle_object(bundle: &ArtifactBundle, role: &str) -> Result<Vec<u8>> {
    let mut matching = bundle.objects.iter().filter(|object| object.role == role);
    let object = matching.next().with_context(|| format!("artifact bundle is missing required '{role}' object"))?;
    if matching.next().is_some() {
        bail!("artifact bundle contains more than one '{role}' object");
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&object.content_base64)
        .with_context(|| format!("artifact bundle '{role}' object is not valid base64"))?;
    if bytes.is_empty() {
        bail!("artifact bundle '{role}' object must not be empty");
    }
    Ok(bytes)
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
        profile: take("--profile")?,
        compatibility_profile_hash: values.remove("--compatibility-profile-hash"),
        artifact_hash: values.remove("--artifact-hash"),
        abi_hash: values.remove("--abi-hash"),
        build_recipe_hash: values.remove("--build-recipe-hash"),
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
            profile: "cellscript_source".to_string(),
            compatibility_profile_hash: Some(compatibility_profile_hash.clone()),
            artifact_hash: None,
            abi_hash: None,
            build_recipe_hash: None,
        })
        .unwrap();
        assert_eq!(output.status, "passed");
        assert_eq!(output.source_hash, source_hash);
        assert_eq!(output.manifest_hash, manifest_hash);
        assert_eq!(output.compatibility_profile_hash.as_deref(), Some(compatibility_profile_hash.as_str()));
        assert_eq!(output.artifact_hash.as_deref().unwrap().len(), 64);
        assert_eq!(output.metadata_hash.len(), 64);
    }

    #[test]
    fn hash_binds_ckb_executable_and_abi_bundle_objects() {
        let source = b"fn main() {}";
        let executable = b"ckb-vm-elf";
        let abi = br#"{"actions":[]}"#;
        let output = verify_bundle(
            "ckb_executable",
            &[("source", source), ("executable", executable), ("abi", abi)],
            Some(hex::encode(cellscript::ckb_blake2b256(executable))),
            Some(hex::encode(cellscript::ckb_blake2b256(abi))),
            None,
        )
        .unwrap();
        assert_eq!(output.status, "passed");
        assert_eq!(output.verification_level, "hash_bound");
        assert_eq!(output.artifact_format, "ckb-vm-executable");
    }

    #[test]
    fn distinguishes_reproducible_build_evidence_from_copy_material() {
        let executable = b"reproducible-output";
        let recipe = b"FROM rust:latest";
        let reproducible = verify_bundle(
            "reproducible_build",
            &[("source", b"source"), ("executable", executable), ("build_recipe", recipe)],
            Some(hex::encode(cellscript::ckb_blake2b256(executable))),
            None,
            Some(hex::encode(cellscript::ckb_blake2b256(recipe))),
        )
        .unwrap();
        assert_eq!(reproducible.verification_level, "evidence_required");
        assert_eq!(reproducible.artifact_format, "reproducible-binary");

        let copy = verify_bundle("copy_material", &[("source", b"starter")], None, None, None).unwrap();
        assert_eq!(copy.verification_level, "hash_bound");
        assert_eq!(copy.artifact_format, "copy-material");
        assert!(copy.artifact_hash.is_none());
    }

    #[test]
    fn rejects_executable_bundle_when_published_hash_does_not_match() {
        let error = verify_bundle(
            "ckb_executable",
            &[("source", b"source"), ("executable", b"elf"), ("abi", b"abi")],
            Some("11".repeat(32)),
            Some(hex::encode(cellscript::ckb_blake2b256(b"abi"))),
            None,
        )
        .unwrap_err();
        assert!(error.to_string().contains("artifact_hash mismatch"));
    }

    fn verify_bundle(
        profile: &str,
        objects: &[(&str, &[u8])],
        artifact_hash: Option<String>,
        abi_hash: Option<String>,
        build_recipe_hash: Option<String>,
    ) -> Result<VerificationOutput> {
        let root = tempfile::tempdir().unwrap();
        let manifest_json = r#"{"name":"demo"}"#;
        let bundle = json!({
            "schema": "cellscript-registry-bundle",
            "namespace": "cellscript",
            "name": "demo",
            "release": "1.2.3",
            "profile": profile,
            "manifest_json": manifest_json,
            "objects": objects.iter().map(|(role, bytes)| json!({
                "role": role,
                "content_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            })).collect::<Vec<_>>(),
        });
        let path = root.path().join("bundle.json");
        fs::write(&path, serde_json::to_vec(&bundle).unwrap()).unwrap();
        let source = objects.iter().find(|(role, _)| *role == "source").unwrap().1;
        verify(Args {
            snapshot: path,
            namespace: "cellscript".to_string(),
            name: "demo".to_string(),
            version: "1.2.3".to_string(),
            source_hash: hex::encode(cellscript::ckb_blake2b256(source)),
            manifest_hash: hex::encode(cellscript::ckb_blake2b256(manifest_json.as_bytes())),
            profile: profile.to_string(),
            compatibility_profile_hash: None,
            artifact_hash,
            abi_hash,
            build_recipe_hash,
        })
    }
}

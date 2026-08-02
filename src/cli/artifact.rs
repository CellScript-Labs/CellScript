use crate::error::{CompileError, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const MAX_REGISTRY_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_BUNDLE_BYTES: usize = 5 * 1024 * 1024;
const DEFAULT_CKB_MAINNET_RPC_URL: &str = "https://mainnet.ckb.dev/rpc";

#[derive(Debug)]
pub struct ArtifactArgs {
    pub operation: ArtifactOperation,
}

#[derive(Debug)]
pub enum ArtifactOperation {
    Fetch {
        coordinate: String,
        output: PathBuf,
        receipt: Option<PathBuf>,
        api_url: Option<String>,
        force: bool,
        json: bool,
    },
    Verify {
        bundle: PathBuf,
        receipt: PathBuf,
        json: bool,
    },
    Pin {
        coordinate: String,
        output: PathBuf,
        api_url: Option<String>,
        accept_hash_bound: bool,
        force: bool,
        json: bool,
    },
    Copy {
        coordinate: String,
        destination: PathBuf,
        api_url: Option<String>,
        accept_hash_bound: bool,
        json: bool,
    },
    CellDep {
        coordinate: String,
        output: PathBuf,
        api_url: Option<String>,
        rpc_url: Option<String>,
        accept_hash_bound: bool,
        force: bool,
        json: bool,
    },
    RecordDeployment {
        coordinate: String,
        code_hash: String,
        hash_type: String,
        dep_type: String,
        tx_hash: String,
        index: u32,
        capability_key_id: String,
        capability_signature: Option<String>,
        api_url: Option<String>,
        print_payload: bool,
        json: bool,
    },
    SetAvailability {
        coordinate: String,
        status: String,
        reason: Option<String>,
        capability_key_id: String,
        capability_signature: Option<String>,
        api_url: Option<String>,
        print_payload: bool,
        json: bool,
    },
    Commitment {
        coordinate: String,
        output: PathBuf,
        api_url: Option<String>,
        force: bool,
        json: bool,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
struct ArtifactBundleObject {
    role: String,
    content_base64: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FetchReceipt {
    schema: String,
    coordinate: String,
    registry_origin: String,
    artifact: Value,
    release: Value,
    bundle_sha256: String,
    bundle_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFileMap {
    schema: String,
    files: Vec<TemplateFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateFile {
    path: String,
    content_base64: String,
    blake2b256: String,
}

struct Coordinate {
    namespace: String,
    name: String,
    release: String,
}

struct FetchedArtifact {
    coordinate: Coordinate,
    registry_origin: String,
    artifact: Value,
    release: Value,
    bundle_url: String,
    bundle: Vec<u8>,
}

struct VerifiedBundle {
    profile_contract: Value,
    source: Vec<u8>,
    object_hashes: BTreeMap<String, String>,
}

pub fn execute(args: ArtifactArgs) -> Result<()> {
    match args.operation {
        ArtifactOperation::Fetch { coordinate, output, receipt, api_url, force, json } => {
            let fetched = fetch(&coordinate, api_url.as_deref())?;
            let verified = verify_fetched(&fetched)?;
            write_bytes(&output, &fetched.bundle, force)?;
            let receipt_path = receipt.unwrap_or_else(|| PathBuf::from(format!("{}.receipt.json", output.display())));
            let receipt = receipt_for(&fetched);
            write_json(&receipt_path, &receipt, force)?;
            emit(
                json,
                json!({
                    "status": "fetched_and_verified",
                    "coordinate": coordinate,
                    "profile": fetched.artifact["profile"],
                    "verification_status": fetched.release["verification_status"],
                    "bundle": output,
                    "receipt": receipt_path,
                    "objects": verified.object_hashes,
                }),
                format!("Fetched and verified {coordinate}\n  Bundle: {}\n  Receipt: {}", output.display(), receipt_path.display()),
            )
        }
        ArtifactOperation::Verify { bundle, receipt, json } => {
            let receipt: FetchReceipt = read_json(&receipt, "artifact fetch receipt")?;
            if receipt.schema != "cellscript-artifact-fetch-receipt-v1" {
                return Err(error("artifact fetch receipt schema is not supported"));
            }
            let bytes = read_limited(&bundle, MAX_BUNDLE_BYTES, "artifact bundle")?;
            require_sha256(&bytes, &receipt.bundle_sha256, "bundle_sha256")?;
            let coordinate = parse_coordinate(&receipt.coordinate)?;
            let fetched = FetchedArtifact {
                coordinate,
                registry_origin: receipt.registry_origin,
                artifact: receipt.artifact,
                release: receipt.release,
                bundle_url: receipt.bundle_url,
                bundle: bytes,
            };
            let verified = verify_fetched(&fetched)?;
            emit(
                json,
                json!({
                    "status": "verified",
                    "coordinate": receipt.coordinate,
                    "profile": fetched.artifact["profile"],
                    "verification_status": fetched.release["verification_status"],
                    "objects": verified.object_hashes,
                    "profile_contract": verified.profile_contract,
                }),
                format!("Verified {} with immutable bundle and profile-contract hashes", receipt.coordinate),
            )
        }
        ArtifactOperation::Pin { coordinate, output, api_url, accept_hash_bound, force, json } => {
            let fetched = fetch(&coordinate, api_url.as_deref())?;
            let verified = verify_fetched(&fetched)?;
            require_assurance(&fetched.release, accept_hash_bound)?;
            let pin = json!({
                "schema": "cellscript-artifact-lock-v1",
                "coordinate": coordinate,
                "registry_origin": fetched.registry_origin,
                "artifact": fetched.artifact,
                "release": fetched.release,
                "bundle_sha256": sha256_identity(&fetched.bundle),
                "bundle_url": fetched.bundle_url,
                "object_hashes": verified.object_hashes,
                "profile_contract": verified.profile_contract,
            });
            write_json(&output, &pin, force)?;
            emit(
                json,
                json!({ "status": "pinned", "coordinate": coordinate, "lockfile": output }),
                format!("Pinned {coordinate} to {}", output.display()),
            )
        }
        ArtifactOperation::Copy { coordinate, destination, api_url, accept_hash_bound, json } => {
            let fetched = fetch(&coordinate, api_url.as_deref())?;
            let verified = verify_fetched(&fetched)?;
            require_assurance(&fetched.release, accept_hash_bound)?;
            if fetched.artifact["kind"].as_str() != Some("template") {
                return Err(error("artifact copy only accepts kind=template"));
            }
            materialize_template(&verified.source, &verified.profile_contract, &destination)?;
            emit(
                json,
                json!({ "status": "copied", "coordinate": coordinate, "destination": destination }),
                format!("Copied {coordinate} into {}", destination.display()),
            )
        }
        ArtifactOperation::CellDep { coordinate, output, api_url, rpc_url, accept_hash_bound, force, json } => {
            let fetched = fetch(&coordinate, api_url.as_deref())?;
            let verified = verify_fetched(&fetched)?;
            require_assurance(&fetched.release, accept_hash_bound)?;
            if fetched.artifact["profile"].as_str() != Some("ckb_executable") {
                return Err(error("artifact cell-dep only accepts profile=ckb_executable"));
            }
            let deployed = chain_verified_deployment(&fetched.release)?;
            let release_identity = signed_release(&fetched.release)?;
            let evidence = object_field(deployed, "evidence", "deployed evidence")?;
            require_deployment_contract(&verified.profile_contract, evidence)?;
            let rpc_url = rpc_url
                .or_else(|| std::env::var(super::commands::CELLSCRIPT_CKB_RPC_URL_ENV).ok())
                .unwrap_or_else(|| DEFAULT_CKB_MAINNET_RPC_URL.to_string());
            revalidate_mainnet_deployment(evidence, &rpc_url)?;
            let descriptor = json!({
                "schema": "cellscript-registry-cell-dep-v1",
                "coordinate": coordinate,
                "artifact_hash": release_identity["artifact_hash"],
                "abi_hash": release_identity["abi_hash"],
                "profile_contract": verified.profile_contract,
                "cell_dep": {
                    "out_point": evidence["out_point"],
                    "dep_type": evidence["dep_type"],
                },
                "script": {
                    "code_hash": evidence["code_hash"],
                    "hash_type": evidence["hash_type"],
                },
                "chain_verification": "get_live_cell:fresh",
                "liveness_checked_at": super::commands::current_utc_timestamp(),
                "resolved_code_out_point": evidence.get("resolved_code_out_point").cloned().unwrap_or(Value::Null),
                "deployed_evidence_hash": deployed["evidence_hash"],
            });
            write_json(&output, &descriptor, force)?;
            emit(
                json,
                json!({ "status": "cell_dep_generated", "coordinate": coordinate, "output": output }),
                format!("Generated chain-verified CellDep descriptor at {}", output.display()),
            )
        }
        ArtifactOperation::RecordDeployment {
            coordinate,
            code_hash,
            hash_type,
            dep_type,
            tx_hash,
            index,
            capability_key_id,
            capability_signature,
            api_url,
            print_payload,
            json,
        } => record_deployment(
            &coordinate,
            &code_hash,
            &hash_type,
            &dep_type,
            &tx_hash,
            index,
            &capability_key_id,
            capability_signature.as_deref(),
            api_url,
            print_payload,
            json,
        ),
        ArtifactOperation::SetAvailability {
            coordinate,
            status,
            reason,
            capability_key_id,
            capability_signature,
            api_url,
            print_payload,
            json,
        } => set_availability(
            &coordinate,
            &status,
            reason.as_deref(),
            &capability_key_id,
            capability_signature.as_deref(),
            api_url,
            print_payload,
            json,
        ),
        ArtifactOperation::Commitment { coordinate, output, api_url, force, json } => {
            let fetched = fetch(&coordinate, api_url.as_deref())?;
            verify_fetched(&fetched)?;
            let deployed = chain_verified_deployment(&fetched.release)?;
            let release_identity = signed_release(&fetched.release)?;
            let deployed_evidence_hash = string_field(deployed, "evidence_hash", "deployed evidence")?;
            let payload = json!({
                "schema": "cellscript-registry-commitment-v1",
                "namespace": fetched.coordinate.namespace,
                "name": fetched.coordinate.name,
                "release": fetched.coordinate.release,
                "source_hash": fetched.release["source_hash"],
                "manifest_hash": fetched.release["manifest_hash"],
                "artifact_hash": release_identity.get("artifact_hash").cloned().unwrap_or(Value::Null),
                "deployed_evidence_hash": deployed_evidence_hash,
            });
            let canonical = canonical_json(&payload)?;
            let commitment_hash = format!("0x{}", hex::encode(crate::ckb_blake2b256(canonical.as_bytes())));
            let cell_data = format!("0x{}{}", hex::encode("CSREGv1"), commitment_hash.trim_start_matches("0x"));
            let commitment = json!({
                "schema": "cellscript-registry-commitment-builder-v1",
                "payload": payload,
                "commitment_hash": commitment_hash,
                "cell_data": cell_data,
                "required_type_index": true,
                "network": "mainnet",
            });
            write_json(&output, &commitment, force)?;
            emit(
                json,
                json!({ "status": "commitment_generated", "coordinate": coordinate, "output": output, "commitment_hash": commitment_hash }),
                format!("Generated mainnet Registry commitment at {}", output.display()),
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn record_deployment(
    coordinate: &str,
    code_hash: &str,
    hash_type: &str,
    dep_type: &str,
    tx_hash: &str,
    index: u32,
    capability_key_id: &str,
    capability_signature: Option<&str>,
    api_url: Option<String>,
    print_payload: bool,
    json_output: bool,
) -> Result<()> {
    if !matches!(hash_type, "data" | "data1" | "data2" | "type") {
        return Err(error("--hash-type must be data, data1, data2, or type"));
    }
    if !matches!(dep_type, "code" | "dep_group") {
        return Err(error("--dep-type must be code or dep_group"));
    }
    require_hash_shape(code_hash, "code_hash")?;
    require_hash_shape(tx_hash, "tx_hash")?;
    let api_base = super::commands::resolve_registry_api_base(api_url)?;
    let registry_origin = super::commands::registry_origin_from_api_base(&api_base)?;
    let fetched = fetch(coordinate, Some(&api_base))?;
    let verified = verify_fetched(&fetched)?;
    if fetched.artifact["profile"].as_str() != Some("ckb_executable") {
        return Err(error("deployment evidence is valid only for profile=ckb_executable"));
    }
    require_deployment_contract_values(&verified.profile_contract, hash_type, dep_type)?;
    let release = signed_release(&fetched.release)?;
    let artifact_hash = map_string_field(release, "artifact_hash", "signed release")?;
    if hash_type != "type" {
        require_ckb_hash(code_hash, artifact_hash, "code_hash")?;
    }
    let issued_at = super::commands::current_utc_timestamp();
    let expires_at = super::commands::utc_timestamp_after_seconds(10 * 60);
    let nonce_material =
        format!("cellscript-registry-deployment\n{registry_origin}\n{coordinate}\n{artifact_hash}\n{tx_hash}\n{index}\n{issued_at}");
    let payload = json!({
        "protocol": "cellscript-registry-deployment",
        "action": "record_deployment",
        "registry_origin": registry_origin,
        "namespace": fetched.coordinate.namespace,
        "name": fetched.coordinate.name,
        "release": fetched.coordinate.release,
        "network": "mainnet",
        "artifact_hash": artifact_hash,
        "data_hash": artifact_hash,
        "code_hash": code_hash,
        "hash_type": hash_type,
        "dep_type": dep_type,
        "out_point": { "tx_hash": tx_hash, "index": index },
        "capability_key_id": capability_key_id,
        "nonce": format!("0x{}", hex::encode(crate::ckb_blake2b256(nonce_material.as_bytes()))),
        "issued_at": issued_at,
        "expires_at": expires_at,
        "cli_version": crate::VERSION,
    });
    let canonical = canonical_json(&payload)?;
    let endpoint = format!(
        "{}/v1/artifacts/{}/{}/releases/{}/deployments",
        api_base, fetched.coordinate.namespace, fetched.coordinate.name, fetched.coordinate.release
    );
    if print_payload {
        return emit(
            json_output,
            json!({ "endpoint": endpoint, "payload": payload, "canonical_payload": canonical }),
            format!("{canonical}\n\nEndpoint: {endpoint}"),
        );
    }
    let signature = match capability_signature {
        Some(value) => value.to_string(),
        None => super::commands::sign_registry_capability_payload(capability_key_id, &canonical)?,
    };
    let response = super::commands::registry_http_client()?
        .post(&endpoint)
        .json(&json!({
            "payload": payload,
            "capability_signature": { "algorithm": "p256-sha256", "signature": signature }
        }))
        .send()
        .map_err(|err| error(format!("failed to submit deployment evidence to '{endpoint}': {err}")))?;
    let status = response.status();
    let body = response.text().map_err(|err| error(format!("failed to read deployment response: {err}")))?;
    if !status.is_success() {
        return Err(error(format!("deployment evidence request failed with HTTP {status}: {}", body.trim())));
    }
    let response_json = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "response": body }));
    emit(json_output, response_json, format!("Recorded and chain-verified mainnet deployment for {coordinate}"))
}

#[allow(clippy::too_many_arguments)]
fn set_availability(
    coordinate: &str,
    status: &str,
    reason: Option<&str>,
    capability_key_id: &str,
    capability_signature: Option<&str>,
    api_url: Option<String>,
    print_payload: bool,
    json_output: bool,
) -> Result<()> {
    if !matches!(status, "active" | "deprecated" | "yanked") {
        return Err(error("--status must be active, deprecated, or yanked"));
    }
    let reason = reason.map(str::trim).filter(|value| !value.is_empty());
    if status == "yanked" && reason.is_none() {
        return Err(error("--reason is required when yanking a release"));
    }
    if reason.is_some_and(|value| value.len() > 500) {
        return Err(error("--reason must be no longer than 500 characters"));
    }
    if capability_key_id.len() != 36
        || !capability_key_id.starts_with("cap_")
        || !capability_key_id[4..].bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error("--capability-key-id must be a Registry capability key id"));
    }
    let coordinate = parse_coordinate(coordinate)?;
    let api_base = super::commands::resolve_registry_api_base(api_url)?;
    let registry_origin = super::commands::registry_origin_from_api_base(&api_base)?;
    let issued_at = super::commands::current_utc_timestamp();
    let expires_at = super::commands::utc_timestamp_after_seconds(10 * 60);
    let nonce_material = format!(
        "cellscript-registry-availability-v1\n{registry_origin}\n{}/{}/{}\n{status}\n{}\n{issued_at}",
        coordinate.namespace,
        coordinate.name,
        coordinate.release,
        reason.unwrap_or("")
    );
    let mut payload = json!({
        "protocol": "cellscript-registry-availability-v1",
        "action": "set_availability",
        "registry_origin": registry_origin,
        "namespace": coordinate.namespace,
        "name": coordinate.name,
        "release": coordinate.release,
        "availability_status": status,
        "capability_key_id": capability_key_id,
        "nonce": format!("0x{}", hex::encode(crate::ckb_blake2b256(nonce_material.as_bytes()))),
        "issued_at": issued_at,
        "expires_at": expires_at,
        "cli_version": crate::VERSION,
    });
    if let Some(reason) = reason {
        payload["reason"] = Value::String(reason.to_string());
    }
    let canonical = canonical_json(&payload)?;
    let endpoint =
        format!("{}/v1/artifacts/{}/{}/releases/{}/availability", api_base, coordinate.namespace, coordinate.name, coordinate.release);
    if print_payload {
        return emit(
            json_output,
            json!({ "endpoint": endpoint, "payload": payload, "canonical_payload": canonical }),
            format!("{canonical}\n\nEndpoint: {endpoint}"),
        );
    }
    let signature = match capability_signature {
        Some(value) => value.to_string(),
        None => super::commands::sign_registry_capability_payload(capability_key_id, &canonical)?,
    };
    let response = super::commands::registry_http_client()?
        .post(&endpoint)
        .json(&json!({
            "payload": payload,
            "capability_signature": { "algorithm": "p256-sha256", "signature": signature }
        }))
        .send()
        .map_err(|err| error(format!("failed to submit availability update to '{endpoint}': {err}")))?;
    let http_status = response.status();
    let body = response.text().map_err(|err| error(format!("failed to read availability response: {err}")))?;
    if !http_status.is_success() {
        return Err(error(format!("availability update failed with HTTP {http_status}: {}", body.trim())));
    }
    let response_json = serde_json::from_str::<Value>(&body).unwrap_or_else(|_| json!({ "response": body }));
    emit(
        json_output,
        response_json,
        format!("Set {}/{}@{} availability to {status}", coordinate.namespace, coordinate.name, coordinate.release),
    )
}

fn fetch(raw_coordinate: &str, api_url: Option<&str>) -> Result<FetchedArtifact> {
    let coordinate = parse_coordinate(raw_coordinate)?;
    let registry_origin = super::commands::resolve_registry_api_base(api_url.map(str::to_string))?;
    let detail_url = format!("{}/v1/artifacts/{}/{}", registry_origin.trim_end_matches('/'), coordinate.namespace, coordinate.name);
    let client = super::commands::registry_http_client()?;
    let response = client
        .get(&detail_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, format!("cellc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .map_err(|err| error(format!("Registry request '{detail_url}' failed: {err}")))?;
    if !response.status().is_success() {
        return Err(error(format!("Registry request '{detail_url}' returned HTTP {}", response.status())));
    }
    let detail_bytes = response.bytes().map_err(|err| error(format!("failed to read Registry response: {err}")))?;
    if detail_bytes.is_empty() || detail_bytes.len() > MAX_REGISTRY_RESPONSE_BYTES {
        return Err(error("Registry detail response is empty or exceeds 2 MiB"));
    }
    let detail: Value =
        serde_json::from_slice(&detail_bytes).map_err(|err| error(format!("Registry detail response is invalid JSON: {err}")))?;
    let releases =
        detail.get("releases").and_then(Value::as_array).ok_or_else(|| error("Registry detail response has no releases array"))?;
    let release = releases
        .iter()
        .find(|item| item.get("release").and_then(Value::as_str) == Some(coordinate.release.as_str()))
        .cloned()
        .ok_or_else(|| {
            error(format!("Registry has no release '{}' for {}/{}", coordinate.release, coordinate.namespace, coordinate.name))
        })?;
    if release.get("availability_status").and_then(Value::as_str) != Some("active") {
        return Err(error("artifact release is not active"));
    }
    if matches!(release.get("verification_status").and_then(Value::as_str), None | Some("pending") | Some("rejected")) {
        return Err(error("artifact release has no accepted verification evidence"));
    }
    let artifact = detail.get("artifact").cloned().ok_or_else(|| error("Registry detail response has no artifact descriptor"))?;
    let immutable = object_field(&release, "immutable_bundle", "Registry release")?;
    let bundle_url = map_string_field(immutable, "url", "immutable_bundle")?.to_string();
    validate_download_url(&bundle_url)?;
    let response = client
        .get(&bundle_url)
        .header(reqwest::header::ACCEPT, "application/octet-stream, application/json")
        .header(reqwest::header::USER_AGENT, format!("cellc/{}", env!("CARGO_PKG_VERSION")))
        .send()
        .map_err(|err| error(format!("immutable bundle request '{bundle_url}' failed: {err}")))?;
    if !response.status().is_success() {
        return Err(error(format!("immutable bundle request returned HTTP {}", response.status())));
    }
    let bundle = response.bytes().map_err(|err| error(format!("failed to read immutable bundle: {err}")))?.to_vec();
    if bundle.is_empty() || bundle.len() > MAX_BUNDLE_BYTES {
        return Err(error("immutable artifact bundle is empty or exceeds 5 MiB"));
    }
    let expected_snapshot = map_string_field(immutable, "snapshot_hash", "immutable_bundle")?;
    require_sha256(&bundle, expected_snapshot, "snapshot_hash")?;
    Ok(FetchedArtifact { coordinate, registry_origin, artifact, release, bundle_url, bundle })
}

fn validate_download_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).map_err(|err| error(format!("immutable bundle URL is invalid: {err}")))?;
    let host = url.host_str().ok_or_else(|| error("immutable bundle URL has no host"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let loopback =
        host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(error("immutable bundle URL must use HTTPS; plaintext HTTP is allowed only for loopback development servers"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(error("immutable bundle URL must not contain credentials or a fragment"));
    }
    Ok(())
}

fn verify_fetched(fetched: &FetchedArtifact) -> Result<VerifiedBundle> {
    let bundle: ArtifactBundle =
        serde_json::from_slice(&fetched.bundle).map_err(|err| error(format!("immutable artifact bundle is invalid: {err}")))?;
    if bundle.schema != "cellscript-registry-bundle"
        || bundle.namespace != fetched.coordinate.namespace
        || bundle.name != fetched.coordinate.name
        || bundle.release != fetched.coordinate.release
    {
        return Err(error("immutable artifact bundle identity does not match the Registry release"));
    }
    let kind = string_field(&fetched.artifact, "kind", "artifact descriptor")?;
    let profile = string_field(&fetched.artifact, "profile", "artifact descriptor")?;
    if bundle.profile != profile {
        return Err(error("immutable artifact bundle profile does not match the Registry descriptor"));
    }
    let contract: Value = serde_json::from_str(&bundle.manifest_json)
        .map_err(|err| error(format!("artifact profile contract is invalid JSON: {err}")))?;
    let canonical_contract = crate::package::registry::canonical_artifact_contract_json(&contract).map_err(error)?;
    let release_identity = signed_release(&fetched.release)?;
    require_ckb_hash(
        &hex::encode(crate::ckb_blake2b256(canonical_contract.as_bytes())),
        string_field(&fetched.release, "manifest_hash", "Registry release")?,
        "manifest_hash",
    )?;
    let mut required = match profile {
        "ckb_executable" => vec!["source", "executable", "abi"],
        "reproducible_build" => vec!["source", "executable", "build_recipe"],
        "copy_material" => vec!["source"],
        _ => return Err(error(format!("artifact profile '{profile}' is not a generic immutable-bundle profile"))),
    };
    if contract.pointer("/security/audit_report_hash").is_some() {
        required.push("audit_report");
    }
    if profile == "ckb_executable" && contract.pointer("/build/reproducible").and_then(Value::as_bool) == Some(true) {
        required.push("build_recipe");
    }
    let mut objects = BTreeMap::new();
    for object in bundle.objects {
        if !required.contains(&object.role.as_str()) || objects.contains_key(&object.role) {
            return Err(error(format!("artifact bundle has duplicate or unsupported role '{}'", object.role)));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&object.content_base64)
            .map_err(|err| error(format!("artifact bundle role '{}' is not valid base64: {err}", object.role)))?;
        if bytes.is_empty() {
            return Err(error(format!("artifact bundle role '{}' is empty", object.role)));
        }
        objects.insert(object.role, bytes);
    }
    if required.iter().any(|role| !objects.contains_key(*role)) {
        return Err(error("artifact bundle does not contain the exact required role set"));
    }
    let source = objects.remove("source").expect("required source role");
    require_ckb_hash(
        &hex::encode(crate::ckb_blake2b256(&source)),
        string_field(&fetched.release, "source_hash", "Registry release")?,
        "source_hash",
    )?;
    let artifact_hash = objects.get("executable").map(|bytes| hex::encode(crate::ckb_blake2b256(bytes)));
    let abi_hash = objects.get("abi").map(|bytes| hex::encode(crate::ckb_blake2b256(bytes)));
    let build_recipe_hash = objects.get("build_recipe").map(|bytes| hex::encode(crate::ckb_blake2b256(bytes)));
    let audit_report_hash = objects.get("audit_report").map(|bytes| hex::encode(crate::ckb_blake2b256(bytes)));
    if let Some(actual) = artifact_hash.as_deref() {
        require_ckb_hash(actual, map_string_field(release_identity, "artifact_hash", "signed release")?, "artifact_hash")?;
    }
    if let Some(actual) = abi_hash.as_deref() {
        require_ckb_hash(actual, map_string_field(release_identity, "abi_hash", "signed release")?, "abi_hash")?;
    }
    if let Some(actual) = build_recipe_hash.as_deref() {
        require_ckb_hash(actual, map_string_field(release_identity, "build_recipe_hash", "signed release")?, "build_recipe_hash")?;
    }
    crate::package::registry::validate_artifact_profile_contract(
        kind,
        profile,
        &contract,
        crate::package::registry::ArtifactContractHashes {
            artifact_hash: artifact_hash.as_deref(),
            abi_hash: abi_hash.as_deref(),
            build_recipe_hash: build_recipe_hash.as_deref(),
            audit_report_hash: audit_report_hash.as_deref(),
        },
    )
    .map_err(error)?;
    let mut object_hashes = BTreeMap::new();
    object_hashes.insert("source".to_string(), hex::encode(crate::ckb_blake2b256(&source)));
    if let Some(value) = artifact_hash {
        object_hashes.insert("executable".to_string(), value);
    }
    if let Some(value) = abi_hash {
        object_hashes.insert("abi".to_string(), value);
    }
    if let Some(value) = build_recipe_hash {
        object_hashes.insert("build_recipe".to_string(), value);
    }
    if let Some(value) = audit_report_hash {
        object_hashes.insert("audit_report".to_string(), value);
    }
    Ok(VerifiedBundle { profile_contract: contract, source, object_hashes })
}

fn materialize_template(source: &[u8], contract: &Value, destination: &Path) -> Result<()> {
    let format = contract.pointer("/copy/format").and_then(Value::as_str).unwrap_or("");
    if format != "file_map_v1" {
        return Err(error("template copy requires profile_contract.copy.format=file_map_v1"));
    }
    let entrypoint =
        contract.pointer("/copy/entrypoint").and_then(Value::as_str).ok_or_else(|| error("template entrypoint is missing"))?;
    let file_map: TemplateFileMap =
        serde_json::from_slice(source).map_err(|err| error(format!("template file map is invalid: {err}")))?;
    if file_map.schema != "cellscript-template-file-map-v1" || file_map.files.is_empty() || file_map.files.len() > 10_000 {
        return Err(error("template source must be a non-empty cellscript-template-file-map-v1 with at most 10000 files"));
    }
    let mut paths = BTreeSet::new();
    let mut materialized = Vec::new();
    for file in file_map.files {
        let relative = safe_relative_path(&file.path)?;
        if !paths.insert(relative.clone()) {
            return Err(error(format!("template contains duplicate path '{}'", file.path)));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&file.content_base64)
            .map_err(|err| error(format!("template file '{}' is not valid base64: {err}", file.path)))?;
        require_ckb_hash(&hex::encode(crate::ckb_blake2b256(&bytes)), &file.blake2b256, "template file hash")?;
        let target = destination.join(&relative);
        if target.exists() {
            return Err(error(format!("template copy refuses to overwrite '{}'", target.display())));
        }
        materialized.push((target, bytes));
    }
    let entrypoint_path = safe_relative_path(entrypoint)?;
    if !paths.contains(&entrypoint_path) {
        return Err(error("template copy.entrypoint is not present in the authenticated file map"));
    }
    for (target, _) in &materialized {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| error(format!("failed to create template directory '{}': {err}", parent.display())))?;
        }
    }
    for (target, bytes) in materialized {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file =
            options.open(&target).map_err(|err| error(format!("failed to create template file '{}': {err}", target.display())))?;
        std::io::Write::write_all(&mut file, &bytes)
            .map_err(|err| error(format!("failed to write template file '{}': {err}", target.display())))?;
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.contains('\\') || value.contains('\0') {
        return Err(error("template paths must be non-empty portable forward-slash paths"));
    }
    let path = Path::new(value);
    if path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err(error(format!("template path '{value}' is not a safe relative path")));
    }
    Ok(path.to_path_buf())
}

fn signed_release(release: &Value) -> Result<&serde_json::Map<String, Value>> {
    let entry = object_field(release, "registry_entry", "Registry release")?;
    let versions = entry.get("versions").and_then(Value::as_array).ok_or_else(|| error("signed registry_entry has no versions"))?;
    let release_name = string_field(release, "release", "Registry release")?;
    versions
        .iter()
        .find(|item| item.get("version").and_then(Value::as_str) == Some(release_name))
        .and_then(Value::as_object)
        .ok_or_else(|| error("signed registry_entry does not contain the selected release"))
}

fn require_deployment_contract(contract: &Value, evidence: &serde_json::Map<String, Value>) -> Result<()> {
    require_deployment_contract_values(
        contract,
        map_string_field(evidence, "hash_type", "deployed evidence")?,
        map_string_field(evidence, "dep_type", "deployed evidence")?,
    )
}

fn require_deployment_contract_values(contract: &Value, hash_type: &str, dep_type: &str) -> Result<()> {
    let contract_hash_type = contract
        .pointer("/ckb/hash_type")
        .and_then(Value::as_str)
        .ok_or_else(|| error("signed profile contract has no ckb.hash_type"))?;
    let contract_dep_type = contract
        .pointer("/ckb/dep_type")
        .and_then(Value::as_str)
        .ok_or_else(|| error("signed profile contract has no ckb.dep_type"))?;
    if hash_type != contract_hash_type {
        return Err(error(format!(
            "deployment hash_type '{hash_type}' does not match signed profile contract '{contract_hash_type}'"
        )));
    }
    if dep_type != contract_dep_type {
        return Err(error(format!("deployment dep_type '{dep_type}' does not match signed profile contract '{contract_dep_type}'")));
    }
    Ok(())
}

fn revalidate_mainnet_deployment(evidence: &serde_json::Map<String, Value>, rpc_url: &str) -> Result<()> {
    let chain = ckb_rpc_call(rpc_url, "get_blockchain_info", json!([]))?;
    let chain_id = chain
        .get("chain")
        .or_else(|| chain.get("chain_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| error("CKB RPC get_blockchain_info returned no chain identity"))?;
    if !matches!(chain_id.trim().to_ascii_lowercase().replace('_', "-").as_str(), "ckb" | "ckb-mainnet") {
        return Err(error(format!("artifact CellDep consumption is mainnet-only; RPC reports chain '{chain_id}'")));
    }

    let declared_out_point =
        evidence.get("out_point").and_then(Value::as_object).ok_or_else(|| error("deployed evidence.out_point must be an object"))?;
    let declared = get_live_cell(rpc_url, declared_out_point)?;
    match map_string_field(evidence, "dep_type", "deployed evidence")? {
        "code" => verify_live_code_cell(&declared, evidence),
        "dep_group" => {
            let content = declared
                .pointer("/cell/data/content")
                .and_then(Value::as_str)
                .ok_or_else(|| error("live DepGroup Cell has no output data"))?;
            let members = parse_dep_group_out_points(content)?;
            if let Some(expected_size) = evidence.get("dep_group_size").and_then(Value::as_u64) {
                if expected_size != members.len() as u64 {
                    return Err(error("live DepGroup member count no longer matches Registry deployment evidence"));
                }
            }
            let resolved = evidence
                .get("resolved_code_out_point")
                .and_then(Value::as_object)
                .ok_or_else(|| error("DepGroup deployment evidence has no resolved_code_out_point"))?;
            let resolved_tx_hash = map_string_field(resolved, "tx_hash", "resolved_code_out_point")?;
            let resolved_index = resolved
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| error("resolved_code_out_point.index must be a u32"))?;
            if !members.iter().any(|(tx_hash, index)| tx_hash.eq_ignore_ascii_case(resolved_tx_hash) && *index == resolved_index) {
                return Err(error("resolved code Cell is not a member of the live DepGroup"));
            }
            let code = get_live_cell(rpc_url, resolved)?;
            verify_live_code_cell(&code, evidence)
        }
        other => Err(error(format!("unsupported deployment dep_type '{other}'"))),
    }
}

fn get_live_cell(rpc_url: &str, out_point: &serde_json::Map<String, Value>) -> Result<Value> {
    let tx_hash = map_string_field(out_point, "tx_hash", "out_point")?;
    require_hash_shape(tx_hash, "out_point.tx_hash")?;
    let index = out_point
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| error("out_point.index must be a u32"))?;
    let live = ckb_rpc_call(rpc_url, "get_live_cell", json!([{ "tx_hash": tx_hash, "index": format!("0x{index:x}") }, true, false]))?;
    if live.get("status").and_then(Value::as_str) != Some("live") {
        return Err(error(format!("deployment Cell {tx_hash}:0x{index:x} is no longer live")));
    }
    if !live.get("cell").is_some_and(Value::is_object) {
        return Err(error("CKB RPC live Cell response has no cell object"));
    }
    Ok(live)
}

fn verify_live_code_cell(live: &Value, evidence: &serde_json::Map<String, Value>) -> Result<()> {
    let data_hash = live
        .pointer("/cell/data/hash")
        .or_else(|| live.pointer("/cell/data_hash"))
        .and_then(Value::as_str)
        .ok_or_else(|| error("CKB RPC live Cell response has no data hash"))?;
    require_ckb_hash(data_hash, map_string_field(evidence, "data_hash", "deployed evidence")?, "live Cell data_hash")?;
    let expected_code_hash = map_string_field(evidence, "code_hash", "deployed evidence")?;
    if map_string_field(evidence, "hash_type", "deployed evidence")? == "type" {
        let type_script = live
            .pointer("/cell/output/type")
            .filter(|value| !value.is_null())
            .ok_or_else(|| error("type-hash deployment Cell has no Type Script"))?;
        let actual = super::commands::ckb_script_hash_from_json(type_script)?;
        require_ckb_hash(&actual, expected_code_hash, "live Cell type script hash")?;
    } else {
        require_ckb_hash(data_hash, expected_code_hash, "live Cell code_hash")?;
    }
    Ok(())
}

fn parse_dep_group_out_points(content: &str) -> Result<Vec<(String, u32)>> {
    let bytes = hex::decode(content.strip_prefix("0x").unwrap_or(content))
        .map_err(|err| error(format!("DepGroup Cell data is not hexadecimal: {err}")))?;
    if bytes.len() < 4 {
        return Err(error("DepGroup Cell data is shorter than an OutPointVec header"));
    }
    let count = u32::from_le_bytes(bytes[..4].try_into().expect("four-byte slice")) as usize;
    if count == 0 || count > 2048 || bytes.len() != 4 + count * 36 {
        return Err(error("DepGroup Cell data is not a canonical non-empty Molecule OutPointVec"));
    }
    Ok((0..count)
        .map(|item| {
            let offset = 4 + item * 36;
            let tx_hash = format!("0x{}", hex::encode(&bytes[offset..offset + 32]));
            let index = u32::from_le_bytes(bytes[offset + 32..offset + 36].try_into().expect("four-byte slice"));
            (tx_hash, index)
        })
        .collect())
}

fn ckb_rpc_call(rpc_url: &str, method: &str, params: Value) -> Result<Value> {
    validate_rpc_url(rpc_url)?;
    let client = super::commands::registry_http_client()?;
    let mut response = client
        .post(rpc_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::USER_AGENT, format!("cellc/{}", env!("CARGO_PKG_VERSION")))
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .send()
        .map_err(|err| error(format!("CKB RPC request '{method}' failed: {err}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(error(format!("CKB RPC request '{method}' returned HTTP {status}")));
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut response)
        .take((MAX_REGISTRY_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| error(format!("failed to read CKB RPC response '{method}': {err}")))?;
    if bytes.is_empty() || bytes.len() > MAX_REGISTRY_RESPONSE_BYTES {
        return Err(error("CKB RPC response is empty or exceeds 2 MiB"));
    }
    let rpc: Value =
        serde_json::from_slice(&bytes).map_err(|err| error(format!("CKB RPC response '{method}' is invalid JSON: {err}")))?;
    if let Some(rpc_error) = rpc.get("error") {
        return Err(error(format!("CKB RPC request '{method}' failed: {rpc_error}")));
    }
    rpc.get("result").cloned().ok_or_else(|| error(format!("CKB RPC response '{method}' has no result")))
}

fn validate_rpc_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).map_err(|err| error(format!("CKB RPC URL is invalid: {err}")))?;
    let host = url.host_str().ok_or_else(|| error("CKB RPC URL has no host"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let loopback =
        host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(error("CKB RPC URL must use HTTPS; plaintext HTTP is allowed only for loopback development servers"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(error("CKB RPC URL must not contain credentials or a fragment"));
    }
    Ok(())
}

fn chain_verified_deployment(release: &Value) -> Result<&Value> {
    if release.get("deployment_status").and_then(Value::as_str) != Some("chain_verified") {
        return Err(error("a chain-verified mainnet deployment is required"));
    }
    release
        .get("evidence")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().rev().find(|item| {
                item.get("kind").and_then(Value::as_str) == Some("deployed")
                    && item.pointer("/evidence/chain_verification").and_then(Value::as_str).is_some()
            })
        })
        .ok_or_else(|| error("Registry release claims chain_verified but contains no RPC-verified deployment evidence"))
}

fn require_assurance(release: &Value, accept_hash_bound: bool) -> Result<()> {
    match release.get("verification_status").and_then(Value::as_str) {
        Some("verified") => Ok(()),
        Some("hash_bound" | "evidence_required") if accept_hash_bound => Ok(()),
        Some("hash_bound") => Err(error("artifact has hash-integrity evidence only; pass --accept-hash-bound to make that trust decision explicit")),
        Some("evidence_required") => Err(error("artifact still requires external/reproducible evidence; pass --accept-hash-bound to pin its current immutable bytes explicitly")),
        _ => Err(error("artifact has no acceptable verification status")),
    }
}

fn receipt_for(fetched: &FetchedArtifact) -> FetchReceipt {
    FetchReceipt {
        schema: "cellscript-artifact-fetch-receipt-v1".to_string(),
        coordinate: format!("{}/{}@{}", fetched.coordinate.namespace, fetched.coordinate.name, fetched.coordinate.release),
        registry_origin: fetched.registry_origin.clone(),
        artifact: fetched.artifact.clone(),
        release: fetched.release.clone(),
        bundle_sha256: sha256_identity(&fetched.bundle),
        bundle_url: fetched.bundle_url.clone(),
    }
}

fn parse_coordinate(value: &str) -> Result<Coordinate> {
    let (package, release) = value.rsplit_once('@').ok_or_else(|| error("artifact coordinate must be namespace/name@release"))?;
    let (namespace, name) = package.split_once('/').ok_or_else(|| error("artifact coordinate must be namespace/name@release"))?;
    for (label, token) in [("namespace", namespace), ("name", name)] {
        let bytes = token.as_bytes();
        let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if token.is_empty()
            || token.len() > 64
            || !edge(bytes[0])
            || !edge(*bytes.last().expect("non-empty identifier"))
            || !bytes.iter().all(|byte| edge(*byte) || matches!(*byte, b'-' | b'_'))
        {
            return Err(error(format!(
                "artifact {label} must be 1-64 lowercase letters or numbers, with '-' or '_' only between characters"
            )));
        }
    }
    if release.is_empty() || release.len() > 80 || !release.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-+_".contains(&byte))
    {
        return Err(error("artifact release is not a valid registry version token"));
    }
    Ok(Coordinate { namespace: namespace.to_string(), name: name.to_string(), release: release.to_string() })
}

fn object_field<'a>(value: &'a Value, key: &str, label: &str) -> Result<&'a serde_json::Map<String, Value>> {
    value.get(key).and_then(Value::as_object).ok_or_else(|| error(format!("{label}.{key} must be an object")))
}

fn string_field<'a>(value: &'a Value, key: &str, label: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| error(format!("{label}.{key} must be a string")))
}

fn map_string_field<'a>(value: &'a serde_json::Map<String, Value>, key: &str, label: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| error(format!("{label}.{key} must be a string")))
}

fn require_hash_shape(value: &str, label: &str) -> Result<()> {
    let bare = value.strip_prefix("0x").unwrap_or(value);
    if bare.len() != 64 || !bare.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(error(format!("{label} must be a 32-byte hexadecimal hash")));
    }
    Ok(())
}

fn require_ckb_hash(actual: &str, expected: &str, label: &str) -> Result<()> {
    let normalize = |value: &str| value.trim_start_matches("0x").to_ascii_lowercase();
    let actual = normalize(actual);
    let expected = normalize(expected);
    if actual.len() != 64 || expected.len() != 64 || actual != expected {
        return Err(error(format!("{label} does not match the signed Registry identity")));
    }
    Ok(())
}

fn require_sha256(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
    let actual = sha256_identity(bytes);
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(error(format!("{label} does not match the downloaded immutable bundle")));
    }
    Ok(())
}

fn sha256_identity(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

fn canonical_json(value: &Value) -> Result<String> {
    serde_json::to_string(&crate::package::registry::canonical_json_value(value))
        .map_err(|err| error(format!("failed to serialize canonical JSON: {err}")))
}

fn read_limited(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|err| error(format!("failed to inspect {label} '{}': {err}", path.display())))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit as u64 {
        return Err(error(format!("{label} must be a non-empty regular file no larger than {limit} bytes")));
    }
    std::fs::read(path).map_err(|err| error(format!("failed to read {label} '{}': {err}", path.display())))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, label: &str) -> Result<T> {
    let bytes = read_limited(path, MAX_REGISTRY_RESPONSE_BYTES, label)?;
    serde_json::from_slice(&bytes).map_err(|err| error(format!("failed to parse {label} '{}': {err}", path.display())))
}

fn write_json(path: &Path, value: &impl Serialize, force: bool) -> Result<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|err| error(format!("failed to serialize '{}': {err}", path.display())))?;
    bytes.push(b'\n');
    write_bytes(path, &bytes, force)
}

fn write_bytes(path: &Path, bytes: &[u8], force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(error(format!("refusing to overwrite '{}'; pass --force explicitly", path.display())));
    }
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|err| error(format!("failed to create '{}': {err}", parent.display())))?;
    }
    std::fs::write(path, bytes).map_err(|err| error(format!("failed to write '{}': {err}", path.display())))
}

fn emit(json_output: bool, machine: Value, human: String) -> Result<()> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&machine).map_err(|err| error(format!("failed to serialize command output: {err}")))?
        );
    } else {
        println!("{human}");
    }
    Ok(())
}

fn error(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_generic_bundle_against_signed_release_and_contract() {
        let source = b"source";
        let executable = b"elf";
        let abi = b"abi";
        let source_hash = hex::encode(crate::ckb_blake2b256(source));
        let artifact_hash = hex::encode(crate::ckb_blake2b256(executable));
        let abi_hash = hex::encode(crate::ckb_blake2b256(abi));
        let contract = json!({
            "schema": crate::package::registry::ARTIFACT_PROFILE_CONTRACT_SCHEMA,
            "artifact_kind": "deployable_contract",
            "profile": "ckb_executable",
            "build": {
                "target": "riscv64imac-unknown-none-elf",
                "toolchain": "rustc 1.97.1",
                "profile": "release",
                "source_revision": "0123456789abcdef",
                "reproducible": false
            },
            "security": { "status": "review_required" },
            "ckb": {
                "vm_version": "2",
                "script_role": "type",
                "hash_type": "data1",
                "dep_type": "code",
                "abi_hash": abi_hash
            }
        });
        let manifest_json = crate::package::registry::canonical_artifact_contract_json(&contract).unwrap();
        let manifest_hash = hex::encode(crate::ckb_blake2b256(manifest_json.as_bytes()));
        let bundle = serde_json::to_vec(&json!({
            "schema": "cellscript-registry-bundle",
            "namespace": "demo",
            "name": "contract",
            "release": "1.0.0",
            "profile": "ckb_executable",
            "manifest_json": manifest_json,
            "objects": [
                { "role": "source", "content_base64": base64::engine::general_purpose::STANDARD.encode(source) },
                { "role": "executable", "content_base64": base64::engine::general_purpose::STANDARD.encode(executable) },
                { "role": "abi", "content_base64": base64::engine::general_purpose::STANDARD.encode(abi) }
            ]
        }))
        .unwrap();
        let fetched = FetchedArtifact {
            coordinate: parse_coordinate("demo/contract@1.0.0").unwrap(),
            registry_origin: "https://registry.example".to_string(),
            artifact: json!({ "kind": "deployable_contract", "profile": "ckb_executable" }),
            release: json!({
                "release": "1.0.0",
                "source_hash": source_hash,
                "manifest_hash": manifest_hash,
                "verification_status": "hash_bound",
                "deployment_status": "undeployed",
                "availability_status": "active",
                "registry_entry": {
                    "versions": [{
                        "version": "1.0.0",
                        "artifact_hash": artifact_hash,
                        "abi_hash": abi_hash
                    }]
                }
            }),
            bundle_url: "https://registry.example/bundle".to_string(),
            bundle,
        };
        let verified = verify_fetched(&fetched).unwrap();
        assert_eq!(verified.object_hashes.get("executable"), Some(&artifact_hash));
    }

    #[test]
    fn template_paths_fail_closed_on_traversal() {
        assert!(safe_relative_path("src/main.cell").is_ok());
        assert!(safe_relative_path("../secret").is_err());
        assert!(safe_relative_path("/absolute").is_err());
        assert!(safe_relative_path("nested\\windows").is_err());
    }

    #[test]
    fn immutable_bundle_downloads_require_safe_transport() {
        assert!(validate_download_url("https://registry.example/bundle?version=1").is_ok());
        assert!(validate_download_url("http://127.0.0.1:8787/bundle").is_ok());
        assert!(validate_download_url("http://registry.example/bundle").is_err());
        assert!(validate_download_url("https://user:secret@registry.example/bundle").is_err());
        assert!(validate_download_url("https://registry.example/bundle#fragment").is_err());
    }

    #[test]
    fn deployment_consumption_is_bound_to_the_signed_ckb_contract() {
        let contract = json!({ "ckb": { "hash_type": "data1", "dep_type": "code" } });
        assert!(require_deployment_contract_values(&contract, "data1", "code").is_ok());
        assert!(require_deployment_contract_values(&contract, "type", "code").is_err());
        assert!(require_deployment_contract_values(&contract, "data1", "dep_group").is_err());
    }

    #[test]
    fn dep_group_members_are_decoded_canonically() {
        let mut bytes = vec![1, 0, 0, 0];
        bytes.extend([0x42; 32]);
        bytes.extend(7_u32.to_le_bytes());
        let members = parse_dep_group_out_points(&format!("0x{}", hex::encode(bytes))).unwrap();
        assert_eq!(members, vec![(format!("0x{}", "42".repeat(32)), 7)]);
        assert!(parse_dep_group_out_points("0x00000000").is_err());
    }

    #[test]
    fn rpc_transport_requires_https_except_for_loopback_development() {
        assert!(validate_rpc_url("https://mainnet.ckb.dev/rpc").is_ok());
        assert!(validate_rpc_url("http://127.0.0.1:8114").is_ok());
        assert!(validate_rpc_url("http://public.example/rpc").is_err());
        assert!(validate_rpc_url("https://user:secret@mainnet.ckb.dev/rpc").is_err());
    }
}

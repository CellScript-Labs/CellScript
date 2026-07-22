use crate::descriptor::{canonical_hash_type, canonical_hex, FiberAssetDescriptor, ScriptIdentity};
use crate::fiber_config::{FiberCellDep, FiberOutPoint, FiberTypeIdScript, FiberUdtDep};
use ckb_types::{
    bytes::Bytes,
    core::ScriptHashType,
    packed,
    prelude::{Builder, Entity, Pack},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, path::Path};

const CKB_TYPE_ID_CODE_HASH: &str = "0x00000000000000000000000000000000000000000000000000545950455f4944";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyMode {
    Direct,
    TypeId,
}

impl std::str::FromStr for DependencyMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "direct" => Ok(Self::Direct),
            "type-id" | "type_id" => Ok(Self::TypeId),
            _ => anyhow::bail!("dependency mode must be 'direct' or 'type-id'"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutPointRef {
    pub tx_hash: String,
    pub index: u32,
}

impl OutPointRef {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        let (tx_hash, index) =
            value.rsplit_once(':').ok_or_else(|| anyhow::anyhow!("outpoint must use 0x<32-byte-tx-hash>:<decimal-or-hex-index>"))?;
        let tx_hash = canonical_hex(tx_hash, Some(32), "outpoint.tx_hash")?;
        let index = if let Some(raw) = index.strip_prefix("0x") { u32::from_str_radix(raw, 16)? } else { index.parse::<u32>()? };
        Ok(Self { tx_hash, index })
    }

    pub fn display(&self) -> String {
        format!("{}:{}", self.tx_hash, self.index)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveCell {
    pub out_point: OutPointRef,
    pub type_script: Option<ScriptIdentity>,
    pub data: Vec<u8>,
}

pub trait CkbEvidenceProvider {
    fn get_live_cell(&self, out_point: &OutPointRef) -> anyhow::Result<LiveCell>;
    fn resolve_type_id(&self, type_id: &ScriptIdentity) -> anyhow::Result<Vec<LiveCell>>;
}

#[derive(Debug, Clone)]
pub struct HttpCkbEvidenceProvider {
    ckb_rpc_url: String,
    ckb_indexer_rpc_url: String,
    client: reqwest::blocking::Client,
}

impl HttpCkbEvidenceProvider {
    pub fn new(ckb_rpc_url: impl Into<String>, ckb_indexer_rpc_url: Option<String>) -> anyhow::Result<Self> {
        let ckb_rpc_url = ckb_rpc_url.into();
        let ckb_indexer_rpc_url = ckb_indexer_rpc_url.unwrap_or_else(|| ckb_rpc_url.clone());
        validate_rpc_url(&ckb_rpc_url, "CKB RPC")?;
        validate_rpc_url(&ckb_indexer_rpc_url, "CKB indexer RPC")?;
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        Ok(Self { ckb_rpc_url, ckb_indexer_rpc_url, client })
    }

    fn rpc(&self, url: &str, method: &str, params: Value) -> anyhow::Result<Value> {
        let response = self
            .client
            .post(url)
            .json(&json!({"id": 1, "jsonrpc": "2.0", "method": method, "params": params}))
            .send()?
            .error_for_status()?
            .json::<Value>()?;
        if let Some(error) = response.get("error") {
            anyhow::bail!("CKB JSON-RPC method {method} failed: {error}");
        }
        response.get("result").cloned().ok_or_else(|| anyhow::anyhow!("CKB JSON-RPC method {method} returned no result"))
    }
}

fn validate_rpc_url(url: &str, label: &str) -> anyhow::Result<()> {
    let parsed = reqwest::Url::parse(url)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        anyhow::bail!("{label} URL must be an absolute http or https URL");
    }
    Ok(())
}

impl CkbEvidenceProvider for HttpCkbEvidenceProvider {
    fn get_live_cell(&self, out_point: &OutPointRef) -> anyhow::Result<LiveCell> {
        let result = self.rpc(
            &self.ckb_rpc_url,
            "get_live_cell",
            json!([{"tx_hash": out_point.tx_hash, "index": format!("0x{:x}", out_point.index)}, true]),
        )?;
        if result.get("status").and_then(Value::as_str) != Some("live") {
            anyhow::bail!("CKB outpoint {} is not live", out_point.display());
        }
        let cell = result.get("cell").ok_or_else(|| anyhow::anyhow!("live-cell result omitted cell payload"))?;
        let output = cell.get("output").ok_or_else(|| anyhow::anyhow!("live-cell result omitted output"))?;
        let type_script = output.get("type").filter(|value| !value.is_null()).map(parse_json_script).transpose()?;
        let content = cell
            .pointer("/data/content")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("live-cell result omitted data.content"))?;
        let data = decode_hex(content, "live-cell data.content")?;
        Ok(LiveCell { out_point: out_point.clone(), type_script, data })
    }

    fn resolve_type_id(&self, type_id: &ScriptIdentity) -> anyhow::Result<Vec<LiveCell>> {
        let type_id = type_id.clone().canonicalized()?;
        let result = self.rpc(
            &self.ckb_indexer_rpc_url,
            "get_cells",
            json!([
                {
                    "script": {
                        "code_hash": type_id.code_hash,
                        "hash_type": type_id.hash_type,
                        "args": type_id.args
                    },
                    "script_type": "type",
                    "script_search_mode": "exact"
                },
                "asc",
                "0x64",
                null
            ]),
        )?;
        let objects = result
            .get("objects")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("CKB indexer get_cells result omitted objects"))?;
        objects
            .iter()
            .map(|object| {
                let out_point =
                    parse_json_out_point(object.get("out_point").ok_or_else(|| anyhow::anyhow!("indexer cell omitted out_point"))?)?;
                self.get_live_cell(&out_point)
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeDeploymentIdentity {
    pub artifact_hash: String,
    pub deployment_name: String,
    pub code_hash: String,
    pub hash_type: String,
    pub code_cell_out_point: OutPointRef,
    pub code_cell_type_id: Option<ScriptIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveCellDepEvidence {
    pub mode: DependencyMode,
    pub dependency: FiberUdtDep,
    pub resolved_out_point: OutPointRef,
    pub live: bool,
    pub artifact_hash_verified: bool,
    pub code_identity_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResolvedAssetScriptSource {
    MaterializedActionPlan { path: String, action: String, output_indexes: Vec<usize> },
    VerifiedLiveAssetCell { out_point: OutPointRef },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedAssetScript {
    pub script: ScriptIdentity,
    pub source: ResolvedAssetScriptSource,
    pub data_length_bytes: usize,
}

pub fn verify_code_deployment(
    provider: &dyn CkbEvidenceProvider,
    manifest: &cellscript_ckb_adapter::DeploymentManifest,
    descriptor: &FiberAssetDescriptor,
    deployment_name: Option<&str>,
    dependency_mode: DependencyMode,
) -> anyhow::Result<(CodeDeploymentIdentity, LiveCellDepEvidence)> {
    let mut candidates = Vec::new();
    let mut failures = Vec::new();
    for deployment in manifest.deployments.iter().filter(|deployment| deployment_name.is_none_or(|name| deployment.name == name)) {
        match verify_deployment_ref(provider, deployment, descriptor, dependency_mode) {
            Ok(candidate) => candidates.push(candidate),
            Err(error) => failures.push(format!("{}: {error}", deployment.name)),
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => anyhow::bail!(
            "no live deployment in the ordinary DeploymentManifest binds artifact {}: {}",
            descriptor.artifact_hash,
            failures.join("; ")
        ),
        count => anyhow::bail!("{count} live deployments bind the artifact; select one deployment name explicitly"),
    }
}

fn verify_deployment_ref(
    provider: &dyn CkbEvidenceProvider,
    deployment: &cellscript_ckb_adapter::DeploymentRef,
    descriptor: &FiberAssetDescriptor,
    dependency_mode: DependencyMode,
) -> anyhow::Result<(CodeDeploymentIdentity, LiveCellDepEvidence)> {
    let out_point = OutPointRef::parse(&deployment.out_point)?;
    let live_cell = provider.get_live_cell(&out_point)?;
    let live_artifact_hash = format!("0x{}", hex::encode(cellscript::ckb_blake2b256(&live_cell.data)));
    if live_artifact_hash != descriptor.artifact_hash {
        anyhow::bail!("live code Cell data hash {} does not match compiled artifact {}", live_artifact_hash, descriptor.artifact_hash);
    }
    let hash_type = canonical_hash_type(&deployment.hash_type)?.to_string();
    let code_hash = match hash_type.as_str() {
        "type" => {
            let type_script =
                live_cell.type_script.as_ref().ok_or_else(|| anyhow::anyhow!("type-hash deployment code Cell has no Type Script"))?;
            packed_script_hash(type_script)?
        }
        "data" | "data1" | "data2" => descriptor.artifact_hash.clone(),
        _ => unreachable!("canonical hash type is closed"),
    };
    let manifest_code_hash = canonical_hex(&deployment.code_hash, Some(32), "deployment.code_hash")?;
    if manifest_code_hash != code_hash {
        anyhow::bail!("deployment code_hash {} does not match live derived code identity {}", manifest_code_hash, code_hash);
    }
    let code_cell_type_id = live_cell.type_script.clone().map(ScriptIdentity::canonicalized).transpose()?;
    let dependency = match dependency_mode {
        DependencyMode::Direct => FiberUdtDep {
            cell_dep: Some(FiberCellDep {
                out_point: FiberOutPoint { tx_hash: out_point.tx_hash.clone(), index: format!("0x{:x}", out_point.index) },
                dep_type: deployment.dep_type.to_ascii_lowercase(),
            }),
            type_id: None,
        },
        DependencyMode::TypeId => {
            let type_id = code_cell_type_id
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("type-id dependency mode requires a live TYPE_ID Script on the code Cell"))?;
            if type_id.code_hash != CKB_TYPE_ID_CODE_HASH || type_id.hash_type != "type" {
                anyhow::bail!("code Cell Type Script is not the canonical CKB TYPE_ID Script");
            }
            let resolved = provider.resolve_type_id(type_id)?;
            if resolved.len() != 1 || resolved[0].out_point != out_point {
                anyhow::bail!("TYPE_ID resolution did not return exactly the verified live code Cell");
            }
            FiberUdtDep {
                cell_dep: None,
                type_id: Some(FiberTypeIdScript {
                    code_hash: type_id.code_hash.clone(),
                    hash_type: type_id.hash_type.clone(),
                    args: type_id.args.clone(),
                }),
            }
        }
    };
    dependency.validate()?;
    Ok((
        CodeDeploymentIdentity {
            artifact_hash: descriptor.artifact_hash.clone(),
            deployment_name: deployment.name.clone(),
            code_hash,
            hash_type,
            code_cell_out_point: out_point.clone(),
            code_cell_type_id,
        },
        LiveCellDepEvidence {
            mode: dependency_mode,
            dependency,
            resolved_out_point: out_point,
            live: true,
            artifact_hash_verified: true,
            code_identity_verified: true,
        },
    ))
}

pub fn resolve_asset_from_action_plan(
    path: impl AsRef<Path>,
    manifest: &cellscript_ckb_adapter::DeploymentManifest,
    descriptor: &FiberAssetDescriptor,
    deployment: &CodeDeploymentIdentity,
) -> anyhow::Result<ResolvedAssetScript> {
    let path = path.as_ref();
    let plan = cellscript_ckb_adapter::load_action_plan(path)?;
    let plan_hash = plan
        .artifact_hash
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("materialized ActionPlan is missing artifact_hash for the checked fungible artifact"))?;
    let plan_hash = canonical_digest(plan_hash, "action plan artifact_hash")?;
    if plan_hash != descriptor.artifact_hash {
        anyhow::bail!("materialized ActionPlan artifact hash does not match the checked fungible artifact");
    }
    let resolved = cellscript_ckb_adapter::resolve_materialized_action_plan_with_manifest(&plan, Some(manifest))?;
    let mut candidates = Vec::new();
    for (index, output) in resolved.outputs.iter().enumerate() {
        let Some(type_script) = output.output.type_().to_opt() else {
            continue;
        };
        let identity = script_identity_from_packed(&type_script)?;
        if identity.code_hash == deployment.code_hash && identity.hash_type == deployment.hash_type {
            if output.data.len() != descriptor.data_length_bytes {
                anyhow::bail!(
                    "ActionPlan output[{index}] uses the asset code identity but encodes {} bytes; expected exactly {}",
                    output.data.len(),
                    descriptor.data_length_bytes
                );
            }
            candidates.push((index, identity));
        }
    }
    resolve_unique_asset_candidates(
        candidates,
        ResolvedAssetScriptSource::MaterializedActionPlan {
            path: path.display().to_string(),
            action: plan.action,
            output_indexes: Vec::new(),
        },
        descriptor.data_length_bytes,
    )
}

pub fn resolve_asset_from_live_cell(
    provider: &dyn CkbEvidenceProvider,
    out_point: &OutPointRef,
    descriptor: &FiberAssetDescriptor,
    deployment: &CodeDeploymentIdentity,
) -> anyhow::Result<ResolvedAssetScript> {
    let live_cell = provider.get_live_cell(out_point)?;
    if live_cell.data.len() != descriptor.data_length_bytes {
        anyhow::bail!(
            "live asset Cell {} has {} data bytes; Fiber v1 requires exactly {}",
            out_point.display(),
            live_cell.data.len(),
            descriptor.data_length_bytes
        );
    }
    let script = live_cell.type_script.ok_or_else(|| anyhow::anyhow!("live asset Cell has no Type Script"))?.canonicalized()?;
    if script.code_hash != deployment.code_hash || script.hash_type != deployment.hash_type {
        anyhow::bail!("live asset Cell Type Script is not bound to the verified code deployment");
    }
    Ok(ResolvedAssetScript {
        script,
        source: ResolvedAssetScriptSource::VerifiedLiveAssetCell { out_point: out_point.clone() },
        data_length_bytes: live_cell.data.len(),
    })
}

fn resolve_unique_asset_candidates(
    candidates: Vec<(usize, ScriptIdentity)>,
    source: ResolvedAssetScriptSource,
    data_length_bytes: usize,
) -> anyhow::Result<ResolvedAssetScript> {
    if candidates.is_empty() {
        anyhow::bail!("materialized ActionPlan contains no exact-16 output using the verified deployment code identity");
    }
    let identities = candidates.iter().map(|(_, script)| serde_json::to_string(script)).collect::<Result<BTreeSet<_>, _>>()?;
    if identities.len() != 1 {
        anyhow::bail!("materialized ActionPlan resolves to multiple concrete asset Script instances");
    }
    let indexes = candidates.iter().map(|(index, _)| *index).collect::<Vec<_>>();
    let script = candidates[0].1.clone();
    let source = match source {
        ResolvedAssetScriptSource::MaterializedActionPlan { path, action, .. } => {
            ResolvedAssetScriptSource::MaterializedActionPlan { path, action, output_indexes: indexes }
        }
        other => other,
    };
    Ok(ResolvedAssetScript { script, source, data_length_bytes })
}

fn parse_json_script(value: &Value) -> anyhow::Result<ScriptIdentity> {
    ScriptIdentity {
        code_hash: value.get("code_hash").and_then(Value::as_str).unwrap_or_default().to_string(),
        hash_type: value.get("hash_type").and_then(Value::as_str).unwrap_or_default().to_string(),
        args: value.get("args").and_then(Value::as_str).unwrap_or_default().to_string(),
    }
    .canonicalized()
}

fn parse_json_out_point(value: &Value) -> anyhow::Result<OutPointRef> {
    let tx_hash = value.get("tx_hash").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("out_point omitted tx_hash"))?;
    let index = value.get("index").and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("out_point omitted index"))?;
    OutPointRef::parse(&format!("{tx_hash}:{index}"))
}

fn canonical_digest(value: &str, field: &str) -> anyhow::Result<String> {
    if value.starts_with("0x") || value.starts_with("0X") {
        canonical_hex(value, Some(32), field)
    } else {
        canonical_hex(&format!("0x{value}"), Some(32), field)
    }
}

fn decode_hex(value: &str, field: &str) -> anyhow::Result<Vec<u8>> {
    let canonical = canonical_hex(value, None, field)?;
    Ok(hex::decode(&canonical[2..])?)
}

fn packed_script_hash(script: &ScriptIdentity) -> anyhow::Result<String> {
    let script = packed_script(script)?;
    Ok(format!("0x{}", hex::encode(script.calc_script_hash().as_slice())))
}

fn packed_script(script: &ScriptIdentity) -> anyhow::Result<packed::Script> {
    let script = script.clone().canonicalized()?;
    let code_hash = packed::Byte32::from_slice(&hex::decode(&script.code_hash[2..])?)?;
    let hash_type = match script.hash_type.as_str() {
        "data" => ScriptHashType::Data,
        "type" => ScriptHashType::Type,
        "data1" => ScriptHashType::Data1,
        "data2" => ScriptHashType::Data2,
        _ => unreachable!("canonical hash type is closed"),
    };
    Ok(packed::Script::new_builder()
        .code_hash(code_hash)
        .hash_type(hash_type)
        .args(Bytes::from(hex::decode(&script.args[2..])?).pack())
        .build())
}

fn script_identity_from_packed(script: &packed::Script) -> anyhow::Result<ScriptIdentity> {
    let hash_type = ScriptHashType::try_from(script.hash_type())?;
    let hash_type = match hash_type {
        ScriptHashType::Data => "data",
        ScriptHashType::Type => "type",
        ScriptHashType::Data1 => "data1",
        ScriptHashType::Data2 => "data2",
        other => anyhow::bail!("unsupported packed CKB Script hash_type {other:?}"),
    };
    ScriptIdentity {
        code_hash: format!("0x{}", hex::encode(script.code_hash().as_slice())),
        hash_type: hash_type.to_string(),
        args: format!("0x{}", hex::encode(script.args().raw_data())),
    }
    .canonicalized()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FiberAssetDescriptor, FIBER_COMPATIBILITY_SCHEMA};
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeProvider {
        cells: HashMap<String, LiveCell>,
        type_ids: HashMap<String, Vec<LiveCell>>,
    }

    impl CkbEvidenceProvider for FakeProvider {
        fn get_live_cell(&self, out_point: &OutPointRef) -> anyhow::Result<LiveCell> {
            self.cells.get(&out_point.display()).cloned().ok_or_else(|| anyhow::anyhow!("not live"))
        }

        fn resolve_type_id(&self, type_id: &ScriptIdentity) -> anyhow::Result<Vec<LiveCell>> {
            Ok(self.type_ids.get(&serde_json::to_string(type_id)?).cloned().unwrap_or_default())
        }
    }

    fn descriptor(data: &[u8]) -> FiberAssetDescriptor {
        FiberAssetDescriptor {
            schema: FIBER_COMPATIBILITY_SCHEMA.to_string(),
            contract: "fungible-type-group-v1".to_string(),
            module: "sample".to_string(),
            display_name: "sample::Asset".to_string(),
            selected_type: "Asset".to_string(),
            selected_invariant: "supply".to_string(),
            selected_field: "quantity".to_string(),
            compiler_version: "0.22.0".to_string(),
            metadata_schema_version: cellscript::METADATA_SCHEMA_VERSION,
            source_hash: format!("0x{}", "01".repeat(32)),
            artifact_hash: format!("0x{}", hex::encode(cellscript::ckb_blake2b256(data))),
            artifact_format: "RISC-V ELF".to_string(),
            target_profile: "ckb".to_string(),
            data_length_bytes: 16,
            amount_offset_bytes: 0,
            amount_width_bytes: 16,
            endianness: "little".to_string(),
            arithmetic: "checked-u128-sum-equality".to_string(),
            group_scope: "complete-ckb-type-script-group".to_string(),
            owner_mode: "script-args-32-byte-owner-lock-hash".to_string(),
            owner_args_length_bytes: 32,
            authority_modes: vec!["input-lock-hash".to_string(), "tagged-input-type-script-hash".to_string()],
            authority_args_lengths_bytes: vec![32, 33],
            owner_authorized_mint: true,
            owner_authorized_burn: true,
            non_owner_input_group_non_empty: true,
            non_owner_output_group_non_empty: true,
            non_owner_conservation_required: true,
            payload_required: false,
            witness_policy: "ignored; no empty-witness requirement".to_string(),
            runtime_helper: "fungible::require_type_group_v1".to_string(),
        }
    }

    #[test]
    fn direct_deployment_is_live_and_artifact_bound() {
        let artifact = b"artifact";
        let descriptor = descriptor(artifact);
        let out_point = OutPointRef { tx_hash: format!("0x{}", "11".repeat(32)), index: 0 };
        let live = LiveCell { out_point: out_point.clone(), type_script: None, data: artifact.to_vec() };
        let mut provider = FakeProvider::default();
        provider.cells.insert(out_point.display(), live);
        let manifest = cellscript_ckb_adapter::DeploymentManifest {
            schema: cellscript_ckb_adapter::DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![cellscript_ckb_adapter::DeploymentRef {
                name: "asset-code".to_string(),
                code_hash: descriptor.artifact_hash.clone(),
                hash_type: "data2".to_string(),
                args: format!("0x{}", "22".repeat(32)),
                dep_type: "code".to_string(),
                out_point: out_point.display(),
            }],
        };
        let (deployment, evidence) = verify_code_deployment(&provider, &manifest, &descriptor, None, DependencyMode::Direct).unwrap();
        assert_eq!(deployment.code_hash, descriptor.artifact_hash);
        assert!(evidence.live);
        assert!(evidence.dependency.cell_dep.is_some());
        assert!(evidence.dependency.type_id.is_none());
    }

    #[test]
    fn http_evidence_provider_rejects_non_http_urls() {
        assert!(HttpCkbEvidenceProvider::new("file:///etc/passwd", None).is_err());
        assert!(HttpCkbEvidenceProvider::new("http://127.0.0.1:8114", Some("file:///etc/passwd".to_string())).is_err());
    }

    #[test]
    fn deployment_rejects_stale_or_wrong_artifact_cell() {
        let descriptor = descriptor(b"expected");
        let out_point = OutPointRef { tx_hash: format!("0x{}", "33".repeat(32)), index: 1 };
        let mut provider = FakeProvider::default();
        provider
            .cells
            .insert(out_point.display(), LiveCell { out_point: out_point.clone(), type_script: None, data: b"wrong".to_vec() });
        let manifest = cellscript_ckb_adapter::DeploymentManifest {
            schema: cellscript_ckb_adapter::DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![cellscript_ckb_adapter::DeploymentRef {
                name: "asset-code".to_string(),
                code_hash: descriptor.artifact_hash.clone(),
                hash_type: "data2".to_string(),
                args: "0x".to_string(),
                dep_type: "code".to_string(),
                out_point: out_point.display(),
            }],
        };
        let error = verify_code_deployment(&provider, &manifest, &descriptor, None, DependencyMode::Direct).unwrap_err();
        assert!(error.to_string().contains("no live deployment"));
    }

    #[test]
    fn type_id_dependency_resolves_to_the_same_live_code_cell() {
        let artifact = b"type-id-artifact";
        let descriptor = descriptor(artifact);
        let out_point = OutPointRef { tx_hash: format!("0x{}", "44".repeat(32)), index: 2 };
        let type_id = ScriptIdentity {
            code_hash: CKB_TYPE_ID_CODE_HASH.to_string(),
            hash_type: "type".to_string(),
            args: format!("0x{}", "55".repeat(32)),
        };
        let live = LiveCell { out_point: out_point.clone(), type_script: Some(type_id.clone()), data: artifact.to_vec() };
        let mut provider = FakeProvider::default();
        provider.cells.insert(out_point.display(), live.clone());
        provider.type_ids.insert(serde_json::to_string(&type_id).unwrap(), vec![live]);
        let manifest = cellscript_ckb_adapter::DeploymentManifest {
            schema: cellscript_ckb_adapter::DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![cellscript_ckb_adapter::DeploymentRef {
                name: "upgradeable-code".to_string(),
                code_hash: packed_script_hash(&type_id).unwrap(),
                hash_type: "type".to_string(),
                args: type_id.args.clone(),
                dep_type: "code".to_string(),
                out_point: out_point.display(),
            }],
        };
        let (_, evidence) = verify_code_deployment(&provider, &manifest, &descriptor, None, DependencyMode::TypeId).unwrap();
        assert!(evidence.dependency.type_id.is_some());
        assert!(evidence.dependency.cell_dep.is_none());
        assert_eq!(evidence.resolved_out_point, out_point);
    }

    #[test]
    fn live_asset_resolution_never_infers_code_cell_type_id_args() {
        let descriptor = descriptor(b"artifact");
        let code_cell_out_point = OutPointRef { tx_hash: format!("0x{}", "66".repeat(32)), index: 0 };
        let asset_out_point = OutPointRef { tx_hash: format!("0x{}", "77".repeat(32)), index: 1 };
        let asset_script = ScriptIdentity {
            code_hash: descriptor.artifact_hash.clone(),
            hash_type: "data2".to_string(),
            args: "0x010203".to_string(),
        };
        let deployment = CodeDeploymentIdentity {
            artifact_hash: descriptor.artifact_hash.clone(),
            deployment_name: "asset-code".to_string(),
            code_hash: descriptor.artifact_hash.clone(),
            hash_type: "data2".to_string(),
            code_cell_out_point,
            code_cell_type_id: Some(ScriptIdentity {
                code_hash: CKB_TYPE_ID_CODE_HASH.to_string(),
                hash_type: "type".to_string(),
                args: format!("0x{}", "88".repeat(32)),
            }),
        };
        let mut provider = FakeProvider::default();
        provider.cells.insert(
            asset_out_point.display(),
            LiveCell {
                out_point: asset_out_point.clone(),
                type_script: Some(asset_script.clone()),
                data: 9u128.to_le_bytes().to_vec(),
            },
        );
        let resolved = resolve_asset_from_live_cell(&provider, &asset_out_point, &descriptor, &deployment).unwrap();
        assert_eq!(resolved.script.args, "0x010203");
        assert_ne!(resolved.script.args, deployment.code_cell_type_id.as_ref().unwrap().args);
    }

    #[test]
    fn action_plan_asset_resolution_requires_artifact_hash() {
        let descriptor = descriptor(b"artifact");
        let out_point = OutPointRef { tx_hash: format!("0x{}", "99".repeat(32)), index: 0 };
        let deployment = CodeDeploymentIdentity {
            artifact_hash: descriptor.artifact_hash.clone(),
            deployment_name: "asset-code".to_string(),
            code_hash: descriptor.artifact_hash.clone(),
            hash_type: "data2".to_string(),
            code_cell_out_point: out_point.clone(),
            code_cell_type_id: None,
        };
        let manifest = cellscript_ckb_adapter::DeploymentManifest {
            schema: cellscript_ckb_adapter::DEPLOYMENT_MANIFEST_SCHEMA.to_string(),
            version: 1,
            deployments: vec![cellscript_ckb_adapter::DeploymentRef {
                name: deployment.deployment_name.clone(),
                code_hash: deployment.code_hash.clone(),
                hash_type: deployment.hash_type.clone(),
                args: "0x".to_string(),
                dep_type: "code".to_string(),
                out_point: out_point.display(),
            }],
        };
        let plan = json!({
            "policy": cellscript_ckb_adapter::ACTION_PLAN_POLICY,
            "action": "mint",
            "metadata_hash": "00".repeat(32),
            "transaction_draft": {
                "state": "ActionPlan",
                "can_submit": false,
                "requires_packed_materialization": true,
                "outputs": [{
                    "capacity": 20_000_000_000u64,
                    "lock": {
                        "code_hash": format!("0x{}", "11".repeat(32)),
                        "hash_type": "data1",
                        "args": format!("0x{}", "22".repeat(20)),
                    },
                    "type": {
                        "code_hash": descriptor.artifact_hash.clone(),
                        "hash_type": "data2",
                        "args": format!("0x{}", "33".repeat(32)),
                    },
                }],
                "outputs_data": [format!("0x{}", "00".repeat(16))],
            },
            "adapter_contract": {
                "schema": cellscript_ckb_adapter::ADAPTER_CONTRACT_SCHEMA,
                "compiler_core_dependency": "no-ckb-sdk-rust",
                "transaction_realizer": "red-team-test",
                "resolved_tx_required_fields": ["outputs_data", "cell_deps", "lineage"],
            },
        });
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("asset-plan.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&plan).unwrap()).unwrap();

        let error = resolve_asset_from_action_plan(&path, &manifest, &descriptor, &deployment).unwrap_err();
        assert!(error.to_string().contains("missing artifact_hash"), "unexpected error: {error:#}");
    }
}

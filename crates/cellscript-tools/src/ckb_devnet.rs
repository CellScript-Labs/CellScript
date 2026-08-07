use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use blake2b_ref::Blake2bBuilder;
use ckb_types::{bytes::Bytes, packed::WitnessArgs, prelude::*};
use k256::schnorr::SigningKey;
use regex::Regex;
use reqwest::blocking::{Client, ClientBuilder};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use wait_timeout::ChildExt;

pub const CKB_PERSONAL: &[u8] = b"ckb-default-hash";
pub const PACKED_HASH_DOMAIN: &[u8] = b"CellScriptPackedHashV0\0";
pub const ALWAYS_SUCCESS_CODE_HASH: &str = "0x28e83a1277d48add8e72fadaa9248559e1b632bab2bd60b27955ebc4c03800a5";
pub const ALWAYS_SUCCESS_INDEX: u64 = 5;
pub const SHANNONS: u64 = 100_000_000;
pub const STATE_CAPACITY: u64 = 1_000 * SHANNONS;
pub const RECEIPT_CAPACITY: u64 = 1_000 * SHANNONS;
pub const ZERO_HASH: [u8; 32] = [0; 32];
pub const TEST_SECRET_KEY: [u8; 32] = hex_literal::hex!("3e7490680639a2f7bbe8361dd3f34eb6429a9c924d8b342c015e555e628f94e5");
pub const TEST_AUX_RAND: [u8; 32] = [0x42; 32];

#[derive(Debug)]
pub struct RpcFailure {
    message: String,
    pub error: Value,
}

impl Display for RpcFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RpcFailure {}

pub fn ckb_hash(data: &[u8]) -> [u8; 32] {
    let mut state = Blake2bBuilder::new(32).personal(CKB_PERSONAL).build();
    state.update(data);
    let mut result = [0_u8; 32];
    state.finalize(&mut result);
    result
}

pub fn ckb_hash_hex(data: &[u8]) -> String {
    hex0x(&ckb_hash(data))
}

pub fn sha256_hex(data: &[u8]) -> String {
    format!("0x{}", hex::encode(Sha256::digest(data)))
}

pub fn hex0x(data: &[u8]) -> String {
    format!("0x{}", hex::encode(data))
}

pub fn entry_witness_input_type_hex(payload: &[u8]) -> String {
    let witness = WitnessArgs::new_builder().input_type(Some(Bytes::copy_from_slice(payload)).pack()).build();
    hex0x(witness.as_slice())
}

pub fn decode_hex(value: &str) -> Result<Vec<u8>> {
    Ok(hex::decode(value.strip_prefix("0x").unwrap_or(value))?)
}

pub fn u8_bytes(value: u64) -> Vec<u8> {
    vec![value as u8]
}

pub fn u16_bytes(value: u64) -> Vec<u8> {
    (value as u16).to_le_bytes().to_vec()
}

pub fn u32_bytes(value: usize) -> Vec<u8> {
    (value as u32).to_le_bytes().to_vec()
}

pub fn u64_bytes(value: u64) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

pub fn packed_hash(type_name: &str, packed: &[u8]) -> [u8; 32] {
    let mut preimage = Vec::with_capacity(PACKED_HASH_DOMAIN.len() + type_name.len() + 5 + packed.len());
    preimage.extend_from_slice(PACKED_HASH_DOMAIN);
    preimage.extend_from_slice(type_name.as_bytes());
    preimage.push(0);
    preimage.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    preimage.extend_from_slice(packed);
    ckb_hash(&preimage)
}

pub fn xonly_pubkey(secret: &[u8; 32]) -> Result<[u8; 32]> {
    let key = SigningKey::from_bytes(secret).map_err(|error| anyhow::anyhow!("invalid BIP340 secret key: {error}"))?;
    Ok(key.verifying_key().to_bytes().into())
}

pub fn schnorr_sign(message: &[u8; 32], secret: &[u8; 32], aux: &[u8; 32]) -> Result<([u8; 32], [u8; 64])> {
    let key = SigningKey::from_bytes(secret).map_err(|error| anyhow::anyhow!("invalid BIP340 secret key: {error}"))?;
    let signature = key.sign_prehash_with_aux_rand(message, aux).map_err(|error| anyhow::anyhow!("BIP340 signing failed: {error}"))?;
    Ok((key.verifying_key().to_bytes().into(), signature.to_bytes()))
}

fn display_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn collect_source_files(root: &Path, path: &Path, files: &mut BTreeSet<PathBuf>, invalid: &mut BTreeSet<String>) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        invalid.insert(display_path(path, root));
        return Ok(());
    }
    if metadata.is_file() {
        files.insert(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let child = entry.path();
        let relative = child.strip_prefix(path).unwrap_or(&child);
        if relative.components().any(|component| matches!(component.as_os_str().to_str(), Some("target" | "build" | ".git"))) {
            continue;
        }
        let metadata = fs::symlink_metadata(&child)?;
        if metadata.file_type().is_symlink() {
            invalid.insert(display_path(&child, root));
            continue;
        }
        if metadata.is_dir() {
            collect_source_files(root, &child, files, invalid)?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let extension = child.extension().and_then(|value| value.to_str());
        if matches!(extension, Some("cell" | "schema" | "toml" | "json" | "rs"))
            || child.file_name().is_some_and(|value| value == "Cargo.lock")
        {
            files.insert(child);
        }
    }
    Ok(())
}

pub fn source_tree_hash(root: &Path, paths: &[PathBuf]) -> Result<Value> {
    let mut files = BTreeSet::new();
    let mut invalid = BTreeSet::new();
    for raw in paths {
        let path = if raw.is_absolute() { raw.clone() } else { root.join(raw) };
        collect_source_files(root, &path, &mut files, &mut invalid)?;
    }
    let mut hasher = Sha256::new();
    let mut rows = Vec::new();
    for path in files {
        let relative = display_path(&path, root);
        let digest = Sha256::digest(fs::read(&path)?);
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(digest);
        rows.push(relative);
    }
    Ok(json!({
        "sha256": if invalid.is_empty() { Value::String(format!("0x{}", hex::encode(hasher.finalize()))) } else { Value::Null },
        "files": rows, "file_count": rows.len(), "valid": invalid.is_empty(), "invalid_paths": invalid
    }))
}

pub fn provenance(root: &Path, source_paths: &[PathBuf], artifacts: &BTreeMap<String, PathBuf>) -> Result<Value> {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let mut artifact_rows = Map::new();
    for (name, path) in artifacts {
        let bytes = fs::read(path)?;
        artifact_rows.insert(name.clone(), json!({
            "path": display_path(path, root), "sha256": sha256_hex(&bytes), "ckb_data_hash": ckb_hash_hex(&bytes), "size_bytes": bytes.len()
        }));
    }
    Ok(json!({"repo_commit": commit, "source_tree": source_tree_hash(root, source_paths)?, "artifacts": artifact_rows}))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn pick_port() -> Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

pub fn resolve_ckb_bin(repo: &Path, configured: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = configured {
        if !path.is_file() {
            bail!("CKB binary is not executable: {}", path.display());
        }
        return Ok(fs::canonicalize(path)?);
    }
    for path in [repo.join("target/debug/ckb"), repo.join("target/release/ckb")] {
        if path.is_file() {
            return Ok(fs::canonicalize(path)?);
        }
    }
    bail!("no CKB binary found under {}; pass --ckb-bin", repo.display())
}

fn patch_config(path: &Path, rpc: u16, p2p: u16) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let rpc_pattern = Regex::new(r#"listen_address = "127\.0\.0\.1:\d+""#)?;
    let p2p_pattern = Regex::new(r#"listen_addresses = \["/ip4/0\.0\.0\.0/tcp/\d+"\]"#)?;
    let text = rpc_pattern.replacen(&text, 1, format!("listen_address = \"127.0.0.1:{rpc}\"").as_str());
    let text = p2p_pattern.replacen(&text, 1, format!("listen_addresses = [\"/ip4/127.0.0.1/tcp/{p2p}\"]").as_str());
    fs::write(path, text.as_bytes())?;
    Ok(())
}

pub struct CkbDevnet {
    pub ckb_repo: PathBuf,
    pub ckb_bin: PathBuf,
    pub ckb_dir: PathBuf,
    pub log_path: PathBuf,
    pub rpc_url: String,
    client: Client,
    process: Option<Child>,
    reserved: BTreeSet<(String, u64)>,
}

impl CkbDevnet {
    pub fn new(ckb_repo: PathBuf, ckb_bin: PathBuf, run_dir: PathBuf) -> Result<Self> {
        let rpc = pick_port()?;
        let p2p = pick_port()?;
        let ckb_dir = run_dir.join("ckb-node");
        let log_path = run_dir.join("ckb.log");
        let client = ClientBuilder::new().no_proxy().timeout(Duration::from_secs(20)).build()?;
        let mut devnet = Self {
            ckb_repo,
            ckb_bin,
            ckb_dir,
            log_path,
            rpc_url: format!("http://127.0.0.1:{rpc}"),
            client,
            process: None,
            reserved: BTreeSet::new(),
        };
        devnet.prepare(rpc, p2p)?;
        Ok(devnet)
    }

    fn prepare(&mut self, rpc: u16, p2p: u16) -> Result<()> {
        let template = self.ckb_repo.join("test/template");
        if !template.is_dir() {
            bail!("CKB test template not found: {}", template.display());
        }
        fs::create_dir_all(self.ckb_dir.parent().context("CKB directory has no parent")?)?;
        if self.ckb_dir.exists() {
            bail!("CKB run directory already exists: {}", self.ckb_dir.display());
        }
        copy_tree(&template, &self.ckb_dir)?;
        patch_config(&self.ckb_dir.join("ckb.toml"), rpc, p2p)
    }

    pub fn start(&mut self) -> Result<()> {
        let log = File::create(&self.log_path)?;
        self.process = Some(
            Command::new(&self.ckb_bin)
                .args(["-C", self.ckb_dir.to_str().unwrap(), "run", "--ba-advanced"])
                .stdout(Stdio::from(log.try_clone()?))
                .stderr(Stdio::from(log))
                .spawn()?,
        );
        for _ in 0..80 {
            if self.rpc("get_tip_header", vec![]).is_ok() {
                return Ok(());
            }
            if self.process.as_mut().and_then(|process| process.try_wait().ok()).flatten().is_some() {
                bail!("CKB process exited early; see {}", self.log_path.display());
            }
            thread::sleep(Duration::from_millis(250));
        }
        bail!("CKB RPC did not become ready at {}; see {}", self.rpc_url, self.log_path.display())
    }

    pub fn stop(&mut self) {
        let Some(process) = self.process.as_mut() else { return };
        if process.try_wait().ok().flatten().is_none() {
            let _ = Command::new("kill").args(["-TERM", &process.id().to_string()]).status();
            if process.wait_timeout(Duration::from_secs(5)).ok().flatten().is_none() {
                let _ = process.kill();
                let _ = process.wait();
            }
        }
    }

    pub fn rpc(&self, method: &str, params: Vec<Value>) -> Result<Value> {
        let mut last = String::new();
        for attempt in 0..6 {
            match self.client.post(&self.rpc_url).json(&json!({"id": 42, "jsonrpc": "2.0", "method": method, "params": params})).send()
            {
                Ok(response) => {
                    let payload: Value = response.json()?;
                    if !payload["error"].is_null() {
                        return Err(RpcFailure {
                            message: format!("RPC {method} returned error: {}", payload["error"]),
                            error: payload["error"].clone(),
                        }
                        .into());
                    }
                    return Ok(payload.get("result").cloned().unwrap_or(Value::Null));
                }
                Err(error) => last = error.to_string(),
            }
            thread::sleep(Duration::from_millis(250 * (attempt + 1)));
        }
        bail!("RPC {method} failed after retries: {last}")
    }

    pub fn get_block(&self, hash: &str) -> Result<Value> {
        for _ in 0..20 {
            let block = self.rpc("get_block", vec![json!(hash)])?;
            if !block.is_null() {
                return Ok(block);
            }
            thread::sleep(Duration::from_millis(50));
        }
        bail!("block not found: {hash}")
    }

    pub fn get_block_by_number(&self, number: u64) -> Result<Value> {
        let block = self.rpc("get_block_by_number", vec![json!(format!("0x{number:x}"))])?;
        if block.is_null() {
            bail!("block number not found: {number}");
        }
        Ok(block)
    }

    pub fn wait_live_cell(&self, hash: &str, index: u64) -> Result<Value> {
        let mut last = Value::Null;
        for _ in 0..40 {
            last = self.rpc("get_live_cell", vec![out_point(hash, index), json!(true)])?;
            if last["status"] == "live" {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(50));
        }
        bail!("cell is not live: {hash}:{index}; last={last}")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn assert_live_cell(
        &self,
        hash: &str,
        index: u64,
        label: &str,
        capacity: Option<u64>,
        lock: Option<&Value>,
        type_script: Option<&Value>,
        data: Option<&[u8]>,
    ) -> Result<Value> {
        let live = self.wait_live_cell(hash, index)?;
        let output = &live["cell"]["output"];
        let actual_data = &live["cell"]["data"];
        if let Some(expected) = capacity {
            let actual = output["capacity"]
                .as_str()
                .and_then(|value| u64::from_str_radix(value.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0);
            if actual != expected {
                bail!("{label} capacity mismatch: {} != 0x{expected:x}", output["capacity"]);
            }
        }
        if let Some(expected) = lock
            && &output["lock"] != expected
        {
            bail!("{label} lock mismatch: {} != {expected}", output["lock"]);
        }
        if let Some(expected) = type_script
            && &output["type"] != expected
        {
            bail!("{label} type mismatch: {} != {expected}", output["type"]);
        }
        if let Some(expected) = data {
            if actual_data["content"] != hex0x(expected) {
                bail!("{label} data content mismatch");
            }
            let expected_hash = ckb_hash_hex(expected);
            if actual_data["hash"] != expected_hash {
                bail!("{label} data hash mismatch: {} != {expected_hash}", actual_data["hash"]);
            }
        }
        Ok(live)
    }

    pub fn wait_dead_cell(&self, hash: &str, index: u64) -> Result<Value> {
        let mut last = Value::Null;
        for _ in 0..40 {
            last = self.rpc("get_live_cell", vec![out_point(hash, index), json!(false)])?;
            if !last.is_null() && last["status"] != "live" {
                return Ok(last);
            }
            thread::sleep(Duration::from_millis(50));
        }
        bail!("cell is still live: {hash}:{index}; last={last}")
    }

    pub fn find_spendable(&mut self) -> Result<Value> {
        for _ in 0..80 {
            let hash = self.rpc("generate_block", vec![])?.as_str().context("generate_block returned no hash")?.to_owned();
            let block = self.get_block(&hash)?;
            let cellbase = &block["transactions"][0];
            let tx_hash = cellbase["hash"].as_str().context("cellbase hash missing")?;
            for (index, output) in cellbase["outputs"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().enumerate() {
                let capacity = output["capacity"]
                    .as_str()
                    .and_then(|value| u64::from_str_radix(value.trim_start_matches("0x"), 16).ok())
                    .unwrap_or(0);
                if capacity > 0 && self.reserved.insert((tx_hash.into(), index as u64)) {
                    self.wait_live_cell(tx_hash, index as u64)?;
                    return Ok(json!({"tx_hash": tx_hash, "index": index, "capacity": capacity}));
                }
            }
        }
        bail!("no spendable cellbase found")
    }

    pub fn collect_spendable(&mut self, minimum: u64) -> Result<Value> {
        let mut cells = Vec::new();
        let mut total = 0;
        while total < minimum {
            let cell = self.find_spendable()?;
            total += cell["capacity"].as_u64().unwrap();
            cells.push(cell);
        }
        Ok(json!({"cells": cells, "total_capacity": total}))
    }

    pub fn submit_and_commit(&self, tx: &Value, label: &str) -> Result<Value> {
        let hash = self
            .rpc("send_test_transaction", vec![tx.clone(), json!("passthrough")])?
            .as_str()
            .context("send_test_transaction returned no hash")?
            .to_owned();
        let mut last = Value::Null;
        for generated in 0..80 {
            let status = self.rpc("get_transaction", vec![json!(hash)])?;
            last = status.get("tx_status").cloned().unwrap_or_else(|| json!({}));
            if last["status"] == "committed" {
                return Ok(json!({"tx_hash": hash, "generated_blocks_after_submit": generated, "status": last}));
            }
            if last["status"] == "rejected" {
                bail!("{label} rejected: {hash}; status={last}");
            }
            self.rpc("generate_block", vec![])?;
            thread::sleep(Duration::from_millis(50));
        }
        bail!("{label} not committed: {hash}; last_status={last}")
    }

    pub fn dry_run(&self, tx: &Value) -> Result<Value> {
        self.rpc("dry_run_transaction", vec![tx.clone()])
    }

    pub fn dry_run_rejects(
        &self,
        tx: &Value,
        label: &str,
        source: Option<&str>,
        data_hash: Option<&str>,
        error_code: Option<i64>,
    ) -> Result<Value> {
        match self.rpc("dry_run_transaction", vec![tx.clone()]) {
            Ok(value) => bail!("{label} unexpectedly passed dry-run: {value}"),
            Err(error) => {
                let reason = error.to_string();
                let rpc = error.downcast_ref::<RpcFailure>();
                let mut checks = Map::new();
                if let Some(expected) = source {
                    checks.insert("source".into(), json!(reason.contains(expected)));
                }
                if let Some(expected) = data_hash {
                    checks.insert(
                        "data_hash".into(),
                        json!(reason.to_lowercase().contains(expected.trim_start_matches("0x").to_lowercase().as_str())),
                    );
                }
                if let Some(expected) = error_code {
                    checks.insert("error_code".into(), json!(script_error_matches(&reason, rpc.map(|value| &value.error), expected)));
                }
                let matched = checks.values().all(|value| value == true);
                if !matched {
                    bail!("{label} rejected for unexpected reason: checks={} reason={reason}", Value::Object(checks));
                }
                Ok(json!({"status": "rejected", "label": label, "reason": reason,
                    "expected": {"source": source, "data_hash": data_hash, "error_code": error_code}, "matched_expected": matched}))
            }
        }
    }
}

impl Drop for CkbDevnet {
    fn drop(&mut self) {
        self.stop();
    }
}

fn script_error_value(value: &Value, keys: &[&str]) -> Option<i64> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if keys.contains(&key.as_str())
                    && let Some(number) = value.as_i64().or_else(|| value.as_str().and_then(|value| value.parse().ok()))
                {
                    return Some(number);
                }
                if let Some(found) = script_error_value(value, keys) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(values) => values.iter().find_map(|value| script_error_value(value, keys)),
        _ => None,
    }
}

fn script_error_matches(reason: &str, error: Option<&Value>, expected: i64) -> bool {
    let keys = ["error_code", "errorCode", "exit_code", "exitCode", "script_error_code", "scriptErrorCode"];
    if error.and_then(|value| script_error_value(value, &keys)) == Some(expected) {
        return true;
    }
    [
        format!(r"\berror code\s*[:#]?\s*{expected}\b"),
        format!(r"\berror_code\s*[:=]\s*{expected}\b"),
        format!(r"\bexit[_ ]?code\s*[:=]\s*{expected}\b"),
        format!(r"\bExitCode\(\s*{expected}\s*\)"),
        format!(r"#{expected}\b"),
    ]
    .iter()
    .any(|pattern| Regex::new(pattern).is_ok_and(|regex| regex.is_match(reason)))
}

pub fn out_point(hash: &str, index: u64) -> Value {
    json!({"tx_hash": hash, "index": format!("0x{index:x}")})
}
pub fn always_success_dep(genesis: &str) -> Value {
    json!({"out_point": out_point(genesis, ALWAYS_SUCCESS_INDEX), "dep_type": "code"})
}
pub fn always_success_lock(args: &str) -> Value {
    json!({"code_hash": ALWAYS_SUCCESS_CODE_HASH, "hash_type": "data", "args": args})
}

pub fn transaction(
    inputs: &[Value],
    outputs: Vec<Value>,
    outputs_data: Vec<String>,
    deps: Vec<Value>,
    witnesses: Vec<String>,
    headers: Vec<String>,
) -> Value {
    json!({"version": "0x0", "cell_deps": deps, "header_deps": headers,
        "inputs": inputs.iter().map(|cell| json!({"previous_output": out_point(cell["tx_hash"].as_str().unwrap(), cell["index"].as_u64().unwrap()), "since": "0x0"})).collect::<Vec<_>>(),
        "outputs": outputs, "outputs_data": outputs_data, "witnesses": witnesses})
}

pub fn funding_cells(funding: &Value) -> &[Value] {
    funding["cells"].as_array().map(Vec::as_slice).unwrap_or(&[])
}

pub fn deploy_code(devnet: &mut CkbDevnet, name: &str, artifact: &[u8], always_dep: &Value) -> Result<Value> {
    let funding = devnet.collect_spendable((artifact.len() as u64 + 1_000) * SHANNONS)?;
    let cells = funding_cells(&funding);
    let total = funding["total_capacity"].as_u64().unwrap();
    let tx = transaction(
        cells,
        vec![json!({"capacity": format!("0x{total:x}"), "lock": always_success_lock("0x"), "type": Value::Null})],
        vec![hex0x(artifact)],
        vec![always_dep.clone()],
        vec!["0x".into(); cells.len()],
        vec![],
    );
    let dry_run = devnet.dry_run(&tx)?;
    let commit = devnet.submit_and_commit(&tx, &format!("deploy {name}"))?;
    devnet.assert_live_cell(
        commit["tx_hash"].as_str().unwrap(),
        0,
        &format!("deploy {name}"),
        Some(total),
        Some(&always_success_lock("0x")),
        Some(&Value::Null),
        Some(artifact),
    )?;
    Ok(json!({"name": name, "artifact_size_bytes": artifact.len(), "data_hash": ckb_hash_hex(artifact),
        "cell_dep": {"out_point": out_point(commit["tx_hash"].as_str().unwrap(), 0), "dep_type": "code"},
        "valid_deploy_dry_run": dry_run, "commit": commit}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_witness_helper_places_payload_in_input_type() {
        let payload = b"CSARGv1\0payload";
        let encoded = decode_hex(&entry_witness_input_type_hex(payload)).unwrap();
        let witness = WitnessArgs::from_slice(&encoded).unwrap();

        assert!(witness.lock().to_opt().is_none());
        assert_eq!(witness.input_type().to_opt().unwrap().raw_data(), Bytes::from_static(payload));
        assert!(witness.output_type().to_opt().is_none());
    }
}

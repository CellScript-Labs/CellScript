use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::ckb_devnet::{
    always_success_dep, always_success_lock, ckb_hash, deploy_code, funding_cells, hex0x, packed_hash, provenance, resolve_ckb_bin,
    schnorr_sign, transaction, u16_bytes, u32_bytes, u64_bytes, u8_bytes, xonly_pubkey, CkbDevnet, RECEIPT_CAPACITY, STATE_CAPACITY,
    TEST_AUX_RAND, TEST_SECRET_KEY, ZERO_HASH,
};
use crate::shared::{stable_json_pretty, stable_json_spaced};

const VERSION: u64 = 0;
const OP_BOOTSTRAP: u64 = 0;
const OP_TRANSITION: u64 = 1;

#[derive(Clone)]
struct CoreState {
    authority: [u8; 32],
    state: [u8; 32],
    policy: [u8; 32],
    receipt: [u8; 32],
    nonce: u64,
    expiry: u64,
}

struct TransitionMaterial {
    flat_header: Vec<u8>,
    signed_intent: Vec<u8>,
    signed_intent_hash: [u8; 32],
    state_hash_commitment: [u8; 32],
    signature_payload: Vec<u8>,
    new_cell_data: Vec<u8>,
    receipt_data: Vec<u8>,
    receipt_hash: [u8; 32],
    new_state_hash: [u8; 32],
}

fn append(target: &mut Vec<u8>, chunks: &[&[u8]]) {
    for chunk in chunks {
        target.extend_from_slice(chunk);
    }
}

fn pack_cell(state: &CoreState) -> Vec<u8> {
    let mut value = u16_bytes(VERSION);
    append(
        &mut value,
        &[&state.authority, &state.state, &state.policy, &state.receipt, &u64_bytes(state.nonce), &u64_bytes(state.expiry)],
    );
    value
}

fn pack_outpoint(hash: &str, index: u64) -> Result<Vec<u8>> {
    let mut bytes = crate::ckb_devnet::decode_hex(hash)?;
    if bytes.len() != 32 {
        bail!("tx hash must be 32 bytes: {hash}");
    }
    bytes.extend_from_slice(&(index as u32).to_le_bytes());
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
fn intent_core(
    protocol: &[u8; 32],
    package: &[u8; 32],
    policy: &[u8; 32],
    hash: &str,
    index: u64,
    old: &[u8; 32],
    new: &[u8; 32],
    old_nonce: u64,
    new_nonce: u64,
    expiry: u64,
) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    append(
        &mut value,
        &[
            protocol,
            package,
            policy,
            &u8_bytes(OP_TRANSITION),
            &u8_bytes(OP_TRANSITION),
            &pack_outpoint(hash, index)?,
            old,
            new,
            &u64_bytes(old_nonce),
            &u64_bytes(new_nonce),
            &u64_bytes(expiry),
        ],
    );
    Ok(value)
}

fn cell_commitment(state: &CoreState, new_hash: &[u8; 32]) -> Vec<u8> {
    let mut value = u16_bytes(VERSION);
    append(&mut value, &[&state.authority, new_hash, &state.policy, &u64_bytes(state.nonce + 1), &u64_bytes(state.expiry)]);
    value
}

#[allow(clippy::too_many_arguments)]
fn receipt_commitment(
    protocol: &[u8; 32],
    package: &[u8; 32],
    state: &CoreState,
    hash: &str,
    index: u64,
    new_cell: &[u8; 32],
    new_state: &[u8; 32],
    intent_hash: &[u8; 32],
) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    append(
        &mut value,
        &[
            protocol,
            package,
            &state.policy,
            &u8_bytes(OP_TRANSITION),
            &u8_bytes(OP_TRANSITION),
            &pack_outpoint(hash, index)?,
            new_cell,
            &state.state,
            new_state,
            &u64_bytes(state.nonce),
            &u64_bytes(state.nonce + 1),
            intent_hash,
            &ZERO_HASH,
        ],
    );
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn receipt(
    protocol: &[u8; 32],
    package: &[u8; 32],
    state: &CoreState,
    hash: &str,
    index: u64,
    new_cell: &[u8; 32],
    new_state: &[u8; 32],
    intent_hash: &[u8; 32],
    signed_hash: &[u8; 32],
) -> Result<Vec<u8>> {
    let mut value = Vec::new();
    append(
        &mut value,
        &[
            protocol,
            package,
            &state.policy,
            &u8_bytes(OP_TRANSITION),
            &u8_bytes(OP_TRANSITION),
            &pack_outpoint(hash, index)?,
            new_cell,
            &state.state,
            new_state,
            &u64_bytes(state.nonce),
            &u64_bytes(state.nonce + 1),
            intent_hash,
            signed_hash,
            &ZERO_HASH,
            &state.authority,
            &u64_bytes(state.expiry),
        ],
    );
    Ok(value)
}

fn material(old_hash: &str, old_index: u64, old: &CoreState, new_state: [u8; 32]) -> Result<TransitionMaterial> {
    let protocol = ckb_hash(b"NovaSeal/core/v0");
    let package = ckb_hash(b"NovaSeal/devnet/stateful/live");
    let new_cell = packed_hash("NovaSealCellCommitmentV0", &cell_commitment(old, &new_state));
    let core = intent_core(
        &protocol,
        &package,
        &old.policy,
        old_hash,
        old_index,
        &old.state,
        &new_state,
        old.nonce,
        old.nonce + 1,
        old.expiry,
    )?;
    let intent_hash = packed_hash("NovaSealIntentCoreV0", &core);
    let commitment = receipt_commitment(&protocol, &package, old, old_hash, old_index, &new_cell, &new_state, &intent_hash)?;
    let receipt_hash = packed_hash("ProofReceiptCommitmentV0", &commitment);
    let mut signed_intent = core.clone();
    signed_intent.extend_from_slice(&receipt_hash);
    let signed_intent_hash = packed_hash("NovaSealSignedIntentV0", &signed_intent);
    let state_hash_commitment = ckb_hash(&new_state);
    let (pubkey, signature) = schnorr_sign(&state_hash_commitment, &TEST_SECRET_KEY, &TEST_AUX_RAND)?;
    if pubkey != old.authority {
        bail!("derived pubkey does not match old cell authority hash");
    }
    let next = CoreState {
        authority: old.authority,
        state: new_state,
        policy: old.policy,
        receipt: receipt_hash,
        nonce: old.nonce + 1,
        expiry: old.expiry,
    };
    let receipt_data =
        receipt(&protocol, &package, old, old_hash, old_index, &new_cell, &new_state, &intent_hash, &signed_intent_hash)?;
    let old_hash_bytes: [u8; 32] = crate::ckb_devnet::decode_hex(old_hash)?
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("old hash has {} bytes", bytes.len()))?;
    let mut flat = Vec::new();
    append(
        &mut flat,
        &[
            &protocol,
            &package,
            &old.policy,
            &old_hash_bytes,
            &old.state,
            &new_state,
            &u64_bytes(old.nonce),
            &u64_bytes(old.nonce + 1),
            &u64_bytes(old.expiry),
        ],
    );
    let mut payload = Vec::with_capacity(96);
    payload.extend_from_slice(&pubkey);
    payload.extend_from_slice(&signature);
    Ok(TransitionMaterial {
        flat_header: flat,
        signed_intent,
        signed_intent_hash,
        state_hash_commitment,
        signature_payload: payload,
        new_cell_data: pack_cell(&next),
        receipt_data,
        receipt_hash,
        new_state_hash: new_state,
    })
}

fn witness(
    op: u64,
    old_cell: &[u8],
    signed: &[u8],
    state_commitment: &[u8; 32],
    signature: &[u8],
    flat: Option<&[u8]>,
) -> Result<String> {
    if signature.len() != 96 {
        bail!("entry witness expects 32-byte pubkey plus 64-byte signature");
    }
    let fallback = vec![0_u8; 216];
    let flat = flat.unwrap_or(&fallback);
    let mut payload = b"CSARGv1\0".to_vec();
    append(
        &mut payload,
        &[
            &u8_bytes(op),
            state_commitment,
            signature,
            &u32_bytes(flat.len()),
            flat,
            &u32_bytes(old_cell.len()),
            old_cell,
            &u32_bytes(signed.len()),
            signed,
        ],
    );
    Ok(hex0x(&payload))
}

fn compile(root: &Path, output: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--locked",
            "--bin",
            "cellc",
            "--",
            "proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell",
            "--target-profile",
            "ckb",
            "--target",
            "riscv64-elf",
            "--entry-action",
            "novaseal_lifecycle",
            "-o",
            output.to_str().unwrap(),
        ])
        .current_dir(root)
        .status()?;
    if !status.success() {
        bail!("failed to compile NovaSeal lifecycle");
    }
    Ok(())
}

fn bootstrap(funding: &Value, lifecycle_hash: &str, deps: Vec<Value>, header: &str, data: &[u8]) -> Result<Value> {
    let total = funding["total_capacity"].as_u64().unwrap();
    let change = total.checked_sub(STATE_CAPACITY).context("bootstrap funding capacity is too small")?;
    if change == 0 {
        bail!("bootstrap funding capacity is too small");
    }
    let type_script = json!({"code_hash": lifecycle_hash, "hash_type": "data2", "args": "0x"});
    let witness = witness(OP_BOOTSTRAP, data, &[0_u8; 254], &ZERO_HASH, &[0_u8; 96], None)?;
    let cells = funding_cells(funding);
    let mut witnesses = vec![witness];
    witnesses.extend(vec!["0x".into(); cells.len().saturating_sub(1)]);
    Ok(transaction(
        cells,
        vec![
            json!({"capacity": format!("0x{STATE_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": type_script}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    ))
}

#[allow(clippy::too_many_arguments)]
fn transition(
    old_ref: &Value,
    old: &CoreState,
    lifecycle_hash: &str,
    deps: Vec<Value>,
    header: &str,
    funding: &Value,
    new_hash: [u8; 32],
    mutate: bool,
) -> Result<(Value, CoreState, TransitionMaterial)> {
    let old_data = pack_cell(old);
    let material = material(old_ref["tx_hash"].as_str().unwrap(), old_ref["index"].as_u64().unwrap(), old, new_hash)?;
    let mut signature = material.signature_payload.clone();
    if mutate {
        *signature.last_mut().unwrap() ^= 1;
    }
    let witness = witness(
        OP_TRANSITION,
        &old_data,
        &material.signed_intent,
        &material.state_hash_commitment,
        &signature,
        Some(&material.flat_header),
    )?;
    let total = funding["total_capacity"].as_u64().unwrap();
    let change = total.checked_sub(RECEIPT_CAPACITY).context("transition funding capacity is too small")?;
    if change == 0 {
        bail!("transition funding capacity is too small");
    }
    let type_script = json!({"code_hash": lifecycle_hash, "hash_type": "data2", "args": "0x"});
    let mut inputs = vec![old_ref.clone()];
    inputs.extend_from_slice(funding_cells(funding));
    let mut witnesses = vec![witness];
    witnesses.extend(vec!["0x".into(); funding_cells(funding).len()]);
    let tx = transaction(
        &inputs,
        vec![
            json!({"capacity": format!("0x{:x}", old_ref["capacity"].as_u64().unwrap()), "lock": always_success_lock("0x"), "type": type_script}),
            json!({"capacity": format!("0x{RECEIPT_CAPACITY:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
            json!({"capacity": format!("0x{change:x}"), "lock": always_success_lock("0x"), "type": Value::Null}),
        ],
        vec![hex0x(&material.new_cell_data), hex0x(&material.receipt_data), "0x".into()],
        deps,
        witnesses,
        vec![header.into()],
    );
    let next = CoreState {
        authority: old.authority,
        state: material.new_state_hash,
        policy: old.policy,
        receipt: material.receipt_hash,
        nonce: old.nonce + 1,
        expiry: old.expiry,
    };
    Ok((tx, next, material))
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    root: &Path,
    ckb_repo: Option<&Path>,
    ckb_bin: Option<&Path>,
    output: Option<&Path>,
    run_dir: Option<&Path>,
    pretty: bool,
    keep_node: bool,
) -> Result<i32> {
    let root = fs::canonicalize(root)?;
    let ckb_repo = fs::canonicalize(ckb_repo.map(Path::to_path_buf).unwrap_or_else(|| root.parent().unwrap().join("ckb")))?;
    let ckb_bin = resolve_ckb_bin(&ckb_repo, ckb_bin)?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let run_dir =
        run_dir.map(Path::to_path_buf).unwrap_or_else(|| root.join(format!("target/novaseal-devnet-stateful-live/{timestamp}")));
    fs::create_dir_all(&run_dir)?;
    let run_dir = fs::canonicalize(run_dir)?;
    let lifecycle_path = run_dir.join("novaseal-lifecycle-type.elf");
    compile(&root, &lifecycle_path)?;
    let verifier_path = root.join("proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf");
    if !verifier_path.is_file() {
        bail!("missing verifier ELF: {}", verifier_path.display());
    }
    let mut devnet = CkbDevnet::new(ckb_repo.clone(), ckb_bin.clone(), run_dir.clone())?;
    let mut report = json!({"schema": "novaseal-devnet-stateful-live-v0.1", "status": "running",
        "scenario": "core_bootstrap_then_key_auth_transition", "repo_root": root.display().to_string(), "ckb_repo": ckb_repo.display().to_string(),
        "ckb_bin": ckb_bin.display().to_string(), "run_dir": run_dir.display().to_string()});
    let scenario = (|| -> Result<()> {
        devnet.start()?;
        let genesis = devnet.get_block_by_number(0)?;
        let always = always_success_dep(genesis["transactions"][0]["hash"].as_str().unwrap());
        let verifier_bytes = fs::read(&verifier_path)?;
        let lifecycle_bytes = fs::read(&lifecycle_path)?;
        let verifier = deploy_code(&mut devnet, "cellscript_btc_bip340_verifier_riscv", &verifier_bytes, &always)?;
        let lifecycle = deploy_code(&mut devnet, "novaseal_lifecycle_type", &lifecycle_bytes, &always)?;
        let deps = vec![verifier["cell_dep"].clone(), lifecycle["cell_dep"].clone(), always];
        let source_paths = [
            "proposals/novaseal/v0-mvp-skeleton/Cell.toml",
            "proposals/novaseal/v0-mvp-skeleton/src",
            "proposals/novaseal/v0-mvp-skeleton/schemas",
            "proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier",
            "crates/cellscript-tools/src/novaseal_core_live.rs",
            "crates/cellscript-tools/src/ckb_devnet.rs",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
        let artifacts = BTreeMap::from([("verifier".into(), verifier_path.clone()), ("lifecycle".into(), lifecycle_path.clone())]);
        let source_provenance = provenance(&root, &source_paths, &artifacts)?;
        let header = devnet.rpc("get_tip_header", vec![])?["hash"].as_str().unwrap().to_owned();
        let initial = CoreState {
            authority: xonly_pubkey(&TEST_SECRET_KEY)?,
            state: ckb_hash(b"novaseal devnet initial state"),
            policy: ckb_hash(b"novaseal devnet policy"),
            receipt: ZERO_HASH,
            nonce: 0,
            expiry: (1_u64 << 63) - 1,
        };
        let initial_data = pack_cell(&initial);
        let bootstrap_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * crate::ckb_devnet::SHANNONS)?;
        let bootstrap_tx =
            bootstrap(&bootstrap_funding, lifecycle["data_hash"].as_str().unwrap(), deps.clone(), &header, &initial_data)?;
        fs::write(run_dir.join("bootstrap-tx.json"), format!("{}\n", stable_json_pretty(&bootstrap_tx)?))?;
        let bootstrap_dry = devnet.rpc("dry_run_transaction", vec![bootstrap_tx.clone()])?;
        let bootstrap_commit = devnet.submit_and_commit(&bootstrap_tx, "novaseal bootstrap")?;
        let type_script = json!({"code_hash": lifecycle["data_hash"], "hash_type": "data2", "args": "0x"});
        let bootstrap_live = devnet.assert_live_cell(
            bootstrap_commit["tx_hash"].as_str().unwrap(),
            0,
            "bootstrap state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&initial_data),
        )?;
        let old_ref = json!({"tx_hash": bootstrap_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY});
        let transition_header = devnet.rpc("get_tip_header", vec![])?["hash"].as_str().unwrap().to_owned();
        let transition_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * crate::ckb_devnet::SHANNONS)?;
        let (transition_tx, next, transition_material) = transition(
            &old_ref,
            &initial,
            lifecycle["data_hash"].as_str().unwrap(),
            deps.clone(),
            &transition_header,
            &transition_funding,
            ckb_hash(b"novaseal devnet state after transition"),
            false,
        )?;
        fs::write(run_dir.join("transition-tx.json"), format!("{}\n", stable_json_pretty(&transition_tx)?))?;
        let transition_dry = devnet.rpc("dry_run_transaction", vec![transition_tx.clone()])?;
        let transition_commit = devnet.submit_and_commit(&transition_tx, "novaseal key-auth transition")?;
        let bootstrap_dead = devnet.wait_dead_cell(bootstrap_commit["tx_hash"].as_str().unwrap(), 0)?;
        let new_live = devnet.assert_live_cell(
            transition_commit["tx_hash"].as_str().unwrap(),
            0,
            "transition new state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&transition_material.new_cell_data),
        )?;
        let receipt_live = devnet.assert_live_cell(
            transition_commit["tx_hash"].as_str().unwrap(),
            1,
            "transition receipt",
            Some(RECEIPT_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&Value::Null),
            Some(&transition_material.receipt_data),
        )?;
        let negative_header = devnet.rpc("get_tip_header", vec![])?["hash"].as_str().unwrap().to_owned();
        let negative_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * crate::ckb_devnet::SHANNONS)?;
        let negative_ref = json!({"tx_hash": transition_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY});
        let (negative_tx, _, _) = transition(
            &negative_ref,
            &next,
            lifecycle["data_hash"].as_str().unwrap(),
            deps,
            &negative_header,
            &negative_funding,
            ckb_hash(b"novaseal devnet rejected state"),
            true,
        )?;
        fs::write(run_dir.join("wrong-signature-tx.json"), format!("{}\n", stable_json_pretty(&negative_tx)?))?;
        let rejection = devnet.dry_run_rejects(
            &negative_tx,
            "wrong signature transition",
            Some("Inputs[0].Type"),
            lifecycle["data_hash"].as_str(),
            Some(56),
        )?;
        let still_live = devnet.assert_live_cell(
            transition_commit["tx_hash"].as_str().unwrap(),
            0,
            "post-negative state",
            Some(STATE_CAPACITY),
            Some(&always_success_lock("0x")),
            Some(&type_script),
            Some(&transition_material.new_cell_data),
        )?;
        report.as_object_mut().unwrap().extend(json!({"status": "passed", "live_devnet_rpc_executed": true, "stateful_lifecycle_executed": true,
            "ckb_log": devnet.log_path.display().to_string(), "rpc_url": devnet.rpc_url, "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
            "provenance": source_provenance,
            "bootstrap": {"dry_run_cycles": bootstrap_dry["cycles"], "commit": bootstrap_commit, "state_cell_live": bootstrap_live["status"] == "live", "state_data_hash": hex0x(&ckb_hash(&initial_data))},
            "transition": {"dry_run_cycles": transition_dry["cycles"], "commit": transition_commit, "old_state_not_live": bootstrap_dead["status"] != "live",
                "new_state_live": new_live["status"] == "live", "receipt_live": receipt_live["status"] == "live", "signed_intent_hash": hex0x(&transition_material.signed_intent_hash), "latest_receipt_hash": hex0x(&next.receipt)},
            "negative_cases": {"wrong_signature_dry_run": rejection, "post_negative_state_still_live": still_live["status"] == "live"}
        }).as_object().unwrap().clone());
        Ok(())
    })();
    if let Err(error) = scenario {
        report["status"] = json!("failed");
        report["error"] = json!(error.to_string());
        report["ckb_log"] = json!(devnet.log_path.display().to_string());
        report["rpc_url"] = json!(devnet.rpc_url);
    }
    if !keep_node {
        devnet.stop();
    }
    let output = match output {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => root.join(path),
        None => root.join("target/novaseal-devnet-stateful-live.json"),
    };
    fs::create_dir_all(output.parent().context("output path has no parent")?)?;
    let text = if pretty { stable_json_pretty(&report)? } else { stable_json_spaced(&report)? };
    fs::write(&output, format!("{text}\n"))?;
    println!(
        "wrote {} status={} live_devnet_rpc_executed={}",
        output.display(),
        report["status"].as_str().unwrap_or("failed"),
        report["live_devnet_rpc_executed"].as_bool().unwrap_or(false)
    );
    Ok(if report["status"] == "passed" { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_signer_matches_expected_xonly_key() {
        assert_eq!(
            hex0x(&xonly_pubkey(&TEST_SECRET_KEY).unwrap()),
            "0xc89fe99d72fcfa969434ddd87bb186a48213e9df3ec4b8a77042cf9559fc5765"
        );
    }
}

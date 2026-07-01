use anyhow::{bail, Result};
use cellscript_ckb_adapter::{
    build_action_transaction, build_deploy_transaction, load_action_plan, load_deployment_manifest, preview_resolved_action,
    resolve_materialized_action_plan, resolve_materialized_action_plan_with_manifest, CellScriptAdapter, DeployArtifactSpec,
};
use ckb_types::{
    bytes::Bytes,
    core::ScriptHashType,
    packed::{CellInput, OutPoint},
    prelude::*,
    H160,
};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cellscript-deploy")]
#[command(about = "CellScript CKB adapter CLI — deploy, act, and query on-chain state")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// CKB node RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8114", global = true)]
    rpc: String,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Deploy a compiled artifact as an on-chain code cell with TYPE_ID
    Deploy(DeployArgs),

    /// Build a headless deploy transaction without submitting
    BuildDeploy(BuildDeployArgs),

    /// Build a transaction from an action plan
    Action(ActionArgs),

    /// Query transaction status on-chain
    Status(StatusArgs),

    /// Show tip block number and node info
    Info,
}

#[derive(clap::Args, Debug)]
struct DeployArgs {
    /// Artifact binary file path (.s or .cell)
    #[arg(long)]
    artifact: PathBuf,

    /// Deployer lock script args (hex, 20 bytes for secp256k1-sighash)
    #[arg(long)]
    lock_arg: String,

    /// Name for the deployment (stored in manifest)
    #[arg(long, default_value = "cellscript-contract")]
    name: String,

    /// Fee in shannons
    #[arg(long, default_value_t = 1_000)]
    fee: u64,

    /// Capacity input out_point (format: 0x<TX_HASH>:<INDEX>)
    #[arg(long)]
    capacity_out_point: String,

    /// Capacity input shannons
    #[arg(long, default_value_t = 200_000_000_000)]
    capacity_shannons: u64,

    /// Max attempts to wait for commitment
    #[arg(long, default_value_t = 30)]
    wait_attempts: u32,

    /// Delay between commitment checks in milliseconds
    #[arg(long, default_value_t = 500)]
    wait_delay_ms: u64,

    /// Output path for deployment manifest JSON
    #[arg(long)]
    manifest_out: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
struct BuildDeployArgs {
    /// Artifact binary file path
    #[arg(long)]
    artifact: PathBuf,

    /// Deployer lock script args (hex, 20 bytes for secp256k1-sighash)
    #[arg(long)]
    lock_arg: String,

    /// Name for the deployment
    #[arg(long, default_value = "cellscript-contract")]
    name: String,

    /// Fee in shannons
    #[arg(long, default_value_t = 1_000)]
    fee: u64,

    /// Capacity input out_point (format: 0x<TX_HASH>:<INDEX>)
    #[arg(long)]
    capacity_out_point: String,

    /// Capacity input shannons
    #[arg(long, default_value_t = 200_000_000_000)]
    capacity_shannons: u64,
}

#[derive(clap::Args, Debug)]
struct ActionArgs {
    /// Path to action plan JSON
    #[arg(long)]
    plan: PathBuf,

    /// Path to deployment manifest JSON (for CellDep resolution)
    #[arg(long)]
    manifest: Option<PathBuf>,
}

#[derive(clap::Args, Debug)]
struct StatusArgs {
    /// Transaction hash (hex with 0x prefix)
    #[arg(long)]
    tx_hash: String,
}

fn main() {
    let cli = Cli::parse();
    let rpc = cli.rpc.clone();
    let json = cli.json;

    if let Err(e) = match cli.command {
        Commands::Deploy(args) => cmd_deploy(&rpc, json, args),
        Commands::BuildDeploy(args) => cmd_build_deploy(&rpc, json, args),
        Commands::Action(args) => cmd_action(&rpc, json, args),
        Commands::Status(args) => cmd_status(&rpc, json, args),
        Commands::Info => cmd_info(&rpc, json),
    } {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn parse_out_point(s: &str) -> Result<OutPoint> {
    let (tx_hash_hex, index_str) =
        s.rsplit_once(':').ok_or_else(|| anyhow::anyhow!("invalid out_point format: expected 0x<hash>:<index>"))?;
    let tx_hash_bytes = hex::decode(tx_hash_hex.trim_start_matches("0x"))?;
    if tx_hash_bytes.len() != 32 {
        bail!("out_point tx_hash must be 32 bytes, got {}", tx_hash_bytes.len());
    }
    let mut tx_hash_arr = [0u8; 32];
    tx_hash_arr.copy_from_slice(&tx_hash_bytes);
    let index: u32 = index_str.parse()?;
    Ok(OutPoint::new_builder().tx_hash(tx_hash_arr.pack()).index(index).build())
}

fn parse_lock_arg(s: &str) -> Result<H160> {
    let bytes = hex::decode(s.trim_start_matches("0x"))?;
    if bytes.len() != 20 {
        bail!("lock_arg must be 20 bytes, got {}", bytes.len());
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(H160::from(arr))
}

/// Shared spec builder for deploy and build-deploy.
fn build_deploy_spec(
    artifact: PathBuf,
    lock_arg: String,
    name: String,
    fee: u64,
    capacity_out_point: String,
    capacity_shannons: u64,
) -> Result<DeployArtifactSpec> {
    let artifact_binary = std::fs::read(&artifact)?;
    let artifact_binary = Bytes::from(artifact_binary);
    let artifact_hash = ckb_hash::blake2b_256(&artifact_binary).iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let lock_arg = parse_lock_arg(&lock_arg)?;
    // Construct secp256k1-sighash lock script (code_hash for mainnet/devnet).
    let lock_script = cellscript_ckb_adapter::construct_script(&cellscript_ckb_adapter::ScriptSpec::new(
        [
            0x9b, 0x81, 0x97, 0x34, 0x7e, 0x6e, 0x47, 0x1d, 0x7e, 0xa2, 0x8b, 0x52, 0x0c, 0x45, 0x3e, 0x18, 0x54, 0xf0, 0x96, 0x2e,
            0xdb, 0xce, 0x20, 0x36, 0x3e, 0x4c, 0x35, 0x7b, 0x1e, 0x5a, 0x64, 0xa6,
        ],
        ScriptHashType::Type,
        lock_arg.as_bytes().to_vec(),
    ));

    let capacity_out_point = parse_out_point(&capacity_out_point)?;
    let capacity_input = CellInput::new_builder().previous_output(capacity_out_point).build();

    Ok(DeployArtifactSpec {
        name,
        artifact_binary,
        artifact_hash,
        deployer_lock: lock_script,
        capacity_input,
        capacity_input_shannons: capacity_shannons,
        capacity_input_data: Bytes::new(),
        type_id_hash_type: ScriptHashType::Type,
        type_script: None,
        cell_deps: Vec::new(),
        header_deps: Vec::new(),
        fee_shannons: fee,
    })
}

fn cmd_deploy(rpc: &str, json: bool, args: DeployArgs) -> Result<()> {
    let spec = build_deploy_spec(args.artifact, args.lock_arg, args.name, args.fee, args.capacity_out_point, args.capacity_shannons)?;
    let name = spec.name.clone();

    let (tx, deploy_evidence) = build_deploy_transaction(&spec)?;

    // Connect and submit.
    let adapter = CellScriptAdapter::connect(rpc)?;

    // Estimate cycles.
    let estimate_cycles = adapter.estimate_cycles(&tx).ok().map(|e| e.cycles.value());

    // Test tx-pool acceptance.
    let tx_pool_accepted = adapter.test_tx_pool_accept(&tx).is_ok();

    // Submit.
    let tx_hash = adapter.submit_transaction(&tx)?;
    eprintln!("submitted tx: 0x{}", hex::encode(tx_hash.as_bytes()));

    // Wait for commitment.
    let committed = adapter.wait_for_commitment(&tx_hash, args.wait_attempts, args.wait_delay_ms)?;

    // Build manifest.
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(tx_hash.as_bytes());
    let manifest = cellscript_ckb_adapter::build_deployment_manifest_from_evidence(&deploy_evidence, &hash_bytes, 0);

    // Write manifest if requested.
    if let Some(ref path) = args.manifest_out {
        let manifest_json = serde_json::to_string_pretty(&manifest)?;
        std::fs::write(path, &manifest_json)?;
        eprintln!("manifest written to {}", path.display());
    }

    if json {
        let output = serde_json::json!({
            "tx_hash": format!("0x{}", hex::encode(tx_hash.as_bytes())),
            "committed": true,
            "block_hash": format!("0x{}", hex::encode(committed.block_hash.as_bytes())),
            "estimate_cycles": estimate_cycles,
            "tx_pool_accepted": tx_pool_accepted,
            "manifest": manifest,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("deployed {} at tx 0x{}", name, hex::encode(tx_hash.as_bytes()));
        println!("  committed in block 0x{}", hex::encode(committed.block_hash.as_bytes()));
        if let Some(cycles) = estimate_cycles {
            println!("  estimate_cycles: {cycles}");
        }
    }

    Ok(())
}

fn cmd_build_deploy(rpc: &str, json: bool, args: BuildDeployArgs) -> Result<()> {
    let spec = build_deploy_spec(args.artifact, args.lock_arg, args.name, args.fee, args.capacity_out_point, args.capacity_shannons)?;

    let (tx, _evidence) = build_deploy_transaction(&spec)?;

    // Try to estimate cycles if node is available.
    let estimate = CellScriptAdapter::connect(rpc).ok().and_then(|a| a.estimate_cycles(&tx).ok()).map(|e| e.cycles.value());

    if json {
        let tx_json = serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&tx))?;
        let output = serde_json::json!({
            "transaction": tx_json,
            "estimate_cycles": estimate,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("built deploy transaction ({} bytes)", tx.data().serialized_size_in_block());
        if let Some(cycles) = estimate {
            println!("  estimate_cycles: {cycles}");
        }
        // Print hex-encoded transaction for external tools.
        println!("0x{}", hex::encode(tx.data().as_bytes()));
    }

    Ok(())
}

fn cmd_action(rpc: &str, json: bool, args: ActionArgs) -> Result<()> {
    let plan = load_action_plan(&args.plan)?;
    let manifest = args.manifest.as_ref().map(load_deployment_manifest).transpose()?;
    let resolved = if let Some(manifest) = manifest.as_ref() {
        resolve_materialized_action_plan_with_manifest(&plan, Some(manifest))
    } else {
        resolve_materialized_action_plan(&plan)
    };

    if json {
        let output = match resolved {
            Ok(resolved) => {
                let (tx, evidence) = build_action_transaction(&resolved)?;
                serde_json::json!({
                    "action": plan.action,
                    "policy": plan.policy,
                    "artifact_hash": plan.artifact_hash,
                    "can_submit": false,
                    "resolution_status": "resolved-action-tx",
                    "manifest_cell_dep_resolution": manifest.is_some(),
                    "preview": preview_resolved_action(&resolved),
                    "evidence": evidence,
                    "transaction": serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&tx))?,
                })
            }
            Err(error) => serde_json::json!({
                "action": plan.action,
                "policy": plan.policy,
                "artifact_hash": plan.artifact_hash,
                "can_submit": plan.transaction_draft.can_submit,
                "resolution_status": "requires-runtime-resolution",
                "manifest_cell_dep_resolution": manifest.is_some(),
                "reason": error.to_string(),
            }),
        };
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("action: {}", plan.action);
        println!("  policy: {}", plan.policy);
        println!("  can_submit: {}", plan.transaction_draft.can_submit);
        println!("  manifest_cell_dep_resolution: {}", manifest.is_some());
        match resolved {
            Ok(resolved) => {
                let (_tx, evidence) = build_action_transaction(&resolved)?;
                println!("  resolution_status: resolved-action-tx");
                println!("  inputs: {}", evidence.inputs);
                println!("  outputs: {}", evidence.outputs);
                println!("  cell_deps: {}", evidence.cell_deps);
                println!("  outputs_data: {}", evidence.outputs_data);
            }
            Err(error) => {
                println!("  resolution_status: requires-runtime-resolution");
                println!("  reason: {error}");
            }
        }
    }

    // rpc is not used yet for action resolution; suppress warning.
    let _ = rpc;

    Ok(())
}

fn cmd_status(rpc: &str, json: bool, args: StatusArgs) -> Result<()> {
    let adapter = CellScriptAdapter::connect(rpc)?;

    let hash_bytes = hex::decode(args.tx_hash.trim_start_matches("0x"))?;
    if hash_bytes.len() != 32 {
        bail!("tx_hash must be 32 bytes, got {}", hash_bytes.len());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&hash_bytes);
    let tx_hash = ckb_types::H256::from(arr);

    let response = adapter.get_transaction_status(&tx_hash)?;

    if json {
        let status_str = response.as_ref().map(|r| format!("{:?}", r.tx_status.status)).unwrap_or_else(|| "unknown".to_string());
        let output = serde_json::json!({
            "tx_hash": format!("0x{}", hex::encode(&hash_bytes)),
            "status": status_str,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        match response {
            Some(r) => println!("tx 0x{} status: {:?}", hex::encode(&hash_bytes), r.tx_status.status),
            None => println!("tx 0x{} status: unknown", hex::encode(&hash_bytes)),
        }
    }

    Ok(())
}

fn cmd_info(rpc: &str, json: bool) -> Result<()> {
    let adapter = CellScriptAdapter::connect(rpc)?;
    let tip = adapter.get_tip_block_number()?;

    if json {
        let output = serde_json::json!({
            "rpc_url": rpc,
            "tip_block_number": tip,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("CKB node: {rpc}");
        println!("tip block: {tip}");
    }

    Ok(())
}

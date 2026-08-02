use anyhow::{bail, Result};
use cellscript_ckb_adapter::{
    build_action_transaction, build_deploy_transaction, load_action_plan, load_deployment_manifest, preview_resolved_action,
    resolve_materialized_action_plan, resolve_materialized_action_plan_with_manifest, CellScriptAdapter, DeployArtifactSpec,
};
use ckb_types::{
    bytes::Bytes,
    core::{DepType, ScriptHashType},
    packed::{CellDep, CellInput, OutPoint},
    prelude::*,
    H160,
};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cellscript-deploy")]
#[command(about = "CellScript CKB adapter CLI — build mainnet deployment transactions, act, and query on-chain state")]
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
    /// Refuse unsigned submission and direct callers to the external-signing flow
    #[command(hide = true)]
    Deploy(DeployArgs),

    /// Build a mainnet unsigned deploy transaction for external signing
    BuildDeploy(BuildDeployArgs),

    /// Build a transaction from an action plan
    Action(ActionArgs),

    /// Query transaction status on-chain
    Status(StatusArgs),

    /// Show tip block number and node info
    Info,
}

#[derive(clap::Args, Debug)]
struct DeploySpecArgs {
    /// Compiled RISC-V ELF artifact path
    #[arg(long)]
    artifact: PathBuf,

    /// Deployer lock script args (hex, 20 bytes for secp256k1-sighash)
    #[arg(long)]
    lock_arg: String,

    /// Name for the deployment (stored in manifest)
    #[arg(long, default_value = "cellscript-contract")]
    name: String,

    /// Fee in shannons
    #[arg(long, default_value_t = 10_000)]
    fee: u64,

    /// Capacity input out_point (format: 0x<TX_HASH>:<INDEX>)
    #[arg(long)]
    capacity_out_point: String,

    /// Code reference hash type: type creates a TYPE_ID cell; data variants create an immutable data cell
    #[arg(long, value_enum, default_value_t = DeploymentHashType::Type)]
    hash_type: DeploymentHashType,

    /// CellDep out_point for the input lock script (format: 0x<TX_HASH>:<INDEX>)
    #[arg(long, default_value = "0x71a7ba8fc96349fea0ed3a5c47992e3b4084b031a42264a018e0072e8172e46c:0")]
    lock_cell_dep_out_point: String,

    /// CellDep kind for the input lock script
    #[arg(long, value_enum, default_value_t = CliDepType::DepGroup)]
    lock_cell_dep_type: CliDepType,
}

#[derive(clap::Args, Debug)]
struct DeployArgs {
    #[command(flatten)]
    spec: DeploySpecArgs,

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
    #[command(flatten)]
    spec: DeploySpecArgs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DeploymentHashType {
    Type,
    Data,
    Data1,
    Data2,
}

impl From<DeploymentHashType> for ScriptHashType {
    fn from(value: DeploymentHashType) -> Self {
        match value {
            DeploymentHashType::Type => Self::Type,
            DeploymentHashType::Data => Self::Data,
            DeploymentHashType::Data1 => Self::Data1,
            DeploymentHashType::Data2 => Self::Data2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum CliDepType {
    Code,
    DepGroup,
}

impl From<CliDepType> for DepType {
    fn from(value: CliDepType) -> Self {
        match value {
            CliDepType::Code => Self::Code,
            CliDepType::DepGroup => Self::DepGroup,
        }
    }
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
fn build_deploy_spec(adapter: &CellScriptAdapter, args: DeploySpecArgs) -> Result<DeployArtifactSpec> {
    let artifact_binary = std::fs::read(&args.artifact)?;
    let artifact_binary = Bytes::from(artifact_binary);
    let artifact_hash = ckb_hash::blake2b_256(&artifact_binary).iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let lock_arg = parse_lock_arg(&args.lock_arg)?;
    // Construct the mainnet secp256k1-sighash lock script.
    let lock_script = cellscript_ckb_adapter::construct_script(&cellscript_ckb_adapter::ScriptSpec::new(
        [
            0x9b, 0x81, 0x97, 0x34, 0x7e, 0x6e, 0x47, 0x1d, 0x7e, 0xa2, 0x8b, 0x52, 0x0c, 0x45, 0x3e, 0x18, 0x54, 0xf0, 0x96, 0x2e,
            0xdb, 0xce, 0x20, 0x36, 0x3e, 0x4c, 0x35, 0x7b, 0x1e, 0x5a, 0x64, 0xa6,
        ],
        ScriptHashType::Type,
        lock_arg.as_bytes().to_vec(),
    ));

    let capacity_out_point = parse_out_point(&args.capacity_out_point)?;
    let (capacity_input_shannons, capacity_input_data) = adapter.resolve_pure_capacity_input(&capacity_out_point, &lock_script)?;
    let capacity_input = CellInput::new_builder().previous_output(capacity_out_point).build();
    let lock_cell_dep = CellDep::new_builder()
        .out_point(parse_out_point(&args.lock_cell_dep_out_point)?)
        .dep_type(DepType::from(args.lock_cell_dep_type))
        .build();

    Ok(DeployArtifactSpec {
        name: args.name,
        artifact_binary,
        artifact_hash,
        deployer_lock: lock_script,
        capacity_input,
        capacity_input_shannons,
        capacity_input_data,
        type_id_hash_type: args.hash_type.into(),
        type_script: None,
        cell_deps: vec![lock_cell_dep],
        header_deps: Vec::new(),
        fee_shannons: args.fee,
    })
}

fn cmd_deploy(_rpc: &str, _json: bool, args: DeployArgs) -> Result<()> {
    let _ = (args.spec, args.wait_attempts, args.wait_delay_ms, args.manifest_out);
    bail!(
        "direct deploy is disabled because this CLI does not hold or invoke a signer; use build-deploy, sign the returned transaction with a CKB wallet, then broadcast it"
    )
}

fn cmd_build_deploy(rpc: &str, json: bool, args: BuildDeployArgs) -> Result<()> {
    let adapter = CellScriptAdapter::connect(rpc)?;
    adapter.require_mainnet()?;
    let spec = build_deploy_spec(&adapter, args.spec)?;

    let (tx, evidence) = build_deploy_transaction(&spec)?;

    // An unsigned secp transaction is expected to fail script verification, so
    // cycle estimation remains informational until the external signer fills it.
    let estimate = adapter.estimate_cycles(&tx).ok().map(|e| e.cycles.value());

    if json {
        let tx_json = serde_json::to_value(cellscript_ckb_adapter::to_rpc_transaction(&tx))?;
        let output = serde_json::json!({
            "can_submit": false,
            "signing_required": true,
            "transaction": tx_json,
            "estimate_cycles": estimate,
            "evidence": evidence,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("built unsigned deploy transaction ({} bytes)", tx.data().serialized_size_in_block());
        println!("  can_submit: false");
        println!("  signing_required: true");
        println!("  hash_type: {}", evidence.hash_type);
        println!("  code_hash: 0x{}", hex::encode(&evidence.code_hash));
        println!("  cell_deps: {}", evidence.cell_deps);
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

#[cfg(test)]
mod tests {
    use super::*;

    const INPUT: &str = "0x1111111111111111111111111111111111111111111111111111111111111111:0";

    fn parse_build_deploy(extra: &[&str]) -> Cli {
        let mut args = vec![
            "cellscript-deploy",
            "build-deploy",
            "--artifact",
            "registry-type-script",
            "--lock-arg",
            "0x2222222222222222222222222222222222222222",
            "--capacity-out-point",
            INPUT,
        ];
        args.extend_from_slice(extra);
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn build_deploy_accepts_immutable_data1() {
        let cli = parse_build_deploy(&["--hash-type", "data1"]);
        let Commands::BuildDeploy(args) = cli.command else {
            panic!("expected build-deploy command");
        };
        assert_eq!(args.spec.hash_type, DeploymentHashType::Data1);
        assert_eq!(args.spec.fee, 10_000);
        assert_eq!(args.spec.lock_cell_dep_type, CliDepType::DepGroup);
        assert_eq!(args.spec.lock_cell_dep_out_point, "0x71a7ba8fc96349fea0ed3a5c47992e3b4084b031a42264a018e0072e8172e46c:0");
    }

    #[test]
    fn build_deploy_defaults_to_type_id() {
        let cli = parse_build_deploy(&[]);
        let Commands::BuildDeploy(args) = cli.command else {
            panic!("expected build-deploy command");
        };
        assert_eq!(args.spec.hash_type, DeploymentHashType::Type);
    }

    #[test]
    fn build_deploy_rejects_unknown_hash_type() {
        let error = Cli::try_parse_from([
            "cellscript-deploy",
            "build-deploy",
            "--artifact",
            "registry-type-script",
            "--lock-arg",
            "0x2222222222222222222222222222222222222222",
            "--capacity-out-point",
            INPUT,
            "--hash-type",
            "data3",
        ])
        .unwrap_err();
        assert!(error.to_string().contains("invalid value 'data3'"));
    }
}

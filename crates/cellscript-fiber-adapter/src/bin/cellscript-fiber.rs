use anyhow::Context;
use cellscript_fiber_adapter::{
    build_fiber_udt_config, check_path, is_audited_fiber_revision, materialize_fiber_config, render_fiber_config_overlay,
    resolve_asset_from_action_plan, resolve_asset_from_live_cell, verify_code_deployment, verify_local_registration,
    write_json_atomic, AcceptanceMatrixReportV1, DependencyMode, FiberCompatibilityReportV1, FiberRpcClient, HttpCkbEvidenceProvider,
    OperationalState, OutPointRef, RegistrationReportV1, TopologyReportV1, AUDITED_FIBER_REVISION, FIBER_CONFIG_SCHEMA,
};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Parser)]
#[command(name = "cellscript-fiber", version, about = "No-profile CellScript fungible Type Script support for Fiber")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Compile and structurally verify the dedicated payload-free fungible entry.
    Check(CheckArgs),
    /// Resolve live CKB identities, generate Fiber config, and optionally verify a restarted local node.
    Enable(EnableArgs),
    /// Generate the same deterministic overlay as enable without querying Fiber RPC.
    Configure(EnableArgs),
    /// Materialize a complete native Fiber config from a verified compatibility report.
    MaterializeConfig(MaterializeConfigArgs),
    /// Verify that a restarted local Fiber node reports and announces the generated asset config.
    Doctor(DoctorArgs),
    /// Validate completeness and outcomes of a generated Fiber lifecycle acceptance matrix.
    Accept(AcceptArgs),
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// CellScript file, package directory, or Cell.toml.
    source: PathBuf,
    /// Optional path for the generated compatibility descriptor.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Optional path for the exact dedicated RISC-V ELF artifact.
    #[arg(long)]
    artifact_output: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct EnableArgs {
    /// CellScript file, package directory, or Cell.toml.
    source: PathBuf,
    /// Explicit node policy. This is not contract semantics.
    #[arg(long)]
    auto_accept: u128,
    /// Ordinary CellScript CKB DeploymentManifest. Auto-discovered only from conventional build paths when omitted.
    #[arg(long)]
    deployment_manifest: Option<PathBuf>,
    /// Select one deployment when a manifest binds more than one live copy of the artifact.
    #[arg(long)]
    deployment_name: Option<String>,
    /// Ordinary fully materialized CellScript ActionPlan used to resolve the concrete asset Script.
    #[arg(long, conflicts_with = "asset_cell")]
    asset_plan: Option<PathBuf>,
    /// Verified live asset Cell outpoint, formatted as 0x<tx-hash>:<index>.
    #[arg(long, conflicts_with = "asset_plan")]
    asset_cell: Option<String>,
    /// CKB node JSON-RPC URL.
    #[arg(long, default_value = "http://127.0.0.1:8114")]
    ckb_rpc: String,
    /// CKB indexer JSON-RPC URL; defaults to ckb-rpc.
    #[arg(long)]
    ckb_indexer_rpc: Option<String>,
    /// Emit a direct code CellDep or a TYPE_ID dependency.
    #[arg(long, default_value = "direct")]
    dependency_mode: DependencyMode,
    /// Optional local Fiber RPC. When supplied, enable verifies node_info plus the signed graph announcement after restart.
    #[arg(long)]
    fiber_rpc: Option<String>,
    /// Network identity included in immutable evidence bindings.
    #[arg(long, default_value = "ckb-dev")]
    network: String,
    /// Exact 0x-prefixed CKB genesis hash used by the operator environment.
    #[arg(long, default_value = "operator-unbound")]
    ckb_revision: String,
    /// Exact Fiber source revision. The audited default has no generic hot-load RPC.
    #[arg(long, default_value = AUDITED_FIBER_REVISION)]
    fiber_revision: String,
    /// Root for generated evidence. Files are outputs, never semantic inputs.
    #[arg(long, default_value = "target/cellscript-fiber")]
    output_root: PathBuf,
}

#[derive(Debug, Args)]
struct DoctorArgs {
    /// Generated compatibility.json from enable/configure.
    report: PathBuf,
    /// Trusted loopback Fiber JSON-RPC endpoint.
    #[arg(long, default_value = "http://127.0.0.1:8227")]
    fiber_rpc: String,
}

#[derive(Debug, Args)]
struct MaterializeConfigArgs {
    /// Existing native Fiber YAML config whose non-UDT settings are preserved.
    base: PathBuf,
    /// Generated compatibility.json in LocalNodeConfiguredRestartRequired state.
    compatibility_report: PathBuf,
    /// Destination for the complete native Fiber YAML config.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Args)]
struct AcceptArgs {
    /// Generated acceptance.json with concrete evidence for every required matrix row.
    report: PathBuf,
    /// Final LocalNodeAdvertised compatibility report bound to the matrix.
    #[arg(long)]
    compatibility_report: PathBuf,
    /// Registration report proving the exact config was locally loaded and announced.
    #[arg(long)]
    registration_report: PathBuf,
    /// Certified topology report bound to the same environment.
    #[arg(long)]
    topology_report: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Check(args) => check(args),
        Command::Enable(args) => enable(args, true),
        Command::Configure(args) => enable(args, false),
        Command::MaterializeConfig(args) => materialize_config(args),
        Command::Doctor(args) => doctor(args),
        Command::Accept(args) => accept(args),
    }
}

fn materialize_config(args: MaterializeConfigArgs) -> anyhow::Result<()> {
    if args.base == args.output {
        anyhow::bail!("materialize-config requires a distinct output path; install the generated file as a separate operator action");
    }
    let report: FiberCompatibilityReportV1 = serde_json::from_slice(&fs::read(&args.compatibility_report)?)?;
    report.validate()?;
    if report.status != OperationalState::LocalNodeConfiguredRestartRequired {
        anyhow::bail!("materialize-config expects a LocalNodeConfiguredRestartRequired report, got {:?}", report.status);
    }
    let config = report.generated_config.ok_or_else(|| anyhow::anyhow!("compatibility report omitted generated Fiber config"))?;
    let rendered = materialize_fiber_config(&fs::read_to_string(&args.base)?, &[config])?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, rendered)?;
    println!("native Fiber config: {}", args.output.display());
    Ok(())
}

fn check(args: CheckArgs) -> anyhow::Result<()> {
    let checked = check_path(&args.source)?;
    if let Some(path) = args.artifact_output.as_deref() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &checked.compile_result.artifact_bytes)?;
    }
    let json = serde_json::to_string_pretty(&checked.descriptor)?;
    if let Some(output) = args.output {
        write_json_atomic(&output, &checked.descriptor)?;
        println!("StaticallyCompatible: {}", output.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn enable(args: EnableArgs, verify_fiber_when_requested: bool) -> anyhow::Result<()> {
    if args.ckb_revision == "operator-unbound" {
        anyhow::bail!("enable requires --ckb-revision with the exact 0x-prefixed CKB genesis hash");
    }
    if !is_audited_fiber_revision(&args.fiber_revision) {
        anyhow::bail!(
            "Fiber revision {} is outside the audited v1 compatibility window rooted at {}; re-audit and update the adapter before use",
            args.fiber_revision,
            AUDITED_FIBER_REVISION
        );
    }
    let checked = check_path(&args.source)?;
    let output_dir = evidence_dir(&args.output_root, &args.network, &checked.descriptor);
    fs::create_dir_all(&output_dir)?;
    fs::write(output_dir.join("fungible-type-group-v1.elf"), &checked.compile_result.artifact_bytes)?;
    write_json_atomic(output_dir.join("fungible-type-group-v1.elf.meta.json"), &checked.compile_result.metadata)?;

    let mut report =
        FiberCompatibilityReportV1::new_static(checked.descriptor.clone(), &args.network, &args.fiber_revision, &args.ckb_revision)?;
    write_json_atomic(output_dir.join("compatibility.json"), &report)?;

    let manifest_path = resolve_manifest_path(&args.source, args.deployment_manifest.as_deref())?;
    let manifest = cellscript_ckb_adapter::load_deployment_manifest(&manifest_path)
        .with_context(|| format!("load ordinary deployment manifest {}", manifest_path.display()))?;
    let provider = HttpCkbEvidenceProvider::new(&args.ckb_rpc, args.ckb_indexer_rpc.clone())?;
    let (deployment, dependency) =
        verify_code_deployment(&provider, &manifest, &checked.descriptor, args.deployment_name.as_deref(), args.dependency_mode)?;
    write_json_atomic(
        output_dir.join("deployment.json"),
        &json!({
            "manifest_path": &manifest_path,
            "identity": &deployment,
            "dependency_evidence": &dependency,
        }),
    )?;
    report.bind_deployment(deployment.clone(), dependency.clone())?;

    let asset_script = if let Some(path) = args.asset_plan.as_deref() {
        resolve_asset_from_action_plan(path, &manifest, &checked.descriptor, &deployment)?
    } else if let Some(out_point) = args.asset_cell.as_deref() {
        let out_point = OutPointRef::parse(out_point)?;
        resolve_asset_from_live_cell(&provider, &out_point, &checked.descriptor, &deployment)?
    } else {
        let path = discover_asset_plan(&args.source)?;
        resolve_asset_from_action_plan(&path, &manifest, &checked.descriptor, &deployment)?
    };
    write_json_atomic(output_dir.join("asset-script.json"), &asset_script)?;
    report.bind_asset_script(asset_script.clone())?;

    let (config, matcher) = build_fiber_udt_config(
        checked.descriptor.display_name.clone(),
        &asset_script.script,
        Some(args.auto_accept),
        vec![dependency.dependency.clone()],
    )?;
    let overlay = render_fiber_config_overlay(&config)?;
    fs::write(output_dir.join("fiber-udt-overlay.yml"), overlay)?;
    write_json_atomic(
        output_dir.join("udt-config.json"),
        &json!({"schema": FIBER_CONFIG_SCHEMA, "config": &config, "matcher_evidence": &matcher}),
    )?;
    report.bind_configuration(config.clone(), matcher)?;
    report.validate()?;
    write_json_atomic(output_dir.join("compatibility.json"), &report)?;
    write_json_atomic(output_dir.join("topology.json"), &TopologyReportV1::pending(report.binding_fingerprint.clone()))?;
    write_json_atomic(output_dir.join("acceptance.json"), &AcceptanceMatrixReportV1::pending(report.binding_fingerprint.clone()))?;

    if verify_fiber_when_requested && args.fiber_rpc.is_some() {
        if let Some(fiber_rpc) = args.fiber_rpc.as_deref() {
            let client = FiberRpcClient::trusted_local(fiber_rpc)?;
            let config_hash = report
                .binding
                .configuration_hash
                .clone()
                .ok_or_else(|| anyhow::anyhow!("configuration hash missing after config binding"))?;
            let (_, registration) = verify_local_registration(
                &client,
                &config,
                &report.binding_fingerprint,
                &config_hash,
                &report.binding.fiber_revision,
                &report.binding.ckb_revision,
            )?;
            report.mark_local_node_advertised(&config_hash)?;
            report.validate()?;
            registration.validate(&report)?;
            write_json_atomic(output_dir.join("registration.json"), &registration)?;
            write_json_atomic(output_dir.join("compatibility.json"), &report)?;
        }
    } else {
        write_json_atomic(
            output_dir.join("registration.json"),
            &json!({
                "schema": cellscript_fiber_adapter::FIBER_REGISTRATION_SCHEMA,
                "status": OperationalState::LocalNodeConfiguredRestartRequired,
                "binding_fingerprint": report.binding_fingerprint.clone(),
                "restart_required": true,
                "installed_in_running_node": false,
                "next_action": "merge fiber-udt-overlay.yml into the ordinary Fiber config, restart the node, then run cellscript-fiber doctor"
            }),
        )?;
    }

    println!("{:?}: {}", report.status, output_dir.display());
    Ok(())
}

fn doctor(args: DoctorArgs) -> anyhow::Result<()> {
    let bytes = fs::read(&args.report)?;
    let mut report: FiberCompatibilityReportV1 = serde_json::from_slice(&bytes)?;
    report.validate()?;
    if report.status != OperationalState::LocalNodeConfiguredRestartRequired {
        anyhow::bail!("doctor expects a LocalNodeConfiguredRestartRequired report, got {:?}", report.status);
    }
    let config = report.generated_config.clone().ok_or_else(|| anyhow::anyhow!("report omitted generated Fiber config"))?;
    let config_hash = report.binding.configuration_hash.clone().ok_or_else(|| anyhow::anyhow!("report omitted configuration hash"))?;
    let client = FiberRpcClient::trusted_local(args.fiber_rpc)?;
    let (_, registration) = verify_local_registration(
        &client,
        &config,
        &report.binding_fingerprint,
        &config_hash,
        &report.binding.fiber_revision,
        &report.binding.ckb_revision,
    )?;
    report.mark_local_node_advertised(&config_hash)?;
    report.validate()?;
    registration.validate(&report)?;
    write_json_atomic(&args.report, &report)?;
    let parent = args.report.parent().unwrap_or_else(|| Path::new("."));
    write_json_atomic(parent.join("registration.json"), &registration)?;
    println!("LocalNodeAdvertised: {}", args.report.display());
    Ok(())
}

fn accept(args: AcceptArgs) -> anyhow::Result<()> {
    let compatibility: FiberCompatibilityReportV1 = serde_json::from_slice(&fs::read(&args.compatibility_report)?)?;
    compatibility.validate()?;
    let registration: RegistrationReportV1 = serde_json::from_slice(&fs::read(&args.registration_report)?)?;
    registration.validate(&compatibility)?;
    let topology: TopologyReportV1 = serde_json::from_slice(&fs::read(&args.topology_report)?)?;
    topology.validate()?;
    let report: AcceptanceMatrixReportV1 = serde_json::from_slice(&fs::read(&args.report)?)?;
    report.validate()?;
    if !report.complete {
        anyhow::bail!("Fiber acceptance matrix is structurally valid but incomplete");
    }
    if !topology.certified
        || topology.status != OperationalState::TopologyCertified
        || topology.binding_fingerprint != compatibility.binding_fingerprint
        || report.binding_fingerprint != compatibility.binding_fingerprint
    {
        anyhow::bail!("acceptance, registration, topology, and compatibility reports are not jointly topology-certified and bound");
    }
    println!("TopologyCertified acceptance evidence: {}", args.report.display());
    Ok(())
}

fn evidence_dir(root: &Path, network: &str, descriptor: &cellscript_fiber_adapter::FiberAssetDescriptor) -> PathBuf {
    let suffix = descriptor.artifact_hash.trim_start_matches("0x").chars().take(12).collect::<String>();
    root.join(sanitize(network)).join(format!("{}-{suffix}", sanitize(&descriptor.selected_type)))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() || character == '-' || character == '_' { character } else { '-' })
        .collect()
}

fn resolve_manifest_path(source: &Path, explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(explicit) = explicit {
        return Ok(explicit.to_path_buf());
    }
    let base = if source.is_dir() { source } else { source.parent().unwrap_or_else(|| Path::new(".")) };
    let candidates =
        [base.join("target/cellscript/deployment.json"), base.join("build/deployment.json"), base.join("deployment.json")];
    select_one_existing("deployment manifest", &candidates)
}

fn discover_asset_plan(source: &Path) -> anyhow::Result<PathBuf> {
    let base = if source.is_dir() { source } else { source.parent().unwrap_or_else(|| Path::new(".")) };
    let stem = source.file_stem().and_then(|value| value.to_str()).unwrap_or("asset");
    let candidates =
        [base.join(format!("build/{stem}.action.json")), base.join("build/action.json"), base.join("target/cellscript/action.json")];
    select_one_existing("materialized asset ActionPlan", &candidates).map_err(|_| {
        anyhow::anyhow!(
            "no unique ordinary materialized asset ActionPlan was discovered; pass --asset-plan or --asset-cell (these are CKB evidence locators, not Fiber profiles)"
        )
    })
}

fn select_one_existing(label: &str, candidates: &[PathBuf]) -> anyhow::Result<PathBuf> {
    let existing = candidates.iter().filter(|path| path.is_file()).cloned().collect::<Vec<_>>();
    match existing.as_slice() {
        [path] => Ok(path.clone()),
        [] => anyhow::bail!("no conventional {label} found; pass its explicit path"),
        _ => anyhow::bail!("multiple conventional {label} files found; select one explicitly"),
    }
}

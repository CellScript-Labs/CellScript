use crate::docgen::{DocGenerator, OutputFormat};
use crate::error::Result;
use crate::fmt::format_default;
use crate::package::{Dependency, DetailedDependency, Lockfile, PackageManager, PolicyConfig};
use crate::runtime_errors::{runtime_error_info, runtime_error_info_by_code, CellScriptRuntimeErrorInfo, ALL_RUNTIME_ERRORS};
use crate::{
    compile_path, compile_path_with_entry_action, compile_path_with_entry_lock, default_metadata_path_for_artifact,
    default_output_path_for_input, load_modules_for_input, resolve_input_path, validate_artifact_metadata,
    validate_source_units_on_disk, ArtifactFormat, CompileMetadata, CompileOptions, EntryWitnessArg, ParamMetadata, ProofPlanMetadata,
    TargetProfile, ENTRY_WITNESS_ABI,
};
use camino::Utf8Path;
#[cfg(feature = "vm-runner")]
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use colored::Colorize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CKB_STANDARD_COMPAT_MANIFEST_SCHEMA: &str = "cellscript-ckb-standard-compat-v0.16";
const CKB_STANDARD_FIXTURE_SCHEMA: &str = "cellscript-ckb-fixture-v0.16";
const ICKB_CLAIM_MANIFEST_SCHEMA: &str = "cellscript-ickb-claim-manifest-v1";
const ICKB_DIFF_MATRIX_SCHEMA: &str = "cellscript-ickb-diff-matrix-v1";
const ICKB_DIFF_EVIDENCE_LEVEL: &str = "DIFFERENTIAL_CKB_VM_EXECUTED";
const ICKB_REQUIRED_PRODUCTION_EVIDENCE: [&str; 8] = [
    "script_group",
    "cell_deps",
    "header_deps",
    "outputs_data",
    "witnesses",
    "capacity_fee_tx_size_cycles",
    "deployment_manifest",
    "builder_plan",
];
const ICKB_REQUIRED_HARDENING_EVIDENCE: [&str; 5] =
    ["mutation_coverage", "deterministic_fuzz_seed", "normalized_fixture_generator", "max_cellscript_cycles", "max_tx_size_bytes"];
const CELLSCRIPT_CKB_RPC_URL_ENV: &str = "CELLSCRIPT_CKB_RPC_URL";
const NOVASEAL_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const NOVASEAL_CERTIFICATION_REPORT_SCHEMA: &str = "cellscript-certification-report-v0.1";
const NOVASEAL_PLUGIN_REPORT_SCHEMA: &str = "novaseal-production-gates-v0.4";
const NOVASEAL_PROFILE_CERTIFICATION_SCHEMA: &str = "novaseal-profile-certification-v0.1";
const NOVASEAL_AGREEMENT_PROFILE: &str = "agreement-profile-v0";
const NOVASEAL_CANONICAL_SCHEMA: &str = "NovaSealCanonicalV0";
const NOVASEAL_PROFILE_CERTIFICATION_GATE: &str = "agreement_profile_public_ecosystem_certification_v0";
const NOVASEAL_LOCAL_V1_DIMENSIONS: &[&str] = &[
    "architecture_and_profile_conformance",
    "planned_profiles_and_business_scenarios",
    "security_audit_coverage",
    "devnet_multi_profile_coverage",
    "multi_business_scenario_coverage",
    "full_stateful_acceptance",
    "wallet_signing_vectors",
    "profile_operator_fixtures",
    "service_builder_fixtures",
    "btc_spv_evidence_adapter",
    "external_attestation_adapter",
    "external_evidence_handoff",
    "local_bip340_tcb_review",
    "local_v1_gate",
];
const NOVASEAL_EXTERNAL_V1_DIMENSIONS: &[&str] = &[
    "external_btc_fiber_endpoint_acceptance",
    "all_profiles_production_completeness",
    "public_shared_cell_dep_attestation",
    "external_bip340_tcb_review_attestation",
    "public_btc_spv_evidence",
    "rwa_legal_registry_review_evidence",
];

#[derive(Debug)]
pub enum Command {
    Build(BuildArgs),
    Test(TestArgs),
    Doc(DocArgs),
    Fmt(FmtArgs),
    Init(InitArgs),
    New(NewArgs),
    Add(AddArgs),
    Remove(RemoveArgs),
    Clean(CleanArgs),
    Repl,
    Check(CheckArgs),
    Metadata(MetadataArgs),
    Constraints(ConstraintsArgs),
    Abi(AbiArgs),
    SchedulerPlan(SchedulerPlanArgs),
    CkbHash(CkbHashArgs),
    CkbStdCompat(CkbStdCompatArgs),
    Explain(ExplainArgs),
    ExplainProfile(ExplainProfileArgs),
    ExplainProof(ExplainProofArgs),
    ExplainAssumptions(ExplainAssumptionsArgs),
    ExplainGenerics(ExplainGenericsArgs),
    OptReport(OptReportArgs),
    ProofDiff(ProofDiffArgs),
    Profile(ProfileArgs),
    TraceTx(TraceTxArgs),
    AuditBundle(AuditBundleArgs),
    ValidateTx(ValidateTxArgs),
    SolveTx(SolveTxArgs),
    VerifyCkbFixtures(VerifyCkbFixturesArgs),
    DeployPlan(DeployPlanArgs),
    VerifyDeploy(VerifyDeployArgs),
    DiffDeploy(DiffDeployArgs),
    LockDeps(LockDepsArgs),
    ActionBuild(ActionBuildArgs),
    GenBuilder(GenBuilderArgs),
    /// Encode generated entry wrapper witness bytes
    EntryWitness(EntryWitnessArgs),
    VerifyArtifact(VerifyArtifactArgs),
    Run(RunArgs),
    Publish(PublishArgs),
    Install(InstallArgs),
    RegistryVerify(RegistryVerifyArgs),
    PackageVerify(PackageVerifyArgs),
    RegistryAdd(RegistryAddArgs),
    Certify(CertifyArgs),
    Update,
    Info(InfoArgs),
    Login(LoginArgs),
}

#[derive(Debug, Default)]
pub struct BuildArgs {
    pub release: bool,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub entry_action: Option<String>,
    pub entry_lock: Option<String>,
    pub jobs: Option<usize>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub verbose: bool,
    pub json: bool,
    pub production: bool,
    pub deny_fail_closed: bool,
    pub deny_ckb_runtime: bool,
    pub deny_runtime_obligations: bool,
    pub primitive_compat: Option<String>,
    /// Build a specific workspace member by package name.
    pub package: Option<String>,
    /// Build all workspace members.
    pub workspace: bool,
}

#[derive(Debug, Default)]
pub struct TestArgs {
    pub filter: Option<String>,
    pub jobs: Option<usize>,
    pub release: bool,
    pub no_run: bool,
    pub nocapture: bool,
    pub fail_fast: bool,
    pub doc: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct DocArgs {
    pub open: bool,
    pub no_deps: bool,
    pub document_private_items: bool,
    pub output_format: OutputFormat,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct FmtArgs {
    pub check: bool,
    pub json: bool,
    pub files: Vec<PathBuf>,
}

#[derive(Debug, Default)]
pub struct InitArgs {
    pub name: Option<String>,
    pub path: Option<PathBuf>,
    pub lib: bool,
    pub namespace: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct NewArgs {
    pub name: String,
    pub path: Option<PathBuf>,
    pub lib: bool,
    pub vcs: String,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AddArgs {
    pub crates: Vec<String>,
    pub dev: bool,
    pub build: bool,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct RemoveArgs {
    pub crates: Vec<String>,
    pub dev: bool,
    pub build: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct CleanArgs {
    pub json: bool,
    /// Also clean incremental compilation cache (.cell/build/cache)
    pub cache: bool,
}

#[derive(Debug, Default)]
pub struct InfoArgs {
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct CheckArgs {
    pub all_targets: bool,
    pub target_profile: Option<String>,
    pub features: Vec<String>,
    pub json: bool,
    pub production: bool,
    pub deny_fail_closed: bool,
    pub deny_ckb_runtime: bool,
    pub deny_runtime_obligations: bool,
    pub primitive_compat: Option<String>,
    /// Check a specific workspace member by package name.
    pub package: Option<String>,
    /// Check all workspace members.
    pub workspace: bool,
}

#[derive(Debug, Default)]
pub struct MetadataArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
}

#[derive(Debug, Default)]
pub struct ConstraintsArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub entry_action: Option<String>,
    pub entry_lock: Option<String>,
}

#[derive(Debug, Default)]
pub struct AbiArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub action: Option<String>,
    pub lock: Option<String>,
}

#[derive(Debug, Default)]
pub struct SchedulerPlanArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
}

#[derive(Debug, Default)]
pub struct CkbHashArgs {
    pub input: Option<String>,
    pub hex: Option<String>,
    pub file: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct CkbStdCompatArgs {
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainArgs {
    pub code: String,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainProfileArgs {
    pub profile: String,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainProofArgs {
    pub input: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainAssumptionsArgs {
    pub input: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainGenericsArgs {
    pub input: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct OptReportArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
}

#[derive(Debug, Default)]
pub struct ProofDiffArgs {
    pub old: PathBuf,
    pub new: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ProfileArgs {
    pub input: Option<PathBuf>,
    pub entry: Option<String>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct TraceTxArgs {
    pub against: PathBuf,
    pub tx: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuditBundleArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ValidateTxArgs {
    pub against: PathBuf,
    pub tx: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct SolveTxArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct VerifyCkbFixturesArgs {
    pub manifest: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct DeployPlanArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct VerifyDeployArgs {
    pub plan: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct DiffDeployArgs {
    pub old: PathBuf,
    pub new: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct LockDepsArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ActionBuildArgs {
    pub input: Option<PathBuf>,
    pub action: Option<String>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub fabric_intent: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct GenBuilderArgs {
    pub input: Option<PathBuf>,
    pub metadata: Option<PathBuf>,
    pub lockfile: Option<PathBuf>,
    pub deployed: Option<PathBuf>,
    pub deployment_network: Option<String>,
    pub action: Option<String>,
    pub output: Option<PathBuf>,
    pub target: String,
    pub target_profile: Option<String>,
    pub package_name: Option<String>,
    pub json: bool,
}

/// Entry witness encoding arguments
#[derive(Debug, Default)]
pub struct EntryWitnessArgs {
    pub input: Option<PathBuf>,
    pub action: Option<String>,
    pub lock: Option<String>,
    pub args: Vec<String>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct VerifyArtifactArgs {
    pub artifact: PathBuf,
    pub metadata: Option<PathBuf>,
    pub verify_sources: bool,
    pub json: bool,
    pub expect_target_profile: Option<String>,
    pub expect_artifact_hash: Option<String>,
    pub expect_source_hash: Option<String>,
    pub expect_source_content_hash: Option<String>,
    pub production: bool,
    pub deny_fail_closed: bool,
    pub deny_ckb_runtime: bool,
    pub deny_runtime_obligations: bool,
    pub primitive_compat: Option<String>,
}

#[derive(Debug, Default)]
pub struct RunArgs {
    pub args: Vec<String>,
    pub release: bool,
    pub simulate: bool,
}

#[derive(Debug, Default)]
pub struct PublishArgs {
    pub dry_run: bool,
    pub allow_dirty: bool,
}

#[derive(Debug, Default)]
pub struct InstallArgs {
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub namespace: Option<String>,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct LoginArgs {
    pub registry: Option<String>,
}

#[derive(Debug, Default)]
pub struct RegistryVerifyArgs {
    pub json: bool,
    pub live: bool,
    pub rpc_url: Option<String>,
    pub network: Option<String>,
    pub require_publisher_signature: bool,
    pub require_audit_report: bool,
}

#[derive(Debug, Default)]
pub struct PackageVerifyArgs {
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct RegistryAddArgs {
    pub namespace: String,
    pub name: String,
    pub source: String,
}

#[derive(Debug, Default)]
pub struct CertifyArgs {
    pub plugin: String,
    pub repo_root: Option<PathBuf>,
    pub report: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub json: bool,
    pub require_production: bool,
}

pub struct CommandExecutor;

impl CommandExecutor {
    fn experimental_command(name: &str, detail: &str) -> Result<()> {
        Err(crate::error::CompileError::without_span(format!("cellc {} is still experimental: {}", name, detail)))
    }

    pub fn execute(cmd: Command) -> Result<()> {
        match cmd {
            Command::Build(args) => Self::build(args),
            Command::Test(args) => Self::test(args),
            Command::Doc(args) => Self::doc(args),
            Command::Fmt(args) => Self::fmt(args),
            Command::Init(args) => Self::init(args),
            Command::New(args) => Self::create_new(args),
            Command::Add(args) => Self::add(args),
            Command::Remove(args) => Self::remove(args),
            Command::Clean(args) => Self::clean(args),
            Command::Repl => Self::repl(),
            Command::Check(args) => Self::check(args),
            Command::Metadata(args) => Self::metadata(args),
            Command::Constraints(args) => Self::constraints(args),
            Command::Abi(args) => Self::abi(args),
            Command::SchedulerPlan(args) => Self::scheduler_plan(args),
            Command::CkbHash(args) => Self::ckb_hash(args),
            Command::CkbStdCompat(args) => Self::ckb_std_compat(args),
            Command::Explain(args) => Self::explain(args),
            Command::ExplainProfile(args) => Self::explain_profile(args),
            Command::ExplainProof(args) => Self::explain_proof(args),
            Command::ExplainAssumptions(args) => Self::explain_assumptions(args),
            Command::ExplainGenerics(args) => Self::explain_generics(args),
            Command::OptReport(args) => Self::opt_report(args),
            Command::ProofDiff(args) => Self::proof_diff(args),
            Command::Profile(args) => Self::profile(args),
            Command::TraceTx(args) => Self::trace_tx(args),
            Command::AuditBundle(args) => Self::audit_bundle(args),
            Command::ValidateTx(args) => Self::validate_tx(args),
            Command::SolveTx(args) => Self::solve_tx(args),
            Command::VerifyCkbFixtures(args) => Self::verify_ckb_fixtures(args),
            Command::DeployPlan(args) => Self::deploy_plan(args),
            Command::VerifyDeploy(args) => Self::verify_deploy(args),
            Command::DiffDeploy(args) => Self::diff_deploy(args),
            Command::LockDeps(args) => Self::lock_deps(args),
            Command::ActionBuild(args) => Self::action_build(args),
            Command::GenBuilder(args) => Self::gen_builder(args),
            Command::EntryWitness(args) => Self::entry_witness(args),
            Command::VerifyArtifact(args) => Self::verify_artifact(args),
            Command::Run(args) => Self::run(args),
            Command::Publish(args) => Self::publish(args),
            Command::Install(args) => Self::install(args),
            Command::Update => Self::update(),
            Command::Info(args) => Self::info(args),
            Command::Login(args) => Self::login(args),
            Command::RegistryVerify(args) => Self::registry_verify(args),
            Command::PackageVerify(args) => Self::package_verify(args),
            Command::RegistryAdd(args) => Self::registry_add(args),
            Command::Certify(args) => Self::certify(args),
        }
    }

    fn build(args: BuildArgs) -> Result<()> {
        // Workspace mode: build all members or a specific member.
        if args.workspace || args.package.is_some() {
            return Self::build_workspace(args);
        }
        // Also check if the current directory is a workspace root without explicit flags.
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?;
        if let Some(ws_root) = ws_root {
            let members = crate::resolve_workspace_members(&ws_root)?;
            if !members.is_empty() {
                // Current dir is a workspace root; build all members.
                let mut ws_args = args;
                ws_args.workspace = true;
                return Self::build_workspace(ws_args);
            }
        }

        let opt_level = if args.release { 3 } else { 1 };
        let input = Utf8Path::new(".");
        let options = CompileOptions {
            opt_level,
            output: None,
            debug: false,
            target: args.target.clone(),
            target_profile: args.target_profile.clone(),
            primitive_compat: args.primitive_compat.clone(),
        };
        if args.entry_action.is_some() && args.entry_lock.is_some() {
            return Err(crate::error::CompileError::without_span("--entry-action and --entry-lock are mutually exclusive"));
        }
        let result = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
            (Some(action), None) => compile_path_with_entry_action(input, options, action),
            (None, Some(lock)) => compile_path_with_entry_lock(input, options, lock),
            (None, None) => compile_path(input, options),
            (Some(_), Some(_)) => unreachable!("validated above"),
        }?;
        let policy_args = effective_build_check_args(&args)?;
        validate_check_policy(&result.metadata, &policy_args)?;
        let resolved = resolve_input_path(input)?;
        let output_path = default_output_path_for_input(input, &resolved, result.artifact_format)?;
        result.write_to_path(&output_path)?;
        let metadata_path = default_metadata_path_for_artifact(&output_path);
        result.write_metadata_to_path(&metadata_path)?;

        refresh_lockfile_from_build(std::path::Path::new("."), &result.metadata)?;

        let policy_verified = policy_args.production
            || policy_args.deny_fail_closed
            || policy_args.deny_ckb_runtime
            || policy_args.deny_runtime_obligations;
        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "artifact": output_path.to_string(),
                "metadata": metadata_path.to_string(),
                "artifact_format": result.artifact_format.display_name(),
                "opt_level": opt_level,
                "target_profile": result.metadata.target_profile.name.as_str(),
                "artifact_hash": result.metadata.artifact_hash,
                "artifact_size_bytes": result.artifact_bytes.len(),
                "source_hash": result.metadata.source_hash,
                "source_content_hash": result.metadata.source_content_hash,
                "metadata_schema_version": result.metadata.metadata_schema_version,
                "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
                "compiler_version": result.metadata.compiler_version,
                "standalone_runner_compatible": result.metadata.runtime.standalone_runner_compatible,
                "ckb_runtime_required": result.metadata.runtime.ckb_runtime_required,
                "verifier_obligations": result.metadata.runtime.verifier_obligations.len(),
                "runtime_required_verifier_obligations": runtime_required_obligation_count(&result.metadata),
                "fail_closed_verifier_obligations": fail_closed_obligation_count(&result.metadata),
                "runtime_required_transaction_invariants": runtime_required_transaction_invariant_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subconditions": runtime_required_transaction_invariant_checked_subcondition_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subcondition_summaries": transaction_invariant_checked_subcondition_summaries(&result.metadata),
                "transaction_runtime_input_requirements": transaction_runtime_input_requirement_count(&result.metadata),
                "transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries(&result.metadata),
                "checked_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "checked-runtime"),
                "checked_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "checked-runtime"),
                "runtime_required_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blockers": transaction_runtime_input_blocker_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_summaries": transaction_runtime_input_blocker_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_classes": transaction_runtime_input_blocker_class_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_class_summaries": transaction_runtime_input_blocker_class_summaries_by_status(&result.metadata, "runtime-required"),
                "checked_pool_invariant_families": checked_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_families": runtime_required_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_blocker_classes": pool_invariant_family_blocker_class_count(&result.metadata, "runtime-required"),
                "runtime_required_pool_invariant_blocker_class_summaries": pool_invariant_family_blocker_class_summaries(&result.metadata, "runtime-required"),
                "pool_runtime_input_requirements": pool_runtime_input_requirement_count(&result.metadata),
                "pool_runtime_input_requirement_summaries": pool_runtime_input_requirement_summaries(&result.metadata),
                "policy_verified": policy_verified,
                "cache_hit": result.cache_hit,
                "constraints": &result.metadata.constraints,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize build summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Build complete".green());
        println!("  Artifact format: {}", result.artifact_format.display_name());
        println!("  Target profile: {}", result.metadata.target_profile.name);
        println!("  Output: {}", output_path);
        println!("  Metadata: {}", metadata_path);
        if result.cache_hit {
            println!("  {}", "(incremental cache hit)".yellow());
        }
        Ok(())
    }

    fn build_workspace(args: BuildArgs) -> Result<()> {
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?.ok_or_else(|| {
            crate::error::CompileError::without_span(
                "no workspace root found; run from a directory containing a [workspace] Cell.toml",
            )
        })?;
        let all_members = crate::resolve_workspace_members(&ws_root)?;
        let members: Vec<_> = if let Some(ref pkg_name) = args.package {
            // Find the specific member by reading its manifest for the package name.
            let mut found = Vec::new();
            for member_dir in &all_members {
                let pm = crate::package::PackageManager::new(member_dir.as_std_path());
                let manifest = pm.read_manifest()?;
                if manifest.package.name == *pkg_name {
                    found.push(member_dir.clone());
                }
            }
            if found.is_empty() {
                return Err(crate::error::CompileError::without_span(format!(
                    "workspace member '{}' not found; available members: {}",
                    pkg_name,
                    all_members.iter().map(|m| m.as_str().to_string()).collect::<Vec<_>>().join(", ")
                )));
            }
            found
        } else {
            all_members
        };

        let opt_level = if args.release { 3 } else { 1 };
        let mut member_results = Vec::new();
        let mut failed = 0;

        for member_dir in &members {
            let options = CompileOptions {
                opt_level,
                output: None,
                debug: false,
                target: args.target.clone(),
                target_profile: args.target_profile.clone(),
                primitive_compat: args.primitive_compat.clone(),
            };

            let compile_result = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
                (Some(action), None) => compile_path_with_entry_action(member_dir, options, action),
                (None, Some(lock)) => compile_path_with_entry_lock(member_dir, options, lock),
                _ => compile_path(member_dir, options),
            };

            match compile_result {
                Ok(result) => {
                    let policy_args = effective_build_check_args(&args)?;
                    if let Err(e) = validate_check_policy(&result.metadata, &policy_args) {
                        if args.json {
                            member_results.push(serde_json::json!({
                                "member": member_dir.as_str(),
                                "status": "policy_failed",
                                "error": e.message,
                            }));
                        } else {
                            eprintln!("{}: policy check failed: {}", member_dir, e.message);
                        }
                        failed += 1;
                        continue;
                    }

                    let resolved = resolve_input_path(member_dir)?;
                    let output_path = default_output_path_for_input(member_dir, &resolved, result.artifact_format)?;
                    result.write_to_path(&output_path)?;
                    let metadata_path = default_metadata_path_for_artifact(&output_path);
                    result.write_metadata_to_path(&metadata_path)?;

                    member_results.push(serde_json::json!({
                        "member": member_dir.as_str(),
                        "status": "ok",
                        "artifact": output_path.to_string(),
                        "metadata": metadata_path.to_string(),
                        "artifact_format": result.artifact_format.display_name(),
                        "target_profile": result.metadata.target_profile.name,
                        "artifact_hash": result.metadata.artifact_hash,
                        "artifact_size_bytes": result.artifact_bytes.len(),
                        "cache_hit": result.cache_hit,
                    }));

                    if !args.json {
                        println!("{} {}", "Built".green(), member_dir);
                    }
                }
                Err(e) => {
                    if args.json {
                        member_results.push(serde_json::json!({
                            "member": member_dir.as_str(),
                            "status": "failed",
                            "error": e.message,
                        }));
                    } else {
                        eprintln!("{}: {}", member_dir, e.message);
                    }
                    failed += 1;
                }
            }
        }

        if args.json {
            let summary = serde_json::json!({
                "status": if failed == 0 { "ok" } else { "failed" },
                "mode": "workspace",
                "members": members.len(),
                "succeeded": members.len() - failed,
                "failed": failed,
                "results": member_results,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize workspace build summary: {}", error))
            })?;
            println!("{}", json);
            if failed > 0 {
                return Err(crate::error::CompileError::without_span(format!(
                    "{} of {} workspace members failed to build",
                    failed,
                    members.len()
                )));
            }
            return Ok(());
        }

        if failed > 0 {
            return Err(crate::error::CompileError::without_span(format!(
                "{} of {} workspace members failed to build",
                failed,
                members.len()
            )));
        }

        // Write workspace-level Cell.lock at the workspace root.
        let mut lockfile = Lockfile::read_from_root(ws_root.as_std_path())?.unwrap_or_else(Lockfile::new);
        // Merge build hashes from each successfully built member.
        for res in &member_results {
            if res["status"].as_str() == Some("ok") {
                let member_name = res["member"].as_str().unwrap_or("unknown");
                let artifact_hash = res.get("artifact_hash").and_then(|v| v.as_str()).unwrap_or("");
                if !artifact_hash.is_empty() {
                    lockfile.dependencies.insert(
                        member_name.to_string(),
                        crate::package::LockedDependency {
                            version: String::new(),
                            source: crate::package::LockedSource::Path { path: member_name.to_string() },
                            source_hash: Some(artifact_hash.to_string()),
                            build: None,
                        },
                    );
                }
            }
        }
        lockfile.write_to_root(ws_root.as_std_path())?;

        println!("{}", format!("Workspace build complete: {} members", members.len()).green());
        Ok(())
    }

    fn test(args: TestArgs) -> Result<()> {
        let doc_output = if args.doc {
            Some(Self::generate_docs(&DocArgs { output_format: OutputFormat::Markdown, ..Default::default() })?)
        } else {
            None
        };
        if args.doc && !args.json {
            println!("{}", "Documentation generated".green());
            if let Some(output) = &doc_output {
                println!("  Output: {}", output.display());
            }
        }

        let mut test_inputs = collect_cell_files(Path::new("tests"))?;
        if let Some(filter) = &args.filter {
            test_inputs.retain(|path| path.to_string_lossy().contains(filter));
        }
        test_inputs.sort();

        if test_inputs.is_empty() {
            compile_path(
                ".",
                CompileOptions {
                    opt_level: 0,
                    output: None,
                    debug: false,
                    target: None,
                    target_profile: None,
                    primitive_compat: None,
                },
            )?;
            if args.json {
                let summary = serde_json::json!({
                    "status": "ok",
                    "package_check": "passed",
                    "test_files": 0,
                    "passed": 0,
                    "failed": 0,
                    "fail_fast": args.fail_fast,
                    "no_run": args.no_run,
                    "execution": if args.no_run { "disabled" } else { "skipped-no-test-files" },
                    "docs_generated": args.doc,
                    "doc_output": doc_output.as_ref().map(|path| path.display().to_string()),
                    "tests": [],
                });
                let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize test summary: {}", error))
                })?;
                println!("{}", json);
                return Ok(());
            }
            println!("{}", "Test compile complete".green());
            println!("  Package check: passed");
            println!("  Test files: 0");
            if !args.no_run {
                println!("  Execution: skipped; no CellScript test files were found");
            }
            return Ok(());
        }

        let mut passed = 0usize;
        let mut failures = Vec::new();
        let mut test_reports = Vec::new();
        for input in &test_inputs {
            let utf8 = Utf8Path::from_path(input)
                .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input.display())))?;
            if args.nocapture && !args.json {
                println!("  Testing {}", utf8);
            }

            let expectation = read_test_expectation(input)?;
            let result = compile_path(
                utf8,
                CompileOptions {
                    opt_level: 0,
                    output: None,
                    debug: false,
                    target: expectation.target.clone(),
                    target_profile: None,
                    primitive_compat: None,
                },
            )
            .and_then(|result| {
                let policy_args = expectation.check_args();
                validate_check_policy(&result.metadata, &policy_args)?;
                Ok(result)
            });
            match evaluate_compile_test_result(utf8, &expectation, result) {
                Ok(()) => {
                    passed += 1;
                    test_reports.push(serde_json::json!({
                        "path": utf8.to_string(),
                        "status": "passed",
                        "target": expectation.target,
                    }));
                }
                Err(error) => {
                    let message = error.message;
                    test_reports.push(serde_json::json!({
                        "path": utf8.to_string(),
                        "status": "failed",
                        "error": message,
                        "target": expectation.target,
                    }));
                    failures.push(message);
                    if args.fail_fast {
                        break;
                    }
                }
            }
        }

        if !failures.is_empty() {
            return Err(crate::error::CompileError::without_span(format!("test failed:\n  - {}", failures.join("\n  - "))));
        }

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "package_check": "not-run",
                "test_files": test_inputs.len(),
                "passed": passed,
                "failed": 0,
                "fail_fast": args.fail_fast,
                "no_run": args.no_run,
                "execution": if args.no_run { "disabled" } else { "skipped-default-toolchain" },
                "docs_generated": args.doc,
                "doc_output": doc_output.as_ref().map(|path| path.display().to_string()),
                "tests": test_reports,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize test summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Test compile complete".green());
        println!("  Compiled {} test file(s)", passed);
        if !args.no_run {
            println!("  Execution: skipped; CellScript test execution is not enabled in the default toolchain yet");
        }
        Ok(())
    }

    fn doc(args: DocArgs) -> Result<()> {
        let output = Self::generate_docs(&args)?;
        let output_size_bytes = std::fs::metadata(&output).map(|metadata| metadata.len()).unwrap_or(0);

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "format": display_doc_output_format(&args.output_format),
                "output": output.display().to_string(),
                "output_size_bytes": output_size_bytes,
                "opened": args.open,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize doc summary: {}", error)))?;
            println!("{}", json);

            if args.open {
                let _ = std::process::Command::new("open").arg(&output).status();
            }

            return Ok(());
        }

        println!("{}", "Documentation generated".green());
        println!("  Output: {}", output.display());

        if args.open {
            let _ = std::process::Command::new("open").arg(&output).status();
        }

        Ok(())
    }

    fn generate_docs(args: &DocArgs) -> Result<PathBuf> {
        let modules = load_modules_for_input(".")?;
        let compile_result = compile_path(
            ".",
            CompileOptions { opt_level: 0, output: None, debug: false, target: None, target_profile: None, primitive_compat: None },
        )?;
        let mut generator = DocGenerator::new(args.output_format);
        for module in &modules {
            generator.add_module(&module.ast);
        }
        generator.set_compile_metadata(&compile_result.metadata);
        let docs = generator.generate()?;
        let output = match args.output_format {
            OutputFormat::Html => PathBuf::from("docs/cellscript-api.html"),
            OutputFormat::Markdown => PathBuf::from("docs/cellscript-api.md"),
            OutputFormat::Json => PathBuf::from("docs/cellscript-api.json"),
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&output, docs)?;

        Ok(output)
    }

    fn fmt(args: FmtArgs) -> Result<()> {
        let modules = if args.files.is_empty() {
            load_modules_for_input(".")?
        } else {
            let mut modules = Vec::new();
            for path in &args.files {
                let utf8 = Utf8Path::from_path(path).ok_or_else(|| {
                    crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", path.display()))
                })?;
                modules.extend(load_modules_for_input(utf8)?);
            }
            modules
        };

        let mut changed = Vec::new();
        for module in modules {
            let formatted = format_default(&module.ast)?;
            if formatted != module.source {
                changed.push(module.path.clone());
                if !args.check {
                    std::fs::write(&module.path, formatted)?;
                }
            }
        }
        let changed_files = changed.iter().map(|path| path.as_str()).collect::<Vec<_>>();

        if args.check {
            if changed.is_empty() {
                if args.json {
                    let summary = serde_json::json!({
                        "status": "ok",
                        "mode": "check",
                        "changed": 0,
                        "changed_files": changed_files,
                    });
                    let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                        crate::error::CompileError::without_span(format!("failed to serialize fmt summary: {}", error))
                    })?;
                    println!("{}", json);
                    return Ok(());
                }
                println!("{}", "Formatting is clean".green());
                Ok(())
            } else {
                if args.json {
                    let summary = serde_json::json!({
                        "status": "failed",
                        "mode": "check",
                        "changed": changed.len(),
                        "changed_files": changed_files,
                    });
                    let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                        crate::error::CompileError::without_span(format!("failed to serialize fmt summary: {}", error))
                    })?;
                    println!("{}", json);
                }
                Err(crate::error::CompileError::without_span(format!(
                    "format check failed for {} file(s): {}",
                    changed.len(),
                    changed_files.join(", ")
                )))
            }
        } else {
            if args.json {
                let summary = serde_json::json!({
                    "status": "ok",
                    "mode": "write",
                    "changed": changed.len(),
                    "changed_files": changed_files,
                });
                let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize fmt summary: {}", error))
                })?;
                println!("{}", json);
                return Ok(());
            }
            println!("{}", "Formatting complete".green());
            println!("  Updated {} file(s)", changed.len());
            Ok(())
        }
    }

    fn init(args: InitArgs) -> Result<()> {
        let path = args.path.unwrap_or_else(|| PathBuf::from("."));
        let name = args.name.unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());

        if !args.json {
            println!("{} {} in {}", "Creating".cyan(), if args.lib { "library" } else { "binary" }, path.display());
        }

        let pm = PackageManager::new(&path);
        if args.lib {
            pm.init_library(&name)?;
        } else {
            pm.init(&name)?;
        }
        if let Some(namespace) = &args.namespace {
            let mut manifest = pm.read_manifest()?;
            manifest.package.namespace = Some(namespace.clone());
            pm.write_manifest(&manifest)?;
        }

        if args.json {
            let entry = if args.lib { "src/lib.cell" } else { "src/main.cell" };
            let summary = serde_json::json!({
                "status": "ok",
                "kind": if args.lib { "library" } else { "binary" },
                "package": name,
                "path": path.display().to_string(),
                "manifest": path.join("Cell.toml").display().to_string(),
                "entry": entry,
                "namespace": args.namespace,
                "created_files": [
                    path.join("Cell.toml").display().to_string(),
                    path.join(entry).display().to_string(),
                    path.join(".gitignore").display().to_string(),
                ],
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize init summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Created package successfully".green());
        println!("  To get started:");
        println!("    cd {}", path.display());
        println!("    cellc build");

        Ok(())
    }

    fn create_new(args: NewArgs) -> Result<()> {
        let path = args.path.unwrap_or_else(|| PathBuf::from(&args.name));
        ensure_new_package_destination(&path)?;

        if !args.json {
            println!("{} {} in {}", "Creating".cyan(), if args.lib { "library" } else { "binary" }, path.display());
        }

        let pm = PackageManager::new(&path);
        if args.lib {
            pm.init_library(&args.name)?;
        } else {
            pm.init(&args.name)?;
        }

        let git_initialized = match args.vcs.as_str() {
            "git" => init_git_repo(&path)?,
            "none" => false,
            other => {
                return Err(crate::error::CompileError::without_span(format!("unsupported VCS '{}'; expected 'git' or 'none'", other)))
            }
        };

        if args.json {
            let entry = if args.lib { "src/lib.cell" } else { "src/main.cell" };
            let summary = serde_json::json!({
                "status": "ok",
                "command": "new",
                "kind": if args.lib { "library" } else { "binary" },
                "package": args.name,
                "path": path.display().to_string(),
                "manifest": path.join("Cell.toml").display().to_string(),
                "entry": entry,
                "vcs": args.vcs,
                "git_initialized": git_initialized,
                "created_files": [
                    path.join("Cell.toml").display().to_string(),
                    path.join(entry).display().to_string(),
                    path.join(".gitignore").display().to_string(),
                ],
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize new summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Created package successfully".green());
        println!("  To get started:");
        println!("    cd {}", path.display());
        println!("    cellc build");
        Ok(())
    }

    fn add(args: AddArgs) -> Result<()> {
        validate_dependency_target_flags(args.dev, args.build)?;
        if args.git.is_some() && args.path.is_some() {
            return Err(crate::error::CompileError::without_span("cellc add accepts either --git or --path, not both"));
        }

        let pm = PackageManager::new(".");
        let mut manifest = pm.read_manifest()?;
        let dependency = dependency_from_add_args(&args);
        let target = dependency_target_label(args.dev, args.build);
        let mut added = Vec::new();

        for crate_name in &args.crates {
            if !args.json {
                println!("{} {} to {}", "Adding".cyan(), crate_name, target);
            }
            validate_not_self_dependency(crate_name, &dependency, &manifest)?;
            dependency_map_mut(&mut manifest, args.dev, args.build).insert(crate_name.clone(), dependency.clone());
            added.push(crate_name.clone());
        }

        pm.write_manifest(&manifest)?;

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "target": target,
                "added": added,
                "dependency": dependency,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize add summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Dependencies added successfully".green());
        Ok(())
    }

    fn remove(args: RemoveArgs) -> Result<()> {
        validate_dependency_target_flags(args.dev, args.build)?;
        let pm = PackageManager::new(".");
        let mut manifest = pm.read_manifest()?;
        let target = dependency_target_label(args.dev, args.build);
        let mut removed = Vec::new();
        let mut missing = Vec::new();

        for crate_name in &args.crates {
            if !args.json {
                println!("{} {} from {}", "Removing".cyan(), crate_name, target);
            }
            if dependency_map_mut(&mut manifest, args.dev, args.build).remove(crate_name).is_some() {
                removed.push(crate_name.clone());
            } else {
                missing.push(crate_name.clone());
            }
        }

        pm.write_manifest(&manifest)?;
        if !args.dev && !args.build && !removed.is_empty() {
            refresh_lockfile_from_manifest(Path::new("."))?;
        }

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "target": target,
                "removed": removed,
                "missing": missing,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize remove summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Dependencies removed successfully".green());
        Ok(())
    }

    fn clean(args: CleanArgs) -> Result<()> {
        if !args.json {
            println!("{}", "Cleaning...".cyan());
        }

        let mut paths = vec!["target", ".cell/cache"];
        if args.cache {
            paths.push(".cell/build/cache");
        }
        let mut removed_paths = Vec::new();

        for path in paths {
            if std::path::Path::new(path).exists() {
                if !args.json {
                    println!("  Removing {}", path);
                }
                std::fs::remove_dir_all(path)?;
                removed_paths.push(path.to_string());
            }
        }

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "removed": removed_paths.len(),
                "removed_paths": removed_paths,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize clean summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Clean complete".green());
        Ok(())
    }

    fn repl() -> Result<()> {
        crate::repl::run_repl().map_err(|e| crate::error::CompileError::without_span(e.to_string()))
    }

    fn check(args: CheckArgs) -> Result<()> {
        // Workspace mode: check all members or a specific member.
        if args.workspace || args.package.is_some() {
            return Self::check_workspace(args);
        }
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?;
        if let Some(ws_root) = ws_root {
            let members = crate::resolve_workspace_members(&ws_root)?;
            if !members.is_empty() {
                let mut ws_args = args;
                ws_args.workspace = true;
                return Self::check_workspace(ws_args);
            }
        }

        let args = effective_check_args(args)?;
        let requested_profile = effective_check_target_profile(&args)?;
        let compile_target_profile = compile_target_profile_for_check(requested_profile);
        let mut checked_targets = Vec::new();
        let mut checked_target_json = Vec::new();
        let targets: Vec<Option<&'static str>> =
            if args.all_targets { vec![Some("riscv64-asm"), Some("riscv64-elf")] } else { vec![None] };

        for target in targets {
            let result = compile_path(
                ".",
                CompileOptions {
                    opt_level: 0,
                    output: None,
                    debug: false,
                    target: target.map(str::to_string),
                    target_profile: compile_target_profile.clone(),
                    primitive_compat: args.primitive_compat.clone(),
                },
            )?;
            validate_check_policy(&result.metadata, &args)?;
            let target_profile_policy_violations =
                target_profile_policy_violations(&result.metadata, result.artifact_format, requested_profile);
            if !target_profile_policy_violations.is_empty() {
                return Err(crate::error::CompileError::without_span(format!(
                    "target profile policy failed for '{}':\n  - {}",
                    requested_profile.name(),
                    target_profile_policy_violations.join("\n  - ")
                )));
            }
            let target_label = match target {
                Some(target) => format!("{} ({})", target, result.artifact_format.display_name()),
                None => format!("package default ({})", result.artifact_format.display_name()),
            };
            let requested_profile_name = requested_profile.name();
            checked_target_json.push(serde_json::json!({
                "requested_target": target.unwrap_or("package-default"),
                "artifact_format": result.artifact_format.display_name(),
                "target_profile": requested_profile_name,
                "compiled_target_profile": result.metadata.target_profile.name.as_str(),
                "target_profile_policy_violations": target_profile_policy_violations,
                "metadata_schema_version": result.metadata.metadata_schema_version,
                "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
                "compiler_version": result.metadata.compiler_version,
                "standalone_runner_compatible": result.metadata.runtime.standalone_runner_compatible,
                "ckb_runtime_required": result.metadata.runtime.ckb_runtime_required,
                "fail_closed_runtime_features": result.metadata.runtime.fail_closed_runtime_features,
                "verifier_obligations": result.metadata.runtime.verifier_obligations.len(),
                "runtime_required_verifier_obligations": runtime_required_obligation_count(&result.metadata),
                "fail_closed_verifier_obligations": fail_closed_obligation_count(&result.metadata),
                "runtime_required_transaction_invariants": runtime_required_transaction_invariant_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subconditions": runtime_required_transaction_invariant_checked_subcondition_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subcondition_summaries": transaction_invariant_checked_subcondition_summaries(&result.metadata),
                "transaction_runtime_input_requirements": transaction_runtime_input_requirement_count(&result.metadata),
                "transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries(&result.metadata),
                "checked_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "checked-runtime"),
                "checked_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "checked-runtime"),
                "runtime_required_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blockers": transaction_runtime_input_blocker_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_summaries": transaction_runtime_input_blocker_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_classes": transaction_runtime_input_blocker_class_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_class_summaries": transaction_runtime_input_blocker_class_summaries_by_status(&result.metadata, "runtime-required"),
                "checked_pool_invariant_families": checked_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_families": runtime_required_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_blocker_classes": pool_invariant_family_blocker_class_count(&result.metadata, "runtime-required"),
                "runtime_required_pool_invariant_blocker_class_summaries": pool_invariant_family_blocker_class_summaries(&result.metadata, "runtime-required"),
                "pool_runtime_input_requirements": pool_runtime_input_requirement_count(&result.metadata),
                "pool_runtime_input_requirement_summaries": pool_runtime_input_requirement_summaries(&result.metadata),
                "constraints": &result.metadata.constraints,
            }));
            checked_targets.push(target_label);
        }

        let policy_verified = args.production || args.deny_fail_closed || args.deny_ckb_runtime;
        let policy_verified = policy_verified || args.deny_runtime_obligations;
        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "checked_targets": checked_target_json,
                "all_targets": args.all_targets,
                "policy_verified": policy_verified,
                "policy": {
                    "production": args.production,
                    "deny_fail_closed": args.deny_fail_closed,
                    "deny_ckb_runtime": args.deny_ckb_runtime,
                    "deny_runtime_obligations": args.deny_runtime_obligations,
                },
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize check summary: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Check succeeded".green());
        println!("  Target profile: {}", requested_profile.name());
        for target in checked_targets {
            println!("  Checked: {}", target);
        }
        Ok(())
    }

    fn check_workspace(args: CheckArgs) -> Result<()> {
        let ws_root = crate::find_workspace_root(Utf8Path::new("."))?.ok_or_else(|| {
            crate::error::CompileError::without_span(
                "no workspace root found; run from a directory containing a [workspace] Cell.toml",
            )
        })?;
        let all_members = crate::resolve_workspace_members(&ws_root)?;
        let members: Vec<_> = if let Some(ref pkg_name) = args.package {
            let mut found = Vec::new();
            for member_dir in &all_members {
                let pm = crate::package::PackageManager::new(member_dir.as_std_path());
                let manifest = pm.read_manifest()?;
                if manifest.package.name == *pkg_name {
                    found.push(member_dir.clone());
                }
            }
            if found.is_empty() {
                return Err(crate::error::CompileError::without_span(format!("workspace member '{}' not found", pkg_name)));
            }
            found
        } else {
            all_members
        };

        let mut member_results = Vec::new();
        let mut failed = 0;

        for member_dir in &members {
            let compile_result = compile_path(
                member_dir,
                CompileOptions {
                    opt_level: 0,
                    output: None,
                    debug: false,
                    target: None,
                    target_profile: args.target_profile.clone(),
                    primitive_compat: args.primitive_compat.clone(),
                },
            );

            match compile_result {
                Ok(result) => {
                    member_results.push(serde_json::json!({
                        "member": member_dir.as_str(),
                        "status": "ok",
                        "artifact_format": result.artifact_format.display_name(),
                        "target_profile": result.metadata.target_profile.name,
                    }));
                    if !args.json {
                        println!("{} {}", "Checked".green(), member_dir);
                    }
                }
                Err(e) => {
                    member_results.push(serde_json::json!({
                        "member": member_dir.as_str(),
                        "status": "failed",
                        "error": e.message,
                    }));
                    if !args.json {
                        eprintln!("{}: {}", member_dir, e.message);
                    }
                    failed += 1;
                }
            }
        }

        if args.json {
            let summary = serde_json::json!({
                "status": if failed == 0 { "ok" } else { "failed" },
                "mode": "workspace",
                "members": members.len(),
                "succeeded": members.len() - failed,
                "failed": failed,
                "results": member_results,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize workspace check summary: {}", error))
            })?;
            println!("{}", json);
            if failed > 0 {
                return Err(crate::error::CompileError::without_span(format!(
                    "{} of {} workspace members failed",
                    failed,
                    members.len()
                )));
            }
            return Ok(());
        }

        if failed > 0 {
            return Err(crate::error::CompileError::without_span(format!("{} of {} workspace members failed", failed, members.len())));
        }
        println!("{}", format!("Workspace check complete: {} members", members.len()).green());
        Ok(())
    }

    fn metadata(args: MetadataArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let json = serde_json::to_string_pretty(&result.metadata)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize metadata: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Metadata generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    fn constraints(args: ConstraintsArgs) -> Result<()> {
        if args.entry_action.is_some() && args.entry_lock.is_some() {
            return Err(crate::error::CompileError::without_span(
                "constraints accepts either --entry-action or --entry-lock, not both",
            ));
        }
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let options = CompileOptions {
            opt_level: 0,
            output: None,
            debug: false,
            target: args.target,
            target_profile: args.target_profile,
            primitive_compat: None,
        };
        let result = match (args.entry_action.as_deref(), args.entry_lock.as_deref()) {
            (Some(action), None) => compile_path_with_entry_action(input, options, action),
            (None, Some(lock)) => compile_path_with_entry_lock(input, options, lock),
            (None, None) => compile_path(input, options),
            (Some(_), Some(_)) => unreachable!("validated above"),
        }?;
        let json = serde_json::to_string_pretty(&result.metadata.constraints)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize constraints: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Constraints generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    fn abi(args: AbiArgs) -> Result<()> {
        if args.action.is_some() && args.lock.is_some() {
            return Err(crate::error::CompileError::without_span("abi accepts either --action or --lock, not both"));
        }

        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let selected = select_entry_witness_metadata(&result.metadata, args.action.as_deref(), args.lock.as_deref())?;
        let entry_constraints = result
            .metadata
            .constraints
            .entry_abi
            .iter()
            .find(|entry| entry.entry_kind == selected.kind && entry.entry_name == selected.name)
            .ok_or_else(|| {
                crate::error::CompileError::without_span(format!(
                    "entry ABI constraints for {} '{}' were not found in metadata",
                    selected.kind, selected.name
                ))
            })?;

        let params = selected
            .params
            .iter()
            .map(|param| {
                let runtime_bound = selected.runtime_bound_param_names.contains(&param.name) || param.lock_args_data_source;
                let payload_bound =
                    !param.lock_args_data_source && !param.cell_bound_abi && !param.ty.starts_with('&') && !runtime_bound;
                let layout = entry_constraints.params.iter().find(|candidate| candidate.name == param.name);
                serde_json::json!({
                    "name": param.name,
                    "type": param.ty,
                    "payload_bound": payload_bound,
                    "runtime_bound": runtime_bound,
                    "cell_bound": param.cell_bound_abi,
                    "schema_pointer_abi": param.schema_pointer_abi,
                    "fixed_byte_len": param.fixed_byte_len,
                    "abi_kind": layout.map(|layout| layout.abi_kind.as_str()),
                    "abi_slots": layout.map(|layout| layout.abi_slots),
                    "slot_start": layout.map(|layout| layout.slot_start),
                    "slot_end": layout.map(|layout| layout.slot_end),
                    "witness_bytes": layout.map(|layout| layout.witness_bytes),
                    "stack_spill_bytes": layout.map(|layout| layout.stack_spill_bytes),
                    "supported": layout.map(|layout| layout.supported).unwrap_or(false),
                    "unsupported_reason": layout.and_then(|layout| layout.unsupported_reason.as_deref()),
                })
            })
            .collect::<Vec<_>>();
        let payload_params = selected
            .params
            .iter()
            .filter(|param| {
                !param.lock_args_data_source
                    && !param.cell_bound_abi
                    && !param.ty.starts_with('&')
                    && !selected.runtime_bound_param_names.contains(&param.name)
            })
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        let runtime_bound_params = selected
            .runtime_bound_param_names
            .iter()
            .map(|name| name.as_str())
            .chain(selected.params.iter().filter(|param| param.lock_args_data_source).map(|param| param.name.as_str()))
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "status": if entry_constraints.unsupported { "fail" } else { "ok" },
            "abi": ENTRY_WITNESS_ABI,
            "target_profile": result.metadata.target_profile.name,
            "entry_kind": selected.kind,
            "entry": selected.name,
            "payload_params": payload_params,
            "runtime_bound_params": runtime_bound_params,
            "layout": entry_constraints,
            "params": params,
        });
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize ABI report: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "ABI report generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    fn scheduler_plan(args: SchedulerPlanArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;

        let actions = result
            .metadata
            .actions
            .iter()
            .map(|action| {
                let mut reasons = Vec::new();
                if !action.parallelizable {
                    reasons.push("parallelizable=false".to_string());
                }
                if !action.touches_shared.is_empty() {
                    reasons.push("touches-shared-state".to_string());
                }
                serde_json::json!({
                    "action": action.name,
                    "effect_class": action.effect_class,
                    "parallelizable": action.parallelizable,
                    "touches_shared": action.touches_shared,
                    "estimated_cycles": action.estimated_cycles,
                    "scheduler_witness_abi": action.scheduler_witness_abi,
                    "admission": if action.parallelizable && action.touches_shared.is_empty() {
                        "parallel-candidate"
                    } else {
                        "serial-required"
                    },
                    "reasons": reasons,
                })
            })
            .collect::<Vec<_>>();

        let mut conflicts = Vec::new();
        for (left_index, left) in result.metadata.actions.iter().enumerate() {
            for right in result.metadata.actions.iter().skip(left_index + 1) {
                let shared =
                    left.touches_shared.iter().filter(|touch| right.touches_shared.contains(*touch)).cloned().collect::<Vec<_>>();
                if !shared.is_empty() {
                    conflicts.push(serde_json::json!({
                        "left": left.name,
                        "right": right.name,
                        "shared_touches": shared,
                        "policy": "must-not-run-in-parallel",
                    }));
                }
            }
        }

        let total_estimated_cycles = result.metadata.actions.iter().map(|action| action.estimated_cycles).sum::<u64>();
        let max_estimated_cycles = result.metadata.actions.iter().map(|action| action.estimated_cycles).max().unwrap_or_default();
        let serial_required_actions = result
            .metadata
            .actions
            .iter()
            .filter(|action| !action.parallelizable || !action.touches_shared.is_empty())
            .map(|action| action.name.as_str())
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "status": "ok",
            "target_profile": result.metadata.target_profile.name,
            "policy": "cellscript-scheduler-hints-v1",
            "action_count": result.metadata.actions.len(),
            "serial_required_actions": serial_required_actions,
            "conflict_count": conflicts.len(),
            "conflicts": conflicts,
            "estimated_cycles": {
                "total": total_estimated_cycles,
                "max_action": max_estimated_cycles,
            },
            "actions": actions,
        });
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize scheduler plan: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Scheduler plan generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    fn ckb_hash(args: CkbHashArgs) -> Result<()> {
        let source_count = usize::from(args.input.is_some()) + usize::from(args.hex.is_some()) + usize::from(args.file.is_some());
        if source_count > 1 {
            return Err(crate::error::CompileError::without_span(
                "ckb-hash accepts at most one input source: positional UTF-8 text, --hex, or --file",
            ));
        }
        let bytes = if let Some(hex) = args.hex.as_deref() {
            decode_hex_arg("ckb-hash", hex, None)?
        } else if let Some(path) = args.file.as_ref() {
            std::fs::read(path).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to read CKB hash input '{}': {}", path.display(), error))
            })?
        } else {
            args.input.unwrap_or_default().into_bytes()
        };
        let hash = crate::ckb_blake2b256(&bytes);
        let hash_hex = crate::hex_encode(&hash);
        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "algorithm": "blake2b-256",
                "personalization": std::str::from_utf8(crate::CKB_DEFAULT_HASH_PERSONALIZATION).unwrap_or("ckb-default-hash"),
                "input_bytes": bytes.len(),
                "hash": hash_hex,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize CKB hash: {}", error)))?;
            println!("{}", json);
        } else {
            println!("{}", hash_hex);
        }
        Ok(())
    }

    fn ckb_std_compat(args: CkbStdCompatArgs) -> Result<()> {
        let report = serde_json::json!({
            "status": "ok",
            "schema": "cellscript-ckb-std-compat-report-v0.19",
            "runtime_policy": "inline",
            "compiler_core_dependency": "no-ckb-std",
            "compatibility_dependency_scope": "dev-test-and-adapter-contract",
            "abi_source": "src/ckb_abi.rs",
            "test_evidence": {
                "compat_tests": "tests/ckb_std_compat.rs",
                "constant_parity": true,
                "source_view_decoding": true,
                "witness_args_layout": true,
                "type_id_contract": true,
                "since_epoch_contract": true,
                "occupied_capacity_field": true,
                "packed_transaction_materialization": true,
                "script_construction_api": true,
            },
            "ckb_std_refs": {
                "constants": "ckb_std::ckb_constants",
                "witness_args": "ckb_types::packed::WitnessArgs",
                "type_id": "ckb_std::type_id",
                "since": "ckb_std::since",
                "occupied_capacity": "ckb_std::high_level::load_cell_occupied_capacity",
            },
            "inline_abi": {
                "syscalls": {
                    "load_cell_by_field": crate::ckb_abi::syscall::LOAD_CELL_BY_FIELD,
                    "load_witness": crate::ckb_abi::syscall::LOAD_WITNESS,
                    "load_input_by_field": crate::ckb_abi::syscall::LOAD_INPUT_BY_FIELD,
                    "spawn": crate::ckb_abi::syscall::SPAWN,
                },
                "sources": {
                    "input": crate::ckb_abi::source::INPUT,
                    "output": crate::ckb_abi::source::OUTPUT,
                    "group_input": crate::ckb_abi::source::GROUP_INPUT,
                    "group_output": crate::ckb_abi::source::GROUP_OUTPUT,
                },
                "fields": {
                    "cell_occupied_capacity": crate::ckb_abi::cell_field::OCCUPIED_CAPACITY,
                    "input_since": crate::ckb_abi::input_field::SINCE,
                },
            },
            "adapter_boundary": {
                "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
                "compiler_core_uses_ckb_sdk_rust": false,
                "action_build_contract": "cellscript-ckb-adapter-contract-v0.19",
                "requires_node_acceptance_for_production": true,
                "script_construction": {
                    "owner": "adapter",
                    "packed_type": "ckb_types::packed::Script",
                    "evidence_schema": "cellscript-ckb-script-evidence-v0.19",
                    "supports": [
                        "arbitrary_code_hash",
                        "hash_type",
                        "args",
                        "script_hash",
                        "script_ref_readback",
                        "explicit_cell_dep_binding",
                        "args_exact_prefix_suffix",
                        "owner_mode_args"
                    ],
                },
            },
            "witness_args_policy": {
                "entry_payload_abi": ENTRY_WITNESS_ABI,
                "entry_payload_owner": "compiler",
                "final_witness_args_owner": "adapter",
                "default_action_payload_field": "input_type",
                "lock_signature_policy": "explicit-adapter-owned-do-not-overwrite",
                "placement_requires_deployment_role": true,
                "ckb_reference": "ckb_types::packed::WitnessArgs",
            },
            "non_goals": [
                "does-not-execute-ckb-vm",
                "does-not-query-live-cells",
                "does-not-resolve-celldeps",
                "does-not-sign-or-submit"
            ],
        });

        if args.json {
            print_json(&report)?;
        } else {
            println!("CKB std compatibility: {}", report["status"].as_str().unwrap_or("unknown"));
            println!("  Schema: {}", report["schema"].as_str().unwrap_or("unknown"));
            println!("  Runtime policy: {}", report["runtime_policy"].as_str().unwrap_or("unknown"));
            println!("  ABI source: {}", report["abi_source"].as_str().unwrap_or("unknown"));
            println!("  Test evidence: {}", report["test_evidence"]["compat_tests"].as_str().unwrap_or("unknown"));
        }
        Ok(())
    }

    fn explain(args: ExplainArgs) -> Result<()> {
        let info = runtime_error_info_from_query(&args.code).ok_or_else(|| {
            crate::error::CompileError::without_span(format!(
                "unknown CellScript runtime error '{}'; use a numeric code, E-code, or runtime error name",
                args.code
            ))
        })?;

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "code": info.code,
                "ecode": format!("E{:04}", info.code),
                "name": info.name,
                "description": info.description,
                "hint": info.hint,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize error explanation: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        println!("CellScript runtime error E{:04} ({}): {}", info.code, info.code, info.name);
        println!("  Description: {}", info.description);
        println!("  Hint: {}", info.hint);
        Ok(())
    }

    fn explain_proof(args: ExplainProofArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let proof_plan = result.metadata.runtime.proof_plan;

        if args.json {
            let proof_plan_summary = proof_plan_summary_json(&proof_plan);
            let summary = serde_json::json!({
                "status": "ok",
                "module": result.metadata.module,
                "target_profile": result.metadata.target_profile.name,
                "proof_plan_summary": proof_plan_summary,
                "proof_plan": proof_plan,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize ProofPlan: {}", error)))?;
            println!("{}", json);
            return Ok(());
        }

        println!("Covenant ProofPlan for module `{}`", result.metadata.module);
        print_proof_plan_summary(&proof_plan);
        if proof_plan.is_empty() {
            println!("  No ProofPlan records emitted.");
            return Ok(());
        }
        for plan in &proof_plan {
            print_proof_plan_record(plan);
        }
        Ok(())
    }

    fn explain_assumptions(args: ExplainAssumptionsArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let assumptions = result.metadata.runtime.builder_assumptions.clone();
        let summary = serde_json::json!({
            "status": "ok",
            "module": result.metadata.module,
            "target_profile": result.metadata.target_profile.name,
            "assumption_count": assumptions.len(),
            "proof_plan_soundness": result.metadata.runtime.proof_plan_soundness,
            "builder_assumptions": assumptions,
        });
        if args.json {
            print_json(&summary)?;
        } else {
            println!("Builder assumptions for module `{}`", result.metadata.module);
            println!("  Assumptions: {}", summary["assumption_count"]);
            println!("  ProofPlan soundness: {}", summary["proof_plan_soundness"]["status"].as_str().unwrap_or("unknown"));
            for assumption in result.metadata.runtime.builder_assumptions {
                println!("  - {} [{}] {}", assumption.assumption_id, assumption.kind, assumption.feature);
            }
        }
        Ok(())
    }

    fn validate_tx(args: ValidateTxArgs) -> Result<()> {
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        let report = crate::assumptions::validate_transaction_against_metadata(&metadata, &tx);
        let summary = serde_json::json!({
            "status": report.status,
            "validation_level": "cellscript-metadata-evidence",
            "ckb_vm_execution": false,
            "tx_pool_acceptance": false,
            "metadata": args.against.display().to_string(),
            "tx": args.tx.display().to_string(),
            "validation": report,
        });
        if args.json {
            print_json(&summary)?;
        } else {
            println!("Transaction validation: {}", summary["status"].as_str().unwrap_or("unknown"));
        }
        if summary["status"] == "failed" {
            return Err(crate::error::CompileError::without_span("transaction violates builder assumptions"));
        }
        Ok(())
    }

    fn solve_tx(args: SolveTxArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let template = transaction_solver_template(&result.metadata);
        write_or_print_json(args.output.as_ref(), &template, args.json, "Transaction template generated (solve-tx is not a solver)")?;
        Ok(())
    }

    fn verify_ckb_fixtures(args: VerifyCkbFixturesArgs) -> Result<()> {
        let manifest_bytes = std::fs::read(&args.manifest).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to read fixture manifest '{}': {}",
                args.manifest.display(),
                error
            ))
        })?;
        let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to parse fixture manifest '{}': {}",
                args.manifest.display(),
                error
            ))
        })?;
        let base_dir = args.manifest.parent().unwrap_or_else(|| Path::new("."));
        let report = ckb_fixture_manifest_report(&manifest, base_dir, &manifest_bytes);
        let issue_count = report["issue_count"].as_u64().unwrap_or(0);
        if args.json {
            print_json(&report)?;
        } else {
            println!("CKB fixture manifest verification: {}", report["status"].as_str().unwrap_or("unknown"));
            println!("  Manifest schema: {}", report["manifest_schema"].as_str().unwrap_or("unknown"));
            println!("  Execution level: {}", report["execution_level"].as_str().unwrap_or("unknown"));
            println!("  Suites: {}", report["suite_count"].as_u64().unwrap_or(0));
            println!("  Fixtures: {}", report["fixture_count"].as_u64().unwrap_or(0));
            println!("  Issues: {issue_count}");
            if let Some(issues) = report["issues"].as_array() {
                for issue in issues {
                    println!("  - {}", issue.as_str().unwrap_or("<invalid issue>"));
                }
            }
        }
        if issue_count == 0 {
            Ok(())
        } else {
            Err(crate::error::CompileError::without_span(format!("CKB fixture manifest failed verification: {issue_count} issue(s)")))
        }
    }

    fn deploy_plan(args: DeployPlanArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let plan = deployment_plan_json(&result.metadata);
        write_or_print_json(args.output.as_ref(), &plan, args.json, "Deployment plan generated")?;
        Ok(())
    }

    fn verify_deploy(args: VerifyDeployArgs) -> Result<()> {
        let plan = read_json_value(&args.plan)?;
        let violations = verify_deploy_plan_json(&plan);
        let summary = serde_json::json!({
            "status": if violations.is_empty() { "ok" } else { "failed" },
            "plan": args.plan.display().to_string(),
            "violations": violations,
        });
        if args.json {
            print_json(&summary)?;
        } else {
            println!("Deploy plan verification: {}", summary["status"].as_str().unwrap_or("unknown"));
        }
        if summary["status"] == "failed" {
            return Err(crate::error::CompileError::without_span("deploy plan verification failed"));
        }
        Ok(())
    }

    fn diff_deploy(args: DiffDeployArgs) -> Result<()> {
        let old = read_json_value(&args.old)?;
        let new = read_json_value(&args.new)?;
        let diff = json_diff_report("deploy", &old, &new);
        print_or_text_json(args.json, &diff, "Deploy diff")?;
        Ok(())
    }

    fn lock_deps(args: LockDepsArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let lock = dependency_lock_json(&result.metadata);
        write_or_print_json(args.output.as_ref(), &lock, args.json, "Dependency lock generated")?;
        Ok(())
    }

    fn proof_diff(args: ProofDiffArgs) -> Result<()> {
        let old = read_metadata_json(&args.old)?;
        let new = read_metadata_json(&args.new)?;
        let diff = proof_diff_report(&old, &new);
        print_or_text_json(args.json, &diff, "Proof diff")?;
        Ok(())
    }

    fn profile(args: ProfileArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let report = profile_report_json(&result.metadata, args.entry.as_deref());
        print_or_text_json(args.json, &report, "Profile")?;
        Ok(())
    }

    fn trace_tx(args: TraceTxArgs) -> Result<()> {
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        let validation = crate::assumptions::validate_transaction_against_metadata(&metadata, &tx);
        let trace = trace_tx_report_json(&metadata, &validation);
        if args.json {
            print_json(&trace)?;
        } else {
            println!("Transaction trace: {}", trace["status"].as_str().unwrap_or("unknown"));
        }
        if validation.status == "failed" {
            return Err(crate::error::CompileError::without_span("transaction trace found builder assumption violations"));
        }
        Ok(())
    }

    fn audit_bundle(args: AuditBundleArgs) -> Result<()> {
        let result = compile_cli_input(
            args.input.as_ref(),
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let output = args.output.unwrap_or_else(|| PathBuf::from("target/cellscript-audit-bundle"));
        std::fs::create_dir_all(&output)?;
        let bundle = audit_bundle_json(&result.metadata);
        let json_path = output.join("audit-bundle.json");
        let html_path = output.join("index.html");
        std::fs::write(
            &json_path,
            serde_json::to_string_pretty(&bundle)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize audit bundle: {}", error)))?,
        )?;
        std::fs::write(&html_path, audit_bundle_html(&bundle))?;
        let summary = serde_json::json!({
            "status": "ok",
            "output": output.display().to_string(),
            "json": json_path.display().to_string(),
            "html": html_path.display().to_string(),
        });
        if args.json {
            print_json(&summary)?;
        } else {
            println!("Audit bundle generated");
            println!("  JSON: {}", json_path.display());
            println!("  HTML: {}", html_path.display());
        }
        Ok(())
    }

    fn explain_generics(args: ExplainGenericsArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;
        let instantiations = result.metadata.runtime.collection_instantiations;

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "count": instantiations.len(),
                "collection_instantiations": instantiations,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize generic explanation: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        if instantiations.is_empty() {
            println!("No checked bounded generic collection instantiations found.");
            return Ok(());
        }

        println!("Checked bounded generic collection instantiations:");
        for instantiation in instantiations {
            println!(
                "  {} {}: {} -> {} ({} byte element, max {}, {})",
                instantiation.scope_kind,
                instantiation.scope_name,
                instantiation.collection_ty,
                instantiation.element_ty,
                instantiation.element_width_bytes,
                instantiation.max_elements,
                instantiation.status
            );
            println!("    backing: {}", instantiation.backing);
            println!("    helpers: {}", instantiation.helpers.join(", "));
        }
        Ok(())
    }

    fn opt_report(args: OptReportArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let mut rows = Vec::new();
        for opt_level in 0..=3u8 {
            let result = compile_path(
                input,
                CompileOptions {
                    opt_level,
                    output: None,
                    debug: false,
                    target: args.target.clone(),
                    target_profile: args.target_profile.clone(),
                    primitive_compat: None,
                },
            )?;
            let artifact = &result.metadata.constraints.artifact;
            let estimated_cycles_total = result.metadata.actions.iter().map(|action| action.estimated_cycles).sum::<u64>();
            let estimated_cycles_max_action =
                result.metadata.actions.iter().map(|action| action.estimated_cycles).max().unwrap_or_default();
            rows.push(serde_json::json!({
                "opt_level": opt_level,
                "artifact_format": result.metadata.artifact_format,
                "target_profile": result.metadata.target_profile.name,
                "artifact_size_bytes": result.artifact_bytes.len(),
                "estimated_cycles_total": estimated_cycles_total,
                "estimated_cycles_max_action": estimated_cycles_max_action,
                "backend_shape": {
                    "text_bytes": artifact.text_bytes,
                    "rodata_bytes": artifact.rodata_bytes,
                    "executable_text_op_count": artifact.executable_text_op_count,
                    "covered_text_op_count": artifact.covered_text_op_count,
                    "relaxed_branch_count": artifact.relaxed_branch_count,
                    "max_cond_branch_abs_distance": artifact.max_cond_branch_abs_distance,
                    "machine_block_count": artifact.machine_block_count,
                    "max_machine_block_size": artifact.max_machine_block_size,
                    "conditional_branch_block_count": artifact.conditional_branch_block_count,
                    "labeled_machine_block_count": artifact.labeled_machine_block_count,
                    "machine_cfg_edge_count": artifact.machine_cfg_edge_count,
                    "machine_call_edge_count": artifact.machine_call_edge_count,
                    "unreachable_machine_block_count": artifact.unreachable_machine_block_count,
                    "layout_order_block_count": artifact.layout_order_block_count,
                    "layout_order_text_size": artifact.layout_order_text_size,
                },
                "constraints_status": result.metadata.constraints.status,
                "constraints_warnings": result.metadata.constraints.warnings.len(),
                "constraints_failures": result.metadata.constraints.failures.len(),
                "source_content_hash": result.metadata.source_content_hash,
            }));
        }
        let baseline_size = rows.first().and_then(|row| row["artifact_size_bytes"].as_u64()).unwrap_or_default();
        let baseline_text_bytes = rows.first().and_then(|row| row["backend_shape"]["text_bytes"].as_u64());
        let baseline_executable_text_ops = rows.first().and_then(|row| row["backend_shape"]["executable_text_op_count"].as_u64());
        let baseline_estimated_cycles_total = rows.first().and_then(|row| row["estimated_cycles_total"].as_u64()).unwrap_or_default();
        let summary_rows = rows
            .into_iter()
            .map(|mut row| {
                let size = row["artifact_size_bytes"].as_u64().unwrap_or_default();
                row["artifact_size_delta_from_o0"] = serde_json::json!(size as i64 - baseline_size as i64);
                row["text_bytes_delta_from_o0"] = match (row["backend_shape"]["text_bytes"].as_u64(), baseline_text_bytes) {
                    (Some(value), Some(baseline)) => serde_json::json!(value as i64 - baseline as i64),
                    _ => serde_json::Value::Null,
                };
                row["executable_text_op_count_delta_from_o0"] =
                    match (row["backend_shape"]["executable_text_op_count"].as_u64(), baseline_executable_text_ops) {
                        (Some(value), Some(baseline)) => serde_json::json!(value as i64 - baseline as i64),
                        _ => serde_json::Value::Null,
                    };
                let estimated_cycles_total = row["estimated_cycles_total"].as_u64().unwrap_or_default();
                row["estimated_cycles_total_delta_from_o0"] =
                    serde_json::json!(estimated_cycles_total as i64 - baseline_estimated_cycles_total as i64);
                row
            })
            .collect::<Vec<_>>();
        let summary = serde_json::json!({
            "status": "ok",
            "policy": "cellscript-opt-report-v1",
            "input": input_path.display().to_string(),
            "baseline_opt_level": 0,
            "rows": summary_rows,
        });
        let json = serde_json::to_string_pretty(&summary)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize opt report: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Optimization report generated".green());
            println!("  Output: {}", output_path.display());
        } else {
            println!("{}", json);
        }
        Ok(())
    }

    fn explain_profile(args: ExplainProfileArgs) -> Result<()> {
        let profile = TargetProfile::from_name(&args.profile)?;
        let metadata = profile.metadata(ArtifactFormat::RiscvElf);
        let summary = serde_json::json!({
            "profile": metadata.name,
            "target_chain": metadata.target_chain,
            "vm_abi": metadata.vm_abi,
            "hash_domain": metadata.hash_domain,
            "syscall_set": metadata.syscall_set,
            "artifact_packaging": metadata.artifact_packaging,
            "header_abi": metadata.header_abi,
            "scheduler_abi": metadata.scheduler_abi,
            "witness_abi": metadata.witness_abi,
            "lock_args_abi": metadata.lock_args_abi,
            "source_encoding": metadata.source_encoding,
            "spawn_ipc_abi": metadata.spawn_ipc_abi,
            "since_abi": metadata.since_abi,
            "cell_dep_abi": metadata.cell_dep_abi,
            "script_ref_abi": metadata.script_ref_abi,
            "output_data_abi": metadata.output_data_abi,
            "capacity_floor_abi": metadata.capacity_floor_abi,
            "type_id_abi": metadata.type_id_abi,
            "tx_version": metadata.tx_version,
            "boundaries": [
                "WitnessArgs fields are explicit CKB witness surfaces, not implicit signer authority",
                "lock_args parameters are typed script args, not implicit signer authority",
                "Source group views are scoped to the active script group",
                "outputs and outputs_data are index-aligned CKB transaction surfaces",
                "capacity floors are declared in shannons and still require builder measurement",
                "script references keep code_hash, hash_type, and args visible",
                "TYPE_ID metadata uses the CKB TYPE_ID ABI and does not hide builder obligations",
                "Spawn/IPC is bounded verifier reuse and does not make type scripts multi-tenant",
                "hash_blake2b(input: Hash) uses CKB Blake2b-256 for one Hash",
                "hash_pair(left: Hash, right: Hash) uses CKB Blake2b-256 over two Hash values; wider byte serialization hashing remains out of scope"
            ],
        });
        if args.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize profile explanation: {}", error))
                })?
            );
        } else {
            println!("Target profile: {}", summary["profile"].as_str().unwrap_or("unknown"));
            println!("  Target chain: {}", summary["target_chain"].as_str().unwrap_or("unknown"));
            println!("  VM ABI: {}", summary["vm_abi"].as_str().unwrap_or("unknown"));
            println!("  Witness ABI: {}", summary["witness_abi"].as_str().unwrap_or("unknown"));
            println!("  Lock args ABI: {}", summary["lock_args_abi"].as_str().unwrap_or("unknown"));
            println!("  Source encoding: {}", summary["source_encoding"].as_str().unwrap_or("unknown"));
            println!("  Spawn/IPC ABI: {}", summary["spawn_ipc_abi"].as_str().unwrap_or("unknown"));
            println!("  Since ABI: {}", summary["since_abi"].as_str().unwrap_or("unknown"));
            println!("  CellDep ABI: {}", summary["cell_dep_abi"].as_str().unwrap_or("unknown"));
            println!("  Script ref ABI: {}", summary["script_ref_abi"].as_str().unwrap_or("unknown"));
            println!("  Output data ABI: {}", summary["output_data_abi"].as_str().unwrap_or("unknown"));
            println!("  Capacity floor ABI: {}", summary["capacity_floor_abi"].as_str().unwrap_or("unknown"));
            println!("  TYPE_ID ABI: {}", summary["type_id_abi"].as_str().unwrap_or("unknown"));
        }
        Ok(())
    }

    fn action_build(args: ActionBuildArgs) -> Result<()> {
        let input_path = args.input.unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 1,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile.or_else(|| Some("ckb".to_string())),
                primitive_compat: None,
            },
        )?;

        let action = if let Some(name) = args.action.as_deref() {
            result
                .metadata
                .actions
                .iter()
                .find(|action| action.name == name)
                .ok_or_else(|| crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", name)))?
        } else {
            result
                .metadata
                .actions
                .first()
                .ok_or_else(|| crate::error::CompileError::without_span("no actions found in compiled metadata"))?
        };
        let entry_constraints =
            result.metadata.constraints.entry_abi.iter().find(|entry| entry.entry_kind == "action" && entry.entry_name == action.name);

        let ckb = result.metadata.constraints.ckb.as_ref();
        let metadata_bytes = serde_json::to_vec(&result.metadata).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize metadata for digest: {}", error))
        })?;
        let metadata_hash = crate::hex_encode(&crate::ckb_blake2b256(&metadata_bytes));
        let ckb_contract = ckb.map(|ckb| {
            serde_json::json!({
                "hash_type_policy": ckb.hash_type_policy,
                "capacity_evidence_contract": ckb.capacity_evidence_contract,
                "timelock_policy": ckb.timelock_policy,
                "tx_size_measurement_required": ckb.tx_size_measurement_required,
                "occupied_capacity_measurement_required": ckb.occupied_capacity_measurement_required,
                "dry_run_required_for_production": ckb.dry_run_required_for_production,
            })
        });
        let transaction_draft = serde_json::json!({
            "format": "cellscript-ccc-transaction-draft-v1",
            "state": "ActionPlan",
            "status": "template",
            "ccc_compatible": true,
            "can_submit": false,
            "ckb_vm_execution": false,
            "tx_pool_acceptance": false,
            "requires_live_cell_resolution": true,
            "requires_packed_materialization": true,
            "packed_materialization": {
                "transaction": "ckb_types::packed::Transaction",
                "cell_output": "ckb_types::packed::CellOutput",
                "cell_dep": "ckb_types::packed::CellDep",
                "out_point": "ckb_types::packed::OutPoint",
                "script": "ckb_types::packed::Script",
                "witness_args": "ckb_types::packed::WitnessArgs",
                "realizer": "cellscript-ckb-adapter via ckb-sdk-rust or CCC",
            },
            "cell_deps": [],
            "header_deps": [],
            "inputs": [],
            "outputs": [],
            "outputs_data": [],
            "witnesses": [],
            "required_evidence": [
                "live_cell_resolution",
                "outputs_data_pairing",
                "witness_args_placement",
                "celldep_resolution",
                "occupied_capacity",
                "fee_and_change",
                "estimate_cycles",
                "tx_pool_acceptance"
            ],
            "notes": [
                "This is a headless draft template produced from compiler metadata.",
                "A builder adapter must resolve live cells, fill args, calculate fees/capacity, dry-run, sign, and submit."
            ]
        });
        let resolved_tx_required_fields = serde_json::json!([
            "schema",
            "state",
            "metadata_hash",
            "artifact_hash",
            "deployment_ref",
            "action_selector",
            "inputs",
            "outputs",
            "outputs_data",
            "witnesses",
            "cell_deps",
            "header_deps",
            "capacity_evidence",
            "fee_policy",
            "change_policy",
            "lineage"
        ]);
        let acceptance_report_template = serde_json::json!({
            "schema": "cellscript-ckb-action-acceptance-report-v0.19",
            "state": "AcceptedActionTx",
            "metadata_hash": metadata_hash,
            "artifact_hash": result.metadata.artifact_hash,
            "deployment_ref": serde_json::Value::Null,
            "action_selector": action.name,
            "ckb_vm_execution": serde_json::Value::Null,
            "estimate_cycles": serde_json::Value::Null,
            "tx_pool_acceptance": serde_json::Value::Null,
            "submitted_tx_hash": serde_json::Value::Null,
            "serialized_tx_size_bytes": serde_json::Value::Null,
            "occupied_capacity_shannons": serde_json::Value::Null,
            "fee_shannons": serde_json::Value::Null,
            "lineage": [],
            "known_limitations": [
                "Template only: adapter must fill live cells, deployment refs, packed transaction bytes, signer policy, and node evidence."
            ],
        });
        let adapter_contract = serde_json::json!({
            "schema": "cellscript-ckb-adapter-contract-v0.19",
            "headless": true,
            "compiler_core_dependency": "no-ckb-sdk-rust",
            "compiler_output_state": "ActionPlan",
            "adapter_output_state": "ResolvedActionTx",
            "accepted_output_state": "AcceptedActionTx",
            "transaction_realizer": "ckb-sdk-rust-or-CCC-adapter",
            "must_not_infer_protocol_semantics_from_action_name": true,
            "must_keep_signer_authority_explicit": true,
            "must_preserve_outputs_outputs_data_pairing": true,
            "must_emit_lineage": true,
            "witness_policy": {
                "entry_payload_abi": ENTRY_WITNESS_ABI,
                "entry_payload_owner": "compiler",
                "final_witness_args_owner": "adapter",
                "default_action_payload_field": "input_type",
                "lock_signature_policy": "explicit-adapter-owned-do-not-overwrite",
                "placement_requires_deployment_role": true,
            },
            "acceptance_methods": ["estimate_cycles", "test_tx_pool_accept", "send_transaction_optional"],
            "not_proven_by_this_plan": ["live_cell_availability", "ckb_vm_execution", "tx_pool_acceptance", "submission"],
            "resolved_tx_required_fields": resolved_tx_required_fields,
            "acceptance_report_template": acceptance_report_template,
        });
        let preview = serde_json::json!({
            "format": "cellscript-action-preview-v1",
            "action": action.name,
            "summary": format!("Build a CKB transaction for CellScript action {}", action.name),
            "consumes": action.transaction_runtime_input_requirements,
            "creates": action.create_set,
            "transitions": action.mutate_set,
            "witnesses": {
                "selector": action.name,
                "args": action.params,
            },
            "warnings": [
                "Builder preview is metadata-backed; live cell freshness and final fee/capacity must be checked at build time."
            ],
            "estimatedFee": serde_json::Value::Null,
            "requiredSigners": []
        });
        let plan = serde_json::json!({
            "status": "ok",
            "policy": "cellscript-action-builder-plan-v1",
            "headless": true,
            "ui_scope": "none",
            "input": input_path.display().to_string(),
            "action": action.name,
            "target_profile": result.metadata.target_profile.name,
            "artifact_hash": result.metadata.artifact_hash,
            "entry_witness_abi": {
                "required": !action.params.is_empty(),
                "params": action.params,
                "constraints": entry_constraints,
            },
            "builder_requirements": {
                "created_outputs": action.create_set,
                "mutated_outputs": action.mutate_set,
                "read_refs": action.read_refs,
                "verifier_obligations": action.verifier_obligations,
                "runtime_input_requirements": action.transaction_runtime_input_requirements,
                "fail_closed_runtime_features": action.fail_closed_runtime_features,
            },
            "ckb": ckb_contract,
            "transaction_draft": transaction_draft,
            "adapter_contract": adapter_contract,
            "preview": preview,
            "constraints_status": result.metadata.constraints.status,
            "constraints_failures": result.metadata.constraints.failures,
            "constraints_warnings": result.metadata.constraints.warnings,
        });
        let output_value = if args.fabric_intent {
            cellfabric_intent_envelope_json(&result.metadata, action, &plan, &input_path, &metadata_hash)?
        } else {
            plan
        };
        let json = serde_json::to_string_pretty(&output_value).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize action build output: {}", error))
        })?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            let label = if args.fabric_intent { "CellFabric intent envelope generated" } else { "Action build plan generated" };
            println!("{}", label.green());
            println!("  Output: {}", output_path.display());
        } else if args.json {
            println!("{}", json);
        } else if args.fabric_intent {
            println!("CellFabric intent envelope: {}", action.name);
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Status: requires-runtime-binding");
            println!("  App conflict key templates: {}", cellfabric_app_conflict_key_templates(&result.metadata.module, action).len());
            println!("  Embedded action plan: yes");
        } else {
            println!("Action build plan: {}", action.name);
            println!("  Target profile: {}", result.metadata.target_profile.name);
            println!("  Constraints: {}", result.metadata.constraints.status);
            println!("  Created outputs: {}", action.create_set.len());
            println!("  Mutated outputs: {}", action.mutate_set.len());
            println!("  Runtime input requirements: {}", action.transaction_runtime_input_requirements.len());
        }
        Ok(())
    }

    fn gen_builder(args: GenBuilderArgs) -> Result<()> {
        if args.target != "typescript" {
            return Err(crate::error::CompileError::without_span(format!(
                "unsupported builder target '{}'; supported targets: typescript",
                args.target
            )));
        }

        let metadata = if let Some(metadata_path) = args.metadata.as_deref() {
            read_metadata_json(metadata_path)?
        } else {
            let input_path = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
            let input = Utf8Path::from_path(&input_path).ok_or_else(|| {
                crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display()))
            })?;
            compile_path(
                input,
                CompileOptions {
                    opt_level: 1,
                    output: None,
                    debug: false,
                    target: None,
                    target_profile: args.target_profile.or_else(|| Some("ckb".to_string())),
                    primitive_compat: None,
                },
            )?
            .metadata
        };

        let metadata_hash = hash_json_value("metadata", &metadata)?;
        let selected_actions = selected_builder_actions(&metadata, args.action.as_deref())?;
        let locked_identity = if let Some(lockfile_path) = args.lockfile.as_deref() {
            Some(verify_builder_lockfile_identity(lockfile_path, &metadata, &metadata_hash)?)
        } else {
            None
        };
        let deployment_identity = if let Some(deployed_path) = args.deployed.as_deref() {
            let lockfile_path = args.lockfile.as_deref().ok_or_else(|| {
                crate::error::CompileError::without_span("gen-builder --deployed requires --lockfile for deployment identity binding")
            })?;
            Some(verify_builder_deployment_identity(
                lockfile_path,
                deployed_path,
                &metadata,
                &metadata_hash,
                args.deployment_network.as_deref(),
            )?)
        } else {
            None
        };
        let output_dir = args.output.unwrap_or_else(|| PathBuf::from("target").join("cellscript-builder").join("typescript"));
        let package_name = args.package_name.unwrap_or_else(|| default_builder_package_name(&metadata));
        let summary = write_typescript_builder_package(
            &output_dir,
            &package_name,
            &metadata,
            &metadata_hash,
            &selected_actions,
            locked_identity.as_ref(),
            deployment_identity.as_ref(),
            args.lockfile.as_deref(),
            args.deployed.as_deref(),
        )?;

        if args.json {
            print_json(&summary)?;
        } else {
            println!("{}", "TypeScript action builder generated".green());
            println!("  Output: {}", output_dir.display());
            println!("  Package: {}", package_name);
            println!("  Actions: {}", selected_actions.len());
        }

        Ok(())
    }

    /// Encode witness bytes for the generated `_cellscript_entry` wrapper.
    fn entry_witness(args: EntryWitnessArgs) -> Result<()> {
        if args.action.is_some() && args.lock.is_some() {
            return Err(crate::error::CompileError::without_span("entry-witness accepts either --action or --lock, not both"));
        }

        let input_path = args.input.clone().unwrap_or_else(|| PathBuf::from("."));
        let input = Utf8Path::from_path(&input_path)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
        let result = compile_path(
            input,
            CompileOptions {
                opt_level: 0,
                output: None,
                debug: false,
                target: args.target,
                target_profile: args.target_profile,
                primitive_compat: None,
            },
        )?;

        let selected = select_entry_witness_metadata(&result.metadata, args.action.as_deref(), args.lock.as_deref())?;
        if selected.params.is_empty() {
            return Err(crate::error::CompileError::without_span(format!(
                "{} '{}' has no parameters; `_cellscript_entry` witness ABI is only emitted for parameterized entries",
                selected.kind, selected.name
            )));
        }

        let payload_params = selected
            .params
            .iter()
            .filter(|param| {
                !param.lock_args_data_source
                    && !param.cell_bound_abi
                    && !param.ty.starts_with('&')
                    && !selected.runtime_bound_param_names.contains(&param.name)
            })
            .collect::<Vec<_>>();
        if args.args.len() != payload_params.len() {
            return Err(crate::error::CompileError::without_span(format!(
                "{} '{}' expects {} witness payload arg(s), got {}",
                selected.kind,
                selected.name,
                payload_params.len(),
                args.args.len()
            )));
        }

        let witness_args = payload_params
            .iter()
            .zip(args.args.iter())
            .map(|(param, value)| parse_entry_witness_arg(param, value))
            .collect::<Result<Vec<_>>>()?;
        let witness = crate::encode_entry_witness_args_for_params_with_runtime_bound(
            selected.params,
            &witness_args,
            &selected.runtime_bound_param_names,
        )
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to encode entry witness: {}", error)))?;
        let witness_hex = crate::hex_encode(&witness);

        if let Some(output_path) = &args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(output_path, &witness)?;
        }

        if args.json {
            let payload_param_names = payload_params.iter().map(|param| param.name.as_str()).collect::<Vec<_>>();
            let summary = serde_json::json!({
                "status": "ok",
                "abi": ENTRY_WITNESS_ABI,
                "entry_kind": selected.kind,
                "entry": selected.name,
                "witness_hex": witness_hex,
                "witness_size_bytes": witness.len(),
                "payload_args": witness_args.len(),
                "payload_params": payload_param_names,
                "output": args.output.as_ref().map(|path| path.display().to_string()),
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize entry witness summary: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        if let Some(output_path) = &args.output {
            println!("{}", "Entry witness encoded".green());
            println!("  ABI: {}", ENTRY_WITNESS_ABI);
            println!("  Entry: {} {}", selected.kind, selected.name);
            println!("  Output: {}", output_path.display());
            println!("  Hex: {}", witness_hex);
        } else {
            println!("{}", witness_hex);
        }
        Ok(())
    }

    fn verify_artifact(args: VerifyArtifactArgs) -> Result<()> {
        let artifact_path = Utf8Path::from_path(&args.artifact).ok_or_else(|| {
            crate::error::CompileError::without_span(format!("artifact path '{}' is not valid UTF-8", args.artifact.display()))
        })?;
        let metadata_path = match args.metadata {
            Some(path) => path,
            None => default_metadata_path_for_artifact(artifact_path).into_std_path_buf(),
        };

        let artifact_bytes = std::fs::read(&args.artifact).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read artifact '{}': {}", args.artifact.display(), error))
        })?;
        let metadata_bytes = std::fs::read(&metadata_path).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read metadata '{}': {}", metadata_path.display(), error))
        })?;
        let metadata: CompileMetadata = serde_json::from_slice(&metadata_bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to parse metadata '{}': {}", metadata_path.display(), error))
        })?;
        let result = validate_artifact_metadata(artifact_bytes, metadata)?;
        if args.verify_sources {
            validate_source_units_on_disk(&result.metadata)?;
        }
        validate_expected_target_profile(result.metadata.target_profile.name.as_str(), args.expect_target_profile.as_deref())?;
        validate_expected_metadata_hash(
            "artifact_hash",
            result.metadata.artifact_hash.as_deref(),
            args.expect_artifact_hash.as_deref(),
        )?;
        validate_expected_metadata_hash("source_hash", result.metadata.source_hash.as_deref(), args.expect_source_hash.as_deref())?;
        validate_expected_metadata_hash(
            "source_content_hash",
            result.metadata.source_content_hash.as_deref(),
            args.expect_source_content_hash.as_deref(),
        )?;
        validate_check_policy(
            &result.metadata,
            &CheckArgs {
                production: args.production,
                deny_fail_closed: args.deny_fail_closed,
                deny_ckb_runtime: args.deny_ckb_runtime,
                deny_runtime_obligations: args.deny_runtime_obligations,
                primitive_compat: args.primitive_compat,
                ..CheckArgs::default()
            },
        )?;

        let expected_target_profile_verified = args.expect_target_profile.is_some();
        let expected_hashes_verified =
            args.expect_artifact_hash.is_some() || args.expect_source_hash.is_some() || args.expect_source_content_hash.is_some();
        let policy_verified = args.production || args.deny_fail_closed || args.deny_ckb_runtime || args.deny_runtime_obligations;

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "artifact": args.artifact.display().to_string(),
                "metadata": metadata_path.display().to_string(),
                "metadata_schema_version": result.metadata.metadata_schema_version,
                "metadata_schema_versions": metadata_schema_versions_json(&result.metadata),
                "compiler_version": result.metadata.compiler_version,
                "artifact_format": result.artifact_format.display_name(),
                "target_profile": result.metadata.target_profile.name.as_str(),
                "artifact_hash": result.metadata.artifact_hash,
                "artifact_size_bytes": result.artifact_bytes.len(),
                "source_hash": result.metadata.source_hash,
                "source_content_hash": result.metadata.source_content_hash,
                "source_units": result.metadata.source_units.len(),
                "verifier_obligations": result.metadata.runtime.verifier_obligations.len(),
                "runtime_required_verifier_obligations": runtime_required_obligation_count(&result.metadata),
                "fail_closed_verifier_obligations": fail_closed_obligation_count(&result.metadata),
                "runtime_required_transaction_invariants": runtime_required_transaction_invariant_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subconditions": runtime_required_transaction_invariant_checked_subcondition_count(&result.metadata),
                "runtime_required_transaction_invariant_checked_subcondition_summaries": transaction_invariant_checked_subcondition_summaries(&result.metadata),
                "transaction_runtime_input_requirements": transaction_runtime_input_requirement_count(&result.metadata),
                "transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries(&result.metadata),
                "checked_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "checked-runtime"),
                "checked_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "checked-runtime"),
                "runtime_required_transaction_runtime_input_requirements": transaction_runtime_input_requirement_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_requirement_summaries": transaction_runtime_input_requirement_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blockers": transaction_runtime_input_blocker_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_summaries": transaction_runtime_input_blocker_summaries_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_classes": transaction_runtime_input_blocker_class_count_by_status(&result.metadata, "runtime-required"),
                "runtime_required_transaction_runtime_input_blocker_class_summaries": transaction_runtime_input_blocker_class_summaries_by_status(&result.metadata, "runtime-required"),
                "checked_pool_invariant_families": checked_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_families": runtime_required_pool_invariant_family_count(&result.metadata),
                "runtime_required_pool_invariant_blocker_classes": pool_invariant_family_blocker_class_count(&result.metadata, "runtime-required"),
                "runtime_required_pool_invariant_blocker_class_summaries": pool_invariant_family_blocker_class_summaries(&result.metadata, "runtime-required"),
                "pool_runtime_input_requirements": pool_runtime_input_requirement_count(&result.metadata),
                "pool_runtime_input_requirement_summaries": pool_runtime_input_requirement_summaries(&result.metadata),
                "sources_verified": args.verify_sources,
                "expected_target_profile_verified": expected_target_profile_verified,
                "expected_hashes_verified": expected_hashes_verified,
                "policy_verified": policy_verified,
                "constraints": &result.metadata.constraints,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize verification summary: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Artifact verification succeeded".green());
        println!("  Artifact: {}", args.artifact.display());
        println!("  Metadata: {}", metadata_path.display());
        println!("  Metadata schema: {}", result.metadata.metadata_schema_version);
        println!(
            "  Metadata schema components: source={}, artifact={}, constraints={}",
            result.metadata.source_metadata_schema_version,
            result.metadata.artifact_metadata_schema_version,
            result.metadata.constraints_metadata_schema_version
        );
        println!("  Compiler: {}", result.metadata.compiler_version);
        println!("  Format: {}", result.artifact_format.display_name());
        println!("  Target profile: {}", result.metadata.target_profile.name);
        println!("  Hash: {}", result.metadata.artifact_hash.as_deref().unwrap_or("missing"));
        println!("  Size: {} bytes", result.artifact_bytes.len());
        if expected_target_profile_verified {
            println!("  Expected target profile: verified");
        }
        if expected_hashes_verified {
            println!("  Expected hashes: verified");
        }
        if args.verify_sources {
            println!("  Sources: verified {} unit(s)", result.metadata.source_units.len());
        }
        if policy_verified {
            println!("  Policy: verified");
        }
        Ok(())
    }

    fn run(args: RunArgs) -> Result<()> {
        let opt_level = if args.release { 3 } else { 0 };
        let compile_result = compile_path(
            ".",
            CompileOptions {
                opt_level,
                output: None,
                debug: false,
                target: Some("riscv64-elf".to_string()),
                target_profile: None,
                primitive_compat: None,
            },
        );

        if args.simulate {
            let result = compile_result?;
            return Self::run_simulate(&result, &args);
        }

        #[cfg(feature = "vm-runner")]
        {
            let result = compile_result?;

            let parameterized_entries = result
                .metadata
                .actions
                .iter()
                .filter(|action| !action.params.is_empty())
                .map(|action| format!("action {}", action.name))
                .chain(result.metadata.locks.iter().filter(|lock| !lock.params.is_empty()).map(|lock| format!("lock {}", lock.name)))
                .collect::<Vec<_>>();
            if !parameterized_entries.is_empty() {
                eprintln!(
                    "{}",
                    format!(
                        "Warning: {} requires transaction/parameter ABI context; falling back to simulate mode",
                        parameterized_entries.join(", ")
                    )
                    .yellow()
                );
                return Self::run_simulate(&result, &args);
            }

            if result.metadata.runtime.ckb_runtime_required {
                eprintln!(
                    "{}",
                    format!(
                        "Warning: CKB runtime required ({}); falling back to simulate mode",
                        result.metadata.runtime.ckb_runtime_features.join(", ")
                    )
                    .yellow()
                );
                return Self::run_simulate(&result, &args);
            }

            if !result.metadata.runtime.standalone_runner_compatible {
                eprintln!("{}", "Warning: ELF is not standalone-compatible; falling back to simulate mode".yellow());
                return Self::run_simulate(&result, &args);
            }

            let vm_args = args.args.into_iter().map(|arg| arg.into_bytes()).collect::<Vec<_>>();
            let cycles = run_elf_in_ckb_vm(&result.artifact_bytes, &vm_args)?;

            println!("{}", "Run complete".green());
            println!("  Artifact format: {}", result.artifact_format.display_name());
            println!("  Cycles: {}", cycles);
            Ok(())
        }

        #[cfg(not(feature = "vm-runner"))]
        {
            let mode = if args.release { "release" } else { "debug" };
            Self::experimental_command(
                "run",
                &format!(
                    "feature-gated VM backend is not enabled (requested {}, {} argument(s)); use --simulate for AST-level simulation or compile with --features vm-runner to execute",
                    mode,
                    args.args.len()
                ),
            )
        }
    }

    fn run_simulate(compile_result: &crate::CompileResult, _args: &RunArgs) -> Result<()> {
        use crate::simulate::{SimValue, SimulateInterpreter};

        let modules = crate::load_modules_for_input(".")?;
        let module =
            modules.iter().find(|module| module.ast.name == compile_result.metadata.module).map(|module| &module.ast).ok_or_else(
                || {
                    crate::error::CompileError::without_span(format!(
                        "failed to load module '{}' for simulation",
                        compile_result.metadata.module
                    ))
                },
            )?;

        let entry = compile_result
            .metadata
            .actions
            .iter()
            .find(|a| a.name == "main")
            .or_else(|| compile_result.metadata.actions.iter().find(|a| a.params.is_empty()));

        let Some(entry) = entry else {
            return Err(crate::error::CompileError::without_span(
                "no suitable entry point found for simulation; define an action main() or a zero-argument action",
            ));
        };

        let mut interp = SimulateInterpreter::new(module, 100_000);
        let sim_args: Vec<SimValue> = Vec::new();
        let sim_result = interp
            .simulate_action(&entry.name, &sim_args)
            .map_err(|e| crate::error::CompileError::without_span(format!("simulation error: {}", e)))?;

        println!("{}", "Simulate complete".green());
        println!("  Entry: action {}", sim_result.entry_name);
        println!("  Steps: {}", sim_result.steps);
        if sim_result.has_cell_ops {
            println!("  Cell operations: {} (simulated)", "yes".yellow());
        } else {
            println!("  Cell operations: none (pure computation)");
        }
        println!("  Result: {}", sim_result.return_value);

        if !sim_result.trace.is_empty() {
            println!("  Trace:");
            for event in &sim_result.trace {
                println!("{}", event);
            }
        }

        Ok(())
    }

    fn publish(args: PublishArgs) -> Result<()> {
        let pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;

        if args.dry_run {
            let mut issues = Vec::<String>::new();
            if manifest.package.name.is_empty() {
                issues.push("package name is empty".to_string());
            }
            if manifest.package.version.is_empty() {
                issues.push("package version is empty".to_string());
            }
            if manifest.package.description.is_empty() {
                issues.push("package description is missing".to_string());
            }
            if manifest.package.license.is_empty() {
                issues.push("package license is missing".to_string());
            }
            if manifest.package.repository.is_empty() {
                issues.push("package repository is missing".to_string());
            }
            if manifest.package.namespace.is_none() {
                issues.push("package namespace is missing (required for publishing)".to_string());
            }

            let entry_path = std::path::Path::new(".").join(&manifest.package.entry);
            if !entry_path.exists() {
                issues.push(format!("entry file '{}' does not exist", manifest.package.entry));
            }

            let compile_result = compile_path(".", CompileOptions::default());
            match compile_result {
                Ok(result) => {
                    println!("{}", "Publish dry-run passed".green());
                    println!("  Package: {} v{}", manifest.package.name, manifest.package.version);
                    println!("  Artifact: {} ({} bytes)", result.artifact_format.display_name(), result.artifact_bytes.len());
                }
                Err(e) => {
                    issues.push(format!("compilation failed: {}", e));
                }
            }

            if !issues.is_empty() {
                println!("{}", "Issues found:".yellow());
                for issue in &issues {
                    println!("  - {}", issue);
                }
                return Err(crate::error::CompileError::without_span(format!("publish dry-run found {} issue(s)", issues.len())));
            }

            Ok(())
        } else {
            // Real publish: compute source_hash, update registry.json
            let namespace = manifest.package.namespace.clone().ok_or_else(|| {
                crate::error::CompileError::without_span(
                    "package namespace is required for publishing; add namespace = \"<your-namespace>\" to [package] in Cell.toml",
                )
            })?;

            if manifest.package.name.is_empty() {
                return Err(crate::error::CompileError::without_span("package name is empty"));
            }
            if manifest.package.version.is_empty() {
                return Err(crate::error::CompileError::without_span("package version is empty"));
            }

            // Compute source_hash
            let source_hash = crate::package::registry::compute_source_hash(std::path::Path::new("."))?;

            // Compile to get build artifact hashes
            let result = compile_path(".", CompileOptions::default())?;

            // Build registry version entry
            let version_entry = crate::package::registry::RegistryVersion {
                version: manifest.package.version.clone(),
                tag: format!("v{}", manifest.package.version),
                source_hash: source_hash.clone(),
                cellscript_version: result.metadata.compiler_version.clone(),
                dependencies: {
                    let mut deps = std::collections::BTreeMap::new();
                    for (dep_name, dep) in &manifest.dependencies {
                        let (ns, ver) = match dep {
                            crate::package::Dependency::Simple(v) => {
                                (manifest.package.namespace.clone().unwrap_or_default(), v.clone())
                            }
                            crate::package::Dependency::Detailed(d) => {
                                let ns = d.namespace.clone().unwrap_or_else(|| manifest.package.namespace.clone().unwrap_or_default());
                                (ns, d.version.clone())
                            }
                        };
                        deps.insert(dep_name.clone(), crate::package::registry::RegistryDependencyRef { namespace: ns, version: ver });
                    }
                    deps
                },
                abi_index: Some(metadata_abi_hash(&result.metadata)?),
                schema_hash: Some(result.metadata.molecule_schema_manifest.manifest_hash.clone()),
                license: if manifest.package.license.is_empty() { None } else { Some(manifest.package.license.clone()) },
                released_at: Some({
                    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    // Lightweight ISO 8601 formatting without chrono dependency
                    let days_since_epoch = secs / 86400;
                    let time_of_day = secs % 86400;
                    let (year, month, day) = civil_date_from_days(days_since_epoch as i32);
                    let hour = (time_of_day / 3600) as u8;
                    let minute = ((time_of_day % 3600) / 60) as u8;
                    let second = (time_of_day % 60) as u8;
                    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
                }),
                yanked: false,
                audit: None,
            };

            // Write to registry.json
            crate::package::registry::RegistryIndex::append_version(
                std::path::Path::new("."),
                &manifest.package.name,
                &namespace,
                version_entry,
            )?;

            println!("{}", "Published".green());
            println!("  Package: {}/{} v{}", namespace, manifest.package.name, manifest.package.version);
            println!("  Source hash: {}", source_hash);
            println!();
            println!("  Next steps:");
            println!("    git add registry.json");
            println!("    git commit -m \"publish v{}\"", manifest.package.version);
            println!("    git tag v{}", manifest.package.version);
            println!("    git push --tags");
            Ok(())
        }
    }

    fn install(args: InstallArgs) -> Result<()> {
        let pm = PackageManager::new(".");

        let _manifest = pm.read_manifest()?;

        if let Some(git_url) = &args.git {
            let crate_name = args.crate_name.clone().unwrap_or_else(|| {
                git_url.trim_end_matches('/').trim_end_matches(".git").split('/').next_back().unwrap_or("unknown").to_string()
            });

            let dep = DetailedDependency {
                version: args.version.clone().unwrap_or_else(|| "*".to_string()),
                namespace: None,
                git: Some(git_url.clone()),
                branch: None,
                tag: None,
                rev: None,
                path: None,
                optional: false,
                features: Vec::new(),
                default_features: true,
            };

            pm.resolve_from_git(&crate_name, git_url, &dep)?;

            let mut manifest = pm.read_manifest()?;
            manifest.dependencies.insert(crate_name.clone(), Dependency::Detailed(dep));
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", format!("Installed {} from git {}", crate_name, git_url).green());
            Ok(())
        } else if let Some(path) = &args.path {
            let crate_name =
                args.crate_name.clone().unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());

            let dep = DetailedDependency {
                version: args.version.clone().unwrap_or_else(|| "*".to_string()),
                namespace: None,
                git: None,
                branch: None,
                tag: None,
                rev: None,
                path: Some(path.to_string_lossy().to_string()),
                optional: false,
                features: Vec::new(),
                default_features: true,
            };

            let manifest_for_check = pm.read_manifest()?;
            validate_not_self_dependency(&crate_name, &Dependency::Detailed(dep.clone()), &manifest_for_check)?;

            pm.resolve_from_path(&crate_name, &path.to_string_lossy())?;

            let mut manifest = pm.read_manifest()?;
            manifest.dependencies.insert(crate_name.clone(), Dependency::Detailed(dep));
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", format!("Installed {} from path {}", crate_name, path.display()).green());
            Ok(())
        } else if let Some(crate_name) = &args.crate_name {
            // Support both:
            //   cellc install cellscript/amm@1.2.0   (Go-style combined format)
            //   cellc install amm --namespace cellscript --version 1.2.0
            let (resolved_name, resolved_namespace, resolved_version) = if args.namespace.is_none() && args.version.is_none() {
                // Try parsing namespace/name@version format
                if let Some((ns, rest)) = crate_name.split_once('/') {
                    if let Some((name, ver)) = rest.split_once('@') {
                        (name.to_string(), Some(ns.to_string()), Some(ver.to_string()))
                    } else {
                        (rest.to_string(), Some(ns.to_string()), None)
                    }
                } else if let Some((name, ver)) = crate_name.split_once('@') {
                    (name.to_string(), None, Some(ver.to_string()))
                } else {
                    (crate_name.clone(), None, None)
                }
            } else {
                (crate_name.clone(), args.namespace.clone(), args.version.clone())
            };

            let version = resolved_version.unwrap_or_else(|| "*".to_string());

            let _resolved = pm.resolve_from_registry_with_namespace(&resolved_name, &version, resolved_namespace.as_deref())?;

            let dep = if resolved_namespace.is_some() {
                Dependency::Detailed(DetailedDependency {
                    version,
                    namespace: resolved_namespace.clone(),
                    git: None,
                    branch: None,
                    tag: None,
                    rev: None,
                    path: None,
                    optional: false,
                    features: Vec::new(),
                    default_features: true,
                })
            } else {
                Dependency::Simple(version)
            };

            let mut manifest = pm.read_manifest()?;
            validate_not_self_dependency(&resolved_name, &dep, &manifest)?;
            manifest.dependencies.insert(resolved_name.clone(), dep);
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            let ns_display = resolved_namespace.as_deref().unwrap_or("<default>");
            println!("{}", format!("Installed {}/{} from registry", ns_display, resolved_name).green());
            Ok(())
        } else {
            let mut pm = PackageManager::new(".");
            pm.resolve_dependencies()?;

            let mut lockfile = Lockfile::read_from_root(std::path::Path::new("."))?.unwrap_or_default();
            lockfile.replace_with_resolved(pm.get_resolved());
            lockfile.write_to_root(std::path::Path::new("."))?;

            println!("{}", "Dependencies resolved and lockfile updated".green());
            Ok(())
        }
    }

    fn update() -> Result<()> {
        let mut pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;

        pm.resolve_dependencies()?;

        let mut lockfile = Lockfile::read_from_root(std::path::Path::new("."))?.unwrap_or_default();

        lockfile.replace_with_resolved(pm.get_resolved());
        lockfile.write_to_root(std::path::Path::new("."))?;

        let resolved = pm.get_resolved();
        if resolved.is_empty() {
            println!("{}", "No dependencies to update".green());
        } else {
            println!("{}", format!("Updated {} dependencies", resolved.len()).green());
            for (name, package) in resolved {
                let source = match &package.source {
                    crate::package::PackageSource::Local(path) => format!("path: {}", path.display()),
                    crate::package::PackageSource::Git { url, revision } => format!("git: {}#{}", url, revision),
                    crate::package::PackageSource::Registry { registry, namespace, version, .. } => {
                        format!("registry: {}/{}@{}", registry, namespace, version)
                    }
                };
                println!("  {} v{} ({})", name, package.version, source);
            }
        }

        let lockfile_issues = lockfile.consistency_issues_with_resolved(&manifest, resolved);
        if !lockfile_issues.is_empty() {
            println!("{}", "Warning: lockfile is not consistent with Cell.toml".yellow());
            for issue in lockfile_issues {
                println!("  - {}", issue);
            }
        }

        Ok(())
    }

    fn info(args: InfoArgs) -> Result<()> {
        let pm = PackageManager::new(".");
        let manifest = pm.read_manifest()?;

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "manifest": "Cell.toml",
                "package": manifest.package,
                "dependencies": manifest.dependencies,
                "dev_dependencies": manifest.dev_dependencies,
                "build": manifest.build,
                "policy": manifest.policy,
                "deploy": manifest.deploy,
                "metadata": manifest.metadata,
            });
            let json = serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize package info summary: {}", error))
            })?;
            println!("{}", json);
            return Ok(());
        }

        println!("{}", "Package Info:".bold());
        println!("  Name:        {}", manifest.package.name);
        println!("  Version:     {}", manifest.package.version);
        println!("  Description: {}", manifest.package.description);
        println!("  License:     {}", manifest.package.license);
        println!("  Authors:     {}", manifest.package.authors.join(", "));
        println!("  Entry:       {}", manifest.package.entry);
        println!("  Dependencies:");
        for (name, dep) in &manifest.dependencies {
            println!("    - {}: {:?}", name, dep);
        }

        Ok(())
    }

    fn login(args: LoginArgs) -> Result<()> {
        let registry = args.registry.unwrap_or_else(|| "https://cellscript.io".to_string());

        let config_dir = dirs_config_dir();
        std::fs::create_dir_all(&config_dir).map_err(|e| {
            crate::error::CompileError::without_span(format!("failed to create config directory '{}': {}", config_dir.display(), e))
        })?;

        let credentials_path = config_dir.join("credentials.toml");

        let mut credentials: HashMap<String, RegistryCredential> = if credentials_path.exists() {
            let content = std::fs::read_to_string(&credentials_path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };

        eprintln!("Logging in to {}", registry);
        eprintln!("Enter your authentication token (or press Enter to use environment variable CELLSCRIPT_TOKEN):");

        let mut token = String::new();
        if std::io::stdin().read_line(&mut token).is_err() || token.trim().is_empty() {
            token = std::env::var("CELLSCRIPT_TOKEN").unwrap_or_default();
        }

        if token.trim().is_empty() {
            return Err(crate::error::CompileError::without_span(
                "no authentication token provided; set CELLSCRIPT_TOKEN environment variable or enter token interactively",
            ));
        }

        let token = token.trim().to_string();

        credentials.insert(registry.clone(), RegistryCredential { registry: registry.clone(), token });

        let content = toml::to_string_pretty(&credentials)?;
        std::fs::write(&credentials_path, content)?;

        println!("{}", format!("Login credentials saved for {}", registry).green());
        println!("  Config directory: {}", config_dir.display());
        Ok(())
    }

    fn registry_verify(args: RegistryVerifyArgs) -> Result<()> {
        let root = std::path::Path::new(".");

        // Read Cell.lock
        let lockfile = Lockfile::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Cell.lock not found; run 'cellc build' first"))?;

        // Read Deployed.toml
        let deployed = crate::package::DeployedManifest::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Deployed.toml not found; deploy the contract first"))?;

        let mut violations = Vec::new();

        if lockfile.package.name != deployed.package.name {
            violations.push(format!(
                "package name mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                lockfile.package.name, deployed.package.name
            ));
        }
        if lockfile.package.version != deployed.package.version {
            violations.push(format!(
                "package version mismatch: Cell.lock has '{}', Deployed.toml has '{}'",
                lockfile.package.version, deployed.package.version
            ));
        }
        if let (Some(lock_hash), Some(deployed_hash)) = (&lockfile.package.source_hash, &deployed.package.source_hash) {
            if lock_hash != deployed_hash {
                violations.push(format!("source_hash mismatch: Cell.lock has '{}', Deployed.toml has '{}'", lock_hash, deployed_hash));
            }
        } else {
            violations.push("source_hash must be present in both Cell.lock and Deployed.toml".to_string());
        }

        match &lockfile.package_build {
            Some(build) => push_missing_locked_build_identity("Cell.lock [package.build]", build, &mut violations),
            None => violations.push("Cell.lock has no [package.build]".to_string()),
        }
        match &deployed.build {
            Some(build) => push_missing_deployed_build_identity("Deployed.toml [build]", build, &mut violations),
            None => violations.push("Deployed.toml has no [build]".to_string()),
        }

        if let (Some(build), Some(deployed_build)) = (&lockfile.package_build, &deployed.build) {
            compare_optional_build_field(
                "compiler_version",
                &build.compiler_version,
                &deployed_build.compiler_version,
                &mut violations,
            );
            compare_optional_build_field("artifact_hash", &build.artifact_hash, &deployed_build.artifact_hash, &mut violations);
            compare_optional_build_field("metadata_hash", &build.metadata_hash, &deployed_build.metadata_hash, &mut violations);
            compare_optional_build_field("schema_hash", &build.schema_hash, &deployed_build.schema_hash, &mut violations);
            compare_optional_build_field(
                "cell_data_codec_manifest_hash",
                &build.cell_data_codec_manifest_hash,
                &deployed_build.cell_data_codec_manifest_hash,
                &mut violations,
            );
            compare_optional_build_field("abi_hash", &build.abi_hash, &deployed_build.abi_hash, &mut violations);
            compare_optional_build_field(
                "constraints_hash",
                &build.constraints_hash,
                &deployed_build.constraints_hash,
                &mut violations,
            );
        }

        // Check deployment records
        let mut seen_networks = BTreeSet::new();
        for deployment in &deployed.deployments {
            seen_networks.insert(deployment.network.clone());
            push_deployment_status_violation(deployment, &mut violations);

            let Some(deployment_ref) = lockfile.deployment.get(&deployment.network) else {
                violations.push(format!("deployment for network '{}' is missing from Cell.lock", deployment.network));
                continue;
            };

            if deployment_ref.record.is_empty() {
                violations.push(format!("deployment ref for network '{}' has empty record", deployment.network));
            } else {
                let chain_record = format!("{}:{}", deployment.chain_id, deployment.out_point);
                let network_record = format!("{}:{}", deployment.network, deployment.out_point);
                if deployment_ref.record != deployment.out_point
                    && deployment_ref.record != chain_record
                    && deployment_ref.record != network_record
                {
                    violations.push(format!(
                        "deployment record mismatch for network '{}': Cell.lock has '{}', Deployed.toml out_point is '{}'",
                        deployment.network, deployment_ref.record, deployment.out_point
                    ));
                }
            }

            match &deployment_ref.code_hash {
                Some(code_hash) if code_hash == &deployment.code_hash => {}
                Some(code_hash) => violations.push(format!(
                    "code_hash mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, code_hash, deployment.code_hash
                )),
                None => violations.push(format!("deployment ref for network '{}' has no code_hash", deployment.network)),
            }
            match &deployment_ref.out_point {
                Some(out_point) if out_point == &deployment.out_point => {}
                Some(out_point) => violations.push(format!(
                    "out_point mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, out_point, deployment.out_point
                )),
                None => violations.push(format!("deployment ref for network '{}' has no out_point", deployment.network)),
            }
            match &deployment_ref.data_hash {
                Some(data_hash) if data_hash == &deployment.data_hash => {}
                Some(data_hash) => violations.push(format!(
                    "data_hash mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
                    deployment.network, data_hash, deployment.data_hash
                )),
                None => violations.push(format!("deployment ref for network '{}' has no data_hash", deployment.network)),
            }
            if let Some(record_hash) = &deployment_ref.record_hash {
                let computed = hash_json_value("deployment record", deployment)?;
                if record_hash != &computed {
                    violations.push(format!(
                        "record_hash mismatch for network '{}': Cell.lock has '{}', computed '{}'",
                        deployment.network, record_hash, computed
                    ));
                }
            }
        }
        for network in lockfile.deployment.keys() {
            if !seen_networks.contains(network) {
                violations.push(format!("Cell.lock has stale deployment ref for network '{}'", network));
            }
        }
        let trust_report =
            verify_registry_trust_metadata(&deployed, args.require_publisher_signature, args.require_audit_report, &mut violations);
        let live_report = if args.live {
            let rpc_url = args.rpc_url.clone().or_else(|| std::env::var(CELLSCRIPT_CKB_RPC_URL_ENV).ok()).ok_or_else(|| {
                crate::error::CompileError::without_span(format!(
                    "registry verify --live requires --rpc-url or {}",
                    CELLSCRIPT_CKB_RPC_URL_ENV
                ))
            })?;
            Some(verify_live_deployments(&deployed, &rpc_url, args.network.as_deref(), &mut violations)?)
        } else {
            None
        };

        if args.json {
            let summary = serde_json::json!({
                "status": if violations.is_empty() { "ok" } else { "failed" },
                "trust": trust_report,
                "live": live_report.unwrap_or_else(|| serde_json::json!({
                    "enabled": false,
                    "evidence": []
                })),
                "violations": violations,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|e| crate::error::CompileError::without_span(format!("failed to serialize: {}", e)))?;
            println!("{}", json);
            if !violations.is_empty() {
                return Err(crate::error::CompileError::without_span("registry verification failed"));
            }
        } else if violations.is_empty() {
            println!("{}", "Registry verification passed".green());
        } else {
            println!("{}", "Registry verification failed".red());
            for v in &violations {
                println!("  - {}", v);
            }
            return Err(crate::error::CompileError::without_span("registry verification failed"));
        }

        Ok(())
    }

    fn package_verify(args: PackageVerifyArgs) -> Result<()> {
        let root = std::path::Path::new(".");
        let mut pm = PackageManager::new(root);
        let manifest = pm.read_manifest()?;

        // Read Cell.lock
        let lockfile = Lockfile::read_from_root(root)?
            .ok_or_else(|| crate::error::CompileError::without_span("Cell.lock not found; run 'cellc build' first"))?;

        let mut violations = Vec::new();

        if lockfile.package.name != manifest.package.name {
            violations.push(format!(
                "package name mismatch: Cell.toml has '{}', Cell.lock has '{}'",
                manifest.package.name, lockfile.package.name
            ));
        }
        if lockfile.package.version != manifest.package.version {
            violations.push(format!(
                "package version mismatch: Cell.toml has '{}', Cell.lock has '{}'",
                manifest.package.version, lockfile.package.version
            ));
        }
        if lockfile.package.namespace != manifest.package.namespace {
            violations.push(format!(
                "package namespace mismatch: Cell.toml has '{:?}', Cell.lock has '{:?}'",
                manifest.package.namespace, lockfile.package.namespace
            ));
        }

        match &lockfile.package.source_hash {
            Some(source_hash) => {
                let computed = crate::package::registry::compute_source_hash(root)?;
                if &computed != source_hash {
                    violations.push(format!("source_hash mismatch: Cell.lock has '{}', computed '{}'", source_hash, computed));
                }
            }
            None => violations.push("Cell.lock [package] has no source_hash; run 'cellc build' to populate".to_string()),
        }

        match &lockfile.package_build {
            Some(build) => push_missing_locked_build_identity("Cell.lock [package.build]", build, &mut violations),
            None => violations.push("Cell.lock has no [package.build]; run 'cellc build' to populate build identity".to_string()),
        }

        pm.resolve_dependencies()?;
        for issue in lockfile.consistency_issues_with_resolved(&manifest, pm.get_resolved()) {
            violations.push(issue);
        }
        for (name, locked) in &lockfile.dependencies {
            if matches!(locked.source, crate::package::LockedSource::Registry { .. }) && locked.source_hash.is_none() {
                violations.push(format!("registry dependency '{}' has no source_hash in Cell.lock", name));
            }
        }

        if args.json {
            let summary = serde_json::json!({
                "status": if violations.is_empty() { "ok" } else { "failed" },
                "violations": violations,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|e| crate::error::CompileError::without_span(format!("failed to serialize: {}", e)))?;
            println!("{}", json);
            if !violations.is_empty() {
                return Err(crate::error::CompileError::without_span("package verification failed"));
            }
        } else if violations.is_empty() {
            println!("{}", "Package verification passed".green());
        } else {
            println!("{}", "Package verification failed".red());
            for v in &violations {
                println!("  - {}", v);
            }
            return Err(crate::error::CompileError::without_span("package verification failed"));
        }

        Ok(())
    }

    fn registry_add(args: RegistryAddArgs) -> Result<()> {
        let root = std::path::Path::new(".");
        let cache_dir = root.join(".cell/registry-cache");
        let registry_url = crate::package::registry::default_registry_url();
        let discovery = crate::package::registry::DiscoveryIndex::new(&registry_url, &cache_dir);

        let entry_path = discovery.add_entry(&args.namespace, &args.name, &args.source)?;
        let discovery_clone = entry_path.parent().and_then(|path| path.parent()).unwrap_or(cache_dir.as_path());
        let entry_rel = entry_path.strip_prefix(discovery_clone).unwrap_or(entry_path.as_path());

        println!("{}", "Registry entry added".green());
        println!("  Namespace: {}", args.namespace);
        println!("  Name: {}", args.name);
        println!("  Source: {}", args.source);
        println!();
        println!("  Next steps:");
        println!("    cd {} && git add {}", discovery_clone.display(), entry_rel.display());
        println!("    git commit -m \"add {}/{}\"", args.namespace, args.name);
        println!("    Open a PR to the cellscript-registry repository");

        Ok(())
    }

    fn certify(args: CertifyArgs) -> Result<()> {
        if args.plugin != NOVASEAL_CERTIFICATION_PLUGIN {
            return Err(crate::error::CompileError::without_span(format!(
                "unknown certification plugin '{}'; available plugins: {}",
                args.plugin, NOVASEAL_CERTIFICATION_PLUGIN
            )));
        }

        let repo_root = args.repo_root.unwrap_or(std::env::current_dir()?);
        let report_provided = args.report.is_some();
        let plugin_report_path = args.report.clone().unwrap_or_else(|| repo_root.join("target/novaseal-production-gates.json"));
        let report_generated = !report_provided;

        let plugin_report = if report_provided {
            read_json_value(&plugin_report_path)?
        } else {
            let report = super::novaseal_certification::build_report(&repo_root)?;
            if let Some(parent) = plugin_report_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(
                &plugin_report_path,
                serde_json::to_string_pretty(&report).map_err(|error| {
                    crate::error::CompileError::without_span(format!("failed to serialize NovaSeal production-gate report: {}", error))
                })?,
            )?;
            report
        };

        let implementation_path = repo_root.join("src/cli/novaseal_certification.rs");
        let summary = novaseal_certification_summary(
            &plugin_report,
            &repo_root,
            &plugin_report_path,
            &implementation_path,
            report_generated,
            args.require_production,
        )?;
        let output_path = args.output.unwrap_or_else(|| repo_root.join("target/cellscript-certification/novaseal-profile-v0.json"));
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            &output_path,
            serde_json::to_string_pretty(&summary).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize certification report: {}", error))
            })?,
        )?;

        if args.json {
            print_json(&summary)?;
        } else {
            println!("Certification report generated");
            println!("  Plugin: {}", args.plugin);
            println!("  Status: {}", summary["status"].as_str().unwrap_or("unknown"));
            println!("  Level: {}", summary["certification_level"].as_str().unwrap_or("unknown"));
            println!("  Output: {}", output_path.display());
            println!("  Plugin report: {}", plugin_report_path.display());
        }

        if summary["status"].as_str() == Some("passed") {
            Ok(())
        } else {
            Err(crate::error::CompileError::without_span(novaseal_certification_failure_message(&summary)))
        }
    }
}

#[cfg(feature = "vm-runner")]
type CliVmMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RegistryCredential {
    registry: String,
    token: String,
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
/// Implements the civil date algorithm from Howard Hinnant.
fn civil_date_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
}

fn compile_cli_input(input: Option<&PathBuf>, options: CompileOptions) -> Result<crate::CompileResult> {
    let input_path = input.cloned().unwrap_or_else(|| PathBuf::from("."));
    let input = Utf8Path::from_path(&input_path)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
    compile_path(input, options)
}

fn read_metadata_json(path: &Path) -> Result<CompileMetadata> {
    let bytes = std::fs::read(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read metadata '{}': {}", path.display(), error))
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to parse metadata '{}': {}", path.display(), error)))
}

fn read_json_value(path: &Path) -> Result<serde_json::Value> {
    let bytes = std::fs::read(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to read JSON '{}': {}", path.display(), error)))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to parse JSON '{}': {}", path.display(), error)))
}

fn print_json(value: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON: {}", error)))?;
    println!("{}", json);
    Ok(())
}

fn ckb_blake2b_file_hash(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to read '{}': {}", path.display(), error)))?;
    Ok(Some(crate::hex_encode(&crate::ckb_blake2b256(&bytes))))
}

fn json_pointer_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(serde_json::Value::as_str)
}

fn json_pointer_bool(value: &serde_json::Value, pointer: &str) -> bool {
    value.pointer(pointer).and_then(serde_json::Value::as_bool).unwrap_or(false)
}

fn novaseal_gate_status<'a>(report: &'a serde_json::Value, gate_name: &str) -> Option<&'a str> {
    report.get("gates")?.as_array()?.iter().find_map(|gate| {
        let name = gate.get("name").and_then(serde_json::Value::as_str)?;
        if name == gate_name {
            gate.get("status").and_then(serde_json::Value::as_str)
        } else {
            None
        }
    })
}

fn novaseal_certification_failure_message(summary: &serde_json::Value) -> String {
    let reason = summary.get("failure_reason").unwrap_or(&serde_json::Value::Null);
    if let Some(message) = json_pointer_str(reason, "/message") {
        return message.to_string();
    }
    if let Some(message) = reason.as_str() {
        return message.to_string();
    }
    if !reason.is_null() {
        return serde_json::to_string(reason).unwrap_or_else(|_| "certification failed".to_string());
    }
    "certification failed".to_string()
}

fn novaseal_failed_dimensions(plugin_report: &serde_json::Value, v1_readiness: &serde_json::Value) -> serde_json::Value {
    let mut seen = BTreeSet::new();
    let mut dimensions = Vec::new();
    for source in [plugin_report.get("failed_dimensions"), v1_readiness.get("failed_dimensions")] {
        let Some(items) = source.and_then(serde_json::Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(name) = item.as_str() else {
                continue;
            };
            if seen.insert(name.to_string()) {
                dimensions.push(serde_json::Value::String(name.to_string()));
            }
        }
    }
    serde_json::Value::Array(dimensions)
}

fn novaseal_failed_dimension_matches(failed_dimensions: &serde_json::Value, expected: &[&str]) -> bool {
    failed_dimensions.as_array().is_some_and(|dimensions| {
        dimensions.iter().filter_map(serde_json::Value::as_str).any(|dimension| expected.contains(&dimension))
    })
}

#[allow(clippy::too_many_arguments)]
fn novaseal_certification_summary(
    plugin_report: &serde_json::Value,
    repo_root: &Path,
    plugin_report_path: &Path,
    implementation_path: &Path,
    report_generated: bool,
    require_production: bool,
) -> Result<serde_json::Value> {
    let plugin_report_hash = ckb_blake2b_file_hash(plugin_report_path)?.ok_or_else(|| {
        crate::error::CompileError::without_span(format!(
            "NovaSeal plugin report '{}' is not a regular file",
            plugin_report_path.display()
        ))
    })?;
    let implementation_hash = ckb_blake2b_file_hash(implementation_path)?;
    let profile_certification = plugin_report.get("profile_certification").unwrap_or(&serde_json::Value::Null);
    let v1_readiness = plugin_report.get("v1_readiness").unwrap_or(&serde_json::Value::Null);

    let mut checks = vec![
        ("plugin_report_schema", json_pointer_str(plugin_report, "/schema") == Some(NOVASEAL_PLUGIN_REPORT_SCHEMA)),
        (
            "profile_certification_schema",
            json_pointer_str(profile_certification, "/schema") == Some(NOVASEAL_PROFILE_CERTIFICATION_SCHEMA),
        ),
        ("profile_id", json_pointer_str(profile_certification, "/profile") == Some(NOVASEAL_AGREEMENT_PROFILE)),
        ("canonical_target", json_pointer_str(profile_certification, "/conforms_to") == Some(NOVASEAL_CANONICAL_SCHEMA)),
        ("profile_certification_passed", json_pointer_str(profile_certification, "/status") == Some("passed")),
        ("public_ecosystem_gate_passed", novaseal_gate_status(plugin_report, NOVASEAL_PROFILE_CERTIFICATION_GATE) == Some("passed")),
        ("local_production_prep_ready", json_pointer_bool(plugin_report, "/local_production_prep_ready")),
    ];
    if !v1_readiness.is_null() {
        checks.push(("v1_readiness_local_ready", json_pointer_bool(v1_readiness, "/local_v1_ready")));
    }

    let production_statement_eligible = plugin_report
        .pointer("/production_statement_eligible")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or_else(|| json_pointer_bool(profile_certification, "/production_statement_eligible"));

    if require_production {
        checks.push(("production_ready", json_pointer_bool(plugin_report, "/production_ready")));
        checks.push(("production_statement_eligible", production_statement_eligible));
    }

    let checks_json =
        checks.iter().map(|(name, passed)| ((*name).to_string(), serde_json::Value::Bool(*passed))).collect::<serde_json::Map<_, _>>();
    let failed_checks: Vec<serde_json::Value> =
        checks.iter().filter(|(_, passed)| !*passed).map(|(name, _)| serde_json::Value::String((*name).to_string())).collect();
    let passed = failed_checks.is_empty();
    let external_blockers = plugin_report
        .get("external_blockers")
        .cloned()
        .or_else(|| v1_readiness.get("external_blockers").cloned())
        .or_else(|| profile_certification.get("production_statement_blockers").cloned())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let failed_dimensions = novaseal_failed_dimensions(plugin_report, v1_readiness);
    let planned_missing =
        v1_readiness.pointer("/planned_profile_matrix/missing").cloned().unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let planned_missing_non_empty = planned_missing.as_array().is_some_and(|items| !items.is_empty());
    let external_blockers_non_empty = external_blockers.as_array().is_some_and(|items| !items.is_empty());
    let failed_local_v1_dimension = novaseal_failed_dimension_matches(&failed_dimensions, NOVASEAL_LOCAL_V1_DIMENSIONS);
    let failed_external_or_endpoint_dimension = novaseal_failed_dimension_matches(&failed_dimensions, NOVASEAL_EXTERNAL_V1_DIMENSIONS);
    let certification_level = json_pointer_str(profile_certification, "/certification_level").unwrap_or("unknown");

    let failure_reason = if passed {
        serde_json::Value::Null
    } else if !v1_readiness.is_null() && !json_pointer_bool(v1_readiness, "/local_v1_ready") && planned_missing_non_empty {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires remaining planned profiles and business scenarios",
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if !v1_readiness.is_null() && !json_pointer_bool(v1_readiness, "/local_v1_ready") && failed_local_v1_dimension {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires fresh local evidence reports",
            "remediation": [
                "rerun live devnet reports for core, Agreement, and planned profiles after source or report changes",
                "rerun NovaSeal wallet, operator fixture, service-builder, BTC SPV adapter, external attestation adapter, BIP340 TCB review, and handoff bundle generators"
            ],
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if !v1_readiness.is_null()
        && !json_pointer_bool(v1_readiness, "/local_v1_ready")
        && (external_blockers_non_empty || failed_external_or_endpoint_dimension)
    {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires external production evidence and endpoint acceptance",
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": planned_missing,
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    } else if require_production && json_pointer_bool(plugin_report, "/local_production_prep_ready") {
        serde_json::json!({
            "message": "NovaSeal production certification requires remaining external attestations",
            "external_blockers": external_blockers.clone(),
            "failed_dimensions": failed_dimensions.clone(),
            "failed_checks": failed_checks,
        })
    } else {
        serde_json::json!({
            "message": "NovaSeal profile certification failed deterministic compiler checks",
            "failed_dimensions": failed_dimensions.clone(),
            "external_blockers": external_blockers.clone(),
            "failed_checks": failed_checks,
        })
    };

    Ok(serde_json::json!({
        "schema": NOVASEAL_CERTIFICATION_REPORT_SCHEMA,
        "status": if passed { "passed" } else { "failed" },
        "plugin": {
            "id": NOVASEAL_CERTIFICATION_PLUGIN,
            "kind": "compiler-builtin-rust",
            "implementation": super::novaseal_certification::IMPLEMENTATION_ID,
            "implementation_path": implementation_path.display().to_string(),
            "implementation_hash_algorithm": "ckb_blake2b_256",
            "implementation_hash": implementation_hash,
            "report_generated": report_generated,
        },
        "plugin_report": {
            "path": plugin_report_path.display().to_string(),
            "schema": json_pointer_str(plugin_report, "/schema"),
            "hash_algorithm": "ckb_blake2b_256",
            "hash": plugin_report_hash,
            "status": json_pointer_str(plugin_report, "/status"),
            "production_ready": json_pointer_bool(plugin_report, "/production_ready"),
            "production_gates_passed": json_pointer_bool(plugin_report, "/production_gates_passed"),
            "local_production_prep_ready": json_pointer_bool(plugin_report, "/local_production_prep_ready"),
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "local_v1_ready": json_pointer_bool(v1_readiness, "/local_v1_ready"),
        },
        "profile": NOVASEAL_AGREEMENT_PROFILE,
        "conforms_to": NOVASEAL_CANONICAL_SCHEMA,
        "certification_level": certification_level,
        "production_statement_eligible": production_statement_eligible,
        "failed_dimensions": failed_dimensions,
        "external_blockers": external_blockers,
        "require_production": require_production,
        "repo_root": repo_root.display().to_string(),
        "checks": checks_json,
        "failure_reason": failure_reason,
    }))
}

fn write_or_print_json(output: Option<&PathBuf>, value: &serde_json::Value, json_stdout: bool, label: &str) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON: {}", error)))?;
    if let Some(output_path) = output {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(output_path, json)?;
        if json_stdout {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "ok",
                    "output": output_path.display().to_string(),
                }))
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON: {}", error)))?
            );
        } else {
            println!("{}", label.green());
            println!("  Output: {}", output_path.display());
        }
    } else {
        println!("{}", json);
    }
    Ok(())
}

fn print_or_text_json(json: bool, value: &serde_json::Value, label: &str) -> Result<()> {
    if json {
        print_json(value)
    } else {
        println!("{}: {}", label, value["status"].as_str().unwrap_or("ok"));
        Ok(())
    }
}

fn selected_builder_actions<'a>(metadata: &'a CompileMetadata, action_name: Option<&str>) -> Result<Vec<&'a crate::ActionMetadata>> {
    if let Some(action_name) = action_name {
        let action =
            metadata.actions.iter().find(|action| action.name == action_name).ok_or_else(|| {
                crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", action_name))
            })?;
        return Ok(vec![action]);
    }

    if metadata.actions.is_empty() {
        return Err(crate::error::CompileError::without_span("no actions found in metadata for generated builder"));
    }

    Ok(metadata.actions.iter().collect())
}

fn read_lockfile_path(path: &Path) -> Result<Lockfile> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read lockfile '{}': {}", path.display(), error))
    })?;
    toml::from_str(&content)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to parse lockfile '{}': {}", path.display(), error)))
}

fn read_deployed_manifest_path(path: &Path) -> Result<crate::package::DeployedManifest> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read deployed manifest '{}': {}", path.display(), error))
    })?;
    toml::from_str(&content).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse deployed manifest '{}': {}", path.display(), error))
    })
}

fn verify_builder_lockfile_identity(
    lockfile_path: &Path,
    metadata: &CompileMetadata,
    metadata_hash: &str,
) -> Result<serde_json::Value> {
    let lockfile = read_lockfile_path(lockfile_path)?;
    let expected_build = locked_build_info_from_metadata(metadata)?;
    let mut violations = Vec::new();

    let locked_compiler_source_hash = lockfile.package.compiler_source_hash.as_ref().or(lockfile.package.source_hash.as_ref());
    let locked_source_label = if lockfile.package.compiler_source_hash.is_some() { "compiler_source_hash" } else { "source_hash" };
    match (locked_compiler_source_hash, &metadata.source_hash) {
        (Some(locked), Some(actual)) if locked == actual => {}
        (Some(locked), Some(actual)) => violations
            .push(format!("{} mismatch: Cell.lock has '{}', metadata source_hash is '{}'", locked_source_label, locked, actual)),
        (None, _) => violations.push("Cell.lock [package] has no compiler_source_hash or source_hash".to_string()),
        (_, None) => violations.push("metadata has no source_hash".to_string()),
    }

    match &lockfile.package_build {
        Some(build) => {
            push_missing_locked_build_identity("Cell.lock [package.build]", build, &mut violations);
            compare_builder_identity_field(
                "compiler_version",
                &build.compiler_version,
                &expected_build.compiler_version,
                &mut violations,
            );
            compare_builder_identity_field("target_profile", &build.target_profile, &expected_build.target_profile, &mut violations);
            compare_builder_identity_field("artifact_hash", &build.artifact_hash, &expected_build.artifact_hash, &mut violations);
            compare_builder_identity_field("metadata_hash", &build.metadata_hash, &Some(metadata_hash.to_string()), &mut violations);
            compare_builder_identity_field("schema_hash", &build.schema_hash, &expected_build.schema_hash, &mut violations);
            compare_builder_identity_field(
                "cell_data_codec_manifest_hash",
                &build.cell_data_codec_manifest_hash,
                &expected_build.cell_data_codec_manifest_hash,
                &mut violations,
            );
            compare_builder_identity_field("abi_hash", &build.abi_hash, &expected_build.abi_hash, &mut violations);
            compare_builder_identity_field(
                "constraints_hash",
                &build.constraints_hash,
                &expected_build.constraints_hash,
                &mut violations,
            );
        }
        None => violations.push("Cell.lock has no [package.build]".to_string()),
    }

    if !violations.is_empty() {
        return Err(crate::error::CompileError::without_span(format!(
            "generated builder identity verification failed: {}",
            violations.join("; ")
        )));
    }

    Ok(serde_json::json!({
        "schema": "cellscript-builder-locked-identity-v0.20",
        "package": lockfile.package,
        "build": lockfile.package_build,
        "verified_fields": [
            locked_source_label,
            "compiler_version",
            "target_profile",
            "artifact_hash",
            "metadata_hash",
            "schema_hash",
            "cell_data_codec_manifest_hash",
            "abi_hash",
            "constraints_hash"
        ]
    }))
}

fn verify_builder_deployment_identity(
    lockfile_path: &Path,
    deployed_path: &Path,
    metadata: &CompileMetadata,
    metadata_hash: &str,
    network_filter: Option<&str>,
) -> Result<serde_json::Value> {
    let lockfile = read_lockfile_path(lockfile_path)?;
    let deployed = read_deployed_manifest_path(deployed_path)?;
    let expected_build = locked_build_info_from_metadata(metadata)?;
    let mut violations = Vec::new();

    match (&lockfile.package.source_hash, &deployed.package.source_hash) {
        (Some(locked), Some(deployed_hash)) if locked == deployed_hash => {}
        (Some(locked), Some(deployed_hash)) => {
            violations.push(format!("source_hash mismatch: Cell.lock has '{}', Deployed.toml has '{}'", locked, deployed_hash))
        }
        (None, _) => violations.push("Cell.lock [package] has no source_hash".to_string()),
        (_, None) => violations.push("Deployed.toml [package] has no source_hash".to_string()),
    }

    let locked_compiler_source_hash = lockfile.package.compiler_source_hash.as_ref().or(lockfile.package.source_hash.as_ref());
    match (locked_compiler_source_hash, &metadata.source_hash) {
        (Some(locked), Some(actual)) if locked == actual => {}
        (Some(locked), Some(actual)) => {
            violations.push(format!("compiler_source_hash mismatch: Cell.lock has '{}', metadata source_hash is '{}'", locked, actual))
        }
        (None, _) => violations.push("Cell.lock [package] has no compiler_source_hash or source_hash".to_string()),
        (_, None) => violations.push("metadata has no source_hash".to_string()),
    }

    match &deployed.build {
        Some(build) => {
            push_missing_deployed_build_identity("Deployed.toml [build]", build, &mut violations);
            compare_builder_deployed_field(
                "compiler_version",
                &build.compiler_version,
                &expected_build.compiler_version,
                &mut violations,
            );
            compare_builder_deployed_field("artifact_hash", &build.artifact_hash, &expected_build.artifact_hash, &mut violations);
            compare_builder_deployed_field("metadata_hash", &build.metadata_hash, &Some(metadata_hash.to_string()), &mut violations);
            compare_builder_deployed_field("schema_hash", &build.schema_hash, &expected_build.schema_hash, &mut violations);
            compare_builder_deployed_field(
                "cell_data_codec_manifest_hash",
                &build.cell_data_codec_manifest_hash,
                &expected_build.cell_data_codec_manifest_hash,
                &mut violations,
            );
            compare_builder_deployed_field("abi_hash", &build.abi_hash, &expected_build.abi_hash, &mut violations);
            compare_builder_deployed_field(
                "constraints_hash",
                &build.constraints_hash,
                &expected_build.constraints_hash,
                &mut violations,
            );
        }
        None => violations.push("Deployed.toml has no [build]".to_string()),
    }

    let mut verified_deployments = Vec::new();
    for deployment in &deployed.deployments {
        if network_filter.is_some_and(|network| network != deployment.network) {
            continue;
        }
        push_deployment_status_violation(deployment, &mut violations);
        compare_builder_deployment_record_field(
            "artifact_hash",
            &deployment.artifact_hash,
            &expected_build.artifact_hash,
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "metadata_hash",
            &deployment.metadata_hash,
            &Some(metadata_hash.to_string()),
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "schema_hash",
            &deployment.schema_hash,
            &expected_build.schema_hash,
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "cell_data_codec_manifest_hash",
            &deployment.cell_data_codec_manifest_hash,
            &expected_build.cell_data_codec_manifest_hash,
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "abi_hash",
            &deployment.abi_hash,
            &expected_build.abi_hash,
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "constraints_hash",
            &deployment.constraints_hash,
            &expected_build.constraints_hash,
            &deployment.network,
            &mut violations,
        );
        compare_builder_deployment_record_field(
            "compiler_version",
            &deployment.compiler_version,
            &expected_build.compiler_version,
            &deployment.network,
            &mut violations,
        );

        match lockfile.deployment.get(&deployment.network) {
            Some(deployment_ref) => {
                compare_required_deployment_ref_field(
                    "code_hash",
                    deployment_ref.code_hash.as_deref(),
                    &deployment.code_hash,
                    &deployment.network,
                    &mut violations,
                );
                compare_required_deployment_ref_field(
                    "out_point",
                    deployment_ref.out_point.as_deref(),
                    &deployment.out_point,
                    &deployment.network,
                    &mut violations,
                );
                compare_required_deployment_ref_field(
                    "data_hash",
                    deployment_ref.data_hash.as_deref(),
                    &deployment.data_hash,
                    &deployment.network,
                    &mut violations,
                );
                if !deployment_ref.record.is_empty() {
                    let chain_record = format!("{}:{}", deployment.chain_id, deployment.out_point);
                    let network_record = format!("{}:{}", deployment.network, deployment.out_point);
                    if deployment_ref.record != deployment.out_point
                        && deployment_ref.record != chain_record
                        && deployment_ref.record != network_record
                    {
                        violations.push(format!(
                            "deployment record mismatch for network '{}': Cell.lock has '{}', Deployed.toml out_point is '{}'",
                            deployment.network, deployment_ref.record, deployment.out_point
                        ));
                    }
                } else {
                    violations.push(format!("deployment ref for network '{}' has empty record", deployment.network));
                }
            }
            None => violations.push(format!("deployment for network '{}' is missing from Cell.lock", deployment.network)),
        }

        verified_deployments.push(deployment);
    }

    if verified_deployments.is_empty() {
        violations.push(match network_filter {
            Some(network) => format!("no deployment record found for requested builder network '{}'", network),
            None => "no deployment records found for generated builder".to_string(),
        });
    }

    if !violations.is_empty() {
        return Err(crate::error::CompileError::without_span(format!(
            "generated builder deployment identity verification failed: {}",
            violations.join("; ")
        )));
    }

    Ok(serde_json::json!({
        "schema": "cellscript-builder-deployment-identity-v0.20",
        "package": deployed.package.clone(),
        "build": deployed.build.clone(),
        "network": network_filter,
        "deployments": verified_deployments,
        "verified_fields": [
            "source_hash",
            "compiler_source_hash",
            "compiler_version",
            "artifact_hash",
            "metadata_hash",
            "schema_hash",
            "cell_data_codec_manifest_hash",
            "abi_hash",
            "constraints_hash",
            "code_hash",
            "out_point",
            "data_hash",
            "deployment_status"
        ]
    }))
}

fn compare_builder_identity_field(
    field: &str,
    locked_value: &Option<String>,
    metadata_value: &Option<String>,
    violations: &mut Vec<String>,
) {
    match (locked_value, metadata_value) {
        (Some(locked), Some(actual)) if locked == actual => {}
        (Some(locked), Some(actual)) => {
            violations.push(format!("{} mismatch: Cell.lock has '{}', metadata has '{}'", field, locked, actual))
        }
        (None, _) => {}
        (_, None) => violations.push(format!("metadata has no {}", field)),
    }
}

fn deployment_status_violation(deployment: &crate::package::DeploymentRecord) -> Option<String> {
    match deployment.status.as_ref() {
        Some(crate::package::DeploymentStatus::Active) => None,
        Some(status) => Some(format!("deployment for network '{}' is not active: {:?}", deployment.network, status)),
        None => Some(format!("deployment for network '{}' has no status; expected active", deployment.network)),
    }
}

fn push_deployment_status_violation(deployment: &crate::package::DeploymentRecord, violations: &mut Vec<String>) {
    if let Some(violation) = deployment_status_violation(deployment) {
        violations.push(violation);
    }
}

fn verify_registry_trust_metadata(
    deployed: &crate::package::DeployedManifest,
    require_publisher_signature: bool,
    require_audit_report: bool,
    violations: &mut Vec<String>,
) -> serde_json::Value {
    let mut evidence = Vec::new();
    if (require_publisher_signature || require_audit_report) && deployed.deployments.is_empty() {
        violations.push("trust metadata policy requires at least one deployment record".to_string());
    }
    for deployment in &deployed.deployments {
        let publisher_signature_present = deployment.publisher_signature.as_deref().is_some_and(|value| !value.trim().is_empty());
        let audit_report_hash_present = deployment.audit_report_hash.as_deref().is_some_and(|value| !value.trim().is_empty());
        let mut deployment_violations = Vec::new();
        if require_publisher_signature && !publisher_signature_present {
            deployment_violations.push(format!(
                "deployment for network '{}' has no publisher_signature required by trust metadata policy",
                deployment.network
            ));
        }
        if require_audit_report && !audit_report_hash_present {
            deployment_violations.push(format!(
                "deployment for network '{}' has no audit_report_hash required by trust metadata policy",
                deployment.network
            ));
        }
        for violation in &deployment_violations {
            if !violations.contains(violation) {
                violations.push(violation.clone());
            }
        }
        evidence.push(serde_json::json!({
            "network": deployment.network,
            "out_point": deployment.out_point,
            "status": if deployment_violations.is_empty() { "policy-satisfied" } else { "failed" },
            "publisher_signature_present": publisher_signature_present,
            "publisher_signature_status": if publisher_signature_present {
                "present-unverified"
            } else if require_publisher_signature {
                "missing"
            } else {
                "not-required"
            },
            "audit_report_hash_present": audit_report_hash_present,
            "audit_report_hash_status": if audit_report_hash_present {
                "present"
            } else if require_audit_report {
                "missing"
            } else {
                "not-required"
            },
            "violations": deployment_violations,
        }));
    }
    serde_json::json!({
        "enabled": require_publisher_signature || require_audit_report,
        "checked": deployed.deployments.len(),
        "verification_boundary": "metadata-presence-only",
        "publisher_signature_verification": "not-implemented",
        "policy": {
            "require_publisher_signature": require_publisher_signature,
            "require_audit_report": require_audit_report,
        },
        "evidence": evidence,
    })
}

fn compare_builder_deployed_field(
    field: &str,
    deployed_value: &Option<String>,
    metadata_value: &Option<String>,
    violations: &mut Vec<String>,
) {
    match (deployed_value, metadata_value) {
        (Some(deployed), Some(actual)) if deployed == actual => {}
        (Some(deployed), Some(actual)) => {
            violations.push(format!("{} mismatch: Deployed.toml has '{}', metadata has '{}'", field, deployed, actual))
        }
        (None, _) => {}
        (_, None) => violations.push(format!("metadata has no {}", field)),
    }
}

fn compare_builder_deployment_record_field(
    field: &str,
    deployed_value: &Option<String>,
    metadata_value: &Option<String>,
    network: &str,
    violations: &mut Vec<String>,
) {
    match (deployed_value, metadata_value) {
        (Some(deployed), Some(actual)) if deployed == actual => {}
        (Some(deployed), Some(actual)) => violations.push(format!(
            "{} mismatch for network '{}': Deployed.toml has '{}', metadata has '{}'",
            field, network, deployed, actual
        )),
        (None, _) => violations.push(format!("deployment record for network '{}' has no {}", network, field)),
        (_, None) => violations.push(format!("metadata has no {}", field)),
    }
}

fn compare_required_deployment_ref_field(
    field: &str,
    locked_value: Option<&str>,
    deployed_value: &str,
    network: &str,
    violations: &mut Vec<String>,
) {
    match locked_value {
        Some(locked) if locked == deployed_value => {}
        Some(locked) => violations.push(format!(
            "{} mismatch for network '{}': Cell.lock has '{}', Deployed.toml has '{}'",
            field, network, locked, deployed_value
        )),
        None => violations.push(format!("deployment ref for network '{}' has no {}", network, field)),
    }
}

fn write_typescript_builder_package(
    output_dir: &Path,
    package_name: &str,
    metadata: &CompileMetadata,
    metadata_hash: &str,
    actions: &[&crate::ActionMetadata],
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
    lockfile_path: Option<&Path>,
    deployed_path: Option<&Path>,
) -> Result<serde_json::Value> {
    let src_dir = output_dir.join("src");
    let test_dir = output_dir.join("test");
    std::fs::create_dir_all(&src_dir)?;
    std::fs::create_dir_all(&test_dir)?;

    let manifest = typescript_builder_manifest(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity);

    let package_json_path = output_dir.join("package.json");
    let tsconfig_path = output_dir.join("tsconfig.json");
    let manifest_path = output_dir.join("cellscript-builder-manifest.json");
    let metadata_path = src_dir.join("metadata.json");
    let index_path = src_dir.join("index.ts");
    let test_path = test_dir.join("builder.test.mjs");

    std::fs::write(&package_json_path, json_bytes_pretty("package.json", &typescript_package_json(package_name))?)?;
    std::fs::write(&tsconfig_path, json_bytes_pretty("tsconfig.json", &typescript_tsconfig_json())?)?;
    std::fs::write(&manifest_path, json_bytes_pretty("builder manifest", &manifest)?)?;
    std::fs::write(&metadata_path, json_bytes_pretty("metadata", metadata)?)?;
    std::fs::write(
        &index_path,
        typescript_builder_index(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity)?,
    )?;
    std::fs::write(&test_path, typescript_builder_test(actions)?)?;

    Ok(serde_json::json!({
        "status": "ok",
        "schema": "cellscript-generated-builder-summary-v0.20",
        "target": "typescript",
        "output_dir": output_dir.display().to_string(),
        "package_name": package_name,
        "metadata_hash": metadata_hash,
        "artifact_hash": metadata.artifact_hash,
        "cell_data_codec_abi": metadata.cell_data_codec_manifest.abi,
        "raw_cell_data_required": metadata.cell_data_codec_manifest.raw_bytes_required,
        "lockfile_verified": locked_identity.is_some(),
        "deployment_verified": deployment_identity.is_some(),
        "lockfile": lockfile_path.map(|path| path.display().to_string()),
        "deployed": deployed_path.map(|path| path.display().to_string()),
        "action_count": actions.len(),
        "actions": actions.iter().map(|action| action.name.as_str()).collect::<Vec<_>>(),
        "files": [
            package_json_path.display().to_string(),
            tsconfig_path.display().to_string(),
            manifest_path.display().to_string(),
            metadata_path.display().to_string(),
            index_path.display().to_string(),
            test_path.display().to_string()
        ],
        "non_claims": [
            "generated package does not prove live-cell availability",
            "generated package does not sign or submit transactions by itself",
            "generated package does not materialize raw cell-data codecs by itself",
            "generated package requires a runtime adapter for CCC or ckb-sdk-rust"
        ]
    }))
}

fn runtime_error_catalog_json() -> Vec<serde_json::Value> {
    ALL_RUNTIME_ERRORS
        .iter()
        .copied()
        .map(|error| {
            let info = runtime_error_info(error);
            serde_json::json!({
                "code": info.code,
                "name": info.name,
                "description": info.description,
                "hint": info.hint,
            })
        })
        .collect()
}

fn builder_action_error_contexts_json(actions: &[&crate::ActionMetadata]) -> Vec<serde_json::Value> {
    actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "action": action.name,
                "fields": action
                    .params
                    .iter()
                    .map(|param| {
                        serde_json::json!({
                            "name": param.name,
                            "type": param.ty,
                            "source": param.source,
                            "is_mut": param.is_mut,
                            "is_ref": param.is_ref,
                            "witness_data_source": param.witness_data_source,
                            "lock_args_data_source": param.lock_args_data_source,
                            "protected_spend_surface": param.protected_spend_surface,
                            "cell_bound_abi": param.cell_bound_abi,
                            "schema_pointer_abi": param.schema_pointer_abi,
                            "schema_length_abi": param.schema_length_abi,
                            "fixed_byte_len": param.fixed_byte_len,
                        })
                    })
                    .collect::<Vec<_>>(),
                "entry_witness_required": !action.params.is_empty(),
                "runtimeInputRequirements": action.transaction_runtime_input_requirements,
                "verifierObligations": action.verifier_obligations,
                "failClosedRuntimeFeatures": action.fail_closed_runtime_features,
            })
        })
        .collect()
}

fn typescript_builder_manifest(
    package_name: &str,
    metadata: &CompileMetadata,
    actions: &[&crate::ActionMetadata],
    metadata_hash: &str,
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
) -> serde_json::Value {
    serde_json::json!({
        "schema": "cellscript-generated-action-builder-v0.20",
        "target": "typescript",
        "package_name": package_name,
        "module": metadata.module,
        "compiler_version": metadata.compiler_version,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "metadata_hash": metadata_hash,
        "artifact_hash": metadata.artifact_hash,
        "source_hash": metadata.source_hash,
        "target_profile": metadata.target_profile.name,
        "molecule_schema_manifest": metadata.molecule_schema_manifest,
        "cell_data_codec_manifest": metadata.cell_data_codec_manifest,
        "locked_identity": locked_identity,
        "deployment_identity": deployment_identity,
        "actions": actions
            .iter()
            .map(|action| {
                serde_json::json!({
                    "name": action.name,
                    "params": action.params,
                    "created_outputs": action.create_set.len(),
                    "mutated_outputs": action.mutate_set.len(),
                    "runtime_input_requirements": action.transaction_runtime_input_requirements.len(),
                    "entry_witness_required": !action.params.is_empty(),
                })
            })
            .collect::<Vec<_>>(),
        "runtime_error_catalog": runtime_error_catalog_json(),
        "runtime_contract": {
            "requires_live_cell_resolution": true,
            "requires_deployment_resolution": true,
            "requires_capacity_and_fee_policy": true,
            "requires_witness_materialization": true,
            "requires_cell_data_codec_materialization": true,
            "requires_external_cell_data_codec_adapter": metadata.cell_data_codec_manifest.raw_bytes_required,
            "cell_data_codec_abi": metadata.cell_data_codec_manifest.abi,
            "requires_dry_run_before_submit": true,
            "must_not_infer_protocol_semantics_from_action_name": true,
        }
    })
}

fn typescript_package_json(package_name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": package_name,
        "version": "0.0.0-cellscript-generated",
        "private": true,
        "type": "module",
        "main": "dist/index.js",
        "types": "dist/index.d.ts",
        "scripts": {
            "build": "tsc -p tsconfig.json",
            "test": "npm run build && node --test test/*.test.mjs"
        },
        "devDependencies": {
            "typescript": "^5.0.0"
        }
    })
}

fn typescript_tsconfig_json() -> serde_json::Value {
    serde_json::json!({
        "compilerOptions": {
            "target": "ES2022",
            "module": "NodeNext",
            "moduleResolution": "NodeNext",
            "declaration": true,
            "outDir": "dist",
            "rootDir": "src",
            "strict": true,
            "resolveJsonModule": true,
            "esModuleInterop": true,
            "skipLibCheck": true
        },
        "include": ["src/**/*.ts", "src/**/*.json"]
    })
}

fn typescript_builder_index(
    package_name: &str,
    metadata: &CompileMetadata,
    actions: &[&crate::ActionMetadata],
    metadata_hash: &str,
    locked_identity: Option<&serde_json::Value>,
    deployment_identity: Option<&serde_json::Value>,
) -> Result<String> {
    let action_specs = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "params": action.params,
                "createSet": action.create_set,
                "mutateSet": action.mutate_set,
                "readRefs": action.read_refs,
                "verifierObligations": action.verifier_obligations,
                "runtimeInputRequirements": action.transaction_runtime_input_requirements,
                "failClosedRuntimeFeatures": action.fail_closed_runtime_features,
            })
        })
        .collect::<Vec<_>>();
    let action_specs_json = json_string_pretty("action specs", &action_specs)?;
    let action_error_contexts_json = json_string_pretty("action error contexts", &builder_action_error_contexts_json(actions))?;
    let runtime_error_catalog_json = json_string_pretty("runtime error catalog", &runtime_error_catalog_json())?;
    let manifest_json = json_string_pretty(
        "builder manifest",
        &typescript_builder_manifest(package_name, metadata, actions, metadata_hash, locked_identity, deployment_identity),
    )?;
    let metadata_json = json_string_pretty("metadata", metadata)?;

    let mut ts = String::new();
    ts.push_str("export const CELLSCRIPT_BUILDER_SCHEMA = \"cellscript-generated-action-builder-v0.20\" as const;\n");
    ts.push_str(&format!("export const builderManifest = {manifest_json} as const;\n"));
    ts.push_str(&format!("export const metadata = {metadata_json} as const;\n"));
    ts.push_str("export const cellDataCodecManifest = metadata.cell_data_codec_manifest;\n");
    ts.push_str(&format!("export const actionSpecs = {action_specs_json} as const;\n\n"));
    ts.push_str(&format!("export const actionErrorContexts = {action_error_contexts_json} as const;\n"));
    ts.push_str(&format!("export const runtimeErrorCatalog = {runtime_error_catalog_json} as const;\n\n"));
    ts.push_str(
        "export type HexString = `0x${string}`;\n\
         export type CellScriptValue = string | number | bigint | boolean | Uint8Array | Record<string, unknown> | null;\n\
         export type CellScriptParams = object;\n\n\
         export interface CellScriptLockfilePackage {\n\
           name?: string;\n\
           version?: string;\n\
           namespace?: string | null;\n\
           source_hash?: string | null;\n\
           compiler_source_hash?: string | null;\n\
         }\n\n\
         export interface CellScriptLockfileBuild {\n\
           compiler_version?: string | null;\n\
           target_profile?: string | null;\n\
           artifact_hash?: string | null;\n\
           metadata_hash?: string | null;\n\
           schema_hash?: string | null;\n\
           cell_data_codec_manifest_hash?: string | null;\n\
           abi_hash?: string | null;\n\
           constraints_hash?: string | null;\n\
         }\n\n\
         export interface CellScriptLockfileDeployment {\n\
           record?: string | null;\n\
           record_hash?: string | null;\n\
           code_hash?: string | null;\n\
           out_point?: string | null;\n\
           data_hash?: string | null;\n\
         }\n\n\
         export interface CellScriptLockfile {\n\
           package?: CellScriptLockfilePackage;\n\
           package_build?: CellScriptLockfileBuild | null;\n\
           deployment?: Record<string, CellScriptLockfileDeployment | null | undefined>;\n\
         }\n\n\
         export interface CellScriptDeploymentRecord {\n\
           network: string;\n\
           chain_id: string;\n\
           tx_hash: string;\n\
           output_index: number;\n\
           code_hash: string;\n\
           hash_type: string;\n\
           dep_type: string;\n\
           data_hash: string;\n\
           out_point: string;\n\
           artifact_hash?: string | null;\n\
           metadata_hash?: string | null;\n\
           schema_hash?: string | null;\n\
           cell_data_codec_manifest_hash?: string | null;\n\
           abi_hash?: string | null;\n\
           constraints_hash?: string | null;\n\
           compiler_version?: string | null;\n\
           type_id?: string | null;\n\
           status?: string | null;\n\
           audit_report_hash?: string | null;\n\
           publisher_signature?: string | null;\n\
         }\n\n\
         export interface CellScriptLiveDeploymentEvidence {\n\
           network?: string;\n\
           out_point?: string;\n\
           rpc_status?: string;\n\
           status?: string;\n\
           deployment_status?: string | null;\n\
           expected_data_hash?: string;\n\
           rpc_data_hash?: string | null;\n\
           expected_code_hash?: string;\n\
           rpc_code_hash?: string | null;\n\
           violations?: readonly string[];\n\
         }\n\n\
         export interface CellScriptTrustPolicy {\n\
           requirePublisherSignature?: boolean;\n\
           requireAuditReportHash?: boolean;\n\
         }\n\n\
         export interface BuildOptions {\n\
           lockfile?: CellScriptLockfile;\n\
           deployment?: CellScriptDeploymentRecord;\n\
           liveDeploymentEvidence?: CellScriptLiveDeploymentEvidence;\n\
           trustPolicy?: CellScriptTrustPolicy;\n\
           deploymentRef?: string;\n\
           dryRun?: boolean;\n\
           submit?: boolean;\n\
           feeRate?: bigint | number | string;\n\
           changeLock?: unknown;\n\
         }\n\n\
         export interface ActionBuilderPlan<P extends CellScriptParams = CellScriptParams> {\n\
           schema: typeof CELLSCRIPT_BUILDER_SCHEMA;\n\
           state: \"GeneratedActionPlan\";\n\
           status: \"requires-runtime-resolution\";\n\
           action: string;\n\
           params: P;\n\
           options: BuildOptions;\n\
           metadataHash: string;\n\
           artifactHash: string | null;\n\
           targetProfile: string;\n\
           canSubmit: false;\n\
           requiresLiveCellResolution: true;\n\
           requiresDeploymentResolution: true;\n\
           cellDataCodecManifest: typeof cellDataCodecManifest;\n\
           notProvenByGeneratedBuilder: readonly string[];\n\
         }\n\n\
         export type ActionBuilderMode = \"build\" | \"dry-run\" | \"submit\";\n\n\
         export interface ActionBuilderResult<P extends CellScriptParams = CellScriptParams> {\n\
           schema: typeof CELLSCRIPT_BUILDER_SCHEMA;\n\
           state: \"ActionBuilderResult\";\n\
           status: \"built-by-runtime\" | \"dry-run-by-runtime\" | \"submitted-by-runtime\";\n\
           mode: ActionBuilderMode;\n\
           plan: ActionBuilderPlan<P>;\n\
           liveCellResolution: LiveCellResolutionResult;\n\
           transaction: unknown;\n\
           dryRunResult?: unknown;\n\
           submitResult?: unknown;\n\
           submittedTxHash?: string | null;\n\
           canSubmit: false;\n\
           notProvenByGeneratedBuilder: readonly string[];\n\
         }\n\n\
         export interface LiveCellResolutionRequest<P extends CellScriptParams = CellScriptParams> {\n\
           plan: ActionBuilderPlan<P>;\n\
           options: BuildOptions;\n\
         }\n\n\
         export interface LiveCellResolutionResult {\n\
           inputs?: readonly unknown[];\n\
           referenceInputs?: readonly unknown[];\n\
           cellDeps?: readonly unknown[];\n\
           headerDeps?: readonly unknown[];\n\
           deploymentRef?: unknown;\n\
           lineage?: readonly unknown[];\n\
         }\n\n\
         export interface CellScriptBuilderRuntime {\n\
           resolveLiveCells<P extends CellScriptParams>(request: LiveCellResolutionRequest<P>): Promise<LiveCellResolutionResult>;\n\
           buildTransaction<P extends CellScriptParams>(plan: ActionBuilderPlan<P> & { liveCellResolution: LiveCellResolutionResult }): Promise<unknown>;\n\
           dryRun?(transaction: unknown): Promise<unknown>;\n\
           submit?(transaction: unknown): Promise<unknown>;\n\
         }\n\n\
         export interface CellScriptRuntimeErrorInfo {\n\
           code: number;\n\
           name: string;\n\
           description: string;\n\
           hint: string;\n\
         }\n\n\
         export interface CellScriptActionFieldContext {\n\
           name: string;\n\
           type: string;\n\
           source: string;\n\
           is_mut: boolean;\n\
           is_ref: boolean;\n\
           witness_data_source: boolean;\n\
           lock_args_data_source: boolean;\n\
           protected_spend_surface: boolean;\n\
           cell_bound_abi: boolean;\n\
           schema_pointer_abi: boolean;\n\
           schema_length_abi: boolean;\n\
           fixed_byte_len?: number | null;\n\
         }\n\n\
         export interface CellScriptActionErrorContext {\n\
           action: string;\n\
           fields: readonly CellScriptActionFieldContext[];\n\
           entry_witness_required: boolean;\n\
           runtimeInputRequirements: readonly unknown[];\n\
           verifierObligations: readonly unknown[];\n\
           failClosedRuntimeFeatures: readonly string[];\n\
         }\n\n\
         export interface CellScriptRuntimeErrorExplanation extends CellScriptRuntimeErrorInfo {\n\
           action?: string;\n\
           actionFields: readonly CellScriptActionFieldContext[];\n\
           entryWitnessRequired: boolean;\n\
           runtimeInputRequirements: readonly unknown[];\n\
           verifierObligations: readonly unknown[];\n\
           failClosedRuntimeFeatures: readonly string[];\n\
         }\n\n",
    );
    ts.push_str(&format!(
        "const GENERATED_METADATA_HASH = {};\n\
         const GENERATED_ARTIFACT_HASH: string | null = {};\n\
         const GENERATED_SOURCE_HASH: string | null = {};\n\
         const GENERATED_COMPILER_VERSION = {};\n\
         const GENERATED_TARGET_PROFILE = {};\n\
         const GENERATED_SCHEMA_HASH = {};\n\
         const GENERATED_CELL_DATA_CODEC_MANIFEST_HASH = {};\n\
         const GENERATED_ABI_HASH = {};\n\
         const GENERATED_CONSTRAINTS_HASH = {};\n\
         const BUILDER_MANIFEST_RUNTIME = builderManifest as unknown as {{\n\
           deployment_identity?: {{ deployments?: readonly CellScriptDeploymentRecord[] }} | null;\n\
         }};\n\n",
        typescript_string_literal(metadata_hash),
        metadata.artifact_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        metadata.source_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        typescript_string_literal(&metadata.compiler_version),
        typescript_string_literal(&metadata.target_profile.name),
        typescript_string_literal(&metadata.molecule_schema_manifest.manifest_hash),
        typescript_string_literal(&metadata.cell_data_codec_manifest.manifest_hash),
        typescript_string_literal(&metadata_abi_hash(metadata)?),
        typescript_string_literal(&hash_json_value("constraints", &metadata.constraints)?),
    ));
    ts.push_str(
        "export function runtimeErrorInfoByCode(code: number | string | bigint): CellScriptRuntimeErrorInfo | null {\n\
           const parsed = runtimeErrorCodeFrom(code);\n\
           if (parsed === null) {\n\
             return null;\n\
           }\n\
           const item = runtimeErrorCatalog.find((error) => error.code === parsed);\n\
           return item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
         }\n\n\
         export function runtimeErrorInfoByName(name: string): CellScriptRuntimeErrorInfo | null {\n\
           const normalized = name.trim().toLowerCase();\n\
           const item = runtimeErrorCatalog.find((error) => error.name === normalized);\n\
           return item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
         }\n\n\
         export function runtimeErrorContextForAction(action: string): CellScriptActionErrorContext | null {\n\
           const context = actionErrorContexts.find((item) => item.action === action);\n\
           if (!context) {\n\
             return null;\n\
           }\n\
           return {\n\
             action: context.action,\n\
             fields: context.fields.map((field) => ({ ...field })),\n\
             entry_witness_required: context.entry_witness_required,\n\
             runtimeInputRequirements: [...context.runtimeInputRequirements],\n\
             verifierObligations: [...context.verifierObligations],\n\
             failClosedRuntimeFeatures: [...context.failClosedRuntimeFeatures],\n\
           };\n\
         }\n\n\
         export function explainCellScriptRuntimeError(error: unknown, action?: string): CellScriptRuntimeErrorExplanation | null {\n\
           const code = runtimeErrorCodeFrom(error);\n\
           const name = runtimeErrorNameFrom(error);\n\
           const message = runtimeErrorMessageFrom(error);\n\
           let info = code === null ? null : runtimeErrorInfoByCode(code);\n\
           if (!info && name) {\n\
             info = runtimeErrorInfoByName(name);\n\
           }\n\
           if (!info && message) {\n\
             const normalizedMessage = message.toLowerCase();\n\
             const item = runtimeErrorCatalog.find((known) => normalizedMessage.includes(known.name));\n\
             info = item ? { code: item.code, name: item.name, description: item.description, hint: item.hint } : null;\n\
             if (!info && (normalizedMessage.includes(\"entry witness\") || normalizedMessage.includes(\"entry-witness\"))) {\n\
               info = runtimeErrorInfoByName(\"entry-witness-abi-invalid\");\n\
             }\n\
             if (!info && normalizedMessage.includes(\"collection\")) {\n\
               info = runtimeErrorInfoByName(\"collection-runtime-unsupported\");\n\
             }\n\
           }\n\
           if (!info) {\n\
             return null;\n\
           }\n\
           const context = action ? runtimeErrorContextForAction(action) : null;\n\
           return {\n\
             ...info,\n\
             action: context?.action ?? action,\n\
             actionFields: context?.fields ?? [],\n\
             entryWitnessRequired: context?.entry_witness_required ?? false,\n\
             runtimeInputRequirements: context?.runtimeInputRequirements ?? [],\n\
             verifierObligations: context?.verifierObligations ?? [],\n\
             failClosedRuntimeFeatures: context?.failClosedRuntimeFeatures ?? [],\n\
           };\n\
         }\n\n\
         function runtimeErrorCodeFrom(error: unknown): number | null {\n\
           if (typeof error === \"number\" && Number.isFinite(error)) {\n\
             return Math.trunc(error);\n\
           }\n\
           if (typeof error === \"bigint\") {\n\
             const value = Number(error);\n\
             return Number.isSafeInteger(value) ? value : null;\n\
           }\n\
           if (typeof error === \"string\") {\n\
             const trimmed = error.trim();\n\
             return /^-?\\d+$/.test(trimmed) ? Number(trimmed) : null;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"code\", \"exitCode\", \"errorCode\", \"error_code\", \"ecode\"] as const) {\n\
               const parsed = runtimeErrorCodeFrom(record[key]);\n\
               if (parsed !== null) {\n\
                 return parsed;\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         function runtimeErrorNameFrom(error: unknown): string | null {\n\
           if (typeof error === \"string\") {\n\
             const trimmed = error.trim().toLowerCase();\n\
             return runtimeErrorCatalog.some((known) => known.name === trimmed) ? trimmed : null;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"name\", \"error\", \"errorName\", \"runtime_error\"] as const) {\n\
               const value = record[key];\n\
               if (typeof value === \"string\") {\n\
                 const match = runtimeErrorNameFrom(value);\n\
                 if (match) {\n\
                   return match;\n\
                 }\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         function runtimeErrorMessageFrom(error: unknown): string | null {\n\
           if (typeof error === \"string\") {\n\
             return error;\n\
           }\n\
           if (typeof error === \"object\" && error !== null) {\n\
             const record = error as Record<string, unknown>;\n\
             for (const key of [\"message\", \"stderr\", \"reason\"] as const) {\n\
               const value = record[key];\n\
               if (typeof value === \"string\") {\n\
                 return value;\n\
               }\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n\
         export function validateCellScriptLockfile(lockfile: CellScriptLockfile): string[] {\n\
           const violations: string[] = [];\n\
           const pkg = lockfile.package;\n\
           if (!pkg) {\n\
             violations.push(\"Cell.lock has no [package]\");\n\
           } else {\n\
             compareRequiredIdentity(\"compiler_source_hash\", pkg.compiler_source_hash ?? pkg.source_hash, GENERATED_SOURCE_HASH, violations);\n\
           }\n\
           const build = lockfile.package_build;\n\
           if (!build) {\n\
             violations.push(\"Cell.lock has no [package.build]\");\n\
           } else {\n\
             compareRequiredIdentity(\"compiler_version\", build.compiler_version, GENERATED_COMPILER_VERSION, violations);\n\
             compareRequiredIdentity(\"target_profile\", build.target_profile, GENERATED_TARGET_PROFILE, violations);\n\
             compareRequiredIdentity(\"artifact_hash\", build.artifact_hash, GENERATED_ARTIFACT_HASH, violations);\n\
             compareRequiredIdentity(\"metadata_hash\", build.metadata_hash, GENERATED_METADATA_HASH, violations);\n\
             compareRequiredIdentity(\"schema_hash\", build.schema_hash, GENERATED_SCHEMA_HASH, violations);\n\
             compareRequiredIdentity(\"cell_data_codec_manifest_hash\", build.cell_data_codec_manifest_hash, GENERATED_CELL_DATA_CODEC_MANIFEST_HASH, violations);\n\
             compareRequiredIdentity(\"abi_hash\", build.abi_hash, GENERATED_ABI_HASH, violations);\n\
             compareRequiredIdentity(\"constraints_hash\", build.constraints_hash, GENERATED_CONSTRAINTS_HASH, violations);\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function assertCellScriptLockfile(lockfile: CellScriptLockfile): void {\n\
           const violations = validateCellScriptLockfile(lockfile);\n\
           if (violations.length > 0) {\n\
             throw new Error(\"CellScript builder identity mismatch: \" + violations.join(\"; \"));\n\
           }\n\
         }\n\n\
         export function validateCellScriptDeployment(\n\
           lockfile?: CellScriptLockfile,\n\
           deployment?: CellScriptDeploymentRecord,\n\
           liveEvidence?: CellScriptLiveDeploymentEvidence,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): string[] {\n\
           const violations: string[] = [];\n\
           if (!deployment) {\n\
             if (liveEvidence) {\n\
               violations.push(\"live deployment evidence requires a deployment record\");\n\
             }\n\
             violations.push(...validateCellScriptDeploymentTrust(deployment, trustPolicy));\n\
             return violations;\n\
           }\n\
           violations.push(...validateCellScriptDeploymentTrust(deployment, trustPolicy));\n\
           if (!deployment.status) {\n\
             violations.push(\"deployment record has no status; expected 'active'\");\n\
           } else if (deployment.status !== \"active\") {\n\
             violations.push(\"deployment status is '\" + deployment.status + \"'\");\n\
           }\n\
           compareDeploymentIdentity(\"compiler_version\", deployment.compiler_version, GENERATED_COMPILER_VERSION, violations);\n\
           compareDeploymentIdentity(\"artifact_hash\", deployment.artifact_hash, GENERATED_ARTIFACT_HASH, violations);\n\
           compareDeploymentIdentity(\"metadata_hash\", deployment.metadata_hash, GENERATED_METADATA_HASH, violations);\n\
           compareDeploymentIdentity(\"schema_hash\", deployment.schema_hash, GENERATED_SCHEMA_HASH, violations);\n\
           compareDeploymentIdentity(\"cell_data_codec_manifest_hash\", deployment.cell_data_codec_manifest_hash, GENERATED_CELL_DATA_CODEC_MANIFEST_HASH, violations);\n\
           compareDeploymentIdentity(\"abi_hash\", deployment.abi_hash, GENERATED_ABI_HASH, violations);\n\
           compareDeploymentIdentity(\"constraints_hash\", deployment.constraints_hash, GENERATED_CONSTRAINTS_HASH, violations);\n\
\n\
           const lockDeployment = lockfile?.deployment?.[deployment.network];\n\
           if (lockfile && !lockDeployment) {\n\
             violations.push(\"Cell.lock has no deployment ref for network '\" + deployment.network + \"'\");\n\
           } else if (lockDeployment) {\n\
             compareHexIdentity(\"deployment.code_hash\", lockDeployment.code_hash, deployment.code_hash, violations);\n\
             compareStringIdentity(\"deployment.out_point\", lockDeployment.out_point, deployment.out_point, violations);\n\
             compareHexIdentity(\"deployment.data_hash\", lockDeployment.data_hash, deployment.data_hash, violations);\n\
           }\n\
\n\
           const embeddedDeployments = BUILDER_MANIFEST_RUNTIME.deployment_identity?.deployments ?? [];\n\
           if (embeddedDeployments.length > 0) {\n\
             const embedded = embeddedDeployments.find((item) => item.network === deployment.network);\n\
             if (!embedded) {\n\
               violations.push(\"builder manifest has no embedded deployment for network '\" + deployment.network + \"'\");\n\
             } else {\n\
               compareHexIdentity(\"embedded_deployment.code_hash\", deployment.code_hash, embedded.code_hash, violations);\n\
               compareStringIdentity(\"embedded_deployment.out_point\", deployment.out_point, embedded.out_point, violations);\n\
               compareHexIdentity(\"embedded_deployment.data_hash\", deployment.data_hash, embedded.data_hash, violations);\n\
               compareStringIdentity(\"embedded_deployment.hash_type\", deployment.hash_type, embedded.hash_type, violations);\n\
               if (embedded.status || deployment.status) {\n\
                 compareStringIdentity(\"embedded_deployment.status\", deployment.status, embedded.status, violations);\n\
               }\n\
               if (embedded.type_id || deployment.type_id) {\n\
                 compareHexIdentity(\"embedded_deployment.type_id\", deployment.type_id, embedded.type_id, violations);\n\
               }\n\
             }\n\
           }\n\
\n\
           if (liveEvidence) {\n\
             if (liveEvidence.status && liveEvidence.status !== \"live-verified\") {\n\
               violations.push(\"live deployment evidence status is '\" + liveEvidence.status + \"'\");\n\
             }\n\
             if (liveEvidence.rpc_status && liveEvidence.rpc_status !== \"live\") {\n\
               violations.push(\"live deployment RPC status is '\" + liveEvidence.rpc_status + \"'\");\n\
             }\n\
             if (!liveEvidence.deployment_status) {\n\
               violations.push(\"live deployment evidence has no deployment_status\");\n\
             } else if (liveEvidence.deployment_status !== \"active\") {\n\
               violations.push(\"live deployment evidence deployment_status is '\" + liveEvidence.deployment_status + \"'\");\n\
             }\n\
             if (liveEvidence.violations && liveEvidence.violations.length > 0) {\n\
               violations.push(\"live deployment evidence reports violations: \" + liveEvidence.violations.join(\"; \"));\n\
             }\n\
             compareStringIdentity(\"live_deployment.out_point\", liveEvidence.out_point, deployment.out_point, violations);\n\
             compareHexIdentity(\"live_deployment.data_hash\", liveEvidence.rpc_data_hash, deployment.data_hash, violations);\n\
             compareHexIdentity(\"live_deployment.code_hash\", liveEvidence.rpc_code_hash, deployment.code_hash, violations);\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function validateCellScriptDeploymentTrust(\n\
           deployment?: CellScriptDeploymentRecord,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): string[] {\n\
           const violations: string[] = [];\n\
           const requirePublisherSignature = trustPolicy?.requirePublisherSignature ?? false;\n\
           const requireAuditReportHash = trustPolicy?.requireAuditReportHash ?? false;\n\
           if (!requirePublisherSignature && !requireAuditReportHash) {\n\
             return violations;\n\
           }\n\
           if (!deployment) {\n\
             violations.push(\"trust policy requires a deployment record\");\n\
             return violations;\n\
           }\n\
           if (requirePublisherSignature && !deployment.publisher_signature) {\n\
             violations.push(\"deployment record has no publisher_signature required by trust policy\");\n\
           }\n\
           if (requireAuditReportHash && !deployment.audit_report_hash) {\n\
             violations.push(\"deployment record has no audit_report_hash required by trust policy\");\n\
           }\n\
           return violations;\n\
         }\n\n\
         export function assertCellScriptDeployment(\n\
           lockfile?: CellScriptLockfile,\n\
           deployment?: CellScriptDeploymentRecord,\n\
           liveEvidence?: CellScriptLiveDeploymentEvidence,\n\
           trustPolicy?: CellScriptTrustPolicy,\n\
         ): void {\n\
           const violations = validateCellScriptDeployment(lockfile, deployment, liveEvidence, trustPolicy);\n\
           if (violations.length > 0) {\n\
             throw new Error(\"CellScript deployment identity mismatch: \" + violations.join(\"; \"));\n\
           }\n\
         }\n\n\
         function compareRequiredIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === null || expected === \"\") {\n\
             violations.push(\"generated metadata has no \" + field);\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(\"Cell.lock has no \" + field);\n\
             return;\n\
           }\n\
           if (actual !== expected) {\n\
             violations.push(field + \" mismatch: Cell.lock has '\" + actual + \"', metadata has '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareDeploymentIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === null || expected === \"\") {\n\
             violations.push(\"generated metadata has no \" + field);\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(\"deployment record has no \" + field);\n\
             return;\n\
           }\n\
           if (!identityEquals(actual, expected)) {\n\
             violations.push(field + \" mismatch: deployment has '\" + actual + \"', metadata has '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareStringIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null | undefined,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === undefined || expected === null || expected === \"\") {\n\
             violations.push(\"expected \" + field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual !== expected) {\n\
             violations.push(field + \" mismatch: actual '\" + actual + \"', expected '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function compareHexIdentity(\n\
           field: string,\n\
           actual: string | null | undefined,\n\
           expected: string | null | undefined,\n\
           violations: string[],\n\
         ): void {\n\
           if (expected === undefined || expected === null || expected === \"\") {\n\
             violations.push(\"expected \" + field + \" is missing\");\n\
             return;\n\
           }\n\
           if (actual === undefined || actual === null || actual === \"\") {\n\
             violations.push(field + \" is missing\");\n\
             return;\n\
           }\n\
           if (!hexEquals(actual, expected)) {\n\
             violations.push(field + \" mismatch: actual '\" + actual + \"', expected '\" + expected + \"'\");\n\
           }\n\
         }\n\n\
         function identityEquals(actual: string, expected: string): boolean {\n\
           return actual === expected || hexEquals(actual, expected);\n\
         }\n\n\
         function hexEquals(actual: string, expected: string): boolean {\n\
           return actual.replace(/^0x/i, \"\").toLowerCase() === expected.replace(/^0x/i, \"\").toLowerCase();\n\
         }\n\n",
    );

    for action in actions {
        let suffix = typescript_type_suffix(&action.name);
        let params_type = typescript_action_params_type(action);
        ts.push_str(&format!("export interface {suffix}Params {{\n"));
        for param in &action.params {
            ts.push_str(&format!("  {}: {};\n", typescript_object_key(&param.name), typescript_param_type(param)));
        }
        if action.params.is_empty() {
            ts.push_str("  readonly __noParams?: never;\n");
        }
        ts.push_str("}\n\n");
        ts.push_str(&format!(
            "export function plan{suffix}(params: {params_type}, options: BuildOptions = {{}}): ActionBuilderPlan<{params_type}> {{\n  \
             return makeActionPlan({}, params, options);\n}}\n\n",
            typescript_string_literal(&action.name)
        ));
    }

    ts.push_str("function makeActionPlan<P extends CellScriptParams>(action: string, params: P, options: BuildOptions): ActionBuilderPlan<P> {\n");
    ts.push_str("  if (options.lockfile) {\n    assertCellScriptLockfile(options.lockfile);\n  }\n");
    ts.push_str(
        "  if (options.deployment || options.liveDeploymentEvidence || options.trustPolicy) {\n    \
         assertCellScriptDeployment(options.lockfile, options.deployment, options.liveDeploymentEvidence, options.trustPolicy);\n  }\n",
    );
    ts.push_str(&format!(
        "  return {{\n    schema: CELLSCRIPT_BUILDER_SCHEMA,\n    state: \"GeneratedActionPlan\",\n    status: \"requires-runtime-resolution\",\n    action,\n    params,\n    options,\n    metadataHash: {},\n    artifactHash: {},\n    targetProfile: {},\n    canSubmit: false,\n    requiresLiveCellResolution: true,\n    requiresDeploymentResolution: true,\n    cellDataCodecManifest,\n    notProvenByGeneratedBuilder: [\n      \"live_cell_availability\",\n      \"deployment_live_chain_match\",\n      \"capacity_fee_balance\",\n      \"signature_authority\",\n      \"ckb_vm_execution\",\n      \"cell_data_codec_materialization\",\n      \"tx_pool_acceptance\",\n      \"submission\"\n    ] as const,\n  }};\n}}\n\n",
        typescript_string_literal(metadata_hash),
        metadata.artifact_hash.as_deref().map(typescript_string_literal).unwrap_or_else(|| "null".to_string()),
        typescript_string_literal(&metadata.target_profile.name)
    ));

    ts.push_str(
        "function assertRuntimeObject(value: unknown, label: string): Record<string, unknown> {\n\
           if (typeof value !== \"object\" || value === null || Array.isArray(value)) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: \" + label + \" must be an object\");\n\
           }\n\
           return value as Record<string, unknown>;\n\
         }\n\n\
         function assertLiveCellResolutionResult(value: unknown): LiveCellResolutionResult {\n\
           const record = assertRuntimeObject(value, \"resolveLiveCells result\");\n\
           for (const field of [\"inputs\", \"referenceInputs\", \"cellDeps\", \"headerDeps\", \"lineage\"] as const) {\n\
             const candidate = record[field];\n\
             if (candidate !== undefined && !Array.isArray(candidate)) {\n\
               throw new Error(\"CellScript runtime builder-shape mismatch: resolveLiveCells.\" + field + \" must be an array when present\");\n\
             }\n\
           }\n\
           return value as LiveCellResolutionResult;\n\
         }\n\n\
         function assertBuiltTransaction(value: unknown): unknown {\n\
           if (value === undefined || value === null) {\n\
             throw new Error(\"CellScript runtime builder-shape mismatch: buildTransaction returned no transaction\");\n\
           }\n\
           return value;\n\
         }\n\n\
         function submittedTxHashFromRuntime(submitResult: unknown): string | null {\n\
           if (typeof submitResult === \"string\") {\n\
             return submitResult;\n\
           }\n\
           if (typeof submitResult === \"object\" && submitResult !== null) {\n\
             const record = submitResult as Record<string, unknown>;\n\
             if (typeof record.txHash === \"string\") {\n\
               return record.txHash;\n\
             }\n\
             if (typeof record.hash === \"string\") {\n\
               return record.hash;\n\
             }\n\
           }\n\
           return null;\n\
         }\n\n",
    );

    ts.push_str("export function createActionBuilder(runtime: CellScriptBuilderRuntime) {\n  return {\n");
    for action in actions {
        let method = typescript_identifier(&action.name, "action");
        let suffix = typescript_type_suffix(&action.name);
        let params_type = typescript_action_params_type(action);
        ts.push_str(&format!(
            "    async {method}(params: {params_type}, options: BuildOptions = {{}}) {{\n      \
             const plan = plan{suffix}(params, options);\n      \
             return executeActionBuilderPlan(runtime, plan, options);\n    }},\n"
        ));
    }
    ts.push_str(
        "  };\n}\n\n\
         async function executeActionBuilderPlan<P extends CellScriptParams>(\n\
           runtime: CellScriptBuilderRuntime,\n\
           plan: ActionBuilderPlan<P>,\n\
           options: BuildOptions,\n\
         ): Promise<ActionBuilderResult<P>> {\n\
           const liveCellResolution = assertLiveCellResolutionResult(await runtime.resolveLiveCells({ plan, options }));\n\
           const transaction = assertBuiltTransaction(await runtime.buildTransaction({ ...plan, liveCellResolution }));\n\
           const result: ActionBuilderResult<P> = {\n\
             schema: CELLSCRIPT_BUILDER_SCHEMA,\n\
             state: \"ActionBuilderResult\",\n\
             status: \"built-by-runtime\",\n\
             mode: \"build\",\n\
             plan,\n\
             liveCellResolution,\n\
             transaction,\n\
             canSubmit: false,\n\
             notProvenByGeneratedBuilder: plan.notProvenByGeneratedBuilder,\n\
           };\n\
           if (options.dryRun || options.submit) {\n\
             if (!runtime.dryRun) {\n\
               throw new Error(\"CellScript builder runtime missing dryRun adapter\");\n\
             }\n\
             result.dryRunResult = await runtime.dryRun(transaction);\n\
             result.status = \"dry-run-by-runtime\";\n\
             result.mode = \"dry-run\";\n\
           }\n\
           if (options.submit) {\n\
             if (!runtime.submit) {\n\
               throw new Error(\"CellScript builder runtime missing submit adapter\");\n\
             }\n\
             result.submitResult = await runtime.submit(transaction);\n\
             result.submittedTxHash = submittedTxHashFromRuntime(result.submitResult);\n\
             result.status = \"submitted-by-runtime\";\n\
             result.mode = \"submit\";\n\
           }\n\
           return result;\n\
         }\n",
    );

    Ok(ts)
}

fn typescript_builder_test(actions: &[&crate::ActionMetadata]) -> Result<String> {
    let cases = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "plan": format!("plan{}", typescript_type_suffix(&action.name)),
                "method": typescript_identifier(&action.name, "action"),
                "params": javascript_sample_params(action),
            })
        })
        .collect::<Vec<_>>();
    let cases_json = json_string_pretty("builder test cases", &cases)?;

    let mut js = String::new();
    js.push_str("import assert from \"node:assert/strict\";\n");
    js.push_str("import test from \"node:test\";\n");
    js.push_str("import * as builder from \"../dist/index.js\";\n\n");
    js.push_str(&format!("const actionCases = {cases_json};\n"));
    js.push_str("const WRONG_HASH = \"0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\";\n\n");
    js.push_str(
        "test(\"plans all generated actions without submitting\", () => {\n\
           for (const actionCase of actionCases) {\n\
             const plan = builder[actionCase.plan](actionCase.params);\n\
             assert.equal(plan.schema, builder.CELLSCRIPT_BUILDER_SCHEMA);\n\
             assert.equal(plan.state, \"GeneratedActionPlan\");\n\
             assert.equal(plan.status, \"requires-runtime-resolution\");\n\
             assert.equal(plan.action, actionCase.name);\n\
             assert.equal(plan.canSubmit, false);\n\
             assert.equal(plan.requiresLiveCellResolution, true);\n\
             assert.equal(plan.requiresDeploymentResolution, true);\n\
             assert.deepEqual(plan.params, actionCase.params);\n\
           }\n\
         });\n\n\
         test(\"delegates live-cell resolution and transaction build to runtime\", async () => {\n\
           const [first] = actionCases;\n\
           const calls = [];\n\
           const runtime = {\n\
             async resolveLiveCells(request) {\n\
               calls.push([\"resolveLiveCells\", request.plan.action]);\n\
               return { inputs: [\"input-0\"], cellDeps: [\"dep-0\"], lineage: [] };\n\
             },\n\
             async buildTransaction(plan) {\n\
               calls.push([\"buildTransaction\", plan.action]);\n\
               return { action: plan.action, inputs: plan.liveCellResolution.inputs };\n\
             },\n\
           };\n\
           const api = builder.createActionBuilder(runtime);\n\
           const result = await api[first.method](first.params);\n\
           assert.equal(result.state, \"ActionBuilderResult\");\n\
           assert.equal(result.status, \"built-by-runtime\");\n\
           assert.equal(result.mode, \"build\");\n\
           assert.equal(result.plan.action, first.name);\n\
           assert.equal(result.transaction.action, first.name);\n\
           assert.deepEqual(result.liveCellResolution.inputs, [\"input-0\"]);\n\
           assert.deepEqual(calls, [[\"resolveLiveCells\", first.name], [\"buildTransaction\", first.name]]);\n\
         });\n\n\
         test(\"delegates dry-run and submit modes to runtime\", async () => {\n\
           const [first] = actionCases;\n\
           const calls = [];\n\
           const runtime = {\n\
             async resolveLiveCells(request) {\n\
               calls.push([\"resolveLiveCells\", request.plan.action]);\n\
               return { inputs: [\"input-0\"], cellDeps: [\"dep-0\"], lineage: [] };\n\
             },\n\
             async buildTransaction(plan) {\n\
               calls.push([\"buildTransaction\", plan.action]);\n\
               return { action: plan.action, built: true };\n\
             },\n\
             async dryRun(transaction) {\n\
               calls.push([\"dryRun\", transaction.action]);\n\
               return { cycles: 42, exitCode: 0 };\n\
             },\n\
             async submit(transaction) {\n\
               calls.push([\"submit\", transaction.action]);\n\
               return { txHash: \"0x1234\" };\n\
             },\n\
           };\n\
           const api = builder.createActionBuilder(runtime);\n\
           const dryRunResult = await api[first.method](first.params, { dryRun: true });\n\
           assert.equal(dryRunResult.mode, \"dry-run\");\n\
           assert.deepEqual(dryRunResult.dryRunResult, { cycles: 42, exitCode: 0 });\n\
           calls.length = 0;\n\
           const submitResult = await api[first.method](first.params, { submit: true });\n\
           assert.equal(submitResult.mode, \"submit\");\n\
           assert.equal(submitResult.submittedTxHash, \"0x1234\");\n\
           assert.deepEqual(calls, [\n\
             [\"resolveLiveCells\", first.name],\n\
             [\"buildTransaction\", first.name],\n\
             [\"dryRun\", first.name],\n\
             [\"submit\", first.name],\n\
           ]);\n\
         });\n\n\
         test(\"rejects missing runtime adapters and malformed runtime shapes\", async () => {\n\
           const [first] = actionCases;\n\
           const noDryRunRuntime = {\n\
             async resolveLiveCells() { return { inputs: [] }; },\n\
             async buildTransaction() { return { tx: true }; },\n\
           };\n\
           const badShapeRuntime = {\n\
             async resolveLiveCells() { return { inputs: \"not-an-array\" }; },\n\
             async buildTransaction() { return { tx: true }; },\n\
           };\n\
           await assert.rejects(\n\
             () => builder.createActionBuilder(noDryRunRuntime)[first.method](first.params, { dryRun: true }),\n\
             /missing dryRun adapter/,\n\
           );\n\
           await assert.rejects(\n\
             () => builder.createActionBuilder(badShapeRuntime)[first.method](first.params),\n\
             /builder-shape mismatch/,\n\
           );\n\
         });\n\n\
         test(\"maps runtime errors to action field context\", () => {\n\
           const [first] = actionCases;\n\
           const context = builder.runtimeErrorContextForAction(first.name);\n\
           assert.equal(context.action, first.name);\n\
           assert.equal(context.fields.length, Object.keys(first.params).length);\n\
           const byCode = builder.runtimeErrorInfoByCode(25);\n\
           assert.equal(byCode.name, \"entry-witness-abi-invalid\");\n\
           const fromObject = builder.explainCellScriptRuntimeError({ exitCode: 25 }, first.name);\n\
           assert.equal(fromObject.code, 25);\n\
           assert.equal(fromObject.action, first.name);\n\
           assert.equal(fromObject.actionFields.length, context.fields.length);\n\
           const fromMessage = builder.explainCellScriptRuntimeError({ message: \"entry witness payload layout failed\" }, first.name);\n\
           assert.equal(fromMessage.name, \"entry-witness-abi-invalid\");\n\
           assert.equal(builder.explainCellScriptRuntimeError({ exitCode: 999999 }, first.name), null);\n\
         });\n\n\
         test(\"rejects mismatched lockfile identity\", () => {\n\
           const [first] = actionCases;\n\
           const badLockfile = {\n\
             package: { source_hash: WRONG_HASH },\n\
             package_build: {\n\
               compiler_version: \"wrong-compiler\",\n\
               target_profile: builder.builderManifest.target_profile,\n\
               artifact_hash: builder.builderManifest.artifact_hash,\n\
               metadata_hash: builder.builderManifest.metadata_hash,\n\
             },\n\
           };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { lockfile: badLockfile }),\n\
             /CellScript builder identity mismatch/,\n\
           );\n\
         });\n\n\
         test(\"rejects mismatched deployment identity when deployment binding is embedded\", () => {\n\
           const [first] = actionCases;\n\
           const deployment = builder.builderManifest.deployment_identity?.deployments?.[0];\n\
           if (!deployment) {\n\
             assert.deepEqual(builder.validateCellScriptDeployment(undefined, undefined), []);\n\
             assert.deepEqual(builder.validateCellScriptDeploymentTrust(undefined, undefined), []);\n\
             assert.deepEqual(\n\
               builder.validateCellScriptDeploymentTrust(undefined, { requirePublisherSignature: true }),\n\
               [\"trust policy requires a deployment record\"],\n\
             );\n\
             assert.throws(\n\
               () => builder[first.plan](first.params, { trustPolicy: { requirePublisherSignature: true } }),\n\
               /trust policy requires a deployment record/,\n\
             );\n\
             return;\n\
           }\n\
           const badDeployment = { ...deployment, code_hash: WRONG_HASH };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: badDeployment }),\n\
             /CellScript deployment identity mismatch/,\n\
           );\n\
           const deprecatedDeployment = { ...deployment, status: \"deprecated\" };\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: deprecatedDeployment }),\n\
             /deployment status/,\n\
           );\n\
           const { status: _deploymentStatus, ...missingStatusDeployment } = deployment;\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: missingStatusDeployment }),\n\
             /no status/,\n\
           );\n\
           const { publisher_signature: _publisherSignature, audit_report_hash: _auditReportHash, ...unsignedDeployment } = deployment;\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, { deployment: unsignedDeployment, trustPolicy: { requirePublisherSignature: true } }),\n\
             /publisher_signature/,\n\
           );\n\
           const signedDeployment = { ...deployment, publisher_signature: \"sig:fixture\", audit_report_hash: \"0xaaa\" };\n\
           assert.deepEqual(\n\
             builder.validateCellScriptDeploymentTrust(signedDeployment, { requirePublisherSignature: true, requireAuditReportHash: true }),\n\
             [],\n\
           );\n\
           assert.throws(\n\
             () => builder[first.plan](first.params, {\n\
               deployment,\n\
               liveDeploymentEvidence: {\n\
                 status: \"failed\",\n\
                 deployment_status: \"deprecated\",\n\
                 rpc_status: \"dead\",\n\
                 out_point: deployment.out_point,\n\
                 rpc_data_hash: deployment.data_hash,\n\
                 rpc_code_hash: deployment.code_hash,\n\
                 violations: [\"deployment for network 'aggron4' is not active: Deprecated\"],\n\
               },\n\
             }),\n\
             /CellScript deployment identity mismatch/,\n\
           );\n\
         });\n",
    );
    Ok(js)
}

fn javascript_sample_params(action: &crate::ActionMetadata) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    for param in &action.params {
        params.insert(param.name.clone(), javascript_sample_param_value(param));
    }
    serde_json::Value::Object(params)
}

fn javascript_sample_param_value(param: &ParamMetadata) -> serde_json::Value {
    if param.schema_pointer_abi
        || param.schema_length_abi
        || param.ty == "Address"
        || param.ty == "Hash"
        || param.fixed_byte_len.is_some()
    {
        let bytes = param.fixed_byte_len.unwrap_or(if param.ty == "Address" || param.ty == "Hash" { 32 } else { 0 });
        return serde_json::Value::String(format!("0x{}", "00".repeat(bytes)));
    }

    match param.ty.as_str() {
        "bool" => serde_json::Value::Bool(false),
        "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128" => serde_json::json!(0),
        "()" => serde_json::Value::Null,
        _ => serde_json::Value::String("0x".to_string()),
    }
}

fn default_builder_package_name(metadata: &CompileMetadata) -> String {
    let module = metadata.module.replace("::", "-").replace('_', "-");
    let trimmed = module.trim_matches('-');
    if trimmed.is_empty() {
        "@cellscript/generated-builder".to_string()
    } else {
        format!("@cellscript/{}-builder", trimmed.to_ascii_lowercase())
    }
}

fn typescript_action_params_type(action: &crate::ActionMetadata) -> String {
    format!("{}Params", typescript_type_suffix(&action.name))
}

fn typescript_type_suffix(name: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if uppercase_next {
                output.push(ch.to_ascii_uppercase());
                uppercase_next = false;
            } else {
                output.push(ch);
            }
        } else {
            uppercase_next = true;
        }
    }
    if output.is_empty() || output.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        output.insert_str(0, "Action");
    }
    output
}

fn typescript_identifier(name: &str, fallback: &str) -> String {
    let mut ident = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch == '_' || ch == '$' || ch.is_ascii_alphabetic() || (index > 0 && ch.is_ascii_digit()) {
            ident.push(ch);
        } else if ch.is_ascii_digit() && index == 0 {
            ident.push('_');
            ident.push(ch);
        } else {
            ident.push('_');
        }
    }
    if ident.is_empty() || TYPESCRIPT_RESERVED_WORDS.contains(&ident.as_str()) {
        format!("{}_{}", fallback, &crate::hex_encode(&crate::ckb_blake2b256(name.as_bytes()))[..8].to_ascii_lowercase())
    } else {
        ident
    }
}

fn typescript_object_key(name: &str) -> String {
    let ident = typescript_identifier(name, "param");
    if ident == name {
        ident
    } else {
        typescript_string_literal(name)
    }
}

fn typescript_param_type(param: &ParamMetadata) -> &'static str {
    if param.schema_pointer_abi
        || param.schema_length_abi
        || param.fixed_byte_len.is_some()
        || param.ty == "Address"
        || param.ty == "Hash"
    {
        return "HexString | Uint8Array";
    }

    match param.ty.as_str() {
        "bool" => "boolean",
        "u8" | "u16" | "u32" | "u64" | "u128" | "i8" | "i16" | "i32" | "i64" | "i128" => "bigint | number | string",
        "()" => "null | undefined",
        _ => "CellScriptValue",
    }
}

fn typescript_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn json_bytes_pretty<T: serde::Serialize>(label: &str, value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize {label}: {error}")))
}

fn json_string_pretty<T: serde::Serialize>(label: &str, value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize {label}: {error}")))
}

const TYPESCRIPT_RESERVED_WORDS: &[&str] = &[
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "as",
    "implements",
    "interface",
    "let",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "yield",
];

fn ckb_fixture_manifest_report(manifest: &serde_json::Value, base_dir: &Path, manifest_bytes: &[u8]) -> serde_json::Value {
    let mut issues = Vec::<String>::new();
    let mut rows = Vec::<serde_json::Value>::new();
    let manifest_hash = crate::hex_encode(&crate::ckb_blake2b256(manifest_bytes));

    match manifest["schema"].as_str() {
        Some(CKB_STANDARD_COMPAT_MANIFEST_SCHEMA) => {}
        Some(ICKB_CLAIM_MANIFEST_SCHEMA) => return ickb_claim_manifest_report(manifest, base_dir, manifest_bytes),
        got => {
            issues.push(format!(
                "manifest schema must be {CKB_STANDARD_COMPAT_MANIFEST_SCHEMA} or {ICKB_CLAIM_MANIFEST_SCHEMA}, got {}",
                got.unwrap_or("<missing>")
            ));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    }

    let Some(suites) = manifest["suites"].as_array() else {
        issues.push("manifest suites must be an array".to_string());
        return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
    };

    for suite in suites {
        validate_ckb_fixture_suite(suite, base_dir, &mut rows, &mut issues);
    }

    ckb_fixture_report_json(manifest, manifest_hash, suites.len(), rows, issues)
}

fn ickb_claim_manifest_report(manifest: &serde_json::Value, base_dir: &Path, manifest_bytes: &[u8]) -> serde_json::Value {
    let mut issues = Vec::<String>::new();
    let mut rows = Vec::<serde_json::Value>::new();
    let manifest_hash = crate::hex_encode(&crate::ckb_blake2b256(manifest_bytes));

    if manifest["schema"].as_str() != Some(ICKB_CLAIM_MANIFEST_SCHEMA) {
        issues.push(format!(
            "iCKB claim manifest schema must be {ICKB_CLAIM_MANIFEST_SCHEMA}, got {}",
            manifest["schema"].as_str().unwrap_or("<missing>")
        ));
    }

    let matrix_path = match manifest["matrix_path"].as_str() {
        Some(path) if !path.is_empty() => base_dir.join(path),
        _ => {
            issues.push("iCKB claim manifest matrix_path must be a non-empty string".to_string());
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };

    let matrix_bytes = match std::fs::read(&matrix_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            issues.push(format!("failed to read iCKB matrix {}: {err}", matrix_path.display()));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };
    let matrix: serde_json::Value = match serde_json::from_slice(&matrix_bytes) {
        Ok(matrix) => matrix,
        Err(err) => {
            issues.push(format!("failed to parse iCKB matrix {}: {err}", matrix_path.display()));
            return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
        }
    };

    validate_ickb_claim_matrix(&matrix, &mut issues);

    let mut matrix_rows = BTreeMap::<String, &serde_json::Value>::new();
    if let Some(active_rows) = matrix["rows"].as_array() {
        for row in active_rows {
            if let Some(scenario) = row["scenario"].as_str() {
                if matrix_rows.insert(scenario.to_string(), row).is_some() {
                    issues.push(format!("iCKB matrix contains duplicate scenario {scenario}"));
                }
            } else {
                issues.push("iCKB matrix row missing scenario".to_string());
            }
        }
    } else {
        issues.push("iCKB matrix rows must be an array".to_string());
    }

    let default_production = manifest.get("default_production_evidence");
    let default_hardening = manifest.get("default_hardening");
    let Some(families) = manifest["families"].as_array() else {
        issues.push("iCKB claim manifest families must be an array".to_string());
        return ckb_fixture_report_json(manifest, manifest_hash, 0, rows, issues);
    };

    for family in families {
        validate_ickb_claim_family(family, &matrix_rows, default_production, default_hardening, &mut rows, &mut issues);
    }

    ckb_fixture_report_json(manifest, manifest_hash, families.len(), rows, issues)
}

fn ckb_fixture_report_json(
    manifest: &serde_json::Value,
    manifest_hash: String,
    suite_count: usize,
    rows: Vec<serde_json::Value>,
    issues: Vec<String>,
) -> serde_json::Value {
    let is_ickb_claim = manifest["schema"].as_str() == Some(ICKB_CLAIM_MANIFEST_SCHEMA);
    let evidence_execution_level = if is_ickb_claim { serde_json::json!(ICKB_DIFF_EVIDENCE_LEVEL) } else { serde_json::Value::Null };
    let required_executable_gate =
        if is_ickb_claim { serde_json::json!("cargo test --locked -p cellscript --test ickb_diff") } else { serde_json::Value::Null };
    serde_json::json!({
        "schema": "cellscript-ckb-fixture-verification-v0.17",
        "manifest_schema": manifest["schema"].as_str().unwrap_or("unknown"),
        "manifest_status": manifest["status"].as_str().unwrap_or("unknown"),
        "manifest_hash": manifest_hash,
        "execution_level": if is_ickb_claim { "DIFFERENTIAL_CKB_VM_MANIFEST" } else { "MODEL" },
        "ckb_vm_execution": false,
        "committed_ckb_vm_evidence": is_ickb_claim,
        "evidence_execution_level": evidence_execution_level,
        "required_executable_gate": required_executable_gate,
        "suite_count": suite_count,
        "fixture_count": rows.len(),
        "status": if issues.is_empty() { "ok" } else { "failed" },
        "issue_count": issues.len(),
        "issues": issues,
        "fixtures": rows,
        "vm_execution_note": if is_ickb_claim {
            "This command does not execute CKB VM. It validates the iCKB claim manifest against committed dual-side CKB VM differential rows, production evidence envelopes, and the required executable Rust gate."
        } else {
            "This command validates transaction-shape model fixtures only; it does not execute CKB VM or prove production compatibility."
        },
    })
}

fn validate_ickb_claim_matrix(matrix: &serde_json::Value, issues: &mut Vec<String>) {
    if matrix["schema"].as_str() != Some(ICKB_DIFF_MATRIX_SCHEMA) {
        issues.push(format!(
            "iCKB matrix schema must be {ICKB_DIFF_MATRIX_SCHEMA}, got {}",
            matrix["schema"].as_str().unwrap_or("<missing>")
        ));
    }
    if matrix["mode"].as_str() != Some("EXECUTED_CKB_VM_DIFF") {
        issues.push("iCKB matrix mode must be EXECUTED_CKB_VM_DIFF".to_string());
    }
    if matrix["equivalence_status"].as_str() != Some("PROVEN") {
        issues.push("iCKB matrix equivalence_status must be PROVEN".to_string());
    }
    if matrix["production_equivalence_claim"].as_bool() != Some(true) {
        issues.push("iCKB matrix production_equivalence_claim must be true".to_string());
    }
    if matrix["remaining_model_blockers"].as_array().is_none_or(|blockers| !blockers.is_empty()) {
        issues.push("iCKB matrix remaining_model_blockers must be empty".to_string());
    }
    if matrix["non_executable_model_assumptions"].as_array().is_none_or(|assumptions| !assumptions.is_empty()) {
        issues.push("iCKB matrix non_executable_model_assumptions must be empty".to_string());
    }
}

fn validate_ickb_claim_family(
    family: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    default_production: Option<&serde_json::Value>,
    default_hardening: Option<&serde_json::Value>,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let family_id = family["id"].as_str().unwrap_or("<missing-family>");
    if family["id"].as_str().is_none_or(str::is_empty) {
        issues.push("iCKB claim family id must be a non-empty string".to_string());
    }
    let Some(branches) = family["branches"].as_array() else {
        issues.push(format!("iCKB claim family {family_id} branches must be an array"));
        return;
    };

    for branch in branches {
        validate_ickb_claim_branch(family_id, branch, matrix_rows, default_production, default_hardening, rows, issues);
    }
}

fn validate_ickb_claim_branch(
    family_id: &str,
    branch: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    default_production: Option<&serde_json::Value>,
    default_hardening: Option<&serde_json::Value>,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let branch_id = branch["id"].as_str().unwrap_or("<missing-branch>");
    if branch["id"].as_str().is_none_or(str::is_empty) {
        issues.push(format!("iCKB claim family {family_id} has branch with missing id"));
    }
    let status = branch["status"].as_str().unwrap_or("<missing-status>");
    let mut matched = ickb_claim_branch_scenarios(branch, matrix_rows);

    match status {
        "in_scope" | "fixture_scoped" => {
            validate_ickb_required_scenarios(family_id, branch_id, branch, matrix_rows, &mut matched, issues);
            if matched.is_empty() {
                issues.push(format!("iCKB claim branch {family_id}/{branch_id} has no matching matrix rows"));
            }
            for scenario in &matched {
                if let Some(row) = matrix_rows.get(scenario) {
                    validate_ickb_claim_row(family_id, branch_id, scenario, row, issues);
                }
            }

            let production = branch.get("production_evidence").or(default_production);
            validate_ickb_evidence_object(
                "production_evidence",
                &ICKB_REQUIRED_PRODUCTION_EVIDENCE,
                production,
                family_id,
                branch_id,
                issues,
            );
            let hardening = branch.get("hardening").or(default_hardening);
            validate_ickb_evidence_object("hardening", &ICKB_REQUIRED_HARDENING_EVIDENCE, hardening, family_id, branch_id, issues);
            validate_ickb_claim_thresholds(family_id, branch_id, hardening, &matched, matrix_rows, issues);
            if status == "fixture_scoped" && branch["limitation"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB fixture-scoped branch {family_id}/{branch_id} must declare limitation"));
            }
        }
        "retired" => {
            if branch["reason"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB retired branch {family_id}/{branch_id} must declare reason"));
            }
            let replacements = json_string_array(branch, "replacement_scenarios");
            if replacements.is_empty() {
                issues.push(format!("iCKB retired branch {family_id}/{branch_id} must declare replacement_scenarios"));
            }
            for scenario in replacements {
                if !matrix_rows.contains_key(&scenario) {
                    issues.push(format!("iCKB retired branch {family_id}/{branch_id} replacement scenario is missing: {scenario}"));
                }
            }
        }
        "out_of_scope" => {
            if branch["reason"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB out-of-scope branch {family_id}/{branch_id} must declare reason"));
            }
            if branch["source_evidence"].as_str().is_none_or(str::is_empty) {
                issues.push(format!("iCKB out-of-scope branch {family_id}/{branch_id} must declare source_evidence"));
            }
        }
        other => issues.push(format!("iCKB claim branch {family_id}/{branch_id} has unsupported status {other}")),
    }

    rows.push(serde_json::json!({
        "family": family_id,
        "branch": branch_id,
        "status": status,
        "matched_rows": matched.len(),
        "reject_rows": matched.iter().filter(|scenario| {
            matrix_rows
                .get(*scenario)
                .is_some_and(|row| row["original_ickb_expected"].as_str() == Some("fail") || row["cellscript_expected"].as_str() == Some("fail"))
        }).count(),
        "evidence_level": if matched.is_empty() { "DECLARATIVE" } else { ICKB_DIFF_EVIDENCE_LEVEL },
    }));
}

fn ickb_claim_branch_scenarios(branch: &serde_json::Value, matrix_rows: &BTreeMap<String, &serde_json::Value>) -> BTreeSet<String> {
    let mut matched = BTreeSet::new();
    let excludes = json_string_array(branch, "exclude_scenario_prefixes");
    for scenario in json_string_array(branch, "evidence_scenarios") {
        matched.insert(scenario);
    }
    for prefix in json_string_array(branch, "evidence_scenario_prefixes") {
        for scenario in matrix_rows.keys() {
            if scenario.starts_with(&prefix) && !excludes.iter().any(|exclude| scenario.starts_with(exclude)) {
                matched.insert(scenario.clone());
            }
        }
    }
    matched
}

fn validate_ickb_required_scenarios(
    family_id: &str,
    branch_id: &str,
    branch: &serde_json::Value,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    matched: &mut BTreeSet<String>,
    issues: &mut Vec<String>,
) {
    for scenario in json_string_array(branch, "required_scenarios") {
        if matrix_rows.contains_key(&scenario) {
            matched.insert(scenario);
        } else {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} required scenario is missing: {scenario}"));
        }
    }
}

fn validate_ickb_claim_row(family_id: &str, branch_id: &str, scenario: &str, row: &serde_json::Value, issues: &mut Vec<String>) {
    if row["evidence_level"].as_str() != Some(ICKB_DIFF_EVIDENCE_LEVEL) {
        issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} must have evidence_level={ICKB_DIFF_EVIDENCE_LEVEL}"
        ));
    }
    if row["ckb_vm_execution"].as_bool() != Some(true)
        || row["original_ickb_executed"].as_bool() != Some(true)
        || row["full_differential"].as_bool() != Some(true)
    {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} is not a full dual-side VM row"));
    }
    let original = row["original_ickb_expected"].as_str();
    let cellscript = row["cellscript_expected"].as_str();
    if original != cellscript {
        issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} expectation mismatch original={original:?} cellscript={cellscript:?}"
        ));
    }
    if (original == Some("fail") || cellscript == Some("fail"))
        && row["failure_mode"].as_str().is_none_or(str::is_empty)
        && row["execution"]["failure_mode"].as_str().is_none_or(str::is_empty)
    {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} reject scenario {scenario} lacks named failure mode"));
    }
    for field in ["tx_size_bytes", "occupied_capacity_shannons", "fee_shannons"] {
        if !row["execution"].get(field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution missing {field}"));
        }
    }
    if !row["execution"]["normalized_fixture"].is_object() {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} missing normalized_fixture"));
    }
    validate_ickb_claim_execution_object(family_id, branch_id, scenario, row, issues);
}

fn validate_ickb_claim_execution_object(
    family_id: &str,
    branch_id: &str,
    scenario: &str,
    row: &serde_json::Value,
    issues: &mut Vec<String>,
) {
    let Some(execution) = row["execution"].as_object() else {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} missing execution object"));
        return;
    };
    for field in [
        "fixture_sha256",
        "normalized_fixture_sha256",
        "transaction_context_sha256",
        "original_ickb_binary_sha256",
        "cellscript_artifact_sha256",
        "ckb_vm_or_testtool_version",
        "original_ickb_exit_code",
        "cellscript_exit_code",
        "original_ickb_status",
        "cellscript_status",
        "statuses_match",
        "original_cycles",
        "cellscript_cycles",
        "tx_size_bytes",
        "occupied_capacity_shannons",
        "fee_shannons",
    ] {
        if !execution.get(field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution missing non-empty {field}"));
        }
    }

    for field in ["fixture_sha256", "normalized_fixture_sha256", "original_ickb_binary_sha256", "cellscript_artifact_sha256"] {
        match execution.get(field).and_then(serde_json::Value::as_str) {
            Some(hash) if ckb_fixture_is_canonical_prefixed_sha256(hash) => {}
            _ => issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.{field} must be canonical 0x-prefixed SHA-256"
            )),
        }
    }

    match execution.get("transaction_context_sha256").and_then(serde_json::Value::as_object) {
        Some(hashes) => {
            for side in ["original", "cellscript"] {
                match hashes.get(side).and_then(serde_json::Value::as_str) {
                    Some(hash) if ckb_fixture_is_canonical_prefixed_sha256(hash) => {}
                    _ => issues.push(format!(
                        "iCKB claim branch {family_id}/{branch_id} scenario {scenario} transaction_context_sha256.{side} must be canonical 0x-prefixed SHA-256"
                    )),
                }
            }
        }
        None => issues.push(format!(
            "iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.transaction_context_sha256 must be an object"
        )),
    }

    if execution.get("statuses_match").and_then(serde_json::Value::as_bool) != Some(true) {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.statuses_match must be true"));
    }

    for (side, expected_field, status_field, exit_field, cycle_field) in [
        ("original", "original_ickb_expected", "original_ickb_status", "original_ickb_exit_code", "original_cycles"),
        ("cellscript", "cellscript_expected", "cellscript_status", "cellscript_exit_code", "cellscript_cycles"),
    ] {
        let expected = row[expected_field].as_str();
        let status = execution.get(status_field).and_then(serde_json::Value::as_str);
        if expected.is_some() && status != expected {
            issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} status {status:?} does not match {expected_field}={expected:?}"
            ));
        }
        if status == Some("pass") {
            if execution.get(exit_field).and_then(serde_json::Value::as_i64) != Some(0) {
                issues
                    .push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} pass must have exit code 0"));
            }
            if execution.get(cycle_field).and_then(serde_json::Value::as_u64).unwrap_or(0) == 0 {
                issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} pass must consume cycles"));
            }
        }
        if status == Some("fail") && execution.get(exit_field).and_then(serde_json::Value::as_i64) == Some(0) {
            issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} {side} fail must have a non-zero exit code"
            ));
        }
    }

    for field in ["tx_size_bytes", "occupied_capacity_shannons"] {
        if execution.get(field).and_then(serde_json::Value::as_u64).unwrap_or(0) == 0 {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} scenario {scenario} execution.{field} must be positive"));
        }
    }

    if row["original_ickb_expected"] == "fail" || row["cellscript_expected"] == "fail" {
        match execution.get("failure_mode").and_then(serde_json::Value::as_str) {
            Some(mode) if !mode.is_empty() => {}
            _ => issues.push(format!(
                "iCKB claim branch {family_id}/{branch_id} scenario {scenario} reject case missing execution.failure_mode"
            )),
        }
    }
}

fn validate_ickb_evidence_object(
    label: &str,
    required: &[&str],
    object: Option<&serde_json::Value>,
    family_id: &str,
    branch_id: &str,
    issues: &mut Vec<String>,
) {
    let Some(object) = object.and_then(serde_json::Value::as_object) else {
        issues.push(format!("iCKB claim branch {family_id}/{branch_id} missing {label} object"));
        return;
    };
    for field in required {
        if !object.get(*field).is_some_and(ckb_fixture_non_empty_json_value) {
            issues.push(format!("iCKB claim branch {family_id}/{branch_id} {label} missing non-empty {field}"));
        }
    }
}

fn validate_ickb_claim_thresholds(
    family_id: &str,
    branch_id: &str,
    hardening: Option<&serde_json::Value>,
    scenarios: &BTreeSet<String>,
    matrix_rows: &BTreeMap<String, &serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let max_cycles = hardening.and_then(|value| value["max_cellscript_cycles"].as_u64());
    let max_tx_size = hardening.and_then(|value| value["max_tx_size_bytes"].as_u64());
    for scenario in scenarios {
        let Some(row) = matrix_rows.get(scenario) else {
            continue;
        };
        if let (Some(max), Some(actual)) = (max_cycles, row["execution"]["cellscript_cycles"].as_u64()) {
            if actual > max {
                issues.push(format!(
                    "iCKB claim branch {family_id}/{branch_id} scenario {scenario} cellscript_cycles {actual} exceeds {max}"
                ));
            }
        }
        if let (Some(max), Some(actual)) = (max_tx_size, row["execution"]["tx_size_bytes"].as_u64()) {
            if actual > max {
                issues.push(format!(
                    "iCKB claim branch {family_id}/{branch_id} scenario {scenario} tx_size_bytes {actual} exceeds {max}"
                ));
            }
        }
    }
}

fn json_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key].as_array().into_iter().flatten().filter_map(serde_json::Value::as_str).map(ToString::to_string).collect()
}

fn ckb_fixture_non_empty_json_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => true,
    }
}

fn ckb_fixture_is_canonical_prefixed_sha256(value: &str) -> bool {
    value
        .strip_prefix("0x")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
}

fn validate_ckb_fixture_suite(
    suite: &serde_json::Value,
    base_dir: &Path,
    rows: &mut Vec<serde_json::Value>,
    issues: &mut Vec<String>,
) {
    let suite_name = suite["name"].as_str().unwrap_or("<unknown>");
    let accepted = ckb_fixture_names(suite, "accepted_fixtures", issues, suite_name);
    let rejected = ckb_fixture_names(suite, "rejected_fixtures", issues, suite_name);
    let Some(files) = suite["fixture_files"].as_object() else {
        issues.push(format!("suite {suite_name} missing fixture_files object"));
        return;
    };
    for fixture_name in accepted.iter().chain(rejected.iter()) {
        let Some(file) = files.get(*fixture_name).and_then(serde_json::Value::as_str) else {
            issues.push(format!("suite {suite_name} missing fixture file mapping for {fixture_name}"));
            continue;
        };
        let path = base_dir.join(file);
        let fixture = match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(value) => value,
                Err(error) => {
                    issues.push(format!("fixture {file} failed to parse: {error}"));
                    continue;
                }
            },
            Err(error) => {
                issues.push(format!("fixture {file} failed to read: {error}"));
                continue;
            }
        };
        match validate_ckb_fixture_model(&fixture, suite_name, fixture_name, accepted.contains(fixture_name), file) {
            Ok(row) => rows.push(row),
            Err(error) => issues.push(error),
        }
    }
}

fn validate_ckb_fixture_model(
    fixture: &serde_json::Value,
    suite_name: &str,
    fixture_name: &str,
    should_accept: bool,
    file: &str,
) -> std::result::Result<serde_json::Value, String> {
    if fixture["schema"].as_str() != Some(CKB_STANDARD_FIXTURE_SCHEMA) {
        return Err(format!("fixture {file} schema must be {CKB_STANDARD_FIXTURE_SCHEMA}"));
    }
    if fixture["suite"].as_str() != Some(suite_name) {
        return Err(format!("fixture {file} suite does not match manifest suite {suite_name}"));
    }
    if fixture["fixture_name"].as_str() != Some(fixture_name) {
        return Err(format!("fixture {file} fixture_name does not match manifest key {fixture_name}"));
    }

    let shape = &fixture["transaction_shape"];
    ckb_fixture_require_array(shape, "inputs")?;
    ckb_fixture_require_array(shape, "outputs")?;
    ckb_fixture_require_array(shape, "cell_deps")?;
    ckb_fixture_validate_metadata_expectation(fixture)?;
    ckb_fixture_validate_capacity_report(fixture)?;

    let expected = fixture["expected_behavior"].as_object().ok_or_else(|| format!("fixture {file} missing expected_behavior"))?;
    let expected_exit = expected
        .get("script_exit_code")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| format!("fixture {file} missing expected_behavior.script_exit_code"))?;
    let expected_reason = expected.get("rejection_reason").and_then(serde_json::Value::as_str);

    if should_accept && fixture["status"].as_str() != Some("accepted") {
        return Err(format!("fixture {file} is listed accepted but status is not accepted"));
    }
    if !should_accept && fixture["status"].as_str() != Some("rejected") {
        return Err(format!("fixture {file} is listed rejected but status is not rejected"));
    }
    if should_accept && expected_exit != 0 {
        return Err(format!("fixture {file} accepted case has non-zero expected exit code"));
    }
    if !should_accept && expected_exit == 0 {
        return Err(format!("fixture {file} rejected case has zero expected exit code"));
    }
    if !should_accept && expected_reason.is_none() {
        return Err(format!("fixture {file} rejected case lacks expected_behavior.rejection_reason"));
    }

    let verdict = ckb_fixture_evaluate_semantics(fixture)?;
    if verdict.0 != expected_exit {
        return Err(format!("fixture {file} model exit {} disagrees with expected exit {expected_exit}", verdict.0));
    }
    if expected_exit != 0 && verdict.1.as_deref() != expected_reason {
        return Err(format!("fixture {file} model rejection {:?} disagrees with expected {:?}", verdict.1, expected_reason));
    }

    Ok(serde_json::json!({
        "suite": suite_name,
        "fixture_name": fixture_name,
        "file": file,
        "expected_status": fixture["status"].as_str().unwrap_or("unknown"),
        "execution_level": "MODEL",
        "ckb_vm_execution": false,
        "model_exit_code": verdict.0,
        "expected_exit_code": expected_exit,
        "rejection_reason": verdict.1.or_else(|| expected_reason.map(str::to_string)),
        "status": if verdict.0 == 0 { "accepted" } else { "rejected" },
    }))
}

fn ckb_fixture_validate_metadata_expectation(fixture: &serde_json::Value) -> std::result::Result<(), String> {
    let metadata = fixture["metadata_expectation"].as_object().ok_or("fixture missing metadata_expectation")?;
    let proof_plan =
        metadata.get("proof_plan").and_then(serde_json::Value::as_object).ok_or("fixture missing proof_plan expectation")?;
    for key in ["trigger", "scope", "reads", "coverage", "on_chain_checked"] {
        if !proof_plan.contains_key(key) {
            return Err(format!("fixture proof_plan expectation missing {key}"));
        }
    }
    if !metadata.contains_key("codegen_coverage_status") {
        return Err("fixture metadata expectation missing codegen_coverage_status".to_string());
    }
    if fixture.get("cycle_report").is_none() {
        return Err("fixture missing cycle_report".to_string());
    }
    if fixture.get("capacity_report").is_none() {
        return Err("fixture missing capacity_report".to_string());
    }
    Ok(())
}

fn ckb_fixture_evaluate_semantics(fixture: &serde_json::Value) -> std::result::Result<(i64, Option<String>), String> {
    let shape = &fixture["transaction_shape"];
    match fixture["suite"].as_str().ok_or("missing suite")? {
        "sudt" => ckb_fixture_evaluate_amount_conservation(shape, "sudt-cell", "output_amount > input_amount; conservation violated"),
        "xudt" => {
            if ckb_fixture_any_cell_dep_name(shape, "lockup") || ckb_fixture_any_input_witness(shape, "lockup-active") {
                return Ok(ckb_fixture_reject("extension_policy_violated: lockup period not expired"));
            }
            ckb_fixture_evaluate_amount_conservation(shape, "xudt-cell", "output_amount > input_amount; conservation violated")
        }
        "acp" => {
            let first_input = ckb_fixture_first_cell(shape, "inputs")?;
            let first_output = ckb_fixture_first_cell(shape, "outputs")?;
            if ckb_fixture_cell_str(first_input, "witness").contains("wrong")
                || ckb_fixture_cell_str(first_input, "lock_script") != ckb_fixture_cell_str(first_output, "lock_script")
            {
                return Ok(ckb_fixture_reject("witness_lock_hash != args_owner_lock_hash"));
            }
            Ok(ckb_fixture_pass())
        }
        "cheque" => {
            let first_input = ckb_fixture_first_cell(shape, "inputs")?;
            let first_output = ckb_fixture_first_cell(shape, "outputs")?;
            if ckb_fixture_cell_str(first_input, "witness").contains("wrong")
                || ckb_fixture_cell_str(first_output, "lock_script").contains("wrong")
            {
                return Ok(ckb_fixture_reject("receiver_lock_hash != args_receiver_hash"));
            }
            Ok(ckb_fixture_pass())
        }
        "omnilock" => {
            if ckb_fixture_any_input_witness(shape, "invalid") {
                Ok(ckb_fixture_reject("auth_verification_failed: invalid_signature_or_wrong_method"))
            } else {
                Ok(ckb_fixture_pass())
            }
        }
        "nervosdao-since" => {
            if shape["header_deps"].as_array().into_iter().flatten().any(|header| header.as_str() == Some("mature-epoch-header")) {
                Ok(ckb_fixture_pass())
            } else {
                Ok(ckb_fixture_reject("since_not_mature: current_epoch < required_epoch"))
            }
        }
        "type-id" => {
            let type_id_outputs = shape["outputs"]
                .as_array()
                .ok_or("missing outputs")?
                .iter()
                .filter(|output| ckb_fixture_cell_str(output, "type_script").starts_with("type-id-script"))
                .count();
            if type_id_outputs > 1 {
                Ok(ckb_fixture_reject("duplicate_type_id: at-most-one-input-and-one-output-per-type-id-group"))
            } else {
                Ok(ckb_fixture_pass())
            }
        }
        other => Err(format!("unsupported compat fixture suite {other}")),
    }
}

fn ckb_fixture_evaluate_amount_conservation(
    shape: &serde_json::Value,
    cell_type: &str,
    reason: &str,
) -> std::result::Result<(i64, Option<String>), String> {
    let input_sum = ckb_fixture_amount_sum(shape, "inputs", cell_type)?;
    let output_sum = ckb_fixture_amount_sum(shape, "outputs", cell_type)?;
    if output_sum > input_sum {
        Ok(ckb_fixture_reject(reason))
    } else {
        Ok(ckb_fixture_pass())
    }
}

fn ckb_fixture_amount_sum(shape: &serde_json::Value, side: &str, cell_type: &str) -> std::result::Result<u128, String> {
    shape[side]
        .as_array()
        .ok_or_else(|| format!("missing transaction_shape.{side}"))?
        .iter()
        .filter(|cell| ckb_fixture_cell_str(cell, "type") == cell_type)
        .try_fold(0u128, |total, cell| Ok(total + ckb_fixture_little_endian_u128(ckb_fixture_cell_str(cell, "data"))?))
}

fn ckb_fixture_little_endian_u128(hex_value: &str) -> std::result::Result<u128, String> {
    let bytes = hex_value.strip_prefix("0x").unwrap_or(hex_value);
    if bytes.is_empty() {
        return Ok(0);
    }
    if !bytes.len().is_multiple_of(2) {
        return Err(format!("odd-length hex amount {hex_value}"));
    }
    let raw = hex::decode(bytes).map_err(|err| format!("invalid hex amount {hex_value}: {err}"))?;
    if raw.len() > 16 {
        return Err(format!("amount data exceeds u128 width: {} bytes", raw.len()));
    }
    let mut padded = [0u8; 16];
    padded[..raw.len()].copy_from_slice(&raw);
    Ok(u128::from_le_bytes(padded))
}

fn ckb_fixture_validate_capacity_report(fixture: &serde_json::Value) -> std::result::Result<(), String> {
    let reported = fixture["capacity_report"]["occupied_capacity_shannons"]
        .as_u64()
        .ok_or("capacity_report missing occupied_capacity_shannons")?;
    let output_capacity = fixture["transaction_shape"]["outputs"]
        .as_array()
        .ok_or("missing outputs")?
        .iter()
        .map(|output| output["capacity_shannons"].as_u64().ok_or("output missing capacity_shannons"))
        .try_fold(0u64, |total, value| value.map(|value| total.saturating_add(value)))?;
    if reported > output_capacity {
        return Err(format!("capacity report occupied capacity {reported} exceeds output capacity {output_capacity}"));
    }
    Ok(())
}

fn ckb_fixture_names<'a>(suite: &'a serde_json::Value, key: &str, issues: &mut Vec<String>, suite_name: &str) -> BTreeSet<&'a str> {
    match suite[key].as_array() {
        Some(values) => values.iter().filter_map(serde_json::Value::as_str).collect(),
        None => {
            issues.push(format!("suite {suite_name} missing {key} array"));
            BTreeSet::new()
        }
    }
}

fn ckb_fixture_require_array(value: &serde_json::Value, key: &str) -> std::result::Result<(), String> {
    value[key].as_array().map(|_| ()).ok_or_else(|| format!("missing transaction_shape.{key}"))
}

fn ckb_fixture_first_cell<'a>(shape: &'a serde_json::Value, side: &str) -> std::result::Result<&'a serde_json::Value, String> {
    shape[side].as_array().and_then(|cells| cells.first()).ok_or_else(|| format!("missing first transaction_shape.{side} cell"))
}

fn ckb_fixture_cell_str<'a>(cell: &'a serde_json::Value, field: &str) -> &'a str {
    cell[field].as_str().unwrap_or("")
}

fn ckb_fixture_any_cell_dep_name(shape: &serde_json::Value, needle: &str) -> bool {
    shape["cell_deps"].as_array().into_iter().flatten().any(|dep| dep["name"].as_str().is_some_and(|name| name.contains(needle)))
}

fn ckb_fixture_any_input_witness(shape: &serde_json::Value, needle: &str) -> bool {
    shape["inputs"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|input| input["witness"].as_str().is_some_and(|witness| witness.contains(needle)))
}

fn ckb_fixture_pass() -> (i64, Option<String>) {
    (0, None)
}

fn ckb_fixture_reject(reason: &str) -> (i64, Option<String>) {
    (1, Some(reason.to_string()))
}

fn transaction_solver_template(metadata: &CompileMetadata) -> serde_json::Value {
    let assumptions = &metadata.runtime.builder_assumptions;
    let ckb = metadata.constraints.ckb.as_ref();

    // Cell selection: derive input requirements from actions and ProofPlan
    let mut input_slots = Vec::new();
    let mut output_slots = Vec::new();
    let mut dep_slots = Vec::new();
    let mut witness_slots = Vec::new();

    // Build input slots from consume/consume_set patterns in actions
    for action in &metadata.actions {
        for plan in &action.proof_plan {
            if plan.reads.iter().any(|r| r == "input" || r == "group_input") {
                input_slots.push(serde_json::json!({
                    "source": "proof-plan-input",
                    "scope_kind": "action",
                    "scope_name": action.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "input" || **r == "group_input").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Build output slots from create/create_set patterns
    for action in &metadata.actions {
        for plan in &action.proof_plan {
            if plan.reads.iter().any(|r| r == "output" || r == "group_output") {
                output_slots.push(serde_json::json!({
                    "source": "proof-plan-output",
                    "scope_kind": "action",
                    "scope_name": action.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "output" || **r == "group_output").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Build lock input/output slots
    for lock in &metadata.locks {
        for plan in &lock.proof_plan {
            if plan.reads.iter().any(|r| r == "input" || r == "group_input") {
                input_slots.push(serde_json::json!({
                    "source": "proof-plan-input",
                    "scope_kind": "lock",
                    "scope_name": lock.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "input" || **r == "group_input").cloned().collect::<Vec<_>>(),
                }));
            }
            if plan.reads.iter().any(|r| r == "output" || r == "group_output") {
                output_slots.push(serde_json::json!({
                    "source": "proof-plan-output",
                    "scope_kind": "lock",
                    "scope_name": lock.name,
                    "feature": plan.feature,
                    "required_reads": plan.reads.iter().filter(|r| **r == "output" || **r == "group_output").cloned().collect::<Vec<_>>(),
                }));
            }
        }
    }

    // Dep resolution from CKB constraints
    if let Some(ckb_constraints) = ckb {
        for dep in &ckb_constraints.dep_group_manifest.declared_cell_deps {
            dep_slots.push(serde_json::json!({
                "source": "metadata-script-reference",
                "name": dep.name,
                "dep_type": dep.dep_type,
                "hash_type": dep.hash_type,
            }));
        }
        for script_ref in &ckb_constraints.script_references {
            dep_slots.push(serde_json::json!({
                "source": "metadata-script-reference",
                "name": script_ref.name,
                "scope": script_ref.scope,
                "purpose": script_ref.purpose,
            }));
        }
    }

    // Witness placement from builder assumptions
    let witness_fields = assumptions
        .iter()
        .flat_map(|assumption| assumption.required_witness_fields.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if !witness_fields.is_empty() {
        witness_slots.push(serde_json::json!({
            "source": "builder-assumption-witness-fields",
            "fields": witness_fields,
        }));
    }

    // Evidence requirements
    let evidence = assumptions
        .iter()
        .filter(|assumption| {
            matches!(
                assumption.kind.as_str(),
                "create_unique_global_uniqueness"
                    | "type_id_builder_plan"
                    | "metadata_only_gap"
                    | "runtime_required_proof_plan"
                    | "lock_group_transaction_scope"
                    | "capacity_policy"
            )
        })
        .map(|assumption| {
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "proof_plan_status": assumption.proof_plan_status,
                "detail": assumption.detail,
                "evidence_schema": evidence_schema_for_assumption(assumption),
            })
        })
        .collect::<Vec<_>>();

    // Fee/change planning from CKB constraints
    let fee_planning = ckb
        .map(|c| {
            serde_json::json!({
                "capacity_planning_required": c.capacity_planning_required,
                "capacity_policy": c.capacity_policy_surface,
                "created_output_count": c.created_output_count,
                "mutated_output_count": c.mutated_output_count,
                "occupied_capacity_evidence": c.capacity_evidence_contract.measured_occupied_capacity_shannons,
                "tx_size_bytes": c.tx_size_bytes,
            })
        })
        .unwrap_or(serde_json::json!(null));

    // Deterministic signing manifest
    let signature_requests = metadata
        .locks
        .iter()
        .map(|lock| {
            serde_json::json!({
                "lock_name": lock.name,
                "witness_index": format!("lock:{}:witness_0", lock.name),
                "signature_policy": "explicit-witness-no-implicit-signer",
            })
        })
        .collect::<Vec<_>>();

    let header_dep_slots = ckb
        .map(|c| {
            if c.uses_header_epoch {
                vec![serde_json::json!({
                    "source": "metadata-requirement",
                    "kind": "header_dep",
                    "status": "unresolved",
                    "required_external_step": "resolve concrete epoch/header dep before transaction construction",
                })]
            } else {
                Vec::new()
            }
        })
        .unwrap_or_default();

    serde_json::json!({
        "status": "template-only",
        "solver": "cellscript-v0.16-transaction-template-emitter",
        "solver_capability": "template-emitter-only",
        "solver_readiness": "not-a-solver",
        "execution_mode": "non-executable-template",
        "can_submit": false,
        "requires_validate_tx": true,
        "module": metadata.module,
        "target_profile": metadata.target_profile.name,
        "transaction_plan": {
            "version": 0,
            "inputs": input_slots,
            "outputs": output_slots,
            "cell_deps": dep_slots,
            "witnesses": witness_slots,
            "header_deps": header_dep_slots,
            "header_deps_status": "unresolved-template-slots",
            "builder_assumption_evidence_requirements": evidence,
        },
        "fee_change_plan": fee_planning,
        "signing_manifest": {
            "policy": "explicit-witness-no-implicit-signer",
            "signature_requests": signature_requests,
        },
        "builder_assumptions": assumptions,
        "required_external_steps": [
            "live cell selection",
            "concrete CellDep and HeaderDep resolution",
            "fee and change calculation",
            "occupied-capacity and under-capacity measurement",
            "witness placement and signing",
            "CKB dry-run or VM verification"
        ],
        "limitations": [
            "template only: does not perform live cell selection",
            "template only: does not resolve concrete deps/header deps",
            "template only: does not calculate fee/change or occupied capacity",
            "template only: does not place final witnesses or signatures",
            "CKB dry-run required for production acceptance"
        ],
    })
}

fn evidence_schema_for_assumption(assumption: &crate::BuilderAssumptionMetadata) -> serde_json::Value {
    let mut payload_arrays = Vec::new();
    if !assumption.required_inputs.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "inputs",
            "aliases": ["input_cells", "required_inputs"],
            "transaction_array": "inputs",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["source", "out_point", "type_hash", "lock_hash", "capacity"],
        }));
    }
    if !assumption.required_outputs.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "outputs",
            "aliases": ["output_cells", "required_outputs"],
            "transaction_array": "outputs",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["source", "type_hash", "lock_hash", "capacity", "data"],
        }));
    }
    if !assumption.required_cell_deps.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "cell_deps",
            "aliases": ["required_cell_deps"],
            "transaction_array": "cell_deps",
            "item_required_fields": ["index"],
            "item_concrete_fields": ["name", "out_point", "code_hash", "tx_hash", "dep_type"],
        }));
    }
    if !assumption.required_witness_fields.is_empty() {
        payload_arrays.push(serde_json::json!({
            "name": "witnesses",
            "aliases": ["witness_fields", "required_witness_fields"],
            "transaction_array": "witnesses",
            "item_required_fields": ["index", "field"],
            "item_concrete_fields": ["field", "lock", "input_type", "output_type", "bytes"],
        }));
    }

    let mut payload_objects = Vec::new();
    if assumption.kind == "capacity_policy" || assumption.capacity_policy != "none" {
        payload_objects.push(serde_json::json!({
            "name": "capacity",
            "required_fields": ["occupied_capacity_shannons", "tx_size_bytes", "under_capacity_output_indexes"],
            "failure_rule": "under_capacity_output_indexes must be an empty array for validate-tx success",
        }));
    }
    if assumption.kind == "type_id_builder_plan" {
        payload_objects.push(serde_json::json!({
            "name": "type_id",
            "required_fields": ["first_input_out_point", "output_index", "expected_type_id_args"],
            "expected_type_id_args": "canonical 0x-prefixed 32-byte hex",
            "transaction_cross_check": "output_index must point to the output whose type args equal expected_type_id_args when the tx JSON exposes type args",
        }));
    }
    if assumption.kind == "create_unique_global_uniqueness" {
        payload_objects.push(serde_json::json!({
            "name": "uniqueness",
            "required_any_of": ["uniqueness_checked=true", "uniqueness_proof", "unique_cell"],
        }));
    }
    if assumption.kind == "lock_group_transaction_scope" {
        payload_objects.push(serde_json::json!({
            "name": "lock_group_transaction_scope",
            "required_any_of": ["transaction_scope_reviewed=true", "covered_lock_groups"],
        }));
    }
    if matches!(assumption.kind.as_str(), "metadata_only_gap" | "runtime_required_proof_plan") {
        payload_objects.push(serde_json::json!({
            "name": "manual_review",
            "required_any_of": ["manual_review", "checked=true"],
        }));
    }

    serde_json::json!({
        "required_fields": ["assumption_id", "kind", "origin", "feature", "proof_plan_status", "evidence"],
        "payload_type": "object",
        "payload_arrays": payload_arrays,
        "payload_objects": payload_objects,
        "cross_checks": [
            "array evidence items must include numeric index fields that bind to the transaction array",
            "when evidence and the indexed transaction object both expose a concrete field, validate-tx requires equality",
            "capacity evidence must fail closed when under-capacity outputs are reported",
            "TYPE_ID evidence must use canonical 32-byte args and match output type args when present"
        ],
        "note": "builder must replace this requirement with concrete evidence before validate-tx can pass",
    })
}

fn deployment_plan_json(metadata: &CompileMetadata) -> serde_json::Value {
    let ckb = metadata.constraints.ckb.as_ref();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-deploy-plan-v0.16",
        "module": metadata.module,
        "compiler_version": metadata.compiler_version,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "artifact": {
            "format": metadata.artifact_format,
            "hash": metadata.artifact_hash,
            "size_bytes": metadata.artifact_size_bytes,
        },
        "target_profile": metadata.target_profile,
        "code_cell_manifest": {
            "hash_type": ckb.map(|c| c.declared_type_id_hash_type.as_str()).unwrap_or("type"),
            "capacity_policy": ckb.map(|c| c.capacity_policy_surface.as_str()).unwrap_or("unknown"),
        },
        "dep_group_manifest": ckb.map(|c| serde_json::to_value(&c.dep_group_manifest).unwrap_or(serde_json::Value::Null)),
        "script_references": ckb.map(|c| serde_json::to_value(&c.script_references).unwrap_or(serde_json::Value::Null)),
        "proof_plan_soundness": metadata.runtime.proof_plan_soundness,
        "builder_assumptions": metadata.runtime.builder_assumptions,
    })
}

fn verify_deploy_plan_json(plan: &serde_json::Value) -> Vec<String> {
    let mut violations = Vec::new();
    if plan.get("schema").and_then(serde_json::Value::as_str) != Some("cellscript-deploy-plan-v0.16") {
        violations.push("schema must be cellscript-deploy-plan-v0.16".to_string());
    }
    if plan.pointer("/artifact/format").is_none() {
        violations.push("artifact.format is required".to_string());
    }
    match plan.pointer("/artifact/hash").and_then(serde_json::Value::as_str) {
        Some(hash) if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) => {}
        Some(_) => violations.push("artifact.hash must be a canonical 32-byte lowercase hex hash".to_string()),
        None => violations.push("artifact.hash is required".to_string()),
    }
    match plan.pointer("/artifact/size_bytes").and_then(serde_json::Value::as_u64) {
        Some(size) if size > 0 => {}
        Some(_) => violations.push("artifact.size_bytes must be greater than zero".to_string()),
        None => violations.push("artifact.size_bytes is required".to_string()),
    }
    match plan.get("metadata_schema_version").and_then(serde_json::Value::as_u64) {
        Some(version) if version > 0 => {}
        Some(_) => violations.push("metadata_schema_version must be greater than zero".to_string()),
        None => violations.push("metadata_schema_version is required".to_string()),
    }
    for field in ["metadata", "source", "artifact", "constraints"] {
        match plan.pointer(&format!("/metadata_schema_versions/{field}")).and_then(serde_json::Value::as_u64) {
            Some(version) if version > 0 => {}
            Some(_) => violations.push(format!("metadata_schema_versions.{field} must be greater than zero")),
            None => violations.push(format!("metadata_schema_versions.{field} is required")),
        }
    }
    if plan.get("target_profile").is_none() {
        violations.push("target_profile is required".to_string());
    }
    match plan.pointer("/proof_plan_soundness/status").and_then(serde_json::Value::as_str) {
        Some("passed") => {}
        Some(status) => violations.push(format!("proof_plan_soundness.status must be passed, got {status}")),
        None => violations.push("proof_plan_soundness.status is required".to_string()),
    }
    if plan.get("builder_assumptions").is_none() {
        violations.push("builder_assumptions is required".to_string());
    }
    violations
}

fn dependency_lock_json(metadata: &CompileMetadata) -> serde_json::Value {
    let ckb = metadata.constraints.ckb.as_ref();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-dependency-lock-v0.16",
        "module": metadata.module,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "artifact_hash": metadata.artifact_hash,
        "cell_deps": ckb.map(|c| serde_json::to_value(&c.dep_group_manifest.declared_cell_deps).unwrap_or(serde_json::Value::Null)),
        "script_references": ckb.map(|c| serde_json::to_value(&c.script_references).unwrap_or(serde_json::Value::Null)),
    })
}

fn proof_diff_report(old: &CompileMetadata, new: &CompileMetadata) -> serde_json::Value {
    let old_map = proof_plan_map(&old.runtime.proof_plan);
    let new_map = proof_plan_map(&new.runtime.proof_plan);
    let old_keys = old_map.keys().cloned().collect::<BTreeSet<_>>();
    let new_keys = new_map.keys().cloned().collect::<BTreeSet<_>>();
    let added = new_keys.difference(&old_keys).cloned().collect::<Vec<_>>();
    let removed = old_keys.difference(&new_keys).cloned().collect::<Vec<_>>();
    let changed = old_keys.intersection(&new_keys).filter(|key| old_map.get(*key) != new_map.get(*key)).cloned().collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-proof-diff-v0.16",
        "old_module": old.module,
        "new_module": new.module,
        "added": added,
        "removed": removed,
        "changed": changed,
    })
}

fn proof_plan_map(plans: &[ProofPlanMetadata]) -> BTreeMap<String, serde_json::Value> {
    plans
        .iter()
        .map(|plan| {
            (
                format!("{}:{}:{}", plan.origin, plan.feature, plan.status),
                serde_json::json!({
                    "trigger": plan.trigger,
                    "scope": plan.scope,
                    "reads": plan.reads,
                    "coverage": plan.coverage,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                    "on_chain_checked": plan.on_chain_checked,
                }),
            )
        })
        .collect()
}

fn json_diff_report(kind: &str, old: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let changed = [
        "/artifact/hash",
        "/artifact/size_bytes",
        "/target_profile/name",
        "/proof_plan_soundness/status",
        "/metadata_schema_version",
        "/metadata_schema_versions/metadata",
        "/metadata_schema_versions/source",
        "/metadata_schema_versions/artifact",
        "/metadata_schema_versions/constraints",
    ]
    .iter()
    .filter(|pointer| old.pointer(pointer) != new.pointer(pointer))
    .map(|pointer| {
        serde_json::json!({
            "path": pointer,
            "old": old.pointer(pointer).cloned().unwrap_or(serde_json::Value::Null),
            "new": new.pointer(pointer).cloned().unwrap_or(serde_json::Value::Null),
        })
    })
    .collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": format!("cellscript-{}-diff-v0.16", kind),
        "changed": changed,
    })
}

fn profile_report_json(metadata: &CompileMetadata, entry: Option<&str>) -> serde_json::Value {
    let mut proof_plan_records = Vec::new();
    let actions = metadata
        .actions
        .iter()
        .filter(|action| entry.is_none_or(|entry| action.name == entry))
        .map(|action| {
            proof_plan_records.extend(action.proof_plan.iter().map(|plan| {
                profile_proof_plan_record_json(
                    "action",
                    &action.name,
                    serde_json::json!(action.estimated_cycles),
                    action.ckb_runtime_accesses.len(),
                    plan,
                )
            }));
            serde_json::json!({
                "kind": "action",
                "name": action.name,
                "estimated_cycles": action.estimated_cycles,
                "proof_plan_records": action.proof_plan.len(),
                "runtime_accesses": action.ckb_runtime_accesses.len(),
            })
        })
        .collect::<Vec<_>>();
    let locks = metadata
        .locks
        .iter()
        .filter(|lock| entry.is_none_or(|entry| lock.name == entry))
        .map(|lock| {
            proof_plan_records.extend(lock.proof_plan.iter().map(|plan| {
                profile_proof_plan_record_json("lock", &lock.name, serde_json::Value::Null, lock.ckb_runtime_accesses.len(), plan)
            }));
            serde_json::json!({
                "kind": "lock",
                "name": lock.name,
                "estimated_cycles": null,
                "proof_plan_records": lock.proof_plan.len(),
                "runtime_accesses": lock.ckb_runtime_accesses.len(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-profile-v0.16",
        "module": metadata.module,
        "entry": entry,
        "actions": actions,
        "locks": locks,
        "proof_plan_records": proof_plan_records,
        "proof_plan_soundness": metadata.runtime.proof_plan_soundness,
    })
}

fn profile_proof_plan_record_json(
    entry_kind: &str,
    entry_name: &str,
    estimated_cycles: serde_json::Value,
    runtime_accesses: usize,
    plan: &ProofPlanMetadata,
) -> serde_json::Value {
    serde_json::json!({
        "entry_kind": entry_kind,
        "entry_name": entry_name,
        "name": plan.name,
        "origin": plan.origin,
        "category": plan.category,
        "feature": plan.feature,
        "trigger": plan.trigger,
        "scope": plan.scope,
        "reads": plan.reads,
        "coverage": plan.coverage,
        "codegen_coverage_status": plan.codegen_coverage_status,
        "on_chain_checked": plan.on_chain_checked,
        "status": plan.status,
        "estimated_cycles": estimated_cycles,
        "runtime_accesses": runtime_accesses,
        "builder_assumptions": plan.builder_assumptions,
        "detail": plan.detail,
    })
}

fn trace_tx_report_json(metadata: &CompileMetadata, validation: &crate::TxValidationReport) -> serde_json::Value {
    serde_json::json!({
        "status": validation.status,
        "schema": "cellscript-tx-trace-v0.16",
        "module": metadata.module,
        "steps": metadata.runtime.builder_assumptions.iter().map(|assumption| {
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "checked": validation.checked_assumptions.contains(&assumption.assumption_id),
            })
        }).collect::<Vec<_>>(),
        "validation": validation,
    })
}

fn audit_bundle_json(metadata: &CompileMetadata) -> serde_json::Value {
    // Source-to-codegen mapping: link ProofPlan records to source spans, IR effects, and codegen coverage
    let source_to_codegen = metadata
        .runtime
        .proof_plan
        .iter()
        .map(|plan| {
            serde_json::json!({
                "origin": plan.origin,
                "feature": plan.feature,
                "status": plan.status,
                "source_span": plan.source_span.as_ref().map(|span| serde_json::json!({
                    "start": span.start,
                    "end": span.end,
                    "line": span.line,
                    "column": span.column,
                })).unwrap_or(serde_json::Value::Null),
                "trigger": plan.trigger,
                "scope": plan.scope,
                "codegen_coverage_status": plan.codegen_coverage_status,
                "on_chain_checked": plan.on_chain_checked,
                "ir_effect_class": match plan.category.as_str() {
                    "cell-access" => "cell-read-write",
                    "transaction-invariant" => "transaction-scan",
                    "declared-invariant" => "metadata-only-invariant",
                    "aggregate-invariant" => "aggregate-check",
                    "pool-primitive" => "pool-operation",
                    _ => "unknown",
                },
                "reads": plan.reads,
                "coverage": plan.coverage,
                "builder_assumptions": plan.builder_assumptions,
                "diagnostics": plan.diagnostics.iter().map(|diag| serde_json::json!({
                    "severity": diag.severity,
                    "message": diag.message,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    // Action-level source-to-IR-to-codegen trace
    let action_traces = metadata
        .actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "name": action.name,
                "estimated_cycles": action.estimated_cycles,
                "proof_plan_records": action.proof_plan.len(),
                "proof_plan_source_mappings": action.proof_plan.iter().map(|plan| serde_json::json!({
                    "origin": plan.origin,
                    "feature": plan.feature,
                    "source_span": plan.source_span,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                })).collect::<Vec<_>>(),
                "runtime_accesses": action.ckb_runtime_accesses.iter().map(|access| serde_json::json!({
                    "source": access.source,
                    "operation": access.operation,
                    "index": access.index,
                    "binding": access.binding,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    // Lock-level source-to-codegen trace
    let lock_traces = metadata
        .locks
        .iter()
        .map(|lock| {
            serde_json::json!({
                "name": lock.name,
                "proof_plan_records": lock.proof_plan.len(),
                "proof_plan_source_mappings": lock.proof_plan.iter().map(|plan| serde_json::json!({
                    "origin": plan.origin,
                    "feature": plan.feature,
                    "source_span": plan.source_span,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                })).collect::<Vec<_>>(),
                "runtime_accesses": lock.ckb_runtime_accesses.iter().map(|access| serde_json::json!({
                    "source": access.source,
                    "operation": access.operation,
                    "index": access.index,
                    "binding": access.binding,
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-audit-bundle-v0.16",
        "module": metadata.module,
        "compiler_version": metadata.compiler_version,
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "target_profile": metadata.target_profile,
        "source_to_codegen": source_to_codegen,
        "proof_plan": metadata.runtime.proof_plan,
        "proof_plan_soundness": metadata.runtime.proof_plan_soundness,
        "builder_assumptions": metadata.runtime.builder_assumptions,
        "constraints": metadata.constraints,
        "actions": action_traces,
        "locks": lock_traces,
        "source_units": metadata.source_units,
        "lowering": metadata.lowering,
        "debug_info_sections": metadata.debug_info_sections,
    })
}

fn audit_bundle_html(bundle: &serde_json::Value) -> String {
    let module = bundle.get("module").and_then(serde_json::Value::as_str).unwrap_or("unknown");
    let status = bundle.pointer("/proof_plan_soundness/status").and_then(serde_json::Value::as_str).unwrap_or("unknown");
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>CellScript Audit Bundle</title>\
         <h1>CellScript Audit Bundle</h1><p>Module: {}</p><p>ProofPlan soundness: {}</p>\
         <pre>{}</pre>",
        module,
        status,
        serde_json::to_string_pretty(bundle).unwrap_or_else(|_| "{}".to_string())
    )
}

fn proof_plan_summary_json(proof_plan: &[ProofPlanMetadata]) -> serde_json::Value {
    let record_count = proof_plan.len();
    let on_chain_checked_count = proof_plan.iter().filter(|plan| plan.on_chain_checked).count();
    let runtime_required_count = proof_plan.iter().filter(|plan| plan.status == "runtime-required").count();
    let metadata_only_gap_count = proof_plan.iter().filter(|plan| plan.codegen_coverage_status == "gap:metadata-only").count();
    let fail_closed_count =
        proof_plan.iter().filter(|plan| plan.status == "fail-closed" || plan.codegen_coverage_status == "fail-closed").count();
    let diagnostic_error_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "error").count();
    let diagnostic_warning_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "warning").count();
    let macro_provenance_count =
        proof_plan.iter().flat_map(|plan| &plan.coverage).filter(|coverage| coverage.starts_with("macro_expansion:")).count();
    let has_runtime_required_gaps = proof_plan.iter().any(|plan| plan.status == "runtime-required" && !plan.on_chain_checked);
    let has_fail_closed_gaps = fail_closed_count > 0;

    serde_json::json!({
        "record_count": record_count,
        "on_chain_checked_count": on_chain_checked_count,
        "runtime_required_count": runtime_required_count,
        "metadata_only_gap_count": metadata_only_gap_count,
        "fail_closed_count": fail_closed_count,
        "diagnostic_error_count": diagnostic_error_count,
        "diagnostic_warning_count": diagnostic_warning_count,
        "macro_provenance_count": macro_provenance_count,
        "has_runtime_required_gaps": has_runtime_required_gaps,
        "has_fail_closed_gaps": has_fail_closed_gaps,
        "has_blocking_diagnostics": has_runtime_required_gaps || has_fail_closed_gaps || diagnostic_error_count > 0,
    })
}

fn print_proof_plan_summary(proof_plan: &[ProofPlanMetadata]) {
    let summary = proof_plan_summary_json(proof_plan);
    println!("  Summary:");
    println!("    records: {}", summary["record_count"]);
    println!("    on_chain_checked: {}", summary["on_chain_checked_count"]);
    println!("    runtime_required: {}", summary["runtime_required_count"]);
    println!("    metadata_only_gaps: {}", summary["metadata_only_gap_count"]);
    println!("    fail_closed: {}", summary["fail_closed_count"]);
    println!("    diagnostic_errors: {}", summary["diagnostic_error_count"]);
    println!("    diagnostic_warnings: {}", summary["diagnostic_warning_count"]);
    println!("    macro_provenance_records: {}", summary["macro_provenance_count"]);
}

fn print_proof_plan_record(plan: &ProofPlanMetadata) {
    let coverage_notes = plan.coverage.iter().filter(|coverage| !coverage.starts_with("macro_expansion:")).collect::<Vec<_>>();
    let macro_provenance = plan.coverage.iter().filter(|coverage| coverage.starts_with("macro_expansion:")).collect::<Vec<_>>();

    println!();
    println!("constraint: {}", plan.name);
    println!("  origin: {}", plan.origin);
    println!("  trigger: {}", plan.trigger);
    println!("  scope: {}", plan.scope);
    println!("  reads:");
    if plan.reads.is_empty() {
        println!("    - none");
    } else {
        for read in &plan.reads {
            println!("    - {}", proof_plan_read_label(read));
        }
    }
    println!("  coverage:");
    if coverage_notes.is_empty() {
        println!("    - none");
    } else {
        for coverage in coverage_notes {
            println!("    - {}", coverage);
        }
    }
    if !macro_provenance.is_empty() {
        println!("  macro_provenance:");
        for provenance in macro_provenance {
            println!("    - {}", provenance);
        }
    }
    println!("  relation_checks:");
    if plan.input_output_relation_checks.is_empty() {
        println!("    - none");
    } else {
        for check in &plan.input_output_relation_checks {
            println!("    - {}", check);
        }
    }
    println!("  on_chain_checked: {}", if plan.on_chain_checked { "yes" } else { "no" });
    println!("  codegen_coverage_status: {}", plan.codegen_coverage_status);
    if !plan.witness_fields.is_empty() {
        println!("  witness_fields:");
        for field in &plan.witness_fields {
            println!("    - {}", field);
        }
    }
    if !plan.lock_args_fields.is_empty() {
        println!("  lock_args_fields:");
        for field in &plan.lock_args_fields {
            println!("    - {}", field);
        }
    }
    println!("  builder_assumption:");
    if plan.builder_assumptions.is_empty() {
        println!("    - none");
    } else {
        for assumption in &plan.builder_assumptions {
            println!("    - {}", assumption);
        }
    }
    for diagnostic in &plan.diagnostics {
        println!("  {}: {}", diagnostic.severity, diagnostic.message);
    }
}

fn proof_plan_read_label(read: &str) -> String {
    match read {
        "input" => "Source::Input".to_string(),
        "output" => "Source::Output".to_string(),
        "group_input" => "Source::GroupInput".to_string(),
        "group_output" => "Source::GroupOutput".to_string(),
        "cell_dep" => "Source::CellDep".to_string(),
        "header_dep" => "Source::HeaderDep".to_string(),
        "witness" => "WitnessArgs".to_string(),
        "lock_args" => "Script.args".to_string(),
        other => other.to_string(),
    }
}

fn dirs_config_dir() -> PathBuf {
    if let Ok(config) = std::env::var("CELLSCRIPT_CONFIG") {
        return PathBuf::from(config);
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("cellscript");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".config").join("cellscript")
}

fn effective_check_args(mut args: CheckArgs) -> Result<CheckArgs> {
    // In a workspace root (virtual manifest without [package]), fall back to default policy.
    let policy = PackageManager::new(".").read_manifest().map(|m| m.policy).unwrap_or_default();
    merge_check_policy(&mut args, &policy);
    Ok(args)
}

fn effective_check_target_profile(args: &CheckArgs) -> Result<TargetProfile> {
    if let Some(profile) = args.target_profile.as_deref() {
        return TargetProfile::from_name(profile);
    }

    if let Some(profile) = manifest_target_profile()? {
        return Ok(profile);
    }

    Ok(TargetProfile::Ckb)
}

fn manifest_target_profile() -> Result<Option<TargetProfile>> {
    let manifest_path = Path::new("Cell.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let source = std::fs::read_to_string(manifest_path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read Cell.toml target profile policy: {}", error))
    })?;
    let manifest: toml::Value = toml::from_str(&source).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse Cell.toml target profile policy: {}", error))
    })?;
    let Some(profile) = manifest.get("build").and_then(|build| build.get("target_profile")).and_then(toml::Value::as_str) else {
        return Ok(None);
    };
    TargetProfile::from_name(profile).map(Some)
}

fn compile_target_profile_for_check(profile: TargetProfile) -> Option<String> {
    match profile {
        TargetProfile::Ckb => Some(TargetProfile::Ckb.name().to_string()),
    }
}

fn display_doc_output_format(format: &OutputFormat) -> &'static str {
    match format {
        OutputFormat::Html => "html",
        OutputFormat::Markdown => "markdown",
        OutputFormat::Json => "json",
    }
}

fn ensure_new_package_destination(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let mut entries = std::fs::read_dir(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to inspect '{}': {}", path.display(), error)))?;
    if entries.next().is_none() {
        return Ok(());
    }

    Err(crate::error::CompileError::without_span(format!("destination '{}' already exists and is not empty", path.display())))
}

fn init_git_repo(path: &Path) -> Result<bool> {
    let output = std::process::Command::new("git").arg("init").arg("--quiet").arg(path).output().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to run git init for '{}': {}", path.display(), error))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::error::CompileError::without_span(format!("git init failed for '{}': {}", path.display(), stderr.trim())));
    }
    Ok(true)
}

fn runtime_error_info_from_query(query: &str) -> Option<CellScriptRuntimeErrorInfo> {
    let trimmed = query.trim().trim_matches('`');
    let numeric = trimmed
        .parse::<u64>()
        .ok()
        .or_else(|| trimmed.strip_prefix('E').or_else(|| trimmed.strip_prefix('e')).and_then(|code| code.parse::<u64>().ok()));

    if let Some(code) = numeric {
        return runtime_error_info_by_code(code);
    }

    ALL_RUNTIME_ERRORS.iter().copied().map(runtime_error_info).find(|info| info.name == trimmed)
}

fn validate_dependency_target_flags(dev: bool, build: bool) -> Result<()> {
    if dev && build {
        return Err(crate::error::CompileError::without_span("dependency target flags --dev and --build are mutually exclusive"));
    }
    Ok(())
}

/// Reject self-dependency writes to the manifest. A package cannot list itself
/// (or a path pointing at its own root) as a dependency because that turns the
/// package graph into an immediate cycle. The empty-name edge case observed in
/// 0.20 ("cellc install --path ." wrote a `[dependencies.""]` row that broke
/// every subsequent `cellc build`) is the canonical failure this helper
/// prevents.
fn validate_not_self_dependency(crate_name: &str, dep: &Dependency, manifest: &crate::package::PackageManifest) -> Result<()> {
    if !crate_name.trim().is_empty() && crate_name == manifest.package.name {
        return Err(crate::error::CompileError::without_span(format!(
            "refusing to add self-dependency: package '{}' cannot depend on itself",
            manifest.package.name
        )));
    }
    if let Dependency::Detailed(detailed) = dep {
        if let Some(dep_path) = &detailed.path {
            let dep_canon = std::path::Path::new(dep_path);
            let manifest_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let dep_abs = dep_canon.canonicalize().unwrap_or_else(|_| manifest_dir.join(dep_canon));
            let manifest_abs = manifest_dir.canonicalize().unwrap_or_else(|_| manifest_dir.clone());
            if dep_abs == manifest_abs {
                return Err(crate::error::CompileError::without_span(format!(
                    "refusing to add self-dependency: path '{}' resolves to the current package root",
                    dep_path
                )));
            }
        }
    }
    Ok(())
}

fn dependency_target_label(dev: bool, build: bool) -> &'static str {
    if build {
        "build-dependencies"
    } else if dev {
        "dev-dependencies"
    } else {
        "dependencies"
    }
}

fn dependency_map_mut(manifest: &mut crate::package::PackageManifest, dev: bool, build: bool) -> &mut HashMap<String, Dependency> {
    if build {
        &mut manifest.build.dependencies
    } else if dev {
        &mut manifest.dev_dependencies
    } else {
        &mut manifest.dependencies
    }
}

fn dependency_from_add_args(args: &AddArgs) -> Dependency {
    match (&args.git, &args.path) {
        (Some(git), _) => Dependency::Detailed(DetailedDependency {
            version: "*".to_string(),
            namespace: None,
            git: Some(git.clone()),
            branch: None,
            tag: None,
            rev: None,
            path: None,
            optional: false,
            features: Vec::new(),
            default_features: true,
        }),
        (_, Some(path)) => Dependency::Detailed(DetailedDependency {
            version: "*".to_string(),
            namespace: None,
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: Some(path.display().to_string()),
            optional: false,
            features: Vec::new(),
            default_features: true,
        }),
        _ => Dependency::Simple("*".to_string()),
    }
}

fn refresh_lockfile_from_manifest(root: &Path) -> Result<()> {
    let mut manager = PackageManager::new(root);
    manager.resolve_dependencies()?;

    let mut lockfile = Lockfile::read_from_root(root)?.unwrap_or_default();
    lockfile.replace_with_resolved(manager.get_resolved());
    lockfile.write_to_root(root)?;
    Ok(())
}

fn refresh_lockfile_from_build(root: &Path, metadata: &CompileMetadata) -> Result<()> {
    let mut manager = PackageManager::new(root);
    let manifest = manager.read_manifest()?;
    manager.resolve_dependencies()?;

    let mut lockfile = Lockfile::read_from_root(root)?.unwrap_or_default();
    let mut package = lockfile_package_info(root, &manifest)?;
    package.compiler_source_hash = metadata.source_hash.clone();
    lockfile.package = package;
    lockfile.replace_with_resolved(manager.get_resolved());
    lockfile.package_build = Some(locked_build_info_from_metadata(metadata)?);
    refresh_lockfile_deployment_refs(root, &mut lockfile);
    lockfile.write_to_root(root)?;
    Ok(())
}

/// Bridge Deployed.toml deployment records into Cell.lock. Without this,
/// `cellc registry verify` would always fail with "deployment for network 'X'
/// is missing from Cell.lock" because nothing in the production build pipeline
/// ever wrote a `lockfile.deployment` entry. We only keep a record if its
/// deployment name + tx_hash + output_index match a real Deployed.toml entry;
/// stale or duplicate networks are dropped. Records that fail the build-identity
/// match (artifact_hash / metadata_hash / etc. mismatch with Cell.lock's locked
/// build) are kept but their hash fields are left None so the registry verifier
/// can surface the mismatch as a violation instead of pretending the deployment
/// is consistent with the locked build.
fn refresh_lockfile_deployment_refs(root: &Path, lockfile: &mut crate::package::Lockfile) {
    let deployed = match crate::package::DeployedManifest::read_from_root(root) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => return,
        Err(_) => return,
    };
    let locked_build = lockfile.package_build.as_ref();
    let mut next: BTreeMap<String, crate::package::LockfileDeploymentRef> = BTreeMap::new();
    for record in deployed.deployments {
        if record.network.trim().is_empty() {
            continue;
        }
        if next.contains_key(&record.network) {
            // First-write wins; later duplicates from Deployed.toml are dropped
            // to keep Cell.lock deterministic for the same source tree.
            continue;
        }
        let artifact_match = match (&record.artifact_hash, locked_build.and_then(|b| b.artifact_hash.as_ref())) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        let record_str = record.out_point.clone();
        let record_hash = if artifact_match { hash_json_value("deployment record", &record).ok() } else { None };
        let code_hash = if artifact_match { Some(record.code_hash.clone()) } else { None };
        let out_point = if artifact_match { Some(record.out_point.clone()) } else { None };
        let data_hash = if artifact_match { Some(record.data_hash.clone()) } else { None };
        next.insert(
            record.network.clone(),
            crate::package::LockfileDeploymentRef { record: record_str, record_hash, code_hash, out_point, data_hash },
        );
    }
    lockfile.deployment = next;
}

fn lockfile_package_info(root: &Path, manifest: &crate::package::PackageManifest) -> Result<crate::package::LockfilePackageInfo> {
    Ok(crate::package::LockfilePackageInfo {
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        namespace: manifest.package.namespace.clone(),
        source_hash: Some(crate::package::registry::compute_source_hash(root)?),
        compiler_source_hash: None,
    })
}

fn locked_build_info_from_metadata(metadata: &CompileMetadata) -> Result<crate::package::LockedBuildInfo> {
    Ok(crate::package::LockedBuildInfo {
        compiler_version: Some(metadata.compiler_version.clone()),
        target_profile: Some(metadata.target_profile.name.clone()),
        artifact_hash: metadata.artifact_hash.clone(),
        metadata_hash: Some(hash_json_value("metadata", metadata)?),
        schema_hash: Some(metadata.molecule_schema_manifest.manifest_hash.clone()),
        cell_data_codec_manifest_hash: Some(metadata.cell_data_codec_manifest.manifest_hash.clone()),
        abi_hash: Some(metadata_abi_hash(metadata)?),
        constraints_hash: Some(hash_json_value("constraints", &metadata.constraints)?),
    })
}

fn metadata_schema_versions_json(metadata: &CompileMetadata) -> serde_json::Value {
    serde_json::json!({
        "metadata": metadata.metadata_schema_version,
        "source": metadata.source_metadata_schema_version,
        "artifact": metadata.artifact_metadata_schema_version,
        "constraints": metadata.constraints_metadata_schema_version,
    })
}

fn metadata_abi_hash(metadata: &CompileMetadata) -> Result<String> {
    let abi = serde_json::json!({
        "metadata_schema_version": metadata.metadata_schema_version,
        "metadata_schema_versions": metadata_schema_versions_json(metadata),
        "target_profile": metadata.target_profile.name.as_str(),
        "types": &metadata.types,
        "actions": &metadata.actions,
        "functions": &metadata.functions,
        "locks": &metadata.locks,
        "molecule_schema_manifest": &metadata.molecule_schema_manifest,
        "cell_data_codec_manifest": &metadata.cell_data_codec_manifest,
    });
    hash_json_value("abi", &abi)
}

fn hash_json_value<T: serde::Serialize>(label: &str, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| crate::error::CompileError::without_span(format!("failed to serialize {} for digest: {}", label, e)))?;
    Ok(crate::hex_encode(&crate::ckb_blake2b256(&bytes)))
}

fn cellfabric_intent_envelope_json(
    metadata: &CompileMetadata,
    action: &crate::ActionMetadata,
    action_plan: &serde_json::Value,
    input_path: &Path,
    metadata_hash: &str,
) -> Result<serde_json::Value> {
    let action_plan_hash = hash_json_value("CellScript action plan", action_plan)?;
    let app_namespace = metadata.module.clone();
    let app_conflict_key_templates = cellfabric_app_conflict_key_templates(&app_namespace, action);
    Ok(serde_json::json!({
        "schema": "cellscript-cellfabric-intent-envelope-v0.20",
        "status": "requires-runtime-binding",
        "bridge_boundary": {
            "kind": "json-bridge",
            "cellscript_core_dependency": "no-cell-fabric-rust-crate",
            "cellfabric_expected_role": "intent-ordering-soft-confirmation-and-settlement-tracking",
            "not_a_cellfabric_signed_intent": true,
            "not_a_soft_confirmation": true,
            "not_l1_finality": true,
            "compiler_must_not_infer_cellfabric_finality": true,
        },
        "source": {
            "input": input_path.display().to_string(),
            "module": metadata.module.clone(),
            "action": action.name.clone(),
            "target_profile": metadata.target_profile.name.clone(),
            "compiler_version": metadata.compiler_version.clone(),
            "metadata_hash": metadata_hash,
            "artifact_hash": &metadata.artifact_hash,
            "action_plan_hash": action_plan_hash.clone(),
        },
        "cellfabric_mapping": {
            "target": "CellFabric IntentBody template",
            "candidate_intent_action": "App",
            "payload_format": "cellscript-action-plan-json-v1",
            "payload_hash_field": "cellscript_action_plan_hash",
            "resource_binding": "runtime-resolved-live-cells",
            "auth_binding": "runtime-wallet-or-live-cell-context",
            "settlement_compiler": "cellscript-ckb-adapter-or-generated-builder",
        },
        "cellfabric_intent_template": {
            "version": 1,
            "domain": {
                "chain_id": metadata.target_profile.name.clone(),
                "app_namespace": app_namespace.clone(),
            },
            "author": {
                "lock_script_hash": serde_json::Value::Null,
                "source": "runtime-wallet-or-live-cell-context",
            },
            "nonce": serde_json::Value::Null,
            "validity": {
                "valid_after_ms": serde_json::Value::Null,
                "valid_until_ms": serde_json::Value::Null,
            },
            "resources": {
                "consumes": [],
                "reads": [],
                "app_keys": app_conflict_key_templates,
                "status": "template-only-runtime-outpoints-required",
            },
            "action": {
                "kind": "App",
                "action": action.name.clone(),
                "payload_format": "cellscript-action-plan-json-v1",
                "payload_hash": action_plan_hash.clone(),
            },
            "constraints": {
                "source": "cellscript-action-plan",
                "runtime_input_requirements": &action.transaction_runtime_input_requirements,
                "verifier_obligations": &action.verifier_obligations,
                "fail_closed_runtime_features": &action.fail_closed_runtime_features,
            },
            "dependencies": {
                "requires": [],
                "source": "service-supplied-cellfabric-intent-ids",
            },
            "replacement": {
                "supersedes": [],
                "rule": "service-policy",
            },
            "fee": {
                "fee_bid_shannons": serde_json::Value::Null,
                "max_fee_shannons": serde_json::Value::Null,
                "source": "runtime-builder-policy",
            },
            "auth_mode": "CoSignConcreteTx",
            "metadata": {
                "cellscript_action": action.name.clone(),
                "cellscript_metadata_hash": metadata_hash,
                "cellscript_action_plan_hash": action_plan_hash.clone(),
                "cellscript_artifact_hash": &metadata.artifact_hash,
            },
        },
        "resource_access_template": {
            "hard_conflicts": {
                "status": "runtime-required",
                "consumed_cell_patterns": &action.consume_set,
                "runtime_input_requirements": &action.transaction_runtime_input_requirements,
                "note": "CellFabric OutPointRef conflicts must be filled from resolved live cells before submitting a SignedIntent.",
            },
            "reads": &action.read_refs,
            "writes": {
                "creates": &action.create_set,
                "mutates": &action.mutate_set,
            },
            "app_conflict_key_templates": cellfabric_app_conflict_key_templates(&app_namespace, action),
        },
        "required_runtime_evidence": [
            "author_lock_script_hash",
            "intent_nonce",
            "resolved_consumed_outpoints",
            "resolved_read_outpoints",
            "cellfabric_auth_signature",
            "deployment_identity",
            "live_cell_resolution",
            "capacity_fee_balance",
            "estimate_cycles",
            "tx_pool_acceptance",
            "l1_status_observation"
        ],
        "non_claims": [
            "does not create a CellFabric SignedIntent",
            "does not prove CellFabric orderer acceptance",
            "does not soft-confirm the action",
            "does not prove live-cell availability",
            "does not prove CKB tx-pool acceptance",
            "does not prove L1 finality"
        ],
        "action_plan": action_plan,
    }))
}

fn cellfabric_app_conflict_key_templates(app_namespace: &str, action: &crate::ActionMetadata) -> Vec<serde_json::Value> {
    let mut keys = BTreeSet::<(String, String)>::new();
    for shared in &action.touches_shared {
        keys.insert(("cellscript-shared-resource".to_string(), shared.clone()));
    }
    for pattern in &action.mutate_set {
        keys.insert(("cellscript-mutate-binding".to_string(), format!("{}:{}", pattern.ty, pattern.binding)));
    }
    for primitive in &action.pool_primitives {
        if let Ok(value) = serde_json::to_value(primitive) {
            keys.insert(("cellscript-pool-primitive".to_string(), value.to_string()));
        }
    }
    keys.into_iter()
        .map(|(key_type, key)| {
            serde_json::json!({
                "namespace": app_namespace,
                "key_type": key_type,
                "key": key,
                "key_encoding": "utf8",
                "key_bytes_hex": crate::hex_encode(key.as_bytes()),
            })
        })
        .collect()
}

fn push_missing_locked_build_identity(label: &str, build: &crate::package::LockedBuildInfo, violations: &mut Vec<String>) {
    if build.compiler_version.is_none() {
        violations.push(format!("{} has no compiler_version", label));
    }
    if build.target_profile.is_none() {
        violations.push(format!("{} has no target_profile", label));
    }
    if build.artifact_hash.is_none() {
        violations.push(format!("{} has no artifact_hash", label));
    }
    if build.metadata_hash.is_none() {
        violations.push(format!("{} has no metadata_hash", label));
    }
    if build.schema_hash.is_none() {
        violations.push(format!("{} has no schema_hash", label));
    }
    if build.cell_data_codec_manifest_hash.is_none() {
        violations.push(format!("{} has no cell_data_codec_manifest_hash", label));
    }
    if build.abi_hash.is_none() {
        violations.push(format!("{} has no abi_hash", label));
    }
    if build.constraints_hash.is_none() {
        violations.push(format!("{} has no constraints_hash", label));
    }
}

fn push_missing_deployed_build_identity(label: &str, build: &crate::package::DeployedBuildInfo, violations: &mut Vec<String>) {
    if build.compiler_version.is_none() {
        violations.push(format!("{} has no compiler_version", label));
    }
    if build.artifact_hash.is_none() {
        violations.push(format!("{} has no artifact_hash", label));
    }
    if build.metadata_hash.is_none() {
        violations.push(format!("{} has no metadata_hash", label));
    }
    if build.schema_hash.is_none() {
        violations.push(format!("{} has no schema_hash", label));
    }
    if build.cell_data_codec_manifest_hash.is_none() {
        violations.push(format!("{} has no cell_data_codec_manifest_hash", label));
    }
    if build.abi_hash.is_none() {
        violations.push(format!("{} has no abi_hash", label));
    }
    if build.constraints_hash.is_none() {
        violations.push(format!("{} has no constraints_hash", label));
    }
}

fn compare_optional_build_field(
    field: &str,
    lock_value: &Option<String>,
    deployed_value: &Option<String>,
    violations: &mut Vec<String>,
) {
    match (lock_value, deployed_value) {
        (Some(lock_value), Some(deployed_value)) if lock_value == deployed_value => {}
        (Some(lock_value), Some(deployed_value)) => {
            violations.push(format!("{} mismatch: Cell.lock has '{}', Deployed.toml has '{}'", field, lock_value, deployed_value))
        }
        (None, _) => violations.push(format!("Cell.lock [package.build] has no {}", field)),
        (_, None) => violations.push(format!("Deployed.toml [build] has no {}", field)),
    }
}

fn verify_live_deployments(
    deployed: &crate::package::DeployedManifest,
    rpc_url: &str,
    network_filter: Option<&str>,
    violations: &mut Vec<String>,
) -> Result<serde_json::Value> {
    let chain_info = ckb_rpc_call(rpc_url, "get_blockchain_info", serde_json::json!([]))?;
    let chain = chain_info.get("chain").or_else(|| chain_info.get("chain_id")).and_then(|value| value.as_str()).map(str::to_string);
    let mut evidence = Vec::new();
    let mut checked = 0usize;

    for deployment in &deployed.deployments {
        if network_filter.is_some_and(|network| network != deployment.network) {
            continue;
        }
        checked += 1;
        let mut deployment_violations = Vec::new();
        if let Some(violation) = deployment_status_violation(deployment) {
            deployment_violations.push(violation);
        }

        match chain.as_deref() {
            Some(chain) if chain_id_matches(chain, &deployment.chain_id) => {}
            Some(chain) => deployment_violations.push(format!(
                "chain_id mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                deployment.network, chain, deployment.chain_id
            )),
            None => deployment_violations.push("RPC get_blockchain_info did not return chain".to_string()),
        }

        let out_point = serde_json::json!({
            "tx_hash": deployment.tx_hash,
            "index": format!("0x{:x}", deployment.output_index),
        });
        let live = ckb_rpc_call(rpc_url, "get_live_cell", serde_json::json!([out_point, true]))?;
        let rpc_status = live.get("status").and_then(|value| value.as_str()).unwrap_or("unknown").to_string();
        if rpc_status != "live" {
            deployment_violations.push(format!(
                "deployment for network '{}' is not live at {}: RPC status '{}'",
                deployment.network, deployment.out_point, rpc_status
            ));
        }

        let rpc_data_hash = live_cell_data_hash(&live);
        match rpc_data_hash.as_deref() {
            Some(hash) if hex_eq(hash, &deployment.data_hash) => {}
            Some(hash) => deployment_violations.push(format!(
                "live data_hash mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                deployment.network, hash, deployment.data_hash
            )),
            None => deployment_violations
                .push(format!("RPC get_live_cell for network '{}' did not return cell.data.hash", deployment.network)),
        }

        let rpc_code_hash =
            live_cell_code_hash_for_deployment(&live, deployment, rpc_data_hash.as_deref(), &mut deployment_violations);
        if let Some(hash) = rpc_code_hash.as_deref() {
            if !hex_eq(hash, &deployment.code_hash) {
                deployment_violations.push(format!(
                    "live code_hash mismatch for network '{}': RPC has '{}', Deployed.toml has '{}'",
                    deployment.network, hash, deployment.code_hash
                ));
            }
        }

        if let Some(type_id) = &deployment.type_id {
            let rpc_type_args = live_cell_type_script(&live).and_then(|script| script.get("args")).and_then(|value| value.as_str());
            match rpc_type_args {
                Some(args) if hex_eq(args, type_id) => {}
                Some(args) => deployment_violations.push(format!(
                    "type_id mismatch for network '{}': RPC type args '{}', Deployed.toml has '{}'",
                    deployment.network, args, type_id
                )),
                None => deployment_violations.push(format!(
                    "deployment for network '{}' declares type_id but live cell has no type script args",
                    deployment.network
                )),
            }
        }

        for violation in &deployment_violations {
            if !violations.contains(violation) {
                violations.push(violation.clone());
            }
        }
        evidence.push(serde_json::json!({
            "network": deployment.network,
            "chain_id": deployment.chain_id,
            "deployment_status": deployment.status.as_ref(),
            "out_point": deployment.out_point,
            "rpc_status": rpc_status,
            "status": if deployment_violations.is_empty() { "live-verified" } else { "failed" },
            "expected_data_hash": deployment.data_hash,
            "rpc_data_hash": rpc_data_hash,
            "expected_code_hash": deployment.code_hash,
            "rpc_code_hash": rpc_code_hash,
            "hash_type": deployment.hash_type,
            "violations": deployment_violations,
        }));
    }

    if checked == 0 {
        violations.push(match network_filter {
            Some(network) => format!("no deployment record found for requested live network '{}'", network),
            None => "no deployment records found for live verification".to_string(),
        });
    }

    Ok(serde_json::json!({
        "enabled": true,
        "rpc_url": rpc_url,
        "network": network_filter,
        "chain": chain,
        "checked": checked,
        "evidence": evidence,
    }))
}

fn live_cell_data_hash(live: &serde_json::Value) -> Option<String> {
    live.pointer("/cell/data/hash").or_else(|| live.pointer("/cell/data_hash")).and_then(|value| value.as_str()).map(str::to_string)
}

fn live_cell_type_script(live: &serde_json::Value) -> Option<&serde_json::Value> {
    let script = live.pointer("/cell/output/type")?;
    (!script.is_null()).then_some(script)
}

fn live_cell_code_hash_for_deployment(
    live: &serde_json::Value,
    deployment: &crate::package::DeploymentRecord,
    rpc_data_hash: Option<&str>,
    violations: &mut Vec<String>,
) -> Option<String> {
    match normalize_hash_type(&deployment.hash_type).as_deref() {
        Some("data" | "data1" | "data2") => rpc_data_hash.map(str::to_string),
        Some("type") => {
            let Some(script) = live_cell_type_script(live) else {
                violations.push(format!(
                    "deployment for network '{}' uses hash_type 'type' but live cell has no type script",
                    deployment.network
                ));
                return None;
            };
            match ckb_script_hash_from_json(script) {
                Ok(hash) => Some(hash),
                Err(error) => {
                    violations
                        .push(format!("failed to compute live type script hash for network '{}': {}", deployment.network, error));
                    None
                }
            }
        }
        Some(other) => {
            violations.push(format!("unsupported deployment hash_type '{}' for live verification", other));
            None
        }
        None => {
            violations.push("deployment hash_type is empty".to_string());
            None
        }
    }
}

fn ckb_script_hash_from_json(script: &serde_json::Value) -> Result<String> {
    let code_hash = script
        .get("code_hash")
        .and_then(|value| value.as_str())
        .ok_or_else(|| crate::error::CompileError::without_span("script has no code_hash"))?;
    let hash_type = script
        .get("hash_type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| crate::error::CompileError::without_span("script has no hash_type"))?;
    let args = script.get("args").and_then(|value| value.as_str()).unwrap_or("0x");

    let code_hash_bytes = hex::decode(code_hash.trim_start_matches("0x"))
        .map_err(|error| crate::error::CompileError::without_span(format!("invalid script code_hash: {}", error)))?;
    if code_hash_bytes.len() != 32 {
        return Err(crate::error::CompileError::without_span(format!(
            "script code_hash must be 32 bytes, got {}",
            code_hash_bytes.len()
        )));
    }
    let hash_type_byte = ckb_hash_type_byte(hash_type)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("unsupported script hash_type '{}'", hash_type)))?;
    let args_bytes = hex::decode(args.trim_start_matches("0x"))
        .map_err(|error| crate::error::CompileError::without_span(format!("invalid script args: {}", error)))?;

    let mut args_molecule = Vec::with_capacity(4 + args_bytes.len());
    args_molecule.extend_from_slice(&(args_bytes.len() as u32).to_le_bytes());
    args_molecule.extend_from_slice(&args_bytes);

    let header_size = 4 + 4 * 3;
    let field_sizes = [32usize, 1usize, args_molecule.len()];
    let mut cursor = header_size;
    let mut offsets = Vec::with_capacity(3);
    for size in field_sizes {
        offsets.push(cursor);
        cursor += size;
    }

    let mut serialized = Vec::with_capacity(cursor);
    serialized.extend_from_slice(&(cursor as u32).to_le_bytes());
    for offset in offsets {
        serialized.extend_from_slice(&(offset as u32).to_le_bytes());
    }
    serialized.extend_from_slice(&code_hash_bytes);
    serialized.push(hash_type_byte);
    serialized.extend_from_slice(&args_molecule);

    Ok(format!("0x{}", crate::hex_encode(&crate::ckb_blake2b256(&serialized))))
}

fn ckb_rpc_call(rpc_url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let endpoint = parse_http_rpc_url(rpc_url)?;
    let body = serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    }))
    .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON-RPC request: {}", error)))?;

    let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to connect to CKB RPC '{}': {}", rpc_url, error)))?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint.path,
        endpoint.host_header,
        body.len(),
        body
    );
    stream.write_all(request.as_bytes()).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to write CKB RPC request '{}': {}", method, error))
    })?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read CKB RPC response '{}': {}", method, error))
    })?;
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return Err(crate::error::CompileError::without_span("CKB RPC returned malformed HTTP response"));
    };
    let status_line = headers.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        return Err(crate::error::CompileError::without_span(format!("CKB RPC '{}' returned HTTP status '{}'", method, status_line)));
    }
    let value: serde_json::Value = serde_json::from_str(body).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse CKB RPC response '{}': {}", method, error))
    })?;
    if let Some(error) = value.get("error") {
        return Err(crate::error::CompileError::without_span(format!("CKB RPC '{}' failed: {}", method, error)));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| crate::error::CompileError::without_span(format!("CKB RPC '{}' returned no result", method)))
}

struct HttpRpcEndpoint {
    host: String,
    host_header: String,
    port: u16,
    path: String,
}

fn parse_http_rpc_url(url: &str) -> Result<HttpRpcEndpoint> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| crate::error::CompileError::without_span("only http:// CKB RPC URLs are supported"))?;
    let (host_port, path) = rest.split_once('/').map_or((rest, "/"), |(host_port, path)| (host_port, path));
    let path = if path == "/" { "/".to_string() } else { format!("/{path}") };
    let (host, port) = if let Some((host, port)) = host_port.rsplit_once(':') {
        let port = port
            .parse::<u16>()
            .map_err(|error| crate::error::CompileError::without_span(format!("invalid CKB RPC port '{}': {}", port, error)))?;
        (host.to_string(), port)
    } else {
        (host_port.to_string(), 80)
    };
    if host.is_empty() {
        return Err(crate::error::CompileError::without_span("CKB RPC host is empty"));
    }
    Ok(HttpRpcEndpoint { host, host_header: host_port.to_string(), port, path })
}

fn chain_id_matches(rpc_chain: &str, expected: &str) -> bool {
    let rpc = normalize_chain_id(rpc_chain);
    let expected = normalize_chain_id(expected);
    rpc == expected || (rpc == "ckb" && expected == "ckb-mainnet") || (rpc == "ckb-mainnet" && expected == "ckb")
}

fn normalize_chain_id(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

fn hex_eq(left: &str, right: &str) -> bool {
    left.trim_start_matches("0x").eq_ignore_ascii_case(right.trim_start_matches("0x"))
}

fn normalize_hash_type(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()).then_some(value)
}

fn ckb_hash_type_byte(value: &str) -> Option<u8> {
    match normalize_hash_type(value)?.as_str() {
        "data" => Some(0),
        "type" => Some(1),
        "data1" => Some(2),
        "data2" => Some(4),
        _ => None,
    }
}

fn effective_build_check_args(args: &BuildArgs) -> Result<CheckArgs> {
    effective_check_args(CheckArgs {
        all_targets: false,
        target_profile: args.target_profile.clone(),
        features: args.features.clone(),
        json: false,
        production: args.production,
        deny_fail_closed: args.deny_fail_closed,
        deny_ckb_runtime: args.deny_ckb_runtime,
        deny_runtime_obligations: args.deny_runtime_obligations,
        primitive_compat: args.primitive_compat.clone(),
        package: None,
        workspace: false,
    })
}

fn merge_check_policy(args: &mut CheckArgs, policy: &PolicyConfig) {
    args.production |= policy.production;
    args.deny_fail_closed |= policy.deny_fail_closed;
    args.deny_ckb_runtime |= policy.deny_ckb_runtime;
    args.deny_runtime_obligations |= policy.deny_runtime_obligations;
}

fn validate_expected_metadata_hash(field: &str, actual: Option<&str>, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err(crate::error::CompileError::without_span(format!(
            "{} expectation must be a 64-character lowercase CKB Blake2b hex digest, got '{}'",
            field, expected
        )));
    }
    match actual {
        Some(actual) if actual.eq_ignore_ascii_case(expected) => Ok(()),
        Some(actual) => Err(crate::error::CompileError::without_span(format!(
            "metadata {} '{}' does not match expected '{}'",
            field, actual, expected
        ))),
        None => Err(crate::error::CompileError::without_span(format!(
            "metadata is missing {} required by expectation '{}'",
            field, expected
        ))),
    }
}

fn validate_expected_target_profile(actual: &str, expected: Option<&str>) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let expected_profile = TargetProfile::from_name(expected)?;
    if actual == expected_profile.name() {
        return Ok(());
    }

    Err(crate::error::CompileError::without_span(format!(
        "metadata target_profile '{}' does not match expected '{}'",
        actual,
        expected_profile.name()
    )))
}

fn validate_check_policy(metadata: &crate::CompileMetadata, args: &CheckArgs) -> Result<()> {
    let mut violations = Vec::new();

    if args.primitive_compat.as_deref() == Some("0.16") {
        if let Err(error) = crate::proof_plan::soundness::validate_metadata(metadata, true) {
            violations.push(error.message);
        }
    } else if matches!(args.primitive_compat.as_deref(), Some("0.17" | "0.18")) {
        if let Err(error) = crate::validate_primitive_strict_017_metadata(metadata) {
            violations.push(error.message);
        }
    } else if metadata.runtime.proof_plan_soundness.status == "failed" {
        violations.push(format!("ProofPlan soundness failed: {} issue(s)", metadata.runtime.proof_plan_soundness.issue_count));
    }

    if args.production || args.deny_fail_closed {
        if !metadata.constraints.failures.is_empty() {
            violations.push(format!("constraints failures: {}", metadata.constraints.failures.join(", ")));
        }

        if !metadata.runtime.fail_closed_runtime_features.is_empty() {
            violations.push(format!("fail-closed runtime features: {}", metadata.runtime.fail_closed_runtime_features.join(", ")));
        }

        let fail_closed_obligations = metadata
            .runtime
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.status == "fail-closed")
            .map(|obligation| format!("{}:{} ({})", obligation.scope, obligation.feature, obligation.category))
            .collect::<Vec<_>>();
        if !fail_closed_obligations.is_empty() {
            violations.push(format!("fail-closed verifier obligations: {}", fail_closed_obligations.join(", ")));
        }
    }

    if args.deny_ckb_runtime && metadata.runtime.ckb_runtime_required {
        violations.push(format!("CKB runtime features: {}", metadata.runtime.ckb_runtime_features.join(", ")));
    }

    if args.deny_runtime_obligations {
        let runtime_required_obligations = metadata
            .runtime
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.status == "runtime-required")
            .map(|obligation| format!("{}:{} ({})", obligation.scope, obligation.feature, obligation.category))
            .collect::<Vec<_>>();
        if !runtime_required_obligations.is_empty() {
            violations.push(format!("runtime-required verifier obligations: {}", runtime_required_obligations.join(", ")));
        }

        let runtime_required_proof_plan = metadata
            .runtime
            .proof_plan
            .iter()
            .filter(|plan| plan.status == "runtime-required" && !plan.on_chain_checked)
            .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
            .collect::<Vec<_>>();
        if !runtime_required_proof_plan.is_empty() {
            violations.push(format!("runtime-required ProofPlan gaps: {}", runtime_required_proof_plan.join(", ")));
        }

        let transaction_invariants = transaction_invariant_checked_subcondition_summaries(metadata);
        if !transaction_invariants.is_empty() {
            violations.push(format!(
                "runtime-required transaction invariants with checked subconditions: {}",
                transaction_invariants.join(", ")
            ));
        }

        let transaction_runtime_inputs = transaction_runtime_input_requirement_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_inputs.is_empty() {
            violations
                .push(format!("runtime-required transaction runtime input requirements: {}", transaction_runtime_inputs.join(", ")));
        }

        let transaction_runtime_input_blockers = transaction_runtime_input_blocker_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_input_blockers.is_empty() {
            violations.push(format!(
                "runtime-required transaction runtime input blockers: {}",
                transaction_runtime_input_blockers.join(", ")
            ));
        }

        let transaction_runtime_input_blocker_classes =
            transaction_runtime_input_blocker_class_summaries_by_status(metadata, "runtime-required");
        if !transaction_runtime_input_blocker_classes.is_empty() {
            violations.push(format!(
                "runtime-required transaction runtime input blocker classes: {}",
                transaction_runtime_input_blocker_classes.join(", ")
            ));
        }

        let runtime_required_pool_invariants = pool_invariant_family_summaries(metadata, "runtime-required");
        if !runtime_required_pool_invariants.is_empty() {
            violations.push(format!("runtime-required Pool invariant families: {}", runtime_required_pool_invariants.join(", ")));
        }

        let runtime_required_pool_blocker_classes = pool_invariant_family_blocker_class_summaries(metadata, "runtime-required");
        if !runtime_required_pool_blocker_classes.is_empty() {
            violations.push(format!(
                "runtime-required Pool invariant blocker classes: {}",
                runtime_required_pool_blocker_classes.join(", ")
            ));
        }

        let pool_runtime_inputs = pool_runtime_input_requirement_summaries(metadata);
        if !pool_runtime_inputs.is_empty() {
            violations.push(format!("runtime-required Pool runtime input requirements: {}", pool_runtime_inputs.join(", ")));
        }
    }

    if violations.is_empty() {
        return Ok(());
    }

    Err(crate::error::CompileError::without_span(format!("check policy failed:\n  - {}", violations.join("\n  - "))))
}

fn target_profile_policy_violations(
    metadata: &crate::CompileMetadata,
    artifact_format: ArtifactFormat,
    profile: TargetProfile,
) -> Vec<String> {
    match profile {
        TargetProfile::Ckb => ckb_target_profile_policy_violations(metadata, artifact_format),
    }
}

fn ckb_target_profile_policy_violations(_metadata: &crate::CompileMetadata, _artifact_format: ArtifactFormat) -> Vec<String> {
    Vec::new()
}

fn runtime_required_obligation_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.verifier_obligations.iter().filter(|obligation| obligation.status == "runtime-required").count()
}

fn fail_closed_obligation_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.verifier_obligations.iter().filter(|obligation| obligation.status == "fail-closed").count()
}

fn runtime_required_transaction_invariant_count(metadata: &crate::CompileMetadata) -> usize {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .count()
}

fn runtime_required_transaction_invariant_checked_subcondition_count(metadata: &crate::CompileMetadata) -> usize {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .map(|obligation| checked_runtime_subconditions(&obligation.detail).len())
        .sum()
}

fn transaction_invariant_checked_subcondition_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| obligation.category == "transaction-invariant" && obligation.status == "runtime-required")
        .filter_map(|obligation| {
            let subconditions = checked_runtime_subconditions(&obligation.detail);
            if subconditions.is_empty() {
                None
            } else {
                Some(format!("{}:{} checked=[{}]", obligation.scope, obligation.feature, subconditions.join(",")))
            }
        })
        .collect()
}

fn transaction_runtime_input_requirement_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.transaction_runtime_input_requirements.len()
}

fn transaction_runtime_input_requirement_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    metadata.runtime.transaction_runtime_input_requirements.iter().filter(|requirement| requirement.status == status).count()
}

fn transaction_runtime_input_requirement_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata.runtime.transaction_runtime_input_requirements.iter().map(transaction_runtime_input_requirement_summary).collect()
}

fn transaction_runtime_input_requirement_summaries_by_status(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .map(transaction_runtime_input_requirement_summary)
        .collect()
}

fn transaction_runtime_input_blocker_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    transaction_runtime_input_blocker_summaries_by_status(metadata, status).len()
}

fn transaction_runtime_input_blocker_summaries_by_status(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .filter_map(|requirement| {
            requirement.blocker.as_deref().map(|blocker| {
                let blocker_class = requirement
                    .blocker_class
                    .as_deref()
                    .map(|blocker_class| format!(" blocker_class={}", blocker_class))
                    .unwrap_or_default();
                format!("{}:{}:{} blocker={}{}", requirement.scope, requirement.feature, requirement.component, blocker, blocker_class)
            })
        })
        .collect()
}

fn transaction_runtime_input_blocker_class_count_by_status(metadata: &crate::CompileMetadata, status: &str) -> usize {
    transaction_runtime_input_blocker_class_summaries_by_status(metadata, status).len()
}

fn transaction_runtime_input_blocker_class_summaries_by_status(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .transaction_runtime_input_requirements
        .iter()
        .filter(|requirement| requirement.status == status)
        .filter_map(|requirement| {
            requirement.blocker_class.as_deref().map(|blocker_class| {
                format!("{}:{}:{} blocker_class={}", requirement.scope, requirement.feature, requirement.component, blocker_class)
            })
        })
        .collect()
}

fn transaction_runtime_input_requirement_summary(requirement: &crate::TransactionRuntimeInputRequirementMetadata) -> String {
    let field = requirement.field.as_deref().map(|field| format!(".{}", field)).unwrap_or_default();
    let bytes = requirement.byte_len.map(|byte_len| format!("[{}]", byte_len)).unwrap_or_default();
    let blocker = requirement.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
    let blocker_class = requirement.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
    format!(
        "{}:{}:{}={}:{}{}:{}{} ({}){}{}",
        requirement.scope,
        requirement.feature,
        requirement.component,
        requirement.source,
        requirement.binding,
        field,
        requirement.abi,
        bytes,
        requirement.status,
        blocker,
        blocker_class
    )
}

fn checked_runtime_subconditions(detail: &str) -> Vec<String> {
    detail
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .filter_map(|part| part.trim().strip_suffix("=checked-runtime"))
        .map(|name| name.trim_matches(|ch: char| ch == '`' || ch == '.' || ch == ':').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

fn checked_pool_invariant_family_count(metadata: &crate::CompileMetadata) -> usize {
    pool_invariant_family_summaries(metadata, "checked-runtime").len()
}

fn runtime_required_pool_invariant_family_count(metadata: &crate::CompileMetadata) -> usize {
    pool_invariant_family_summaries(metadata, "runtime-required").len()
}

fn pool_runtime_input_requirement_count(metadata: &crate::CompileMetadata) -> usize {
    metadata.runtime.pool_primitives.iter().map(|primitive| primitive.runtime_input_requirements.len()).sum()
}

fn pool_runtime_input_requirement_summaries(metadata: &crate::CompileMetadata) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.runtime_input_requirements.iter().map(move |requirement| {
                let field = requirement.field.as_deref().map(|field| format!(".{}", field)).unwrap_or_default();
                let blocker = requirement.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
                let blocker_class =
                    requirement.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
                format!(
                    "{}:{}:{}={}#{}:{}{}:{}[{}]{}{}",
                    primitive.scope,
                    primitive.feature,
                    requirement.component,
                    requirement.source,
                    requirement.index,
                    requirement.binding,
                    field,
                    requirement.abi,
                    requirement.byte_len,
                    blocker,
                    blocker_class
                )
            })
        })
        .collect()
}

fn pool_invariant_family_summaries(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.invariant_families.iter().filter(move |family| family.status == status).map(move |family| {
                let blocker = family.blocker.as_deref().map(|blocker| format!(" blocker={}", blocker)).unwrap_or_default();
                let blocker_class =
                    family.blocker_class.as_deref().map(|class| format!(" blocker_class={}", class)).unwrap_or_default();
                format!("{}:{}:{} ({}){}{}", primitive.scope, primitive.feature, family.name, family.source, blocker, blocker_class)
            })
        })
        .collect()
}

fn pool_invariant_family_blocker_class_count(metadata: &crate::CompileMetadata, status: &str) -> usize {
    pool_invariant_family_blocker_class_summaries(metadata, status).len()
}

fn pool_invariant_family_blocker_class_summaries(metadata: &crate::CompileMetadata, status: &str) -> Vec<String> {
    metadata
        .runtime
        .pool_primitives
        .iter()
        .flat_map(|primitive| {
            primitive.invariant_families.iter().filter(move |family| family.status == status).filter_map(move |family| {
                family.blocker_class.as_deref().map(|blocker_class| {
                    format!("{}:{}:{} blocker_class={}", primitive.scope, primitive.feature, family.name, blocker_class)
                })
            })
        })
        .collect()
}

#[derive(Debug, Default)]
struct CompileTestExpectation {
    expect_success: bool,
    expect_fail: bool,
    expected_errors: Vec<String>,
    target: Option<String>,
    production: bool,
    deny_fail_closed: bool,
    deny_ckb_runtime: bool,
    deny_runtime_obligations: bool,
    expect_standalone: Option<bool>,
    expect_ckb_runtime: Option<bool>,
    expect_fail_closed: Option<bool>,
    expected_runtime_features: Vec<String>,
    forbidden_runtime_features: Vec<String>,
    expected_verifier_obligations: Vec<String>,
    forbidden_verifier_obligations: Vec<String>,
    expected_runtime_required_obligations: Vec<String>,
    forbidden_runtime_required_obligations: Vec<String>,
    expected_artifact_format: Option<String>,
    expected_actions: Vec<String>,
    forbidden_actions: Vec<String>,
    expected_functions: Vec<String>,
    forbidden_functions: Vec<String>,
    expected_locks: Vec<String>,
    forbidden_locks: Vec<String>,
}

impl CompileTestExpectation {
    fn check_args(&self) -> CheckArgs {
        CheckArgs {
            all_targets: false,
            target_profile: None,
            features: Vec::new(),
            json: false,
            production: self.production,
            deny_fail_closed: self.deny_fail_closed,
            deny_ckb_runtime: self.deny_ckb_runtime,
            deny_runtime_obligations: self.deny_runtime_obligations,
            primitive_compat: None,
            package: None,
            workspace: false,
        }
    }
}

fn read_test_expectation(path: &Path) -> Result<CompileTestExpectation> {
    let source = std::fs::read_to_string(path)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to read test '{}': {}", path.display(), error)))?;
    parse_test_expectation(path, &source)
}

fn parse_test_expectation(path: &Path, source: &str) -> Result<CompileTestExpectation> {
    let mut expectation = CompileTestExpectation::default();
    for (line_number, line) in source.lines().enumerate() {
        let Some(marker) = line.split("//").nth(1).map(str::trim) else {
            continue;
        };
        let Some(directive) = marker.strip_prefix("cellscript-test:").map(str::trim) else {
            continue;
        };

        if directive == "expect-success" {
            expectation.expect_success = true;
        } else if directive == "expect-fail" {
            expectation.expect_fail = true;
        } else if let Some(expected) = directive.strip_prefix("expect-error:").map(str::trim) {
            expectation.expect_fail = true;
            if !expected.is_empty() {
                expectation.expected_errors.push(expected.to_string());
            }
        } else if let Some(target) = directive.strip_prefix("target:").map(str::trim) {
            if target.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "target directive requires a non-empty target"));
            }
            if expectation.target.replace(target.to_string()).is_some() {
                return Err(compile_test_directive_error(path, line_number, "target directive may only appear once"));
            }
        } else if directive == "production" {
            expectation.production = true;
        } else if directive == "deny-fail-closed" {
            expectation.deny_fail_closed = true;
        } else if directive == "deny-ckb-runtime" {
            expectation.deny_ckb_runtime = true;
        } else if directive == "deny-runtime-obligations" {
            expectation.deny_runtime_obligations = true;
        } else if directive == "expect-standalone" {
            expectation.expect_standalone = Some(true);
        } else if directive == "expect-not-standalone" {
            expectation.expect_standalone = Some(false);
        } else if directive == "expect-ckb-runtime" {
            expectation.expect_ckb_runtime = Some(true);
        } else if directive == "expect-no-ckb-runtime" {
            expectation.expect_ckb_runtime = Some(false);
        } else if directive == "expect-fail-closed-runtime" {
            expectation.expect_fail_closed = Some(true);
        } else if directive == "expect-no-fail-closed-runtime" {
            expectation.expect_fail_closed = Some(false);
        } else if let Some(feature) = directive.strip_prefix("expect-runtime-feature:").map(str::trim) {
            if feature.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-runtime-feature requires non-empty text"));
            }
            expectation.expected_runtime_features.push(feature.to_string());
        } else if let Some(feature) = directive.strip_prefix("expect-no-runtime-feature:").map(str::trim) {
            if feature.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-no-runtime-feature requires non-empty text"));
            }
            expectation.forbidden_runtime_features.push(feature.to_string());
        } else if let Some(obligation) = directive.strip_prefix("expect-verifier-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-verifier-obligation",
                obligation,
                &mut expectation.expected_verifier_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-no-verifier-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-no-verifier-obligation",
                obligation,
                &mut expectation.forbidden_verifier_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-runtime-required-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-runtime-required-obligation",
                obligation,
                &mut expectation.expected_runtime_required_obligations,
            )?;
        } else if let Some(obligation) = directive.strip_prefix("expect-no-runtime-required-obligation:").map(str::trim) {
            push_non_empty_test_directive(
                path,
                line_number,
                "expect-no-runtime-required-obligation",
                obligation,
                &mut expectation.forbidden_runtime_required_obligations,
            )?;
        } else if let Some(format) = directive.strip_prefix("expect-artifact-format:").map(str::trim) {
            if format.is_empty() {
                return Err(compile_test_directive_error(path, line_number, "expect-artifact-format requires non-empty text"));
            }
            if expectation.expected_artifact_format.replace(format.to_string()).is_some() {
                return Err(compile_test_directive_error(path, line_number, "expect-artifact-format may only appear once"));
            }
        } else if let Some(name) = directive.strip_prefix("expect-action:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-action", name, &mut expectation.expected_actions)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-action:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-action", name, &mut expectation.forbidden_actions)?;
        } else if let Some(name) = directive.strip_prefix("expect-function:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-function", name, &mut expectation.expected_functions)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-function:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-function", name, &mut expectation.forbidden_functions)?;
        } else if let Some(name) = directive.strip_prefix("expect-lock:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-lock", name, &mut expectation.expected_locks)?;
        } else if let Some(name) = directive.strip_prefix("expect-no-lock:").map(str::trim) {
            push_non_empty_test_directive(path, line_number, "expect-no-lock", name, &mut expectation.forbidden_locks)?;
        } else {
            return Err(compile_test_directive_error(
                path,
                line_number,
                &format!("unknown cellscript-test directive '{}'", directive),
            ));
        }
    }
    if expectation.expect_success && expectation.expect_fail {
        return Err(crate::error::CompileError::without_span(format!(
            "{}: conflicting cellscript-test directives: expect-success cannot be combined with expect-fail/expect-error",
            path.display()
        )));
    }
    Ok(expectation)
}

fn push_non_empty_test_directive(
    path: &Path,
    zero_based_line: usize,
    directive: &str,
    value: &str,
    values: &mut Vec<String>,
) -> Result<()> {
    if value.is_empty() {
        return Err(compile_test_directive_error(path, zero_based_line, &format!("{} requires non-empty text", directive)));
    }
    values.push(value.to_string());
    Ok(())
}

fn compile_test_directive_error(path: &Path, zero_based_line: usize, message: &str) -> crate::error::CompileError {
    crate::error::CompileError::without_span(format!("{}:{}: {}", path.display(), zero_based_line + 1, message))
}

fn evaluate_compile_test_result(
    path: &Utf8Path,
    expectation: &CompileTestExpectation,
    result: Result<crate::CompileResult>,
) -> Result<()> {
    match (expectation.expect_fail, result) {
        (false, Ok(result)) => validate_compile_test_metadata(path, expectation, &result.metadata),
        (false, Err(error)) => {
            Err(crate::error::CompileError::without_span(format!("{}: expected compile success, got error: {}", path, error)))
        }
        (true, Ok(_)) => Err(crate::error::CompileError::without_span(format!("{}: expected compile failure, got success", path))),
        (true, Err(error)) => {
            let message = error.to_string();
            let missing = expectation
                .expected_errors
                .iter()
                .filter(|expected| !message.contains(expected.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if missing.is_empty() {
                Ok(())
            } else {
                Err(crate::error::CompileError::without_span(format!(
                    "{}: expected error text not found: {}; actual error: {}",
                    path,
                    missing.join(", "),
                    message
                )))
            }
        }
    }
}

fn validate_compile_test_metadata(
    path: &Utf8Path,
    expectation: &CompileTestExpectation,
    metadata: &crate::CompileMetadata,
) -> Result<()> {
    if let Some(expected) = &expectation.expected_artifact_format {
        if &metadata.artifact_format != expected {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected artifact_format='{}', got '{}'",
                path, expected, metadata.artifact_format
            )));
        }
    }

    if let Some(expected) = expectation.expect_standalone {
        if metadata.runtime.standalone_runner_compatible != expected {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected standalone_runner_compatible={}, got {}",
                path, expected, metadata.runtime.standalone_runner_compatible
            )));
        }
    }
    if let Some(expected) = expectation.expect_ckb_runtime {
        if metadata.runtime.ckb_runtime_required != expected {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected ckb_runtime_required={}, got {}",
                path, expected, metadata.runtime.ckb_runtime_required
            )));
        }
    }
    if let Some(expected) = expectation.expect_fail_closed {
        let actual = !metadata.runtime.fail_closed_runtime_features.is_empty()
            || metadata.runtime.verifier_obligations.iter().any(|obligation| obligation.status == "fail-closed");
        if actual != expected {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected fail_closed_runtime={}, got {}",
                path, expected, actual
            )));
        }
    }

    let runtime_summary = compile_test_runtime_summary(metadata);
    for expected in &expectation.expected_runtime_features {
        if !runtime_summary.contains(expected) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected runtime metadata to contain '{}'",
                path, expected
            )));
        }
    }
    for forbidden in &expectation.forbidden_runtime_features {
        if runtime_summary.contains(forbidden) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected runtime metadata not to contain '{}'",
                path, forbidden
            )));
        }
    }

    validate_compile_test_summary_contains(
        path,
        "verifier obligation",
        &compile_test_obligation_summary(metadata, None),
        &expectation.expected_verifier_obligations,
        &expectation.forbidden_verifier_obligations,
    )?;
    validate_compile_test_summary_contains(
        path,
        "runtime-required verifier obligation",
        &compile_test_obligation_summary(metadata, Some("runtime-required")),
        &expectation.expected_runtime_required_obligations,
        &expectation.forbidden_runtime_required_obligations,
    )?;

    validate_named_metadata_set(
        path,
        "action",
        &metadata.actions.iter().map(|action| action.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_actions,
        &expectation.forbidden_actions,
    )?;
    validate_named_metadata_set(
        path,
        "function",
        &metadata.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_functions,
        &expectation.forbidden_functions,
    )?;
    validate_named_metadata_set(
        path,
        "lock",
        &metadata.locks.iter().map(|lock| lock.name.as_str()).collect::<Vec<_>>(),
        &expectation.expected_locks,
        &expectation.forbidden_locks,
    )?;

    Ok(())
}

fn validate_compile_test_summary_contains(
    path: &Utf8Path,
    label: &str,
    summary: &str,
    expected: &[String],
    forbidden: &[String],
) -> Result<()> {
    for expected in expected {
        if !summary.contains(expected) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata to contain '{}'",
                path, label, expected
            )));
        }
    }
    for forbidden in forbidden {
        if summary.contains(forbidden) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata not to contain '{}'",
                path, label, forbidden
            )));
        }
    }
    Ok(())
}

fn validate_named_metadata_set(path: &Utf8Path, kind: &str, actual: &[&str], expected: &[String], forbidden: &[String]) -> Result<()> {
    for name in expected {
        if !actual.iter().any(|actual_name| actual_name == name) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata to contain '{}'",
                path, kind, name
            )));
        }
    }
    for name in forbidden {
        if actual.iter().any(|actual_name| actual_name == name) {
            return Err(crate::error::CompileError::without_span(format!(
                "{}: expected {} metadata not to contain '{}'",
                path, kind, name
            )));
        }
    }
    Ok(())
}

fn compile_test_runtime_summary(metadata: &crate::CompileMetadata) -> String {
    let mut values = Vec::new();
    values.extend(metadata.runtime.ckb_runtime_features.iter().cloned());
    values.extend(metadata.runtime.fail_closed_runtime_features.iter().cloned());
    for access in &metadata.runtime.ckb_runtime_accesses {
        values.push(format!("{}:{}:{}:{}:{}", access.operation, access.syscall, access.source, access.index, access.binding));
    }
    for obligation in &metadata.runtime.verifier_obligations {
        values.push(format!(
            "{}:{}:{}:{}:{}",
            obligation.scope, obligation.category, obligation.feature, obligation.status, obligation.detail
        ));
    }
    values.join("\n")
}

fn compile_test_obligation_summary(metadata: &crate::CompileMetadata, status: Option<&str>) -> String {
    metadata
        .runtime
        .verifier_obligations
        .iter()
        .filter(|obligation| match status {
            Some(status) => obligation.status == status,
            None => true,
        })
        .map(|obligation| {
            format!("{}:{}:{}:{}:{}", obligation.scope, obligation.category, obligation.feature, obligation.status, obligation.detail)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collect_cell_files(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    if root.is_file() {
        return Ok(if root.extension().and_then(|ext| ext.to_str()) == Some("cell") { vec![root.to_path_buf()] } else { Vec::new() });
    }

    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("cell") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[cfg(feature = "vm-runner")]
fn run_elf_in_ckb_vm(program: &[u8], args: &[Vec<u8>]) -> Result<u64> {
    let core_machine =
        <<CliVmMachine as DefaultMachineRunner>::Inner as SupportMachine>::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 10_000_000);
    let builder = DefaultMachineBuilder::new(core_machine).instruction_cycle_func(Box::new(estimate_cycles));
    let mut machine = CliVmMachine::new(builder.build());
    let program = Bytes::copy_from_slice(crate::strip_vm_abi_trailer(program));
    let args = args.iter().cloned().map(Bytes::from).map(Ok);

    machine
        .load_program(&program, args)
        .map_err(|error| crate::error::CompileError::without_span(format!("cellc run failed to load ELF: {}", error)))?;
    let exit_code =
        machine.run().map_err(|error| crate::error::CompileError::without_span(format!("cellc run VM error: {}", error)))?;
    if exit_code != 0 {
        return Err(crate::error::CompileError::without_span(format!("cellc run exited with code {}", exit_code)));
    }

    Ok(machine.machine.cycles())
}

struct SelectedEntryWitnessMetadata<'a> {
    kind: &'static str,
    name: &'a str,
    params: &'a [ParamMetadata],
    runtime_bound_param_names: std::collections::BTreeSet<String>,
}

fn select_entry_witness_metadata<'a>(
    metadata: &'a CompileMetadata,
    action: Option<&str>,
    lock: Option<&str>,
) -> Result<SelectedEntryWitnessMetadata<'a>> {
    if let Some(name) = action {
        let action = metadata
            .actions
            .iter()
            .find(|candidate| candidate.name == name)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", name)))?;
        return Ok(SelectedEntryWitnessMetadata {
            kind: "action",
            name: action.name.as_str(),
            params: &action.params,
            runtime_bound_param_names: action
                .consume_set
                .iter()
                .map(|pattern| pattern.binding.clone())
                .chain(action.read_refs.iter().map(|pattern| pattern.binding.clone()))
                .chain(action.mutate_set.iter().map(|pattern| pattern.binding.clone()))
                .collect(),
        });
    }
    if let Some(name) = lock {
        let lock = metadata
            .locks
            .iter()
            .find(|candidate| candidate.name == name)
            .ok_or_else(|| crate::error::CompileError::without_span(format!("lock '{}' was not found in metadata", name)))?;
        return Ok(SelectedEntryWitnessMetadata {
            kind: "lock",
            name: lock.name.as_str(),
            params: &lock.params,
            runtime_bound_param_names: lock
                .consume_set
                .iter()
                .map(|pattern| pattern.binding.clone())
                .chain(lock.read_refs.iter().map(|pattern| pattern.binding.clone()))
                .chain(lock.mutate_set.iter().map(|pattern| pattern.binding.clone()))
                .collect(),
        });
    }

    let mut entries = metadata
        .actions
        .iter()
        .filter(|action| !action.params.is_empty())
        .map(|action| SelectedEntryWitnessMetadata {
            kind: "action",
            name: action.name.as_str(),
            params: action.params.as_slice(),
            runtime_bound_param_names: action
                .consume_set
                .iter()
                .map(|pattern| pattern.binding.clone())
                .chain(action.read_refs.iter().map(|pattern| pattern.binding.clone()))
                .chain(action.mutate_set.iter().map(|pattern| pattern.binding.clone()))
                .collect(),
        })
        .chain(metadata.locks.iter().filter(|lock| !lock.params.is_empty()).map(|lock| {
            SelectedEntryWitnessMetadata {
                kind: "lock",
                name: lock.name.as_str(),
                params: lock.params.as_slice(),
                runtime_bound_param_names: lock
                    .consume_set
                    .iter()
                    .map(|pattern| pattern.binding.clone())
                    .chain(lock.read_refs.iter().map(|pattern| pattern.binding.clone()))
                    .chain(lock.mutate_set.iter().map(|pattern| pattern.binding.clone()))
                    .collect(),
            }
        }))
        .collect::<Vec<_>>();

    match entries.len() {
        1 => Ok(entries.remove(0)),
        0 => Err(crate::error::CompileError::without_span(
            "no parameterized action or lock found; specify --action or --lock for explicit selection",
        )),
        _ => Err(crate::error::CompileError::without_span(
            "multiple parameterized actions/locks found; specify --action NAME or --lock NAME",
        )),
    }
}

fn parse_entry_witness_arg(param: &ParamMetadata, value: &str) -> Result<EntryWitnessArg> {
    if param.schema_pointer_abi || param.schema_length_abi {
        return decode_hex_arg(&param.name, value, None).map(EntryWitnessArg::Bytes);
    }

    if let Some(width) = param.fixed_byte_len {
        return parse_entry_witness_fixed_arg(param, value, width);
    }

    match param.ty.as_str() {
        "bool" => parse_bool_arg(&param.name, value).map(EntryWitnessArg::Bool),
        "u8" => parse_integer_arg(&param.name, value, u8::MAX as u128).map(|value| EntryWitnessArg::U8(value as u8)),
        "u16" => parse_integer_arg(&param.name, value, u16::MAX as u128).map(|value| EntryWitnessArg::U16(value as u16)),
        "u32" => parse_integer_arg(&param.name, value, u32::MAX as u128).map(|value| EntryWitnessArg::U32(value as u32)),
        "u64" => parse_integer_arg(&param.name, value, u64::MAX as u128).map(|value| EntryWitnessArg::U64(value as u64)),
        "()" => Ok(EntryWitnessArg::Unit),
        other => {
            let Some(width) = crate::entry_witness_static_type_len(other).filter(|width| (1..=8).contains(width)) else {
                return Err(crate::error::CompileError::without_span(format!(
                    "parameter '{}' has unsupported entry witness CLI type '{}'",
                    param.name, param.ty
                )));
            };
            decode_hex_arg(&param.name, value, Some(width)).map(EntryWitnessArg::Bytes)
        }
    }
}

fn parse_entry_witness_fixed_arg(param: &ParamMetadata, value: &str, width: usize) -> Result<EntryWitnessArg> {
    match param.ty.as_str() {
        "u128" if width == 16 => parse_integer_arg(&param.name, value, u128::MAX).map(EntryWitnessArg::U128),
        "Address" if width == 32 => {
            let bytes = decode_hex_arg(&param.name, value, Some(32))?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                crate::error::CompileError::without_span(format!("parameter '{}' expects exactly 32 hex bytes", param.name))
            })?;
            Ok(EntryWitnessArg::Address(bytes))
        }
        "Hash" if width == 32 => {
            let bytes = decode_hex_arg(&param.name, value, Some(32))?;
            let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
                crate::error::CompileError::without_span(format!("parameter '{}' expects exactly 32 hex bytes", param.name))
            })?;
            Ok(EntryWitnessArg::Hash(bytes))
        }
        _ => decode_hex_arg(&param.name, value, Some(width)).map(EntryWitnessArg::Bytes),
    }
}

fn parse_bool_arg(name: &str, value: &str) -> Result<bool> {
    match value.trim() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        other => Err(crate::error::CompileError::without_span(format!(
            "parameter '{}' expects bool value true/false/1/0, got '{}'",
            name, other
        ))),
    }
}

fn parse_integer_arg(name: &str, value: &str, max: u128) -> Result<u128> {
    let trimmed = value.trim();
    let parsed = if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<u128>()
    }
    .map_err(|error| crate::error::CompileError::without_span(format!("parameter '{}' expects integer: {}", name, error)))?;
    if parsed > max {
        return Err(crate::error::CompileError::without_span(format!(
            "parameter '{}' integer value {} is out of range",
            name, parsed
        )));
    }
    Ok(parsed)
}

fn decode_hex_arg(name: &str, value: &str, expected_len: Option<usize>) -> Result<Vec<u8>> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("hex:")
        .or_else(|| trimmed.strip_prefix("HEX:"))
        .or_else(|| trimmed.strip_prefix("0x"))
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    if !hex.len().is_multiple_of(2) {
        return Err(crate::error::CompileError::without_span(format!("parameter '{}' hex value must contain full bytes", name)));
    }
    let bytes = hex
        .as_bytes()
        .chunks_exact(2)
        .enumerate()
        .map(|(pair_index, pair)| {
            let offset = pair_index * 2;
            let high = hex_nibble(pair[0]).ok_or_else(|| invalid_hex_arg_error(name, offset))?;
            let low = hex_nibble(pair[1]).ok_or_else(|| invalid_hex_arg_error(name, offset))?;
            Ok((high << 4) | low)
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(expected_len) = expected_len {
        if bytes.len() != expected_len {
            return Err(crate::error::CompileError::without_span(format!(
                "parameter '{}' expects {} byte(s), got {}",
                name,
                expected_len,
                bytes.len()
            )));
        }
    }
    Ok(bytes)
}

fn invalid_hex_arg_error(name: &str, offset: usize) -> crate::error::CompileError {
    crate::error::CompileError::without_span(format!(
        "parameter '{}' has invalid hex byte at offset {}: invalid digit found in string",
        name, offset
    ))
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub struct CliParser;

impl CliParser {
    pub fn parse() -> Command {
        use clap::{Arg, ArgAction, Command as ClapCommand};

        let matches = ClapCommand::new("cellc")
            .version(crate::VERSION)
            .about("CellScript compiler for CKB blockchain")
            .subcommand_required(true)
            .arg_required_else_help(true)
            .subcommand(
                ClapCommand::new("build")
                    .about("Compile the current package")
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Build in release mode"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(
                        Arg::new("entry-action")
                            .long("entry-action")
                            .value_name("ACTION")
                            .help("Compile only this action as the artifact entrypoint"),
                    )
                    .arg(
                        Arg::new("entry-lock")
                            .long("entry-lock")
                            .value_name("LOCK")
                            .conflicts_with("entry-action")
                            .help("Compile only this lock as the artifact entrypoint"),
                    )
                    .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").help("Number of parallel jobs"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON build summary"))
                    .arg(
                        Arg::new("production")
                            .long("production")
                            .action(ArgAction::SetTrue)
                            .help("Reject generated fail-closed runtime paths before writing artifacts"),
                    )
                    .arg(
                        Arg::new("deny-fail-closed").long("deny-fail-closed").action(ArgAction::SetTrue).help(
                            "Reject metadata that contains fail-closed runtime features or obligations before writing artifacts",
                        ),
                    )
                    .arg(
                        Arg::new("deny-ckb-runtime")
                            .long("deny-ckb-runtime")
                            .action(ArgAction::SetTrue)
                            .help("Reject CKB transaction/syscall runtime requirements before writing artifacts"),
                    )
                    .arg(
                        Arg::new("deny-runtime-obligations")
                            .long("deny-runtime-obligations")
                            .action(ArgAction::SetTrue)
                            .help("Reject runtime-required verifier obligations before writing artifacts"),
                    )
                    .arg(
                        Arg::new("primitive-compat")
                            .long("primitive-compat")
                            .value_name("VERSION")
                            .conflicts_with("primitive-strict")
                            .help("Accept primitive syntax from a previous version (e.g. 0.14) with migration hints"),
                    )
                    .arg(
                        Arg::new("primitive-strict")
                            .long("primitive-strict")
                            .value_name("VERSION")
                            .conflicts_with("primitive-compat")
                            .help("Require primitive syntax from a specific version (e.g. 0.15, 0.16, 0.17, or 0.18), reject legacy forms"),
                    )
                    .arg(
                        Arg::new("package")
                            .long("package")
                            .short('p')
                            .value_name("NAME")
                            .help("Build a specific workspace member"),
                    )
                    .arg(
                        Arg::new("workspace")
                            .long("workspace")
                            .action(ArgAction::SetTrue)
                            .help("Build all workspace members"),
                    ),
            )
            .subcommand(
                ClapCommand::new("test")
                    .about("Run the tests")
                    .arg(Arg::new("filter").value_name("FILTER").help("Filter tests by name"))
                    .arg(
                        Arg::new("no-run")
                            .long("no-run")
                            .action(ArgAction::SetTrue)
                            .help("Compile tests without attempting execution"),
                    )
                    .arg(Arg::new("nocapture").long("nocapture").action(ArgAction::SetTrue).help("Don't capture stdout"))
                    .arg(Arg::new("fail-fast").long("fail-fast").action(ArgAction::SetTrue).help("Stop on first failure"))
                    .arg(Arg::new("doc").long("doc").action(ArgAction::SetTrue).help("Generate docs before compiling tests"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON test summary")),
            )
            .subcommand(
                ClapCommand::new("doc")
                    .about("Generate documentation")
                    .arg(Arg::new("open").long("open").short('o').action(ArgAction::SetTrue).help("Open docs in browser"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON doc summary"))
                    .arg(
                        Arg::new("format")
                            .long("format")
                            .value_name("FORMAT")
                            .default_value("html")
                            .help("Output format: html, markdown, json"),
                    ),
            )
            .subcommand(
                ClapCommand::new("fmt")
                    .about("Format source code")
                    .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("Check formatting without modifying files"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON format summary"))
                    .arg(Arg::new("files").value_name("FILES").num_args(1..).help("Files to format")),
            )
            .subcommand(
                ClapCommand::new("init")
                    .about("Create a new package")
                    .arg(Arg::new("name").value_name("NAME").help("Package name"))
                    .arg(Arg::new("path").value_name("PATH").help("Path to create package"))
                    .arg(Arg::new("lib").long("lib").action(ArgAction::SetTrue).help("Create a library package"))
                    .arg(Arg::new("namespace").long("namespace").value_name("NAMESPACE").help("Package namespace for registry publishing"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON init summary")),
            )
            .subcommand(
                ClapCommand::new("new")
                    .about("Create a new package directory")
                    .arg(Arg::new("name").value_name("NAME").required(true).help("Package name"))
                    .arg(Arg::new("path").long("path").value_name("PATH").help("Path to create package"))
                    .arg(Arg::new("lib").long("lib").action(ArgAction::SetTrue).help("Create a library package"))
                    .arg(
                        Arg::new("vcs")
                            .long("vcs")
                            .value_name("VCS")
                            .default_value("git")
                            .value_parser(["git", "none"])
                            .help("Initialize version control: git or none"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON new summary")),
            )
            .subcommand(
                ClapCommand::new("add")
                    .about("Add dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to add"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Add as dev dependency"))
                    .arg(Arg::new("build").long("build").action(ArgAction::SetTrue).help("Add as build dependency"))
                    .arg(Arg::new("git").long("git").value_name("URL").help("Add a git dependency source"))
                    .arg(Arg::new("path").long("path").value_name("PATH").help("Add a local path dependency source"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON add summary")),
            )
            .subcommand(
                ClapCommand::new("clean")
                    .about("Remove build artifacts")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON clean summary"))
                    .arg(Arg::new("cache").long("cache").action(ArgAction::SetTrue).help("Also remove incremental compilation cache (.cell/build/cache)")),
            )
            .subcommand(
                ClapCommand::new("remove")
                    .about("Remove dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to remove"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Remove from dev dependency section"))
                    .arg(Arg::new("build").long("build").action(ArgAction::SetTrue).help("Remove from build dependency section"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON remove summary")),
            )
            .subcommand(ClapCommand::new("repl").about("Start interactive REPL"))
            .subcommand(
                ClapCommand::new("check")
                    .about("Type-check and lower the current package without writing artifacts")
                    .arg(
                        Arg::new("all-targets")
                            .long("all-targets")
                            .action(ArgAction::SetTrue)
                            .help("Also check the current ELF-compatible target path"),
                    )
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON check summary"))
                    .arg(
                        Arg::new("production")
                            .long("production")
                            .action(ArgAction::SetTrue)
                            .help("Reject generated fail-closed runtime paths"),
                    )
                    .arg(
                        Arg::new("deny-fail-closed")
                            .long("deny-fail-closed")
                            .action(ArgAction::SetTrue)
                            .help("Reject metadata that contains fail-closed runtime features or obligations"),
                    )
                    .arg(
                        Arg::new("deny-ckb-runtime")
                            .long("deny-ckb-runtime")
                            .action(ArgAction::SetTrue)
                            .help("Reject CKB transaction/syscall runtime requirements"),
                    )
                    .arg(
                        Arg::new("deny-runtime-obligations")
                            .long("deny-runtime-obligations")
                            .action(ArgAction::SetTrue)
                            .help("Reject runtime-required verifier obligations"),
                    )
                    .arg(
                        Arg::new("primitive-compat")
                            .long("primitive-compat")
                            .value_name("VERSION")
                            .conflicts_with("primitive-strict")
                            .help("Accept primitive syntax from a previous version (e.g. 0.14) with migration hints"),
                    )
                    .arg(
                        Arg::new("primitive-strict")
                            .long("primitive-strict")
                            .value_name("VERSION")
                            .conflicts_with("primitive-compat")
                            .help("Require primitive syntax from a specific version (e.g. 0.15, 0.16, 0.17, or 0.18), reject legacy forms"),
                    )
                    .arg(
                        Arg::new("package")
                            .long("package")
                            .short('p')
                            .value_name("NAME")
                            .help("Check a specific workspace member"),
                    )
                    .arg(
                        Arg::new("workspace")
                            .long("workspace")
                            .action(ArgAction::SetTrue)
                            .help("Check all workspace members"),
                    ),
            )
            .subcommand(
                ClapCommand::new("metadata")
                    .about("Emit compile metadata for lowering, scheduler, and CKB runtime auditing")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON metadata to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb")),
            )
            .subcommand(
                ClapCommand::new("constraints")
                    .about("Emit profile-aware production constraints for compiler, builder, CI, and acceptance gates")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON constraints to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(
                        Arg::new("entry-action")
                            .long("entry-action")
                            .value_name("ACTION")
                            .help("Report constraints for this action entry"),
                    )
                    .arg(Arg::new("entry-lock").long("entry-lock").value_name("LOCK").help("Report constraints for this lock entry")),
            )
            .subcommand(
                ClapCommand::new("abi")
                    .about("Explain the generated _cellscript_entry witness ABI for an action or lock")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON ABI report to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("action").long("action").value_name("NAME").help("Explain ABI for this action"))
                    .arg(Arg::new("lock").long("lock").value_name("NAME").help("Explain ABI for this lock")),
            )
            .subcommand(
                ClapCommand::new("scheduler-plan")
                    .about("Consume scheduler hints and emit a CKB admission/conflict policy report")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON scheduler plan to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb")),
            )
            .subcommand(
                ClapCommand::new("ckb-hash")
                    .about("Compute CKB default Blake2b-256 hashes for builders, manifests, and release evidence")
                    .arg(Arg::new("input").value_name("TEXT").help("UTF-8 text to hash; omitted input hashes empty bytes"))
                    .arg(Arg::new("hex").long("hex").value_name("HEX").help("Hex bytes to hash"))
                    .arg(Arg::new("file").long("file").value_name("FILE").help("File bytes to hash"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON summary")),
            )
            .subcommand(
                ClapCommand::new("ckb-std-compat")
                    .about("Report the ckb-std ABI compatibility boundary for CellScript's inline CKB backend")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON report")),
            )
            .subcommand(
                ClapCommand::new("explain")
                    .about("Explain a CellScript runtime error code")
                    .arg(Arg::new("code").value_name("CODE").required(true).help("Runtime error code, E-code, or error name"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON explanation")),
            )
            .subcommand(
                ClapCommand::new("explain-profile")
                    .about("Explain a CellScript target profile semantic contract")
                    .arg(Arg::new("profile").value_name("PROFILE").required(true).help("Target profile name, e.g. ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON explanation")),
            )
            .subcommand(
                ClapCommand::new("explain-proof")
                    .about("Explain Covenant ProofPlan trigger, scope, reads, coverage, and on-chain status")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON ProofPlan")),
            )
            .subcommand(
                ClapCommand::new("explain-assumptions")
                    .about("Explain v0.16 builder assumptions derived from ProofPlan metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("explain-generics")
                    .about("Explain checked bounded generic collection instantiations")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON explanation")),
            )
            .subcommand(
                ClapCommand::new("opt-report")
                    .about("Compile O0..O3 and emit artifact-size/constraints comparison evidence")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(
                        Arg::new("output")
                            .long("output")
                            .short('o')
                            .value_name("FILE")
                            .help("Write JSON optimization report to a file"),
                    )
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb")),
            )
            .subcommand(
                ClapCommand::new("proof-diff")
                    .about("Diff ProofPlan semantics between two metadata files")
                    .arg(Arg::new("old").value_name("OLD_METADATA").required(true))
                    .arg(Arg::new("new").value_name("NEW_METADATA").required(true))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("profile")
                    .about("Emit v0.16 cycle/profile summary per action, lock, and ProofPlan record")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("entry").long("entry").value_name("NAME").help("Limit profile to one action or lock"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("trace-tx")
                    .about("Trace a transaction JSON against v0.16 builder assumptions")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("audit-bundle")
                    .about("Generate a v0.16 audit bundle linking metadata, ProofPlan, assumptions, and profile data")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("DIR").help("Output directory"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("validate-tx")
                    .about("Validate a transaction JSON against v0.16 builder assumptions before signing")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("solve-tx")
                    .about("Emit a deterministic v0.16 transaction template from metadata and builder assumptions")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON solver template"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("verify-ckb-fixtures")
                    .about("Verify standard CKB compatibility fixtures with the deterministic model runner")
                    .arg(Arg::new("manifest").value_name("MANIFEST_JSON").required(true).help("CKB fixture manifest JSON"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("deploy-plan")
                    .about("Emit a reproducible v0.16 deployment plan")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON deploy plan"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("verify-deploy")
                    .about("Verify a v0.16 deployment plan schema and local integrity fields")
                    .arg(Arg::new("plan").value_name("DEPLOY_PLAN").required(true))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("diff-deploy")
                    .about("Diff two v0.16 deployment plans")
                    .arg(Arg::new("old").value_name("OLD_DEPLOY_PLAN").required(true))
                    .arg(Arg::new("new").value_name("NEW_DEPLOY_PLAN").required(true))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("lock-deps")
                    .about("Emit a v0.16 dependency lock from deployment metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write dependency lock JSON"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("action").about("Plan and explain action-level transaction builder inputs").subcommand(
                    ClapCommand::new("build")
                        .about("Emit a builder plan for a CellScript action without signing or submitting a transaction")
                        .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                        .arg(Arg::new("action").long("action").value_name("NAME").help("Action to plan; defaults to the first action"))
                        .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON builder plan to a file"))
                        .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                        .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                        .arg(
                            Arg::new("fabric-intent")
                                .long("fabric-intent")
                                .action(ArgAction::SetTrue)
                                .help("Emit a CellFabric intent envelope instead of the raw CellScript action plan"),
                        )
                        .arg(
                            Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON builder plan"),
                        ),
                ),
            )
            .subcommand(
                ClapCommand::new("gen-builder")
                    .about("Generate a registry-bound action builder package from CellScript metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(
                        Arg::new("metadata")
                            .long("metadata")
                            .value_name("FILE")
                            .help("Read compile metadata JSON instead of compiling INPUT"),
                    )
                    .arg(
                        Arg::new("lockfile")
                            .long("lockfile")
                            .value_name("FILE")
                            .help("Verify generated builder identity against Cell.lock before writing"),
                    )
                    .arg(
                        Arg::new("deployed")
                            .long("deployed")
                            .value_name("FILE")
                            .help("Verify generated builder deployment identity against Deployed.toml before writing"),
                    )
                    .arg(
                        Arg::new("deployment-network")
                            .long("deployment-network")
                            .value_name("NAME")
                            .help("Verify and embed only this deployment network when using --deployed"),
                    )
                    .arg(
                        Arg::new("target")
                            .long("target")
                            .value_name("TARGET")
                            .required(true)
                            .value_parser(["typescript"])
                            .help("Generated builder target"),
                    )
                    .arg(Arg::new("action").long("action").value_name("NAME").help("Generate only this action; defaults to all actions"))
                    .arg(Arg::new("output").long("output").short('o').value_name("DIR").help("Output package directory"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile when compiling INPUT: ckb"))
                    .arg(Arg::new("package-name").long("package-name").value_name("NAME").help("Generated package.json name"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable generation summary")),
            )
            .subcommand(
                ClapCommand::new("entry-witness")
                    .about("Encode witness bytes for the generated _cellscript_entry wrapper")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("action").long("action").value_name("NAME").help("Encode witness bytes for this action"))
                    .arg(Arg::new("lock").long("lock").value_name("NAME").help("Encode witness bytes for this lock"))
                    .arg(
                        Arg::new("arg")
                            .long("arg")
                            .value_name("VALUE")
                            .num_args(1)
                            .action(ArgAction::Append)
                            .help("Witness payload argument; schema-backed params are omitted, byte params use hex"),
                    )
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write raw witness bytes to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON summary")),
            )
            .subcommand(
                ClapCommand::new("verify-artifact")
                    .about("Verify an emitted CellScript artifact against its metadata sidecar")
                    .arg(Arg::new("artifact").value_name("ARTIFACT").required(true).help("Artifact file to verify"))
                    .arg(
                        Arg::new("metadata")
                            .long("metadata")
                            .short('m')
                            .value_name("FILE")
                            .help("Metadata JSON file; defaults to ARTIFACT.meta.json"),
                    )
                    .arg(
                        Arg::new("verify-sources")
                            .long("verify-sources")
                            .action(ArgAction::SetTrue)
                            .help("Also verify metadata source_units against files on disk"),
                    )
                    .arg(
                        Arg::new("json")
                            .long("json")
                            .action(ArgAction::SetTrue)
                            .help("Emit a machine-readable JSON verification summary"),
                    )
                    .arg(
                        Arg::new("expect-target-profile")
                            .long("expect-target-profile")
                            .value_name("PROFILE")
                            .help("Require metadata target_profile to match this value: ckb"),
                    )
                    .arg(
                        Arg::new("expect-artifact-hash")
                            .long("expect-artifact-hash")
                            .value_name("HASH")
                            .help("Require metadata artifact_hash to match this value"),
                    )
                    .arg(
                        Arg::new("expect-source-hash")
                            .long("expect-source-hash")
                            .value_name("HASH")
                            .help("Require metadata source_hash to match this path-bound value"),
                    )
                    .arg(
                        Arg::new("expect-source-content-hash")
                            .long("expect-source-content-hash")
                            .value_name("HASH")
                            .help("Require metadata source_content_hash to match this path-independent value"),
                    )
                    .arg(
                        Arg::new("production")
                            .long("production")
                            .action(ArgAction::SetTrue)
                            .help("Reject fail-closed runtime paths in emitted metadata"),
                    )
                    .arg(
                        Arg::new("deny-fail-closed")
                            .long("deny-fail-closed")
                            .action(ArgAction::SetTrue)
                            .help("Reject metadata that contains fail-closed runtime features or obligations"),
                    )
                    .arg(
                        Arg::new("deny-ckb-runtime")
                            .long("deny-ckb-runtime")
                            .action(ArgAction::SetTrue)
                            .help("Reject CKB transaction/syscall runtime requirements"),
                    )
                    .arg(
                        Arg::new("deny-runtime-obligations")
                            .long("deny-runtime-obligations")
                            .action(ArgAction::SetTrue)
                            .help("Reject runtime-required verifier obligations"),
                    )
                    .arg(
                        Arg::new("primitive-compat")
                            .long("primitive-compat")
                            .value_name("VERSION")
                            .conflicts_with("primitive-strict")
                            .help("Accept primitive syntax from a previous version (e.g. 0.14) with migration hints"),
                    )
                    .arg(
                        Arg::new("primitive-strict")
                            .long("primitive-strict")
                            .value_name("VERSION")
                            .conflicts_with("primitive-compat")
                            .help("Require primitive syntax from a specific version (e.g. 0.15, 0.16, 0.17, or 0.18), reject legacy forms"),
                    ),
            )
            .subcommand(
                ClapCommand::new("run")
                    .about("Experimental: build and run a package")
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Run in release mode"))
                    .arg(
                        Arg::new("simulate")
                            .long("simulate")
                            .short('s')
                            .action(ArgAction::SetTrue)
                            .help("Simulate execution using AST interpreter instead of ckb-vm"),
                    )
                    .arg(Arg::new("args").value_name("ARGS").num_args(0..).trailing_var_arg(true)),
            )
            .subcommand(
                ClapCommand::new("publish")
                    .about("Experimental: publish a package")
                    .arg(Arg::new("dry-run").long("dry-run").action(ArgAction::SetTrue))
                    .arg(Arg::new("allow-dirty").long("allow-dirty").action(ArgAction::SetTrue)),
            )
            .subcommand(
                ClapCommand::new("install")
                    .about("Install a package from registry, git, or path")
                    .arg(Arg::new("crate").value_name("CRATE"))
                    .arg(Arg::new("version").long("version").value_name("VERSION"))
                    .arg(Arg::new("namespace").long("namespace").value_name("NAMESPACE").help("Package namespace for registry install"))
                    .arg(Arg::new("git").long("git").value_name("URL"))
                    .arg(Arg::new("path").long("path").value_name("PATH")),
            )
            .subcommand(ClapCommand::new("update").about("Experimental: update dependencies"))
            .subcommand(
                ClapCommand::new("info")
                    .about("Show package information")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable package information")),
            )
            .subcommand(
                ClapCommand::new("login")
                    .about("Experimental: authenticate against a registry")
                    .arg(Arg::new("registry").long("registry").value_name("URL")),
            )
            .subcommand(
                ClapCommand::new("certify")
                    .about("Run a deterministic compiler-hosted certification plugin (currently: novaseal-profile-v0)")
                    .arg(
                        Arg::new("plugin")
                            .long("plugin")
                            .value_name("PLUGIN")
                            .required(true)
                            .help("Certification plugin id, e.g. novaseal-profile-v0"),
                    )
                    .arg(
                        Arg::new("repo-root")
                            .long("repo-root")
                            .value_name("DIR")
                            .help("Repository root for Rust certification evidence"),
                    )
                    .arg(
                        Arg::new("report")
                            .long("report")
                            .value_name("JSON")
                            .help("Verify an existing plugin report instead of regenerating it"),
                    )
                    .arg(
                        Arg::new("output")
                            .long("output")
                            .short('o')
                            .value_name("FILE")
                            .help("Write compiler certification report JSON"),
                    )
                    .arg(
                        Arg::new("require-production")
                            .long("require-production")
                            .action(ArgAction::SetTrue)
                            .help("Require external production attestations, not only local profile certification"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("package").about("Package integrity commands").subcommand_required(true).subcommand(
                    ClapCommand::new("verify")
                        .about("Verify package integrity against Cell.lock and source tree")
                        .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON output")),
                ),
            )
            .subcommand(
                ClapCommand::new("registry")
                    .about("Registry integrity commands")
                    .subcommand_required(true)
                    .subcommand(
                        ClapCommand::new("verify")
                            .about("Verify deployment registry records against Cell.lock and chain facts")
                            .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON output"))
                            .arg(Arg::new("live").long("live").action(ArgAction::SetTrue).help("Verify deployment facts with CKB RPC get_live_cell"))
                            .arg(Arg::new("rpc-url").long("rpc-url").value_name("URL").help("CKB RPC URL for --live"))
                            .arg(Arg::new("network").long("network").value_name("NAME").help("Verify only this deployment network with --live"))
                            .arg(
                                Arg::new("require-publisher-signature")
                                    .long("require-publisher-signature")
                                    .action(ArgAction::SetTrue)
                                    .help("Require deployment publisher_signature metadata; cryptographic verification is not yet implemented"),
                            )
                            .arg(
                                Arg::new("require-audit-report")
                                    .long("require-audit-report")
                                    .action(ArgAction::SetTrue)
                                    .help("Require deployment audit_report_hash metadata"),
                            ),
                    )
                    .subcommand(
                        ClapCommand::new("add")
                            .about("Register a new package in the discovery index")
                            .arg(Arg::new("namespace").long("namespace").required(true).value_name("NAMESPACE").help("Package namespace"))
                            .arg(Arg::new("name").long("name").required(true).value_name("NAME").help("Package name"))
                            .arg(Arg::new("source").long("source").required(true).value_name("URL").help("Source repository URL")),
                    ),
            )
            .subcommand(
                ClapCommand::new("registry-verify")
                    .about("Verify deployment registry records against Cell.lock and chain facts")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON output"))
                    .arg(Arg::new("live").long("live").action(ArgAction::SetTrue).help("Verify deployment facts with CKB RPC get_live_cell"))
                    .arg(Arg::new("rpc-url").long("rpc-url").value_name("URL").help("CKB RPC URL for --live"))
                    .arg(Arg::new("network").long("network").value_name("NAME").help("Verify only this deployment network with --live"))
                    .arg(
                        Arg::new("require-publisher-signature")
                            .long("require-publisher-signature")
                            .action(ArgAction::SetTrue)
                            .help("Require deployment publisher_signature metadata; cryptographic verification is not yet implemented"),
                    )
                    .arg(
                        Arg::new("require-audit-report")
                            .long("require-audit-report")
                            .action(ArgAction::SetTrue)
                            .help("Require deployment audit_report_hash metadata"),
                    ),
            )
            .subcommand(
                ClapCommand::new("package-verify")
                    .about("Verify package integrity against Cell.lock and source tree")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON output")),
            )
            .subcommand(
                ClapCommand::new("registry-add")
                    .about("Register a new package in the discovery index")
                    .arg(Arg::new("namespace").long("namespace").required(true).value_name("NAMESPACE").help("Package namespace"))
                    .arg(Arg::new("name").long("name").required(true).value_name("NAME").help("Package name"))
                    .arg(Arg::new("source").long("source").required(true).value_name("URL").help("Source repository URL")),
            )
            .get_matches();

        match matches.subcommand() {
            Some(("build", m)) => Command::Build(BuildArgs {
                release: m.get_flag("release"),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                entry_action: m.get_one::<String>("entry-action").cloned(),
                entry_lock: m.get_one::<String>("entry-lock").cloned(),
                jobs: m.get_one::<String>("jobs").and_then(|s| s.parse().ok()),
                json: m.get_flag("json"),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                package: m.get_one::<String>("package").cloned(),
                workspace: m.get_flag("workspace"),
                ..Default::default()
            }),
            Some(("test", m)) => Command::Test(TestArgs {
                filter: m.get_one::<String>("filter").cloned(),
                no_run: m.get_flag("no-run"),
                nocapture: m.get_flag("nocapture"),
                fail_fast: m.get_flag("fail-fast"),
                doc: m.get_flag("doc"),
                json: m.get_flag("json"),
                ..Default::default()
            }),
            Some(("doc", m)) => Command::Doc(DocArgs {
                open: m.get_flag("open"),
                json: m.get_flag("json"),
                output_format: match m.get_one::<String>("format").map(|s| s.as_str()) {
                    Some("markdown") => OutputFormat::Markdown,
                    Some("json") => OutputFormat::Json,
                    _ => OutputFormat::Html,
                },
                ..Default::default()
            }),
            Some(("fmt", m)) => Command::Fmt(FmtArgs {
                check: m.get_flag("check"),
                json: m.get_flag("json"),
                files: m.get_many::<String>("files").map(|v| v.map(PathBuf::from).collect()).unwrap_or_default(),
            }),
            Some(("init", m)) => Command::Init(InitArgs {
                name: m.get_one::<String>("name").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                lib: m.get_flag("lib"),
                namespace: m.get_one::<String>("namespace").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("new", m)) => Command::New(NewArgs {
                name: m.get_one::<String>("name").cloned().expect("required package name"),
                path: m.get_one::<String>("path").map(PathBuf::from),
                lib: m.get_flag("lib"),
                vcs: m.get_one::<String>("vcs").cloned().unwrap_or_else(|| "git".to_string()),
                json: m.get_flag("json"),
            }),
            Some(("add", m)) => Command::Add(AddArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                build: m.get_flag("build"),
                git: m.get_one::<String>("git").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                json: m.get_flag("json"),
            }),
            Some(("remove", m)) => Command::Remove(RemoveArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                build: m.get_flag("build"),
                json: m.get_flag("json"),
            }),
            Some(("clean", m)) => Command::Clean(CleanArgs { json: m.get_flag("json"), cache: m.get_flag("cache") }),
            Some(("repl", _)) => Command::Repl,
            Some(("check", m)) => Command::Check(CheckArgs {
                all_targets: m.get_flag("all-targets"),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                features: Vec::new(),
                package: m.get_one::<String>("package").cloned(),
                workspace: m.get_flag("workspace"),
            }),
            Some(("metadata", m)) => Command::Metadata(MetadataArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
            }),
            Some(("constraints", m)) => Command::Constraints(ConstraintsArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                entry_action: m.get_one::<String>("entry-action").cloned(),
                entry_lock: m.get_one::<String>("entry-lock").cloned(),
            }),
            Some(("abi", m)) => Command::Abi(AbiArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                action: m.get_one::<String>("action").cloned(),
                lock: m.get_one::<String>("lock").cloned(),
            }),
            Some(("scheduler-plan", m)) => Command::SchedulerPlan(SchedulerPlanArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
            }),
            Some(("ckb-hash", m)) => Command::CkbHash(CkbHashArgs {
                input: m.get_one::<String>("input").cloned(),
                hex: m.get_one::<String>("hex").cloned(),
                file: m.get_one::<String>("file").map(PathBuf::from),
                json: m.get_flag("json"),
            }),
            Some(("ckb-std-compat", m)) => Command::CkbStdCompat(CkbStdCompatArgs { json: m.get_flag("json") }),
            Some(("explain", m)) => Command::Explain(ExplainArgs {
                code: m.get_one::<String>("code").cloned().expect("required runtime error code"),
                json: m.get_flag("json"),
            }),
            Some(("explain-profile", m)) => Command::ExplainProfile(ExplainProfileArgs {
                profile: m.get_one::<String>("profile").cloned().expect("required target profile"),
                json: m.get_flag("json"),
            }),
            Some(("explain-proof", m)) => Command::ExplainProof(ExplainProofArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("explain-assumptions", m)) => Command::ExplainAssumptions(ExplainAssumptionsArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("explain-generics", m)) => Command::ExplainGenerics(ExplainGenericsArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("opt-report", m)) => Command::OptReport(OptReportArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
            }),
            Some(("proof-diff", m)) => Command::ProofDiff(ProofDiffArgs {
                old: m.get_one::<String>("old").map(PathBuf::from).expect("required old metadata"),
                new: m.get_one::<String>("new").map(PathBuf::from).expect("required new metadata"),
                json: m.get_flag("json"),
            }),
            Some(("profile", m)) => Command::Profile(ProfileArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                entry: m.get_one::<String>("entry").cloned(),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("trace-tx", m)) => Command::TraceTx(TraceTxArgs {
                against: m.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                tx: m.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                json: m.get_flag("json"),
            }),
            Some(("audit-bundle", m)) => Command::AuditBundle(AuditBundleArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("validate-tx", m)) => Command::ValidateTx(ValidateTxArgs {
                against: m.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                tx: m.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                json: m.get_flag("json"),
            }),
            Some(("solve-tx", m)) => Command::SolveTx(SolveTxArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("verify-ckb-fixtures", m)) => Command::VerifyCkbFixtures(VerifyCkbFixturesArgs {
                manifest: m.get_one::<String>("manifest").map(PathBuf::from).expect("required CKB fixture manifest"),
                json: m.get_flag("json"),
            }),
            Some(("deploy-plan", m)) => Command::DeployPlan(DeployPlanArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("verify-deploy", m)) => Command::VerifyDeploy(VerifyDeployArgs {
                plan: m.get_one::<String>("plan").map(PathBuf::from).expect("required deploy plan"),
                json: m.get_flag("json"),
            }),
            Some(("diff-deploy", m)) => Command::DiffDeploy(DiffDeployArgs {
                old: m.get_one::<String>("old").map(PathBuf::from).expect("required old deploy plan"),
                new: m.get_one::<String>("new").map(PathBuf::from).expect("required new deploy plan"),
                json: m.get_flag("json"),
            }),
            Some(("lock-deps", m)) => Command::LockDeps(LockDepsArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("action", m)) => match m.subcommand() {
                Some(("build", build)) => Command::ActionBuild(ActionBuildArgs {
                    input: build.get_one::<String>("input").map(PathBuf::from),
                    action: build.get_one::<String>("action").cloned(),
                    output: build.get_one::<String>("output").map(PathBuf::from),
                    target: build.get_one::<String>("target").cloned(),
                    target_profile: build.get_one::<String>("target-profile").cloned(),
                    fabric_intent: build.get_flag("fabric-intent"),
                    json: build.get_flag("json"),
                }),
                _ => Command::ActionBuild(ActionBuildArgs::default()),
            },
            Some(("gen-builder", m)) => Command::GenBuilder(GenBuilderArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                metadata: m.get_one::<String>("metadata").map(PathBuf::from),
                lockfile: m.get_one::<String>("lockfile").map(PathBuf::from),
                deployed: m.get_one::<String>("deployed").map(PathBuf::from),
                deployment_network: m.get_one::<String>("deployment-network").cloned(),
                action: m.get_one::<String>("action").cloned(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned().unwrap_or_else(|| "typescript".to_string()),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                package_name: m.get_one::<String>("package-name").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("entry-witness", m)) => Command::EntryWitness(EntryWitnessArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                action: m.get_one::<String>("action").cloned(),
                lock: m.get_one::<String>("lock").cloned(),
                args: m.get_many::<String>("arg").map(|values| values.cloned().collect()).unwrap_or_default(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("verify-artifact", m)) => Command::VerifyArtifact(VerifyArtifactArgs {
                artifact: m.get_one::<String>("artifact").map(PathBuf::from).expect("required artifact"),
                metadata: m.get_one::<String>("metadata").map(PathBuf::from),
                verify_sources: m.get_flag("verify-sources"),
                json: m.get_flag("json"),
                expect_target_profile: m.get_one::<String>("expect-target-profile").cloned(),
                expect_artifact_hash: m.get_one::<String>("expect-artifact-hash").cloned(),
                expect_source_hash: m.get_one::<String>("expect-source-hash").cloned(),
                expect_source_content_hash: m.get_one::<String>("expect-source-content-hash").cloned(),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
            }),
            Some(("run", m)) => Command::Run(RunArgs {
                args: m.get_many::<String>("args").map(|values| values.cloned().collect()).unwrap_or_default(),
                release: m.get_flag("release"),
                simulate: m.get_flag("simulate"),
            }),
            Some(("publish", m)) => {
                Command::Publish(PublishArgs { dry_run: m.get_flag("dry-run"), allow_dirty: m.get_flag("allow-dirty") })
            }
            Some(("install", m)) => Command::Install(InstallArgs {
                crate_name: m.get_one::<String>("crate").cloned(),
                version: m.get_one::<String>("version").cloned(),
                namespace: m.get_one::<String>("namespace").cloned(),
                git: m.get_one::<String>("git").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
            }),
            Some(("update", _)) => Command::Update,
            Some(("info", m)) => Command::Info(InfoArgs { json: m.get_flag("json") }),
            Some(("login", m)) => Command::Login(LoginArgs { registry: m.get_one::<String>("registry").cloned() }),
            Some(("certify", m)) => Command::Certify(CertifyArgs {
                plugin: m.get_one::<String>("plugin").cloned().unwrap_or_else(|| NOVASEAL_CERTIFICATION_PLUGIN.to_string()),
                repo_root: m.get_one::<String>("repo-root").map(PathBuf::from),
                report: m.get_one::<String>("report").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: m.get_flag("json"),
                require_production: m.get_flag("require-production"),
            }),
            Some(("package", m)) => match m.subcommand() {
                Some(("verify", verify)) => Command::PackageVerify(PackageVerifyArgs { json: verify.get_flag("json") }),
                _ => unreachable!(),
            },
            Some(("registry", m)) => match m.subcommand() {
                Some(("verify", verify)) => Command::RegistryVerify(RegistryVerifyArgs {
                    json: verify.get_flag("json"),
                    live: verify.get_flag("live"),
                    rpc_url: verify.get_one::<String>("rpc-url").cloned(),
                    network: verify.get_one::<String>("network").cloned(),
                    require_publisher_signature: verify.get_flag("require-publisher-signature"),
                    require_audit_report: verify.get_flag("require-audit-report"),
                }),
                Some(("add", add)) => Command::RegistryAdd(RegistryAddArgs {
                    namespace: add.get_one::<String>("namespace").cloned().unwrap_or_default(),
                    name: add.get_one::<String>("name").cloned().unwrap_or_default(),
                    source: add.get_one::<String>("source").cloned().unwrap_or_default(),
                }),
                _ => unreachable!(),
            },
            Some(("registry-verify", m)) => Command::RegistryVerify(RegistryVerifyArgs {
                json: m.get_flag("json"),
                live: m.get_flag("live"),
                rpc_url: m.get_one::<String>("rpc-url").cloned(),
                network: m.get_one::<String>("network").cloned(),
                require_publisher_signature: m.get_flag("require-publisher-signature"),
                require_audit_report: m.get_flag("require-audit-report"),
            }),
            Some(("package-verify", m)) => Command::PackageVerify(PackageVerifyArgs { json: m.get_flag("json") }),
            Some(("registry-add", m)) => Command::RegistryAdd(RegistryAddArgs {
                namespace: m.get_one::<String>("namespace").cloned().unwrap_or_default(),
                name: m.get_one::<String>("name").cloned().unwrap_or_default(),
                source: m.get_one::<String>("source").cloned().unwrap_or_default(),
            }),
            _ => unreachable!(),
        }
    }
}

/// Resolve --primitive-compat and --primitive-strict into a single version string.
/// --primitive-strict=X takes precedence and sets strict mode.
/// --primitive-compat=X sets compat mode.
fn resolve_primitive_compat(compat: Option<String>, strict: Option<String>) -> Option<String> {
    if strict.is_some() {
        strict
    } else {
        compat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_execution() {
        let _cmd = Command::Clean(CleanArgs::default());
    }
}

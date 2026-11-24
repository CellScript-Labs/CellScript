use super::artifact::ArtifactArgs;
use super::certification::CertifyArgs;
use super::output::{json_output, print_json, read_json_value, CliDiagnosticCode, CommandOutcome};
use crate::docgen::OutputFormat;
use crate::error::{CompileError, Result};
use crate::package::{Dependency, DetailedDependency, Lockfile, PackageManager, PackageManifest};
use crate::runtime_errors::{runtime_error_info, runtime_error_info_by_code, CellScriptRuntimeErrorInfo, ALL_RUNTIME_ERRORS};
use crate::{compile_path, compile_path_metadata_with_diagnostics, CompileMetadata, CompileOptions, TargetProfile};
use base64::Engine;
use camino::{Utf8Path, Utf8PathBuf};
#[cfg(feature = "vm-runner")]
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use colored::Colorize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

mod argument_parsing;
mod argument_schema;

mod abi;
mod analysis_reports;
mod artifact_verification;
mod audit_reports;
mod authoring;
mod build;
mod builders;
mod check;
mod check_policy;
mod ckb_compat;
mod deployment;
mod deployment_verification;
mod entry_witness_values;
mod explain;
mod fixture_verification;
mod inspection;
mod install;
mod intent_records;
mod package_resolution;
mod package_test_expectations;
mod package_tests;
mod publish;
mod receipt_records;
mod receipts;
mod registry_auth;
mod registry_keys;
mod registry_requests;
mod registry_verification;
mod run;
mod transaction_solver;
mod transactions;
mod typescript_builder;

use check_policy::merge_check_policy;
pub(super) use deployment_verification::ckb_script_hash_from_json;
use entry_witness_values::{decode_hex_arg, parse_entry_witness_arg};
use registry_keys::sign_registry_publish_payload;
pub(super) use registry_keys::{sign_registry_capability_payload, sign_registry_reproducer_payload};
pub(super) use registry_requests::registry_http_client;
use registry_requests::{authorise_registry_publish_key, submit_registry_publish_request};

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
pub(super) const CELLSCRIPT_CKB_RPC_URL_ENV: &str = "CELLSCRIPT_CKB_RPC_URL";
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
    ResolveGraph(ResolveGraphArgs),
    BuildPlan(BuildPlanArgs),
    Expand(ExpandArgs),
    Interface(InterfaceArgs),
    InterfaceDiff(InterfaceDiffArgs),
    SchemaAck(SchemaAckArgs),
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
    ExplainCollections(ExplainCollectionsArgs),
    ExplainGraph(ExplainGraphArgs),
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
    Receipt(ReceiptArgs),
    SignReceipt(SignReceiptArgs),
    VerifyReceipt(VerifyReceiptArgs),
    VerifyArtifact(VerifyArtifactArgs),
    ProtocolBundleCheck(ProtocolBundleCheckArgs),
    Run(RunArgs),
    Artifact(ArtifactArgs),
    Publish(PublishArgs),
    Install(InstallArgs),
    RegistryVerify(RegistryVerifyArgs),
    PackageVerify(PackageVerifyArgs),
    RegistryAdd(RegistryAddArgs),
    RegistryEdit(RegistryEditArgs),
    Certify(CertifyArgs),
    Lock(PackageLockArgs),
    UpdatePlan(UpdatePlanArgs),
    Update(UpdateArgs),
    Info(InfoArgs),
    LanguageContract(InfoArgs),
    Login(LoginArgs),
    AuthLogin(AuthCapabilityArgs),
    AuthCapabilityCreate(AuthCapabilityArgs),
    AuthCapabilitySubmit(AuthCapabilitySubmitArgs),
    AuthCapabilityRevoke(AuthCapabilityRevokeArgs),
    AuthReproducerCreate(AuthReproducerCreateArgs),
    AuthNamespaceClaim(AuthNamespaceClaimArgs),
}

#[derive(Debug, Default)]
pub struct BuildArgs {
    pub source_contracts: bool,
    pub release: bool,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub entry_action: Option<String>,
    pub entry_lock: Option<String>,
    pub artifact: Option<String>,
    pub jobs: Option<usize>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub locked: bool,
    pub frozen: bool,
    pub offline: bool,
    pub environment: Option<String>,
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
    pub backend: Option<String>,
    pub jobs: Option<usize>,
    pub release: bool,
    pub no_run: bool,
    pub nocapture: bool,
    pub fail_fast: bool,
    pub doc: bool,
    pub json: bool,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub locked: bool,
    pub frozen: bool,
    pub offline: bool,
    pub environment: Option<String>,
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
    pub package: Option<String>,
    pub dev: bool,
    pub build: bool,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
    pub use_environment: Option<String>,
    pub environment_independent: bool,
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
pub struct PackageLockArgs {
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct UpdatePlanArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub package: Option<String>,
    pub precise: Option<String>,
    pub scope: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub environment: Option<String>,
    pub offline: bool,
    pub acknowledgements: Vec<String>,
    pub schema_baselines: Vec<String>,
    pub schema_acknowledgements: Vec<String>,
    pub schema_version: u32,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct UpdateArgs {
    pub plan: Option<PathBuf>,
    pub planning: UpdatePlanArgs,
}

#[derive(Debug, Default)]
pub struct CheckArgs {
    pub all_targets: bool,
    pub artifact: Option<String>,
    pub target_profile: Option<String>,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub locked: bool,
    pub frozen: bool,
    pub offline: bool,
    pub environment: Option<String>,
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
    pub artifact: Option<String>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
}

#[derive(Debug, Default)]
pub struct ResolveGraphArgs {
    pub input: Option<PathBuf>,
    pub package: Option<String>,
    pub scope: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub environment: Option<String>,
    pub offline: bool,
    pub schema_version: u32,
    pub output: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct BuildPlanArgs {
    pub source_contracts: bool,
    pub input: Option<PathBuf>,
    pub package: Option<String>,
    pub scope: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub environment: Option<String>,
    pub offline: bool,
    pub schema_version: u32,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub release: bool,
    pub debug: bool,
    pub primitive_compat: Option<String>,
    pub entry_action: Option<String>,
    pub entry_lock: Option<String>,
    pub artifact: Option<String>,
    pub production: bool,
    pub deny_fail_closed: bool,
    pub deny_ckb_runtime: bool,
    pub deny_runtime_obligations: bool,
    pub output: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExpandArgs {
    pub input: Option<PathBuf>,
    pub artifact: Option<String>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct InterfaceArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct InterfaceDiffArgs {
    pub old: PathBuf,
    pub new: PathBuf,
    pub output: Option<PathBuf>,
    pub json: bool,
    pub checked: bool,
}

#[derive(Debug, Default)]
pub struct SchemaAckArgs {
    pub upgrade_plan: Option<PathBuf>,
    pub subject: Option<String>,
    pub environment: Option<String>,
    pub module: Option<String>,
    pub old: PathBuf,
    pub new: PathBuf,
    pub action: String,
    pub before: String,
    pub after: String,
    pub output: Option<PathBuf>,
    pub acknowledge_by: Option<String>,
    pub rationale: Option<String>,
    pub verify: Option<PathBuf>,
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
pub struct ExplainCollectionsArgs {
    pub input: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ExplainGraphArgs {
    pub input: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
    pub format: Option<String>,
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
    pub artifact: Option<String>,
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
    pub artifact: Option<String>,
    pub script_hash: Option<String>,
    pub action: Option<String>,
    pub lock: Option<String>,
    pub args: Vec<String>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct ReceiptArgs {
    pub input: Option<PathBuf>,
    pub output: PathBuf,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct SignReceiptArgs {
    pub receipt: PathBuf,
    pub role: String,
    pub key: String,
    pub output: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct VerifyReceiptArgs {
    pub receipt: PathBuf,
    pub metadata: PathBuf,
    pub artifact: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct VerifyArtifactArgs {
    pub artifact: PathBuf,
    pub metadata: Option<PathBuf>,
    pub lowering_record: Option<PathBuf>,
    pub source_map: Option<PathBuf>,
    pub receipt: Option<PathBuf>,
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
pub struct ProtocolBundleCheckArgs {
    pub manifest: PathBuf,
    pub open_admission: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct RunArgs {
    pub args: Vec<String>,
    pub release: bool,
    pub simulate: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct PublishArgs {
    pub dry_run: bool,
    pub offline: bool,
    pub allow_dirty: bool,
    pub api_url: Option<String>,
    pub capability_key_id: Option<String>,
    pub authorise: bool,
    pub no_open: bool,
    pub capability_signature: Option<String>,
    pub idempotency_key: Option<String>,
    pub payload: Option<PathBuf>,
    pub source_snapshot: Option<PathBuf>,
    pub artifact_manifest: Option<PathBuf>,
    pub artifact_kind: Option<String>,
    pub print_payload: bool,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct InstallArgs {
    pub crate_name: Option<String>,
    pub version: Option<String>,
    pub namespace: Option<String>,
    pub git: Option<String>,
    pub path: Option<PathBuf>,
    pub allow_unverified: bool,
    pub allow_quarantined: bool,
}

#[derive(Debug, Default)]
pub struct LoginArgs {
    pub registry: Option<String>,
}

#[derive(Debug, Default)]
pub struct AuthCapabilityArgs {
    pub registry_origin: Option<String>,
    pub principal_type: Option<String>,
    pub principal_id: Option<String>,
    pub capability_pubkey: Option<String>,
    pub scopes: Vec<String>,
    pub expires: Option<String>,
    pub capability_expires_at: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuthCapabilitySubmitArgs {
    pub api_url: Option<String>,
    pub payload: PathBuf,
    pub wallet_signature: PathBuf,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuthCapabilityRevokeArgs {
    pub api_url: Option<String>,
    pub registry_origin: Option<String>,
    pub principal_type: Option<String>,
    pub principal_id: Option<String>,
    pub capability_key_id: Option<String>,
    pub payload: Option<PathBuf>,
    pub wallet_signature: Option<PathBuf>,
    pub reason: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuthReproducerCreateArgs {
    pub builder_id: String,
    pub trust_domain: String,
    pub private_key_output: Option<PathBuf>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuthNamespaceClaimArgs {
    pub api_url: Option<String>,
    pub namespace: String,
    pub payload: PathBuf,
    pub wallet_signature: PathBuf,
    pub json: bool,
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
pub struct RegistryEditArgs {
    pub yank: Option<String>,
    pub reason: Option<String>,
    pub replaced_by: Option<String>,
    pub yanked_at: Option<String>,
}

pub struct CommandExecutor;

#[derive(Debug, Clone, serde::Serialize)]
struct RunEntryOutcome {
    kind: String,
    name: String,
}

#[derive(Debug, serde::Serialize)]
struct RunOutcome {
    status: &'static str,
    mode: &'static str,
    artifact_format: String,
    entry: Option<RunEntryOutcome>,
    cycles: Option<u64>,
    steps: Option<u64>,
    has_cell_operations: Option<bool>,
    result: Option<String>,
    trace: Vec<String>,
}

impl RunOutcome {
    fn emit(self, json: bool) -> Result<()> {
        if json {
            let value = serde_json::to_value(&self).map_err(|error| {
                CompileError::without_span(format!("failed to serialize run outcome: {}", error))
                    .with_category(crate::error::CompileErrorCategory::Internal)
                    .with_source(error)
            })?;
            return print_json(&value);
        }

        match self.mode {
            "ckb-vm" => {
                println!("{}", "Run complete".green());
                println!("  Artifact format: {}", self.artifact_format);
                if let Some(entry) = self.entry {
                    println!("  Entry: {} {}", entry.kind, entry.name);
                }
                if let Some(cycles) = self.cycles {
                    println!("  Cycles: {}", cycles);
                }
            }
            "simulate" => {
                println!("{}", "Simulate complete".green());
                if let Some(entry) = self.entry {
                    println!("  Entry: {} {}", entry.kind, entry.name);
                }
                if let Some(steps) = self.steps {
                    println!("  Steps: {}", steps);
                }
                match self.has_cell_operations {
                    Some(true) => println!("  Cell operations: {} (simulated)", "yes".yellow()),
                    Some(false) => println!("  Cell operations: none (pure computation)"),
                    None => {}
                }
                if let Some(result) = self.result {
                    println!("  Result: {}", result);
                }
                if !self.trace.is_empty() {
                    println!("  Trace:");
                    for event in self.trace {
                        println!("{}", event);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(feature = "vm-runner")]
fn run_entry_outcome(metadata: &CompileMetadata) -> Option<RunEntryOutcome> {
    let (kind, name) = metadata.typed_semantics.foundation.entry_contract.exact_entry.split_once(':')?;
    match kind {
        "action" | "lock" => Some(RunEntryOutcome { kind: kind.to_string(), name: name.to_string() }),
        _ => None,
    }
}

fn collect_workspace_incremental_caches(root: &Path, caches: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if name == ".cell" {
            let build = path.join("build");
            let Some(build) = managed_directory_if_present(&build)? else { continue };
            let cache = build.join("cache");
            if let Some(cache) = managed_directory_if_present(&cache)? {
                caches.push(cache);
            }
            continue;
        }
        if matches!(name.to_str(), Some(".git" | "target" | "node_modules")) {
            continue;
        }
        collect_workspace_incremental_caches(&path, caches)?;
    }
    Ok(())
}

fn managed_directory_if_present(path: &Path) -> Result<Option<PathBuf>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => Err(CompileError::without_span(format!(
            "refusing managed cache path with a symlink or non-directory component: '{}'",
            path.display()
        ))),
        Ok(_) => Ok(Some(path.to_path_buf())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn ensure_workspace_directory_for_removal(workspace_root: &Path, path: &Path) -> Result<PathBuf> {
    let canonical_root = std::fs::canonicalize(workspace_root)?;
    let absolute = if path.is_absolute() { path.to_path_buf() } else { canonical_root.join(path) };
    let canonical_path = std::fs::canonicalize(&absolute)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(CompileError::without_span(format!(
            "refusing to clean path outside workspace '{}': '{}'",
            canonical_root.display(),
            canonical_path.display()
        )));
    }
    let relative = absolute
        .strip_prefix(&canonical_root)
        .map_err(|_| CompileError::without_span(format!("refusing non-workspace clean path '{}'", absolute.display())))?;
    let mut cursor = canonical_root;
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Err(CompileError::without_span(format!("refusing non-canonical clean path '{}'", path.display())));
        };
        cursor.push(component);
        let metadata = std::fs::symlink_metadata(&cursor)?;
        if metadata.file_type().is_symlink() {
            return Err(CompileError::without_span(format!("refusing to clean through symlink '{}'", cursor.display())));
        }
    }
    let metadata = std::fs::symlink_metadata(&canonical_path)?;
    if !metadata.is_dir() {
        return Err(CompileError::without_span(format!("refusing to clean non-directory path '{}'", path.display())));
    }
    Ok(canonical_path)
}

impl CommandExecutor {
    #[cfg(not(feature = "vm-runner"))]
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
            Command::ResolveGraph(args) => Self::resolve_graph(args),
            Command::BuildPlan(args) => Self::build_plan(args),
            Command::Expand(args) => Self::expand(args),
            Command::Interface(args) => Self::interface(args),
            Command::InterfaceDiff(args) => Self::interface_diff(args),
            Command::SchemaAck(args) => Self::schema_ack(args),
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
            Command::ExplainCollections(args) => Self::explain_collections(args),
            Command::ExplainGraph(args) => Self::explain_graph(args),
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
            Command::Receipt(args) => Self::receipt(args),
            Command::SignReceipt(args) => Self::sign_receipt(args),
            Command::VerifyReceipt(args) => Self::verify_receipt(args),
            Command::VerifyArtifact(args) => Self::verify_artifact(args),
            Command::ProtocolBundleCheck(args) => Self::protocol_bundle_check(args),
            Command::Run(args) => Self::run(args),
            Command::Artifact(args) => super::artifact::execute(args),
            Command::Publish(args) => Self::publish(args),
            Command::Install(args) => Self::install(args),
            Command::Lock(args) => Self::lock(args),
            Command::UpdatePlan(args) => Self::update_plan(args),
            Command::Update(args) => Self::update(args),
            Command::Info(args) => Self::info(args),
            Command::LanguageContract(args) => {
                let contract = crate::source_language_contract();
                CommandOutcome {
                    machine: serde_json::to_value(&contract)?,
                    human_lines: vec![
                        format!("CellScript source: {} ({})", contract.source_semantics, contract.edition),
                        format!("Identifier Unicode: {}", contract.identifier_unicode_version),
                    ],
                }
                .emit(args.json)
            }
            Command::Login(args) => Self::login(args),
            Command::AuthLogin(args) | Command::AuthCapabilityCreate(args) => Self::auth_capability(args),
            Command::AuthCapabilitySubmit(args) => Self::auth_capability_submit(args),
            Command::AuthCapabilityRevoke(args) => Self::auth_capability_revoke(args),
            Command::AuthReproducerCreate(args) => Self::auth_reproducer_create(args),
            Command::AuthNamespaceClaim(args) => Self::auth_namespace_claim(args),
            Command::RegistryVerify(args) => Self::registry_verify(args),
            Command::PackageVerify(args) => Self::package_verify(args),
            Command::RegistryAdd(args) => Self::registry_add(args),
            Command::RegistryEdit(args) => Self::registry_edit(args),
            Command::Certify(args) => super::certification::run(args),
        }
    }
}

#[cfg(feature = "vm-runner")]
type CliVmMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

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

pub(super) fn current_utc_timestamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    utc_timestamp_from_unix_secs(secs)
}

pub(super) fn utc_timestamp_after_seconds(delta_secs: u64) -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    utc_timestamp_from_unix_secs(secs.saturating_add(delta_secs))
}

fn utc_timestamp_from_unix_secs(secs: u64) -> String {
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let (year, month, day) = civil_date_from_days(days_since_epoch as i32);
    let hour = (time_of_day / 3600) as u8;
    let minute = ((time_of_day % 3600) / 60) as u8;
    let second = (time_of_day % 60) as u8;
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

fn resolve_requested_scopes(mut scopes: Vec<String>) -> Result<Vec<String>> {
    if scopes.iter().any(|scope| scope.trim().is_empty()) {
        return Err(invalid_capability_scope(""));
    }
    scopes = scopes.into_iter().map(|scope| scope.trim().to_string()).collect();
    if !scopes.is_empty() {
        validate_capability_scopes(&scopes)?;
        return Ok(scopes);
    }

    let manifest = PackageManager::new(".").read_manifest().map_err(|_| {
        crate::error::CompileError::without_span(
            "at least one capability scope is required outside a package directory; pass --scope <publish|deployment|availability>:<namespace>/<package>",
        )
    })?;
    let namespace = manifest.package.namespace.ok_or_else(|| {
        crate::error::CompileError::without_span(
            "cannot infer the publish capability scope because [package].namespace is missing; pass --scope publish:<namespace>/<package>",
        )
    })?;
    if manifest.package.name.is_empty() {
        return Err(crate::error::CompileError::without_span(
            "cannot infer the publish capability scope because [package].name is empty; pass --scope publish:<namespace>/<package>",
        ));
    }

    let coordinate = format!("{}/{}", namespace, manifest.package.name);
    Ok(vec![format!("publish:{coordinate}")])
}

fn validate_capability_scopes(scopes: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for scope in scopes {
        if !seen.insert(scope.as_str()) {
            return Err(crate::error::CompileError::without_span(format!("duplicate capability scope '{scope}'"))
                .with_category(crate::error::CompileErrorCategory::Usage));
        }
        let Some((action, coordinate)) = scope.split_once(':') else {
            return Err(invalid_capability_scope(scope));
        };
        if !matches!(action, "publish" | "deployment" | "availability") {
            return Err(invalid_capability_scope(scope));
        }
        let Some((namespace, name)) = coordinate.split_once('/') else {
            return Err(invalid_capability_scope(scope));
        };
        validate_declared_artifact_ident(namespace, "capability scope namespace").map_err(|_| invalid_capability_scope(scope))?;
        if name != "*" {
            validate_declared_artifact_ident(name, "capability scope package").map_err(|_| invalid_capability_scope(scope))?;
        }
    }
    Ok(())
}

fn invalid_capability_scope(scope: &str) -> CompileError {
    crate::error::CompileError::without_span(format!(
        "invalid capability scope '{scope}'; expected <publish|deployment|availability>:<namespace>/<package-or-*>",
    ))
    .with_category(crate::error::CompileErrorCategory::Usage)
}

fn resolve_capability_expires_at(explicit_timestamp: Option<String>, relative: Option<String>) -> Result<String> {
    if let Some(timestamp) = explicit_timestamp {
        return Ok(timestamp);
    }

    let Some(relative) = relative else {
        return Ok(utc_timestamp_after_seconds(90 * 24 * 60 * 60));
    };
    let trimmed = relative.trim();
    if let Some(days) = trimmed.strip_suffix('d') {
        let days: u64 = days.parse().map_err(|_| {
            crate::error::CompileError::without_span(format!("invalid --expires value '{}'; expected a duration like 90d", relative))
                .with_category(crate::error::CompileErrorCategory::Usage)
        })?;
        return Ok(utc_timestamp_after_seconds(days.saturating_mul(24 * 60 * 60)));
    }
    if let Some(hours) = trimmed.strip_suffix('h') {
        let hours: u64 = hours.parse().map_err(|_| {
            crate::error::CompileError::without_span(format!(
                "invalid --expires value '{}'; expected a duration like 90d or 24h",
                relative
            ))
            .with_category(crate::error::CompileErrorCategory::Usage)
        })?;
        return Ok(utc_timestamp_after_seconds(hours.saturating_mul(60 * 60)));
    }
    if trimmed.contains('T') && trimmed.ends_with('Z') {
        return Ok(trimmed.to_string());
    }

    Err(crate::error::CompileError::without_span(format!(
        "invalid --expires value '{}'; expected a duration like 90d/24h or an absolute UTC timestamp",
        relative
    ))
    .with_category(crate::error::CompileErrorCategory::Usage))
}

fn registry_auth_nonce(
    registry_origin: &str,
    principal_type: &str,
    principal_id: &str,
    capability_pubkey: &str,
    requested_scopes: &[String],
    issued_at: &str,
) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}",
        crate::package::registry::REGISTRY_AUTH_PROTOCOL,
        registry_origin,
        principal_type,
        principal_id,
        capability_pubkey,
        requested_scopes.join(","),
        issued_at
    );
    format!("0x{}", hex::encode(crate::ckb_blake2b256(material.as_bytes())))
}

fn registry_revoke_nonce(
    registry_origin: &str,
    principal_type: &str,
    principal_id: &str,
    capability_key_id: &str,
    issued_at: &str,
) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        crate::package::registry::REVOKE_CAPABILITY_ACTION,
        registry_origin,
        principal_type,
        principal_id,
        capability_key_id,
        issued_at
    );
    format!("0x{}", hex::encode(crate::ckb_blake2b256(material.as_bytes())))
}

struct GeneratedRegistryCapabilityKey {
    key_id: String,
    capability_pubkey: String,
}

struct GeneratedRegistryKeyMaterial {
    key_id: String,
    public_key: String,
    private_key_pkcs8: Vec<u8>,
}

const REGISTRY_KEYCHAIN_SECRET_SCHEMA: &str = "cellscript-registry-private-key-v1";

#[derive(serde::Deserialize, serde::Serialize)]
struct RegistryKeychainSecret {
    schema: String,
    status: String,
    pkcs8_b64: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pending_expires_at_unix_seconds: Option<u64>,
}

fn registry_publish_nonce(
    registry_origin: &str,
    namespace: &str,
    name: &str,
    version: &str,
    source_hash: &str,
    capability_key_id: &str,
    issued_at: &str,
) -> String {
    let material = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        crate::package::registry::REGISTRY_PUBLISH_PROTOCOL,
        registry_origin,
        namespace,
        name,
        version,
        source_hash,
        capability_key_id,
        issued_at
    );
    format!("0x{}", hex::encode(crate::ckb_blake2b256(material.as_bytes())))
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredArtifactManifest {
    schema: String,
    namespace: String,
    name: String,
    release: String,
    kind: String,
    language: String,
    bundle: PathBuf,
    #[serde(default)]
    description: String,
    #[serde(default)]
    repository: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    documentation: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredArtifactBundle {
    schema: String,
    namespace: String,
    name: String,
    release: String,
    profile: String,
    manifest_json: String,
    objects: Vec<DeclaredArtifactBundleObject>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredArtifactBundleObject {
    role: String,
    content_base64: String,
}

fn publish_declared_artifact(args: PublishArgs, manifest_path: &Path) -> Result<()> {
    if args.payload.is_some() || args.source_snapshot.is_some() {
        return Err(crate::error::CompileError::without_span(
            "--artifact-manifest owns the publish payload and immutable bundle; do not combine it with --payload or --source-snapshot",
        ));
    }
    let manifest_text = std::fs::read_to_string(manifest_path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read artifact manifest '{}': {}", manifest_path.display(), error))
    })?;
    let manifest: DeclaredArtifactManifest = toml::from_str(&manifest_text).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse artifact manifest '{}': {}", manifest_path.display(), error))
    })?;
    if manifest.schema != "cellscript-registry-artifact" {
        return Err(crate::error::CompileError::without_span("Artifact.toml schema must be 'cellscript-registry-artifact'"));
    }
    validate_declared_artifact_ident(&manifest.namespace, "namespace")?;
    validate_declared_artifact_ident(&manifest.name, "name")?;
    validate_declared_artifact_release(&manifest.release)?;
    let artifact = declared_artifact_descriptor(&manifest.kind, &manifest.language)?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let bundle_path = if manifest.bundle.is_absolute() { manifest.bundle.clone() } else { manifest_dir.join(&manifest.bundle) };
    let bundle_bytes = std::fs::read(&bundle_path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read artifact bundle '{}': {}", bundle_path.display(), error))
    })?;
    if bundle_bytes.is_empty() || bundle_bytes.len() > 5 * 1024 * 1024 {
        return Err(crate::error::CompileError::without_span("artifact bundle must be a non-empty JSON file no larger than 5 MiB"));
    }
    let bundle: DeclaredArtifactBundle = serde_json::from_slice(&bundle_bytes).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse artifact bundle '{}': {}", bundle_path.display(), error))
    })?;
    if bundle.schema != "cellscript-registry-bundle"
        || bundle.namespace != manifest.namespace
        || bundle.name != manifest.name
        || bundle.release != manifest.release
        || bundle.profile != artifact.profile
    {
        return Err(crate::error::CompileError::without_span(
            "artifact bundle schema, coordinate, release, or profile does not match Artifact.toml",
        ));
    }
    let manifest_json: serde_json::Value = serde_json::from_str(&bundle.manifest_json).map_err(|error| {
        crate::error::CompileError::without_span(format!("artifact bundle manifest_json must be valid JSON: {error}"))
    })?;
    if !manifest_json.is_object() {
        return Err(crate::error::CompileError::without_span("artifact bundle manifest_json must encode a JSON object"));
    }
    validate_declared_bundle_roles(&bundle, &artifact.profile, &manifest_json)?;
    let source = declared_bundle_object(&bundle, "source")?;
    let source_hash = hex::encode(crate::ckb_blake2b256(&source));
    let mut artifact_hash = None;
    let mut abi_hash = None;
    let mut abi_sha256 = None;
    let mut executable_ls_idl_bound = None;
    let mut build_recipe_hash = None;
    let audit_report_hash = if manifest_json.pointer("/security/audit_report_hash").is_some() {
        Some(hex::encode(crate::ckb_blake2b256(&declared_bundle_object(&bundle, "audit_report")?)))
    } else {
        None
    };
    if artifact.profile == "ckb_executable" {
        let executable = declared_bundle_object(&bundle, "executable")?;
        let abi = declared_bundle_object(&bundle, "abi")?;
        artifact_hash = Some(hex::encode(crate::ckb_blake2b256(&executable)));
        abi_hash = Some(hex::encode(crate::ckb_blake2b256(&abi)));
        if manifest_json.get("interface").is_some() {
            use sha2::Digest as _;
            crate::package::registry::validate_ls_idl_document(&abi).map_err(crate::error::CompileError::without_span)?;
            let digest = sha2::Sha256::digest(&abi);
            abi_sha256 = Some(hex::encode(digest));
            executable_ls_idl_bound = Some(executable.ends_with(digest.as_slice()));
        }
        if manifest_json.pointer("/build/reproducible").and_then(serde_json::Value::as_bool) == Some(true) {
            build_recipe_hash = Some(hex::encode(crate::ckb_blake2b256(&declared_bundle_object(&bundle, "build_recipe")?)));
        }
    } else if artifact.profile == "reproducible_build" {
        artifact_hash = Some(hex::encode(crate::ckb_blake2b256(&declared_bundle_object(&bundle, "executable")?)));
        build_recipe_hash = Some(hex::encode(crate::ckb_blake2b256(&declared_bundle_object(&bundle, "build_recipe")?)));
    }
    crate::package::registry::validate_artifact_profile_contract(
        &artifact.kind,
        &artifact.profile,
        &manifest_json,
        crate::package::registry::ArtifactContractHashes {
            artifact_hash: artifact_hash.as_deref(),
            abi_hash: abi_hash.as_deref(),
            abi_sha256: abi_sha256.as_deref(),
            executable_ls_idl_bound,
            build_recipe_hash: build_recipe_hash.as_deref(),
            audit_report_hash: audit_report_hash.as_deref(),
        },
    )
    .map_err(crate::error::CompileError::without_span)?;
    let canonical_manifest_json = crate::package::registry::canonical_artifact_contract_json(&manifest_json)
        .map_err(crate::error::CompileError::without_span)?;
    let manifest_hash = hex::encode(crate::ckb_blake2b256(canonical_manifest_json.as_bytes()));
    let mut release = serde_json::json!({
        "version": manifest.release,
        "tag": format!("v{}", manifest.release),
        "source_hash": source_hash,
        "verification_status": "pending",
        "deployment_status": if artifact.profile == "ckb_executable" { "undeployed" } else { "not_applicable" },
        "availability_status": "active",
        "profile_contract": manifest_json,
    });
    if let Some(value) = artifact_hash {
        release["artifact_hash"] = serde_json::Value::String(value);
    }
    if let Some(value) = abi_hash {
        release["abi_hash"] = serde_json::Value::String(value);
    }
    if let Some(value) = build_recipe_hash {
        release["build_recipe_hash"] = serde_json::Value::String(value);
    }
    let mut registry_entry = serde_json::json!({
        "schema_version": crate::package::registry::RegistryIndex::CURRENT_SCHEMA_VERSION,
        "namespace": manifest.namespace,
        "name": manifest.name,
        "artifact": artifact,
        "versions": [release],
    });
    let entry = registry_entry.as_object_mut().expect("registry entry JSON object");
    for (key, value) in [
        ("description", &manifest.description),
        ("repository", &manifest.repository),
        ("homepage", &manifest.homepage),
        ("documentation", &manifest.documentation),
    ] {
        if !value.is_empty() {
            entry.insert(key.to_string(), serde_json::Value::String(value.clone()));
        }
    }
    if !manifest.keywords.is_empty() {
        entry.insert(
            "keywords".to_string(),
            serde_json::to_value(&manifest.keywords).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize artifact keywords: {error}"))
            })?,
        );
    }
    if !manifest.categories.is_empty() {
        entry.insert(
            "categories".to_string(),
            serde_json::to_value(&manifest.categories).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize artifact categories: {error}"))
            })?,
        );
    }

    if args.dry_run {
        let summary = serde_json::json!({
            "status": "valid",
            "coordinate": format!("{}/{}@{}", manifest.namespace, manifest.name, manifest.release),
            "artifact": artifact,
            "source_hash": source_hash,
            "manifest_hash": manifest_hash,
            "bundle": bundle_path,
        });
        if args.json {
            return print_json(&summary);
        }
        println!("{}", "Artifact publish dry-run passed".green());
        println!("  Coordinate: {}/{}@{}", manifest.namespace, manifest.name, manifest.release);
        println!("  Kind: {}", artifact.kind);
        println!("  Profile: {}", artifact.profile);
        println!("  Source hash: {}", source_hash);
        return Ok(());
    }

    let api_base = resolve_registry_api_base(args.api_url)?;
    let registry_origin = registry_origin_from_api_base(&api_base)?;
    let endpoint = registry_publish_endpoint(&api_base, &manifest.namespace, &manifest.name);
    let capability_key_id = match args.capability_key_id.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_KEY_ID").ok()) {
        Some(key_id) => key_id,
        None if args.authorise => {
            authorise_registry_publish_key(&api_base, &manifest.namespace, &manifest.name, &artifact.kind, args.no_open)?
        }
        None => {
            return Err(crate::error::CompileError::without_span(format!(
                "publishing {}/{} requires a wallet-authorised publishing key; run `cellc publish --authorise` for the continuous browser flow, or pass an existing --capability-key-id",
                manifest.namespace, manifest.name
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication));
        }
    };
    let issued_at = current_utc_timestamp();
    let expires_at = utc_timestamp_after_seconds(10 * 60);
    let nonce = registry_publish_nonce(
        &registry_origin,
        &manifest.namespace,
        &manifest.name,
        &manifest.release,
        &source_hash,
        &capability_key_id,
        &issued_at,
    );
    let payload = crate::package::registry::RegistryPublishPayload {
        protocol: crate::package::registry::REGISTRY_PUBLISH_PROTOCOL.to_string(),
        action: crate::package::registry::PUBLISH_ACTION.to_string(),
        registry_origin,
        namespace: manifest.namespace,
        name: manifest.name,
        version: manifest.release,
        source_hash: source_hash.clone(),
        manifest_hash,
        lockfile_hash: crate::package::registry::compute_lockfile_hash(std::path::Path::new(".")),
        capability_key_id,
        nonce,
        issued_at,
        expires_at,
        cli_version: crate::VERSION.to_string(),
        artifact,
        registry_entry,
    };
    let canonical_payload = registry_publish_canonical_payload(&payload)?;
    if args.print_payload {
        return print_json(&serde_json::json!({ "endpoint": endpoint, "payload": payload, "canonical_payload": canonical_payload }));
    }
    let capability_signature =
        if let Some(signature) = args.capability_signature.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_SIGNATURE").ok()) {
            signature
        } else {
            sign_registry_publish_payload(&payload.capability_key_id, &canonical_payload)?
        };
    let request = crate::package::registry::RegistryPublishRequest {
        payload,
        capability_signature: crate::package::registry::RegistryCapabilitySignature {
            algorithm: "p256-sha256".to_string(),
            signature: capability_signature,
        },
        source_snapshot: crate::package::registry::RegistrySourceSnapshot {
            content_base64: base64::engine::general_purpose::STANDARD.encode(&bundle_bytes),
            content_type: "application/vnd.cellscript.artifact-bundle+json".to_string(),
            size_bytes: bundle_bytes.len() as u64,
            source_hash,
        },
    };
    let idempotency_key = resolve_registry_publish_idempotency_key(args.idempotency_key.as_deref(), &request)?;
    submit_registry_publish_request(&endpoint, &request, &idempotency_key, args.json)
}

fn declared_artifact_descriptor(kind: &str, language: &str) -> Result<crate::package::registry::RegistryArtifactDescriptor> {
    let allowed_language = match kind {
        "runtime_verifier" | "deployable_contract" => matches!(language, "cellscript" | "rust" | "c" | "javascript" | "other"),
        "reproducible_binary" => matches!(language, "rust" | "c" | "other"),
        "template" => matches!(language, "cellscript" | "rust" | "c" | "javascript" | "other" | "unspecified"),
        "source_library" | "profile_library" => {
            return Err(crate::error::CompileError::without_span(
                "source_library and profile_library use the native CellScript package publish path",
            ));
        }
        _ => return Err(crate::error::CompileError::without_span(format!("unknown artifact kind '{kind}'"))),
    };
    if !allowed_language {
        return Err(crate::error::CompileError::without_span(format!(
            "language '{language}' is not valid for artifact kind '{kind}'"
        )));
    }
    let (profile, consumption_mode) = match kind {
        "runtime_verifier" => ("ckb_executable", "tcb"),
        "deployable_contract" => ("ckb_executable", "deployment"),
        "reproducible_binary" => ("reproducible_build", "tcb"),
        "template" => ("copy_material", "copy"),
        _ => unreachable!("artifact kind was checked above"),
    };
    Ok(crate::package::registry::RegistryArtifactDescriptor {
        kind: kind.to_string(),
        profile: profile.to_string(),
        consumption_mode: consumption_mode.to_string(),
        language: language.to_string(),
    })
}

fn declared_bundle_object(bundle: &DeclaredArtifactBundle, role: &str) -> Result<Vec<u8>> {
    let mut objects = bundle.objects.iter().filter(|object| object.role == role);
    let object = objects
        .next()
        .ok_or_else(|| crate::error::CompileError::without_span(format!("artifact bundle is missing required '{role}' object")))?;
    if objects.next().is_some() {
        return Err(crate::error::CompileError::without_span(format!("artifact bundle contains more than one '{role}' object")));
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(&object.content_base64).map_err(|error| {
        crate::error::CompileError::without_span(format!("artifact bundle '{role}' object is not valid base64: {error}"))
    })?;
    if bytes.is_empty() {
        return Err(crate::error::CompileError::without_span(format!("artifact bundle '{role}' object must not be empty")));
    }
    Ok(bytes)
}

fn validate_declared_bundle_roles(bundle: &DeclaredArtifactBundle, profile: &str, contract: &serde_json::Value) -> Result<()> {
    let mut required = match profile {
        "ckb_executable" => vec!["source", "executable", "abi"],
        "reproducible_build" => vec!["source", "executable", "build_recipe"],
        "copy_material" => vec!["source"],
        other => {
            return Err(crate::error::CompileError::without_span(format!("unsupported declared artifact profile '{other}'")));
        }
    };
    if contract.pointer("/security/audit_report_hash").is_some() {
        required.push("audit_report");
    }
    if profile == "ckb_executable" && contract.pointer("/build/reproducible").and_then(serde_json::Value::as_bool) == Some(true) {
        required.push("build_recipe");
    }
    let mut seen = BTreeSet::new();
    for object in &bundle.objects {
        if !required.contains(&object.role.as_str()) {
            return Err(crate::error::CompileError::without_span(format!(
                "artifact bundle role '{}' is not allowed for profile '{profile}'",
                object.role
            )));
        }
        if !seen.insert(object.role.as_str()) {
            return Err(crate::error::CompileError::without_span(format!(
                "artifact bundle contains more than one '{}' object",
                object.role
            )));
        }
    }
    for role in required {
        if !seen.contains(role) {
            return Err(crate::error::CompileError::without_span(format!("artifact bundle is missing required '{role}' object")));
        }
    }
    Ok(())
}

fn validate_declared_artifact_ident(value: &str, field: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let edge = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if bytes.is_empty()
        || bytes.len() > 64
        || !edge(bytes[0])
        || !edge(*bytes.last().expect("non-empty identifier"))
        || !bytes.iter().all(|byte| edge(*byte) || matches!(*byte, b'_' | b'-'))
    {
        return Err(crate::error::CompileError::without_span(format!(
            "artifact {field} must be 1-64 lowercase letters or numbers, with '_' or '-' only between characters"
        )));
    }
    Ok(())
}

fn validate_declared_artifact_release(value: &str) -> Result<()> {
    let mut core = value.split(['-', '+']).next().unwrap_or_default().split('.');
    let valid = (0..3).all(|_| core.next().is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())))
        && core.next().is_none();
    if !valid {
        return Err(crate::error::CompileError::without_span("artifact release must be semver-like"));
    }
    Ok(())
}

fn build_publish_registry_version(
    manifest: &PackageManifest,
    result: &crate::CompileResult,
    source_hash: &str,
) -> Result<crate::package::registry::RegistryVersion> {
    let mut deps = BTreeMap::new();
    for (dep_name, dep) in &manifest.dependencies {
        let (namespace, version) = match dep {
            crate::package::Dependency::Simple(version) => (manifest.package.namespace.clone().unwrap_or_default(), version.clone()),
            crate::package::Dependency::Detailed(detail) => {
                let namespace = detail.namespace.clone().unwrap_or_else(|| manifest.package.namespace.clone().unwrap_or_default());
                (namespace, detail.version.clone())
            }
        };
        deps.insert(dep_name.clone(), crate::package::registry::RegistryDependencyRef { namespace, version });
    }

    Ok(crate::package::registry::RegistryVersion {
        version: manifest.package.version.clone(),
        tag: format!("v{}", manifest.package.version),
        source_hash: source_hash.to_string(),
        cellscript_version: result.metadata.compiler_version.clone(),
        compiler_requirement: manifest.package.cellscript_version.clone(),
        edition: result.metadata.edition,
        compatibility_profile_hash: hash_json_value("compatibility_profile", &result.metadata.compatibility_profile)?,
        dependencies: deps,
        abi_index: Some(metadata_abi_hash(&result.metadata)?),
        schema_hash: Some(result.metadata.molecule_schema_manifest.manifest_hash.clone()),
        license: if manifest.package.license.is_empty() { None } else { Some(manifest.package.license.clone()) },
        released_at: Some(current_utc_timestamp()),
        status: crate::package::registry::RegistryEntryStatus::SourcePublished,
        yanked: false,
        yanked_at: None,
        yanked_reason: None,
        replaced_by: None,
        audit: None,
    })
}

fn build_publish_registry_entry(
    manifest: &PackageManifest,
    namespace: &str,
    version_entry: crate::package::registry::RegistryVersion,
    artifact: &crate::package::registry::RegistryArtifactDescriptor,
    public_interface: &crate::interface::PackageInterface,
    interface_hash: &str,
) -> Result<serde_json::Value> {
    let index = crate::package::registry::RegistryIndex {
        schema_version: crate::package::registry::RegistryIndex::CURRENT_SCHEMA_VERSION,
        name: manifest.package.name.clone(),
        namespace: namespace.to_string(),
        versions: vec![version_entry],
    };
    let mut value = serde_json::to_value(index).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to serialize registry entry for publish: {}", error))
    })?;
    let Some(object) = value.as_object_mut() else {
        return Err(crate::error::CompileError::without_span("registry entry did not serialize as a JSON object"));
    };
    object.insert(
        "artifact".to_string(),
        serde_json::to_value(artifact).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize CellScript artifact descriptor: {}", error))
        })?,
    );
    let published = object
        .get_mut("versions")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|versions| versions.first_mut())
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| crate::error::CompileError::without_span("registry entry has no publish release"))?;
    published.remove("status");
    published.remove("yanked");
    published.insert("verification_status".to_string(), serde_json::Value::String("pending".to_string()));
    published.insert("interface_hash".to_string(), serde_json::Value::String(interface_hash.to_string()));
    published.insert(
        "interface".to_string(),
        serde_json::to_value(public_interface).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to serialize canonical public interface: {error}"))
        })?,
    );
    published.insert("deployment_status".to_string(), serde_json::Value::String("not_applicable".to_string()));
    published.insert("availability_status".to_string(), serde_json::Value::String("active".to_string()));
    if !manifest.package.repository.is_empty() {
        object.insert("repository".to_string(), serde_json::Value::String(manifest.package.repository.clone()));
    }
    if !manifest.package.description.is_empty() {
        object.insert("description".to_string(), serde_json::Value::String(manifest.package.description.clone()));
    }
    if !manifest.package.homepage.is_empty() {
        object.insert("homepage".to_string(), serde_json::Value::String(manifest.package.homepage.clone()));
    }
    if !manifest.package.documentation.is_empty() {
        object.insert("documentation".to_string(), serde_json::Value::String(manifest.package.documentation.clone()));
    }
    if !manifest.package.keywords.is_empty() {
        object.insert(
            "keywords".to_string(),
            serde_json::to_value(&manifest.package.keywords).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize package keywords for publish: {}", error))
            })?,
        );
    }
    if !manifest.package.categories.is_empty() {
        object.insert(
            "categories".to_string(),
            serde_json::to_value(&manifest.package.categories).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to serialize package categories for publish: {}", error))
            })?,
        );
    }
    Ok(value)
}

fn cellscript_artifact_descriptor(kind: Option<&str>) -> Result<crate::package::registry::RegistryArtifactDescriptor> {
    let kind = kind.unwrap_or("source_library");
    if !matches!(kind, "source_library" | "profile_library") {
        return Err(crate::error::CompileError::without_span(
            "CellScript package artifact kind must be source_library or profile_library",
        ));
    }
    Ok(crate::package::registry::RegistryArtifactDescriptor {
        kind: kind.to_string(),
        profile: "cellscript_source".to_string(),
        consumption_mode: "dependency".to_string(),
        language: "cellscript".to_string(),
    })
}

pub(super) fn resolve_registry_api_base(api_url: Option<String>) -> Result<String> {
    let value = api_url
        .or_else(|| std::env::var("CELLSCRIPT_REGISTRY_API_URL").ok())
        .or_else(|| std::env::var("CELLSCRIPT_REGISTRY_ORIGIN").ok())
        .unwrap_or_else(|| crate::package::registry::DEFAULT_PUBLIC_REGISTRY_ORIGIN.to_string());
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(crate::error::CompileError::without_span("registry API URL is empty"));
    }
    let _ = parse_registry_api_url(trimmed)?;
    Ok(trimmed.to_string())
}

pub(super) fn registry_origin_from_api_base(api_base: &str) -> Result<String> {
    Ok(parse_registry_api_url(api_base)?.origin().ascii_serialization())
}

fn parse_registry_api_url(api_base: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(api_base)
        .map_err(|error| crate::error::CompileError::without_span(format!("invalid registry API URL '{}': {}", api_base, error)))?;
    let Some(host) = url.host_str() else {
        return Err(crate::error::CompileError::without_span(format!("registry API URL '{}' has no host", api_base)));
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let is_loopback =
        host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback());
    match url.scheme() {
        "https" => {}
        "http" if is_loopback => {}
        "http" => {
            return Err(crate::error::CompileError::without_span(
                "registry API URL must use HTTPS; plaintext HTTP is allowed only for loopback development servers",
            ));
        }
        scheme => {
            return Err(crate::error::CompileError::without_span(format!(
                "registry API URL '{}' uses unsupported scheme '{}'",
                api_base, scheme
            )));
        }
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(crate::error::CompileError::without_span("registry API URL must not contain credentials"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(crate::error::CompileError::without_span("registry API URL must not contain a query or fragment"));
    }
    Ok(url)
}

fn registry_publish_endpoint(api_base: &str, namespace: &str, name: &str) -> String {
    format!("{}/v1/artifacts/{}/{}/releases", api_base.trim_end_matches('/'), namespace, name)
}

fn resolve_registry_publish_idempotency_key(
    cli_value: Option<&str>,
    request: &crate::package::registry::RegistryPublishRequest,
) -> Result<String> {
    let value = if let Some(value) = cli_value {
        value.to_string()
    } else if let Ok(value) = std::env::var("CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY") {
        value
    } else {
        let digest = hash_json_value("registry publish request", request)?;
        format!("cellc-publish-{}", digest)
    };
    let trimmed = value.trim();
    if trimmed.len() < 16 || trimmed.len() > 160 || !trimmed.bytes().all(is_idempotency_key_byte) {
        return Err(crate::error::CompileError::without_span(
            "publish Idempotency-Key must be 16..160 ASCII token characters: letters, digits, '.', '_', ':', or '-'",
        ));
    }
    Ok(trimmed.to_string())
}

fn is_idempotency_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-')
}

fn read_registry_publish_payload(path: &Path) -> Result<crate::package::registry::RegistryPublishPayload> {
    let value = read_json_value(path)?;
    let payload_value = value.get("payload").cloned().unwrap_or(value);
    serde_json::from_value(payload_value).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse registry publish payload '{}': {}", path.display(), error))
    })
}

fn read_capability_authorisation_payload(path: &Path) -> Result<crate::package::registry::CapabilityAuthorisationPayload> {
    let value = read_json_value(path)?;
    let payload_value = value.get("payload").cloned().unwrap_or(value);
    serde_json::from_value(payload_value).map_err(|error| {
        crate::error::CompileError::without_span(format!(
            "failed to parse capability authorisation payload '{}': {}",
            path.display(),
            error
        ))
    })
}

fn read_capability_revocation_payload(path: &Path) -> Result<crate::package::registry::CapabilityRevocationPayload> {
    let value = read_json_value(path)?;
    let payload_value = value.get("payload").cloned().unwrap_or(value);
    serde_json::from_value(payload_value).map_err(|error| {
        crate::error::CompileError::without_span(format!(
            "failed to parse capability revocation payload '{}': {}",
            path.display(),
            error
        ))
    })
}

fn validate_publish_payload_matches_local_package(
    payload: &crate::package::registry::RegistryPublishPayload,
    registry_origin: &str,
    namespace: &str,
    manifest: &PackageManifest,
    source_hash: &str,
    public_interface: &crate::interface::PackageInterface,
    interface_hash: &str,
) -> Result<()> {
    if payload.protocol != crate::package::registry::REGISTRY_PUBLISH_PROTOCOL
        || payload.action != crate::package::registry::PUBLISH_ACTION
    {
        return Err(crate::error::CompileError::without_span("publish payload has the wrong protocol or action"));
    }
    if payload.registry_origin != registry_origin {
        return Err(crate::error::CompileError::without_span(format!(
            "publish payload registry_origin '{}' does not match API origin '{}'",
            payload.registry_origin, registry_origin
        )));
    }
    if payload.namespace != namespace || payload.name != manifest.package.name || payload.version != manifest.package.version {
        return Err(crate::error::CompileError::without_span(format!(
            "publish payload targets {}/{} v{}, but local package is {}/{} v{}",
            payload.namespace, payload.name, payload.version, namespace, manifest.package.name, manifest.package.version
        )));
    }
    if payload.source_hash != source_hash {
        return Err(crate::error::CompileError::without_span(format!(
            "publish payload source_hash '{}' does not match local source_hash '{}'",
            payload.source_hash, source_hash
        )));
    }
    let payload_interface_hash = payload
        .registry_entry
        .get("versions")
        .and_then(serde_json::Value::as_array)
        .and_then(|versions| versions.first())
        .and_then(|version| version.get("interface_hash"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if payload_interface_hash != interface_hash {
        return Err(crate::error::CompileError::without_span(format!(
            "publish payload interface_hash '{}' does not match local canonical interface '{}'",
            payload_interface_hash, interface_hash
        )));
    }
    let payload_interface = payload
        .registry_entry
        .get("versions")
        .and_then(serde_json::Value::as_array)
        .and_then(|versions| versions.first())
        .and_then(|version| version.get("interface"))
        .cloned()
        .ok_or_else(|| crate::error::CompileError::without_span("publish payload is missing the canonical public interface"))?;
    let payload_interface: crate::interface::PackageInterface = serde_json::from_value(payload_interface)
        .map_err(|error| crate::error::CompileError::without_span(format!("publish payload public interface is invalid: {error}")))?;
    if &payload_interface != public_interface || crate::interface::hash(&payload_interface) != interface_hash {
        return Err(crate::error::CompileError::without_span(
            "publish payload public interface does not match the locally compiled canonical interface",
        ));
    }
    if !matches!(payload.artifact.kind.as_str(), "source_library" | "profile_library")
        || payload.artifact.profile != "cellscript_source"
        || payload.artifact.consumption_mode != "dependency"
        || payload.artifact.language != "cellscript"
    {
        return Err(crate::error::CompileError::without_span(
            "CellScript package publish payload must declare the source_library/cellscript_source dependency contract",
        ));
    }
    Ok(())
}

fn registry_publish_canonical_payload(payload: &crate::package::registry::RegistryPublishPayload) -> Result<String> {
    let value = serde_json::to_value(payload)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize publish payload: {}", error)))?;
    canonical_json_string(&value)
}

fn canonical_json_string(value: &serde_json::Value) -> Result<String> {
    serde_json::to_string(&crate::canonical_json::canonical_json_value(value))
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize canonical JSON: {}", error)))
}

fn build_registry_source_snapshot(
    source_snapshot: Option<&Path>,
    root: &Path,
    manifest: &PackageManifest,
    source_hash: &str,
) -> Result<crate::package::registry::RegistrySourceSnapshot> {
    let (bytes, content_type) = if let Some(path) = source_snapshot {
        let bytes = std::fs::read(path).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read source snapshot '{}': {}", path.display(), error))
        })?;
        (bytes, source_snapshot_content_type(path).to_string())
    } else {
        (build_generated_source_snapshot_bytes(root, manifest)?, "application/vnd.cellscript.source-snapshot+json".to_string())
    };
    if bytes.is_empty() {
        return Err(crate::error::CompileError::without_span("source snapshot is empty"));
    }
    Ok(crate::package::registry::RegistrySourceSnapshot {
        content_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        content_type,
        size_bytes: bytes.len() as u64,
        source_hash: source_hash.to_string(),
    })
}

fn source_snapshot_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or_default() {
        "json" => "application/vnd.cellscript.source-snapshot+json",
        "tar" => "application/x-tar",
        "tgz" | "gz" => "application/gzip",
        _ => "application/octet-stream",
    }
}

fn build_generated_source_snapshot_bytes(root: &Path, manifest: &PackageManifest) -> Result<Vec<u8>> {
    let files = collect_publish_snapshot_files(root, manifest)?;
    // The snapshot's source files must be exactly the set the compiler and
    // the signed source_hash cover; a drift here could publish a differently
    // compiled package behind an unchanged hash.
    let authoritative = publish_authoritative_source_files(root, manifest)?;
    let snapshot_cells =
        files.iter().filter(|path| path.extension().is_some_and(|ext| ext == "cell")).cloned().collect::<BTreeSet<_>>();
    if snapshot_cells != authoritative {
        return Err(crate::error::CompileError::without_span(
            "publish snapshot source files disagree with the authoritative compile file set",
        ));
    }
    let mut entries = Vec::new();
    for path in files {
        let bytes = std::fs::read(&path)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to read '{}': {}", path.display(), error)))?;
        entries.push(serde_json::json!({
            "path": normalized_relative_path(root, &path),
            "blake2b256": crate::hex_encode(&crate::ckb_blake2b256(&bytes)),
            "content_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
        }));
    }
    let snapshot = serde_json::json!({
        "schema": "cellscript-source-snapshot-v1",
        "generated_by": crate::VERSION,
        "package": {
            "namespace": manifest.package.namespace.as_deref(),
            "name": &manifest.package.name,
            "version": &manifest.package.version,
        },
        "files": entries,
    });
    serde_json::to_vec(&snapshot)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize generated source snapshot: {}", error)))
}

/// The authoritative `.cell` file set for publication, straight from the
/// single compiler-side implementation (explicit entry semantics included).
fn publish_authoritative_source_files(root: &Path, manifest: &PackageManifest) -> Result<BTreeSet<std::path::PathBuf>> {
    let utf8_root = Utf8PathBuf::from_path_buf(root.to_path_buf())
        .map_err(|path| crate::error::CompileError::without_span(format!("package root '{}' is not valid UTF-8", path.display())))?;
    let entry = crate::explicit_manifest_entry(&utf8_root)?;
    let files = crate::package_source_file_set(&utf8_root, &manifest.package.source_roots, entry.as_deref())?;
    Ok(files.into_iter().map(|path| path.as_std_path().to_path_buf()).collect())
}

fn collect_publish_snapshot_files(root: &Path, manifest: &PackageManifest) -> Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();
    let manifest_path = root.join("Cell.toml");
    if manifest_path.is_file() {
        files.insert(manifest_path);
    }
    let lockfile_path = root.join("Cell.lock");
    if lockfile_path.is_file() {
        files.insert(lockfile_path);
    }

    // Source selection is the single authoritative compile set: no local
    // walker, no single-file source roots, no drift from the hashed set.
    for path in publish_authoritative_source_files(root, manifest)? {
        files.insert(path);
    }
    Ok(files.into_iter().collect())
}

fn normalized_relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn compile_cli_input(input: Option<&PathBuf>, options: CompileOptions) -> Result<crate::CompileResult> {
    let input_path = input.cloned().unwrap_or_else(|| PathBuf::from("."));
    let input = Utf8Path::from_path(&input_path)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", input_path.display())))?;
    compile_path(input, options)
}

fn compile_failure_diagnostics(input: &Utf8Path, options: CompileOptions, fallback: CompileError) -> Vec<CompileError> {
    if fallback.category != crate::error::CompileErrorCategory::Compilation
        || fallback.code.as_deref().is_some_and(|code| code.starts_with("CG"))
    {
        return vec![fallback];
    }
    let report = compile_path_metadata_with_diagnostics(input, options);
    if report.diagnostics.is_empty() {
        vec![fallback]
    } else {
        report.diagnostics
    }
}

fn diagnostics_to_error(diagnostics: &[CompileError]) -> CompileError {
    match diagnostics {
        [] => CompileError::without_span("compile failed"),
        [diagnostic] => diagnostic.clone(),
        _ => {
            CompileError::without_span(format!("aborting due to {} diagnostics", diagnostics.len())).with_related(diagnostics.to_vec())
        }
    }
}

fn read_metadata_json(path: &Path) -> Result<CompileMetadata> {
    let bytes = std::fs::read(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read metadata '{}': {}", path.display(), error))
            .with_category(crate::error::CompileErrorCategory::Io)
            .with_source(error)
    })?;
    serde_json::from_slice(&bytes)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to parse metadata '{}': {}", path.display(), error)))
}

fn read_or_compile_interface(path: &Path) -> Result<crate::interface::PackageInterface> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("json") {
        let bytes = std::fs::read(path).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to read interface '{}': {error}", path.display()))
        })?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to parse interface '{}': {error}", path.display()))
        })?;
        let interface_value = value.get("interface").cloned().unwrap_or(value);
        let interface = serde_json::from_value(interface_value).map_err(|error| {
            crate::error::CompileError::without_span(format!("invalid public interface '{}': {error}", path.display()))
        })?;
        crate::interface::validate(&interface)?;
        return Ok(interface);
    }
    let input = Utf8Path::from_path(path)
        .ok_or_else(|| crate::error::CompileError::without_span(format!("path '{}' is not valid UTF-8", path.display())))?;
    Ok(compile_path(input, CompileOptions::default())?.metadata.public_interface)
}

fn render_public_interface_human(interface: &crate::interface::PackageInterface, interface_hash: &str) -> String {
    let mut output =
        format!("package {}\ninterface {}\nruntime {}\n", interface.module, interface_hash, interface.runtime_contract.target_profile);
    if interface.types.is_empty() && interface.constants.is_empty() && interface.callables.is_empty() {
        output.push_str("exports (none)\n");
        return output;
    }
    output.push_str("exports\n");
    for item in &interface.types {
        output.push_str(&format!(
            "  {} {} {}{}  [layout {}]\n",
            item.visibility,
            item.kind,
            item.name,
            render_interface_type_parameters(&item.type_parameters),
            item.layout_identity
        ));
    }
    for item in &interface.constants {
        output.push_str(&format!("  {} const {}: {}\n", item.visibility, item.name, item.r#type));
    }
    for item in &interface.callables {
        let params = item.params.iter().map(render_interface_param).collect::<Vec<_>>().join(", ");
        let returns = if !item.outputs.is_empty() {
            format!(" -> ({})", item.outputs.iter().map(render_interface_param).collect::<Vec<_>>().join(", "))
        } else {
            item.return_type.as_ref().map(|ty| format!(" -> {ty}")).unwrap_or_default()
        };
        output.push_str(&format!(
            "  {} {} {}{}({}){}\n",
            item.visibility,
            item.kind,
            item.name,
            render_interface_type_parameters(&item.type_parameters),
            params,
            returns
        ));
    }
    output
}

fn render_interface_type_parameters(params: &[crate::interface::InterfaceTypeParameter]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let fixed_value = ["copy", "drop", "store", "fixed", "serializable", "non_linear"];
    let params = params
        .iter()
        .map(|param| {
            let mut rendered = if param.phantom { format!("phantom {}", param.name) } else { param.name.clone() };
            if !param.constraints.is_empty() {
                let constraints = if param.constraints.iter().map(String::as_str).eq(fixed_value) {
                    crate::ast::ValueAbility::FIXED_VALUE_PROFILE_NAME.to_string()
                } else {
                    param.constraints.join(" + ")
                };
                rendered.push_str(": ");
                rendered.push_str(&constraints);
            }
            rendered
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{params}>")
}

fn render_interface_param(param: &crate::interface::InterfaceParam) -> String {
    let mut rendered = String::new();
    if param.mutable {
        rendered.push_str("mut ");
    }
    if param.reference {
        rendered.push('&');
    }
    if param.source != "default" {
        rendered.push_str(&param.source);
        rendered.push(' ');
    }
    rendered.push_str(&param.name);
    rendered.push_str(": ");
    rendered.push_str(&param.r#type);
    rendered
}

fn read_generated_lockfile(root: &Path) -> Result<Lockfile> {
    Lockfile::read_from_root(root)?.ok_or_else(|| CliDiagnosticCode::GeneratedLockfileMissing.error())
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
    if let Some(policy) = &metadata.runtime.policy_artifact {
        policy.validate()?;
        let variants = policy
            .declaration
            .actions
            .iter()
            .filter(|variant| action_name.is_none_or(|name| variant.action == name))
            .map(|variant| {
                let matches = metadata.actions.iter().filter(|action| action.name == variant.action).collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err(CompileError::without_span(format!("policy action '{}' must resolve exactly once", variant.action)));
                }
                Ok(matches[0])
            })
            .collect::<Result<Vec<_>>>()?;
        if variants.is_empty() {
            return Err(CompileError::without_span(format!(
                "action '{}' is not an exported variant of policy '{}'",
                action_name.unwrap_or(""),
                policy.declaration.name
            )));
        }
        return Ok(variants);
    }
    if let Some(action_name) = action_name {
        let action =
            metadata.actions.iter().find(|action| action.name == action_name).ok_or_else(|| {
                crate::error::CompileError::without_span(format!("action '{}' was not found in metadata", action_name))
            })?;
        return Ok(vec![action]);
    }

    if metadata.actions.is_empty() && metadata.locks.is_empty() {
        return Err(crate::error::CompileError::without_span("no actions or locks found in metadata for generated builder"));
    }

    Ok(metadata.actions.iter().collect())
}

fn policy_entry_witness(args: &EntryWitnessArgs, metadata: &CompileMetadata) -> Result<()> {
    let artifact_name = args.artifact.as_deref().expect("selected policy artifact");
    if args.lock.is_some() {
        return Err(CompileError::without_span("entry-witness --artifact supports Type policies, not --lock"));
    }
    let action_name =
        args.action.as_deref().ok_or_else(|| CompileError::without_span("entry-witness --artifact requires --action"))?;
    let hash = args.script_hash.as_deref().ok_or_else(|| {
        CompileError::without_span("entry-witness --artifact requires --script-hash with the full deployed Script hash")
    })?;
    let script_hash: [u8; 32] = decode_hex_arg("script-hash", hash, Some(32))?.try_into().expect("validated full Script hash");
    let selected = selected_builder_actions(metadata, Some(action_name))?;
    let action = selected[0];
    let runtime_bound = crate::runtime_bound_param_names(&action.params);
    let params =
        action.params.iter().filter(|param| crate::param_consumes_entry_witness_payload(param, &runtime_bound)).collect::<Vec<_>>();
    if args.args.len() != params.len() {
        return Err(CompileError::without_span(format!(
            "policy action '{action_name}' expects {} witness payload arg(s), got {}",
            params.len(),
            args.args.len()
        )));
    }
    let values =
        params.iter().zip(&args.args).map(|(param, value)| parse_entry_witness_arg(param, value)).collect::<Result<Vec<_>>>()?;
    let record = crate::artifact::encode_policy_action_record(metadata, &script_hash, action_name, &values)?;
    let bundle = crate::policy_witness::encode_policy_witness_bundle(std::slice::from_ref(&record))
        .map_err(|error| CompileError::without_span(format!("failed to encode policy witness: {error}")))?;
    if let Some(path) = &args.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, &bundle)?;
    }
    let witness_hex = crate::hex_encode(&bundle);
    CommandOutcome {
        machine: serde_json::json!({
            "status": "ok",
            "abi": crate::policy_witness::POLICY_WITNESS_ABI,
            "placement_abi": crate::artifact::POLICY_WITNESS_PLACEMENT_ABI,
            "witness_args_field": crate::artifact::POLICY_WITNESS_PLACEMENT_FIELD,
            "witness_source": crate::artifact::POLICY_WITNESS_PLACEMENT_SOURCE,
            "policy_artifact": artifact_name,
            "entry_kind": "type-policy-action",
            "entry": action_name,
            "role": "type",
            "tag": record.tag,
            "script_hash": crate::hex_encode(&script_hash),
            "script_hash_is_authentication": false,
            "placement_performed": false,
            "requires_shared_witness_aggregation": true,
            "requires_pre_signing_placement": true,
            "raw_v1_compatible": false,
            "witness_hex": witness_hex,
            "witness_size_bytes": bundle.len(),
            "payload_args": values.len(),
            "payload_params": params.iter().map(|param| &param.name).collect::<Vec<_>>(),
            "output": args.output.as_ref().map(|path| path.display().to_string()),
        }),
        human_lines: vec![
            format!("Policy witness encoded: {artifact_name} / {action_name} (tag {})", record.tag),
            "Full Script hash is caller-supplied, not authentication; aggregate and place before signing.".to_string(),
            witness_hex,
        ],
    }
    .emit(args.json)
}

fn read_lockfile_path(path: &Path) -> Result<Lockfile> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read lockfile '{}': {}", path.display(), error))
    })?;
    let lockfile: Lockfile = toml::from_str(&content).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse lockfile '{}': {}", path.display(), error))
    })?;
    lockfile.validate_schema()?;
    Ok(lockfile)
}

fn read_deployed_manifest_path(path: &Path) -> Result<crate::package::DeployedManifest> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read deployed manifest '{}': {}", path.display(), error))
    })?;
    let manifest: crate::package::DeployedManifest = toml::from_str(&content).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to parse deployed manifest '{}': {}", path.display(), error))
    })?;
    manifest.validate_schema()?;
    Ok(manifest)
}

fn verify_builder_lockfile_identity(
    lockfile_path: &Path,
    metadata: &CompileMetadata,
    metadata_hash: &str,
) -> Result<serde_json::Value> {
    let lockfile = read_lockfile_path(lockfile_path)?;
    let expected_build = locked_build_info_from_metadata(metadata)?;
    let mut violations = Vec::new();
    if lockfile.package.edition != metadata.edition {
        violations.push(format!(
            "edition mismatch: Cell.lock package has '{}' but metadata has '{}'",
            lockfile.package.edition, metadata.edition
        ));
    }

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
            if build.edition != metadata.edition {
                violations.push(format!(
                    "edition mismatch: Cell.lock build has '{}' but metadata has '{}'",
                    build.edition, metadata.edition
                ));
            }
            if build.compatibility_profile_hash != expected_build.compatibility_profile_hash {
                violations.push(format!(
                    "compatibility_profile_hash mismatch: Cell.lock has '{}', metadata has '{}'",
                    build.compatibility_profile_hash, expected_build.compatibility_profile_hash
                ));
            }
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
            "edition",
            "compatibility_profile_hash",
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
    for (label, edition) in [("Cell.lock package", lockfile.package.edition), ("Deployed.toml package", deployed.package.edition)] {
        if edition != metadata.edition {
            violations.push(format!("edition mismatch: {} has '{}' but metadata has '{}'", label, edition, metadata.edition));
        }
    }

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
            if build.edition != metadata.edition {
                violations.push(format!(
                    "edition mismatch: Deployed.toml build has '{}' but metadata has '{}'",
                    build.edition, metadata.edition
                ));
            }
            if build.compatibility_profile_hash != expected_build.compatibility_profile_hash {
                violations.push(format!(
                    "compatibility_profile_hash mismatch: Deployed.toml has '{}', metadata has '{}'",
                    build.compatibility_profile_hash, expected_build.compatibility_profile_hash
                ));
            }
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
        if deployment.edition != metadata.edition {
            violations.push(format!(
                "edition mismatch for network '{}': deployment has '{}' but metadata has '{}'",
                deployment.network, deployment.edition, metadata.edition
            ));
        }
        if deployment.compatibility_profile_hash != expected_build.compatibility_profile_hash {
            violations.push(format!(
                "compatibility_profile_hash mismatch for network '{}': Deployed.toml has '{}', metadata has '{}'",
                deployment.network, deployment.compatibility_profile_hash, expected_build.compatibility_profile_hash
            ));
        }
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
            "edition",
            "compatibility_profile_hash",
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

fn resolution_options(
    features: &[String],
    all_features: bool,
    no_default_features: bool,
    environment: Option<&str>,
    offline: bool,
    frozen: bool,
    scope: crate::package::DependencyScope,
) -> crate::package::ResolutionOptions {
    crate::package::ResolutionOptions {
        scope,
        features: features.iter().cloned().collect(),
        all_features,
        no_default_features,
        environment: environment.map(str::to_string),
        offline: offline || frozen,
    }
}

fn inspection_resolution_options(
    scope: &str,
    features: &[String],
    all_features: bool,
    no_default_features: bool,
    environment: Option<&str>,
    offline: bool,
) -> Result<crate::package::ResolutionOptions> {
    let scope = match scope {
        "runtime" => crate::package::DependencyScope::Runtime,
        "test" => crate::package::DependencyScope::Test,
        other => return Err(CompileError::without_span(format!("unsupported dependency scope '{other}'; expected runtime or test"))),
    };
    Ok(resolution_options(features, all_features, no_default_features, environment, offline, false, scope))
}

fn emit_inspection_document<T: serde::Serialize>(label: &str, value: &T, output: Option<&Path>, json: bool) -> Result<()> {
    if let Some(output) = output {
        if let Some(parent) = output.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        std::fs::write(output, bytes)?;
        return CommandOutcome {
            machine: serde_json::json!({
                "status": "ok",
                "document": label,
                "output": output.display().to_string(),
            }),
            human_lines: vec![format!("{} written to {}", label, output.display())],
        }
        .emit(json);
    }
    print_json(&serde_json::to_value(value)?)
}

fn build_resolution_options(args: &BuildArgs, scope: crate::package::DependencyScope) -> crate::package::ResolutionOptions {
    resolution_options(
        &args.features,
        args.all_features,
        args.no_default_features,
        args.environment.as_deref(),
        args.offline,
        args.frozen,
        scope,
    )
}

fn inspect_build_plan_for_build(input: &Path, args: &BuildArgs) -> Result<crate::package::inspection::BuildPlan> {
    let resolution = build_resolution_options(args, crate::package::DependencyScope::Runtime);
    let graph = crate::package::inspection::resolve_graph(input, args.package.as_deref(), &resolution)?;
    crate::package::inspection::build_plan(
        &graph,
        &crate::package::inspection::BuildPlanOptions {
            source_contracts: args.source_contracts,
            target: args.target.clone(),
            target_profile: args.target_profile.clone(),
            release: args.release,
            debug: false,
            primitive_compat: args.primitive_compat.clone(),
            entry_action: args.entry_action.clone(),
            entry_lock: args.entry_lock.clone(),
            artifact: args.artifact.clone(),
            production: args.production,
            deny_fail_closed: args.deny_fail_closed,
            deny_ckb_runtime: args.deny_ckb_runtime,
            deny_runtime_obligations: args.deny_runtime_obligations,
        },
    )
}

fn validate_build_unit_result(
    unit: &crate::package::inspection::BuildUnit,
    result: &crate::CompileResult,
    output_path: &Utf8Path,
) -> Result<()> {
    let mut mismatches = Vec::new();
    if unit.artifact_format != result.artifact_format.display_name() {
        mismatches.push(format!(
            "artifact format planned '{}' but compiled '{}'",
            unit.artifact_format,
            result.artifact_format.display_name()
        ));
    }
    if unit.target_profile != result.metadata.target_profile.name {
        mismatches
            .push(format!("target profile planned '{}' but compiled '{}'", unit.target_profile, result.metadata.target_profile.name));
    }
    if unit.compatibility_profile != result.metadata.compatibility_profile {
        mismatches.push(format!(
            "compatibility profile planned '{}' but compiled '{}'",
            unit.compatibility_profile.id, result.metadata.compatibility_profile.id
        ));
    }
    if unit.outputs.artifact != output_path.as_str() {
        mismatches.push(format!("artifact path planned '{}' but build selected '{}'", unit.outputs.artifact, output_path));
    }
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(CompileError::without_span(format!(
            "build result diverged from {}: {}",
            crate::package::inspection::BUILD_PLAN_SCHEMA,
            mismatches.join("; ")
        )))
    }
}

fn check_resolution_options(args: &CheckArgs, scope: crate::package::DependencyScope) -> crate::package::ResolutionOptions {
    resolution_options(
        &args.features,
        args.all_features,
        args.no_default_features,
        args.environment.as_deref(),
        args.offline,
        args.frozen,
        scope,
    )
}

fn test_resolution_options(args: &TestArgs) -> crate::package::ResolutionOptions {
    resolution_options(
        &args.features,
        args.all_features,
        args.no_default_features,
        args.environment.as_deref(),
        args.offline,
        args.frozen,
        crate::package::DependencyScope::Test,
    )
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
        TargetProfile::CkbTypeHash => Some(TargetProfile::CkbTypeHash.name().to_string()),
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
    // Absolute destinations are supported by `cellc new`; `--` keeps a
    // destination beginning with '-' from being interpreted as a git option.
    let output = std::process::Command::new("git").arg("init").arg("--quiet").arg("--").arg(path).output().map_err(|error| {
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
    if build {
        return Err(crate::error::CompileError::without_span(
            "--build dependencies are reserved until isolated build-script execution is implemented",
        ));
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
    if let Dependency::Detailed(detailed) = dep
        && let Some(dep_path) = &detailed.path
    {
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
            package: args.package.clone(),
            resolver: None,
            git: Some(git.clone()),
            branch: None,
            tag: None,
            rev: None,
            path: None,
            optional: false,
            features: Vec::new(),
            default_features: true,
            use_environment: args.use_environment.clone(),
            environment_independent: args.environment_independent,
            allow_unverified: false,
            allow_quarantined: false,
        }),
        (_, Some(path)) => Dependency::Detailed(DetailedDependency {
            version: "*".to_string(),
            namespace: None,
            package: args.package.clone(),
            resolver: None,
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: Some(path.display().to_string()),
            optional: false,
            features: Vec::new(),
            default_features: true,
            use_environment: args.use_environment.clone(),
            environment_independent: args.environment_independent,
            allow_unverified: false,
            allow_quarantined: false,
        }),
        _ if args.package.is_some() || args.use_environment.is_some() || args.environment_independent => {
            Dependency::Detailed(DetailedDependency {
                version: "*".to_string(),
                namespace: None,
                package: args.package.clone(),
                resolver: None,
                git: None,
                branch: None,
                tag: None,
                rev: None,
                path: None,
                optional: false,
                features: Vec::new(),
                default_features: true,
                use_environment: args.use_environment.clone(),
                environment_independent: args.environment_independent,
                allow_unverified: false,
                allow_quarantined: false,
            })
        }
        _ => Dependency::Simple("*".to_string()),
    }
}

fn auth_capability_args_from_matches(m: &clap::ArgMatches) -> AuthCapabilityArgs {
    AuthCapabilityArgs {
        registry_origin: m.get_one::<String>("registry-origin").cloned(),
        principal_type: m.get_one::<String>("principal-type").cloned(),
        principal_id: m.get_one::<String>("principal-id").cloned(),
        capability_pubkey: m.get_one::<String>("capability-pubkey").cloned(),
        scopes: m.get_many::<String>("scope").map(|values| values.cloned().collect()).unwrap_or_default(),
        expires: m.get_one::<String>("expires").cloned(),
        capability_expires_at: m.get_one::<String>("capability-expires-at").cloned(),
        json: json_output(m),
    }
}

fn auth_capability_submit_args_from_matches(m: &clap::ArgMatches) -> AuthCapabilitySubmitArgs {
    AuthCapabilitySubmitArgs {
        api_url: m.get_one::<String>("api-url").cloned(),
        payload: m.get_one::<String>("payload").map(PathBuf::from).expect("required payload"),
        wallet_signature: m.get_one::<String>("wallet-signature").map(PathBuf::from).expect("required wallet-signature"),
        json: json_output(m),
    }
}

fn auth_reproducer_create_args_from_matches(m: &clap::ArgMatches) -> AuthReproducerCreateArgs {
    AuthReproducerCreateArgs {
        builder_id: m.get_one::<String>("builder-id").cloned().expect("required builder-id"),
        trust_domain: m.get_one::<String>("trust-domain").cloned().expect("required trust-domain"),
        private_key_output: m.get_one::<String>("private-key-output").map(PathBuf::from),
        json: json_output(m),
    }
}

fn auth_namespace_claim_args_from_matches(m: &clap::ArgMatches) -> AuthNamespaceClaimArgs {
    AuthNamespaceClaimArgs {
        api_url: m.get_one::<String>("api-url").cloned(),
        namespace: m.get_one::<String>("namespace").cloned().expect("required namespace"),
        payload: m.get_one::<String>("payload").map(PathBuf::from).expect("required payload"),
        wallet_signature: m.get_one::<String>("wallet-signature").map(PathBuf::from).expect("required wallet-signature"),
        json: json_output(m),
    }
}

fn auth_capability_revoke_args_from_matches(m: &clap::ArgMatches) -> AuthCapabilityRevokeArgs {
    AuthCapabilityRevokeArgs {
        api_url: m.get_one::<String>("api-url").cloned(),
        registry_origin: m.get_one::<String>("registry-origin").cloned(),
        principal_type: m.get_one::<String>("principal-type").cloned(),
        principal_id: m.get_one::<String>("principal-id").cloned(),
        capability_key_id: m.get_one::<String>("capability-key-id").cloned(),
        payload: m.get_one::<String>("payload").map(PathBuf::from),
        wallet_signature: m.get_one::<String>("wallet-signature").map(PathBuf::from),
        reason: m.get_one::<String>("reason").cloned(),
        json: json_output(m),
    }
}

fn refresh_lockfile_from_manifest(root: &Path) -> Result<()> {
    let manager = PackageManager::new(root);
    let manifest = manager.read_manifest()?;
    let mut lockfile = read_lockfile_for_explicit_repin(root)?;
    lockfile.dependencies.clear();
    lockfile.root = crate::package::LockedRootGraph::default();
    lockfile.environments.clear();
    lockfile.package = lockfile_package_info(root, &manifest)?;

    let mut environments: Vec<Option<String>> = Vec::new();
    if manifest.dependency_overrides.is_empty() {
        environments.push(None);
    }
    environments.extend(manifest.environments.keys().cloned().map(Some));
    for environment in environments {
        let mut manager = PackageManager::new(root);
        let options = crate::package::ResolutionOptions {
            scope: crate::package::DependencyScope::Test,
            all_features: true,
            environment,
            ..crate::package::ResolutionOptions::default()
        };
        manager.resolve_dependencies_with_options(&options)?;
        lockfile.merge_resolution(&manager, &manifest, &options)?;
    }
    lockfile.write_to_root(root)?;
    Ok(())
}

fn read_lockfile_for_explicit_repin(root: &Path) -> Result<Lockfile> {
    Lockfile::read_for_explicit_repin(root)
}

fn refresh_lockfile_from_build(root: &Path, metadata: &CompileMetadata) -> Result<()> {
    let manager = PackageManager::new(root);
    let manifest = manager.read_manifest()?;

    let mut lockfile = Lockfile::read_from_root(root)?.unwrap_or_default();
    if (!manifest.dependencies.is_empty() || !manifest.dev_dependencies.is_empty()) && lockfile.root.manifest_digest.is_empty() {
        return Err(crate::error::CompileError::without_span(
            "Cell.lock has no pinned dependency graph; run 'cellc lock' or 'cellc update' before building",
        ));
    }
    if manifest.dependencies.is_empty() && manifest.dev_dependencies.is_empty() {
        lockfile.root.manifest_digest = crate::package::compute_manifest_digest(root)?;
    }
    let mut package = lockfile_package_info(root, &manifest)?;
    package.compiler_source_hash = metadata.source_hash.clone();
    lockfile.package = package;
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
        edition: manifest.package.edition,
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        namespace: manifest.package.namespace.clone(),
        source_hash: Some(crate::package::registry::compute_source_hash(root)?),
        compiler_source_hash: None,
        compiler_requirement: manifest.package.cellscript_version.clone(),
        resolver_compiler_version: crate::VERSION.to_string(),
    })
}

fn locked_build_info_from_metadata(metadata: &CompileMetadata) -> Result<crate::package::LockedBuildInfo> {
    Ok(crate::package::LockedBuildInfo {
        edition: metadata.edition,
        compatibility_profile_hash: hash_json_value("compatibility_profile", &metadata.compatibility_profile)?,
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
    crate::script_handle::compile_metadata_abi_hash(metadata)
}

fn hash_json_value<T: serde::Serialize>(label: &str, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| crate::error::CompileError::without_span(format!("failed to serialize {} for digest: {}", label, e)))?;
    Ok(crate::hex_encode(&crate::ckb_blake2b256(&bytes)))
}

struct CompileReceiptVerificationReport {
    payload_hash: String,
    signatures_verified: usize,
    unsigned_advisory: bool,
}

fn push_missing_locked_build_identity(label: &str, build: &crate::package::LockedBuildInfo, violations: &mut Vec<String>) {
    if build.compatibility_profile_hash.is_empty() {
        violations.push(format!("{} has no compatibility_profile_hash", label));
    }
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
    if build.compatibility_profile_hash.is_empty() {
        violations.push(format!("{} has no compatibility_profile_hash", label));
    }
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

fn warn_legacy_alias(legacy: &str, canonical: &str, json: bool) {
    if !json && std::io::stderr().is_terminal() {
        eprintln!("warning: `cellc {legacy}` is a compatibility alias; use `cellc {canonical}`");
    }
}

fn update_clap_command(name: &'static str, about: &'static str, allow_apply: bool) -> clap::Command {
    use clap::{Arg, ArgAction, Command as ClapCommand};

    let planning_conflict = allow_apply.then_some("apply-plan");
    let mut command = ClapCommand::new(name)
        .about(about)
        .arg(
            Arg::new("input")
                .value_name("INPUT")
                .conflicts_with_all(planning_conflict)
                .help("Package, workspace, Cell.toml, or contained source path"),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .value_name("FILE")
                .conflicts_with_all(planning_conflict)
                .help("Write the canonical upgrade plan JSON"),
        )
        .arg(
            Arg::new("package")
                .long("package")
                .short('p')
                .value_name("PACKAGE")
                .conflicts_with_all(planning_conflict)
                .help("Update only this dependency alias or package coordinate and its required closure"),
        )
        .arg(
            Arg::new("precise")
                .long("precise")
                .value_name("VERSION")
                .requires("package")
                .conflicts_with_all(planning_conflict)
                .help("Select this exact SemVer for the direct dependency selected by --package"),
        )
        .arg(
            Arg::new("scope")
                .long("scope")
                .value_name("SCOPE")
                .value_parser(["runtime", "test"])
                .default_value("test")
                .conflicts_with_all(planning_conflict)
                .help("Resolve runtime dependencies or runtime plus test dependencies"),
        )
        .arg(
            Arg::new("features")
                .long("features")
                .value_delimiter(',')
                .num_args(1..)
                .value_name("FEATURES")
                .conflicts_with_all(planning_conflict)
                .help("Resolve the candidate with these feature roots"),
        )
        .arg(
            Arg::new("all-features")
                .long("all-features")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(planning_conflict)
                .help("Resolve all feature roots"),
        )
        .arg(
            Arg::new("no-default-features")
                .long("no-default-features")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(planning_conflict)
                .help("Do not activate the default feature root"),
        )
        .arg(
            Arg::new("environment")
                .long("environment")
                .value_name("NAME")
                .conflicts_with_all(planning_conflict)
                .help("Update only this declared CKB dependency environment"),
        )
        .arg(
            Arg::new("offline")
                .long("offline")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(planning_conflict)
                .help("Resolve and compile only from existing local caches"),
        )
        .arg(
            Arg::new("schema-baseline")
                .long("schema-baseline")
                .value_name("MEMBER=PATH")
                .action(ArgAction::Append)
                .conflicts_with_all(planning_conflict)
                .help("Preserved old package tree whose source and Cell.lock match this member's old graph"),
        )
        .arg(
            Arg::new("schema-ack")
                .long("schema-ack")
                .value_name("SUBJECT[@ENVIRONMENT]=FILE")
                .action(ArgAction::Append)
                .conflicts_with_all(planning_conflict)
                .help("Bind a schema receipt to one root member or exact dependency review and environment"),
        )
        .arg(
            Arg::new("schema-version")
                .long("schema-version")
                .value_name("VERSION")
                .default_value("2")
                .value_parser(clap::value_parser!(u32))
                .conflicts_with_all(planning_conflict)
                .help("Negotiate the upgrade plan schema version"),
        )
        .arg(
            Arg::new("acknowledge")
                .long("acknowledge")
                .value_name("CODE")
                .value_delimiter(',')
                .num_args(1..)
                .action(ArgAction::Append)
                .help("Acknowledge a named policy diagnostic after reviewing the plan"),
        );
    if allow_apply {
        command = command.arg(
            Arg::new("apply-plan")
                .long("apply-plan")
                .value_name("FILE")
                .help("Verify and atomically apply this previously reviewed upgrade plan"),
        );
    }
    command
}

fn parse_schema_baselines(values: &[String]) -> Result<std::collections::BTreeMap<String, PathBuf>> {
    let mut baselines = std::collections::BTreeMap::new();
    for value in values {
        let (member, path) = value
            .split_once('=')
            .filter(|(member, path)| !member.is_empty() && !path.is_empty())
            .ok_or_else(|| crate::error::CompileError::without_span("--schema-baseline expects MEMBER=PATH"))?;
        if baselines.insert(member.to_string(), PathBuf::from(path)).is_some() {
            return Err(crate::error::CompileError::without_span(format!("duplicate schema baseline for member '{member}'")));
        }
    }
    Ok(baselines)
}

fn read_upgrade_schema_acknowledgements(values: &[String]) -> Result<Vec<crate::package::upgrade::UpgradeSchemaAcknowledgement>> {
    values
        .iter()
        .map(|value| {
            let (scope, path) = value
                .split_once('=')
                .filter(|(scope, path)| !scope.is_empty() && !path.is_empty())
                .ok_or_else(|| crate::error::CompileError::without_span("--schema-ack expects SUBJECT[@ENVIRONMENT]=FILE"))?;
            let (member, environment) = match scope.split_once('@') {
                Some((member, environment)) if !member.is_empty() && !environment.is_empty() => {
                    (member, Some(environment.to_string()))
                }
                None => (scope, None),
                _ => return Err(crate::error::CompileError::without_span("--schema-ack has an empty member or environment")),
            };
            let bytes = std::fs::read(path)?;
            let receipt = serde_json::from_slice(&bytes).map_err(|error| {
                crate::error::CompileError::without_span(format!("failed to parse schema acknowledgement '{path}': {error}"))
            })?;
            Ok(crate::package::upgrade::UpgradeSchemaAcknowledgement { subject: member.to_string(), environment, receipt })
        })
        .collect()
}

fn update_plan_args_from_matches(matches: &clap::ArgMatches) -> UpdatePlanArgs {
    UpdatePlanArgs {
        input: matches.get_one::<String>("input").map(PathBuf::from),
        output: matches.get_one::<String>("output").map(PathBuf::from),
        package: matches.get_one::<String>("package").cloned(),
        precise: matches.get_one::<String>("precise").cloned(),
        scope: matches.get_one::<String>("scope").cloned().unwrap_or_else(|| "test".to_string()),
        features: matches.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
        all_features: matches.get_flag("all-features"),
        no_default_features: matches.get_flag("no-default-features"),
        environment: matches.get_one::<String>("environment").cloned(),
        offline: matches.get_flag("offline"),
        acknowledgements: matches.get_many::<String>("acknowledge").map(|values| values.cloned().collect()).unwrap_or_default(),
        schema_baselines: matches.get_many::<String>("schema-baseline").map(|values| values.cloned().collect()).unwrap_or_default(),
        schema_acknowledgements: matches.get_many::<String>("schema-ack").map(|values| values.cloned().collect()).unwrap_or_default(),
        schema_version: matches.get_one::<u32>("schema-version").copied().unwrap_or(2),
        json: json_output(matches),
    }
}

pub struct CliParser;

impl CliParser {
    pub fn parse() -> Result<Command> {
        let matches = match Self::command().try_get_matches() {
            Ok(matches) => matches,
            Err(error) if matches!(error.kind(), clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion) => {
                error.print().map_err(crate::error::CompileError::from)?;
                std::process::exit(0);
            }
            Err(error) => {
                return Err(crate::error::CompileError::without_span(error.to_string())
                    .with_code(crate::error::DiagnosticCode::CLI0001)
                    .with_category(crate::error::CompileErrorCategory::Usage));
            }
        };
        let color = matches
            .get_one::<String>("color")
            .map(String::as_str)
            .or_else(|| matches.subcommand().and_then(|(_, subcommand)| subcommand.get_one::<String>("color").map(String::as_str)));
        crate::cli::apply_color_policy(color);
        Self::parse_matches(matches)
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
mod tests;

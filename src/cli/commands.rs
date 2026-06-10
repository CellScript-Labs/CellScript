use crate::docgen::{DocGenerator, OutputFormat};
use crate::error::Result;
use crate::fmt::format_default;
use crate::package::{validate_git_revision, Dependency, DetailedDependency, Lockfile, PackageManager, PolicyConfig};
use crate::runtime_errors::{runtime_error_info, runtime_error_info_by_code, CellScriptRuntimeErrorInfo, ALL_RUNTIME_ERRORS};
use crate::{
    compile_path, compile_path_with_entry_action, compile_path_with_entry_lock, default_metadata_path_for_artifact,
    default_output_path_for_input, load_modules_for_input, resolve_input_path, validate_artifact_metadata,
    validate_source_units_on_disk_under, validate_source_units_primitive_mode_under, ArtifactFormat, CompileMetadata, CompileOptions,
    EntryWitnessArg, ParamMetadata, ProofPlanMetadata, ProofPlanSoundnessReport, TargetProfile, ENTRY_WITNESS_ABI,
};
use camino::{Utf8Path, Utf8PathBuf};
#[cfg(feature = "vm-runner")]
use ckb_vm::{
    cost_model::estimate_cycles, machine::VERSION2, Bytes, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner,
    SparseMemory, SupportMachine, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
};
use colored::Colorize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const CKB_HASH_FILE_SIZE_LIMIT_BYTES: u64 = 1024 * 1024;
const NOVASEAL_CERTIFICATION_PLUGIN: &str = "novaseal-profile-v0";
const NOVASEAL_CERTIFICATION_REPORT_SCHEMA: &str = "cellscript-certification-report-v0.1";
const NOVASEAL_PLUGIN_REPORT_SCHEMA: &str = "novaseal-production-gates-v0.3";
const NOVASEAL_PROFILE_CERTIFICATION_SCHEMA: &str = "novaseal-profile-certification-v0.1";
const NOVASEAL_AGREEMENT_PROFILE: &str = "agreement-profile-v0";
const NOVASEAL_CANONICAL_SCHEMA: &str = "NovaSealCanonicalV0";
const NOVASEAL_PROFILE_CERTIFICATION_GATE: &str = "agreement_profile_public_ecosystem_certification_v0";
const STRICT_V0_16_PRIMITIVE_COMPAT: &str = "0.16";

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
    Certify(CertifyArgs),
    ValidateTx(ValidateTxArgs),
    SolveTx(SolveTxArgs),
    DeployPlan(DeployPlanArgs),
    VerifyDeploy(VerifyDeployArgs),
    DiffDeploy(DiffDeployArgs),
    LockDeps(LockDepsArgs),
    ActionBuild(ActionBuildArgs),
    /// Encode generated entry wrapper witness bytes
    EntryWitness(EntryWitnessArgs),
    VerifyArtifact(VerifyArtifactArgs),
    Run(RunArgs),
    Publish(PublishArgs),
    Install(InstallArgs),
    Update,
    Info(InfoArgs),
    Login(LoginArgs),
    Invalid(String),
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
    pub rev: Option<String>,
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
    pub primitive_compat: Option<String>,
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
    pub primitive_compat: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct TraceTxArgs {
    pub against: PathBuf,
    pub tx: PathBuf,
    pub primitive_compat: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct AuditBundleArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub primitive_compat: Option<String>,
    pub json: bool,
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

#[derive(Debug, Default)]
pub struct ValidateTxArgs {
    pub against: PathBuf,
    pub tx: PathBuf,
    pub primitive_compat: Option<String>,
    pub json: bool,
}

#[derive(Debug, Default)]
pub struct SolveTxArgs {
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub target: Option<String>,
    pub target_profile: Option<String>,
    pub primitive_compat: Option<String>,
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
    pub git: Option<String>,
    pub rev: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Default)]
pub struct LoginArgs {
    pub registry: Option<String>,
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
            Command::Certify(args) => Self::certify(args),
            Command::ValidateTx(args) => Self::validate_tx(args),
            Command::SolveTx(args) => Self::solve_tx(args),
            Command::DeployPlan(args) => Self::deploy_plan(args),
            Command::VerifyDeploy(args) => Self::verify_deploy(args),
            Command::DiffDeploy(args) => Self::diff_deploy(args),
            Command::LockDeps(args) => Self::lock_deps(args),
            Command::ActionBuild(args) => Self::action_build(args),
            Command::EntryWitness(args) => Self::entry_witness(args),
            Command::VerifyArtifact(args) => Self::verify_artifact(args),
            Command::Run(args) => Self::run(args),
            Command::Publish(args) => Self::publish(args),
            Command::Install(args) => Self::install(args),
            Command::Update => Self::update(),
            Command::Info(args) => Self::info(args),
            Command::Login(args) => Self::login(args),
            Command::Invalid(message) => Err(crate::error::CompileError::without_span(message)),
        }
    }

    fn build(args: BuildArgs) -> Result<()> {
        if let Some(jobs) = args.jobs {
            if jobs == 0 {
                return Err(crate::error::CompileError::without_span("cellc build --jobs must be at least 1"));
            }
            if jobs != 1 {
                return Err(crate::error::CompileError::without_span(
                    "cellc build --jobs is reserved for future parallel package builds; current builds compile one package entry, so omit --jobs or use --jobs 1",
                ));
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
            (Some(_), Some(_)) => {
                Err(crate::error::CompileError::without_span("--entry-action and --entry-lock are mutually exclusive"))
            }
        }?;
        let policy_args = effective_build_check_args(&args)?;
        validate_check_policy(&result.metadata, &policy_args)?;
        let resolved = resolve_input_path(input)?;
        let output_path = default_output_path_for_input(input, &resolved, result.artifact_format)?;
        result.write_to_path(&output_path)?;
        let metadata_path = default_metadata_path_for_artifact(&output_path);
        result.write_metadata_to_path(&metadata_path)?;

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
        let opened = if args.open {
            open_doc_output(&output)?;
            true
        } else {
            false
        };

        if args.json {
            let summary = serde_json::json!({
                "status": "ok",
                "format": display_doc_output_format(&args.output_format),
                "output": output.display().to_string(),
                "output_size_bytes": output_size_bytes,
                "opened": opened,
            });
            let json = serde_json::to_string_pretty(&summary)
                .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize doc summary: {}", error)))?;
            println!("{}", json);

            return Ok(());
        }

        println!("{}", "Documentation generated".green());
        println!("  Output: {}", output.display());

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

        if args.json {
            let entry = if args.lib { "src/lib.cell" } else { "src/main.cell" };
            let summary = serde_json::json!({
                "status": "ok",
                "kind": if args.lib { "library" } else { "binary" },
                "package": name,
                "path": path.display().to_string(),
                "manifest": path.join("Cell.toml").display().to_string(),
                "entry": entry,
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
        validate_dependency_source_args(args.git.as_deref(), args.path.as_deref(), args.rev.as_deref())?;

        let pm = PackageManager::new(".");
        let mut manifest = pm.read_manifest()?;
        let dependency = dependency_from_add_args(&args)?;
        let target = dependency_target_label(args.dev, args.build);
        let mut added = Vec::new();

        for crate_name in &args.crates {
            if !args.json {
                println!("{} {} to {}", "Adding".cyan(), crate_name, target);
            }
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

        let package_root = std::env::current_dir()?.canonicalize()?;
        let manifest = PackageManager::new(&package_root).read_manifest()?;
        let existing_paths =
            clean_generated_paths(&package_root, &manifest).into_iter().filter(|path| path.exists()).collect::<Vec<_>>();
        for path in &existing_paths {
            validate_clean_path(&package_root, path)?;
        }

        let mut removed_paths = Vec::new();
        for path in existing_paths {
            let label = clean_path_label(&package_root, &path);
            if path.exists() {
                if !args.json {
                    println!("  Removing {}", label);
                }
                remove_clean_path(&package_root, &path)?;
                removed_paths.push(label);
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
            (Some(_), Some(_)) => {
                Err(crate::error::CompileError::without_span("constraints accepts either --entry-action or --entry-lock, not both"))
            }
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
            read_ckb_hash_file(path)?
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
        let compile_options = metadata_workflow_compile_options(args.target, args.target_profile, args.primitive_compat);
        let result = compile_cli_input(args.input.as_ref(), compile_options)?;
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
        validate_metadata_workflow_primitive_compat(args.primitive_compat.as_deref())?;
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        if let Some(soundness) = strict_v0_16_soundness_report_for_mode(&metadata, args.primitive_compat.as_deref()) {
            let error_message = strict_v0_16_soundness_error_message(&soundness);
            let summary = serde_json::json!({
                "status": "failed",
                "metadata": args.against.display().to_string(),
                "tx": args.tx.display().to_string(),
                "proof_plan_soundness": soundness,
            });
            if args.json {
                print_json(&summary)?;
            } else {
                println!("Transaction validation: failed");
                println!("  Strict v0.16 ProofPlan soundness: failed");
            }
            return Err(crate::error::CompileError::without_span(error_message));
        }
        let report = crate::assumptions::validate_transaction_against_metadata(&metadata, &tx);
        let summary = serde_json::json!({
            "status": report.status,
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
        let compile_options = metadata_workflow_compile_options(args.target, args.target_profile, args.primitive_compat);
        let result = compile_cli_input(args.input.as_ref(), compile_options)?;
        let template = transaction_solver_template(&result.metadata);
        write_or_print_json(args.output.as_ref(), &template, args.json, "Transaction solver template generated")?;
        Ok(())
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
        let compile_options = metadata_workflow_compile_options(args.target, args.target_profile, args.primitive_compat);
        let result = compile_cli_input(args.input.as_ref(), compile_options)?;
        let report = profile_report_json(&result.metadata, args.entry.as_deref());
        print_or_text_json(args.json, &report, "Profile")?;
        Ok(())
    }

    fn trace_tx(args: TraceTxArgs) -> Result<()> {
        validate_metadata_workflow_primitive_compat(args.primitive_compat.as_deref())?;
        let metadata = read_metadata_json(&args.against)?;
        let tx = read_json_value(&args.tx)?;
        if let Some(soundness) = strict_v0_16_soundness_report_for_mode(&metadata, args.primitive_compat.as_deref()) {
            let error_message = strict_v0_16_soundness_error_message(&soundness);
            let trace = serde_json::json!({
                "status": "failed",
                "schema": "cellscript-tx-trace-v0.16",
                "module": metadata.module,
                "proof_plan_soundness": soundness,
                "steps": [],
            });
            if args.json {
                print_json(&trace)?;
            } else {
                println!("Transaction trace: failed");
                println!("  Strict v0.16 ProofPlan soundness: failed");
            }
            return Err(crate::error::CompileError::without_span(error_message));
        }
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
        let compile_options = metadata_workflow_compile_options(args.target, args.target_profile, args.primitive_compat);
        let result = compile_cli_input(args.input.as_ref(), compile_options)?;
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

    fn certify(args: CertifyArgs) -> Result<()> {
        if args.plugin != NOVASEAL_CERTIFICATION_PLUGIN {
            return Err(crate::error::CompileError::without_span(format!(
                "unknown certification plugin '{}'; available plugins: novaseal-profile-v0",
                args.plugin
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
            rows.push(serde_json::json!({
                "opt_level": opt_level,
                "artifact_format": result.metadata.artifact_format,
                "target_profile": result.metadata.target_profile.name,
                "artifact_size_bytes": result.artifact_bytes.len(),
                "constraints_status": result.metadata.constraints.status,
                "constraints_warnings": result.metadata.constraints.warnings.len(),
                "constraints_failures": result.metadata.constraints.failures.len(),
                "source_content_hash": result.metadata.source_content_hash,
            }));
        }
        let baseline_size = rows.first().and_then(|row| row["artifact_size_bytes"].as_u64()).unwrap_or_default();
        let summary_rows = rows
            .into_iter()
            .map(|mut row| {
                let size = row["artifact_size_bytes"].as_u64().unwrap_or_default();
                row["artifact_size_delta_from_o0"] = serde_json::json!(size as i64 - baseline_size as i64);
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
                "hash_blake2b(input: Hash) uses CKB Blake2b-256; wider byte serialization hashing remains out of scope"
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
        let plan = serde_json::json!({
            "status": "ok",
            "policy": "cellscript-action-builder-plan-v1",
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
            "ckb": ckb.map(|ckb| serde_json::json!({
                "hash_type_policy": ckb.hash_type_policy,
                "capacity_evidence_contract": ckb.capacity_evidence_contract,
                "timelock_policy": ckb.timelock_policy,
                "tx_size_measurement_required": ckb.tx_size_measurement_required,
                "occupied_capacity_measurement_required": ckb.occupied_capacity_measurement_required,
                "dry_run_required_for_production": ckb.dry_run_required_for_production,
            })),
            "constraints_status": result.metadata.constraints.status,
            "constraints_failures": result.metadata.constraints.failures,
            "constraints_warnings": result.metadata.constraints.warnings,
        });
        let json = serde_json::to_string_pretty(&plan)
            .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize action build plan: {}", error)))?;

        if let Some(output_path) = args.output {
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&output_path, json)?;
            println!("{}", "Action build plan generated".green());
            println!("  Output: {}", output_path.display());
        } else if args.json {
            println!("{}", json);
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
        let primitive_compat = args.primitive_compat.clone();
        let sources_verified = args.verify_sources || primitive_compat.is_some();
        let source_verification_root = source_verification_root_for_artifact(artifact_path);
        if sources_verified {
            validate_source_units_on_disk_under(&result.metadata, &source_verification_root)?;
        }
        if primitive_compat.is_some() {
            validate_source_units_primitive_mode_under(&result.metadata, primitive_compat.clone(), &source_verification_root)?;
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
                "sources_verified": sources_verified,
                "primitive_mode_verified": primitive_compat.is_some(),
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
        if sources_verified {
            println!("  Sources: verified {} unit(s)", result.metadata.source_units.len());
        }
        if primitive_compat.is_some() {
            println!("  Primitive mode: verified");
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
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run only supports no-argument pure ELF entrypoints; {} requires transaction/parameter ABI context; use `cellc run --simulate` for AST-level simulation or build a transaction witness with `cellc entry-witness`",
                    parameterized_entries.join(", ")
                )));
            }

            if result.metadata.runtime.ckb_runtime_required {
                return Err(crate::error::CompileError::without_span(format!(
                    "cellc run cannot provide CKB transaction/syscall context ({}); use `cellc run --simulate` for AST-level simulation or run builder-backed CKB acceptance",
                    result.metadata.runtime.ckb_runtime_features.join(", ")
                )));
            }

            if !result.metadata.runtime.standalone_runner_compatible {
                return Err(crate::error::CompileError::without_span(
                    "cellc run only supports standalone-compatible no-argument pure ELF entrypoints; use `cellc run --simulate` for AST-level simulation",
                ));
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
            let dirty = if args.allow_dirty { "allow-dirty" } else { "clean-tree-only" };
            Self::experimental_command(
                "publish",
                &format!(
                    "registry publication is not implemented yet (package {} v{}, {})",
                    manifest.package.name, manifest.package.version, dirty
                ),
            )
        }
    }

    fn install(args: InstallArgs) -> Result<()> {
        let pm = PackageManager::new(".");
        validate_dependency_source_args(args.git.as_deref(), args.path.as_deref(), args.rev.as_deref())?;

        let _manifest = pm.read_manifest()?;

        if let Some(git_url) = &args.git {
            let crate_name = args.crate_name.clone().unwrap_or_else(|| {
                git_url.trim_end_matches('/').trim_end_matches(".git").split('/').next_back().unwrap_or("unknown").to_string()
            });

            let dep = DetailedDependency {
                version: args.version.clone().unwrap_or_else(|| "*".to_string()),
                git: Some(git_url.clone()),
                branch: None,
                tag: None,
                rev: args.rev.clone(),
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
                git: None,
                branch: None,
                tag: None,
                rev: None,
                path: Some(path.to_string_lossy().to_string()),
                optional: false,
                features: Vec::new(),
                default_features: true,
            };

            pm.resolve_from_path(&crate_name, &path.to_string_lossy())?;

            let mut manifest = pm.read_manifest()?;
            manifest.dependencies.insert(crate_name.clone(), Dependency::Detailed(dep));
            pm.write_manifest(&manifest)?;

            refresh_lockfile_from_manifest(std::path::Path::new("."))?;

            println!("{}", format!("Installed {} from path {}", crate_name, path.display()).green());
            Ok(())
        } else if let Some(crate_name) = &args.crate_name {
            Self::experimental_command(
                "install",
                &format!(
                    "registry package installation is not implemented yet; use --git URL or --path PATH to install {}",
                    crate_name
                ),
            )
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
                    crate::package::PackageSource::Registry { name, version } => format!("registry: {}@{}", name, version),
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
        #[cfg(unix)]
        {
            use std::io::Write as _;

            let mut file = std::fs::OpenOptions::new().create(true).write(true).truncate(true).mode(0o600).open(&credentials_path)?;
            file.write_all(content.as_bytes())?;
            std::fs::set_permissions(&credentials_path, std::fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&credentials_path, content)?;
        }

        println!("{}", format!("Login credentials saved for {}", registry).green());
        println!("  Config directory: {}", config_dir.display());
        Ok(())
    }
}

#[cfg(feature = "vm-runner")]
type CliVmMachine = TraceMachine<DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RegistryCredential {
    registry: String,
    token: String,
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
    let failed_checks = checks
        .iter()
        .filter(|(_, passed)| !*passed)
        .map(|(name, _)| serde_json::Value::String((*name).to_string()))
        .collect::<Vec<_>>();
    let passed = failed_checks.is_empty();
    let external_blockers = plugin_report
        .get("external_blockers")
        .cloned()
        .or_else(|| v1_readiness.get("external_blockers").cloned())
        .or_else(|| profile_certification.get("production_statement_blockers").cloned())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let failed_dimensions = plugin_report
        .get("failed_dimensions")
        .cloned()
        .or_else(|| v1_readiness.get("failed_dimensions").cloned())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let certification_level = json_pointer_str(profile_certification, "/certification_level").unwrap_or("unknown");
    let failure_reason = if passed {
        serde_json::Value::Null
    } else if !v1_readiness.is_null() && !json_pointer_bool(v1_readiness, "/local_v1_ready") {
        serde_json::json!({
            "message": "NovaSeal V1 readiness requires remaining planned profiles and business scenarios",
            "v1_status": json_pointer_str(v1_readiness, "/status"),
            "missing": v1_readiness.pointer("/planned_profile_matrix/missing").cloned().unwrap_or(serde_json::Value::Null),
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

fn print_json(value: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to serialize JSON: {}", error)))?;
    println!("{}", json);
    Ok(())
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

fn metadata_workflow_compile_options(
    target: Option<String>,
    target_profile: Option<String>,
    primitive_compat: Option<String>,
) -> CompileOptions {
    CompileOptions { opt_level: 0, output: None, debug: false, target, target_profile, primitive_compat }
}

fn validate_metadata_workflow_primitive_compat(primitive_compat: Option<&str>) -> Result<()> {
    if primitive_compat.is_some_and(|mode| !matches!(mode, "0.14" | "0.15" | "0.16")) {
        return Err(crate::error::CompileError::without_span(format!(
            "unsupported primitive compatibility mode '{}'; supported values: 0.14, 0.15, 0.16",
            primitive_compat.unwrap_or_default()
        )));
    }
    Ok(())
}

fn strict_v0_16_soundness_report_for_mode(
    metadata: &CompileMetadata,
    primitive_compat: Option<&str>,
) -> Option<ProofPlanSoundnessReport> {
    if primitive_compat != Some(STRICT_V0_16_PRIMITIVE_COMPAT) {
        return None;
    }
    let soundness = crate::proof_plan::soundness::check_metadata(metadata, true);
    (soundness.status != "passed").then_some(soundness)
}

fn strict_v0_16_soundness_error_message(report: &ProofPlanSoundnessReport) -> String {
    let messages = report
        .issues
        .iter()
        .filter(|issue| issue.severity == "error")
        .map(|issue| format!("{} {}:{} - {}", issue.code, issue.origin, issue.feature, issue.message))
        .collect::<Vec<_>>();
    if messages.is_empty() {
        "metadata fails strict v0.16 ProofPlan soundness".to_string()
    } else {
        format!("metadata fails strict v0.16 ProofPlan soundness:\n  - {}", messages.join("\n  - "))
    }
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
                "tx_hash": dep.tx_hash,
                "index": dep.index,
                "hash_type": dep.hash_type,
                "data_hash": dep.data_hash,
                "type_id": dep.type_id,
            }));
        }
        for script_ref in &ckb_constraints.script_references {
            dep_slots.push(serde_json::json!({
                "source": "metadata-script-reference",
                "name": script_ref.name,
                "scope": script_ref.scope,
                "purpose": script_ref.purpose,
                "dep_source": script_ref.dep_source,
                "status": script_ref.status,
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
                    | "spawn_target_cell_dep_binding"
                    | "lock_group_transaction_scope"
                    | "capacity_policy"
            )
        })
        .map(|assumption| {
            let evidence_schema = if assumption.kind == "spawn_target_cell_dep_binding" {
                serde_json::json!({
                    "required_fields": ["assumption_id", "kind", "origin", "feature", "proof_plan_status", "evidence"],
                    "evidence_payload": "non-empty object or array; scalar booleans, numbers, and strings are rejected",
                    "evidence_required_fields": ["dep_source", "cell_dep_index", "cell_dep_name", "dep_type"],
                    "optional_manifest_identity_fields": ["tx_hash", "out_index", "hash_type", "data_hash", "type_id"],
                    "required_cell_deps": assumption.required_cell_deps,
                    "note": "builder must provide the same manifest-bound CellDep identity in transaction_plan.cell_deps and builder_assumption_evidence before validate-tx can pass"
                })
            } else {
                serde_json::json!({
                    "required_fields": ["assumption_id", "kind", "origin", "feature", "proof_plan_status", "evidence"],
                    "evidence_payload": "non-empty object or array; scalar booleans, numbers, and strings are rejected",
                    "note": "builder must replace this requirement with concrete evidence before validate-tx can pass"
                })
            };
            serde_json::json!({
                "assumption_id": assumption.assumption_id,
                "kind": assumption.kind,
                "origin": assumption.origin,
                "feature": assumption.feature,
                "proof_plan_status": assumption.proof_plan_status,
                "detail": assumption.detail,
                "evidence_schema": evidence_schema,
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

    serde_json::json!({
        "status": "template",
        "solver": "cellscript-v0.16-transaction-template-emitter",
        "module": metadata.module,
        "target_profile": metadata.target_profile.name,
        "transaction_plan": {
            "version": 0,
            "inputs": input_slots,
            "outputs": output_slots,
            "cell_deps": dep_slots,
            "witnesses": witness_slots,
            "header_deps": ckb.map(|c| if c.uses_header_epoch { vec!["epoch-header"] } else { vec![] }).unwrap_or_default(),
            "builder_assumption_evidence_requirements": evidence,
        },
        "fee_change_plan": fee_planning,
        "signing_manifest": {
            "policy": "explicit-witness-no-implicit-signer",
            "signature_requests": signature_requests,
        },
        "builder_assumptions": assumptions,
        "limitations": [
            "template only: does not perform live cell selection",
            "template only: does not resolve concrete deps/header deps",
            "template only: does not calculate fee/change or occupied capacity",
            "template only: does not place final witnesses or signatures",
            "CKB dry-run required for production acceptance"
        ],
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
    if plan.get("status").and_then(serde_json::Value::as_str) != Some("ok") {
        violations.push("status must be ok".to_string());
    }
    if plan.get("module").and_then(serde_json::Value::as_str).is_none_or(str::is_empty) {
        violations.push("module must be a non-empty string".to_string());
    }
    if plan.get("compiler_version").and_then(serde_json::Value::as_str).is_none_or(str::is_empty) {
        violations.push("compiler_version must be a non-empty string".to_string());
    }
    match plan.pointer("/artifact/format").and_then(serde_json::Value::as_str) {
        Some(format) if !format.is_empty() => {}
        Some(_) => violations.push("artifact.format must be a non-empty string".to_string()),
        None => violations.push("artifact.format is required".to_string()),
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
    match plan.pointer("/target_profile/name").and_then(serde_json::Value::as_str) {
        Some("ckb") => {}
        Some(profile) => violations.push(format!("target_profile.name must be ckb, got {profile}")),
        None => violations.push("target_profile.name is required".to_string()),
    }
    match plan.pointer("/proof_plan_soundness/status").and_then(serde_json::Value::as_str) {
        Some("passed") => {}
        Some(status) => violations.push(format!("proof_plan_soundness.status must be passed, got {status}")),
        None => violations.push("proof_plan_soundness.status is required".to_string()),
    }
    match plan.get("builder_assumptions").and_then(serde_json::Value::as_array) {
        Some(_) => {}
        None => violations.push("builder_assumptions must be an array".to_string()),
    }
    violations
}

fn dependency_lock_json(metadata: &CompileMetadata) -> serde_json::Value {
    let ckb = metadata.constraints.ckb.as_ref();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-dependency-lock-v0.16",
        "module": metadata.module,
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
    let changed_records = changed
        .iter()
        .filter_map(|key| {
            let (Some(old_record), Some(new_record)) = (old_map.get(key), new_map.get(key)) else {
                log::warn!("proof diff skipped inconsistent changed proof-plan key '{}'", key);
                return None;
            };
            Some(serde_json::json!({
                "key": key,
                "fields": changed_proof_plan_fields(old_record, new_record),
            }))
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "status": "ok",
        "schema": "cellscript-proof-diff-v0.16",
        "old_module": old.module,
        "new_module": new.module,
        "added": added,
        "removed": removed,
        "changed": changed,
        "changed_records": changed_records,
    })
}

fn changed_proof_plan_fields(old: &serde_json::Value, new: &serde_json::Value) -> Vec<serde_json::Value> {
    [
        "trigger",
        "scope",
        "reads",
        "coverage",
        "group_cardinality",
        "builder_assumptions",
        "codegen_coverage_status",
        "on_chain_checked",
    ]
    .iter()
    .filter_map(|field| {
        let old_value = old.get(*field).cloned().unwrap_or(serde_json::Value::Null);
        let new_value = new.get(*field).cloned().unwrap_or(serde_json::Value::Null);
        (old_value != new_value).then(|| {
            serde_json::json!({
                "field": field,
                "old": old_value,
                "new": new_value,
            })
        })
    })
    .collect()
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
                    "group_cardinality": plan.group_cardinality,
                    "builder_assumptions": plan.builder_assumptions,
                    "codegen_coverage_status": plan.codegen_coverage_status,
                    "on_chain_checked": plan.on_chain_checked,
                }),
            )
        })
        .collect()
}

fn json_diff_report(kind: &str, old: &serde_json::Value, new: &serde_json::Value) -> serde_json::Value {
    let mut changed = Vec::new();
    collect_json_diffs("", old, new, &mut changed);
    serde_json::json!({
        "status": "ok",
        "schema": format!("cellscript-{}-diff-v0.16", kind),
        "changed": changed,
    })
}

fn collect_json_diffs(path: &str, old: &serde_json::Value, new: &serde_json::Value, changed: &mut Vec<serde_json::Value>) {
    if old == new {
        return;
    }

    match (old, new) {
        (serde_json::Value::Object(old_object), serde_json::Value::Object(new_object)) => {
            let keys = old_object.keys().chain(new_object.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let child_path = json_pointer_child(path, key);
                collect_json_diffs(
                    &child_path,
                    old_object.get(key).unwrap_or(&serde_json::Value::Null),
                    new_object.get(key).unwrap_or(&serde_json::Value::Null),
                    changed,
                );
            }
        }
        (serde_json::Value::Array(old_items), serde_json::Value::Array(new_items)) => {
            let max_len = old_items.len().max(new_items.len());
            for index in 0..max_len {
                let child_path = json_pointer_child(path, &index.to_string());
                collect_json_diffs(
                    &child_path,
                    old_items.get(index).unwrap_or(&serde_json::Value::Null),
                    new_items.get(index).unwrap_or(&serde_json::Value::Null),
                    changed,
                );
            }
        }
        _ => changed.push(serde_json::json!({
            "path": if path.is_empty() { "/" } else { path },
            "old": old,
            "new": new,
        })),
    }
}

fn json_pointer_child(parent: &str, token: &str) -> String {
    let escaped = token.replace('~', "~0").replace('/', "~1");
    if parent.is_empty() {
        format!("/{escaped}")
    } else {
        format!("{parent}/{escaped}")
    }
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
    let checked_partial_count = proof_plan.iter().filter(|plan| plan.status == "checked-partial").count();
    let metadata_only_gap_count = proof_plan.iter().filter(|plan| plan.codegen_coverage_status == "gap:metadata-only").count();
    let fail_closed_count =
        proof_plan.iter().filter(|plan| plan.status == "fail-closed" || plan.codegen_coverage_status == "fail-closed").count();
    let diagnostic_error_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "error").count();
    let diagnostic_warning_count =
        proof_plan.iter().flat_map(|plan| &plan.diagnostics).filter(|diagnostic| diagnostic.severity == "warning").count();
    let macro_provenance_count =
        proof_plan.iter().flat_map(|plan| &plan.coverage).filter(|coverage| coverage.starts_with("macro_expansion:")).count();
    let invariant_action_match_count = invariant_action_coverage_match_count(proof_plan);
    let invariant_unmatched_action_coverage_count = invariant_unmatched_action_coverage_count(proof_plan);
    let has_runtime_required_gaps = proof_plan.iter().any(|plan| plan.status == "runtime-required" && !plan.on_chain_checked);
    let has_partial_gaps = proof_plan.iter().any(|plan| plan.status == "checked-partial" && !plan.on_chain_checked);
    let has_fail_closed_gaps = fail_closed_count > 0;

    serde_json::json!({
        "record_count": record_count,
        "on_chain_checked_count": on_chain_checked_count,
        "runtime_required_count": runtime_required_count,
        "checked_partial_count": checked_partial_count,
        "metadata_only_gap_count": metadata_only_gap_count,
        "fail_closed_count": fail_closed_count,
        "diagnostic_error_count": diagnostic_error_count,
        "diagnostic_warning_count": diagnostic_warning_count,
        "macro_provenance_count": macro_provenance_count,
        "invariant_action_match_count": invariant_action_match_count,
        "invariant_unmatched_action_coverage_count": invariant_unmatched_action_coverage_count,
        "has_runtime_required_gaps": has_runtime_required_gaps,
        "has_partial_gaps": has_partial_gaps,
        "has_fail_closed_gaps": has_fail_closed_gaps,
        "has_unmatched_invariant_action_coverage": invariant_unmatched_action_coverage_count > 0,
        "has_blocking_diagnostics": has_runtime_required_gaps || has_partial_gaps || has_fail_closed_gaps || diagnostic_error_count > 0,
    })
}

fn invariant_action_coverage_match_count(proof_plan: &[ProofPlanMetadata]) -> usize {
    proof_plan
        .iter()
        .flat_map(|plan| &plan.coverage)
        .filter(|coverage| coverage.starts_with("invariant_coverage:matched-action-obligation:"))
        .count()
}

fn invariant_unmatched_action_coverage_count(proof_plan: &[ProofPlanMetadata]) -> usize {
    proof_plan
        .iter()
        .filter(|plan| {
            plan.category == "aggregate-invariant"
                && plan.builder_assumptions.iter().any(|assumption| {
                    assumption.starts_with("declared(no_checked_action_obligation_matches:")
                        || assumption.starts_with("declared(unmatched_related_action_obligation_count:")
                })
        })
        .count()
}

fn invariant_unmatched_action_coverage_summaries(proof_plan: &[ProofPlanMetadata]) -> Vec<String> {
    proof_plan
        .iter()
        .filter(|plan| {
            plan.category == "aggregate-invariant"
                && plan.builder_assumptions.iter().any(|assumption| {
                    assumption.starts_with("declared(no_checked_action_obligation_matches:")
                        || assumption.starts_with("declared(unmatched_related_action_obligation_count:")
                })
        })
        .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
        .collect()
}

fn checked_runtime_proof_plan_evidence_gap_summaries(proof_plan: &[ProofPlanMetadata]) -> Vec<String> {
    proof_plan
        .iter()
        .filter(|plan| plan.status == "checked-runtime" || plan.on_chain_checked)
        .filter(|plan| plan.executable_evidence.is_empty() || plan.codegen_coverage_status.starts_with("gap:"))
        .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
        .collect()
}

fn print_proof_plan_summary(proof_plan: &[ProofPlanMetadata]) {
    let summary = proof_plan_summary_json(proof_plan);
    println!("  Summary:");
    println!("    records: {}", summary["record_count"]);
    println!("    on_chain_checked: {}", summary["on_chain_checked_count"]);
    println!("    runtime_required: {}", summary["runtime_required_count"]);
    println!("    checked_partial: {}", summary["checked_partial_count"]);
    println!("    metadata_only_gaps: {}", summary["metadata_only_gap_count"]);
    println!("    fail_closed: {}", summary["fail_closed_count"]);
    println!("    diagnostic_errors: {}", summary["diagnostic_error_count"]);
    println!("    diagnostic_warnings: {}", summary["diagnostic_warning_count"]);
    println!("    macro_provenance_records: {}", summary["macro_provenance_count"]);
    println!("    invariant_action_matches: {}", summary["invariant_action_match_count"]);
    println!("    invariant_unmatched_action_coverage: {}", summary["invariant_unmatched_action_coverage_count"]);
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
    let policy = PackageManager::new(".").read_manifest()?.policy;
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

fn open_doc_output(output: &Path) -> Result<()> {
    let status = std::process::Command::new("open").arg(output).status().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to open documentation '{}': {}", output.display(), error))
    })?;
    if !status.success() {
        return Err(crate::error::CompileError::without_span(format!(
            "failed to open documentation '{}': open exited with {}",
            output.display(),
            status
        )));
    }
    Ok(())
}

fn read_ckb_hash_file(path: &Path) -> Result<Vec<u8>> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read CKB hash input '{}': {}", path.display(), error))
    })?;
    let mut bytes = Vec::new();
    file.by_ref().take(CKB_HASH_FILE_SIZE_LIMIT_BYTES + 1).read_to_end(&mut bytes).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read CKB hash input '{}': {}", path.display(), error))
    })?;
    if bytes.len() as u64 > CKB_HASH_FILE_SIZE_LIMIT_BYTES {
        return Err(crate::error::CompileError::without_span(format!(
            "CKB hash input '{}' is too large: limit is {} bytes",
            path.display(),
            CKB_HASH_FILE_SIZE_LIMIT_BYTES
        )));
    }
    Ok(bytes)
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
    let output = std::process::Command::new("git").arg("init").arg("--quiet").arg("--").arg(path).output().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to run git init for '{}': {}", path.display(), error))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::error::CompileError::without_span(format!("git init failed for '{}': {}", path.display(), stderr.trim())));
    }
    Ok(true)
}

fn clean_generated_paths(package_root: &Path, manifest: &crate::package::PackageManifest) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = BTreeSet::new();
    for raw_path in ["target", "build", "dist", ".cell"] {
        push_clean_path(package_root, raw_path, &mut paths, &mut seen);
    }

    if let Some(out_dir) = manifest.build.out_dir.as_deref() {
        push_clean_path(package_root, out_dir, &mut paths, &mut seen);
    }

    let mut source_roots = manifest.package.source_roots.clone();
    if source_roots.is_empty() {
        source_roots.push("src".to_string());
    }
    source_roots.push(
        Path::new(&manifest.package.entry)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .display()
            .to_string(),
    );

    for source_root in source_roots {
        if let Some(raw_path) = source_root_clean_cache_path(&source_root) {
            push_clean_path(package_root, &raw_path, &mut paths, &mut seen);
        }
    }

    paths
}

fn source_root_clean_cache_path(source_root: &str) -> Option<String> {
    let path = Path::new(source_root);
    if source_root.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(component, std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir)
        })
    {
        return None;
    }
    Some(path.join(".cell").display().to_string())
}

fn push_clean_path(package_root: &Path, raw_path: &str, paths: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>) {
    let path = Path::new(raw_path);
    if raw_path.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(component, std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir)
        })
    {
        return;
    }

    let candidate = package_root.join(path);
    if seen.insert(candidate.clone()) {
        paths.push(candidate);
    }
}

fn clean_path_label(package_root: &Path, path: &Path) -> String {
    path.strip_prefix(package_root).unwrap_or(path).display().to_string()
}

fn validate_clean_path(package_root: &Path, path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(crate::error::CompileError::without_span(format!(
            "refusing to clean '{}' because it is a symbolic link",
            path.display()
        )));
    }

    let canonical = path.canonicalize()?;
    if !canonical.starts_with(package_root) {
        return Err(crate::error::CompileError::without_span(format!(
            "refusing to remove '{}' because it resolves outside the package root",
            path.display()
        )));
    }

    if !metadata.is_dir() {
        return Err(crate::error::CompileError::without_span(format!(
            "refusing to clean '{}' because it is not a directory",
            path.display()
        )));
    }

    Ok(())
}

fn remove_clean_path(package_root: &Path, path: &Path) -> Result<()> {
    validate_clean_path(package_root, path)?;
    std::fs::remove_dir_all(path)?;
    Ok(())
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

fn validate_dependency_source_args(git: Option<&str>, path: Option<&Path>, rev: Option<&str>) -> Result<()> {
    if git.is_some() && path.is_some() {
        return Err(crate::error::CompileError::without_span("dependency source accepts either --git or --path, not both"));
    }
    if path.is_some() && rev.is_some() {
        return Err(crate::error::CompileError::without_span("--rev is only valid with --git dependencies"));
    }
    match (git, rev) {
        (Some(_), Some(rev)) => validate_git_revision(rev),
        (Some(_), None) => Err(crate::error::CompileError::without_span(
            "git dependencies must specify --rev with a full commit hash; branch/tag/default-branch dependencies are not accepted",
        )),
        (None, Some(_)) => Err(crate::error::CompileError::without_span("--rev is only valid with --git dependencies")),
        (None, None) => Ok(()),
    }
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

fn dependency_from_add_args(args: &AddArgs) -> Result<Dependency> {
    match (&args.git, &args.path) {
        (Some(git), _) => Ok(Dependency::Detailed(DetailedDependency {
            version: "*".to_string(),
            git: Some(git.clone()),
            branch: None,
            tag: None,
            rev: args.rev.clone(),
            path: None,
            optional: false,
            features: Vec::new(),
            default_features: true,
        })),
        (_, Some(path)) => Ok(Dependency::Detailed(DetailedDependency {
            version: "*".to_string(),
            git: None,
            branch: None,
            tag: None,
            rev: None,
            path: Some(path.display().to_string()),
            optional: false,
            features: Vec::new(),
            default_features: true,
        })),
        _ => Ok(Dependency::Simple("*".to_string())),
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
        Some(actual) if actual == expected => Ok(()),
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

    if args.production || args.deny_runtime_obligations {
        let checked_runtime_evidence_gaps = checked_runtime_proof_plan_evidence_gap_summaries(&metadata.runtime.proof_plan);
        if !checked_runtime_evidence_gaps.is_empty() {
            violations.push(format!("evidence-missing checked-runtime ProofPlan gaps: {}", checked_runtime_evidence_gaps.join(", ")));
        }
    }

    if args.production || args.deny_runtime_obligations {
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

        let partial_obligations = metadata
            .runtime
            .verifier_obligations
            .iter()
            .filter(|obligation| obligation.status == "checked-partial")
            .map(|obligation| format!("{}:{} ({})", obligation.scope, obligation.feature, obligation.category))
            .collect::<Vec<_>>();
        if !partial_obligations.is_empty() {
            violations.push(format!("partial verifier obligations: {}", partial_obligations.join(", ")));
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

        let partial_proof_plan = metadata
            .runtime
            .proof_plan
            .iter()
            .filter(|plan| plan.status == "checked-partial" && !plan.on_chain_checked)
            .map(|plan| format!("{}:{} ({})", plan.origin, plan.feature, plan.codegen_coverage_status))
            .collect::<Vec<_>>();
        if !partial_proof_plan.is_empty() {
            violations.push(format!("partial ProofPlan gaps: {}", partial_proof_plan.join(", ")));
        }

        let unmatched_invariant_action_coverage = invariant_unmatched_action_coverage_summaries(&metadata.runtime.proof_plan);
        if !unmatched_invariant_action_coverage.is_empty() {
            violations.push(format!("unmatched invariant action coverage: {}", unmatched_invariant_action_coverage.join(", ")));
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
    expected_error_lines: Vec<ExpectedErrorLine>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedErrorLine {
    line: usize,
    text: String,
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
        } else if let Some(expected) = directive.strip_prefix("expect-error-line:").map(str::trim) {
            expectation.expect_fail = true;
            expectation.expected_error_lines.push(parse_expected_error_line(path, line_number, expected)?);
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
            "{}: conflicting cellscript-test directives: expect-success cannot be combined with expect-fail/expect-error/expect-error-line",
            path.display()
        )));
    }
    Ok(expectation)
}

fn parse_expected_error_line(path: &Path, zero_based_line: usize, directive: &str) -> Result<ExpectedErrorLine> {
    let Some((line, text)) = directive.split_once(':') else {
        return Err(compile_test_directive_error(
            path,
            zero_based_line,
            "expect-error-line requires N:TEXT, for example expect-error-line:12:type mismatch",
        ));
    };
    let line = line.trim().parse::<usize>().map_err(|_| {
        compile_test_directive_error(path, zero_based_line, "expect-error-line requires a positive numeric source line")
    })?;
    if line == 0 {
        return Err(compile_test_directive_error(path, zero_based_line, "expect-error-line source line must be greater than zero"));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(compile_test_directive_error(path, zero_based_line, "expect-error-line requires non-empty error text"));
    }
    Ok(ExpectedErrorLine { line, text: text.to_string() })
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
            let missing_text = expectation
                .expected_errors
                .iter()
                .filter(|expected| !message.contains(expected.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let missing_line = expectation
                .expected_error_lines
                .iter()
                .filter(|expected| !compile_error_matches_line(&error, &message, expected))
                .map(|expected| format!("{}:{}", expected.line, expected.text))
                .collect::<Vec<_>>();
            if missing_text.is_empty() && missing_line.is_empty() {
                Ok(())
            } else {
                let mut missing = Vec::new();
                if !missing_text.is_empty() {
                    missing.push(format!("text [{}]", missing_text.join(", ")));
                }
                if !missing_line.is_empty() {
                    missing.push(format!("line [{}]", missing_line.join(", ")));
                }
                Err(crate::error::CompileError::without_span(format!(
                    "{}: expected error not found: {}; actual error: {}",
                    path,
                    missing.join(", "),
                    message
                )))
            }
        }
    }
}

fn compile_error_matches_line(error: &crate::error::CompileError, message: &str, expected: &ExpectedErrorLine) -> bool {
    if error.span.line == expected.line && message.contains(expected.text.as_str()) {
        return true;
    }

    let line_marker = format!("line {}", expected.line);
    message.lines().any(|line| line.contains(expected.text.as_str()) && line.contains(&line_marker))
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

fn source_verification_root_for_artifact(artifact_path: &Utf8Path) -> Utf8PathBuf {
    let artifact_dir = artifact_path.parent().unwrap_or_else(|| Utf8Path::new("."));
    match artifact_dir.file_name() {
        Some("artifacts" | "build" | "target") => artifact_dir.parent().unwrap_or(artifact_dir).to_path_buf(),
        _ => artifact_dir.to_path_buf(),
    }
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
    if hex.len() % 2 != 0 {
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
                    .arg(
                        Arg::new("jobs")
                            .long("jobs")
                            .short('j')
                            .value_name("N")
                            .value_parser(clap::value_parser!(usize))
                            .help("Reserved for future parallel package builds; only 1 is currently accepted"),
                    )
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
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
                    .arg(
                        Arg::new("rev")
                            .long("rev")
                            .value_name("COMMIT")
                            .requires("git")
                            .help("Pin a git dependency to a full commit hash"),
                    )
                    .arg(Arg::new("path").long("path").value_name("PATH").help("Add a local path dependency source"))
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON add summary")),
            )
            .subcommand(
                ClapCommand::new("clean")
                    .about("Remove build artifacts")
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON clean summary")),
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
                    )
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("trace-tx")
                    .about("Trace a transaction JSON against v0.16 builder assumptions")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    .arg(
                        Arg::new("primitive-compat")
                            .long("primitive-compat")
                            .value_name("VERSION")
                            .conflicts_with("primitive-strict")
                            .help("Accept primitive metadata compatibility mode"),
                    )
                    .arg(
                        Arg::new("primitive-strict")
                            .long("primitive-strict")
                            .value_name("VERSION")
                            .conflicts_with("primitive-compat")
                            .help("Require strict metadata assurance mode, e.g. 0.16"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("audit-bundle")
                    .about("Generate a v0.16 audit bundle linking metadata, ProofPlan, assumptions, and profile data")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("DIR").help("Output directory"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("certify")
                    .about("Run a deterministic compiler-hosted certification plugin")
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
                ClapCommand::new("validate-tx")
                    .about("Validate a transaction JSON against v0.16 builder assumptions before signing")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    .arg(
                        Arg::new("primitive-compat")
                            .long("primitive-compat")
                            .value_name("VERSION")
                            .conflicts_with("primitive-strict")
                            .help("Accept primitive metadata compatibility mode"),
                    )
                    .arg(
                        Arg::new("primitive-strict")
                            .long("primitive-strict")
                            .value_name("VERSION")
                            .conflicts_with("primitive-compat")
                            .help("Require strict metadata assurance mode, e.g. 0.16"),
                    )
                    .arg(Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit machine-readable JSON")),
            )
            .subcommand(
                ClapCommand::new("solve-tx")
                    .about("Emit a deterministic v0.16 transaction template from metadata and builder assumptions")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON solver template"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb"))
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
                    )
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
                            Arg::new("json").long("json").action(ArgAction::SetTrue).help("Emit a machine-readable JSON builder plan"),
                        ),
                ),
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
                            .help("Require primitive syntax from a specific version (e.g. 0.15 or 0.16), reject legacy forms"),
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
                    .about("Experimental: install a package")
                    .arg(Arg::new("crate").value_name("CRATE"))
                    .arg(Arg::new("version").long("version").value_name("VERSION"))
                    .arg(Arg::new("git").long("git").value_name("URL"))
                    .arg(
                        Arg::new("rev")
                            .long("rev")
                            .value_name("COMMIT")
                            .requires("git")
                            .help("Pin a git dependency to a full commit hash"),
                    )
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
            .get_matches();

        macro_rules! required_string {
            ($matches:expr, $id:literal, $label:literal) => {
                match $matches.get_one::<String>($id).cloned() {
                    Some(value) => value,
                    None => return Command::Invalid(format!("internal CLI parser missing required argument '{}'", $label)),
                }
            };
        }

        macro_rules! required_path {
            ($matches:expr, $id:literal, $label:literal) => {
                PathBuf::from(required_string!($matches, $id, $label))
            };
        }

        match matches.subcommand() {
            Some(("build", m)) => Command::Build(BuildArgs {
                release: m.get_flag("release"),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                entry_action: m.get_one::<String>("entry-action").cloned(),
                entry_lock: m.get_one::<String>("entry-lock").cloned(),
                jobs: m.get_one::<usize>("jobs").copied(),
                json: m.get_flag("json"),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
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
                json: m.get_flag("json"),
            }),
            Some(("new", m)) => Command::New(NewArgs {
                name: required_string!(m, "name", "package name"),
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
                rev: m.get_one::<String>("rev").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                json: m.get_flag("json"),
            }),
            Some(("remove", m)) => Command::Remove(RemoveArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                build: m.get_flag("build"),
                json: m.get_flag("json"),
            }),
            Some(("clean", m)) => Command::Clean(CleanArgs { json: m.get_flag("json") }),
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
            Some(("explain", m)) => {
                Command::Explain(ExplainArgs { code: required_string!(m, "code", "runtime error code"), json: m.get_flag("json") })
            }
            Some(("explain-profile", m)) => Command::ExplainProfile(ExplainProfileArgs {
                profile: required_string!(m, "profile", "target profile"),
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
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
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
                old: required_path!(m, "old", "old metadata"),
                new: required_path!(m, "new", "new metadata"),
                json: m.get_flag("json"),
            }),
            Some(("profile", m)) => Command::Profile(ProfileArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                entry: m.get_one::<String>("entry").cloned(),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                json: m.get_flag("json"),
            }),
            Some(("trace-tx", m)) => Command::TraceTx(TraceTxArgs {
                against: required_path!(m, "against", "metadata"),
                tx: required_path!(m, "tx", "transaction JSON"),
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                json: m.get_flag("json"),
            }),
            Some(("audit-bundle", m)) => Command::AuditBundle(AuditBundleArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                json: m.get_flag("json"),
            }),
            Some(("certify", m)) => Command::Certify(CertifyArgs {
                plugin: required_string!(m, "plugin", "certification plugin"),
                repo_root: m.get_one::<String>("repo-root").map(PathBuf::from),
                report: m.get_one::<String>("report").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: m.get_flag("json"),
                require_production: m.get_flag("require-production"),
            }),
            Some(("validate-tx", m)) => Command::ValidateTx(ValidateTxArgs {
                against: required_path!(m, "against", "metadata"),
                tx: required_path!(m, "tx", "transaction JSON"),
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                json: m.get_flag("json"),
            }),
            Some(("solve-tx", m)) => Command::SolveTx(SolveTxArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                primitive_compat: resolve_metadata_workflow_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                json: m.get_flag("json"),
            }),
            Some(("deploy-plan", m)) => Command::DeployPlan(DeployPlanArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: m.get_flag("json"),
            }),
            Some(("verify-deploy", m)) => {
                Command::VerifyDeploy(VerifyDeployArgs { plan: required_path!(m, "plan", "deploy plan"), json: m.get_flag("json") })
            }
            Some(("diff-deploy", m)) => Command::DiffDeploy(DiffDeployArgs {
                old: required_path!(m, "old", "old deploy plan"),
                new: required_path!(m, "new", "new deploy plan"),
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
                    json: build.get_flag("json"),
                }),
                _ => Command::ActionBuild(ActionBuildArgs::default()),
            },
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
                artifact: required_path!(m, "artifact", "artifact"),
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
                git: m.get_one::<String>("git").cloned(),
                rev: m.get_one::<String>("rev").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
            }),
            Some(("update", _)) => Command::Update,
            Some(("info", m)) => Command::Info(InfoArgs { json: m.get_flag("json") }),
            Some(("login", m)) => Command::Login(LoginArgs { registry: m.get_one::<String>("registry").cloned() }),
            Some((name, _)) => Command::Invalid(format!("internal CLI parser missing handler for subcommand '{}'", name)),
            None => Command::Invalid("internal CLI parser did not receive a subcommand".to_string()),
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

fn resolve_metadata_workflow_primitive_compat(compat: Option<String>, strict: Option<String>) -> Option<String> {
    resolve_primitive_compat(compat, strict).or_else(|| Some(STRICT_V0_16_PRIMITIVE_COMPAT.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof_plan_record(status: &str, coverage: &str, evidence: Vec<crate::ProofPlanEvidenceMetadata>) -> ProofPlanMetadata {
        ProofPlanMetadata {
            name: "test".to_string(),
            origin: "action:test".to_string(),
            category: "create-output".to_string(),
            feature: "create-output:Token:out".to_string(),
            source_span: None,
            trigger: "action".to_string(),
            scope: "group".to_string(),
            reads: Vec::new(),
            coverage: Vec::new(),
            input_output_relation_checks: Vec::new(),
            group_cardinality: "single".to_string(),
            identity_lifecycle_policy: "none".to_string(),
            preserved_fields: Vec::new(),
            witness_fields: Vec::new(),
            lock_args_fields: Vec::new(),
            on_chain_checked: status == "checked-runtime",
            on_chain_checked_obligations: Vec::new(),
            executable_evidence: evidence,
            builder_assumptions: Vec::new(),
            codegen_coverage_status: coverage.to_string(),
            status: status.to_string(),
            detail: "test detail".to_string(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn test_command_execution() {
        let _cmd = Command::Clean(CleanArgs::default());
    }

    #[test]
    fn invalid_parser_mapping_returns_error_instead_of_panicking() {
        let err = CommandExecutor::execute(Command::Invalid("missing parser mapping".to_string()))
            .expect_err("invalid parser mapping should be reported");

        assert!(err.to_string().contains("missing parser mapping"));
    }

    #[test]
    fn ckb_hash_file_rejects_inputs_above_limit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("too-large.bin");
        std::fs::write(&path, vec![0u8; CKB_HASH_FILE_SIZE_LIMIT_BYTES as usize + 1]).unwrap();

        let err = read_ckb_hash_file(&path).expect_err("oversized ckb-hash input should fail");

        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn expected_metadata_hash_comparison_is_case_sensitive() {
        let expected = "ab".repeat(32);
        let actual = expected.to_uppercase();

        let err = validate_expected_metadata_hash("artifact_hash", Some(&actual), Some(&expected)).unwrap_err();

        assert!(err.message.contains("does not match expected"), "unexpected error: {}", err.message);
    }

    fn novaseal_test_plugin_report(production_ready: bool, production_statement_eligible: bool) -> serde_json::Value {
        serde_json::json!({
            "schema": NOVASEAL_PLUGIN_REPORT_SCHEMA,
            "status": if production_ready { "production_ready" } else { "local_production_prep_ready_external_attestation_required" },
            "production_ready": production_ready,
            "production_gates_passed": production_ready,
            "local_production_prep_ready": true,
            "production_statement_eligible": production_statement_eligible,
            "failed_dimensions": [
                "public_shared_cell_dep_attestation",
                "external_bip340_tcb_review_attestation",
                "public_btc_spv_evidence",
                "rwa_legal_registry_review_evidence",
            ],
            "external_blockers": [
                "public_shared_cell_dep_attested",
                "external_bip340_tcb_review_attested",
                "public_btc_spv_evidence_attested",
                "rwa_legal_registry_review_attested",
            ],
            "profile_certification": {
                "schema": NOVASEAL_PROFILE_CERTIFICATION_SCHEMA,
                "profile": NOVASEAL_AGREEMENT_PROFILE,
                "conforms_to": NOVASEAL_CANONICAL_SCHEMA,
                "status": "passed",
                "certification_level": "public_ecosystem_profile_certification_local_ready",
                "production_statement_eligible": production_statement_eligible,
                "production_statement_blockers": [
                    "public_shared_cell_dep_attested",
                    "external_bip340_tcb_review_attested",
                    "public_btc_spv_evidence_attested",
                    "rwa_legal_registry_review_attested"
                ]
            },
            "gates": [
                {
                    "name": NOVASEAL_PROFILE_CERTIFICATION_GATE,
                    "status": "passed"
                }
            ]
        })
    }

    #[test]
    fn novaseal_certification_summary_accepts_local_ready_profile_report() {
        let temp = tempfile::tempdir().unwrap();
        let report_path = temp.path().join("novaseal-production-gates.json");
        let implementation_path = temp.path().join("novaseal_certification.rs");
        let report = novaseal_test_plugin_report(false, false);
        std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        std::fs::write(&implementation_path, b"pub(crate) fn build_report() {}\n").unwrap();

        let summary = novaseal_certification_summary(&report, temp.path(), &report_path, &implementation_path, false, false)
            .expect("certification summary");

        assert_eq!(summary["schema"], NOVASEAL_CERTIFICATION_REPORT_SCHEMA);
        assert_eq!(summary["status"], "passed");
        assert_eq!(summary["plugin"]["id"], NOVASEAL_CERTIFICATION_PLUGIN);
        assert_eq!(summary["plugin"]["kind"], "compiler-builtin-rust");
        assert_eq!(summary["plugin_report"]["schema"], NOVASEAL_PLUGIN_REPORT_SCHEMA);
        assert_eq!(summary["checks"]["local_production_prep_ready"], true);
        assert_eq!(summary["plugin_report"]["production_gates_passed"], false);
        assert_eq!(summary["failed_dimensions"][0], "public_shared_cell_dep_attestation");
        assert_eq!(summary["external_blockers"][0], "public_shared_cell_dep_attested");
        assert_eq!(summary["external_blockers"][3], "rwa_legal_registry_review_attested");
    }

    #[test]
    fn novaseal_certification_summary_requires_v1_local_ready_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let report_path = temp.path().join("novaseal-production-gates.json");
        let implementation_path = temp.path().join("novaseal_certification.rs");
        let mut report = novaseal_test_plugin_report(false, false);
        report["v1_readiness"] = serde_json::json!({
            "status": "planned_profiles_incomplete",
            "local_v1_ready": false,
            "planned_profile_matrix": {
                "missing": ["fungible_xudt_value_flow"]
            }
        });
        std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        std::fs::write(&implementation_path, b"pub(crate) fn build_report() {}\n").unwrap();

        let summary = novaseal_certification_summary(&report, temp.path(), &report_path, &implementation_path, false, false)
            .expect("certification summary");

        assert_eq!(summary["status"], "failed");
        assert_eq!(summary["checks"]["v1_readiness_local_ready"], false);
        assert_eq!(
            summary["failure_reason"]["message"],
            "NovaSeal V1 readiness requires remaining planned profiles and business scenarios"
        );
    }

    #[test]
    fn novaseal_certification_summary_requires_external_attestations_in_production_mode() {
        let temp = tempfile::tempdir().unwrap();
        let report_path = temp.path().join("novaseal-production-gates.json");
        let implementation_path = temp.path().join("novaseal_certification.rs");
        let mut report = novaseal_test_plugin_report(false, false);
        report["production_gates_passed"] = serde_json::json!(true);
        std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        std::fs::write(&implementation_path, b"pub(crate) fn build_report() {}\n").unwrap();

        let summary = novaseal_certification_summary(&report, temp.path(), &report_path, &implementation_path, false, true)
            .expect("certification summary");
        let failed_checks = summary["failure_reason"]["failed_checks"].as_array().expect("failed checks");

        assert_eq!(summary["status"], "failed");
        assert_eq!(summary["plugin_report"]["production_ready"], false);
        assert_eq!(summary["plugin_report"]["production_gates_passed"], true);
        assert!(failed_checks.iter().any(|check| check == "production_ready"));
        assert!(failed_checks.iter().any(|check| check == "production_statement_eligible"));
        assert_eq!(summary["failure_reason"]["failed_dimensions"][0], "public_shared_cell_dep_attestation");
        assert_eq!(summary["failure_reason"]["external_blockers"][0], "public_shared_cell_dep_attested");
        assert_eq!(summary["failure_reason"]["external_blockers"][3], "rwa_legal_registry_review_attested");
        assert_eq!(summary["failure_reason"]["failed_dimensions"][3], "rwa_legal_registry_review_evidence");
    }

    #[test]
    fn novaseal_certification_failure_message_uses_structured_reason_message() {
        let summary = serde_json::json!({
            "failure_reason": {
                "message": "NovaSeal production certification requires remaining external attestations",
                "failed_checks": ["production_ready"]
            }
        });
        assert_eq!(
            novaseal_certification_failure_message(&summary),
            "NovaSeal production certification requires remaining external attestations"
        );

        let legacy = serde_json::json!({ "failure_reason": "legacy certification failure" });
        assert_eq!(novaseal_certification_failure_message(&legacy), "legacy certification failure");

        let missing = serde_json::json!({});
        assert_eq!(novaseal_certification_failure_message(&missing), "certification failed");
    }

    #[test]
    fn production_policy_finds_evidence_less_checked_runtime_proof_plan_gap() {
        let proof_plan = vec![proof_plan_record("checked-runtime", "gap:evidence-missing", Vec::new())];

        let gaps = checked_runtime_proof_plan_evidence_gap_summaries(&proof_plan);

        assert_eq!(gaps, vec!["action:test:create-output:Token:out (gap:evidence-missing)"]);
    }

    #[test]
    fn production_policy_finds_evidence_less_on_chain_checked_proof_plan_gap() {
        let mut proof_plan = vec![proof_plan_record("metadata-only", "gap:evidence-missing", Vec::new())];
        proof_plan[0].on_chain_checked = true;

        let gaps = checked_runtime_proof_plan_evidence_gap_summaries(&proof_plan);

        assert_eq!(gaps, vec!["action:test:create-output:Token:out (gap:evidence-missing)"]);
    }
}

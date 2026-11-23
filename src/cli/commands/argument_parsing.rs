use super::super::artifact::{ArtifactArgs, ArtifactOperation};
use super::super::certification::CertifyArgs;
use super::super::output::json_output;
use super::{
    auth_capability_args_from_matches, auth_capability_revoke_args_from_matches, auth_capability_submit_args_from_matches,
    auth_namespace_claim_args_from_matches, auth_reproducer_create_args_from_matches, resolve_primitive_compat,
    update_plan_args_from_matches, warn_legacy_alias, AbiArgs, ActionBuildArgs, AddArgs, AuditBundleArgs, BuildArgs, BuildPlanArgs,
    CheckArgs, CkbHashArgs, CkbStdCompatArgs, CleanArgs, CliParser, Command, ConstraintsArgs, DeployPlanArgs, DiffDeployArgs, DocArgs,
    EntryWitnessArgs, ExpandArgs, ExplainArgs, ExplainAssumptionsArgs, ExplainCollectionsArgs, ExplainGenericsArgs, ExplainGraphArgs,
    ExplainProfileArgs, ExplainProofArgs, FmtArgs, GenBuilderArgs, InfoArgs, InitArgs, InstallArgs, InterfaceArgs, InterfaceDiffArgs,
    LockDepsArgs, LoginArgs, MetadataArgs, NewArgs, PackageLockArgs, PackageVerifyArgs, ProfileArgs, ProofDiffArgs,
    ProtocolBundleCheckArgs, PublishArgs, ReceiptArgs, RegistryAddArgs, RegistryEditArgs, RegistryVerifyArgs, RemoveArgs,
    ResolveGraphArgs, RunArgs, SchedulerPlanArgs, SchemaAckArgs, SignReceiptArgs, SolveTxArgs, TestArgs, TraceTxArgs, UpdateArgs,
    ValidateTxArgs, VerifyArtifactArgs, VerifyCkbFixturesArgs, VerifyDeployArgs, VerifyReceiptArgs,
};
use crate::docgen::OutputFormat;
use crate::error::Result;
use std::path::PathBuf;

impl CliParser {
    pub(super) fn parse_matches(matches: clap::ArgMatches) -> Result<Command> {
        Ok(match matches.subcommand() {
            Some(("build", m)) => Command::Build(BuildArgs {
                source_contracts: m.get_flag("source-contracts"),
                release: m.get_flag("release"),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                entry_action: m.get_one::<String>("entry-action").cloned(),
                entry_lock: m.get_one::<String>("entry-lock").cloned(),
                artifact: m.get_one::<String>("artifact").cloned(),
                jobs: m.get_one::<String>("jobs").and_then(|s| s.parse().ok()),
                features: m.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
                all_features: m.get_flag("all-features"),
                no_default_features: m.get_flag("no-default-features"),
                locked: m.get_flag("locked"),
                frozen: m.get_flag("frozen"),
                offline: m.get_flag("offline"),
                environment: m.get_one::<String>("environment").cloned(),
                json: json_output(m),
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
                backend: m.get_one::<String>("backend").cloned(),
                no_run: m.get_flag("no-run"),
                nocapture: m.get_flag("nocapture"),
                fail_fast: m.get_flag("fail-fast"),
                doc: m.get_flag("doc"),
                features: m.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
                all_features: m.get_flag("all-features"),
                no_default_features: m.get_flag("no-default-features"),
                locked: m.get_flag("locked"),
                frozen: m.get_flag("frozen"),
                offline: m.get_flag("offline"),
                environment: m.get_one::<String>("environment").cloned(),
                json: json_output(m),
                ..Default::default()
            }),
            Some(("doc", m)) => Command::Doc(DocArgs {
                open: m.get_flag("open"),
                json: json_output(m),
                output_format: match m.get_one::<String>("format").map(|s| s.as_str()) {
                    Some("markdown") => OutputFormat::Markdown,
                    Some("json") => OutputFormat::Json,
                    _ => OutputFormat::Html,
                },
                ..Default::default()
            }),
            Some(("fmt", m)) => Command::Fmt(FmtArgs {
                check: m.get_flag("check"),
                json: json_output(m),
                files: m.get_many::<String>("files").map(|v| v.map(PathBuf::from).collect()).unwrap_or_default(),
            }),
            Some(("init", m)) => Command::Init(InitArgs {
                name: m.get_one::<String>("name").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                lib: m.get_flag("lib"),
                namespace: m.get_one::<String>("namespace").cloned(),
                json: json_output(m),
            }),
            Some(("new", m)) => Command::New(NewArgs {
                name: m.get_one::<String>("name").cloned().expect("required package name"),
                path: m.get_one::<String>("path").map(PathBuf::from),
                lib: m.get_flag("lib"),
                vcs: m.get_one::<String>("vcs").cloned().unwrap_or_else(|| "git".to_string()),
                json: json_output(m),
            }),
            Some(("add", m)) => Command::Add(AddArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                package: m.get_one::<String>("package-name").cloned(),
                dev: m.get_flag("dev"),
                build: m.get_flag("build"),
                git: m.get_one::<String>("git").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                use_environment: m.get_one::<String>("use-environment").cloned(),
                environment_independent: m.get_flag("environment-independent"),
                json: json_output(m),
            }),
            Some(("remove", m)) => Command::Remove(RemoveArgs {
                crates: m.get_many::<String>("crates").map(|v| v.cloned().collect()).unwrap_or_default(),
                dev: m.get_flag("dev"),
                build: m.get_flag("build"),
                json: json_output(m),
            }),
            Some(("clean", m)) => Command::Clean(CleanArgs { json: json_output(m), cache: m.get_flag("cache") }),
            Some(("repl", _)) => Command::Repl,
            Some(("check", m)) => Command::Check(CheckArgs {
                all_targets: m.get_flag("all-targets"),
                artifact: m.get_one::<String>("artifact").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                features: m.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
                all_features: m.get_flag("all-features"),
                no_default_features: m.get_flag("no-default-features"),
                locked: m.get_flag("locked"),
                frozen: m.get_flag("frozen"),
                offline: m.get_flag("offline"),
                environment: m.get_one::<String>("environment").cloned(),
                package: m.get_one::<String>("package").cloned(),
                workspace: m.get_flag("workspace"),
            }),
            Some(("metadata", m)) => Command::Metadata(MetadataArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                artifact: m.get_one::<String>("artifact").cloned(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
            }),
            Some(("resolve-graph", m)) => Command::ResolveGraph(ResolveGraphArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                package: m.get_one::<String>("package").cloned(),
                scope: m.get_one::<String>("scope").cloned().unwrap_or_else(|| "runtime".to_string()),
                features: m.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
                all_features: m.get_flag("all-features"),
                no_default_features: m.get_flag("no-default-features"),
                environment: m.get_one::<String>("environment").cloned(),
                offline: m.get_flag("offline"),
                schema_version: m.get_one::<u32>("schema-version").copied().unwrap_or(1),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: json_output(m),
            }),
            Some(("build-plan", m)) => Command::BuildPlan(BuildPlanArgs {
                source_contracts: m.get_flag("source-contracts"),
                input: m.get_one::<String>("input").map(PathBuf::from),
                package: m.get_one::<String>("package").cloned(),
                scope: m.get_one::<String>("scope").cloned().unwrap_or_else(|| "runtime".to_string()),
                features: m.get_many::<String>("features").map(|values| values.cloned().collect()).unwrap_or_default(),
                all_features: m.get_flag("all-features"),
                no_default_features: m.get_flag("no-default-features"),
                environment: m.get_one::<String>("environment").cloned(),
                offline: m.get_flag("offline"),
                schema_version: m
                    .get_one::<u32>("schema-version")
                    .copied()
                    .unwrap_or(crate::package::inspection::BUILD_PLAN_SCHEMA_VERSION),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                release: m.get_flag("release"),
                debug: m.get_flag("debug"),
                primitive_compat: resolve_primitive_compat(
                    m.get_one::<String>("primitive-compat").cloned(),
                    m.get_one::<String>("primitive-strict").cloned(),
                ),
                entry_action: m.get_one::<String>("entry-action").cloned(),
                entry_lock: m.get_one::<String>("entry-lock").cloned(),
                artifact: m.get_one::<String>("artifact").cloned(),
                production: m.get_flag("production"),
                deny_fail_closed: m.get_flag("deny-fail-closed"),
                deny_ckb_runtime: m.get_flag("deny-ckb-runtime"),
                deny_runtime_obligations: m.get_flag("deny-runtime-obligations"),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: json_output(m),
            }),
            Some(("expand", m)) => Command::Expand(ExpandArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                artifact: m.get_one::<String>("artifact").cloned(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("interface", m)) => Command::Interface(InterfaceArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("interface-diff", m)) => Command::InterfaceDiff(InterfaceDiffArgs {
                old: m.get_one::<String>("old").map(PathBuf::from).expect("required old interface"),
                new: m.get_one::<String>("new").map(PathBuf::from).expect("required new interface"),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: json_output(m),
                checked: m.get_flag("checked"),
            }),
            Some(("schema-ack", m)) => Command::SchemaAck(SchemaAckArgs {
                upgrade_plan: m.get_one::<String>("upgrade-plan").map(PathBuf::from),
                subject: m.get_one::<String>("subject").cloned(),
                environment: m.get_one::<String>("environment").cloned(),
                module: m.get_one::<String>("module").cloned(),
                old: m.get_one::<String>("old").map(PathBuf::from).unwrap_or_default(),
                new: m.get_one::<String>("new").map(PathBuf::from).unwrap_or_default(),
                action: m.get_one::<String>("action").cloned().expect("required action"),
                before: m.get_one::<String>("before").cloned().expect("required predecessor role"),
                after: m.get_one::<String>("after").cloned().expect("required successor role"),
                output: m.get_one::<String>("output").map(PathBuf::from),
                acknowledge_by: m.get_one::<String>("acknowledge-by").cloned(),
                rationale: m.get_one::<String>("rationale").cloned(),
                verify: m.get_one::<String>("verify").map(PathBuf::from),
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
                json: json_output(m),
            }),
            Some(("ckb-std-compat", m)) => Command::CkbStdCompat(CkbStdCompatArgs { json: json_output(m) }),
            Some(("explain", m)) => match m.subcommand() {
                Some(("profile", profile)) => Command::ExplainProfile(ExplainProfileArgs {
                    profile: profile.get_one::<String>("profile").cloned().expect("required target profile"),
                    json: json_output(profile),
                }),
                Some(("proof", proof)) => Command::ExplainProof(ExplainProofArgs {
                    input: proof.get_one::<String>("input").map(PathBuf::from),
                    target: proof.get_one::<String>("target").cloned(),
                    target_profile: proof.get_one::<String>("target-profile").cloned(),
                    json: json_output(proof),
                }),
                Some(("assumptions", assumptions)) => Command::ExplainAssumptions(ExplainAssumptionsArgs {
                    input: assumptions.get_one::<String>("input").map(PathBuf::from),
                    target: assumptions.get_one::<String>("target").cloned(),
                    target_profile: assumptions.get_one::<String>("target-profile").cloned(),
                    json: json_output(assumptions),
                }),
                Some(("generics", generics)) => Command::ExplainGenerics(ExplainGenericsArgs {
                    input: generics.get_one::<String>("input").map(PathBuf::from),
                    target: generics.get_one::<String>("target").cloned(),
                    target_profile: generics.get_one::<String>("target-profile").cloned(),
                    json: json_output(generics),
                }),
                Some(("collections", collections)) => Command::ExplainCollections(ExplainCollectionsArgs {
                    input: collections.get_one::<String>("input").map(PathBuf::from),
                    json: json_output(collections),
                }),
                Some(("graph", graph)) => Command::ExplainGraph(ExplainGraphArgs {
                    input: graph.get_one::<String>("input").map(PathBuf::from),
                    target: graph.get_one::<String>("target").cloned(),
                    target_profile: graph.get_one::<String>("target-profile").cloned(),
                    json: json_output(graph),
                    format: graph.get_one::<String>("format").cloned(),
                }),
                _ => Command::Explain(ExplainArgs {
                    code: m.get_one::<String>("code").cloned().expect("required runtime error code"),
                    json: json_output(m),
                }),
            },
            Some(("explain-profile", m)) => {
                warn_legacy_alias("explain-profile", "explain profile", json_output(m));
                Command::ExplainProfile(ExplainProfileArgs {
                    profile: m.get_one::<String>("profile").cloned().expect("required target profile"),
                    json: json_output(m),
                })
            }
            Some(("explain-proof", m)) => {
                warn_legacy_alias("explain-proof", "explain proof", json_output(m));
                Command::ExplainProof(ExplainProofArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("explain-assumptions", m)) => {
                warn_legacy_alias("explain-assumptions", "explain assumptions", json_output(m));
                Command::ExplainAssumptions(ExplainAssumptionsArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("explain-generics", m)) => {
                warn_legacy_alias("explain-generics", "explain generics", json_output(m));
                Command::ExplainGenerics(ExplainGenericsArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("explain-graph", m)) => {
                warn_legacy_alias("explain-graph", "explain graph", json_output(m));
                Command::ExplainGraph(ExplainGraphArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                    format: m.get_one::<String>("format").cloned(),
                })
            }
            Some(("proof-diff", m)) => Command::ProofDiff(ProofDiffArgs {
                old: m.get_one::<String>("old").map(PathBuf::from).expect("required old metadata"),
                new: m.get_one::<String>("new").map(PathBuf::from).expect("required new metadata"),
                json: json_output(m),
            }),
            Some(("profile", m)) => Command::Profile(ProfileArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                entry: m.get_one::<String>("entry").cloned(),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("tx", m)) => match m.subcommand() {
                Some(("validate", validate)) => Command::ValidateTx(ValidateTxArgs {
                    against: validate.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                    tx: validate.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                    json: json_output(validate),
                }),
                Some(("solve", solve)) => Command::SolveTx(SolveTxArgs {
                    input: solve.get_one::<String>("input").map(PathBuf::from),
                    output: solve.get_one::<String>("output").map(PathBuf::from),
                    target: solve.get_one::<String>("target").cloned(),
                    target_profile: solve.get_one::<String>("target-profile").cloned(),
                    json: json_output(solve),
                }),
                Some(("trace", trace)) => Command::TraceTx(TraceTxArgs {
                    against: trace.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                    tx: trace.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                    json: json_output(trace),
                }),
                _ => unreachable!(),
            },
            Some(("trace-tx", m)) => {
                warn_legacy_alias("trace-tx", "tx trace", json_output(m));
                Command::TraceTx(TraceTxArgs {
                    against: m.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                    tx: m.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                    json: json_output(m),
                })
            }
            Some(("audit-bundle", m)) => Command::AuditBundle(AuditBundleArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("validate-tx", m)) => {
                warn_legacy_alias("validate-tx", "tx validate", json_output(m));
                Command::ValidateTx(ValidateTxArgs {
                    against: m.get_one::<String>("against").map(PathBuf::from).expect("required metadata"),
                    tx: m.get_one::<String>("tx").map(PathBuf::from).expect("required transaction JSON"),
                    json: json_output(m),
                })
            }
            Some(("solve-tx", m)) => {
                warn_legacy_alias("solve-tx", "tx solve", json_output(m));
                Command::SolveTx(SolveTxArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    output: m.get_one::<String>("output").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("verify-ckb-fixtures", m)) => Command::VerifyCkbFixtures(VerifyCkbFixturesArgs {
                manifest: m.get_one::<String>("manifest").map(PathBuf::from).expect("required CKB fixture manifest"),
                json: json_output(m),
            }),
            Some(("deploy", m)) => match m.subcommand() {
                Some(("plan", plan)) => Command::DeployPlan(DeployPlanArgs {
                    input: plan.get_one::<String>("input").map(PathBuf::from),
                    output: plan.get_one::<String>("output").map(PathBuf::from),
                    target: plan.get_one::<String>("target").cloned(),
                    target_profile: plan.get_one::<String>("target-profile").cloned(),
                    json: json_output(plan),
                }),
                Some(("verify", verify)) => Command::VerifyDeploy(VerifyDeployArgs {
                    plan: verify.get_one::<String>("plan").map(PathBuf::from).expect("required deploy plan"),
                    json: json_output(verify),
                }),
                Some(("diff", diff)) => Command::DiffDeploy(DiffDeployArgs {
                    old: diff.get_one::<String>("old").map(PathBuf::from).expect("required old deploy plan"),
                    new: diff.get_one::<String>("new").map(PathBuf::from).expect("required new deploy plan"),
                    json: json_output(diff),
                }),
                Some(("lock-deps", lock_deps)) => Command::LockDeps(LockDepsArgs {
                    input: lock_deps.get_one::<String>("input").map(PathBuf::from),
                    output: lock_deps.get_one::<String>("output").map(PathBuf::from),
                    target: lock_deps.get_one::<String>("target").cloned(),
                    target_profile: lock_deps.get_one::<String>("target-profile").cloned(),
                    json: json_output(lock_deps),
                }),
                _ => unreachable!(),
            },
            Some(("deploy-plan", m)) => {
                warn_legacy_alias("deploy-plan", "deploy plan", json_output(m));
                Command::DeployPlan(DeployPlanArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    output: m.get_one::<String>("output").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("verify-deploy", m)) => {
                warn_legacy_alias("verify-deploy", "deploy verify", json_output(m));
                Command::VerifyDeploy(VerifyDeployArgs {
                    plan: m.get_one::<String>("plan").map(PathBuf::from).expect("required deploy plan"),
                    json: json_output(m),
                })
            }
            Some(("diff-deploy", m)) => {
                warn_legacy_alias("diff-deploy", "deploy diff", json_output(m));
                Command::DiffDeploy(DiffDeployArgs {
                    old: m.get_one::<String>("old").map(PathBuf::from).expect("required old deploy plan"),
                    new: m.get_one::<String>("new").map(PathBuf::from).expect("required new deploy plan"),
                    json: json_output(m),
                })
            }
            Some(("lock-deps", m)) => {
                warn_legacy_alias("lock-deps", "deploy lock-deps", json_output(m));
                Command::LockDeps(LockDepsArgs {
                    input: m.get_one::<String>("input").map(PathBuf::from),
                    output: m.get_one::<String>("output").map(PathBuf::from),
                    target: m.get_one::<String>("target").cloned(),
                    target_profile: m.get_one::<String>("target-profile").cloned(),
                    json: json_output(m),
                })
            }
            Some(("action", m)) => match m.subcommand() {
                Some(("build", build)) => Command::ActionBuild(ActionBuildArgs {
                    input: build.get_one::<String>("input").map(PathBuf::from),
                    action: build.get_one::<String>("action").cloned(),
                    output: build.get_one::<String>("output").map(PathBuf::from),
                    target: build.get_one::<String>("target").cloned(),
                    target_profile: build.get_one::<String>("target-profile").cloned(),
                    fabric_intent: build.get_flag("fabric-intent"),
                    json: json_output(build),
                }),
                _ => Command::ActionBuild(ActionBuildArgs::default()),
            },
            Some(("gen-builder", m)) => Command::GenBuilder(GenBuilderArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                artifact: m.get_one::<String>("artifact").cloned(),
                metadata: m.get_one::<String>("metadata").map(PathBuf::from),
                lockfile: m.get_one::<String>("lockfile").map(PathBuf::from),
                deployed: m.get_one::<String>("deployed").map(PathBuf::from),
                deployment_network: m.get_one::<String>("deployment-network").cloned(),
                action: m.get_one::<String>("action").cloned(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned().unwrap_or_else(|| "typescript".to_string()),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                package_name: m.get_one::<String>("package-name").cloned(),
                json: json_output(m),
            }),
            Some(("entry-witness", m)) => Command::EntryWitness(EntryWitnessArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                artifact: m.get_one::<String>("artifact").cloned(),
                script_hash: m.get_one::<String>("script-hash").cloned(),
                action: m.get_one::<String>("action").cloned(),
                lock: m.get_one::<String>("lock").cloned(),
                args: m.get_many::<String>("arg").map(|values| values.cloned().collect()).unwrap_or_default(),
                output: m.get_one::<String>("output").map(PathBuf::from),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("receipt", m)) => Command::Receipt(ReceiptArgs {
                input: m.get_one::<String>("input").map(PathBuf::from),
                output: m.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                target: m.get_one::<String>("target").cloned(),
                target_profile: m.get_one::<String>("target-profile").cloned(),
                json: json_output(m),
            }),
            Some(("sign-receipt", m)) => Command::SignReceipt(SignReceiptArgs {
                receipt: m.get_one::<String>("receipt").map(PathBuf::from).expect("required receipt"),
                role: m.get_one::<String>("role").cloned().expect("required role"),
                key: m.get_one::<String>("key").cloned().expect("required key"),
                output: m.get_one::<String>("output").map(PathBuf::from),
                json: json_output(m),
            }),
            Some(("verify-receipt", m)) => Command::VerifyReceipt(VerifyReceiptArgs {
                receipt: m.get_one::<String>("receipt").map(PathBuf::from).expect("required receipt"),
                metadata: m.get_one::<String>("metadata").map(PathBuf::from).expect("required metadata"),
                artifact: m.get_one::<String>("artifact").map(PathBuf::from).expect("required artifact"),
                json: json_output(m),
            }),
            Some(("protocol", protocol)) => match protocol.subcommand() {
                Some(("bundle", bundle)) => match bundle.subcommand() {
                    Some(("check", check)) => Command::ProtocolBundleCheck(ProtocolBundleCheckArgs {
                        manifest: check.get_one::<String>("manifest").map(PathBuf::from).expect("required ProtocolBundle manifest"),
                        open_admission: check.get_one::<String>("open-admission").map(PathBuf::from),
                        output: check.get_one::<String>("output").map(PathBuf::from),
                        json: json_output(check),
                    }),
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            Some(("verify-artifact", m)) => Command::VerifyArtifact(VerifyArtifactArgs {
                artifact: m.get_one::<String>("artifact").map(PathBuf::from).expect("required artifact"),
                metadata: m.get_one::<String>("metadata").map(PathBuf::from),
                lowering_record: m.get_one::<String>("lowering-record").map(PathBuf::from),
                source_map: m.get_one::<String>("source-map").map(PathBuf::from),
                receipt: m.get_one::<String>("receipt").map(PathBuf::from),
                verify_sources: m.get_flag("verify-sources"),
                json: json_output(m),
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
                json: json_output(m),
            }),
            Some(("artifact", m)) => Command::Artifact(ArtifactArgs {
                operation: match m.subcommand() {
                    Some(("ls-idl", action)) => match action.subcommand() {
                        Some(("validate", command)) => ArtifactOperation::LsIdlValidate {
                            idl: command.get_one::<String>("idl").map(PathBuf::from).expect("required IDL"),
                            executable: command.get_one::<String>("executable").map(PathBuf::from),
                            json: json_output(command),
                        },
                        Some(("bind", command)) => ArtifactOperation::LsIdlBind {
                            idl: command.get_one::<String>("idl").map(PathBuf::from).expect("required IDL"),
                            executable: command.get_one::<String>("executable").map(PathBuf::from).expect("required executable"),
                            output: command.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                            force: command.get_flag("force"),
                            json: json_output(command),
                        },
                        Some(("fetch", command)) => ArtifactOperation::LsIdlFetch {
                            code_hash: command.get_one::<String>("code-hash").cloned().expect("required code hash"),
                            hash_type: command.get_one::<String>("hash-type").cloned(),
                            data_hash: command.get_one::<String>("data-hash").cloned(),
                            network: command.get_one::<String>("network").cloned().expect("defaulted network"),
                            output: command.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                            api_url: command.get_one::<String>("api-url").cloned(),
                            force: command.get_flag("force"),
                            json: json_output(command),
                        },
                        Some(("bundle", command)) => ArtifactOperation::LsIdlBundle {
                            idl: command.get_one::<String>("idl").map(PathBuf::from).expect("required IDL"),
                            executable: command.get_one::<String>("executable").map(PathBuf::from).expect("required executable"),
                            source: command.get_one::<String>("source").map(PathBuf::from).expect("required source"),
                            namespace: command.get_one::<String>("namespace").cloned().expect("required namespace"),
                            name: command.get_one::<String>("name").cloned().expect("required name"),
                            release: command.get_one::<String>("release").cloned().expect("required release"),
                            language: command.get_one::<String>("language").cloned().expect("required language"),
                            hash_type: command.get_one::<String>("hash-type").cloned().expect("defaulted hash type"),
                            dep_type: command.get_one::<String>("dep-type").cloned().expect("defaulted dep type"),
                            toolchain: command.get_one::<String>("toolchain").cloned().expect("required toolchain"),
                            source_revision: command.get_one::<String>("source-revision").cloned().expect("required source revision"),
                            output: command.get_one::<String>("output").map(PathBuf::from).expect("defaulted output"),
                            artifact_manifest_output: command
                                .get_one::<String>("artifact-manifest-output")
                                .map(PathBuf::from)
                                .expect("defaulted artifact manifest output"),
                            force: command.get_flag("force"),
                            json: json_output(command),
                        },
                        _ => unreachable!(),
                    },
                    Some(("fetch", action)) => ArtifactOperation::Fetch {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                        receipt: action.get_one::<String>("receipt").map(PathBuf::from),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    Some(("verify", action)) => ArtifactOperation::Verify {
                        bundle: action.get_one::<String>("bundle").map(PathBuf::from).expect("required bundle"),
                        receipt: action.get_one::<String>("receipt").map(PathBuf::from).expect("required receipt"),
                        json: json_output(action),
                    },
                    Some(("pin", action)) => ArtifactOperation::Pin {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("defaulted output"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        accept_hash_bound: action.get_flag("accept-hash-bound"),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    Some(("copy", action)) => ArtifactOperation::Copy {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        destination: action.get_one::<String>("destination").map(PathBuf::from).expect("defaulted destination"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        accept_hash_bound: action.get_flag("accept-hash-bound"),
                        json: json_output(action),
                    },
                    Some(("cell-dep", action)) => ArtifactOperation::CellDep {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        rpc_url: action.get_one::<String>("rpc-url").cloned(),
                        accept_hash_bound: action.get_flag("accept-hash-bound"),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    Some(("record-deployment", action)) => ArtifactOperation::RecordDeployment {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        network: action.get_one::<String>("network").cloned().expect("defaulted network"),
                        code_hash: action.get_one::<String>("code-hash").cloned().expect("required code hash"),
                        hash_type: action.get_one::<String>("hash-type").cloned().expect("required hash type"),
                        dep_type: action.get_one::<String>("dep-type").cloned().expect("required dep type"),
                        tx_hash: action.get_one::<String>("tx-hash").cloned().expect("required tx hash"),
                        index: *action.get_one::<u32>("index").expect("required index"),
                        capability_key_id: action.get_one::<String>("capability-key-id").cloned().expect("required capability key id"),
                        capability_signature: action.get_one::<String>("capability-signature").cloned(),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        print_payload: action.get_flag("print-payload"),
                        json: json_output(action),
                    },
                    Some(("set-availability", action)) => ArtifactOperation::SetAvailability {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        status: action.get_one::<String>("status").cloned().expect("required status"),
                        reason: action.get_one::<String>("reason").cloned(),
                        capability_key_id: action.get_one::<String>("capability-key-id").cloned().expect("required capability key id"),
                        capability_signature: action.get_one::<String>("capability-signature").cloned(),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        print_payload: action.get_flag("print-payload"),
                        json: json_output(action),
                    },
                    Some(("reproduction-report", action)) => ArtifactOperation::ReproductionReport {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        artifact: action.get_one::<String>("artifact").map(PathBuf::from).expect("required artifact"),
                        build_log: action.get_one::<String>("build-log").map(PathBuf::from).expect("required build log"),
                        builder_id: action.get_one::<String>("builder-id").cloned().expect("required builder id"),
                        trust_domain: action.get_one::<String>("trust-domain").cloned().expect("required trust domain"),
                        builder_key_id: action.get_one::<String>("builder-key-id").cloned().expect("required builder key id"),
                        builder_public_key: action
                            .get_one::<String>("builder-public-key")
                            .cloned()
                            .expect("required builder public key"),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    Some(("reproduction-evidence", action)) => ArtifactOperation::ReproductionEvidence {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        reports: action
                            .get_many::<String>("report")
                            .expect("required reproduction reports")
                            .map(PathBuf::from)
                            .collect(),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    Some(("commitment", action)) => ArtifactOperation::Commitment {
                        coordinate: action.get_one::<String>("coordinate").cloned().expect("required coordinate"),
                        output: action.get_one::<String>("output").map(PathBuf::from).expect("required output"),
                        api_url: action.get_one::<String>("api-url").cloned(),
                        force: action.get_flag("force"),
                        json: json_output(action),
                    },
                    _ => unreachable!(),
                },
            }),
            Some(("publish", m)) => Command::Publish(PublishArgs {
                dry_run: m.get_flag("dry-run"),
                offline: m.get_flag("offline"),
                allow_dirty: m.get_flag("allow-dirty"),
                api_url: m.get_one::<String>("api-url").cloned(),
                capability_key_id: m.get_one::<String>("capability-key-id").cloned(),
                authorise: m.get_flag("authorise"),
                no_open: m.get_flag("no-open"),
                capability_signature: m.get_one::<String>("capability-signature").cloned(),
                idempotency_key: m.get_one::<String>("idempotency-key").cloned(),
                payload: m.get_one::<String>("payload").map(PathBuf::from),
                source_snapshot: m.get_one::<String>("source-snapshot").map(PathBuf::from),
                artifact_manifest: m.get_one::<String>("artifact-manifest").map(PathBuf::from),
                artifact_kind: m.get_one::<String>("artifact-kind").cloned(),
                print_payload: m.get_flag("print-payload"),
                json: json_output(m),
            }),
            Some(("install", m)) => Command::Install(InstallArgs {
                crate_name: m.get_one::<String>("crate").cloned(),
                version: m.get_one::<String>("version").cloned(),
                namespace: m.get_one::<String>("namespace").cloned(),
                git: m.get_one::<String>("git").cloned(),
                path: m.get_one::<String>("path").map(PathBuf::from),
                allow_unverified: m.get_flag("allow-unverified"),
                allow_quarantined: m.get_flag("allow-quarantined"),
            }),
            Some(("lock", m)) => Command::Lock(PackageLockArgs { json: json_output(m) }),
            Some(("update-plan", m)) => Command::UpdatePlan(update_plan_args_from_matches(m)),
            Some(("update", m)) => Command::Update(UpdateArgs {
                plan: m.get_one::<String>("apply-plan").map(PathBuf::from),
                planning: update_plan_args_from_matches(m),
            }),
            Some(("info", m)) => Command::Info(InfoArgs { json: json_output(m) }),
            Some(("language-contract", m)) => Command::LanguageContract(InfoArgs { json: json_output(m) }),
            Some(("login", m)) => {
                warn_legacy_alias("login", "auth capability create", false);
                Command::Login(LoginArgs { registry: m.get_one::<String>("registry").cloned() })
            }
            Some(("auth", m)) => match m.subcommand() {
                Some(("login", login)) => Command::AuthLogin(auth_capability_args_from_matches(login)),
                Some(("capability", capability)) => match capability.subcommand() {
                    Some(("create", create)) => Command::AuthCapabilityCreate(auth_capability_args_from_matches(create)),
                    Some(("submit", submit)) => Command::AuthCapabilitySubmit(auth_capability_submit_args_from_matches(submit)),
                    Some(("revoke", revoke)) => Command::AuthCapabilityRevoke(auth_capability_revoke_args_from_matches(revoke)),
                    _ => unreachable!(),
                },
                Some(("reproducer", reproducer)) => match reproducer.subcommand() {
                    Some(("create", create)) => Command::AuthReproducerCreate(auth_reproducer_create_args_from_matches(create)),
                    _ => unreachable!(),
                },
                Some(("namespace", namespace)) => match namespace.subcommand() {
                    Some(("claim", claim)) => Command::AuthNamespaceClaim(auth_namespace_claim_args_from_matches(claim)),
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            Some(("certify", m)) => Command::Certify(CertifyArgs::from_matches(m, json_output(m))?),
            Some(("package", m)) => match m.subcommand() {
                Some(("lock", lock)) => Command::Lock(PackageLockArgs { json: json_output(lock) }),
                Some(("update-plan", update)) => Command::UpdatePlan(update_plan_args_from_matches(update)),
                Some(("update", update)) => Command::Update(UpdateArgs {
                    plan: update.get_one::<String>("apply-plan").map(PathBuf::from),
                    planning: update_plan_args_from_matches(update),
                }),
                Some(("verify", verify)) => Command::PackageVerify(PackageVerifyArgs { json: json_output(verify) }),
                _ => unreachable!(),
            },
            Some(("registry", m)) => match m.subcommand() {
                Some(("verify", verify)) => Command::RegistryVerify(RegistryVerifyArgs {
                    json: json_output(verify),
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
                Some(("edit", edit)) => Command::RegistryEdit(RegistryEditArgs {
                    yank: edit.get_one::<String>("yank").cloned(),
                    reason: edit.get_one::<String>("reason").cloned(),
                    replaced_by: edit.get_one::<String>("replaced-by").cloned(),
                    yanked_at: edit.get_one::<String>("yanked-at").cloned(),
                }),
                _ => unreachable!(),
            },
            Some(("registry-verify", m)) => {
                warn_legacy_alias("registry-verify", "registry verify", json_output(m));
                Command::RegistryVerify(RegistryVerifyArgs {
                    json: json_output(m),
                    live: m.get_flag("live"),
                    rpc_url: m.get_one::<String>("rpc-url").cloned(),
                    network: m.get_one::<String>("network").cloned(),
                    require_publisher_signature: m.get_flag("require-publisher-signature"),
                    require_audit_report: m.get_flag("require-audit-report"),
                })
            }
            Some(("package-verify", m)) => {
                warn_legacy_alias("package-verify", "package verify", json_output(m));
                Command::PackageVerify(PackageVerifyArgs { json: json_output(m) })
            }
            Some(("registry-add", m)) => {
                warn_legacy_alias("registry-add", "registry add", false);
                Command::RegistryAdd(RegistryAddArgs {
                    namespace: m.get_one::<String>("namespace").cloned().unwrap_or_default(),
                    name: m.get_one::<String>("name").cloned().unwrap_or_default(),
                    source: m.get_one::<String>("source").cloned().unwrap_or_default(),
                })
            }
            _ => unreachable!(),
        })
    }
}

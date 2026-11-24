use super::super::certification::CertifyArgs;
use super::{update_clap_command, CliParser};

impl CliParser {
    pub fn command() -> clap::Command {
        use clap::{Arg, ArgAction, Command as ClapCommand};

        ClapCommand::new("cellc")
            .version(crate::VERSION)
            .about("CellScript compiler for CKB blockchain")
            .subcommand_required(true)
            .arg_required_else_help(true)
            .arg(
                Arg::new("color")
                    .long("color")
                    .value_name("WHEN")
                    .value_parser(["auto", "always", "never"])
                    .global(true)
                    .help("Control ANSI colour output: auto, always, or never"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .global(true)
                    .action(ArgAction::SetTrue)
                    .help("Emit one machine-readable JSON result on stdout for success or failure"),
            )
            .arg(
                Arg::new("message-format")
                    .long("message-format")
                    .value_name("FORMAT")
                    .value_parser(["human", "json"])
                    .global(true)
                    .hide(true)
                    .help("Deprecated compatibility alias for --json"),
            )
            .subcommand(
                ClapCommand::new("build")
                    .display_order(10)
                    .about("Compile the current package")
                    .arg(Arg::new("source-contracts").long("source-contracts").action(ArgAction::SetTrue).help("Emit bounded source declaration evidence for independent interface inspection"))
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Build in release mode"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
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
                        Arg::new("artifact")
                            .long("artifact")
                            .value_name("NAME")
                            .conflicts_with_all(["entry-action", "entry-lock"])
                            .help("Compile the named artifact declared in Cell.toml"),
                    )
                    .arg(Arg::new("jobs").long("jobs").short('j').value_name("N").help("Number of parallel jobs"))
                    .arg(Arg::new("features").long("features").value_delimiter(',').num_args(1..).value_name("FEATURES").help("Activate package features"))
                    .arg(Arg::new("all-features").long("all-features").action(ArgAction::SetTrue).help("Activate all package features"))
                    .arg(Arg::new("no-default-features").long("no-default-features").action(ArgAction::SetTrue).help("Do not activate the default feature"))
                    .arg(Arg::new("locked").long("locked").action(ArgAction::SetTrue).help("Require the existing Cell.lock dependency graph"))
                    .arg(Arg::new("frozen").long("frozen").action(ArgAction::SetTrue).help("Require Cell.lock and cached sources without network or lockfile writes"))
                    .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Do not access the network while materializing locked sources"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").help("Select an explicitly declared CKB dependency environment"))

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
                        Arg::new("backend")
                            .long("backend")
                            .value_name("BACKEND")
                            .value_parser(["simulator", "ckb-vm", "all"])
                            .help("Execution backend; required unless --no-run: simulator, ckb-vm, or all"),
                    )
                    .arg(
                        Arg::new("no-run")
                            .long("no-run")
                            .action(ArgAction::SetTrue)
                            .help("Compile tests without attempting execution"),
                    )
                    .arg(Arg::new("nocapture").long("nocapture").action(ArgAction::SetTrue).help("Don't capture stdout"))
                    .arg(Arg::new("fail-fast").long("fail-fast").action(ArgAction::SetTrue).help("Stop on first failure"))
                    .arg(Arg::new("doc").long("doc").action(ArgAction::SetTrue).help("Generate docs before compiling tests"))
                    .arg(Arg::new("features").long("features").value_delimiter(',').num_args(1..).value_name("FEATURES").help("Activate package features"))
                    .arg(Arg::new("all-features").long("all-features").action(ArgAction::SetTrue).help("Activate all package features"))
                    .arg(Arg::new("no-default-features").long("no-default-features").action(ArgAction::SetTrue).help("Do not activate the default feature"))
                    .arg(Arg::new("locked").long("locked").action(ArgAction::SetTrue).help("Require the existing Cell.lock dependency graph"))
                    .arg(Arg::new("frozen").long("frozen").action(ArgAction::SetTrue).help("Require cached locked sources without network or lockfile writes"))
                    .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Do not access the network while materializing locked sources"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").help("Select an explicitly declared CKB dependency environment"))
                    ,
            )
            .subcommand(
                ClapCommand::new("doc")
                    .about("Generate documentation")
                    .arg(Arg::new("open").long("open").short('o').action(ArgAction::SetTrue).help("Open docs in browser"))

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
                    .display_order(130)
                    .about("Format source code")
                    .arg(Arg::new("check").long("check").action(ArgAction::SetTrue).help("Check formatting without modifying files"))

                    .arg(Arg::new("files").value_name("FILES").num_args(1..).help("Files to format")),
            )
            .subcommand(
                ClapCommand::new("init")
                    .display_order(140)
                    .about("Create a new package")
                    .arg(Arg::new("name").value_name("NAME").help("Package name"))
                    .arg(Arg::new("path").value_name("PATH").help("Path to create package"))
                    .arg(Arg::new("lib").long("lib").action(ArgAction::SetTrue).help("Create a library package"))
                    .arg(Arg::new("namespace").long("namespace").value_name("NAMESPACE").help("Package namespace for registry publishing"))
                    ,
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
                    ,
            )
            .subcommand(
                ClapCommand::new("add")
                    .about("Add dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to add"))
                    .arg(Arg::new("package-name").long("package").value_name("NAME").help("Declared package name when it differs from the local alias"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Add as dev dependency"))
                    .arg(Arg::new("build").long("build").action(ArgAction::SetTrue).help("Add as build dependency"))
                    .arg(Arg::new("git").long("git").value_name("URL").help("Add a git dependency source"))
                    .arg(Arg::new("path").long("path").value_name("PATH").help("Add a local path dependency source"))
                    .arg(
                        Arg::new("use-environment")
                            .long("use-environment")
                            .value_name("NAME")
                            .conflicts_with("environment-independent")
                            .help("Map this edge to an exact dependency-local environment with the root chain identity"),
                    )
                    .arg(
                        Arg::new("environment-independent")
                            .long("environment-independent")
                            .action(ArgAction::SetTrue)
                            .conflicts_with("use-environment")
                            .help("Do not apply dependency-local environment overrides on this edge"),
                    )
                    ,
            )
            .subcommand(
                ClapCommand::new("clean")
                    .about("Remove build artifacts")

                    .arg(
                        Arg::new("cache")
                            .long("cache")
                            .action(ArgAction::SetTrue)
                            .help("Also remove incremental compilation caches below the current workspace"),
                    ),
            )
            .subcommand(
                ClapCommand::new("remove")
                    .about("Remove dependencies")
                    .arg(Arg::new("crates").value_name("CRATES").required(true).num_args(1..).help("Crates to remove"))
                    .arg(Arg::new("dev").long("dev").action(ArgAction::SetTrue).help("Remove from dev dependency section"))
                    .arg(Arg::new("build").long("build").action(ArgAction::SetTrue).help("Remove from build dependency section"))
                    ,
            )
            .subcommand(ClapCommand::new("repl").about("Start interactive REPL"))
            .subcommand(
                ClapCommand::new("check")
                    .display_order(20)
                    .about("Type-check and lower the current package without writing artifacts")
                    .arg(
                        Arg::new("artifact")
                            .long("artifact")
                            .value_name("NAME")
                            .help("Check the named artifact declared in Cell.toml"),
                    )
                    .arg(
                        Arg::new("all-targets")
                            .long("all-targets")
                            .action(ArgAction::SetTrue)
                            .help("Also check the current ELF-compatible target path"),
                    )
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    .arg(Arg::new("features").long("features").value_delimiter(',').num_args(1..).value_name("FEATURES").help("Activate package features"))
                    .arg(Arg::new("all-features").long("all-features").action(ArgAction::SetTrue).help("Activate all package features"))
                    .arg(Arg::new("no-default-features").long("no-default-features").action(ArgAction::SetTrue).help("Do not activate the default feature"))
                    .arg(Arg::new("locked").long("locked").action(ArgAction::SetTrue).help("Require the existing Cell.lock dependency graph"))
                    .arg(Arg::new("frozen").long("frozen").action(ArgAction::SetTrue).help("Require cached locked sources without network or lockfile writes"))
                    .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Do not access the network while materializing locked sources"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").help("Select an explicitly declared CKB dependency environment"))

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
                    .display_order(30)
                    .about("Emit compile metadata for lowering, scheduler, and CKB runtime auditing")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("artifact").long("artifact").value_name("NAME").help("Inspect the named artifact without generating machine code"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON metadata to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash")),
            )
            .subcommand(
                ClapCommand::new("resolve-graph")
                    .display_order(31)
                    .about("Inspect the versioned read-only package or workspace resolve graph")
                    .arg(Arg::new("input").value_name("INPUT").help("Package, workspace, Cell.toml, or source path"))
                    .arg(Arg::new("package").long("package").short('p').value_name("NAME").help("Select one workspace member and its transitive closure"))
                    .arg(Arg::new("scope").long("scope").value_name("SCOPE").default_value("runtime").value_parser(["runtime", "test"]).help("Dependency scope"))
                    .arg(Arg::new("features").long("features").value_delimiter(',').num_args(1..).value_name("FEATURES").help("Activate package features"))
                    .arg(Arg::new("all-features").long("all-features").action(ArgAction::SetTrue).help("Activate all package features"))
                    .arg(Arg::new("no-default-features").long("no-default-features").action(ArgAction::SetTrue).help("Do not activate the default feature"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").help("Select an explicitly declared CKB dependency environment"))
                    .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Declare that the query must use only local locked sources; inspection is always read-only and effectively offline"))
                    .arg(Arg::new("schema-version").long("schema-version").value_name("N").default_value("1").value_parser(clap::value_parser!(u32)).help("Requested inspection JSON schema version"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the resolve graph JSON to a file")),
            )
            .subcommand(
                ClapCommand::new("build-plan")
                    .display_order(32)
                    .about("Inspect versioned build units without compiling or changing locks")
                    .arg(Arg::new("source-contracts").long("source-contracts").action(ArgAction::SetTrue).help("Plan a build with bounded source declaration evidence"))
                    .arg(Arg::new("input").value_name("INPUT").help("Package, workspace, Cell.toml, or source path"))
                    .arg(Arg::new("package").long("package").short('p').value_name("NAME").help("Select one workspace member and its transitive closure"))
                    .arg(Arg::new("scope").long("scope").value_name("SCOPE").default_value("runtime").value_parser(["runtime", "test"]).help("Dependency scope"))
                    .arg(Arg::new("features").long("features").value_delimiter(',').num_args(1..).value_name("FEATURES").help("Activate package features"))
                    .arg(Arg::new("all-features").long("all-features").action(ArgAction::SetTrue).help("Activate all package features"))
                    .arg(Arg::new("no-default-features").long("no-default-features").action(ArgAction::SetTrue).help("Do not activate the default feature"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").help("Select an explicitly declared CKB dependency environment"))
                    .arg(Arg::new("offline").long("offline").action(ArgAction::SetTrue).help("Declare that the query must use only local locked sources; inspection is always read-only and effectively offline"))
                    .arg(Arg::new("schema-version").long("schema-version").value_name("N").default_value("2").value_parser(clap::value_parser!(u32)).help("Requested inspection JSON schema version"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    .arg(Arg::new("release").long("release").short('r').action(ArgAction::SetTrue).help("Plan release output with the normal compilation strategy"))
                    .arg(Arg::new("debug").long("debug").action(ArgAction::SetTrue).help("Plan debug metadata"))
                    .arg(Arg::new("entry-action").long("entry-action").value_name("ACTION").help("Plan one action entrypoint"))
                    .arg(Arg::new("entry-lock").long("entry-lock").value_name("LOCK").conflicts_with("entry-action").help("Plan one lock entrypoint"))
                    .arg(Arg::new("artifact").long("artifact").value_name("NAME").conflicts_with_all(["entry-action", "entry-lock"]).help("Plan one named artifact"))
                    .arg(Arg::new("primitive-compat").long("primitive-compat").value_name("VERSION").conflicts_with("primitive-strict").help("Select an older primitive compatibility profile"))
                    .arg(Arg::new("primitive-strict").long("primitive-strict").value_name("VERSION").conflicts_with("primitive-compat").help("Select a strict primitive compatibility profile"))
                    .arg(Arg::new("production").long("production").action(ArgAction::SetTrue).help("Include the production policy requirement"))
                    .arg(Arg::new("deny-fail-closed").long("deny-fail-closed").action(ArgAction::SetTrue).help("Include the fail-closed denial requirement"))
                    .arg(Arg::new("deny-ckb-runtime").long("deny-ckb-runtime").action(ArgAction::SetTrue).help("Include the CKB-runtime denial requirement"))
                    .arg(Arg::new("deny-runtime-obligations").long("deny-runtime-obligations").action(ArgAction::SetTrue).help("Include the runtime-obligation denial requirement"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the build plan JSON to a file")),
            )
            .subcommand(
                ClapCommand::new("expand")
                    .display_order(31)
                    .about("Render the canonical typed semantic foundation; the rendering is not a hash boundary")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("artifact").long("artifact").value_name("NAME").help("Expand the named artifact without generating machine code"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the expansion to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash")),
            )
            .subcommand(
                ClapCommand::new("interface")
                    .display_order(32)
                    .about("Show the public package interface and its canonical CKB hash")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the complete canonical interface JSON to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash")),
            )
            .subcommand(
                ClapCommand::new("interface-diff")
                    .display_order(32)
                    .about("Compare source API, layout, ABI, effects, builders, and deployment contracts")
                    .arg(Arg::new("checked").long("checked").action(ArgAction::SetTrue).help("Compare independently checked nominal contracts; --old and --new must name ELF bundles with their sidecars"))
                    .arg(Arg::new("old").long("old").value_name("INPUT_OR_JSON").required(true).help("Previous source/package or interface JSON"))
                    .arg(Arg::new("new").long("new").value_name("INPUT_OR_JSON").required(true).help("Candidate source/package or interface JSON"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the compatibility report to a file")),
            )
            .subcommand(
                ClapCommand::new("schema-ack")
                    .display_order(33)
                    .about("Plan, acknowledge, or verify one focused `same except` schema migration")
                    .arg(Arg::new("upgrade-plan").long("upgrade-plan").value_name("FILE").conflicts_with_all(["old", "new"]).requires("subject").help("Review one exact schema change recorded in a transactional upgrade plan"))
                    .arg(Arg::new("subject").long("subject").value_name("MEMBER_OR_DEPENDENCY_ID").requires("upgrade-plan").help("Root member name or exact dependency review ID selected in the upgrade plan"))
                    .arg(Arg::new("environment").long("environment").value_name("NAME").requires("upgrade-plan").help("Exact dependency environment selected in the upgrade plan"))
                    .arg(Arg::new("module").long("module").value_name("MODULE").requires("upgrade-plan").help("Select an exact retired or fresh module endpoint when source namespaces differ"))
                    .arg(Arg::new("old").long("old").value_name("PACKAGE").required_unless_present("upgrade-plan").help("Previous Edition 2027 package or source"))
                    .arg(Arg::new("new").long("new").value_name("PACKAGE").required_unless_present("upgrade-plan").help("Candidate Edition 2027 package or source"))
                    .arg(Arg::new("action").long("action").value_name("ACTION").required(true).help("Action containing the relation"))
                    .arg(Arg::new("before").long("before").value_name("ROLE").required(true).help("Relation predecessor role"))
                    .arg(Arg::new("after").long("after").value_name("ROLE").required(true).help("Relation successor role"))
                    .arg(Arg::new("acknowledge-by").long("acknowledge-by").value_name("REVIEWER").help("Create a receipt naming the focused reviewer"))
                    .arg(Arg::new("rationale").long("rationale").value_name("TEXT").help("Create a receipt with the review rationale"))
                    .arg(Arg::new("verify").long("verify").value_name("RECEIPT").help("Verify a receipt against the current old/new candidates"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write the plan, receipt, or verification result")),
            )
            .subcommand(
                ClapCommand::new("constraints")
                    .about("Emit profile-aware production constraints for compiler, builder, CI, and acceptance gates")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON constraints to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
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
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    .arg(Arg::new("action").long("action").value_name("NAME").help("Explain ABI for this action"))
                    .arg(Arg::new("lock").long("lock").value_name("NAME").help("Explain ABI for this lock")),
            )
            .subcommand(
                ClapCommand::new("scheduler-plan")
                    .about("Consume scheduler hints and emit a CKB admission/conflict policy report")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON scheduler plan to a file"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash")),
            )
            .subcommand(
                ClapCommand::new("ckb-hash")
                    .about("Compute CKB default Blake2b-256 hashes for builders, manifests, and release evidence")
                    .arg(Arg::new("input").value_name("TEXT").help("UTF-8 text to hash; omitted input hashes empty bytes"))
                    .arg(Arg::new("hex").long("hex").value_name("HEX").help("Hex bytes to hash"))
                    .arg(Arg::new("file").long("file").value_name("FILE").help("File bytes to hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("ckb-std-compat")
                    .about("Report the ckb-std ABI compatibility boundary for CellScript's inline CKB backend")
                    ,
            )
            .subcommand(
                ClapCommand::new("explain")
                    .about("Explain runtime errors, target profiles, ProofPlan records, assumptions, generics, collection budgets, and graph views")
                    .arg_required_else_help(true)
                    .arg(Arg::new("code").value_name("CODE").help("Runtime error code, E-code, or error name"))

                    .subcommand(
                        ClapCommand::new("profile")
                            .about("Explain a CellScript target profile semantic contract")
                            .arg(Arg::new("profile").value_name("PROFILE").required(true).help("Target profile name, e.g. ckb"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("proof")
                            .about("Explain Covenant ProofPlan trigger, scope, reads, coverage, and on-chain status")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("assumptions")
                            .about("Explain v0.16 builder assumptions derived from ProofPlan metadata")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("generics")
                            .about("Explain value-generic monomorphizations and bounded collection instantiations")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("collections")
                            .about("Explain named collection preallocation and cumulative growth budgets")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml")),
                    )
                    .subcommand(
                        ClapCommand::new("graph")
                            .about("Derive a cyclic ProtocolGraph audit view from compile metadata")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            .arg(
                                Arg::new("format")
                                    .long("format")
                                    .value_name("FORMAT")
                                    .value_parser(["json", "mermaid"])
                                    .help("ProtocolGraph output format"),
                            )
                            ,
                    ),
            )
            .subcommand(
                ClapCommand::new("explain-profile")
                    .hide(true)
                    .about("Explain a CellScript target profile semantic contract")
                    .arg(Arg::new("profile").value_name("PROFILE").required(true).help("Target profile name, e.g. ckb"))
                    ,
            )
            .subcommand(
                ClapCommand::new("explain-proof")
                    .hide(true)
                    .about("Explain Covenant ProofPlan trigger, scope, reads, coverage, and on-chain status")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("explain-assumptions")
                    .hide(true)
                    .about("Explain v0.16 builder assumptions derived from ProofPlan metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("explain-generics")
                    .hide(true)
                    .about("Explain value-generic monomorphizations and bounded collection instantiations")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("explain-graph")
                    .hide(true)
                    .about("Derive a cyclic ProtocolGraph audit view from compile metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    .arg(
                        Arg::new("format")
                            .long("format")
                            .value_name("FORMAT")
                            .value_parser(["json", "mermaid"])
                            .help("ProtocolGraph output format"),
                    )
                    ,
            )
            .subcommand(
                ClapCommand::new("proof-diff")
                    .about("Diff ProofPlan semantics between two metadata files")
                    .arg(Arg::new("old").value_name("OLD_METADATA").required(true))
                    .arg(Arg::new("new").value_name("NEW_METADATA").required(true))
                    ,
            )
            .subcommand(
                ClapCommand::new("profile")
                    .about("Emit v0.16 cycle/profile summary per action, lock, and ProofPlan record")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("entry").long("entry").value_name("NAME").help("Limit profile to one action or lock"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("tx")
                    .display_order(70)
                    .about("Validate, solve, and trace transaction evidence")
                    .subcommand_required(true)
                    .arg_required_else_help(true)
                    .subcommand(
                        ClapCommand::new("validate")
                            .about("Validate a transaction JSON against v0.16 builder assumptions before signing")
                            .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                            .arg(Arg::new("tx").long("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("solve")
                            .about("Emit a deterministic v0.16 transaction template from metadata and builder assumptions")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON solver template"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("trace")
                            .about("Trace a transaction JSON against v0.16 builder assumptions")
                            .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                            .arg(Arg::new("tx").long("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                            ,
                    ),
            )
            .subcommand(
                ClapCommand::new("trace-tx")
                    .hide(true)
                    .about("Trace a transaction JSON against v0.16 builder assumptions")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    ,
            )
            .subcommand(
                ClapCommand::new("audit-bundle")
                    .about("Generate a v0.16 audit bundle linking metadata, ProofPlan, assumptions, and profile data")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("DIR").help("Output directory"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("validate-tx")
                    .hide(true)
                    .about("Validate a transaction JSON against v0.16 builder assumptions before signing")
                    .arg(Arg::new("against").long("against").value_name("METADATA").required(true).help("Metadata JSON"))
                    .arg(Arg::new("tx").value_name("TX_JSON").required(true).help("Transaction JSON"))
                    ,
            )
            .subcommand(
                ClapCommand::new("solve-tx")
                    .hide(true)
                    .about("Emit a deterministic v0.16 transaction template from metadata and builder assumptions")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON solver template"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("verify-ckb-fixtures")
                    .about("Verify standard CKB compatibility fixtures with the deterministic model runner")
                    .arg(Arg::new("manifest").value_name("MANIFEST_JSON").required(true).help("CKB fixture manifest JSON"))
                    ,
            )
            .subcommand(
                ClapCommand::new("deploy")
                    .display_order(80)
                    .about("Plan, verify, diff, and lock deployment evidence")
                    .subcommand_required(true)
                    .arg_required_else_help(true)
                    .subcommand(
                        ClapCommand::new("plan")
                            .about("Emit a reproducible v0.16 deployment plan")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON deploy plan"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("verify")
                            .about("Verify a v0.16 deployment plan schema and local integrity fields")
                            .arg(Arg::new("plan").long("plan").value_name("DEPLOY_PLAN").required(true).help("Deployment plan JSON"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("diff")
                            .about("Diff two v0.16 deployment plans")
                            .arg(Arg::new("old").long("old").value_name("OLD_DEPLOY_PLAN").required(true).help("Previous deployment plan JSON"))
                            .arg(Arg::new("new").long("new").value_name("NEW_DEPLOY_PLAN").required(true).help("New deployment plan JSON"))
                            ,
                    )
                    .subcommand(
                        ClapCommand::new("lock-deps")
                            .about("Emit a v0.16 dependency lock from deployment metadata")
                            .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write dependency lock JSON"))
                            .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                            .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                            ,
                    ),
            )
            .subcommand(
                ClapCommand::new("deploy-plan")
                    .hide(true)
                    .about("Emit a reproducible v0.16 deployment plan")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON deploy plan"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("verify-deploy")
                    .hide(true)
                    .about("Verify a v0.16 deployment plan schema and local integrity fields")
                    .arg(Arg::new("plan").value_name("DEPLOY_PLAN").required(true))
                    ,
            )
            .subcommand(
                ClapCommand::new("diff-deploy")
                    .hide(true)
                    .about("Diff two v0.16 deployment plans")
                    .arg(Arg::new("old").value_name("OLD_DEPLOY_PLAN").required(true))
                    .arg(Arg::new("new").value_name("NEW_DEPLOY_PLAN").required(true))
                    ,
            )
            .subcommand(
                ClapCommand::new("lock-deps")
                    .hide(true)
                    .about("Emit a v0.16 dependency lock from deployment metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write dependency lock JSON"))
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("action")
                    .display_order(50)
                    .about("Plan and explain action-level transaction builder inputs")
                    .subcommand(
                    ClapCommand::new("build")
                        .about("Emit a builder plan for a CellScript action without signing or submitting a transaction")
                        .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                        .arg(Arg::new("action").long("action").value_name("NAME").help("Action to plan; defaults to the first action"))
                        .arg(Arg::new("output").long("output").short('o').value_name("FILE").help("Write JSON builder plan to a file"))
                        .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                        .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
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
                    .display_order(60)
                    .about("Generate a registry-bound action builder package from CellScript metadata")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("artifact").long("artifact").value_name("NAME").help("Select a named source artifact or match a policy declaration in metadata"))
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
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile when compiling INPUT: ckb or ckb-type-hash"))
                    .arg(Arg::new("package-name").long("package-name").value_name("NAME").help("Generated package.json name"))
                    ,
            )
            .subcommand(
                ClapCommand::new("entry-witness")
                    .about("Encode witness bytes for the generated _cellscript_entry wrapper")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(Arg::new("artifact").long("artifact").value_name("NAME").conflicts_with("lock").help("Select a named artifact; tagged policy requests also require action and full Script hash"))
                    .arg(Arg::new("script-hash").long("script-hash").value_name("HEX").requires("artifact").help("Full deployed 32-byte Script hash; caller-supplied routing identity, not authentication"))
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
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("receipt")
                    .display_order(40)
                    .about("Emit an authenticated compile-receipt envelope over metadata and artifact hashes")
                    .arg(Arg::new("input").value_name("INPUT").help("Input .cell file, package directory, or Cell.toml"))
                    .arg(
                        Arg::new("output")
                            .long("output")
                            .short('o')
                            .value_name("FILE")
                            .required(true)
                            .help("Write the compile receipt JSON to this file"),
                    )
                    .arg(Arg::new("target").long("target").short('t').value_name("TARGET").help("Target architecture"))
                    .arg(Arg::new("target-profile").long("target-profile").value_name("PROFILE").help("Target profile: ckb or ckb-type-hash"))
                    ,
            )
            .subcommand(
                ClapCommand::new("sign-receipt")
                    .display_order(41)
                    .about("Append an Ed25519 signature to a compile receipt")
                    .arg(Arg::new("receipt").value_name("RECEIPT").required(true).help("Compile receipt JSON to sign"))
                    .arg(
                        Arg::new("role")
                            .long("role")
                            .value_name("ROLE")
                            .required(true)
                            .help("Signature role to record: compiler or publisher"),
                    )
                    .arg(
                        Arg::new("key")
                            .long("key")
                            .value_name("KEY")
                            .required(true)
                            .help("Ed25519 PKCS#8 key as a file path, @file, or base64 DER"),
                    )
                    .arg(
                        Arg::new("output")
                            .long("output")
                            .short('o')
                            .value_name("FILE")
                            .help("Write the signed receipt here; defaults to updating RECEIPT in place"),
                    )
                    ,
            )
            .subcommand(
                ClapCommand::new("verify-receipt")
                    .display_order(42)
                    .about("Verify a compile receipt against metadata, artifact bytes, and Ed25519 signatures")
                    .arg(Arg::new("receipt").value_name("RECEIPT").required(true).help("Compile receipt JSON to verify"))
                    .arg(
                        Arg::new("metadata")
                            .long("metadata")
                            .short('m')
                            .value_name("FILE")
                            .required(true)
                            .help("Metadata JSON sidecar to bind to the receipt"),
                    )
                    .arg(
                        Arg::new("artifact")
                            .long("artifact")
                            .short('a')
                            .value_name("FILE")
                            .required(true)
                            .help("Artifact bytes to bind to the receipt"),
                    )
                    ,
            )
            .subcommand(
                ClapCommand::new("protocol")
                    .display_order(42)
                    .about("Compose and inspect independently checked multi-Script protocols")
                    .arg_required_else_help(true)
                    .subcommand(
                        ClapCommand::new("bundle")
                            .about("Compose independently checked artifacts into one transaction skeleton")
                            .arg_required_else_help(true)
                            .subcommand(
                                ClapCommand::new("check")
                                    .about("Verify artifact identities and reject transaction-role conflicts offline")
                                    .arg(
                                        Arg::new("manifest")
                                            .value_name("BUNDLE_JSON")
                                            .required(true)
                                            .help("cellscript-protocol-bundle-input-v1 manifest with relative checked-artifact paths"),
                                    )
                                    .arg(
                                        Arg::new("open-admission")
                                            .long("open-admission")
                                            .value_name("OPEN_JSON")
                                            .help("Recompute frozen source policies and bind the complete open-participant transaction"),
                                    )
                                    .arg(
                                        Arg::new("output")
                                            .long("output")
                                            .short('o')
                                            .value_name("REPORT_JSON")
                                            .help("Write the resolved bundle, conflicts, and evidence template"),
                                    ),
                            ),
                    ),
            )
            .subcommand(
                ClapCommand::new("verify-artifact")
                    .display_order(43)
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
                        Arg::new("receipt")
                            .long("receipt")
                            .value_name("FILE")
                            .help("Also verify a compile receipt against the artifact and metadata"),
                    )
                    .arg(
                        Arg::new("lowering-record")
                            .long("lowering-record")
                            .value_name("FILE")
                            .help("Canonical lowering record; defaults to ARTIFACT.lowering.json for ELF"),
                    )
                    .arg(
                        Arg::new("source-map")
                            .long("source-map")
                            .value_name("FILE")
                            .help("Canonical source map; defaults to ARTIFACT.sourcemap.json for ELF"),
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
                            .help("Require metadata target_profile to match this value: ckb or ckb-type-hash"),
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
                ClapCommand::new("artifact")
                    .display_order(105)
                    .about("Fetch, verify, pin, copy, and consume non-CellScript Registry artifacts")
                    .subcommand_required(true)
                    .arg_required_else_help(true)
                    .subcommand(
                        ClapCommand::new("ls-idl")
                            .about("Validate, bind, or fetch an exact-byte LS-IDL lock-script interface")
                            .subcommand_required(true)
                            .arg_required_else_help(true)
                            .subcommand(
                                ClapCommand::new("validate")
                                    .about("Validate an LS-IDL 0.1 document and optionally verify its executable suffix")
                                    .arg(Arg::new("idl").long("idl").value_name("FILE").required(true))
                                    .arg(Arg::new("executable").long("executable").value_name("CKB_ELF")),
                            )
                            .subcommand(
                                ClapCommand::new("bind")
                                    .about("Append SHA-256(raw idl.json bytes) to a CKB executable")
                                    .arg(Arg::new("idl").long("idl").value_name("FILE").required(true))
                                    .arg(Arg::new("executable").long("executable").value_name("CKB_ELF").required(true))
                                    .arg(Arg::new("output").long("output").short('o').value_name("CKB_ELF").required(true))
                                    .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                            )
                            .subcommand(
                                ClapCommand::new("fetch")
                                    .about("Fetch exact LS-IDL bytes by a chain-verified CKB script identity")
                                    .arg(Arg::new("code-hash").long("code-hash").value_name("HASH").required(true))
                                    .arg(
                                        Arg::new("hash-type")
                                            .long("hash-type")
                                            .value_name("TYPE")
                                            .value_parser(["data", "data1", "data2", "type"]),
                                    )
                                    .arg(Arg::new("data-hash").long("data-hash").value_name("HASH"))
                                    .arg(
                                        Arg::new("network")
                                            .long("network")
                                            .value_name("NETWORK")
                                            .value_parser(["mainnet", "testnet"])
                                            .default_value("mainnet"),
                                    )
                                    .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                                    .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                                    .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                            )
                            .subcommand(
                                ClapCommand::new("bundle")
                                    .about("Create a publish-ready LS-IDL Registry bundle and Artifact.toml")
                                    .arg(Arg::new("idl").long("idl").value_name("FILE").required(true))
                                    .arg(Arg::new("executable").long("executable").value_name("CKB_ELF").required(true))
                                    .arg(Arg::new("source").long("source").value_name("FILE").required(true))
                                    .arg(Arg::new("namespace").long("namespace").value_name("NAME").required(true))
                                    .arg(Arg::new("name").long("name").value_name("NAME").required(true))
                                    .arg(Arg::new("release").long("release").value_name("VERSION").required(true))
                                    .arg(
                                        Arg::new("language")
                                            .long("language")
                                            .value_name("LANGUAGE")
                                            .value_parser(["cellscript", "rust", "c", "javascript", "other"])
                                            .required(true),
                                    )
                                    .arg(
                                        Arg::new("hash-type")
                                            .long("hash-type")
                                            .value_name("TYPE")
                                            .value_parser(["data", "data1", "data2", "type"])
                                            .default_value("data1"),
                                    )
                                    .arg(
                                        Arg::new("dep-type")
                                            .long("dep-type")
                                            .value_name("TYPE")
                                            .value_parser(["code", "dep_group"])
                                            .default_value("code"),
                                    )
                                    .arg(Arg::new("toolchain").long("toolchain").value_name("IDENTITY").required(true))
                                    .arg(
                                        Arg::new("source-revision")
                                            .long("source-revision")
                                            .value_name("REVISION")
                                            .required(true),
                                    )
                                    .arg(
                                        Arg::new("output")
                                            .long("output")
                                            .short('o')
                                            .value_name("BUNDLE_JSON")
                                            .default_value("bundle.json"),
                                    )
                                    .arg(
                                        Arg::new("artifact-manifest-output")
                                            .long("artifact-manifest-output")
                                            .value_name("ARTIFACT_TOML")
                                            .default_value("Artifact.toml"),
                                    )
                                    .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                            ),
                    )
                    .subcommand(
                        ClapCommand::new("fetch")
                            .about("Download an immutable artifact bundle and write an authenticated receipt")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                            .arg(Arg::new("receipt").long("receipt").value_name("FILE"))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("verify")
                            .about("Verify a downloaded bundle against its signed Registry receipt")
                            .arg(Arg::new("bundle").long("bundle").value_name("FILE").required(true))
                            .arg(Arg::new("receipt").long("receipt").value_name("FILE").required(true)),
                    )
                    .subcommand(
                        ClapCommand::new("pin")
                            .about("Write a deterministic TCB/deployment artifact lock")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").default_value("Artifacts.lock"))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(
                                Arg::new("accept-hash-bound")
                                    .long("accept-hash-bound")
                                    .action(ArgAction::SetTrue)
                                    .help("Explicitly pin immutable bytes that have integrity evidence but no semantic/security certification"),
                            )
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("copy")
                            .about("Materialize an authenticated template file map without overwriting files")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("destination").long("destination").short('d').value_name("DIR").default_value("."))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("accept-hash-bound").long("accept-hash-bound").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("cell-dep")
                            .visible_alias("celldep")
                            .about("Generate a transaction-builder CellDep descriptor from chain-verified deployment evidence")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(
                                Arg::new("rpc-url")
                                    .long("rpc-url")
                                    .value_name("URL")
                                    .help("Mainnet CKB RPC used to re-check Cell liveness (defaults to CELLSCRIPT_CKB_RPC_URL or mainnet.ckb.dev)"),
                            )
                            .arg(
                                Arg::new("accept-hash-bound")
                                    .long("accept-hash-bound")
                                    .action(ArgAction::SetTrue)
                                    .help("Explicitly consume chain-verified deployment bytes that have integrity evidence but no semantic/security certification"),
                            )
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("record-deployment")
                            .about("Sign, submit, and RPC-verify a CKB deployment record")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(
                                Arg::new("network")
                                    .long("network")
                                    .value_name("NETWORK")
                                    .value_parser(["mainnet", "testnet"])
                                    .default_value("mainnet")
                                    .help("CKB network whose live Cell will be verified; testnet defaults to the isolated Pudge Registry API"),
                            )
                            .arg(Arg::new("code-hash").long("code-hash").value_name("HASH").required(true))
                            .arg(
                                Arg::new("hash-type")
                                    .long("hash-type")
                                    .value_name("TYPE")
                                    .value_parser(["data", "data1", "data2", "type"])
                                    .required(true),
                            )
                            .arg(
                                Arg::new("dep-type")
                                    .long("dep-type")
                                    .value_name("TYPE")
                                    .value_parser(["code", "dep_group"])
                                    .required(true),
                            )
                            .arg(Arg::new("tx-hash").long("tx-hash").value_name("HASH").required(true))
                            .arg(Arg::new("index").long("index").value_name("U32").value_parser(clap::value_parser!(u32)).required(true))
                            .arg(Arg::new("capability-key-id").long("capability-key-id").value_name("KEY_ID").required(true))
                            .arg(Arg::new("capability-signature").long("capability-signature").value_name("SIGNATURE"))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("print-payload").long("print-payload").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("set-availability")
                            .about("Set a release active, deprecated, or yanked with a publisher capability")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(
                                Arg::new("status")
                                    .long("status")
                                    .value_name("STATUS")
                                    .value_parser(["active", "deprecated", "yanked"])
                                    .required(true),
                            )
                            .arg(Arg::new("reason").long("reason").value_name("TEXT"))
                            .arg(Arg::new("capability-key-id").long("capability-key-id").value_name("KEY_ID").required(true))
                            .arg(Arg::new("capability-signature").long("capability-signature").value_name("SIGNATURE"))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("print-payload").long("print-payload").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("reproduction-report")
                            .about("Hash a clean reproduction and sign a builder-authenticated report")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("artifact").long("artifact").value_name("FILE").required(true))
                            .arg(Arg::new("build-log").long("build-log").value_name("FILE").required(true))
                            .arg(Arg::new("builder-id").long("builder-id").value_name("ID").required(true))
                            .arg(Arg::new("trust-domain").long("trust-domain").value_name("DOMAIN").required(true))
                            .arg(Arg::new("builder-key-id").long("builder-key-id").value_name("KEY_ID").required(true))
                            .arg(
                                Arg::new("builder-public-key")
                                    .long("builder-public-key")
                                    .value_name("P256_SPKI")
                                    .required(true),
                            )
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("reproduction-evidence")
                            .about("Validate signed independent reproduction reports and generate an admin promotion request")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(
                                Arg::new("report")
                                    .long("report")
                                    .value_name("FILE")
                                    .action(ArgAction::Append)
                                    .required(true)
                                    .help("Signed independent cellscript-reproduction-report-v2 JSON; pass once per trusted builder"),
                            )
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    )
                    .subcommand(
                        ClapCommand::new("commitment")
                            .about("Generate the canonical mainnet Registry commitment payload and Cell data")
                            .arg(Arg::new("coordinate").value_name("NAMESPACE/NAME@RELEASE").required(true))
                            .arg(Arg::new("output").long("output").short('o').value_name("FILE").required(true))
                            .arg(Arg::new("api-url").long("api-url").value_name("URL"))
                            .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
                    ),
            )
            .subcommand(
                ClapCommand::new("publish")
                    .display_order(110)
                    .about("Publish a package to the public registry, or write an offline fixture with --offline")
                    .arg(
                        Arg::new("dry-run")
                            .long("dry-run")
                            .action(ArgAction::SetTrue)
                            .help("Validate the publish request without submitting it"),
                    )
                    .arg(
                        Arg::new("offline")
                            .long("offline")
                            .action(ArgAction::SetTrue)
                            .help("Write local registry.json fixture metadata instead of using the public write API"),
                    )
                    .arg(
                        Arg::new("allow-dirty")
                            .long("allow-dirty")
                            .action(ArgAction::SetTrue)
                            .help("Allow publishing from a working tree with uncommitted changes"),
                    )
                    .arg(
                        Arg::new("api-url")
                            .long("api-url")
                            .value_name("URL")
                            .help("Registry write API base URL; defaults to CELLSCRIPT_REGISTRY_API_URL or api.registry.cellscript.dev"),
                    )
                    .arg(
                        Arg::new("capability-key-id")
                            .long("capability-key-id")
                            .value_name("KEY_ID")
                            .help("Registry capability key id authorised by a root wallet"),
                    )
                    .arg(
                        Arg::new("authorise")
                            .long("authorise")
                            .action(ArgAction::SetTrue)
                            .conflicts_with_all(["capability-key-id", "capability-signature", "payload", "offline", "dry-run", "print-payload"])
                            .help("Create a short-lived browser wallet session, wait for approval, then continue publishing"),
                    )
                    .arg(
                        Arg::new("no-open")
                            .long("no-open")
                            .action(ArgAction::SetTrue)
                            .requires("authorise")
                            .help("Print the browser authorisation URL without opening it automatically"),
                    )
                    .arg(
                        Arg::new("capability-signature")
                            .long("capability-signature")
                            .value_name("SIGNATURE")
                            .help("P-256 signature over the canonical publish payload"),
                    )
                    .arg(
                        Arg::new("idempotency-key")
                            .long("idempotency-key")
                            .value_name("KEY")
                            .help("Publish retry key; defaults to CELLSCRIPT_REGISTRY_IDEMPOTENCY_KEY or a request hash"),
                    )
                    .arg(
                        Arg::new("payload")
                            .long("payload")
                            .value_name("FILE")
                            .help("Previously generated publish payload JSON to submit with a capability signature"),
                    )
                    .arg(
                        Arg::new("source-snapshot")
                            .long("source-snapshot")
                            .value_name("FILE")
                            .help("Immutable source snapshot bytes to upload; defaults to a generated CellScript source snapshot"),
                    )
                    .arg(
                        Arg::new("artifact-manifest")
                            .long("artifact-manifest")
                            .value_name("FILE")
                            .conflicts_with("offline")
                            .help("Publish a non-package artifact described by Artifact.toml and its immutable bundle"),
                    )
                    .arg(
                        Arg::new("artifact-kind")
                            .long("artifact-kind")
                            .value_name("KIND")
                            .value_parser(["source_library", "profile_library"])
                            .conflicts_with("artifact-manifest")
                            .help("CellScript dependency kind; defaults to source_library"),
                    )
                    .arg(
                        Arg::new("print-payload")
                            .long("print-payload")
                            .action(ArgAction::SetTrue)
                            .help("Print the publish payload and canonical signing bytes without submitting"),
                    )
                    ,
            )
            .subcommand(
                ClapCommand::new("install")
                    .display_order(120)
                    .about("Install a package from registry, git, or path")
                    .arg(Arg::new("crate").value_name("CRATE").help("Registry package name, or namespace/name@version"))
                    .arg(Arg::new("version").long("version").value_name("VERSION").help("Package version constraint to install"))
                    .arg(Arg::new("namespace").long("namespace").value_name("NAMESPACE").help("Package namespace for registry install"))
                    .arg(Arg::new("git").long("git").value_name("URL").help("Install from a git repository URL"))
                    .arg(Arg::new("path").long("path").value_name("PATH").help("Install from a local package path"))
                    .arg(
                        Arg::new("allow-unverified")
                            .long("allow-unverified")
                            .action(ArgAction::SetTrue)
                            .help("Allow direct install of source_published or indexed_pending registry entries"),
                    )
                    .arg(
                        Arg::new("allow-quarantined")
                            .long("allow-quarantined")
                            .action(ArgAction::SetTrue)
                            .help("Allow explicit incident-review install of quarantined registry entries"),
                    ),
            )
            .subcommand(ClapCommand::new("lock").about("Resolve and write the complete Cell.lock dependency graph"))
            .subcommand(update_clap_command(
                "update-plan",
                "Resolve, compile, and classify a transactional dependency upgrade without changing Cell.lock",
                false,
            ))
            .subcommand(update_clap_command(
                "update",
                "Plan an upgrade by default, or verify and atomically apply --apply-plan",
                true,
            ))
            .subcommand(
                ClapCommand::new("language-contract").about("Describe this compiler source grammar and limits")
            )
            .subcommand(
                ClapCommand::new("info")
                    .about("Show package information")
                    ,
            )
            .subcommand(
                ClapCommand::new("login")
                    .hide(true)
                    .about("Experimental: authenticate against a registry")
                    .arg(Arg::new("registry").long("registry").value_name("URL")),
            )
            .subcommand(
                ClapCommand::new("auth")
                    .about("Manage wallet-rooted registry capability authorisation")
                    .subcommand_required(true)
                    .arg_required_else_help(true)
                    .subcommand(
                        ClapCommand::new("login")
                            .hide(true)
                            .about("Create a wallet capability authorisation payload")
                            .arg(
                                Arg::new("registry-origin")
                                    .long("registry-origin")
                                    .value_name("URL")
                                    .help("Registry origin bound into the wallet capability challenge"),
                            )
                            .arg(
                                Arg::new("principal-type")
                                    .long("principal-type")
                                    .value_name("TYPE")
                                    .help("Principal type for the identity binding; defaults to joyid_ckb"),
                            )
                            .arg(
                                Arg::new("principal-id")
                                    .long("principal-id")
                                    .value_name("ID")
                                    .help("Normalized principal binding derived from a supported CCC CKB signer"),
                            )
                            .arg(
                                Arg::new("capability-pubkey")
                                    .long("capability-pubkey")
                                    .value_name("PUBKEY")
                                    .help("Capability public key that will authorise scoped registry requests"),
                            )
                            .arg(
                                Arg::new("scope")
                                    .long("scope")
                                    .value_name("SCOPE")
                                    .action(ArgAction::Append)
                                    .help("Repeatable least-privilege scope: publish, deployment, or availability for namespace/package (or namespace/*)"),
                            )
                            .arg(
                                Arg::new("expires")
                                    .long("expires")
                                    .value_name("DURATION")
                                    .conflicts_with("capability-expires-at")
                                    .help("Capability lifetime, e.g. 90d or 24h"),
                            )
                            .arg(
                                Arg::new("capability-expires-at")
                                    .long("capability-expires-at")
                                    .value_name("TIMESTAMP")
                                    .help("Absolute UTC capability expiry timestamp, e.g. 2026-12-31T00:00:00Z"),
                            )
                            .arg(
                                Arg::new("json")
                                    .long("json")
                                    .action(ArgAction::SetTrue)
                                    .help("Emit the capability authorisation payload as JSON"),
                            ),
                    )
                    .subcommand(
                        ClapCommand::new("capability")
                            .about("Manage scoped publisher capabilities")
                            .subcommand_required(true)
                            .arg_required_else_help(true)
                            .subcommand(
                                ClapCommand::new("create")
                                    .about("Create a wallet capability authorisation payload for CI or local publishing")
                                    .arg(
                                        Arg::new("registry-origin")
                                            .long("registry-origin")
                                            .value_name("URL")
                                            .help("Registry origin bound into the wallet capability challenge"),
                                    )
                                    .arg(
                                        Arg::new("principal-type")
                                            .long("principal-type")
                                            .value_name("TYPE")
                                            .help("Principal type for the identity binding; defaults to joyid_ckb"),
                                    )
                                    .arg(
                                        Arg::new("principal-id")
                                            .long("principal-id")
                                            .value_name("ID")
                                            .help("Normalized principal binding derived from a supported CCC CKB signer"),
                                    )
                                    .arg(
                                        Arg::new("capability-pubkey")
                                            .long("capability-pubkey")
                                            .value_name("PUBKEY")
                                            .help("Capability public key that will authorise scoped registry requests"),
                                    )
                                    .arg(
                                        Arg::new("scope")
                                            .long("scope")
                                            .value_name("SCOPE")
                                            .action(ArgAction::Append)
                                            .help("Repeatable least-privilege scope: publish, deployment, or availability for namespace/package (or namespace/*)"),
                                    )
                                    .arg(
                                        Arg::new("expires")
                                            .long("expires")
                                            .value_name("DURATION")
                                            .conflicts_with("capability-expires-at")
                                            .help("Capability lifetime, e.g. 90d or 24h"),
                                    )
                                    .arg(
                                        Arg::new("capability-expires-at")
                                            .long("capability-expires-at")
                                            .value_name("TIMESTAMP")
                                            .help("Absolute UTC capability expiry timestamp, e.g. 2026-12-31T00:00:00Z"),
                                    )
                                    .arg(
                                        Arg::new("json")
                                            .long("json")
                                            .action(ArgAction::SetTrue)
                                            .help("Emit the capability authorisation payload as JSON"),
                                    ),
                            )
                            .subcommand(
                                ClapCommand::new("submit")
                                    .about("Submit a wallet-signed capability authorisation payload to the registry")
                                    .arg(
                                        Arg::new("api-url")
                                            .long("api-url")
                                            .value_name("URL")
                                            .help("Registry write API base URL; defaults to CELLSCRIPT_REGISTRY_API_URL"),
                                    )
                                    .arg(
                                        Arg::new("payload")
                                            .long("payload")
                                            .value_name("FILE")
                                            .required(true)
                                            .help("Capability authorisation payload JSON created by auth capability create"),
                                    )
                                    .arg(
                                        Arg::new("wallet-signature")
                                            .long("wallet-signature")
                                            .visible_alias("joyid-signature")
                                            .value_name("FILE")
                                            .required(true)
                                            .help("JoyID or CKB wallet signature JSON whose challenge is the canonical payload"),
                                    )
                                    .arg(
                                        Arg::new("json")
                                            .long("json")
                                            .action(ArgAction::SetTrue)
                                            .help("Emit machine-readable capability submission output"),
                                    ),
                            )
                            .subcommand(
                                ClapCommand::new("revoke")
                                    .about("Create or submit a wallet-signed capability revocation payload")
                                    .arg(
                                        Arg::new("api-url")
                                            .long("api-url")
                                            .value_name("URL")
                                            .help("Registry write API base URL; defaults to CELLSCRIPT_REGISTRY_API_URL"),
                                    )
                                    .arg(
                                        Arg::new("registry-origin")
                                            .long("registry-origin")
                                            .value_name("URL")
                                            .help("Registry origin bound into the wallet revocation challenge"),
                                    )
                                    .arg(
                                        Arg::new("principal-type")
                                            .long("principal-type")
                                            .value_name("TYPE")
                                            .help("Principal type for the identity binding; defaults to joyid_ckb"),
                                    )
                                    .arg(
                                        Arg::new("principal-id")
                                            .long("principal-id")
                                            .value_name("ID")
                                            .help("Normalized principal binding derived from a supported CCC CKB signer"),
                                    )
                                    .arg(
                                        Arg::new("capability-key-id")
                                            .long("capability-key-id")
                                            .value_name("KEY_ID")
                                            .help("Capability key id to revoke"),
                                    )
                                    .arg(
                                        Arg::new("payload")
                                            .long("payload")
                                            .value_name("FILE")
                                            .help("Previously generated capability revocation payload JSON"),
                                    )
                                    .arg(
                                        Arg::new("wallet-signature")
                                            .long("wallet-signature")
                                            .visible_alias("joyid-signature")
                                            .value_name("FILE")
                                            .help("JoyID or CKB wallet signature JSON whose challenge is the canonical revoke payload"),
                                    )
                                    .arg(
                                        Arg::new("reason")
                                            .long("reason")
                                            .value_name("TEXT")
                                            .help("Human-readable reason to include with the revocation request"),
                                    )
                                    .arg(
                                        Arg::new("json")
                                            .long("json")
                                            .action(ArgAction::SetTrue)
                                            .help("Emit machine-readable capability revocation output"),
                                    ),
                            ),
                    )
                    .subcommand(
                        ClapCommand::new("reproducer")
                            .about("Manage independent reproducibility builder identities")
                            .subcommand_required(true)
                            .arg_required_else_help(true)
                            .subcommand(
                                ClapCommand::new("create")
                                    .about("Create a P-256 reproducer key and public policy enrollment record")
                                    .arg(
                                        Arg::new("builder-id")
                                            .long("builder-id")
                                            .value_name("ID")
                                            .required(true)
                                            .help("Stable builder identifier assigned by the independent operator"),
                                    )
                                    .arg(
                                        Arg::new("trust-domain")
                                            .long("trust-domain")
                                            .value_name("DOMAIN")
                                            .required(true)
                                            .help("Administrative and private-key custody domain for this builder"),
                                    )
                                    .arg(
                                        Arg::new("private-key-output")
                                            .long("private-key-output")
                                            .value_name("FILE")
                                            .help(
                                                "On Unix, write PKCS#8 base64 to a new mode-0600 file for CI secret enrollment instead of the OS keychain",
                                            ),
                                    )
                                    .arg(
                                        Arg::new("json")
                                            .long("json")
                                            .action(ArgAction::SetTrue)
                                            .help("Emit the public builder enrollment record as JSON without private-key material"),
                                    ),
                            ),
                    )
                    .subcommand(
                        ClapCommand::new("namespace")
                            .about("Manage Registry namespace ownership")
                            .subcommand_required(true)
                            .arg_required_else_help(true)
                            .subcommand(
                                ClapCommand::new("claim")
                                    .about("Claim a namespace with a wallet-signed capability authorisation payload")
                                    .arg(
                                        Arg::new("api-url")
                                            .long("api-url")
                                            .value_name("URL")
                                            .help("Registry write API base URL; defaults to CELLSCRIPT_REGISTRY_API_URL"),
                                    )
                                    .arg(
                                        Arg::new("namespace")
                                            .long("namespace")
                                            .value_name("NAMESPACE")
                                            .required(true)
                                            .help("Namespace to claim; the signed payload must contain a matching publish scope"),
                                    )
                                    .arg(
                                        Arg::new("payload")
                                            .long("payload")
                                            .value_name("FILE")
                                            .required(true)
                                            .help("Capability authorisation payload JSON created by auth capability create"),
                                    )
                                    .arg(
                                        Arg::new("wallet-signature")
                                            .long("wallet-signature")
                                            .visible_alias("joyid-signature")
                                            .value_name("FILE")
                                            .required(true)
                                            .help("JoyID or CKB wallet signature JSON whose challenge is the canonical capability payload"),
                                    )
                                    .arg(
                                        Arg::new("json")
                                            .long("json")
                                            .action(ArgAction::SetTrue)
                                            .help("Emit machine-readable namespace claim output"),
                                    ),
                            ),
                    ),
            )
            .subcommand(<CertifyArgs as clap::Args>::augment_args(
                ClapCommand::new("certify")
                    .display_order(100)
                    .about("Run a deterministic certification plugin (NovaSeal requires the novaseal-certification build feature)"),
            ))
            .subcommand(
                ClapCommand::new("package")
                    .about("Package integrity commands")
                    .subcommand_required(true)
                    .subcommand(ClapCommand::new("lock").about("Resolve and write the complete Cell.lock dependency graph"))
                    .subcommand(update_clap_command(
                        "update-plan",
                        "Resolve, compile, and classify a transactional dependency upgrade without changing Cell.lock",
                        false,
                    ))
                    .subcommand(update_clap_command(
                        "update",
                        "Plan an upgrade by default, or verify and atomically apply --apply-plan",
                        true,
                    ))
                    .subcommand(ClapCommand::new("verify").about("Verify package integrity against Cell.lock and source tree")),
            )
            .subcommand(
                ClapCommand::new("registry")
                    .display_order(90)
                    .about("Registry integrity commands")
                    .subcommand_required(true)
                    .subcommand(
                        ClapCommand::new("verify")
                            .about("Verify deployment registry records against Cell.lock and chain facts")

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
                    )
                    .subcommand(
                        ClapCommand::new("edit")
                            .about("Edit the package registry.json in the current package")
                            .arg(Arg::new("yank").long("yank").value_name("VERSION").help("Mark an existing version as yanked"))
                            .arg(Arg::new("reason").long("reason").value_name("TEXT").help("Reason recorded with --yank"))
                            .arg(Arg::new("replaced-by").long("replaced-by").value_name("VERSION").help("Suggested replacement version recorded with --yank"))
                            .arg(Arg::new("yanked-at").long("yanked-at").value_name("ISO8601").help("Override yank timestamp; defaults to current UTC")),
                    ),
            )
            .subcommand(
                ClapCommand::new("registry-verify")
                    .hide(true)
                    .about("Verify deployment registry records against Cell.lock and chain facts")

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
                    .hide(true)
                    .about("Verify package integrity against Cell.lock and source tree")
                    ,
            )
            .subcommand(
                ClapCommand::new("registry-add")
                    .hide(true)
                    .about("Register a new package in the discovery index")
                    .arg(Arg::new("namespace").long("namespace").required(true).value_name("NAMESPACE").help("Package namespace"))
                    .arg(Arg::new("name").long("name").required(true).value_name("NAME").help("Package name"))
                    .arg(Arg::new("source").long("source").required(true).value_name("URL").help("Source repository URL")),
            )
    }
}

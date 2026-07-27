//! Port of `scripts/validate_cellscript_tooling_release.py`.
//!
//! Asserts that the CellScript release boundary is consistent across
//! `Cargo.toml`, `Cargo.lock`, the VS Code extension, the changelogs, the
//! wiki, the gate script, the website, and the source pin points.
//!
//! Behavioural contract (must match the Python script byte-for-byte on stdout
//! and on exit code; stderr text is allowed to differ):
//! - success: prints exactly `valid CellScript tooling release boundary` to
//!   stdout and returns exit code 0;
//! - assertion failure: prints
//!   `invalid CellScript tooling release boundary: <message>` to stderr and
//!   returns exit code 1;
//! - structural failure (missing file / malformed JSON or TOML / missing gate
//!   marker): the Python original raises an uncaught traceback and exits 1;
//!   this port returns exit code 1 with a clean `anyhow` message. The dev/CI
//!   gate only compares stdout and exit code, so this is a strictly-better
//!   diagnostic without changing the contract.

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use regex::Regex;

use crate::shared::{contains, read_text, slice_between};

/// A small helper for the substring-check idiom `token in text`.
///
/// Mirrors `require_contains(path, tokens)` from the Python script: re-reads
/// the file once per call (the Python original also re-reads on every call) so
/// the behaviour is preserved exactly, including the per-token error message
/// format `<path> is missing '<token>'` (single quotes, matching Python
/// `repr()` of a string that contains double quotes).
fn require_contains(root: &Path, path: &str, tokens: &[impl AsRef<str>]) -> Result<()> {
    let text = read_text(root, path)?;
    for token in tokens {
        let token = token.as_ref();
        if !contains(&text, token) {
            return Err(anyhow!("{path} is missing '{token}'"));
        }
    }
    Ok(())
}

/// Mirror `require(condition, message)` from the Python script. The message is
/// the inner text only; the wrapping
/// `invalid CellScript tooling release boundary: ` prefix is added here so
/// that callers can use the bare inner message, matching the Python source.
fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(anyhow!("invalid CellScript tooling release boundary: {}", message.into()))
    }
}

/// Same as `require`, but the message is constructed only when the condition
/// fails. Mirrors Python's eager `f""` interpolation while skipping the work
/// in the common (passing) case.
fn require_with<F: FnOnce() -> String>(condition: bool, msg: F) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(anyhow!("invalid CellScript tooling release boundary: {}", msg()))
    }
}

/// The single regex used by the script: capture the semver from the first
/// `## <semver> - ` heading. Python uses `re.MULTILINE`, equivalent to `(?m)`
/// here, so `^` matches at every line start; `re.search` returns the first
/// match anywhere in the text.
fn changelog_head() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?m)^## ([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?) - ").expect("changelog heading regex must compile")
    })
}

/// Compute `release_surface`: first two dotted components of the version with
/// any `-pre-release` stripped. Mirrors
/// `".".join(crate_version.split("-", 1)[0].split(".")[:2])`.
fn release_surface(crate_version: &str) -> String {
    let base = crate_version.split('-').next().unwrap_or(crate_version);
    base.split('.').take(2).collect::<Vec<_>>().join(".")
}

/// Entry point. Returns `Ok(())` (exit 0) on a valid boundary; otherwise an
/// error whose display string is the full Python-shaped message.
pub fn run(root: &Path) -> Result<()> {
    // --- Stage A: load inputs and derive version-dependent values ---------
    let cargo_toml = read_text(root, "Cargo.toml")?;
    let cargo: toml::Value = cargo_toml.parse().map_err(|e| anyhow!("Cargo.toml is not valid TOML: {e}"))?;
    let cargo_lock: toml::Value = read_text(root, "Cargo.lock")?.parse().map_err(|e| anyhow!("Cargo.lock is not valid TOML: {e}"))?;
    let package_json: serde_json::Value = serde_json::from_str(&read_text(root, "editors/vscode-cellscript/package.json")?)
        .map_err(|e| anyhow!("VS Code package.json is not valid JSON: {e}"))?;
    let changelog = read_text(root, "CHANGELOG.md")?;
    let extension_changelog = read_text(root, "editors/vscode-cellscript/CHANGELOG.md")?;
    let extension_readme = read_text(root, "editors/vscode-cellscript/README.md")?;

    let crate_version = cargo
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Cargo.toml package.version is missing"))?
        .to_string();

    let lock_versions: Vec<String> = cargo_lock
        .get("package")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| {
                    let name = entry.get("name").and_then(|n| n.as_str())?;
                    let version = entry.get("version").and_then(|v| v.as_str())?;
                    (name == "cellscript").then(|| version.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let surface = release_surface(&crate_version);
    let changelog_match = changelog_head().captures(&changelog);

    // --- Stage B: version-consistency checks ------------------------------
    require_with(lock_versions.as_slice() == [crate_version.as_str()], || {
        "Cargo.lock cellscript version must match Cargo.toml package.version".to_string()
    })?;
    require_with(package_json.get("version").and_then(|v| v.as_str()) == Some(crate_version.as_str()), || {
        "VS Code extension version must match Cargo.toml package.version".to_string()
    })?;
    require(changelog_match.is_some(), "CHANGELOG.md must start with a semver release heading")?;
    require_with(changelog_match.as_ref().and_then(|c| c.get(1)).map(|m| m.as_str()) == Some(crate_version.as_str()), || {
        "CHANGELOG.md current release heading must match Cargo.toml package.version".to_string()
    })?;
    require(
        extension_changelog.contains(&format!("## {crate_version}")),
        "VS Code extension changelog must include the current package version",
    )?;
    require(
        extension_readme.contains(&format!("current {surface} authoring surface")),
        "VS Code extension README must name the current authoring surface",
    )?;
    require(
        !extension_readme.contains("current 0.15 authoring surface"),
        "VS Code extension README must not describe the current surface as 0.15",
    )?;

    // --- Stage C: source-pin contains checks ------------------------------
    require_contains(root, "src/lib.rs", &[r#"pub const VERSION: &str = env!("CARGO_PKG_VERSION");"#])?;
    require_contains(root, "src/main.rs", &["#[command(version = cellscript::VERSION)]"])?;
    require_contains(root, "README.md", &[format!("version = \"{crate_version}\"")])?;

    // --- Stage D: wiki gate-version loop -----------------------------------
    for wiki_path in &[
        "docs/wiki/Tutorial-01-Getting-Started.md",
        "docs/wiki/Cookbook-Recipes.md",
        "docs/wiki/Tutorial-03-Resources-and-Cell-Effects.md",
        "docs/wiki/Tutorial-08-Bundled-Example-Contracts.md",
        "docs/wiki/Tutorial-11-Scoped-Invariants-and-ProofPlan.md",
    ] {
        let text = read_text(root, wiki_path)?;
        require(
            !text.contains("--primitive-strict 0.15"),
            format!("{wiki_path} must use the current 0.16 assurance gate in command examples"),
        )?;
        require(
            !text.contains("--primitive-strict=0.15"),
            format!("{wiki_path} must use the current 0.16 assurance gate in command examples"),
        )?;
    }

    // --- Stage E: ckb_acceptance ------------------------------------------
    let ckb_acceptance = read_text(root, "scripts/ckb_cellscript_acceptance.sh")?;
    require(
        !ckb_acceptance.contains(r#""--primitive-strict", "0.15""#),
        "CKB acceptance runner must not use the retired 0.15 assurance gate",
    )?;
    require(
        ckb_acceptance.contains(r#""--primitive-strict", "0.16""#),
        "CKB acceptance runner must use the current 0.16 assurance gate",
    )?;
    require(
        ckb_acceptance.contains("ORIGINAL_SCOPED_ACTION_FAIL_CLOSED = {}"),
        "CKB acceptance runner must keep token/AMM/launch out of strict 0.16 fail-closed coverage",
    )?;
    require(
        ckb_acceptance.contains(r#""token.cell": ["mint_with_authority", "transfer_token", "burn", "merge"]"#),
        "CKB acceptance runner must compile token actions as original strict scoped actions",
    )?;
    require(
        ckb_acceptance.contains(r#""amm_pool.cell": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity"]"#),
        "CKB acceptance runner must compile AMM actions as original strict scoped actions",
    )?;
    require(
        ckb_acceptance.contains(r#""launch.cell": ["launch_token", "bootstrap_token"]"#),
        "CKB acceptance runner must compile launch actions as original strict scoped actions",
    )?;
    require(
        !ckb_acceptance.contains("mapfile") && !ckb_acceptance.contains("readarray"),
        "CKB acceptance runner must remain compatible with macOS Bash 3.2",
    )?;
    require(ckb_acceptance.contains("while IFS= read -r value"), "CKB acceptance pin parsing must use the portable read loop")?;

    // --- Stage F: Tutorial-08 ---------------------------------------------
    let tutorial_08 = read_text(root, "docs/wiki/Tutorial-08-Bundled-Example-Contracts.md")?;
    require(
        tutorial_08.contains("strict v0.16 ProofPlan gate"),
        "bundled example tutorial must document the strict 0.16 ProofPlan gate",
    )?;
    // The token literal contains embedded newlines and exactly two spaces of
    // indent before `echo`. Carry it verbatim.
    require(
        tutorial_08
            .contains("for f in examples/*.cell; do\n  echo \"==> $f\"\n  cellc \"$f\" --target riscv64-elf --target-profile ckb -o"),
        "bundled example compile-all loop must not claim every example passes strict 0.16",
    )?;

    // --- Stage G: package.json structural checks --------------------------
    require(package_json.get("name").and_then(|v| v.as_str()) == Some("cellscript-vscode"), "VS Code extension package name changed")?;
    require(package_json.get("main").and_then(|v| v.as_str()) == Some("./dist/extension.js"), "VS Code extension entrypoint changed")?;
    require(
        package_json.get("devDependencies").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("vscode-languageclient")),
        "VS Code extension must build with vscode-languageclient",
    )?;
    require(
        package_json.get("devDependencies").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("esbuild")),
        "VS Code extension must bundle with esbuild",
    )?;
    require(
        package_json.get("devDependencies").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("@vscode/vsce")),
        "VS Code extension must pin vsce for package dry runs",
    )?;
    require(
        package_json.get("scripts").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("build")),
        "VS Code extension must expose a build script",
    )?;
    require(
        package_json.get("scripts").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("vscode:prepublish")),
        "VS Code extension must build before publish",
    )?;
    require(
        package_json.get("scripts").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("package")),
        "VS Code extension must expose a package script",
    )?;
    require(
        package_json.get("scripts").and_then(|v| v.as_object()).is_some_and(|o| o.contains_key("publish:dry-run")),
        "VS Code extension must expose a publish dry-run script",
    )?;
    let publish_dry_run = package_json
        .get("scripts")
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("publish:dry-run"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            anyhow!("invalid CellScript tooling release boundary: VS Code extension must expose a publish dry-run script")
        })?;
    require(
        publish_dry_run.contains("vsce package --no-dependencies --out /tmp/cellscript-vscode-dry-run.vsix"),
        "VS Code publish dry-run must package a local VSIX instead of using an unsupported publish --dry-run flag",
    )?;

    // --- Stage H: contributed commands + activation events ----------------
    let commands: std::collections::BTreeSet<String> = package_json
        .get("contributes")
        .and_then(|c| c.get("commands"))
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(|c| c.get("command").and_then(|v| v.as_str()).map(String::from)).collect())
        .unwrap_or_default();
    let activation: std::collections::BTreeSet<String> = package_json
        .get("activationEvents")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    for command in &[
        "cellscript.compileCurrentFile",
        "cellscript.showMetadata",
        "cellscript.showConstraints",
        "cellscript.showAbi",
        "cellscript.showActionBuildPlan",
        "cellscript.generateTypescriptBuilder",
        "cellscript.verifyPackage",
        "cellscript.verifyRegistry",
        "cellscript.verifyLiveRegistry",
        "cellscript.showProductionReport",
    ] {
        require(commands.contains(*command), format!("VS Code extension must contribute {command}"))?;
        require(activation.contains(&format!("onCommand:{command}")), format!("VS Code extension must activate for {command}"))?;
    }

    // --- Stage I: contributed configuration settings ----------------------
    let settings: std::collections::BTreeSet<String> = package_json
        .get("contributes")
        .and_then(|c| c.get("configuration"))
        .and_then(|c| c.get("properties"))
        .and_then(|v| v.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    for setting in &[
        "cellscript.compilerPath",
        "cellscript.useCargoRunFallback",
        "cellscript.commandTimeoutMs",
        "cellscript.maxOutputBytes",
        "cellscript.target",
        "cellscript.builderOutputDir",
        "cellscript.ckbRpcUrl",
        "cellscript.deploymentNetwork",
        "cellscript.registryRequirePublisherSignature",
        "cellscript.registryRequireAuditReport",
    ] {
        require(settings.contains(*setting), format!("VS Code extension must expose {setting}"))?;
    }

    // --- Stage J: source/extension require_contains blocks ----------------
    require_contains(
        root,
        "src/main.rs",
        &["Start the language server (JSON-RPC over stdio).", "cellscript::lsp::server::run_lsp_server_blocking();"],
    )?;
    require_contains(
        root,
        "src/lsp/server.rs",
        &[
            "tower_lsp::LanguageServer",
            "JSON-RPC",
            "completion_provider",
            "hover_provider",
            "definition_provider",
            "references_provider",
            "rename_provider",
            "document_formatting_provider",
            "signature_help_provider",
            "folding_range_provider",
            "selection_range_provider",
        ],
    )?;
    require_contains(
        root,
        "editors/vscode-cellscript/extension.js",
        &[
            "LanguageClient",
            "TransportKind.stdio",
            "--lsp",
            "selectMetadataEntry",
            "findPackageRootForDocument",
            "cellscript.showConstraints",
            "cellscript.showAbi",
            "cellscript.showActionBuildPlan",
            "cellscript.generateTypescriptBuilder",
            "cellscript.verifyPackage",
            "cellscript.verifyRegistry",
            "cellscript.verifyLiveRegistry",
            "cellscript.showProductionReport",
            "gen-builder",
            "package",
            "verify",
            "registry",
            "ckbRpcUrl",
            "registryRequirePublisherSignature",
            "registryRequireAuditReport",
            "--require-publisher-signature",
            "--require-audit-report",
        ],
    )?;
    require_contains(
        root,
        "editors/vscode-cellscript/scripts/validate.mjs",
        &[
            "LanguageClient",
            "TransportKind.stdio",
            "cellscript.generateTypescriptBuilder",
            "cellscript.verifyLiveRegistry",
            "cellscript.builderOutputDir",
            "extension README must describe the production local tooling surface",
        ],
    )?;
    require_contains(
        root,
        "scripts/cellscript_ckb_release_gate.sh",
        &[r#"exec "$ROOT_DIR/scripts/cellscript_gate.sh" release"#, r#"exec "$ROOT_DIR/scripts/cellscript_gate.sh" release-quick"#],
    )?;
    require_contains(
        root,
        "README.md",
        &["cellc action build", "cellc gen-builder --target typescript", "cellc package verify", "cellc registry verify --live"],
    )?;
    require_contains(
        root,
        "website/package.json",
        &[
            r#""prepare:registry": "python3 scripts/generate-registry-data.py""#,
            r#""build": "npm run prepare:registry && astro check && astro build && npm run check:docs && npm run check:dist""#,
            r#""check:docs": "node scripts/check-doc-links.mjs""#,
            r#""check:dist": "node scripts/check-dist-regressions.mjs""#,
        ],
    )?;
    require_contains(root, "website/src/pages/index.astro", &[r#"href="/registry""#, r#"data-i18n="nav.registryBrowse""#])?;
    require_contains(
        root,
        "scripts/cellscript_gate.sh",
        &[
            "run_in_dir",
            "run_website_build_check",
            "website registry data is stale",
            "run_in_dir website npm exec -- astro check",
            "run_in_dir website npm exec -- astro build",
            "run_in_dir editors/vscode-cellscript npm exec -- vsce package --no-dependencies --out /tmp/cellscript-vscode-dry-run.vsix",
            "node editors/vscode-cellscript/scripts/validate.mjs",
        ],
    )?;

    // --- Stage K: gate-script slice + tx_measure_gate checks --------------
    let gate_script = read_text(root, "scripts/cellscript_gate.sh")?;
    let tx_measure_gate = slice_between(&gate_script, "check_ckb_tx_measure_tool() {", "check_novaseal_rust_tooling() {")?;
    require(
        tx_measure_gate.contains("cargo test --manifest-path tools/ckb-tx-measure/Cargo.toml --locked"),
        "CKB transaction measure tooling must be tested by the release gate",
    )?;
    require(
        !tx_measure_gate.contains("RUSTUP_TOOLCHAIN"),
        "CKB transaction measure tooling must use CellScript's pinned Rust toolchain",
    )?;
    require(
        gate_script.contains(r#"print(manifest["package"]["version"])"#),
        "release source identity must read the root package version from Cargo.toml",
    )?;
    require(
        !gate_script.contains(r#"manifest["workspace"]["package"]"#),
        "release source identity must not assume a virtual workspace package table",
    )?;

    // --- Stage L: website workflow -----------------------------------------
    require_contains(
        root,
        ".github/workflows/website-build.yml",
        &["workflow_dispatch:", "Generate registry website data", "Check generated registry data is committed", "Upload website dist"],
    )?;
    let website_build_workflow = read_text(root, ".github/workflows/website-build.yml")?;
    require(
        !website_build_workflow.contains("pull_request:"),
        "website artifact workflow must not duplicate the unified CI gate on pull requests",
    )?;
    require(!website_build_workflow.contains("push:"), "website artifact workflow must not duplicate the unified CI gate on pushes")?;

    // --- Stage M: CLI wiring ----------------------------------------------
    require_contains(root, "src/main.rs", &["cellc_cli_command().get_subcommands()", "cellscript::cli::run()"])?;
    require_contains(root, "src/cli/mod.rs", &["mod novaseal_certification;"])?;
    require_contains(root, "src/cli/commands.rs", &["Command::Certify", "novaseal-profile-v0"])?;

    // --- Stage N: docs + Rust source require_contains ---------------------
    require_contains(
        root,
        "docs/wiki/Tutorial-07-LSP-and-Tooling.md",
        &[
            "CellScript: Generate TypeScript Action Builder",
            "cellscript.builderOutputDir",
            "cellc registry verify --live",
            "cellscript.registryRequirePublisherSignature",
            "cellscript.registryRequireAuditReport",
            "npm test",
        ],
    )?;
    require_contains(
        root,
        "docs/archive/0.20/CELLSCRIPT_0_20_ROADMAP.md",
        &["VS Code extension", "check_action_builder_toolchain", "CellFabric is frozen"],
    )?;
    require_contains(
        root,
        "src/package/mod.rs",
        &[
            "failed to resolve registry dependency '{}/{}@{}' via discovery index '{}': {}",
            "registry package '{}/{}@{}' has no source_hash in registry.json",
            "source_hash mismatch for '{}/{}@{}': expected '{}', got '{}'",
            "Git { url: String, revision: String }",
            "pub fn consistency_issues(&self, manifest: &PackageManifest) -> Vec<String>",
            "pub fn replace_with_resolved(&mut self, resolved: &HashMap<String, ResolvedPackage>)",
        ],
    )?;
    require_contains(
        root,
        "tests/cli.rs",
        &[
            "cellc_rejects_registry_dependency_without_namespace",
            "cellc_build_resolves_registry_dependency_and_writes_phase1_lockfile",
            "cellc_install_path_updates_lockfile_and_remove_prunes_it",
            "cellc_fmt_subcommand_formats_sources",
            "cellc_run_subcommand_executes_pure_elf_package",
            "cellc_gen_builder_typescript_emits_package_scaffold",
            "cellc_gen_builder_lockfile_identity_fails_closed",
        ],
    )?;
    require_contains(
        root,
        "tests/registry.rs",
        &[
            "package_manager_resolves_registry_dependency_with_source_hash_from_local_git_fixture",
            "package_manager_rejects_registry_source_hash_mismatch",
            "lockfile_consistency_accepts_matching_registry_source",
        ],
    )?;

    // --- Stage O: Cargo.toml exclude array --------------------------------
    // The `excluded` literals include the surrounding double quotes so they
    // match the TOML array element verbatim via substring on the raw text.
    for excluded in
        &[r#"".github/""#, r#""docs/""#, r#""docs/wiki/""#, r#""editors/""#, r#""proposals/""#, r#""scripts/__pycache__/""#]
    {
        require(cargo_toml.contains(excluded), format!("Cargo.toml package exclude is missing {excluded}"))?;
    }

    // --- Stage P: .gitignore ----------------------------------------------
    let gitignore = read_text(root, ".gitignore")?;
    require(gitignore.contains("__pycache__/"), ".gitignore must ignore generated Python bytecode directories")?;
    require(gitignore.contains("*.py[cod]"), ".gitignore must ignore generated Python bytecode files")?;

    // --- Stage Q: success -------------------------------------------------
    println!("valid CellScript tooling release boundary");
    Ok(())
}

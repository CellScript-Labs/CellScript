#!/usr/bin/env python3
"""Validate CellScript package/LSP/tooling release boundaries."""

from __future__ import annotations

import json
import re
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"invalid CellScript tooling release boundary: {message}")


def require_contains(path: str, tokens: list[str]) -> None:
    text = read(path)
    for token in tokens:
        require(token in text, f"{path} is missing {token!r}")


def main() -> int:
    cargo_toml = read("Cargo.toml")
    cargo = tomllib.loads(cargo_toml)
    cargo_lock = tomllib.loads(read("Cargo.lock"))
    package_json = json.loads(read("editors/vscode-cellscript/package.json"))
    changelog = read("CHANGELOG.md")
    extension_changelog = read("editors/vscode-cellscript/CHANGELOG.md")
    extension_readme = read("editors/vscode-cellscript/README.md")

    crate_version = cargo["package"]["version"]
    lock_versions = [
        package.get("version")
        for package in cargo_lock.get("package", [])
        if package.get("name") == "cellscript"
    ]
    changelog_match = re.search(r"^## ([0-9]+\.[0-9]+\.[0-9]+) - ", changelog, re.MULTILINE)

    require(lock_versions == [crate_version], "Cargo.lock cellscript version must match Cargo.toml package.version")
    require(package_json["version"] == crate_version, "VS Code extension version must match Cargo.toml package.version")
    require(changelog_match is not None, "CHANGELOG.md must start with a semver release heading")
    require(changelog_match.group(1) == crate_version, "CHANGELOG.md current release heading must match Cargo.toml package.version")
    require(f"## {crate_version}" in extension_changelog, "VS Code extension changelog must include the current package version")
    require(f"current {crate_version.rsplit('.', 1)[0]} authoring surface" in extension_readme, "VS Code extension README must name the current authoring surface")
    require("current 0.15 authoring surface" not in extension_readme, "VS Code extension README must not describe the current surface as 0.15")
    require_contains(
        "src/lib.rs",
        ['pub const VERSION: &str = env!("CARGO_PKG_VERSION");'],
    )
    require_contains(
        "src/main.rs",
        ["#[command(version = cellscript::VERSION)]"],
    )
    require_contains("README.md", [f'version = "{crate_version}"'])
    for wiki_path in [
        "docs/wiki/Tutorial-01-Getting-Started.md",
        "docs/wiki/Cookbook-Recipes.md",
        "docs/wiki/Tutorial-03-Resources-and-Cell-Effects.md",
        "docs/wiki/Tutorial-08-Bundled-Example-Contracts.md",
        "docs/wiki/Tutorial-11-Scoped-Invariants-and-ProofPlan.md",
    ]:
        require("--primitive-strict 0.15" not in read(wiki_path), f"{wiki_path} must use the current 0.16 assurance gate in command examples")
        require("--primitive-strict=0.15" not in read(wiki_path), f"{wiki_path} must use the current 0.16 assurance gate in command examples")

    ckb_acceptance = read("scripts/ckb_cellscript_acceptance.sh")
    require('"--primitive-strict", "0.15"' not in ckb_acceptance, "CKB acceptance runner must not use the retired 0.15 assurance gate")
    require('"--primitive-strict", "0.16"' in ckb_acceptance, "CKB acceptance runner must use the current 0.16 assurance gate")
    require("ORIGINAL_SCOPED_ACTION_FAIL_CLOSED = {}" in ckb_acceptance, "CKB acceptance runner must keep token/AMM/launch out of strict 0.16 fail-closed coverage")
    require('"token.cell": ["mint_with_authority", "transfer_token", "burn", "merge"]' in ckb_acceptance, "CKB acceptance runner must compile token actions as original strict scoped actions")
    require('"amm_pool.cell": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity", "isqrt", "min"]' in ckb_acceptance, "CKB acceptance runner must compile AMM actions as original strict scoped actions")
    require('"launch.cell": ["launch_token", "bootstrap_token"]' in ckb_acceptance, "CKB acceptance runner must compile launch actions as original strict scoped actions")

    tutorial_08 = read("docs/wiki/Tutorial-08-Bundled-Example-Contracts.md")
    require("strict v0.16 ProofPlan gate" in tutorial_08, "bundled example tutorial must document the strict 0.16 ProofPlan gate")
    require('for f in examples/*.cell; do\n  echo "==> $f"\n  cellc "$f" --target riscv64-elf --target-profile ckb -o' in tutorial_08, "bundled example compile-all loop must not claim every example passes strict 0.16")

    require(package_json["name"] == "cellscript-vscode", "VS Code extension package name changed")
    require(package_json["main"] == "./dist/extension.js", "VS Code extension entrypoint changed")
    require("vscode-languageclient" in package_json.get("devDependencies", {}), "VS Code extension must build with vscode-languageclient")
    require("esbuild" in package_json.get("devDependencies", {}), "VS Code extension must bundle with esbuild")
    require("@vscode/vsce" in package_json.get("devDependencies", {}), "VS Code extension must pin vsce for package dry runs")
    require("build" in package_json.get("scripts", {}), "VS Code extension must expose a build script")
    require("vscode:prepublish" in package_json.get("scripts", {}), "VS Code extension must build before publish")
    require("package" in package_json.get("scripts", {}), "VS Code extension must expose a package script")
    require("publish:dry-run" in package_json.get("scripts", {}), "VS Code extension must expose a publish dry-run script")
    require(
        "vsce package --no-dependencies --out /tmp/cellscript-vscode-dry-run.vsix"
        in package_json["scripts"]["publish:dry-run"],
        "VS Code publish dry-run must package a local VSIX instead of using an unsupported publish --dry-run flag",
    )
    commands = {command.get("command") for command in package_json.get("contributes", {}).get("commands", [])}
    for command in [
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
    ]:
        require(command in commands, f"VS Code extension must contribute {command}")
        require(
            f"onCommand:{command}" in package_json.get("activationEvents", []),
            f"VS Code extension must activate for {command}",
        )
    settings = package_json.get("contributes", {}).get("configuration", {}).get("properties", {})
    for setting in [
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
    ]:
        require(setting in settings, f"VS Code extension must expose {setting}")

    require_contains(
        "src/main.rs",
        [
            "Start the language server (JSON-RPC over stdio).",
            "cellscript::lsp::server::run_lsp_server_blocking();",
        ],
    )
    require_contains(
        "src/lsp/server.rs",
        [
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
    )
    require_contains(
        "editors/vscode-cellscript/extension.js",
        [
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
    )
    require_contains(
        "editors/vscode-cellscript/scripts/validate.mjs",
        [
            "LanguageClient",
            "TransportKind.stdio",
            "cellscript.generateTypescriptBuilder",
            "cellscript.verifyLiveRegistry",
            "cellscript.builderOutputDir",
            "extension README must describe the production local tooling surface",
        ],
    )
    require_contains(
        "scripts/cellscript_ckb_release_gate.sh",
        [
            "check_action_builder_toolchain",
            "cellc gen-builder",
            "npm --prefix",
            "generated builder",
            "check_novaseal_certify_invariant",
            "check_novaseal_certify_runs",
        ],
    )
    require_contains(
        "README.md",
        [
            "cellc action build",
            "cellc gen-builder --target typescript",
            "cellc package verify",
            "cellc registry verify --live",
        ],
    )
    require_contains(
        "src/main.rs",
        [
            '"certify"',
        ],
    )
    require_contains(
        "src/cli/mod.rs",
        [
            "mod novaseal_certification;",
        ],
    )
    require_contains(
        "src/cli/commands.rs",
        [
            "Command::Certify",
            "novaseal-profile-v0",
        ],
    )
    require_contains(
        "docs/wiki/Tutorial-07-LSP-and-Tooling.md",
        [
            "CellScript: Generate TypeScript Action Builder",
            "cellscript.builderOutputDir",
            "cellc registry verify --live",
            "cellscript.registryRequirePublisherSignature",
            "cellscript.registryRequireAuditReport",
            "npm test",
        ],
    )
    require_contains(
        "docs/CELLSCRIPT_0_20_ROADMAP.md",
        [
            "VS Code extension",
            "check_action_builder_toolchain",
            "CellFabric is frozen",
        ],
    )
    require_contains(
        "src/package/mod.rs",
        [
            "registry dependency '{}' with version '{}' is not supported yet; use a local path dependency",
            "Git { url: String, revision: String }",
            "pub fn consistency_issues(&self, manifest: &PackageManifest) -> Vec<String>",
            "pub fn replace_with_resolved(&mut self, resolved: &HashMap<String, ResolvedPackage>)",
        ],
    )
    require_contains(
        "tests/cli.rs",
        [
            "cellc_rejects_registry_package_dependencies_fail_closed",
            "cellc_install_path_updates_lockfile_and_remove_prunes_it",
            "cellc_fmt_subcommand_formats_sources",
            "cellc_run_subcommand_executes_pure_elf_package",
            "cellc_gen_builder_typescript_emits_package_scaffold",
            "cellc_gen_builder_lockfile_identity_fails_closed",
        ],
    )

    for excluded in [
        '".github/"',
        '"docs/"',
        '"docs/wiki/"',
        '"editors/"',
        '"proposals/"',
        '"scripts/__pycache__/"',
    ]:
        require(excluded in cargo_toml, f"Cargo.toml package exclude is missing {excluded}")

    require("__pycache__/" in read(".gitignore"), ".gitignore must ignore generated Python bytecode directories")
    require("*.py[cod]" in read(".gitignore"), ".gitignore must ignore generated Python bytecode files")

    print("valid CellScript tooling release boundary")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

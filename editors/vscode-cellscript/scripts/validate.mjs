import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const repoRoot = path.resolve(root, "..", "..");

const requiredFiles = [
  "package.json",
  "extension.js",
  "dist/extension.js",
  "README.md",
  "CHANGELOG.md",
  ".vscodeignore",
  "language-configuration.json",
  "syntaxes/cellscript.tmLanguage.json",
  "snippets/cellscript.json"
];

for (const relative of requiredFiles) {
  const file = path.join(root, relative);
  if (!fs.existsSync(file)) {
    throw new Error(`missing required file: ${relative}`);
  }
}

const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const cargoToml = fs.readFileSync(path.join(repoRoot, "Cargo.toml"), "utf8");
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const grammar = JSON.parse(fs.readFileSync(path.join(root, "syntaxes/cellscript.tmLanguage.json"), "utf8"));
const languageConfig = JSON.parse(fs.readFileSync(path.join(root, "language-configuration.json"), "utf8"));
const snippets = JSON.parse(fs.readFileSync(path.join(root, "snippets/cellscript.json"), "utf8"));

if (pkg.name !== "cellscript-vscode") {
  throw new Error(`unexpected package name: ${pkg.name}`);
}

if (!cargoVersion) {
  throw new Error("unable to read root Cargo.toml package version");
}

if (pkg.version !== cargoVersion) {
  throw new Error(`extension version ${pkg.version} does not match crate version ${cargoVersion}`);
}

if (!pkg.repository?.url?.includes("a19q3/CellScript")) {
  throw new Error(`extension repository must point at standalone CellScript repo: ${pkg.repository?.url}`);
}

for (const field of [pkg.homepage, pkg.bugs?.url]) {
  if (!field?.includes("a19q3/CellScript")) {
    throw new Error(`extension package URL must point at standalone CellScript repo: ${field}`);
  }
}

if (!Array.isArray(pkg.contributes?.languages) || pkg.contributes.languages.length === 0) {
  throw new Error("package.json must contribute at least one language");
}

if (pkg.main !== "./dist/extension.js") {
  throw new Error(`unexpected extension entrypoint: ${pkg.main}`);
}

const commands = new Set((pkg.contributes?.commands || []).map((command) => command.command));
for (const command of [
  "cellscript.compileCurrentFile",
  "cellscript.showMetadata",
  "cellscript.showConstraints",
  "cellscript.showAbi",
  "cellscript.showActionBuildPlan",
  "cellscript.generateTypescriptBuilder",
  "cellscript.verifyPackage",
  "cellscript.verifyRegistry",
  "cellscript.verifyLiveRegistry",
  "cellscript.showProductionReport"
]) {
  if (!commands.has(command)) {
    throw new Error(`missing command contribution: ${command}`);
  }
}

const properties = pkg.contributes?.configuration?.properties || {};
for (const setting of [
  "cellscript.compilerPath",
  "cellscript.useCargoRunFallback",
  "cellscript.commandTimeoutMs",
  "cellscript.maxOutputBytes",
  "cellscript.target",
  "cellscript.builderOutputDir",
  "cellscript.ckbRpcUrl",
  "cellscript.deploymentNetwork",
  "cellscript.registryRequirePublisherSignature",
  "cellscript.registryRequireAuditReport"
]) {
  if (!properties[setting]) {
    throw new Error(`missing configuration setting: ${setting}`);
  }
}

if (!Array.isArray(grammar.patterns) || grammar.patterns.length === 0) {
  throw new Error("grammar must contain top-level patterns");
}

if (grammar.scopeName !== "source.cellscript") {
  throw new Error(`unexpected grammar scope: ${grammar.scopeName}`);
}

if (!languageConfig.comments?.lineComment) {
  throw new Error("language configuration must declare line comments");
}

if (typeof snippets !== "object" || snippets === null || Object.keys(snippets).length === 0) {
  throw new Error("snippets file must contain at least one snippet");
}

const grammarText = JSON.stringify(grammar);
const snippetsText = JSON.stringify(snippets);
for (const token of [
  "create_unique",
  "replace_unique",
  "destroy_singleton_type",
  "destroy_unique",
  "destroy_instance",
  "burn_amount",
  "identity",
  "ckb_type_id",
  "script_args",
  "singleton_type",
  "assert_sum",
  "assert_conserved",
  "assert_delta",
  "assert_distinct",
  "assert_singleton",
  "retarget_type"
]) {
  if (!grammarText.includes(token)) {
    throw new Error(`grammar is missing current CellScript editor surface token: ${token}`);
  }
}

for (const snippet of [
  "create_unique",
  "replace_unique",
  "destroy_unique",
  "burn_amount",
  "assert_sum",
  "assert_conserved",
  "assert_delta",
  "assert_distinct",
  "assert_singleton",
  "#[type_id"
]) {
  if (!snippetsText.includes(snippet)) {
    throw new Error(`snippets are missing current CellScript authoring surface: ${snippet}`);
  }
}

const extensionSource = fs.readFileSync(path.join(root, "extension.js"), "utf8");
const bundledExtension = fs.readFileSync(path.join(root, "dist/extension.js"), "utf8");
const vscodeIgnore = fs.readFileSync(path.join(root, ".vscodeignore"), "utf8");
const changelog = fs.readFileSync(path.join(root, "CHANGELOG.md"), "utf8");
for (const token of [
  "LanguageClient",
  "vscode-languageclient/node",
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
  "cellc",
  "Cell.toml",
  "action",
  "gen-builder",
  "package",
  "registry",
  "registryRequirePublisherSignature",
  "registryRequireAuditReport",
  "--require-publisher-signature",
  "--require-audit-report",
  "typescript",
  "--lsp",
  "TransportKind.stdio"
]) {
  if (!extensionSource.includes(token)) {
    throw new Error(`extension runtime is missing expected wiring: ${token}`);
  }
}

if (!bundledExtension.includes("LanguageClient")) {
  throw new Error("bundled extension is missing language client runtime");
}

for (const ignored of ["node_modules/**", "extension.js", "scripts/**"]) {
  if (!vscodeIgnore.includes(ignored)) {
    throw new Error(`.vscodeignore must exclude bundled-only input: ${ignored}`);
  }
}

if (extensionSource.includes('"--target", "riscv64-asm", ...targetProfileArgs(document)')) {
  throw new Error("compile command must not hard-code a second target before configured targetProfileArgs");
}

const readme = fs.readFileSync(path.join(root, "README.md"), "utf8");
if (/\bbeta\b|\bthin\b|placeholder|metadata-only/i.test(readme)) {
  throw new Error("extension README must describe the production local tooling surface, not beta/thin placeholder scope");
}
if (readme.includes("cellc lsp --stdio")) {
  throw new Error("extension README must document the supported LSP entrypoint as `cellc --lsp`");
}
if (/\brename\b/i.test(readme)) {
  throw new Error("extension README must not claim rename support while the LSP rename provider is disabled");
}
if (!changelog.includes(`## ${pkg.version}`)) {
  throw new Error(`extension changelog is missing current package version entry: ${pkg.version}`);
}

console.log("CellScript VS Code extension manifest is valid.");

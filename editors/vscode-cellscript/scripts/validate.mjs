import fs from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");

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
const grammar = JSON.parse(fs.readFileSync(path.join(root, "syntaxes/cellscript.tmLanguage.json"), "utf8"));
const languageConfig = JSON.parse(fs.readFileSync(path.join(root, "language-configuration.json"), "utf8"));
const snippets = JSON.parse(fs.readFileSync(path.join(root, "snippets/cellscript.json"), "utf8"));
const grammarSource = fs.readFileSync(path.join(root, "syntaxes/cellscript.tmLanguage.json"), "utf8");
const snippetsSource = fs.readFileSync(path.join(root, "snippets/cellscript.json"), "utf8");

if (pkg.name !== "cellscript-vscode") {
  throw new Error(`unexpected package name: ${pkg.name}`);
}

if (pkg.version !== "0.13.2") {
  throw new Error(`unexpected extension version: ${pkg.version}`);
}

if (!pkg.repository?.url?.includes("tsukifune-kosei/CellScript")) {
  throw new Error(`extension repository must point at standalone CellScript repo: ${pkg.repository?.url}`);
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
  "cellscript.target"
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

for (const keyword of [
  "where",
  "flow",
  "transition",
  "read",
  "invariant",
  "input",
  "output",
  "protected",
  "witness",
  "lock_args"
]) {
  if (!grammarSource.includes(keyword)) {
    throw new Error(`grammar is missing current 0.13 keyword coverage: ${keyword}`);
  }
}

for (const snippet of [
  "where",
  "transition ${1:input}.${2:state}:",
  "create ${1:output} =",
  "protected ${2:cell}:",
  "witness ${4:arg}:",
  "read ${1:config}:",
  "std::cell::same_lock",
  "std::cell::preserve_lock",
  "std::cell::preserve_capacity",
  "std::lifecycle::transfer",
  "std::receipt::claim",
  "std::lifecycle::settle"
]) {
  if (!snippetsSource.includes(snippet)) {
    throw new Error(`snippets are missing current 0.13 syntax: ${snippet}`);
  }
}

for (const [name, snippet] of Object.entries(snippets)) {
  if (!/action/i.test(name)) {
    continue;
  }
  const body = Array.isArray(snippet.body) ? snippet.body.join("\n") : String(snippet.body || "");
  const actionLine = body.split(/\r?\n/).find((line) => line.trimStart().startsWith("action "));
  if (!body.includes("\nwhere\n") || (actionLine && actionLine.trimEnd().endsWith("{"))) {
    throw new Error(`action snippet '${name}' must use a where proof block, not a brace body`);
  }
}

if (snippetsSource.includes(": protected ") || snippetsSource.includes(": witness ") || snippetsSource.includes(": read_ref ")) {
  throw new Error("source qualifiers in action/lock snippets must be prefix qualifiers");
}

const extensionSource = fs.readFileSync(path.join(root, "extension.js"), "utf8");
const bundledExtension = fs.readFileSync(path.join(root, "dist/extension.js"), "utf8");
const vscodeIgnore = fs.readFileSync(path.join(root, ".vscodeignore"), "utf8");
for (const token of [
  "LanguageClient",
  "vscode-languageclient/node",
  "cellscript.compileCurrentFile",
  "cellscript.showMetadata",
  "cellscript.showConstraints",
  "cellscript.showProductionReport",
  "cellc",
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

for (const requiredReadmePhrase of [
  "where proof blocks",
  "transition input.state: A -> output.state: B",
  "create output = T",
  "read name: T"
]) {
  if (!readme.includes(requiredReadmePhrase)) {
    throw new Error(`extension README is missing 0.13 authoring guidance: ${requiredReadmePhrase}`);
  }
}

console.log("CellScript VS Code extension manifest is valid.");

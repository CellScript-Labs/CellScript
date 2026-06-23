const vscode = require("vscode");
const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const cp = require("node:child_process");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

const LANGUAGE_ID = "cellscript";
const OUTPUT_NAME = "CellScript";

/** @type {LanguageClient | undefined} */
let languageClient = undefined;

function activate(context) {
  const output = vscode.window.createOutputChannel(OUTPUT_NAME);
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);

  status.name = "CellScript";
  status.text = "$(check) CellScript";
  status.tooltip = "CellScript Language Server";
  status.show();

  context.subscriptions.push(output, status);

  // ---- Start the LSP language server ----
  startLanguageServer(context, output, status);

  // ---- CLI-backed commands (not covered by LSP) ----
  context.subscriptions.push(
    vscode.commands.registerCommand("cellscript.compileCurrentFile", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "compile");
      }
    }),
    vscode.commands.registerCommand("cellscript.showMetadata", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "metadata");
      }
    }),
    vscode.commands.registerCommand("cellscript.showConstraints", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "constraints");
      }
    }),
    vscode.commands.registerCommand("cellscript.showAbi", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runAbiReport(document, output, status);
      }
    }),
    vscode.commands.registerCommand("cellscript.showActionBuildPlan", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runActionBuildPlan(document, output, status);
      }
    }),
    vscode.commands.registerCommand("cellscript.generateTypescriptBuilder", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runGenerateTypescriptBuilder(document, output, status);
      }
    }),
    vscode.commands.registerCommand("cellscript.verifyPackage", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runPackageVerify(document, output, status);
      }
    }),
    vscode.commands.registerCommand("cellscript.verifyRegistry", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runRegistryVerify(document, output, status, false);
      }
    }),
    vscode.commands.registerCommand("cellscript.verifyLiveRegistry", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runRegistryVerify(document, output, status, true);
      }
    }),
    vscode.commands.registerCommand("cellscript.showBuilderAssumptions", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "builder-assumptions");
      }
    }),
    vscode.commands.registerCommand("cellscript.showTxTemplate", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "tx-template");
      }
    }),
    vscode.commands.registerCommand("cellscript.showDeployPlan", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "deploy-plan");
      }
    }),
    vscode.commands.registerCommand("cellscript.showProfile", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "profile");
      }
    }),
    vscode.commands.registerCommand("cellscript.generateAuditBundle", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runCompilerReport(document, output, status, "audit-bundle");
      }
    }),
    vscode.commands.registerCommand("cellscript.showProductionReport", async () => {
      const document = activeCellScriptDocument();
      if (document) {
        await runProductionReport(document, output, status);
      }
    })
  );
}

function deactivate() {
  if (languageClient) {
    return languageClient.stop();
  }
  return undefined;
}

// ============================================================================
// LSP Language Server
// ============================================================================

async function startLanguageServer(context, output, status) {
  const config = vscode.workspace.getConfiguration("cellscript");
  const serverPath = resolveServerPath(config, output);

  if (!serverPath) {
    status.text = "$(warning) CellScript";
    status.tooltip = "CellScript compiler not found. Configure cellscript.compilerPath or install cellc.";
    output.appendLine("[cellscript] No cellc binary found for language server.");
    return;
  }

  const serverOptions = {
    command: serverPath.command,
    args: [...serverPath.args, "--lsp"],
    transport: TransportKind.stdio,
    options: {
      cwd: serverPath.cwd
    }
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: LANGUAGE_ID }],
    outputChannel: output,
    synchronize: {
      configurationSection: "cellscript"
    }
  };

  languageClient = new LanguageClient(
    LANGUAGE_ID,
    "CellScript Language Server",
    serverOptions,
    clientOptions
  );

  try {
    await languageClient.start();
    status.text = "$(check) CellScript";
    status.tooltip = "CellScript Language Server active";
    output.appendLine("[cellscript] Language server started successfully.");
  } catch (error) {
    status.text = "$(error) CellScript";
    status.tooltip = "CellScript Language Server failed to start";
    output.appendLine(`[cellscript] Language server failed: ${error}`);
  }
}

function resolveServerPath(config, output) {
  const compilerPath = config.get("compilerPath", "cellc");

  // Direct cellc path.
  if (compilerPath) {
    try {
      cp.execFileSync(compilerPath, ["--version"], { timeout: 5000 });
      return { command: compilerPath, args: [], cwd: undefined };
    } catch {
      // Fall through to cargo fallback.
    }
  }

  // Cargo fallback.
  if (canUseCargoRunFallback(config)) {
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (workspaceFolder) {
      const cwd = workspaceFolder.uri.fsPath;
      const cargoToml = findCargoWorkspace(cwd);
      if (cargoToml) {
        const manifestPath = path.join(cargoToml, "Cargo.toml");
        const args = ["run", "-q", "--manifest-path", manifestPath, "-p", "cellscript", "--"];
        try {
          cp.execFileSync("cargo", [...args, "--version"], { cwd: cargoToml, timeout: 15000 });
          return { command: "cargo", args, cwd: cargoToml };
        } catch {
          // Fall through.
        }
      }
    }
  } else if (config.get("useCargoRunFallback", true)) {
    output.appendLine("[cellscript] Cargo fallback is disabled until this workspace is trusted.");
  }

  return null;
}

function canUseCargoRunFallback(config) {
  return config.get("useCargoRunFallback", true) && vscode.workspace.isTrusted === true;
}

// ============================================================================
// CLI-backed report commands (kept for compile, metadata, constraints, production report)
// ============================================================================

function activeCellScriptDocument() {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== LANGUAGE_ID) {
    vscode.window.showWarningMessage("Open a .cell file first.");
    return null;
  }
  return editor.document;
}

async function resolveCompilerCommand(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  const compilerPath = config.get("compilerPath", "cellc");
  const options = {
    timeout: Math.max(config.get("commandTimeoutMs", 15000), 1000),
    maxBuffer: Math.max(config.get("maxOutputBytes", 4 * 1024 * 1024), 64 * 1024)
  };

  if (compilerPath && (await canExecute(compilerPath, ["--version"], undefined, options))) {
    return { command: compilerPath, args: [], cwd: undefined, options };
  }

  if (!canUseCargoRunFallback(config)) {
    return null;
  }

  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);
  const cwd = workspaceFolder ? workspaceFolder.uri.fsPath : path.dirname(document.uri.fsPath);
  const cargoToml = findCargoWorkspace(cwd);
  if (cargoToml) {
    const manifestPath = path.join(cargoToml, "Cargo.toml");
    const args = ["run", "-q", "--manifest-path", manifestPath, "-p", "cellscript", "--"];
    if (await canExecute("cargo", [...args, "--version"], cargoToml, options)) {
      return { command: "cargo", args, cwd: cargoToml, options };
    }
  }

  return null;
}

function findCargoWorkspace(startDir) {
  let current = startDir;
  while (current && current !== path.dirname(current)) {
    if (fs.existsSync(path.join(current, "Cargo.toml"))) {
      return current;
    }
    current = path.dirname(current);
  }
  return null;
}

function canExecute(command, args, cwd, options) {
  return new Promise((resolve) => {
    cp.execFile(command, args, { cwd, timeout: options.timeout, maxBuffer: options.maxBuffer }, (error) => {
      resolve(!error);
    });
  });
}

function runCommand(command, args, cwd, options) {
  return new Promise((resolve) => {
    cp.execFile(command, args, { cwd, timeout: options.timeout, maxBuffer: options.maxBuffer }, (error, stdout, stderr) => {
      resolve({
        code: error && typeof error.code === "number" ? error.code : error ? 1 : 0,
        stdout: stdout || "",
        stderr: stderr || ""
      });
    });
  });
}

async function runSingleCellcCommand(document, output, status, plan) {
  const command = await resolveCompilerCommand(document);
  if (!command) {
    vscode.window.showErrorMessage("CellScript compiler not found. Configure cellscript.compilerPath or install cellc.");
    return;
  }

  const cwd = plan.cwd || command.cwd;
  updateStatus(status, "running", plan.source);
  const result = await runCommand(command.command, [...command.args, ...plan.args], cwd, command.options);

  output.clear();
  output.appendLine(`[cellscript] ${plan.source}`);
  output.appendLine(`$ ${command.command} ${[...command.args, ...plan.args].join(" ")}`);
  if (cwd) {
    output.appendLine(`cwd: ${cwd}`);
  }
  if (plan.input) {
    output.appendLine(`input: ${plan.input}`);
  }
  if (plan.outputPath) {
    output.appendLine(`output: ${plan.outputPath}`);
  }
  output.appendLine("");
  if (result.stdout.trim()) {
    output.appendLine(result.stdout.trimEnd());
  }
  if (result.stderr.trim()) {
    output.appendLine(result.stderr.trimEnd());
  }
  output.show(true);

  if (result.code === 0) {
    updateStatus(status, "ok", plan.source);
    if (plan.successMessage) {
      vscode.window.showInformationMessage(plan.successMessage);
    }
  } else {
    updateStatus(status, "error", plan.source);
    vscode.window.showErrorMessage(`${plan.errorMessage || plan.source} failed. See the CellScript output channel.`);
  }
}

async function runCompilerReport(document, output, status, kind) {
  const command = await resolveCompilerCommand(document);
  if (!command) {
    vscode.window.showErrorMessage("CellScript compiler not found. Configure cellscript.compilerPath or install cellc.");
    return;
  }

  const plan = buildReportPlan(document, kind, command.cwd);
  updateStatus(status, "running", plan.source);
  const result = await runCommand(command.command, [...command.args, ...plan.args], command.cwd, command.options);

  output.clear();
  output.appendLine(`[cellscript] ${plan.source}`);
  output.appendLine(`$ ${command.command} ${[...command.args, ...plan.args].join(" ")}`);
  output.appendLine("");
  if (result.stdout.trim()) {
    output.appendLine(result.stdout.trimEnd());
  }
  if (result.stderr.trim()) {
    output.appendLine(result.stderr.trimEnd());
  }
  output.show(true);

  if (result.code === 0) {
    updateStatus(status, "ok", plan.source);
  } else {
    updateStatus(status, "error", plan.source);
    vscode.window.showErrorMessage(`CellScript ${kind} failed. See the CellScript output channel.`);
  }
}

async function runAbiReport(document, output, status) {
  const input = packageInputOrDocument(document);
  const entry = await selectMetadataEntry(document, output, status, input, {
    source: "cellc metadata for ABI entry selection",
    includeLocks: true,
    placeHolder: "Select the action or lock entry for the witness ABI"
  });
  if (!entry) {
    return;
  }

  const entryArgs = entry.kind === "lock" ? ["--lock", entry.name] : ["--action", entry.name];
  await runSingleCellcCommand(document, output, status, {
    source: "cellc abi",
    args: ["abi", input, ...targetProfileArgs(document), ...entryArgs],
    input,
    errorMessage: "CellScript ABI report"
  });
}

async function runActionBuildPlan(document, output, status) {
  const input = packageInputOrDocument(document);
  const entry = await selectMetadataEntry(document, output, status, input, {
    source: "cellc metadata for action build selection",
    includeLocks: false,
    placeHolder: "Select the action for the build plan"
  });
  if (!entry) {
    return;
  }

  await runSingleCellcCommand(document, output, status, {
    source: "cellc action build",
    args: ["action", "build", input, ...targetProfileArgs(document), "--action", entry.name, "--json"],
    input,
    errorMessage: "CellScript action build plan"
  });
}

async function runGenerateTypescriptBuilder(document, output, status) {
  const packageRoot = findPackageRootForDocument(document);
  const cwd = packageRoot || undefined;
  const input = packageRoot ? "." : document.uri.fsPath;
  const outputPath = builderOutputDir(document, packageRoot);
  const args = [
    "gen-builder",
    input,
    "--target",
    "typescript",
    "--output",
    outputPath,
    "--target-profile",
    "ckb",
    "--json"
  ];

  if (packageRoot) {
    const lockfilePath = path.join(packageRoot, "Cell.lock");
    const deployedPath = path.join(packageRoot, "Deployed.toml");
    if (fs.existsSync(lockfilePath)) {
      args.push("--lockfile", lockfilePath);
      if (fs.existsSync(deployedPath)) {
        args.push("--deployed", deployedPath);
        const network = deploymentNetwork(document);
        if (network) {
          args.push("--deployment-network", network);
        }
      }
    }
  }

  await runSingleCellcCommand(document, output, status, {
    source: "cellc gen-builder",
    args,
    cwd,
    input: packageRoot || document.uri.fsPath,
    outputPath,
    successMessage: `CellScript TypeScript builder generated at ${outputPath}`,
    errorMessage: "CellScript TypeScript builder generation"
  });
}

async function runPackageVerify(document, output, status) {
  const packageRoot = requirePackageRoot(document);
  if (!packageRoot) {
    return;
  }
  await runSingleCellcCommand(document, output, status, {
    source: "cellc package verify",
    args: ["package", "verify", "--json"],
    cwd: packageRoot,
    input: packageRoot,
    errorMessage: "CellScript package verification"
  });
}

async function runRegistryVerify(document, output, status, live) {
  const packageRoot = requirePackageRoot(document);
  if (!packageRoot) {
    return;
  }
  const args = ["registry", "verify", "--json"];
  if (live) {
    args.push("--live");
    const rpcUrl = ckbRpcUrl(document);
    if (rpcUrl) {
      args.push("--rpc-url", rpcUrl);
    }
    const network = deploymentNetwork(document);
    if (network) {
      args.push("--network", network);
    }
  }
  if (registryRequirePublisherSignature(document)) {
    args.push("--require-publisher-signature");
  }
  if (registryRequireAuditReport(document)) {
    args.push("--require-audit-report");
  }

  await runSingleCellcCommand(document, output, status, {
    source: live ? "cellc registry verify --live" : "cellc registry verify",
    args,
    cwd: packageRoot,
    input: packageRoot,
    errorMessage: live ? "CellScript live registry verification" : "CellScript registry verification"
  });
}

async function selectMetadataEntry(document, output, status, input, options) {
  const metadata = await loadMetadataForSelection(document, output, status, input, options.source);
  if (!metadata) {
    return null;
  }

  const entries = [];
  for (const action of metadata.actions || []) {
    entries.push({
      label: `action: ${action.name}`,
      description: `${(action.params || []).length} params`,
      kind: "action",
      name: action.name
    });
  }

  if (options.includeLocks) {
    for (const lock of metadata.locks || []) {
      entries.push({
        label: `lock: ${lock.name}`,
        description: `${(lock.params || []).length} params`,
        kind: "lock",
        name: lock.name
      });
    }
  }

  if (entries.length === 0) {
    vscode.window.showErrorMessage(options.includeLocks ? "No CellScript action or lock entries found." : "No CellScript actions found.");
    return null;
  }

  if (entries.length === 1) {
    return entries[0];
  }

  return vscode.window.showQuickPick(entries, {
    placeHolder: options.placeHolder,
    matchOnDescription: true
  });
}

async function loadMetadataForSelection(document, output, status, input, source) {
  const command = await resolveCompilerCommand(document);
  if (!command) {
    vscode.window.showErrorMessage("CellScript compiler not found. Configure cellscript.compilerPath or install cellc.");
    return null;
  }

  const args = ["metadata", input, ...targetProfileArgs(document)];
  updateStatus(status, "running", source);
  const result = await runCommand(command.command, [...command.args, ...args], command.cwd, command.options);
  if (result.code !== 0) {
    output.clear();
    output.appendLine(`[cellscript] ${source}`);
    output.appendLine(`$ ${command.command} ${[...command.args, ...args].join(" ")}`);
    output.appendLine("");
    if (result.stdout.trim()) {
      output.appendLine(result.stdout.trimEnd());
    }
    if (result.stderr.trim()) {
      output.appendLine(result.stderr.trimEnd());
    }
    output.show(true);
    updateStatus(status, "error", source);
    vscode.window.showErrorMessage("CellScript metadata entry selection failed. See the CellScript output channel.");
    return null;
  }

  try {
    updateStatus(status, "ok", source);
    return JSON.parse(result.stdout);
  } catch (error) {
    output.clear();
    output.appendLine(`[cellscript] ${source}`);
    output.appendLine("Failed to parse metadata JSON.");
    output.appendLine(String(error));
    output.show(true);
    updateStatus(status, "error", source);
    vscode.window.showErrorMessage("CellScript metadata JSON could not be parsed. See the CellScript output channel.");
    return null;
  }
}

async function runProductionReport(document, output, status) {
  const command = await resolveCompilerCommand(document);
  if (!command) {
    vscode.window.showErrorMessage("CellScript compiler not found. Configure cellscript.compilerPath or install cellc.");
    return;
  }

  const metadataPlan = buildReportPlan(document, "metadata", command.cwd);
  const constraintsPlan = buildReportPlan(document, "constraints", command.cwd);
  updateStatus(status, "running", "cellc production report");

  const versionResult = await runCommand(command.command, [...command.args, "--version"], command.cwd, command.options);
  const metadataResult = await runCommand(command.command, [...command.args, ...metadataPlan.args], command.cwd, command.options);
  const constraintsResult = await runCommand(command.command, [...command.args, ...constraintsPlan.args], command.cwd, command.options);

  output.clear();
  output.appendLine("[cellscript] production report");
  output.appendLine(`source: ${document.uri.fsPath}`);
  output.appendLine(`target args: ${targetProfileArgs(document).join(" ") || "(default)"}`);
  output.appendLine("");
  appendCommandSection(output, "Compiler Version", command, ["--version"], versionResult);
  appendCommandSection(output, "Artifact Metadata", command, metadataPlan.args, metadataResult);
  appendCommandSection(output, "Constraints", command, constraintsPlan.args, constraintsResult);
  output.appendLine("## Release Audit Boundary");
  output.appendLine("- Verify artifact metadata, compiler version pin, schema hash, constraints hash, and build provenance from the JSON above.");
  output.appendLine("- Audit signatures are release artifacts produced by the release process; this extension displays compiler evidence but does not sign artifacts.");
  output.appendLine("- Chain production readiness still requires CKB acceptance gates and builder-generated transactions.");
  output.show(true);

  if (versionResult.code === 0 && metadataResult.code === 0 && constraintsResult.code === 0) {
    updateStatus(status, "ok", "cellc production report");
  } else {
    updateStatus(status, "error", "cellc production report");
    vscode.window.showErrorMessage("CellScript production report failed. See the CellScript output channel.");
  }
}

function appendCommandSection(output, title, command, args, result) {
  output.appendLine(`## ${title}`);
  output.appendLine(`$ ${command.command} ${[...command.args, ...args].join(" ")}`);
  output.appendLine(`exit_code: ${result.code}`);
  if (result.stdout.trim()) {
    output.appendLine(result.stdout.trimEnd());
  }
  if (result.stderr.trim()) {
    output.appendLine(result.stderr.trimEnd());
  }
  output.appendLine("");
}

function buildReportPlan(document, kind, cwd) {
  if (kind === "metadata") {
    return {
      args: ["metadata", document.uri.fsPath, ...targetProfileArgs(document)],
      outputPath: null,
      source: "cellc metadata"
    };
  }

  if (kind === "constraints") {
    return {
      args: ["constraints", document.uri.fsPath, ...targetProfileArgs(document)],
      outputPath: null,
      source: "cellc constraints"
    };
  }

  if (kind === "builder-assumptions") {
    return {
      args: ["explain-assumptions", document.uri.fsPath, ...targetProfileArgs(document), "--json"],
      outputPath: null,
      source: "cellc explain-assumptions"
    };
  }

  if (kind === "tx-template") {
    return {
      args: ["solve-tx", document.uri.fsPath, ...targetProfileArgs(document), "--json"],
      outputPath: null,
      source: "cellc solve-tx"
    };
  }

  if (kind === "deploy-plan") {
    return {
      args: ["deploy-plan", document.uri.fsPath, ...targetProfileArgs(document), "--json"],
      outputPath: null,
      source: "cellc deploy-plan"
    };
  }

  if (kind === "profile") {
    return {
      args: ["profile", document.uri.fsPath, ...targetProfileArgs(document), "--json"],
      outputPath: null,
      source: "cellc profile"
    };
  }

  if (kind === "audit-bundle") {
    const outputPath = getScratchDirectoryPath(document, cwd, "audit-bundle");
    return {
      args: ["audit-bundle", document.uri.fsPath, ...targetProfileArgs(document), "--output", outputPath, "--json"],
      outputPath,
      source: "cellc audit-bundle"
    };
  }

  const target = compilerTarget(document);
  const outputPath = getScratchOutputPath(document, cwd, `compile.${targetFileExtension(target)}`);
  return {
    args: [document.uri.fsPath, ...targetProfileArgs(document), "-o", outputPath],
    outputPath,
    source: "cellc compile"
  };
}

function packageInputOrDocument(document) {
  return findPackageRootForDocument(document) || document.uri.fsPath;
}

function findPackageRootForDocument(document) {
  return findPackageRoot(path.dirname(document.uri.fsPath));
}

function requirePackageRoot(document) {
  const packageRoot = findPackageRootForDocument(document);
  if (!packageRoot) {
    vscode.window.showErrorMessage("CellScript package root not found. Open a .cell file inside a package whose Cell.toml has [package].");
    return null;
  }
  return packageRoot;
}

function findPackageRoot(startDir) {
  let current = startDir;
  while (current && current !== path.dirname(current)) {
    const manifestPath = path.join(current, "Cell.toml");
    if (fs.existsSync(manifestPath) && isPackageManifest(manifestPath)) {
      return current;
    }
    current = path.dirname(current);
  }
  return null;
}

function isPackageManifest(manifestPath) {
  try {
    const manifest = fs.readFileSync(manifestPath, "utf8");
    return /^\s*\[package\]\s*$/m.test(manifest) && /^\s*name\s*=/m.test(manifest);
  } catch {
    return false;
  }
}

function builderOutputDir(document, packageRoot) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  const configured = config.get("builderOutputDir", "target/cellscript-builder/typescript");
  const outputDir = configured && configured.trim() ? configured.trim() : "target/cellscript-builder/typescript";
  if (path.isAbsolute(outputDir)) {
    return outputDir;
  }

  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);
  const baseDir = packageRoot || workspaceFolder?.uri.fsPath || path.dirname(document.uri.fsPath);
  return path.join(baseDir, outputDir);
}

function ckbRpcUrl(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  const value = config.get("ckbRpcUrl", "");
  return value && value.trim() ? value.trim() : "";
}

function deploymentNetwork(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  const value = config.get("deploymentNetwork", "");
  return value && value.trim() ? value.trim() : "";
}

function registryRequirePublisherSignature(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  return config.get("registryRequirePublisherSignature", false) === true;
}

function registryRequireAuditReport(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  return config.get("registryRequireAuditReport", false) === true;
}

function compilerTarget(document) {
  const config = vscode.workspace.getConfiguration("cellscript", document.uri);
  return config.get("target", "riscv64-asm");
}

function targetProfileArgs(document) {
  const target = compilerTarget(document);
  const args = [];
  if (target) {
    args.push("--target", target);
  }
  args.push("--target-profile", "ckb");
  return args;
}

function targetFileExtension(target) {
  return target === "riscv64-elf" ? "elf" : "s";
}

function getScratchOutputPath(document, cwd, suffix) {
  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);
  const baseDir = workspaceFolder
    ? path.join(workspaceFolder.uri.fsPath, ".cellscript-vscode")
    : path.join(cwd || path.dirname(document.uri.fsPath) || os.tmpdir(), ".cellscript-vscode");

  fs.mkdirSync(baseDir, { recursive: true });
  const stem = path.basename(document.uri.fsPath, path.extname(document.uri.fsPath));
  return path.join(baseDir, `${stem}.${process.pid}.${Date.now()}.${suffix}`);
}

function getScratchDirectoryPath(document, cwd, suffix) {
  const workspaceFolder = vscode.workspace.getWorkspaceFolder(document.uri);
  const baseDir = workspaceFolder
    ? path.join(workspaceFolder.uri.fsPath, ".cellscript-vscode")
    : path.join(cwd || path.dirname(document.uri.fsPath) || os.tmpdir(), ".cellscript-vscode");

  fs.mkdirSync(baseDir, { recursive: true });
  const stem = path.basename(document.uri.fsPath, path.extname(document.uri.fsPath));
  return path.join(baseDir, `${stem}.${process.pid}.${Date.now()}.${suffix}`);
}

function updateStatus(status, state, detail) {
  if (state === "running") {
    status.text = "$(sync~spin) CellScript";
    status.tooltip = detail ? `Running ${detail}` : "CellScript is running";
    status.show();
    return;
  }

  if (state === "ok") {
    status.text = "$(check) CellScript";
    status.tooltip = detail ? `${detail} passed` : "CellScript validation passed";
    status.show();
    return;
  }

  if (state === "error") {
    status.text = "$(error) CellScript";
    status.tooltip = detail ? `${detail} failed` : "CellScript validation failed";
    status.show();
    return;
  }

  status.text = "$(check) CellScript";
  status.tooltip = "CellScript Language Server";
  status.show();
}

module.exports = { activate, deactivate };

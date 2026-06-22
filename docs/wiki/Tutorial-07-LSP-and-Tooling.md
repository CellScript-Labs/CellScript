You can write CellScript with any text editor and the `cellc` CLI. The LSP and
VS Code extension make that loop shorter. Parse errors, type errors,
flow mistakes, symbols, hovers, formatting, and compiler-backed reports
can show up while you work instead of after a long command sequence.

The useful thing to remember is that editor feedback is not a separate language
implementation. It is tied to the same parser, type checker, state-transition checks,
and lowering metadata used by `cellc`.

## What You Will Learn

- what the LSP server supports;
- how the VS Code extension starts the server;
- which settings matter for local development;
- where editor tooling helps;
- where release gates still need CLI and CKB evidence.

## LSP Capabilities

The LSP implementation supports the editor features you expect while writing a
contract:

- diagnostics for parse, type, flow, and lowering errors;
- hover information for actions, receipts, fields, local variables, flow
  states, and lowering metadata;
- keyword, type, symbol, field, local, enum variant, and qualified flow
  state completions such as `Ticket::Active`;
- go-to-definition;
- find-references;
- workspace rename with identifier-boundary checks;
- document symbols;
- document highlight;
- signature help;
- folding ranges;
- selection ranges;
- formatting;
- code actions for lowering diagnostics;
- incremental document sync using LSP UTF-16 positions.

Run the server over stdio:

```bash
cellc --lsp
```

In practice you usually let the editor start it for you.

## VS Code Extension

The extension lives in:

```text
editors/vscode-cellscript
```

Validate and package it locally:

```bash
cd editors/vscode-cellscript
npm install
npm run validate
npm run package
```

Install the generated `.vsix` in VS Code. If `cellc` is not on `PATH`, set
`cellscript.compilerPath`.

Useful settings:

| Setting | Purpose |
|---|---|
| `cellscript.compilerPath` | Path to the `cellc` binary used for LSP and CLI-backed commands. |
| `cellscript.useCargoRunFallback` | Use `cargo run -q -p cellscript --` from a trusted workspace when `cellc` is unavailable. |
| `cellscript.target` | Compiler target for command-backed reports: `riscv64-asm` or `riscv64-elf`. |
| `cellscript.commandTimeoutMs` | Timeout for compiler-backed commands. |
| `cellscript.builderOutputDir` | Output directory for generated TypeScript action-builder packages. Relative paths resolve from the nearest package `Cell.toml`. |
| `cellscript.ckbRpcUrl` | Optional CKB RPC URL for live registry verification. |
| `cellscript.deploymentNetwork` | Optional network filter for live registry verification and generated builder deployment binding. |
| `cellscript.registryRequirePublisherSignature` | Add `--require-publisher-signature` to registry verification commands. This is a metadata-presence gate, not cryptographic signature verification. |
| `cellscript.registryRequireAuditReport` | Add `--require-audit-report` to registry verification commands. |

The extension contributes commands for the local compiler and builder loop:

| Command | CLI boundary |
|---|---|
| `CellScript: Compile Current File` | `cellc <file>` |
| `CellScript: Show Metadata` | `cellc metadata` |
| `CellScript: Show Constraints` | `cellc constraints` |
| `CellScript: Show Entry Witness ABI` | selects an action/lock, then runs `cellc abi` |
| `CellScript: Show Action Build Plan` | selects an action, then runs `cellc action build --json` |
| `CellScript: Generate TypeScript Action Builder` | `cellc gen-builder --target typescript` |
| `CellScript: Verify Package` | `cellc package verify --json` |
| `CellScript: Verify Registry` | `cellc registry verify --json` |
| `CellScript: Verify Live Registry` | `cellc registry verify --live --json` |
| `CellScript: Show Production Report` | compiler version + metadata + constraints + release-audit boundary |

`CellScript: Show Production Report` is useful while editing because it displays
compiler version, metadata, constraints, and release-audit boundaries.

That report is a guide, not a deployment certificate. Chain acceptance still
requires CLI evidence and builder-backed CKB transactions.

Generated builder packages are local artifacts. After using
`CellScript: Generate TypeScript Action Builder`, run the generated package's
own checks before treating it as usable transaction-building evidence. This is
the generated package's `npm test` boundary:

```bash
npm --prefix target/cellscript-builder/typescript install --ignore-scripts
npm --prefix target/cellscript-builder/typescript test
```

The generated tests prove the TypeScript package compiles, plans actions,
delegates live-cell resolution/build/dry-run/submit to the runtime adapter, and
fails closed on mismatched lockfile or deployment identity. They do not prove
wallet signing, CKB node acceptance, or committed stateful flows.

## A Comfortable Local Loop

While editing, let the LSP catch small mistakes quickly. Before committing, run
the CLI checks explicitly:

```bash
cellc fmt --check
cellc check --all-targets --json
cellc metadata . --target riscv64-elf --target-profile ckb -o /tmp/metadata.json
cellc build --target riscv64-elf --target-profile ckb --json
cellc verify-artifact build/main.elf --verify-sources --expect-target-profile ckb
cellc package verify --json
cellc registry verify --json
```

For trust metadata review, add the explicit presence gate:

```bash
cellc registry verify --require-publisher-signature --require-audit-report --json
```

Run these from a package directory that contains `Cell.toml`. The `.` argument
refers to the current package; for a single file, pass the file path instead.

For CKB admission, keep the profile visible:

```bash
cellc check --target-profile ckb --json
cellc build --target riscv64-elf --target-profile ckb --json
cellc verify-artifact build/main.elf --expect-target-profile ckb
cellc registry verify --live --rpc-url "$CELLSCRIPT_CKB_RPC_URL" --json
cellc action build . --action mint --target-profile ckb --fabric-intent --json
cellc gen-builder . --target typescript --output target/cellscript-builder/typescript --target-profile ckb --json
npm --prefix target/cellscript-builder/typescript install --ignore-scripts
npm --prefix target/cellscript-builder/typescript test
```

This loop gives fast feedback first, then more formal evidence as the contract
gets closer to review.

## Formatting

Apply formatting:

```bash
cellc fmt
```

Check formatting without changing files:

```bash
cellc fmt --check
```

The formatter is especially useful after applying field shorthand or cleaning up
example code. It keeps the source style consistent without turning style into a
manual review topic.

## Generated Documentation

Generate package docs:

```bash
cellc doc
```

With JSON summary:

```bash
cellc doc --json
```

Documentation output includes the public contract surface and metadata-derived
lowering information.

## Local Package Workflow

The package manager supports:

- `cellc init`
- `cellc build`
- `cellc check`
- `cellc fmt`
- `cellc doc`
- `cellc add --path`
- `cellc remove`
- `cellc info`
- `cellc package verify`
- `cellc registry verify`
- lockfile consistency checks for local dependencies

Use the top-level `cellc path/to/file.cell` form for one-off file compilation.
Use `cellc build` for package builds.

Local `cellc install --path`, registry source-package `cellc install`, and
`cellc update` are supported lockfile workflows for packages that can be
resolved and source-hash verified. `cellc publish` and `cellc registry add`
prepare local Git-backed registry records; commit/push/PR remains outside the
editor. Treat `login`, `run`, registry proxy use, cryptographic publisher
signature verification, and non-CellScript artifact profiles as future-facing or
fail-closed.

## Next

With the tooling loop in place, continue with
[Bundled Example Contracts](https://github.com/a19q3/CellScript/wiki/Tutorial-08-Bundled-Example-Contracts).

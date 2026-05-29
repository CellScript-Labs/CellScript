# CellScript VS Code Extension

Production-grade VS Code tooling for `.cell` contracts, powered by a
CellScript Language Server (`cellc --lsp`).

The extension connects to a `cellc` binary running as a JSON-RPC language
server over stdio. This provides real-time diagnostics, completion, hover,
go-to-definition, find-references, signature help, document highlighting,
folding, formatting, code actions, and document symbols — all backed by the
CellScript compiler's parser, type-checker, and lowering pipeline.

CLI-backed commands continue to spawn `cellc` directly for one-shot operations
that are outside the LSP scope, including the active-file 0.16 builder,
transaction-template, deployment, profile, and audit-bundle reports.

## Features

### LSP-powered (via `cellc --lsp`)

- real-time diagnostics on open / edit / save with incremental sync
- context-aware completion (keywords, types, user symbols, fields, locals)
- hover information (types, lowering metadata, lifecycle states)
- go-to-definition (top-level symbols, fields, local variables, cross-module)
- find-references (lexer-accurate, skips comments and strings)
- signature help (action, function, lock parameters)
- document highlight
- folding ranges
- selection ranges
- document symbols
- code actions (lowering diagnostics quickfix)
- document formatting

### CLI-backed

- compile to a scratch artifact for the configured RISC-V target
- `cellc metadata` JSON report
- `cellc constraints` JSON report
- `cellc explain-assumptions --json` builder-assumption report
- `cellc solve-tx --json` deterministic transaction-template report
- `cellc deploy-plan --json` deployment-plan report
- `cellc profile --json` metadata-level profile report
- `cellc audit-bundle --json` generation into `.cellscript-vscode`
- production report (version + metadata + constraints)
- CKB target-profile arguments for compiler-backed reports

### Editor basics

- `.cell` file association
- TextMate syntax highlighting for the current 0.16 authoring surface (`where`
  proof blocks, `transition input.state: A -> output.state: B`, `flow`, named
  output `create out = T { ... }`, and source qualifiers such as `read`,
  `protected`, `witness`, and `lock_args`)
- comment, bracket, auto-close, and folding configuration
- snippets for resources, shared state, receipts, flows, action proof blocks,
  field-to-field state transitions, locks, source-qualified parameters, effects,
  named output `create ... = ... with_lock`, anonymous `require` blocks,
  `preserve` blocks, and stdlib lifecycle/cell metadata helpers
- 0.14 lock-boundary snippets and highlighting for `protected`, `lock_args`,
  `witness`, `require`, `source::*`, `witness::*`, and `env::sighash_all`
- identity, destruction-policy, and aggregate-invariant snippets for
  `identity`, `create_unique`, `replace_unique`, `destroy_unique`,
  `burn_amount`, `assert_sum`, `assert_delta`, `assert_distinct`, and
  `assert_singleton`
- status bar state indicator

## Authoring Surface

The extension snippets and grammar follow the signature-direction action
surface:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.price == input.price
    require output.seller == input.seller
```

Use where proof blocks for action proof logic; do not use the old brace-body
action form.

At action and lock boundaries, source qualifiers are written before the
parameter name:

```cellscript
action grant(read config: Config, tokens: Token) -> grant: Grant
where
    create grant = Grant { admin: config.admin }

lock owner_only(protected cell: Wallet, witness owner: Address) -> bool {
    require owner == cell.owner
}
```

`create output = T { ... }` constrains a named proposed output Cell. It is
not runtime allocation. Expression-level `read_ref<T>()` still exists for
lower-level reference reads, but action-boundary read-only Cell parameters
should use `read name: T`.

0.16 identity-aware lifecycle forms are also exposed through snippets and
highlighting:

```cellscript
#[type_id("cellscript::token::Token:v1")]
resource Token has store, create, consume, replace, burn, relock {
    identity(ckb_type_id)
    amount: u64
}

let minted = create_unique<Token>(identity = ckb_type_id) {
    amount: 1
}
```

## Architecture

```
VS Code ──(LanguageClient)──> cellc --lsp ──(JSON-RPC)──> CellScriptBackend
```

The `CellScriptBackend` in `server.rs` wraps the in-process `LspServer` and
implements the `tower_lsp::LanguageServer` trait. Document changes use
incremental sync; diagnostics are pushed automatically after each
open/change event.

## Requirements

Install `cellc` and make it available on `PATH`, or set
`cellscript.compilerPath` to the full compiler path.

When developing inside the CellScript Rust workspace, the extension can
fall back to this command after the workspace is trusted:

```bash
cargo run -q -p cellscript --
```

Set `cellscript.useCargoRunFallback` to `false` to disable that fallback.

## Commands

| Command | Purpose |
|---|---|
| `CellScript: Compile Current File` | Compile the active file to a scratch RISC-V assembly artifact and print compiler output. |
| `CellScript: Show Metadata` | Run `cellc metadata` for the active file and show JSON in the CellScript output channel. |
| `CellScript: Show Constraints` | Run `cellc constraints` for the active file and show JSON in the CellScript output channel. |
| `CellScript: Show Builder Assumptions` | Run `cellc explain-assumptions --json` for the active file. |
| `CellScript: Show Transaction Template` | Run `cellc solve-tx --json` for the active file. |
| `CellScript: Show Deploy Plan` | Run `cellc deploy-plan --json` for the active file. |
| `CellScript: Show Profile` | Run `cellc profile --json` for the active file. |
| `CellScript: Generate Audit Bundle` | Run `cellc audit-bundle --json` for the active file and write the bundle under `.cellscript-vscode`. |
| `CellScript: Show Production Report` | Show compiler version, artifact metadata, constraints, and release audit boundaries for the active file. |

Diagnostics, completion, hover, go-to-definition, references, formatting,
signature help, folding, and code actions are provided automatically by the
language server — no explicit commands needed.

## Settings

| Setting | Default | Description |
|---|---:|---|
| `cellscript.compilerPath` | `cellc` | Compiler binary used for the language server and CLI commands. |
| `cellscript.useCargoRunFallback` | `true` | Use workspace `cargo run -q -p cellscript --` if `cellc` is unavailable and the workspace is trusted. |
| `cellscript.commandTimeoutMs` | `15000` | Timeout for compiler-backed CLI commands. |
| `cellscript.maxOutputBytes` | `4194304` | Captured stdout/stderr limit. |
| `cellscript.target` | `riscv64-asm` | Compiler target for active-file compiler reports. |

## Local Validation

```bash
cd editors/vscode-cellscript
npm run validate
```

The validation script checks the extension manifest, grammar, snippets,
language configuration, commands, settings, and runtime wiring.

## Packaging

```bash
cd editors/vscode-cellscript
npm run package
npm run publish:dry-run
```

`npm run publish:dry-run` builds the extension and writes a disposable VSIX to
`/tmp/cellscript-vscode-dry-run.vsix`; it does not contact the Marketplace.
Generated `.vsix` files are ignored by git and excluded from packaged source
archives.

## Release Review Checklist

For production release review, use `CellScript: Show Production Report` and
check the JSON/prose output for:

- compiler version pin;
- artifact metadata and artifact hash;
- schema hash and ABI/schema metadata;
- constraints hash or constraints JSON saved by the build;
- build provenance and source hash fields;
- builder assumptions, transaction template, deploy plan, profile report, and audit bundle paths when those active-file commands are used;
- target profile and entry-action/entry-lock scope;
- CKB capacity/cycle limits;
- external audit signatures attached by the release process.

The extension displays compiler evidence. It does not create audit signatures,
publish packages, deploy code cells, or replace CKB acceptance gates.
Commands that require extra files, such as `validate-tx`, `trace-tx`,
`proof-diff`, `verify-deploy`, `diff-deploy`, and `lock-deps`, remain CLI-first
tools.

## Scope

The extension is a stable local editor integration. It is not a debugger, and
it does not replace release gates such as `cargo test`, `cargo clippy`,
`cellc check --production`, or chain acceptance scripts.

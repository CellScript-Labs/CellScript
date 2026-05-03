# Changelog

## Unreleased

- Updated lock snippets for the 0.14 lock-boundary surface:
  `protected`, `lock_args`, `witness`, and `require`.
- Added LSP completions for `lock_args`, CKB source views, witness fields,
  `env::sighash_all`, and CKB epoch/since helpers.
- Extended syntax highlighting for `source::`, `witness::`, `ckb::`, and
  nested `std::...` namespace builtins.

## 0.13.2

- Updated extension package metadata for the CellScript 0.13.2 release.
- Added release-blocking validation for stdlib lifecycle and cell metadata
  snippets: `std::lifecycle::transfer`, `std::receipt::claim`,
  `std::lifecycle::settle`, `std::cell::same_lock`,
  `std::cell::preserve_lock`, and `std::cell::preserve_capacity`.
- Highlight `std::...` namespaces as builtin support namespaces.
- Fixed the publish dry-run script so it performs a local VSIX packaging check
  with a pinned `@vscode/vsce` dependency instead of invoking an unsupported
  Marketplace publish flag.

## 0.13.0

- Updated extension package metadata for the CellScript 0.13 release.
- Added editor completion coverage for the 0.13 Vec helper surface.
- Updated TextMate grammar and snippets for the 0.13 action model:
  signature-direction outputs, `where` proof blocks, colon state moves,
  `flow`, named output `create out = T { ... }`, and prefix source
  qualifiers (`read`, `protected`, `witness`, `lock_args`).
- Tightened extension validation so stale brace-body action snippets and
  missing 0.13 syntax keywords are release blockers.

## 0.12.0

- Replaced direct CLI diagnostics with a full LSP language server integration
  (`cellc lsp --stdio`) using `vscode-languageclient`.
- LSP-powered features: real-time diagnostics (open/edit/save with incremental
  sync), context-aware completion, hover, go-to-definition, find-references,
  rename, signature help, document highlight, folding ranges, selection ranges,
  document symbols, code actions, and document formatting.
- CLI-backed commands continue to work for compile, metadata, constraints,
  production report, and CKB target-profile arguments.
- Updated extension architecture: VS Code → LanguageClient → `cellc lsp --stdio`
  → `CellScriptBackend` (tower-lsp) → in-process `LspServer`.
- Removed stale validation-mode and validation-debounce settings (diagnostics
  are now driven by the language server, not by CLI polling).
- Updated README to reflect the new LSP architecture.

## 0.11.0

- Promoted the extension from a thin syntax package to stable local editor
  tooling for CellScript authoring.
- Added compiler-backed commands for validation, scratch compilation, metadata,
  constraints, formatting, and target-profile arguments.
- Added `CellScript: Show Production Report`, which combines compiler version,
  artifact metadata, constraints, and release-audit boundary notes for the
  active `.cell` file.
- Documented the transport boundary: this extension is mature local
  compiler-backed tooling, not a standalone JSON-RPC/stdin language-server
  process.
- Added edit-time validation settings, command timeout/output limits, status
  bar feedback, command palette/context menu entries, and stricter manifest
  validation.
- Updated repository metadata to the standalone CellScript repository.

## 0.1.0

- initial CellScript VS Code language extension skeleton
- `.cell` file association
- TextMate syntax highlighting
- language configuration
- basic snippets

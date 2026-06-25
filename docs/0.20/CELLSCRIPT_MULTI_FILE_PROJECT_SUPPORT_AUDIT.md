# CellScript Multi-File Project Support Audit

> **Superseded.** This first-pass audit is intentionally preserved as review
> history, but its high-severity findings no longer describe the current
> worktree. Use
> [`CELLSCRIPT_MULTI_FILE_AUDIT_V2.md`](CELLSCRIPT_MULTI_FILE_AUDIT_V2.md)
> for the corrected status after the multi-file, LSP, WASM, playground, and
> NovaSeal evidence fixes.

Date: 2026-06-25
Repository: `/home/arthur/a19q3/CellScript`
Branch: `main`
HEAD: `35f691e`
Method: Read-only audit of the compiler, CLI, LSP, WASM playground surface,
package loader, resolver, current example/proposal packages, and focused
temporary repro projects. No source changes were made during the audit.

## Scope

This report consolidates two audit rounds:

1. Project-level support for multi-`.cell` packages, workspace builds,
   diagnostics, cache invalidation, LSP, and playground behavior.
2. Cross-`.cell` references, module/import resolution, `mod`-like project
   semantics, single-file compatibility impact, and whether real protocol
   packages such as iCKB, NovaSeal, and DOB-EVO need multi-file support.

The question under review is not whether a sibling `.cell` file can sometimes
be loaded. It can. The question is whether CellScript currently has a
production-ready project model where package files, imports, modules,
diagnostics, cache keys, and tooling agree on one fail-closed boundary.

## Executive Summary

CellScript currently has **partial entry-driven multi-file support**, not a
complete production-ready project model.

The compiler can load package source roots and local path dependencies into a
`ModuleResolver`, and basic imported type usage from another `.cell` file can
compile. However, several high-impact gaps remain:

- Explicit import paths are not enforced strongly enough. A wrong import path
  can still compile if another loaded module exposes a same-named type.
- Incremental cache keys only account for the entry source, not imported or
  dependency `.cell` files.
- `cellc build` and `cellc check` only compile the package entry artifact path;
  they do not guarantee every package `.cell` module is semantically valid.
- LSP and WASM playground diagnostics remain mostly single-source, so editor
  and browser behavior can disagree with package compilation.
- The language has no explicit Rust-like `mod` item. That is acceptable as a
  design choice, but the current implicit `source_roots` scanning model needs
  stricter validation and documentation before it can be called production
  ready.

Pure single-file projects are not the source of the risk. Most fixes can be
made without changing single-file behavior. The only likely compatibility
break is for code that accidentally relied on global basename fallback instead
of declaring a correct import path; that should be treated as a compiler bug,
not as a supported compatibility contract.

## Current Implementation Boundary

### What Works Today

- `compile_path` accepts a package directory or `Cell.toml`, resolves the
  configured entry, and compiles that entry.
- Package source roots are collected from `[package].source_roots`, defaulting
  to `src` when unset.
- The current entry module and other package/dependency modules are registered
  into a shared `ModuleResolver`.
- Basic imports such as `use demo::token::Token` can resolve across `.cell`
  files when the package root and source roots are configured correctly.
- Source metadata records all collected source units after a successful
  compile.
- Workspace `build` and `check` can iterate workspace members.

Relevant references:

- `src/lib.rs:4571-4610` compiles one entry file, builds a resolver, binds
  source metadata, and stores an incremental cache result.
- `src/lib.rs:4729-4777` collects package and local dependency source units.
- `src/lib.rs:4779-4797` builds the module resolver for the current entry.
- `src/lib.rs:4945-4978` registers other package/dependency `.cell` files.
- `src/lib.rs:15035-15105` collects package `.cell` files from `source_roots`,
  default `src`, and the explicit entry parent.
- `src/lib.rs:27869-27915` already tests configured source roots with a shared
  token module.

### What Does Not Yet Hold

The package boundary is not yet a single semantic unit. The compiler mainly
uses other `.cell` files as symbol sources for the entry module. That is useful,
but it is weaker than "this package is checked, cached, diagnosed, and built as
one coherent project."

## Findings

### F1. Wrong Import Paths Can Resolve Through Global Basename Fallback

Severity: HIGH

`ModuleResolver::process_import` records a full import path but does not
validate the target module or symbol at import registration time:

- `src/resolve/mod.rs:157-168`

Type resolution then checks local symbols, imported aliases, and finally a
global basename fallback:

- `src/resolve/mod.rs:171-195`
- `src/resolve/mod.rs:250-253`

The imported type path branch is also prefix-based:

```rust
for (mod_name, table) in &self.symbol_tables {
    if full_path.starts_with(mod_name) {
        return table.types.get(*type_name).cloned();
    }
}
```

This is unsafe for nested modules and prefix-related module names, and the
fallback can hide an incorrect import path.

Focused repro result:

- A package with `src/main.cell` containing `use missing::module::Token`
  compiled successfully when another loaded file defined `resource Token`.
- `cellc build --json` exited with status `0`.
- This means the declared import path was not enforced.

Impact:

- Reviewers may believe a package is importing a specific module while the
  compiler silently uses a same-named type from another loaded module.
- Protocol packages can accidentally couple to the wrong cell schema.
- Hashes and metadata may look valid while the source graph is not the graph
  the author wrote.

This aligns with an earlier local audit note in
`docs/0.20/compiler_swarm_audit.md:168-171`, which already flagged unresolved
import-target handling and nested module qualifier risk.

### F2. Resolver Has an Import/Module Validation Function That Is Not Wired

Severity: MEDIUM

`ModuleResolver::check_circular_deps` checks that imported target modules are
registered:

- `src/resolve/mod.rs:308-319`

Search results show this function is only defined; it is not called by the main
compile path:

- `src/resolve/mod.rs:308`
- `src/package/mod.rs:942` is a separate package-manager dependency cycle
  check, not resolver import validation.

Impact:

- A validation primitive exists, but compile does not use it to make imports
  fail closed.
- Missing module paths are likely to surface as later type errors, misleading
  diagnostics, or in the F1 case, no error at all.

### F3. Incremental Cache Ignores Imported And Dependency Source Files

Severity: HIGH

The entry compile path checks cache before collecting package source units:

- `src/lib.rs:4577-4584`

Cache hit and cache key are based on the entry file source string:

- `src/lib.rs:4617-4635`
- `src/lib.rs:4663-4675`
- `src/lib.rs:4689-4706`

The compiler later collects all package and dependency source units for
metadata:

- `src/lib.rs:4729-4777`

but those units are not included in the cache key.

Focused repro result:

- First build: entry imports `dep::types::Pair` from `src/types.cell`.
- Then only `src/types.cell` changed from `value: u64` to `nope: bool`.
- Second build exited with status `0`, returned `cache_hit: true`, and reused
  the old artifact hash.

Impact:

- A package can ship stale artifacts after a dependency `.cell` changes.
- This is a release-gate risk because metadata can claim a source set while the
  artifact came from an older source set.
- The current source-unit metadata is good evidence data, but the cache layer
  does not yet enforce it.

### F4. Package Build And Check Do Not Validate Every `.cell` Module

Severity: MEDIUM

`compile_file_with_entry_scope` parses and semantically checks the entry AST:

- `src/lib.rs:4571-4599`

Other package files are registered into the resolver, but their bodies are not
independently type/flow checked unless the entry path requires enough of their
symbols to trigger resolution.

Workspace commands also call `compile_path` per package member:

- `src/cli/commands.rs:771-929`
- `src/cli/commands.rs:1567-1660`

Focused repro result:

- A package entry `src/main.cell` was valid.
- An unreferenced `src/helper.cell` had an action returning `true` from a
  declared `u64` return type.
- `cellc build --json` exited with status `0`.
- `cellc check --json` also exited with status `0`.

Impact:

- `cellc check` does not mean "every source file in this package is valid."
- Registry/package publishing can include invalid `.cell` files that are not
  entry-reachable.
- This is especially problematic once packages contain multiple script entries,
  reusable schema modules, lifecycle modules, and proposal-local helper modules.

### F5. LSP Diagnostics Are Not Package-Aware

Severity: MEDIUM

LSP parsing currently runs diagnostics through the single-source metadata path:

- `src/lsp/mod.rs:188-213`

The server can scan workspace modules for navigation:

- `src/lsp/mod.rs:1793-1810`

but those workspace modules are not used by diagnostics.

Focused repro result:

- `src/main.cell` imported a type from `src/types.cell`.
- Opening only `main.cell` in the LSP produced diagnostics such as:
  `unknown type 'Token'`.
- The same package shape is expected to compile through the path-aware
  resolver.

Impact:

- Editor diagnostics can reject valid package code.
- Navigation and diagnostics can disagree inside the same language server.
- Playground/editor users will think cross-file imports are broken even when
  the CLI entry build succeeds.

### F6. WASM Playground API Is Single-Source

Severity: MEDIUM

The WASM API takes only one source string:

- `crates/cellscript-wasm/src/lib.rs:64-68`
- `crates/cellscript-wasm/src/lib.rs:83-90`
- `crates/cellscript-wasm/src/lib.rs:100-110`

The website worker uses those single-source exports:

- `website/public/playground-worker.js:42-44`

Impact:

- The browser playground cannot faithfully model a package with multiple
  `.cell` files.
- Cross-file diagnostics, source-unit hashes, and package-level metadata cannot
  be represented without an additive multi-file API.

### F7. Multi-Diagnostic Support Exists, But It Is Not The Same As Project Diagnostics

Severity: LOW/MEDIUM

The metadata diagnostics path can collect multiple independent type/flow
diagnostics:

- `src/lib.rs:4189-4231`
- `src/types/mod.rs:6760-6773`
- `src/types/mod.rs:6793-6858`

There is also a path-aware metadata diagnostics function:

- `src/lib.rs:4238-4335`

However, the normal build/check artifact path is still `Result`-based and
entry-centered, and project-wide diagnostics are not yet collected across all
package modules.

Impact:

- "Only one error displayed" is not because the type checker is inherently
  single-error. It can collect several errors in diagnostics mode.
- The limitation is routing: CLI build/check, LSP, WASM, and package-wide
  module iteration do not consistently use a project diagnostics pipeline.

### F8. The `mod` Model Is Implicit And Needs A Contract

Severity: MEDIUM

CellScript does not currently expose a Rust-like `mod foo;` syntax. The
effective module graph comes from:

- file discovery through package `source_roots`;
- each file's `module ...` declaration;
- `use path::to::Symbol` import statements;
- local path dependencies resolved by `PackageManager`.

That model can be good for CellScript, but it needs to become explicit in docs
and enforcement:

- What is the canonical mapping between package namespace, module declaration,
  and file path?
- Are multiple files allowed to declare the same module?
- Are unqualified names allowed to search globally?
- Does `cellc check` mean entry-only or all package modules?
- Is a package allowed to contain invalid non-entry `.cell` sources?

Until these are answered in code and docs, reviewers cannot treat a CellScript
package as a closed semantic unit.

## Single-File Compatibility Impact

The required fixes should have low risk for single-file projects:

- Cache source-set hashing can preserve current single-file behavior by making
  the source set contain only the entry file when no package/dependency files
  exist.
- Import validation does not affect pure local single-file symbols.
- Package-wide module checking only changes packages with multiple files. A
  one-file package should produce the same result.
- LSP and WASM multi-file APIs can be additive; existing single-source APIs can
  stay stable.

The main intentional break is code that only compiles because of global
basename fallback. For example, `use missing::module::Token` resolving to some
other loaded `Token` should stop compiling. That is not a valid compatibility
surface; it is a correctness bug.

## Protocol Need Assessment

### iCKB

Current local benchmark specs are flat single-file sources:

- `tests/benchmarks/ickb_specs/ickb_logic.cell`
- `tests/benchmarks/ickb_specs/limit_order.cell`
- `tests/benchmarks/ickb_specs/owned_owner.cell`

That shape is adequate for benchmark snapshots and isolated reasoning. For a
production iCKB-style protocol, cross-file support becomes practically useful
once the model includes shared receipt types, owner/position resources, order
flow helpers, reusable constants, and several entry scripts that need one
schema vocabulary.

Conclusion: iCKB does not strictly require multi-file support for the current
local benchmark files, but serious production packaging would benefit from it
and should not rely on copy-pasted single-file duplication.

### NovaSeal

NovaSeal already demonstrates real multi-file pressure. Local proposal
packages include multiple `.cell` files for state, receipt, lifecycle, lock,
and profile-specific transition surfaces:

- `proposals/novaseal/v0-mvp-skeleton/src/nova_state_type.cell`
- `proposals/novaseal/v0-mvp-skeleton/src/nova_state_lifecycle_type.cell`
- `proposals/novaseal/v0-mvp-skeleton/src/nova_receipt_type.cell`
- `proposals/novaseal/v0-mvp-skeleton/src/nova_btc_authority_lock.cell`
- `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_type.cell`
- `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell`
- `proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell`
- `proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell`

Several `Cell.toml` files also reference source actions or stateful dispatchers
in different source files.

Conclusion: NovaSeal needs robust cross-file package semantics. Its current
source organization is direct evidence that protocol packages want multiple
schema/action/lifecycle files under one reviewable package boundary.

### DOB-EVO

The local checkout contains an empty `proposals/evolving-dob/evolving-dob-profile-v1`
directory, so this audit could not inspect the current source package directly.
However, registry data and existing docs describe a DOB-EVO profile with an
evolving state type and actions such as initialise, evolve, and finalise:

- `website/src/data/registry-packages.json`
- `docs/0.20/CELLSCRIPT_0_20_DOB_EVO_SWARM_AUDIT.md`

The documented package shape includes `src/evolving_dob_type.cell`, schemas,
scripts, fixtures, and registry pressure tooling. A mature DOB-EVO package
would naturally split state schema, intent/event receipt schema, lifecycle
actions, policy constants, and registry/deployment evidence.

Conclusion: the local checkout is insufficient for direct source verification,
but the protocol shape strongly supports multi-file needs as soon as the
profile moves beyond one monolithic source file.

### General Protocol Reality

For small demos, single-file `.cell` programs are enough. For most serious CKB
protocol packages, multi-file support is a real requirement because protocols
naturally separate:

- resource/shared/receipt schemas;
- action and lifecycle dispatch;
- lock/type script entry points;
- shared constants and helper functions;
- schema bindings and generated metadata;
- registry/deployment evidence;
- test fixtures and package-local proof artifacts.

Without fail-closed cross-file support, developers either duplicate schema
definitions across files or hide important package structure inside a large
monolithic source file. Both options reduce auditability.

## Recommended Fix Plan

### Phase 1: Resolver Hardening

- Replace prefix-based imported type lookup with exact module path resolution
  using `full_path.rsplit_once("::")`.
- Validate every import after all modules are registered:
  - target module exists;
  - target symbol exists;
  - alias collisions are rejected;
  - duplicate module declarations are rejected deterministically.
- Remove or sharply limit global basename fallback. Unqualified lookup should
  prefer:
  1. local module symbols;
  2. explicitly imported aliases;
  3. built-in/core symbols.
- Add deterministic diagnostics with source file and span.

### Phase 2: Source Graph And Cache

- Build a deterministic project source graph before cache lookup.
- Include every source unit hash, manifest build fields, target profile,
  primitive mode, dependency lock/provenance data, and compiler/schema version
  in the cache key.
- Store and validate the source-unit set alongside cached metadata.
- Add a regression test where changing an imported `.cell` invalidates cache.

### Phase 3: Package-Wide Checking

- Define a `cellc check` contract:
  - default should check the package entry and every loaded package module;
  - output should report file-qualified diagnostics;
  - dependency modules may be checked as dependencies or under a separate
    `--include-deps` policy, but the behavior must be explicit.
- Keep `cellc build` artifact emission entry-scoped by default, but run enough
  package validation to prevent invalid package sources from being published
  silently.
- Add tests for invalid unreferenced package modules.

### Phase 4: Tooling Alignment

- Make LSP diagnostics path-aware when the file URI maps to a real workspace
  file.
- Add an additive WASM multi-file API, for example a JSON envelope of
  `{ path, source, role }` inputs with an entry path.
- Keep current single-source WASM APIs for backwards compatibility.
- Teach the playground to use the multi-file API only when examples or user
  sessions contain more than one source file.

### Phase 5: Documentation And Release Gates

- Document the module/file/package contract in the wiki and compiler reference:
  source roots, module declaration, imports, package entry, and check/build
  behavior.
- Add gate tests for:
  - missing import module fails;
  - missing import symbol fails;
  - prefix module names do not cross-resolve;
  - local dependency source changes invalidate cache;
  - unreferenced invalid `.cell` files fail package check;
  - single-file compile behavior remains stable.

## Bottom Line

CellScript is close to having a useful multi-file package model, but the current
implementation should be described as **partial and entry-driven**. It is not
yet safe to claim production-ready project support for cross-`.cell` references
or module imports.

The fix is not to abandon single-file simplicity. The right path is to keep
single-file ergonomics, then make packages fail closed: exact imports, full
source-set cache keys, package-wide checking, and aligned CLI/LSP/WASM
diagnostics.

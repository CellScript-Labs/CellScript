# CellScript Multi-Cell, Cross-Cell, and Mod-Reference Audit (v2)

Date: 2026-06-25
Repository: `/home/arthur/a19q3/CellScript`
Branch: `main`
HEAD: `35f691e`
Method: Initial read-only re-audit of the compiler, resolver, IR, codegen, CLI,
LSP, WASM, package loader, tests, and the iCKB / NovaSeal / DobEvo proposal
sources, followed by post-audit fixes in the working tree and local validation.

> This report **supersedes and corrects**
> `CELLSCRIPT_MULTI_FILE_PROJECT_SUPPORT_AUDIT.md` (same date/HEAD). That
> document's four highest-severity findings (F1 global basename fallback,
> F2 `validate_imports` unwired, F3 cache ignores imported files, F4 build/check
> do not validate every module) describe a state that **no longer matches the
> current code**. A focused re-read of `src/lib.rs` shows the main compile path
> was refactored to route through a validated, source-graph-aware project
> loader. The stale findings are itemised in [§5 Corrections](#5-corrections-to-the-prior-audit).
> This v2 focuses on what **genuinely holds today** plus new findings the prior
> audit missed.

## 0. TL;DR

| Question | Answer |
| --- | --- |
| Is multi-`.cell` project support complete? | **Complete for the entry-driven compile/check/cache/LSP/WASM path covered by 0.20.** The remaining boundary is any ELF-linker-style or cross-script runtime-linking claim, not core type/schema/helper resolution. |
| Cross-cell references? | **Type imports work end-to-end** (`use m::Type`, inlined into the entry IR/ELF). **Helper calls are inlined into the entry artifact** via imported aliases (`use m::f as g; g()`), unqualified imports (`use m::f; f()`), fully-qualified calls (`m::f()`), and transitive helper calls. Same-basename helpers from different modules get stable internal labels instead of basename fallback. |
| Mod references? | **No Rust-style `mod foo;` keyword exists** (by design). Modules come from file discovery + `module ...;` declarations + `use ...`. The post-audit docs now make this model explicit. |
| Impact on single-cell projects? | **None / positive.** The new project-diagnostics and source-graph cache only add work for multi-file projects. A single-file compile is unchanged; the cache degrades to the entry-only key. |
| Do iCKB / NovaSeal / DobEvo need cross-`.cell` business logic? | **No.** iCKB and DobEvo remain single-file/no-import in this checkout. NovaSeal fungible-xUDT now uses shared schema imports with live devnet evidence, but it still does not need cross-file business-logic linking. CKB scripts are independent RISC-V binaries that interact by runtime hash-matching + syscalls. |

## 1. How Multi-Cell Projects Work Today

### 1.1 Project loading (the real path)

`compile_file_with_entry_scope` (`src/lib.rs:4644`) no longer builds an ad-hoc
resolver. It calls `load_project_for_entry` (`src/lib.rs:4142`), which:

1. Loads **every** `.cell` file reachable from the entry's package root
   (`load_project_modules_for_entry`, `src/lib.rs:4152`) — including local path
   dependencies walked recursively (`local_dependency_roots`).
2. Builds a single `ModuleResolver` via
   `build_module_resolver_from_loaded_modules`, then runs project-aware import
   validation that attaches each import error to the source file that declared
   it — **every `use` target module must be registered, or the build fails
   closed**.
3. Returns a `LoadedProject { modules, resolver, entry_index }`.

The entry's package is discovered by `find_package_root`
(walks up for a `Cell.toml`). `source_roots` (default `src/`) plus the entry's
parent dir are scanned recursively; directories containing their own
`Cell.toml` are treated as nested packages and skipped unless declared as a
path dependency (`should_skip_cell_dir`, `src/lib.rs:15145`).

### 1.2 One artifact per entry; inlining, not linking

There is **no ELF linker**. A single `compile_file` produces one artifact.
Imported cross-file code is merged into one `IrModule` *before* codegen:

- `ir::generate_with_resolver` registers external type/function ABIs from
  `use` items, then `append_external_callable_bodies` (`src/ir/mod.rs:6044`)
  walks the **transitive imported-call closure** and copies external `IrItem`
  bodies into the entry module's `ir.items`. Codegen then emits everything into
  one ELF/ASM.
- Only reachable code is pulled in (dead-code elimination by default).
- Helper calls are normalized to stable local labels before codegen. Imported
  aliases keep their alias label (`use dep::math::add_one as plus_one;
  plus_one(x)`), while fully-qualified calls without an alias use a stable
  internal label derived from the owner module. Transitive helper calls are
  rewritten relative to the owner module, so dependency helpers can call their
  own private helpers without leaking `module::name` labels into assembly.

### 1.3 Workspace builds

`cellc build --workspace` / `--package` (`src/cli/commands.rs:771`) reads
`[workspace] members` (`resolve_workspace_members`, `src/lib.rs:5144`) and
compiles **each member independently to its own artifact**. There is no
cross-member linking; each member is a self-contained single-entry compile.

## 2. Cross-Cell Reference Audit

### 2.1 What works: cross-file type imports

Verified working end-to-end (parse → resolve → type-check → IR → codegen → ELF):

- `use dep::token::Token;` resolves across a path dependency
  (`tests/examples.rs:1228` compiles `launch` → `token`/`amm_pool` to ELF).
- Imported types are treated as linear where appropriate
  (`src/types/mod.rs:6859`).
- LSP navigation (goto-definition, references, rename) works across modules
  (`src/lsp/mod.rs:2925`, `:2946`, `:2969`).

### 2.2 Function-call support: inlined helpers

Imported, aliased, fully-qualified, and transitive helper calls are inlined:

```cell
use dep::math::add_one
use dep::math::add_two as plus_two

action run(x: u64) -> u64 {
    verification
        return add_one(x) + plus_two(x) + dep::math::add_three(x)
}
```

Regression coverage:

- `tests/cli.rs` now compiles `dep::math::add_one(x)` directly.
- Aliased imports (`use dep::math::add_one as plus_one`) compile and keep the
  alias as the emitted helper label.
- Two dependencies can both expose `add_one`; fully-qualified calls compile to
  separate stable labels (`__cellscript_ext_dep_a__math__add_one`,
  `__cellscript_ext_dep_b__math__add_one`) instead of falling back to a shared
  basename.
- External helpers can call other helpers in their owner module; the transitive
  call closure is copied into the same entry artifact.

> **Severity: FIXED.** This is compile-time inlining into one artifact, not ELF
> linking. The codegen fallback now treats any remaining `module::function`
> label as an IR-normalization bug.

### 2.3 Resolution is exact-path; global fallback is dead code

The dangerous "global basename fallback" the prior audit flagged (F1) is **no
longer reachable**:

- `resolve_type` / `resolve_function` / `resolve_constant`
  (`src/resolve/mod.rs:171`, `:191`, `:220`) resolve via exact
  `rsplit_once("::")` module path, then local symbol, then imported alias →
  exact target. **No global scan.**
- The basename-fallback variants `resolve_type_global` /
  `resolve_function_global[_with_module]` were removed during the post-audit
  fix pass. A wrong import path now fails at `validate_imports` instead of
  silently resolving to a same-named symbol in another module.

## 3. Mod-Reference Audit

- **There is no `mod foo;` keyword.** This is an intentional design choice.
  The module graph is implicit: file discovery (`source_roots` walk) +
  `module <path>;` declarations + `use <path>::Symbol;` imports +
  `PackageManager` path dependencies.
- `module` declarations support dotted paths (`module a::b::c;`), and `use`
  supports grouping (`use m::{A, B};`) and `as` aliasing.
- `validate_imports` (`src/resolve/mod.rs:304`) now enforces that every `use`
  target module is registered, and alias collisions are rejected
  (`src/resolve/mod.rs:397`, `:487`).
- **Open contract questions** (still need documentation, not code):
  - Is a package allowed to contain invalid non-entry `.cell` sources?
    (Answer now: **no** — see §5 F4 correction.)
  - Can multiple files declare the same module? (Duplicate-module detection
    exists: `src/lib.rs:27953`.)
  - Canonical mapping between package namespace, module declaration, and file
    path.

## 4. Protocol Need Assessment

### 4.1 iCKB — naturally single-file-per-script

- 3 benchmark specs, each fully self-contained, **0 `use` statements**:
  `tests/benchmarks/ickb_specs/{ickb_logic,limit_order,owned_owner}.cell`.
- iCKB has multiple lock/type scripts (Logic, Limit Order, Owned Owner, xUDT);
  each is a **separate on-chain cell with its own RISC-V binary**. The
  one-file-per-script mapping is architecturally correct.
- Cross-script interaction is runtime-only: CKB syscalls
  (`LOAD_CELL_BY_FIELD`, `LOAD_SCRIPT_HASH`) + 32-byte **code-hash matching**,
  never compiled code linkage.

### 4.2 NovaSeal — shared schema imports, zero cross-file *business logic*

- Most NovaSeal packages still keep 4+ `.cell` files per package (state_type,
  receipt_type, lifecycle, lock) as reviewable, independently compiled entries.
- The fungible-xUDT profile has now been refactored to use a real shared schema
  module:
  - `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_schema.cell`
    owns shared witness and commitment structs.
  - `nova_fungible_xudt_type.cell` and
    `nova_fungible_xudt_lifecycle_type.cell` import those types.
  - Live local devnet stateful evidence passed for issue, transfer, settle, and
    required negative cases for lifecycle data hash
    `0x394da78133cb2f5a5d6cd911feceeab9e97e6ad5d36c0e50f18be56653af85e5`.
- This is still schema/type reuse, not cross-file business-logic linking.
  Older NovaSeal entries often **re-declare types inline** with explicit
  comments:
  - `proposals/novaseal/v0-mvp-skeleton/src/nova_btc_authority_lock.cell:49-53`:
    *"These local type definitions exist only so this lock file can compile on
    its own during targeted review."*
  - `proposals/novaseal/v0-mvp-skeleton/src/nova_state_type.cell:20-23`: types
    inlined *"so the transition can compile as one self-contained entry."*
- Profile conformance is enforced by **compiler certification + schema hash
  pinning + live profile evidence**, not runtime cross-cell calls
  (`NOVASEAL_ARCHITECTURE_EXPLAINED.md:68-70`).

### 4.3 DobEvo — single-file protocol

- Sparse submodule checkout; registry data
  (`website/src/data/registry-packages.json:80`) confirms a **single**
  `src/evolving_dob_type.cell` with three actions (initialise/evolve/finalise)
  dispatched by opcode byte. One type script, one cell, one file.

### 4.4 General protocol reality

The CKB model makes cross-`.cell` **compiled** linkage semantically irrelevant:

| CKB concept | Implication |
| --- | --- |
| Each script = independent RISC-V binary | No shared compiled code between scripts |
| Deployed as separate code cells | Scripts reference each other by 32-byte code hash |
| Cross-cell data access via syscalls | Runtime (`LOAD_CELL_*`), not compile-time |
| CellDeps mechanism | Runtime dependency declaration, not compiled linkage |

> **Bottom line:** cross-`.cell`-file *business logic* is **not** a real CKB
> protocol need. What is real is the **organisational** desire to deduplicate
> shared schema/struct definitions for human review (NovaSeal's motivation).
> CellScript's current cross-file type/schema imports cover that need, and
> NovaSeal fungible-xUDT now demonstrates it with live local devnet evidence.
> The inlined-copy model is sufficient because each script compiles to one
> self-contained ELF. Cross-file helper calls now improve developer ergonomics
> while preserving that one-artifact boundary; an ELF linker would not improve
> on-chain correctness.

## 5. Corrections to the Prior Audit

The prior `CELLSCRIPT_MULTI_FILE_PROJECT_SUPPORT_AUDIT.md` listed eight
findings. Re-verification against `HEAD 35f691e`:

| Prior finding | Status now | Evidence |
| --- | --- | --- |
| **F1** Global basename fallback hides wrong imports | **FIXED** | `resolve_type_global`/`resolve_function_global*` were removed during the post-audit fix pass. Resolution is exact-path; `validate_imports` rejects unknown modules. |
| **F2** `validate_imports` defined but not wired | **FIXED** | The main path now validates imports while building `LoadedProject`; the post-audit implementation attaches import failures to the declaring module file for CLI/LSP/WASM routing. The old unvalidated `build_module_resolver` path is gone. |
| **F3** Cache ignores imported/dependency files | **FIXED** | `collect_cache_units_for_compile_file` (`src/lib.rs:4862`) now includes every package + dependency `.cell`, each `Cell.toml`, and each `Cell.lock`; `incremental_cache_hit` hashes the full set (`source_set_hash`, `src/lib.rs:4788`). Changing a dependency `.cell` invalidates the cache. |
| **F4** build/check don't validate every module | **FIXED** | `project_frontend_diagnostics` (`src/lib.rs:4368`) runs type + flow + IR checks on **every** loaded module; `compile_file_with_entry_scope` fails the build if any module errors (`src/lib.rs:4661-4664`). |
| **F5** LSP diagnostics not package-aware | **FIXED IN WORKTREE** | `src/lsp/mod.rs` routes file-backed diagnostics through `compile_path_metadata_with_diagnostics_for_source`, so unsaved entry buffers use the loaded project graph. |
| **F6** WASM playground single-source | **FIXED IN WORKTREE** | `crates/cellscript-wasm/src/lib.rs` keeps the single-source exports and adds `compile_metadata_json_sources([{ path, source, role }], entry_path, target)`. The website WASM bundle was rebuilt and stayed under the 600 KB gzip budget by using a single release codegen unit in `website/scripts/build-wasm.sh`. |
| **F7** Multi-diagnostic routing inconsistent | **FIXED IN WORKTREE** | CLI/package diagnostics, LSP diagnostics, and the new WASM multi-source API all return collected frontend diagnostics where recovery is possible. |
| **F8** `mod` model implicit, needs contract | **DOCUMENTED IN WORKTREE** | `docs/wiki/Tutorial-02-Language-Basics.md` and `docs/wiki/Tutorial-04-Packages-and-CLI-Workflow.md` now describe source discovery, module declarations, `use`, duplicate-module rejection, and the no-linker boundary. |

## 6. Remaining Genuine Gaps (New / Carried Forward)

| # | Finding | Severity | Notes |
| --- | --- | --- | --- |
| **G1** | LSP diagnostics are single-source | FIXED IN WORKTREE | File-backed LSP diagnostics now compile through the package-aware metadata diagnostics path and only publish diagnostics belonging to the open URI. |
| **G2** | WASM/playground API is single-source | FIXED IN WORKTREE | An additive multi-source JSON API was added without removing the legacy single-source exports. The generated website WASM bundle was rebuilt: 602,232 bytes gzip (588 KiB), within the script's 600 KiB budget. |
| **G3** | Fully-qualified cross-file function calls are rejected | FIXED IN WORKTREE | Qualified, aliased, same-basename, and transitive helper-call fixtures now compile by normalizing external call targets to stable labels and copying the reachable helper body closure into the entry artifact. ELF-linker semantics remain outside the production claim. |
| **G4** | Dead code | FIXED IN WORKTREE | The stale `resolve_*_global*` fallback APIs were removed; the old unvalidated resolver path had already been removed. |
| **G5** | Duplicated manifest schema | FIXED IN WORKTREE | `src/lib.rs` now reuses `package::PackageManifest`, `BuildConfig`, `CkbCellDepConfig`, and `WorkspaceManifest` instead of maintaining a parallel `CellManifest` schema. |
| **G6** | `mod` contract undocumented | FIXED IN WORKTREE | The wiki now documents package source discovery, module declarations, `use`, duplicate-module rejection, and the no-ELF-linker boundary. |
| **G7** | Workspace CLI untested with real deps | COVERED IN WORKTREE | Added and ran a CLI fixture where a workspace member imports a resource type from a local path dependency and builds with `cellc build -p app --json`. |

## 7. Single-File Compatibility Impact

**Negligible.** The refactored project path is additive for single-file projects:

- `load_project_for_entry` for a one-file package loads exactly that file; the
  diagnostics loop runs once; the cache set contains one unit. Behaviour is
  unchanged.
- The source-graph cache key reduces to the entry-only hash when no package or
  dependency files exist — same as before.
- The only intentional "break" is code that compiled purely because of the old
  global basename fallback. Such code was relying on a correctness bug (wrong
  import path silently resolving); failing it is correct, not a regression.

## 8. Validation

Validation completed in this worktree:

- `cargo clippy --locked -p cellscript --all-targets -- -D warnings`
- `cargo test --locked -p cellscript -- --test-threads=1`
- `website/scripts/build-wasm.sh`
- `npm run build` in `website/`
- `git diff --check`

The full cargo test run passed all non-ignored suites, including 605 library
tests, 129 CLI tests, 30 bundled example tests, and the 218-test iCKB
differential suite. The normal Rust test harness keeps live devnet tests
`ignored`; the NovaSeal fungible-xUDT live local devnet profile was run
separately and passed issue, transfer, settle, and required negative cases.

The normal `./scripts/cellscript_gate.sh ci` entrypoint is still blocked before
it reaches project checks by an external parent-workspace Cargo formatting
issue: Cargo sees two packages named `kaspa-addresses` under
`/home/arthur/a19q3/{crypto,rusty-kaspa}/crypto/addresses`. Direct formatting
of touched Rust files, clippy, full tests, website WASM rebuild, website build,
and whitespace checks passed.

## 9. Recommendation

Do **not** pursue cross-file compiled linkage / an ELF linker: it does not match
the CKB execution model and none of iCKB, NovaSeal, or DobEvo require it.
Compile-time helper-call inlining is now the mature developer-ergonomics layer
for shared pure/action helper code inside one entry artifact.

Before claiming CI merge-readiness from the unified gate, fix or isolate the
external parent workspace duplicate-crate issue so `cargo fmt --all --check`
inside `./scripts/cellscript_gate.sh ci` can start from the CellScript
workspace cleanly.

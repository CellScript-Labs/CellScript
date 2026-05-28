# Rust Compiler Comparative Audit for CellScript

**Date:** 2026-05-28
**Auditors:** 6 parallel agents + coordinator synthesis
**Coordinator re-audit:** 2026-05-28 local evidence spot-check
**Rust reference:** `/Users/arthur/RustroverProjects/rust` (`main` @ `8f02e856be9`)
**CellScript target:** `/Users/arthur/RustroverProjects/CellScript` (`nightly-0.16` @ `05f8e46d`)
**Classification key:** Adopt (directly applicable) / Adapt (principle applies, scale down) / Do Not Copy (complexity unwarranted)

---

## Executive Summary

Ten things the Rust compiler teaches CellScript, ranked by impact:

1. **IR lowering must not produce live sentinel values after errors.** Rust's `ErrorGuaranteed` / `construct_error` pattern prevents semantically poisoned artifacts from reaching codegen. CellScript has several error-path `IrConst::U64(0)` sentinels that continue with `Some(current)`; those paths need a poison/vacuous-body protocol so invalid IR cannot masquerade as real zero. *(Adapt)*

2. **Every IR instruction needs a source span.** Rust MIR carries `SourceInfo` per statement/terminator. CellScript IR has spans only at block level. Verifier errors and debug info are blind without instruction-level provenance. *(Adapt)*

3. **Parse recovery is not optional for developer experience.** Rust emits errors for every item in a file before stopping. CellScript halts at the first parse error. For a smart-contract DSL, this is a usability showstopper. *(Adapt)*

4. **Release gates need per-function assembly validation.** Rust validates MIR after every optimisation pass. CellScript validates at ELF assembly time but lacks per-function checks for stack balance, register clobbering, and syscall ABI consistency. *(Adopt)*

5. **Diagnostic snapshot tests catch regression that substring matching misses.** Rust's `//~ ERROR` inline annotations verify error location, severity, and message. CellScript's `expect-error:TEXT` only checks message substrings. *(Adopt)*

6. **The 27K-line `lib.rs` is the single biggest maintainability risk.** Rust splits its compiler into phase-oriented crates. CellScript's monolithic entry point mixes compilation orchestration, metadata generation, resource analysis, witness encoding, and 366 test functions. *(Adopt)*

7. **Expected-type propagation eliminates unnecessary annotations.** Rust's `Expectation` enum flows type hints down from assignment targets to array literals. CellScript has `infer_expr_with_contextual_literals` but it's a bolt-on, not the primary inference path. *(Adapt)*

8. **Type::Named(String) string-parsing on every comparison is a performance and correctness trap.** Rust uses structural `TyKind` with proper generic args. CellScript runs a hand-written parser inside `types_equal()` for every named-type comparison. *(Adapt)*

9. **Warning-level diagnostics enable graceful migration paths.** Rust's Level enum supports Bug/Fatal/Error/Warning/Allow. CellScript has only `CompileError` — everything is a hard stop. Migration diagnostics and deprecation warnings are impossible. *(Adopt)*

10. **CellScript already exceeds Rust in one important discipline: no `unsafe` blocks, and no observed `panic!`/`unreachable!`/`todo!` macros outside test modules in the spot-check.** Production `expect()` calls still exist for invariants, so the correct action is to formalise which invariants may abort and keep user-facing failures on the `Result`/diagnostic path.

**What not to copy from rustc:** The query system (`TyCtxt`), `SyntaxContext`/hygiene machinery, the full dataflow framework, `Place`/projection model, multi-crate architecture, `SourceMap`+interning, and any suggestion to "just use LLVM." These serve a general-purpose language with macros, incremental compilation, and multiple backends. CellScript is a domain-specific compiler targeting one VM. Westminster Abbey's flying buttresses do not belong in a CKB contract pocket watch.

---

## Coordinator Re-Audit Notes

The original report is directionally strong, but several details needed tightening:

- Rust's local branch is `main`, not `master`; both repositories are now pinned above with short commits.
- `src/cli/commands.rs` is currently 5,826 lines, not 4,894.
- The repository currently has 26 `#[allow(clippy::too_many_arguments)]` occurrences under `src/`, not 27.
- `instruction_dest()` and `instruction_operands()` are exhaustive Rust matches today. The real risk is semantic drift between IR visitor helpers, so this should be a regression-test item rather than a P0 "compile-time guard" item.
- `IrConst::U64(0)` is not inherently wrong. It is wrong when an error path records a diagnostic and then returns a live block as if a legitimate value had been produced. That distinction matters, or the audit will be both correct and unfair, which is the least useful Oxford combination.
- `Newline` tokens are not literally ignored everywhere; they require many explicit `skip_newlines()` calls. The issue is parser ceremony and recovery fragility, not the mere existence of a newline token.

---

## Comparison Matrix

| Area | Rust Practice | CellScript Current State | Verdict | Recommended Action |
|------|---------------|--------------------------|---------|-------------------|
| Token/Span | 8-byte interned `Span` + `SourceMap` lazy resolution | `Span { start, end, line, column }` byte offsets; no `end_line`/`end_column` | Adapt | Add `end_line`/`end_column`; fix multi-line carets |
| Parse recovery | `recover_stmt`, `ErrorGuaranteed` AST nodes, snapshots | First parse error halts via `?`; `ErrorReporter` can accumulate later-phase errors but the parser emits one | Adapt | Add recoverable parse result; skip to next top-level keyword |
| Pretty printing | `pp.rs` streaming algorithm + `FixupContext` for parens | `String`-based formatter; no parenthesization logic; `Type::Tuple` drops trailing comma | Adapt | Fix trailing comma; add precedence-aware parenthesization |
| Name resolution | Deterministic `Determinacy` enum; fixpoint import loop | `SymbolTable.imported` loses import spans; `HashMap` storage makes debugging/order-sensitive output harder to reason about | Adapt | Preserve import spans; add explicit finalization; use deterministic maps for emitted diagnostics/debug output |
| Type inference | `Expectation` enum flowing down expression tree | `infer_expr` returns standalone types; `infer_expr_with_contextual_literals` is bolt-on | Adapt | Introduce `ExpectedTy { None, Hint, Exact }` |
| Type representation | Structural `TyKind` with `Vec<GenericArg>` | `Type::Named(String)` with string-parsing `types_equal()` | Adapt | Add `Type::Named { base, args }` with parsed generics |
| IR lowering | `construct_error` produces vacuous body + `ErrorGuaranteed`; downstream skips | `record_error` + live `IrConst::U64(0)` sentinels on some error paths; lowering can continue | Adapt | Return `current: None`/poison on lowering errors; terminate blocks deliberately |
| IR source info | `SourceInfo { span, scope }` on every statement/terminator | `IrBlock.source_span` only; instructions carry no span | Adapt | Add `span: Span` to `IrInstruction` and `IrTerminator` |
| IR validation | Multi-phase `MirPhase` with per-phase legality | Single `verify_module` pass; no phase distinction | Adapt | Add `IrPhase` enum as documentation marker |
| Codegen passes | Declared pass ordering; per-pass validation | Linear emit; no pass concept | Adapt | Add `CodegenPhase` state machine for `debug_assert!` |
| Backend validation | `Validator` + `CfgChecker` after each MIR pass | `reject_unresolved_calls` + `validate_machine_block_coverage` at assembly time | Adopt | Add per-function stack/register/ABI checklist |
| Register contract | Abstracted via `BackendTypes` trait | `s10`/`s11`/`t6` implicit; documented only in comments | Adapt | Named constants + gate test for callee-saved invariance |
| Release gates | beta/bors merge gates; sanitizer runs; UI test baselines | `cellscript_gate.sh` with dev/ci/backend/release modes; CKB acceptance | Adopt | Extend gate with syscall ABI baseline + witness fuzzing |
| Diagnostics | `DiagCtxt` with dedup, stash, levels, JSON emitter | `ErrorReporter` = `Vec<CompileError>`; no dedup; no warnings | Adopt | Add severity enum + dedup + warning channel |
| Test strategy | `compiletest` with `//~ ERROR` location+severity assertions | `expect-error:TEXT` substring matching only | Adapt | Add `expect-error-line:N:TEXT` directive |
| File organization | Phase-oriented crates; tidy enforces line-length/style conventions | `lib.rs` = 27,411 lines; `types/mod.rs` = 8,806 lines; `commands.rs` = 5,826 lines | Adopt | Split `lib.rs`, `types/mod.rs`, `commands.rs` into submodules |
| Error handling | Compiler bugs may panic; user errors always `Diagnostic` | No observed `panic!`/`unreachable!`/`todo!` macros outside test modules; production `expect()` remains for invariants | Match | Formalise invariant-abort policy; keep user errors diagnostic |
| Unsafe usage | 955+ blocks with mandatory `// SAFETY:` comments | Zero `unsafe` blocks | ✅ N/A | No action needed |
| Clippy discipline | Project-level `[lints]`; minimal `#[allow]` | No crate-level lint config; 26 `#[allow(clippy::too_many_arguments)]` under `src/` | Adapt | Add lint config; group repeated arg patterns into structs |

---

## Agent Findings

### Agent 1: Frontend / Parse / AST / Formatter

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| FE-01 | High | `Span` lacks `end_line`/`end_column`; multi-line error carets are wrong | Adapt |
| FE-02 | High | No parse recovery; first error halts compilation | Adapt |
| FE-03 | Medium | `Type::Tuple` formatting drops trailing comma for single-element tuples; breaks round-trip | Adopt |
| FE-04 | Medium | `Newline` handling is scattered through explicit `skip_newlines()` calls; recovery and grammar maintenance are brittle | Adapt |
| FE-05 | Medium | Duplicate `format_type` in `fmt/mod.rs` and `docgen/mod.rs`; FE-03 bug exists in both | Adopt |
| FE-06 | Low | No syntax context / hygiene tracking | Do Not Copy |
| FE-07 | Medium | String-based formatter with hardcoded indentation | Adapt |
| FE-08 | Low | Token heap allocation via `String` payload; no interning | Adapt |
| FE-09 | Low | No `Spanned` trait; manual span access is inconsistent | Adopt |
| FE-10 | Medium | No parenthesization logic; `-(x + y)` formats to `-x + y` | Adapt |

**Key Rust evidence:**
- `rustc_parse/src/parser/diagnostics.rs:2131-2209` — `recover_stmt` with depth tracking
- `rustc_ast_pretty/src/pprust/state.rs:1354-1361` — single-element tuple trailing comma
- `rustc_ast_pretty/src/pprust/state/expr.rs:403-444` — `FixupContext` for precedence-aware parens
- `rustc_ast/src/ast.rs:1924,950,2570` — `ExprKind::Err(ErrorGuaranteed)` for error recovery

**Key CellScript evidence:**
- `src/error/mod.rs:4-10` — `Span` struct definition
- `src/parser/mod.rs:78-83` — `expect()` returns `Err` immediately
- `src/fmt/mod.rs:796` — `Type::Tuple` formatting without trailing comma
- `src/fmt/mod.rs:490-492` — `Binary` expr formatting without parenthesization

---

### Agent 2: Resolution / Types / Semantics

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| TS-01 | High | 8800-line monolithic type checker with no separation of concerns | Adapt |
| TS-02 | High | Resolution phase drops spans; `SymbolTable.imported` stores `HashMap<String, String>` | Adapt |
| TS-03 | Medium | Internal `HashMap` storage makes diagnostic/debug order easy to destabilize unless every emission path sorts explicitly | Adapt |
| TS-04 | Medium | No expected-type propagation; `infer_expr` always returns standalone type | Adapt |
| TS-05 | Medium | Resolution ↔ type-check boundary is implicit; no structured protocol | Adapt |
| TS-06 | Medium | Linear type obligations are inline rather than collected-and-evaluated | Adapt |
| TS-07 | Medium | Flow verification and type checking are separate passes with no shared state | Adapt |
| TS-08 | Low | `primitive_strict` check duplicated in `lib.rs` and `types/mod.rs` | Adopt |
| TS-09 | Low | Import validation is implicit, not a mandatory phase | Adapt |
| TS-10 | Medium | `types_equal()` parses type name strings on every comparison | Adapt |
| TS-11 | Low | No error context chain ("expected X because Y") | Adapt |
| TS-12 | Low | `TypeEnv::clone()` for branch analysis; potential performance issue | Adapt |

**Key Rust evidence:**
- `rustc_resolve/src/imports.rs:195-224` — `use_span`/`span`/`root_span` on every `ImportKind`
- `rustc_resolve/src/imports.rs:719-745` — `determined_imports` with explicit `Determinacy` enum
- `rustc_hir_typeck/src/expectation.rs:11-24` — `Expectation` enum
- `rustc_trait_selection/src/traits/fulfill.rs:28` — `ObligationCause` with `Span` + `ObligationCauseCode`

**Key CellScript evidence:**
- `src/types/mod.rs` — 8806 lines
- `src/resolve/mod.rs:176-230` — `process_import()` span loss in `SymbolTable.imported`
- `src/types/mod.rs:5983-6035` — `types_equal()` with `split_named_generic()` string parsing
- `src/types/mod.rs:3347-3350` — `infer_expr()` without expected type parameter

---

### Agent 3: IR Lowering / MIR / Dataflow

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| IR-01 | **Critical** | Error-path `U64(0)` sentinels can remain live after `record_error`; invalid IR may continue | Adapt |
| IR-02 | High | No span on `IrInstruction`; block-level only | Adapt |
| IR-03 | High | No span on `IrTerminator` | Adapt |
| IR-04 | Medium | Unreachable blocks rejected as error vs. cleaned up | Adapt |
| IR-05 | High | Defined-on-all-paths fixpoint co-mingled with verifier | Adapt |
| IR-06 | Medium | No `MirPhase` equivalent; single verification gate | Adapt |
| IR-07 | High | Poison vs vacuous body: error recovery produces structurally broken IR | Adapt |
| IR-08 | Medium | No CFG-aware linear resource tracking in dataflow | Adapt |
| IR-09 | Low | Monolithic 192-line `lower_expr` match; no category dispatch | Adapt |
| IR-10 | Low | `instruction_dest`/`instruction_operands` are exhaustive matches today, but semantic visitor drift lacks regression tests | Adopt |

**Rust MIR vs CellScript IR comparison:**

| Dimension | Rust MIR | CellScript IR |
|-----------|----------|---------------|
| Frontend input | THIR (typed high-level IR) | AST `Module` directly |
| Terminator variants | 14 (Goto, SwitchInt, Call, Drop, Assert, Return, Unreachable…) | 4 (Jump, Branch, Return, Abort) |
| Source info | `SourceInfo { span, scope }` on every statement/terminator | `IrBlock.source_span` only; no per-instruction span |
| Error recovery | `construct_error` -> vacuous body + `ErrorGuaranteed` | `record_error` -> live sentinel values on some error paths |
| Phase gating | `MirPhase::Built/Analysis/Runtime` with per-phase legality | Single `verify_module` pass |
| Dataflow | Generic `Analysis` trait with lattice join, fixpoint | Hand-rolled defined-on-all-paths intersection inside verifier |

**Key Rust evidence:**
- `compiler/rustc_mir_build/src/builder/mod.rs:615-726` — `construct_error` with `ErrorGuaranteed`
- `compiler/rustc_middle/src/mir/mod.rs:842-857` — `SourceInfo` on every statement
- `compiler/rustc_mir_transform/src/validate.rs:32-99` — `Validator` with `CfgChecker`
- `compiler/rustc_middle/src/mir/syntax.rs:71-145` — `MirPhase` enum

**Key CellScript evidence:**
- `src/ir/mod.rs:1970-1972` — unresolved identifier records an error but returns a live `IrConst::U64(0)`
- `src/ir/mod.rs:302-308` — `IrBlock` with `source_span` but no instruction spans
- `src/ir/mod.rs:5773-5807` — fixpoint inside verifier
- `src/ir/mod.rs:6570-6652` — exhaustive `instruction_dest`/`instruction_operands` match sites; add semantic regression tests if IR variants evolve

---

### Agent 4: Passes / Codegen / Release Gates

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| CG-01 | High | No per-function assembly validation (stack balance, register clobber) | Adopt |
| CG-02 | High | s10/s11/t6 register contract is implicit; no invariance check | Adapt |
| CG-03 | Medium | No phase transition enforcement in codegen pipeline | Adapt |
| CG-04 | Medium | No `-Zvalidate-mir` equivalent for dev-time exhaustive checking | Adapt |
| CG-05 | Medium | Backend shape baseline lacks stack frame depth and register pressure metrics | Adopt |
| CG-06 | High | No syscall ABI regression test against CKB VM specification | Adapt |
| CG-07 | Medium | Witness ABI magic-number checks lack property/fuzz testing | Adapt |
| CG-08 | Low | High unreachable machine-block ratio (81-94%) with no size gate | Do Not Copy |

**Gate classification:**

| Check | Target Gate | Priority |
|-------|------------|----------|
| Register invariance (s10/s11 not mutated outside prologue) | Fast gate | P1 |
| Per-function stack offset balance | Fast gate | P1 |
| Syscall number whitelist | Fast gate | P1 |
| `CELLSCRIPT_VALIDATE_BACKEND=1` exhaustive check | Dev gate | P2 |
| Extended shape metrics (frame size, spill count) | Fast gate | P2 |
| Witness ABI property test (arbitrary payloads) | Release gate | P1 |
| Syscall ABI baseline with CKB VM version binding | Release gate | P1 |
| Unreachable block ratio warning (>50%) | Release gate | P3 |

---

### Agent 5: Diagnostics / Tooling / Tests

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| DT-01 | Medium | No diagnostic deduplication; `ErrorReporter` is a bare `Vec` | Adopt |
| DT-02 | Low | Mutex poisoning recovery exists but untested; LSP resets all state on panic | Adopt |
| DT-03 | Medium | No error location assertion in tests (`expect-error:TEXT` only) | Adapt |
| DT-04 | Medium | `commands.rs` = 5,826 lines; no file-length gate | Adapt |
| DT-05 | Medium | LSP has no cancellation support; long compilations block the editor | Adapt |
| DT-06 | Medium | No warning-level diagnostics; all issues are hard errors | Adopt |
| DT-07 | Low | No tidy equivalent for codebase hygiene checks | Adapt |
| DT-08 | Low | `Span::Display` format is unparseable (`1:5-10:5:10`) | Adopt |

**Top 5 checks to add to CI/release gate:**

1. `cargo clippy --locked -p cellscript --all-targets -- -D warnings` as hard gate
2. `cargo fmt --all -- --check` as hard gate
3. Diagnostic regression test suite with `expect-error-line:N:TEXT` directives
4. Assembly snapshot stability with `--bless` mode
5. Tidy script checking: no `dbg!` remains, error codes are documented, `MigrationDiagnostic` variants have tests

---

### Agent 6: Code Style / Architecture / Error Handling

| ID | Severity | Issue | Verdict |
|----|----------|-------|---------|
| CS-01 | High | `lib.rs` = 27,411 lines with 877 `fn` definitions and 366 tests | Adopt |
| CS-02 | Medium | No crate-level lint configuration (`#![deny]`/`#![warn]`) | Adopt |
| CS-03 | Medium | 26 `#[allow(clippy::too_many_arguments)]` occurrences under `src/`, concentrated in codegen and orchestration helpers | Adapt |
| CS-04 | Low-Medium | Production `expect()` used for compiler invariants; acceptable but should be formalised | Adopt |
| CS-05 | Medium | 366 test functions mixed with production code in `lib.rs` | Adopt |
| CS-06 | Low | `syscalls.rs` uses crate-level `#![allow(dead_code)]` | Adopt |
| CS-07 | Low | No `#[must_use]` on key compiler result types | Adopt |
| CS-08 | High | No compile-phase isolation in `lib.rs`; metadata/resource/witness logic interleaved | Adapt |

**CellScript's strengths (exceeding Rust compiler in some dimensions):**
- Zero `unsafe` blocks
- No observed `panic!()`, `unreachable!()`, or `todo!()` macros outside test modules in the spot-check
- Broad `Result` coverage on compile pipeline
- DSL-level forbidden-method enforcement
- Existing CKB-facing gate scripts and acceptance fixtures

---

## Adopt / Adapt / Do Not Copy

### Adopt (short-term, directly landable)

| # | Finding | Effort |
|---|---------|--------|
| A1 | Fix `Type::Tuple` trailing comma in `fmt/mod.rs` and `docgen/mod.rs` (FE-03/FE-05) | S |
| A2 | Add `Spanned` trait for uniform span access (FE-09) | S |
| A3 | Deduplicate `format_type` into shared module (FE-05) | S |
| A4 | Unify `primitive_strict` checks into single enforcement point (TS-08) | S |
| A5 | Add focused regression tests for IR visitor helper semantics when new `IrInstruction` variants are added (IR-10) | S |
| A6 | Add per-function assembly validation (stack balance, call targets) (CG-01) | M |
| A7 | Extend backend shape baseline with frame size, spill count, li count (CG-05) | S |
| A8 | Add diagnostic deduplication to `ErrorReporter` (DT-01) | S |
| A9 | Add warning-level diagnostics (`DiagnosticSeverity` enum) (DT-06) | M |
| A10 | Fix `Span::Display` format to standard `file:line:col` (DT-08) | S |
| A11 | Add `#![deny(unused_must_use)]` + `#[must_use]` on key types (CS-02/CS-07) | S |
| A12 | Replace crate-level `#![allow(dead_code)]` with per-item allows (CS-06) | S |
| A13 | Split `lib.rs` tests into `src/lib_tests.rs` or `tests/` (CS-05) | M |

### Adapt (principle applies, scale down for CellScript)

| # | Finding | Effort |
|---|---------|--------|
| D1 | Add `end_line`/`end_column` to `Span`; fix multi-line error carets (FE-01) | M |
| D2 | Implement parse recovery with `ParseResult` enum and skip-to-keyword (FE-02) | L |
| D3 | Add precedence-aware parenthesization to formatter (FE-10) | M |
| D4 | Centralise `Newline` handling so parser productions do not each remember `skip_newlines()` (FE-04) | M |
| D5 | Split `types/mod.rs` (8806 lines) into submodules (TS-01) | L |
| D6 | Preserve import spans in `SymbolTable` (TS-02) | M |
| D7 | Introduce `ExpectedTy` for type hint propagation (TS-04) | M |
| D8 | Define `ResolutionResult` struct as resolve ↔ type-check boundary (TS-05) | M |
| D9 | Replace `Type::Named(String)` with structured generics (TS-10) | L |
| D10 | Change live error-path `U64(0)` sentinels to `current: None` or explicit poison + block termination (IR-01) | M |
| D11 | Add `span: Span` to `IrInstruction` and `IrTerminator` (IR-02/IR-03) | M |
| D12 | Extract defined-on-all-paths into separate analysis function (IR-05) | M |
| D13 | Add `IrPhase` and `CodegenPhase` marker enums (IR-06/CG-03) | S |
| D14 | Formalise register contract as named constants + gate test (CG-02) | M |
| D15 | Add `expect-error-line:N:TEXT` test directive (DT-03) | M |
| D16 | Add LSP cancellation via `AtomicBool` (DT-05) | M |
| D17 | Split `lib.rs` production code into phase-specific submodules (CS-01/CS-08) | L |
| D18 | Group repeated codegen arguments into structs (CS-03) | M |

### Do Not Copy (complexity unwarranted for CellScript)

| # | Rust Practice | Why Not |
|---|---------------|---------|
| X1 | Query system / `TyCtxt` | CellScript is single-pass; incremental compilation not needed |
| X2 | `SyntaxContext` / `ExpnId` / hygiene | CellScript has no macro system |
| X3 | Full dataflow framework with `Analysis` trait | CellScript's CFG is simple enough for hand-rolled fixpoints |
| X4 | `Place` / projection model | CellScript uses flat field access, not nested projections |
| X5 | Multi-crate architecture | CellScript is ~50K lines; single crate with good modules suffices |
| X6 | `SourceMap` + interning | `Span` with line/column pairs is sufficient for CellScript's scale |
| X7 | LLVM / Cranelift backend integration | CKB-VM RISC-V assembly is CellScript's only target |
| X8 | MIR dead-code elimination passes (for now) | CellScript restricts linear ops to straight-line code; dead blocks are rejected by design |
| X9 | `Symbol` interning with global state | Token deduplication via `&str` slices is sufficient |
| X10 | Glob import resolution / re-export validation | CellScript has explicit named imports only |

---

## Priority Roadmap for CellScript

### P0: Correctness / Release Gate (must fix before next release)

| # | Finding | Action |
|---|---------|--------|
| P0-1 | IR-01: Live error sentinels | Return explicit poison after lowering errors; keep value validity separate from block liveness |
| P0-2 | CG-02: Register contract undocumented | Named constants + gate test for s10/s11/t6 invariance |
| P0-3 | CG-06: No syscall ABI regression | Create `tests/syscall_abi_baseline.json` + fast-gate check |

### Key P1: Keep In 0.16 Freeze

| # | Finding | Action |
|---|---------|--------|
| P1-Freeze-1 | IR-02/IR-03: Instruction/terminator provenance | Add `SpannedIrInstruction { kind, span }` and `SpannedIrTerminator { kind, span }` wrappers or equivalent sidecar provenance |
| P1-Freeze-2 | DT-03: No error location tests | Add `expect-error-line:N:TEXT` directive |

### P1: Diagnostics / Maintainability / Test Confidence Deferred To 0.17

| # | Finding | Action |
|---|---------|--------|
| P1-1 | IR-01 refinement | Replace bridge poison IR with `Lowered<T>` / `LoweredOperand::{Value, Poisoned}` |
| P1-2 | FE-03: Tuple round-trip breakage | Fix `Type::Tuple` trailing comma in `fmt` + `docgen` |
| P1-3 | DT-08: Misleading `Span::Display` | Replace with parseable `line:column-end_line:end_column` or file-qualified format |
| P1-4 | FE-02: No parse recovery | Implement `ParseResult` + skip-to-keyword |
| P1-5 | DT-06: No warning level | Add `DiagnosticSeverity` enum to `CompileError` |
| P1-6 | CS-01/CS-08: `lib.rs` 27K lines | Split into `src/compile.rs`, `src/metadata.rs`, `src/resource.rs`, `src/witness.rs` |
| P1-7 | TS-01: `types/mod.rs` 8806 lines | Split into `env.rs`, `infer.rs`, `check.rs`, `linear.rs`, `flow_check.rs` |
| P1-8 | CG-01: No per-function assembly validation | Add stack/register/ABI checklist |
| P1-9 | FE-10: No parenthesization | Add precedence-aware wrapping in formatter |
| P1-10 | IR-10: IR visitor helper drift | Add regression tests covering dest/operand semantics for every `IrInstruction` variant |

### P2: Style / Long-term Engineering Quality

| # | Finding | Action |
|---|---------|--------|
| P2-1 | CS-02: No crate-level lints | Add `#![deny(unused_must_use)]` |
| P2-2 | CS-05/CS-07: Tests mixed with production code | Extract tests to `src/lib_tests.rs` |
| P2-3 | TS-04: No expected-type propagation | Introduce `ExpectedTy` enum |
| P2-4 | TS-10: String-parsing type comparison | Replace `Type::Named(String)` with structured variant |
| P2-5 | DT-07: No tidy equivalent | Create `scripts/tidy.sh` with 5-6 targeted checks |
| P2-6 | FE-04: Scattered newline handling | Centralise parser newline policy or make productions recovery-aware |
| P2-7 | CG-05: Extended backend metrics | Add `max_frame_size`, `total_spill_instructions` to baseline |
| P2-8 | TS-06: Inline linear obligations | Collect `LinearObligation`s; evaluate at function end |

---

## Appendix

### A. File Path Evidence (Rust)

| Finding | Rust file |
|---------|-----------|
| FE-01 Span encoding | `compiler/rustc_span/src/span_encoding.rs:82-86` |
| FE-02 Parse recovery | `compiler/rustc_parse/src/parser/diagnostics.rs:2131-2209` |
| FE-03 Tuple trailing comma | `compiler/rustc_ast_pretty/src/pprust/state.rs:1354-1361` |
| FE-10 FixupContext | `compiler/rustc_ast_pretty/src/pprust/state/expr.rs:403-444` |
| FE-08 Symbol interning | `compiler/rustc_ast/src/token.rs:209-212`, `compiler/rustc_ast/src/token.rs:505-516` |
| TS-02 Import spans | `compiler/rustc_resolve/src/imports.rs:195-224` |
| TS-03 Determinacy | `compiler/rustc_resolve/src/lib.rs:98-105`, `compiler/rustc_resolve/src/imports.rs:719-745` |
| TS-04 Expectation | `compiler/rustc_hir_typeck/src/expectation.rs:11-24` |
| TS-06 FulfillmentCtxt | `compiler/rustc_trait_selection/src/traits/fulfill.rs:28` |
| IR-01 Construct error | `compiler/rustc_mir_build/src/builder/mod.rs:615-726` |
| IR-02 SourceInfo | `compiler/rustc_middle/src/mir/mod.rs:842-857` |
| IR-06 MirPhase | `compiler/rustc_middle/src/mir/syntax.rs:71-145` |
| CG-01 Per-pass validation | `compiler/rustc_mir_transform/src/validate.rs:32-99` |
| CG-06 FnAbi | `compiler/rustc_target/src/callconv/mod.rs:593-614` |
| DT-01 DiagCtxt | `compiler/rustc_errors/src/lib.rs:274-370` |
| DT-03 Compiletest errors | `src/tools/compiletest/src/errors.rs:95-195` |
| DT-07 Tidy | `src/tools/tidy/src/style.rs:344-716` |
| CS-01 File length | `src/tools/tidy/src/style.rs:39` |

### B. File Path Evidence (CellScript)

| Finding | CellScript file |
|---------|-----------------|
| FE-01 Span | `src/error/mod.rs:4-10` |
| FE-02 Parser halt | `src/parser/mod.rs:78-83` |
| FE-03 Tuple format | `src/fmt/mod.rs:796` |
| FE-05 Duplicate format_type | `src/fmt/mod.rs:784-801`, `src/docgen/mod.rs:1106-1123` |
| FE-10 No parens | `src/fmt/mod.rs:490-492` |
| TS-01 Type checker size | `src/types/mod.rs` (8806 lines) |
| TS-02 Import span loss | `src/resolve/mod.rs:176-230` |
| TS-04 No expected type | `src/types/mod.rs:3347-3350` |
| TS-10 String type compare | `src/types/mod.rs:5983-6035` |
| IR-01 Live error sentinel | `src/ir/mod.rs:1970-1972` plus other `record_error` + `IrConst::U64(0)` paths |
| IR-02 No instruction span | `src/ir/mod.rs:302-308` |
| IR-05 Fixpoint in verifier | `src/ir/mod.rs:5773-5807` |
| IR-10 Visitor helper semantics | `src/ir/mod.rs:6570-6652` |
| CG-02 Register contract | `src/codegen/abi.rs:229-230`, `src/codegen/frame.rs:56-57` |
| CG-05 Shape baseline | `tests/backend_shape_baseline.json` |
| CG-06 Syscall numbers | `src/codegen/runtime.rs:28-56` |
| DT-02 Mutex poisoning | `src/lsp/server.rs:33-39` |
| DT-05 No LSP cancel | `src/lsp/server.rs:525-534` |
| DT-08 Span display | `src/error/mod.rs:23-27` |
| CS-01 lib.rs size | `src/lib.rs` (27,411 lines) |
| DT-04 commands.rs size | `src/cli/commands.rs` (5,826 lines) |
| CS-06 Dead code allow | `src/syscalls.rs:8` |

### C. Commands and Checks Performed

Original agents used read-only tools: `rg`, `git grep`, `ast-outline`, `cat`, `wc -l`, `ls`, `head`/`tail`. The coordinator re-audit additionally ran targeted `rg`, `wc -l`, `ast-outline digest src`, and `git rev-parse` checks, then updated this Markdown file only. No Rust source files, Rust reference files, bootstrap builds, or full builds were executed.

### D. Unverified Items

| Item | Reason |
|------|--------|
| Semantic coverage of `instruction_dest`/`instruction_operands` | The matches are exhaustive today; proving semantic completeness needs dedicated variant-level tests |
| CellScript `assembly_snapshots.rs` snapshot content | Examined structure but not full snapshot corpus |
| Rust `rustc_mir_transform` full pass list | 50+ passes; agents sampled representative passes |
| CellScript `cellscript_gate.sh` full execution | Script existence and structure verified; not run end-to-end |
| Backend shape baseline `text_size` trend over time | Would require git history analysis beyond agent scope |

---

## Top 10 Next Actions

1. **Fix IR-01 live error sentinels** — After lowering errors, return an explicit poison result; do not let `IrConst::U64(0)` continue as if it were a legitimate value. This is the single most critical correctness fix for 0.16.

2. **Formalise register contract** — Named constants for s10/s11/t6 + gate test asserting no mutation outside approved entry-wrapper and branch-relaxation sites.

3. **Create syscall ABI baseline** — `tests/syscall_abi_baseline.json` mapping CKB syscall and VM2 helper numbers, checked in the fast gate.

4. **Add IR provenance** — Use `SpannedIrInstruction { kind, span }` and `SpannedIrTerminator { kind, span }` wrappers or equivalent sidecar provenance. Enables precise verifier errors and future debug info.

5. **Add `expect-error-line:N:TEXT` test directive** — Verify error locations, not just message substrings. Catches span regression.

6. **Refine poison representation in 0.17** — Move from bridge `IrConst::Poisoned` toward `Lowered<T>` / `LoweredOperand::{Value, Poisoned}` so poison remains a lowering result rather than a semantic IR constant.

7. **Fix Type::Tuple trailing comma and `Span::Display` in 0.17** — Keep these as cheap hygiene, but do not let them masquerade as 0.16 release blockers.

8. **Add per-function backend validation in 0.17** — Extend the current register/syscall gates into stack/register/ABI checks before final assembly.

9. **Add warning-level diagnostics and parse recovery in 0.17** — `DiagnosticSeverity` plus `ParseResult::Recovered` should improve UX without making 0.16 depend on a wider front-end rewrite.

10. **Split large modules after freeze** — Extract `lib.rs`, `types/mod.rs`, and CLI command ownership only after the correctness and backend gates are stable.

# CellScript Error Handling, Flow Analysis & Proof Plan Audit

**Scope:** `src/error/`, `src/runtime_errors.rs`, `src/flow/`, `src/types/`, `src/debug/`, `src/proof_plan/`, `src/codegen/`, `src/ir/`, `src/lib.rs`
**Date:** 2026-05-22
**Auditor:** Kimi Code CLI (automated assisted audit)
**Review status:** Corrected after local validation. The original draft overstated several flow-analysis findings around uninitialized `let`, tail `match` expressions, and lock terminal `match` expressions. Follow-up fixes now cover IR lowering error aggregation, IR lowering source spans, expression-level unreachable diagnostics, debug expression spans, and assembly parse line context.

**0.16 status note (2026-06-10):** Treat the ProofPlan findings below as a
historical audit baseline, not as the current 0.16 contract. The
0.16 release line now emits `runtime.proof_plan_soundness`, rejects
local/runtime ProofPlan drift, rejects checked records with coverage gaps, and
fails `--primitive-strict 0.16` on metadata-only or runtime-required ProofPlan
gaps. Aggregate invariant executable lowering and cryptographic metadata
binding are still future work, but the old blanket statement that metadata-only
ProofPlan gaps are acceptable in pre-production is no longer correct.

---

## 1. CompileError Span Propagation & Staleness

### 1.1 Span Storage Model

`CompileError` (`src/error/mod.rs:30`) stores spans as an inline `Span` struct (byte `start`/`end`, `line`, `column`). There is no span-interning table or source-file ID; spans are copied by value into every error.

- **No source-file context in generic errors:** `CompileError::without_span()` (line 47) and the `From<std::io::Error>` impl (line 75) create errors with `Span::default()` (0,0,0,0), losing any ability to point back to source.
- **File path is optional:** `file: Option<Utf8PathBuf>` is only populated by `ErrorReporter` (line 205) or manual `.with_file()` calls. Many pipeline stages never attach a file.

### 1.2 Span Staleness After Lowering / Expansion

#### IR Lowering Loses Spans for Synthetic Constructs
**File:** `src/ir/mod.rs`

**Fix status:** Fixed for IR lowering diagnostics. `IrGenerator` now uses the available AST expression/statement spans for recorded lowering errors instead of defaulting to `Span::default()`.

Before the fix, the IR generator often fell back to `Span::default()` on error paths:

| Line | Context |
|------|---------|
| 1878 | `record_error("tuple binding requires a lowered tuple aggregate", Span::default())` |
| 1916 | Error for unsupported constant materialization |
| 1956 | Unresolved identifier fallback |
| 1965 | Another unresolved identifier path |
| 2112 | String literal error |
| 2120 | Range expression error |
| 3713 | Empty array literal without type |
| 3742 | Non-empty array literal missing element type |
| 3766 | Integer out of range |
| 3778 | Integer const conversion failure |
| 3783 | Byte string literal type mismatch |
| 3918 | Vec literal missing type |
| 3941 | Vec literal type mismatch |
| 3955 | Empty array literal with wrong type |

When lowering inserts synthetic instructions (e.g., expanding `transfer` into `consume` + `create` + `require`), new `RequireExpr`/`PreserveExpr` nodes inherit the original `call.span`. This is correct for provenance, but if the generated IR is later rejected by `ir::verify_module()` or codegen, the error points to the original call site, which may be confusing if the issue is in the expansion logic itself.

#### Optimizer Substitution Preserves Spans (with displacement risk)
**File:** `src/optimize/mod.rs:868–948`

`substitute_expr` copies the original expression's `span` into the substituted expression. This means inlined pure-function bodies retain the **callee's** spans, not the call-site span. If type checking or debug info is run on the optimized AST, diagnostics may point inside the callee body even for errors triggered at the call site.

#### Debug Info Is Broken Due to Missing Expression Spans
**File:** `src/lib.rs:1536–1541`

```rust
fn expr_span(expr: &ast::Expr) -> error::Span {
    // AST expressions don't carry their own Span in the current definition,
    // so we fall back to a default span.
    let _ = expr;
    error::Span::default()
}
```

`expr_span` **always** returns `Span::default()`. This is used by the debug-info generator (`src/lib.rs:3922`) with a hardcoded address of `0`, rendering DWARF line-number tables non-functional.

### 1.3 Assembler Errors Carry Assembly Line Context, Not Source Spans
**File:** `src/codegen/assembler.rs`

Assembler errors still do not carry original CellScript source spans, but assembly parse failures are now wrapped with the generated assembly line number and line text. This is weaker than source-span provenance, but it makes malformed generated assembly diagnosable without stepping through the whole emitted file.

### 1.4 Error Accumulation vs. Early Exit
**File:** `src/ir/mod.rs:615–619`

```rust
if let Some(error) = self.errors.into_iter().next() {
    Err(error)
} else {
    Ok(self.module)
}
```

**Fix status:** Fixed. `IrGenerator::generate()` now returns an aggregated `CompileError` when multiple lowering errors were recorded, preserving the first span and including the remaining messages in the aggregate diagnostic.

---

## 2. Result<>, ? Operator, expect/unwrap, panic!

### 2.1 Consistent `?` Usage (Good)
- **Parser:** `src/parser/mod.rs` uses `self.expect(TokenKind::...)?` consistently (~40 occurrences).
- **IR Lowering:** `src/ir/mod.rs` uses `?` heavily for `LoweredExpr.current?` and block transitions.
- **Pipeline:** `src/lib.rs` `compile()` chains lexer → parser → AST compilation with `?`.

### 2.2 `expect()` / `unwrap()` Hiding Failure Modes (Concern)
**File:** `src/codegen/` (assembler, collections, cell_ops, schema, frame, calls)

The codegen backend uses `.expect()` extensively for internal invariants that **should** be impossible if the IR is valid, but if they trigger, the compiler panics rather than returning a `CompileError`:

| File | Line | Usage |
|------|------|-------|
| `codegen/collections.rs` | 581, 582, 752, 858, 874 | `runtime_expr_temp_offset(...).expect("runtime temp slot")` |
| `codegen/schema.rs` | 646–648 | Multiple `runtime_expr_temp_offset(...).expect(...)` |
| `codegen/cell_ops.rs` | 521–524, 914, 1324–1325 | `runtime_expr_temp_offset(...).expect(...)` |
| `codegen/frame.rs` | 107 | `i64::try_from(offset).expect("stack offset should fit in i64")` |
| `codegen/abi.rs` | 609–610 | `i64::try_from(...).expect(...)` |
| `codegen/calls.rs` | 342–343 | `i64::try_from(...).expect(...)` |
| `codegen/mod.rs` | 902 | `self.pure_const_returns.get(func).cloned().expect("guarded pure const return")` |
| `codegen/mod.rs` | 1957 | `i64::try_from(offset).expect("memory offset should fit in i64")` |

**Risk:** A malformed IR module (or a bug in lowering) can crash the compiler process instead of emitting a diagnostic. Given that `ir::verify_module()` exists, these invariants are assumed to hold, but there is no graceful degradation.

### 2.3 `panic!` in Non-Test Code
Most `panic!` calls are confined to tests. Notable exceptions or test-adjacent library panics were found in parser tests and optimizer tests, but the production compiler pipeline itself does not use `panic!` for control flow.

### 2.4 `src/runtime_errors.rs` is Clean
This file contains no panics, no unwraps, just a stable enum with const methods. It is the model the rest of the compiler should follow.

---

## 3. Flow Analysis: Definite Assignment, Unreachable Code, Return Paths

**Note:** Traditional flow analysis is implemented in `src/types/mod.rs`, not `src/flow/mod.rs` (which is a domain-specific validator for blockchain state-machine transitions).

### 3.1 Definite Assignment: No Current Uninitialized-`let` Surface
**File:** `src/types/mod.rs:91–137`

`TypeEnv` tracks `vars`, `mutability`, and `linear_states`, but there is no separate `initialized: HashMap<String, bool>`. That is not currently a soundness issue because the language grammar and AST require every `let` binding to have an initializer (`LetStmt.value`), and `check_stmt` infers that value before binding the pattern.

The original audit example is not valid CellScript:

```cellscript
fn bad() -> u64 {
    let x: u64
    return x
}
```

It is rejected by the parser before type checking with an `expected '='` error. Therefore the correct finding is not "definite assignment is missing and exploitable"; it is:

- there is no current uninitialized-local syntax path to analyze;
- if the grammar later adds declarations without initializers, definite-assignment tracking must be added at that point;
- an explicit regression test should preserve the current invariant that typed `let` bindings require an initializer.

### 3.2 Unreachable Code Detection: Partially Implemented, with Source-Level Gaps
**File:** `src/types/mod.rs:2945–2971`

`check_no_unreachable_stmts` correctly catches statements after unconditional `return` and after `if/else` where both branches unconditionally return.

It also recurses into nested statement bodies for:

- `Stmt::If`
- `Stmt::For`
- `Stmt::While`
- `Stmt::Expr(Expr::Block(...))`

Fixed gaps in `check_no_unreachable_nested`:

- `Stmt::Expr(Expr::If(...))` branches are now recursively checked.
- `Stmt::Expr(Expr::Match(...))` arms are now recursively checked.
- Nested `Expr::Match` and `Expr::If` inside other statements are now recursively checked through expression traversal.

This should be treated as a diagnostic/source-level gap rather than a proven runtime bypass. In local validation, a statement after a `match` expression whose arms all return was rejected later by the IR verifier, but the diagnostic degraded to an imprecise location. The type checker should catch these cases earlier and preserve source spans.

Example shape:

```cellscript
fn bad() -> u64 {
    if true {
        match choice {
            Choice::A => { return 1 },
            Choice::B => { return 2 },
        }
        let x = 3
    }
    return 4
}
```

### 3.3 Return Path Coverage: Conservative and Imprecise
**File:** `src/types/mod.rs:4289–4353`

#### `stmt_always_returns` Missing Cases
```rust
fn stmt_always_returns(&self, stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Return(_) => true,
        Stmt::If(if_stmt) => { /* ... */ }
        Stmt::Expr(Expr::Block(stmts, _)) => self.stmts_always_return(stmts),
        _ => false,
    }
}
```

Missing:
- `Stmt::While` — even `while true { return 1 }` returns `false`
- `Stmt::For` — loop bodies are not used as return-path proof
- `Stmt::Expr(Expr::If(...))` and `Stmt::Expr(Expr::Match(...))` when those expression statements are used before the final tail-expression path

This is conservative, not unsound. For example:

```cellscript
fn bad() -> u64 {
    while true {
        return 1
    }
}
```

This is incorrectly rejected with "function 'bad' with a return type must return a value on all paths", even though it always returns at runtime if `true` is treated as a constant. The current analysis prefers false negatives over accepting a non-returning function.

#### Tail `match` Expressions Are Accepted
The original draft incorrectly claimed that `body_returns_or_tail_expr` misses tail `match` expressions. The implementation accepts any final `Stmt::Expr(expr)` and infers it against the expected return type, so a properly formed tail `Expr::Match` is accepted when its arms type-check.

Likewise, `infer_lock_terminal_stmt` accepts any `Stmt::Expr(expr)`, including `Expr::Match`, so lock bodies ending in a match expression are accepted.

#### `stmts_always_return` Uses Tail Semantics
`src/types/mod.rs:4285–4287`:
```rust
fn stmts_always_return(&self, stmts: &[Stmt]) -> bool {
    stmts.last().is_some_and(|stmt| self.stmt_always_returns(stmt))
}
```
**Fix status:** Fixed. Statement-list return analysis now uses the final statement, and expression-level `if`/`match` blocks can contribute to the return proof when all branches return.

### 3.4 Test Coverage Gaps
- Added a parser regression test proving typed `let` bindings without initializers remain syntactically rejected.
- Added a type-checker regression test ensuring unreachable code inside expression-level `match` branches is caught with source-level diagnostics.
- Added a regression test documenting that tail `match` expressions are accepted for functions and locks.
- Conservative return-path behavior through `while true` and `for` remains intentionally unchanged.

---

## 4. Debug Info Generation: Codegen Side Effects & Timing

### 4.1 Does Enabling Debug Info Alter Codegen Decisions?
**No — with one cosmetic exception.**

- `src/codegen/mod.rs:439` is the **only** place `options.debug` is consumed:
  ```rust
  self.assembly.push(format!("# opt_level={}, debug={}", self.options.opt_level, self.options.debug));
  ```
  This is a comment only; it does not change instructions, register allocation, frame layout, or branch selection.
- There are **no** `#[cfg(debug_assertions)]` or `#[cfg(debug)]` conditional compilation gates anywhere in `src/`.
- The `debug` flag does **not** interact with `opt_level`.

### 4.2 Timing Side Channels
**No runtime timing side channel is introduced.**

- DWARF sections are appended **after** codegen completes (`src/lib.rs:3928–3932`). The generated RISC-V instructions are identical with or without debug info.
- Debug sections live in the ELF file but are **not loaded into CKB-VM memory** during execution, so they have zero impact on cycle count or branch timing.
- A compiler-level timing difference exists (iterating over AST statements to build line tables), but this is not observable on-chain.

### 4.3 Debug Line Table Remains Address-Imprecise
**File:** `src/lib.rs:3922`

```rust
for stmt in &action.body {
    debug_gen.add_line_info(0, stmt_span(stmt));   // address hardcoded to 0
}
```

- `expr_span()` now extracts real AST expression spans instead of always returning `Span::default()`.
- `add_line_info` is still called with a hardcoded `address: 0` for every statement, so address-to-source mapping remains incomplete.
- The loop only iterates `ast::Item::Action` bodies; **pure functions and locks are completely omitted** from line table generation.

---

## 5. Proof Plan System (`src/proof_plan/`)

### 5.1 What It Does
The proof plan system produces `ProofPlanMetadata` records documenting:
- Verifier obligations (`checked-runtime`, `runtime-required`, `checked-partial`, `fail-closed`)
- Source-level triggers/scopes (`lock_group`, `transaction`)
- Cell reads, coverage labels, builder assumptions
- Cross-references between aggregate invariants and action obligations

### 5.2 Is It Actually Invoked During Compilation?
**Yes — unconditionally and redundantly.**

1. **Module-level** (`src/lib.rs:4453–4454`): stores result in `RuntimeMetadata.proof_plan`
2. **Per-item level** (`src/lib.rs:4569–4577`, `4674`, `4740`): stores in `ActionMetadata`, `FunctionMetadata`, `LockMetadata`
3. **Linkage pass** (`src/lib.rs:5577`): `link_invariant_action_coverage`

There are **no** `TODO`, `FIXME`, or `unimplemented!()` macros in `src/proof_plan/mod.rs`.

### 5.3 Completeness Gaps

#### All Invariants Are `metadata-only`
- `src/proof_plan/mod.rs:293` — `codegen_coverage_status: "gap:metadata-only"`
- `src/proof_plan/mod.rs:361` — same for aggregate invariants
- The system records that an invariant exists, but **does not generate or verify** the corresponding on-chain checking code.

#### `link_invariant_action_coverage` Only Handles `assert_conserved` and `assert_sum`
- `src/proof_plan/mod.rs:960–990` — coverage query extraction is limited to these two kinds.
- `Delta`, `Distinct`, and `Singleton` aggregate invariants receive **no action-coverage linkage** at all.

#### Action Coverage Is Admitted to Be Non-Exhaustive
- `src/proof_plan/mod.rs:167`:
  ```rust
  plan.builder_assumptions.push(
      "declared(action_coverage_evidence_is_existential_not_exhaustive)".to_string()
  );
  ```
  The system itself acknowledges that finding a matching action obligation does **not** prove full coverage.

#### Aggregate Relation Checks Have No Executable Evidence
- `src/proof_plan/mod.rs:910–920` — all aggregate relation labels are suffixed with `=metadata-only`.
- No codegen evidence IDs are produced for them.

#### Evidence Strings Are Coarse-Grained
- `codegen_evidence_id` (`src/proof_plan/mod.rs:465–488`) maps many distinct obligations to broad bucket IDs like `"codegen:input-lifecycle-runtime-check"`. Two different `consume-input` obligations on different types receive the same evidence ID, losing granularity.

### 5.4 Tamper Resistance of Proof Artifacts
**The metadata file is trivially tamperable.**

- Metadata is serialized as **plain JSON** and written to a sidecar file (`src/lib.rs:3696–3699`):
  ```rust
  let json = serde_json::to_vec_pretty(&self.metadata)?;
  std::fs::write(output_path, json)?;
  ```
- `ValidatedArtifact::validate` (`src/lib.rs:3705–3731`) checks:
  - `artifact_hash` matches artifact bytes
  - `artifact_size_bytes` matches
  - Schema-version and profile field equalities
- **It does NOT:**
  - Sign or MAC the metadata
  - Include the proof plan contents in the artifact hash
  - Recompute or cross-check proof plan entries against the IR or generated assembly

An attacker with filesystem access can edit `proof_plan` arrays, flip `on_chain_checked` from `false` to `true`, remove `builder_assumptions`, or delete `diagnostics`, and `cellc verify-artifact` will not detect the modification.

---

## 6. Severity Summary

| Concern | File | Severity |
|---------|------|----------|
| **Only first IR error returned; rest silently dropped** | `src/ir/mod.rs:615` | **Fixed** |
| **Debug line table uses hardcoded address 0** | `src/lib.rs:3922` | **High** |
| **Assembler errors never have source spans** | `src/codegen/assembler.rs` | **Partially fixed: assembly line context added** |
| **All invariants are `metadata-only`; no executable verification** | `src/proof_plan/mod.rs` | **High** |
| **`link_invariant_action_coverage` ignores Delta/Distinct/Singleton** | `src/proof_plan/mod.rs` | **High** |
| **Metadata JSON has no signature/MAC** | `src/lib.rs:3696` | **High if metadata crosses a trust boundary; otherwise Medium** |
| **Proof plan not cryptographically bound to artifact** | `ValidatedArtifact::validate` | **High if metadata crosses a trust boundary; otherwise Medium** |
| Codegen uses `expect()` for recoverable invariant failures | `src/codegen/*.rs` | Medium |
| IR lowering falls back to `Span::default()` on many errors | `src/ir/mod.rs` | Fixed |
| Expression-level unreachable code is not diagnosed early with source spans | `src/types/mod.rs:2957` | Fixed |
| Evidence IDs are overly coarse | `src/proof_plan/mod.rs` | Medium |
| `stmt_always_returns` conservatively ignores loop-return proofs | `src/types/mod.rs:4289` | Low |
| Optimized inlined expressions retain callee spans | `src/optimize/mod.rs` | Low |
| `stmts_always_return` uses `.any()` | `src/types/mod.rs:4285` | Fixed |
| No explicit test for parser rejection of uninitialized typed `let` | parser/types tests | Fixed |
| Redundant proof plan computation (module + per-item) | `src/lib.rs` | Low |

---

## 7. Recommendations

### Error Handling & Spans
1. Introduce `Span::synthetic()` or `is_synthetic` to distinguish original source from defaulted spans.
2. **Fixed:** `IrGenerator::generate()` aggregates recorded errors instead of dropping all but the first.
3. Replace codegen `.expect()` calls with `Result`-returning helpers that propagate `CompileError`.
4. **Fixed:** `expr_span` in `src/lib.rs` now extracts real spans from AST expressions.
5. **Partially fixed:** assembly parse errors now include generated assembly line context; original CellScript source spans are still not threaded through assembler errors.

### Flow Analysis
6. **Fixed:** added a regression test proving typed `let` bindings without initializers are rejected by the parser.
7. **Fixed:** `check_no_unreachable_nested` recurses into expression-level `if` and `match` branches.
8. Decide whether constant/infinite loops should contribute to return-path proof. If yes, extend `stmt_always_returns` conservatively for cases such as `while true { return ... }`.
9. **Fixed:** added regression coverage for tail `match` expressions in functions and locks.
10. **Fixed:** added regression coverage for source-level unreachable diagnostics in expression branches.

### Debug Info
11. Fix `src/lib.rs:3922` to use actual instruction addresses instead of `0`.
12. Extend debug line generation to cover pure functions and locks, not just actions.

### Proof Plan
13. Generate or verify on-chain checking code for invariants rather than marking them `metadata-only`.
14. Extend `link_invariant_action_coverage` to handle `Delta`, `Distinct`, and `Singleton` invariants.
15. Cryptographically bind metadata to the artifact (e.g., include proof plan hash in artifact hash, or sign the metadata file).
16. Make evidence IDs more fine-grained so distinct obligations are distinguishable.

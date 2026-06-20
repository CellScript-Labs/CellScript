# CellScript Compiler Swarm Audit

Date: 2026-06-20
Method: Parallel cross-comparison of the CellScript compiler
(`a19q3/CellScript`, branch `main`, HEAD `f375155`, on `origin` =
`git@github.com:a19q3/CellScript_Private.git`) against its own published
contracts (`AGENTS.md`, `CODING_STYLE.md`), gate scripts, baseline JSON
fixtures, and the iCKB v1-core Rust source (for behavioural reference where
the compiler emits Cell-model programs).

The swarm ran four parallel cross-comparison passes:

| Pass | Area | Files |
|---|---|---|
| 1 | Parser + Lexer + AST surface | `src/lexer/`, `src/parser/`, `src/ast/`, `src/fmt/`, `tests/syntax_combo/`, `tests/v0_14.rs`, `tests/v0_16.rs` |
| 2 | Types + Flow + Optimize + Resolve | `src/types/`, `src/flow/`, `src/optimize/`, `src/resolve/`, `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md` |
| 3 | IR + Codegen + Assembler + ABI | `src/ir/`, `src/codegen/`, `tests/assembly_snapshots.rs`, `tests/backend_shape_baseline.json`, `tests/syscall_abi_baseline.json`, `scripts/cellscript_strict_backend_audit.{sh,py}` |
| 4 | ABI + Stdlib + Runtime + ProofPlan + NovaSeal doc claims | `src/stdlib/`, `src/syscalls.rs`, `src/runtime_errors.rs`, `src/assumptions.rs`, `src/proof_plan/`, `src/verifier_registry/btc.rs`, `src/cli/novaseal_certification.rs`, `proposals/novaseal/`, `scripts/novaseal_*.py`, `scripts/cellscript_gate.sh` |

The audit was a **read-only** review — no source files were modified.

## Executive Summary

The CellScript compiler is well-gated and largely self-consistent. The
gate scripts, baseline JSON, and snapshot tests enforce the bulk of the
backend rules, the strict-mode capability gate works, the NovaSeal
production-claim boundary is correctly fail-closed, and the assembler
Tier-1 surface is exactly the allow-list in `CODING_STYLE.md`. **Twelve
findings are decisive**, none of which are load-bearing for production
CKB claims today, but several are silent contract failures that could
ship as features without breaking any existing test:

1. **Protected-cell contract is silently escapable through `*`.** A lock
   body can `let owned = *t; consume owned;` and the type checker will
   accept it, because `Deref` on a `Ref` returns an owned linear value
   and `consume` succeeds. The `protected T` semantics ("the lock guards
   the cell, it does not own it") are documented in `AGENTS.md:188-190`
   and `CODING_STYLE.md:104-105` but the type checker does not enforce
   the "not consumed inside the lock body" half. *(Pass 2 / F1)*

2. **`validate_action_state_edges` does not require the body to
   consume/create the inputs/outputs of the state edge.** A `flow` with
   `Zero -> One` and an action that names both states can compile cleanly
   without ever calling `consume` on the input or `create` on the output.
   The flow module's `validate_state_transition_create` covers the
   create-side only when `state_field_span.is_some()`. *(Pass 2 / F2)*

3. **`emit_runtime_type_hash` / `emit_type_hash` fall back to a shared
   scratch buffer when no per-dest cell-buffer slot is allocated.** Two
   type-hash operations, or a type-hash followed by a destroy scan /
   mutate preserved-field check, will share `runtime_scratch_size_offset()`
   in the same function frame and corrupt each other. *(Pass 3 / F-1)*

4. **The six CKB protocol stubs (`sUDT`, `xUDT`, `ACP`, `Cheque`,
   `HTLC`, `Type ID`) and the stdlib signature tables
   (`StdLib::functions()`, `Collections::functions()`) are unreachable
   dead code.** No compiler phase queries them; only
   `generate_assembly` is consumed. The `protocol_function` records use
   `IrType::Ref(IrType::Named("Vec"))` parameter shapes that the parser
   cannot produce. *(Pass 4 / F1, F2)*

5. **NovaSeal BTC anchor template shape diverges from the contract.**
   `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json`
   nests fields inside a `btc_transaction_binding` sub-document, but
   `scripts/novaseal_btc_anchor_contract.py:30-91` and
   `novaseal_btc_spv_evidence_adapter.py:184-211` expect flat fields.
   A producer following the template would silently fail
   `public_btc_anchor_shape_matches_profile`. *(Pass 4 / F3)*

6. **`assumptions.rs` `required_reads` collapses all reads to `:*`
   wildcards and the validator only checks transaction-array length.**
   A builder can attach this assumption, drop every meaningful
   witness/output field, and still pass
   `validate_transaction_against_assumptions`. *(Pass 4 / F5)*

7. **`optimize::optimize_module` constant-folds `if` and discards the
   unselected branch.** If the dropped branch held the unique `consume`
   of a linear binding, the type-checker rerun (which only checks "the
   optimized AST is well-typed in isolation") passes a body that lost
   its linear effect. *(Pass 2 / F4)*

8. **The two `emit_*_outgoing_stack_arg_store` helpers in `calls.rs`
   and `abi.rs` diverge in their `avoid` set.** The call-side uses bare
   `emit_stack_store`; the entry-side uses
   `emit_stack_store_avoiding(..., &[ENTRY_ABI_CURSOR_REG])`. A future
   change to `ENTRY_ABI_CURSOR_REG` would be reflected in one site and
   not the other. *(Pass 3 / F-3)*

9. **`TokenKind::Invariant` is missing from the parser's top-level
   boundary detection (`looks_like_top_level_item_decl`)**. A sibling
   `invariant` item after an action body can silently fold into the
   action body. *(Pass 1 / C-1)*

10. **`tests/v0_14.rs` does not exercise its own named version's
    compat/strict semantics.** The 0.14 capability acceptance path is
    only verified through the CLI; the library-level `compile()` API
    with `primitive_compat: Some("0.14")` is untested. *(Pass 1 / H-1)*

11. **`TokenKind::Pipe` is dead lexer output.** Lexer produces it; no
    parser handler consumes it; no `TokenKind::Invalid('|')` either.
    *(Pass 1 / H-2)*

12. **`proof_plan/soundness.rs` does not gate `status ==
    "checked-partial"` in strict mode**, so a partial-checked plan can
    pass `--primitive-strict 0.16`. *(Pass 4 / F16)*

## Methodology Notes

- **Cross-comparator was the published contract.** Where possible, the
  audit used the compiler's own documents (`AGENTS.md`, `CODING_STYLE.md`,
  `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`) as the reference. The
  iCKB Rust source under `/home/arthur/a19q3/ickb-v1-core` was used
  only where the compiler emits programs that *should* match upstream
  CKB Cell-model semantics (proof-plan soundness, ABI, syscall tables).
- **Baseline JSON was treated as a contract.** `backend_shape_baseline.json`
  and `syscall_abi_baseline.json` are pinned by `tests/examples.rs` and
  `tests/assembly_snapshots.rs`. The audit did not propose baseline
  changes; only emission-path changes.
- **Production claim boundary was preserved.** NovaSeal devnet-only
  paths that emit `local_devnet_passed_external_endpoint_required` were
  *not* suggested for upgrade to `status=passed`. The boundary phrase is
  correctly required by `cellscript_gate.sh:381-422`
  (`check_novaseal_acceptance_boundaries`) and remains in place.

## Per-Pass Findings

### Pass 1 — Parser + Lexer

**Scope:** `src/lexer/{mod.rs,token.rs}`, `src/parser/mod.rs`,
`src/ast/mod.rs`, `src/fmt/mod.rs`, `tests/syntax_combo/`,
`tests/v0_14.rs`, `tests/v0_16.rs`.

**Decisive findings:**
- `TokenKind::Invariant` missing from top-level boundary detection →
  silent corruption of action-body parsing when followed by `invariant`.
- `tests/v0_14.rs` does not exercise the v0.14 compat/strict acceptance
  path at the library level.
- `TokenKind::Pipe` is dead lexer output.
- `if cond expr else expr` (unbraced branch) round-trips through the
  parser but not through the formatter.
- `Match` arm patterns are stringly-typed — a feature-boundary concern.

**Doc drift:**
- `docs/wiki/Tutorial-02-Language-Basics.md:232-237` correctly notes
  the `transfer`/`destroy` capability gate is `primitive-strict`, but
  the default mode (no flag) is **also v0.14 compat** (per `lib.rs:81-83`).
  This is not stated clearly in the docs.
- `validate_callable_param_reference_shape` (`types/mod.rs:2884-2890`)
  suggests `read name: T` for a "read-only referenced cell", but no such
  syntax exists in the AST (`ast/mod.rs:290-298` only enumerates
  `Default, Input, Output, Protected, Witness, LockArgs`). Stale error
  message.

### Pass 2 — Types + Flow + Optimize + Resolve

**Scope:** `src/types/mod.rs` (~10k lines), `src/flow/mod.rs`,
`src/optimize/mod.rs`, `src/resolve/mod.rs`, plus `lib.rs`
(`compile_ast_with_build`, `check_primitive_strict_015`).

**Decisive findings:**
- Protected-cell contract escapable via `*` (F1).
- `validate_action_state_edges` does not require body consumption
  (F2).
- Optimizer constant-`if`-fold may drop the unique linear consume (F4).
- `is_linear_type` returns `false` for `Type::Ref(_)` and `Type::MutRef(_)`,
  so transitively-moved linear values through `*t` are tracked but the
  underlying reference contract is not enforced.
- `resolve::validate_loaded_import_target` early-returns `Ok(())` for
  unregistered target modules, producing misleading later errors.
- `resolve_type` uses a single `rsplit_once("::")` to look up paths,
  losing everything before `inner::Type` for nested module qualifiers.

**Doc drift:**
- `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md:48-56` defines
  `Returned(name, ty)` semantics; the checker's `LinearState` does not
  implement this (it has `Available, Consumed, Transferred, Destroyed`).
  Spec drift.
- `AGENTS.md:139` lists `retarget_type` as a kernel effect for 0.15;
  the AST enum has it but no type-checker path validates it.

### Pass 3 — IR + Codegen + Assembler + ABI

**Scope:** `src/ir/mod.rs` (~10k lines), `src/codegen/*.rs`
(mod, frame, expr, calls, schema, cell_ops, abi, collections, runtime,
assembler), `tests/assembly_snapshots.rs`, baseline JSONs.

**Decisive findings:**
- F-1: shared `runtime_scratch_*` fall-back races type-hash with
  destroy-scan and mutate preserved-field checks in the same frame.
- F-3: `emit_outgoing_call_stack_arg_store` (`calls.rs`) and
  `emit_entry_outgoing_stack_arg_store` (`abi.rs`) diverge in their
  `avoid` set.
- F-2: `emit_unaligned_scalar_load` avoids-set can contain `dest_reg`
  twice; helper does not deduplicate.
- F-4: `add t0, sp, reg` + `addi t0, t0, N` in `abi.rs:394-395, 469-470,
  662-664` bypasses `emit_sp_addi`.
- F-6: `CELL_OPS_SCAN_INDEX_REG` and `CELL_OPS_U128_EXPECTED_HI_REG`
  both pinned to `t6` — single-register conflict if a future code path
  uses both in the same scope.
- F-8: `emit_dynamic_table_vector_append_check` (`cell_ops.rs:561-580`)
  reuses `t4` and `a0` across a loop / helper-call boundary without an
  explicit avoid set.
- F-9 / F-14: destruction policies `Instance { identity_field }` and
  `BurnAmount { field }` are recorded as "metadata-visible and
  runtime-required" but the backend only emits a comment; there is no
  fail-closed verifier if the IR/type-checker mark is missing.

**Codegen boundary compliance (POSITIVE):**
- `cell_ops.rs` contains only cell-operation policy; no general
  type-width code. ✓
- `schema.rs` contains only layout computation and type-width helpers;
  no cell-operation policy. ✓
- The assembler Tier-1 instruction set matches `CODING_STYLE.md`
  exactly; no CSR/atomics/FP/compressed/`fence`/`tail` mnemonics. ✓
- Branch relaxation (`assembler.rs:1474-1504`) correctly excludes
  `t6` from the 12-byte fall-back sequence. ✓
- `emit_fail` is used in `cell_ops.rs:1700, 1730, 1788, 1794` and
  pinned by tests for unsupported runtime semantics. ✓
- Constants use `.rodata` labels via `const_data_label_for_bytes` and
  `emit_const_data_pool` (`mod.rs:295-303`); no placeholder labels. ✓

### Pass 4 — ABI + Stdlib + Runtime + ProofPlan + NovaSeal

**Scope:** `src/stdlib/`, `src/syscalls.rs`, `src/runtime_errors.rs`,
`src/assumptions.rs`, `src/proof_plan/`, `src/verifier_registry/btc.rs`,
`src/cli/novaseal_certification.rs`, `proposals/novaseal/`,
`scripts/novaseal_*.py`, `scripts/cellscript_gate.sh`.

**Decisive findings:**
- F1: `src/stdlib/ckb_protocols/*.rs` are unreachable dead code.
- F2: `StdLib::functions()` and `Collections::functions()` signature
  tables are dead; only `generate_assembly` is wired.
- F3: NovaSeal `public_btc_spv_evidence.template.json` nests anchor
  fields inside `btc_transaction_binding`; the contract and adapter
  expect flat fields.
- F5: `assumptions.rs` `required_reads` wildcard + length-only validator.
- F16: `proof_plan/soundness.rs` does not gate `checked-partial` in
  strict mode.

**NovaSeal production-claim boundary (POSITIVE — preserved):**
- `novaseal_certification.rs:2308-2309` correctly returns
  `local_devnet_passed_external_endpoint_required` only when
  `local_live_acceptance_passed && external_required`. ✓
- `production_gate_status` (l.1498-1513) returns
  `local_production_prep_ready_external_attestation_required` for
  local-only runs, never `passed`. ✓
- `check_novaseal_verifier_pinning` in the gate (`cellscript_gate.sh:
  166-289`) recomputes the verifier ELF SHA-256/Blake2b and matches
  every `Cell.toml` CellDep + the public/external attestation
  templates + `proofplan_mapping.json`. Currently pinned
  `data_hash = "0xcf90469f…762e"`, `artifact_hash = "0xb8a7e8e5…37459"`,
  size `187808` bytes — all consistent. ✓
- The phrase `local_devnet_passed_external_endpoint_required` is
  present at `novaseal_certification.rs:2309` and
  `scripts/novaseal_devnet_stateful_acceptance.sh:101`, both required
  by `check_novaseal_acceptance_boundaries` (`cellscript_gate.sh:385,
  403`). ✓

## Severity-Rolled Finding Index

| ID | Severity | Pass | File:line | Summary |
|---|---|---|---|---|
| F1 | **HIGH** | P2 | `src/types/mod.rs:3964-3967` | `*protected_t` produces an owned linear value; consume succeeds. Protected-cell contract broken. |
| F2 | **HIGH** | P2 | `src/types/mod.rs:2347-2471` | `validate_action_state_edges` does not require the body to consume/create the inputs/outputs of the state edge. |
| F-1 | **HIGH** | P3 | `src/codegen/mod.rs:2361-2362, 2295-2301` | Shared `runtime_scratch_*` fall-back races type-hash with destroy-scan. |
| F-2 | **HIGH** | P3 | `src/codegen/mod.rs:2233-2242` | `emit_unaligned_scalar_load` avoid-set contains `dest_reg` twice; no dedup. |
| F-3 | **HIGH** | P3 | `src/codegen/calls.rs:578-593` vs `abi.rs:669-681` | Two `emit_*_outgoing_stack_arg_store` helpers diverge in avoid-set. |
| F1 | **HIGH** | P4 | `src/stdlib/ckb_protocols/*.rs` | Unreachable dead protocol stubs. |
| F2 | **HIGH** | P4 | `src/stdlib/{mod,collections}.rs` | Signature tables dead; only `generate_assembly` wired. |
| F3 | **HIGH** | P4 | `scripts/novaseal_btc_anchor_contract.py:30-91` vs `public_btc_spv_evidence.template.json:31-46` | BTC anchor template nests fields; contract expects flat. |
| C-1 | **CRITICAL** | P1 | `src/parser/mod.rs:1539-1555` | `TokenKind::Invariant` missing from top-level boundary detection. |
| H-1 | **HIGH** | P1 | `tests/v0_14.rs` | v0.14 compat/strict acceptance not exercised at library level. |
| H-2 | **HIGH** | P1 | `src/lexer/{mod.rs:481, token.rs:195}` | `TokenKind::Pipe` is dead lexer output. |
| H-3 | **HIGH** | P1 | `src/fmt/mod.rs` + `parser/mod.rs:3033-3045` | Unbraced `if cond expr else expr` does not round-trip. |
| H-4 | **HIGH** | P1 | `src/ast/mod.rs:678-683` | `MatchArm.pattern: String` — stringly-typed patterns. |
| F4 | **MEDIUM-HIGH** | P2 | `src/optimize/mod.rs:132-137` | `if` constant-folding may drop the unique linear consume. |
| F5 | **MEDIUM** | P4 | `src/assumptions.rs:278-280, 163-167` | `required_reads` wildcard + length-only validator. |
| F6 | **MEDIUM** | P4 | `scripts/novaseal_bip340_tcb_review.py:101-104` | Substring `unsafe`/`panic!` matching is low-precision. |
| F7 | **MEDIUM** | P4 | `src/proof_plan/soundness.rs:135-144` | Soundness report uses `strict=false` by default; PP0150 only fires under `is_assurance_strict_016()`. |
| F16 | **MEDIUM** | P4 | `src/proof_plan/soundness.rs` | `status == "checked-partial"` not gated in strict mode. |
| F-4 | **MEDIUM** | P3 | `src/codegen/abi.rs:394-395, 469-470, 662-664` | `add t0, sp, reg` + `addi t0, t0, N` bypasses `emit_sp_addi`. |
| F-6 | **MEDIUM** | P3 | `src/codegen/cell_ops.rs:158-160, 766` | `CELL_OPS_SCAN_INDEX_REG` and `CELL_OPS_U128_EXPECTED_HI_REG` both pinned to `t6`. |
| F-8 | **MEDIUM** | P3 | `src/codegen/cell_ops.rs:561-580` | `emit_dynamic_table_vector_append_check` loop reuses `t4` and `a0` across helper-call boundary without avoid set. |
| F-9 / F-14 | **MEDIUM** | P3 | `src/codegen/cell_ops.rs:43-49, mod.rs:2430-2445` | `Instance` / `BurnAmount` destruction policies only emit comments, no fail-closed verifier. |
| F17 | **LOW** | P2 | `src/resolve/mod.rs:197-203` | `validate_loaded_import_target` silently passes for unregistered modules. |
| F18 | **LOW** | P2 | `src/resolve/mod.rs:250-256` | `rsplit_once("::")` discards nested module qualifiers. |
| F4 | **LOW** | P4 | `src/stdlib/mod.rs:166-175, 232-242` | `ConsumeInvalidOperand` reused for target-profile rejection (should be a distinct code). |
| F8 | **LOW** | P4 | `src/codegen/runtime.rs:144-188` | `__env_current_timepoint` and `__ckb_header_epoch_number` differ only in profile gate. |
| F11 | **LOW** | P4 | `src/syscalls.rs:738` | `Deprecated` label for fail-closed collection helpers is misleading. |
| F13 | **LOW** | P4 | `src/simulate.rs:142-146` | Simulator is AST-level only; stdlib calls are traced-only — should be documented. |

## Doc Claims Contradicted Or Stale

| # | Source | Claim | Reality | Verdict |
|---|---|---|---|---|
| 1 | `AGENTS.md:188-190` / `CODING_STYLE.md:104-105` | "`protected T` is a typed view of one selected input Cell guarded by the current lock invocation. It is **not** a global scan or an output Cell." | `*t` produces an owned linear value; `consume` succeeds. Type checker does not enforce "not consumed inside the lock body." | **Partially false** — the borrow view is enforced, the consumption ban is not. |
| 2 | `docs/CELLSCRIPT_LINEAR_OWNERSHIP.md:22` | "references rooted in linear values cannot outlive the root value" | `reject_stored_linear_reference_alias` covers the local case; deref of `protected` is not checked. | **Incomplete** — implementation gap. |
| 3 | `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md:48-56` | `Returned(name, ty)` semantics | `LinearState` has `Available, Consumed, Transferred, Destroyed`; `Returned` not implemented. | **Stale** — spec drift. |
| 4 | `AGENTS.md:139` | "0.15 — ... kernel-effect capabilities (`create`, `consume`, `replace`, `burn`, `relock`, `retarget_type`, ...)" | `retarget_type` is in the AST enum but no type-checker path validates it. | **Stale** — capability listed, not enforced. |
| 5 | `src/types/mod.rs:2884-2890` error message | "use `read name: T` for a read-only referenced cell" | No `ParamSource::Read` in `ast/mod.rs:290-298`. | **Stale** — message references non-existent syntax. |
| 6 | `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json:31-46` | Anchor fields nested under `btc_transaction_binding` | `novaseal_btc_anchor_contract.py:30-91` and `novaseal_btc_spv_evidence_adapter.py:184-211` expect flat fields. | **Internal drift** — template vs contract vs adapter. |
| 7 | `src/stdlib/ckb_protocols/{sudt,xudt,type_id,htlc,cheque,acp}.rs:16` `stability: "schema-stub"` | Honest "schema-stub" label | The stubs are dead code; nothing consumes them. | **Honest but unused** — labels correct, surface unreachable. |

## Recommended Next Steps (Priority Order)

1. **(F1, P2)** Reject `Expr::Deref` whose root is a `protected` parameter
   inside a lock body, in `validate_callable_param_source`
   (`types/mod.rs:2787`).
2. **(F2, P2)** Extend `validate_action_state_edges` to require that
   the action body consumes `path.base` and creates `to_path.base` with
   the declared `to` state. Mirror the existing
   `validate_state_transition_create` for the consume side.
3. **(F-1, P3)** In `emit_runtime_type_hash` / `emit_type_hash`
   (`codegen/mod.rs:2361-2362, 2295-2301`), fail closed when no
   per-dest cell-buffer slot exists, instead of falling back to
   `runtime_scratch_*`.
4. **(C-1, P1)** Add `TokenKind::Invariant` to
   `looks_like_top_level_item_decl` (`parser/mod.rs:1539-1555`).
5. **(F1+F2, P4)** Either wire the protocol stubs and stdlib signature
   tables into the resolver/IR generator, or move them to
   `tests/fixtures/` behind `#[cfg(test)]`.
6. **(F3, P4)** Either flatten
   `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json`
   to match the contract shape, or add a normalizer in
   `novaseal_btc_spv_evidence_adapter.py` that unwraps
   `btc_transaction_binding`. Add a regression test.
7. **(F-3, P3)** Unify the two `emit_*_outgoing_stack_arg_store`
   helpers into a single shared helper in `calls.rs` (or new
   `abi_stack_args.rs`) that takes an explicit `avoid: &[&str]`.
8. **(F5, P4)** Tighten `assumptions.rs` validator so that
   `required_inputs: ["input:*"]` requires the transaction to bind a
   specific cell (lock/type hash or index), not merely a non-zero count.
9. **(F4, P2)** In `optimize::optimize_module` constant-fold of `if`,
   before returning the kept branch, require the dropped branch's linear
   effects are replicated.
10. **(F16, P4)** Add a strict-only PP0xxx check on
    `status == "checked-partial"` in `proof_plan/soundness.rs`.
11. **(H-1, P1)** Add library-level tests in `tests/v0_14.rs` that
    exercise `compile()` with `primitive_compat: Some("0.14"|"0.15"|"0.16")`.
12. **(H-2, P1)** Drop `TokenKind::Pipe` from the lexer; let `|` produce
    `TokenKind::Invalid('|')`.
13. **(H-3, P1)** Either round-trip the unbraced `if cond expr else expr`
    form in the formatter, or remove the unbraced branch from
    `parse_branch_expr` (`parser/mod.rs:3033-3045`).
14. **(F-6, P3)** Rename `CELL_OPS_U128_EXPECTED_HI_REG` from `t6` to a
    non-reserved register (e.g. `t5`) and add a compile-time assertion
    `CELL_OPS_SCAN_INDEX_REG != CELL_OPS_U128_EXPECTED_HI_REG`.
15. **(F17, P2)** Change `validate_loaded_import_target`
    (`resolve/mod.rs:197-203`) to error if the target module is not
    registered, instead of silently passing.

## Honesty Boundary

This swarm audit is a source-comparison audit, not a CKB VM behavioural
differential. It does not execute generated CellScript artifacts. Its
value is to demonstrate that the compiler has silent contract gaps in
the parser boundary detection, type checker, codegen scratch-allocation,
stdlib table wiring, NovaSeal anchor templates, and builder-assumption
validator — not to enumerate every byte-level difference.

All findings are corroborated by file:line citations. No findings are
based on speculative behaviour. The audit did not modify any source
files. The compiler's release gate (`cellscript_gate.sh release`) was
not run; this audit is independent of the gate's pass/fail state.
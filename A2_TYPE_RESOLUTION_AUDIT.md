# Agent 2 Audit: Type / Name Resolution / Semantic Analysis

**Date**: 2026-05-28
**Branch**: nightly-0.16 (5c484496)
**Scope**: `src/types/mod.rs`, `src/resolve/mod.rs`, `src/flow/mod.rs`, `src/proof_plan/soundness.rs`, related semantic paths in `src/lib.rs`

---

## Executive Summary

The type system and name resolution are **architecturally sound** for CellScript's CKB-specific semantics. Linear resource tracking is well-designed with consume/transfer/destroy states, branch merging, and loop rejection. The resolver has proper duplicate symbol detection, import handling, and cycle detection. However, the audit identifies several findings ranging from **Medium** to **Low** severity that could lead to misleading diagnostics, missed validation, or edge-case bypasses.

**Critical Issues**: 0
**High Issues**: 0
**Medium Issues**: 5
**Low Issues**: 5
**Info/Nits**: 2

---

## Findings

### A2-01: `infer_expr_with_expected_type` ignores the expected type for non-literal expressions

- **File**: `src/types/mod.rs`, lines 3179-3188
- **Severity**: Medium
- **Description**: When `infer_expr_with_expected_type` is called with an expected type (e.g., from a `let` annotation, binary right-hand side, or return context), it only uses the expected type to refine integer literals and array literals. For all other expression forms, it calls `infer_expr` and **returns the inferred type without checking compatibility against the expected type**. This means the caller must separately validate compatibility, creating a fragile contract. Currently the `check_stmt_with_context` for `Let` does perform a separate `types_equal` check, but the `infer_expr_with_expected_type` name suggests it will handle this, and any future call sites may forget the extra check.
- **Evidence**:
  ```rust
  fn infer_expr_with_expected_type(&mut self, env: &mut TypeEnv, expr: &Expr, expected_ty: &Type, span: Span) -> Result<Type> {
      match expr {
          Expr::Integer(value, _) => self.infer_integer_literal_with_expected_type(*value, expected_ty, span),
          Expr::Array(elems, _) => self.infer_array_literal_with_expected_type(env, elems, expected_ty, span),
          _ => {
              let actual_ty = self.infer_expr(env, expr)?;
              Ok(actual_ty) // <-- expected_ty is silently discarded
          }
      }
  }
  ```
- **Reproduction**: A `.cell` file is not directly affected today because `check_stmt_with_context` does its own `types_equal` check for `let` statements. However, if `infer_expr_with_expected_type` is used for binary operand contextualization (lines 3296-3307), the inferred type returned may not match `expected_ty`, and subsequent validation may miss mismatches for complex expression types.
- **Why current tests miss it**: Tests focus on integer widening boundaries, not on non-literal expressions passed through this path.
- **Fix direction**: Either rename the function to clarify its limited scope (e.g., `infer_expr_with_integer_and_array_coercion`) or add a compatibility check for the catch-all case.
- **Regression test**: Add a test that calls `infer_expr_with_expected_type` with a struct literal and a mismatched expected type, verifying the result is still validated.

---

### A2-02: `process_import` does not validate that imported symbols actually exist in the target module

- **File**: `src/resolve/mod.rs`, lines 169-181
- **Severity**: Medium
- **Description**: When processing a `use` statement, `process_import` only checks for empty paths and duplicate local symbols. It does **not** verify that the imported name actually exists in the target module's symbol table. The import is recorded as a `(local_name, full_path)` pair in `symbol_table.imported` without checking whether the target module and symbol exist. This means invalid imports are deferred to `check_circular_deps` (which only checks module existence) or to later type resolution where the import resolves to `None`.
- **Evidence**:
  ```rust
  fn process_import(&mut self, symbol_table: &mut SymbolTable, import: &ImportItem) -> Result<()> {
      if import.module_path.is_empty() || import.name.is_empty() {
          return Err(CompileError::new("empty import path", import.span));
      }
      let full_path = import.module_path.iter().chain(std::iter::once(&import.name)).cloned().collect::<Vec<_>>().join("::");
      let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());
      Self::ensure_symbol_available(symbol_table, &local_name, import.span)?;
      symbol_table.imported.insert(local_name, full_path);
      Ok(())
  }
  ```
- **Reproduction**:
  ```cell
  module app
  use nonexistent_module::FakeType
  // No error at import registration time
  // Error only when FakeType is actually used in code
  ```
- **Why current tests miss it**: Tests register modules with valid symbols, then import them. No test imports a symbol that doesn't exist in the target module and verifies an early error.
- **Fix direction**: In `process_import`, resolve the target module from `import.module_path`, then verify the imported name exists in that module's symbol table. This produces a better error at the import site rather than at usage.
- **Regression test**: Register a module with type `Foo`, then try `use that_module::Bar` and verify the error message mentions the unresolved import symbol.

---

### A2-03: `Span::default()` used in error diagnostics loses source location

- **File**: `src/types/mod.rs`, lines 192, 201, 307, 5831, 5843, 5849, 5872, 5884, 5888, 5892, 2885
- **Severity**: Medium
- **Description**: Multiple error paths use `Span::default()` instead of carrying a meaningful source span. The most impactful cases are in `set_linear_state` (line 192, 201) and `check_linear_complete` (line 307), where linear resource errors point to no source location. Similarly, `validate_named_type` errors (lines 5831, 5843, 5849, 5872) and `vec_type_argument` errors all use `Span::default()`.
- **Evidence**:
  ```rust
  // Line 192
  return Err(CompileError::new(format!("resource '{}' already {:?}", name, state), Span::default()));
  // Line 307
  Span::default(),
  // Line 5872
  Err(CompileError::new(format!("unknown type '{}'", name), Span::default()))
  ```
- **Reproduction**: Write a `.cell` module that fails linear completeness, e.g., a function that takes a linear resource parameter and returns without consuming it. The error will lack a source position.
- **Why current tests miss it**: Tests check error messages but don't assert on span contents.
- **Fix direction**: Thread the relevant `Span` through to these call sites. For `set_linear_state`, the `TypeEnv` could store a `span: Span` alongside each linear state. For `validate_named_type`, the type-use span should be threaded from the caller.
- **Regression test**: After fix, assert that error spans have non-zero line/column numbers.

---

### A2-04: `type_contains_mutable_reference` uses string matching for `Named` types

- **File**: `src/types/mod.rs`, lines 2222-2230
- **Severity**: Medium
- **Description**: The function `type_contains_mutable_reference` uses `name.contains("&mut ")` (string matching) to detect mutable references in `Named` types. This is fragile because: (a) it depends on the specific formatting of the type name string, (b) a type named `MyStruct_mut_Foo` or a type whose name legitimately contains `&mut ` would be a false positive, (c) it's inconsistent with the structured matching used for `Ref`/`MutRef` variants of the `Type` enum.
- **Evidence**:
  ```rust
  fn type_contains_mutable_reference(ty: &Type) -> bool {
      match ty {
          Type::MutRef(_) => true,
          Type::Array(inner, _) => Self::type_contains_mutable_reference(inner),
          Type::Tuple(items) => items.iter().any(Self::type_contains_mutable_reference),
          Type::Named(name) => name.contains("&mut "), // <-- string matching
          _ => false,
      }
  }
  ```
- **Reproduction**: If a user defines a type named `struct &mut Trap {}` or if a generated type name contains the substring `&mut `, the function returns `true` incorrectly.
- **Why current tests miss it**: No test exercises `type_contains_mutable_reference` with edge-case type names.
- **Fix direction**: Parse the `Named` type string to extract its generic arguments and check each with structured `Type` matching (similar to `type_contains_reference`), or avoid storing types as strings in `Named` variants for types that could contain references.
- **Regression test**: Add a unit test calling `type_contains_mutable_reference` with `Type::Named("Vec<&mut Token>".to_string())` and verify correct behavior.

---

### A2-05: `named_type_contains_reference` uses overly broad string matching

- **File**: `src/types/mod.rs`, lines 2245-2247
- **Severity**: Medium
- **Description**: `named_type_contains_reference` checks `name.contains("read_ref ")` or `name.contains('&')`. The `&` check will match any type name containing an ampersand, which could be a false positive for types that legitimately contain `&` in their string representation (e.g., hypothetical generated names). More importantly, `read_ref ` (with trailing space) is a magic string that must exactly match how the formatter writes `read_ref` types.
- **Evidence**:
  ```rust
  fn named_type_contains_reference(&self, name: &str) -> bool {
      name.contains("read_ref ") || name.contains('&')
  }
  ```
- **Reproduction**: Any `Named` type whose name string contains `&` (even as part of a different token) triggers a false positive reference detection.
- **Why current tests miss it**: No test with a type name containing `&` that is not actually a reference.
- **Fix direction**: Parse the named type string into structured type components before checking, or add a `Type::ReadRef` variant instead of encoding reference kinds in strings.
- **Regression test**: Test with a type named `Tom&Jerry` (contrived but valid string) and verify it's not flagged as containing a reference.

---

### A2-06: `is_linear_type` does not account for `Ref`/`MutRef` wrapping linear types

- **File**: `src/types/mod.rs`, lines 6073-6087
- **Severity**: Low
- **Description**: `is_linear_type` checks `Array`, `Tuple`, and `Named` types for linearity, but returns `false` for `Ref(inner)` and `MutRef(inner)` without checking if `inner` is linear. This means a `&Token` (reference to a linear type) is not itself considered linear. This is actually **correct behavior** for CellScript semantics (references don't own the resource), but the asymmetry with `Array` (which does propagate) could confuse maintainers.
- **Evidence**:
  ```rust
  fn is_linear_type(&self, ty: &Type) -> bool {
      match ty {
          Type::Array(inner, _) => self.is_linear_type(inner),
          Type::Tuple(items) => items.iter().any(|item| self.is_linear_type(item)),
          Type::Named(name) => { ... }
          _ => false,  // Ref and MutRef fall through here
      }
  }
  ```
- **Reproduction**: Not directly exploitable; references to linear values cannot be consumed/transferred/destroyed (the `require_named_linear_cell_operand` function requires a named identifier, and references are rejected). This is a code clarity issue.
- **Fix direction**: Add explicit `Type::Ref(_) | Type::MutRef(_) => false` match arms with a comment explaining the semantic rationale.
- **Regression test**: Add a comment-only test documenting the intended behavior.

---

### A2-07: `resolve_type_global` and `resolve_type_global_with_module` have non-deterministic resolution order

- **File**: `src/resolve/mod.rs`, lines 291-297, 231-237
- **Severity**: Low
- **Description**: The global resolution functions iterate over `self.symbol_tables` (a `HashMap`) and return the first unique match. Since `HashMap` iteration order is non-deterministic, this could resolve to different modules across different program executions (though in practice the ambiguity rejection logic means it will usually return `None` when there are 2+ matches). The `resolve_type_global_with_module` returns the "first unique match", but this is an arbitrary one if only one module defines the type. This is benign as long as types are unique per-program or are imported with module qualification, but it means the resolver's behavior can technically vary across runs.
- **Evidence**:
  ```rust
  pub fn resolve_type_global(&self, name: &str) -> Option<TypeDef> {
      let symbol = name.rsplit("::").next().unwrap_or(name);
      let mut matches = self.symbol_tables.values().filter_map(|table| table.types.get(symbol).cloned());
      let resolved = matches.next()?;
      matches.next().is_none().then_some(resolved)
  }
  ```
- **Reproduction**: Register two modules with different types of the same name, remove the ambiguity rejection, and observe which module resolves first varying across runs.
- **Why current tests miss it**: Tests don't exercise the non-deterministic path; they only check that ambiguity returns `None`.
- **Fix direction**: Use `BTreeMap` or sort modules alphabetically before resolution to ensure deterministic behavior. Or document that global resolution is a best-effort fallback.
- **Regression test**: Add a test with two modules having the same type name and verify consistent resolution behavior.

---

### A2-08: `check_module` registers types before validating them

- **File**: `src/types/mod.rs`, lines 500-616
- **Severity**: Low
- **Description**: In `check_module`, type definitions (Resource, Shared, Receipt, Struct) are registered into `type_fields`, `linear_types`, `cell_type_kinds`, etc. during the first pass (lines 520-571), but their field types are not validated until `check_item` runs later (line 613). This means a type with invalid field types is registered as valid in the type system during the first pass, and could theoretically be referenced by other types before the field validation catches the error.
- **Evidence**:
  ```rust
  // Line 520-528: registers without validating field types
  Item::Resource(resource) => {
      register_type_id(&mut seen_type_ids, &resource.name, resource.type_id.as_ref())?;
      self.linear_types.insert(resource.name.clone());
      // ...
      self.type_fields.insert(resource.name.clone(),
          resource.fields.iter().map(|field| (field.name.clone(), field.ty.clone())).collect());
  }
  // Line 612-614: validates later
  for item in &module.items {
      self.check_item(item)?;
  }
  ```
- **Reproduction**: Define a `resource Foo { x: UnknownType }` and a `resource Bar { y: Foo }`. `Bar`'s fields reference `Foo`, which is registered. `Foo`'s field validation fails, but `Bar` was already registered with a reference to the valid `Foo` name. In practice, `validate_local_schema_type_graph_acyclic` and `check_item` will catch this, but the ordering creates a window where internal state is inconsistent.
- **Why current tests miss it**: Tests use valid field types.
- **Fix direction**: Validate field types during the registration loop (before `check_item`), or do registration in two passes: first register names, then validate and register fields.
- **Regression test**: Define a resource with an invalid field type and verify the error is caught before any dependent type registration.

---

### A2-09: `flow/mod.rs` state tracking only follows `Identifier` patterns for consume/transfer

- **File**: `src/flow/mod.rs`, lines 121-137
- **Severity**: Low
- **Description**: The flow state tracking in `collect_state_context_from_expr` only follows `Expr::Identifier` patterns for consume and transfer expressions. If a linear value is aliased through a `let` binding and then consumed, the flow context may not track the consumption correctly because the `consume.expr` may be an `Identifier` of the alias, not the original flow-tracked variable.
- **Evidence**:
  ```rust
  Expr::Consume(consume) => {
      if let Expr::Identifier(name, _) = consume.expr.as_ref() {
          if let Some(ty) = context.variable_flow_types.get(name) {
              context.consumed_flow_types.insert(ty.clone());
          }
      }
      // If consume.expr is NOT a simple identifier, consumption is not tracked
  }
  ```
- **Reproduction**: Create a flow-tracked variable, bind it with a `let` alias, then consume the alias. The flow module won't track the consumption through the alias.
- **Why current tests miss it**: Tests use direct variable names in consume/transfer.
- **Fix direction**: Extend the tracking to follow `let` binding aliases, or validate in the type checker that consume/transfer only operates on named identifiers (which is already done via `require_named_linear_cell_operand` in the type checker, making this a secondary check).
- **Regression test**: Add a flow test with aliased variables being consumed.

---

### A2-10: `types_equal` does not normalize `Named` type names (e.g., `Token` vs `mod::Token`)

- **File**: `src/types/mod.rs`, lines 5898-5918
- **Severity**: Low
- **Description**: `types_equal` compares `Named` types by exact string equality (`a1 == b1`). However, the same type might be referred to as `"Token"` in one context and `"app::Token"` in another (from qualified references). This means two types that are semantically identical could be considered unequal, leading to false type mismatch errors.
- **Evidence**:
  ```rust
  (Type::Named(a1), Type::Named(b1)) => a1 == b1,
  ```
- **Reproduction**: In a module that imports `Token`, if one expression produces `Type::Named("Token")` and another produces `Type::Named("cellscript::token::Token")`, `types_equal` returns `false` even though they refer to the same type.
- **Why current tests miss it**: Tests use consistent naming within a single module.
- **Fix direction**: Add a normalization step that strips module prefixes when comparing, or ensure all type references are normalized to a canonical form during type construction.
- **Regression test**: Create a module that imports a type, uses it both qualified and unqualified, and verify type equality.

---

### A2-11: Duplicate `TypeDependencyEdge` / `TypeGraphVisitState` / `visit_type_dependency_graph` definitions

- **File**: `src/resolve/mod.rs` lines 12-23, 534-561 AND `src/types/mod.rs` lines 80-90, 6170-6198, 6115-6168
- **Severity**: Low (code quality)
- **Description**: The `TypeDependencyEdge`, `TypeGraphVisitState`, and `visit_type_dependency_graph` function are duplicated between `resolve/mod.rs` and `types/mod.rs`. The implementations are similar but not identical: the `types/mod.rs` version uses a `local_types: &HashSet<String>` parameter while the `resolve/mod.rs` version uses `self.resolve_type_with_module`. This duplication risks divergence.
- **Evidence**: Compare `src/resolve/mod.rs:374-413` with `src/types/mod.rs:1197-1250` and the corresponding visitor functions.
- **Fix direction**: Extract a shared `type_graph` module with a single implementation parameterized by the resolution strategy.
- **Regression test**: N/A (refactoring correctness).

---

### A2-12: `infer_call_type` for `Vec::with_capacity` returns unparameterized `Named("Vec")`

- **File**: `src/types/mod.rs`, lines 5060-5066
- **Severity**: Low
- **Description**: `Vec::with_capacity` returns `Type::Named("Vec".to_string())` without a type parameter. Later code that calls `vec_type_argument` on this type will fail because it lacks `<T>`. This is likely intentional (the Vec is empty and its type will be refined by later push operations), but it means the type system treats `Vec` and `Vec<Token>` as different types, and a `Vec` returned from `with_capacity` won't match a `Vec<Token>` parameter type.
- **Evidence**:
  ```rust
  ("Vec", "with_capacity") => {
      self.validate_builtin_arity(name, 1, arg_types, call.span)?;
      if arg_types[0] != Type::U64 {
          return Err(CompileError::new("Vec::with_capacity expects a u64 capacity", call.span));
      }
      Type::Named("Vec".to_string()) // <-- no type parameter
  }
  ```
- **Reproduction**: Call `Vec::with_capacity(10)` and try to pass the result to a function expecting `Vec<Token>`. This will fail with a type mismatch.
- **Why current tests miss it**: No test exercises passing a `Vec` from `with_capacity` to a typed context.
- **Fix direction**: Either add a type inference mechanism for Vec (tracking element type through push operations) or document that `Vec::with_capacity` returns an untyped Vec that must be explicitly typed via let annotation.
- **Regression test**: Test that `let v: Vec<Token> = Vec::with_capacity(10)` typechecks correctly.

---

## Positive Findings

The following aspects are well-implemented:

1. **Linear resource tracking**: The `TypeEnv::LinearState` with `Available`/`Consumed`/`Transferred`/`Destroyed` states, branch merging via `merge_branch_linear_states`/`merge_match_linear_states`, and loop rejection via `reject_loop_linear_state_changes` are thorough and correct.

2. **Capability gating**: The `require_type_capability` and `require_capability_or_kernel_effects` functions properly enforce CKB-specific permission models for cell operations.

3. **Callable boundary enforcement**: `validate_expr_allowed_in_current_callable` correctly prevents pure functions from performing cell lifecycle operations and prevents locks from performing state transitions.

4. **Duplicate symbol detection**: `ensure_symbol_available` checks all four namespaces (types, functions, constants, imported) to prevent shadowing/confusion.

5. **Type dependency cycle detection**: Both `resolve/mod.rs` and `types/mod.rs` implement proper DFS-based cycle detection with span tracking.

6. **Branch obligation validation**: `validate_branch_obligations_in_stmts` and `validate_lifecycle_effects_in_stmts` ensure that actions properly handle all output bindings across all control flow paths.

7. **Name resolution with module qualification**: The resolver correctly handles qualified paths (`module::Type`), imports with aliases, and ambiguity rejection in global resolution.

---

## Summary Table

| ID | Severity | Component | Summary |
|----|----------|-----------|---------|
| A2-01 | Medium | types | `infer_expr_with_expected_type` silently discards expected type |
| A2-02 | Medium | resolve | Import processing does not verify symbol existence |
| A2-03 | Medium | types | `Span::default()` in 11 error sites loses source location |
| A2-04 | Medium | types | String-based `&mut ` detection in `type_contains_mutable_reference` |
| A2-05 | Medium | types | Overly broad `&` matching in `named_type_contains_reference` |
| A2-06 | Low | types | `is_linear_type` doesn't match Ref/MutRef (by design) |
| A2-07 | Low | resolve | Non-deterministic global resolution order |
| A2-08 | Low | types | Types registered before field validation |
| A2-09 | Low | flow | Flow state tracking misses aliased variables |
| A2-10 | Low | types | `types_equal` doesn't normalize qualified names |
| A2-11 | Low | both | Duplicate type dependency cycle code |
| A2-12 | Low | types | `Vec::with_capacity` returns unparameterized Vec |

# Issue #105: IR consistency repair

Branch: `0.36`, based on `31c808c688ef3527759e82e3309d4570ae56222a`.
Date: 2026-11-07. Tracking: [issue #105](https://github.com/CellScript-Labs/CellScript/issues/105).
The [IR contract](../../CELLSCRIPT_IR_CONTRACT.md) explains phase ownership,
author-facing behavior, the before/after comparison and terminology.

## Implemented scope

- Reserve exact intrinsic representation type names across declarations and
  imported type aliases before inference. Direct IR/codegen admission rejects
  conflicting declarations, origins and nominal aliases. Ordinary records and
  actual builtin uses retain positive VM controls.
- Preserve checked U128 negation, including compound operands, casts, enclosing
  expressions and discarded helper arguments. Keep existing U64 behavior and
  reject source-invalid narrow negative literals rather than silently widening.
- Preserve statement-branch scopes and contextual returns. All three byte-loop
  matchers reject fusion when their prefix writes the induction variable.
- Preserve ordinary call provenance through ProofPlan, typed validation and
  runtime-specific machine/metadata consumers. Authentic runtime contracts
  retain their specialized checks.
- Reject ordinary action/helper calls into an exact ZK owner action in both
  compiler and independent checker admission. Direct action-entry verification
  remains supported; a nested-call profile is not introduced.
- Fix simulator statement/tail behavior, remove silent loop truncation and
  propagate field-evaluation failures. Explicit step exhaustion remains an error.
- Share the pure AST-to-IR projection, remove obsolete generic-text fallback
  and packed-wrapper readers, and add bounded structural IR validation through
  the existing exhaustive instruction traversal.
- Advance the cache generation to v200 and bind checker/lowering/source-map
  identities in its key. Document the unchanged-schema invalidation obligation.

The representation-name reservation is a source admission restriction. It is
not a new TypeId system, grammar or public wire-schema version. Existing
commitment V0 domain spellings are preserved. Rebuilding artifacts can change
ELF/code identities; prior cached products must not substitute for the repair.

## Validation receipts

Status: implemented and focused validation completed. The issue links the
implementation commit. These are component receipts, not one full final gate.

| Check | Result |
| --- | --- |
| Compiler library run | 1,212 passed; one obsolete frontend expectation corrected and passed on focused rerun |
| `allocated_runtime` component | 43 passed, including real-VM negation and scope/control-flow regressions |
| Final nine-target compiler integration run | 81 passed; breakdown below |
| Independent artifact checker | 109 unit tests, 16 integration tests and one compile-fail documentation test passed; one pre-existing documentation example remains ignored |
| `frozen_interface open_` | 3 passed |
| `frozen_interface instance_` | 5 passed |
| WASM target check with `--features wasm` | Passed after all source changes |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |

The final nine-target run passed `commitment_opening` (8), `effect_bindings`
(13), `exact_script_handles` (17), `external_verifier_scenarios` (1),
`sighash_zero_lock` (6), `structured_committed_types` (7),
`structured_nominal_types` (16), `trusted_external` (8) and `zk_transition` (5).
The call-binding suite covers 25 scalar runtime-looking labels and 8 view
property labels, real VM execution, and fully rebound hostile call records.

```sh
cargo test --locked -p cellscript --lib
cargo test --locked -p cellscript --lib optimizing_group_entries_preserves_scopes_and_rebuilds_occurrence_references
cargo test --locked -p cellscript --test allocated_runtime --test effect_bindings --test structured_nominal_types
cargo test --locked -p cellscript --test effect_bindings --test structured_nominal_types --test commitment_opening --test structured_committed_types --test sighash_zero_lock --test exact_script_handles --test zk_transition --test trusted_external --test external_verifier_scenarios
cargo test --locked -p cellscript-artifact-checker
cargo test --locked -p cellscript --test frozen_interface open_
cargo test --locked -p cellscript --test frozen_interface instance_
cargo check --locked -p cellscript-wasm --target wasm32-unknown-unknown --features wasm
cargo fmt --all -- --check
git diff --check
```

The first library run executed 1,213 tests: 1,212 passed and one frontend
expectation failed because it required constant statement branches to lose
their scope. The fixture now checks both retained occurrence references,
preserved scope and parse/format/type-check round-tripping. Its focused rerun
passed. This is not described as a clean first full-suite pass.

The initial effect-binding run exposed a second name-based consumer in
committed-state machine admission after typed admission had been fixed. That
failure was retained and the fix was extended to runtime-specific consumers;
the final nine-target run supersedes that failed run. Subsequent cases exposed
the Script-hash activator and an invalid test fixture using `InputView<Struct>`;
the activator was gated and the fixture uses a Cell-backed resource. The exact
ZK suite's previously accepted owner-action call is now explicitly rejected by
both compiler and independent checker. No negative test was disabled.

The large library invocation was not repeated after its fixture correction and
the subsequent runtime-profile changes. The changed fixture passed separately;
the final integration/checker suites cover the profile follow-ups. Do not
reinterpret the initial library command's nonzero exit as a clean suite pass.

The first WASM invocation omitted the crate's required `--features wasm` and
entered native-only code without native optional dependencies. It was stopped
and corrected. That configuration failure is not a compiler regression or a
successful target check.

## Evidence limits

No commit/dev/backend/CI/release gate or hosted CI was run, as explicitly
requested by the maintainer. Focused checks do not establish integration,
publication, deployment or source-equivalence acceptance. Existing ignored
tests remain ignored; no new ignore or raised resource ceiling is introduced.

The byte-loop source that originally produced wrong ASM was previously rejected
at the ELF boundary with V2420 for an unproved bounded syscall destination.
Rejecting index-changing fusion does not complete that separate fast-path
machine proof, and the checker guard is not relaxed.

Structural validation checks graph/local consistency, including explicit borrow
backing aliases and empty-vector refinement. It does not prove dominance,
definition-before-use, all opcode rules or complete allocation safety.
Simulator evidence remains non-consensus. Generic ProofRecords remain review
links; their profile-specific machine guarantees are documented separately.

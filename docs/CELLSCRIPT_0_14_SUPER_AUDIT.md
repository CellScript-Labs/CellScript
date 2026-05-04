
# 0. 总控 Prompt：CellScript 0.14 Scope Audit

```text
You are auditing CellScript v0.14 against roadmap/CELLSCRIPT_0_14_ROADMAP.md.

Your task is NOT to invent new features.
Your task is to verify whether the roadmap claims are implemented, tested, and worded correctly.

Strict rules:
1. Treat v0.12 and v0.13 deliverables as regression baseline, not new 0.14 scope.
2. Do not reopen the v0.13 action model unless the roadmap explicitly changes.
3. Do not treat examples compiling as CKB transaction conformance.
4. Do not treat metadata emission as runtime or transaction-level evidence.
5. Do not treat P2 stretch items as required release claims unless tests prove they are production-gated.
6. Do not claim full protocol composability from Spawn/IPC. Spawn/IPC is bounded verifier reuse only.
7. Do not claim Action Builder, CellFabric, or automatic transaction generation unless implemented and tested.
8. Do not claim executable browser WASM simulation unless the WASM backend actually runs examples in a tested harness.
9. If a roadmap sentence overstates evidence, patch the wording or add a minimal test.
10. Produce a final matrix:
   - claim
   - priority P0/P1/P2
   - implementation location
   - test evidence
   - evidence level
   - allowed release wording
   - forbidden release wording

Evidence levels:
A. committed local CKB transaction fixture
B. dry-run / simulated CKB fixture
C. compiler + lowering test
D. metadata validation test
E. parser/typechecker only
F. audit-only scaffold
G. not implemented
```

---

# 1. Roadmap Internal Consistency Prompt

```text
Audit CELLSCRIPT_0_14_ROADMAP.md for internal contradictions.

Check especially:
1. v0.14 says it does not reopen v0.13 action model.
2. v0.13 stable surface includes signature-direction outputs, where proof scopes, and explicit field-to-field move state edges.
3. Any proposed "transition", "verification", action braces, update sugar, or action-level syntax cleanup must not be claimed as 0.14 unless roadmap explicitly adds it.
4. P2 items such as WASM simulation and Transaction Builder integration must not appear in Required Success Metrics unless explicitly promoted.
5. Conditional dynamic BLAKE2b must not be described as shipped unless the production gate passes.
6. Capacity static verification and builder auto change-output must be separated.

Output:
- contradiction
- affected section
- risk
- recommended wording patch
```

---

# 2. P0 Gate Prompt：Must Complete for 0.14

```text
Audit all P0 blocking items in the 0.14 roadmap.

P0 items:
1. Spawn/IPC bounded verifier composition
2. Structured CKB WitnessArgs and Source views
3. Target Profile Formalization
4. CKB Transaction Shape and ScriptGroup Conformance

For each P0:
- locate parser/typechecker/lowering/codegen implementation
- locate tests
- check positive cases
- check negative fail-closed cases
- check metadata evidence
- check CKB profile gating
- check docs wording

Final status per P0:
- ready
- ready with wording downgrade
- missing tests
- implementation gap
- roadmap overclaim

Do not mark v0.14 stable-ready if any P0 item lacks evidence matching the claim.
```

---

# 3. Spawn/IPC Bounded Verifier Composition Prompt

```text
Audit Spawn/IPC bounded verifier composition.

Roadmap scope:
- spawn
- pipe
- pipe_write
- pipe_read
- wait
- process_id
- inherited_fd
- close
- syscall mapping 2601-2608
- fd lifetime tracking
- bounded depth
- known script target resolution
- no multi-tenant type-script composition claim

Check implementation:
1. Lexer / builtin keywords.
2. AST nodes.
3. Typechecker validation.
4. IR instructions.
5. Codegen syscall mapping.
6. Max spawn depth enforcement.
7. Shared cycle budget modelling or explicit documentation.
8. FD lifetime checks:
   - use-after-close
   - double-close
   - leaked fd warning/error
   - read/write wrong fd direction
9. Spawn target resolution:
   - known script
   - dep cell or inline target
   - unknown target rejects or diagnoses
10. Tests for delegate verifier and pipe pipeline examples.

Forbidden release wording:
- "full protocol composability"
- "multi-tenant type-script composition"
- "unbounded verifier composition"
- "general IPC runtime"

Allowed wording:
- "bounded Spawn/IPC verifier reuse"
- "typed low-level Spawn/IPC surface"
- "fd lifetime checked bounded verifier composition"
```

---

# 4. WitnessArgs + Source Views Prompt

```text
Audit Structured CKB WitnessArgs and source::* views.

Roadmap scope:
- source::input(n)
- source::output(n)
- source::cell_dep(n)
- source::header_dep(n)
- source::group_input(n)
- source::group_output(n)
- witness::raw<T>
- witness::lock<T>
- witness::input_type<T>
- witness::output_type<T>
- Molecule WitnessArgs decoding under CKB profile
- raw/entry witness ABI compatibility only when explicit

Check:
1. Parser/typechecker support.
2. Profile-correct Source encoding.
3. Structured WitnessArgs decoding.
4. Metadata records:
   - witness field
   - source view
   - index
   - ABI type
   - byte bounds
5. Positive fixture:
   - lock reads WitnessArgs.lock
   - type script reads input_type/output_type
6. Negative cases:
   - malformed WitnessArgs table
   - missing lock/input_type/output_type field
   - wrong field placement
   - wrong source index
   - global source used where group source required
   - group source used outside active ScriptGroup
   - portable profile pretending to decode CKB WitnessArgs

Allowed wording:
- "structured CKB WitnessArgs access"
- "explicit source transaction/group views"

Forbidden wording:
- "complete action ABI redesign"
- "wallet-level authentication abstraction"
- "portable profile emulates CKB WitnessArgs"
```

---

# 5. Target Profile Formalization Prompt

```text
Audit Target Profile Formalization.

Roadmap scope:
- TargetProfile::Ckb semantic contract
- CKB vs portable profile table
- profile-gated hash policy
- CKB Blake2b policy decision
- script mapping registry design
- docs/wiki/CELLSCRIPT_TARGET_PROFILES.md

Check:
1. TargetProfile enum exists and is documented.
2. CKB profile defines:
   - BLAKE2b hash policy
   - block number / epoch / timestamp semantics
   - since metric
   - script hash identity
   - WitnessArgs
   - Source encoding
   - Spawn/IPC availability
   - tx version assumptions
3. Portable profile rejects CKB-only features:
   - epoch
   - CKB Source group flags
   - Spawn/IPC if not supported
   - CKB WitnessArgs unless explicit compatibility mode
4. CI runs dual-profile tests.
5. Profile-specific diagnostics are clear.
6. No implicit profile leakage.

Output:
- feature
- CKB behaviour
- portable behaviour
- tests
- docs coverage
```

---

# 6. Dynamic BLAKE2b Decision Prompt

```text
Audit conditional dynamic hash_blake2b() support.

Roadmap says this is conditional:
- promote only if a v0.14 bundled example or compatibility target requires dynamic in-script BLAKE2b
- must link real RISC-V implementation
- stubs forbidden
- must pass known test vectors and cycle reporting

Check:
1. Is hash_blake2b() available in on-chain CellScript?
2. If yes:
   - real implementation, not stub
   - CKB profile gated
   - known BLAKE2b test vectors
   - cycle reporting
   - fail-closed when unavailable
3. If no:
   - compiler rejects on-chain hash_blake2b() with precise diagnostic
   - docs say dynamic in-script BLAKE2b deferred
4. Ensure no release wording says dynamic hash support shipped unless yes path is fully tested.

Allowed if deferred:
- "profile hash policy rejects unavailable dynamic BLAKE2b cleanly"

Allowed if promoted:
- "dynamic in-script BLAKE2b is available under CKB profile with test vectors and cycle evidence"
```

---

# 7. CKB Transaction Shape / ScriptGroup Conformance Prompt

```text
Audit CKB Transaction Shape and ScriptGroup Conformance.

Roadmap scope:
- ScriptGroup metadata
- source-to-group mapping
- output_data index obligations
- outputs[i] <-> outputs_data[i] binding
- TYPE_ID metadata validation MVP
- acceptance fixtures for ScriptGroup, outputs_data mismatch, TYPE_ID create/continue failures

Check ScriptGroup metadata:
1. entry_group_kind emitted.
2. input group index set emitted.
3. output group index set emitted.
4. selected Source view emitted.
5. source-to-group mapping emitted.

Check outputs_data:
1. every created/updated output has output-data obligation.
2. outputs[i] and outputs_data[i] treated as inseparable pair.
3. detached output data rejected.
4. wrong index rejected.
5. malformed data rejected.

Check fixtures:
Positive:
- lock group fixture
- type group fixture
- group_input access
- group_output access
- outputs_data correct binding

Negative:
- wrong group source
- empty group output where required
- wrong output index
- outputs_data mismatch
- output exists but data detached
- wrong lock/type pairing
- malformed output data

Evidence must be CKB-compatible transaction fixture, not only metadata.
```

---

# 8. TYPE_ID Metadata Validation MVP Prompt

```text
Audit TYPE_ID metadata validation MVP.

Roadmap scope:
- #[type_id] under CKB profile
- validate output index
- validate first-input args source
- one-input/one-output group rule
- duplicate output rejection
- missing-plan rejection
- create/continue failure cases

Check:
1. TYPE_ID create metadata plan.
2. First input outpoint source recorded.
3. Output index recorded.
4. Duplicate output rejected.
5. Missing output plan rejected.
6. Missing continuation rejected if continue is claimed.
7. Wrong TYPE_ID args rejected.
8. Wrong type/hash_type rejected.
9. Tests separate:
   - metadata validation
   - dry-run fixture
   - committed CKB fixture

Strict wording:
- If only metadata validation exists, say "TYPE_ID metadata validation MVP".
- Do not say "full TYPE_ID conformance" unless create, continue, duplicate, missing-plan, and malformed transaction cases are fixture-tested.
```

---

# 9. Declarative Capacity Syntax Prompt

```text
Audit Declarative Capacity Syntax.

Roadmap scope:
- @capacity_floor(shannons: ...)
- occupied_capacity(T)
- compiler validation
- compiler auto-inserts assert(capacity >= floor) on create
- builder integration depends on Transaction Builder Integration P2

Check:
1. @capacity_floor parser and AST attribute.
2. explicit shannons validation.
3. compiler-computed floor support.
4. occupied_capacity(T) const fn.
5. create operations receive capacity floor checks.
6. dynamic-length fields:
   - runtime fallback check
   - compiler warning
7. tests:
   - valid floor
   - insufficient capacity reject
   - override floor
   - occupied_capacity calculation
8. Separate static verifier checks from builder auto change-output.

Allowed wording:
- "declarative capacity floors"
- "occupied_capacity(T)"
- "capacity floor checks on create"

Forbidden unless P2 implemented:
- "automatic change-output generation"
- "automatic capacity balancing"
- "near-zero capacity failures" unless backed by builder tests
```

---

# 10. Declarative Time / Since / Epoch Prompt

```text
Audit Declarative Time and Since Constraints.

Roadmap scope:
- require_maturity(blocks: N)
- require_time(after: Timestamp(T))
- require_epoch(after: EpochFraction(...))
- require_epoch(relative: EpochFraction(...))
- CKB profile only
- EpochFraction well-formedness
- compile-time restriction: time/since constraints must appear before state mutations
- low-level ckb::input_since() remains available

Check:
1. Syntax support.
2. AST nodes.
3. Profile-gated lowering.
4. CKB consensus-compatible since encoding.
5. Absolute timestamp tests.
6. Relative block maturity tests.
7. Absolute epoch tests.
8. Relative epoch tests.
9. malformed EpochFraction rejects:
   - index >= length
   - length == 0
   - invalid number/index/length encoding
10. portable profile rejects require_epoch/require_maturity.
11. ordering rule rejects after consume/create/destroy/state mutation.

Allowed wording:
- "profile-gated CKB since/time/epoch constraints"
- "absolute and relative epoch since if tests exist"

Forbidden:
- "epoch support outside CKB profile"
- "portable epoch emulation"
```

---

# 11. Script Reference and HashType Strictness Prompt

```text
Audit Script Reference and HashType Strictness.

Roadmap scope:
- code_hash
- hash_type
- args
- dep source
- resolved profile
- lock/type/spawn target records
- CKB-supported hash_type validation
- CellDep/DepGroup path linkage
- audit output table

Check:
1. Metadata includes code_hash, hash_type, args.
2. Metadata identifies dep source.
3. Profile recorded.
4. Unknown hash_type rejects under CKB profile.
5. Profile-incompatible hash_type rejects.
6. spawn target references have resolvable dep.
7. lock/type metadata references have resolvable dep.
8. read/read_ref dep references resolvable.
9. generated audit docs include script reference table.
10. Missing dep linkage test.
11. Wrong hash_type test.
12. Wrong code_hash test.

Boundary:
Do not claim Address/LockScript/LockHash type split.
Do not claim full v0.15 semantic script-type separation.
```

---

# 12. WASM Backend Scope Prompt

```text
Audit WASM Script Execution Backend.

Roadmap marks it P2 / stretch or later.

Check:
1. Is WASM codegen implemented?
2. Is there a JS/WASM syscall shim?
3. Are spawn/pipe/wait/read/write/close mocked?
4. Can browser test harness load compiled WASM and run actions?
5. Are known divergences documented?
6. Are examples tested under WASM?
7. Does release wording clearly say simulation only, not on-chain WASM?

If not implemented:
- mark as deferred/audit-only/scaffold
- remove from core v0.14 delivered claims

Forbidden:
- "on-chain WASM"
- "browser simulation backend shipped" without runnable tests
- "WASM parity with RISC-V" without test vectors
```

---

# 13. Transaction Builder Integration Scope Prompt

```text
Audit Transaction Builder Language Integration.

Roadmap marks it P2 and continued from v0.13 stretch.

Check whether implemented:
1. cellc build --emit-builder-template
2. transaction skeleton output
3. builder auto-capacity planning
4. output minimum capacity from type layout
5. CellDep auto-resolution from registry
6. integration tests building transaction JSON or CCC-compatible draft

If missing:
- mark as P2 deferred
- do not include in 0.14 shipped feature summary

Allowed if partial:
- "builder-facing metadata"
- "builder template design"

Forbidden unless implemented:
- "Action Builder"
- "automatic transaction construction"
- "CCC integration"
- "auto capacity / change-output builder"
```

---

# 14. Advanced CellDep Patterns Prompt

```text
Audit Advanced CellDep Patterns.

Roadmap scope:
- DepGroup dynamic composition
- multi-module CellDep dependency graph
- transitive dep resolution
- shared code cell version locking
- pinned dep out_point in manifest

Check:
1. Manifest format exists.
2. DepGroup declaration exists.
3. Transitive dep graph resolution exists.
4. Version/out_point pinning exists.
5. Tests for missing dep, wrong dep, wrong version.
6. Integration with script reference metadata.

If absent, keep as P2/later.
Do not let summary claim this is shipped unless implemented.
```

---

# 15. Surface Ergonomics Backlog Prompt

```text
Audit Surface Ergonomics Backlog.

Roadmap classifies these as deferred candidates, not 0.14 blockers:
- transfer token { ... } with_lock(to)
- create_each
- named tuple returns
- multi-field move sugar
- Option<T> / Result<T,E>
- #[default_hash_type(Data1)]

Check:
1. Are any accidentally implemented?
2. Are any accidentally documented as shipped?
3. Are parser tokens accidentally accepted without lowering?
4. Are unsupported forms rejected with precise diagnostics?
5. Docs must say these require parser/typechecker/lowering/codegen/formatter/LSP/regression coverage before promotion.

Forbidden:
- claiming these as 0.14 delivered unless complete.
```

---

# 16. v0.12 / v0.13 Regression Prompt

```text
Audit v0.12 and v0.13 deliverables as regression baseline.

Do not re-plan them as 0.14 work.

Run regression gates proving no breakage:
- entry witness ABI CSARGv1
- scheduler witness ABI
- secp256k1 signature verification
- output transition patterns
- type_hash / lock_hash preservation
- low-level ckb::input_since()
- dep cell typed reads
- 43/43 production actions
- 7/7 bundled examples
- metadata schema 29/30 compatibility
- package manager registry fail-closed
- LSP JSON-RPC stdio
- value-vector helpers
- signature-direction action model
- where proof scopes
- explicit field-to-field move edges
- preserve sugar
- require blocks
- builder-backed CKB action/lock acceptance
- stateful local CKB release evidence

If any regression appears, v0.14 cannot be stable.
```

---

# 17. Success Metrics Audit Prompt

```text
Audit the roadmap Success Metrics table.

For every metric marked Required:
1. verify it is actually P0/P1 required
2. verify tests exist
3. verify evidence level matches wording
4. if metric depends on P2 stretch item, downgrade or mark conditional

Pay special attention:
- "At least 2 spawn-based examples" required
- WitnessArgs examples required
- Source global/group view tests required
- ScriptGroup metadata matches fixtures required
- outputs_data mismatch rejects required
- TYPE_ID create/continue/duplicate/missing-plan required
- absolute and relative epoch tests required
- capacity static verification covers 100% create operations required
- script reference metadata includes code_hash/hash_type/args/CellDep linkage required
- dynamic BLAKE2b conditional
- WASM simulation P2

Output:
metric | roadmap priority | actual evidence | contradiction? | patch.
```

---

# 18. CI Gate Prompt

```text
Construct the strict 0.14 CI gate.

Required commands:
cargo test --locked -p cellscript -- --test-threads=1
cargo test --locked -p cellscript --test v0_14 -- --test-threads=1
cargo test --locked -p cellscript --test fuzzy_debug -- --test-threads=1
git diff --check

Profile gate:
for file in examples/*.cell; do
    cellc "$file" --target-profile ckb
done

If dual profile exists:
for profile in ckb portable; do
    run all feature-specific compile tests
done

If release gate script exists:
./scripts/cellscript_ckb_release_gate.sh full

Record:
- command
- result
- failure reason
- feature area
- whether failure blocks 0.14 stable
```

---

# 19. New v0.14 Examples Prompt

```text
Audit required new v0.14 examples.

Expected examples:
- delegate_verify.cell
- multi_step_pipeline.cell
- witness_args_lock.cell
- script_group_type_transition.cell
- ckb_type_id_create.cell
- capacity_aware_token.cell
- cross_chain_htlc.cell
- script_reference_manifest.cell

For each example:
1. compile under CKB profile
2. exercise intended feature
3. has at least one negative fixture or diagnostic test
4. does not rely on unsupported P2 features unless marked stretch
5. has audit notes / metadata evidence

If example missing, mark feature incomplete unless another equivalent example exists.
```

---

# 20. Final Release Wording Prompt

```text
Generate final 0.14 release wording from evidence only.

Rules:
1. P0 complete features may be described as delivered.
2. P1 complete features may be described as included.
3. P2 incomplete features must be described as deferred, stretch, or design-only.
4. Conditional dynamic BLAKE2b must say either:
   - shipped with real RISC-V implementation and vectors
   - deferred with fail-closed diagnostic
5. WASM must say simulation-only and only if runnable tests exist.
6. Transaction Builder must not be described as Action Builder unless implemented.
7. Spawn/IPC must not imply protocol-level multi-tenant composition.
8. TYPE_ID must be called "metadata validation MVP" unless transaction fixtures prove more.
9. Target profile must distinguish CKB from portable.

Output:
- public summary
- technical release notes
- non-goals
- evidence table
```

---

# 21. 最终 Acceptance Prompt

```text
Final CellScript v0.14 acceptance audit.

Mark stable-ready only if:

1. All P0 items pass implementation and tests:
   - Spawn/IPC bounded verifier composition
   - WitnessArgs and Source views
   - Target profile formalization
   - CKB transaction shape / ScriptGroup conformance

2. Required success metrics have evidence or are downgraded.

3. P1 items are either completed with tests or clearly marked as included/deferred:
   - capacity syntax
   - time/since/epoch constraints
   - dynamic BLAKE2b
   - script reference strictness

4. P2 items do not leak into core release claims:
   - WASM
   - Transaction Builder
   - Advanced CellDep
   - surface ergonomics backlog

5. v0.12/v0.13 regression gates pass.

6. No roadmap claim exceeds evidence.

7. Docs clearly state:
   - Spawn/IPC is bounded verifier reuse, not full protocol composability.
   - CKB profile semantics are target-specific.
   - transition/move action model is not reopened unless explicitly scoped.
   - WASM is simulation-only if shipped.
   - builder integration is not shipped unless tested.

Output one of:
- Stable-ready
- Stable-ready after docs downgrade
- Not stable-ready: missing P0 tests
- Not stable-ready: implementation gap
- Not stable-ready: roadmap contradiction
```

---

## 最短的一句总 prompt

```text
Audit CellScript v0.14 strictly against CELLSCRIPT_0_14_ROADMAP.md. Treat P0 as blocking, P1 as included only with evidence, and P2 as stretch unless implemented. Separate parser/lowering, metadata, dry-run, and committed CKB fixture evidence. Downgrade every claim that exceeds tests. Do not reopen the v0.13 action model or imply full protocol composability from Spawn/IPC.
```

---

# 22. Audit Results - 2026-05-04

## Final Classification

**Stable-ready after docs downgrade.**

The implementation evidence supports the corrected v0.14 scope as a **CKB semantic
completeness release** for compiler/lowering/codegen metadata, strict metadata
validation, language examples, and the existing production CKB acceptance suite.

It does **not** support claims of:

- full protocol composability from Spawn/IPC;
- portable profile implementation;
- Action Builder / CellFabric / CCC transaction generation;
- executable browser/WASM simulation;
- full accepted/rejected transaction fixture matrices for ScriptGroup,
  `outputs_data`, and TYPE_ID continue;
- source-level `max_cycles` spawn budgeting;
- registry-backed CellDep/DepGroup resolution.

Those claims were downgraded in `roadmap/CELLSCRIPT_0_14_ROADMAP.md` and
`docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES_DRAFT.md`.

## Contradictions Fixed

| Contradiction | Affected section | Risk | Patch |
|---|---|---|---|
| v0.13 baseline incorrectly described field-to-field state edges as `transition` instead of legacy `move`. | v0.13 deliverables | Reopens the v0.13 action model by accident. | v0.13 baseline now says explicit field-to-field `move`; v0.14 owns the `move` -> `transition` spelling cleanup. |
| Spawn/IPC table claimed dedicated AST/IR nodes and source-level `max_cycles`. | Spawn/IPC implementation path and risk 1 | Release notes could claim implementation structure that does not exist. | Wording now says typed builtin calls lower to CKB helper calls; `max_cycles` is not a 0.14 release claim. |
| Spawn target resolution implied compile-time known dep resolution. | Spawn/IPC safety and script references | Could make builders treat a static string as authority. | Wording now says static source target plus runtime-required CellDep/DepGroup metadata obligation; full registry resolution is deferred. |
| Target profile section implied implemented CKB/portable dual-profile behavior. | Target Profile Formalization, risk 2, CI | Could claim portability tests that do not exist. | Wording now says only `TargetProfile::Ckb` is implemented; unsupported profile names fail closed. |
| ScriptGroup/TYPE_ID/outputs_data acceptance text implied dedicated CKB transaction fixture matrices. | P0 transaction shape, success metrics, release notes | Metadata evidence could be overstated as transaction-level conformance. | Wording now says metadata/tamper validation in 0.14; accepted/rejected transaction matrix is deferred. |
| Capacity syntax used old `@capacity_floor` and `occupied_capacity(T)` examples. | Capacity syntax and summary | Examples did not match shipped syntax. | Wording now uses `with_capacity_floor(...)` and `occupied_capacity("TypeName")`. |
| WASM mitigation described tested shims despite audit-only backend. | P2 WASM, risk 7 | Could imply browser simulation shipped. | Wording now says WASM is audit-only and fail-closed for executable entries. |
| Peripheral tooling listed `cellc spawn-test`, wallet/SDK/WASM sync, and dual-profile CI as release work. | Tool coordination and quick start | Could imply unimplemented tools. | Wording now marks these as later integration tracks or compile/metadata evidence only. |
| Expected outcome claimed near-zero capacity failures. | Summary | Builder integration evidence does not support this. | Wording now says capacity floors and measurement obligations are explicit, while builders still own funding/change/capacity evidence. |

## Evidence Matrix

| Claim | Priority | Implementation location | Test evidence | Evidence level | Allowed release wording | Forbidden release wording |
|---|---:|---|---|---|---|---|
| Spawn/IPC bounded verifier reuse | P0 | `src/types/mod.rs`, `src/ir/mod.rs`, `src/codegen/mod.rs`, `src/lib.rs` | `tests/v0_14.rs` Spawn/IPC positive and fd negative tests; `examples/language/v0_14_delegate_verify.cell`; `examples/language/v0_14_multi_step_pipeline.cell` | C/D | Bounded Spawn/IPC verifier reuse with typed helper calls, syscalls 2601-2608, fd lifetime checks, and metadata-visible CellDep obligation. | Full protocol composability; multi-tenant type-script composition; unbounded verifier composition; general IPC runtime; source-level `max_cycles`. |
| Structured WitnessArgs and Source views | P0 | `src/ir/mod.rs`, `src/lib.rs`, `src/codegen/mod.rs` | `tests/v0_14.rs`; `examples/language/v0_14_witness_source.cell`; `examples/language/canonical_style.cell` | C/D | Structured CKB WitnessArgs access and explicit Source transaction/group views with metadata-visible runtime accesses. | Wallet-level auth abstraction; portable profile emulates WitnessArgs; malformed WitnessArgs transaction matrix shipped. |
| Target profile formalization | P0 | `src/lib.rs::TargetProfile`, `src/cli/commands.rs`, `docs/wiki/Tutorial-05-CKB-Target-Profiles.md` | `tests/cli.rs::cellc_explain_profile_reports_ckb_v0_14_contract`; `tests/v0_14.rs` profile ABI assertions | C/D | Formalized CKB target-profile ABI contract; unsupported target profiles fail closed. | Implemented portable profile; dual-profile CI; portable epoch/WitnessArgs semantics. |
| ScriptGroup / transaction-shape metadata | P0 | `src/lib.rs` metadata validation, `src/ir/mod.rs` runtime access lowering | `tests/v0_14.rs::v0_14_rejects_tampered_runtime_access_and_script_group_metadata` | D | ScriptGroup/runtime-access metadata and tamper validation for CKB strict mode. | Full CKB accepted/rejected ScriptGroup transaction fixture matrix. |
| `outputs` / `outputs_data` binding | P0 | `src/lib.rs` output-data metadata validation | `tests/v0_14.rs::v0_14_rejects_tampered_type_id_output_data_and_script_reference_metadata`; fuzzy metadata tamper tests | D | Index-aligned `outputs[i]` / `outputs_data[i]` metadata obligations with tamper rejection. | Detached output-data transaction fixture matrix shipped. |
| TYPE_ID metadata validation MVP | P0 | `src/lib.rs` TYPE_ID metadata generation/validation | `tests/v0_14.rs` TYPE_ID create plan and tamper tests; language example `v0_14_ckb_type_id_create.cell` | D | TYPE_ID metadata validation MVP for create plans, duplicate/missing/mismatched metadata-plan cases. | Full TYPE_ID conformance; TYPE_ID continue transaction fixtures shipped. |
| Declarative capacity syntax | P1 | `src/parser/mod.rs`, `src/types/mod.rs`, `src/ir/mod.rs`, `src/lib.rs` | `tests/v0_14.rs::v0_14_exposes_declarative_capacity_floor_metadata`; `examples/language/v0_14_capacity_time.cell` | C/D | `with_capacity_floor(...)` and `occupied_capacity("TypeName")` metadata/constraint surface; builders still provide capacity evidence. | Automatic capacity balancing; automatic change-output generation; near-zero capacity failures. |
| Declarative since/time/epoch helpers | P1 | `src/types/mod.rs`, `src/ir/mod.rs`, `src/lib.rs` | `tests/v0_14.rs`; `examples/language/v0_14_capacity_time.cell` includes absolute and relative epoch helpers | C/D | Profile-gated CKB since/time/epoch obligation surface. | Epoch support outside CKB profile; portable epoch emulation; complete consensus fixture matrix. |
| Dynamic fixed-Hash Blake2b | P1 | `src/types/mod.rs`, `src/ir/mod.rs`, `src/codegen/mod.rs`, `src/lib.rs` | `tests/v0_14.rs::v0_14_compiles_dynamic_blake2b_hash_helper`; full release gate covers real `timelock.cell` lock valid/invalid spend flow | A/C | `hash_blake2b(input: Hash) -> Hash` lowers to real CKB-profile RISC-V helper with metadata-visible `CKB_BLAKE2B`. | Arbitrary byte-slice hashing; resource serialization hashing; stubbed hash helper. |
| Script reference / HashType strictness | P1 | `src/lib.rs` script-reference metadata validation; `Cell.toml` manifest handling | `tests/v0_14.rs` script-reference tamper tests; `tests/cli.rs` profile/script reference checks | D | Script references expose `code_hash`, `hash_type`, `args`, dep source, and metadata validation. | Full registry-backed dep resolution; Address/LockScript/LockHash type split. |
| WASM backend | P2 | `src/wasm/mod.rs` | Unit tests reject executable action/lock lowering | F | WASM remains audit-only and fail-closed for executable entries. | Browser simulation backend shipped; WASM parity with RISC-V; on-chain WASM. |
| Transaction Builder integration | P2 | Not implemented as 0.14 language feature | Existing production builder acceptance is regression evidence, not new 0.14 Action Builder | G | Builder-facing metadata only. | Action Builder shipped; CCC integration; automatic transaction construction. |
| Advanced CellDep/DepGroup patterns | P2 | Metadata obligations and manifest parsing only | Metadata/script-reference tests | F/D | Script-reference and CellDep obligations are visible; full registry-backed DepGroup resolution is deferred. | Transitive dep graph, dynamic DepGroup composition, or full dep registry linkage shipped. |
| Surface ergonomics backlog | P2 | Not promoted | Unsupported forms remain outside 0.14 release claims | G | Deferred candidates requiring full parser/typechecker/lowering/codegen/LSP/docs coverage before promotion. | Transfer sugar, `create_each`, named tuple returns, `Option`/`Result`, or `#[default_hash_type]` shipped. |
| v0.12/v0.13 regression baseline | P0 regression | Release gate scripts and compiler tests | `./scripts/cellscript_ckb_release_gate.sh full`; `quick`; targeted example and v0_14 tests | A | Existing production actions/examples/stateful evidence remain regression baseline. | Treating v0.12/v0.13 deliverables as new 0.14 scope. |

## Strict Gate Evidence Recorded

| Command | Result | Notes |
|---|---|---|
| `./scripts/cellscript_ckb_release_gate.sh full` | PASS | Report: `/Users/arthur/RustroverProjects/CellScript/target/ckb-cellscript-acceptance/20260504-192212-34186/ckb-cellscript-acceptance-report.json` |
| `./scripts/cellscript_ckb_release_gate.sh quick` | PASS | Report: `/Users/arthur/RustroverProjects/CellScript/target/ckb-cellscript-acceptance/20260504-192003-28456/ckb-cellscript-acceptance-report.json` |
| `cargo test --locked -p cellscript -- --test-threads=1` | PASS | Covered by quick release gate; 482 lib tests plus integration/doc suites passed. |
| `cargo test --locked -p cellscript --test v0_14 -- --test-threads=1` | PASS | Targeted v0.14 compiler/metadata tests. |
| `cargo test --locked -p cellscript --test examples release_examples_are_free_of_placeholder_hashes_and_formatter_artifacts -- --test-threads=1` | PASS | Example realism/display guard. |
| `./scripts/cellscript_syntax_combo_audit.sh deep --seed 2026050401..2026050410` | PASS | 10 seeds; each generated 40 cases, accepted 22, rejected 18, failures 0. |
| Strict language example asm/ELF/metadata oracle | PASS | Checked 7 v0.14 language example metadata files under `target/strict-0-14-scope-audit/20260504-190320-after-fix`. |
| Strict production report oracle | PASS | Verified production-ready full report, 43/43 action coverage, 17/17 lock matrix, cycles/tx-size/capacity/stateful flags. |
| `git diff --check` | PASS | Whitespace check clean after audit edits. |

## Release Wording

### Public Summary

CellScript 0.14 exposes CKB-native semantics directly in the language and
metadata: bounded Spawn/IPC verifier reuse, structured WitnessArgs access,
explicit Source views, ScriptGroup/output-data metadata validation, TYPE_ID
metadata plans, fixed-Hash dynamic Blake2b, and CKB-profile capacity/time
surfaces.

The release deliberately keeps the boundary conservative. Spawn/IPC is bounded
verifier reuse, not full protocol composability. ScriptGroup, `outputs_data`,
and TYPE_ID are metadata/tamper validated in 0.14, while full accepted/rejected
transaction fixture matrices remain a later compatibility-suite task.

### Non-Goals

- no Action Builder, CellFabric, CCC integration, or automatic transaction
  generation;
- no executable browser/WASM simulation;
- no portable target profile implementation;
- no full ProofPlan / covenant trigger-scope-coverage;
- no Address / LockScript / LockHash type split;
- no arbitrary byte-slice or resource-serialization Blake2b hashing;
- no external audit or mainnet-value certification claim.

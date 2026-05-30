# CellScript v0.14 Roadmap

**Status**: Draft (Pending Team Review)
**Scope**: CKB Semantic Completeness, Source/Witness Ergonomics, and Bounded Verifier Composition
**Dependencies**: v0.13.2 released (explicit action model, bounded value-vector
helpers, stdlib syntax governance, syntax-combination audit, and full
builder-backed/stateful CKB release evidence)

---

## 📊 Executive Summary

**v0.14 Theme**: **CKB Semantic Completeness and Bounded Verifier Composition**

CellScript's evolution follows a deliberate maturity curve:

- **v0.12** — Production closure: proved CellScript can compile production-grade cell contracts (43/43 actions, 7/7 examples, entry witness ABI, output preservation checks, low-level time helpers, dep cell reads).
- **v0.13** — Stable explicit verifier surface: signature-direction action model, bounded value-vector helpers, stdlib lifecycle/Cell metadata patterns, syntax-combination gates, and builder-backed plus stateful CKB release evidence.
- **v0.14** — CKB semantic completeness and bounded verifier composition: structured `WitnessArgs`, profile-aware `since`/epoch time constraints, explicit Source views, ScriptGroup/transaction-shape conformance, bounded verifier reuse via Spawn/IPC, formalized target profiles, declarative capacity syntax, and the intentional `move` -> `transition` syntax cleanup. The WASM simulation backend remains a P2/stretch track unless explicitly promoted.

v0.14 closes the remaining DSL-level semantic gaps between CKB VM reality and CellScript source code: CKB witness structure, CKB epoch-based `since`, Source transaction/group views, ScriptGroup/outputs_data conformance, TYPE_ID metadata validation MVP, Spawn/IPC, and action-level `transition` state edges. It should not re-plan v0.13 bounded generics, repeat 0.13 stateful production evidence as new scope, or start the v0.15 primitive-kernel reset.

v0.14 provides low-level Spawn/IPC and CKB Source/Witness semantics. It does not define the full protocol composability model. The higher-level question of trigger, scope, reads, coverage, and builder assumptions is intentionally deferred to v0.15's Scoped Invariants and Covenant ProofPlan.

---

## Current Nightly Exploration Snapshot

The `nightly-0.14` branch carries the prior `feat/ckb-surface` exploration as a
working CKB semantic-completeness surface:

| Track | Current nightly status |
|---|---|
| Spawn/IPC surface | Implemented as bounded verifier helper calls with metadata-visible runtime accesses. |
| Spawn/IPC fd safety | Type checker rejects statically visible use-after-close, double-close, and leaked fd paths. |
| Source views | `source::input`, `source::output`, `source::cell_dep`, `source::header_dep`, `source::group_input`, and `source::group_output` are typed and metadata-visible. |
| Witness fields | `witness::raw`, `witness::lock`, `witness::input_type`, and `witness::output_type` are explicit CKB witness surfaces. |
| Lock args source | Fixed-width `lock_args` parameters decode executing `Script.args`; this is data-source binding only, not signer authority. |
| ScriptGroup metadata | Actions and locks expose entry kind, active group kind, selected Source surfaces, and group-scoped Source usage; metadata validation rejects missing or mismatched ScriptGroup/runtime-access records. |
| outputs/outputs_data binding | CKB create outputs record index-aligned output-data bindings and metadata validation rejects missing or mismatched bindings. |
| TYPE_ID/script references | TYPE_ID output plans, spawn targets, and read_ref CellDep references are surfaced in `constraints.ckb.script_references`; metadata validation rejects malformed, missing, duplicate, or extra script-reference records. |
| Since/time/capacity | Declarative since/time helpers and `with_capacity_floor(shannons)` are metadata-visible; CKB constraints summary, capacity-floor records, and capacity-evidence flags are checked against compiler metadata, while builder capacity evidence remains required. |
| Dynamic BLAKE2b | `hash_blake2b(input: Hash) -> Hash` lowers to a real CKB-profile Blake2b-256 RISC-V helper with CKB default personalization and metadata-visible `CKB_BLAKE2B` access. |
| Examples | Language examples cover delegate verification, Spawn/IPC pipelines, witness/source views, TYPE_ID creation, capacity/time policy, dynamic Blake2b hashing, and canonical lock-boundary style. |
| WASM simulation | Not covered by the current nightly release surface. `src/wasm` is still audit-only and rejects executable action/lock modules. |

### Current Coverage Gaps

The current `nightly-0.14` branch covers the implemented CKB semantic surface,
but it does not yet cover every broader roadmap aspiration:

| Gap | Current status |
|---|---|
| Executable browser/WASM simulation | Deferred. Existing WASM support is an audit-only scaffold. |
| TYPE_ID continue transaction fixtures | Not covered as a CKB transaction fixture. Current coverage is TYPE_ID metadata, create output plans, duplicate stable type_id rejection, and missing/mismatched metadata-plan rejection. |
| ScriptGroup/outputs_data CKB transaction negative fixtures | ScriptGroup/runtime-access and outputs_data metadata validation exist; a dedicated CKB positive/negative transaction fixture matrix remains open. |
| Full script-reference dep registry linkage | Script references are surfaced in metadata; full registry-backed dep resolution remains an integration track. |

---

## 📋 What v0.14 Does NOT Redo

The following capabilities are already delivered and will not be re-planned:

### v0.12 Deliverables (Production Closure)

- ✅ Entry witness ABI (CSARGv1) for CellScript action/lock parameters
- ✅ Scheduler witness ABI and claim witness runtime loading/signature metadata
- ✅ secp256k1 signature verification
- ✅ Output transition patterns (Set/Add/Sub/Append)
- ✅ type_hash / lock_hash preservation
- ✅ Low-level `ckb::input_since()` and CKB header epoch helper APIs
- ✅ Timelock fixtures and runtime since validation for profile time/timestamp
- ✅ Dep cell typed reads for declared action-boundary `read` parameters and expression-level `read_ref<T>()` CellDep paths
- ✅ 43/43 production actions, 7/7 bundled examples deployed
- ✅ Molecule ABI manifest, metadata schema 29
- ✅ Package manager local workflow (registry fail-closed)
- ✅ LSP: JSON-RPC stdio + VS Code integration

### v0.13 Deliverables (Stable Explicit Verifier Surface)

- ✅ Stack-backed fixed-width value-vector helpers for checked `Vec<T>` paths
- ✅ Metadata and `cellc explain-generics` for concrete checked vector instantiations
- ✅ Signature-direction action model with named outputs, `where` proof scopes, and explicit field-to-field `move` state edges
- ✅ Stable v0.13 action boundary: state topology is declared separately from proof obligations, and proof logic remains in `where`
- ✅ `preserve` sugar and anonymous `require` blocks with canonical verifier expansion and pure-boolean enforcement
- ✅ Compiler-recognized stdlib patterns for lifecycle and Cell metadata:
  `std::lifecycle::transfer`, `std::receipt::claim`,
  `std::lifecycle::settle`, `std::cell::same_lock`,
  `std::cell::preserve_lock`, and `std::cell::preserve_capacity`
- ✅ Syntax-combination audit methodology and quick/CI gates across parser,
  formatter, type checking, lowering, metadata, and codegen
- ✅ Builder-backed CKB action and lock acceptance, including valid-spend and
  invalid-spend lock matrices
- ✅ Stateful local CKB release evidence: 7 end-to-end business scenarios, 20
  action-branch scenarios, 46 committed stateful steps, and 43/43 production
  acceptance actions covered
- ✅ Deserialization code specialization
- ✅ Function inlining for safe pure helpers
- ✅ Dead code elimination + constant propagation
- ✅ CLI: `cellc new`, `build` default O1, error codes with `cellc explain`
- ✅ Hash type DSL exposure (`with_default_hash_type`)
- ✅ Metadata schema 30
- ✅ Clear fail-closed boundary for `Option<T>`, phantom asset tags, generic
  interfaces/templates, full maps, cell-backed collection ownership, hidden
  signer authority, and hidden sighash defaults

---

## 📋 Feature List (By Priority)

### P0 - Blocking (Must Complete in v0.14)

#### 1. Spawn/IPC Bounded Verifier Composition 🔴

**This is one of the core low-level features in v0.14.**

**Problem**: The VM layer already implements Spawn/IPC syscalls (2601-2608), but the DSL has no first-class support. Developers must drop to raw syscall numbers to compose scripts, which is error-prone, untyped, and unauditable.

**Why It Matters**: Bounded verifier composition is an important building block for:
- Delegate verification patterns (lock script spawns a verifier)
- Reusable verification libraries (shared utility scripts)
- Multi-step validation pipelines (hash → signature → authorization)
- Modular validation pipelines with explicit lock/type boundaries
- CKB VM v2 compatibility

**Composability Boundary**:

Spawn/IPC does not make a CKB cell's `type script` slot multi-tenant.

If protocol A already occupies the type script of a cell, protocol B cannot simply attach another independent type-level rule to that same cell through spawn. Spawn/IPC is a mechanism for bounded verifier reuse, delegated checks, and modular validation pipelines. It does not erase lock/type coverage boundaries.

Protocol composition around an existing cell should still use receipt/companion cells, read-only deps, explicit transaction constraints, validating locks where appropriate, and later ProofPlan-scoped covenant patterns.

Full protocol composability remains a v0.15+ ProofPlan / scoped-invariant concern, not a v0.14 Spawn/IPC promise.

**DSL Design**:

**Basic spawn — launch a child script for verification**:
```cellscript
action verify_with_delegate(proof: Proof)
where
    let result = spawn("secp256k1_verifier", args: [proof.pubkey, proof.signature])
    assert(result == 0, "delegate verification failed")
```

**Pipe-based verification chain**:
```cellscript
action multi_step_verify(data: VerifyData)
where
    let (read_fd, write_fd) = pipe()
    let pid = spawn("hash_checker", fds: [read_fd])
    pipe_write(write_fd, data.payload)
    let hash_result = wait(pid)
    assert(hash_result == 0, "hash check failed")
```

**Implementation Path**:

| Layer | Change | Details |
|-------|--------|---------|
| Lexer / builtins | New helper surface | `spawn`, `pipe`, `pipe_write`, `pipe_read`, `wait`, `process_id`, `inherited_fd`, `close` are accepted as typed builtin helper calls |
| AST | Existing call expressions | The implemented surface uses normal call expressions plus builtin validation, not dedicated `SpawnExpr` / `PipeExpr` / `WaitExpr` nodes |
| Type checker | Argument validation | Verify spawn target is a string literal or `String` const; fd usage tracking rejects use-after-close, double-close, and leaked descriptors |
| Metadata | Spawn target evidence | Emit runtime-required CellDep/DepGroup script-reference obligations for each spawn target so builders cannot treat a string name as authority |
| IR | Runtime helper calls | Lower builtin calls to CKB runtime helper calls such as `__ckb_spawn`, `__ckb_pipe`, `__ckb_pipe_write`, `__ckb_pipe_read`, `__ckb_wait`, and `__ckb_close` |
| Codegen | Syscall mapping | `spawn` -> 2601, `wait` -> 2602, `process_id` -> 2603, `pipe` -> 2604, `pipe_write` -> 2605, `pipe_read` -> 2606, `inherited_fd` -> 2607, `close` -> 2608 |

**Safety Constraints**:
- Cycle budget allocation: shared budget model (parent + children share a total cycle limit, matching CKB's existing semantics). v0.14 does not ship a source-level `max_cycles` spawn parameter.
- File descriptor lifetime tracking: compiler rejects use-after-close, double-close, and statically visible leaked fds
- Spawn target resolution boundary: the source target must be static, and metadata records a runtime-required CellDep/DepGroup obligation for the transaction builder. Full registry-backed dep resolution is deferred.

**Risk**: **MEDIUM** — Syscalls are stable; complexity is in DSL ergonomics and fd tracking
**Depends on**: v0.13 fixed-width value metadata for typed spawn arguments

---

#### 2. Structured CKB WitnessArgs and Source Views 🔴

**Problem**: CellScript has entry witness bytes, but CKB's standard `WitnessArgs { lock, input_type, output_type }` structure is still not a first-class DSL concept. CKB lock/type scripts also rely on precise Source selection (`Input`, `Output`, `CellDep`, `HeaderDep`, and group-scoped variants). Today this is mostly implicit in lowering.

**Why It Matters**:
- Standard lock scripts read signatures from `WitnessArgs.lock`.
- Type scripts may use `input_type` / `output_type` for protocol-specific proofs.
- Advanced scripts need to choose transaction-global vs script-group views intentionally.
- Profile-correct Source encodings are CKB-specific in v0.14, so the compiler must own this boundary and future profiles must opt in explicitly.

**DSL Design**:

```cellscript
lock standard_lock(lock_args args: OwnerArgs, witness sig: RecoverableSignature) -> bool {
    let sig = witness::lock<RecoverableSignature>(source: source::group_input(0))
    let sighash = env::sighash_all(source: source::group_input(0))
    return secp256k1_verify(args.pubkey_hash, sig, sighash)
}

action prove_type_transition(state_before: State) -> state_after: State
where
    let proof = witness::input_type<TransitionProof>(source: source::group_input(0))
    assert(verify_transition(proof, state_before, state_after), "bad transition proof")
```

**Implementation Items**:

| Item | Details |
|------|---------|
| `source::*` DSL | `input(n)`, `output(n)`, `cell_dep(n)`, `header_dep(n)`, `group_input(n)`, `group_output(n)` with profile-correct encoding |
| `witness::*` DSL | `raw<T>`, `lock<T>`, `input_type<T>`, `output_type<T>` with CKB Molecule `WitnessArgs` decoding |
| Metadata exposure | Emit runtime access records with witness field, source view, index, ABI, and expected byte bounds |
| Profile gates | The implemented release profile is `ckb`. Future non-CKB profiles must reject CKB-only WitnessArgs/Source assumptions unless an explicit compatibility mode is implemented and tested. |
| Tests | Metadata/lowering tests and language examples cover lock/input_type/output_type and global/group Source views; malformed transaction WitnessArgs fixtures remain part of the later compatibility suite. |

**Risk**: **HIGH** — This changes author-facing authentication/proof semantics and must fail closed
**Depends on**: Target Profile Formalization (#3)

---

#### 3. Target Profile Formalization 🔴

**Problem**: The target-profile architecture has existed implicitly since v0.12, but the semantics are not formally documented or enforced. Developers encounter surprising differences (hash domains, CKB block/epoch time, since encoding, and Source group encoding) without clear guidance.

**Profile Semantic Reference**:

| Feature | Implemented CKB Profile | Future Portable Profile Boundary |
|---------|-------------|---------------|
| Hash function | BLAKE2B | configurable |
| Time reference | Block Number / EpochNumberWithFraction | abstract |
| Since metric | `block_number` / `epoch` / `timestamp` | N/A |
| Script hash / identity | BLAKE2B standard | profile-declared |
| Witness structure | Molecule `WitnessArgs` + raw bytes fallback | explicit |
| Source encoding | CKB strict high-bit group flag | explicit |
| Spawn/IPC | Available (VM v2+) | not available |
| Tx version | 0 | N/A |

**Key Design Decision**: CKB epoch semantics are CKB-specific. The compiler currently ships the `ckb` profile only. A future portable profile must not emulate CKB epoch behavior without an explicit target profile and tests.

**Implementation Items**:

**3a. TargetProfile Enum Specification**
- Formalize `TargetProfile::Ckb` with a complete semantic contract
- Document which builtins, syscalls, and constraints the CKB profile enables
- Publish through `docs/wiki/Tutorial-05-CKB-Target-Profiles.md` and `cellc explain-profile ckb`

**3b. Profile-gated hash policy**
- Keep existing hash-domain metadata explicit; do not silently make portable code depend on different hash algorithms.
- Add `hash_chain(data)` only for code that intentionally wants the active profile's canonical data hash.
- Keep explicit CKB Blake2b helpers profile-gated and metadata-visible.

**3c. Dynamic CKB BLAKE2b fixed-Hash support**
- v0.13 scoped BLAKE2b to builder/release tooling, not a guaranteed in-script stdlib.
- v0.14 promotes `hash_blake2b(input: Hash) -> Hash` for 32-byte runtime digest inputs.
- Arbitrary byte-slice and resource serialization hashing remain deferred until their ABI and serialization contract are specified.

**3d. Profile Script Mapping Registry Design**
- Standard scripts (secp256k1, multisig, etc.) may have different `code_hash` values across target profiles
- Design a registry format: `scripts.toml` mapping `(script_name, profile) → code_hash`
- v0.14 records script-reference obligations in metadata; full registry-backed resolution remains deferred to the deployment/compatibility track

**Risk**: **LOW** — Formalizing existing implicit behavior
**Depends on**: None

---

#### 4. CKB Transaction Shape and ScriptGroup Conformance 🔴

**Problem**: v0.14 Source/Witness APIs expose CKB views at the DSL level, but the compiler must also prove that emitted metadata and strict-mode checks match CKB's concrete transaction model: lock/type ScriptGroups, `outputs` ↔ `outputs_data` indexing, standard TYPE_ID creation constraints, and script reference hash types.

**Why It Matters**:
- CKB lock groups are formed from input lock scripts; type groups are formed from input and output type scripts.
- `source::group_input(n)` and `source::group_output(n)` are only meaningful relative to the active script group.
- Every `outputs_data[i]` belongs to `outputs[i]`; data obligations cannot be tracked independently from output cell indexes.
- Standard TYPE_ID has consensus-level verifier rules: args derive from the first input and output index, and the group must not contain multiple created/consumed instances.

**Implementation Items**:

| Item | Details |
|------|---------|
| ScriptGroup metadata | Emit entry kind, active lock/type group kind, selected Source surfaces, and group-scoped Source usage for every CKB entry |
| Source conformance tests | Cover metadata/lowering for `Input`, `Output`, `CellDep`, `HeaderDep`, `GroupInput`, and `GroupOutput`; out-of-bounds and wrong-profile transaction fixtures stay in the later compatibility suite |
| Output data binding | Emit output-data index obligations for every created or updated output; reject metadata where output data is detached from the output cell index |
| TYPE_ID metadata validation MVP | For `#[type_id]` under CKB profile, validate output index, first-input args source, one-input/one-output group rule, duplicate output rejection, and missing-plan rejection |
| Acceptance fixtures | Add metadata/tamper fixtures for ScriptGroup views, outputs_data mismatch, and TYPE_ID create-plan failure cases; defer dedicated accepted/rejected CKB transaction fixture matrices to the later standard compatibility suite |

**Boundary**: This is not the v0.15 identity-policy redesign. v0.14 validates CKB transaction-shape facts and existing TYPE_ID metadata plans. It does not add new identity primitives, destruction policies, or protocol macro lowering.

**Risk**: **HIGH** — Mis-modeling ScriptGroup or TYPE_ID behavior creates false confidence in CKB strict mode
**Depends on**: Structured CKB WitnessArgs and Source Views (#2), Target Profile Formalization (#3)

---

### P1 - Important (Strongly Recommended)

#### 5. Declarative Capacity Syntax 🟡

**Problem**: Capacity management is the most common source of CKB transaction failures. The compiler, builder, and acceptance layers expose capacity evidence, but the DSL has no declarative capacity policy — developers still reason about byte counts and change outputs outside the source contract.

**DSL Design**:

**Declaration form — compile-time static capacity floor**:
```cellscript
resource Token has store, transfer, destroy
with_capacity_floor(6100000000)  // minimum 61 CKB
{
    amount: u64
    symbol: [u8; 8]
}
```

**Action-level capacity visibility**:
```cellscript
action capacity_visible(amount: u64) -> output: Token
where
    let floor = occupied_capacity("Token")
    assert(floor >= 0, "capacity floor visible")
    create output = Token { amount }
```

**Implementation Items**:

| Item | Details |
|------|---------|
| `with_capacity_floor(...)` declaration | Parser + AST declaration + validation; support explicit shannons. Compiler-computed floors remain builder/acceptance evidence for now. |
| `occupied_capacity("TypeName")` helper | Metadata-visible CKB capacity policy helper keyed by type name |
| Capacity floor check insertion | Metadata and constraints expose required floors; automatic verifier insertion remains future work unless separately promoted. |
| Builder integration | Existing acceptance measures occupied capacity and tx size; automatic change-output generation remains future builder work. |

**Risk**: **LOW** — Additive syntax, no breaking changes
**Depends on**: Transaction Builder Integration (#10) for full change-output automation; standalone static checks can land earlier

---

#### 6. Declarative Time and Since Constraints 🟡

**Problem**: Time-based constraints (`since` encoding) require CKB-specific handling for block-number, epoch-with-fraction, and timestamp metrics. The low-level `ckb::input_since()` and header epoch APIs work, but they expose raw encoding details and do not express policy at the DSL level.

**DSL Design**:

```cellscript
action claim_after_ckb_timeout(htlc: HtlcReceipt)
where
    require_maturity(100)                 // CKB: block-number delta
    require_time(target)                  // CKB: absolute timestamp since
    require_epoch_after(10, 0, 1)         // CKB-only absolute epoch since
    require_epoch_relative(10, 0, 1)      // CKB-only relative epoch since
    consume htlc
```

**Profile-gated Compilation**:

| Primitive | Implemented CKB Profile | Future Portable Profile Boundary |
|-----------|-------------|---------------|
| `require_maturity(N)` | Relative block-number since obligation | Must reject unless a portable semantics is explicitly designed |
| `require_time(T)` | Absolute timestamp since obligation | Must reject unless a portable semantics is explicitly designed |
| `require_epoch_after(number, index, length)` | Absolute epoch since obligation | Must reject |
| `require_epoch_relative(number, index, length)` | Relative epoch since obligation | Must reject |

**Implementation Items**:

- `require_maturity(N)` → typed helper call + CKB-profile runtime-access lowering
- `require_time(T)` → typed helper call + CKB-profile runtime-access lowering
- `require_epoch_after(number, index, length)` and `require_epoch_relative(number, index, length)` expose CKB epoch since obligations
- Epoch helper arguments are type-checked and metadata-visible; full consensus-vector and ordering-rule coverage remains a later compatibility-suite concern unless promoted with tests
- Coexistence: `ckb::input_since()` low-level API remains available (not removed)

**Risk**: **MEDIUM** — CKB epoch since semantics must match consensus exactly
**Depends on**: Target Profile Formalization (#3)

---

#### 7. `hash_blake2b()` Fixed-Hash Runtime Helper ✅

> Tracked as part of Target Profile Formalization (#3c) and promoted for the v0.14 CKB compatibility surface.

- `hash_blake2b(input: Hash) -> Hash` is supported for runtime 32-byte digest inputs.
- The helper uses CKB Blake2b-256 personalization `ckb-default-hash`.
- Stubs are forbidden; the codegen path emits an executable RISC-V mixing helper and stores the 32-byte result in a caller-owned buffer.
- Wider byte-slice/resource serialization hashing remains out of scope until its ABI and Molecule serialization contract are specified.

**Risk**: **MEDIUM**
**Depends on**: Target Profile Formalization (#3)

---

#### 8. Script Reference and HashType Strictness 🟡

**Problem**: v0.13 exposes hash type configuration, but v0.14 CKB semantic completeness needs strict script-reference records for deployed artifacts and dep cells. A CKB script reference is not just a hash string; it is `code_hash + hash_type + args` plus the dep-cell path that makes the script loadable.

**Implementation Items**:

| Item | Details |
|------|---------|
| Script reference metadata | Emit `code_hash`, `hash_type`, `args`, dep source, and resolved profile for lock/type/spawn targets |
| HashType validation | Accept only CKB-supported hash types under CKB profile; reject unknown or profile-incompatible values |
| Dep-cell linkage checks | Verify metadata shape for script references used by `spawn`, lock/type metadata, action-boundary `read` parameters, or expression-level `read_ref<T>()`; full registry-backed CellDep/DepGroup resolution remains deferred |
| Audit output | Include script reference table in generated audit docs and metadata validation errors |

**Boundary**: This does not split `Address`, `LockScript`, and `LockHash` in the type system. That is v0.15. v0.14 only makes CKB artifact references precise and auditable.

**Risk**: **MEDIUM** — Incorrect hash_type or dep linkage can produce artifacts that look valid but cannot execute on CKB
**Depends on**: Target Profile Formalization (#3), Advanced CellDep Patterns (#11) for full DepGroup coverage

---

### P2 - Optimization (v0.14 Stretch or Later)

#### 9. WASM Script Execution Backend 🟢

**Problem**: The current WASM backend is an audit-only scaffold. Developers cannot run CellScript contracts in browsers for simulation and testing.

**Goal**: CellScript → WASM compilation for browser-side script simulation and testing.
**Non-Goal**: On-chain WASM execution. RISC-V remains the on-chain target.

**Current nightly boundary**: Not covered. The current WASM module is audit-only
and rejects executable action/lock lowering. This track must not be described as
release-covered until it has executable tests and documented CKB/WASM divergence
rules.

**Implementation Items**:
- WASM codegen backend (parallel to existing RISC-V backend)
- Syscall shim layer: mock `spawn`, `pipe`, `read`, `write`, `wait` in JS/WASM environment
- Browser test harness: load compiled WASM, inject mock cells/witnesses, run actions
- Integration with existing `wasm/` SDK package

**Risk**: **MEDIUM** — Syscall shimming complexity
**Depends on**: Spawn/IPC DSL (#1)

---

#### 10. Transaction Builder Language Integration 🟢

**Continued from v0.13 P2 stretch goal.**

**Problem**: Building transactions that exercise CellScript actions requires manual JSON/SDK construction. The compiler knows the full transaction shape — it should generate builder templates.

**Implementation Items**:
- `cellc build --emit-builder-template` outputs a transaction skeleton
- Builder auto-capacity planning: compute minimum capacity per output from type layout
- CellDep auto-resolution: resolve script references to dep cells from registry

**Risk**: **HIGH** — Transaction builder correctness is critical
**Depends on**: Declarative Capacity Syntax (#5)

---

#### 11. Advanced CellDep Patterns 🟢

**Problem**: Complex scripts depend on multiple dep cells (shared libraries, data cells, verifier scripts). Current dep cell handling is manual and flat.

**Implementation Items**:

- DepGroup dynamic composition: declare a group of related dep cells
- Multi-module CellDep dependency graph: compiler resolves transitive deps
- Shared code cell version locking: pin dep cell `out_point` in manifest

**Current nightly boundary**: P2/later. v0.14 exposes script-reference and
CellDep metadata obligations, but does not ship transitive registry-backed
CellDep resolution or dynamic DepGroup composition.

**Risk**: **LOW**
**Depends on**: None

#### 12. Surface Ergonomics Backlog 🟢

**Problem**: v0.13 intentionally prioritizes verifier correctness and explicit CKB semantics over syntax sugar. Several useful ergonomic features are good candidates for v0.14 design, but they are not v0.13 correctness blockers.

**Deferred from the 0.13 syntax audit**:
- Optional source-level `transfer token { ... } with_lock(to)` sugar remains deferred. v0.13.2 already provides compiler-recognized stdlib lifecycle patterns such as `std::lifecycle::transfer`, which expand to explicit `consume` + named output constraints.
- `create_each` or bounded batch-create sugar that compiles to statically auditable repeated `create` operations.
- Named tuple returns such as `-> (royalty: Payment, seller: Payment)` for readability without changing ABI layout.
- `Option<T>` / `Result<T, E>` as an explicit optional/error model, including type checking, lowering, ABI representation, and match-pattern support.
- Attribute-form hash type declarations such as `#[default_hash_type(Data1)]` as a possible spelling alongside or instead of `with_default_hash_type(Data1)`.

**Boundary**: These items must not hide Cell layout, invent recoverable verifier errors casually, or weaken fail-closed semantics. Each item needs parser, type checker, lowering, codegen, formatter, LSP, docs, and regression coverage before promotion.

---

## 🔧 Peripheral Tool Coordination

v0.14 introduces Spawn/IPC and profile formalization at the DSL layer. Peripheral tools need targeted updates:

| Component | Path | v0.14 Work |
|-----------|------|------------|
| **Wallet** | `wallet/` | Later integration track: spawn-aware transaction construction must pass child script deps and respect shared cycle budget |
| **SDK Adaptor** | `sdk/adaptor/` | Later integration track: spawn transaction examples and capacity planning APIs |
| **WASM SDK** | `wasm/` | Deferred; current WASM backend is audit-only and fail-closed for executable entries |
| **Standard Scripts** | `exec/src/scripts/` | Language examples cover bounded spawn verifier patterns; production standard-script packaging remains separate |
| **CLI** | `cli/` | v0.14 ships compile/metadata/profile evidence; no `cellc spawn-test` release claim unless separately implemented |
| **CI** | `.github/workflows/` | CKB-profile tests and release gates cover implemented 0.14 surfaces |

---

## 🎯 Success Metrics

### Feature Completeness

| Metric | Target |
|--------|--------|
| All CKB-targeted bundled examples compile under CKB profile | ✅ Required |
| At least 2 spawn-based language examples | ✅ Required |
| Structured `WitnessArgs.lock/input_type/output_type` tests and examples pass under CKB profile | ✅ Required |
| Source global/group view tests pass under CKB strict mode | ✅ Required |
| ScriptGroup metadata matches CKB lock/type group metadata/tamper fixtures | ✅ Required |
| `outputs` ↔ `outputs_data` binding metadata tests reject detached or mismatched output data | ✅ Required |
| CKB TYPE_ID metadata validation covers create, duplicate stable type_id, and missing/mismatched metadata-plan cases | ✅ Required |
| CKB `require_epoch_after` and `require_epoch_relative` tests match the expected metadata/runtime surface | ✅ Required |
| Capacity floor metadata covers 100% of declared `with_capacity_floor` operations and rejects mismatched top-level constraint records | ✅ Required |
| Script reference metadata includes `code_hash`, `hash_type`, `args`, declared dep source, and exact expected-vs-actual metadata validation | ✅ Required |
| Zero regression on v0.12 production evidence | ✅ Required |
| Profile hash policy exposes dynamic fixed-Hash BLAKE2b with metadata-visible `CKB_BLAKE2B` access | ✅ Required |
| `hash_blake2b(input: Hash)` compiles to assembly/ELF and is covered by the real `timelock.cell` `lock_id_commitment` valid/invalid CKB lock-spend flow | ✅ Required |
| Profile semantic spec published | ✅ Required |

### Profile CI Gate

All features introduced in v0.14 must pass CKB profile CI:
```bash
for file in examples/*.cell; do
    cellc "$file" --target-profile ckb
done
```

---

## 🚫 Non-Goals for v0.14

| Non-Goal | Rationale |
|----------|-----------|
| Epoch support outside CKB profile | Epoch is CKB-specific and must not leak into portable semantics. |
| On-chain WASM execution | RISC-V remains the on-chain target. Executable browser/WASM simulation is deferred until a tested harness exists. |
| Reopening the action model beyond state-edge spelling | Signature-direction outputs, `where`, named `create`, and stdlib lifecycle patterns stay intact; v0.14 only renames action-level state edges from legacy `move` to `transition` and rejects the old spelling. |
| Broad breaking DSL changes | v0.14 intentionally makes the `move` -> `transition` cleanup without compatibility aliases; other 0.13.2 syntax should remain stable unless separately justified. |
| Primitive kernel reset | v0.15 owns protocol-macro lowering, ProofPlan unification, and core primitive redesign. |
| Reintroducing compiler-core `transfer` / `claim` / `settle` verbs | v0.13.2 removed these from the executable core; v0.14 may add source sugar only when it expands to auditable stdlib/core effects. |
| `Address` / `LockScript` / `LockHash` type-system split | v0.14 records precise CKB script references; v0.15 owns semantic type separation. |
| Destruction-policy redesign | Bare `destroy` behavior is not redefined in v0.14; explicit destruction policies are v0.15 scope. |
| Formal verification | Future milestone (v0.16+). v0.14 focuses on bounded verifier composition, not proof. |
| `T: CellBacked` / `T: Linear` generic constraints | Deferred to v0.15+ per the phased generics plan from v0.13. |
| Full generic `HashMap<K, V>` | Remains fail-closed per v0.13 boundary. |
| Recoverable verifier error model by default | CellScript remains a verifier DSL: failed validation rejects the transaction. Optional/error types require an explicit design before source-level use. |

---

## ⚠️ Risks and Mitigations

### Risk 1: Spawn Cycle Budget Allocation 🟡

**Scenario**: Parent script spawns children that consume unbounded cycles, making total cycle cost unpredictable.

**Mitigation**: Use CKB's existing shared budget model — parent and children share a total cycle limit. v0.14 records Spawn/IPC runtime accesses and requires builder/dry-run evidence for concrete transactions. A source-level `max_cycles` parameter is not a release claim.

---

### Risk 2: Profile Divergence on New Features 🟡

**Scenario**: New features (spawn, WitnessArgs, Source views, capacity syntax, time constraints) are accidentally described as portable even though only the CKB profile is implemented.

**Mitigation**: Keep the release profile explicit: `TargetProfile::Ckb` is the implemented profile, and unsupported target-profile names fail closed. Future portable-profile work must add its own tests before any portability claim.

---

### Risk 3: WitnessArgs and Source View Misbinding 🔴

**Scenario**: A lock or type script reads the wrong witness slot, wrong `WitnessArgs` field, or wrong transaction/group Source view. That can turn a signature or proof check into a false positive or false negative.

**Mitigation**:
- Structured witness APIs must always include source view and index in metadata.
- CKB profile lowers structured `WitnessArgs` field reads explicitly and records the runtime access.
- Metadata/tamper tests cover wrong runtime-access and ScriptGroup records.
- Malformed WitnessArgs transaction fixtures remain part of the later compatibility suite.
- Non-CKB profiles must not pretend raw witness bytes are CKB `WitnessArgs` unless compatibility mode is explicit and tested.

---

### Risk 4: CKB Epoch Since Semantics Drift 🔴

**Scenario**: `require_epoch` compiles but encodes or compares CKB `EpochNumberWithFraction` incorrectly, breaking DAO-style or epoch-maturity contracts.

**Mitigation**:
- Reuse CKB-compatible bit encoding and well-formedness rules in tests.
- Include absolute and relative epoch cases against the expected metadata/runtime surface.
- Keep `require_epoch` unavailable outside CKB profile; do not emulate epoch in portable semantics.

---

### Risk 5: Capacity Proof Completeness 🟢

**Scenario**: Compile-time capacity floor checks may be too conservative (rejecting valid transactions) or too lenient (missing edge cases like dynamic-length fields).

**Mitigation**:
- Conservative default: compiler checks based on fixed-width layout only
- Dynamic-length fields still require builder-side capacity evidence
- `with_capacity_floor(...)` allows developer override when compiler estimate is insufficient
- Builder integration provides a second safety net at transaction construction time

---

### Risk 6: Dynamic BLAKE2b Scope Creep 🟡

**Scenario**: Dynamic in-script BLAKE2b scope expands from fixed 32-byte digest hashing into arbitrary byte-slice or resource serialization hashing without a stable ABI.

**Mitigation**: Keep v0.14 scoped to `hash_blake2b(input: Hash) -> Hash`. Any wider hashing surface must define byte ownership, length ABI, serialization, vectors, cycle limits, and production gate evidence before promotion.

---

### Risk 7: WASM Syscall Shim Fidelity 🟢

**Scenario**: WASM simulation environment diverges from actual on-chain behavior, giving false confidence.

**Mitigation**: The executable WASM simulation backend is deferred. The current `src/wasm` module is audit-only and fail-closed for executable action/lock modules, so 0.14 must not claim browser simulation until runnable harness tests exist.

---

### Risk 8: ScriptGroup and Transaction Shape Drift 🔴

**Scenario**: CellScript metadata claims a group/source/output-data relation that CKB would not actually provide to the running script.

**Mitigation**:
- Test lock and type ScriptGroup metadata against CKB-profile runtime-access records.
- Treat `outputs[i]` and `outputs_data[i]` as one indexed pair in metadata validation.
- Include negative metadata/tamper tests for wrong group source/runtime-access drift and detached output data. Dedicated accepted/rejected transaction fixture matrices are deferred to the compatibility suite.

---

### Risk 9: TYPE_ID MVP Scope Creep 🟡

**Scenario**: v0.14 TYPE_ID validation turns into a full identity-policy primitive redesign.

**Mitigation**: v0.14 only validates existing `#[type_id]` metadata plans and CKB transaction-shape facts. New identity policies and destruction-policy redesign remain v0.15 scope.

---

## 📝 Integration with Existing Plans

### 0.13.2 Production Plan Carry-Over

v0.14 **extends** the 0.13.2 production plan:

- ✅ CKB production gate remains 43/43+ actions, with stateful coverage for every production acceptance action
- ✅ 7+ bundled examples remain regression test suite (extended only when new v0.14 features are production-gated)
- ✅ Stateful business-flow acceptance remains a full release requirement
- ✅ Molecule ABI remains public format
- ✅ Registry remains fail-closed
- **New**: Profile semantic spec becomes a mandatory production artifact
- **New**: CKB ScriptGroup, outputs_data, and TYPE_ID metadata/tamper validation fixtures become mandatory CKB strict-mode evidence; dedicated accepted/rejected transaction fixture matrices stay deferred to the later compatibility suite
- **New**: CKB-profile CI becomes a release gate; unsupported target-profile names fail closed until future profiles are implemented

### v0.13 Stretch Goals Carried Forward

| v0.13 P2 Item | v0.14 Status |
|----------------|-------------|
| Transaction Builder MVP | → v0.14 P2 (#10), extended with capacity planning |
| Loop Unrolling | Completed in v0.13 or deferred to v0.15 |
| Broader Fuzz Testing | Ongoing, not version-gated |

---

## 🚀 Quick Start

### Development Commands

```bash
# Run all CellScript tests
./scripts/cellscript_gate.sh ci

# Compile all examples through the CKB top-level file workflow
for file in examples/*.cell; do
    cargo run -p cellscript -- "$file" --target-profile ckb
done

# Check profile-specific compilation
cargo run -p cellscript -- explain-profile ckb
```

### New Examples to Ship with v0.14

| Example | Pattern | Features Exercised |
|---------|---------|-------------------|
| `examples/language/v0_14_delegate_verify.cell` | Lock script spawns external verifier | `spawn`, `wait`, runtime-required spawn CellDep obligation |
| `examples/language/v0_14_multi_step_pipeline.cell` | Pipe-connected verification chain | `spawn`, `pipe`, `pipe_write`, `pipe_read`, `wait`, `close` |
| `examples/language/v0_14_witness_source.cell` | CKB-style lock reads `WitnessArgs.lock` | `witness::lock`, `source::group_input(0)` |
| `examples/language/canonical_style.cell` | Canonical lock/action boundary style | `protected`, `lock_args`, `witness`, `env::sighash_all` |
| `examples/language/v0_14_ckb_type_id_create.cell` | TYPE_ID creation metadata | `#[type_id]`, output index plan, missing/mismatched metadata-plan validation in tests |
| `examples/language/v0_14_capacity_time.cell` | Capacity and CKB since policy | `with_capacity_floor`, `occupied_capacity("TypeName")`, `require_maturity`, `require_time`, absolute and relative epoch helpers |
| `examples/language/v0_14_hash_blake2b.cell` | Dynamic fixed-Hash Blake2b | `hash_blake2b(input: Hash)` |
| `examples/language/registry.cell` | Bounded local registry sketch | Stack-backed value-vector helpers, not persistent registry deployment |

---

## 🎉 Summary

**v0.12 proved CellScript can compile production-grade cell contracts.**
**v0.13 proved CellScript has a stable explicit verifier surface with strict CKB release evidence.**
**v0.14 will prove CellScript exposes bounded verifier composition, and the CKB target-profile contract is explicit and testable.**

v0.14 delivers:

- **Bounded Verifier Composition**: First-class `spawn`/`pipe`/`wait`/fd operations in DSL, mapped to VM syscalls 2601-2608, without claiming multi-tenant type-script composition
- **CKB Semantic Completeness**: Structured `WitnessArgs`, explicit Source views, CKB epoch since, and a formalized CKB profile contract. Portable profile behavior remains a future explicit target.
- **CKB Transaction Conformance**: ScriptGroup metadata, outputs_data binding, TYPE_ID metadata validation MVP, and strict script-reference records
- **Declarative Safety**: `with_capacity_floor`, `occupied_capacity("TypeName")`, `require_maturity`, `require_time`, `require_epoch_after`, and `require_epoch_relative`
- **Hash Policy Clarity**: Profile-aware hash-domain metadata and fixed-Hash dynamic BLAKE2b support with production-gated evidence
- **Simulation**: Deferred P2. WASM remains audit-only until executable browser-side tests exist.

**Expected Outcomes**:
- Bounded verifier reuse patterns unlocked (delegate verify, multi-step pipelines)
- CKB lock/type witness patterns become source-level, typed, and auditable
- CKB transaction shape assumptions become metadata/tamper-tested instead of implicit; dedicated accepted/rejected transaction fixture matrices stay deferred to the compatibility suite
- Profile divergence becomes explicit instead of implicit
- Capacity floors and measurement obligations become explicit; builders still own funding, change, occupied-capacity, and tx-size evidence
- Foundation laid for the v0.15 primitive-kernel reset and later formal verification

---

*Document End.*
*Status: Draft (Pending Team Review)*
*Prerequisites*: [CELLSCRIPT_0_13_RELEASE_SCOPE.md](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md), [0.13.2 production plan carry-over](#0132-production-plan-carry-over)

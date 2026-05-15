# CellScript Roadmap: v0.12 → v0.16
> From Production Foundation to Formal Assurance

**Status**: Living Document
**Audience**: CKB Smart Contract Developers

---

## 1. The Big Picture

CellScript's mission is to make the power of CKB's Cell model — linear resources, capacity gating, script-based verification — accessible through compile-time type safety. Each release addresses a core question in that journey.

| Version | Theme | One-liner | Status |
|---------|-------|-----------|--------|
| v0.12 | Production Foundation | "Write real CKB contracts safely" | ✅ Released |
| v0.13 | Bounded Helpers & Evidence-Gated Optimization | "Write less while keeping helper boundaries explicit" | ✅ Released |
| v0.14 | CKB Semantic Completeness | "Expose CKB surface and bounded verifier reuse" | ✅ Implemented |
| v0.15 | Scoped Invariants & Covenant ProofPlan | "Show when constraints run, what they read, and who they protect" | 🚧 In Progress |
| v0.16 | Formal Semantics & Production Tooling | "Prove, validate, deploy, and audit" | 📋 Planned |

**Evolution arc**:
- **v0.12** — *Can we use it?* Prove CellScript can compile production-grade CKB contracts.
- **v0.13** — *Is it good to use?* Improve bounded helper ergonomics, CLI workflows, and optimization evidence without turning benchmark ideas into release promises.
- **v0.14** — *Is the CKB surface complete?* Cover Spawn as bounded verifier composition, WitnessArgs, Source views, ScriptGroup, outputs_data binding, TYPE_ID metadata validation, script references, capacity, and time constraints.
- **v0.15** — *Is the safety boundary auditable?* Model scoped invariants, covenant triggers, coverage, builder assumptions, and ProofPlan output without hiding lock/type semantics.
- **v0.16** — *Can we trust it in production?* Add formal semantics, ProofPlan soundness checks, standard CKB compatibility suites, transaction solving, deployment governance, and audit tooling.

---

## 2. v0.12 — Production Foundation (Released)

**What it delivered**: A production-ready compiler that turns CellScript source into optimized RISC-V ELF binaries for CKB VM, with compile-time safety guarantees no existing CKB toolchain provides.

### 2.1 Linear Type System for Cell Safety

CellScript models Cells with three type classes:

| Type Class | CKB Mapping | Capabilities |
|-----------|-------------|-------------|
| `resource` | Consumed Cell (CellInput) | `has store, create, consume, replace, burn, relock` |
| `shared` | Reference Cell (CellDep) | read-only, no consumption |
| `receipt` | Proof / witness artifact | consumed via `consume` |

Compile-time safety guarantees:
- **Double-spend prevention**: Linear state tracking (`Available → Consumed / Destroyed`) — the compiler rejects any code path that uses a Cell after consumption.
- **Branch consistency**: Both sides of an `if-else` must leave every resource in the same state.
- **Capability gating**: destructive operations require either legacy compatibility capability or the 0.15 `consume + burn` kernel effects.

### 2.2 Cell Effect Operations

```cellscript
consume token                                       // consume Cell input, reclaim capacity
create token = Token { amount: 100 } with_lock(recipient) // constrain named Cell output
destroy token                                       // destroy (requires destroy capability)
read oracle: OracleData                             // non-consuming read from CellDep
require pool_after.reserve_a == pool.reserve_a + delta // explicit output constraint
```

### 2.3 Entry Witness ABI (CSARGv1)

- Structured parameter passing via Witness field
- Serialization of scalars, fixed bytes, and schema-backed dynamic data
- CellScript entry witness ABI; structured CKB `WitnessArgs` field access lands in v0.14

### 2.4 CKB Syscall Integration

- Complete coverage of CKB VM syscalls (`load_cell`, `load_header`, `load_witness`, `load_cell_data`, etc.)
- Standard Lock Script verification (secp256k1 signature)
- Four timelock patterns: absolute/relative × block-height/timestamp

### 2.5 Production Evidence

- **43/43** production actions compiled and accepted
- **7 example contracts**: `token`, `amm_pool`, `vesting`, `timelock`, `multisig`, `nft`, `launch`
- Occupied-capacity evidence recorded per action

---

## 3. v0.13 — Bounded Helpers & Evidence-Gated Optimization (In Progress)

**Theme**: Write less code, keep ownership boundaries explicit, and make optimization evidence auditable.

### 3.1 Bounded Value-Vector Helpers (P0)

- Stack-backed `Vec<Address>`, `Vec<Hash>`, `Vec<u64>` and other fixed-width value-vector helpers
- Helps simple registries, whitelists, fixed membership sets, and AMM helper patterns
- Proof-backed maps, order books, and cell-backed collection ownership remain explicit future work
- Compile-time monomorphization for supported value helpers without hidden ownership semantics
- Value-level generics only — linear/cell-backed ownership remains explicit and fail-closed

### 3.2 Zero-Cost Abstractions (P0)

| Optimization | Evidence Required Before Performance Claim |
|-------------|---------------------------------------------|
| Deserialization specialization | Tagged v0.12 comparison plus backend shape and CKB cycle data |
| Function inlining (core lib) | Instruction/backend shape delta under the same target profile |
| Dead code elimination | ELF/text-size delta plus regression guard for all bundled examples |
| Constant propagation | Assembly/backend shape delta for representative examples |

No fixed v0.13 size or cycle target is claimed here; comparative benchmark
reports are future/evidence-gated release material.

### 3.3 CLI Ergonomics (P0)

- `cellc new` — project scaffolding (Cargo-compatible workflow)
- `cellc build` — default O1 optimization
- Error code system + `cellc explain <code>` — Rustc-style diagnostics with `codespan-reporting`
- Code formatting support (future milestone)

---

## 4. v0.14 — CKB Semantic Completeness (Planned)

**Theme**: Expose CKB's full execution surface before redesigning higher-level primitives.

### 4.1 Spawn/IPC Bounded Verifier Composition (P0)

```cellscript
// Delegate verification: Lock Script spawns a child verifier
action verify_with_delegate(proof: Proof)
where
    let result = spawn("secp256k1_verifier", args: [proof.pubkey, proof.signature])
    assert(result == 0, "verification failed")

// Pipe-based multi-step verification chain
action multi_step_verify(data: VerifyData)
where
    let (read_fd, write_fd) = pipe()
    spawn("hash_checker", fds: [read_fd])
    pipe_write(write_fd, data.payload)
    let result = wait()
    assert(result == 0, "hash check failed")
```

- Maps to CKB VM v2 Spawn syscalls (2601–2606)
- Type-safe inter-process communication
- Compile-time cycle budget static analysis
- Does not make a CKB cell's type script slot multi-tenant; full protocol composability remains a v0.15+ ProofPlan/scoped-invariant concern

### 4.2 Structured WitnessArgs & Source Views (P0)

```cellscript
lock standard_lock(lock_args args: OwnerArgs, witness sig: Signature) -> bool {
    let sig = witness::lock<Signature>(source: source::group_input(0))
    let proof = witness::input_type<ProofData>(source: source::group_input(0))

    let pubkey_hash = args.pubkey_hash
    assert(
        secp256k1_verify(pubkey_hash, sig, env::tx_hash()),
        "signature verification failed"
    )
}
```

- Function-style witness field access
- Dual source views: full Transaction view vs ScriptGroup view
- `SOURCE_GROUP_INPUT` / `SOURCE_GROUP_OUTPUT` compile-time switching

### 4.3 CKB Transaction Shape & ScriptGroup Consistency (P0)

- ScriptGroup metadata for lock/type entries
- `outputs[i]` ↔ `outputs_data[i]` binding obligations
- Source conformance fixtures for global and group views
- TYPE_ID metadata validation MVP: output index, first-input args source, group cardinality, duplicate/missing-plan rejection
- Explicit boundary: no v0.15 identity-policy redesign in v0.14

### 4.4 Script Reference & HashType Strictness (P1)

- CKB script reference metadata: `code_hash`, `hash_type`, `args`, dep source, resolved profile
- CKB profile rejects unsupported or profile-incompatible hash types
- Every script reference used by spawn, lock/type metadata, action-boundary
  `read` parameters, or expression-level `read_ref<T>()` must link to a
  CellDep/DepGroup path
- Audit output includes a script reference table

### 4.5 Declarative Capacity Syntax (P1)

```cellscript
@capacity_floor(61_00000000)  // minimum 61 CKB (in Shannons)
resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

action transfer_with_fee(token: Token, fee: u64) -> next_token: Token
where
    let freed = consume token
    assert(freed >= occupied_capacity(Token) + fee, "insufficient")
    create next_token = Token { amount: token.amount } with_lock(recipient)
```

### 4.6 Declarative Time Constraints (P1)

```cellscript
action claim_after_ckb_timeout(htlc: HtlcReceipt) -> recovered: Token
where
    require_maturity(blocks: 100)               // CKB block-number lock
    require_time(after: Timestamp(1714000000))  // CKB timestamp since
    consume htlc
    create recovered = Token { amount: htlc.amount } with_lock(htlc.beneficiary)
```

### 4.7 Fixed-Hash hash_blake2b() Support (P1)

> **Status:** v0.14 supports `hash_blake2b(input: Hash) -> Hash` for runtime
> 32-byte digest inputs. Wider byte-slice and resource serialization hashing
> remains out of scope until its ABI is specified.

- CKB-native BLAKE2B hash function (with `"ckb-default-hash"` personalization)
- CKB Blake2b helper support selected by the CKB target profile

---

## 5. v0.15 — Scoped Invariants & Covenant ProofPlan (In Progress)

**Theme**: Make CKB safety boundaries explicit instead of hiding lock/type differences.

v0.15 is not an automatic constraint-placement system. Lock and type scripts have different execution triggers and coverage models, so CellScript should expose those semantics directly:

```text
constraint = what must hold
trigger    = when the verifier runs
scope      = which cell universe it reasons over
reads      = which transaction views it observes
coverage   = which cells are actually protected
```

**Implementation status**: P0 complete, P1 partial. The following items have been implemented:

- First-class script semantics (scoped invariants with trigger/scope/reads)
- Aggregate invariant primitives (metadata-only aggregate lowering, with
  invariant/action coverage links)
- Covenant ProofPlan metadata and `cellc explain-proof`
- Invariant/action coverage summaries in ProofPlan, CLI, and audit docs
- Runtime-obligation policy gate
- Lock-group transaction risk diagnostics
- Protocol macro provenance
- Cell identity and TYPE_ID lifecycle (`IdentityPolicy`, `CreateUnique`/`ReplaceUnique`)
- Explicit destruction policies (`DestructionPolicy`)
- Kernel/protocol primitive split (extended `Capability`)
- Capability vocabulary reset (strict/compat modes)
- Internal `type_hash` renaming
- Compatibility and migration infrastructure (`--primitive-compat`/`--primitive-strict`)

Deferred to v0.16 production assurance:

- executable verifier lowering for aggregate invariants
- full ProofPlan soundness checker
- formal invariant satisfaction gate beyond bounded action-coverage matching
- full macro-only lowering with no protocol-name recognizers in core/codegen
- covenant helper stdlib
- `Address` / `LockScript` / `LockHash` type split
- explicit `#[entry(lock)]` / `#[entry(type)]` script roles
- versioned cell data-layout preserve/migrate policies
- full `cellc explain-macro` source maps
- non-TYPE-ID global uniqueness proof boundaries
- standard CKB compatibility suite with accepted/rejected transaction fixtures,
  ScriptGroup / `outputs_data` matrices, cycles, and script reference metadata

### 5.1 First-Class Script Semantics (P0)

```cellscript
invariant udt_amount_non_increase {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>, group_outputs<Token>

    assert sum(group_outputs<Token>.amount) <= sum(group_inputs<Token>.amount)
}
```

- `trigger = lock_group | type_group | explicit_entry`
- `scope = group | transaction | selected_cells`
- `reads = input | output | group_input | group_output | cell_dep | header_dep | witness`
- `coverage` is emitted in metadata and audit output
- `lock_group + transaction` produces a coverage warning unless explicitly acknowledged

### 5.2 Scoped Aggregate Invariants (P0)

```cellscript
assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
assert_conserved(Token.amount, scope = group)
assert_delta(Token.amount, witness.delta, scope = selected_cells)
assert_distinct(outputs<NFT>.id, scope = transaction)
assert_singleton(type_id, scope = group)
```

- Every aggregate invariant must bind an explicit scope
- Field types must be fixed-width integers or fixed bytes
- Non-literal `assert_delta` values must be bound to `witness.*` or `lock_args.*`
- Future executable aggregate lowering must fail closed on overflow and malformed
  cell data
- UDT, pool, settlement, and covenant helpers are represented through these
  primitives in ProofPlan; executable aggregate verifier-loop lowering remains
  v0.16 scope

### 5.3 Covenant ProofPlan (P0)

`cellc explain-proof` becomes the key audit surface:

```text
constraint: udt_amount_non_increase
trigger: lock_group
scope: transaction
reads:
  - Source::Input
  - Source::Output
coverage:
  - only inputs sharing this lock script
warning:
  - Not equivalent to type-group conservation unless all relevant UDT inputs are locked by this lock.
on_chain_checked: yes
builder_assumption: none
```

ProofPlan records:

- invariant name and source span
- trigger / scope / reads / coverage
- input/output relation checks
- group cardinality
- identity policy
- on-chain checked obligations
- builder assumptions
- codegen coverage status
- invariant action coverage status, including matched checked action obligations
  and unmatched declarations

### 5.4 Protocol Macros Lower Through Scoped Invariants (P0)

Protocol verbs move out of compiler-core recognizers:

- `transfer` (stdlib: consume + create with lock mapping)
- `claim` (stdlib: receipt consume + output create)
- `settle` (stdlib: receipt finalize + state transition)
- `shared`
- pool/AMM flows

They become stdlib proof macros that expand into:

- kernel cell operations
- scoped aggregate invariants
- explicit ProofPlan obligations
- macro expansion provenance

### 5.5 Identity, Destroy, and Script-Type Precision (P0/P1)

v0.15 promotes identity and script semantics beyond the v0.14 metadata MVP:

- First-class cell identity policies: `ckb_type_id`, field identity, script args, singleton type
- TYPE_ID identity policy across create / update-output / destroy
- Explicit destruction policies: `destroy_unique`, `destroy_instance`, `burn_amount`, `destroy_singleton_type`
- Public metadata rename: `dsl_type_fingerprint` vs `ckb_type_script_hash`

### 5.6 Covenant Helper Stdlib and Migration (P1/P2)

Ergonomic helpers are allowed, but they must not perform automatic lock/type placement:

```text
lock_covenant(...)
type_invariant(...)
builder_assumption(...)
selected_cells(...)
```

Migration support:

- `--primitive-compat=0.14`
- `--primitive-strict=0.15`
- diagnostics for implicit trigger/scope, protocol capabilities, metadata-only obligations, and builder-only assumptions

---

## 6. v0.16 — Formal Semantics & Production Tooling (Planned)

**Theme**: Turn v0.15's semantic audit layer into production assurance.

v0.16 does not add another large DSL surface. It proves, validates, and operationalizes the v0.14/v0.15 model:

- formal semantics
- ProofPlan soundness checks
- invariant satisfaction/completeness checks
- executable aggregate invariant lowering
- macro-only protocol lowering
- CKB standard compatibility suites
- explicit lock/type role and identity-type boundaries
- versioned data-layout policies
- non-TYPE-ID uniqueness proof boundaries
- builder assumption validation
- transaction solving
- deployment governance
- audit/debug tooling

### 6.1 Formal Operational Semantics (P0)

- Formal rules for expression evaluation, linear resource states, branch merge, cell effects, lock/type triggers, scopes, and ProofPlan obligations
- Spec artifact: `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`
- Conformance fixtures linked to compiler tests

### 6.2 ProofPlan Soundness Checks (P0)

Strict mode must prove the chain:

```text
source invariant
  -> ProofPlan obligation
  -> IR operation
  -> codegen check
  -> metadata coverage record
```

Rejected cases:

- metadata-only obligations marked as on-chain
- stale ProofPlan after optimization
- mismatched Source views
- unchecked builder assumptions
- missing emitted checks

v0.16 also adds a separate invariant satisfaction gate: every declared
invariant must have executable aggregate coverage or checked action-obligation
coverage, otherwise strict mode fails instead of leaving the audit trail as a
manual comparison exercise.

### 6.3 Standard CKB Compatibility Suite (P0)

Compatibility fixtures for:

- sUDT / xUDT
- ACP
- Cheque
- Omnilock-compatible lock patterns
- NervosDAO-style epoch/since cases
- Type ID

Each suite covers script args, witness layout, Molecule layout, ScriptGroup and
`outputs` / `outputs_data` positive/negative transaction fixture matrices,
accepted/rejected transactions, cycles, and script reference metadata.

### 6.4 Builder Assumption Contract and Transaction Solver (P0/P1)

Builder assumptions become a stable machine-readable contract:

```text
assumption_id
required_inputs
required_outputs
required_cell_deps
required_witness_fields
capacity_policy
fee_policy
change_policy
signature_policy
```

Tooling:

```bash
cellc explain-assumptions
cellc validate-tx --against metadata.json tx.json
cellc solve-tx
```

The solver handles cell selection, dep resolution, output planning, occupied capacity, fee/change planning, witness placement, signing manifests, and dry-run validation.

### 6.5 Deployment Governance and Audit UX (P1)

Deployment artifacts:

- code cell manifest
- dep group manifest
- version lock file
- audit hash record
- upgrade policy
- script reference registry entry

Audit tooling:

- source maps
- proof diff
- cycle profiler per invariant/check
- tx trace viewer
- HTML audit bundle

### 6.6 Standard Library Release Track (P1)

v0.16 can ship stable stdlib modules only when they are backed by compatibility fixtures and audit bundles:

- `std::sudt`
- `std::xudt`
- `std::type_id`
- `std::htlc`
- `std::cheque`
- `std::acp`

---

## 7. Delivery Cadence

The grant proposal is the authoritative schedule. This roadmap overview defines
scope, dependencies, and release gates only; it intentionally avoids separate
dates, quarters, week counts, and effort estimates.

---

## 8. For CKB Developers: What This Means For You

**Today (v0.12)**: You can write safe CKB contracts in CellScript right now. The compiler prevents double-spend at compile time, records capacity evidence, and generates optimized RISC-V ELF binaries. Seven example contracts cover token, AMM, vesting, timelock, multisig, NFT, and launch patterns.

**Stable (v0.13)**: Bounded value-vector helpers make whitelists, fixed membership sets, simple registries, and AMM helper code easier to write while keeping performance claims tied to release evidence. Proof-backed maps and order books stay explicit future work instead of being hidden inside generic collection syntax.

**Implemented (v0.14)**: CellScript covers CKB's execution surface. Spawn/IPC enables bounded verifier reuse and delegated checks within explicit lock/type boundaries; it is not a promise of multi-tenant type-script composition. WitnessArgs, Source views, ScriptGroup, outputs_data binding, TYPE_ID metadata validation, script references, Capacity, and time constraints are explicit and testable.

**Current (v0.15)**: CellScript is a semantic auditing layer for CKB transaction invariants. It does not hide lock/type differences; it shows when each invariant runs, what it reads, which cells it protects, which obligations are checked on-chain, which declared invariants match checked action obligations, and which are builder assumptions. Cell identity is now a first-class primitive; `create_unique`/`replace_unique` carry identity through the full compile pipeline. Destruction policies make it explicit whether you're proving output absence, identity continuation, or quantity delta. The capability vocabulary has been reset from protocol verbs (`transfer`, `destroy`) to kernel effects (`create`, `consume`, `replace`, `burn`, `relock`). A compat/strict migration path lets existing code compile with `--primitive-compat=0.14` while new code enforces v0.15 semantics with `--primitive-strict=0.15`.

**After that (v0.16)**: CellScript turns those visible semantics into production assurance. It checks ProofPlan soundness, adds a strict invariant satisfaction gate, validates transactions against builder assumptions before signing, ships CKB compatibility fixtures, and produces audit bundles that link source, proof, generated code, metadata, and cycles.

---

## 9. Appendix: CKB Concept Mapping

| CKB Concept | CellScript Primitive | Since |
|-------------|---------------------|-------|
| Cell (UTXO) | `resource` / `shared` / `receipt` | v0.12 |
| Lock Script | `lock { ... }` block | v0.12 |
| Type Script | implicit via `action` | v0.12 |
| CellInput (consume) | `consume expr` | v0.12 |
| CellOutput (create) | `create output = T { ... } with_lock(addr)` | v0.12 |
| CellDep (read) | `read param: T` | v0.13 |
| Witness | Entry Witness ABI (CSARGv1) | v0.12 |
| OutPoint | Implicit via `consume` (input) / `create` (output) | v0.12 |
| Capacity (Shannon) | `occupied_capacity(T)` + freed capacity | v0.12 |
| WitnessArgs | `witness::lock<T>()` / `witness::input_type<T>()` | v0.14 |
| `@capacity_floor` | `@capacity_floor(shannons)` annotation | v0.14 |
| Since (timelock) | `require_maturity` / `require_time` | v0.14 |
| ScriptGroup | explicit group metadata + Source views | v0.14 |
| outputs_data | output-data index binding obligations | v0.14 |
| TYPE_ID metadata | CKB TYPE_ID create/continue validation MVP | v0.14 |
| Spawn | `spawn("verifier", args: [...])` | v0.14 |
| hash_type | `hash_type(Data1)` / `with_default_hash_type(Data1)` DSL | v0.13 |
| code_hash | script reference metadata with `code_hash + hash_type + args` | v0.14 |
| Scoped invariant | `invariant { trigger; scope; reads; assert ... }` | v0.15 |
| Lock covenant | `trigger: lock_group`, explicit reads and coverage diagnostics | v0.15 |
| Type invariant | `trigger: type_group`, group-scoped invariants | v0.15 |
| ProofPlan | `cellc explain-proof` trigger/scope/reads/coverage report | v0.15 |
| Invariant action coverage | matched/unmatched declared invariant coverage in ProofPlan and audit docs | v0.15 |
| Builder assumption metadata | ProofPlan `builder_assumptions` marked as not on-chain checked | v0.15 |
| TypeID lifecycle | `identity ckb_type_id`, `create_unique`, `replace_unique`, `destroy_unique` | v0.15 |
| Destruction policy | `destroy_singleton_type`, `destroy_instance`, `burn_amount`, `DestructionPolicy` | v0.15 |
| Capability reset | `has create/consume/replace/burn/relock` (kernel effects); strict/compat modes | v0.15 |
| Identity policy | `IdentityPolicy` (`none`, `ckb_type_id`, `field`, `script_args`, `singleton_type`) | v0.15 |
| Formal semantics | operational semantics spec + conformance fixtures | v0.16 |
| Proof soundness | ProofPlan-to-code coverage checker | v0.16 |
| Invariant satisfaction gate | strict coverage/completeness check for declared invariants | v0.16 |
| Standard compatibility | CKB standard script fixture suites | v0.16 |
| Transaction solving | `cellc solve-tx` / `cellc validate-tx` | v0.16 |
| Deployment governance | deploy plan, dep locks, proof diff, audit bundle | v0.16 |

---

*Document End.*
*Status: Living Document*

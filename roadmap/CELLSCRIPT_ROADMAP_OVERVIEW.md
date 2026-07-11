# CellScript Roadmap: v0.12 -> v0.22
> From Production Foundation to Protocol Builders

**Updated**: 2026-07-04
**Status**: Living Document
**Audience**: CKB Smart Contract Developers
**Canonical folder**: `roadmap/`

This roadmap keeps the current release status from the newer high-level
planning document while preserving the detail level of the older roadmap
overview. The version-specific files in this folder remain the detailed design
records for each release.

---

## 1. Current Direction

CellScript's mission is to make the power of CKB's Cell model — linear
resources, capacity gating, script-based verification, explicit witness data,
and builder-visible transaction obligations — accessible through compile-time
type safety.

The current project direction is:

1. keep the CKB Cell model visible in the language;
2. keep release claims tied to compiler evidence and builder-backed CKB
   evidence;
3. make the language surface easier to teach without hiding authorization,
   capacity, witness, lock-group, or type-group boundaries.

Each release answers a specific question:

- **v0.12** — *Can we use it?* Prove CellScript can compile production-grade CKB
  contracts.
- **v0.13** — *Is it good to use?* Make contracts smaller, faster, and the CLI
  friendlier without hiding ownership semantics.
- **v0.14** — *Is the CKB surface complete?* Cover Spawn as bounded verifier
  composition, structured WitnessArgs, Source views, ScriptGroup metadata,
  outputs_data binding, TYPE_ID metadata validation, script references,
  capacity policy, and time constraints.
- **v0.15** — *Is the safety boundary auditable?* Model scoped invariants,
  covenant triggers, coverage, builder assumptions, identity lifecycle, and
  ProofPlan output without hiding lock/type semantics.
- **v0.16** — *Can we audit the assumptions before production evidence?* Add
  metadata semantics, ProofPlan soundness checks, descriptive standard CKB
  compatibility suites, transaction templates, deployment governance, and
  audit tooling.
- **v0.17** — *Can iCKB-grade CKB protocol semantics execute?* Close the
  scoped protocol-semantics milestone with partial CKB VM differential evidence
  and a fail-closed equivalence gate.
- **v0.18** — *Can protocol equivalence be closed honestly?* Add the
  first-class read-only ScriptRef / ScriptArgs surface and remaining iCKB
  equivalence prerequisites.
- **v0.19** — *Can compiler artifacts be resolved and verified reproducibly?*
  Close CKB ecosystem reuse, `ckb-std` compatibility, grammar governance, and
  Phase 1 package/deployment identity registry support.
- **v0.20** — *Can verified artifacts build real transactions?* Ship generated
  Action Builder support, live-chain deployment verification, stateful flows,
  and registry trust hardening.
- **v0.21** — *Can declared protocol semantics become executable and
  authenticated?* Close aggregate invariant lowering, flow-edge validation,
  authenticated compiler evidence, CLI UX reorganisation, dedicated MCP server
  and CellScript programming skills, derived protocol graphs, and audit-visible
  template layout without actorising the core.
- **v0.22** — *Can theory-guided protocol law become readable and
  evidence-tiered?* Add callable effect signatures, terminal flow metadata,
  typed transaction-view handles, bounded source-view quantifiers, bounded
  cell-collection design, type validity blocks, explicit borrow regions,
  capability algebra diagnostics, concrete payload ADTs, and ProtocolGraph
  role UX while keeping the action core intact.

---

## 2. Current State

| Area | Current status | Detailed document |
|---|---|---|
| v0.12 release scope | Released production foundation. | Historical release evidence and bundled examples |
| v0.13 release scope | Implementation scope is closed for the `v0.13.2` stable release; the full gate includes stateful business-flow/action coverage. | [v0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md), [v0.13 release tracker](CELLSCRIPT_0_13_TODOLIST.md), [v0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md) |
| v0.14 release scope | CKB semantic-completeness scope is complete for the current stable line. | [v0.14 roadmap](CELLSCRIPT_0_14_ROADMAP.md), [v0.14 release notes](../docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md) |
| v0.15 release scope | `v0.15.0` is released from `nightly-0.15` with scoped invariants, aggregate invariant primitives, Covenant ProofPlan output, risk diagnostics, macro provenance, identity-aware lifecycle forms, and final release-gate evidence. | [v0.15 roadmap](CELLSCRIPT_0_15_ROADMAP.md), [v0.15 release notes](../docs/releases/CELLSCRIPT_0_15_RELEASE_NOTES.md) |
| v0.16 release scope | Released as `v0.16.1` for the scoped metadata/tooling line. Adds operational semantics, ProofPlan soundness, builder assumptions, schema-bound transaction validation, solver templates, deployment governance, audit tooling, descriptive standard CKB compatibility fixtures, compiler hardening, proposal-local NovaSeal devnet/profile certification, and bundled example bootstrap cleanup. | [v0.16 roadmap](CELLSCRIPT_0_16_ROADMAP.md), [v0.16.1 release notes](../docs/releases/CELLSCRIPT_0_16_1_RELEASE_NOTES.md) |
| v0.17/0.18 iCKB equivalence state | The standalone v0.17 branch is a partial protocol-equivalence milestone; the carried-forward v0.18 work closes the manifest-declared executable iCKB claim set as `EXECUTED_CKB_VM_DIFF` / `PROVEN`. | [v0.17 roadmap](../docs/archive/0.17/CELLSCRIPT_0_17_ROADMAP.md), [iCKB final report](../docs/archive/0.17/CELLSCRIPT_0_17_ICKB_FINAL_REPORT.md) |
| v0.18 planning scope | First-class read-only ScriptRef / ScriptArgs API and iCKB equivalence-closure prerequisites. | [v0.18 roadmap](../docs/archive/0.18/CELLSCRIPT_0_18_ROADMAP.md) |
| v0.19 scope | Scope complete for CKB ecosystem reuse, `ckb-std` compatibility, grammar governance, and Phase 1 package/deployment identity registry closure. | [v0.19 roadmap](../docs/archive/0.19/CELLSCRIPT_0_19_ROADMAP.md), [v0.16-0.20 release notes](../docs/releases/CELLSCRIPT_0_16_TO_0_20_RELEASE_NOTES.md) |
| v0.20 planned scope | Generated Action Builder, live-chain deployment verification, stateful transaction flows, and registry trust hardening. | [v0.20 roadmap](../docs/archive/0.20/CELLSCRIPT_0_20_ROADMAP.md) |
| v0.21 planned scope | Semantic closure, authenticated compiler evidence, CLI UX reorganisation, dedicated MCP server and CellScript programming skills, derived cyclic ProtocolGraph views, type-level TemplateLayout metadata, and deferred optional template Merkleisation. | [v0.21 roadmap](../docs/CELLSCRIPT_0_21_ROADMAP.md), [v0.21 CLI UX plan](CELLSCRIPT_0_21_CLI_UX_PLAN.md) |
| v0.22 draft scope | Draft type-theory and set-theory guided language hardening proposal. This scope requires pre-talk soundness fixes and Nervos Talk Discussion before adoption: callable effects for ordinary functions, terminal flow metadata, typed transaction-view handles, finite source-view quantifiers, bounded cell-collection design, type validity blocks, explicit borrow regions, capability algebra explanations, concrete payload ADTs, and ProtocolGraph role UX. | [v0.22 type and set theory roadmap draft](CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md) |
| CKB language fit | CKB-first design is confirmed; remaining hardening areas are signer binding, continuity policy, capacity policy, and declarative time policy. | [CKB target profiles](../docs/wiki/Tutorial-05-CKB-Target-Profiles.md), [production gates](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md) |
| Surface syntax | Low-risk syntax pass is implemented; authority-sensitive syntax remains staged. | [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md) |
| Collections | Stack-backed fixed-width `Vec<T>` helper surface is implemented; cell-backed and generic map ownership remain fail-closed. | [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md), [v0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md) |
| CKB production evidence | Bundled actions and locks have builder-backed local CKB evidence; full release claims also require stateful coverage for every production acceptance action. | [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md) |
| Documentation and wiki | Wiki is version-neutral, cookbook-oriented, includes a standard-library chapter, and is published separately to GitHub Wiki. | [GitHub Wiki](https://github.com/a19q3/CellScript/wiki) |

---

## 3. Release Arc

| Version | Theme | One-liner | Status |
|---|---|---|---|
| v0.12 | Production Foundation | "Write real CKB contracts safely." | Released |
| v0.13 | Performance and Expressiveness | "Write less, run faster." | Stable scope closed |
| v0.14 | CKB Semantic Completeness | "Expose CKB surface and bounded verifier reuse." | Complete for the stable line |
| v0.15 | Scoped Invariants and Covenant ProofPlan | "Show when constraints run, what they read, and who they protect." | Released from `nightly-0.15` |
| v0.16 | Metadata Assurance and Production Tooling Skeleton | "Make assumptions explicit and auditable." | Released as `v0.16.1` |
| v0.17 | iCKB-Grade Protocol Semantics | "Turn protocol-equivalence gaps into executable evidence gates." | Standalone milestone partial; selected claim set proven on the carried-forward 0.18+ line |
| v0.18 | First-Class Script API and Equivalence Closure | "Make ScriptRef/ScriptArgs and remaining iCKB proof prerequisites first-class." | Planning |
| v0.19 | Package Registry Phase 1 and Adapter Boundary | "Resolve and verify package/build/deployment identity before builders consume it." | Scope complete |
| v0.20 | Generated Builder and Live Registry Proof | "Turn verified artifacts into valid transactions through registry-bound builders." | In progress: generated TypeScript builders, live registry verification, VS Code commands, and generated-builder tooling-gate checks are active. |
| v0.21 | Semantic Closure and Authenticated Evidence | "Make declared protocol law executable and tamper-evident without changing the action core." | Implementation checkpoint: RC cut 2026-07-01 as 0.21.0-rc.1; aggregate lowering, flow-edge validation, compile receipts, nested CLI, MCP server + 6 skills, ProtocolGraph view, and TemplateLayout metadata are active; v0.21.0 tag pending. |
| v0.22 | Theory-Guided Protocol Law | "Make protocol law readable, finite, effect-aware, and evidence-tiered." | Draft: requires pre-talk soundness fixes and Nervos Talk Discussion before adoption; proposed scope covers callable effects, terminal flow metadata, typed transaction-view handles, bounded quantifiers, bounded cell collections, validity blocks, borrow regions, capability algebra, payload ADTs, and ProtocolGraph role UX. |

The roadmap is intentionally cumulative. Later releases should not re-open an
earlier feature boundary unless the prior boundary was proven unsafe or
misleading.

---

## 4. v0.12 — Production Foundation

**What it delivered**: A production-ready compiler path that turns CellScript
source into optimized RISC-V ELF binaries for CKB VM, with compile-time safety
guarantees aimed at CKB's Cell model.

### 4.1 Linear Type System for Cell Safety

CellScript models Cells with three core type classes:

| Type class | CKB mapping | Capabilities |
|---|---|---|
| `resource` | Consumed Cell / `CellInput` | `has store, transfer, destroy` |
| `shared` | Reference Cell / `CellDep` | read-only, no consumption |
| `receipt` | Proof or witness artifact | one-time claim |

Compile-time safety guarantees:

- double-spend prevention through linear state tracking;
- branch consistency, where both sides of an `if` must leave resources in the
  same state;
- capability gating, where destructive or transfer operations require explicit
  type capability;
- fail-closed behavior for ownership surfaces the compiler cannot prove.

### 4.2 Cell Effect Operations

```cellscript
consume token                                       // consume Cell input
create Token { amount: 100 } with_lock(recipient)   // create Cell output
std::lifecycle::transfer(token, next_token, recipient) { amount } // consume + create
destroy token                                       // destroy when capability allows it
read_ref OracleData                                 // read CellDep without consuming it
mutate pool { reserve_a: pool.reserve_a + delta }   // replacement-style update
```

These operations are intentionally CKB-shaped. They are not modeled as generic
heap mutation, account storage mutation, or implicit object ownership.

### 4.3 Entry Witness ABI

v0.12 established the CellScript entry witness ABI:

- structured parameter passing through witness bytes;
- serialization of scalars, fixed bytes, and schema-backed dynamic data;
- compatibility with bundled examples and production action reports;
- a clear boundary that structured CKB `WitnessArgs` field access is v0.14
  scope, not v0.12 scope.

### 4.4 CKB Syscall Integration

The production foundation covered low-level CKB VM interaction:

- `load_cell`, `load_header`, `load_witness`, `load_cell_data`, and related
  syscall paths;
- standard lock-script verification patterns such as secp256k1 signature
  verification;
- absolute and relative timelock patterns for block height and timestamp;
- dep-cell reads through explicit `read_ref` surfaces.

### 4.5 Production Evidence

v0.12 production claims are tied to evidence rather than marketing scope:

- **43/43** production actions compiled and accepted;
- **7 bundled example contracts**: token, AMM pool, vesting, timelock,
  multisig, NFT, and launch patterns;
- occupied-capacity evidence recorded per action;
- acceptance and malformed cases documented as compiler and builder evidence,
  not as blanket mainnet-deployment guarantees.

---

## 5. v0.13 — Performance and Expressiveness

**Status**: Beta released; implementation scope closed.

**Theme**: Write less code and generate faster contracts while keeping
ownership semantics explicit.

v0.13 focuses on three themes:

- executable stack-backed `Vec<T>` helper support for fixed-width values;
- low-risk source-surface improvements and cleaner example organization;
- CKB lock-boundary classification with `protected`, `witness`, and
  lock-only `require`.

v0.13 deliberately does not introduce hidden signer authority, hidden sighash
defaults, full generic maps, or cell-backed collection ownership.

### 5.1 Bounded Value-Vector Helpers

v0.13 adds checked local helper support for stack-backed value vectors:

- `Vec::new`
- `Vec::with_capacity`
- `capacity`
- `push`
- `extend_from_slice`
- `len`
- `first`
- `last`
- indexing
- `set`
- `remove`
- `pop`
- `insert`
- `reverse`
- `truncate`
- `swap`
- `clear`
- `is_empty`
- `contains`

Supported value categories:

| Source / element category | Read helpers | Local mutation helpers | Removal / reorder helpers | Status |
|---|---:|---:|---:|---|
| Stack-backed `Vec<u64>` | yes | yes | yes | implemented and tested |
| Stack-backed fixed bytes / `Address` / `Hash` | yes | yes | yes | implemented and tested |
| Stack-backed fixed-width schema values | yes | yes | yes | implemented where fixed-width layout is known |
| Molecule dynamic fields / entry-witness vectors | read-oriented paths | no local mutation helpers | no local mutation helpers | v0.12 foundation, not new v0.13 runtime scope |
| Cell-backed / linear vectors | no | no | no | fail-closed until ownership proof exists |

The key boundary is:

```cellscript
Vec<T: FixedWidth>
```

This is not a source-level unconstrained generic system. It is a bounded,
inspectable helper layer for values whose ABI width and ownership behavior are
known.

Metadata and `cellc explain-generics` expose each checked instantiation with:

- concrete element type;
- element width;
- fixed backing capacity;
- backing model;
- helper set;
- scope;
- accepted or rejected status.

### 5.2 Collections Non-Goals

The following remain unsupported or fail-closed:

```cellscript
Vec<Cell<T>>
Vec<Linear<T>>
HashMap<Hash, Token<T>>
HashMap<K, V>
T: CellBacked
T: Linear
```

These are deferred because CKB Cell ownership is not a generic heap model.
Before supporting cell-backed collections, the compiler needs executable
ownership proof, membership proof, consume-each semantics, builder validation,
and inspectable witness/schema commitments.

### 5.3 Zero-Cost Abstractions

v0.13 implements conservative optimizer work on the checked compiler path:

| Optimization | Status | Boundary |
|---|---|---|
| Deserialization specialization | implemented | field access uses known type/schema layout offsets |
| Function inlining | implemented | pure helper functions only; no stateful cell/resource effects |
| Dead code elimination | implemented | unused pure helper functions and pure immutable locals |
| Constant propagation | implemented | top-level constants, immutable local constants, literal arithmetic/boolean/string/bytes comparisons, branch pruning |
| Loop unrolling | implemented | fixed-size array foreach paths only |

Expected gains remain benchmark-dependent. Release claims should be tied to
measured ELF size, instruction count, cycles, and acceptance reports rather than
generic percentage promises.

### 5.4 CLI and Source-Surface Improvements

Implemented CLI and surface work includes:

- `cellc new` with Cargo-compatible workflow;
- `cellc init` compatibility preservation;
- `cellc build` using O1 as the default development optimization level;
- `--vcs git|none`;
- `--lib` package creation using `src/lib.cell` correctly;
- CLI diagnostic codes backed by the runtime error registry where applicable;
- `cellc explain <error-code>`;
- field shorthand lowering such as `field` -> `field: field`;
- contextual `Vec<T>` literals for local stack vectors;
- clearer example organization by audience.

Full rustc-style source-span rendering remains future polish.

### 5.5 Lock Boundary Surface

v0.13 makes the lock-boundary data source more explicit:

- `protected` classifies protected inputs;
- `witness` identifies witness-originating data;
- lock-only `require` records constraints that belong to lock verification;
- `lock_args` spelling is reserved but rejected until typed script-args binding
  is implemented.

Deferred authorization work:

- explicit sighash verification primitive;
- digest mode selection;
- script-group scope selection;
- witness layout and replay assumptions;
- first-class verified signer values;
- optional `protects T { self ... }` sugar only after binding semantics are
  exact.

### 5.6 v0.13 Release Gates

v0.13 should be considered closed only when:

- bounded collection helper coverage is tested;
- metadata exposes checked `Vec<T>` instantiations;
- unsupported generic and cell-backed patterns fail closed;
- optimizer passes do not rewrite resource effects unsafely;
- CLI changes are covered by tests;
- top-level `examples/*.cell` remain the single checked-in bundled business
  source, while `examples/language/*.cell` covers compiler/tooling examples and
  `tests/benchmarks/ickb_specs/*.cell` covers the iCKB-inspired benchmark
  surface;
- release notes distinguish v0.12 foundations from genuine v0.13 work.

---

## 6. v0.14 — CKB Semantic Completeness

**Status**: Feature-complete beta scope in the implementation branch.

**Theme**: Expose CKB's concrete execution surface before redesigning
higher-level primitives.

v0.14 exposes more of CKB without hiding lock/type boundaries:

- Spawn/IPC builtins for bounded verifier reuse;
- explicit Source views;
- typed fixed-width lock args;
- structured WitnessArgs field access;
- target profile metadata for CKB ABI contracts;
- declarative since/time and capacity surfaces;
- fixed-Hash dynamic BLAKE2b via `hash_blake2b(input: Hash) -> Hash`, backed by
  a real CKB-profile RISC-V helper and metadata-visible `CKB_BLAKE2B` access.

### 6.1 Implemented Branch Scope

| Track | Status | Notes |
|---|---|---|
| Spawn/IPC surface | implemented | `spawn`, `wait`, `process_id`, `pipe`, `pipe_write`, `pipe_read`, `inherited_fd`, and `close` lower to CKB VM v2 syscall stubs and metadata |
| Spawn/IPC fd safety | implemented | type checker rejects statically visible use-after-close, double-close, and unclosed fd paths |
| Source views | implemented | `source::input`, `source::output`, `source::cell_dep`, `source::header_dep`, `source::group_input`, and `source::group_output` are typed and metadata-visible |
| ScriptGroup metadata | implemented | CKB actions and locks expose entry kind, active lock/type group kind, selected Source surfaces, and group-scoped Source usage |
| outputs/outputs_data binding | implemented | each CKB create output records index-aligned output/data binding; metadata validation rejects missing or mismatched bindings |
| Structured witness fields | implemented | `witness::raw`, `witness::lock`, `witness::input_type`, and `witness::output_type` are typed CKB witness surfaces |
| Lock args source | implemented | lock parameters can declare `lock_args` for fixed-width typed CKB `Script.args` data |
| Sighash surface | implemented | `env::sighash_all(source)` is explicit and metadata-visible; no hidden signer derivation is introduced |
| Target profile contract | implemented | metadata records witness ABI, lock args ABI, Source encoding, Spawn/IPC ABI, since ABI, CellDep ABI, script reference ABI, outputs/data ABI, capacity floor ABI, TYPE_ID ABI, and tx version |
| Script reference table | implemented | aggregates TYPE_ID script references, spawn CellDep/DepGroup targets, and read-ref CellDep references |
| Declarative since/time surface | implemented | `require_maturity`, `require_time`, `require_epoch_after`, and `require_epoch_relative` are profile-visible runtime checks |
| Declarative capacity surface | implemented | `with_capacity_floor(shannons)` declares a type-level CKB output floor; `occupied_capacity("TypeName")` remains available for runtime-visible capacity evidence |
| Dynamic BLAKE2b policy | fail-closed | `hash_blake2b` is rejected until a real linked RISC-V implementation is selected; `hash_chain` is metadata-visible |
| Examples | implemented | examples cover delegate verification, Spawn/IPC pipelines, witness/source views, TYPE_ID creation, capacity/time policy, and canonical style |

### 6.2 Spawn/IPC Bounded Verifier Composition

Spawn/IPC enables bounded verifier reuse, delegated checks, and modular
validation pipelines. It does not make a CKB cell's `type script` slot
multi-tenant.

```cellscript
action verify_with_delegate(proof: Proof) {
    let result = spawn("secp256k1_verifier", args: [proof.pubkey, proof.signature])
    assert_invariant(result == 0, "verification failed")
}

action multi_step_verify(data: VerifyData) {
    let (read_fd, write_fd) = pipe()
    let pid = spawn("hash_checker", fds: [read_fd])
    pipe_write(write_fd, data.payload)
    let result = wait(pid)
    assert_invariant(result == 0, "hash check failed")
}
```

Compiler-owned obligations:

- spawn targets must resolve to known scripts;
- CellDep/DepGroup requirements are emitted as runtime-required obligations;
- fd lifetime is checked where statically visible;
- cycle-budget assumptions remain explicit builder/runtime concerns;
- profile metadata records Spawn/IPC ABI and syscall availability.

### 6.3 Structured WitnessArgs and Source Views

v0.14 makes CKB witness and Source selection explicit:

```cellscript
lock standard_lock(pubkey_hash: Hash160) -> bool {
    let sig = witness::lock<RecoverableSignature>(source: source::group_input(0))
    let sighash = env::sighash_all(source: source::group_input(0))
    return secp256k1_verify(pubkey_hash, sig, sighash)
}

action prove_type_transition(state: &mut State) {
    let proof = witness::input_type<TransitionProof>(source: source::group_input(0))
    assert_invariant(verify_transition(proof, state), "bad transition proof")
}
```

Supported surfaces:

- `source::input(n)`
- `source::output(n)`
- `source::cell_dep(n)`
- `source::header_dep(n)`
- `source::group_input(n)`
- `source::group_output(n)`
- `witness::raw<T>`
- `witness::lock<T>`
- `witness::input_type<T>`
- `witness::output_type<T>`

The CKB profile owns CKB-specific Source encoding. Portable profiles must not
silently emulate CKB group semantics.

### 6.4 Target Profile Contract

v0.14 formalizes profile-specific behavior that previously existed implicitly.

| Feature | CKB profile | Portable profile |
|---|---|---|
| Hash function | CKB BLAKE2B policy | profile-declared |
| Time reference | block number, timestamp, epoch | abstract or unavailable |
| Since metric | CKB `since` encoding | unavailable unless explicitly modeled |
| Witness structure | Molecule `WitnessArgs` plus raw fallback | explicit raw/entry ABI |
| Source encoding | CKB strict global/group encoding | profile-owned |
| Spawn/IPC | available for VM v2-compatible targets | unavailable |
| Tx version | CKB transaction version contract | not applicable |

`cellc explain-profile ckb` reports the same contract that metadata validation
expects.

### 6.5 CKB Transaction Shape and ScriptGroup Conformance

The compiler and metadata layer must match CKB's concrete transaction model:

- lock groups are formed from input lock scripts;
- type groups are formed from input and output type scripts;
- `source::group_input(n)` and `source::group_output(n)` are relative to the
  active script group;
- every `outputs_data[i]` belongs to `outputs[i]`;
- TYPE_ID creation rules depend on first input, output index, and group
  cardinality.

v0.14 metadata and tests cover:

- entry kind;
- active lock/type group kind;
- selected Source surfaces;
- output-data index obligations;
- duplicate or missing TYPE_ID plan rejection;
- positive and negative fixture transactions for strict CKB profile behavior.

### 6.6 Declarative Capacity and Time Policy

Capacity policy:

```cellscript
resource Token has store, transfer, destroy
with_capacity_floor(6_100_000_000) {
    amount: u64
    symbol: [u8; 8]
}
```

Time policy:

```cellscript
action claim_after_ckb_timeout(htlc: HtlcReceipt) {
    require_maturity(blocks: 100)
    require_time(after: Timestamp(target))
    require_epoch_relative(number: 10, index: 0, length: 1)
    claim htlc
}
```

The compiler records these requirements, but builders still have to measure
occupied capacity, transaction size, fees, change, header availability, and
actual since/header satisfaction.

### 6.7 Script References and HashType Strictness

v0.14 script references are precise CKB artifacts, not loose names:

- `code_hash`;
- `hash_type`;
- `args`;
- dep source;
- resolved target profile;
- CellDep or DepGroup linkage.

Every script reference used by spawn, lock/type metadata, or `read_ref` must
have a resolvable dep path. Audit output includes the script reference table.

### 6.8 v0.14 Non-Goals

v0.14 does not:

- redefine the primitive kernel;
- move protocol verbs out of compiler core;
- split `Address`, `LockScript`, and `LockHash` in the type system;
- redesign destruction policy;
- claim formal verification;
- introduce full generic `HashMap<K, V>`;
- claim dynamic on-chain BLAKE2b without a real linked RISC-V implementation.

Those belong to v0.15, v0.16, or later tracks.

---

## 7. v0.15 — Scoped Invariants and Covenant ProofPlan

**Status**: Implemented P0; P1 partial.

**Theme**: Make CKB safety boundaries explicit instead of hiding lock/type
differences.

v0.15 makes invariant scope and enforcement status visible without pretending
that metadata-only declarations are already executable CKB verifier code.

Key facts every invariant should expose:

- `trigger`: when the verifier runs;
- `scope`: which cell universe the invariant reasons over;
- `reads`: which transaction views are inspected;
- `coverage`: which cells are actually protected;
- `on_chain_checked`: which obligations are enforced by generated code;
- `builder_assumption`: which obligations are only construction/deployment
  assumptions.

### 7.1 Implemented P0/P1 Scope

Implemented:

- first-class script semantics;
- scoped invariant syntax;
- aggregate invariant primitives as metadata-only;
- Covenant ProofPlan metadata;
- `cellc explain-proof`;
- runtime-obligation policy gate;
- lock-group transaction risk diagnostics;
- protocol macro provenance for selected compiler-recognized flows;
- cell identity and TYPE_ID lifecycle policy;
- explicit destruction policies;
- kernel/protocol primitive split;
- capability vocabulary reset;
- internal `type_hash` renaming;
- compatibility and migration infrastructure;
- documentation and tests for the stable surface.

Still partial or future:

- Covenant helper stdlib;
- `Address`, `LockScript`, and `LockHash` type split;
- explicit CKB script role in all user-facing entry forms;
- versioned cell data layout policies;
- claim/receipt name heuristic removal;
- explicit mutation cardinality;
- full macro expansion provenance;
- moving `shared` to a scheduler policy library.

### 7.2 First-Class Script Semantics

```cellscript
invariant udt_amount_non_increase {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>, group_outputs<Token>

    assert sum(group_outputs<Token>.amount) <= sum(group_inputs<Token>.amount)
}
```

Core vocabulary:

- `trigger = lock_group | type_group | explicit_entry`
- `scope = group | transaction | selected_cells`
- `reads = input | output | group_input | group_output | cell_dep | header_dep | witness`
- `coverage = covered_cells(...)`
- `on_chain_checked = true | false`

A lock-group verifier that scans transaction-wide views is not equivalent to a
type-group conservation rule. The ProofPlan must surface that distinction.

### 7.3 Scoped Aggregate Invariant Primitives

Aggregate primitives:

```cellscript
assert_sum(group_outputs<Token>.amount)
assert_conserved(Token.amount, scope = group)
assert_delta(Token.amount, delta, scope = selected_cells)
assert_distinct(outputs<NFT>.id, scope = transaction)
assert_singleton(type_id, scope = group)
```

Rules:

- every aggregate invariant binds an explicit scope;
- fields must resolve to fixed-width integer or fixed-byte schema fields;
- overflow and malformed cell data fail closed;
- metadata-only aggregate claims must not be reported as executable CKB checks.

### 7.4 Covenant ProofPlan

`cellc explain-proof` is the audit surface for invariant reasoning:

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

ProofPlan records include:

- invariant name and source span;
- trigger, scope, reads, and coverage;
- input/output relation checks;
- group cardinality;
- identity lifecycle policy;
- builder assumptions;
- diagnostics;
- codegen coverage status.

### 7.5 Kernel/Protocol Primitive Split

v0.15 resets vocabulary from protocol verbs to kernel effects.

Legacy protocol verbs:

- `transfer`
- `destroy`
- `claim`
- `settle`
- selected pool/AMM flows

Kernel effects:

- `create`
- `consume`
- `replace`
- `burn`
- `relock`
- `retarget_type`
- `read_ref`

Compatibility mode accepts legacy vocabulary:

```bash
cellc check --primitive-compat=0.14
```

Strict mode rejects protocol verbs where kernel effects and scoped invariants
are required:

```bash
cellc check --primitive-strict=0.15
```

### 7.6 Identity and Destruction Policies

v0.15 promotes identity from ad hoc metadata to a first-class primitive policy:

- `identity(none)`
- `identity(ckb_type_id)`
- `identity(field(path))`
- `identity(script_args)`
- `identity(singleton_type)`

Lifecycle forms:

- `create_unique<T>(identity = ...)`
- `replace_unique<T>(identity = ...)`
- `destroy_unique(cell, identity = type_id)`
- `destroy_instance(cell, identity_field = id)`
- `destroy_singleton_type(cell)`
- `burn_amount(cell, field = amount)`

The TYPE_ID path has executable boundaries. Other identity policies may remain
declarative metadata until executable verifier semantics are added.

### 7.7 Compatibility and Migration

v0.15 migration keeps old code buildable while making new semantics explicit:

- `--primitive-compat=0.14`;
- `--primitive-strict=0.15`;
- CS0151-CS0160 migration diagnostics;
- ProofPlan diagnostics for metadata-only obligations;
- source and metadata renames for `ckb_type_script_hash` and
  `ckb_lock_script_hash`;
- examples in both compatibility and strict migration tracks.

### 7.8 v0.15 Release Gates

v0.15 cannot ship until:

- every invariant records trigger, scope, reads, coverage, and enforcement
  status;
- `lock_group + transaction` covenant patterns produce coverage diagnostics;
- strict CKB mode has zero protocol-verb codegen recognizers;
- protocol verbs lower through stdlib proof macros and scoped invariants where
  still supported;
- every protocol macro has expansion provenance;
- `cellc explain-proof` exposes trigger/scope/reads/coverage/on-chain status;
- every CKB artifact has an explicit entry role;
- `Address`, `LockScript`, and `LockHash` are distinct in type checking and
  metadata;
- TYPE_ID lifecycle is covered by ProofPlan and runtime codegen;
- bare `destroy` is removed or compatibility-gated;
- resource capabilities use kernel effect names in strict mode;
- schema-backed replacement declares preserve or migrate layout policy;
- ProofPlan coverage is checked in tests;
- examples pass in compatibility and strict migration tracks.

---

## 8. v0.16 — Metadata Assurance and Production Tooling Skeleton

**Status**: Implemented for scoped release.

**Theme**: Turn the v0.15 semantic audit layer into metadata assurance and
deterministic local tooling. Production-completeness work moves to v0.17.

v0.16 does not re-open v0.13 bounded collections, v0.14 CKB surface exposure,
or v0.15 invariant syntax. It validates and operationalizes those models at
the compiler metadata/tooling layer.

The v0.15 hardening backlog is still tracked, but 0.16 keeps the claim scoped:

- aggregate invariant executable lowering remains a 0.17 production track;
- ProofPlan soundness is a metadata consistency checker, not a formal proof;
- protocol macro provenance is visible, but full macro-only lowering is later;
- covenant stdlib modules are schema stubs, not stable implementations;
- `Address` / `LockScript` / `LockHash`, explicit entry-role annotations,
  versioned layout policies, non-TYPE-ID global uniqueness certification, and
  full `cellc explain-macro` source maps remain explicit follow-up tracks.

### 8.1 Formal Operational Semantics

Publish a mechanically precise semantics for:

- expression evaluation;
- linear resource state transitions;
- branch merge rules;
- cell input/output/ref effects;
- lock/type trigger execution;
- group and transaction scopes;
- ProofPlan obligation coverage;
- builder assumption boundaries.

Expected artifacts:

- operational semantics document;
- conformance fixtures linked to compiler tests;
- examples that connect source constructs to semantics rules.

### 8.2 ProofPlan Soundness Checks

Strict mode validates the metadata chain:

```text
source/runtime obligation
  -> ProofPlan obligation
  -> metadata coverage record
  -> builder assumption boundary
```

Rejected cases:

- metadata-only/runtime-required gaps in strict mode;
- local/runtime ProofPlan record drift;
- mismatched trigger, scope, reads, coverage, assumptions, detail, or codegen
  coverage;
- unchecked builder assumptions;
- checked records whose codegen coverage metadata is not `covered`.

### 8.3 Standard CKB Contract Compatibility Suite

Descriptive compatibility fixtures cover:

- sUDT / xUDT;
- ACP;
- Cheque;
- Omnilock-compatible lock patterns;
- NervosDAO-style epoch/since cases;
- Type ID.

Each suite should include:

- script args;
- witness layout;
- Molecule layout;
- ScriptGroup and `outputs` / `outputs_data` positive and negative matrices;
- accepted transaction shapes;
- rejected transaction shapes;
- cycle report envelopes;
- script reference metadata;
- capacity and transaction-size evidence where relevant.

### 8.4 Builder Assumption Contract

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

For manifest-bound spawn targets, `required_cell_deps` carries the concrete
CellDep slot and manifest identity. `validate-tx` checks both the transaction
`cell_deps[index]` object and matching `builder_assumption_evidence`.

Tooling:

```bash
cellc explain-assumptions
cellc validate-tx --against metadata.json tx.json
```

`validate-tx` checks transaction shape and schema-bound builder evidence before
signing or submission. It is not full CKB transaction semantic validation.

### 8.5 Transaction Template Emitter

`cellc solve-tx` emits a deterministic template from compiler metadata:

- required input/output/dep/header/witness slots;
- builder assumption evidence requirements;
- fee/change metadata that builders must satisfy;
- signing manifest skeleton;
- explicit limitations.

It is not a final solver. Live cell selection, concrete deps, fee/change,
witness placement, signing, and dry-run move to v0.17.

### 8.6 Deployment Governance and Audit UX

Local deployment artifacts:

- code cell manifest;
- dep group manifest;
- version lock file;
- audit hash record;
- local upgrade diff;
- script reference metadata.

Audit tooling:

- metadata/IR-level source-to-codegen mapping;
- field-level proof diff;
- cycle profiler per invariant/check;
- transaction assumption trace viewer;
- HTML audit bundle linking source, ProofPlan, metadata, IR effect classes, and
  codegen coverage.

### 8.7 Standard Library Schema Track

v0.16 ships schema stubs, not stable protocol stdlib implementations:

- `std::sudt`;
- `std::xudt`;
- `std::type_id`;
- `std::htlc`;
- `std::cheque`;
- `std::acp`.

### 8.8 v0.16 Release Gates

v0.16 can ship when:

- operational semantics covers resource state, cell effects, triggers, scopes,
  and ProofPlan;
- ProofPlan soundness checker is mandatory in strict mode;
- standard CKB compatibility suites cover descriptive accepted and rejected
  fixture shapes, including ScriptGroup and `outputs_data` matrices;
- builder assumption schema is stable;
- `cellc validate-tx` rejects missing, bare, or malformed schema-bound builder
  evidence;
- `cellc solve-tx` emits a deterministic template with explicit limitations;
- deployment manifests are reproducible and locally verified for integrity;
- audit bundle links source, ProofPlan, metadata, IR effect classes, and codegen
  coverage;
- stdlib protocol descriptors are marked `schema-stub`, not `stable`.

---

## 9. Cross-Cutting Tracks

These tracks span release boundaries and should stay visible in roadmap updates.

### 9.1 Authorization Hardening

Authorization-sensitive syntax must become literal before it becomes ergonomic.

Planned order:

1. explicit sighash verification primitive with digest mode, script group scope,
   witness layout, and replay assumptions;
2. stable metadata and report fields for signature verification obligations;
3. first-class verified signer values only after explicit primitives are proven;
4. optional `protects T { self ... }` sugar only after protected-input
   selection and lock-group aggregation semantics are exact.

Non-goals:

- no implicit signer derivation from `Address`;
- no hidden sighash defaults;
- no parameter-name-based authority.

Source documents:

- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
- [CKB target profiles](../docs/wiki/Tutorial-05-CKB-Target-Profiles.md)

### 9.2 CKB Evidence Hardening

The CKB acceptance surface should move from broad acceptance evidence to
predicate-specific evidence.

Priorities:

- keep action acceptance builder-backed and report-validated;
- keep lock valid-spend and invalid-spend matrices mandatory for bundled locks;
- require invalid-spend cases to match stable script failure paths, not generic
  transaction rejection;
- keep cycles, serialized transaction size, occupied capacity, and malformed
  rejection evidence in reports;
- extend the matrix when new bundled locks enter production scope.

Source documents:

- [CKB target profiles](../docs/wiki/Tutorial-05-CKB-Target-Profiles.md)
- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md)

### 9.3 Collections and Ownership

The collections roadmap stays conservative because CKB Cell ownership is not a
generic heap model.

Completed:

- stack-backed fixed-width `Vec<T>` helper support;
- typed/contextual `Vec<T>` literals for local stack vectors;
- metadata and `cellc explain-generics` visibility for checked instantiations.

Deferred:

- full generic `HashMap<K, V>` and `HashSet<T>`;
- `Vec<Cell<T>>` and other cell-backed linear ownership collections;
- source-level `Option<T>` lowering;
- explicit `Vec<T, N>[...]` bounded-vector literal syntax.

Source documents:

- [v0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)
- [Linear ownership](../docs/CELLSCRIPT_LINEAR_OWNERSHIP.md)

### 9.4 Declarative CKB Policy

Some CKB facts are source-level policies; others remain metadata and builder
obligations.

Important boundaries:

- declarative capacity requirements can be checked by the compiler where type
  layout and explicit floors are known;
- builders still own concrete occupied-capacity measurement, change, fees, and
  transaction size;
- declarative since/header/timepoint assumptions require target-profile
  semantics and concrete transaction/header evidence;
- continuity policy for signature-directed input/output Cell updates must cover type id, lock, data
  schema, and capacity continuity;
- action builder plans must show obligations rather than silently satisfying
  them.

Source documents:

- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Output bindings](../docs/CELLSCRIPT_OUTPUT_BINDINGS.md)
- [CKB target profiles](../docs/wiki/Tutorial-05-CKB-Target-Profiles.md)

### 9.5 Documentation and Developer Experience

The docs should stay useful to new readers and strict enough for reviewers.

Completed:

- GitHub Wiki is version-neutral and cookbook-oriented;
- `_Sidebar.md` gives a book-like navigation structure;
- cookbook recipes and CKB glossary exist;
- LSP and VS Code grammar/snippets cover the new lock-boundary syntax.

Future work:

- keep wiki links rendered through GitHub Wiki URLs;
- add recipes when new stable language patterns land;
- keep release notes in `docs/releases/` and roadmap files in `roadmap/`,
  separate from tutorial pages;
- keep top-level `examples/*.cell` as the single bundled business source, with
  language and benchmark examples in explicit subdirectories.

Source documents:

- [GitHub Wiki](https://github.com/a19q3/CellScript/wiki)
- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)

---

## 10. Delivery Cadence

The grant proposal is the authoritative schedule. This roadmap overview defines
scope, dependencies, status, and release gates only. It intentionally avoids
separate dates, quarters, week counts, or effort estimates.

Roadmap status should be updated when implementation evidence changes, not when
aspirational scope is proposed.

---

## 11. For CKB Developers

**Today**: You can write safe CKB contracts in CellScript. The compiler prevents
linear-resource double use at compile time, records capacity evidence, and
generates optimized RISC-V ELF binaries. Bundled examples cover token, AMM,
vesting, timelock, multisig, NFT, and launch patterns.

**v0.13**: Contracts get smaller, faster, and easier to author. Bounded
value-vector helpers make whitelists, fixed membership sets, simple registries,
and AMM helper code easier to write. Proof-backed maps and order books stay
explicit future work instead of being hidden inside generic collection syntax.

**v0.14**: CellScript covers more of CKB's concrete execution surface. Spawn/IPC
enables bounded verifier reuse and delegated checks within explicit lock/type
boundaries. WitnessArgs, Source views, ScriptGroup, outputs_data binding,
TYPE_ID metadata validation, script references, capacity, and time constraints
become explicit and testable.

**v0.15**: CellScript becomes a semantic auditing layer for CKB transaction
invariants. It shows when each invariant runs, what it reads, which cells it
protects, which obligations are checked on-chain, and which are builder
assumptions. Identity and destruction policies become explicit. The capability
vocabulary moves from protocol verbs to kernel effects, with compat and strict
migration paths.

**v0.16**: CellScript turns visible semantics into scoped metadata assurance.
It checks ProofPlan soundness, validates transaction shapes against
schema-bound builder assumptions before signing, ships descriptive CKB
compatibility fixtures, and produces audit bundles that link source, proof,
metadata, IR effect classes, and codegen coverage.

---

## 12. CKB Concept Mapping

| CKB concept | CellScript primitive | Since / track |
|---|---|---|
| Cell / UTXO | `resource`, `shared`, `receipt` | v0.12 |
| Lock Script | `lock { ... }` block | v0.12 |
| Type Script | type/action metadata and later explicit role | v0.12 -> v0.15 |
| CellInput | `consume expr` | v0.12 |
| CellOutput | `create T { ... } with_lock(addr)` | v0.12 |
| CellDep | `read_ref T` | v0.12 |
| Witness | entry witness ABI / CSARGv1 | v0.12 |
| OutPoint | input/output obligations through consume/create metadata | v0.12 |
| Capacity | `occupied_capacity(T)` and capacity evidence | v0.12 |
| `hash_type` | `with_default_hash_type(...)` / metadata | v0.13 |
| Bounded value collection | stack-backed `Vec<T: FixedWidth>` helpers | v0.13 |
| WitnessArgs | `witness::lock<T>`, `witness::input_type<T>`, `witness::output_type<T>` | v0.14 |
| Source views | `source::input`, `source::output`, `source::group_input`, `source::group_output` | v0.14 |
| ScriptGroup | group metadata and Source view conformance | v0.14 |
| outputs_data | output-data index binding obligations | v0.14 |
| TYPE_ID metadata | create/continue validation MVP | v0.14 |
| Spawn | `spawn`, `wait`, `pipe`, fd helpers | v0.14 |
| Script reference | `code_hash + hash_type + args + dep source` metadata | v0.14 |
| Capacity floor | `with_capacity_floor(shannons)` | v0.14 |
| Since / timelock policy | `require_maturity`, `require_time`, `require_epoch_*` | v0.14 |
| Scoped invariant | top-level `invariant` with trigger/scope/reads | v0.15 |
| Lock covenant | `trigger: lock_group` plus coverage diagnostics | v0.15 |
| Type invariant | `trigger: type_group` and group-scoped relations | v0.15 |
| ProofPlan | `cellc explain-proof` | v0.15 |
| Builder assumption | explicit non-on-chain obligation records | v0.15 |
| Identity policy | `none`, `ckb_type_id`, `field`, `script_args`, `singleton_type` | v0.15 |
| TYPE_ID lifecycle | `create_unique`, `replace_unique`, `destroy_unique` | v0.15 |
| Destruction policy | `destroy_singleton_type`, `destroy_instance`, `burn_amount` | v0.15 |
| Capability reset | `create`, `consume`, `replace`, `burn`, `relock`, `retarget_type`, `read_ref` | v0.15 |
| Formal semantics | operational semantics spec and conformance fixtures | v0.16 |
| Proof soundness | ProofPlan metadata consistency checker | v0.16 |
| Standard compatibility | descriptive CKB standard script fixture suites | v0.16 |
| Transaction validation | schema-bound `cellc validate-tx` | v0.16 |
| Transaction templates | `cellc solve-tx` template emitter | v0.16 |
| Deployment governance | local deploy plan, dep locks, field-level proof diff, audit bundle | v0.16 |

---

## 13. Roadmap Discipline

Roadmap entries should follow these rules:

- completed work must point to tests, release notes, or evidence reports;
- deferred work must say why it is deferred;
- security-sensitive syntax must distinguish data source from authority;
- CKB production claims must distinguish compiler evidence from chain evidence;
- metadata-only invariant claims must not be described as executable verifier
  checks;
- builder assumptions must be visible as assumptions;
- wiki pages should teach the current stable surface, not act as release notes;
- version-specific roadmap files should remain detailed enough for reviewers to
  audit scope, risk, dependencies, and release gates.

---

*Document End.*

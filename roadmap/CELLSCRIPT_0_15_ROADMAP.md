# CellScript v0.15 Roadmap

**Status**: Implemented for P0 release scope; P1/P2 items are deferred unless explicitly promoted
**Scope**: Scoped Invariants and Covenant ProofPlan
**Dependencies**: v0.13 and v0.14 complete

---

## Goal

v0.15 makes CKB script semantics explicit and auditable.

CellScript should let developers express transaction and cell invariants in a CKB-native way while making these facts visible:

- `trigger`: when the verifier runs
- `scope`: which cell universe the invariant reasons over
- `reads`: which transaction views are inspected
- `coverage`: which cells are actually protected
- `on_chain_checked`: which obligations are enforced by generated code
- `builder_assumption`: which obligations are only construction/deployment assumptions

CellScript must not hide lock/type differences behind placement syntax. Lock and type scripts are different execution triggers with different coverage models.

v0.15 still resets the primitive layer, but the user-facing theme is:

```text
Scoped Invariants & Covenant ProofPlan
```

---

## Out of Scope

Do not re-plan v0.13:

- bounded generics
- value collections
- phantom tags
- generic interfaces/templates
- specialization/inlining/DCE/const propagation
- CLI ergonomics
- hash type DSL exposure
- transaction builder MVP
- fuzz expansion

Do not re-plan v0.14:

- Spawn/IPC
- structured `WitnessArgs`
- explicit Source views
- ScriptGroup and CKB transaction-shape conformance
- TYPE_ID metadata validation MVP
- target profile formalization
- declarative capacity/time/since syntax
- wider byte-slice/resource Blake2b hashing beyond v0.14 fixed-Hash support
- WASM backend
- builder integration
- advanced CellDep/DepGroup patterns
- script reference and HashType strictness

---

## P0

### 1. First-Class Script Semantics *(Implemented)*

**Problem**

CKB lock/type is not just "where a constraint is placed". It is a trigger and coverage boundary. A lock covenant can scan global inputs/outputs, but it only runs for inputs sharing that lock. A type invariant runs for the type group and naturally covers cells sharing that type script.

**Change**

Add first-class script semantics to invariant/proof metadata:

```text
trigger = lock_group | type_group | explicit_entry
scope = group | transaction | selected_cells
reads = input | output | group_input | group_output | cell_dep | header_dep | witness
coverage = covered_cells(...)
on_chain_checked = true | false
builder_assumption = none | declared(...)
```

Example:

```cellscript
invariant udt_amount_non_increase {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>, group_outputs<Token>

    assert sum(group_outputs<Token>.amount) <= sum(group_inputs<Token>.amount)
}
```

**Code Areas**

- invariant AST
- semantic analyzer
- metadata schema
- docgen audit output
- CKB strict diagnostics

**Acceptance**

- every invariant records trigger, scope, reads, coverage, and enforcement status
- strict mode rejects invariants without explicit trigger and scope
- compiler warns when `trigger = lock_group` and `scope = transaction` are used together
- diagnostics explain that transaction scans from a lock do not imply type-group conservation

---

### 2. Scoped Aggregate Invariant Primitives *(Implemented as metadata-only)*

**Problem**

Protocol macros for UDTs, pools, settlements, rentals, and covenant locks need aggregate transaction checks. Without scoped aggregate primitives, the compiler keeps growing protocol-specific recognizers.

**Change**

Add scoped aggregate assertions:

```text
assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
assert_conserved(Token.amount, scope = group)
assert_delta(Token.amount, delta, scope = selected_cells)
assert_distinct(outputs<NFT>.id, scope = transaction)
assert_singleton(type_id, scope = group)
```

Rules:

- every aggregate assertion must bind `scope`
- source view must be explicit
- field type must be fixed-width integer or fixed bytes
- overflow traps fail closed
- loops are bounded by declared group/transaction limits

**Code Areas**

- type checker field projection
- invariant lowering
- IR aggregate ops
- CKB codegen loops
- ProofPlan aggregate obligations

**Acceptance**

- UDT-style amount conservation lowers without token-specific recognizers
- pool invariant helpers lower through aggregate primitives
- generated code traps on overflow and malformed cell data
- tests cover `group`, `transaction`, and `selected_cells` scopes

---

### 3. Covenant ProofPlan *(Implemented)*

**Problem**

Verifier obligations are split between IR patterns, metadata recognizers, and codegen-specific checks. Auditors need to see not only what code was emitted, but what CKB trigger/scope/coverage semantics it has.

**Change**

Add a `ProofPlan` stage:

```text
AST / stdlib macro
  -> ProofPlan
  -> IR
  -> codegen
  -> metadata
```

`ProofPlan` must contain:

- invariant name and source span
- trigger
- scope
- reads
- coverage
- input/output relation checks
- group cardinality
- identity policy
- preserved script/data/capacity fields
- witness fields and decoded proof payloads
- on-chain checked obligations
- builder assumptions
- codegen coverage status

Example diagnostic:

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

**Code Areas**

- new `src/proof_plan/`
- invariant lowering
- IR builder
- metadata emitter
- codegen verifier coverage reporting
- `cellc explain-proof`

**Acceptance**

- strict CKB mode fails if any invariant obligation is metadata-only
- `cellc explain-proof` prints trigger/scope/reads/coverage/on-chain status
- dangerous trigger/scope combinations produce warnings or strict-mode errors
- tests compare ProofPlan obligations with emitted code coverage

---

### 4. Split Kernel Primitives from Protocol Macros *(Implemented for capability vocabulary; macro-only lowering deferred)*

**Problem**

`Create`, `Consume`, `Transfer`, `Destroy`, `Read`, `Claim`, and `Settle` sit at the same AST level. `create/consume/read parameters/read_ref<T>()` are kernel-level. `transfer/claim/settle/shared/pool` are protocol-level.

**Change**

Keep these as kernel primitives:

```text
input<T>
output<T>
cell_dep<T>
create_output
consume_input
input_output_binding
read_cell_dep
assert_data
assert_lock
assert_type
assert_absence
assert_field
assert_group_cardinality
```

Move these to stdlib proof macros:

```text
transfer
claim
settle
shared
pool.swap
pool.add_liquidity
pool.remove_liquidity
```

Protocol macros must lower through scoped invariants and ProofPlan, not protocol-name recognizers.

**Code Areas**

- `src/ast/mod.rs`
- `src/parser/mod.rs`
- `src/types/mod.rs`
- `src/ir/mod.rs`
- `src/codegen/mod.rs`
- stdlib macro expander
- metadata schema
- examples using protocol verbs

**Acceptance**

- strict mode rejects protocol-verb capability declarations such as `has destroy`
- 0.15 kernel-effect capabilities are accepted by direct lifecycle checks where applicable
- stdlib and source lifecycle forms emit inspectable ProofPlan obligations and macro provenance
- fully macro-only lowering for `transfer`, `claim`, `settle`, and `shared` remains deferred

---

### 5. Add First-Class Cell Identity and TYPE_ID Lifecycle *(Implemented)*

**Problem**

v0.14 validates CKB TYPE_ID metadata plans and transaction-shape facts. v0.15 promotes identity into a first-class primitive policy across create, update-output, and destroy flows. CKB TYPE_ID remains one supported identity backend, with verifier rules derived from first input, output index, and group cardinality.

**Change**

Add identity policies:

```text
identity none
identity ckb_type_id
identity field(path)
identity script_args
identity singleton_type
```

Add identity policy forms:

```text
create_unique<T>(identity = ckb_type_id)
replace_unique<T>(identity = ckb_type_id)
destroy_unique<T>(identity = ckb_type_id)
```

`preserve_identity(input, output)` and
`assert_identity_absent(identity, scope)` remain future helper names until they
have canonical lowering and ProofPlan coverage.

**Implementation**

- `IdentityPolicy` enum (`None`, `CkbTypeId`, `Field(String)`, `ScriptArgs`, `SingletonType`) added as first-class AST primitive on `ResourceDef`, `SharedDef`, `ReceiptDef`
- `IrIdentityPolicy` enum mirrors AST in IR layer; `IrTypeDef.identity` field carries the policy
- `IrInstruction::CreateUnique { dest, pattern, identity }` and `IrInstruction::ReplaceUnique { dest, operand, pattern, identity }` carry identity metadata through full pipeline
- Parser: `identity(ckb_type_id)`, `identity(field(path))`, `identity(script_args)`, `identity(singleton_type)` on type declarations; `create_unique<T>(identity = ...) { ... }` and `replace_unique<T>(identity = ...) { ... }` as expression forms
- Type checker: validates identity policy constraints for CreateUnique and ReplaceUnique
- Codegen: `emit_create_unique` and `emit_replace_unique` emit identity-aware RISC-V with identity labels
- Metadata: `TypeMetadata.identity_policy` field exposes the policy in compiled JSON (hidden for default `none`)
- Formatter: `format_identity_policy()` handles all 5 variants
- 5 dedicated tests for identity metadata emission, default policy, field/script_args/singleton_type variants

**Code Areas**

- type identity attributes
- IR cell identity metadata
- ProofPlan identity obligations
- CKB TYPE_ID codegen
- metadata validation

**Acceptance**

- CKB TYPE_ID creation is runtime-checked or rejected in strict mode
- TYPE_ID continuation proves identity preservation, not only type-script preservation
- group cardinality follows CKB TYPE_ID rules
- tests cover create, update-output, destroy, duplicate output, and unrelated same-type output

---

### 6. Add Explicit Destruction Policies *(Implemented)*

**Problem**

Current CKB lowering scans all outputs and rejects any output with the same TypeHash as the consumed input. That proves singleton-type absence, not instance destruction.

**Change**

Add policy-specific forms alongside bare `destroy`:

```text
destroy_unique(cell, identity = type_id)
destroy_instance(cell, identity_field = id)
burn_amount(cell, field = amount)
destroy_singleton_type(cell)
```

`forbid_output_successor(cell, match = script_hash + identity)` remains a
future helper. v0.15 implements the four policy-specific destruction forms
above.

**Implementation**

- `DestructionPolicy` enum (`Default`, `SingletonType`, `Unique { identity }`, `Instance { identity_field }`, `BurnAmount { field }`) added to AST
- `IrDestructionPolicy` mirrors AST in IR layer; `IrInstruction::Destroy` carries `policy: IrDestructionPolicy`
- Parser: `destroy_singleton_type(cell)`, `destroy_unique(cell, identity = type_id)`, `destroy_instance(cell, identity_field = id)`, `burn_amount(cell, field = amount)` as context-sensitive identifiers; bare `destroy cell` still accepted as `DestructionPolicy::Default`
- `lower_destruction_policy()` converts AST→IR policy
- Codegen: all `Destroy` instruction matches updated with `policy` field
- Formatter: policy-specific output for each destruction variant
- `check_primitive_strict_015()` rejects legacy `has destroy` capability declarations in strict mode; bare `destroy cell` requires the `consume + burn` kernel effects

**Code Areas**

- AST destroy node
- type checker resource obligations
- IR destroy pattern
- CKB output scan codegen
- metadata transaction obligations

**Acceptance**

- `destroy_instance` allows unrelated outputs with the same type script
- `destroy_singleton_type` preserves the current same-TypeHash absence behavior
- burn policies prove quantity deltas instead of output absence
- tests cover multi-instance cells sharing one type script

---

## P1

### 7. Covenant Helper Stdlib *(Deferred)*

**Problem**

Developers need ergonomic helpers for common lock covenant and type invariant patterns, but CellScript must not pretend it can automatically move constraints between lock and type without changing semantics.

**Change**

Add explicit helpers:

```text
lock_covenant(...)
type_invariant(...)
builder_assumption(...)
selected_cells(...)
```

Helpers must emit ProofPlan records with trigger/scope/coverage.

**Code Areas**

- stdlib invariant helpers
- macro expansion provenance
- ProofPlan metadata
- example contracts

**Acceptance**

- helper output is fully visible in `cellc explain-proof`
- no helper performs automatic lock/type placement
- builder-only assumptions are clearly marked and rejected by strict on-chain enforcement checks when required

---

### 8. Split Address, LockScript, and LockHash *(Deferred)*

**Problem**

`Address` currently behaves like a 32-byte lock hash. CKB lock identity is a full `Script { code_hash, hash_type, args }`; `lock_hash` is only its hash.

**Change**

Add distinct semantic types:

```text
Address
LockArgs
LockScript
LockHash
TypeScript
TypeHash
ScriptHash
```

Define explicit transfer macro targets:

```text
transfer_to_lock_hash(asset, lock_hash)
transfer_to_lock_script(asset, lock_script)
transfer_to_address(asset, address, resolver = standard_lock)
```

**Code Areas**

- builtin type table
- ABI/schema typing
- transfer macro expansion
- output lock verification
- builder metadata

**Acceptance**

- source code cannot pass `Address` where `LockHash` is required without a resolver
- full `LockScript` verification can check script fields, not only hash equality
- metadata distinguishes `recipient_address`, `expected_lock_script`, and `expected_lock_hash`

---

### 9. Make CKB Script Role Explicit *(Deferred)*

**Problem**

The compiler still has heuristic entry selection: `main`, first no-arg action, first action, then first lock. CKB artifacts need explicit role and entry identity.

**Change**

Add explicit entry declarations:

```cellscript
#[entry(lock)]
lock owner_lock(owner: LockHash) -> bool {
    ...
}

#[entry(type)]
action verify_transition(state_before: State) -> state_after: State
where
    ...
```

or add first-class item kinds:

```text
lock_script
type_script
transition
```

**Code Areas**

- parser attributes
- AST item role
- entrypoint resolver
- codegen artifact metadata
- CLI scoped compile path

**Acceptance**

- strict CKB compile rejects modules with multiple possible entries and no explicit entry
- artifact metadata records `entry_name`, `entry_role`, and group scope
- lock and type entries cannot silently compete by source order

---

### 10. Rename Internal `type_hash` *(Implemented)*

**Problem**

CellScript uses CKB Blake2b for compiler-facing hashes and CKB TypeHash over packed `Script`; public metadata keeps these hash domains explicit.

**Change**

Rename public metadata fields:

```text
dsl_type_fingerprint
molecule_schema_hash
ckb_type_script_hash
ckb_lock_script_hash
ckb_type_id_args
```

**Implementation**

- Metadata fields renamed: `type_hash-absence` → `ckb_type_script_hash-absence`, `type_hash-preservation` → `ckb_type_script_hash-preservation`, `lock_hash-preservation` → `ckb_lock_script_hash-preservation`
- All internal references updated in `src/lib.rs`

**Code Areas**

- IR pattern metadata
- manifest generation
- scheduler metadata
- builder-facing JSON
- tests asserting metadata keys

**Acceptance**

- no public metadata field named `type_hash` refers to a source type-name hash
- CKB script hashes are always derived from packed `Script`
- diagnostics point metadata consumers to the canonical field names

---

### 11. Reset Resource Capability Vocabulary *(Implemented)*

**Problem**

`has destroy` keeps protocol verbs inside the resource type system. After protocol verbs move to stdlib macros, capabilities must describe kernel effects, not business actions.

**Change**

Replace protocol capabilities with effect capabilities:

```text
store
create
consume
update_output
burn
relock
retarget_type
read_cell_dep
```

Rejected protocol capability spellings:

```text
transfer
destroy
```

**Implementation**

- AST `Capability` extended with 7 new variants: `Create`, `Consume`, `Replace`, `Burn`, `Relock`, `RetargetType`, `ReadRef`
- New capabilities are context-sensitive identifiers in `has ...` clauses (not global lexer keywords), preserving backward compatibility with code using these words as identifiers
- `Capability::is_protocol_verb()` and `Capability::kernel_effects()` classify capabilities for migration
- `format_capability()` and `capability_name()` updated in fmt, docgen, and types modules
- Strict mode (`--primitive-strict=0.15`) rejects `has destroy` (CS0151) with precise diagnostics
- Compatibility mode (`--primitive-compat=0.14`) accepts legacy vocabulary

**Code Areas**

- capability parser and formatter
- type checker linear obligations
- stdlib transfer/destroy macro requirements
- metadata type capability export
- migration diagnostics

**Acceptance**

- strict mode rejects `has destroy`
- protocol macros state their required kernel capabilities explicitly
- metadata exports effect capabilities, not protocol verbs

---

### 12. Add Versioned Cell Data Layout Policies *(Deferred)*

**Problem**

CKB cells store bytes. Molecule schema metadata exists, but transition rules do not yet make data layout version and migration policy a primitive obligation.

**Change**

Add layout policies:

```text
#[data_layout(molecule, version = 1)]
preserve_layout<T>()
preserve_schema_hash<T>()
migrate_layout<T>(from = 1, to = 2)
assert_data_version<T>(version)
```

**Code Areas**

- type attributes
- Molecule schema manifest
- ProofPlan data-layout obligations
- deserialization bounds checks
- migration diagnostics

**Acceptance**

- schema-backed cell transitions declare preserve or migrate behavior
- migration requires explicit old/new layout binding
- metadata includes data layout hash, version, and migration policy
- strict mode rejects schema-backed update outputs with no layout policy

---

### 13. Remove Claim/Receipt Name Heuristics *(Deferred)*

**Problem**

Claim logic recognizes fields and functions by names such as signer, beneficiary, recipient, amount, and claim variants.

**Change**

Require explicit proof bindings:

```cellscript
claim_proof(
    receipt,
    signer = receipt.signer_pubkey_hash,
    recipient = receipt.beneficiary,
    amount = receipt.amount,
    nonce = receipt.nonce
)
```

**Code Areas**

- receipt type checker
- claim macro
- metadata recognizers
- examples

**Acceptance**

- deleting or renaming a field does not silently change claim semantics
- compiler no longer uses function-name special cases for claim behavior
- claim examples emit the same checks through explicit ProofPlan bindings

---

### 14. Make Update Cardinality Explicit *(Deferred)*

**Problem**

One-to-one updates are explicit in 0.13 through signature-directed
`action(before: T) -> after: T` topology and `transition`/`require` constraints, but
split, merge, and rebalance transactions still need a first-class way to
declare cardinality and pairing policy. Those shapes should not fall back to
compiler guessing or scattered consume/create reconstruction.

**Change**

Add explicit multi-cell update/cardinality forms:

```text
update_one(input, output)
split_one_to_many(input, outputs)
merge_many_to_one(inputs, output)
rebalance(inputs, outputs, invariant)
```

**Code Areas**

- update cardinality analysis
- update pattern metadata
- codegen source selection
- metadata obligations

**Acceptance**

- one-to-one updates keep current signature-directed behavior
- split/merge requires explicit invariant
- compiler diagnostics name the exact missing pairing or cardinality rule

---

### 15. Emit Macro Expansion Provenance *(Partially implemented)*

**Problem**

After protocol verbs become stdlib macros, audits need to see what each macro expanded into. Source-level `transfer` must not hide verifier obligations.

**Change**

Emit expansion provenance:

```text
macro_name
macro_version
source_span
expanded_kernel_ops
proof_plan_obligations
codegen_coverage
```

Implemented in v0.15:

- ProofPlan coverage records expose selected `macro_expansion:*` provenance for
  lifecycle and pool-related flows.
- `cellc explain-proof` reports those records in human-readable and JSON output.

Deferred:

```text
cellc explain-macro <entry>
```

**Code Areas**

- stdlib macro expander
- source span tracking
- metadata schema
- docgen audit output

**Acceptance**

- every protocol macro expansion appears in metadata
- audit output links source span to emitted kernel checks
- strict mode rejects opaque macro expansion

---

## P2

### 16. Move `shared` to a Scheduler Policy Library *(Deferred)*

**Problem**

`shared` is a scheduling/protocol policy, not a CKB primitive.

**Change**

Keep the core language limited to cell access and proof obligations. Implement shared-state flows as library policies:

```text
shared.read
shared.locked_update
shared.versioned_update
shared.queue_claim
```

**Acceptance**

- no core AST item is required only for shared-state scheduling
- shared policies emit explicit ProofPlan constraints
- scheduler metadata is derived from ProofPlan, not from source-name recognition

---

### 17. Compatibility and Migration *(Implemented)*

**Change**

Keep one canonical primitive surface:

```text
--primitive-strict=0.15
```

Add diagnostics:

```text
CS0151 legacy destroy capability must use consume + burn kernel effects
CS0152 Address cannot be used as LockHash
CS0153 CKB entry role must be explicit
CS0154 claim proof bindings must be explicit
CS0155 type_id identity policy must be explicit
CS0156 protocol capabilities are not allowed in strict mode
CS0157 schema-backed update output requires a layout policy
CS0158 invariant trigger and scope must be explicit
CS0159 lock_group + transaction scope requires explicit coverage acknowledgement
CS0160 builder assumption is not on-chain checked
```

**Implementation**

- `--primitive-compat=0.14` and `--primitive-strict=0.15` CLI flags added to `CompileOptions`
- `check_primitive_strict_015()` gate in `src/lib.rs` rejects protocol verbs (`has destroy`) in strict mode
- CS0151 (legacy destroy capability) diagnostic emitted; CS0156 remains reserved for broader protocol-capability diagnostics
- Remaining CS0152–CS0160 codes reserved for future P1/P2 items

**Acceptance**

- bundled examples compile only in canonical strict mode
- diagnostics include the rejected syntax, canonical syntax, and affected proof obligation

---

## Release Gates

v0.15 cannot ship until:

- every invariant records trigger, scope, reads, coverage, and enforcement status
- `lock_group + transaction` covenant patterns produce coverage diagnostics
- strict primitive mode rejects protocol-verb capability declarations
- 0.15 kernel-effect capabilities compile in the canonical examples
- selected protocol lifecycle forms emit ProofPlan macro provenance
- `cellc explain-proof` exposes trigger/scope/reads/coverage/on-chain status
- TYPE_ID identity policy is covered by ProofPlan and runtime codegen
- direct `destroy` accepts explicit 0.15 `consume + burn` kernel effects and
  policy-specific destruction forms remain available
- resource capabilities use kernel effect names in strict mode
- ProofPlan coverage is checked in tests
- examples pass in the canonical strict track

Deferred release-gate candidates for later milestones:

- full macro-only lowering with no protocol-name codegen recognizers
- explicit CKB entry roles
- `Address` / `LockScript` / `LockHash` type-system split
- versioned data-layout preserve/migrate policies
- full `cellc explain-macro` source-map output

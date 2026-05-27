# CellScript CKB Language Fit Audit

This note records the 0.13 audit summary for the CellScript language design and
its fit with the CKB execution model.

## Audit Note 1: Language Fit With CKB

CellScript is now shaped as a CKB-first language rather than a general smart
contract DSL with a CKB backend. Its main abstractions map directly to CKB
concepts:

| CellScript construct | CKB-facing meaning |
|---|---|
| `resource` | Linear Cell-backed asset, represented by input/output Cells and typed data. |
| `shared` | Contention-sensitive state Cell, read through CellDeps or updated by consume/create. |
| `receipt` | Single-use proof Cell for claim, settlement, or protocol evidence. |
| `consume` | Spend an input Cell. |
| `create output = T { ... }` | Constrain a named proposed output Cell with typed data and optional lock. |
| `read param: T` / `read_ref<T>()` | Load read-only CellDep-backed state. |
| `action` | Transaction-shaped state transition entrypoint. |
| `lock` | Spend predicate entrypoint compiled to ckb-vm RISC-V. |
| `protected` | Marks a typed input Cell view guarded by the current lock invocation. |
| `witness` | Marks typed transaction witness data; it is not signer authority by itself. |
| `lock_args` | Typed fixed-width script-args source; this is data-source binding only, not signer authority. |
| `require` | Fail the current script validation when a lock condition is false. |

The strongest design point is that persistent state is explicit. Ordinary local
values do not silently become chain state; action signatures bind transaction
inputs and named proposed outputs, while `create output = T { ... }` constrains
those outputs rather than allocating runtime objects. Linear values must be
consumed, destroyed, transferred, claimed, settled, or linked to explicit output
constraints. This is a good fit for CKB because it keeps the transaction input,
output, data, witness, and dependency shape visible to the compiler and release
evidence.

The 2026-04-26 surface pass keeps this alignment intact. Its completed changes
are presentation-level or classification-level: cleaner example modules,
DSL-native capability declarations, field shorthand, typed empty `Vec<T>`
literals, explicit `protected` / `witness` / `require` lock syntax, and
fixed-width `lock_args` binding. It does not add implicit signer authority,
hidden sighash defaults, or automatic signature verification.

The 2026-05-18 coercion hardening keeps numeric convenience inside verifier
expressions while preserving explicit boundaries. The rule is named
expression-local unsigned widening: primitive unsigned integers may widen only
inside arithmetic and numeric comparison. Assignment, return, ABI, witness,
Molecule/layout, struct field initialization, and serialization boundaries
remain exact-type boundaries unless the source contains an explicit cast.
Integer literals may still be context-typed by an expected primitive integer
type, but non-literal values do not pretend to have another width at a boundary.

The 0.13 compiler also exposes CKB-specific evidence instead of hiding it behind
a generic artifact:

- CKB Blake2b hash policy and supported script `hash_type` values.
- Molecule-facing schema and ABI metadata.
- Entry witness ABI and witness-size accounting.
- DepGroup and deployment-manifest policy surfaces.
- Runtime error registry and fail-closed production policy.
- Capacity, tx-size, and measured-cycle evidence requirements.
- Scheduler/access metadata for shared or mutable state.

The current production acceptance evidence is therefore meaningful: the seven
production bundled examples strict-compile under the CKB profile, every bundled
business action is builder-backed on a local CKB chain, every bundled lock has
builder-backed valid-spend and invalid-spend coverage, and the production gate
requires dry-run cycles, committed valid transactions, consensus tx size, and
occupied-capacity checks.

## Audit Note 2: Remaining Semantic Gaps

The current design is CKB-aligned, but the language does not yet fully encode
the complete CKB security model as first-class syntax. Some guarantees are still
split across compiler metadata, builders, and production evidence.

| Gap | Current status | Required direction |
|---|---|---|
| Signer authorization | `witness Address` parameters can prove equality only inside explicit lock predicates such as `vesting_admin`; `lock_args Address` now exposes script-args data, but neither value proves witness-sighash ownership by itself. | Add explicit script-hash policy, sighash verification, and later first-class verified signer binding. |
| Lock behavior | All 17 bundled locks are strict-compiled and covered by builder-backed local CKB valid-spend and invalid-spend transactions. | Keep the matrix mandatory and extend it when new locks enter the bundled production scope. |
| Explicit Cell updates | Metadata exposes input/output access through action signatures and `require` constraints; source no longer looks like in-place account storage. | Keep continuity policy explicit for type id, lock, data schema, and capacity. |
| Capacity policy | Capacity evidence is builder/runtime-required and validated by reports. | Promote common capacity requirements into declarative DSL policy where practical. |
| Timelock policy | since/header/runtime features are visible in metadata. | Make since/header assumptions more directly declarative and statically auditable. |
| Language examples | `examples/registry.cell` and every checked-in `examples/language/*.cell` file cover compiler/tooling surfaces such as bounded local Vec behavior, stdlib patterns, CKB source/witness, TYPE_ID, Spawn/IPC, capacity/time, and dynamic BLAKE2b. | Keep them outside production CKB scope unless promoted into builder-backed chain evidence. |

The most important correction is to avoid overstating what action coverage
proves. The current production run proves transaction shape, Cell data layout,
builder integration, capacity sufficiency, and runtime acceptance for all bundled
business actions. Authorization-sensitive examples now expose authority checks as
lock predicates and the bundled lock predicates are exercised with positive and
negative on-chain spend cases. That still does not make a witness `Address`
parameter a cryptographic signer proof by itself. In CKB terms, the current
syntax should be read as a typed view over one guarded input Cell plus decoded
witness data. `lock_args` is script-bound data, but it is not a hidden
`WitnessArgs.lock` convention and not automatic sighash verification.

After the 0.14 source-surface work, the recommended order is:

1. Add explicit sighash verification primitives before adding a higher-level
   verified signer abstraction.
2. Make mutable Cell transitions declare continuity requirements explicitly.
3. Turn common capacity and timelock assumptions from report-only evidence into
   DSL-level policy where the compiler can check them.
4. Promote collection-heavy examples to production scope only after they have
   builder-backed CKB transactions and capacity evidence.
5. Advance the surface elegance RFC without implying unsupported signer or
   `protects` sugar semantics before explicit binding rules exist.

Bottom line: CellScript's language shape is correct for CKB. The next hardening
step is to move more of CKB's authorization and continuity model from evidence
surfaces into first-class language rules.

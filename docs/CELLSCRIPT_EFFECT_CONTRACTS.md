# Inferred effect facts

Status: working-tree implementation present; final-source acceptance is pending.
Decision date: 2026-10-30.
Owner: compiler, independent artifact checker, interface and tooling maintainers.
Tracked by [#85](https://github.com/CellScript-Labs/CellScript/issues/85).

## Decision

The effect contract is a structured set of automatically inferred facts:

```text
EffectFacts {
    context_reads: CanonicalSet<ContextRead>,
    lifecycle_ops: CanonicalSet<LifecycleOperation>,
    process_ops: CanonicalSet<ProcessOperation>,
}
```

These three dimensions are the only effect model. There is no exclusive category
enum, category ranking, category replacement table, or category-based admission.
The same prohibition applies to presentation: the editor and CLI show concrete
facts and evidence status, without projecting them back into five labels.

Source code contains no effect declarations. There are no attributes, function
qualifiers, body declarations, manifest settings, handwritten interface promises,
CLI overrides, or alternate spellings for declaring or reserving an effect.
Ordinary operations and resolved bindings express the program's behavior.

```cellscript
public fn capacity() -> u64 {
    return ckb::cell_capacity(source::input(0))
}
```

This callable has a context-read fact for the input capacity observation. Its
lifecycle and process sets are empty. The developer writes no effect annotation.

## Fact identities and representation

Each set uses a bounded, closed vocabulary with canonical wire names. The
current fact vocabulary is:

- Source kinds: `input`, `group-input`, `output`, `group-output`, `cell-dep`,
  `header-dep`, `witness`, `current-script`, `transaction`, `header-evidence`,
  and `parameter` with a `u16` parameter position. Indexed witness observations
  retain their input/output group source; `witness` names the whole inventory.
- Context accesses: `cell-data`, `capacity`, `occupied-capacity`, `lock-script`,
  `type-script`, `out-point`, `since`, `index`, `cardinality`, `role-selection`,
  `header`, `transaction`, `script`, `witness`, and `environment-evidence`.
- Lifecycle operations: `consume`, `destroy`, `create`, `replace`, `transfer`,
  `claim`, and `settle`. Destruction includes its `consume` constituent;
  replacement, transfer, claim and settlement include `consume` and `create`.
  The specific operation tag remains present. Wire sets missing a required
  constituent reject; operation tags still do not prove lifecycle correspondence.
- Process operations: `spawn`, `exec`, `wait`, `pipe-create`, `pipe-read`,
  `pipe-write`, `close`, `inherited-descriptors`, and `process-id`.

Sets are capped at 4,096 facts per dimension. Canonical order uses wire names,
with numeric ordering for parameter positions; context reads sort by source kind,
parameter position and access name. It does not use internal enum discriminants.
Source substitution uses one immutable resolved binding map for each call edge.
Forwarding a symbolic source is explicit; a missing binding rejects.

The versioned operation registry owns explicit mappings for every admitted typed
operation and runtime helper, including operations with no external effects.
Registering an operation requires an effect decision; symbol prefixes, action
names, string heuristics and a default empty set are not classification rules.

- A context-read fact identifies the observed transaction context and access.
  Distinguish inputs, outputs, CellDeps, header dependencies, witnesses, current
  Script and transaction-level observations as supported by resolved source
  evidence. Preserve group/source qualifications and parameter-relative origins
  where they affect the contract. A parameter-relative read is instantiated
  through the bound call argument, rather than guessed from a type name.
- A lifecycle fact identifies the admitted lifecycle operation. Creation,
  consumption, destruction and explicit replacement retain distinct operation
  identities wherever the language gives them distinct semantics. A combination
  of operations is represented by their union. It is not compressed into an
  aggregate mutation tag.
- A process fact identifies the admitted operation, including spawn, exec, wait,
  pipe creation, reading, writing and closing, inherited-descriptor access, and
  process identity observation. They remain distinct from transaction reads.
  Composite operations include every justified constituent fact.

Merely passing a view or an already decoded ordinary value does not add a new
transaction read. Reading transaction-backed data through a view must retain its
resolved source and observation evidence. Taking or invalidating lifecycle
authority continues to require the existing binding and ownership checks.

The concrete fact vocabularies and their operation mappings must be closed and
tested together. An unrecognized operation, source, substitution or wire name is
an error. There is no wildcard fact, unknown-as-empty case, extensible string bag,
or catch-all category. Unsupported semantics retain their specific fail-closed
behavior and cannot acquire executable evidence from an effect fact.

All three wire fields are required. Sets have deterministic ordering and no
duplicates; unknown fields, unknown members, duplicate or noncanonical entries,
and over-budget values reject. Empty sets represent completed inference of no
external operations. They are never a deserialization default for missing data.
Internal allocation IDs and compiler enum ordinals do not enter public hashes.

## Inference and evidence

For a callable, the result is the component-wise union of its direct facts and
its resolved, instantiated callees' facts. The inference engine must preserve
each dimension independently. Reading an input and then validating a new output
retains both the read and creation facts.

Analyze actions, ordinary helpers, locks, native entries, imported and aliased
calls, qualified calls, generic instances and recursive components where
admitted. Use deterministic bounded work and reject graph or convergence budget
exhaustion. Resolve operations in their defining owner and substitute actual call
bindings. A user function must not be confused with a same-spelled intrinsic.

Source, type checking, lowering and independent artifact checking must agree on
the same resolved operation identity. An AST name or syntax footprint alone is
insufficient evidence for a published contract. Missing owners, unresolved calls,
unknown substitutions and unavailable evidence fail explicitly.

The compiler's phase order is structural typing and resolved binding, raw
unoptimized IR lowering, fact inference, and validation of deferred effect
constraints. Borrowed observation helpers and invariant calls are checked only
after that inference succeeds. This exposes reads inserted by lifecycle,
membership and exact-role lowering. An AST visitor is not a parallel semantic
authority. Only complete facts may leave the raw lowering boundary. Optional
optimization is followed by reconstruction and equality checks; it cannot seed
facts from the earlier summary.

Preserve source validation before optional optimization can erase an operation
or invalid body. Bind the source contract to admitted optimized operation and
call evidence; an optimizer must not silently change the public facts. A
temporary empty accumulator may initialize fixed-point analysis but cannot
escape before resolution and convergence.

The independent checker reconstructs the facts from bound typed/lowered
operations and callee evidence, and compares the result exactly. It does not
trust producer-written facts, interface copies, broadened sets, rebound hashes
or equal ELF bytes. Keep this reconstruction independent from compiler analysis.
Shared closed descriptions do not replace independent verification.

Private pointer/stream-descriptor helpers require the enclosing machine contract.
They are explicitly excluded from direct typed-call admission; they do not receive
empty facts simply because their buffers cannot be described by a source-level
argument. Shared descriptions require a decision for every runtime identity.

A registered deferred helper has a separate exact non-returning failure contract:
its error identity, availability feature and `NoReturn` ABI must agree. The
analysis stops that control-flow path; it does not invent a successful operation
or reserve permission for an unimplemented semantic operation. A known abort
before any external operation can therefore have empty facts, just as an
explicit local validation failure can. Missing descriptors and unresolved
behavior still reject, and production availability policy still rejects the
deferred feature. Tests must preserve both the failure code and that policy.

Bare named `spawn` is rejected during semantic checking with E1021, including
metadata-only analysis. It has no child binding or argument/descriptor transport,
no runtime helper and no deferred feature. Resolution precedes this restriction:
ordinary user functions named `spawn`, including imports and aliases, remain
ordinary source calls. Implemented CellDep spawn/exec transports retain their
explicit process facts and independent binding/machine contracts. The earlier
status stub and subsequent error-88 abort helper are removed; regenerate their
artifacts rather than treating the old descriptor as current.

The current closed `DeferredFeature` inventory has three families and six helper
symbols, all `NoReturn`. Each aborts in development and rejects under production
policy. These are specific unavailable contracts, not promises of future delivery:

| Deferred family | Helpers and failure | Existing, separately scoped capability |
| --- | --- | --- |
| `ckb-sighash-all-deferred` | `__ckb_sighash_all`; error 66 | `env::sighash_all_zero_lock` provides a bounded current-group signing-message domain. It does not support multisig layouts retaining a nonzero configuration prefix. |
| `ckb-since-constraints-deferred` | The four `__ckb_require_maturity`, `__ckb_require_time`, `__ckb_require_epoch_after`, `__ckb_require_epoch_relative` helpers; error 86 | Typed `InputView.since` reads, checked decoding and temporal comparisons remain executable. A script must bind its business requirement to the selected input's actual since. |
| `ckb-occupied-capacity-deferred` | `__ckb_occupied_capacity`; error 87 | `ckb::cell_occupied_capacity` reads a Cell's actual occupied capacity. It does not implement the legacy compile-visible floor API. |

The signing message follows the selected Lock's convention; CKB does not impose
one universal signature-message algorithm on every Script. Since is a field on
each transaction input, not on the Cell's Lock Script. Occupied capacity counts
the capacity field, Cell data, Lock Script and optional Type Script, excluding
witnesses; known field lengths can be calculated exactly. See the
[runtime matrix](CELLSCRIPT_0_30_CKB_RUNTIME_VIEW_MATRIX.md),
[standard Lock implementation](https://github.com/nervosnetwork/ckb-system-scripts/blob/master/c/secp256k1_blake160_sighash_all.c),
[since specification](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0017-tx-valid-since/0017-tx-valid-since.md)
and [occupied-capacity implementation](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/src/extension/capacity.rs).

`Fails(CollectionRuntimeUnsupported)` is a separate exact-failure classification:
the two bounded consume/create-each guards abort with error 24 and do not emit a
deferred feature. Surface `reserved` is another axis: for example,
`cell-backed-collection-return` has its own production-rejected fail-closed
feature without belonging to `DeferredFeature`. Exhaustive Rust matches require
a decision for new variants; inventory/payload/ABI tests and generated-surface
freshness still establish agreement between separately maintained descriptions.

Generated source interfaces retain justified facts for concrete callables even
when optimization prunes their executable entry. Their facts use the bundle's
resolved artifact policy and source/group placements. A pruned declaration does
not receive verified executable evidence merely by carrying source facts.

An uninstantiated generic template has no single executable effect contract:
its required `effects` field is explicitly `null`. It is inferred separately for
each concrete instance. No concrete instance proves a universal template effect
guarantee. A missing `effects` field is an error; a nullable source-evidence field
is not an optional declaration, default or old-protocol reader. Concrete
callables must have facts, and generic template declarations must carry explicit
unavailability. Typed executable entries always carry the three mandatory fact
sets. Checked projections represent missing executable evidence explicitly, and
a required checked guarantee cannot be satisfied by that absence.

## Consumer rules

- An external-effect-free context requires all three sets to be empty. Local
  assignment, temporary collection mutation and validation failure are allowed
  under that condition. No promise of success, termination or low cost follows.
- Borrow admission checks lifecycle authority, invalidation, aliasing and escape
  directly. Ordinary observation helpers cannot perform lifecycle or process
  operations. Any future process-capable borrowing rule requires its own explicit
  policy and evidence; a display string cannot grant permission.
- Lock predicates retain their lifecycle restrictions and authorization rules.
  Infer their actual facts without injecting a category merely because the
  callable is a lock.
- Lifecycle bindings establish the exact consumed/proposed Cells, correspondence,
  identity preservation and obligations. A replacement operation fact alone does
  not prove a successor relationship. CKB state changes remain Cell replacement.
- Scheduler conflicts and parallelism use the relevant access/source and binding
  evidence. A coarse summary must not authorize scheduling or erase process I/O.
- Hover, generated docs and CLI output list the concrete read, lifecycle and
  process facts with evidence status. An empty result can be displayed as
  "no external operations". No retired category label or numeric category ID is
  serialized or used as an alternate public summary.

## Directional replacement policy

This decision explicitly replaces #28's effect-category comparator. The effect
dimension of required-interface admission is component-wise inclusion:

```text
candidate.context_reads ⊆ required.context_reads
candidate.lifecycle_ops ⊆ required.lifecycle_ops
candidate.process_ops   ⊆ required.process_ops
```

Compare canonical semantic facts, including resolved or appropriately bound
source identities. There is no category-order fallback or exceptional legacy
table. Additional candidate facts in any dimension reject, even if another
dimension is unchanged. Removing facts satisfies only the effect ceiling.

Required signatures, lifecycle obligations, input/output correspondence,
ownership, capabilities, codecs, ABI, dispatch, policy and deployment bindings
remain independently enforced. Removing creation cannot fulfill a required
output obligation. Replacing a checked replacement relation with unrelated
consumption and creation does not establish correspondence. Effect inclusion is
not behavioral equivalence or sufficient artifact admission.

The generated interface and receipt bind the inferred facts. Compilation must
not silently rewrite a required baseline. CLI diffs, upgrades, independent
projections, receipts, handles, policies and builders use this same effect
relation and preserve their other obligations.

Required examples include adding an input read to creation, adding a pipe write
to an observing helper, and changing a bound replacement to unrelated lifecycle
operations. The first two fail the effect ceiling; the third must pass every
operation and correspondence obligation separately. This policy is intentional,
not a claim that the retired table has equivalent behavior.

## Source and wire cutover

Deliver one clean model across all active producers and consumers. There is no
backward-compatibility requirement and no second admission path.

- Delete the category enum, class fields, joins, ranks, replacement matrix,
  display projections and numeric category encodings, including scheduler-facing
  copies. Do not implement a category-to-facts translator or retain a type alias.
- Delete manual declarations, default declarations, presence booleans, optional
  upper bounds, declared/inferred dual models and obsolete APIs.
- Reject retired attributes and the native `effects { ... }` wrapper. Do not
  strip, ignore, auto-convert or alias them. Native `replace`, `pool`, `retire`
  and `fresh` appear directly after verification and audit declarations, with
  their full disposition and authorization checks retained.
- Replace affected metadata, typed-semantic, interface, projection, receipt,
  scheduler and adapter contracts together. Advance every affected source
  profile/schema/domain and the actual next artifact-cache generation. Current
  partial implementation generations do not reserve compatibility obligations.
- Remove old readers, defaulting deserializers, version negotiation, compatibility
  flags, dormant branches and transition shims. Reject old or mixed bundles with
  a complete rebuild/regeneration diagnostic; old caches must miss.
- Preserve rejection fixtures and immutable historical evidence. They do not
  authorize any current syntax, wire reader or semantic fallback.
- Explain intentional identity changes. Preserve unrelated runtime failure,
  handle encoding and lifecycle authorization contracts. Do not regenerate
  resource ceilings or receipts merely to conceal differences.

The current implementation allocates these identities together; validation is
still in progress and none is an alternate reader for an older generation:

| Boundary | Current identity |
| --- | --- |
| Compile metadata envelope | 74 |
| Typed semantics / lowering record | v11 / v11 |
| Source artifact map / artifact boundary | v4 / v4 |
| Package interface / checked projection | v5 / v3 |
| Nominal declaration catalog | v3 |
| Exact Script / deployment line receipts | v3 / v3 |
| Frozen source / fixed policy receipts | v3 / v3 |
| Compile receipt | v4 |
| Scheduler facts | `cellscript-scheduler-facts-molecule-v2` |
| Source semantics | `2026-0.33-effect-facts-v1` / `2027-0.33-effect-facts-v1` |
| Artifact cache | v148, `0.33-allocated-result-effect-flow` |

This document contains the sole accepted design. No previous proposal is an
optional mode, retained draft specification or source of implementation policy.

## Delivery and acceptance

Implementation ownership follows the phase boundaries: compiler maintainers own
resolved inference and context rules; checker maintainers own independent fact
reconstruction and admission; interface/tooling maintainers own wire cutover,
replacement consumers and presentation. Completion requires the entire flow.

- [x] Implement closed canonical facts and exhaustive operation/helper mappings.
  Check set union, source substitution, equality, canonical encoding and bounds.
- [x] Integrate source/type/lowering inference for all admitted callable forms.
  Cover transitive, imported, aliased, generic and recursive calls, same-spelled
  user/intrinsic resolution, source validation and optimization boundaries.
- [x] Verify exact independent reconstruction. Hostile omission, addition,
  broadening, substitution, rebound facts/hashes and missing evidence must reject.
- [x] Cover input reads plus creation, consumption plus creation versus bound
  replacement, and every admitted process/IPC operation. Local mutation and
  failing validation alone must not invent external effects.
- [x] Replace context, borrow, lock and scheduler consumers without losing their
  independent authority, aliasing, lifetime, correspondence or failure checks.
- [x] Implement product-subset replacement consistently. Test reflexivity,
  transitivity, union behavior, widening in each dimension, source changes,
  missing evidence and required lifecycle obligations across real checked bundles,
  CLI diffs, receipts, handles, builders and upgrade plans.
- [x] Remove all active category models and manual declarations; retain no display
  labels, numeric encodings, old readers, translators, aliases or fallback modes.
- [x] Complete current-only profiles, schemas, cache invalidation and every
  affected producer/consumer. Reject retired and mixed wire/source generations.
- [x] Complete parser/recovery, formatter, diagnostics, LSP/editor, browser/native
  metadata, examples and normative documentation parity. Keep heavy native
  evidence out of browser payloads and measure affected artifact sizes.
- [x] Preserve the unannotated capacity-observation reproduction as a permanent
  checked-bundle regression. Retired syntax and category encodings reject.
- [ ] Run focused checks and the commit gate; backend before IR/codegen integration
  claims, dev at integration checkpoints, and ci before merge-readiness claims.
  Record exact source, commands, outcomes, identity changes and exclusions.

The core category model and duplicate labels have been removed in the working
tree. Compiler facts and independently reconstructed facts pass focused bundle,
binding, interface and borrowed-source tests. Generic source templates require
explicit unavailable evidence rather than a contract borrowed from one instance.
Website consumers and generated examples use concrete facts with strict
current-field validation. The canonical browser build passes its size budget,
82 tests of the distributed binary and the website's full static build checks.
The page and worker share the build-generated WASM content identity, without an
old compiler API fallback or a separately maintained version string.
These focused checks do not establish full acceptance, release readiness or
publication approval.

Registry source-interface admission accepts only v5. Its independent TypeScript
fact reader validates the same mandatory closed sets, and upgrade admission
compares candidate facts separately in every dimension. Concrete callable facts
are mandatory; template evidence is explicit null. Earlier source-interface
readers and category-based comparison fields are removed. Wire conformance data
is shared by the standalone Rust checker, Registry tests and browser tests;
additional compiler-produced interface fixtures check Rust/Registry comparison
agreement. They provide format/compatibility evidence, not executable authority.

Context-source objects reject additional members, including an `index` attached
to a non-parameter source. The strict Rust deserializer uses only the current
grammar; it provides no alternate representation or earlier-format reader.
Generated SDK receipt schemas, numeric version types and runtime guards derive
from the native receipt constants. Old numeric versions reject even if their
schema field is rewritten to the current name.

The resolved-call follow-up also removes lossy external-label sanitization and
the separate import-alias label route. A foreign callable has one assembly
identity derived from its complete defining module and source symbol; the
generated dotted namespace cannot be spelled as a source declaration. Dependency
calls use their recorded source-binding index when rewritten, rather than
parsing assembly labels as source paths. Distinct owners and direct/transitive
aliases are covered by native/checker/VM regressions. Cache v143 invalidates
earlier outputs; it provides no reader for the retired label scheme.

Ordinary function IR is named `Function` / `IrFunction`, without a purity label
or retained type alias. These internal types are not serialized; the current
checked callable kind and source function syntax are unchanged.
`docs/CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md` records that audit and
the retained edition, visibility and storage contracts separately from retired
effect protocols.
Do not trigger TAC or Daybreak.

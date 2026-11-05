# Authoring Implementation and Production Acceptance

## Objective and current state

Implement the [adopted authoring target](CELLSCRIPT_AUTHORING_TARGET.md) to
production quality, with no loss of the admitted language's functionality or
feature completeness. This includes the complete language and toolchain, not
only a Token example or acceptance of old source text.

The issue #23 acceptance boundary passed `dev`, `ci`, and `backend` on
2026-09-08 at `69488316`. The backend evidence includes the clean-source CKB
production/stateful acceptance harness.

The first implementation step restores familiar authoring over the shared
declaration, type, value, and statement grammar. Ordinary action/lock bodies
may omit `verification`; the formatter currently emits the established explicit
marker. Multiple source entries, default parameter provenance, read references,
and existing lifecycle operations are retained. A legacy consume keeps its
existing semantics and evidence classification. It does not acquire a native
retirement or pooled-accounting guarantee.

The bounded native preview4 grammar remains accepted as an implementation
reference. It is not the final authoring surface. Complete shared-policy product
support, executable branch-alternative successors, the remaining relation
policies, and graph-wide schema-acknowledgement integration are still required.
Neither reuse of the shared parser kernel nor successful scoped action ELFs
satisfies those requirements.

## Completion requirements

This table covers the wider authoring objective, including constructor and
ABI work outside the accepted bounded 0.30 portfolio. Every row needs
current-state evidence before that wider objective can be called complete.
A passing test must actually exercise the stated behavior. Missing
evidence, a metadata label, and an unsupported construct that rejects safely
are not implementation of a required supported feature.

The target includes all A1-A6 contracts, all acceptance fixtures in the authoring
target, and the applicable RFC gates. This checklist does not remove their
requirements or turn an experimental source identity into a release identity.
Public publication or a mainnet deployment is not implied by local acceptance.

## Retained language inventory

The authoritative baseline is the admitted language and its positive/negative
evidence, not an older minimal tutorial. Preserve existing
supported behavior and the exact unsupported boundaries. An already unsupported
dynamic collection does not become a new parity obligation; a supported
bounded 0.26 collection cannot be downgraded to a fixed-role preview.

| Feature family | Evidence to retain and extend |
|---|---|
| Modules and interfaces | Imports and aliases, qualified names, project resolution, public/package/private visibility, stable interface extraction and compatibility diagnostics. |
| Declarations | Resources, shared Cells, receipts, structs, enums, constants, invariants, flows, helpers, actions and locks; attributes and documentation survive. |
| Types and generics | Scalars, fixed bytes, tuples/arrays, canonical `fixed_value` normalization, derived aggregate abilities, phantom identity, bounded monomorphization, imported public generic instantiation and unsupported-layout rejection. |
| Policies | Capabilities and entailment, identity and destruction policies, type/hash declarations, capacity floors, validity and effect annotations. |
| Values and control flow | Checked arithmetic, wide integers, bitwise/shifts, casts, calls, assignments, aggregates, nested patterns/guards, branching, loops, labels, early returns and borrow regions. |
| Entry roles | Existing value, witness, input/output, protected, lock-args and read-only sources, rich ABI marshalling and named outputs; provenance is not authorization. |
| Verification | Individual/block `require`, custom failures, pure/read-only helper calls, flow transitions, terminal evidence, invariants and bounded quantifiers. |
| Lifecycle | Create, consume, destroy, claim, settle, unique creation/replacement and continuity; equivalent new forms must exist before any old supported operation is removed. |
| Bounded collections | Exact GroupInput scanning, fixed-width decoding, zero/one/N/N+1, checked accumulation, output-plan codec and complete output correspondence, including negative cases. |
| Consumers and evidence | Metadata, ProofPlan, scheduler/effects, source maps, checker, CLI/builders, project/lock handling, formatter, LSP, editor and WASM. |

`tests/authoring_parity.rs` provides representative differential and runtime
cases. `tests/syntax_combo/matrix.toml` drives the generated Edition 2027
combination audit. Neither corpus by itself proves the whole table: imported
packages, runtime ABI, deployment and product evidence need their matching
integration suites and gates.

## Authoring successor relations

The authoring route now accepts `replace before -> after { ... }` as a
statement inside ordinary actions. The declaration is the sole authority for
its checks: `data { f = same | f = expr }` and `data = same except { f = expr }`
resolve against the concrete schema of the bound predecessor (exhaustive
coverage, unknown and duplicate fields rejected), `capacity = same` and
`identity = same` lower to the canonical capacity and type-identity
preservation checks. `lock = exact(address)` binds the successor through the
create kernel exactly like `std::lifecycle::transfer`.
`lock = exact_hash(script_hash)` uses the same machine comparison but accepts
only the dedicated `ScriptHash` source domain. Complete Lock and Type Script
hashes read through typed CKB transaction views already inhabit this domain;
`ckb::script_hash(hash)` explicitly treats a trusted raw `Hash` as a complete
Script hash without claiming that the Script exists, is deployed, or provides
authorization. An `Address` or unconverted `Hash` fails typing.

Elaboration stays at the structured AST/IR level — no preview4 text is
generated or reparsed — and `tests/authoring_replace.rs` holds the relation to
the spelled-out lifecycle form in Edition 2027 (identical obligation set, formatter
round-trip) plus real CKB-VM positives and negatives for address and complete
hash locks. `replace` remains a contextual keyword outside relation syntax.

A relation whose successor would need to bind more than the relation states is
rejected rather than defaulted. Source-level successor completeness is enforced
for the authoring route: a role disposed anywhere must be disposed exactly once
on every accepting path, and disposal inside loops is rejected.

`lock = same` is executable: conservation recognizes the updated-successor
shape, where every field is either a verbatim alias of the consumed input or
a verifier-checked u64 update whose provenance roots in it (constant offsets
keep field provenance, mirroring subtraction). Branch-local relations work
in both `if` arms: cached schema-field reads now carry a branch depth and
epoch, and sibling arms or loop bodies re-materialize when the defining
context does not dominate, which removes an unsound cross-arm reuse that
previously failed the typed-record dataflow check. Existing generated code
is unchanged; only the previously invalid branch-local shapes differ.

## Binding correctness found during parity work

The comparison exposed existing truthfulness gaps. The fixed-Cell implementation
now addresses these rather than copying inaccurate records as reference semantics:

- Ordinary consume and Cell-backed parameter/output lowering retains transaction
  `Input`/`Output` positions and reports them accurately. Native ports use actual
  GroupInput/GroupOutput locations with membership and complete fixed-group
  coverage checks. Nonzero-group VM regressions reject foreign same-layout
  Cells disguising an invalid active group.
- Ordinary and native `protected` Lock parameters use the current Lock group,
  correcting the implementation against the documented contract. They do not
  impose a new exact lock-group cardinality requirement. The role guard rejects
  ambiguous Cells whose Lock and Type hashes both equal the executing Script
  hash; this is a deliberate acceptance-set tightening, not unchanged behavior.
- Explicit `read` parameters use positional CellDep loads. These loads do not
  implicitly authenticate a deployed Type Script identity. Provenance and role
  records now classify that membership as `unproven`.
- Read parameters and standalone `read_ref` expressions share one CellDep
  ordinal allocation. Four-dependency VM cases check each slot and reject the
  previous parameter/expression alias bypass. Repeated generated names and
  source aliases do not replace local binding identity.
- Anonymous creates retain distinct output/local identities and distinct
  role/disposition records even when generated names repeat.
- Witness arguments omit runtime-bound parameters from their encoded sequence,
  and Script.args uses byte offsets. Signature parameter indexes alone do not
  describe either physical layout. Full shared-layout ABI contracts remain
  part of production acceptance.

The mandatory fixed-Cell table introduced in typed semantics v5 is retained in
`cellscript-typed-semantics-v12`. The
independent checker cross-checks typed locals, roles and provenance, including
hash-rebound source/ordinal/identity and missing-record mutations. This does
not establish general syscall dataflow equivalence. The source-set artifact
cache identity now uses `project-source-set-v27-verifier-failure`.
Fixed membership checks share a demand-driven runtime helper with an explicit
96-byte frame in lowering evidence. This avoids repeating the full hash and
role checks in every callable; it does not relax membership or group coverage.


These findings are grounded in the compiler binding paths, not a complete
security assessment. The implementation must distinguish preserved ordinary
source behavior, fixes to inaccurate records, and deliberate new group semantics.
Hash equality with an inaccurate record is not a reason to retain the error.

## Fatal verification and ordinary helper values

The helper call boundary previously allowed a numeric verification error to
return as an ordinary value, which a caller could discard or compare as business
data. Generated fatal checks now use a demand-driven current-process EXIT sink.
Explicit typed failures end in `verifier-failure`; ordinary values and deliberate
raw statuses keep their existing channels. The [artifact boundary](CELLSCRIPT_VERIFIED_ARTIFACT_BOUNDARY.md#fatal-verifier-failures)
specifies the new typed, core-identity and machine records and their limits.

Optimization must preserve an expression unless it is both side-effect-free
and proven non-failing. Unused checked arithmetic, casts, indexing, schema
reads and transitive failing calls therefore remain evaluations. A bounded
constant specialization can still fold proven-total literal helper calls;
it cannot substitute a partial callee expression into a differently typed
caller context. The optimizer regressions changed from four failing families
to 16 passing tests, including existing controls. The lexical-shadowing test
is defensive AST coverage: ordinary type checking already rejects duplicate
bindings, so it is not evidence of an accepted-source shadowing vulnerability.

## Signing-message boundaries

Authorization integration also exposed a pre-existing unsupported path:
`env::sighash_all` did not construct a canonical transaction digest. It could
reach a runtime placeholder returning a zero pointer instead. This is not an
implemented signing primitive and cannot be used as an authorization reference.

The shared deferred-call classification now reports
`ckb-sighash-all-deferred`. Production policy rejects it before artifact
generation. Audit artifacts terminate the process with runtime error 66 at the
call, rather than treating an error integer as a Hash or returning through a
helper whose caller could ignore the result. Optimizer call-purity checks retain
deferred, unresolved and imported evaluations; argument substitution cannot
erase or duplicate an evaluation without the required purity evidence.

The 0.30 branch adds a separate executable contract,
`env::sighash_all_zero_lock(max_group_inputs, max_inputs,
max_extra_witnesses, max_witness_bytes) -> SighashAllDigest`. It streams the
current input Script-group signing message, replaces the complete first
`WitnessArgs.lock` payload with equal-length zero bytes, and commits to later
group witnesses and transaction-level extra witnesses. All four bounds are
integer literals and are enforced at runtime. Metadata schema 75 and the
standalone checker bind the scope, transform, order, type, and limits to typed
semantics and runtime provenance. Differential CKB-VM evidence matches the
pinned `ckb-sdk-rust` generator for this all-zero placeholder domain.

This narrower contract does not implement prefix-preserving multisig message
layouts. The legacy `env::sighash_all(source)` remains deferred, and standard
multisig Locks continue to own their message construction and signing flow.

The independent checker rejects reclassification of this known deferred helper
after outer-hash rebinding. Its terminal-failure records now independently check
the recorded static error assignment and jump to the exact current-process EXIT
sink. That does not establish that every deferred call reaches the required
failure site: complete call-to-failure machine binding remains an explicit
checker completion requirement. The deferred-call behavior has direct VM
evidence, separate from the classification and static-site checks.

## Bounded persistent Type policy

An explicit package `[[artifacts]]` declaration selects a concrete resource,
numeric action tags, `policy-witness-v1` dispatch and ordered common checks.
There is no inferred selector, implicit export from source order, or builder
choice that can replace a stored Cell's verifier. Package, in-memory and virtual
source APIs use the same checked policy resolver; executable and metadata-only
paths retain the same selected contract. Explicit builds do not read or overwrite
the default entry's artifact cache.

The first implementation has deliberately bounded executable scope:

The [policy ABI](CELLSCRIPT_POLICY_WITNESS_ABI.md) uses a bounded canonical
Molecule envelope keyed by Script role and complete Script hash. It validates
the entire record vector before selection, rejects duplicate or unsorted keys,
and leaves each selected action's existing `CSARGv1` payload unchanged. The
whole WitnessArgs limit is 4,096 bytes with at most eight records. Host and CKB
adapter codecs are independently implemented; placement preserves other fields,
rejects occupied `input_type`, and must occur before signing.

Metadata schema 75, `cellscript-typed-semantics-v12` and
`cellscript-semantic-foundation-v4` bind the declared policy, selector provenance,
resource layout, variant payload schemas, fixed counts and ordered common
checks. Runtime metadata also binds `cellscript-ckb-runtime-view-v1`, the
closed typed CKB view contract used by 0.30 runtime field access. Lowering
record v12 retains separate terminal-failure sites and requires the exact new
nested versions. The parser-free checker also derives builder encoding flags
and parameter order/source/type from the typed record. Its specialized policy
check binds the canonical scanner, selector, tag branches, selected argument
range and exact action adapters to machine instructions. It does not prove
arbitrary action semantics or deployment authentication.

Typed semantics v8 additionally records versioned trusted external verifier
dependencies. That record is admissible only when emitted code checks the
selected CellDep's exact data hash before EXEC or SPAWN/WAIT. Its separate
`trusted-external` evidence tier binds identity and delegation while explicitly
denying any compiler proof of the external verifier's internals; see
[Trusted External Verifiers](CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md).

Focused CKB-VM tests execute mint, transfer, merge and burn against the same
compiled policy bytes, including optimization levels 0–3, nonzero group
positions, output-only creation, shared witness positions for two complete
Script identities, and rejection cases for role/hash/tag/version/length errors,
extra or missing group members and failing common checks. These are mechanics
fixtures without issuance or ownership authentication. They are not the
required authenticated, stateful Token lifecycle acceptance fixture.

`tests/policy_authorization.rs` separately spends a Token under a real bundled
multisig-v2 Lock at a nonzero input position. Transfer and burn reject missing,
partial and wrong-key signatures. Post-signing changes to the selector, action
memo and sibling record change the actual signing message without changing the
raw transaction hash, and fail specifically at the owner Lock. Permitted memo
and sibling-record edits succeed after legitimate re-signing. This establishes
spending authorization for that witness group, not authorization of arbitrary
other groups, issuer-authorized minting or the full required Token lifecycle.

`tests/policy_lifecycle.rs` separately runs the issuer-authorized Token
lifecycle in real CKB-VM: positive Token Cells are always outputs of earlier
verified transactions, and every action runs under the identical persistent
policy Script. Six committed transactions per edition and optimization level
cover authorized mint, transfer, merge and burn, replay rejection, wrong
amounts, missing/extra outputs and wrong-key credentials. Its negatives also
pin the issuer bound itself: an attacker Cell, an issuer Cell reachable only
through CellDeps, and out-of-range issuer indexes must all fail. The last case
exposed a generated-code defect now fixed: the runtime SourceView helper added
an unchecked dynamic index to the tagged view word, so an index at or above
2^32 carried into the view tag and re-routed an `Input` request to an
`Output` view, letting an issuer-locked output counterfeit input authority.
The helper now fails closed with `ckb-source-view-invalid` (44) before
encoding. This is local live-Cell bookkeeping against the workspace SDK, not
node admission, chain confirmation or clean-tag release evidence, and the
complete required lifecycle fixture remains pending.

## Next implementation boundaries

The next vertical slices must share an explicit resolved entry context. They
must not infer group ownership from the source edition or from the presence of
the retained preview AST annotation.

- A policy declaration is the sole authority for its exported variants,
  explicit tags, common checks, group resource and versioned dispatch ABI.
  Package, in-memory and virtual-source compilation must resolve the same
  declaration model before producing executable or metadata-only products.
  Builders must not expose retained helper actions as dispatch variants.
- Group rebinding applies to the exact resolved policy resource, not every
  resource-shaped parameter. Read dependencies keep their CellDep locations.
  Mixed absolute/group lifecycle roles need physical alias and coverage checks;
  changing every role to GroupInput is not a substitute. Unsupported combinations
  in the new policy path need explicit diagnostics without removing the existing
  ordinary/scoped compilation path.
- Internal calls and entry adapters are distinct. Current action preludes can
  reload transaction Cells instead of using passed Cell pointers. A retained
  action must not silently acquire either a public tag or a new binding context.
  Cell-bearing action reuse needs caller-anchored bindings, not merely disabling
  one parameter-loading flag.
- Direct successor nodes must carry individual occurrence identity and their
  actual CFG position. Every successful path must cover its lifecycle roles;
  no successful path may dispose of one role twice. Explicit failure exits need
  not finish a transition. A global union of branch dispositions cannot prove
  those properties. Field, identity, capacity and authorization obligations remain
  separate from unique lifecycle accounting.
- `same except` expands after concrete schema resolution and instantiation.
  Every field treatment must be explicit in the resulting semantic record.
  The v1 schema acknowledgement is separate review evidence bound to the
  relation and old/new schema identities; a newly added implicitly preserved
  field blocks receipt creation. Existing preview envelope labels are not a
  substitute for deriving policies from the actual relation and its checks.
- The policy witness encoder must preserve existing single-entry CSARGv1 bytes.
  Introduce a separate bounded, versioned multi-record envelope for overlapping
  Script groups, with explicit field ownership and rejection of duplicate or
  unknown selectors. Assemble it before signing; never silently overwrite or
  merge an occupied signature or argument field.

The signed multisig fixture in `tests/entry_witness_abi.rs` is a useful existing
authorization anchor: it combines a real Lock and generated Type Script at a
nonzero input position and rejects witness tampering after signing. It is not
yet the shared-policy lifecycle fixture. Its current local SDK dependency is
not clean-tag release evidence; release validation must use the required pinned
SDK without overwriting unrelated checkout changes.

## Local differential-evidence refresh

The fixed-binding runtime and exact-size guard optimization satisfy the existing
example size and machine-shape budgets without increasing them. The ensuing
backend gate exposed stale measured iCKB snapshots. The supported refresh path
reran all 218 `ickb_diff` tests; each retained its asserted transaction outcome.
A recursive comparison of the 187 matrix rows found no added or removed scenario,
fixture change, accepted/rejected outcome change, original binary change, or
capacity, fee or transaction-size change.

There are 92 changed generated-artifact hashes and eight lower CellScript cycle
measurements: two deposit positives each save eight cycles, and six limit-order
positives each save 22. Error-URL code hashes and dynamic transaction context
hashes also change. Three malformed-deposit negatives require an explicit
additional qualification:

- Nonzero DAO data previously reported the DAO Script at `Outputs[0].Type` with
  error -19; it now reports the generated policy at `Outputs[1].Type` with error 11.
- Short and long DAO data previously reported the DAO Script with error -4;
  each now reports the generated policy with error 11.

The pinned `ckb-script` 1.1.0 verifier iterates Script groups by full Script hash
and returns the first failure. The changed generated ELF changes its Script
identity and moves its explicit malformed-deposit rejection before the DAO
rejection. These are first-reported-error changes, not identical exit-code
evidence. The named failure modes and rejection outcomes remain unchanged.

This is a working-tree evidence refresh, not a new production-equivalence claim
for the authoring target. The matrix's pre-existing `cellscript_source_commit`
marker still names `0be86497a0c1918a9b11bd74f9ccbf234f6c49fe`; the refresh command
does not update it or prove current-source reproducibility. The compiler source
pin and locally refreshed benchmark submodule must be closed together before
release. No benchmark submodule publication is authorized by this refresh.

### Terminal-failure snapshot refresh

A second supported refresh after the terminal-failure backend changes passed
all 218 tests. Compared with the preceding working-tree matrix, all 187
differential outcomes remain unchanged: 37 accepting and 150 rejecting rows.
The 22 supporting evidence rows are unchanged. Fees, capacities and transaction
sizes are unchanged. The new matrix SHA-256 is
`1d7e3d07797b96fbbfe080981b1fad1a16963f01327e3c9974a0f93c770756bb`.

All 37 measured positive transactions use fewer cycles on both sides of the
comparison: the CellScript-side reductions range from 238 to 480 cycles,
totaling 9,540, and the original-side reductions total 5,310. These are complete
transaction measurements, including shared generated auxiliary Scripts, not
isolated measurements of either principal verifier. Rejecting rows retain zero
cycle placeholders, not measured zero-cycle executions. The matrix does not
record ELF byte sizes, so this comparison supplies no iCKB ELF-size delta.

Generated artifact and shared auxiliary hashes change. Four normalized fixtures
change only their derived related-Type hashes; the wrong-Type relationships and
checked data-rule failures remain intact. Fourteen expected patched Owned-Owner
artifacts also change with their embedded related-Type hash. Source pins,
pinned original binary inputs, test assertions and failure-mode classifications
are unchanged.

One negative changes its first reported failure: the receipt wrong-XUDT-owner
case previously reported generated error 48 at `Inputs[0].Type`, and now reports
standard XUDT error -52 at `Outputs[0].Type`. Recomputed full Script hashes put
the unchanged XUDT group between the old and new generated group in the pinned
verifier's ordered map. Thus this transaction still rejects, but this row no
longer independently executes the later generated failure. A separate
receipt-group wrong-XUDT case still reaches generated error 48. This is a
qualified diagnostic change, not identical exit-code evidence or a new
production-equivalence claim. The earlier source-pin and submodule-publication
limitations continue to apply.

### Source-view bound snapshot refresh

A third supported refresh after the SourceView index-bound fix passed all 218
tests. All 187 differential rows keep their scenario, normalized fixture,
fee, capacity and transaction-size values, and all accepting/rejecting
outcomes are unchanged: 37 accepting and 150 rejecting rows. The new matrix
SHA-256 is
`899eb2844bd940132a3936f17aca264ac7fc57c55599b1e1dbb2eebd74c2725d`.

Every generated artifact hash changes because each measured artifact
references the hardened SourceView helper. The 37 measured positive
transactions use more cycles to execute the added bounds check: increases
range from 15 to 83 cycles, totaling 1,926. Rejecting rows keep their zero
cycle placeholders. The 150 rejecting rows' diagnostic URLs embed the new
Script hashes; error codes and reported sources are otherwise unchanged.

One negative again changes its first reported failure, reversing the earlier
qualified direction: the receipt wrong-XUDT-args case previously reported
standard XUDT error -52 at `Outputs[0].Type`, and now reports generated error
48 at `Inputs[0].Type`. The recomputed full Script hashes reorder the pinned
verifier's ordered group map back. The transaction still rejects on both
sides with the recorded `wrong_xudt_binding` failure mode. This is a
qualified diagnostic change, not identical exit-code evidence or a new
production-equivalence claim. The earlier source-pin and
submodule-publication limitations continue to apply.

### Acceptance recipe identity closure

The audited transaction recipes (`transactions-v0.23.json`) still recorded the
pre-0.26b artifact identities, so the CKB acceptance live replay aborted at
its first `artifact_data_hash` comparison. Following the established refresh
precedent, the rebind closes three distinct CKB identity layers as one audited
set:

- all sixty action/Lock case artifacts and 417 exact code-hash references name
  the final VM2 output;
- all 253 Script selectors that reference generated artifacts use Data2, while
  external dependencies retain their original hash type;
- 143 embedded full-Script-hash payload occurrences across 30 generated
  identities are recomputed from the final code hash, Data2 tag and empty
  args.

The third layer is necessary because the recipes carry complete Script hashes
inside witnesses and Cell data as resource identities. Updating an outer
`code_hash` and selector does not update those values. The intermediate replay
therefore passed every `token.cell` action but stopped at
`nft.cell:create_collection` with `CellLoadFailed`: the transaction selected
the new Data2 artifact while its payload still named the old Data1 Script.
Recomputing these values from the last full-pass recipe preserves transaction
shape and business payloads while changing only the cryptographically derived
identity bytes. The clean-source production replay accepts the rebound set and
passes all 43 action cases, 17 Lock cases and 26 stateful scenarios / 46
committed steps. Its seven end-to-end lifecycles and 19 action-branch scenarios
all commit their valid transactions and retain their required negative
rejections.

### Matched cost corpus

`tests/cost_corpus.rs` compares three named CellScript scenarios against
hand-written Rust CKB references with the same checked scope, built under
the audited profile (no_std, ckb-std 1.1.0, opt-level z, thin LTO, one
codegen unit, aborting panics, llvm-strip). Both sides run on the same VM
fixtures and must agree on every accept/reject outcome.

| Scenario | CellScript | stripped Rust | ratio | cycles CS / Rust |
| --- | ---: | ---: | ---: | ---: |
| Pool merge (2-in, checked sum, lock binding) | 2,512 B | 2,816 B | 0.89x | 6,000 / 9,232 |
| Schema roll (2 fields, one updated) | 2,272 B | 2,760 B | 0.82x | 8,661 / 10,350 |
| Ownership-claim Lock | 2,232 B | 2,304 B | 0.97x | 5,583 / 6,333 |

### WASM playground bundle budget (historical failure and current closure)

At that earlier audit, rebuilding the canonical playground bundle in the pinned container
(rust 1.97.1, wasm-bindgen 0.2.121, binaryen 131) after the 0.26b tranches
produces a 1,730,601-byte module that gzips to 659,050 bytes (643 KB) —
44,650 bytes over the 600 KB budget that `website/scripts/build-wasm.sh`
enforces, so the script correctly refuses. The committed bundle, last built
within budget before these tranches, gzips to 590,700 bytes (576 KB) and was
left in place; the over-budget output was not committed. The growth is real
surface: the policy, artifact, binding and authoring modules are reachable
from the metadata-only compile path and cannot currently be dead-stripped
from the wasm build. Binaryen 131 metrics on the rebuilt module attribute it
to aggregate compiler code (3,389 functions, ~690K wasm-opt units, ~190 KB
of memory data) rather than a single outlier block, so the trim is a
feature-gating and surface-reduction work item, not a one-line fix. The `ci` website check does not rebuild the bundle,
which is why this stayed latent. At that point the release gate's
`check_wasm_release_bundle` failed, and resolution belonged to the
language-services/product row. The 0.30 closure above fixes that row without
changing the budget.

## Historical tranche verification and current workflow

The policy tranche passed the complete `dev` gate on 2026-09-05. Its strict
quick audit report is
`target/cellscript-strict-backend-audit/strict-backend-audit-quick-20260905-205049.json`;
the accompanying quick syntax audit accepted 66 and rejected 56 of 122 generated
cases, with zero failures. Focused evidence also includes 868 compiler unit
tests, native compiler/checker clippy, real policy VM/authorization tests,
outer-hash-rebound checker mutations, CLI/builders, imports, native transport
parity and the metadata-only WASM-feature test. Generated TypeScript typechecks
and all nine generated Node tests pass. These are working-tree development
results; `ci` and clean-source production evidence are
separate requirements.

The policy tranche's full backend run also passed compiler, checker and Fiber
tests, clippy, both package scenario backends, and the strict audit checks except
the final clean-source stateful stage. Its report is
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20260905-205657.json`.
That stage stopped before scenario execution because the source tree is dirty.
Both policy gate runs predate the terminal-verifier-failure changes; they are
not evidence for those newer bytes.

The pre-policy backend run on 2026-09-05 passed compiler tests (including the
218-case iCKB suite), checker/Fiber tests, clippy, both package scenario backends,
and the 166-case syntax combination audit. Its final stateful CKB stage rejected
the dirty source tree before scenario execution. The report is
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20260905-202015.json`.
That is an incomplete backend gate, predates the policy tranche, and cannot be
reused as current-source production evidence. Existing user-owned worktree and
external SDK changes have not been cleared to bypass the gate.

Focused tests are development feedback. They do not replace the unified gates:

```bash
cargo test --locked -p cellscript --test authoring_parity
cargo test --locked -p cellscript --test entry_selection
cargo test --locked -p cellscript --test native_group_binding --test protected_group_binding
cargo test --locked -p cellscript --test read_ref_binding --test resolved_binding_metadata
cargo test --locked -p cellscript --test policy_artifact_checker --test policy_artifact_cli --test policy_imports
cargo test --locked -p cellscript --test policy_authorization --test policy_witness_codec_parity
cargo test --locked -p cellscript-tools syntax_combo::tests
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh backend
./scripts/cellscript_gate.sh ci
```

The release gate remains subject to its clean-source, pinned-toolchain,
artifact, external-tooling and chain-evidence requirements. Do not weaken a gate
or clear unrelated worktree changes to obtain a passing result. Current test
outputs and gate reports, rather than this command list, are the evidence.

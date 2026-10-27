# Bounded exact-role Cell selection and correspondence

**Status: experimental fixed/bounded-table selection and coordinate reads; stable admission remains blocked.**
This is the first semantic contract for [#49](https://github.com/CellScript-Labs/CellScript/issues/49),
`cellscript-exact-role-cell-selection-fixed-v1`. The independent host model in
[`tests/support/cell_selection_reference.rs`](../tests/support/cell_selection_reference.rs)
and its [adversarial tests](../tests/bounded_cell_selection.rs) fix the intended
outcomes independently of compiler lowering. They import neither the compiler
nor the CKB adapter. They are not a serializer, a builder admission path, a
consensus implementation or evidence of peer execution.

Implementation ownership remains the #47–#49 compiler/backend workstream.
Scope-specific review disposition and full executable admission remain pending.
Do not remove an executable-surface blocker on the strength of this model.

Named and positional source catalogs now also admit canonical nested Cell tables
under `cellscript-exact-role-cell-selection-bounded-v2`; see the
[runtime ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#nested-table-selection-execution-prerequisite).
The compiler selects this profile from resolved schemas. Named `data_shape`
fields retain their codec limits; authors do not specify byte offsets or another
profile argument. Whole-table validation precedes fixed identity extraction,
including when variable payloads precede the identity. Counts, role aliases and
input/output/successor coordinates and bounded nested `.data` reads are available.
The [nested migration example](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#executable-nested-batch-migration)
executes the variable-payload business transition. The
[mixed settlement example](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#executable-fixed-mixed-role-settlement)
executes fixed full-fill orders and two independently checked asset groups.
Finite complete-program resource fixtures now include maximum batches, full
scan domains and late business failures. Final builder/source admission, the
remaining acceptance audit and integration gates remain pending.

## Authoring target and prototype boundary

The 2026-10-19 maintainer decision requires a named-role authoring API before
stable acceptance; see the shared
[authoring contract](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#authoring-direction-adopted-on-2026-10-19).
The positional `ckb::verify_cell_roles`, `ckb::exact_cell_role` and
`ckb::selected_*` forms below describe the current experimental implementation.
They are not the final public API to freeze merely because they execute.

Declare named roles and named scan/count/byte limits once. Bind each role's
schema, complete Script identities, authority and correspondence there, then
derive the type at named role access sites instead of repeating a generic type
and numeric catalog ordinal. Resolve identity fields and policy choices
structurally, with declaration-time errors and editor rename support. Keep
role-local ordinals distinct from global/group indices, and preserve `.data`
as the distinction between schema fields and transaction properties.

The complete mixed-role settlement and nested batch examples must demonstrate
this authoring form and checked, non-escaping read-only helpers. Current view
escape/call restrictions remain until the proofs are implemented. Named access
must retain complete membership, correspondence and source/authority checking;
it cannot authenticate a policy or grant foreign ownership by itself. The
[initial-form decision](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#initial-authoring-form-complete-program-decision-2026-10-26)
selects module-local named catalogs after reviewing complete programs and their
positional comparisons. The chosen form reuses structured lowering and
ProtocolBundle/generated-builder contracts. Stable admission and all acceptance
requirements below remain mandatory.

### Experimental named verification catalog

The Edition 2027 authoring frontend now parses a module-local `cell_roles`
declaration. Its `limits` block names `input_scan`, `output_scan`,
`selected_per_side` and `aggregate_bytes`. Each named role declares its Cell
schema, `type_hash`, `lock_hash`, `identity` field, `authority`,
`correspondence`, and inclusive `inputs`/`outputs` count intervals. See the
[runnable syntax fixture](../tests/syntax_combo/seeds/named-role-verification.cell).
This is the selected initial form; stable API admission remains pending.

```cellscript
action inspect() -> u64 {
    ckb::verify_cell_roles<Settlement>()
    return 0
}
```

The referenced catalog declares budgets and policies once; repeated checks do
not repeat numeric arguments. Identity is a resolved schema field identifier,
authority is explicitly `observe` or `local_type`, and correspondence is
`preserve`, `create`, `burn` or `merge(b"...")`. Complete Script hashes are
still explicit literal policy inputs; naming a catalog does not authenticate
them. Unknown, duplicate and missing members, unsupported fields and invalid
intervals reject. Formatting retains named fields and the inclusive interval.

The compiler elaborates directly to the existing `VerifyExactCellRoles` IR
contract. No intermediate source text or new VM protocol is involved. Catalogs
are module-local and cannot be exported as public interface types yet.
`ckb::select_cell_roles<Catalog>()` constructs an immutable catalog. A named
role projection retains the complete catalog identity and a checked role
ordinal; it is not just a source alias for an arbitrary integer. For example:

```cellscript
let selected = ckb::select_cell_roles<Settlement>()
let orders = selected.orders
let count = orders.input_count()
let before = orders.input(count - 1)
let after = orders.successor(count - 1)
require before.data.amount == after.data.amount
```

The role determines the schema; no repeated type argument or numeric role
identifier is accepted on these methods. `input_count()` / `output_count()`
return `u64`; `input(i)` / `output(i)` / `successor(i)` return immutable typed
transaction views. Indexed reads check a role-local `u64` ordinal, and missing
successors reject. Unknown roles, extra arguments, mutable aliases, nesting,
storage and handle escape reject. Copying a role alias preserves its full
nominal binding, including when another role uses the same Cell schema.
See the [selection fixture](../tests/syntax_combo/seeds/named-role-selection.cell).

The existing structured field-access IR represents role projection. Its typed
field name fixes the destination role; the independent machine checker checks
the source pointer load and destination store and carries retained-table
lifetime evidence through that projection. Numeric role reads are checked
against the projected role, preventing same-schema role substitution. No
runtime name lookup or text-to-source expansion is involved. Bounded-table
selection and complete business examples are implemented as described above;
their final acceptance remains open. Exported catalog declarations are outside
the initial module-local interface. The `exact-role-selection-artifact-proof`
blocker remains required.

Focused tests compare named and positional verification in complete CKB
transactions at O0/O3, with two roles and with the maximum eight roles and
32 selected Cells per side. Repeated verification preserves ordinary locals; valid transaction
cycles are equal, and wrong layout/custody and a rejecting peer retain their
failure outcomes. Separate parser/formatter/editor tests cover malformed
declarations and catalog navigation/rename; the independent checker and WASM
metadata tests retain the same admission boundary. Named selection tests exercise
both catalog sizes at O0–O3, including aliases, all five methods, invalid ordinals
and rejecting peers. Reversing role and top-level declaration order, or renaming
catalogs and roles consistently, preserves named access in complete transactions.
These cases check independently expected successor output coordinates as well as
pairing/custody failures and rejecting peers; equal role counts alone are not
used as evidence that names still select the intended Cells. Rebound metadata and machine mutations cover role-name,
role-ordinal, constructor-name and projection/read instruction substitutions.
Editor completion follows catalog members, role aliases and resulting `.data`
fields. Edition 2027 authoring offers named declaration templates and defaults
`ckb::verify_cell_roles` / `ckb::select_cell_roles` to the module's first catalog
as an editable snippet placeholder. The author still chooses the catalog and
its exact Script hashes. Native preview and older editions keep their existing
completion surface. Expanded snippets are compiled in a focused test. Reproduce with
`cargo test --locked -p cellscript --lib named_cell_roles`,
`cargo test --locked -p cellscript --test artifact_checker named_cell_roles`,
and `cargo test --locked -p cellscript-wasm --features wasm wasm_named_cell_roles`.
These checks do not replace the full backend/CI gates or the complete-program
and builder acceptance for #49.

## Source, identity and layout matrix

One selection operation owns one closed catalog of 1–8 nominal roles. Each
role binds an independently admitted schema, one complete Type Script hash,
one complete Lock Script hash, a fixed encoded width, one fixed-width identity
field, input/output count intervals, access classification and correspondence
mode. The compiler must derive width and identity-field location from the
typed canonical layout; caller-supplied byte offsets are not a source API.
The model receives the derived layout to avoid depending on the compiler.

The catalog is an authenticated policy input, not a witness-selected filter.
Exact role hashes, access class, count bounds and correspondence mode must come
from source commitments or the existing admitted, current-Script-bound policy
path. A witness cannot substitute an empty-matching catalog or lower a minimum.
Merge target keys likewise need the declared policy binding or an explicit
source authorization predicate before selection. The host oracle assumes this
trusted contract input; it does not authenticate raw host structs. Typed source,
independent provenance and post-build substitution tests must enforce this
boundary before executable admission.

| Axis | Admitted fixed-layout contract | Rejected boundary |
|---|---|---|
| Sources | Whole transaction Input and Output, scanned independently | CellDep/HeaderDep, witness lists as the authoritative set, group-relative indexes interpreted as global |
| Protection | A Cell is protected when its complete Type hash matches one catalog role | Filtering a protected Cell out because its Lock, data, key or proposed role is wrong |
| Role identity | Unique complete Type hash per catalog; full Lock hash checked after membership | Overlapping roles, code-hash-only matching, equal-width schema substitution |
| Layout | Existing canonical fixed layout, 1–512 encoded bytes; unsigned/opaque bit-pattern fields | Recursive, dynamically sized or noncanonical values in this fixed profile |
| Pairing key | One declared fixed identity field, 1–32 bytes; byte equality within its nominal role | Unchecked offsets, mutable identity or implicit cross-role key equivalence |
| Access | Read-only observation; or current-Type-local classification checked against actual current Script role/hash | A foreign observation yielding a linear resource, a current Lock claiming Type authority |
| Operations | Complete selection, immutable typed reads, cardinality checks and the explicit correspondence modes below | Mutating the selection, silent truncation, implicit allocation/conservation or inferred peer success |
| Proposed indexes | Optional exact canonical projection of a complete selection | A proposal defining scan length, protected membership or authority |

Schemas are properties of the admitted role contract, not trusted labels read
from a Cell. A same-size data buffer does not establish interface compatibility.
The compiler/checker and ProtocolBundle must bind the schema and complete
Script identity to the same artifact/role receipt. The host model deliberately
does not pretend to prove that binding by attaching a schema hash to bytes.

The experimental source operation below verifies membership and correspondence
and returns unit. `ckb::select_cell_roles` retains the same verified catalog in
an immutable invocation-local handle. Neither form creates a container of
linear Cell resources or grants foreign ownership.

## Experimental source verification

The [mixed-role source example](../examples/language/ckb/exact_role_verification.cell)
uses `ckb::verify_cell_roles(input_scan, output_scan, selected_per_side,
aggregate_bytes, roles...)` in an action or lock entry. Each of its 1–8 inline
`ckb::exact_cell_role<CellType>` declarations takes complete Type and Lock hash
strings, an identity field name, access (`"observe"` or `"local-type"`), mode
(`"preserve"`, `"create"`, `"burn"`, `"merge"`), and input/output minimum and
maximum counts. Merge additionally requires a literal byte-string target key.
The example's hashes are illustrative; they identify no deployed contracts.

Budgets, hashes, names and policy choices must be source literals. A witness
cannot choose a catalog, access class, merge target or weaker bound. Width and
key offset come from the resolved Cell schema. The profile accepts unsigned
scalars, Hash, Address and byte-array fields of 1–256 bytes, with the whole Cell
and key bounds above. Nested fields, booleans, standalone role declarations and
calls from pure functions reject. `local-type` additionally checks the actual
executing Type Script identity; it does not grant resource ownership.

The source path now connects typing, structured IR, typed metadata, precise
bounded runtime accesses and code generation. The compiler reserves a private
4992-byte frame region, reused across sequential verification calls. Tests keep
an ordinary local alive across two verifications and execute representative
and maximum catalogs at O0–O3 in actual all-group CKB transactions, including
wrong custody/data width and a rejecting peer. Formatter idempotence and editor
completion cover the new spelling. These checks are not complete builder or
independent machine evidence.

`exact-role-selection-artifact-proof` remains a mandatory executable-surface
blocker. Strict `DenyFailClosed` compilation rejects this operation. The
independent checker validates declarations and rejects removing that blocker,
including after sidecar hash rebinding. It now reconstructs the complete fixed
verification instruction contract and checks successful typed-block paths as
described below. Enclosing aggregate-buffer alias analysis, complete source
equivalence and transaction builder binding remain pending.

### Immutable selected views

`ckb::select_cell_roles` takes the same literal arguments as verification and
infers `ExactCellSelection<T0, T1, ...>` from its nominal role declarations.
The following operations take that handle and a literal role ordinal:

| Operation | Result | Additional argument |
|---|---|---|
| `ckb::selected_input_count<T>` | `u64` | none |
| `ckb::selected_output_count<T>` | `u64` | none |
| `ckb::selected_input<T>` | `InputView<T>` | input ordinal within the role |
| `ckb::selected_output<T>` | `OutputView<T>` | output ordinal within the role |
| `ckb::selected_successor<T>` | `OutputView<T>` | input ordinal within the role |

`T` must match the chosen role, including when two roles use the same nominal
type. Role-local ordinals follow ascending global transaction indices;
successor lookup follows the checked correspondence record and can return an
output with a different ordinal. Bounds reject with runtime error 21; a missing
successor rejects with error 7. Returned views retain the existing source-view
ABI and explicit Input/Output provenance. They expose transaction fields,
existing bounded data reads and the immutable `.data` projection
described below. None grants lifecycle authority.

Handles permit immutable local aliases. Parameters, results, stored fields,
containers, mutable aliases, arithmetic, equality and unrelated calls reject.
Each selecting producer owns a distinct 4992-byte frame region. Unit-returning
verifications retain a separate reusable scratch region; later scans do not
overwrite an earlier handle's table. This is a per-producer frame cost, not a
claim that a complete invocation uses only 4992 bytes.

The independent checker binds constructor pointer materialization, alias moves
and all five read bodies to typed slots and operands. It checks role counts,
bounded record traversal, exact Input/Output tagging, partner lookup, terminal
failures and successful-path operation order. It rejects overlapping retained
selection regions and includes constructor pointer stores in the nested-view
write-effect checks. Rebound machine/typed mutations exercise these contracts.
The enclosing CFG proof additionally checks definite handle initialization,
physical slot uniqueness and retained-table write protection, as detailed below.
Complete aggregate/source proofs remain separate obligations; the experimental
blocker is retained.

Actual CKB transaction tests cover multiple simultaneous catalogs, unrelated
fee Cells, reversed outputs, local aliases and every role at O0–O3. The maximum
case has eight roles and 32 selected Cells on each side. End and `u64::MAX`
ordinals reject, and another Script's rejection still rejects the transaction.
The selected-view/nested-witness regression retains valid and invalid payload
cases across cold/hot cached byte reads and a helper reading the paired output.
Independent helper and lifetime checks now prove the cache's initialization,
bounded writes, retained pointers and borrowing across calls.
Full Cell-field hash reads have independently checked helper bodies and two
bounded caller write extents (32-byte result and eight-byte size). Those extents
must be disjoint and cannot overlap a live nested pointer, length or Cell buffer.
These fixtures do not replace the pending mixed-role settlement and nested
batch examples with complete ProtocolBundle/generated-builder binding.

### Fixed schema fields through transaction views

The fixed-layout profile below is retained. The bounded nested-table extension
is described immediately after it.

The experimental `.data` projection is available on `InputView<T>` and
`OutputView<T>`, including values returned by `ckb::input`, `ckb::output`,
`ckb::group_input`, `ckb::group_output` and the selected-role operations:

```cell
let input = ckb::selected_input<Order>(selected, 0, 0)
let successor = ckb::selected_successor<Order>(selected, 0, 0)
let before = input.data
require before.id == successor.data.id
require before.amount == successor.data.amount
```

`before` has type `CellDataView<Order>`. It retains the source domain and index
of `input`, with immutable local aliases and no allocation or lifecycle
authority. It is an observation view, separate from lexical ownership borrows.
Parameters, returns, stored fields, references, containers, mutable aliases,
arithmetic, equality on the view itself and unrelated calls reject. Field
values can participate in ordinary supported value expressions.

An immutable `InputView<T>` or `OutputView<T>` may now be passed directly to
a `Pure`/`ReadOnly` helper, which can project `.data` locally. The
[typed predicate example](../examples/language/ckb/typed_cell_predicate.cell)
extracts a two-field comparison and forwards both views through another helper.
This passes a tagged transaction coordinate, not an owned Cell or a pointer to
the caller's scratch buffer. `CellDataView` itself still cannot be passed or
returned. Mutable parameters, containers, mismatched source kinds or schemas,
and lifecycle operations remain rejected.

The helper's metadata reports an inherited `SourceView` with a bounded dynamic
index; it does not invent a concrete global/group source from the nominal type.
VM fixtures exercise distinct global/group coordinates, register and outgoing
stack arguments, full-width fields and malformed data at O0–O3. Independent
checker mutations cover the helper's field-load bodies and coordinate transport:
typed call target/order/coverage, caller source slots, register/stack argument
placement and callee parameter spills. Branch joins cannot supply facts from a
skipped setup; writes invalidate previously loaded slot facts. The independently
checked callee prefix now seeds the enclosing memory-effect proof, which protects
the coordinate parameter slots through the helper body, including helpers with
no `.data` projection. Rebound full/partial/overlapping body stores into those
slots reject at O0/O3. The proof derives outgoing stack extents from each checked
callee's expanded, aligned ABI and retains mixed nested decoder roots.
Caller-local constructors and aliases are also independently recognized: the
four global/group input/output constructors must validate the 32-bit index,
pack the correct source tag, and publish to the typed destination slot. Alias
loads/stores must preserve their nominal type and exact source/destination
slots. Typed CFG coverage rejects skipped or erased producer ranges. The
memory-effect proof requires a live checked coordinate at `.data` reads,
aliases and helper calls; full or partial intervening writes invalidate it.
Checked selection reads supply coordinates through their existing table proof.
Dynamic index computations and complete source/value reconstruction remain
separate obligations under `cell-data-view-artifact-proof`; this slice does not
freeze the named-role API or establish complete selection/settlement acceptance.

`T` must be a Cell-backed resource, shared type or receipt whose complete
fixed layout is 1–512 bytes. Fields may be `u8`, `u16`, `u32`, `u64`, `u128`,
`Hash`, `Address` or `[u8; N]` with `1 <= N <= 256`. Dynamic fields, nested
schemas, booleans and signed fields remain outside this profile. Every read
loads the complete declared Cell data into bounded invocation scratch and
checks its exact length before copying the field. Short and long data reject
with runtime error 16; syscall failures reject with 3. Invalid source domains
reject with 44. Wide fields are copied in full into fixed-byte storage, never
truncated to a scalar stack slot. The `.data` projection itself does not load
bytes or authenticate the nominal schema; exact role selection provides its
separate identity/layout checks when used.

The independent checker derives field offsets and widths from the typed
schema and checks the complete projection/load/copy instruction sequence,
including source decoding, exact length, terminal errors and branch targets.
It derives the scratch region from the actual frame and checks that scratch,
wide results and scalar destinations preserve live nested and selection
storage. Only after those checks does lifetime analysis summarize the writes,
retaining its original state budget even for maximum selection fixtures.
Copied field buffers are also disjoint from every other checked data producer's
write regions in the same invocation, including nested table load buffers.
Transient load scratch may be reused; copied results retain distinct storage.
The [coherent-copy regression](../tests/artifact_checker/cell_data_results.rs)
rebounds all byte stores, result-pointer publication, markers and hashes for
Hash, u128 and 9/256-byte fields in fixed and nested layouts at O0/O3. Exact and
one-byte result overlaps reject, as does redirecting a nested fixed-field copy
into another table load buffer. Its VM case demonstrates that the forged alias
can turn unequal input/output hashes into an accepted equality check, while the
original program rejects. CFG lifetime analysis additionally protects each copied
buffer from ordinary stores and summarized memory effects; only its recognized
producer may initialize it. The regression redirects byte, halfword, word and
doubleword body stores into both copied buffers, including partial overlaps,
for fixed/nested layouts at O0/O3. Legal mutable local assignment still copies
the value: an earlier immutable binding retains its own bytes after reassignment.
VM cases cover both equal and unequal values, and branch assignment covers
inferred/annotated u64, u128, Hash and 9-byte bindings at O0–O3.
These are buffer-integrity checks, not a complete proof of every field-value use.
Complete dynamic index and later field-value provenance remain pending:
`cell-data-view-artifact-proof` is mandatory and strict stable admission rejects
these programs. Constructor-only programs still require checked producer ranges;
their transaction-property reads do not gain complete value provenance from this
Cell-data/helper lifetime proof.

The [source example](../examples/language/ckb/typed_cell_data.cell) is also the
input to `tests/cell_data_views.rs`. Its real CKB cases exercise global/group
inputs and outputs at O0–O3, exact/short/long data, the last Hash byte, high
u128 bytes and short fixed-array bytes. Checker mutations rebind outer hashes
before changing load, copy, immediate and branch instructions. The all-group
selected/nested fixture additionally compares selected input/successor fields
at ordinary and maximum sizes. These tests do not replace complete settlement,
builder parity or production admission.

Editor completion follows local transaction-view constructors, immutable
aliases and `.data` chains, including a temporarily unfinished trailing dot.
Branch-local names and shadowed bindings do not leak into suggestions. These
lexical hints are not type or authority evidence; ordinary compiler diagnostics
remain authoritative. The warning identifies experimental artifact admission
separately from an unimplemented operation that emits a runtime trap.

### Bounded nested fields through transaction views

A typed transaction view may also project fields from a canonical table with
fixed siblings and direct bounded nested-value fields. The complete table's
maximum encoded size must be at most 32768 bytes. The same named `data_shape`
limits and Molecule codec apply to witness, direct Cell and transaction-view
reads; arbitrary recursive schemas are not admitted.

```cell
let before = selected.orders.input(i)
let after = selected.orders.successor(i)
let messages = before.data.messages
require before.data.id == after.data.id
require messages[0].len() <= 256
```

Each field read loads and validates the complete Cell, including unused fields,
into a separate statically bounded buffer in the current invocation's frame.
A nested result retains a pointer and length into that buffer; fixed siblings
are copied as scalars or complete fixed bytes. Subsequent field reads and
selection operations cannot overwrite a retained result. Repeated execution
of a producer invalidates its old derived-view facts before initializing the
new result. This is read-only stack storage, not allocation-backed collections.

Whole nested values and their admitted item/payload views compose with existing
immutable `Pure`/`ReadOnly` helpers. A helper receiving `InputView<T>` or
`OutputView<T>` may perform its own `.data` read. No view may escape its backing
lifetime, mutate the Cell, or confer foreign lifecycle authority. `CellDataView`
itself retains the restrictions listed above.

The checker independently derives table layouts and buffer extents, checks the
full load/codec/result sequence and source coordinate, and protects retained
buffers, pointers and lengths through its CFG lifetime analysis. Coherently
rebinding both the machine addresses and marker to reuse another result's
buffer rejects. Missing `nested-values-artifact-proof` or
`cell-data-view-artifact-proof` markers reject even after outer identities are
rebound. Selected-role programs also retain
`exact-role-selection-artifact-proof`; complete source and interface proofs,
transaction builder binding and stable admission remain pending.

The `named-nested-data-views.cell` fixture exercises two named shapes, fixed
siblings, derived aliases and helpers. `tests/cell_data_views.rs` covers O0–O3,
global/group inputs and outputs, large buffers, malformed unused fields,
trailing bytes and payload limits. Selected-role transaction tests additionally
exercise correspondence-selected nested fields with live witness views and
required peer groups. Per-read complete decoding is finite. The complete
byte-list and record-list migration programs now have
[frozen resource fixtures](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#frozen-complete-program-resource-fixtures),
including their repeated reads, maximum declared batches, full scans and
late-invalid executions. These finite program ceilings do not claim a single
cost bound for arbitrary user programs or settle stable admission.

## Numeric bounds and scan termination

Bounds are explicit compile-time values and apply to one transaction:

| Budget | Fixed profile |
|---|---|
| Roles | 1–8 |
| Input and Output scan limits | Independently 1–64 actual transaction Cells |
| Selected Cells | Configured 1–32 maximum per side across all roles; zero actual selection is allowed when role minima and mode allow it |
| Role count intervals | Inclusive minimum/maximum in 0–32, separately for Input and Output |
| Encoded Cell size | Exactly the declared fixed width, 1–512 bytes |
| Aggregate selected data | Configured 1–32768 bytes, summed across both sides |
| Proposed index entries | At most 64, before iterating or allocating from their lengths |

A runtime scanner probes each global index until the capacity field reports
`INDEX_OUT_OF_BOUND`. At the configured scan limit it must still probe once:
success means over-limit, not a truncated set. A non-end syscall failure is
terminal. Missing Type is an unprotected Cell only after existence has been
established; it is not the end of the transaction. Exact returned field widths
must be checked. The model's complete slices stand in for this checked scan;
its length comparisons alone are not VM evidence.

Fee/change and unrelated protocol Cells consume scan budget even when they are
not selected. No selected data is read past its exact bound; aggregate bytes
are checked across Input and Output together before retaining the next value.
Runtime memory need not retain all data: streaming reads and bounded index/key
storage are permitted, with their actual lifetime proved independently.

The source/index distinction and end behavior follow the existing
[typed runtime-view contract](CELLSCRIPT_0_30_CKB_RUNTIME_VIEW_MATRIX.md),
[VM syscall RFC](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0009-vm-syscalls/0009-vm-syscalls.md),
and CKB `script/src/syscalls/load_cell.rs` inspected at
`d40e4722525d45d5ce7a8ae29a1dc6d18d2f747f`. Local Type group coordinates count
matching Type hashes; local Lock coordinates exist for inputs only. Lock
outputs are not invented by comparing output Lock hashes.

## Ordering, complete membership and correspondence

Selection order is Input global-index order followed by Output global-index
order. Every selected Cell retains side, global index, its optional actual
current-group index, nominal role/schema, access classification and identity
key. Duplicate keys in one role on the same side reject. The same key on
opposite sides is expected for preservation; equal keys in different roles
remain unrelated.

An optional proposed list must equal that entire ordered projection, including
roles. Missing, additional, duplicated, reordered, out-of-range and wrong-side
entries reject. No individual entry can make an unprotected Cell protected.
Actual transaction permutations are allowed: rescan and reconstruct global
indexes, then pair by role and key. A stale proposal after reordering rejects.
These rules do not promise identical transaction hashes or signatures after
reordering.

| Explicit mode | Required shape | Result |
|---|---|---|
| Preserve | Equal counts and identical unique key sets for the role | One pair per input in global-input order |
| Create | Zero inputs, at least one output | Each output has no predecessor |
| Burn | At least one input, zero outputs | Each input has no successor |
| Merge | At least two inputs, exactly one output with the explicitly bound target key | One many-to-one obligation containing every selected role input |

Count intervals apply in addition to the mode. Preserve admits an empty role
only when both minima allow it. Create/Burn/Merge do not silently become no-ops.
Pair records are role-catalog order, then input order (output order for Create).
Unique role Type hashes and unique within-role keys prevent an output from
discharging two independent obligations. Merge's one shared successor belongs
to one explicit obligation, not to several independently accepted pairs.

Modes specify correspondence, not economic or access rules. Amount conservation,
capacity ownership, immutable fields, allowed payload changes and authorization
must still be source predicates. A foreign role may be observed and paired,
but only its actual peer group validates its own lifecycle. Successful local
observation never supplies a peer-success receipt. A deliberately rejecting
peer must reject the final all-group transaction in executable acceptance.

## Stable failure taxonomy

The reference errors below are stable semantic categories. Runtime lowering
must retain their named provenance and use the existing numeric failures shown;
no new runtime code is allocated by this document. Static contract errors must
be rejected before an executable artifact is admitted.

| Reference category | Boundary | Runtime mapping when applicable |
|---|---|---|
| Contract | Invalid numeric bounds, layout/key range or merge key width | Source/admission diagnostic |
| AmbiguousRole | Two roles protect the same Type hash | Source/admission diagnostic |
| Authority | Local Type claim without matching current Type context | `script-role-mismatch` (47) |
| ScanBound, SelectedBound, ByteBound, Cardinality | Complete scan, global selection, aggregate bytes or role counts exceed their limits | `collection-bounds-invalid` (21) |
| LockIdentity | Protected Cell has wrong complete Lock identity | `script-identity-mismatch` (41) |
| Layout | Protected Cell does not have its exact canonical layout | `exact-size-mismatch` (4); canonical field errors retain their existing codec code |
| DuplicateKey, DuplicateIndex | Reused role identity or proposed physical index | `collection-bounds-invalid` (21) |
| Correspondence, ProposalMismatch | Missing/extra/mispaired successor or incorrect complete proposal | `flow-transition-mismatch` (7) |
| ProposalBound | Overlarge proposed list | `collection-bounds-invalid` (21) |
| ProposalIndex | Proposed coordinate outside its actual source | `ckb-source-view-invalid` (44) |

The reference validates the contract first, then scan limits, scans Input before
Output, and finally evaluates catalog roles in order. Per selected Cell it
checks Lock, layout, selected count, byte budget and key uniqueness in that
order. Proposal checks run only after successful complete correspondence.
This gives adversarial tests deterministic first-failure expectations.

## Required extension and acceptance work

### Complete source-scan primitive

`src/codegen/cell_scan.rs` now provides the capacity-field scan used by the
existing `ckb::cell_count` backend. Its internal bounded form accepts an already
validated limit in 1–64 and always probes at the limit. Only
`INDEX_OUT_OF_BOUND` terminates successfully; missing-item and other failures,
short/long returned capacity widths and index overflow take a terminal failure
path. Scan state lives in caller-owned frame slots across syscalls. The existing
source-level `cell_count` remains unbounded and retains its source/group meaning;
this does not expose a new bounded source API.

The primitive tests execute actual generated ELF with CKB transaction syscalls
for Input/Output scans at empty, exact-limit and over-limit sizes, including
untyped Cells outside the executing Type group. Separate fault-injection VM
tests cover every limit in 1–64, non-end errors, malformed lengths, clobbered
temporary registers and large frame offsets. These establish scan termination
and source cardinality only. The separate fixed-role primitive below builds
on that complete scan; source-level selection admission remains pending.

```bash
cargo test --locked -p cellscript --lib bounded_cell_scan -- --test-threads=1
cargo test --locked -p cellscript --test typed_runtime_views -- --test-threads=1
```

### Internal fixed-role selection primitive

`src/codegen/cell_selection.rs` now emits executable fixed-layout selection and
correspondence from an internal derived contract. The experimental source
wrapper derives that contract from literal roles and canonical type layouts.
No executable-surface blocker is removed by these primitive tests.

The emitter checks actual current Script identity and distinguishes an
unambiguous Lock or Type context using the executing group. A current Script
matching both Lock and Type at the group anchor rejects with code 47, consistent
with the existing fail-closed role boundary. Creation-only Type execution uses
the actual GroupOutput anchor. A LocalType role must match that Type context.
Both transaction source counts are checked before selected Cell inspection.
The scanner then retains all protected Cells, validates complete Lock hashes,
exact data widths, global selected/data budgets and per-role key uniqueness,
and checks role cardinalities and Preserve/Create/Burn/Merge correspondence.
Unrelated and untyped Cells still count toward the scan limit and actual Lock
group coordinates. Lock outputs never receive invented group coordinates.

The caller owns a 4992-byte scratch frame. Two bounded tables retain at most 32
records per side. Each 64-byte record contains actual global and optional group
indexes, nominal role ordinal, up to 32 key bytes and the partner global index.
The role catalog supplies schema and access classification; a schema commitment
in an assembly comment is explanatory, not an independent schema proof. Merge
records point every predecessor to the one explicit successor and do not turn
it into multiple independent preservation obligations. All selected data is
validated through one 512-byte scratch buffer rather than retained in the
tables. There is no VM heap allocation. The tables remain private to the caller
and are not an admitted user pointer or general resource collection.

Differential VM tests compare records, indexes, key bytes, partners and named
failure outcomes with the independent host oracle. They cover all 576
permutations of a mixed four-Cell transaction, custody/layout substitutions,
duplicate keys, cardinality and byte/scan bounds, empty protected selections,
all four modes, creation-only groups, foreign-authority rejection and the joint
maximum of eight roles, 64 selected Cells and 32768 data bytes. Maximum tests
use hashes and keys with long common prefixes; late invalidity changes the last
key byte. Real CKB transaction tests separately execute all required Lock/Type
groups and exercise a deliberately rejecting peer. Test code cells have explicit
Type identities so the current Script commitment does not depend circularly on
the emitted ELF; this fixture setup is not deployment evidence.

```bash
cargo test --locked -p cellscript --lib exact_role_machine -- --test-threads=1 --nocapture
cargo test --locked -p cellscript --lib exact_role_source -- --test-threads=1 --nocapture
cargo test --locked -p cellscript --test artifact_checker exact_role -- --test-threads=1
```

### Independent fixed verification instruction checks

The standalone checker derives widths and identity-field offsets from the typed
Cell catalog. It consumes decoded RV64 instructions, not compiler assembly text
or the schema comment. Optional bounded lowering markers identify each typed
block/operation's candidate range; missing, orphaned, overlapping, wrong-owner
and duplicated ranges reject. Non-selection records omit this extension.

Within each range it checks the actual current-Script/group anchors, local-Type
authority, complete bounded capacity scans and end probes, full Type/Lock
identities, exact selected-data loads, cardinality and byte limits, group/global
coordinates, key uniqueness, record writes and all four correspondence modes.
Every branch must target its reconstructed logical destination. Failures load
their exact error and jump to the independently checked non-returning sink.
The wrapper's SP adjustments restore the caller frame; scratch must be inside
the actual allocating prologue's frame and outside scalar locals and saved
registers. These checks do not yet prove disjointness from every other aggregate
buffer or caller-owned pointer in an enclosing program.

After proving the complete selector body and control flow, the checker passes
its private write extent to nested-view lifetime analysis. This extent is
derived from recognized instructions, not an additional trusted sidecar field.
The analysis rejects overlap with nested pointer/length slots and live loaded
Cell buffers, invalidates scratch memory facts and caller-saved registers, and
preserves the proved caller frame. Coherently relocating both SP adjustments
and marker coordinates into a length slot or loaded Cell buffer rejects even
after all bundle identities are rebound. Witness and Cell-backed views remain
readable before and after verification. Empty fallthrough blocks sharing an
address with the next typed block retain the latter's instruction ownership;
the producer preserves label emission order for that mapping.

The checker normalizes independently decoded relaxed branches and admits the
optimizer's immediate store/reload forwarding only when the preceding SD proves
the value and no branch can enter at the replacement. Successful exits from a
typed block must traverse its verifications in declared order. External jumps
cannot enter a partial body or bypass the owning entry's prologue. Calls and
other source operations retain their separate existing proof obligations.

Tests mutate predicates, field/key/frame loads, counter/record stores and policy
constants after artifact and sidecar hashes are rebound. Well-formed changes to
Type/Lock identities, keys, budgets, counts and authority reject independently
of declaration shape checks. Reversing two identical verification bindings also
rejects, while valid conditional blocks and O0–O3 source transactions retain
their behavior. Stable admission remains closed pending the remaining proofs,
complete source provenance, nested transitions and complete builder acceptance.

Retained selection handles now also enter the enclosing CFG memory analysis,
including entries without nested values. Only a completely checked constructor
establishes a table and its pointer slot. Reads and aliases require that same
table to be definitely initialized on every incoming path. Branch joins retain
possibly live table regions for write protection while intersecting definite
initialization and pointer facts. Distinct typed handle locals cannot alias a
physical slot. Ordinary stores, private callee effects, fixed results, hash
outputs, loaded Cell buffers and exact-byte caches must preserve both handle
slots and live table regions. The analysis rejects unknown write destinations;
it does not infer safety from the source-level immutability rule alone.

Checked constructor and alias summaries may materialize their own typed handle
destination; selected-count/view reads write only their declared scalar result.
Other selection scans must remain disjoint from live retained tables. A table
that may still be live cannot be reconstructed over itself: repeated constructor
execution needs an explicit last-use/reinitialization proof before admission.
Full aggregate producer provenance remains a separate obligation. The fixed
schema observation path above has its own mandatory admission blocker.

The all-group source fixture also keeps a nested witness root and element view
live across two role scans at O0–O3, including the eight-role/32-selected-Cell
limit. It places the public-builder witness at the current Type group's actual
first input, after an unrelated fee input. Post-encoding payload substitutions,
malformed nested bytes and a rejecting peer Script remain negative cases. This
is coexistence evidence, not variable-payload Cell selection or a complete
transaction-builder admission claim.

Current working-tree observations, including the test entry and result assertions:

| Fixture | Valid cycles | Late-invalid cycles | Test ELF bytes |
|---|---:|---:|---:|
| Maximum primitive with fault-injection syscall fixture | 395427 | 376235 | 82440 |
| Real CKB mixed-role transaction, two selected Cells per side | 23552 | — | not recorded |
| Real CKB maximum transaction, 32 selected plus 32 fee Cells per side | 382711 | — | not recorded |

The first row uses VM instruction costs but does not charge real CKB syscall
transfer costs; it must not be compared as a faster/slower production result
against the other rows. The latter rows use full CKB transaction verification
and include all required groups. These observations are not frozen resource
budgets or full source/builder acceptance. The private selection frame is 4992
bytes; fixture/VM memory, peer frames and transaction capacity are separate.

### Candidate transaction and materialization binding

The experimental host projection now reconstructs exact-role selections from
checked typed entries. It follows the selected entry and its called helpers,
ignores unreachable sites, and accepts only selection sites that are mandatory
on every successful path. Selection construction itself remains restricted to
action/Lock entries; ordinary read-only helpers retain their admitted scope.
Conditional selection and spawned-verifier selection
without an actual caller Script context remain rejected at this builder
boundary; execution-path binding is still required for those combinations.

The projection carries the original role contracts and typed layouts into the
generated builder manifest and resolved ProtocolBundle. Native bundle checking
requires exact agreement with checked metadata, computes full Script hashes,
requires concrete protected input/output data, and reconstructs membership,
global/group coordinates, duplicate identities, pairs and aggregate bytes.
Nested tables validate every field, including payloads unused by business code.
The shared decoder retains the existing canonical Molecule contract.

Selection-bearing bundles use `cellscript-protocol-bundle-v2` and
`cellscript-protocol-bundle-report-v2`, with the bundle schema as the canonical
hash domain. Other bundles retain v1. Old adapters consequently reject the new
schema instead of silently ignoring its additional obligations. The current
adapter rechecks resolved input declarations and actual packed output bytes
after serialization, comparing the complete selection evidence. Generated
TypeScript binds the selected entry's catalog and refuses a missing catalog,
altered bounds or downgrade to v1.

These records are candidate-construction evidence. An adapter given only JSON
cannot authenticate substituted source sidecars or an attacker-rewritten
catalog and its hashes; the checked native artifact and generated binding remain
required. Input live-state resolution, full-group execution, signing and
business predicates retain their separate checks. No proof-admission blocker is
removed. Conditional-path support, source/interface provenance and stable
admission remain separate from the complete-program fixtures below.

`tests/selection_bundle.rs` covers source projection, generated contracts,
native checking, packed materialization, nested data and rebound mutations;
`tests/bounded_cell_selection.rs` compares fixed-layout reconstruction with the
independent reference model. Existing adapter and CLI ProtocolBundle regressions
retain the non-selection v1 boundary.

The [complete settlement bundle fixture](../tests/selection_bundle/settlement.rs)
adds generated bindings for all four participants, exact Type-hash deployment-line
admission, canonical packed-byte comparison and all-group VM execution at 1/3/6
orders. Local input/dependency resolution checks reject substitutions, while
business amount predicates remain enforced by the Scripts. The [evidence ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#executable-fixed-mixed-role-settlement)
separates its observations from live-chain evidence and final stable admission.

The [nested migration bundle fixture](../tests/selection_bundle/migration.rs)
compares native, generated TypeScript and independent Molecule nested vectors,
puts generated bytes back into the complete transaction, checks canonical adapter
bytes and executes migration, Registry and Lock groups at 1/3/8 documents.
Malformed outputs, changed SDK bounds, substituted histories and late-invalid
notes reject at their respective codec, binding, resolution or runtime boundaries.
Its [record-list counterpart](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#record-list-migration-and-authoring-comparison)
compares complete named/explicit-bound programs at O0/O3 with identical canonical
transactions, checks record revision identities and exercises both the exact
aggregate budget and over-budget rejection. These fixtures retain every stable
admission blocker.

The complete-program fixtures also publish frozen per-case cycles, observed
stack and guest pages, ELF, witness/transaction bytes and occupied capacity in
the [resource ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#frozen-complete-program-resource-fixtures).
The remaining acceptance audit must check source/role/layout provenance,
enclosing memory effects, field-value provenance, dynamic field-view proofs,
stable builder/source admission, final gates and review against the
[completion ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#completion-ledger).
Primitive VM execution alone does not satisfy those obligations.

Executable selection derives protected membership through the bounded complete
source scans; it does not accept a witness-supplied index list as authoritative.
The reference proposal validator separately tests omissions, insertions,
duplicates and source substitution. Adding an optional proposed-index source
API is not a prerequisite for this full-scan profile and would require its own
checked integration if introduced.

The variable-payload phase remains part of #49, not a follow-up substituted for
its completion. It must combine this selection/authority/correspondence
contract with #48's canonical nested codecs, freeze derived whole-Cell and
aggregate byte bounds before positive lowering, and execute a complete batch
transition. It cannot treat dynamic records as opaque equal-sized bytes.
General allocation from #47 is not a prerequisite for either selection phase.

The reference tests cover heterogeneous layouts, wrong custody/layout,
proposed-list omissions/insertion/duplicates/source confusion, shared-output
reuse, empty/maximum/late-invalid sets, independent budgets, explicit lifecycle
shapes, foreign-authority rejection and all 576 input/output permutations of a
four-Cell transaction. They intentionally do not claim source typing, VM,
independent machine proof, builder parity, all-group execution or resource
acceptance. Those remain mandatory in the
[completion ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#completion-ledger).

The fixed mixed-role and nested batch programs now have source, generated SDK,
ProtocolBundle byte comparisons and all-group VM fixtures. Their substitutions
and independent mutations are evidence toward completion, not a waiver of
complete source/scan/correspondence/lifetime proof, frozen maximum/late-invalid
resources, review or the applicable dev/backend/ci gates. No alternate transaction
serializer is introduced by the reference model.

# CellScript Source Contract

Edition 2027 is the sole source-language contract. Every package explicitly
sets `edition = "2027"`; unknown or missing values are errors. Standalone native
and language-service defaults use the same contract, and browser callers pass
it explicitly.

Source semantics are `cellscript-source-semantics-2027-0.33-authoring-v9`.
Ordinary statements end with `;`; a final admitted expression without `;`
provides a tail value. Lists require commas. Construct ordinary values with
`new Type { ... }`; generic calls use `function::<Type>(...)`. The optional
`verification` marker and `#[capability(...)]` spelling were removed; lifecycle
permissions use `has ...`. Ordinary `resource`, `action`, `lock`, `require`,
value types and lifecycle operations share one checked syntax kernel. Type
group artifacts select `action` declarations. An artifact separately states
its execution context, its Cell-role coordinate system, its coverage
obligations, any fixed layout, and its dispatch. An explicit Lock-group artifact
selects an ordinary `lock`, preserving protected/read provenance, Boolean result
and witness placement. See the [authoring grammar](CELLSCRIPT_AUTHORING_GRAMMAR.md) for the
admitted combinations. Every action and Lock parameter has explicit provenance,
using prefix roles or `read name: T` for CellDep views. Markerless entry sources
receive E1032 after resolved boundary checks; ordinary helper arguments remain
unqualified. Named Cell outputs
use inline `output name: Type` ports. Removed postfix source markers and named
arrow outputs receive E1032. Ordinary value and anonymous Cell returns remain
separate contracts. Type validity predicates use `validity { ... }`.

Named unsigned bounds retain a source reference until lexical constant
resolution, then normalize to the existing literal types and policy limits.
Supported arrays, named `data_shape`/`collections`/`cell_roles` bounds and closed
builtin numeric type arguments share that rule. Only unsigned scalar literals
and constant-reference chains are admitted; calls and computed expressions do
not acquire compile-time execution. Declaration widths, visibility, reference
depth and the original domain-specific limits remain checked. See the
[named-bound grammar](CELLSCRIPT_AUTHORING_GRAMMAR.md#named-unsigned-bounds).

## Artifact and interface contract

The native convergence candidate uses metadata 79, typed semantics and lowering
records v16, semantic foundation v8 and package interface v7. The native,
editor and canonical distributed WASM consumers use the same authoring-v9
identity; observed pairing checks and final delivery status are recorded in the
[implementation record](reports/0.36/SYNTAX_CONVERGENCE_IMPLEMENTATION.md).
Verified Type group entry
contracts record the selected binding and coverage policies. For fixed
coverage the independent checker matches the exact group bounds in the ELF
against the resolved roles. Structured successor dispositions retain their
entry and CFG bindings for path-sensitive lifecycle coverage. Lower-level
consume, destroy and creation occurrences retain their typed block, instruction
index, opcode and local binding; their envelope cannot acquire a complete
replacement policy through a shared role name. Package interface v7 also retains
declared Cell identity policies and receipt claim result types. Changing either
policy changes the interface identity and is an incompatible change. Receipt
result declarations designate resource/shared Cells; an omitted result keeps
the separate scalar runtime-required boundary.

Fresh creation and the unique operations may derive an omitted identity request
from the resolved Cell declaration. Explicit requests must still match; the
unique operations reject a non-unique declaration. This source convenience
does not supply missing Type ID, Lock, capacity or retirement-absence policies.
The effective policy remains concrete in typed operation/disposition records.

Creation/pool Type Script labels follow the actual resolved role membership.
Transaction-absolute bindings with no enforced membership retain
`unconstrained-by-this-relation`; a nominal schema is not Type-group evidence.
Native expansion and hover share a policy-category projection, with enforcement
and builder evidence displayed separately. Metadata-only analysis validates
type capacity-floor declarations without requiring artifact constraints that
have not been produced; artifact metadata still requires the matching CKB floor
summary.

Public items
with omitted visibility use `implicit-public`. Public value layouts require
explicit fixed, serializable, non-linear parameters.

Aggregate shorthand and matching explicit group sums use one resolved field
domain. Raw u128 helper evidence requires exact 16-byte storage and coverage of
the selected entry's accepting paths; a module-wide union of helper accesses
is insufficient. Invariant schema dependencies survive entry pruning. Delta
arguments retain their runtime-required boundary until typed operand binding
is available. The explicitly selected fungible profile has a structural IR
entry marker and conditional owner-or-conservation ProofPlan records, separate
from unconditional source equality. See the
[aggregate evidence boundary](CELLSCRIPT_AUTHORING_GRAMMAR.md#scoped-aggregates-and-executable-evidence).

Deployment records use `cellscript-deployed-v2` with numeric version 2.
Generated action builders use `cellscript-generated-action-builder-v1`.
Their exact schema identities, the source edition, compiler release, target,
assurance policy, witness payload and placement are independent contracts.
Readers admit one current shape; producers and consumers must agree on it.

## Packages and tools

`Cell.lock` binds the reviewed manifest/source graph. Only explicit `cellc lock`
or dependency update operations select versions and write pins. Build/check/test
validate the existing graph; `--frozen` additionally forbids network and writes.

Native metadata, LSP workspace analysis and hover share manifest-aware inputs.
Browser exports use the same grammar and analysis but return the bounded
`cellscript-browser-summary-v1`, omitting native typed/verified records.
An invalid package cannot produce successful semantic hover metadata through a
standalone compiler fallback. WASM, Registry, generated builders, adapters and
project examples use the same current interface and record vocabulary.

## Verification

Behavioral tests cover explicit and concise spellings, field preservation,
resource disposition, source/Cell bindings, runtime positives and negatives,
unknown contracts, hash-rebound artifact mutations, package locks and consumer
builds. Fixtures are current compiler output, independently checked where the
artifact boundary applies. The `commit`, `dev`, `ci` and `backend` gates keep
their documented scopes; none imply publication or production acceptance.

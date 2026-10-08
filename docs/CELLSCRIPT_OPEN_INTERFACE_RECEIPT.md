# Checked compatible-interface receipt contract

Status: draft implementation input for #28, following the
[open-participant plan](CELLSCRIPT_OPEN_PARTICIPANTS_PLAN.md). Implementer: Codex.
Independent security reviewer: unassigned. This document does not admit a source
handle, prove executable template instances, or authorize a deployment.

## Interface identity and evidence

The interface parameter `I` denotes a resolver-owned checked package interface
contract. Its source name is qualified by its defining package/module; a local
struct, raw Script, package display name or boolean cannot supply that identity.
Dependency aliases resolve to the same defining identity. Same-width values from
different owners remain distinct. Resolution consumes actual ELF, metadata,
lowering and source-map bytes through the standalone checker before introducing
an interface contract into the source type environment.

Keep the existing v3 interface wire/digest rules and historical exact-handle
receipts unchanged. Recompiling previously pruned public functions now retains
their source declarations and consequently corrects the new declaration hash. A new
`cellscript-open-interface-receipt-v1` receipt separately commits to the checked
effective projection and its exact artifact/deployment. The policy's required
interface identity is the required effective contract digest, rather than a
producer compatibility flag or the candidate's private implementation layout.

The projection must retain every public declaration and distinguish declarations
from actual executable entries. It contains qualified nested nominal layouts,
field order/offset/width, enum tags/payloads, generic parameters and symbolic
shapes, universal declared ability guarantees, source signatures, actual entry
and dispatch availability, inferred effects, Cell capabilities, witness/data
codecs, target profile, temporal and runtime ABIs, and builder requirements.
Private implementation details enter artifact identities, not compatibility by
coincidental layout size. Missing evidence for a projected claim rejects receipt
construction; adding a metadata field alone cannot create evidence.

Uninstantiated generic declarations must have independently checked symbolic
contracts, including parameter occurrence and ability guarantees. They cannot
be marked executable. Concrete instances additionally require the checked
substitution/layout/call ABI and their qualified declaration owner. Universal
behavioral or source-to-machine equivalence is not inferred from either record.

## Directional compatibility

Compare a required baseline with a candidate, not two unordered hashes. All
required nominal identities, serialized layouts, enum tags, callable sources,
parameter/return contracts, witness tags/codecs and builder inputs must remain
present and agree. Candidate-only declarations may be added if they do not
change any required ABI or dispatch tag. Unknown changes reject.

Inferred effects may only become more restrictive under an explicitly enumerated
effect relation. Cell ownership/capability changes are not silently treated as
effect improvements. Generic constraints cannot be strengthened, and abilities
required by the baseline cannot disappear. Target, VM, source/witness codec and
temporal/runtime contract changes require a separately admitted contract.
Interface compatibility makes no statement about application behavior, circuit
soundness, peer acceptance or deployment authority.

Admission checks every catalog member's actual four-file bundle and complete
receipt against this contract. One valid Merkle selection cannot establish that
all other members were checked. Keep raw policy membership construction separate
from a privately constructed checked admission result.

## Deployment and immutable authorization

Reuse the bounded [policy wire](CELLSCRIPT_OPEN_HANDLE_POLICY.md): 1–32 canonical
members, depth five and a 656-byte selection. Each member is exact after final
transaction construction, including complete Script args and Lock/Type/verifier
role, artifact bytes, deployment OutPoint, network context, status and sequences.
Data-hash deployments bind their code hash to the checked artifact. Type-hash
deployments additionally bind an authenticated deployment line/history tip;
Type identity alone cannot authorize replacement bytes or a different OutPoint.

The current Script obtains its authorized root from committed code/args or an
explicitly controlled state transition. The witness cannot authorize its own
root. The initial runtime binding requires a matching raw direct code CellDep
and an independently identified, unique resolved dependency with the selected
data/Type identity. Raw and expanded indices are not assumed equal. Raw OutPoint
presence, resolved code bytes and complete selected Script identity are separate
checks. Consensus supplies dependency liveness; receipt discovery does not.

Exact mode requires the exact receipt. Compatible mode requires independently
checked directional admission. Active status and closed admission floors are
checked against the immutable snapshot. Package/version downgrade and Type-hash
deployment history are separately validated, not inferred from an admission
sequence. A later Registry yank cannot retroactively modify an unchanged root.
Live revocation would need an authenticated policy-state contract and is outside
this initial snapshot model. Network genesis is an admission/build context;
runtime cannot claim a genesis syscall that CKB does not provide.

## Admission evidence

H1 uses these host-side limits before any receipt parsing/traversal: each bundle
retains the standalone checker's 4 MiB per-file ceilings; the required bundle
and all catalog bundles together are at most 16 MiB. An interface has at most
256 types, 128 callables, 256 constants, eight parameters per generic declaration,
64 fields per type and 32 variants per enum. Qualified identifiers/type text
are at most 512 bytes, symbolic nesting is at most 16 and expanded traversal
visits at most 4,096 nodes. Overflow, recursion or unknown representation rejects.
These host limits do not change the existing 656-byte runtime selection format.

The H1 prototype must retain two implementations of one required interface and
a known incompatible implementation, with exact and compatible selections.
Rebound mutations cover nested ownership/layout, missing executable entries,
generic shapes/constraints, codecs/effects, profile, Script role/args, artifact,
raw/resolved dependencies, Type history, network, status/floors and conflicting
catalog members. H2 subsequently adds nominal source values and runtime checks;
R1/R2 add open roles and byte-identical all-group transaction construction.

Use explicit byte/count/recursion limits before parsing or traversing receipts.
Runtime cardinalities remain the frozen policy bounds; no proving key, prover,
network lookup or unbounded allocation enters ordinary compilation. Resource
ceilings, checker/producer agreement, SDK vectors and editor/WASM behavior must
be fixed by executable implementation, not asserted complete by this draft.
Independent review remains a final stable-admission requirement.

## Implemented declaration prerequisite

With native `CompileOptions::source_contracts = true` or package
`cellc build --source-contracts`, the producer emits optional
`cellscript-generic-declaration-catalog-v1`
and `cellscript-nominal-declaration-catalog-v1` records inside typed semantics
v9. These catalogs retain the pre-optimization API, declaration owners, import
scopes, ordinary concrete layout bindings and absent templates. Source visibility
and declared effects remain distinct from checked entry availability and inferred
effects. The browser metadata path does not emit these heavy catalogs.

The independent checker verifies bounded syntax/scopes, concrete field order,
qualified field types, enum tags/payloads, generic instance agreement and public
source completeness. `InterfaceInspection::validate_symbolic_declarations`
separately checks public type/signature closure and universal value-ability
claims against parameter minima and symbolic fields. It rejects recursive,
unknown or unbounded graphs, stronger nested generic demands and missing catalogs;
it does not prove generic function behavior. Its result cannot authorize a
policy root, deployment or open runtime value.

Oversized optional catalogs, ambiguous merged lowered owners and missing source
origins preserve the historical compilation/inspection boundary without new
catalog evidence. Such bundles cannot pass the symbolic open-interface
prerequisite. Dynamic declarations remain inspectable; a complete fixed-layout
receipt must additionally require proven widths/offsets and codecs. The existing
202-byte exact-handle wire, v3 interface schema, global v9 lowering/typed schema,
source-map v2 and package/toolchain versions are unchanged. The artifact cache
advances to `project-source-set-v64-0.32-source-declaration-contracts`.

Full receipt projection, directional comparison, checked catalog admission,
nominal resolver values and CKB enforcement are still required. These declaration
checks do not complete H1, H2, #28 or #29, and independent review remains pending.

## Checked module projection prerequisite

`interface::project_bundle` applies a 4 MiB ceiling per actual artifact, metadata,
lowering and source-map file, and 16 MiB per bundle before parsing. The private
`CheckedModuleProjection` can also be constructed from an actual
`InterfaceInspection`; its retained original byte lengths receive the same
ceilings. A serialized projection cannot be deserialized into this checked
value. Its artifact report retains the exact checked artifact/sidecar identities
separately from its canonical module API identity.

The projection requires both bounded source catalogs where applicable and the
universal symbolic prerequisite. It follows root public type/signature/constant
type references into qualified nested nominal declarations, preserving source
field/variant order, binder constraints, capabilities and identity policies. It
retains complete checked layouts for those nominal owners and independently
checked instantiated arguments. Effective callable facts include actual inferred
effects, ordered parameters, fixed Cell bindings, external dispatch tags and
Script/witness placement ABI. Source callable declarations remain present when
optimization prunes their runtime entry; retained helpers are explicitly distinct
from external dispatch.

`check_required_contracts` compares every required projected key against a
candidate and allows additional candidate keys. It conservatively requires exact
effect and binder-constraint agreement; it does not yet implement safe effect
weakening or binder-constraint relaxation. Optimizer levels can retain different
helper/layout evidence, so equality of projection identities across optimizer
levels is not promised. Body predicates are outside this projection: two checked
artifacts can have identical API projections and different behavior.

This is a module coherence prerequisite, **not the H1 open receipt**. Frozen
package/module ownership, complete builder requirements, independently checked
codec and executable availability obligations, exact deployment/network/history
binding and whole-catalog admission remain required. Source constants retain
types, not independently proven values. Runtime errors/unsupported paths and
transitive helper behavior are not removed merely by projecting an entry. No
source equivalence, successful peer execution or security review is inferred.

## Explicit source evidence mode

Ordinary builds omit these additional source catalogs and retain the existing
metadata size budgets. Their declared public API still comes from the original
source, including optimizer-pruned functions. Independent artifact inspection
continues to support historical/default bundles; module projection requires the
complete catalogs and rejects their absence. Requesting the catalogs changes
sidecar identities, while preserving the ELF and declared API at O0–O3.

`cellc build-plan --source-contracts` uses the same build-unit identity and cache
mode as the corresponding build. The optional selection bit is serialized only
when true; ordinary v1 plan serialization remains unchanged. Native cache keys
include the bit and use `project-source-set-v65-0.32-optional-source-contracts`,
so alternating default and source-evidence builds cannot reuse each other's
metadata. The browser summary does not emit these catalogs and explicitly
rejects a programmatic source-evidence request. This option only supplies
independently checkable inputs; it cannot authorize a policy root or deployment.

## Native frozen-source prerequisite

`package::frozen_interface::compile_module` constructs a private
`FrozenPackageModule` from an actual package directory, existing version-5
`Cell.lock`, selected pinned environment and real source entry/artifact. It
forces offline runtime resolution, ELF and source catalogs, rejects a planned
lock override, and does not repin or write the lock. Before and after native
compilation it compares the actual root/dependency manifests, source hashes,
module ownership, paths and lock bytes. Parsed declarations are rebuilt and
compared with the emitted source catalogs before independent four-file checking.
Exported context JSON cannot reconstruct the private checked value.

The versioned context binds exact package sources, compiler release, chain
identity, defining module owners and package-relative source paths. Registry/Git
sources retain their pinned origin/revision. Local source snapshots exclude
physical directory names; dependency aliases share the actual selected defining
package. Context identity describes this exact source closure, not a publisher
authority, stable `I` family, upgrade line or behavioral equivalence. The stored
bundle is immutable through this API.

The static native preflight limits files to 4 MiB, total input bytes to 16 MiB,
packages to 32, source modules to 256, directory entries to 4,096 and directory
depth to 16. Source roots/entries must remain inside their package; source
symlinks and nonregular input files reject. This preflight runs before the
ordinary resolver hashes the selected package trees. The native filesystem is
not sandboxed: snapshot comparison detects source changes, but upstream native
re-reads do not provide allocation guarantees against concurrent hostile file
replacement. These are source-input limits, separate from receipt wire limits.

Focused tests cover optimization-independent context identity, actual bundle
checks, portable local snapshots, defining dependency aliases, changed
dependencies, missing/stale locks and environments, invalid entry/target requests,
oversized files, excessive module count/depth and Unix symlink loops. Whole H1
codec/executable availability, stable package/interface identity, deployment
history, all-member admission, H2 runtime handles and #29 remain outstanding.

## Finite Type-policy receipt prerequisite

The [finite checked policy receipt](FIXED_POLICY_RECEIPT.md) now composes actual
private module/codec/code-origin/target proofs with the checked declared API,
entry contract, original four-file byte hashes/lengths and artifact report.
Native code-catalog v3 requires it for every candidate. It covers only the
unit/scalar/flat-unsigned-Cell Type-policy profile; public constants reject and
generic declarations remain non-executable. Its separate finite receipt schema
is not the complete `cellscript-open-interface-receipt-v1` proposed above.

Exact input rechecks detect bundle, transaction, selected-index and complete
Script substitution after construction. Directional required/candidate matching
uses actual privately checked module proofs and permits only their supported
conservative relation. Source publisher authority, version/status/floors,
authenticated Type history, active chain context, immutable authorization,
complete general public codecs and H2 remain required. No full H1, source handle,
behavioral equivalence, peer execution or stable admission is claimed.

## Finite declared-policy field binding

The [native code-policy binding](FROZEN_CODE_POLICY.md) matches all finite receipts
against a separately declared authorization-set wire snapshot, binding required
API, pinned genesis, checked target/runtime and every candidate's receipt,
interface, ELF, complete Script and exact creation OutPoint. Exact/compatible
selection returns host evidence for the real candidate and rejects declared
yank/floor violations and later input/source changes. This data2 Type-policy
path rejects Type-hash history without evidence. Snapshot status/sequence choices
are not verified Registry/version facts. Root authority, full admission, nominal
source handles and H2 remain required.

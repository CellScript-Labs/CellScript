# Compatible-open handle design record and threat model

Status: versioned design record v1 (`cellscript-open-handle-design-v1`) for
issue #28, consolidating the implemented authorization-set boundary. The
implemented portion covers the host wire codec, finite receipts, native
policy/source bindings and transaction snapshots. Nominal imported-interface
handle witnesses and bounded membership/dependency helpers are implemented,
including roots read from committed current-Script args. Complete general
receipts and integrated H2 admission remain unfinished. Bounded Type-ID
lineage is now checked from actual creation/replacement transactions; this is
policy-authorized byte consistency, not independent chain inclusion evidence. The maintainer waived #28 and #29's external
audit requirement on [2026-10-11](CELLSCRIPT_0_32_IMPLEMENTATION.md#maintainer-decision--2026-10-11-28-external-audit);
technical acceptance and independent artifact checking remain required. Companion records:
[receipt contract](CELLSCRIPT_OPEN_INTERFACE_RECEIPT.md),
[authorization-set wire](CELLSCRIPT_OPEN_HANDLE_POLICY.md),
[participants plan](CELLSCRIPT_OPEN_PARTICIPANTS_PLAN.md).

## Protected claim

A compatible-open handle selection asserts: one member of a specific
authorization set, whose exact four-file artifact bundle, complete selected
Script (code hash, hash type, concrete args), deployment OutPoint and network
context were independently checked, satisfies the required interface contract
of the consuming package under the declared selection mode. It does not assert
behavioral equivalence, peer acceptance, Cell liveness, Registry freshness, or
authority over any Cell. Compatibility is directional (required baseline to
candidate) and never symmetrical.

## Bounded native ProtocolBundle integration

`check_protocol_bundle_with_open_handles` accepts private frozen consumer
modules and source policies, actual final Molecule transaction bytes, and every
input/dependency snapshot. The existing artifact-only constructor rejects open
handle parameters when these proofs are absent. Serialized `verified` fields
cannot construct a native source policy. The standalone checker compares all
concrete skeleton inputs, outputs, dependencies, headers and witnesses with the
actual bytes before selecting a receipt. The adapter uses its existing SDK
serializer and requires identical raw and complete transaction hashes.

This profile admits one `ScriptHandle<I>` witness per Type consumer, with the
32-byte authorization root committed in an actual input Script's args. All
policy members retain the checked finite Type-policy profile. The consumer
must unconditionally require membership in its typed entry block. A selection
also requires that block's action to be the actual independently checked
single Type entry with the canonical `WitnessArgs.input_type` placement. A
different declared action cannot stand in for the running entry. The selection
binds the provider's exact admitted module, source receipt, complete Script,
network and actual direct code dependency. The complete selected peer group
and its canonical policy witness are derived from the transaction. At most
32 bindings, 64 direct dependencies and 256 inputs/outputs/witnesses are
accepted; input bytes are bounded before parsing to 4 MiB each and 16 MiB total.

`ckb::require_open_script_participant_from_args(dep, handle, subject)` adds a
read-only complete-Script observation using `InputView<T>` or `OutputView<T>`.
Runtime decoding preserves transaction-wide versus group-relative indices.
Bundle admission currently requires a constant view index and an entry-block
constructor, resolves `T` through the consumer's defining-owner catalog,
compares its declaration with the independently checked provider layout and
checks fixed data length. Pruned consumer layouts are not invented from a
display name. The report retains the source, relative and transaction indices,
qualified type name, schema hash and complete peer-group indices; adapter
materialization rechecks the concrete mappings.

The applicable claim boundaries are:

Each open binding serializes these assignments in `claim_boundaries`. They
identify responsibility, not successful execution. Materialization rejects a
rebound report which relabels peer, dry-run or consensus claims as local checks,
or grants lifecycle authority through a foreign observation.

| Claim | Boundary |
| --- | --- |
| Membership, committed root, selected code bytes/OutPoint, optional observed Script identity | `checked_by_current_script` |
| Selected participant's own predicates, data decoding and lifecycle rules | `required_peer_script_group` |
| Every Script group succeeds over the materialized bytes | `checked_by_bundle_dry_run` (not established by the offline report) |
| Input liveness, network consensus admission and committed transaction | `guaranteed_only_by_ckb_consensus` |

Foreign observations do not authorize local consumption or creation. An
admitted canonical witness can still fail the participant's own predicate.
`open_protocol_admission_binds_private_receipts_to_complete_transaction_bytes`
in `tests/frozen_interface.rs` covers actual private receipts, generated
builder manifests, SDK byte parity and real SDK secp256k1 signing at O0–O3.
The signed transaction executes every group in CKB-VM; the unsigned transaction
fails its Lock. A signed wrong peer argument fails the selected Type group,
while a post-signing witness change fails the Lock group. The adapter also
rejects changed bytes against the original signing preparation. These local
fixtures do not establish live-network observations or transaction inclusion.
`tests/exact_script_handles.rs` covers runtime source/identity substitutions.
General Lock/verifier admission and general codec closure remain unfinished;
these bounded tests do not close #28 or #29.

### CLI and generated witness transport

`cellc protocol bundle check bundle.json --open-admission open.json --output
report.json --json` reconstructs the private native proofs before producing
the ordinary ProtocolBundle report. Both manifests confine their file paths
to their own directory. Compilation uses existing lockfiles, pinned environments
and offline resolution; it cannot repin a dependency. A serialized receipt ID
or `verified` flag cannot replace recompilation and independent artifact checks.

The open input uses schema `cellscript-protocol-open-admission-input-v1`:

| Field | Required content |
| --- | --- |
| `transaction_file` | Relative path to the complete canonical Molecule transaction |
| `inputs`, `dependencies` | Snapshot objects containing lowercase `0x` hex `out_point` (36 bytes), complete `output`, and `data` |
| `bindings` | 1–32 consumer/parameter/participant bindings |
| Binding `consumer_module`, `required` | Source module requests: relative `package`, pinned `environment`, `entry_kind` (`action`, `lock`, or `artifact`), `entry`, `opt_level` (0–3), optional `target_profile` |
| Binding `candidates` | 1–32 objects with a module request, `creation_transaction_file`, `output_index`, complete hex `selected_script`, and optional Type-ID `history` |
| History entry | Relative `transaction_file` and `output_index`, in genesis-to-tip order, at most 32 steps |
| Binding `policy_header`, `policy_members`, `selection` | Canonical existing wire encodings in lowercase `0x` hex; every member is rechecked against its actual source receipt |

The request JSON is limited to 16 MiB. Decoded transaction, snapshot and policy
inputs share a 16 MiB budget with a 4 MiB individual ceiling; source compilation
and checked catalogs retain their own documented input budgets. Unknown fields,
invalid counts, escaped file paths, incompatible sources and substituted final
bytes reject. The CLI/native parity regression compares the entire report at
O0–O3 after independent CLI reconstruction.

Generated TypeScript exports `encodeOpenHandleWitness(selection)` for the exact
656-byte selection plus `CSARGv1` envelope, and
`openHandleWitnessFromCheckedBundle(report, consumer, parameter)` to extract a
selection matching the generated artifact, declared parameter and committed
root. The latter consumes native admission evidence; JavaScript serialization
does not create a private checked receipt. Its Node regression compares all
664 payload bytes with Rust/SDK materialization and rejects malformed lengths,
magic, missing/duplicate bindings and a changed root. Initial policy construction
uses the existing native source/catalog APIs; the CLI never infers authorization
from Registry discovery.

## Parties and trust boundaries

| Party | Controls | Cannot grant |
| --- | --- | --- |
| Consumer author | Required interface `I`, source closure, lockfile | Runtime facts, deployment history |
| Policy maintainer | Authorization-set members, status, floors, sequences | Artifact validity (checker recomputes) |
| Artifact producer | Source, compiled bundle | Own admission; `compatible` bits are not evidence |
| Transaction builder | Final transaction bytes, signing | The policy root; post-build substitution |
| Registry | Discovery records only | Consensus authority, authorization, freshness |
| Current Script at runtime | Committed code/args, syscalls | Peer authorization, genesis identity |
| CKB consensus | Dependency resolution, group execution | Semantic compatibility |

The policy root is authorized by the current Script's committed code/args or
an explicit controlled state transition. A witness carrying a selection proof
never authorizes its own expected root; the expected root is supplied
separately and retained in the immutable membership result.

## Design decisions

1. **Nominal interface parameter `I`.** `I` denotes the resolver-owned checked
   package interface: its defining package/module owner, pinned source closure
   and effective checked projection digest. Raw Scripts, local structs,
   package display names and producer flags cannot supply it. Implemented for
   host receipts via source receipts and projection identities. Nominal
   `ScriptHandle<I>`/`VerifierHandle<I>` witnesses use imported module markers;
   complete receipt-to-consumer admission remains H2.
2. **Bounded authorization set.** 1–32 canonically ordered members over a
   depth-five tree with a 656-byte selection; duplicate receipts (including
   same hash with differing fields) reject at wire construction. Implemented.
3. **Immutable authorization root.** Snapshot semantics: a later Registry yank
   cannot retroactively modify an unchanged root; changing the expected
   authenticated root invalidates old selection witnesses. Live revocation
   requires a separately authenticated policy-state contract and is out of
   scope for this record. Constant roots and exactly 32-byte current-Script
   args are supported by the runtime helpers; policy-state updates remain H2.
4. **Exact and compatible selection.** Exact mode requires the named receipt
   present in the set. Compatible mode requires an active member within the
   closed floor/policy-sequence interval plus independently checked
   directional admission; the source profile adds SemVer precedence with
   stable-major compatibility, and build metadata is committed without
   ordering. Implemented as declared immutable snapshot facts; they are not
   authenticated Registry/version facts.
5. **Type-hash deployment binding.** Wire members retain deployment line,
   history tip and deployment sequence separately from policy admission
   sequence. Type identity alone never authorizes replacement bytes or a
   different OutPoint; two receipts cannot claim different bytes at one
   OutPoint or competing tips at one line/sequence. The source/history constructor independently checks 1–32 built-in
   Type-ID creation/replacement transactions, preserving custody and consuming
   each exact predecessor. The root commits the checked line, tip and sequence.
   No supplied history digest alone can admit a Type-hash member.
6. **Compatibility axes.** Serialized layouts, callable contracts, witness
   codecs, effects, capabilities, target profile, runtime ABI and builder
   requirements are compared field-wise under a conservative relation with
   candidate-only additions permitted. Unclassified changes reject. The
   implemented finite profile covers unit results, bounded scalar parameters
   and flat unsigned Cell layouts. Public constants require independently
   re-evaluated value evidence; unsupported values reject. Uninstantiated
   generics stay declaration-only.

## Threat model

| # | Adversary action | Enforcing boundary | State |
| --- | --- | --- | --- |
| T1 | Producer asserts compatibility without evidence | Checker recomputes every receipt field from the actual bundle; no producer flag is read | Enforced (finite profile) |
| T2 | Builder substitutes artifact, CellDep, Script or transaction after checking | `check_unchanged_inputs` rechecks bundle/RawTransaction/output index/Script; direct-dependency snapshot binds exact creation OutPoint and every supplied Cell | Enforced host-side; runtime helpers bind actual code bytes and exact OutPoint under a bounded direct-dependency profile; full transaction admission remains H2 |
| T3 | Witness authorizes its own policy root | Expected root supplied separately from the membership proof; runtime roots are compile-time constants or exactly 32 bytes of committed current-Script args | Wire separation and constant/args-root membership enforced; controlled-state updates remain pending |
| T4 | Downgrade to an older or incompatible-major version | SemVer precedence for selectable members, floors, exact-mode receipt pinning; unselectable history stays fully checked | Enforced as declared snapshot facts |
| T5 | Yank bypass or stale-snapshot confusion | Yanked/below-floor members committed but unselectable; immutable root ignores later Registry changes; changing the root invalidates old witnesses | Enforced |
| T6 | Duplicate or ambiguous code dependency at runtime | Raw versus resolved dependency indices stay distinct; data-hash lookup keeps the last match and Type-hash lookup rejects different-data duplicates, so ambiguous matching code/Type identities reject instead of guessing | Runtime rejects dep groups, more than 64 direct deps and duplicate selected code/Type identities |
| T7 | Cross-network replay | Header commits network genesis; native catalog rejects other pinned networks | Enforced |
| T8 | Replacement bytes under an unchanged Type hash | Member binds exact artifact hash, code OutPoint, deployment line and history tip together | Actual Type-ID lineage checked host-side; code bytes, Type identity and OutPoint checked on-chain |
| T9 | Duplicate or conflicting members in one set | Canonical member ordering plus duplicate/conflict rejection at construction, before any policy exists | Enforced |
| T10 | Rebound outer hashes hiding inner changes | Native binding rechecks every member against its private receipt; rebinding interface/bundle/outer hashes cannot smuggle layout, ABI, effect, profile, role/args or Script changes | Enforced (12-axis and substitution tests) |
| T11 | Oversized or algorithmically hostile inputs | Preparse ceilings (4 MiB per file, 16 MiB shared, 16 MiB source closure), bounded H1 counts (256 types/128 callables/64 fields/32 variants/16 nesting/4,096 traversal nodes), no proof-controlled allocation | Enforced |
| T12 | Demand live revocation from an offline snapshot | Explicit non-goal; requires an authenticated policy-state transition contract | Out of scope, documented |
| T13 | Claim behavioral equivalence from API compatibility | Body predicates are outside the projection; two artifacts with identical API projections and different behavior are both admissible | Non-goal by contract |

## Residual risks before stable admission

Unimplemented surfaces are residual risk until H1/H2 complete: the finite
profile excludes unsupported constant/codec shapes and executable generics, so
interfaces outside that profile cannot be admitted at all (fail-closed) rather
than weakly checked. CKB-VM helpers enforce membership, exact receipt selection, active status and
admission-sequence bounds against a constant or committed-args root. They bind
actual code data hash, Type hash where selected, and exact deployment OutPoint.
The direct-dependency profile rejects groups, ambiguity and counts over 64.
Complete interface receipt admission and historical-chain inclusion are not
inferred from this runtime check. Native filesystem reads are not sandboxed
against concurrent hostile file replacement; snapshot comparison detects
changes but provides no allocation guarantee. The bounded Type-policy
integration above establishes generated-builder/ProtocolBundle byte parity;
general Lock/verifier admission and codec coverage remain unfinished.
First-slice VM tests enforce a 3,000,000-cycle action ceiling and fixed
664-byte witness payload; a complete worst-case admission resource corpus
remains pending. External human review for #28 was waived on 2026-10-11; that
decision does not resolve these technical risks or claim an audit occurred.

## Evidence index

- Wire codec, vectors and tag freeze: [CELLSCRIPT_OPEN_HANDLE_POLICY.md](CELLSCRIPT_OPEN_HANDLE_POLICY.md),
  `crates/cellscript-artifact-checker/src/open_handle_policy.rs`,
  `crates/cellscript-artifact-checker/tests/open_handle_policy.rs`.
- Finite receipt construction and directional matching:
  [FIXED_POLICY_RECEIPT.md](FIXED_POLICY_RECEIPT.md),
  `crates/cellscript-artifact-checker/src/fixed_policy_receipt.rs`,
  `tests/policy_artifact_checker.rs`.
- Native policy/source bindings and selection:
  [FROZEN_CODE_POLICY.md](FROZEN_CODE_POLICY.md),
  `src/package/frozen_interface/code_policy.rs`, `tests/frozen_interface.rs`.
- Final transaction snapshots: [DIRECT_CODE_DEPENDENCY.md](DIRECT_CODE_DEPENDENCY.md),
  [DIRECT_TYPE_GROUP.md](DIRECT_TYPE_GROUP.md).
- H1 host limits and receipt projection prerequisites:
  [CELLSCRIPT_OPEN_INTERFACE_RECEIPT.md](CELLSCRIPT_OPEN_INTERFACE_RECEIPT.md).

## H2 first-slice decisions (frozen before source-syntax admission)

The plan requires the interface-parameter mapping to be frozen before any
`ScriptHandle<I>`/`VerifierHandle<I>` source syntax is admitted. These
decisions bind the implementation order that follows; the two class spellings
are reserved and fail closed at type validation, declaration registration and
independent nominal-catalog verification until each slice below ships.

1. **Interface parameter `I`** denotes the resolver-owned checked defining
   module interface identity (defining package/module, pinned source closure,
   effective checked projection digest) — the same identity the receipt
   contract already fixes. In source it will be spelled only through an
   explicit resolver-bound marker introduced with the parser slice: an
   imported module interface designation, not a local struct, raw `Script`,
   package display name, producer flag or boolean. Ambiguous unqualified
   markers never manufacture an interface.
2. **Bounded encoding**: a runtime handle value carries at most the existing
   656-byte selection witness and 32-member authorization-set identity from
   the frozen wire. No new runtime budgets, no witness expansion, no proving
   key or network lookup enters ordinary compilation.
3. **Class and role separation**: `ScriptHandle` and `VerifierHandle` are
   distinct nominal classes; the Lock/Type/spawned-verifier role belongs to
   the policy header, not the type. No coercion or retyping move from
   `Script`, `Hash`, `ScriptHash`, bytes or any other nominal exists or will
   be added; construction is only through versioned builtin helpers that
   consume the checked admission boundary, mirroring the exact-handle
   operand-flow jail.
4. **Runtime enforcement**: the expected authorization root comes from
   committed current-Script code/args or an explicitly controlled state
   transition, never from the same witness that carries the membership proof.
   Membership and selected-dependency checks execute on CKB-VM before any
   on-chain claim; a compiler-only compatibility check is not runtime
   evidence. Expected-root binding, helper admission and measured
   cycle/stack/witness budgets land with the runtime slices.

The marker slice landed after these decisions: an interface designation is a
plain module path in the handle's single type argument, validated by the
project resolver on the producer side (exactly one imported, loaded module —
never the current module, a local type, a primitive or a nested application)
and independently by the checker against the nominal-catalog scopes of the
checked source closure. Handle leaves carry copy/drop only, never occupy a
layout parameter, and reject in value positions (fields, callable parameters)
until the runtime encoding slice; identity use as a generic application
argument is admitted and projected. Bare spellings and user declarations of
the two class names keep their reserved-surface rejection.

The bounded encoding slice landed next: an action `witness` parameter of
handle type is the entry-ABI position for the frozen 656-byte selection
witness. The width is carried consistently through the producer's string and
IR width tables, the witness payload layout (fixed-byte pointer/length ABI,
compact adapter frame), the action parameter metadata, and the independent
checker's policy-ABI projection and adapter-frame expectation — the producer
and checker now derive the same compact adapter frame for a 656-byte handle
parameter. Every other parameter kind and callable class stays identity-only.
No versioned helper consumes the value yet, so the checker's operand-flow jail
holds: any operation touching a handle operand fails closed until the runtime
helper ships with its membership and selected-dependency enforcement.

## H2 versioned helper specification (implementation-ready, next slice)

The consumption helper's complete design is frozen here so the next session
implements without re-derivation. The on-chain runtime already provides
`__ckb_hash_blake2b_var` and `__cellscript_memcmp_fixed`; the exact-handle
requirement helper (`emit_runtime_exact_script_handle_requirement_helper`,
`src/codegen/runtime.rs`) is the frame/syscall skeleton to reuse.

**DSL surface**: `ckb::require_cell_dep_open_script_handle(dep: CellDepView,
handle: ScriptHandle<I>, expected_root: Hash) -> unit` (mirrors the exact
helper's three-argument shape; `VerifierHandle` gets the spawned-verifier
variant). The expected root is caller-supplied in this slice; binding it to
committed current-Script code/args is the frozen decision #4 follow-up.

**Selection wire offsets inside the 656-byte value** (magic `CSOHWv1\0`):
the header block starts at 8 and carries its own `CSOHPv1\0` magic, so class
sits at 16, role at 17, mode at 18 and member_count at 19 (each eight bytes
deeper than its in-header position); member at 196 (292 bytes; the member carries its own CSOHMv1\0 magic, so
status sits at 204 and hash_type at 205; complete_script hash at 324,
code_hash at 356, code OutPoint tx-hash at 388, output index at 484,
admission/deployment sequences at 212/220), canonical member index at 488 (1 byte), seven zero bytes at 489,
five sibling hashes leaf-to-root at 496..656.

**On-chain steps**: (1) structural checks — magic, class/role match the
helper variant, index < member_count, status active, mode tag sane;
(2) membership — recompute the member leaf
`H("cellscript-open-handle-member-v1\0" || u8(index) || member[196..488])`
and fold five levels
`H("cellscript-open-handle-node-v1\0" || left || right)` using the index bit
at each depth to order (slot index equals the canonical index for occupied
slots), then the root
`H("cellscript-open-handle-policy-v1\0" || header[8..196] || tree_root)` and
compare against expected_root, then enforce active status, admission floor/ceiling
and exact-mode receipt selection; (3) selected dependency — restrict the view to
CellDep, read canonical raw CellDeps through LOAD_TRANSACTION and require 1–64
direct entries without groups. Bind the selected raw OutPoint to member bytes
388..420 and 484..488. Scan every resolved dependency in that same direct order:
data2 selections require one matching data hash; Type selections require one
matching Type hash plus the exact admitted data hash. Both helper classes use
this code-dependency check. The Script guarding a code Cell is not its deployed
executable identity. Complete selected Script/args and interface admission
remain separate host receipt claims; this does not imply peer execution.
Every mismatch returns `CellScriptRuntimeError::OpenHandleInvalid`.


**From-args binding status**: the `..._from_args` variants (expected root
derived from the current Script's committed 32-byte args via LOAD_SCRIPT)
are implemented through typecheck, lowering, argument loading, runtime
generation, checker signatures and LSP. The CKB-VM regression now executes
normally at O0–O3: committed roots accept, while changed roots, 31/33-byte
args and selection substitutions reject. The previous failure was overlapping
eight-byte stack slots at offsets 822 and 828: LOAD_SCRIPT overwrote the saved
selection length. The aligned slots now follow the policy hash buffer without
overlap. Verifier tests cover both root sources with distinct Script/artifact
hashes, changed dependency bytes and wrong artifacts under valid commitments.
These tests establish the bounded helper behavior, not full H1/H2 admission.

**Measured budgets** (CKB-VM acceptance fixture, one-member compatible
selection, action with one handle witness parameter and one CellDep):
76,181 cycles at O0 and 76,176 at O1–O3 — frozen ceiling 3,000,000 cycles
for the verification action; witness payload fixed at 664 bytes (8-byte
entry ABI magic + the 656-byte selection). Those cycle measurements belong to
the original constant-root helper. The current helper frame is 992 bytes over
the entry wrapper's 5,376-byte frame, including current-Script scratch storage.
Current Script and verifier tests enforce the same cycle ceiling. The 64-direct-dependency
case repeats identical transaction hashes and cycle counts, rejects a matching
code identity in the last slot and rejects a 65th dependency. A valid resolved
dep group rejects with the open-handle error. The Script test checks the actual
generated payload length. The ceiling is an assertion in the VM
acceptance test, so regressions fail the suite rather than silently
consuming the policy budget.

The direct-dependency maximum fixture (`open_code_dependency_budget_covers_every_direct_cell`)
now measures 153,074 cycles at O0 and 153,069 at O1–O3, including all 64 raw
direct dependencies. Its selection fields are synthetic: this measures the
runtime primitive, not complete independent receipt admission. Hashes below
are CKB Blake2b-256 of the executable ELF (without the metadata trailer) and
canonical raw transaction, respectively; the test repeats the transaction and
cycle count and prints these identities with `--nocapture`.

| Optimization | Executable artifact | Raw transaction |
| --- | --- | --- |
| O0 | `74e73540276da3fb659c56102a5977a963c174f9bb686773f9a131d8e9da93a4` | `2af51dd71428d304fc9462484192583af6343431184e8b9acbf83ba21ed5b9da` |
| O1–O3 | `d1531417ff1bfab63fc45a534ab9b1d1f9957f94a04ca3172857731ecfd1a81d` | `6226b5fc3f44406e5709a62a16961fe12a704a1e651f15f44cd608d0b6d3a195` |

**Layer checklist**: typecheck arm beside the exact handles
(`src/types/mod.rs` ~7144), lowering to
`__ckb_require_cell_dep_open_script_handle`, `calls.rs` argument loading
(656-byte fixed-byte source + 32-byte root, exact pattern), the runtime
helper generator (frame layout, frame-resident hash input buffers for the
domain-prefixed leaves — 326/97-byte staging slots), checker jail exemption
naming exactly this target plus its signature checks, a new runtime error
code, LSP completion, and O0–O3 CKB-VM positives with substituted-root,
swapped-member, wrong-dep and malformed-wire negatives.

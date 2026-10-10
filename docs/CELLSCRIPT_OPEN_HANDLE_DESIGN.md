# Compatible-open handle design record and threat model

Status: versioned design record v1 (`cellscript-open-handle-design-v1`) for
issue #28, consolidating the implemented authorization-set boundary. The
implemented portion covers the host wire codec, finite receipts, native
policy/source bindings and transaction snapshots. Complete general receipts,
immutable on-chain root authorization, nominal source handles and CKB runtime
enforcement (H2) remain unimplemented. Independent security review remains
unassigned and required before stable admission. Companion records:
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
   host receipts via source receipts and projection identities; the nominal
   `ScriptHandle<I>`/`VerifierHandle<I>` source values remain H2.
2. **Bounded authorization set.** 1–32 canonically ordered members over a
   depth-five tree with a 656-byte selection; duplicate receipts (including
   same hash with differing fields) reject at wire construction. Implemented.
3. **Immutable authorization root.** Snapshot semantics: a later Registry yank
   cannot retroactively modify an unchanged root; changing the expected
   authenticated root invalidates old selection witnesses. Live revocation
   requires a separately authenticated policy-state contract and is out of
   scope for this record. Root binding to committed code/args remains H2.
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
   OutPoint or competing tips at one line/sequence. The native binding
   currently fails closed on Type-hash members because authenticated history
   validation is not implemented; the data2 profile is the only natively
   bound deployment mode.
6. **Compatibility axes.** Serialized layouts, callable contracts, witness
   codecs, effects, capabilities, target profile, runtime ABI and builder
   requirements are compared field-wise under a conservative relation with
   candidate-only additions permitted. Unclassified changes reject. The
   implemented finite profile covers unit results, bounded scalar parameters
   and flat unsigned Cell layouts; public constants reject (types proven,
   values not), and uninstantiated generics stay declaration-only.

## Threat model

| # | Adversary action | Enforcing boundary | State |
| --- | --- | --- | --- |
| T1 | Producer asserts compatibility without evidence | Checker recomputes every receipt field from the actual bundle; no producer flag is read | Enforced (finite profile) |
| T2 | Builder substitutes artifact, CellDep, Script or transaction after checking | `check_unchanged_inputs` rechecks bundle/RawTransaction/output index/Script; direct-dependency snapshot binds exact creation OutPoint and every supplied Cell | Enforced host-side; runtime recheck is H2 |
| T3 | Witness authorizes its own policy root | Expected root supplied separately from the membership proof; H2 binds it to committed code/args or controlled state | Wire separation enforced; on-chain binding pending |
| T4 | Downgrade to an older or incompatible-major version | SemVer precedence for selectable members, floors, exact-mode receipt pinning; unselectable history stays fully checked | Enforced as declared snapshot facts |
| T5 | Yank bypass or stale-snapshot confusion | Yanked/below-floor members committed but unselectable; immutable root ignores later Registry changes; changing the root invalidates old witnesses | Enforced |
| T6 | Duplicate or ambiguous code dependency at runtime | Raw versus resolved dependency indices stay distinct; data-hash lookup keeps the last match and Type-hash lookup rejects different-data duplicates, so ambiguous matching code/Type identities reject instead of guessing | Dependency facts pinned to CKB `f7fa4436`; runtime enforcement is H2 |
| T7 | Cross-network replay | Header commits network genesis; native catalog rejects other pinned networks | Enforced |
| T8 | Replacement bytes under an unchanged Type hash | Member binds exact artifact hash, code OutPoint, deployment line and history tip together | Wire fields enforced; authenticated history validation pending |
| T9 | Duplicate or conflicting members in one set | Canonical member ordering plus duplicate/conflict rejection at construction, before any policy exists | Enforced |
| T10 | Rebound outer hashes hiding inner changes | Native binding rechecks every member against its private receipt; rebinding interface/bundle/outer hashes cannot smuggle layout, ABI, effect, profile, role/args or Script changes | Enforced (12-axis and substitution tests) |
| T11 | Oversized or algorithmically hostile inputs | Preparse ceilings (4 MiB per file, 16 MiB shared, 16 MiB source closure), bounded H1 counts (256 types/128 callables/64 fields/32 variants/16 nesting/4,096 traversal nodes), no proof-controlled allocation | Enforced |
| T12 | Demand live revocation from an offline snapshot | Explicit non-goal; requires an authenticated policy-state transition contract | Out of scope, documented |
| T13 | Claim behavioral equivalence from API compatibility | Body predicates are outside the projection; two artifacts with identical API projections and different behavior are both admissible | Non-goal by contract |

## Residual risks before stable admission

Unimplemented surfaces are residual risk until H1/H2 complete: the finite
profile excludes public constants, executable generics and general codecs, so
interfaces outside that profile cannot be admitted at all (fail-closed) rather
than weakly checked. Root authorization is currently host-side only; a
compromised builder with unchanged inputs is detected, but nothing yet executes
the membership check inside CKB-VM. Native filesystem reads are not sandboxed
against concurrent hostile file replacement; snapshot comparison detects
changes but provides no allocation guarantee. Generated-builder and
ProtocolBundle parity on identical transaction bytes is not yet integrated.
Worst-case runtime cycles/stack/witness budgets for on-chain enforcement are
unmeasured. Independent security review is unassigned; the 2026-10-03 waiver
does not cover this contract.

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
code_hash at 356, code OutPoint tx-hash at 388, output index at 420,
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
compare against expected_root; (3) selected dependency — load the dep's
lock/type Script hash by the member role (field ids as the exact helpers) and
compare against the member's complete_script at 316. Every mismatch fails
closed with a new specific `CellScriptRuntimeError`.

**From-args binding status**: the `..._from_args` variants (expected root
derived from the current Script's committed 32-byte args via LOAD_SCRIPT)
are implemented through every layer — typecheck, lowering, argument
loading, runtime generator, checker signature and LSP — with the prologue
live-verified (committed Script size 85, staged root reading the committed
args, reloaded selection pointer and length byte-verified). One
control-flow defect keeps the helper failing closed with
`open-handle-invalid` before the membership recomputation despite the
verified inputs; it is tracked by an ignore-marked VM test carrying the
complete matrix. The constant-root variant remains the fully admitted
surface.

**Measured budgets** (CKB-VM acceptance fixture, one-member compatible
selection, action with one handle witness parameter and one CellDep):
76,181 cycles at O0 and 76,176 at O1–O3 — frozen ceiling 3,000,000 cycles
for the verification action; witness payload fixed at 664 bytes (8-byte
entry ABI magic + the 656-byte selection); helper frame 848 bytes over the
entry wrapper's 5,376-byte frame. The ceiling is an assertion in the VM
acceptance test, so regressions fail the suite rather than silently
consuming the policy budget.

**Layer checklist**: typecheck arm beside the exact handles
(`src/types/mod.rs` ~7144), lowering to
`__ckb_require_cell_dep_open_script_handle`, `calls.rs` argument loading
(656-byte fixed-byte source + 32-byte root, exact pattern), the runtime
helper generator (frame layout, frame-resident hash input buffers for the
domain-prefixed leaves — 326/97-byte staging slots), checker jail exemption
naming exactly this target plus its signature checks, a new runtime error
code, LSP completion, and O0–O3 CKB-VM positives with substituted-root,
swapped-member, wrong-dep and malformed-wire negatives.

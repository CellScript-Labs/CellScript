# Finite checked Type-policy receipt

`cellscript_artifact_checker::fixed_policy_receipt::check_fixed_policy_receipt`
consumes the actual four-file artifact bundle, canonical RawTransaction bytes,
code output index and complete selected Script bytes. It accepts no serialized
receipt or producer compatibility flag. Its private `CheckedFixedPolicyReceipt`
retains actual independently checked external codec, code-byte origin and target
selection proofs; exported JSON cannot reconstruct it.

Construction applies the existing six-input preflight before parsing: 4 MiB per
file, 16 MiB shared, and caller artifact/record/source-map ceilings. It requires
the independently checked Type-policy role and the finite
`policy-unit-scalars-nested-unsigned-cell-v1` external profile: unit results,
bounded scalar parameters and unsigned Cell layouts. Public layout declarations
may compose nested ordinary struct layouts — each nested member must resolve to
exactly one retained concrete struct layout and recursively satisfy the same
unsigned obligations (contiguous offsets, matched widths, depth ≤ 8, total
≤ 512 bytes); these declaration-level obligations are unit-checked in the
checker. Entry-bound Cell layouts stay flat: their field materialization evidence is
certified per direct scalar field. Nested public layouts retained through
entry-body construction are now admitted end to end: the fixed memory-copy
helper receives its own frame certification (owned instruction range, internal
loops allowed but no calls, no return/stack-register writes, at least one
return), and every call site proves its destination — the only definition in
the pre-call window is a stack-relative offset inside the caller's frame with a
positive constant length ≤ 512 bytes, disjoint from checked Cell data and the
received Cell pointer, with no incoming flow into the argument window. Public
constants still reject because the current projection proves their types but
not their values; an independent constant-value proof needs its own evaluation
contract. Uninstantiated generic
declarations remain declaration-only; they cannot claim an executable external
codec. Broader bundle inspection remains unchanged.

The record schema is `cellscript-fixed-policy-interface-receipt-v1`; its identity
uses domain `cellscript-fixed-policy-interface-receipt-id-v1` over canonical
record bytes. It contains the actual checked declared interface, entry contract,
module API/codec identities, exact code-origin/target identities, hashes and
lengths of every original bundle file, and the independent artifact report.
The report retains `ckb_vm_evidence = not-executed`,
`chain_evidence = not-provided` and no semantic-equivalence claim. JSON
whitespace changes therefore produce a different receipt even when the checked
API remains directionally compatible.

`check_required_contracts` compares the actual private required/candidate module
proofs. It preserves required keys, concrete codecs, effects and binder
constraints; additional candidate declarations can pass only under that checked
relation. A different predicate or exact deployment can satisfy the same API.
Compatibility does not imply behavioral equivalence or deployment authority.

`check_unchanged_inputs` applies fixed six-input byte ceilings before any hashing,
then compares every original bundle hash/length, RawTransaction hash, selected
output index and complete Script hash. It detects later input substitution; it
neither parses new receipts nor makes a new semantic claim under new budgets.
The caller must perform a fresh construction to use changed inputs.

The native [frozen code catalog](FROZEN_CODE_CATALOG.md) requires an actual finite
receipt for every member, including unselected and final members. Its source
ownership and pinned chain checks remain separate native evidence. These finite
receipts do not contain authenticated source publisher identity, version/status
or downgrade decisions, Type replacement history, active chain VM context,
immutable consumer authorization, nominal source `I`/handles, or H2 runtime
checks. They do not complete H1, #28, #29 or stable admission. Independent security
review remains unassigned and required before stable admission.

`tests/policy_artifact_checker.rs` checks both targets at O0–O3, every recorded
field and an independent domain/hash oracle, deterministic reconstruction,
exact-args separation, directional extension/incompatibility, declaration-only
templates, constants, post-check substitution of every input and preparse byte
limits. `tests/frozen_interface.rs` checks receipt ownership for all members and
rejection of otherwise byte/target-valid public constants in an unselected or
32nd candidate. These are host tests; no new VM or public-chain claim is made.

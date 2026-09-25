# 0.31 engineering boundary review — 2026-09-25

Reviewer: Codex, post-implementation source and evidence review of
`352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71..6196a7fc60c5186240e3e9acf31bed2b764a580e`.
This is an automated engineering review, not an independent security audit,
release approval, or deployment receipt.

The maintainer requested completion through 0.31, explicitly excluding #30 and
the 0.32 scope. Engineering closure covers #27 and #31–#35. Release version
promotion, fresh distribution bundles, the final clean release replay and
independent release review remain under the skipped #30. No 0.30 waiver is
transferred. #22/#28/#29 and #36–#40 remain deferred.

## Decisions and source review

| Issue | Reviewed contract | Outcome |
| --- | --- | --- |
| #31 / D2 | `assembler/immediate.rs`: literal range validation precedes bit normalization; signed low 12 bits; i128 subtraction at endpoints; shifts 1–63; lexicographic deterministic tie break; reconstructed bits and non-growing fallback. Sizing and emission call the same planner. The fixed 20-byte entry trampoline stays separate. | Retain the bounded existing-ISA implementation. Encoded VM tests compare with independent rodata, exercise x0 and far branches. No minimality claim. |
| #32 / D3 | `policy_decoder_key` includes the complete callable ABI and resolved payload, including ordered parameter types/sources/bindings, TypeHash/runtime/plan indices and layout flags. Unknown/dynamic/Script-arg layouts remain dedicated. The independent checker separately projects parameter contracts and binds every decoder caller, direct action target and return. | Retain shared decoders with direct stubs and linear dispatch. Equal byte width is insufficient. No selector cache, tree or table is added. |
| #33 / D4 | Only selected-record adapters shrink. Fixed capacity includes the CSARG header, length slot, alignment and saved ra; the parent retains its 4,096-byte loader. Decoder borrows the stub's frame, makes no nested calls and cannot move sp. Outgoing arguments occupy a separate temporary reservation. | Retain the private copy and conservative 5,376-byte fallback. No witness borrowing or copy-loop rewrite. Add a combined shared-decoder/outgoing-argument saved-ra mutation regression. |
| #34 / D5 | `scalar_slots.rs` computes successor-based liveness to a fixed point and interference backwards per instruction, including terminator uses. Parameters form a clique because every parameter spills, even if unused. Sorted IDs receive the lowest non-conflicting eight-byte slot. Named mutable storage and buffers remain separate. | Retain whole-function fallback for resources, pointers, wide values, external calls and unclassified operations. Central offset lookup covers address formation as well as loads/stores. Reused offsets are not an independently verified liveness certificate. |
| #35 / D1 | `cost_measurement.rs` selects the exact Script hash and role with explicit input/output membership. Development hardfork/tip/environment match locked ckb-testtool 1.1.1; ckb-script 1.1.0 `detailed_run` returns scheduler cycles with nonzero exits. Normal transaction verification remains the acceptance oracle. Missing/trap/limit observations have no cycle value. | Keep transaction and group metrics separate. Required unavailable observations reject budgets. Static single-VM call-chain analysis includes temporary outgoing arguments; recursion, inconsistent joins, unknown targets/syscalls and EXEC/SPAWN remain unknown. |

No production compiler/checker change was required by this source review.
The new test strengthens evidence at the intersection of two existing contracts;
the existing VM test already covers shared decoding with wide and stack arguments.

## Acceptance map

Paths below are repository-relative. The frozen fixture and budget inventory is
in [README.md](README.md#coverage-map); numerical attribution is recorded
separately from this review.

| Boundary | Evidence |
| --- | --- |
| Full-width literal boundaries, deterministic sampling, no growth | `src/codegen/assembler/immediate.rs::signed_boundaries_sparse_patterns_and_full_width_samples_never_grow` |
| Encoded values, x0, labels/relaxation and unchanged trampoline | `src/codegen/assembler.rs::immediate_plans_execute_in_ckb_vm_with_relaxed_branches_and_zero_destination` |
| 1–64 actions; dense, sparse and high tags; mixed layouts; every action; 1–8 records | 41 frozen rows in `tests/fixtures/cost_corpus/expanded_fixtures.json`, executed by `tests/support/cost_expanded.rs` |
| Empty, exact, malformed/truncated, unknown/late tags, wide bytes, first/last record and optional whole-witness fields at 4,096/4,097 bytes | Expanded executions plus `src/artifact/vm_tests.rs::{payload_free_action_still_requires_selector_and_exact_empty_args,selected_record_reuses_exact_positional_argument_widths,malformed_canonical_layouts_and_oversized_whole_witness_fail_before_dispatch}` |
| Wide arguments and more than eight flattened args through a shared decoder | `src/artifact/vm_tests.rs::shared_policy_decoder_preserves_wide_and_outgoing_stack_arguments`, both source editions and optimizer levels 0/3 |
| Unknown tags precede common checks; failure propagation and exact routing | Policy VM tests for selector identity, common failure and discarded callable failures; `tests/policy_artifact_checker.rs` dispatch and rebound machine mutations |
| Private capacity, ra slot, v8 rejection and shared decoder caller/return restrictions | `tests/artifact_checker.rs::{compact_policy_adapter_rejects_frame_capacity_and_saved_return_address_mutations,shared_policy_decoder_has_only_bound_callers_and_cannot_clobber_its_borrowed_frame}` |
| Shared private ownership with temporary outgoing storage | `tests/policy_artifact_checker.rs::{typed_outgoing_stack_args_are_bound_to_the_policy_adapter_frame,shared_decoder_preserves_private_return_storage_with_outgoing_stack_arguments}` |
| Dynamic/Script-arg positional fallback and nested callable failures | `src/artifact/compile.rs::policy_transport_preserves_supported_positional_payload_families`; policy callable VM tests; `tests/entry_witness_abi.rs` |
| Joins, backedges, simultaneous/dead parameter spills, nested nine-argument calls, 2,040/2,048 offset threshold, generic instantiations | `scalar_slots.rs` liveness test; nine frozen scalar rows and their success/rejection executions in `tests/support/cost_scalar.rs` |
| Nonzero cycles increase with extra executed work; unavailable limit/setup/trap | `tests/cost_measurement.rs` |
| Nested frames, temporary outgoing areas, recursion, unknown callees, external VM effects | `tests/support/cost_stack.rs` and `tests/cost_measurement.rs` |
| Retained matched/growth rows, separate static loads/stores, multi-Script outcomes | Three matched, eighteen growth and nine scalar rows; six multi-Script transactions with five groups each; native `compare-cost-evidence` |

Static load/store counts describe decoded text, including unreachable code.
They do not measure dynamic memory traffic. The observed stores are unchanged
in all nine archived scalar rows. No dynamic stack peak, universal Rust parity,
or global optimization claim is made.

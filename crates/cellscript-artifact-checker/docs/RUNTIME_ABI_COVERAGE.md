# Runtime ABI evidence coverage

Status: current coverage inventory, 2026-11-03. This inventories assurance;
it does not claim complete runtime-body verification or source equivalence.

The [closed registry](../src/runtime_contract.rs)
contains 219 helper contracts across 12 return conventions, including eight
inline contracts. Counts below include inline and deferred/failing contracts.
They count descriptions, not independent proofs. The checker test
`runtime_abi_coverage_inventory_matches_documented_counts` binds these counts to
the actual registry and requires an explicit row for every convention.

| Return convention | Helpers | Return obligation | Independent machine evidence and explicit limit |
| --- | ---: | --- | --- |
| `ScalarValue` | 11 | Result in a0 | Selected fixed memory/hash dependency bodies have pinned complete shapes; there is no generic proof of every scalar helper's value. |
| `ScalarOrAbort` | 18 | Result in a0; failures terminate internally | Bounded header/witness/profile recognizers cover their admitted paths. IPC read/inherited-fd have focused VM controls; this is not a class-wide body proof. |
| `ScalarZeroStatusOrAbort` | 16 | Value in a0 and zero in a1; failure aborts | Typed Cell property recognizers validate capacity/data-size bodies and caller transport; the compiler inventory requires decoded normal returns to establish zero in a1. This regression is separate from production body admission and memory-content proof. |
| `ScalarStatusA1` | 53 | a0 valid only after checking a1 | Four legacy getters have complete helper and incoming-guard decoding; typed property recognizers cover additional named shapes. No generic decoder covers all 53. |
| `StatusA0` | 58 | Caller interprets a0; EXEC may return only on failure | Selected membership/instance/handle/profile paths are checked separately. No single class-wide proof establishes successful status semantics. |
| `ReturnedStatusA0` | 6 | Explicit returned status/predicate in a0 | Call sites must interpret the public status contract. WAIT/write/close VM fixtures do not constitute complete independent decoding of every status body. |
| `BufferStatusA0` | 39 | Caller-owned output buffer plus a0 status | Named hash/args/witness decoders bind finite buffer extents, calls and guards. General caller-provided destinations remain outside those finite contracts. |
| `NoReturn` | 9 | No successful return | Abort sink and deferred-error paths are checked; six deferred symbols abort with 66/86/87, two inline collection guards with 24. Shared availability descriptions alone do not prove their bodies. |
| `Void` | 6 | No result-register value promised | Compression has a pinned dependency shape; ZK/BIP transport has separate request/lifecycle checks. No general semantic proof follows for all six. |
| `StatusA0ValueA1` | 1 | a0 status, a1 child id | `__ckb_spawn_with_fd1` independently decodes the complete finite body, disjoint private syscall buffers and every incoming status guard. Child identity, subsequent caller storage and child behavior require their separate contracts. |
| `StatusA0ValuesA1A2` | 1 | a0 status, a1/a2 descriptors | PIPE independently decodes helper, every incoming guard and source tuple storage. Its admitted storage-prefix forms are finite; unknown forms reject with V2420. |
| `ValuesA0ToA3StatusA4` | 1 | Offset/length/index/source in a0..a3, status a4 | Bounded witness resolver has a complete finite recognizer and caller binding; it does not authorize arbitrary witness ABI shapes. |

## Where the evidence lives

- [Compiler runtime inventory](../../../src/codegen/runtime/stack_audit.rs): emits all
  registered standalone bodies and stdlib variants, independently decodes their
  machine frames and checks aligned/restored stack paths.
  Its [return-register audit](../../../src/codegen/runtime/stack_audit/return_registers.rs)
  checks defined statuses, zero-status returns, multi-register results and
  guarded propagation of conditional results, including known stack spills.
  Mutations remove result definitions, invert guards, truncate statuses and
  attempt to hide unchecked spills behind opaque calls or partial stores.
  It is a compiler regression, not production checker admission. Loaded buffer
  contents, arbitrary memory aliases, caller arguments, preserved registers and
  general semantic values remain outside this register analysis.
- [Scalar-status recognizer](../src/nested_machine/scalar_status.rs):
  exact input OutPoint index/hash-low and Cell lock/type hash-low bodies, their
  failure alternative, branch targets and every incoming a1 guard.
- [Typed Cell properties](../src/nested_machine/view_values.rs),
  [Cell fields](../src/nested_machine/cells.rs),
  [Cell hashes](../src/nested_machine/cell_read_hashes.rs)
  and the other responsibility-owned `nested_machine` modules: selected finite
  property/byte-view contracts. A module's existence is not blanket coverage of
  every helper in the same return class.
- [PIPE](../src/pipe_machine.rs),
  [mutations](../../../tests/artifact_checker/pipe.rs) and
  [VM controls](../../../tests/pipe_runtime.rs): status/descriptor separation,
  branch-target substitution, full-width tuple storage and private-frame overlap.
- [Witness resolver](../src/nested_machine/witness_resolver.rs):
  complete helper shape and source/offset/range transport.
- [Single-fd SPAWN](../src/spawn_machine.rs) and
  [rebound mutations](../../../tests/artifact_checker/spawn.rs): complete
  helper frame, full-width child-id output, terminated inherited-fd list,
  argument pointers, status alternatives, typed call inventory and every
  incoming success/failure guard. BIP340 and exact ZK typed calls bind their
  distinct failure codes (51/76); both mixed call orders retain exact inventory
  and guard matching. Foreign entry into a partial helper or
  directly past the caller guard rejects. The
  [controlled VM regression](../../../src/codegen/runtime/stack_audit/spawn_vm.rs)
  checks buffer separation, a high-bit child id, failure and an executed
  truncation mutation. These controls do not model a child or establish
  scheduler consensus behavior. The argument layout follows
  [RFC0050](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0050-vm-syscalls-3/0050-vm-syscalls-3.md).
- [Instance dependency pins](../src/instance_machine.rs):
  complete named bodies with relocation-normalized callees. A known body digest
  establishes that exact admitted implementation, not correctness of an arbitrary
  implementation or cryptographic theorem.
- [Deferred regressions](../../../tests/deferred_ckb_semantics.rs): availability and
  abort identities remain separate from implemented CKB capabilities.

## Admission and remaining proof obligations

Bundle hashes, structural checks, effect-fact equality, helper inventory closure,
decoded calling convention, complete finite body recognition, VM scenarios and
source equivalence are different claims. No percentage based on 12 conventions
is a meaningful semantic coverage score. A convention can contain both checked
finite bodies and bodies whose semantics remain in the trusted runtime boundary.

For every added or broadened executable profile, the responsible checker/backend
change must name the exact helper and caller shape, result/status registers,
buffer ownership/extent, branch and failure paths, dependency closure, positive
compiler-produced fixture and negative machine mutations. A shared descriptor
cannot grant machine admission. Unknown forms at an existing finite recognizer
must continue to reject; do not weaken it to make the inventory appear complete.

This closes the missing coverage inventory, not the listed semantic proof gaps.
Further body recognizers are explicit backend/checker work under issue #92.
Focused tests are appropriate for this inventory; integration/release claims
continue to require the corresponding repository gates.

## Complete helper index

Every registered symbol appears below, including inline guards. The execution
form says whether the all-body stack inventory can emit a standalone body; it
is not an admission verdict. Apply the convention-specific evidence and limits
above to each row. No unlisted helper can acquire an implicit ABI contract.

| Helper | Return convention | Execution form |
| --- | --- | --- |
| `__c256_require_u128_product_eq` | `StatusA0` | standalone |
| `__c256_require_u128_product_lte` | `StatusA0` | standalone |
| `__c256_require_u128_sum2_products_eq` | `StatusA0` | standalone |
| `__c256_require_u128_sum2_products_lte` | `StatusA0` | standalone |
| `__cellscript_abort` | `NoReturn` | standalone |
| `__cellscript_add_u256` | `ScalarValue` | standalone |
| `__cellscript_blake2b_compress` | `Void` | standalone |
| `__cellscript_blake2b_prefixed_transaction_span` | `BufferStatusA0` | standalone |
| `__cellscript_blake2b_segments` | `BufferStatusA0` | standalone |
| `__cellscript_blake2b_transaction_span` | `BufferStatusA0` | standalone |
| `__cellscript_bounded_consume_each_fail_closed` | `NoReturn` | inline |
| `__cellscript_bounded_create_each_fail_closed` | `NoReturn` | inline |
| `__cellscript_current_script_role_at` | `ScalarStatusA1` | standalone |
| `__cellscript_exact_read_cached` | `ScalarZeroStatusOrAbort` | standalone |
| `__cellscript_load_u64_le` | `ScalarValue` | standalone |
| `__cellscript_memcmp_fixed` | `ScalarValue` | standalone |
| `__cellscript_memcpy_fixed` | `BufferStatusA0` | standalone |
| `__cellscript_memzero_fixed` | `ScalarValue` | standalone |
| `__cellscript_mul_u128_to_u256` | `Void` | standalone |
| `__cellscript_require_cell_membership` | `ReturnedStatusA0` | standalone |
| `__cellscript_require_exact_size` | `ReturnedStatusA0` | standalone |
| `__cellscript_require_fungible_type_group_v1` | `StatusA0` | standalone |
| `__cellscript_require_min_size` | `ReturnedStatusA0` | standalone |
| `__cellscript_sha256_compress` | `BufferStatusA0` | standalone |
| `__cellscript_sha256_fixed` | `BufferStatusA0` | standalone |
| `__cellscript_witness_bounded_resolve` | `ValuesA0ToA3StatusA4` | standalone |
| `__ckb_block_number_to_u64` | `ScalarValue` | standalone |
| `__ckb_cell_capacity` | `ScalarStatusA1` | standalone |
| `__ckb_cell_count` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_data_blake2b_span` | `BufferStatusA0` | standalone |
| `__ckb_cell_data_bytes32` | `BufferStatusA0` | standalone |
| `__ckb_cell_data_equal` | `ScalarStatusA1` | standalone |
| `__ckb_cell_data_hash_field` | `BufferStatusA0` | standalone |
| `__ckb_cell_data_size` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_data_u32_le` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_data_u64_le` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_data_u8` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_has_type` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_lock_args32` | `BufferStatusA0` | standalone |
| `__ckb_cell_lock_args_empty` | `ScalarStatusA1` | standalone |
| `__ckb_cell_lock_code_hash` | `BufferStatusA0` | standalone |
| `__ckb_cell_lock_hash` | `BufferStatusA0` | standalone |
| `__ckb_cell_lock_hash_low` | `ScalarStatusA1` | standalone |
| `__ckb_cell_lock_hash_type` | `ScalarStatusA1` | standalone |
| `__ckb_cell_lock_size` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_lock_u8` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_occupied_capacity` | `ScalarStatusA1` | standalone |
| `__ckb_cell_output_index` | `ScalarStatusA1` | standalone |
| `__ckb_cell_type_args32` | `BufferStatusA0` | standalone |
| `__ckb_cell_type_args_empty` | `ScalarStatusA1` | standalone |
| `__ckb_cell_type_code_hash` | `BufferStatusA0` | standalone |
| `__ckb_cell_type_hash` | `BufferStatusA0` | standalone |
| `__ckb_cell_type_hash_low` | `ScalarStatusA1` | standalone |
| `__ckb_cell_type_hash_type` | `ScalarStatusA1` | standalone |
| `__ckb_cell_type_size` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_type_u8` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_cell_unoccupied_capacity` | `ScalarStatusA1` | standalone |
| `__ckb_close` | `ReturnedStatusA0` | standalone |
| `__ckb_commitment_open` | `Void` | inline |
| `__ckb_current_role` | `ScalarStatusA1` | standalone |
| `__ckb_current_script_hash` | `BufferStatusA0` | standalone |
| `__ckb_epoch_add` | `ScalarStatusA1` | standalone |
| `__ckb_epoch_duration` | `ScalarStatusA1` | standalone |
| `__ckb_epoch_duration_to_u64` | `ScalarValue` | standalone |
| `__ckb_epoch_length_to_u64` | `ScalarValue` | standalone |
| `__ckb_epoch_number_to_u64` | `ScalarValue` | standalone |
| `__ckb_epoch_sub` | `ScalarStatusA1` | standalone |
| `__ckb_exec_cell_dep_hex4` | `StatusA0` | standalone |
| `__ckb_exec_cell_dep_u8_args` | `StatusA0` | standalone |
| `__ckb_hash_blake2b` | `BufferStatusA0` | standalone |
| `__ckb_hash_blake2b_packed` | `BufferStatusA0` | inline |
| `__ckb_hash_blake2b_var` | `BufferStatusA0` | standalone |
| `__ckb_hash_chain` | `BufferStatusA0` | standalone |
| `__ckb_hash_data_packed` | `BufferStatusA0` | inline |
| `__ckb_hash_pair` | `BufferStatusA0` | standalone |
| `__ckb_hash_sha256` | `BufferStatusA0` | standalone |
| `__ckb_hash_sha256_pair` | `BufferStatusA0` | standalone |
| `__ckb_hash_sha256d` | `BufferStatusA0` | standalone |
| `__ckb_hash_sha256d_pair` | `BufferStatusA0` | standalone |
| `__ckb_header_dep_block_number` | `ScalarOrAbort` | standalone |
| `__ckb_header_dep_epoch_length` | `ScalarOrAbort` | standalone |
| `__ckb_header_dep_epoch_number` | `ScalarOrAbort` | standalone |
| `__ckb_header_dep_epoch_start_block_number` | `ScalarOrAbort` | standalone |
| `__ckb_header_dep_timestamp_millis` | `ScalarOrAbort` | standalone |
| `__ckb_header_epoch_length` | `ScalarOrAbort` | standalone |
| `__ckb_header_epoch_number` | `ScalarOrAbort` | standalone |
| `__ckb_header_epoch_start_block_number` | `ScalarOrAbort` | standalone |
| `__ckb_inherited_fd` | `ScalarOrAbort` | standalone |
| `__ckb_input_out_point_index` | `ScalarStatusA1` | standalone |
| `__ckb_input_out_point_tx_hash` | `BufferStatusA0` | standalone |
| `__ckb_input_out_point_tx_hash_low` | `ScalarStatusA1` | standalone |
| `__ckb_input_since` | `ScalarOrAbort` | standalone |
| `__ckb_input_since_at` | `ScalarStatusA1` | standalone |
| `__ckb_occupied_capacity` | `NoReturn` | standalone |
| `__ckb_pipe` | `StatusA0ValuesA1A2` | standalone |
| `__ckb_pipe_read` | `ScalarOrAbort` | standalone |
| `__ckb_pipe_write` | `ReturnedStatusA0` | standalone |
| `__ckb_process_id` | `ScalarValue` | standalone |
| `__ckb_raw_transaction_hash_without_cell_deps` | `BufferStatusA0` | standalone |
| `__ckb_require_bounded_cell_dep_data_hash` | `StatusA0` | standalone |
| `__ckb_require_cell_data_hash` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_deployment_line_verifier_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_exact_verifier_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_open_script_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_open_script_handle_from_args` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_open_verifier_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_dep_open_verifier_handle_from_args` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_args32` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_args_empty` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_args_exact` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_args_prefix32` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_args_suffix32` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_deployment_line_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_exact_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_hash` | `StatusA0` | standalone |
| `__ckb_require_cell_lock_script_hash_type` | `StatusA0` | standalone |
| `__ckb_require_cell_type_args32` | `StatusA0` | standalone |
| `__ckb_require_cell_type_args_empty` | `StatusA0` | standalone |
| `__ckb_require_cell_type_args_exact` | `StatusA0` | standalone |
| `__ckb_require_cell_type_args_prefix32` | `StatusA0` | standalone |
| `__ckb_require_cell_type_args_suffix32` | `StatusA0` | standalone |
| `__ckb_require_cell_type_deployment_line_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_type_exact_handle` | `StatusA0` | standalone |
| `__ckb_require_cell_type_hash` | `StatusA0` | standalone |
| `__ckb_require_cell_type_script_hash_type` | `StatusA0` | standalone |
| `__ckb_require_current_script_args_empty` | `StatusA0` | standalone |
| `__ckb_require_epoch_after` | `NoReturn` | standalone |
| `__ckb_require_epoch_relative` | `NoReturn` | standalone |
| `__ckb_require_input_out_point` | `StatusA0` | standalone |
| `__ckb_require_input_out_point_tx_hash` | `StatusA0` | standalone |
| `__ckb_require_instance_script_participant_from_args` | `StatusA0` | standalone |
| `__ckb_require_lock_match_master_out_point_pairs_from_data` | `StatusA0` | standalone |
| `__ckb_require_lock_type_metapoint_pairs` | `StatusA0` | standalone |
| `__ckb_require_lock_type_metapoint_pairs_from_i32_data` | `StatusA0` | standalone |
| `__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered` | `StatusA0` | standalone |
| `__ckb_require_maturity` | `NoReturn` | standalone |
| `__ckb_require_metapoint_relative` | `StatusA0` | standalone |
| `__ckb_require_open_script_participant_from_args` | `StatusA0` | standalone |
| `__ckb_require_sha256d_merkle_root` | `StatusA0` | standalone |
| `__ckb_require_time` | `NoReturn` | standalone |
| `__ckb_require_type_lock_metapoint_pairs` | `StatusA0` | standalone |
| `__ckb_require_type_lock_metapoint_pairs_from_i32_data` | `StatusA0` | standalone |
| `__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered` | `StatusA0` | standalone |
| `__ckb_require_witness_size_at_least` | `StatusA0` | standalone |
| `__ckb_script_hash` | `BufferStatusA0` | standalone |
| `__ckb_sighash_all` | `NoReturn` | standalone |
| `__ckb_sighash_all_zero_lock` | `BufferStatusA0` | standalone |
| `__ckb_since_as_absolute_block` | `ScalarStatusA1` | standalone |
| `__ckb_since_as_absolute_epoch` | `ScalarStatusA1` | standalone |
| `__ckb_since_as_absolute_timestamp` | `ScalarStatusA1` | standalone |
| `__ckb_since_as_relative_block` | `ScalarStatusA1` | standalone |
| `__ckb_since_as_relative_epoch` | `ScalarStatusA1` | standalone |
| `__ckb_since_as_relative_timestamp` | `ScalarStatusA1` | standalone |
| `__ckb_since_block_absolute` | `ScalarStatusA1` | standalone |
| `__ckb_since_block_relative` | `ScalarStatusA1` | standalone |
| `__ckb_since_decode` | `ScalarStatusA1` | standalone |
| `__ckb_since_epoch_absolute` | `ScalarStatusA1` | standalone |
| `__ckb_since_epoch_relative` | `ScalarStatusA1` | standalone |
| `__ckb_since_from_raw_checked` | `ScalarStatusA1` | standalone |
| `__ckb_since_is_disabled` | `ScalarStatusA1` | standalone |
| `__ckb_since_is_relative` | `ScalarStatusA1` | standalone |
| `__ckb_since_metric` | `ScalarStatusA1` | standalone |
| `__ckb_since_timestamp_absolute` | `ScalarStatusA1` | standalone |
| `__ckb_since_timestamp_relative` | `ScalarStatusA1` | standalone |
| `__ckb_since_to_raw` | `ScalarValue` | standalone |
| `__ckb_since_value` | `ScalarStatusA1` | standalone |
| `__ckb_source_bytes_equal` | `ScalarStatusA1` | standalone |
| `__ckb_source_bytes_equal_memory` | `ScalarStatusA1` | standalone |
| `__ckb_source_bytes_zero` | `ScalarStatusA1` | standalone |
| `__ckb_source_cell_dep` | `ScalarOrAbort` | standalone |
| `__ckb_source_group_input` | `ScalarOrAbort` | standalone |
| `__ckb_source_group_output` | `ScalarOrAbort` | standalone |
| `__ckb_source_header_dep` | `ScalarOrAbort` | standalone |
| `__ckb_source_input` | `ScalarOrAbort` | standalone |
| `__ckb_source_output` | `ScalarOrAbort` | standalone |
| `__ckb_spawn_wait_cell_dep_hex4` | `StatusA0` | standalone |
| `__ckb_spawn_with_fd1` | `StatusA0ValueA1` | standalone |
| `__ckb_timestamp_millis_to_u64` | `ScalarValue` | standalone |
| `__ckb_transaction_blake2b_gather` | `BufferStatusA0` | standalone |
| `__ckb_transaction_hash` | `BufferStatusA0` | standalone |
| `__ckb_transaction_u32_le` | `ScalarStatusA1` | standalone |
| `__ckb_wait` | `ReturnedStatusA0` | standalone |
| `__ckb_witness_blake2b_select_chunks` | `BufferStatusA0` | standalone |
| `__ckb_witness_blake2b_span` | `BufferStatusA0` | standalone |
| `__ckb_witness_bounded_blake2b` | `BufferStatusA0` | standalone |
| `__ckb_witness_bounded_size` | `ScalarStatusA1` | standalone |
| `__ckb_witness_bounded_u32_le` | `ScalarStatusA1` | standalone |
| `__ckb_witness_bounded_u64_le` | `ScalarStatusA1` | standalone |
| `__ckb_witness_bounded_u8` | `ScalarStatusA1` | standalone |
| `__ckb_witness_bytes32` | `BufferStatusA0` | standalone |
| `__ckb_witness_count` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_witness_input_type_exact32` | `BufferStatusA0` | standalone |
| `__ckb_witness_lock_exact32` | `BufferStatusA0` | standalone |
| `__ckb_witness_output_type_exact32` | `BufferStatusA0` | standalone |
| `__ckb_witness_size` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_witness_u32_le` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_witness_u64_le` | `ScalarZeroStatusOrAbort` | standalone |
| `__ckb_witness_u8` | `ScalarZeroStatusOrAbort` | standalone |
| `__dao_accumulated_rate` | `ScalarStatusA1` | standalone |
| `__dao_has_dao_type` | `ScalarStatusA1` | standalone |
| `__dao_input_accumulated_rate` | `ScalarStatusA1` | standalone |
| `__dao_is_deposit_data` | `ScalarStatusA1` | standalone |
| `__dao_is_withdrawal_request_data` | `ScalarStatusA1` | standalone |
| `__dao_require_header_dep_for_input` | `StatusA0` | standalone |
| `__dao_require_input_relative_epoch_since_at_least` | `StatusA0` | standalone |
| `__dao_require_input_since_at_least` | `StatusA0` | standalone |
| `__env_current_timepoint` | `ScalarOrAbort` | standalone |
| `__novaseal_bip340_require_signature` | `Void` | inline |
| `__novaseal_bip340_require_signature_from_cell_dep` | `Void` | inline |
| `__xudt_amount_high` | `ScalarStatusA1` | standalone |
| `__xudt_amount_low` | `ScalarStatusA1` | standalone |
| `__xudt_owner_mode_input_type_hash` | `ScalarStatusA1` | standalone |
| `__xudt_require_group_amount_burned` | `StatusA0` | standalone |
| `__xudt_require_group_amount_conserved` | `StatusA0` | standalone |
| `__xudt_require_group_amount_minted` | `StatusA0` | standalone |
| `__xudt_require_owner_mode_input_type` | `StatusA0` | standalone |
| `__xudt_require_owner_mode_type_args` | `StatusA0` | standalone |
| `__xudt_require_owner_mode_type_args_current_script` | `StatusA0` | standalone |
| `__zk_require_transition` | `Void` | inline |

## Authenticated opening materialization

The [finite materialization recognizer](../src/checker/commitment_materialization.rs)
binds the opening pointer and exact width to the post-authentication result.
It validates caller-frame copy destinations or the emitted scalar byte assembly,
rejects intervening stores and bypass branches, and retains the existing hash,
size/status and digest-comparison checks. This closes a reproduced rebound
pointer mutation that the earlier authentication-plus-result-store check
accepted. [Array/tuple/scalar mutations](../../../tests/artifact_checker/committed_aggregates.rs)
exercise each shape. These caller checks do not substitute for the separate
memcpy/runtime contract or establish universal machine-body correctness.

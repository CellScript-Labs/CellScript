# iCKB Production Equivalence Gate

This gate defines the minimum evidence required before CellScript may claim
production-equivalent behaviour for any selected iCKB scenario.

Current status: `NOT_PROVEN` (partial CKB VM execution evidence available).

The current benchmark has advanced from `MODEL_LEVEL_ONLY` to
`PARTIAL_CKB_VM_EXECUTION`: CellScript-generated scripts and original iCKB
ELFs both execute under real CKB VM/syscall context via `ckb-testtool`, and
seventy-five rows now have original-vs-CellScript pass/fail differential evidence;
fourteen additional CellScript-only VM rows provide precursor syscall and
DAO-maturity evidence, and eight original-side rows now include iCKB Logic plus
unmodified DAO binary execution.
Passing model fixtures, compiling CellScript, emitting RISC-V
assembly, or matching ProofPlan metadata is still not enough to claim
behavioural equivalence with the audited iCKB Rust scripts.

## Claim Levels

| Level | Meaning | Allowed wording |
|---|---|---|
| `MODEL_LEVEL_ONLY` | CellScript examples and JSON fixtures model the intended invariant, but no original iCKB binary and generated CellScript binary were executed side by side. | "model-level", "iCKB-style", "partial" |
| `CELL_SCRIPT_CKB_VM_EXECUTED` | CellScript-generated script executed in real CKB VM with full syscall context (ckb-testtool), but original iCKB binary not yet run against the same fixtures. | "CellScript CKB VM executable", "partial VM evidence" |
| `ORIGINAL_ICKB_CKB_VM_EXECUTED` | Original iCKB ELF executed in real CKB VM, without a matching CellScript side for that row. | "original iCKB VM evidence" |
| `DIFFERENTIAL_CKB_VM_EXECUTED` | Original iCKB script and generated CellScript script were run on the same normalized CKB VM/testtool scenario fixture, with pass/fail status matching. | "executed differential subset" |
| `PROVEN` | Every scenario in the selected equivalence matrix has executed evidence and matching pass/fail behaviour with named reject reasons. | "production-equivalent for the selected executed subset" |

Full iCKB equivalence must not be claimed unless the matrix reaches
`PROVEN`. A partial executed subset must still identify every unsupported row as
`NOT_PROVEN` or `UNSUPPORTED`.

## Required Evidence

`tests/benchmarks/ickb_diff/matrix.json` is the executable claim manifest. The
integration test `tests/ickb_diff.rs` rejects any production equivalence claim
unless the manifest provides all of the following:

1. original iCKB repository commit;
2. original iCKB script binary SHA-256;
3. CellScript source commit;
4. generated CellScript artifact SHA-256;
5. CKB VM or CKB testtool version;
6. transaction fixture manifest SHA-256;
7. proof that both sides used identical inputs, outputs, cell deps, header deps,
   witnesses, and output data for the overlapping scenario;
8. original and generated script exit codes;
9. named failure mode for every reject case;
10. cycle and transaction-size measurements;
11. per-row execution objects;
12. pass/fail status match evidence;
13. transaction context hashes;
14. capacity and fee measurements.

Each row that claims executed equivalence must additionally contain an
`execution` object with:

- `fixture_sha256`;
- `transaction_context_sha256`;
- `original_ickb_binary_sha256`;
- `cellscript_artifact_sha256`;
- `ckb_vm_or_testtool_version`;
- `original_ickb_exit_code`;
- `cellscript_exit_code`;
- `original_ickb_status`;
- `cellscript_status`;
- `statuses_match = true`;
- `original_cycles`;
- `cellscript_cycles`;
- `tx_size_bytes`;
- `occupied_capacity_shannons`;
- `fee_shannons`;
- `failure_mode` for reject cases.

All SHA-256 values must be canonical `0x`-prefixed 32-byte hex. A passing row
must report exit code `0`; a failing row must report a non-zero exit code and a
named failure mode. `tests/ickb_diff.rs` rejects `PROVEN` if any row remains
`MODEL`, uses a `model-*` result, lacks CKB VM execution, lacks original iCKB
execution, or has mismatched status/exit-code evidence.

## Running The Gate

The iCKB-specific gate is executable through Rust integration tests. It is not
exposed from the generic `cellc` CLI:

```bash
cargo test --locked -p cellscript --test ickb_diff
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
```

`verify-ckb-fixtures` validates the standard CKB fixture manifest with the
deterministic model runner and emits a manifest hash. It still reports
`ckb_vm_execution = false`. `tests/ickb_benchmark.rs` validates the iCKB-style
positive and adversarial fixtures directly and also reports model-only coverage.
`tests/ickb_diff.rs` accepts the current `NOT_PROVEN` matrix as an honest
non-equivalence manifest. It fails if a matrix claims
`PROVEN` or production mode `EXECUTED_CKB_VM_DIFF` without the required top-level and per-row
execution evidence.

## Current Enforcement

The current matrix sets:

- `mode = PARTIAL_CKB_VM_EXECUTION`
- `equivalence_status = NOT_PROVEN`
- `production_equivalence_claim = false`
- `equivalence_evidence = null`

Current row counts by evidence level:

- `CELL_SCRIPT_CKB_VM_EXECUTED`: 14
- `ORIGINAL_ICKB_CKB_VM_EXECUTED`: 8
- `DIFFERENTIAL_CKB_VM_EXECUTED`: 75
- `MODEL`: 0

The matrix also carries a `remaining_model_blockers` registry. The test suite
requires it to match the active `MODEL` rows exactly and to include a non-empty
blocker explanation plus the required upgrade capability for each row. A
separate `non_executable_model_assumptions` registry records legacy model
assumptions that are no longer active executable-evidence rows; currently this
contains duplicate receipt-id, wrong-owner synthetic resource fields, and
synthetic current-epoch redeem maturity. Each entry names the replacement
differential row that already executes the closest chain-level fixture shape.
For production mode, this registry must be empty; otherwise the gate rejects the
claim even if all active rows have CKB VM evidence.

The seventy-five differential rows are:

1. **Non-empty script args reject**: unpatched original iCKB Logic and a
   generated CellScript ELF both reject the same normalized non-empty type args
   fixture.
2. **Valid deposit phase 1**: patched original iCKB Logic and a generated
   CellScript ELF both pass the same normalized deposit/receipt fixture.
3. **Deposit capacity-bound reject**: patched original iCKB Logic and the
   generated CellScript ELF both reject the same normalized under-capacity
   deposit/receipt fixture. The exit codes are recorded separately; pass/fail
   status, not numeric code identity, is the row-level differential condition.
4. **Deposit capacity upper-bound reject**: patched original iCKB Logic and a
   generated CellScript upper-bound probe both reject the same normalized
   oversized deposit/receipt fixture, with named
   `deposit_capacity_upper_bound_rejected` failure mode.
5. **Receipt without deposit reject**: patched original iCKB Logic and the
   generated CellScript ELF both reject the same normalized receipt-only
   fixture with named `receipt_without_deposit_rejected` failure mode.
6. **Duplicate receipt output reject**: patched original iCKB Logic and the
   generated CellScript ELF both reject the same normalized deposit/receipt
   fixture where one DAO deposit output is paired with two receipt outputs of
   the same amount, with named `duplicate_receipt_output` failure mode. This is
   output-accounting `ReceiptMismatch` evidence, not the model-only duplicate
   receipt-id double-mint fixture.
7. **Receipt group exact mint**: unpatched original iCKB Logic and a generated
   CellScript aggregate probe both accept the same normalized two-receipt input
   fixture when the xUDT output mints exactly two receipts worth of iCKB.
8. **Receipt group over-mint reject**: unpatched original iCKB Logic and a
   generated CellScript aggregate probe both reject the same normalized
   two-receipt input fixture when the xUDT output mints one shannon more than
   two receipts worth of iCKB, with named `receipt_group_over_mint` failure
   mode. This is multi-receipt aggregate evidence, not duplicate receipt-id
   proof.
9. **Receipt group missing-header reject**: unpatched original iCKB Logic and
   a generated CellScript aggregate probe both reject the same normalized
   two-receipt input fixture when both receipt inputs are linked to a DAO
   header in the test context but the transaction omits that header dep, with
   named `receipt_group_missing_header_dep` failure mode.
10. **Receipt group wrong accumulated-rate reject**: unpatched original iCKB
   Logic and a generated CellScript aggregate probe both reject the same
   normalized two-receipt input fixture when both receipt inputs are linked to
   a DAO header whose accumulated rate differs from the receipt data, with
   named `receipt_group_wrong_accumulated_rate` failure mode.
11. **Receipt group wrong xUDT args reject**: unpatched original iCKB Logic and
   a generated CellScript aggregate probe both reject the same normalized
   two-receipt input fixture when the xUDT output mints the exact two-receipt
   amount but uses owner-mode args that are not bound to the script-under-test
   hash, with named `receipt_group_wrong_xudt_binding` failure mode.
12. **Receipt group malformed receipt data reject**: unpatched original iCKB
   Logic and a generated CellScript receipt-data-size probe both reject the same
   normalized two-receipt input fixture when the first receipt input has
   malformed 4-byte receipt data, while the second receipt, DAO header, xUDT
   owner-mode args, and exact two-receipt xUDT output remain valid, with named
   `receipt_group_malformed_receipt_data` failure mode.
13. **Receipt group second malformed receipt-data reject**: unpatched original
   iCKB Logic and a generated CellScript receipt-data-size probe both reject
   the same normalized two-receipt input fixture when the second receipt input
   has malformed 4-byte receipt data, while the first receipt, DAO header, xUDT
   owner-mode args, and exact two-receipt xUDT output remain valid, with named
   `receipt_group_second_malformed_receipt_data` failure mode.
14. **Receipt group under-mint reject**: unpatched original iCKB Logic and a
   generated CellScript aggregate probe both reject the same normalized
   two-receipt input fixture when the xUDT output mints only one receipt worth
   of iCKB, with named `receipt_group_under_mint` failure mode. This is
   multi-receipt aggregate evidence, not duplicate receipt-id proof.
15. **Valid mint from receipt**: unpatched original iCKB Logic and the generated
   CellScript ELF both accept the same normalized receipt-to-xUDT mint fixture.
   The fixture uses the original xUDT binary with `Data1` hash type and
   owner-mode args bound to the script-under-test hash.
16. **Mint from malformed receipt data reject**: unpatched original iCKB Logic
   and a generated CellScript receipt-data-size probe both reject the same
   normalized single-receipt mint fixture when the receipt input has malformed
   4-byte data, while the DAO header, xUDT owner-mode args, and exact xUDT
   output remain valid, with named `mint_malformed_receipt_data` failure mode.
17. **Amount inflation reject**: unpatched original iCKB Logic and the generated
   CellScript ELF both reject the same normalized inflated xUDT output amount
   fixture with named `amount_inflation` failure mode. Numeric exit codes are
   recorded separately and differ by implementation.
18. **Amount deflation reject**: unpatched original iCKB Logic and the generated
   CellScript ELF both reject the same normalized under-minted xUDT output
   amount fixture with named `amount_deflation` failure mode.
19. **Wrong xUDT args reject**: unpatched original iCKB Logic and the generated
   CellScript ELF both reject the same normalized receipt-to-xUDT mint fixture
   with a fixed wrong owner-mode hash and named `wrong_xudt_binding` failure
   mode.
20. **Wrong accumulated rate reject**: unpatched original iCKB Logic and the
   generated CellScript ELF both reject the same normalized receipt-to-xUDT mint
   fixture when the receipt input header accumulated rate is wrong, with named
   `wrong_accumulated_rate` failure mode.
21. **Missing header dep reject**: unpatched original iCKB Logic and the
   generated CellScript ELF both reject the same normalized receipt-to-xUDT mint
   fixture when the receipt input is linked to a DAO header but that header is
   omitted from transaction header deps, with named `missing_header_dep`
   failure mode.
22. **DAO mature withdrawal**: unmodified original DAO ELF and a generated
   CellScript ELF both accept the same normalized phase-2 withdrawal fixture
   with withdraw/deposit headers, witness `input_type = 1`, and mature since
   `0x2003e8022a0002f3`.
23. **DAO immature withdrawal reject**: unmodified original DAO ELF and a
   generated CellScript ELF both reject the same normalized phase-2 withdrawal
   fixture when since is reduced to `0x2003e802290002f3`, with named
   `dao_incorrect_since` failure mode.
24. **DAO max withdrawal capacity**: unmodified original DAO ELF and a
   generated CellScript capacity upper-bound probe both accept the same
   normalized mature phase-2 withdrawal fixture when output capacity is exactly
   the observed original DAO boundary `123468305678`.
25. **DAO wrong deposit accumulated rate reject**: unmodified original DAO ELF
   and a generated CellScript capacity/rate probe both reject the same
   normalized mature phase-2 withdrawal fixture when the deposit header
   accumulated rate is `10000001` instead of `10000000`, with named
   `dao_wrong_deposit_accumulated_rate` failure mode.
26. **DAO over-withdraw capacity reject**: unmodified original DAO ELF and a
   generated CellScript capacity upper-bound probe both reject the same
   normalized mature phase-2 withdrawal fixture when output capacity is one
   shannon above that observed boundary, with named `dao_over_withdraw_capacity` failure
   mode.
27. **DAO missing withdraw header reject**: unmodified original DAO ELF and a
   generated CellScript input-header probe both reject the same normalized
   phase-2 withdrawal fixture when the withdrawing input is linked to a
   withdraw header in the test context but the transaction omits that withdraw
   header dep. The deposit header remains present at header dep index 0, with
   named `dao_missing_withdraw_header` failure mode.
28. **DAO missing deposit header reject**: unmodified original DAO ELF and a
   generated CellScript deposit-header probe both reject the same normalized
   phase-2 withdrawal fixture when the withdraw header remains present at
   header dep index 0 but the transaction omits the deposit header while
   witness `input_type` still points to header dep index 1, with named
   `dao_missing_deposit_header` failure mode.
29. **DAO deposit header index out-of-bounds reject**: unmodified original DAO
   ELF and a generated CellScript deposit-header probe both reject the same
   normalized phase-2 withdrawal fixture when withdraw and deposit header deps
   are present but witness `input_type` points past them to header dep index 2,
   with named `dao_deposit_header_index_out_of_bounds` failure mode.
30. **DAO withdrawal deposit-data input reject**: unmodified original DAO ELF
   and a generated CellScript withdrawal-data classifier probe both reject the
   same normalized phase-2 withdrawal fixture when the input keeps DAO type,
   mature since, withdraw/deposit headers, and witness `input_type = 1`, but
   carries deposit data `0x0000000000000000` instead of a withdrawal-request
   block number, with named `dao_withdrawal_deposit_data_input` failure mode.
31. **DAO withdrawal malformed input-data reject**: unmodified original DAO
   ELF and a generated CellScript malformed withdrawal-data classifier probe
   both reject the same normalized phase-2 withdrawal fixture when the input
   keeps DAO type, mature since, withdraw/deposit headers, and witness
   `input_type = 1`, but carries only four bytes of data `0x12060000`, with
   named `dao_withdrawal_malformed_input_data` failure mode.
32. **DAO wrong deposit header index reject**: unmodified original DAO ELF and a
   generated CellScript deposit-header witness probe both reject the same
   normalized phase-2 withdrawal fixture when withdraw and deposit header deps
   are both present but witness `input_type` points to header dep index 0
   (withdraw header) instead of index 1 (deposit header), with named
   `dao_wrong_deposit_header_index` failure mode.
33. **DAO wrong withdraw committed header reject**: unmodified original DAO ELF
   and a generated CellScript input-header probe both reject the same normalized
   phase-2 withdrawal fixture when the withdrawing input is committed to the
   deposit header instead of the withdraw header while both header deps remain
   present and witness `input_type` still points to the deposit header at index
   1, with named `dao_wrong_withdraw_committed_header` failure mode.
34. **Valid limit order**: original iCKB Limit Order and a generated
   CellScript ELF both accept the same normalized CKB-to-UDT fulfilment fixture
   with value preserved.
35. **Limit order CKB-to-UDT min-match boundary**: original iCKB Limit Order
   and a generated CellScript ELF both accept the same normalized CKB-to-UDT
   fulfilment fixture when the order pays exactly `64` shannons of CKB and
   receives exactly `64` UDT units, proving the equality boundary of
   `1 << min_match_log`.
36. **Valid limit order UDT-to-CKB**: original iCKB Limit Order and a generated
   CellScript ELF both accept the same normalized UDT-to-CKB fulfilment fixture
   with value preserved, full UDT fill, matching auxiliary UDT type hash, and
   an explicit funding input for the increased order CKB capacity.
37. **Limit order UDT-to-CKB min-match boundary**: original iCKB Limit Order
   and a generated CellScript ELF both accept the same normalized UDT-to-CKB
   fulfilment fixture when the order receives exactly `64` shannons of CKB and
   spends exactly `64` UDT units, proving the reverse equality boundary of
   `1 << min_match_log`.
38. **Limit order UDT-to-CKB no UDT paid reject**: original iCKB Limit Order
   and a generated CellScript ELF both reject the same normalized UDT-to-CKB
   fulfilment fixture when the output order keeps the full UDT amount and pays
   no CKB to the order, with named `no_udt_paid_out` failure mode.
39. **Limit order UDT-to-CKB wrong asset reject**: original iCKB Limit Order
   and a generated CellScript ELF both reject the same normalized UDT-to-CKB
   fulfilment fixture when the output order uses a different auxiliary UDT type
   script hash, with named `wrong_asset` failure mode.
40. **Limit order UDT-to-CKB insufficient match reject**: original iCKB Limit
   Order and a generated CellScript ELF both reject the same normalized
   UDT-to-CKB fulfilment fixture when value is preserved but the UDT delta is
   50, below `ckb_min_match = 64`, with named `insufficient_match` failure
   mode.
41. **Limit order UDT-to-CKB underpayment reject**: original iCKB Limit Order
   and a generated CellScript ELF both reject the same normalized UDT-to-CKB
   fulfilment fixture when the output order receives only 5,000,000,000 CKB for
   a full 10,000,000,000 UDT fill, with named `limit_order_underpayment`
   failure mode.
42. **Limit order underpayment reject**: original iCKB Limit Order and a
   generated CellScript ELF both reject the same normalized CKB-to-UDT
   fulfilment fixture when output value is lower than input value, with named
   `limit_order_underpayment` failure mode.
43. **Limit order CKB-to-UDT wrong asset reject**: original iCKB Limit Order and a
   generated CellScript ELF both reject the same normalized CKB-to-UDT
   fulfilment fixture when the input and output UDT type hashes differ, with
   named `wrong_asset` failure mode.
44. **Limit order CKB-to-UDT insufficient match reject**: original iCKB Limit Order and a
   generated CellScript ELF both reject the same normalized CKB-to-UDT
   fulfilment fixture when value is preserved but the CKB delta is below
   `ckb_min_match`, with named `insufficient_match` failure mode.
45. **Limit order no CKB paid reject**: original iCKB Limit Order and a
   generated CellScript ELF both reject the same normalized CKB-to-UDT
   fulfilment fixture when output CKB is unchanged, with named
   `no_ckb_paid_out` failure mode.
46. **Limit order UDT decreased reject**: original iCKB Limit Order and a
   generated CellScript ELF both reject the same normalized CKB-to-UDT
   fulfilment fixture when the output order UDT amount is lower than the input
   order amount, with named `udt_decreased` failure mode.
47. **Valid Owned-Owner pairing**: patched original iCKB Owned-Owner and a
   generated CellScript ELF both accept the same normalized lock-owned/type-owner
   input fixture when the owner cell's stored i32 relative distance points from
   owner OutPoint index 1 to the owned withdrawal request at index 2.
48. **Valid Owned-Owner output pairing**: patched original iCKB Owned-Owner and
   a generated CellScript ELF both accept the same normalized output-side
   lock-owned/type-owner fixture when the owner output's stored i32 relative
   distance is `-1` and points from output index 1 to the owned withdrawal
   request at output index 0.
49. **Owned-Owner output relative mismatch reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   output-side lock-owned/type-owner fixture when the owner output's stored i32
   relative distance is `1` and points to missing output index 2 instead of the
   owned withdrawal request at output index 0, with named
   `output_relative_distance_mismatch` failure mode.
50. **Owned-Owner output duplicate owner reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   output-side fixture where type-owner outputs at indices 1 and 2 store `-1`
   and `-2`, so both point to the lock-owned withdrawal request output at index
   0, with named `output_duplicate_owner_pair` failure mode.
51. **Owned-Owner output missing owner reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   output-side fixture where output 2 is a type-owner pointing to output 1, but
   output 0 is another lock-owned withdrawal request with no owner, with named
   `output_missing_owner_pair` failure mode.
52. **Owned-Owner output missing owned reject**: unpatched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   output-side type-owner fixture when the owner output points to no lock-owned
   cell, with named `output_missing_owned_pair` failure mode.
53. **Owned-Owner output script misuse reject**: unpatched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   output fixture where the script under test appears as both lock and type on
   output 0, with named `output_script_misuse` failure mode.
54. **Owned-Owner output non-withdrawal request reject**: unpatched original
   iCKB Owned-Owner and a generated CellScript ELF both reject the same
   normalized output fixture where output 1 triggers type execution and output
   0 is lock-owned but lacks DAO withdrawal request type/data, with named
   `output_not_withdrawal_request` failure mode.
55. **Owned-Owner output owner data length mismatch reject**: patched original
   iCKB Owned-Owner and a generated CellScript ELF both reject the same
   normalized output fixture where the owner output data is only three bytes
   and cannot decode the signed i32 relative MetaPoint distance, with named
   `output_owner_data_length_mismatch` failure mode.
56. **Owned-Owner output related type hash mismatch reject**: patched original
   iCKB Owned-Owner and a generated CellScript ELF both reject the same
   normalized output fixture where the lock-owned output has nonzero
   withdrawal-request data but its type script hash differs from the expected
   auxiliary withdrawal type hash, with named
   `output_related_type_hash_mismatch` failure mode.
57. **Owned-Owner output related data rule mismatch reject**: patched original
   iCKB Owned-Owner and a generated CellScript ELF both reject the same
   normalized output fixture where the lock-owned output has the expected
   auxiliary withdrawal type hash but carries 8-byte zero/deposit-marker data
   instead of nonzero withdrawal-request data, with named
   `output_related_data_rule_mismatch` failure mode.
58. **Owned-Owner related type hash mismatch reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   input fixture where the lock-owned input has nonzero withdrawal-request data
   but its type script hash differs from the expected auxiliary withdrawal type
   hash, with named `related_type_hash_mismatch` failure mode.
59. **Owned-Owner related data rule mismatch reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   input fixture where the lock-owned input has the expected auxiliary
   withdrawal type hash but carries 8-byte zero/deposit-marker data instead of
   nonzero withdrawal-request data, with named `related_data_rule_mismatch`
   failure mode.
60. **Owned-Owner owner data length mismatch reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   lock-owned/type-owner input fixture where the owner cell data is only three
   bytes and cannot decode the signed i32 relative MetaPoint distance, with
   named `owner_data_length_mismatch` failure mode.
61. **Owned-Owner relative mismatch reject**: patched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   lock-owned/type-owner input fixture when the owner cell's stored i32
   relative distance points to index 0 instead of the owned withdrawal request
   at index 2, with named `relative_distance_mismatch` failure mode.
62. **Owned-Owner script misuse reject**: unpatched original iCKB Owned-Owner
   and a generated CellScript ELF both reject the same normalized single-input
   fixture where the script under test appears as both lock and type on the
   same cell, with named `script_misuse` failure mode.
63. **Owned-Owner non-withdrawal request reject**: unpatched original iCKB
   Owned-Owner and a generated CellScript ELF both reject the same normalized
   lock-owned input fixture with no DAO withdrawal request type/data, with
   named `not_withdrawal_request` failure mode.
64. **Owned-Owner missing owner reject**: patched original iCKB Owned-Owner
   and a generated CellScript ELF both reject the same normalized lock-owned
   withdrawal request input when no matching type-owner cell is present, with
   named `missing_owner_pair` failure mode.
65. **Owned-Owner missing owned reject**: unpatched original iCKB Owned-Owner
   and a generated CellScript ELF both reject the same normalized type-owner
   input when its relative metapoint points to no lock-owned cell, with named
   `missing_owned_pair` failure mode.
66. **Owned-Owner duplicate owner reject**: patched original iCKB Owned-Owner
   and a generated CellScript ELF both reject the same normalized fixture where
   two type-owner cells point to the same lock-owned withdrawal request, with
   named `duplicate_owner_pair` failure mode.
67. **DAO wrong withdraw accumulated rate reject**: unmodified original DAO ELF
   and a generated CellScript runtime capacity-compensation probe both reject
   the same normalized phase-2 withdrawal fixture when the withdraw header
   accumulated rate is `10000999` instead of `10001000`, causing the correct-rate
   output capacity to exceed the fixture-rate maximum by `11526` shannons, with
   named `dao_wrong_withdraw_accumulated_rate` failure mode.
68. **DAO deposit-rate adjusted max withdrawal capacity**: unmodified original
   DAO ELF and a generated CellScript runtime capacity-compensation probe both
   accept the same normalized phase-2 withdrawal fixture when the deposit header
   accumulated rate is `10000001` and output capacity is exactly the fixture-rate
   maximum `123468294151`.
69. **DAO withdraw-rate adjusted max withdrawal capacity**: unmodified original
   DAO ELF and a generated CellScript runtime capacity-compensation probe both
   accept the same normalized phase-2 withdrawal fixture when the withdraw header
   accumulated rate is `10000999` and output capacity is exactly the fixture-rate
   maximum `123468294152`.
70. **DAO deposit-rate adjusted over-withdraw capacity reject**: unmodified
   original DAO ELF and a generated CellScript runtime capacity-compensation
   probe both reject the same normalized phase-2 withdrawal fixture when the
   deposit header accumulated rate is `10000001` and output capacity is one
   shannon above the fixture-rate maximum `123468294151`.
71. **DAO withdraw-rate adjusted over-withdraw capacity reject**: unmodified
   original DAO ELF and a generated CellScript runtime capacity-compensation
   probe both reject the same normalized phase-2 withdrawal fixture when the
   withdraw header accumulated rate is `10000999` and output capacity is one
   shannon above the fixture-rate maximum `123468294152`.
72. **DAO missing witness input_type reject**: unmodified original DAO ELF and a
   generated CellScript WitnessArgs input_type presence probe both reject the
   same normalized phase-2 withdrawal fixture when the witness omits
   `input_type` entirely.
73. **DAO empty witness input_type reject**: unmodified original DAO ELF and a
   generated CellScript WitnessArgs input_type non-empty probe both reject the
   same normalized phase-2 withdrawal fixture when witness `input_type` is
   present but has zero payload bytes.
74. **DAO short witness input_type reject**: unmodified original DAO ELF and a
   generated CellScript WitnessArgs input_type width probe both reject the same
   normalized phase-2 withdrawal fixture when witness `input_type` is present
   and non-empty but only one byte long instead of the expected 8-byte
   little-endian header dep index.
75. **DAO long witness input_type reject**: unmodified original DAO ELF and a
   generated CellScript WitnessArgs input_type exact-width probe both reject
   the same normalized phase-2 withdrawal fixture when witness `input_type` is
   present but nine bytes long instead of the expected 8-byte little-endian
   header dep index.

No model-level rows remain as active benchmark coverage entries. Legacy model
rows whose scenarios now have fixture-bound differential coverage were removed
from the active matrix. Duplicate receipt-id, wrong owner, and immature redeem
were moved to `non_executable_model_assumptions`: executable receipt data has no
receipt-id byte field; executable Owned-Owner fixtures encode ownership through
script placement plus OutPoint/MetaPoint relative distance rather than
`owner` / `claimed_owner` fields; executable DAO redeem maturity is represented
by input `since`, header deps, and witness input-type data rather than
`current_epoch` / `maturity_epoch` fields.
The tests
assert that model-level rows lack CKB VM execution,
that one-sided VM rows do not claim `full_differential`, and that differential
rows carry per-row execution objects with fixture hashes, transaction context
hashes, artifact hashes, status/exit/cycle data, transaction size, occupied
capacity, fee, and reject failure modes where applicable.
A future change that flips the matrix to `PROVEN` without populated
execution evidence fails the test suite.

## What Still Blocks Equivalence

- Seventy-five normalized fixtures have full original-vs-CellScript differential
  execution evidence.
- DAO hash patching is used for original iCKB Logic in the current test
  environment and is recorded in each differential execution object. This is
  functional evidence under controlled script identity, not mainnet identity
  reconstruction.
- Selected DAO phase-2 lineage, maturity, occupied capacity, rate, witness
  input_type, and cell dep substitution paths now have byte-accurate execution
  rows; full multi-input DAO redeem accounting and production fixture manifest
  closure still remain outside the proven subset.
- Mint from receipt, amount inflation, amount deflation, wrong xUDT args, wrong
  accumulated rate, and missing header dep now have partial receipt/xUDT
  differential evidence, but full receipt byte decoding and aggregate
  recomputation are still not executable CellScript lowering.
- Duplicate receipt output accounting has one executed `ReceiptMismatch` reject
  row. The model-level duplicate receipt-id double-mint fixture is now tracked
  as a non-executable model assumption rather than an active matrix row, because
  original executable receipt data does not include a receipt-id field.
- Limit Order CKB-to-UDT valid fulfilment, UDT-to-CKB valid fulfilment,
  UDT-to-CKB min-match boundary, UDT-to-CKB no-UDT-paid rejection,
  UDT-to-CKB wrong-asset rejection,
  UDT-to-CKB insufficient-match rejection, UDT-to-CKB underpayment rejection,
  CKB-to-UDT underpayment rejection, CKB-to-UDT wrong-asset rejection,
  CKB-to-UDT insufficient-match rejection, CKB-to-UDT no-CKB-paid rejection,
  and UDT-decreased rejection now have fixture-bound differential VM evidence,
  but full action-aware MetaPoint/OutPoint map remains open. First-class
  `Script` equality semantics are explicitly scoped to 0.18, not the 0.17 gate.
- Owned-Owner relative-distance pairing now has input-side and output-side pass
  rows plus input-side and output-side mismatch reject rows, with original DAO
  hash patched to a shared auxiliary withdrawal type hash for ckb-testtool where
  withdrawal classification is needed. The script-misuse and non-withdrawal
  request reject rows additionally cover the original script's lock-and-type
  role misuse path and input-side lock-owned withdrawal-shape guard without DAO
  patching; the output non-withdrawal request row covers the same
  withdrawal-shape guard on Source::Output. The input-side and output-side
  related type-hash mismatch rows cover patched auxiliary type-hash binding
  failures in both source views, and the input-side and output-side related
  data-rule mismatch rows cover matching-type/non-withdrawal-data failures. The input-side and output-side owner data length mismatch rows cover
  malformed owner-side i32 distance data in both source views.
  The missing-owner reject row covers a patched input withdrawal request whose
  owner pair is absent, and the missing-owned reject row covers the input
  owner-only direction without DAO patching. The output missing-owner and
  output missing-owned rows cover the same missing-pair directions on
  Source::Output. The output script-misuse row covers lock/type role misuse on
  Source::Output. The duplicate-owner reject row covers owner count overflow
  for a single owned metapoint on the input side, and the output duplicate-owner
  reject row covers the same cardinality failure on the output side. These still
  do not cover full
  synthetic wrong-owner resource fields or complete first-class MetaPoint map
  behaviour. First-class Script API work is tracked separately as 0.18 scope.
- Wrong owner and immature redeem are no longer active model rows. They are
  recorded as non-executable model assumptions with replacement differential
  evidence for the executable Owned-Owner and DAO maturity fixture shapes.
- `solve-tx` emits a non-executable transaction template with `can_submit=false`
  and explicit unresolved external steps; it is not a cell-selection, fee,
  change, witness, dep, or dry-run solver.

Until these blockers are closed, the correct conclusion remains: CellScript is
partially iCKB-grade for modelling and compiler-surface audit work, and now has
partial CKB VM differential evidence for deposit, deposit/receipt output
accounting, mint, selected Limit Order
paths, and selected Owned-Owner input/output pairing, cardinality,
script-role, non-withdrawal, related-cell, owner-data, and missing-pair paths,
but it does not pass a complete production-equivalence iCKB benchmark.

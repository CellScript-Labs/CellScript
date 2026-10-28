# Wide value arguments and owned storage

Status: candidate implementation after `0.33.0-alpha.1`; focused VM/checker
regressions pass, full backend integration acceptance remains pending.

## Representation

| Value | Internal argument | Return |
| --- | --- | --- |
| Primitive <=64-bit scalar | One machine word | a0 |
| `[u8; 1..=8]` | Packed word | Packed a0 |
| `u128` | Pointer and actual length, exactly 16 | Low/high words in a0/a1 |
| Hash, Address, ScriptHash, SighashAllDigest | Pointer and actual length, exactly 32 | Caller-owned hidden 32-byte destination |
| Fixed byte arrays of 9..=64 bytes | Pointer and actual length | Caller-owned hidden destination |
| Admitted fixed ordinary records | Pointer and checked length | Caller-owned hidden destination |

Arguments beyond the eight register words use the aligned outgoing stack area.
A hidden destination follows the ordinary words, including lengths. Nominal hash
types stay distinct even when their physical widths match. These are current
admitted contracts, not a promise that every larger array or dynamic value has
an unrestricted direct-return ABI.

## Confirmed waste and repair

U128 temporaries previously reserved 16 bytes in both the general fixed-byte
bank and the U128 arithmetic bank. Read-only U128 parameters also reserved
scratch that was never used. The frame planner now uses one owned buffer for
each materialized value and retains the incoming pointer/length for unwritten
parameters. Written parameters still receive owned storage; modifying them
cannot change the caller's original or another saved copy.

The measured fixture passes five U128 arguments and returns a two-U128 record.
Both versions use the same DSL source hash. All frame sizes remain 16-byte
aligned; the saving removes reserved value storage rather than moving padding.

| Configuration | Before helper frame | After helper frame | Removed storage | ELF bytes before/after | Successful cycles before/after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Standard | 1632 | 1456 | 176 | 5032 / 5032 | 7468 / 7468 |
| Internal unoptimized diagnostics | 1632 | 1456 | 176 | 5040 / 5040 | 7477 / 7477 |

This is a 10.8% reduction in this helper's owned frame. It is not a claim about
whole-program peak stack or faster execution. Three successful vectors cover
zero, upper limbs and U128::MAX; an overflowing vector must reject with the
existing error. Rejected executions are not assigned a fabricated cycle cost.
[Machine-readable before/after observations](reports/wide-argument-storage-candidate.json)
retain the exact source hash and measurements.

## Contract fixes

- Named ScriptHash and SighashAllDigest arguments now use consistent fixed-value
  length slots and caller-owned results. The independent checker recognizes
  their 32-byte widths without treating them as ordinary Hash.
- Computed zero-lock signing digests keep SighashAllDigest through IR and
  helper calls/results. `Hash::from_sighash_all` explicitly converts the value,
  and metadata retains the destination layout rather than the previous domain.
  SDK-oracle VM tests cover repeated returned digests and later calls.
- Full ScriptHash `.0` serialization and constant Hash/Address `.0` projections
  agree across typing, IR, storage and independent field checks.
- Explicit `Hash::from_bytes(bytes)` uses a typed constructor rather than an
  implicit cross-domain Move. Rebound constructors and wrong copy sources reject.
- Branch fusion only accepts primitive scalar operands. Epoch comparisons retain
  fraction semantics; `1/2 == 2/4` is not decided by packed integer equality.

## Verification and further opportunities

The 12 argument/result tests cover caller-owned returns, forwarding, mutable
parameter isolation, fixed arrays across the 8/9-byte boundary, 32-byte nominal
values, reordered record fields, large outgoing offsets, hidden stack arguments,
nested calls and both return branches. Nominal-result mutations reject wrong
width, wrong hidden argument, absent copies, type relabelling and bad source
pointers. The collection runtime's 42 tests passed after U128 storage sharing.
The complete artifact-checker integration suite also passed all 212 tests
before the final browser-summary packaging refinement. Native ABI emitters
and the independent checker were unchanged by that refinement.

Length descriptor elimination for internally proven fixed arguments could save
ABI words. Forwarding a caller's return destination could avoid some copies.
Neither change is implemented here: each needs its own source/length/alias and
independent machine contract. Entry and dynamic-source length checks stay intact.
The transport tests with externally supplied fixed bytes prove byte preservation,
not that a transaction message was computed or authority was established. The
separate signing tests compute the message and compare it with the CKB SDK.
Full backend integration and clean-source acceptance are still pending.

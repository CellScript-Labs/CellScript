# Fixed policy parameter decoder evidence

This is an optional prerequisite for issue #28, not a complete interface receipt
or compatible-open admission. Ordinary bundle inspection retains its existing
meaning. Request this stronger evidence explicitly through
`cellscript_artifact_checker::entry_codec::check_fixed_policy_parameter_decoders`.
It requires native source catalogs and actual ELF, metadata, lowering record and
source-map bytes. No new CellScript syntax or runtime codec is introduced.

The returned `CheckedFixedPolicyParameterDecoders` has a private constructor and
no deserializer. Its canonical record uses
`cellscript-fixed-policy-parameter-decoder-v1`; its identity uses the separate
`cellscript-fixed-policy-parameter-decoder-id-v1` domain. The record binds the
checked module API identity and each external tag's action, parameter order,
name, type, source, transport, payload offset, width and ABI register placement.
The contained module projection retains the actual artifact inspection report.
Identical decoder contracts can have different artifact identities; neither
identity establishes source or business-behavior equivalence.

## Accepted machine profile

Only `policy-witness-v1` adapters are eligible. Fixed witness payloads are at
most 1536 bytes and use at most eight ABI registers. Scalars are unit, u8, u16,
u32, i32 and u64. U128, Address and Hash use fixed bytes; arrays and tuples
recursively composed from these types are also eligible. All bit patterns of
these witness types are representable. Values up to eight bytes use scalar
transport; wider values use pointer/length transport. I32 additionally requires
the exact 32-bit sign-extension sequence.

The independent checker first checks the whole bundle and source declarations,
then checks the actual decoder instructions, including private frame, bounded
copy placement, exact length, every CSARG magic byte, little-endian byte loads,
shifts, destination registers, fixed-byte pointers/lengths and action target.
Shared decoders must preserve the same guards and return through their checked
done block. Interior incoming control-flow edges reject. The existing checked
branch-relaxation view is used without discarding actual machine evidence.

Runtime-bound Cell/reference parameters are certified only for their null-pair
transport. This does not certify transaction Cell-data decoding, ownership,
cardinality, peer obligations, codecs or lifecycle execution. Bool (including
nested bool), enums, named witness types, dynamic witness values, Script.args,
outgoing stack arguments and oversized/noncompact frames reject this evidence.
There is no fallback to a weaker inspection result.

Fixed ceilings apply before parsing: 4 MiB per input and 16 MiB across the four
files. Caller checker budgets may narrow the limits, never enlarge them. No
artifact, metadata, resource or runtime ceiling is increased.

## Evidence and remaining boundaries

Focused tests cover O0–O3 contracts, shared decoders, scalar and aggregate widths,
sign extension, exact payload/register limits and unsupported profiles. Machine
mutations rebind ELF and all sidecar hashes: ordinary inspection accepts these
mutations while this decoder check rejects wrong byte offsets, registers,
shifts, magic and signed extension. Real CKB-VM tests exercise valid cardinalities
at nonzero group positions, malformed magic/length/trailing bytes, and signed
arguments through both shared variants. Small-aggregate coverage is a machine
decoder check, not a claim that arbitrary aggregate expressions are executable.

Full issue #28 still needs complete Cell-data/source codec and deployment
receipts, package interface ownership, all-member admission and nominal runtime
handles. Issue #29 still needs independently enforced participant obligations.
Stable admission also requires the independent security review specified by
those issues. Full CI remains deferred until the user's entire queue is
implemented.

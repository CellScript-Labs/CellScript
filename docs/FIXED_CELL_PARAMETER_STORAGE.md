# Fixed Cell parameter storage evidence

`cellscript_artifact_checker::fixed_cell_storage::check_fixed_cell_parameter_storage`
combines the independently checked fixed policy parameter decoder and fixed
Cell read gates with actual source-parameter pointer reception. It is an
optional #28 prerequisite. It grants no complete interface receipt or open
admission and changes no compiler output, ordinary checker requirement or
resource ceiling.

The private `CheckedFixedCellParameterStorage` has no deserializer or raw
constructor. Its canonical `cellscript-fixed-cell-parameter-storage-v1` record
binds both prerequisite identities and each eligible Cell's typed local/source
identity, ABI register, scalar pointer slot, size/buffer slots and actual spill
and receiver addresses. The identity domain is
`cellscript-fixed-cell-parameter-storage-id-v1`. Prerequisite identities bind
all four exact artifact bundle inputs.

## Finite profile

Only directly bound non-output source parameters are eligible. Their checked
policy transport must be a runtime-bound null pair; output observations and
observations without a source local do not establish parameter storage. A
bundle without eligible parameters rejects rather than returning empty proof.
Helper-derived or alias-derived parameter provenance rejects.

The checker requires the actual compact frame prologue, at most eight initial
argument register spills and unique spill slots/registers. Spills must be
aligned and within the scalar region, except an identified Cell parameter's
length register may use that Cell's checked read size slot. The source pointer slot is independently
computed as `source_id * 8`, not accepted from a producer slot-valid flag. At
most 256 local source IDs must be unique and fit below the first checked read
size slot. Checked parameter read buffers must be disjoint from other checked
Cell reads; the compact frame remains at most 2047 bytes.

The pointer's actual ABI register must spill into that exact slot. Immediately
after the checked syscall status and exact-length gates, decoded RISC-V must
form the checked buffer pointer and store it into the same source slot.
External flow into the combined read/guard/receiver interior rejects. This
extends read-gate checks to the receiver, including its first instruction.

## Evidence and limits

O0–O3 fully rebound mutations alter receiver bases/registers, initial ABI
spills, local source IDs and overlapping argument spills. Ordinary inspection
and the read-gate certificate accept these changes; this additional check
rejects them. Output-only fixtures reject. The same storage check runs before
the real VM fixture's 16 exact-byte positive cases and 64 short/trailing data
negative cases at zero and nonzero absolute group positions.

This evidence ends at pointer reception. It does not certify later field loads,
pointer lifetime, all scratch storage, general alias freedom, helper closure,
source-to-machine equivalence, complete signed/boolean/enum codecs, role or
cardinality authorization, peer execution, deployment or Type-hash history.
Those applicable contracts remain necessary for full #28/#29 admission.
Independent security review remains required before stable admission. Full CI
is deferred until the entire requested implementation queue is complete.

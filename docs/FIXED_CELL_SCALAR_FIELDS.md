# Direct fixed Cell scalar field evidence

`cellscript_artifact_checker::fixed_cell_fields::check_fixed_cell_scalar_fields`
is an optional #28 prerequisite. It owns the private parameter-storage check,
which owns the independent policy decoder and read-gate checks. It certifies
direct unsigned field materializations at actual registers. It grants no full
interface receipt, source equivalence, statement truth or open admission.

The private object has no deserializer or raw constructor. Its canonical record
uses `cellscript-fixed-cell-scalar-fields-v1` and its identity domain is
`cellscript-fixed-cell-scalar-fields-id-v1`. Records commit to the storage
identity, parameter/field/type, packed offset and width, actual pointer-load
and decoder-end addresses, destination register and native-load form. The
prerequisite chain binds all four exact bundle inputs.

## Finite machine profile

Direct fields of an eligible source Cell parameter may be u8, u16, u32 or u64.
Their widths and offsets come from independently checked layouts and direct
typed field operations. Every actual load from the parameter's source pointer
slot must match a field materialization; every required direct field key must
have a materialization. Multiple materializations of one field are permitted.
Aliases, nested/signed/wide fields and unproved helper-derived accesses reject.
The initial implementation conservatively rejects field operations belonging to
another Cell parameter in the same owner; it does not grant a multiple-Cell
field profile.

The actual receiver must dominate the pointer load. Decoded instructions must
load the pointer into t4 and then reconstruct little-endian bytes with an
initial zero, ordered LBU, exact shifts and OR into t0/t1. An aligned eight-byte
u64 field may instead use an actual native LD. Incoming control flow into either
sequence's interior rejects. This ends at the decoded register value, rather
than proving every subsequent source-variable assignment or predicate.

Owners have at most 4096 instructions; the bundle has at most 256 field sites.
Owned intervals are indexed before traversal, without scanning every block for
every instruction. Stores must remain in the compact owned frame, preserve
saved caller RA/FP, leave the checked Cell buffer intact and avoid overwriting
the received pointer. Unknown store bases, unproved direct buffer aliases,
body stack-base changes, return-address substitution, incoming owner-interior
flow and returns bypassing frame restoration reject.

Only the existing Cell membership helper's checked memory/return boundary is
allowed as a call. Its actual 96-byte private frame, frame-local loads/stores,
protected registers, three bounded hash observations, initialization flow and
restored return are checked. Unknown calls, escaping/backward helper flow and
fallthrough outside owned instructions reject. This proves caller storage
preservation; it does not certify that helper's role authorization semantics.

Inline owner syscalls must be a checked Cell read, or an independently bounded
capacity/hash observation. Actual Script-hash reads use 32 bytes. Cell-field
reads admit only the pinned capacity (field 0, eight bytes), LockHash (field 3,
32 bytes) and TypeHash (field 5, 32 bytes) representations. Direct stack
pointers, an actual dominating 64-bit capacity initialization and zero byte
offset are required. Initialization through the syscall must be straight-line,
without outgoing control flow or incoming interior flow. Buffer/size writes cannot overlap scalar locals, saved
state or any checked Cell read. A register alias cannot substitute these
pointers. Observation success, group-count predicates and role authorization
remain outside this memory evidence.

The shared checker register analysis recognizes all accepted destination-write
opcodes, including RV64 word arithmetic. ADDIW/ADDW/SUBW constant facts use
32-bit truncation and sign extension; unsupported word forms clear facts.
Opaque calls invalidate caller-saved facts and CKB syscalls invalidate a0's
previous input fact. Pointer definitions cannot survive an intervening word
write, call or syscall return.

## Evidence and limits

Compiler O0–O3 fixtures cover bytewise decoding at offsets 0, 1, 3 and 7, with
u8/u16/u32/u64 widths. Their 96 fully rebound pointer/base/offset/initial-value/
shift/OR mutations pass ordinary inspection and parameter-storage checking but
fail this check. Another 24 rebound helper capacity/store/buffer/register
mutations fail here. Alternate, padded native-LD ELF fixtures are independently
accepted after rebinding every identity; eight wrong native offset/width
mutations reject. This alternate machine evidence does not claim that the
original compiler fixtures emitted LD.

Another 32 ordinary-accepted/storage-accepted rebound mutations cover inline
observation capacity, store widths and pointer bases; live Cell-pointer aliases
passed to new declared syscalls; and ADDIW/ADDW/SUBW pointer substitutions.
Eight alternate actual hash-observation forms and four ADDIW syscall-number
materializations pass independent checking. These alternate ELF cases grant
memory/constant evidence, without claiming the original compiler emitted them
or that their downstream source predicates are equivalent.

Real CKB-VM tests run the compiler's bytewise forms at zero/nonzero absolute
group positions: 16 valid literal-byte cases, 184 single-byte changes rejecting
with error 5, and 64 short/trailing/512/513-byte cases rejecting with error 4.
Signed Cell field fixtures pass prerequisite checks but reject this unsigned
profile. Nested Cell access remains rejected by the production executable
surface with E2105; that boundary was not weakened to manufacture evidence.
At the existing 64-variant/64-read limits, 256 field sites pass and 257 reject.

Complete public codecs, output serialization, general helper/lifetime closure,
stable owned interface identities, deployment/history, all-member authorization
and nominal runtime handles remain necessary for full #28. #29 and #44–#46
remain unfinished. Independent security review remains required and unassigned.
Full CI stays deferred until the whole requested implementation queue is done.

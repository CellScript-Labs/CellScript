# Fixed Cell read gate evidence

`cellscript_artifact_checker::fixed_cell_reads::check_fixed_cell_reads` is an
optional prerequisite for issue #28. It checks actual ELF, metadata, lowering
and source-map bytes through the independent module checker first. Ordinary
inspection, compiler output and runtime ceilings are unchanged. This API does
not return an interface receipt or grant compatible-open admission.

The private `CheckedFixedCellReads` object has no deserializer. Its canonical
record uses `cellscript-fixed-cell-read-gates-v1` and its identity uses
`cellscript-fixed-cell-read-gates-id-v1`. The record binds all four exact input
hashes, module projection, actual read addresses, typed Cell locations, fixed
widths, buffer/size offsets and checked failure codes. It is artifact-specific;
matching identities do not establish behavioral equivalence.

## Initial machine profile

For every ECALL in a typed body inside the checked text range, the checker
requires a known syscall number. This initial profile permits LOAD_CELL_DATA
and the accompanying LOAD_SCRIPT_HASH / LOAD_CELL_BY_FIELD observations; those
other observations do not acquire codec or role-authorization evidence here.
Every fixed typed Cell binding must have exactly one checked read at its source
and ordinal. Missing, duplicate, ambiguous and dynamic locations reject.

The read uses a compact stack frame of at most 2047 bytes, a 512-byte buffer
immediately following its aligned eight-byte size slot, and constant source and
index. The checked byte sequence initializes capacity, sets actual buffer/size
pointers and zero byte offset, materializes the bound source/ordinal and invokes
syscall 2092. Buffer bounds exclude saved FP/RA. This does not independently
prove alias freedom of other frame storage or all field consumers.

Immediately after the syscall, actual control flow must reject nonzero status
through a checked terminating runtime error. It must then load the same size
slot and require the independently derived exact Cell width, rejecting both
short and trailing bytes with error 4. External incoming flow into the middle
of setup or either guard rejects. Relaxed branches retain the existing checked
logical view. No producer `return_code_checked` bit substitutes for these tests.

Widths are derived recursively from representable fixed primitive, array,
tuple and concrete struct/resource field layouts. Traversal is bounded to depth
32 and 4096 visits. Cell widths exceeding 512 bytes reject. Bool, enum and
dynamic canonicality lack this profile and reject, including nested occurrences.
The check allows at most 64 actual reads and at most 4096 instructions in each
classification block. Existing 4 MiB/file and 16 MiB/bundle preflight and caller
instruction budgets apply before parsing and cannot be enlarged by this API.

## Evidence and limits

O0–O3 tests rebind ELF, block digests and every sidecar identity. Ordinary
inspection accepts changes to capacity, size/buffer bases, byte offset,
source/index, syscall number, status/length branch conditions and comparison
operands; this optional check rejects them. Real CKB-VM fixtures exercise mint
and burn at zero and nonzero absolute group positions. Exact little-endian
eight-byte data succeeds; 7, 9, 512 and 513 bytes reject with error 4.

Runtime helper bodies and unowned generated library text are outside this
certificate. It does not prove helper-call closure, decoded field values,
parameter-to-Cell pointer provenance, general signed/boolean/enum codecs,
source equivalence, group/cardinality authorization, peer execution, deployment
identity or Type-hash history. Full issue #28 still requires those applicable
contracts plus complete receipts, nominal identities and bounded runtime
handles. Independent security review remains required before stable admission.
Full CI stays deferred until the entire requested queue is implemented.

The separate [fixed Cell parameter storage check](FIXED_CELL_PARAMETER_STORAGE.md)
adds direct source-parameter ABI spill and immediate pointer-receiver evidence.
It does not expand this read-gate certificate into a field codec or full receipt.

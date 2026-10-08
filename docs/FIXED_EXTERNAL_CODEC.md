# Fixed external codec and availability prerequisite

`external_codec::check_fixed_external_codec` consumes actual ELF, metadata,
lowering and source-map bytes. Its private result cannot be reconstructed from
exported JSON. It owns the independent policy decoder and, where applicable,
Cell read/storage/scalar-field evidence. All byte identities remain bound through
those checked objects.

The versioned `policy-unit-scalars-flat-unsigned-cell-v1` profile checks every
concrete public callable against exactly one actual policy-witness dispatch tag.
Unit returns and no output serialization are required. A public helper retained
in ELF without external dispatch, or a pruned public function, fails instead of
acquiring executable availability from its declaration. Uninstantiated generic
functions/types remain explicitly `declaration-only`; concrete generic instances
require a separate supported profile and reject here.

Every concrete public nominal type needs its uniquely bound, independently
checked layout, including types unused by the selected action. This finite
profile permits 1–64 contiguous unsigned u8/u16/u32/u64 fields and at most 512
encoded bytes. Nested/array fields, enum tags, booleans, empty layouts, missing
layouts and dynamic representations reject. Source signatures, effects,
qualified declarations and all other module axes remain in the separately
checked module projection; a coincidental scalar width does not create ownership.

Each external action admits at most one directly bound non-output Cell. Witness
parameters use u8/u16/u32/u64/i32 scalar transport. No references, common-check
call ABI, hidden results, Script.args decoder or output Cell encoder belongs to
this profile. Actual source-ID locals must have disjoint bounded frame slots;
all initial argument-register spills and the Cell size spill must agree with
those slots. Incoming flow cannot skip or repeat this reception. Parameters
remain limited to eight machine argument registers and a compact <=2047-byte
frame. Existing decoder/Cell budgets and aggregate bundle ceilings remain intact.

The certificate establishes this initial external reception and bitpattern
layout boundary. It does not prove application predicates, source-to-machine
behavior equivalence, outputs or general helper/lifetime closure. Its exact
identity changes when machine bytes change even if a directional module
comparison permits the same public contract.

This is a partial #28 prerequisite, not the complete H1 receipt or H2 admission.
Nominal resolver-owned I, actual deployment/Script args/raw and resolved deps,
Type history, immutable policy authorization, all-member admitted receipts,
builder/runtime/editor parity and independent security review remain necessary.
Verifier-specific entry profiles are not admitted by this policy-action profile.
Full CI remains deferred until the whole requested implementation queue is ready.

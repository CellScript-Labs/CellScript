# Independent cache ownership for scalar forwarding Locks

[Issue #101](https://github.com/CellScript-Labs/CellScript/issues/101) records a
general artifact-admission mismatch. A Lock with three scalar parameters can
forward them to an ordinary helper and reuse a dead parameter slot for the
returned predicate. Its physical scalar region occupies 24 bytes, while its
four semantic local identities have an identity-derived upper bound of 32
bytes. An exact-read cache starting at byte 24 is valid in that layout.

The checker retains its conservative identity-derived boundary for ordinary
bodies. When that boundary reports a cache overlap, a separate finite machine
contract can establish the physical layout for a scalar forwarding Lock:

- One through eight immutable scalar parameters, with no reference or outgoing
  stack arguments, and one typed block containing a single ordinary helper
  call followed by returning that call's Boolean result. The call forwards
  each declared parameter in declaration order.
- The complete actual prologue, owned frame, RA/FP stores, cache initialization,
  cache register save and retained pointer, and every parameter spill are
  decoded. Parameter slots must be distinct private eight-byte regions entirely
  before the actual cache base.
- Every outgoing parameter load must read its corresponding proven spill. The
  actual relocated call must enter the declared helper's prologue. Its result
  store must also stay before the cache. An optional reload reads exactly that
  freshly written result slot.
- The Boolean predicate, false-result process abort, success status and complete
  cache/RA/FP/frame restoration are decoded through the final return. Both the
  ordinary and hot-cache headers have their complete save/restore contract.
  Extra instructions, other branch destinations and external partial entry
  reject.

This proof covers every scalar read and write in the forwarding entry. The
ordinary lifetime analysis still checks the called helper and protects the
cache against subsequent stores, unrelated helper effects and overlapping
retained buffers. The call is bound by its typed helper identity; application
or function names grant no admission. No producer-supplied physical map is
treated as authority. The profile establishes this complete forwarding shape;
it does not establish a general scalar allocator or liveness equivalence proof.
Other shapes retain their existing conservative admission boundary.

The non-financial fixture is
[`document_cache_scalar_forwarding.cell`](../tests/fixtures/document_cache_scalar_forwarding.cell).
It composes a typed Document read and an exact byte read through ordinary
helpers. The owning regression executes real Lock groups using public entry
witness encoding, including a renamed eight-parameter entry and the hot-cache
header. Mutations rebind artifact, block and sidecar identities before checking
parameter loads/stores, result storage, frame adjustments, actual call targets
and partial entry.

The focused command is `cargo test --locked -p cellscript --test artifact_checker
cache_forwarders -- --nocapture`. This focused evidence is separate from
backend, integration and production gate evidence.

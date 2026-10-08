# All-bundle module catalog evidence

`interface::check_module_catalog` in the standalone artifact checker consumes an
actual required four-file bundle and 1–32 actual candidate bundles. It returns
`CheckedModuleCatalog`, a private checked object without a deserializer. This is
API coherence evidence for issue #28, separate from complete interface receipts,
package families, codecs, deployment/history admission and policy authorization.

Each input file is at most 4 MiB; the required bundle and every candidate together
are at most 16 MiB. Count and aggregate byte preflight happens before any parser
runs. Repeated input bytes count again. Exactly 16 MiB reaches ordinary parsing;
16 MiB plus one byte rejects even when each individual file fits. Caller checker
budgets may narrow per-bundle limits, never enlarge the fixed catalog limits.

Every candidate independently passes whole-bundle inspection and module
projection. Every required module contract must match directionally; candidate
additions remain permitted. A valid selected member cannot hide invalid or
incompatible unselected members. No partially checked catalog is returned.

The canonical record uses `cellscript-checked-module-catalog-v1` and its identity
uses `cellscript-checked-module-catalog-id-v1`. It records the checked API identity
and independently recomputed CKB BLAKE2b hashes of the exact ELF, metadata,
lowering-record and source-map files for required and candidate tuples. These
four file hashes use 64 lowercase hexadecimal digits. Candidate order remains
significant. `check_unchanged_inputs` rechecks count, fixed input ceilings and
every raw file fingerprint, rejecting replacement, reordering or even metadata
whitespace changes after checking. Exported JSON alone grants no checked state.

## Native source snapshots

`package::frozen_interface::freeze_module_catalog` consumes actual private
`FrozenPackageModule` values produced by native locked compilation. The native
catalog owns those immutable bundles and binds every source-context identity to
the independently checked catalog identity. Pinned chain ID and genesis must
agree; this is host environment context, not an on-chain genesis syscall.
Source compilation and its per-module source preflight occur earlier; the
catalog's shared wire budget applies before its fresh all-bundle inspection.

Its separate record and hash domains are `cellscript-frozen-module-catalog-v1`
and `cellscript-frozen-module-catalog-id-v1`. Source paths may change after a
snapshot is created; the object proves its stored snapshot, not the path's
current contents. Source ownership is still snapshot ownership, not a stable
publisher/package interface family. The native source declaration comparison
uses the same canonical declaration identity ordering as the emitted record;
field, parameter and generic binder order remain significant.

## Evidence and limits

Focused tests cover every cardinality from one to 32, exact byte ceilings,
incompatible nested layouts, missing evidence, all four malformed unselected
files, narrower checker budgets, input substitution/order/count changes, pinned
network conflicts and native snapshot immutability. Reusing one code bundle can
be legitimate for different concrete Script args later; this layer neither
rejects nor admits duplicate/conflicting deployment receipts.

Neither factory creates a full admission receipt, `AuthorizationSet` or nominal
source handle. It does not establish complete Cell-data codecs, source/behavior
equivalence, actual Code Cell/CellDep/Script args, live Registry status, Type
replacement history, peer execution or transaction signing authorization.
Those remain mandatory work for #28/#29. Their independent security review is
still unassigned and required before stable admission. Whole-queue CI remains
deferred until all requested implementations are complete.

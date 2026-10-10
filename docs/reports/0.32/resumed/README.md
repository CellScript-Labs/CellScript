# Resumed 0.32 evidence

Storage note (2026-10-10): this historical replay's JSON payloads are retained
in the [pinned Git archive](../README.md#historical-json-storage). The original
manifest and logs remain here; restore archived payloads before verifying the
complete manifest. The following results retain their original source scope.


This archive preserves the clean `6dba17681e755720081c678015324cdd0cf46bb9`
backend run from 2026-10-08. [manifest.json](manifest.json) binds the stored and
uncompressed bytes. It includes the complete backend log, the pinned CKB
stateful acceptance report and the default counter node report. The source
provenance inside the CKB report records a clean tree at that exact commit.
These receipts do not describe later checked-result ABI or Vec/context changes.
Complete CI for the remaining queue is deferred until implementation finishes.

## #39 and #40 disposition

The second retention candidate remains rejected. Its
[original archive](../retention-second/README.md) is already published on `0.32`
in `6dba1768`; neither candidate is enabled in the default backend. Rechecking
all twenty original files reproduced every stored/raw hash and byte count. The
native `compare-cost-evidence` consumer replayed the archived main/multi-script
pairs and passed 6,782 comparable metrics with no regression. This replay checks
the finite comparison; it does not claim a new execution of the candidate.

The original 35 paired probes and 1,707 paired traces retain their original
source snapshots. Only four corpus executions, across two scalar fixtures,
saved one cycle. ELF size, stores and observed stack did not improve. The
measured decision closes this candidate evaluation without adding a general
allocator or weakening helper scratch ownership. The wider research tracks
retain the concrete adopt/reject/defer decisions in [RESEARCH.md](../RESEARCH.md).
The independent cost review was waived, not performed. Original budgets and
matched Rust references remain unchanged.

## #42 evidence boundary

The clean backend log includes native/CCC client and codec tests, ordinary VM
migration, separate migration resource replay and artifact reproducibility.
The default node report commits initialization and two single-state updates,
and rejects malformed successors and reused proofs under the pinned local CKB
node. It uses the public test setup and makes no production setup admission.

The fresh node run in this archive did not set `CELLSCRIPT_COUNTER_CCC=1`.
Therefore it does not reproduce the historical CCC old-update → authorized
migration → new-update node sequence. A clean-source replay explicitly enabling
that workflow remains required before the resumed P1 acceptance is recorded.
Immutable legacy Cells keep their existing non-migratable disposition. Broader
multi-Cell, manifest lifecycle and proving-cost work remains #44, #45 and #46.

Neither this archive nor a successful local test authorizes a release, setup
ceremony or public deployment. #28/#29 still require their independent security
review; the cost-review waiver does not apply to them.

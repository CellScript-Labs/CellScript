# Controlled ZK application cost study

Issue #46 uses the existing exact private counter and its paired migration.
This research harness does not change the circuit, verifier, setup admission,
compiler modes or #40's cost corpus. Codex owns implementation and review of
the measurement harness. No new cryptographic/runtime/ABI contract is admitted.
The maintainer requested no CI or gates; reproduction uses focused tests.

## Pilot protocol, fixed before measurement

Use the public deterministic migration fixture and its exact key 0. Measure
ordinary old-key update and the maximum admitted migration envelope (64 inputs,
64 outputs, 64 dependencies, 16,384 serialized bytes). The latter still contains
one protected counter and its configuration; it is not #44 multi-Cell evidence.
Measure a late-invalid proof by changing the raw transaction after proving while
retaining canonical proof bytes and legal application state. Rejection must
reach the pairing boundary; malformed proof decoding is not this case.

Host candidates form a finite 2 × 2 comparison: existing automatic Rayon thread
count versus four workers, each with one-shot checked key loading versus reuse
of an already checked key. Compare each individual change with the one-shot
automatic reference before considering their combination. The existing CLI and
client defaults remain unchanged until evidence supports a separate decision.
Every proof is freshly produced over the exact transaction statement; no proof,
statement, key validation or verifier check is cached or omitted.

Each observation uses a fresh process, one warmup and three measured repetitions.
The filesystem cache is warm and is not forcibly dropped; the report captures
CPU/kernel/memory/affinity, available parallelism and exact binaries/source/key
identities. Jobs run serially with a 60-second per-process stop bound. This bound
is a pilot safety limit, not a measured performance ceiling.

The parent measures elapsed wall time with `Instant`; the worker reports checked
key-load and proving-call durations separately. GNU Time 1.9 reports process
maximum RSS in KiB, including code, checked key loading, warmup and proof work.
It is not a pure prover heap peak, and phase-local RSS is unavailable. Reused-key
observations exclude key loading from the reported proving request latency but
retain it in process memory and total startup time. Setup is measured separately.

The existing scheduler supplies authoritative Script-group cycles, all-group
success/rejection and an independent instruction replay. Each VM/EXEC generation
keeps its own stack peak, instruction cycles and synchronous syscall cycles;
shared scheduling/loading costs are separate. The sum of VM stack peaks is never
reported as a stack requirement. On rejected transactions, ordinary all-group
cycle totals may be unavailable; per-group measured rejection remains explicit.

Before final acceptance, retain pilot observations and freeze numeric host and
application ceilings in a versioned record. Existing on-chain maxima are never
relaxed. Include proof/public-input/VK/PK, witness/full transaction, deployed ELF
and occupied-capacity sizes. Any omitted metric is null with a reason.

The thread control follows [Rayon's documented environment option](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html#method.num_threads).
An additional proof system, batching, aggregation or recursion remains deferred
unless its distinct statement/codec/key/setup/verifier contract and a concrete
implementation issue are supplied. A finite measured decision does not establish
a global optimum.

## Complete protected-set extension

The separately versioned `cellscript-zk-set-costs-v1` corpus consumes #44's
actual 1/2/4-counter transactions. It constructs and proves every leaf from
its resolved input and actual successor and checks each public-input vector
against the independent-consumer fixture. Four counters mean four fresh proofs;
measuring only the first input is not a complete-set observation.

The finite comparison keeps automatic Rayon and sequential leaves. Reference
requests load and validate the same PK separately for each leaf, matching the
existing per-invocation API cost. The candidate loads/checks once per request
and shares that immutable key across its leaves, using the existing native API.
Both produce and locally verify every proof. Neither retains a key across
requests or changes proof composition. Byte equality under identical test RNG
seeds is required for every candidate/repetition. The earlier four-worker
candidate was rejected, so no new combination with that candidate is selected.

Fresh workers retain one warmup and three measured repetitions, warm filesystem
cache and whole-process GNU Time peak RSS. Request timers include checked key
loads and every proof/local verification. They include per-leaf temporary-key
disposal; the shared key's final disposal occurs after that request timer and
is included in whole-process elapsed time. This is a native API experiment,
not a CLI launch/IPC benchmark. Setup generation time is separate, with its
peak RSS unavailable. Workers have a 180-second timeout; the pilot-derived
acceptance limits are independently frozen in `set-limits.json`.

The full application replay executes every protected Type group and fee Lock,
then traces every lifecycle/parent/child generation separately for valid and
late-invalid transactions. It preserves #44's pre-frozen 4-counter,
64/64/64-Cell, 16,384-byte, 500-million-total-cycle and 32,768-byte-per-VM-stack
limits. The original single-state `limits.json` and #40 budgets remain unchanged.

# ZK application cost corpus v1

Issue #46's single-counter baseline and complete 1/2/4-counter extension are
implemented with separately frozen pilot/acceptance evidence. A maximum
fixed-pair transaction is not evidence for multiple protected counters.
This record makes no release,
new setup, profile admission or public deployment claim. CI and gates were not
run, following the maintainer's 2026-10-12 instruction.

The [measurement contract](../../../CELLSCRIPT_ZK_COST_STUDY.md) was written
before the pilot. Pilot raw observations were archived before
[`limits.json`](limits.json), SHA-256
`4be69d3b4201527ca0c06958020c0133475bdba6610ba026c125822c1420b8f1`,
froze the ceilings. Separate host and all-group acceptance runs then passed
against that exact digest. Existing #40 source, references and budgets were
unchanged. `manifest.json` binds archived report bytes and current harness source.

## Host results

Intel Core i9-13900H, 20 available logical CPUs, approximately 31 GiB physical
memory, Rust 1.97.1, release build, warm filesystem cache. Processes run serially;
the host is not reserved exclusively. Each fresh worker runs one warmup and
three measured requests. GNU Time 1.9 records whole-process maximum RSS,
including key loading and warmup. It is not a phase-local heap measurement.

| Case / policy | Median request ms | Median prove + local verify ms | Process peak RSS KiB |
| --- | ---: | ---: | ---: |
| Ordinary / reference | 6,615.10 | 769.75 | 606,296 |
| Ordinary / four workers | 8,256.75 | 1,054.21 | 585,932 |
| Ordinary / reused checked key | 783.01 | 783.01 | 606,508 |
| Ordinary / combined | 1,071.39 | 1,071.39 | 583,668 |
| Maximum / reference | 6,842.29 | 794.97 | 606,036 |
| Maximum / four workers | 8,311.30 | 1,051.82 | 581,416 |
| Maximum / reused checked key | 785.66 | 785.66 | 606,388 |
| Maximum / combined | 1,069.82 | 1,069.82 | 586,364 |

See [`decisions.json`](decisions.json) and the raw observations for exact values.
Request timing includes checked PK loading for one-shot calls, while a reused
key was fully checked at startup. Every request still produces a fresh proof and
locally verifies it. Equal public test RNG seeds give byte-identical proofs
across all four policies. This isolates host scheduling/loading effects from
the relation and transport.

The major host cost is checked key loading, not the proving call alone. Reuse
through the existing native API reduces request latency substantially, without
materially reducing process peak RSS. Four workers save a little RSS but slow
both cases, including the combination. Retain the existing CLI defaults and
native caller-owned key API. An automatic persistent CLI cache is deferred until
its authenticated identity/invalidation and secret lifetime contracts exist.

## On-chain accounting

The ordinary valid transaction consumes 110,202,074 cycles, with 1,198 serialized
bytes and 370 witness-vector bytes. The maximum valid transaction consumes
110,919,123 cycles at 16,384 serialized bytes and 5,021 witness-vector bytes.
It has 64 inputs, outputs and dependencies, with one protected counter and its
paired configuration. All required Script groups accept the same transaction.

The child verifier accounts for approximately 109.8–110.0 million attributed
instruction/syscall cycles. The reports keep parent/lifecycle/child VM generations
separate and retain shared scheduling/loading costs. Per-group authoritative
cycles exactly match independent instruction replay; valid group totals equal
the all-group oracle. These are distinct from individual attributed VM costs.

Changing only a fee output after proving reaches error 79 after the expensive
child verification, for both ordinary and maximum transactions. The ordinary
all-group API returns no cycle total on rejection, so those totals are null;
the measured rejecting group and other independent group observations remain
available. No sum of separately executed rejected groups is presented as an
actual all-group total. Fee Locks in this diagnostic fixture are always-success;
[the separately identified signed CCC node run](../zk-lifecycle/README.md)
records real fee-signature costs and confirms the existing application flow.

Each VM retains its own observed stack: the child peaks at 20,208 bytes, parent
at 7,152, counter lifecycle at 1,584 and configuration guard at 1,664. They are
not added together. Heap peaks remain unavailable. The reports contain every
deployed code/data Cell's byte size, exact hash, capacity and occupied capacity.
The verifier is 197,384 bytes; the two parents are 14,560 and 14,568 bytes; the
lifecycle/configuration ELFs are 67,536 and 65,024 bytes. These deployment costs
are separate from each update's witness and transaction costs.

The proving key is 32,056,112 bytes, VK 744, canonical proof 128, canonical
statement 228, public-input vector 484 and parent/child request 464 bytes.
Setup generation time is reported separately; its peak RSS is not measured.
All setups and witnesses in this corpus are deterministic public test fixtures.

## Decisions and reproduction

Six proof compression trials expand 128-byte canonical proofs to 151-byte gzip
records, so gzip transport is rejected. No codec change is adopted. Additional
systems, batching, aggregation and recursion are deferred with explicit missing
statement/key/setup/verifier and authorization contracts in `decisions.json`.
No additional proof profile is proposed, so no implementation/admission issue
is created. These finite outcomes do not establish a global optimum.

Produce the existing migration fixtures first. The following opt-in tests create
fresh output directories and refuse to overwrite previous measurements:

```bash
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test migration
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test proving_costs application_proving_cost_pilot -- --ignored --nocapture
cargo test --locked --release -p cellscript --test zk_application_costs application_cost_accounting_pilot -- --ignored --nocapture
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test proving_costs application_proving_cost_acceptance -- --ignored --nocapture
cargo test --locked --release -p cellscript --test zk_application_costs application_cost_accounting_acceptance -- --ignored --nocapture
```

The host harness needs Linux `/proc`, GNU Time and GNU `timeout`; it measures the
actual native proving API with checked package loading, not a browser prover or
CLI process-launch benchmark. Keep large PKs/proofs and executable fixtures in
the ignored output directory. Archived observations retain their commitments.
The harnesses passed focused clippy/formatting checks. Review disposition:
Codex implementation and measurement review; no external human review or new
cryptographic trust decision is claimed.

## Complete 1/2/4-counter extension

[`set-manifest.json`](set-manifest.json) binds a separate host corpus and
[`set-decisions.json`](set-decisions.json) its finite decisions. The original
v1 reports/decisions remain historical single-counter observations. After the
complete-set pilot, [`set-limits.json`](set-limits.json) froze SHA-256
`27a0dbe8f87eebca5f824df4d5f18c1f9b6a1e72f0df29db1e9057de18ee9bb4`.
The independent acceptance run passed that exact freeze. Original v1 and #40
budgets were not modified.

| Protected counters / key policy | Median complete request ms | Median sum of leaf prove/verify ms | Worker peak RSS KiB |
| --- | ---: | ---: | ---: |
| 1 / checked separately per leaf | 6,627.13 | 775.24 | 605,068 |
| 1 / checked once per request | 6,809.66 | 788.58 | 605,880 |
| 2 / checked separately per leaf | 13,836.16 | 1,577.69 | 605,408 |
| 2 / checked once per request | 7,675.30 | 1,552.79 | 627,712 |
| 4 / checked separately per leaf | 27,459.08 | 3,111.97 | 658,268 |
| 4 / checked once per request | 9,328.84 | 3,157.79 | 624,712 |

Every request produces and locally verifies all 1/2/4 proofs. There is one warmup
and three measured requests in each fresh process. Automatic Rayon, serial
requests/leaves and warm filesystem cache are retained; the host is not
exclusive. Timings include key checks but exclude the shared key's final drop;
whole-process elapsed time includes it. GNU Time reports whole-process RSS,
not a phase-local heap peak. Proof bytes match for every candidate/repetition.
Setup generation/PK/VK identities and exact public inputs are retained separately.

Sharing one already checked key within a bounded native request reduces repeated
loading cost for 2/4 leaves. It does not reduce proving work or imply a consistent
memory saving; the two-leaf candidate uses more peak RSS in this run. A single
leaf gains no systematic benefit. Retain the existing native caller-owned key
API and CLI defaults; no persistent cache or new CLI batch mode is adopted.

The [complete application evidence](../zk-counter-set/README.md) records all
groups and separate VM generations, actual full transaction/witness bytes,
code/data occupied capacity, late-invalid rejection and real-node conflict
recovery. Four exact proofs cost approximately 442 million cycles under the
pre-frozen 500-million ceiling. That measured limit motivates a finite defer
decision for aggregation/recursion, not an automatic change of relation or setup.
No additional proof system/profile is proposed by this cost study.

After generating the native counter-set fixtures, explicitly run:

```bash
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test proving_set_costs set_cost_pilot -- --exact --ignored --nocapture
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test proving_set_costs set_cost_acceptance -- --exact --ignored --nocapture
```

Both refuse to overwrite existing observations. The acceptance limits allow a
40-second complete request, 2-second individual proving call, 160-second worker
and 768-MiB worker RSS; the external worker timeout is 180 seconds. These are
host-specific regression ceilings, not user-facing latency guarantees.

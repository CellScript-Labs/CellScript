# 0.32 cost research: decisions and remaining work

Status: research candidate passes dev/CI/backend gates, not release acceptance. Implementer: Codex. Independent
review was waived by the maintainer on 2026-10-03. The waiver does not waive
the independent artifact checker or the frozen budgets. This record extends
the completed [low-mask experiment](IMMEDIATE_MASKS.md).

## Measurement contract

`tests/support/cost_trace.rs` emits `cellscript-execution-trace-v1`. It runs
the real pinned CKB scheduler/syscalls with an instruction-stepping runner,
then compares its exit and total cycles with `measure_group` from the ordinary
verifier. A trace is measured only when both agree exactly. Limits, traps,
missing groups, decoding failures and discrepancies remain unavailable, never
zero-valued measurements. The transaction verifier remains the verdict oracle.

Each VM has its own `(vm_id, generation)`. SPAWN gets a child ID; EXEC starts a
new generation under the same ID. The stack baseline is SP after argv loading
and before the first instruction of that generation. The peak is the largest
observed downward displacement, including transient outgoing reservations.
It excludes initial argv placement and does not prove a bound on other inputs,
non-SP memory accesses or host allocations. Independent VM stacks are never
added together. Static call-chain bounds retain their separate report fields.

Histograms count decoded instruction attempts, including the pinned VM's MOP
fusion. They are not interchangeable with static four-byte instruction counts.
Each PC records instruction charges and synchronous syscall charges. Residual
scheduler/loading charges reconcile with the authoritative group total.
Load/store opcode counts describe guest accesses, not host syscall memory
traffic. Host traffic and whole-transaction tracing are explicitly unavailable.

Hand-calculated tests cover nested frames (32 + 16 outgoing + 48 callee = 96),
loops (three 24-byte reservations have peak 24, not 72), nonzero exits, budget
exhaustion, and 64-byte SPAWN/EXEC child reservations. Instrumented runtime
fixtures exercise both ordinary success and rejection.

`tests/cost_attribution.rs` reuses the frozen nine scalar and 41 expanded policy
fixtures and checks all their existing ceilings. It executes 1,707 cases. ELF
headers, text, rodata, other sections and padding reconcile with file length.
Policy block ranges attribute the policy entry, decoder, stub, action and other
runtime work; shared code is counted once. The inherited `dispatch` category
means the whole policy entry, including witness loading, canonical validation
and record scanning; it is not a measurement of tag comparisons alone.
Unmapped trampoline/runtime PCs remain
explicit. Witness/transaction byte lengths, static bounds and per-VM observed
peaks remain distinct. This diagnostic companion does not replace the complete
matched-Rust, growth and multi-Script comparator.

## Experiments and decisions

| Track | Evidence | Decision and remaining boundary |
| --- | --- | --- |
| Low-mask immediates, #41 | Archived clean before/after reports and 6,782 comparable metrics | Adopt the already implemented strict improvement; replay after integrating published 0.31 |
| Local register retention, #39 | First binary-left-only candidate: all 1,707 runs unchanged. [Second candidate](retention-second/README.md): 35 paired probes, full 6,782-metric comparison, and 1,707 paired traces; only four corpus executions save one cycle | Reject both candidates for the 0.32 default. The second candidate exercises right-operand reuse, but saves no ELF bytes, stores or stack and improves only two scalar fixtures. This does not establish that broader retention is useless |
| Helper scratch ownership, #39 | `scalar_slots.rs` keeps address-exposed, resource, wide, borrowed and unknown-call values outside scalar reuse; parameters have distinct entry spill slots | Retain conservative ownership. Defer wider reuse until helpers expose clobber/live-register contracts and dead parameter spills are proved removable before interference is relaxed |
| Balanced routing, #37 | `cost_experiments` sweeps 1/2/4/8/16/32/64 dense, sparse and high-u32 tags, every action and unknown tag | Retain linear routing. Late-action savings trade off first-action cycles and ELF growth; no universally dominating threshold measured |
| Saved selector, #37 | Separate two-pass experiment with a caller-owned saved ordinal and clobbering common-check surrogate, including unknown-tag versus common-check failure | Retain current routing. Saving the selector introduces per-target code and early-path work. No production selector lifetime or checker pattern is changed |
| Word copies, #38 | Byte versus word-plus-tail loops at 0/1/7/8/9/16/31/32/33/64/128/256 bytes and all eight source alignments; VM independently compares every copied byte | Reject unconditional replacement: short copies regress and the helper grows. Any disjoint-copy specialization still needs caller classification; forward overlapping normalization cannot inherit disjoint-copy assumptions |
| Borrowing, #38 | 18 isolated private-copy/borrowed-span pairs, including empty/wide/unaligned spans, a live caller buffer, clobbering nested callee and separate outgoing reservation | Defer production adoption despite measured savings. The read-only probe is not a canonical witness decoder. General adapters need a versioned span, lifetime and alias contract in both emitter and checker |
| Staged loading, #38 | Real scheduler probe reads the outer header, two optional-field length prefixes and selected input_type, independently varying optional fields and payload width | Retain full bounded load. Probe isolates syscall/stack tradeoffs and omits canonical decoding; runtime offsets, malformed late records and full accepted-witness equivalence remain blockers to admission |
| Repeated code/specialization, #40 | Existing exact-key decoder sharing plus repeated/distinct generic fixtures and per-range attribution | Keep existing sharing. Wider outlining remains deferred until complete equivalence keys and call-cost/checker attribution are defined; equal widths alone are insufficient |
| Pure IR/CFG, #40 | Existing AST folding/inlining/dead-code passes and the integrated 0.31 loop-fusion correction; repeated nontrapping AND source-rewrite probe improves 1,608 to 1,584 ELF bytes and 3,152 to 3,138 sample group cycles | Keep current passes; defer a general CSE pass. The measured source rewrite does not define value numbering across mutable definitions, traps, syscall/resource effects or first-failure ordering |
| Rematerialization/constant pools, #40 | #41 exhausts only 63 low-mask widths under its stated grammar; real emitted constants are exercised by the corpus | Wider grammar/pool search deferred. Additional loads, rodata, scratch pressure and relocation/branch effects have no measured favorable candidate |
| ELF/data layout, #40 | Exact file accounting; the previous 128-byte segment alignment is already baseline | No additional layout transformation adopted. Keep deterministic output and independent-loader rules; no re-counting historical padding savings |
| Compressed instructions, #40 | Bounded five-instruction-family operand survey; exact C.LI/C.JR mixed-width VM probe passes while the current checker rejects it | Defer production support. The probe preserves file layout and claims no ELF saving; assembler relaxation, source-map ranges and checker schema need a coordinated design |

These are finite experiments. Their isolated VM costs exclude surrounding
decoder and application work unless explicitly measured through the CKB
scheduler. They do not establish whole-transaction speedups or a global optimum.
No new optimization profile or CLI switch is selected. The default remains the
published 0.31 backend plus the separately validated #41 low-mask slice.

The complete paired observations and their cost dominance classifications are
in [experiment-frontier.json](experiment-frontier.json). Dominance compares
every listed dimension and every recorded row, without averaging away a
regression. Balanced routing has 314 byte/cycle tradeoff rows, 82 rows favoring
linear routing and six ties. Saved-selector routing has 205 tradeoff rows and
599 rows favoring the current design. Word copies have 72 tradeoff rows and
24 rows favoring byte copies. All 12 staged-load pairs trade stack space for
cycles and ELF bytes. Borrowing dominates its 18 isolated copy counterparts,
and source expression reuse dominates its ten counterparts; neither result
supplies the missing general semantic/checker contract. The compressed probe
ties under its unchanged layout.

These routing probes embed the selected input in each paired program. They
establish the recorded local comparisons, not a production dispatcher
threshold. The existing complete policy fixture matrix separately exercises
the retained production dispatcher, decoder and rejection paths.

The compressed-site survey follows the operand restrictions in the
[RISC-V C 2.0 specification](https://docs.riscv.org/reference/isa/v20260120/unpriv/c-st-ext.html)
for C.LI, C.ADDI, C.LDSP, C.SDSP and C.JR only. It excludes the fixed entry
trampoline. Two bytes per eligible site is a local instruction-byte opportunity,
not a prediction of final ELF size after relaxation/alignment or execution cost.
The isolated probe uses the pinned
[CKB-VM 0.24.14](https://github.com/nervosnetwork/ckb-vm/tree/v0.24.14)
with the same explicitly enabled IMC/B/MOP instruction behavior as the other
experiments. It does not migrate CellScript's four-byte checking boundary.

## Ownership and witness constraints

The outer policy frame owns the bounded 4,096-byte loaded witness. It validates
the exact WitnessArgs table and every policy record before executing an action,
including records after the selected one. Optional lock/output_type fields are
part of that admitted outer boundary. A small selected argument is not evidence
that the whole witness is small. Normalization can overlap its source and
destination. The selected adapter currently owns its copied bytes; outgoing
argument areas and caller-local buffers must remain disjoint during nested calls.

A future borrowed implementation must prove that neither normalization nor any
callee overwrites a still-live span, and it must keep the parent frame alive.
A word-copy implementation must separately prove alignment, overlap direction,
tails and page/length bounds. A staged loader must validate unselected records
and optional-field encodings without silently shrinking the accepted set.
Existing malformed/boundary tests remain authoritative for the retained design.

## Residual costs and unfinished acceptance

[residual-sites.json](residual-sites.json) lists each of the 38 distinct ELF
artifacts, its worst observed case, per-VM stack maximum, per-class charges
and hottest program-counter sites across the 1,707 executions. Each recorded
execution contributes once; this is coverage-weighted diagnostic evidence,
not an assumed production workload. The largest observed group cost is
27,373 cycles for `policy-dense-64-width-128/action-63/witness-at-bound`, with
a 43,592-byte ELF and a 6,352-byte observed VM stack peak. Neither maximum
is a bound over untested inputs. Guest accesses alone do not identify copied
bytes, and host syscall memory traffic remains unavailable.

The observed scalar stack traffic includes explicit named-variable storage,
entry parameter spills and joins/calls; the rejected adjacency experiment does
not remove them. Policy costs include canonical outer validation, scanning all
records, linear routing, private argument copies and actual action checks.
Some checks are required by the selected contract; redundancy has not been
proved merely because a site is hot or repeated.

The fresh integrated baseline is commit
`4e1047980bce94d8ad0317b132a6f4c15a45df08`. Its clean-source main and
multi-Script reports are [integrated-main.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/integrated-main.json) and
[integrated-multi.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/integrated-multi.json). The compressed full attribution
report and its summary now identify that same clean commit. The historical
before/after #41 evidence retains its original identities and is not relabeled
as a new optimization result.

The second clean CI replay passes all 6,782 comparable metrics in
[integrated-replay-comparison.json](integrated-replay-comparison.json), with
zero regressions. Its main report is archived separately; its multi-Script
report is byte-identical to the integrated baseline. This validates same-code
reproduction and the unchanged budgets, not another before/after saving.

Final `backend` and `ci` gates pass at clean commit
`6a453d3015784784a27ba1aa0a4b6b1a73db79a8`; see
[validation.json](validation.json) for logs, the dev source adjustment and
fresh pinned-node acceptance. The final inventory/report correction also
passes all 6,782 comparable metrics in [final-comparison.json](final-comparison.json).
It does not change compiler code or claim a new optimization saving.

Deferred wider outlining, general CSE, immediate/pool search and layout
transformations retain their specific blockers above. The #38/#39 evaluations
record the span/alias and wider retention/scratch decisions without adopting
those transformations. A subsequently selected implementation needs its own
scoped issue, contract and acceptance evidence. Closing a finite evaluation does
not turn an unmeasured transformation into a rejected experiment or an
implemented feature.

Reproduce the diagnostic companion with the existing Cargo test target:

```bash
cargo test --locked -p cellscript --test cost_measurement -- --test-threads=1
cargo test --locked -p cellscript --test cost_attribution -- --test-threads=1
cargo test --locked -p cellscript cost_experiments --lib -- --test-threads=1
```

Reports are written under `target/cellscript-cost/`. Exact source/lock/toolchain
provenance is in the diagnostic report. Isolated microbenchmarks are test-only
and use the current assembler, VM2 and `ISA_IMC | ISA_B | ISA_MOP` cost model.

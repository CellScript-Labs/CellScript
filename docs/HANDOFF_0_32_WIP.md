# CellScript 0.32 WIP handoff — 2026-10-08

## Status and publication boundary

The original WIP snapshot was **paused at the maintainer's explicit request**.
The maintainer requested resumption on 2026-10-08. The historical inventory and
logs below describe the paused snapshot; the resumed work is recorded separately
at the end of this document. Neither record is a release receipt, a completed
issue, or a merge-readiness claim.

- Branch: `codex/post-032-followups`.
- Remote: `https://github.com/CellScript-Labs/CellScript.git`.
- Base before the WIP commit: `622c850550428bbe000017c1b55944a9248717cd`.
- Active checkout: `/home/arthur/.codex/worktrees/post-032-followups/CellScript`.
- The containing commit is the authoritative source snapshot; historical logs
  below were generated before that commit and have narrower evidence scope.
- Latest focused struct-return run: **1 passed, 3 failed**, exit status 101.
- A final register-liveness adjustment was made after that run started and has
  not been tested. No full dev, backend, or CI gate passed on this final snapshot.
- No builds or tests were started after the pause request. Process inspection
  found no remaining compiler, test, or gate process from this work.
- Normal pre-commit gate work was deferred because the user expressly requested
  stopping work and publishing a WIP snapshot. Required release checks have not
  been waived. Preserve commit signing; do not disable it to publish.

The original checkout at `/home/arthur/RustRoverProjects/CellScript` is separate.
Its previously observed changes to `docs/README.md`, the `website` submodule,
and untracked `docs/CELLSCRIPT_EVM_CLASS_FINANCE_PLAN.md` are not part of this
snapshot and must not be overwritten or swept into this branch. The isolated
retention experiment checkout at
`/home/arthur/.codex/worktrees/open-handle-receipts/CellScript` must also be kept.

## Scope and issue state at handoff

The preceding objective was to finish the agreed 0.32 follow-ups and comment on
each issue. Publication of this WIP does **not** satisfy that objective. Issue
states below reflect the last session read; recheck them when work resumes.

| Issues | State / remaining work |
| --- | --- |
| #22, #36, #37, #38, #41 | Closed at last check; retain their existing evidence boundaries. |
| #28 | Open. Public/interface checking, value abilities, generic declarations and policy membership groundwork exist. Complete receipts, nominal qualification, universal template coverage, and H2 runtime handles remain unfinished. Independent security review remains required and unassigned. |
| #29 | Open. Open participant roles remain substantially unfinished and depend on #28. Independent security review remains required. |
| #39, #40 | Open. Cost experiments and retention comparisons exist; both retention candidates were rejected. Preserve conservative defaults, publish reproducible evidence, then finish issue-specific closure comments. |
| #42 | Open. Native/CCC migration workflow and local-node evidence exist. Final source-bound full backend validation and completion publication remain pending. |
| #43 | Open. EVM-class finance plan is separate from this implementation scope. |
| #44, #45, #46 | Open, linked ZK P2–P4 follow-ups: multi-Cell behavior, broader lifecycle, application costs. Not complete. #42's first migration cannot be deferred to #45. |
| #47, #48, #49 | Open, planned 0.33 work, outside the original core 0.32 closure. |

The earlier waiver for independent human cost review must not be interpreted as
a waiver of the security review required for #28 and #29.

## Implementation inventory

### Generic contracts and independent interface checks (#28)

Relevant files include `src/ir/generic_contract.rs`, `src/ir/mod.rs`,
`src/generics.rs`, `src/interface.rs`, `src/typed_semantics.rs`, and the standalone
checker's `schema.rs`, `interface.rs`, `interface/generics.rs`,
`generic_projection.rs`, `value_abilities.rs`, and `checker.rs`.

- Retain generic parameters, defining module identity, concrete arguments, and
  checked lowered aliases, including private and imported instantiations.
- Retain symbolic struct fields, enum variants/payloads, and function parameter
  and return declarations. Public interface declarations bind these symbolic
  shapes, preventing a template such as `T` from being replaced by `u64` merely
  because a present instantiation uses `u64`.
- Independently check type substitution, packed layout order, abilities,
  phantom/non-phantom occurrence, and agreement among instances of a template.
- Normalize aliases only after checking their common instantiation identity.
  Equal widths do not make different owners or type arguments interchangeable.
- Support Unicode identifiers consistently in the relevant substitution and
  public-parameter checks, with bounded token substitution.
- Prior groundwork derives value abilities independently, rejects unknown or
  cyclic nominal layouts, reserves `__mono__`, and follows identity-only phantom
  arguments through entry-scope closure without adding runtime Cell roles.

Current experimental versions are typed semantics v9, generic instantiations
v2, metadata schema 72, and lowering record v9. The source cache marker is
`project-source-set-v60-0.32-generic-shapes`. These have **not** been reconciled
with the new machine return ABI described next. Do not present this work as
complete handle admission or complete source/ELF equivalence.

### Fixed struct helper returns: unfinished backend prototype

An expanded imported-generic regression exposed a real return mismatch: a
constructor returned tuple fields in registers while its caller treated `a0` as
a struct pointer. After alias checking was corrected, that regression reached
the VM and failed with code 5. A caller-owned output-buffer prototype then made
that specific O0–O3 regression pass.

Files: `src/codegen/value_returns.rs`, `calls.rs`, `frame.rs`, and `mod.rs`.
Fixed ordinary structs returned by helper functions use a hidden output pointer
appended after normal machine arguments. The caller allocates the result buffer;
the callee copies into it before its frame is destroyed. Zero-width results skip
dereferencing. Existing stack access helpers are used. Cell types and enums are
excluded from this new return path; it is not a generalized action-entry ABI.

**This is not an admitted or stable ABI.** Its independent machine-contract
validation, lowering/source-map/schema compatibility, cache invalidation,
mutation coverage, and full backend gates remain unfinished. The existing
bounded `policy-witness-v1` call contract still rejects aggregate parameters and
returns; that boundary was not broadened by the prototype.

The last untested edit is in `value_returns.rs`: loading the source pointer with
`emit_stack_load_with_avoid("a1", offset, &["a0"])` to preserve the destination
pointer. It was made after the latest test command started. No final-source
result can be inferred from that command.

### Open-handle policy wire prerequisite (#28)

Files: `crates/cellscript-artifact-checker/src/open_handle_policy.rs`, its
Molecule schema, checker tests/CCC fixture vectors, and
`docs/CELLSCRIPT_OPEN_HANDLE_POLICY.md`.

The bounded host policy contains 1–32 canonical members, depth 5, a 188-byte
header, 292-byte member, and 656-byte selection. Membership is checked against
an independently supplied root. Commitments include receipt/code/Script,
outpoint/history/network/ABI, exact or compatible policy, active/yanked status,
and floors. Earlier tests cover CCC 1.23.0 parity, all 528 member positions, and
single-byte mutations/truncations of the selection.

This proves bounded host membership only. It does not implement compatible
artifact admission, nominal runtime handles, or on-chain freshness enforcement.

### Counter migration and client integration (#42)

Files: `contracts/zk-private-counter/MIGRATION.md`, Script modules `config`,
`migratable`, and `migration`, native client migration modules/tests, and
`examples/zk/{migration,node-migration,migration-cli}.ts` plus tests and scripts.

The bounded workflow is an old-key-authorized, one-time 0→1 migration between
two exact deployments. It binds counter/config Type IDs, raw transaction and
proof, and preserves owner/state/custody. Native and CCC builders have parity
coverage. Historical evidence includes 39 native VM cases (6 positive,
33 negative), 11 original CCC tests and 6 migration tests.

The second setup is a public test fixture, not production admission. Legacy
immutable deployments cannot acquire a migration path retroactively. No public
network deployment or general key-rotation framework is claimed.

Local pinned-node evidence is at
`target/counter-node/1791182285-1291346` in the active checkout:

| Transition | Cycles | Bytes | Local transaction hash |
| --- | ---: | ---: | --- |
| Old update | 111603359 | 1237 | `0x385dff9601cbaa0b5bea924fec07549293b5acce338621cb91e52935dadaa886` |
| Migration | 111703450 | 1632 | `0x130c54b10b55868000d280b204c6693643b058c489e82e35640b995091c0e28b` |
| New update | 111474440 | 1237 | `0xdfb02b0cd49e8306ab25e8c0cb7d872c1f2660534d8f16f012f1d8550458a9da` |

The historical run used real fee signatures and confirmed a continuous state
chain. Old-key-after-migration and new-key-self-install rejected with parent
code 79; dead/stale inputs and second migration were also rejected. The local
node directory is not included in this WIP commit.

### Cost research (#39, #40)

`docs/reports/0.32/retention-second/` preserves the second candidate's source
patches, manifest, compressed reports and logs. There were 35 same-IR probe
pairs, 6,782 comparable metrics with zero regressions, and 1,707 scheduler
traces: four saved one cycle, 1,703 tied. ELF sizes (3,560 / 10,024 bytes) and
stack sizes (7,088 / 8,656 bytes) did not improve; one load became one `addi`
and stores were unchanged. The candidate was rejected for the default backend.
The archive contains experiments, not an instruction to enable them.

## Validation record and known failures

Selected existing logs are copied, without rerunning commands, under
`docs/reports/0.32/wip-handoff/`. Its manifest records their SHA-256 hashes.
Logs are chronological observations from evolving sources, **not** a passing
receipt for the containing WIP commit.

The packaging-time `git diff --cached --check` also reports whitespace in the
three archived retention `.patch` files (including blank context lines) and a
blank line at EOF in `generic-struct-return-tests.log`. The evidence bytes were
preserved rather than rewritten, since the archive manifests bind them. Resolve
their storage format and gate handling before claiming a clean gate; this WIP
does not claim that the staged diff check passed.

| Log | Observed result and limit |
| --- | --- |
| `value-return-runtime-tests.log` | Latest run: 1 passed, 3 failed; all three failures are pre-codegen `E2105`, `action verify: field-access`. The zero-width result case passes. |
| `value-return-expanded-tests.log` | 21 policy/artifact checker tests passed, then the earlier 3-test struct-return suite had 1 pass and 2 failures. |
| `generic-struct-return-tests.log` | Imported generic alias/struct-return regression passed, with O0–O3 VM execution inside the test. Later changes are not covered. |
| `generic-shape-other-tests.log` | 8 interface tests passed; policy tests were then 20/21. That fixture failure was subsequently corrected, producing the 21/21 run above. |

The current failing test names in `tests/value_return_abi.rs` are:

1. `fixed_struct_return_storage_survives_nested_calls_and_both_branches`
2. `fixed_struct_return_hidden_pointer_uses_outgoing_stack_without_overwriting_arguments`
3. `constructed_result_uses_declaration_order_with_reordered_initializers`

Explicit local result type annotations did not resolve their `field-access`
admission failures. These tests do not yet establish runtime correctness of
nested return buffers, outgoing-stack hidden arguments, or reordered fields.

Two previous broader evidence sets must remain separate:

- Generic-contract snapshot under local
  `target/0.32-open-handles/evidence/generic-contract-2026-10-07/`:
  991 compiler library tests + 64 integration tests + 46 checker tests = 1,101,
  with strict Clippy passing. Tracked diff SHA-256:
  `3841530439e7f81a37c1166123c00fde3e5763ae597e1ebde7d1feaba58f47cc`;
  manifest SHA-256:
  `291b41454f256347abb6e9a56037a9cec96a6145974179a728e9b706a693a50d`.
  This predates declaration-shape and struct-return changes and is local-only.
- Earlier migration-source CI evidence: tracked diff SHA-256
  `3a30e2d16896a4932a8add558d67b84a87725b64afde14584e20ec8edd6206d1`,
  manifest SHA-256
  `4131dc08277dc2dd5ccee2c1f8126e836bba50e35b85500c2e2f8176fe75d53d`,
  log SHA-256
  `382e5831edf2be98edd1f61d982439b5bd96fb15a0b59059041ff8176719efac`.
  It is not CI evidence for later #28/backend changes.

Interrupted/lost test sessions with only compilation output are not passes.
An earlier lost CI result was explicitly corrected in the public issue history.
Do not rely on `/tmp` logs surviving a resumed session.

## Resume checklist (not executed during this handoff)

1. Obtain a resume instruction, read repository `AGENTS.md`, `CODING_STYLE.md`,
   `CHANGELOG.md`, `BRANCHES.md`, gate policy and Tutorial 06, then inspect this
   branch and current issue states. Preserve the unrelated original checkout.
2. Reproduce the three `value_return_abi` failures. Inspect the executable
   field-access classification in `src/lib.rs` (`is_executable_schema_field_access`,
   `is_executable_aggregate_field_access`, and
   `is_executable_tuple_call_return_field_access`). Add only proven support;
   do not bypass fail-closed admission globally.
3. Investigate constructor field ordering. `lower_struct_init` currently builds
   operands in initializer order while fixed named tuple emission associates
   operands with fields by offset. This is a suspected mismatch, not yet a
   confirmed runtime failure because the new test stops before codegen.
   Preserve source evaluation order while arranging the serialized result in
   declaration order; include zero-width field ties in the investigation.
4. Complete the hidden return pointer's independent machine-contract checks,
   schema/lowering/source-map and cache version decisions, plus negative
   mutation tests. Recheck lifetime, nested calls, both branches, register and
   stack argument placement, wide fields, and zero-sized values at O0–O3.
5. Finish #28's remaining H1 receipts, nominal qualification and universal or
   absent-template coverage, then H2 runtime enforcement and #29 participants.
   Arrange the required independent security review rather than inheriting the
   cost-review waiver. Keep business rules in DSL/IR/metadata, not name-based
   backend special cases.
6. Run applicable focused tests and the required dev/backend/CI gates on the
   final source. Keep clean-source provenance and original evidence budgets.
   Do not turn old receipts into claims about new source. Revisit migration
   evidence when relevant code changes; do not run release gates casually.
7. Finish issue-specific comments for #28, #29, #39, #40 and #42 only when their
   acceptance conditions are actually met. Separate remaining #44–#46 work.

Useful existing focused test commands for the next implementation session:

```bash
cargo test --locked -p cellscript --test value_return_abi
cargo test --locked -p cellscript --test entry_selection imported_generic_contracts_keep_owner_identity_and_lowered_aliases -- --exact
cargo test --locked -p cellscript --test interface_inspection
cargo test --locked -p cellscript --test policy_artifact_checker
cargo test --locked -p cellscript-artifact-checker
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh backend
./scripts/cellscript_gate.sh ci
```

Focused successes do not replace gates. Preserve the pinned Rust/toolchain and
dependency versions; no version bump is authorized by this WIP handoff.

## Existing public progress references

- [#28 generic contracts and identity-only closure](https://github.com/CellScript-Labs/CellScript/issues/28#issuecomment-6032342190)
- [#28 ability foundation](https://github.com/CellScript-Labs/CellScript/issues/28#issuecomment-6031893693)
- [#28 earlier CI evidence correction](https://github.com/CellScript-Labs/CellScript/issues/28#issuecomment-6031540096)
- [#39 cost progress](https://github.com/CellScript-Labs/CellScript/issues/39#issuecomment-5986708331)
- [#40 cost progress](https://github.com/CellScript-Labs/CellScript/issues/40#issuecomment-5986714186)
- [#42 earlier migration CI](https://github.com/CellScript-Labs/CellScript/issues/42#issuecomment-5989985389)
- [#45 lifecycle foundation](https://github.com/CellScript-Labs/CellScript/issues/45#issuecomment-5990855521)

The latest declaration-shape/return-ABI work was not separately posted as a
completed issue update. Publishing this WIP must not close those issues.

## Resumed implementation — 2026-10-08

The resumed checkout is `/Users/arthur/RustroverProjects/CellScript`, on
`codex/post-032-followups` after fetching and fast-forward-only pulling the
remote WIP commit `ba86b1c5`. Existing editor, website, NovaSeal and benchmark
branches and commits were preserved. Their clean checkouts were then aligned
to the parent commit's gitlinks for validation; no submodule commit was created.

The first slice repairs the fixed ordinary-struct return regressions:

- Reproduced the original three `E2105` field-access failures (one of four
  tests passed before changes).
- Track result storage only for retained ordinary-struct helper declarations
  whose complete layouts are fixed, and propagate that origin through local
  bindings. Dynamic, Cell and enum returns retain their separate admission
  rules; the bounded policy-witness call contract is unchanged.
- Evaluate struct initializer expressions in source order, then arrange tuple
  operands in declaration order, including imported declarations. Use the
  backend's declaration indices for zero-width offset ties and skip copying
  empty fields.
- Project named tuple operands by field identity into the typed record's
  canonical layout order. Distinct empty nominal fields at the same offset
  must not be swapped merely because canonical order differs from declaration
  order. The checker retains its existing strict tuple type rule.
- Advance the internal artifact-cache marker to
  `project-source-set-v61-0.32-fixed-struct-results`; package, toolchain and
  public schema versions are unchanged in this slice.
- Recheck the handoff's source-pointer register-liveness adjustment through
  O0–O3 VM execution, and add nested-layout/zero-width and IR evaluation-order
  regressions. Each VM fixture also checks its complete compiler-produced
  four-file bundle with the existing independent checker.
- Canonicalize the local-dependency test's expected temporary source path,
  matching the resolver on macOS where `/var` aliases `/private/var`.

Focused validation on this slice passed: all five `value_return_abi` tests
(100 O0–O3 CKB-VM executions with bundle checks), the initializer evaluation
order regression, the imported-generic return regression, all eight interface
inspection tests, all 22 policy checker tests, and the independent checker's
40 unit plus nine integration tests. The full 992-test compiler library suite
and strict `cargo clippy --locked -p cellscript --all-targets -- -D warnings`
also passed. These results do not replace the required gates.

The gate attempts exposed stale sidecar identities in the WIP's business,
committed-state and runtime-view fixtures. Refresh only measured lowering,
source-map, bundle and ProtocolBundle hashes from compiler/test outputs while
preserving ELF hashes, raw/serialized transaction hashes, measured costs,
budgets, expected failures and candidate release status. Re-run the positive,
adversarial and resource checks after each refresh; hash updates alone are
not execution evidence. The business-corpus suite passed all seven tests after
its refresh. All 25 tests across commitment opening, external verifiers,
fungible/NFT/authorization, Order/AMM, temporal scenarios and runtime views
passed after the remaining fixture refreshes.

The resumed `dev` gate passed (exit 0), including strict backend quick audit,
133 syntax combinations (75 accepted, 58 expected rejections, zero failures),
simulator package scenarios, Registry and ZK build/runtime checks, fixture and
documentation freshness, native source policy and `git diff --check`. Its local
log is `.cap/logs/1791434999-73646.log`. The subsequent `backend` gate's compiler
test section passed 1,875 tests, with four explicitly ignored tests and zero
failures; gate-wide status must still include its later checks. These local,
dirty-checkout logs are diagnostic evidence, not clean release receipts.

The `backend` gate exited 1 at production stateful acceptance: it requires a
clean CellScript source tree, while this reviewable slice is still uncommitted.
Its full audit passed every preceding contract, including the 177-case CI
syntax matrix (103 accepted, 74 expected rejections, zero failures). See
`.cap/logs/1791435919-9330.log` and
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261008-131955.json`.
The sibling CKB checkout is also at `a7b8fb5365296a97bbf552741a258e40db53bc64`,
whereas acceptance pins `f7fa4436737756f97a24e254f22c13a36316ecea`; do not
overwrite that checkout or infer fresh node evidence from historical receipts.
This gate attempt did not reach the final counter-node acceptance test.

An overlapping CI attempt had one CLI process-launch `ENOENT` while another
gate was rebuilding the shared binary. The exact failing test passed when
rerun alone (`.cap/logs/1791437086-48848.log`), without a source change. Preserve
the CI test-thread flags and run subsequent gates serially; the overlapping run
is not a passing CI receipt.

The following serial CI attempt passed all Rust tests, Clippy, package checks,
Registry API/Node builds and both independent Registry verifiers, then failed
the website CSS assertion (`.cap/logs/1791437161-50950.log`). Existing website
dependencies belonged to the previous checkout: Astro 5.18.2 was installed
while the current lockfile pins 7.3.2. `npm --prefix website ci` restored the
locked dependencies without source or lockfile changes. The complete website
`build:ci` then passed (`.cap/logs/1791438478-95673.log`); the website submodule
remains clean. Do not substitute this focused success for a complete CI receipt.

A later serial attempt passed the complete compiler test section but failed
`check-cost-evidence` with `tracked_diff_sha256` mismatch
(`.cap/logs/1791438524-97448.log`): this handoff was edited while that run was
capturing source-bound cost reports. Freeze all tracked files, including docs,
during subsequent gates. The next full serial run must regenerate both reports
from one unchanged checkout. Record its final status in the chat/local logs;
editing this file again would change the tracked-diff identity of those reports.

The hidden return ABI is still an experimental prototype. Existing bundle
checks do not supply the missing dedicated machine-contract validation,
schema/compatibility decision or adversarial hidden-pointer mutations from
resume step 4. The next implementation slice must make these obligations
independently reviewable:

1. Bind the result's nominal type and byte width, the hidden pointer's machine
   argument position, its register/outgoing-stack placement, and the caller's
   result-buffer extent after all ordinary argument expansion.
2. Record and check the callee's saved destination, live source pointer, bounded
   copy and frame teardown. Zero-width returns must have an explicit contract
   without requiring a dereference. Implement this in the standalone checker
   without importing compiler/codegen structures.
3. Mutate hidden-pointer positions, saved/outgoing offsets, copy lengths and
   buffer ownership after rebinding outer hashes. Reject callee-frame escapes
   and overlap with witness/ordinary outgoing arguments. Keep the O0–O3 nested,
   branch, wide-field and stack-argument VM fixtures as positive evidence.
4. Decide the versioned lowering/source-map and compatibility boundary before
   treating this return convention as admitted. Keep Cell, enum and dynamic
   returns, and the policy-witness helper-call boundary, outside this slice.

H1/H2, open participants and independent security review remain unfinished;
no issue has been closed or externally commented on by this slice.

## Continued queue and checked result ABI — 2026-10-08

The first slice is signed commit `6dba1768`, fast-forwarded onto the actual
`0.32` branch and pushed to `public/0.32`. The serial CI completed before the
user's later instruction to defer further complete CI runs. The clean backend
gate on that commit passed, including production stateful CKB acceptance and
counter-node acceptance (`.cap/logs/1791440802-82352.log`). The pinned CKB
checkout was isolated at `/tmp/cellscript-ckb-032`; the user's dirty sibling CKB
checkout was preserved. These results belong to `6dba1768`, not later changes.

The next slice replaces the prototype's missing dedicated checker contract with
`cellscript-fixed-struct-result-abi-v1`. Lowering v9 retains its scalar-only JSON
shape; aggregate helpers and their callers require feature-specific extensions.
Old deny-unknown-fields checkers reject these extensions. The independent
checker derives fixed nominal widths and expanded hidden argument placement,
then checks decoded save/source/copy/receive ranges, separate buffer extents,
outgoing stack ownership, zero-width behavior and frame/RA/FP teardown. Source
pointer loads bind to typed return locals and fixed field offsets. Arbitrary
pointer dataflow and source-to-machine equivalence remain outside this claim.
The source-map remains v2, and the cache marker advances to v63.

A nested outgoing-stack call regression exposed lost exact-width provenance
when a helper result was stored in a named local and passed to another helper.
Propagate widths only when every store has matching proven nominal storage;
borrowed/schema runtime lengths are not inferred from their declared types.
The checker distinguishes decoded owned-frame size from the existing
conservative frame peak, which includes outgoing argument reservations.

Focused tests passed seven value-return cases (108 O0–O3 CKB-VM executions),
including nested calls, nonzero field offsets, wide fields and empty nominal
values, plus rebound machine/source-pointer mutations. All four artifact files
are checked for every positive runtime fixture. Required gates for this new
slice remain to be recorded after the final source freeze; the earlier clean
receipt is not a substitute.

The user-directed execution queue is:

1. WIP blockers (including this return ABI boundary).
2. #50: contextual Vec constructor typing and recovering diagnostics.
3. #51: exact 52-byte context, Rust byte oracle, parent/child CKB-VM and costs.
4. #39/#40: finite retention decision and source-bound replay publication.
5. #42: exact ZKP1 SDK/reference lifecycle closure.
6. #28, then #29: compatible handles and open participants.
7. #44, then #45, then #46: bounded multi-Cell statements, cross-version
   impact classification, and controlled proving/runtime resource evidence.

Run focused tests and applicable dev/backend gates while advancing the queue.
Run complete CI only after all implementation work in this queue is finished,
as explicitly requested by the user. #50/#51 do not wait for heap collections
or dynamic codec work. The 0.33 order remains #49 fixed-layout selection /
correspondence, #48 dynamic codec, then #49 variable payload; #47 follows actual
collection requirements. H1/H2 and open-participant security review remain
required and unassigned; the independent cost review waiver does not waive
that security review. No issue closure or external issue comment is authorized
by this document.

The contextual Vec slice (#50) now passes normal constructor validation before
propagating the declared element type. IR emits the same typed CollectionNew,
so metadata and backend storage provenance agree. Recovering type checks retain
valid owned annotated Vec locals after a fatal initializer diagnostic; CLI and
LSP tests reject the primary error without undefined-local cascades. Thirty-two
O0–O3 CKB-VM cases cover new/with_capacity, u8/u64/Hash/fixed named elements,
empty length, actual fixed capacity, push and length. Incompatible pushes,
invalid capacity/arity and dynamic/nested element layouts remain rejected.
Three syntax seeds and the collections support matrix carry the same boundary.

The bounded 52-byte slice (#51) composes existing loops, shifts and byte reads;
no new generic helper is necessary. A vendored, hash-bound AgoraSeal context
child from the issue's fixed upstream commit parses the real hex argv and checks
raw CellDeps plus committed header coordinates. The independent Rust oracle
checks every byte, including an all-distinct 52-byte runtime vector. O0–O3
execute 48 positive matched encoder-only/parent-child cases and 40 rejection
cases with real scheduler replay. Zero/max/non-palindromic heights, truncated
ballots, out-of-range reads, backing overflow, wrong length/oracle/OutPoint/dep/
code hash, false heights and missing headers are covered. The example's
upstream vote template and generated source both use the checked loops and
compile strictly at all four levels; whole voting-protocol runtime equivalence
is not claimed. The separate AgoraSeal working tree remains untouched.

The new fixture budget freezes only these explicit vectors and leaves the
existing cost corpus budgets unchanged. At O3, the looped parent-child fixture
uses 5,040 ELF bytes and at most 203,626 group cycles versus 11,240 and 204,068
for the unrolled fixture. Parent observed stack is 9,072 versus 10,256 bytes;
the same Rust child observes 66,704 bytes. Parent instruction cost increases
while ELF loading cost decreases, so do not describe all submetrics as wins.
Source-bound raw observations are regenerated under target/byte-context, and
the gate rebuilds the child twice and compares it with the pinned source/lock/
ELF fixture. The v63 cache generation includes the constructor-context change.
Complete CI remains deferred until the entire user-directed queue is finished.

The first dev attempt for this combined ABI/Vec/context slice passed its runtime,
checker, reproducibility, strict-backend quick and 139-case syntax checks, then
failed at a stale business-corpus inventory digest
(`.cap/logs/1791444891-2681.log`). Refresh the inventory through the native
`check-business-corpus --write` consumer; preserve the fixture budgets. A complete
passing dev run is still required before committing this slice.

For #39/#40, all twenty stored/raw archive hashes and sizes match the published
retention archive, and the native comparator replay passes 6,782 metrics.
[Resumed evidence](reports/0.32/resumed/README.md) preserves the clean `6dba1768`
backend and pinned CKB receipt without relabeling the original candidate runs.
Both retention candidates remain rejected; wider scratch ownership is deferred.

The clean backend's default counter node report covers creation and two updates,
not the optional CCC migration sequence. Fresh CCC migration node acceptance
remains pending. The node harness now keeps its fallback public-test setup alive
and explicitly passes its package path to both CCC workflows, so setting
`CELLSCRIPT_COUNTER_CCC=1` does not require a separately generated setup package.
No process-global environment mutation or production setup admission is added.

The combined slice passed complete dev at `.cap/logs/1791445936-28775.log`,
was signed as `670953c2`, and was pushed to `public/0.32`. Its clean backend
attempt (`.cap/logs/1791446419-43116.log`) failed the imported-generic return
regression before reaching node acceptance. The dedicated result checker compared
callee/caller alias spellings directly instead of using the already checked
generic instantiation's nominal alias set. Normalize source/caller field types
only through that set, preserving declaration owner and type arguments; equal
layout width never supplies identity. The existing 13 entry-selection, 24 policy
mutation and seven return-ABI tests pass after correction. Add entry-selection
coverage to dev so this imported return boundary is not omitted again.

The full serial compiler suite then passed
(`.cap/logs/1791447596-75012.log`), including the frozen costs and business
fixtures. The node harness additionally requires the pinned acceptance receipt
to identify the current clean CellScript commit and exports that source
provenance in its own report; a stale passing receipt cannot stand in for a
fresh source-bound node run. Full CI remains deferred as requested.

## Clean-source CCC replay after nominal-alias correction

- Signed commit `bfb1910070d90a15e6268796a0e329e584806305` (`G`) is pushed to `public/0.32`. The dev gate passed in `.cap/logs/1791448444-1238.log`.
- The complete clean-source backend gate passed with `CELLSCRIPT_COUNTER_CCC=1`; full log `.cap/logs/1791449427-24329.log`. This includes the compiler suite, checker, frozen cost corpus, complete strict audit/stateful pinned-CKB acceptance and the actual CCC node application/migration run. It is not a full CI run.
- [The archived receipts](reports/0.32/resumed-ccc/README.md) retain that commit, clean-source status and raw/stored hashes. The node independently rejects an acceptance receipt from another commit or a dirty source.
- #42 P1 now has the continuous old-key update → old-key-authorized migration → new-key successor update, confirmed with actual fee signatures. Both setup versions are public test fixtures. Immutable legacy Cells are not retroactively migratable. P2/P3/P4 remain #44/#45/#46.
- Continue the ordered queue with #28 then #29, followed by #44/#45/#46. Full CI remains deferred until all implementations are complete. Independent security review for #28/#29 remains unassigned and has not been waived.
- The attached `fixed-result-contract` worktree is based on `bfb19100` and contains an uncommitted H1 declaration-evidence prototype plus its design draft. The first generic-catalog slice passed 13 entry-selection, eight interface-inspection and 25 policy/checker tests. The subsequent nominal-scope extension is still under focused verification and is not admitted source syntax or a complete compatibility receipt. Preserve that work when continuing.


## Resumed #28 declaration prerequisite — 2026-10-08

Base: signed/published `1dffc25d`, including the clean `bfb19100` CCC/backend
archive. The managed `fixed-result-contract` worktree retains the next H1
prerequisite; no full receipt or runtime open-handle admission is claimed.

- Optional native generic/nominal catalogs retain the original source API before
  optimization. This fixes removal of unused public functions from declarations;
  their executable availability remains separately absent when pruned.
- Qualified owner/import scopes and ordinary IR type origins survive imported
  helper merging and entry pruning. Conflicting merged lowered names have no
  source evidence. The checker binds ordinary concrete layouts to complete
  source field order/types, variants, abilities, capabilities and identity policy.
- Bounded independent type parsing preserves Unicode identifiers, references,
  arrays/tuples, complete token boundaries and generic binder shadowing.
- A separate symbolic declaration check uses parameter minima, recursively
  checks nested generic demands and rejects cyclic/unknown/unbounded contracts.
  It never uses one observed specialization as a universal guarantee and does
  not claim generic body behavior or source-to-machine equivalence.
- Catalog omissions, duplicate declarations/bindings, same-width foreign owner
  swaps, malformed types and rebound public omissions/additions have real
  compiler-bundle mutation fixtures. Oversized absent templates keep ordinary
  compilation but fail the new open-interface prerequisite.
- Cache generation advances to v64. Package/toolchain/dependency pins, historical
  exact receipts, v3 wire/digest rules, v9 global records and source-map v2 remain.
  Corrected pre-optimization declaration retention can change newly compiled
  interface hashes; historical archives are not relabeled or rewritten.

Targeted compiler/checker tests and strict clippy were used during development.
Formal dev/backend checks and final source publication must be recorded after
this slice is frozen. Whole CI remains deferred until the entire user-ordered
queue is implemented. Full H1 receipt projection/directional admission, H2
nominal values/runtime, #29 and #44/#45/#46 remain outstanding. Independent
security reviewer for #28/#29 is still unassigned and has not been waived.

### Current declaration-record business replay

The first full backend replay of signed `40e24d51` stopped at stale current
business anchor sidecar identities after its 992 library tests passed. The actual
four-artifact transaction and persistent-policy partial-fill/settle/cancel
transactions were replayed. Their ELF hashes, transaction hashes and every
recorded resource measurement remain unchanged. Only the current lowering,
source-map, verified-bundle identities and ProtocolBundle hash are refreshed;
historical reports and all resource ceilings remain unchanged. All seven
`business_corpus` tests pass in `.cap/logs/1791455346-12833.log`. This focused
replay does not replace the next clean full backend gate. Whole-queue CI remains
deferred until the requested implementation queue is complete.

### Remaining current fixture identities after declaration catalogs

The next backend replay on signed `77737444` passed the refreshed business
anchor and stopped at committed-state fixtures with old sidecar identities.
Actual CKB-VM replay then covered committed-state, Order/AMM, external verifier,
temporal, fungible/authorization/NFT and typed runtime-view fixtures together.
Their source/scenario sets, expected exits, all ELF and transaction identities,
all measured resources and all budgets are unchanged. Nineteen recorded
artifact identity tuples across nine current fixture files receive only their
new lowering/source-map/verified-bundle hashes. Temporary replay instrumentation
was removed before validation; all 32 focused tests pass in root
`.cap/logs/1791456549-43289.log`. Historical evidence archives remain unchanged.
This does not turn the failed backend replay into a passing receipt; a fresh
clean backend gate is still required, and whole-queue CI remains deferred.

### Independent module projection prototype

The managed worktree adds `interface::project_bundle` and a private
`CheckedModuleProjection`, with fixed byte/traversal ceilings and conservative
directional matching. Actual compiled candidates exercise nested same-width
owner substitution, field order, generic constraint, witness signature, inferred
effect, public omission and dispatch-tag changes at O0–O3. Extra candidate types
and dispatch variants are permitted; optimizer-pruned helper declarations remain
present without inventing an executable binding. The 44 focused interface and
checker tests pass in worktree `.cap/logs/1791455910-23121.log`. This is API
coherence only: whole H1 receipt/admission, H2 runtime handles, #29 and #44–#46
remain unfinished. Full CI is still deferred until the queue is implemented.

### Explicit source evidence and unchanged ordinary metadata budgets

Backend replay on signed `4986c5bc` stopped at the existing entry metadata size
budget (`amm_pool` 164,966 > 159,744 bytes); it is a failed gate, not a passing
receipt. Source catalogs are now explicitly requested by native
`CompileOptions::source_contracts` / `cellc build --source-contracts` and matching
`build-plan --source-contracts`. Default builds omit the additional catalogs;
module projection still rejects missing evidence. The original public API stays
complete, including pruned public functions. All four bundled examples and their
entries pass unchanged metadata/ELF budgets in root
`.cap/logs/1791459143-10330.log`; no resource ceiling was raised.

The 58 focused interface/entry/checker tests pass in
`.cap/logs/1791459152-10520.log`. The new CLI test passes in
`.cap/logs/1791459551-19006.log`, checking actual four-file bundles, identical ELF,
mode-specific plan/cache identities, repeated cache hits and lock-bound mode
switches. It also caught and fixed cache refresh using the caller's default
edition rather than the package's mandatory edition. Cache v65 binds source
contracts explicitly; browser programmatic requests reject unavailable evidence.

Actual CKB-VM replay refreshes 24 current artifact identity tuples (72 sidecar
hashes) and the canonical anchor ProtocolBundle hash. All source/scenario sets,
exits, ELF/transaction hashes, resource measurements and ceilings match the
previous committed fixtures exactly. Temporary replay instrumentation was
removed byte-for-byte. All 32 current business/resource tests pass in
`.cap/logs/1791459586-19729.log`. Historical archives are unchanged. The module
projection commit is signed `b7ada744`, whose worktree dev passed in
`.cap/logs/1791457344-68409.log`. A fresh full backend gate is still required.
Whole H1/H2, #29 and #44–#46 remain unfinished, and whole-queue CI stays deferred.

### Frozen source closure prerequisite

Signed `34ce0fec` makes source evidence explicit without raising metadata/ELF
budgets; root dev passed in `.cap/logs/1791459781-24010.log`. Its clean backend
with `CELLSCRIPT_COUNTER_CCC=1` is being replayed separately. The worktree native
`frozen_interface` prerequisite consumes actual locked packages, pinned chain
identity, entry selection and sources, compares before/after closures and actual
source declarations, and independently checks the stored four-file bundle.
It never repins, accepts planned locks or reconstructs a checked object from
raw JSON. Physical local directory paths are excluded from exported identities.

Static preflight rejects >4 MiB files, >16 MiB aggregate input, >32 packages,
>256 modules, >4,096 directory entries, depth >16, escaped roots and source
symlinks/nonregular files before ordinary source hashing. Concurrent hostile
filesystem allocation bounds are not claimed. Six focused native tests pass in
worktree `.cap/logs/1791461213-61749.log`; the earlier four-test precursor and
strict compiler clippy passed in `.cap/logs/1791461009-53316.log` and
`.cap/logs/1791461034-53968.log`. The expanded source-preflight changes still need
their own dev gate before commit. Whole H1/H2, #29 and #44–#46 remain unfinished,
independent security review is unassigned, and full CI remains deferred.

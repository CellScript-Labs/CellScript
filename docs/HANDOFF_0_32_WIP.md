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

### Transaction-pool snapshot race in the clean backend replay

The complete root backend on signed `34ce0fec` failed in the live stateful
acceptance step; `.cap/logs/1791460969-52061.log` and
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261008-201317.json`
retain the failure. Compiler/checker/shape/resource tests passed, but the gate
is not a pass. Deployment `nft.cell:create_listing` was rejected with
`Resolve failed Unknown` for funding OutPoint
`f03a7fa1c105d6cdb8c2c68fcaea273da3117a9768f8edcddb658fb26c241619:0`.
Inspection of that stopped test node confirms this is its latest block's
cellbase (block 165, transaction index zero) and remains live.

Pinned CKB `f7fa4436` documents `tx_pool_info.tip_hash/tip_number` as the pool's
associated chain snapshot; its reorg service updates that snapshot asynchronously.
The harness previously observed core liveness before immediately submitting to
the pool. It now waits for both pool hash and height to match the core tip before
submission and after commitment, with 80 polls and 50 ms intervals. A rejected
transaction is never retried; malformed identities or failure to catch up reject
the run. This changes harness synchronization, not Script semantics, transaction encoding or
VM/resource ceilings. Focused mocked-RPC tests cover lag, matching hash but wrong
height, missing/malformed identities, bounded timeout and no transaction resend.
Fresh dev and a new clean backend/CCC replay are required before pushing.

Frozen-source prerequisite `53c36dc6` is signed. Its full worktree dev passed in
`.cap/logs/1791461406-69970.log`; staging the new native sources and rerunning
source policy also passed in `.cap/logs/1791462349-21846.log`. Whole H1/H2, #29
and #44–#46 remain unfinished, review is unassigned, and full CI remains deferred.


### Fixed policy parameter decoder prerequisite

Transaction-pool snapshot synchronization is signed `23822919`; root dev passed
in `.cap/logs/1791462533-25880.log`. A fresh clean backend/CCC replay is running
on that exact source state. The isolated worktree adds optional independent
`entry_codec` evidence described in `docs/FIXED_POLICY_PARAMETER_DECODER.md`.
This does not change the emitter, ordinary checker admission, source syntax or
metadata/resource ceilings. Runtime-bound null transport is not Cell-data
codec evidence. Bool, enums, named/dynamic witness values, Script.args, outgoing
stack arguments and profiles exceeding 1536 payload bytes/eight ABI registers
reject rather than downgrade.

Five new focused checker tests plus two existing fixed-result tests passed in
worktree `.cap/logs/1791463805-58584.log`. Twenty rebound machine mutations
across O0–O3 reject despite ordinary bundle inspection passing, including
removed i32 sign extension. The original real-VM test name filter mistakenly
selected zero tests; it is not evidence. The corrected qualified invocation
passes in `.cap/logs/1791463601-50873.log`, with 16 positive and 48 malformed
parameter cases. A host encoder rejection was fixed by corrupting magic only
after canonical envelope construction. Initial aggregate-index VM experiments
failed closed before codegen and are not executable aggregate evidence; the
remaining shared-variant VM test uses supported signed scalar comparisons; it
passes in `.cap/logs/1791464032-67375.log` with eight positive and eight negative
cases across both variants and O0–O3. Strict checker clippy passes in
`.cap/logs/1791464086-68469.log`.

Full dev, fresh matching backend and signed commit remain required for this
slice. Full H1/H2, #29 and #44–#46 remain unfinished. Independent security review
is still unassigned, and full CI remains deferred until the whole queue is
implemented.


### All-bundle module catalog prerequisite and fresh replay archive

The optional fixed decoder certificate is signed `3fec7445`; full worktree dev
passed in `.cap/logs/1791464299-74345.log`. Native and checker clippy passed, with
no emitter, metadata wire or resource ceiling changes. Root adopted the commit
but it still needs fresh matching backend evidence before its own push.

The complete clean root backend with CCC on `23822919` passed in
`.cap/logs/1791463561-49750.log`; that commit and all seven pending signed
commits were pushed to public `0.32`. Fresh source-bound acceptance and actual
CCC migration evidence, the earlier failed backend and live-funding inspection
are copied byte-for-byte under `docs/reports/0.32/pool-snapshot-replay/`, with
normalized gzip storage and raw/stored hashes. This archive binds `23822919`,
not the later containing archive commit. Full CI remains deferred.

The next worktree prerequisite checks an actual required four-file bundle and
all 1–32 candidate bundles. Per-file 4 MiB and total 16 MiB preflight happens
before any parser, including unselected candidates and repeated code bundles.
All candidates undergo independent module projection and directional checking;
no partial token escapes on error. Its private checked object separately binds
exact ELF, metadata, lowering and source-map bytes and rejects substitution,
reordering, required-bundle changes or count changes. Reused code bytes can back
distinct Script args; this layer does not classify deployment receipt duplicates.

Four focused catalog tests pass in worktree `.cap/logs/1791465339-26996.log`.
The exact aggregate limit tests, full dev and matching backend are still pending.
This is all-bundle API coherence only: complete source/Cell-data codecs,
package interface ownership, actual deployment/history admission, nominal source
handles, H2 runtime enforcement, #29 and #44–#46 remain unfinished. Independent
security review remains required and unassigned.


The exact total-byte boundary tests pass in
`.cap/logs/1791465514-30653.log`; strict compiler/checker clippy passes in
`.cap/logs/1791465581-32114.log`. The native `freeze_module_catalog` companion
owns actual frozen source snapshots, checks pinned chain identity agreement and
binds all source-context identities to the checked catalog. Its two focused
native tests pass in `.cap/logs/1791466005-40806.log`. Their multi-declaration
fixture exposed a source-order versus canonical declaration-order comparison
bug; reconstruction now canonicalizes declaration identities without changing
field, parameter or binder order. Full updated clippy/dev and matching backend
remain required. See `docs/CHECKED_MODULE_CATALOG.md` for the exact limits;
none of this establishes a stable package family or full open admission.

The expanded native snapshot tests cover O0–O3 and pass in
`.cap/logs/1791466306-47169.log`; updated strict compiler/checker clippy passes
in `.cap/logs/1791466403-49349.log`. The fresh replay archive's eight files
were rechecked against all raw/stored sizes and SHA-256 digests. Full dev is
now the next gate for the exact staged catalog source state.


The exact staged catalog/native/archive source state passed the full worktree
`dev` gate in `.cap/logs/1791466560-52668.log` (92,977 bytes). It was signed as
`76ad140e` and fast-forwarded into root `0.32`, after signed `3fec7445`. Root
is running the matching clean backend with the pinned CKB checkout and CCC;
these two commits are not yet pushed. Full CI remains deferred.

The next partial #28 prerequisite independently checks actual fixed Cell-data
read setup and immediate success/exact-length gates. It binds all four raw
bundle hashes and actual ELF addresses. Each typed fixed Cell location must
have exactly one checked read; source/index ambiguity, missing reads, dynamic
indexes, unknown/unsupported syscalls, noncompact frames, changed capacity,
status gates, length gates or incoming interior flow reject. The initial
profile uses the existing 512-byte buffer and excludes saved FP/RA from its
bounds. It adds no emitter, sidecar schema, cache, dependency or resource
ceiling changes. Runtime helpers, field decoding and general alias/provenance
closure remain outside this evidence; it grants no full receipt or admission.

The optional branch-relaxation view now captures a logical-to-actual address
map when explicitly requested; existing callers avoid the extra map. Read
records restore real instruction addresses before hashing. The 64-read test
checks all restored syscall addresses against the actual ELF; a real
65-read/64-variant artifact fails the finite read-site ceiling without raising
the existing 64-variant limit. Four focused tests pass in worktree
`.cap/logs/1791468707-4587.log`, including 52 O0–O3 fully rebound mutations
accepted by ordinary inspection but rejected by this stronger check. The new
real VM test passes in `.cap/logs/1791468509-1251.log`: 16 valid mint/burn
cases at zero/nonzero absolute group positions and 64 short, trailing and
512/513-byte negative cases with stable error 4. The fixture byte oracle
checks literal little-endian bytes. Full updated clippy/dev remain pending.

The issue #28 required contract was re-read from GitHub this turn. Complete
public codec receipts, stable nominal ownership, actual deployment/args/deps,
authorized Type history and H2 runtime enforcement are still required. #29
and #44–#46 are not completed. Independent security review remains required
and unassigned. Full CI runs only after the entire user queue is implemented.

## Clean catalog replay, public push and parameter storage follow-up

The root catalog/backend/CCC replay completed successfully on clean signed
`76ad140efa158b99c450d4b0b79113b5d6c4f7b4`:
`.cap/logs/1791467658-76374.log` (225,441 bytes). Strict full audit is
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261008-221146.json`;
acceptance is
`target/ckb-cellscript-acceptance/1791468920-production-12484/ckb-cellscript-acceptance-report.json`;
node evidence is `target/counter-node/1791469205-45779/evidence.json`.
Acceptance and node evidence bind that commit with `git_dirty=false` and
tracked-source SHA-256
`0x0412aa1ff567f36c095c1690aaf8e6d4a5136aee1753aace564cbb551f0daaea`.
The three CCC transition rows confirmed with 111,664,042 / 111,914,152 /
111,830,878 cycles and 1237 / 1632 / 1237 bytes. Local public test seeds remain
non-production; this does not complete general #45 lifecycle admission.
Signed `3fec7445` and `76ad140e` were pushed to `public/0.32` and the remote
head verified. The tracked replay archive still accurately binds earlier
`23822919`, not this later replay.

Fixed read gates passed updated strict clippy in
`.cap/logs/1791468842-8789.log` and full worktree dev in
`.cap/logs/1791469039-42175.log` (94,131 bytes), then were signed as
`6aa93612`. They await a batched root backend replay with the following
parameter-storage prerequisite before publication.

The new optional parameter-storage checker combines private read and decoder
objects, derives source-ID scalar slots, checks actual ABI spills and immediate
post-guard buffer reception, rejects incoming interior flow, and bounds/disjoins
the eligible storage. It adds no producer validity flag, emitter change,
sidecar schema, dependency, cache or resource ceiling. Its evidence ends at
pointer reception; later fields, lifetime, general alias freedom and helper
closure remain unproven. See `FIXED_CELL_PARAMETER_STORAGE.md`.
The initial 24 O0–O3 rebound pointer/spill/source-ID mutations pass ordinary
inspection/read gates but reject storage checking in
`.cap/logs/1791470782-79188.log`. The real VM fixture passes with all three
checks in `.cap/logs/1791470982-83428.log` (16 positive, 64 negative cases).
Additional spill-collision/output-only fixtures and full clippy/dev are the
next checks for this exact source state. H1, #29 and #44–#46 remain unfinished;
independent review remains unassigned and full CI stays deferred.

The spill-collision/output-only follow-up passes at O0–O3 in
`.cap/logs/1791471404-92360.log`. The first collision-check prototype wrongly
excluded the actual ABI length spill at the checked Cell size slot and failed
two positive fixtures. It was corrected to admit only the identified Cell's
length register at that checked slot while retaining unique slots/registers
and scalar-region bounds elsewhere. No fixture, ABI or resource limit changed.
Updated strict compiler/checker all-target clippy passes in
`.cap/logs/1791471497-94579.log`. Full dev is next; sources freeze during it.

## Signed read/storage publication and direct field materializations

Exact staged storage sources passed full dev in
`.cap/logs/1791471606-97232.log` (94,311 bytes), then were signed as
`f07b8a1cb7451ad33f2b14b9773b09f473fca2aa`, after signed `6aa93612`.
Root fast-forwarded both commits and passed clean full backend/CCC in
`.cap/logs/1791472481-18774.log` (226,047 bytes). Strict full audit is
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261008-233206.json`;
acceptance is
`target/ckb-cellscript-acceptance/1791473738-production-55704/ckb-cellscript-acceptance-report.json`;
node evidence is `target/counter-node/1791474025-88743/evidence.json`.
Acceptance and node evidence bind `f07b8a1c` with `git_dirty=false` and
tracked-source SHA-256
`0xfb7c7e131d9afaf8f444ba6773643a369e8637196f7d4758716f58ff7039ff66`.
All three CCC rows confirmed using actual fee signatures, with 111,742,208 /
111,890,107 / 111,741,254 cycles and 1237 / 1632 / 1237 bytes. The two commits
were pushed and `public/0.32` verified at the exact signed storage commit.
The seven-file replay archive is `docs/reports/0.32/read-storage-replay/`;
its source identity is separate from its containing archive commit. No
production setup or general #45 admission follows from these local test seeds.

While root backend sources stayed frozen, the isolated worktree added the next
partial #28 field check. Direct unsigned fields bind actual pointer loads,
bytewise or native materializations and checked layout offsets/widths.
Receiver dominance, caller pointer/buffer/frame/return preservation and owned
instruction coverage are independently checked. Unknown aliases, field forms
and calls reject. The existing membership helper now has a finite checked
96-byte private memory/return profile, separate from role authorization.
The first no-call prototype rejected that real helper; the helper was proved
from decoded bytes instead of permitting opaque calls.

Three focused field tests pass in `.cap/logs/1791474688-2960.log`: 96 ordinary-
accepted/storage-accepted field mutations, 24 helper mutations, alternate
native-LD positives plus eight wrong native offsets/widths, and signed-field
rejection. Nested Cell access initially failed fixture compilation with E2105;
the test now verifies that production rejection rather than weakening it.
The 64-variant/64-read boundary test passes in
`.cap/logs/1791474868-6702.log`: 256 materializations accepted, 257 rejected.
Real VM unsigned-byte oracles passed in `.cap/logs/1791473315-44619.log`
(16 positive, 184 byte-change and 64 length negative cases). That run preceded
the later return/frame hardening; the exact final sources still require
updated strict clippy/dev, including the real VM test now wired into dev.
The field evidence ends at decoded registers, not source assignments,
predicates, output codecs, complete receipts or open admission. No emitter,
metadata/lowering schema, cache, dependency or resource ceiling was changed.
Full H1/H2, #29 and #44–#46 remain unfinished; independent security review
remains required/unassigned. Full CI remains deferred until all queue work.

Updated strict compiler/checker all-target clippy passes in
`.cap/logs/1791475309-15801.log`. The replay archive's seven compressed/raw
hashes and sizes are checked again before staging. Full dev is next for the
exact field/archive source state; sources remain frozen during it.

The first field/archive dev gate passed in `.cap/logs/1791475859-27741.log`
(94,902 bytes). Subsequent review reproduced an owner syscall through a live
Cell-pointer alias that the first field check accepted; its fully rebound
reproducer passed in `.cap/logs/1791476868-49719.log`. Existing inline group-end
capacity observations made a blanket syscall ban reject valid compiler output.
They now have an independent frame-local capacity/hash memory profile, with
field IDs verified against the pinned CKB source (0/3/5, not Script fields 2/4).

The shared register-definition/constant analysis omitted RV64 word writes.
Twelve actual rebound pointer substitutions passed before correction in
`.cap/logs/1791477653-65849.log`. Destination-write recognition now shares the
ELF opcode classifier; word constants truncate/sign-extend correctly, opaque
calls invalidate caller-saved facts and syscall returns invalidate a0.
Six focused field tests pass in `.cap/logs/1791477793-68808.log`, including
32 added observation/alias/word negatives and 12 alternate observation/ADDIW
positives. The exact updated sources still require strict clippy and fresh dev
before their signed commit, and a clean backend gate before push. Whole CI
remains deferred; complete H1/H2 and the remaining issue queue are not done.


## Field/register closure and finite external availability follow-up

The exact staged field, shared register and syscall source state passed full
worktree dev in `.cap/logs/1791478438-82279.log` (95,361 bytes), then was
G-signed as `588f6a2821a00ab561ced676ad522a6d874f5ca6`. Clean root adopted
that commit and is running the matching pinned backend with CCC before push.
Its all-field focused tests reject 160 fully rebound counterexamples; actual
unsigned byte oracles remain in the dev gate. Full CI remains deferred.

The next #28 slice combines actual public policy dispatch availability, flat
unsigned public layouts, fixed witness decoding, Cell field/storage evidence
and the actual external scalar source-slot reception. See
`docs/FIXED_EXTERNAL_CODEC.md` for its deliberately finite profile. Pruned
public functions and retained helpers without external dispatch reject;
uninstantiated generic declarations remain non-executable. It does not supply
full H1/H2, general outputs/helpers, verifier-specific ABI, nominal I, actual
deployment/history/authorization, #29 or #44–#46. Tests and formal gates for
this new slice are pending. Independent security review remains unassigned.


The four focused external-codec tests pass in
`.cap/logs/1791479803-21936.log`: 20 O0–O3 typed scalar fixtures and 40
fully rebound scalar-spill changes that ordinary/Cell field checking accepts
but the stronger external reception check rejects; 24 unsupported/pruned/output/
multi-Cell cases; absent public generic declarations and actual retained type/
function instances. Optimization can erase an inlined function instance; tests
then require declaration-only availability and no retained function entry.
The implementation matches actual `struct`/`enum` kinds, not a fictitious
`type` instance kind. No generic executable ABI is inferred from source calls.

Four focused real VM certificate tests pass in
`.cap/logs/1791479853-23160.log`. The new scalar-witness test includes 40
positive runs, 80 truncated/appended negatives (exit 25), 128 byte/signed-value
negatives (exit 5) and literal byte oracles; the existing unsigned Cell-field
oracle now additionally requires the external certificate. Final resource
output, formatting, strict clippy, inventory, full dev and matching backend
for this slice remain pending. It does not complete H1/H2 or change CI deferral.


Updated strict compiler/checker clippy passes in
`.cap/logs/1791479926-24843.log`. Final focused scalar VM resource output
passes in `.cap/logs/1791479977-25830.log`; all 40 rows and the normalized
raw log are archived under `docs/reports/0.32/external-codec/`, with staged
source SHA-256 bindings and honest dirty-checkout provenance. These fixtures
measure 9,081–9,390
CKB-VM cycles, 3,528–3,664
ELF bytes and 9–16 inner argument bytes. They are not general #46 host/prover
benchmarks or full transaction/deployment evidence. The 30M runner ceiling is
unchanged. Inventory refresh passes in `.cap/logs/1791479987-25947.log`.
The staged source state is now frozen for full dev before signing.


## Clean field replay and deployment-byte binding follow-up

Signed `588f6a2821a00ab561ced676ad522a6d874f5ca6` passed the complete clean
root backend with CCC in `.cap/logs/1791479308-2595.log` (226,722 bytes).
Strict full report: `strict-backend-audit-full-20261009-012610.json`;
acceptance: `target/ckb-cellscript-acceptance/1791480596-production-39471/ckb-cellscript-acceptance-report.json`;
node: `target/counter-node/1791480907-72893/evidence.json`. Both source-bound
reports have `git_dirty=false`, exact commit `588f6a28` and tracked-source
SHA-256 `0x01eb765851b48c0aea059b5fffe42c4f9b8f54234232155bc07553061f00cc5e`.
CCC's three fee-signed, confirmed transition rows measure 111,675,752 /
111,909,903 / 111,836,422 cycles and 1237 / 1632 / 1237 bytes. Setup is still
local/public-test-seed, with no production admission or general #45 claim.
Public push is in progress; no full CI has run.

The finite external availability/codec source state passed full worktree dev
in `.cap/logs/1791480120-28254.log` (95,889 bytes), then was G-signed as
`baae1aad3f1a26c1f0a14a028c1ffddfeb318963`. It still needs the matching
batched backend with the following origin-byte prerequisite before push.

The next #28 slice computes an exact code Cell OutPoint from canonical CKB
RawTransaction bytes, verifies every output Script/data structure including
unselected outputs, and compares the selected code output bytes with the checked
ELF. Data modes bind the selected code hash to those bytes; Type mode binds it
to the actual output Type Script. Full selected Script args remain in its exact
hash/identity. This is byte-origin evidence, not history authorization,
network observation, on-chain liveness, full H1/H2 or stable admission.
Tests and docs for this new slice remain pending.


Public `0.32` now exactly matches signed `588f6a28`; push passed in root
`.cap/logs/1791481104-78875.log`, and the remote head was independently checked.
Seven raw field/register dev/backend/strict/acceptance/node/CCC/push files are
archived with raw/stored SHA-256 and byte counts under
`docs/reports/0.32/field-register-replay/`. They bind `588f6a28`, not their
later containing commit. The external scalar archive's six staged-source
SHA-256 entries were checked against signed `baae1aad` before recording that
source commit; its original dirty pre-commit provenance stays explicit.

Four focused code-origin tests pass in `.cap/logs/1791481578-89238.log`.
SDK-produced raw transaction/Script/data hashes agree across O0–O3 and all four
supported hash-type bytes. Mutated ELF/selected code identity, absent/wrong output,
corrupted tables, truncation/trailing bytes, unselected invalid Script tags and
output/data-count mismatch reject. Exact 256/257 Cell and input, 64/65 raw/header
dep and 4096/4097 Script-byte boundaries are exercised, as are duplicates,
unknown dep tags, missing Type Scripts and preparse aggregate/per-file budgets.
A follow-up applies every caller byte budget before any parser; final focused
replay, strict clippy, full dev and batched backend remain required. This is
host byte-origin only. See `docs/CODE_CELL_ORIGIN.md` for its unsupported claims.


The final code-origin focused tests pass in
`.cap/logs/1791482071-433.log`; all artifact/record/source-map byte budgets
are now checked before malformed raw transaction bytes could reach a parser.
Strict compiler/checker clippy passes in `.cap/logs/1791482194-3035.log`.
All seven field/register archive files were rechecked against their manifest's
raw/stored sizes and SHA-256. No emitter, dependency, metadata wire, compiler
version or resource ceiling changed. The exact staged source state now freezes
for full dev. The two pending signed/checker slices will share the next clean
backend/CCC replay before push. Full CI remains deferred; #28/H1/H2, #29 and
#44–#46 remain incomplete and independent security review remains unassigned.

## Native all-member code/source closure follow-up

The exact staged code-origin state passed full dev in
`.cap/logs/1791482265-4721.log` (96,267 raw bytes) and was G-signed as
`8ea6105ecbace5c392eab729ede232bdef801203`. The earlier finite external-codec
slice remains G-signed at `baae1aad`; both descend from public `588f6a28`.
Remote `0.32` was fetched again and still equals `588f6a28`; no remote divergence
needs merging. These two commits still require the batched clean backend/CCC
replay before push. Full CI has not been started.

The next native `freeze_code_catalog` constructor consumes actual privately
compiled source snapshots and the raw creation-transaction/complete selected
Script bytes. It checks every 1–32 candidate's directional module contract,
finite public policy codec and exact deployment byte origin under one shared
16 MiB input ceiling before new parsers. The required and candidate chain
ID/genesis pins must agree. Concrete (code transaction hash, output index,
complete Script hash) duplicates reject; different concrete args remain distinct
without an authorization claim. No partially checked catalog escapes on error.
Owned immutable getters retain source context, bundles and raw bytes for later
materialization. The native source compilation was already separately bounded;
the new shared preflight does not claim to occur before those earlier compiles.

This is still a host-only prerequisite. It grants no nominal source I/handle,
consumer authorization, consensus commitment, Type replacement history, liveness,
ProtocolBundle or generated-builder parity. Whole H1/H2, #29 and #44–#46 remain
unfinished, and independent security review is still unassigned. Focused tests,
strict clippy and full dev for this new slice are pending; all three slices will
share one clean backend/CCC replay before publication. See
`docs/FROZEN_CODE_CATALOG.md` for the exact scope and limits.

Final native code-catalog focused replay passes four tests in
`.cap/logs/1791483210-26066.log` (36.95 seconds). The negatives now include
independent chain/genesis conflicts and an invalid final member in a full
32-member set, as well as per-file/shared/caller-budget preflight before
malformed raw bytes. Strict compiler/checker all-target clippy passes in
`.cap/logs/1791483233-26540.log`. The candidate directories remain local test
snapshots and their code transactions are synthetic SDK-produced byte oracles;
these results are not network deployment or consensus evidence. The exact
staged source will now freeze for dev and then G-signed clean backend/CCC replay.

## Resolver-owned baseline source follow-up

Native all-member source/codec/code-origin closure passed dev in
`.cap/logs/1791483288-27803.log` (93,132 raw bytes), then was G-signed as
`16e6b7215d85d3680704f1b180ddbb382f9c05a1`. The clean root branch `0.32`
fast-forwarded to this exact commit and started batched full backend/CCC replay
in `.cap/logs/1791484072-47744` with the clean pinned CKB checkout
`/tmp/cellscript-ckb-032`. Root tracked files remain frozen while that gate runs.
The public branch still points to `588f6a28` until this replay passes; full CI
has not been started or dispatched.

In the managed worktree, the next native `resolve_code_catalog_source` slice
binds the baseline to the consumer's actual defining source owner. It consumes
private compiled/checked values, never caller labels or exported JSON. A root
compilation's RootSnapshot label must not erase the selected dependency's real
Git/Registry/local origin. Owner identity contains the actual defining package,
module, required checked contract and all selected baseline-closure package IDs;
the separate binding includes consumer context and code-catalog identities.
Dependency alias changes therefore preserve owner identity, while different
owners and changed transitive pins remain distinct. Baseline module owner,
relative path, byte hash/length, package snapshot/version and every transitive
origin must match; the consumer and all catalog inputs share 16 MiB before
ownership traversal. No partial token escapes.

Five focused tests pass in `.cap/logs/1791484953-74208.log` (20.00 seconds).
Actual O0–O3 source compilation covers alias invariance and equal-shaped foreign
definitions from different packages, including crossed baseline rejection.
Changed source/version, chain/genesis and absent imports reject. A local Git
fixture adds an empty commit: identical source/manifest bytes with another
selected transitive revision still reject. A valid synthetic code catalog just
below 16 MiB plus the additional consumer exceeds the shared budget and rejects
before its deliberately absent defining module is visited. The initial Git
fixture accidentally appended a duplicate manifest table; it was corrected to
use the typed manifest API, and the final complete replay passes.

Strict clippy and full dev remain pending for this worktree slice. It changes
native package ownership evidence only, with no parser/type syntax, emitter,
machine ABI, dependency, schema wire, compiler version or budget expansion.
This does not complete source-level nominal I, H1/H2, immutable authorization,
Type replacement history, ProtocolBundle/generated-builder parity, #29 or
#44–#46. Independent review remains unassigned and mandatory for stable admission.
See `docs/RESOLVED_CODE_CATALOG.md` for the supported host boundary.

Final strict compiler/checker all-target clippy passes in
`.cap/logs/1791485127-77943.log`. The source-owner slice is ready to freeze for
dev. No fresh matching full-backend evidence is claimed for this later native
ownership-only delta; the clean full-backend/CCC run binds exactly `16e6b721`.
Keep root tracked files frozen until that run completes, verify its actual
source commit and clean state, then push that tested commit before advancing
root to any later worktree commit. Full CI remains deferred until the complete
implementation queue is ready.

## Clean catalog publication and pre-typing source capture

Clean root full backend/CCC passed in `.cap/logs/1791484072-47744.log`
(227,807 raw bytes), binding G-signed `16e6b721`. Strict full audit
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261009-024554.json`
passes. CKB acceptance
`target/ckb-cellscript-acceptance/1791485365-production-86000/ckb-cellscript-acceptance-report.json`
and Counter node `target/counter-node/1791485669-19925/evidence.json` pass, both
with this exact source commit and `git_dirty = false`. Their tracked-source
SHA-256 is `0xfd0ce2e706921c6af739a0c830bf10f76cf8e210f3d96f3423d9a23405b20927`.
The CCC migration report contains three actual fee-signed/confirmed rows:
old-key update 111,647,830 cycles / 1,237 bytes; old-key authorized migration
111,915,349 / 1,632; new-key successor update 111,779,027 / 1,237.
This is existing private local-network replay, not general #45 or #46 evidence.

`16e6b721` was pushed to public `0.32` in
`.cap/logs/1791485889-23519.log`. The later native source-owner slice passed dev
in `.cap/logs/1791485360-85467.log` (93,598 raw bytes), was G-signed as
`04a70f612587255b1aec1c422f004b155cde2b92`, and was fast-forwarded/pushed in
`.cap/logs/1791486319-35071.log`. The remote head was independently verified as
this exact commit. This ownership-only delta introduces no machine ABI/emitter
change; no later clean full-backend claim was made. Eleven normalized raw replay
files and their sizes/SHA-256 are archived under
`docs/reports/0.32/code-catalog-replay/`, retaining the actual earlier provenance.

The next native `FrozenPackageSources` / `ResolvedSourceCatalog` path captures
real parsed/pinned source ownership before consumer type checking. A new type
context cannot require an already compiled consumer that itself depends on that
context. Source capture reuses existing 4 MiB/file, 16 MiB/source, 32-package,
256-module and offline lock/environment rules. Binding rereads unchanged actual
sources and reuses the compiled-owner matcher, extracted from G-signed
`04a70f61` without rewriting its ownership comparisons. Both construction and
recheck reject planned lock overrides, even for an identical graph. The source
record stores hashes/context, not all file contents or a consumer ELF.

The five existing owner tests pass after extraction in
`.cap/logs/1791486421-37404.log`. Eleven source/ownership tests pass in
`.cap/logs/1791486828-47390.log` (10.64 seconds): O0–O3 context parity, lock byte
preservation, changed/repinned sources, unchanged-owner binding before typing,
source mutation before binding, missing/unpinned/malformed/oversized input and
planned override isolation. An unknown FutureHandle is source-bound but still
rejected by semantic compilation; its diagnostic is retained in the related
diagnostic list. No source handle syntax, consumer typing or execution is admitted.
Strict clippy and full dev remain pending for this slice. Full CI stays deferred;
whole H1/H2, #29 and #44–#46 remain incomplete and independent review unassigned.

Final strict compiler/checker all-target clippy passes in
`.cap/logs/1791486929-50100.log`. All eleven replay archive files were rechecked
against their recorded raw/stored sizes and SHA-256. The exact staged parsed-source
slice now freezes for dev before signing/publication. No emitter, machine ABI,
dependency, historical wire, compiler/toolchain version or budget changed; no
fresh full-backend claim is attached to this native-only delta.

## Parsed-source publication and target-selection correction

The exact staged parsed-source/archive slice passed full dev in worktree
`.cap/logs/1791487169-55472.log` (93,995 command-output bytes), with unchanged
staged files. It was G-signed as `3a15ed9a0a462c089845bff6eb08993dd9221deb`,
fast-forwarded into clean root `0.32` and pushed in root
`.cap/logs/1791487942-73961.log`. Independent remote lookup confirmed that exact
head. This native-only delta carries no new clean full-backend/CCC claim;
clean replay provenance remains `16e6b721` as archived above.

The next correction closes an actual catalog selection gap: a selected Script
could be byte-bound to the actual code Cell while using data1 for an ELF whose
independently checked `ckb` profile requires data2/VM2. Source catalog fixtures
previously inherited data1 from their unrelated deployment lock fixture. Their
selected Script now explicitly uses data2. The original byte-origin v1 API
remains byte-only and still accepts all four legal hash types.

A separate private `CheckedTargetCodeCellOrigin` consumes actual origin/codec
proofs and the independently recomputed runtime contract. It enforces the
existing checked target mapping (`ckb` -> data2; `ckb-type-hash` -> type). Frozen
native catalogs require this check for every member and advance their own
experimental record/hash domain to v2, binding ordered target-origin identities.
The checker dependency boundary and existing lowering/metadata/wire versions
remain unchanged. Type chain activation/history and immutable authorization are
not inferred; whole H1/H2 and #29/#44–#46 remain unfinished. Focused tests and
dev for this next slice remain pending. Whole-queue CI remains deferred and
independent security review remains unassigned.

Final target-selection replay passes five code-origin/target tests in
`.cap/logs/1791488110-77793.log` and all 23 frozen-interface tests in
`.cap/logs/1791488178-80108.log` (16.86 seconds). The target matrix includes
both compiler profiles at O0–O3, eight valid/24 mismatched hash-type selections,
exact-args identity separation and independent hashing of canonical record bytes.
Native otherwise byte-valid data/data1 members reject even when unselected or
in slot 31. Two initial test assertions were corrected: hashing a generic JSON
Value reordered record keys, and the existing native record assertion still
expected v1. No producer/checker logic was changed to satisfy those assertions.
Strict compiler/checker all-target clippy passes in
`.cap/logs/1791488198-81214.log`. The earlier parsed-source dev/push logs are
now archived with all thirteen raw/stored sizes and SHA-256 verified. This exact
source state freezes for full dev before G-signing and public push.

The first full target-selection dev run in
`.cap/logs/1791488244-82258.log` reached the final business-corpus freshness
check and rejected a stale inventory digest. All preceding tests, reproducible
builds, quick backend audit and 139 syntax combinations passed. The authoritative
`check-business-corpus --write` command in `.cap/logs/1791489102-2087.log`
changed only `tests/fixtures/business_corpus.json`'s inventory SHA-256; all release
requirements remain pending. The exact refreshed source state now reruns full
dev. The failed gate is not presented as a passing dev or full-CI result.

## Target publication and finite receipt construction

The refreshed target-selection state passed full staged dev in
`.cap/logs/1791489149-3091.log` (92,932 command-output bytes), was G-signed as
`35a9565b260890e2b32f6082af51e5cfa36b3866` and pushed to public `0.32` in
root `.cap/logs/1791489620-16694.log`. Independent remote lookup confirmed this
exact head; root and worktree were clean. The passing dev/push logs are now
archived alongside thirteen previous files; all fifteen raw/stored lengths and
SHA-256 were verified. The earlier stale-inventory failure remains a failed run.
No later clean full-backend/CCC evidence replaces the actual `16e6b721` source.

The next private `CheckedFixedPolicyReceipt` composes actual independent
API/codec/origin/target proofs with the checked declaration and entry contract,
exact bundle hashes/lengths and the artifact report. The finite profile remains
Type-policy/unit/scalar/flat-unsigned-Cell. Public constants explicitly reject:
existing projection checks their types but not their values. Uninstantiated
generic declarations remain declaration-only. Exact input rechecks apply fixed
six-input ceilings before hashing and reject any bundle, transaction, index or
complete Script substitution. Directional matching uses actual private module
proofs; predicate differences are not behavioral-equivalence evidence.

Native frozen catalog v3 owns and binds each candidate's actual finite receipt,
including unselected and final members. Historical v1/v2 catalog records are not
reinterpreted. Source/chain ownership checks retain their prior boundary. This
slice changes native/checker evidence only, with no emitted machine/ABI,
dependency, toolchain, wire or runtime budget expansion.

Five focused receipt tests pass in `.cap/logs/1791490432-34228.log` (4.35 seconds),
covering O0–O3 and both targets, actual fields/hash reconstruction, args,
directional extension/incompatibility, templates/constants, substitution and
preparse limits. All 24 frozen-interface tests pass in
`.cap/logs/1791490438-34428.log`. One initial test oracle wrongly expected
`not-provided` for CKB-VM evidence; the checker correctly reports `not-executed`
and chain evidence `not-provided`. Only that assertion was corrected.
Strict clippy and full dev remain pending for this new slice. Whole H1/H2,
source handles, immutable authorization, version/status/floors and authenticated
Type history remain required; #28/#29/#44–#46 are unfinished. Independent review
is unassigned and unwaived. Full CI stays deferred until the implementation queue
is complete. See `docs/FIXED_POLICY_RECEIPT.md` for the exact finite boundary.

Final strict compiler/checker all-target clippy passes in
`.cap/logs/1791490460-34013.log` (21.53 seconds). This source state will refresh
the authoritative business-corpus inventory and then freeze for full dev before
G-signing/publication. No fresh clean full-backend/CCC claim is attached to this
native/checker-only delta.

The authoritative inventory refresh in `.cap/logs/1791490544-37501.log` changed
only the business-corpus inventory SHA-256 to
`0xcc0bdea0ef4c4825eaf3046ff77d9e8fe5cdddf8aff53cc3891b560ce5405e37`.
Read-only validation passes in `.cap/logs/1791490574-38344.log`; release readiness
remains false and all release requirements remain pending. The exact staged
receipt/catalog/documentation/archive state now freezes for full dev.

## Finite receipt publication and all-member policy field binding

Finite receipt/catalog v3 passed full dev in worktree
`.cap/logs/1791490595-38964.log` (98,237 command-output bytes; 98,264 raw archived
bytes). The staged diff SHA-256 was unchanged before/after:
`14fb28246a6f880e9274f845f074d8fa0af0c436413edf9a93707c79c607b356`.
It was G-signed as `e546d0f93dafc52a2e522f80b7422a7f88307a34`, fast-forwarded
into clean root and pushed in `.cap/logs/1791491476-59406.log`.
Independent `.cap/logs/1791491483-59727.log` lookup confirmed that exact public
`0.32` head. These three logs are now archived; all eighteen raw/stored lengths
and SHA-256 were verified. Clean full-backend/CCC evidence remains exactly
`16e6b721`; no fresh machine/production claim is made for this host-only delta.

The next native `FrozenCodePolicy` consumes actual `ResolvedSourceCatalog` and
separately declared `AuthorizationSet` proofs. It checks Header/API/pinned
genesis/runtime/target and every member's private receipt, candidate API, ELF,
complete Script, hash type and actual creation OutPoint. It requires exactly all
catalog members, including unselected/final/inactive members. The finite data2
Type-policy boundary rejects Type-hash receipts lacking history evidence.
Its record binds source-catalog identity, declared root and canonical wire-index
mapping. Exported JSON cannot reconstruct either the private policy binding or
its borrowed selection. Selection applies the existing wire rules and rechecks
actual source ownership; exact receipt input rechecks catch later substitution.

The declared root is not authorized by this result. Status/sequence/floors are
application snapshot choices, not authenticated Registry or package-version
facts. Original code creation input checks are not final transaction/CellDep
checks. Full H1/H2, source handles, Type history, immutable root authority,
ProtocolBundle/builders, #29 and #44–#46 remain required. Independent security
review remains unassigned and unwaived; full CI remains deferred.

The first five focused binding tests pass in `.cap/logs/1791491765-65657.log`
(37.45 seconds). The expanded full frozen-interface suite passes all 30 tests in
`.cap/logs/1791491875-68263.log`: O0–O3 exact/compatible implementations, all 32
members, 12 rebound Header/unselected-Member axes under valid rebuilt trees,
yank/floor/root selection, original input/source substitution and actual valid
native Type-hash receipts rejected without authenticated history. Strict clippy
and frozen full dev remain pending for this latest slice. See
`docs/FROZEN_CODE_POLICY.md` for the exact finite host boundary.

Final strict compiler/checker all-target clippy passes in
`.cap/logs/1791491916-68022.log` (17.92 seconds). This host-only source state
will refresh the authoritative business inventory before freezing for full dev.
No emitter, machine ABI, historical wire, dependency, toolchain or runtime budget
changed. No fresh clean full-backend/CCC or complete H1 claim is supplied.

The authoritative business-corpus refresh in `.cap/logs/1791492045-72488.log`
produced no tracked fixture diff; read-only validation passes in
`.cap/logs/1791492048-72408.log` with existing inventory SHA-256
`0xcc0bdea0ef4c4825eaf3046ff77d9e8fe5cdddf8aff53cc3891b560ce5405e37`.
Release readiness stays false and all release requirements stay pending. The
exact staged host binding/test/docs/archive state now freezes for full dev.


## All-member policy publication and source receipt/version binding

The prior exact frozen host-policy diff passed full dev in worktree
`.cap/logs/1791492108-73842.log` (95,118 command-output bytes; 95,145 raw log
bytes). Its staged 65,380-byte diff retained SHA-256
`084e46062ef1c0c99df28e7116aa40a3a6ce4f364f729873791e38707bc2d13d`
before/after. It was G-signed as
`f6ef9711ece01738794ff6e7c8288faa63a7fc29`, fast-forwarded into clean root
and pushed in root `.cap/logs/1791492948-93324.log`. Independent
`.cap/logs/1791492955-93640.log` confirmed the exact public `0.32` head; both
checkouts were clean. These three logs are now archived; all twenty-one
raw/stored lengths and SHA-256 were verified. Clean backend/CCC source remains
exactly `16e6b721`; no later host delta replaces its provenance.

The next private native source receipt binds each actual candidate source
context, defining module, manifest coordinate/edition/full SemVer version and
independently checked finite artifact receipt. Only the actual catalog factory
constructs it; exported JSON/context labels cannot do so. Native catalog v4
binds all source receipts and does not reinterpret historical v1–v3 records.
Coordinate/module text limits apply before cloning/version parsing/hashing.

The separately named `freeze_source_code_policy` binds the resolver-owned
required owner identity and actual source receipt Member identities. Both exact
and compatible selectable members enforce baseline SemVer precedence; compatible
mode also requires stable baseline/candidate versions and the same major. Build
metadata is committed but does not order precedence. Exact nonselected and
inactive/below-floor history can remain in the tree with older versions;
all their coordinates, API/codec and actual byte bindings still check and they
cannot select. The artifact-only constructor keeps its v1 schema/domain and
omits all source-profile fields; the new source profile uses native v2.

Manifest versions are captured source facts, not authenticated publisher or
Registry release/status facts. The wire root now commits those source receipts
and the actual baseline owner, but remains application-declared and requires
immutable authorization. No source-level `I`/handles, complete H1/H2, general
codec, active chain/VM, authenticated Type history or final CellDep/transaction
proof is supplied. #28/#29/#44–#46 remain unfinished and independent security
review remains unassigned/unwaived. Full CI stays deferred until the complete
implementation queue. No emitted ABI/machine, dependency, toolchain, historical
wire or runtime budget changes in this native-only slice.


The initial full 35-test regression in `.cap/logs/1791493758-11682.log`
passed 33 tests and failed two new assertions. No production rule was weakened:
the existing wire constructor already rejects admissions above the policy
sequence; a private candidate snapshot remains historical evidence after its
physical manifest changes. The corrected tests require the above-sequence wire
to reject and newly captured candidate/version receipts to reject stale policy
members, while retained original snapshots preserve their original versions.
Selection still rechecks the consumer's actual pinned source closure. An initial
test-only compile typo referenced edition on the runtime contract instead of
the fixture's actual package edition and was corrected. A complete rerun and
final clippy/dev are pending; the failed/quiet debugging runs are not passing
validation evidence.

The authoritative business-corpus refresh in `.cap/logs/1791493961-17112.log`
produced no final tracked fixture diff. Read-only validation passes in
`.cap/logs/1791493990-17880.log`, retaining inventory SHA-256
`0xcc0bdea0ef4c4825eaf3046ff77d9e8fe5cdddf8aff53cc3891b560ce5405e37`.
Release readiness remains false and all release requirements remain pending.


The corrected complete frozen-interface regression passes all 35 tests in
`.cap/logs/1791494035-18975.log` (230.63 seconds). Actual O0–O3 builds show
manifest-only version edits leave all four artifact files and finite receipt
unchanged while source receipt and wire root change. Tests cover both selection
modes, SemVer build/pre/major/downgrade boundaries, all 32 members, source owner
versus artifact-only identities, stale recaptured versions, immutable historical
candidate facts and bounded version text. Artifact-only v1 records continue to
omit all source-profile fields. Final strict compiler/checker all-target clippy
passes in `.cap/logs/1791494073-20194.log` (0.40 seconds). The prior failure
remains a failed run, not validation evidence.

The exact staged native source receipt/version/test/docs/archive state now
freezes for full dev before G-signing/publication. No fresh clean full backend,
CCC, full CI, production admission or complete H1/H2 claim is made.

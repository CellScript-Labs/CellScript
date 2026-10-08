# CellScript 0.32 WIP handoff — 2026-10-08

## Status and publication boundary

Implementation is **paused at the maintainer's explicit request**. This document
and its containing WIP commit preserve the current work for another session or
engineer. They are not a release receipt, a completed issue, or a merge-readiness
claim. Do not resume implementation until requested.

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

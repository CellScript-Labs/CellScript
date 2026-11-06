# Bounded Script and memory hash admission

[Issue #100](https://github.com/CellScript-Labs/CellScript/issues/100) closes the
independent checker contract needed to compose existing typed Cell reads with
complete Script hashing and memory BLAKE2b. The change adds checker admission
and regressions for existing APIs; grammar, schema versions, ISA and compiler
emitters retain their existing contracts.

The canonical `__ckb_script_hash` helper has a complete 316-byte instruction
body, including its rejecting path and final return. Its ABI is `a0 = code_hash
pointer`, `a1 = hash_type`, `a2 = args pointer`, `a3 = args length`, and `a4 =
32-byte output pointer`. The checker decodes every instruction, branch and
actual call destination. The helper copies exactly 32 code-hash bytes, admits
hash types 0, 1, 2 and 4, and serializes the canonical 53-byte Molecule Script
prefix plus at most 459 argument bytes. Its 560-byte frame contains a bounded
512-byte preimage and saved arguments/linkage.

The `__ckb_hash_blake2b_var` ABI is `a0 = input pointer`, `a1 = input length`,
and `a2 = 32-byte output pointer`. The checker reuses the independently frozen
complete-body profile for this helper and its compression dependency. Call
relocations normalize only to the exact admitted dependency identity. Producer
sidecar digests grant no body authority. Both closures require contiguous code
ownership and reject external entry into a partial helper body.

For actual typed callers, the memory-effect proof checks:

- One-to-one typed-operation and decoded-call inventories, including hash-only
  callers. Replacing or removing an actual target cannot remove the expected
  caller from checking.
- The actual Script args length equals the typed fixed operand width and stays
  within 459 bytes. Raw packed-data hashing likewise binds its actual length
  to the typed fixed width. Existing packed/opening materialization contracts
  retain their separate byte-computation checks.
- Every read has a concrete extent in the caller's private frame, read-only
  ELF data, or an admitted immutable fixed parameter. Borrowed parameter facts
  originate at incoming ABI words and are checked against every actual caller's
  pointer/extent transport. The public borrowed boundary uses the existing
  complete bounded fixed single-entry witness decoder. Unsupported transports
  continue to reject.
- Every result pointer owns 32 private caller bytes, excludes frame linkage,
  and avoids live protected Cell/view/arena/cache storage. The proof invalidates
  overwritten symbolic facts before continuing ordinary caller analysis.

The proof establishes these runtime bodies, call targets, read extents and
write effects. It does not establish arbitrary source computation equivalence,
the identity of every byte already present in a private frame, deployed Script
existence, authorization or node acceptance. Those remain separate contracts.
No application or finance function name grants an exception.

The general owner is `tests/artifact_checker/memory_hashes.rs`, with the
non-financial `document_config_script_hash.cell` fixture. It covers direct and
ordinary-helper calls under both compiler diagnostic strategies, empty and
459-byte Script args, real encoded VM outcomes, live nested Cell protection,
and rebinding of caller lengths/pointers/results, dependency writes and call
targets. Focused checks are `cargo test --locked -p cellscript --test
artifact_checker memory_hash -- --nocapture` and `cargo test --locked -p
cellscript --test script_hash`. They are separate from gate and production
acceptance evidence.

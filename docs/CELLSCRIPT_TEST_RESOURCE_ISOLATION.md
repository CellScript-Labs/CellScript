# Test binary identity and execution isolation

Status: current execution policy; parallel-suite admission is not established.
Reviewed: 2026-10-30 for the alpha.2 follow-up.

## Binary authority

[The common command helper](../tests/common/mod.rs) executes
`CARGO_BIN_EXE_cellc`, resolving a relative value against `CARGO_MANIFEST_DIR`.
It does not enumerate `target/debug/deps`, inspect modification times, select a
neighboring executable, or fall back to PATH. Direct compiler invocations in the
CLI suite use the same Cargo-provided identity. The MCP subprocess receives that
identity through its child-specific `CELLSCRIPT_CELLC` environment setting.

The common regression creates neighboring executables representing stale and
different-feature builds after the declared executable. Neither changes binary
selection. It also checks absolute and manifest-relative Cargo paths. This
addresses executable identity independently of test scheduling.

## Resource inventory

The review covered [CLI tests](../tests/cli.rs),
[iCKB tests](../tests/ickb_diff.rs), their common command helper and the directly
used execution/cost helpers under `tests/support/`.

| Resource | Current ownership and boundary |
| --- | --- |
| CLI package manifests, locks, source trees and output paths | Most cases create a `tempfile::TempDir` and set only the child command's working directory. A subprocess working directory does not change the test process's directory. |
| Mock HTTP services | CLI fixtures bind `127.0.0.1:0`; each listener receives its own ephemeral port. Registry URL overrides belong to individual child commands. |
| Process environment | The two suites use child-command environment overrides. This review found no `set_var`, `remove_var` or process-wide `set_current_dir` in them. This is a scoped observation, not a guarantee about every future helper. |
| CKB transaction state | The iCKB harness constructs a `ckb_testtool::Context` and transaction fixtures per invocation. Compilation and VM execution consume in-memory bytes. |
| Original iCKB binaries | The execution helper reads pinned fixtures under the benchmark submodule. Initialization, revision changes and fixture writes remain separate coordinated operations. |
| Reports and measurements | Cost helpers also write fixed paths such as `target/cellscript-cost/expanded-measurements.json`; other scenario helpers retain shared evidence directories. These are not isolated merely because compiler selection is deterministic. |
| Package build/cache state and child processes | Tests can invoke build tooling and subprocesses. Unique source directories alone do not prove that every cache, compiler target, report, process lifetime and transitive helper is independently owned. |

## Retained scheduling rule

The [gate](../scripts/cellscript_gate.sh) continues to run the large suites with
`--test-threads=1`, as required by [AGENTS.md](../AGENTS.md). This change makes no
parallel-execution claim and changes no scheduling flag. The known shared report
paths and transitive tooling require a separate isolation change before that
policy can be relaxed.

Any later proposal to parallelize these suites must inventory transitive helper
writes, give each test or run an explicit output/cache namespace, own and join
its child processes, and verify simultaneous execution and cleanup. Faster
binary lookup is not evidence that those conditions already hold.

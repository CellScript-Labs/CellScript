# Test resources and measured fixture candidates

CLI tests that compile bundled examples use a private copy of the example tree.
The copy retains dependency-relative paths and source/lock bytes, excludes
generated output/cache directories, and rejects symlinks. Each test owns its
build products and cleanup. The copy regressions check two independent copies
and preservation of pre-existing source and output files.

The CLI behavior files under [`tests/cli/`](../tests/cli.rs) are included in the
original `cli` integration target. They retain exact test names, feature/platform
attributes, the Cargo-selected executable and existing gate selectors. Shared
helpers remain in `cli.rs`; package copies belong to
[`fixtures.rs`](../tests/cli/fixtures.rs). Splitting source ownership does not
establish independent process, Cargo-build or global-cache isolation.

The compiler's 417 integration cases are grouped by lowering, lifecycle, value,
metadata, CKB and package behavior under `src/compile_pipeline/tests/`. They retain
the original `crate::tests` identities and shared helpers. Mechanical source
checks preserve every attributed test item and the complete 1,137-test library
inventory; relative source includes resolve the same files.

Keep `--test-threads=1` for both `cli` and `ickb_diff`. The gate retains those
flags. No parallel execution claim follows from these file moves or from
deterministic binary selection.

## Ordinary iCKB tests

Ordinary tests only read the checked-in matrix and original binaries. A missing
row or a changed measured execution is a failure. The environment-driven
in-place updater has been removed; no environment variable can turn an ordinary
assertion into fixture acceptance or a fixture write.

The [187 measured cases](../tests/ickb_diff/measured_cases.rs) form one definition
inventory shared by the ordinary tests and the explicit exporter. The original
`ickb_diff` target and test identities remain. Matrix comparison and candidate
capture are owned by [`measurement.rs`](../tests/ickb_diff/measurement.rs).

```bash
cargo test --locked -p cellscript --test cli -- --test-threads=1
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
```

## Explicit candidate export

Initialize the benchmark submodule before executing its original binaries.
This ignored maintenance entry runs every measured case, retains their existing
status/resource assertions, and requires coverage of every executed baseline
row:

```bash
cargo test --locked -p cellscript --test ickb_diff measurement::export_candidate_matrix -- --ignored --exact --nocapture --test-threads=1
```

Only that entry owns the thread-local capture. Capture is cleared on completion
or unwind and cannot be enabled by environment variables. A successful export
creates a new file under a unique `target/ickb-candidates/measured-*/` directory
and prints its path. It never overwrites the checked-in matrix. The candidate
sets `equivalence_status = NOT_PROVEN` and
`production_equivalence_claim = false`; measurement does not approve a baseline
change or confer production equivalence.

Review any intentional fixture change separately, including every measured
identity, outcome and resource ceiling. Coordinated changes to benchmark data
must still follow the submodule procedure in [AGENTS.md](../AGENTS.md).

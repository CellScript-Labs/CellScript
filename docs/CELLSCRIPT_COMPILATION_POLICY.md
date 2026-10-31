# Unified compilation strategy

Status: candidate implementation; backend integration acceptance is pending.

Normal artifact compilation uses `cellscript-unified-v1`. Standalone files,
packages and release builds use the same optional transformation set.
`--release` does not select another optimization strategy. Output directories,
debug information and target selection retain independent purposes.

The strategy includes syntax-local constant folding and propagation, bounded
inlining, unused function and binding elimination, exact Cell-data and source
byte equality lowering, repeated exact-read hot-cache selection, immediate stack
reload elimination and scalar branch fusion. Required validation, overflow
checks, failure propagation and memory boundaries remain mandatory.

## Metadata inspection paths

Metadata built by the artifact pipeline describes its optimized IR. The Rust
`compile_metadata` API instead performs parsing, monomorphization, type/flow and
IR validation, then constructs metadata without the eight optional pass groups
or assembly/ELF emission. Optional-transformation parity with the artifact
pipeline is not implemented in this path. Inlining and unused-function
elimination currently leave different helper/function entries in the two
results; whole-metadata equality has not been established. Shared interface
and effect-fact claims require their own scoped cross-path checks.

The browser uses the metadata-only path and its bounded WASM summaries. That
output does not establish native ELF admission, runtime execution or complete
source equivalence. Inspect actual native artifacts separately when those
properties are required.

## Removed controls

The Rust compile/codegen options no longer have an `opt_level` field. The AST
optimizer no longer accepts a numeric level. The CLI rejects `-O`, `--opt`,
level aliases and the four-level `opt-report` command. Package build settings
reject unknown fields. Open-source admission requests no longer contain an
optimization selector and reject the retired fields.

Build plans and open/instance admission inputs advance to v2; cost evidence
and comparisons advance to v3. Old schema inputs are not accepted.
Build-plan selection, build JSON and cost-report provenance identify the
compilation strategy by name. Current evidence producers and validators do
not translate old numeric profiles. Historical release reports retain their
original measurements and are not evidence for this new strategy.

This is an intentional API and artifact change. There are no backward aliases.
Previously built ELF files, code hashes, frozen modules, build plans and cache
entries must not be treated as equivalent to rebuilt output. Rebuild and check
actual artifacts, then explicitly update any package or deployment identities
that should move. Cache generation v149 invalidates artifacts whose
source-to-memory or zero-scan byte-loop optimization could discard trapping
prefix operations; the earlier v135 correction covered source-to-source loops.
All three matchers now share non-trapping prefix admission. Keys distinguish
strategy identity and every diagnostic pass mask.

## Compiler diagnosis

`CompilerDiagnostics` is an internal Rust API, hidden from normal API docs and
absent from CLI, package and admission wire formats. Defaults enable all eight
optional pass groups. `without_optimizations()` disables them; `without(pass)`
isolates one group. These controls have no numeric level conversion and no
Serde deserializer. Each pass tests its own switch instead of inferring behavior
from a purported optimization grade.

An unoptimized diagnostic build is not a correctness oracle. Compare successful
execution, rejection behavior, independent artifact checks and resource costs.
Normal releases use the default strategy. Diagnostic output is an experiment
whose ELF identity remains distinct.

Rust host build settings such as Cargo's `opt-level = "z"` configure the compiler
binary or separate Rust contracts; they are not CellScript optimization levels
and remain intact.

## Validation

The focused strategy tests execute the standard strategy, all-pass suppression
and each single-pass suppression in CKB-VM. They compare success, failed checks
and overflow, and independently check every generated artifact. CLI regressions
reject removed controls, and request/configuration regressions reject retired
fields. Existing VM and checker regressions retain the standard/diagnostic
comparison and numeric resource ceilings.

The latest compiler library run passed 1096 tests. The five strategy tests,
including equal epoch fractions and branch rejection cases, passed. The 12
wide-value ABI cases and 12 bounded admission/mutation tests also passed.
A commit-gate run passed after the explicit-byte/resolver updates. The complete
checker integration suite passed all 212 tests against the unchanged
compiler/checker source. Its structural reference-mutation fixture explicitly
retains an otherwise unused helper under internal DeadCode diagnostics.
The source fingerprint was revalidated after completion. These checks do not
replace a complete clean-source backend receipt. All 254 CLI
regressions and strict compiler/checker clippy runs passed. The 47 frozen-interface
cases, four cost-corpus tests and seven multi-contract composition tests passed;
resource ceilings were not widened. See the
[retirement work record](CELLSCRIPT_BYTE_BUFFER_RETIREMENT.md) and
[wide-value measurement](CELLSCRIPT_WIDE_VALUE_ARGUMENTS.md) for scoped evidence.
Those historical runs did not include complete CI or TAC. Current alpha.2
acceptance is recorded in the [alpha.2 backlog](releases/CELLSCRIPT_0_33_ALPHA_2_TODO.md).
No partial result constitutes backend integration or release acceptance.

The 212-test run preceded the final browser-summary boundary refinement and
shared trusted-call inventory extraction. After that refinement, the current
native library's 1096 tests, 8 external-delegation VM/checker tests, 254 CLI
tests, 30 WASM crate tests and 63 rebuilt-browser contract tests passed.
The browser artifact passed the former 600 KiB gzip limit. The maintainer's
October 28 budget update raises the current limit to 1500 KiB (1,536,000 bytes).

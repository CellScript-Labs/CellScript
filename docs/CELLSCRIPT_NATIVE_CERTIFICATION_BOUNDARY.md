# Native certification packaging boundary

The native NovaSeal evidence implementation is an optional workspace package,
`cellscript-novaseal-certification`. Default `cellc` and the metadata-only
browser compiler do not depend on it. Build the command host with:

```bash
cargo build --locked -p cellscript --bin cellc --features novaseal-certification
```

`cellc certify --plugin novaseal-profile-v0` keeps the existing arguments and
output locations. An installation without this feature rejects with `CLI0002`
before reading a report or creating output. This also applies to `--report`:
interpreting a supplied NovaSeal report requires the same policy implementation.
Unknown plugin IDs reject before capability selection.

## Ownership and dependency graph

| Owner | Responsibility |
| --- | --- |
| [CLI host](../src/cli/certification.rs) | Arguments, report I/O, compiler diagnostics, presentation and exit code |
| [Package API](../crates/cellscript-novaseal-certification/src/lib.rs) | Explicit report-generation and summary request/result boundary |
| [Evidence engine](../crates/cellscript-novaseal-certification/src/engine.rs) | Proposal manifests, profile evidence, verifier/source pinning and path admission |
| [Summary policy](../crates/cellscript-novaseal-certification/src/summary.rs) | Local versus production status and failure reasons |
| [Producer JSON](../crates/cellscript-canonical-json/src/lib.rs) | Existing key-sorted producer representation; no compiler dependency |

The proposal package imports neither the compiler nor the independent checker.
The checker retains its independently implemented canonical representation and
shared conformance vectors. The compiler and proposal package share the producer
implementation. Gate checks inspect resolved normal dependency graphs for the
default host, browser and proposal package. Development-only conformance vectors
do not become production dependencies.

## Evidence and identity contract

The engine's evidence-status algorithms, hash domains, source-tree inventory,
verifier pinning, symlink/path restrictions and distinction between local evidence
and external attestations are retained. Moving code does not grant certification,
production equivalence, deployment authority or missing external evidence.

The implementation ID is now `cellscript-novaseal-certification`; the engine
source path is `crates/cellscript-novaseal-certification/src/engine.rs`. These
provenance fields intentionally change with the implementation. The host summary
uses `cellscript-certification-report-v0.2` and plugin kind
`optional-rust-package`. It records the engine hash and individual package/source
hashes, including summary policy and the shared producer encoder. Missing source
files remain explicitly null; a source hash is not a signed attestation. The
underlying `novaseal-production-gates-v0.4` evidence schema and hash algorithms
are unchanged. There is no old implementation, compatibility re-export or
alternative summary policy left in the CLI.

The Cargo package does not bundle a production proposal checkout. Evidence generation still needs
the requested checkout and its inputs. Release guards follow the moved engine
source while retaining NovaSeal pins, verifier builds and acceptance requirements.

## Archive resources and host feature selection

The versioned `proposals/...` paths are evidence inputs relative to the supplied
checkout root. They are not installed-package resource paths. Production report
generation keeps that explicit root and never falls back to unit-test data.

The package bundles a closed 17-file unit-input snapshot under `tests/resources/`.
Its index records exact bytes and SHA-256; package tests read those inputs or
construct temporary repositories, without walking to an assumed parent workspace.
The compiler checkout's
[`certification_resources` test](../tests/certification_resources.rs) checks the
snapshot against current proposal inputs and rejects inventory drift. That
checkout-only guard is excluded from the root compiler archive. Archive acceptance
runs the certification package's own unit tests from an extracted archive.
Unit data and successful archive tests do not certify a profile or replace live
evidence.

Native `cellscript-tools` requests the native library/CLI surface with default
LSP/VM features disabled. It does not select the browser `wasm` feature for other native
workspace members. The ordinary `--workspace --all-targets` host check is part
of dev/CI gates; browser checks and the canonical WASM build still select their
own profile explicitly. This does not claim a combined native/browser
`--all-features` profile or a browser size saving.

## Validation and measurement

The migrated package passed 48 engine/summary tests, including pinning/provenance
and symlink rejection, plus producer conformance vectors and archive compilation.
Host default/enabled checks and capability regressions cover the command boundary.

The 2026-11-01 current-source native release measurement used Rust 1.97.1 and
the tracked release profile. Both commands reused the existing release dependency
cache and overlapped native validation. Their wall times are observations, not
a controlled cold-build comparison or a historical performance improvement.

| Profile | Binary bytes | Normal dependency nodes | Observed rebuild seconds |
| --- | ---: | ---: | ---: |
| Default | 53,671,880 | 182 | 494.08 |
| `novaseal-certification` enabled | 55,283,968 | 183 | 585.60 |

The enabled binary adds 1,612,088 bytes on the same source. Its normal dependency
graph contains the NovaSeal package; the default graph excludes it. Actual release
binary regressions confirm default CLI0002 rejection before report I/O/output.
The enabled command rejects a missing report without output; an invalid-schema
report produces a failed summary and nonzero exit, with production eligibility
false. Retaining failure reports is part of its existing contract. Those cases
do not certify any profile. Source, binary, graph and command hashes are retained
in `target/structure-debt-20261101/native-profiles/`.
These native measurements precede the subsequent specialization-provenance
migration; they do not measure the final combined type migration.

`sha2` remains required by package/Registry/workspace/upgrade integrity and native
artifact consumers. The CLI was already excluded from WASM, so moving NovaSeal
alone does not establish browser savings. The preceding canonical browser
checkpoint measured 679,794 gzip bytes, an increase of 742 over its prior bundle;
it includes several compiler changes and precedes the structured emission phase.
This is not a packaging-only delta or final combined-source browser acceptance.
Native and browser measurements must be reported separately. The maintainer's
current instruction excludes running CI; earlier snapshot receipts do not
certify this change, and release requirements remain open.

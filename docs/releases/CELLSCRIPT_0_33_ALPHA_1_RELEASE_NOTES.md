# CellScript 0.33.0-alpha.1

Release date: 2026-10-28. This is the first 0.33 alpha. The maintainer authorized
publication after compiler integration testing and explicitly deferred CI,
peripheral review and stable-release acceptance on 2026-10-28. The language and
public APIs may change during the alpha series.

## Included changes

- Bounded temporary Vec, OrderedMap and OrderedSet workspaces, shared resource
  budgets, copied reads, read-only helpers and budget explanations (#47).
- Named bounded byte/record shapes, canonical codecs, non-escaping views and
  checked Cell successors (#48).
- Named heterogeneous Cell roles, complete bounded selection, correspondence,
  source/schema authority and full-transaction examples (#49).
- Compact aligned runtime stack frames, explicit return conventions, checked
  scalar error propagation, fixed-read length checks and scratch-register safety.
  Independent machine-code checks and rebound mutation regressions remain in place.

Legacy `cell_data_hash` still computes BLAKE2b over at most 512 bytes, including
BLAKE2b(empty). Consensus `.data_hash` / `cell_data_hash_field` remains a separate
contract: empty data yields zero32 and the legacy buffer limit does not apply.
Use `cell_data_bytes32` for an exact raw 32-byte slice; `cell_data_hash_at` remains
a compatibility alias. Legacy witness prefix projections do not prove complete
witness equality.

## Validation and publication scope

Compiler coverage completed on unchanged signed source: 2,445 tests
passed across all 79 integration targets plus unit/doc tests;
7 ignored tests remain unexecuted. The initial run stopped after one
test encountered a temporary-filesystem quota error. Its exact retry and every
remaining target passed with TMPDIR on the project filesystem. The original
failed receipt is retained alongside the successful continuation in
[the test receipt](../reports/bounded-runtime-v129-alpha-tests.json).

The tested implementation source is signed commit
`309eb23e5efd4276f4ac902d4979e43ca3e2ef01`. The alpha packaging commit changes
version declarations, matching local-package lock entries, documentation and
the exact-tag workflow exclusion. Its compiler/checker Rust source is unchanged.
The normal commit gate and focused version/build checks cover those packaging
changes. This distinction prevents a version-only release commit from being
misrepresented as the source of the earlier complete test run.

The remainder of backend, and the queued dev and ci gates, were cancelled under
the maintainer's instruction. No full backend/dev/ci/release pass, independent
human audit, production equivalence or deployment approval is claimed. Existing
resource ceilings and syscall/ownership/checker boundaries are unchanged. The
publication exception applies only to `v0.33.0-alpha.1`.

## Opting into the alpha compiler

Package compiler requirements follow SemVer prerelease matching. Existing stable
requirements (including `*`) do not opt into an alpha. Set an explicit requirement
in each participating `Cell.toml` before intentionally regenerating its lock:

```toml
[package]
cellscript_version = "=0.33.0-alpha.1"
```

Use `cellc lock` for that intentional repin. Frozen build/check/test do not rewrite
locks or silently select a prerelease. Rebuild artifact and checker sidecars
together with the matching alpha compiler and checker.

## Retained boundaries

Temporary arenas are monotonic; retired buffers remain charged until the owner
scope ends. Member `items` ceilings do not guarantee all maxima fit a shared
budget. Exported declaration schemas, mutable/member-level loans, escaping
nested values and general heap reuse remain outside the initial profile.

The stack audit retains justified ABI reservations and conservative ordinary
function scratch. It does not claim globally minimal stack usage. Legacy xUDT
and WitnessArgs length repairs have VM regressions without newly added complete
independent machine profiles. Static checking and finite tests do not establish
arbitrary program equivalence.

This publication does not rebuild or publish the website, browser bundle, VS Code
extension, Registry services or deployed contracts. Their existing distributions
and evidence keep their own versions. See the admission, stack-layout and
legacy-read contract documents for the precise supported boundary.

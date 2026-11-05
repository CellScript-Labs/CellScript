# CellScript 0.33.0-alpha.2

Release date: 2026-11-05. This is the second 0.33 alpha. The maintainer authorized
publication after the v193 acceptance closeout and the v194 schema-vector
correction on 2026-11-05. The language and public APIs may change during the
alpha series.

## Included changes

- Structured type identity migration checkpoint (#96): the parser retains
  `Type::Applied` applications and unsigned-constant arguments, AST and IR share
  one immutable specialization-origin index, generated monomorph labels are no
  longer decoded from strings, and compatible-open interface roles are validated
  in their original source scope before generic expansion.
- Structured effect facts replace the five-category model: context reads, Cell
  lifecycle operations and process operations are inferred by the compiler and
  independently reconstructed by the checker; manual declarations, the native
  `effects` wrapper and every category reader are removed. Public interfaces
  require schema v5.
- Legacy byte buffers retire in favor of streaming hashing with exact witness
  resolution; the retired runtime identities are denied at the frontend, codegen,
  checker and typed-call layers.
- Independent machines reject foreign branches into internal PIPE success paths,
  bind every SPAWN caller to its own status guard and failure code, and audit
  every declared return ABI class against decoded instructions. Deferred
  maturity/time/epoch constraints, occupied-capacity floors and the former bare
  spawn fail closed; bare named spawn is removed from the executable surface.
- The checker's single-image ELF admission, RA-slot pairing and call-depth
  analysis harden artifact boundaries. `checker.rs` is modularized into record,
  ELF, typed-semantic, policy, Script-hash and committed-state modules with an
  unchanged admission order. Canonicalization for compiler producers moves to
  `cellscript-canonical-json`; the checker keeps its independent implementation
  with shared conformance vectors. NovaSeal certification moves behind the
  non-default `novaseal-certification` feature.
- Registry operations serialize namespace claim admission on the database,
  unify the cooldown clock and boundary, require claim-before-publish on both
  stores, and purge rejected snapshots tombstone-first. IPC reads return a
  complete u64 or abort.
- LSP analysis runs on revision-bound snapshots outside the session lock with a
  shared view index. Unterminated strings, byte strings and block comments
  produce stable source diagnostics instead of silent acceptance; pool lowering
  and unrepresentable fixed layouts return diagnostics instead of panics.
- The v194 correction restores byte-length storage for schema-backed structural
  `Vec<T>` fields with fixed-width elements through the retained span ABI,
  superseding the v193 codec-retirement diagnosis of the stateful replay
  failure, and corrects the Edition 2027 single-entry claim: that restriction
  belongs to `cellc migrate --to 2027`, not to ordinary authoring.

## Validation and publication scope

The v193 snapshot (commit `c50266ad`, source manifest and receipts retained
under `target/ordered-followup-20261104/`, untracked) passed the commit and dev
gates, every backend step before the node-stateful replay, and the canonical
consumer evidence: the browser WASM build (705 KiB gzip within the 1500 KiB
budget, SHA-256 recorded in the tracked website compiler-artifact data), 86
distributed compiler tests, both Registry verifiers (7 + 5 tests), the
Registry API suite against real PostgreSQL (184 passed, 8 environment skips)
and the NovaSeal certification crate (48 tests, workspace and detached).

The stateful replay on that snapshot failed at the timelock
emergency-approval path. The initial codec-retirement diagnosis recorded in
`v193-final-stateful-analysis.json` is superseded by the v194 field-span
repair; nineteen focused regressions now cover the repaired behavior,
including the recorded timelock Cell/witness bytes in both editions and
compiler profiles. A fresh full backend stateful pass is not claimed by this
publication, and gate scripts were intentionally not rerun for the v194 fix
under the maintainer's 2026-11-05 instruction.

The publication packaging commit changes version declarations, matching
lockfile entries, documentation and submodule pointers only. Its compiler and
checker Rust source is unchanged from `719c9d11`, which passed the commit gate
with focused regressions. This distinction prevents a version-only release
commit from being misrepresented as the source of the earlier acceptance runs.

The alpha.1 publication exception is not extended by this release; the
remaining backend stateful pass, `#96` completion and the full CI/release
gates stay open as recorded in the
[alpha.2 backlog](CELLSCRIPT_0_33_ALPHA_2_TODO.md).

## Opting into the alpha compiler

Package compiler requirements follow SemVer prerelease matching. Existing
stable requirements (including `*`) do not opt into an alpha. Existing
`>=0.33.0-alpha.1` requirements already admit this prerelease; to pin it
exactly, set an explicit requirement in each participating `Cell.toml` before
intentionally regenerating its lock:

```toml
[package]
cellscript_version = "=0.33.0-alpha.2"
```

Use `cellc lock` for that intentional repin. Frozen build/check/test do not
rewrite locks or silently select a prerelease. Rebuild artifact and checker
sidecars together with the matching alpha compiler and checker.

## Retained boundaries

`ExactCellSelection` remains a reserved source application; generic nominal
identity, default-build alias evidence and the Cell/selection family
representation stay open under #96. Legacy dynamic-element vectors keep their
separate fail-closed boundary. Unbounded `Vec<T>` receipt fields with
non-fixed elements remain outside the admitted codec. Deferred runtime
semantics abort with their recorded error codes instead of returning success.
Static checking and finite tests do not establish arbitrary program
equivalence; the bounded admission ceilings and ownership rules are unchanged.

This publication does not rebuild or publish the website, browser bundle,
VS Code extension, Registry services or deployed contracts. Their tracked
sources and evidence carry the versions recorded in their own manifests.

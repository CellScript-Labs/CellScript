# CellScript Dependency, Package, and Build Cache Audit

**Scope:** `Cargo.toml`, `Cargo.lock`, `src/package/mod.rs`, `src/incremental/mod.rs`, `src/cli/commands.rs`, `src/bin/`, `tools/`
**Date:** 2026-05-22
**Review status:** Corrected after local validation and hardening.

---

## 1. Dependency Version Pinning

**Finding: exact pins are present for selected stability-sensitive dependencies.**

`Cargo.toml` pins:

```toml
indexmap = "=2.2.6"
clap = { version = "=4.5.49", features = ["derive"] }
```

`Cargo.lock` records the resolved versions and checksums. This prevents patch-level upstream changes to deterministic map behavior or CLI parsing from entering the compiler without an explicit dependency update.

**Verdict:** Pass.

---

## 2. Package Fetching, Hash Verification, and Downgrade Resistance

**Finding before hardening: registry dependencies were unsupported, path dependencies were constrained, but git dependencies trusted branch/tag/default-branch resolution.**

Registry dependencies remain fail-closed: `resolve_from_registry()` rejects them and asks users to use local path dependencies.

Local path dependencies are canonicalized and must stay under the package root. This prevents package-path traversal for the supported package source that participates in normal compilation.

Git dependencies now require an immutable commit pin:

- branch refs are rejected;
- tag refs are rejected;
- default-branch git dependencies are rejected;
- `rev` must be a full 40-character SHA-1 or 64-character SHA-256 commit hash;
- `cellc add --git` and `cellc install --git` require `--rev`.

The lockfile still records the resolved revision and consistency checks require an exact revision match with `Cell.toml`. The security boundary is the explicit commit pin in `Cell.toml`, not remote branch or tag state. The implementation does not verify commit signatures or an out-of-band source archive hash.

**Verdict:** Pass for branch/tag downgrade resistance after hardening. Signature/provenance verification remains a future feature.

---

## 3. Incremental Build Cache Key

**Finding before hardening: the incremental cache key tracked only `opt_level`, `target`, and `debug`.**

The current compiler did not return cached artifacts from `.cell/build/cache`; it always recompiles and only records cache metadata. Therefore the missing fields were not a current stale-artifact vulnerability.

The key is now future-hardened and includes:

- `target_profile`;
- `primitive_compat`;
- CKB limit environment variables:
  - `CELLSCRIPT_CKB_MAX_TX_VERIFY_CYCLES`
  - `CELLSCRIPT_CKB_MAX_BLOCK_CYCLES`
  - `CELLSCRIPT_CKB_MAX_BLOCK_BYTES`
- external RISC-V toolchain environment variables:
  - `CELLSCRIPT_RISCV_CC`
  - `CELLSCRIPT_RISCV_AS`
  - `CELLSCRIPT_RISCV_LD`

This preserves correctness if the cache later starts returning serialized compile results.

**Verdict:** Low current risk, fixed as future hardening.

---

## 4. `cellc add`, `install`, and `update`

**Finding before hardening: package commands did not authenticate dependencies.**

`cellc add` still only records a dependency in `Cell.toml`; it does not fetch or resolve dependencies. For git dependencies, it now requires `--rev` so the recorded dependency is pinned at creation time.

`cellc install --path` resolves the local path and refreshes `Cell.lock`.

`cellc install --git URL --rev COMMIT` resolves a git dependency at the pinned commit and writes both `Cell.toml` and `Cell.lock`.

`cellc update` re-resolves dependencies and rewrites `Cell.lock`, but git dependencies are now required to be commit-pinned before resolution. It still does not verify commit signatures.

**Verdict:** Pass for mandatory git commit pins. Signature validation remains out of scope.

---

## 5. Excluded Directories

**Finding: no current secret leakage was found in excluded helper directories.**

`Cargo.toml` excludes `src/bin/` and `tools/` from published crates. The inspected files are a CKB transaction measurement helper and its local tool manifest/documentation. They do not contain private keys, mnemonics, credentials, or test secrets.

The exclusion is still appropriate because these helpers are release-evidence tooling, not part of the published `cellscript` crate.

**Verdict:** Pass.

---

## Summary

| Question | Verdict | Severity |
| --- | --- | --- |
| Exact version pinning | `indexmap` and `clap` are exactly pinned. | Good practice |
| Registry package fetching | Unsupported and fail-closed. | Low |
| Git downgrade protection | Fixed by requiring full commit `rev` pins and rejecting branch/tag/default branch refs. | Medium reduced to Low |
| Git signatures/provenance | Not implemented. | Medium future hardening |
| Incremental cache env poisoning | Not currently exploitable because cached artifacts are not returned; key now includes profile, primitive mode, and relevant env vars. | Low |
| `cellc add/install/update` provenance | Git dependencies must now be commit-pinned; signatures are still not verified. | Low/Medium |
| Excluded helper directories | No current secret leakage; exclusion remains correct. | Low |

No critical issues remain for the current package and cache model.

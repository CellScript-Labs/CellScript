# Constrained Script instance acceptance

Acceptance status: bounded technical acceptance passed on 2026-10-18
(Asia/Kuala_Lumpur). The complete backend replay, strict audit and clean-source
CKB receipt passed. This record supports issue #80; release, public deployment
and independent human audit remain separate.

The implementation is `dd58ff19db94d9a7003824c784d27f6fa7d81f84`.
Signed runtime/test snapshot `f55508b572aa56c9c7298766bda28f089d57fe14` additionally
checks actual Lock/Type fields through global and group-relative input/output
views at O0–O3. The normative profile is in
[Constrained Script instances](../../../CELLSCRIPT_INSTANCE_POLICY.md).

The new 1768-byte selection and 208-byte consumer-args layout authenticate bounded
fixed-layout predicates and the selected concrete Script. The existing 202-byte
exact and 656-byte open formats retain their semantics. This profile does not
add SSRI, IPC, verifier argv or arbitrary predicates.

Evidence ownership is Codex under the maintainer's implementation request.
The issue-specific author self-review is recorded in the normative profile.
Independent human audit was not performed; no earlier waiver is inherited.

## Gate provenance

- `dev` passed in the original worktree on the same compiler/checker and
  application implementation, before the added view regression and final prose.
- `ci` passed in the isolated worktree. Its original root test binary predates
  the test-only `f55508b5` addition; that new regression passed separately, and
  the later CI lint/package steps see the added source. Production compiler,
  checker and application code did not change during this delta.
- Initial `backend` failed when the full test suite launched temporary
  executable fixtures concurrently (`ETXTBSY`). The failed log is retained.
  The successful full replay uses `RUST_TEST_THREADS=1` on clean signed snapshot `f55508b5`.
  The accompanying permanent gate change makes this existing CI/preflight
  serialization explicit in backend mode too; it changes no compiler behavior.
- The next backend attempt reached stateful acceptance but rejected the shared
  CKB checkout after it advanced to `d40e4722`. The successful final replay explicitly
  binds `CELLSCRIPT_CKB_REPO` to a clean checkout of
  `f7fa4436737756f97a24e254f22c13a36316ecea`, the repository pin. The original
  user CKB checkout remains unchanged. Both failed attempts are retained.
- Ordinary commits use the `commit` gate and the focused regressions. The final
  acceptance report is documentation/evidence plus the serialization fix.

## Retained evidence

The final archive contains identified compiler/checker sources, application
source and ELF/metadata/lowering/source-map bundles, exact CellDeps, policy
bytes, complete transactions/input snapshots, and 88 resource traces.
The source commit and every retained file's hash are recorded in its inventory.
The two-pool transactions are replayed against a fresh VM context from retained
bytes; no recompilation or resigning occurs during that replay.

Four optimization levels each cover 16 document cases and 19 pool cases. The
pool cases include separately issued assets, authentic Type-ID genesis, custody,
independent and concurrent swaps, instance substitution and otherwise-valid
signed rejecting peers. They establish this bounded #80 application slice;
#52/#61 financial accounting, LP operations and public deployment stay separate.

Resource measurements are named fixture observations, not a universal worst-case
proof. Full combined-bound cases permit 32 implementation members, 64 direct
code dependencies, 256 args bytes, 128 configuration bytes and 16 constraints.
The observed maxima are 271812 group cycles, 9808 stack bytes, 32768 touched
guest-page bytes, 16376 ELF bytes and 4686 isolated transaction bytes. The
resource report preserves each valid/late-invalid case and optimization level.

## Evidence navigation and reproduction

- `acceptance.json` identifies the implementation, executable acceptance commit,
  GPG verification result, gate provenance and archive/summary SHA-256 hashes.
- `applications-summary.json` indexes all eight application reports. Each full
  report and its complete source/artifact/dependency/transaction material is
  retained in the archive.
- `resources-summary.json` indexes all 88 measured cases. It omits instruction
  traces for readability; the complete resource report remains in the archive.
- `acceptance-evidence.tar.gz` expands to `instance-policy-evidence/`. Its
  `inventory.json` records source and evidence hashes; `gates/` retains the
  successful logs, earlier failed backend logs, full strict-audit report and
  clean-source CKB acceptance receipt. Paths prefixed by `target/` in the
  inventory identify the original collection root. Extracting under the
  repository's `target/` restores that layout.
- `SHA256SUMS` covers the public review files and archive. Source changes since
  the recorded base are also retained as `source-snapshot/working-tree.patch`;
  unchanged sources are identified by the signed Git commit and inventory.

For a fresh full replay, use signed CellScript source
`f55508b572aa56c9c7298766bda28f089d57fe14` and its pinned submodules, a clean
CKB checkout at `f7fa4436737756f97a24e254f22c13a36316ecea`, and the SDK at
`v5.1.0`. Follow the normal backend prerequisites in the gate policy. The
recorded backend command sets `RUST_TEST_THREADS=1` and
`CELLSCRIPT_CKB_REPO` to that CKB checkout, then runs
`./scripts/cellscript_gate.sh backend`. CKB is rebuilt from source by the
acceptance harness. A fresh replay has its own receipt and source-path-bound
identities; do not relabel this retained receipt with a later commit.

The CKB node receipts cover the existing stateful fixtures and private counter.
They do not promote the separate document/two-pool local VM observations to
public-chain deployment evidence.

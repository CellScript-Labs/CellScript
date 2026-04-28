# iCKB CellScript Completeness Benchmark: Discovery

Date: 2026-04-28

## Repositories

- CellScript: current repository, `/Users/arthur/RustroverProjects/CellScript`.
- iCKB proposal: cloned to `/tmp/cellscript-ickb-audit/proposal`.
  - Commit inspected: `055f0cb2c44b2988531c241a6f7167397bbe42c7`.
- iCKB v1-core: cloned to `/tmp/cellscript-ickb-audit/v1-core`.
  - Commit inspected: `f7bbf7fe691d449a68a4b973d3102b7af28b2c9b`.

No iCKB repository was vendored into CellScript source control.

## CellScript Architecture Map

- Parser and AST:
  - `src/parser/mod.rs`
  - `src/ast/mod.rs`
- Semantic analysis and name/type handling:
  - `src/types/mod.rs`
  - `src/resolve/mod.rs`
  - `src/lifecycle/mod.rs`
- IR and compile pipeline:
  - `src/lib.rs`
  - `src/ir/mod.rs`
  - `src/optimize/mod.rs`
- CKB backend/codegen/runtime metadata:
  - `src/codegen/mod.rs`
  - `src/stdlib/mod.rs`
  - `src/stdlib/ckb_protocols/*`
  - `src/assumptions.rs`
  - `src/runtime_errors.rs`
- CLI and user-facing commands:
  - `src/main.rs`
  - `src/cli/commands.rs`
- LSP/editor:
  - `src/lsp/mod.rs`
  - `src/lsp/server.rs`
- Formatting:
  - `src/fmt/mod.rs`
- Existing examples:
  - `examples/*.cell`
  - `examples/acceptance/*.cell`
  - `examples/language/*.cell`
- Existing tests:
  - Rust integration tests under `tests/*.rs`
  - CKB compatibility fixtures under `tests/compat/ckb_standard/*.json`
  - Existing CI runs `cargo test --locked --manifest-path Cargo.toml -- --test-threads=1`.

## iCKB Architecture Map

- Proposal:
  - `proposal/README.md`
  - `proposal/2024_overview.md`
- Script workspace:
  - `v1-core/scripts/Cargo.toml`
  - `v1-core/scripts/README.md`
  - `v1-core/scripts/capsule.toml`
- iCKB Logic script:
  - `scripts/contracts/ickb_logic/src/entry.rs`
  - `scripts/contracts/ickb_logic/src/celltype.rs`
  - `scripts/contracts/ickb_logic/src/constants.rs`
  - `scripts/contracts/ickb_logic/src/utils.rs`
  - `scripts/contracts/ickb_logic/src/error.rs`
- Limit Order script:
  - `scripts/contracts/limit_order/src/entry.rs`
  - `scripts/contracts/limit_order/src/error.rs`
- Owned-Owner script:
  - `scripts/contracts/owned_owner/src/entry.rs`
  - `scripts/contracts/owned_owner/src/error.rs`
- Shared utilities and schemas:
  - `scripts/contracts/utils/src/utils.rs`
  - `scripts/contracts/utils/src/dao.rs`
  - `scripts/contracts/utils/src/constants.rs`
  - `scripts/contracts/utils/src/c256.rs`
  - `schemas/encoding.mol`
- iCKB test harness:
  - `scripts/tests/src/lib.rs`
  - `scripts/tests/src/tests.rs`

## Commands Run

CellScript discovery:

```bash
ast-outline digest src
git status --short --branch
find .github -maxdepth 3 -type f -print
sed -n '1,220p' .github/workflows/ci.yml
```

iCKB checkout:

```bash
rm -rf /tmp/cellscript-ickb-audit
mkdir -p /tmp/cellscript-ickb-audit
git clone --depth 1 https://github.com/ickb/proposal.git /tmp/cellscript-ickb-audit/proposal
git clone --depth 1 https://github.com/ickb/v1-core.git /tmp/cellscript-ickb-audit/v1-core
git -C /tmp/cellscript-ickb-audit/proposal rev-parse HEAD
git -C /tmp/cellscript-ickb-audit/v1-core rev-parse HEAD
```

iCKB source inspection:

```bash
ast-outline digest scripts/contracts scripts/tests/src
ast-outline scripts/contracts/ickb_logic/src/entry.rs
ast-outline scripts/contracts/limit_order/src/entry.rs
ast-outline scripts/contracts/owned_owner/src/entry.rs
ast-outline show scripts/contracts/ickb_logic/src/entry.rs main
ast-outline show scripts/contracts/ickb_logic/src/entry.rs check_input
ast-outline show scripts/contracts/ickb_logic/src/entry.rs deposit_to_ickb
ast-outline show scripts/contracts/ickb_logic/src/entry.rs check_output
ast-outline show scripts/contracts/limit_order/src/entry.rs main
ast-outline show scripts/contracts/limit_order/src/entry.rs validate
ast-outline show scripts/contracts/limit_order/src/entry.rs extract_order
ast-outline show scripts/contracts/owned_owner/src/entry.rs main
ast-outline show scripts/contracts/owned_owner/src/entry.rs extract_owned_metapoint
```

Baseline tests before benchmark changes:

```bash
cargo test --locked -p cellscript
```

Result: passed. The run executed 529 Rust tests across library and integration
targets plus doc-tests with no failures.

iCKB tests:

```bash
cd /tmp/cellscript-ickb-audit/v1-core/scripts
cargo test --locked
```

Result: Rust crates compiled, but the `tests` crate failed both tests because
the loader expected contract binaries under `scripts/build/debug` and they were
not present. `scripts/tests/src/lib.rs:53-67` constructs that path. The iCKB
README requires Capsule/Cross release builds first:

```bash
cargo install cross --git https://github.com/cross-rs/cross --rev=6982b6c --locked
cargo install ckb-capsule --git https://github.com/nervosnetwork/capsule --rev=04fd58c --locked
capsule build --release
```

`docker` was installed locally, but `capsule` and `cross` were not available.
I did not install global tooling or run Docker builds as part of this benchmark.

Benchmark tests after implementation:

```bash
cargo test --locked -p cellscript --test ickb_benchmark
```

Result: passed, 4 tests.

## Assumptions And Gaps

- iCKB source references are to the shallow clone commit listed above.
- The CellScript benchmark is a partial semantic model. It is not a byte-for-byte
  or VM-behaviour-equivalent port.
- Current CellScript can compile the benchmark specs to RISC-V assembly, but the
  benchmark fixtures are model-level checks rather than generated CKB VM
  executions.
- iCKB's own reusable CKB test harness could not be run to completion without
  a prior Capsule build producing `build/debug` or `build/release` binaries.

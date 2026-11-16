# Contributing to CellScript

Thanks for your interest in contributing. CellScript is a compiler/tooling project for CKB Cell-model contracts with strict correctness, evidence, and release-boundary expectations. This guide summarizes the **repository’s actual workflow and contracts**—when in doubt, follow the tracked docs over this summary.

## Before you start

- **Read first (required for non-trivial changes):**
  - `AGENTS.md` — repo overview, build/test/gate commands, workspace/layout, backend and CKB/NovaSeal gotchas.
  - `CODING_STYLE.md` — compiler/backend/docs/release contract (phase boundaries, assembler/stack rules, verified-artifact boundaries, tests, refactor procedure).
  - `docs/CELLSCRIPT_GATE_POLICY.md` — what each gate mode proves and which checks are release/evidence boundaries.
  - `BRANCHES.md` and `CHANGELOG.md` — active/release/research lines and current scope (do not misstate evidence levels, especially for `research/protocol-equivalence`).
- **Do not treat the README as the sole source of truth** for current implementation boundaries; verify against source, tests, config, and policy docs.
- **CKB-facing work:** prefer official CKB docs/LLM resources and verify version-sensitive behavior (node/VM/RPC/syscalls/Molecule/network/SDK) before coding or making claims (see `AGENTS.md` “Official CKB AI resources”).

## Prerequisites

- **Rust toolchain (pinned):** `1.97.1` with components/targets as in `rust-toolchain.toml` (`clippy`, `rustfmt`, `llvm-tools-preview`; targets include `riscv64imac-unknown-none-elf`, `wasm32-unknown-unknown`).
- **MSRV/rust-version:** `1.97.1` (workspace manifests). Do not bump toolchain/versions without coordinating with the release/gate contract.
- **Node.js 22:** required by `ci`/website/Registry toolchain paths enforced by gates (`docs/CELLSCRIPT_GATE_POLICY.md`).
- **Submodules (as needed):** `editors/vscode-cellscript`, `website`, `tests/benchmarks`, `proposals/novaseal`, `proposals/evolving-dob/evolving-dob-profile-v1` may be uninitialized in a fresh checkout; initialize only what your task requires (`git submodule update --init <path>`). Coordinate before pushing submodule remotes/pointer changes per `AGENTS.md`.
- **Heavy/external dependencies (mode-specific):** `release`/`release-quick` and some acceptance flows require pinned sibling/external checkouts and tools (e.g. pinned CKB checkout per `scripts/ckb_acceptance_pin.json`, sibling `ckb-sdk-rust` expectations, NovaSeal/tooling, Docker, etc.). Do not run them casually; see `AGENTS.md` and `docs/CELLSCRIPT_GATE_POLICY.md`.

## Getting set up

```bash
git clone https://github.com/CellScript-Labs/CellScript.git
cd CellScript
# Optional, task-dependent:
# git submodule update --init editors/vscode-cellscript website tests/benchmarks proposals/novaseal

cargo build --locked -p cellscript --bin cellc
Use --locked for Cargo operations in line with project practice.
Build, test, and validate
The unified gate is the authoritative readiness check:
| Goal | Command | Notes |
| --- | --- | --- |
| Local pre-commit check | `./scripts/cellscript_gate.sh dev` | Fast contract formatting/checks, key package/crate checks, audits, policy/source checks, etc. |
| Merge/CI readiness | `./scripts/cellscript_gate.sh ci` | Broader tests/clippy/scenario/website/registry/tooling checks; needs Node 22. |
| IR/codegen/assembler/ABI/ELF/RISC-V work | `./scripts/cellscript_gate.sh backend` | Required for backend/compiler-contract changes. |
| Release evidence (only when appropriate) | `./scripts/cellscript_gate.sh release` / `release-quick` | Heavy; `release-quick` is not external live/devnet evidence. |

Focused commands are useful while iterating but do not replace the matching gate:
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
# clippy with -D warnings as applicable
git diff --check
Direct compile smoke test (example): cellc examples/token.cell or build an artifact as shown in README.md.
Development guidelines
- Scope changes to the relevant compiler phase (lexer/parser/AST, resolve/types/flow/proof, IR/optimize, codegen, package, tooling, docs). Prefer existing structured representations over parallel/stringly-typed paths.
- A feature is not “done” at parse time. New syntax/behavior must agree across parsing, formatting, type checking, lowering, metadata, LSP/editor behavior, examples, docs, and tests where applicable (CODING_STYLE.md).
- Fail closed for unsupported/ambiguous/runtime-unsafe forms; do not emit clean success paths for unsupported semantics. Use precise CKB terminology and evidence boundaries.
- Backend/codegen (high risk): follow CODING_STYLE.md and AGENTS.md exactly—assembler mnemonic tiers and closure rules, stack access via approved helpers, explicit register liveness/avoid sets, no domain-rule matching by action/function name in codegen, and the refactor procedure for src/codegen/mod.rs (exact source movement, full tests, careful deletions/visibility).
- Verified artifact boundary: treat ELF, compile metadata, lowering record, and source map as one bundle; keep the checker independent of frontend/codegen; new invariants need deterministic negative mutations and valid fixtures as appropriate.
- Package/lock: Cell.lock is authoritative, reviewed build input; builds/tests must not perform mutable version selection as described in AGENTS.md—use cellc lock/cellc update for intentional repins.
- Docs/release notes: state only what is implemented and evidenced; use “reserved/deferred/fail-closed” accurately; separate evidence levels (compile-only vs runtime vs chain/deployment/production claims).
- Do not invent commands or claims not supported by Cargo.toml, scripts, gates, or tracked docs.
Tests and evidence
- Add/update tests appropriate to the change (parser/formatter/type/lowering/metadata/LSP, negative CKB/safety cases, assembler/codegen instruction-surface and compile-through paths, scenario/gate fixtures as required).
- Integration tests live under tests/; package scenarios use versioned tests/scenarios/*.scenario.json with cellc test --backend simulator|ckb-vm|all semantics (AGENTS.md).
- Do not claim a test/gate passes unless you executed it and observed the result. Do not run destructive or externally state-changing tests without clear intent and authority.
- Large suites/files exist (e.g. tests/cli.rs, tests/ickb_diff.rs, src/cli/commands.rs, src/codegen/mod.rs, src/lib.rs); scope edits/reads and use focused commands first.
Code style and hygiene
- Run cargo fmt (workspace/tooling as appropriate) and keep formatting consistent; trailing whitespace in tracked files is a gate violation in many areas—check with git diff --check.
- Prefer narrow, justified lint allowances; keep new APIs/structures typed and explicit.
- .cell example naming and organization follow CODING_STYLE.md (semantic names, no release/version markers in filenames).
- *.local.md notes are outside the project contract; do not add them to gates or treat them as committed policy.
Branches, commits, and pull requests
- Use BRANCHES.md to understand which branch/tag/line carries which evidence and release meaning; compare stable behavior against exact release tags where relevant.
- Keep commits focused and messages clear about the phase/contract being changed.
- Before opening/updating a PR, run at least the gate matching your change (dev/ci/backend) and report what you actually ran. If you could not run a required gate (environment/tooling limits), state that explicitly.
- If your change affects generated/checked artifacts (e.g. website registry data, verified bundles, reproducible outputs), follow the corresponding gate/script expectations so CI stays clean.
Security, correctness, and CKB-specific caution
- Validate inputs and trust boundaries; do not weaken fail-closed checks to “make things pass.”
- Treat witness data, Script args/identities, capacities, hashes, and deployment/chain evidence with the precision required by CKB and the project’s ABI/profile contracts.
- Distinguish confirmed issues from potential risks; tie production/on-chain claims to the required evidence (acceptance, cycles, tx size/capacity, lock/action matrices, etc.), not compile-only results.
Where to ask / propose larger changes
- For substantial semantic, ABI, metadata-schema, package/lock, or backend-contract changes, prefer a concrete proposal/RFC/design discussion with explicit ownership, compatibility, and acceptance criteria (see CODING_STYLE.md / release-note guidance).
- Check existing docs in docs/, docs/wiki/, and issue/discussion history in the project’s GitHub tracker for prior decisions.
License
By contributing, you agree that your contributions will be licensed under the repository’s license (see LICENSE-MIT and Cargo.toml).

**Notes if you add this file:**
- Keep it consistent with `AGENTS.md`/`CODING_STYLE.md` (they are the stronger contracts) and update it when gate commands, toolchain, or policies change.
- Consider whether your project wants a PR template, issue templates, DCO/CLA, or code-of-conduct sections—those are **not** specified in the files I inspected, so I didn’t assert them as current policy.
# CellScript

<p align="center">
  <img src="assets/cellscript-logo.png" alt="CellScript" width="560">
</p>

[![CellScript CI](https://github.com/tsukifune-kosei/CellScript/actions/workflows/ci.yml/badge.svg)](https://github.com/tsukifune-kosei/CellScript/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](Cargo.toml)
[![Targets: CKB](https://img.shields.io/badge/targets-CKB-2f6f4e.svg)](#target-profiles)
[![Package Workflow: Local First](https://img.shields.io/badge/package%20workflow-local%20first-2f6f4e.svg)](#package-workflow)
[![LSP: Local Tooling](https://img.shields.io/badge/LSP-local%20tooling-2f6f4e.svg)](#editor-support)
[![Wiki Tutorials](https://img.shields.io/badge/wiki-tutorials-6f42c1.svg)](https://github.com/tsukifune-kosei/CellScript/wiki)

[English](README.md) | [Chinese](README_CH.md)

**Write Cell contracts the way you think about them — not the way the wire format does.**

CellScript is a domain-specific language for Cell-based smart contracts on
CKB. It compiles `.cell` source into ckb-vm RISC-V assembly or ELF
artifacts, together with typed metadata for auditing, policy checks, schema
binding, and scheduler-aware execution.

In this README, metadata means machine-readable semantic facts emitted by the
compiler: schema layout, Cell effects, access summaries, source hashes,
verifier obligations, runtime requirements, and target-profile policy flags.

The language is intentionally narrow: it is not a new VM, and it is not an
account-storage contract language. CellScript gives protocol authors a typed
way to describe assets, shared Cell state, receipts, explicit state transitions,
locks, and transaction-shaped effects — while still mapping directly to the
Cell model used by CKB.

---

## Why CellScript

CKB exposes powerful Cell-oriented execution, but hand-written
scripts force authors to work close to the wire format:

- parse witness bytes manually
- track inputs, CellDeps, outputs, and output data by index
- encode typed state into raw byte arrays
- write RISC-V C or assembly against syscall numbers
- preserve linear asset semantics by convention rather than by the compiler

CellScript raises that programming model to explicit language constructs:
`resource`, `shared`, `receipt`, `action`, `lock`, source qualifiers such as
`read`, `protected`, `witness`, and `lock_args`, and Cell effects such as
`consume`, `create`, and `destroy`. Higher-level lifecycle patterns such as
`std::lifecycle::transfer`, `std::receipt::claim`, and
`std::lifecycle::settle` expand into those explicit effects instead of living
as compiler-core verbs.

## Current Status

CellScript is currently in a CKB-focused alpha / stabilisation phase.

It is suitable for:
- experimenting with CKB Cell-contract authoring;
- compiling and inspecting the bundled examples;
- exploring typed Cell effects, metadata, constraints, and CKB target-profile
  checks;
- trying the local VS Code extension and LSP tooling.

It is not yet recommended for unaudited mainnet deployment without manual
review. The current focus is developer-readiness, diagnostics, ProofPlan /
metadata visibility, and CKB target-profile stability.

## Quick Start

Install from this repository:

```bash
cd cellscript
cargo install --path .
```

Compile your first contract:

```bash
# Just type-check
cellc examples/token.cell

# Emit a RISC-V ELF for CKB
cellc examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15

# Emit a RISC-V ELF for CKB, with a specific entry action
cellc examples/nft.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15 --entry-action transfer
```

Start a package:

```bash
cellc init token-package
cd token-package
cellc add shared-types --path ../shared-types
cellc build --target riscv64-elf --target-profile ckb
```

Run a CKB profile check:

```bash
cellc check --target-profile ckb
```

Inspect what the compiler can explain about the token example:

```bash
cellc metadata examples/token.cell --target-profile ckb --json
cellc constraints examples/token.cell --target-profile ckb
cellc scheduler-plan examples/token.cell --target-profile ckb
```

These commands show what the compiler believes the protocol reads, writes,
creates, consumes, assumes, and exposes to CKB-facing policy tooling.

> **Next:** Read on for the [language model](#core-model), [full examples](#example),
> or dive into the [architecture](#architecture).

---

## Target Profiles

CellScript now supports CKB as its only target profile:

| Profile | When to use | What you get |
|---|---|---|
| `ckb` | CKB mainnet artifacts | BLAKE2b/Molecule conventions, CKB syscall profile |

> The `ckb` profile is production-gated for the bundled CellScript suite. It
> emits raw CKB ckb-vm artifacts, uses CKB syscall
> and Molecule/BLAKE2b conventions, and rejects unsupported shapes through
> normal target-profile policy.

```bash
cellc examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15
cellc check --target-profile ckb
```

## Core Model

CellScript programs are written as verifier constraints over proposed Cell
transformations:

| Concept | What it compiles to |
|---|---|
| `resource T { ... }` | A linear Cell-backed asset (`CellOutput` + `outputs_data[i]`) |
| `shared T { ... }` | Shared state Cell, read via `CellDep` or updated by consume + create |
| `receipt T { ... }` | A single-use proof Cell (deposits, vesting, votes, liquidity) |
| `consume value` | Spend a transaction input |
| `create output = T { ... }` | Constrain a named proposed output Cell with typed data |
| `read param: T` / `read_ref<T>()` | Load a read-only CellDep-backed value |
| `action` | Type-script transition logic → compiled to RISC-V |
| `lock` | Lock-script authorization logic → compiled to RISC-V |
| Local `let` values | Transaction-local computation; never persistent storage |

> **Key rule:** only `create` materializes persistent state. Ordinary local
> values do not become Cells unless explicitly created as `resource`,
> `shared`, or `receipt`.

## Language Features

- **Cell-native resources** — `resource` values are linear. They cannot be
  copied, silently dropped, or hidden inside ordinary values. Every resource
  must reach an explicit lifecycle or output-binding role: for example
  `consume`, `destroy`, a declared successor output, or a compiler-recognized
  stdlib lifecycle pattern that expands to `consume` plus output constraints.
- **Explicit shared state** — `shared` marks contention-sensitive protocol
  state (pools, registries, configuration Cells). Reads and writes stay
  visible to metadata and tooling.
- **Receipts as stateful proofs** — `receipt` is a single-use Cell that proves
  an operation happened and can later be consumed directly or through an explicit
  stdlib claim/settlement pattern.
- **Capability gates** — `has store, create, consume, replace, burn, relock`
  makes asset permissions explicit in kernel-effect terms instead of protocol
  verbs.
- **Declarative flows** — state remains explicit schema data, while
  `flow Name for Type.field { A -> B by action; }` or compact
  `flow Type.field { A -> B; }` declares allowed edges. The canonical verifier
  shape separates topology, state edge, and proof obligations:
  `action(old: T) -> new: T`, `transition old.field: A -> new.field: B`, then a
  `where` proof block with explicit `require` constraints. Explicit `output`
  parameters and `consume`/`create` actions remain accepted, but the signature
  direction is the normal input-to-output surface. Multiple state edges may be
  grouped in a non-empty `transition { ... }` block.
  Each state field has exactly one flow declaration; split/partial flow merging
  is not supported.
- **Scoped proof blocks** — action proof logic lives under `where`; `transition` is an
  action-level state edge declaration before `where`, not a statement inside
  conditional proof logic. The type checker rejects asymmetric branch
  constraints when an output field is required in one proof branch but not its
  siblings.
- **Effect inference** — `action` bodies are classified as `Pure`, `ReadOnly`,
  `Mutating`, `Creating`, or `Destroying` based on their Cell operations.
- **Scheduler-aware metadata** — CKB-targeted builds expose access summaries
  and shared touch domains so block builders can reason about independent work.
- **Typed schema metadata** — Cell data layout, type identity, source hashes,
  runtime accesses, and verifier obligations are emitted as machine-readable
  metadata.
- **RISC-V output** — the executable target is ckb-vm-compatible RISC-V
  assembly or ELF. CellScript does not introduce a separate VM.
- **Package-aware compilation** — packages use `Cell.toml`, local modules,
  source roots, and local path dependencies.
- **Policy gates** — build, check, metadata, and artifact verification can
  reject outputs that violate the selected target or deployment policy.

## Example

A module contains schema declarations and executable entries. Persistent values
are declared as `resource`, `shared`, or `receipt`; executable logic as `action`
or `lock`; effects are written with explicit Cell operations and state
transition clauses.

**Declarations:**

```cellscript
module ckb::example

struct Config {
    threshold: u64
}

resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

shared Pool has store {
    token_reserve: u64
    ckb_reserve: u64
}

receipt VestingGrant has store, create, consume {
    beneficiary: Address
    amount: u64
    unlock_epoch: u64
}

struct Wallet {
    owner: Address
}

lock owner_only(protected wallet: Wallet, witness claimed_owner: Address) -> bool {
    require wallet.owner == claimed_owner
}
```

**Effects:**

```cellscript
action move_token(token: Token, to: Address) -> next_token: Token
where
    assert(token.amount > 0, "empty token")

    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
```

The compiler treats `consume`, `create`, `destroy`, action-boundary source
parameters, expression-level `read_ref<T>()`, and compiler-recognized stdlib
lifecycle patterns as **Cell effects**, not ordinary opaque function calls.
Those effects are reflected in metadata so CKB admission policy, schema
decoding, and artifact verification can audit the generated script.

**Scoped invariants and ProofPlan metadata:**

```cellscript
invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount

    assert_conserved(Token.amount, scope = group)
}
```

Declared invariants must state their CKB trigger and scope explicitly. In v0.15
they are emitted into Covenant ProofPlan metadata with trigger/scope/read
coverage, aggregate primitive relation checks, and a `gap:metadata-only` status
until executable verifier lowering is available. ProofPlan records also carry
macro expansion provenance for selected protocol flows and warnings for risky
coverage assumptions such as `lock_group` verifiers that scan transaction-wide
views.

**Complete fungible-token example:**

```cellscript
module ckb::fungible_token

resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

resource MintAuthority has store {
    token_symbol: [u8; 8]
    max_supply: u64
    minted: u64
}

action mint(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token)
where
    assert(auth_before.minted + amount <= auth_before.max_supply, "exceeds max supply")

    require auth_after.token_symbol == auth_before.token_symbol
    require auth_after.max_supply == auth_before.max_supply
    require auth_after.minted == auth_before.minted + amount

    create token = Token {
        amount: amount,
        symbol: auth_before.token_symbol
    } with_lock(to)

action transfer_token(token: Token, to: Address) -> next_token: Token
where
    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)

action burn(token: Token)
where
    assert(token.amount > 0, "cannot burn zero")
    destroy token
```

**Bundled protocol examples:**

| Example | What it shows |
|---|---|
| `examples/token.cell` | Mint, transfer, burn, guarded same-symbol merge |
| `examples/timelock.cell` | Time-gated release checks, delayed claim paths |
| `examples/multisig.cell` | Authorization thresholds, signature-oriented locks |
| `examples/nft.cell` | Unique assets, metadata, ownership transfer |
| `examples/vesting.cell` | Receipt-style grants and explicit claim state transitions |
| `examples/amm_pool.cell` | Shared pool state, swap/liquidity effects |
| `examples/launch.cell` | Launch/pool composition patterns |

Non-production language examples live under `examples/language/`. They compile
and exercise compiler/tooling surfaces, but they are not part of the seven-file
CKB production acceptance matrix. `registry.cell` covers bounded local
`Vec<Address>` / `Vec<Hash>` helpers; `examples/registry.cell` keeps that
surface available from the top-level examples directory. `order_book.cell` is a
local stack-backed order-vector sketch and does not claim persistent order-book
semantics. The v0.14 language examples cover CKB source/witness, capacity/time,
TYPE_ID, Spawn/IPC, and dynamic BLAKE2b surfaces as compiler/tooling examples.

## Comparison

Why CellScript is shaped around typed Cells, linear resources, explicit
transaction effects, and ckb-vm artifacts — instead of account storage or a
chain-specific VM:

| Dimension | CellScript | Solidity | Move | Sway |
|---|---|---|---|---|
| Execution target | RISC-V ELF / asm on ckb-vm | EVM bytecode | Move bytecode | FuelVM bytecode |
| State model | Typed Cells, explicit inputs/deps/outputs | Account storage slots | Resources in global storage | UTXO + native assets |
| Asset model | Native `resource`, state transitions, receipts, shared Cells | Manual token contracts | Native resources | Native assets |
| Linear ownership | Compiler-enforced | No | Yes (abilities) | No general user-defined |
| Shared state | Explicit `shared` Cells | Implicit contract storage | Shared objects (some chains) | No shared Cell analogue |
| Reentrancy | No callback-style reentrancy | Common risk surface | Lower by design | Lower predicate risk |
| Scheduler metadata | Native for CKB | None | Not GhostDAG-oriented | Predicate-level |
| CKB compatibility | Production-gated CKB ckb-vm artifact profile for the bundled Cell suite | Requires different VM | Requires different VM | Requires FuelVM |

Compared with hand-written CKB scripts, CellScript keeps the same
runtime substrate but replaces raw byte and syscall programming with typed Cell
operations, linear checking, schema metadata, and policy-verifiable artifacts.

---

## Editor Support

CellScript includes production-style local language tooling for early users:

- **In-process LSP** — diagnostics, completions, hover, go-to-definition,
  references, rename, formatting, and metadata-oriented code actions. The
  compiler crate exposes an `LspServer`; `cellc --lsp` provides a full
  `tower-lsp` JSON-RPC transport over stdio. Completions include flow
  states after `Type::`.
- **VS Code extension** — syntax highlighting, snippets, on-save diagnostics,
  compiler-backed formatting, scratch compilation, metadata/constraints/production
  reports, CKB target-profile arguments, and status-bar feedback. It shells out to
  `cellc` (or a `cargo run` fallback), so behavior stays identical to CLI and
  CI gates.

- [VS Code extension](https://github.com/tsukifune-kosei/CellScript/tree/main/editors/vscode-cellscript)
- [Runtime error codes](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_RUNTIME_ERROR_CODES.md)
- [Entry witness ABI](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md)
- [Collections support matrix](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)
- [Output bindings](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_OUTPUT_BINDINGS.md)
- [Historical signature-direction execution plan](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md)
- [CKB target profile tutorial](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/wiki/Tutorial-05-CKB-Target-Profiles.md)
- [CKB deployment manifest](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_CKB_DEPLOYMENT_MANIFEST.md)
- [Capacity and builder contract](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Linear ownership](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_LINEAR_OWNERSHIP.md)
- [Scheduler hints](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/CELLSCRIPT_SCHEDULER_HINTS.md)
- [Metadata verification and production gates](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md)
- [Standard library](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/wiki/Tutorial-10-Standard-Library.md)
- [CKB hashing workflow example](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/examples/ckb_hashing.md)
- [Collections matrix example](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/examples/collections_matrix.md)
- [Deployment manifest example](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/examples/deployment_manifest.md)
- [Output append example](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/examples/output_append.md)
- [Roadmap overview](https://github.com/tsukifune-kosei/CellScript/blob/main/roadmap/CELLSCRIPT_ROADMAP.md)
- [0.13 release scope](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [Mutate append example](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/examples/mutate_append.md)
- [0.14 roadmap](https://github.com/tsukifune-kosei/CellScript/blob/main/roadmap/CELLSCRIPT_0_14_ROADMAP.md)
- [0.14 release notes draft](https://github.com/tsukifune-kosei/CellScript/blob/main/docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES_DRAFT.md)
- [0.15 roadmap](https://github.com/tsukifune-kosei/CellScript/blob/main/roadmap/CELLSCRIPT_0_15_ROADMAP.md)

---

## Architecture

CellScript is a multi-pass compiler that lowers `.cell` source through five
well-defined stages, then emits RISC-V artifacts, typed metadata, and
profile-aware policy checks. Every module listed below lives in a single Rust
crate (`cellscript`) with its own `mod.rs` entry point under `src/`.

```mermaid
graph LR
    Source["Source (.cell)"] --> Lexer
    Lexer --> Parser
    Parser --> TypeCheck["Type Checker\n+ State Checks"]
    TypeCheck --> IRLower["IR Lowering\n+ Optimize"]
    IRLower --> Codegen["Codegen (RISC-V)"]
    IRLower --> Metadata["Metadata (JSON)"]
    Codegen --> Artifact[".s / .elf Artifact"]
```

### Compilation Pipeline

**1. Lexical analysis** (`lexer/`)
Scans `.cell` source into a typed token stream. Handles CellScript keywords,
operators, literals, and string interpolation. Every token carries a
line/column span for diagnostics.

**2. Parsing** (`parser/`)
Builds an AST from the token stream. The AST models the full surface:
`resource`, `shared`, `receipt`, `struct`, `enum`, `action`, `lock`,
`function`, `use`, `const`, capability gates, declarative flows,
action `transition` clauses, and all statement/expression forms.

**3. Semantic analysis** (`types/` + state-transition checks)
- *Type checking* — enforces linear resource semantics: every
  `resource`/`receipt` value must reach an explicit lifecycle or
  output-binding role before the action body exits. Also validates shared-state
  mutability rules, capability gates, effect classification (`Pure` /
  `ReadOnly` / `Mutating` / `Creating` / `Destroying`), and call signatures.
- *State transition checking* — validates explicit state fields,
  `flow` transition graphs, action `transition` clauses, legal state
  transitions, and static create-site checks.

**4. IR lowering** (`ir/` + `optimize/` + `resolve/`)
- *`resolve/`* — builds per-module symbol tables and resolves `use` imports
  across packages.
- *`ir/`* — lowers the typed AST into a flat, RISC-V-oriented intermediate
  representation (`IrAction`, `IrLock`, `IrPureFn`, `IrTypeDef`) with explicit
  Cell-effect instructions (`IrConsume`, `IrCreate`, `IrReadRef`,
  `IrDestroy`), cell-metadata equality checks, witness/layout slot assignments,
  and verifier obligations.
- *`optimize/`* — syntax-local constant folding and dead-branch pruning when
  `-O1+` is set. Intentionally conservative to preserve resource semantics.

**5. Code generation** (`codegen/`)
Emits ckb-vm-compatible RISC-V assembly (`.s`) or ELF (`.elf`):
- Syscall wrappers: `ckb_load_cell_data`, `ckb_load_witness`,
  `ckb_load_header_by_field`, `ckb_load_input_by_field`, and CKB extension
  syscalls (`secp256k1_verify`, `load_ecdsa_signature_hash`).
- Cell input/output/dep index mapping, witness ABI frames, runtime scratch
  buffers, and per-entrypoint trampolines.
- CKB syscall ABI with proper syscall number tables and source-flag conventions.

### Metadata & Policy

The compiler emits a single JSON metadata sidecar (`.elf.meta.json` /
`.s.meta.json`) that captures everything the chain scheduler, audit tools, and
policy gates need — without re-parsing source:

| What | Produced by | Consumed by |
|---|---|---|
| Schema layout, type IDs, field offsets | `ir/` | Schema decoder, indexer |
| Effect classification, resource summaries | `types/` | Scheduler, audit tools |
| Scheduler witness ABI & access domains | `codegen/` | CKB block builder, parallel scheduler |
| Source hashes, artifact CKB Blake2b | `lib.rs` | `cellc verify-artifact`, CI gates |
| Verifier obligations, pool invariants | `ir/` | On-chain verifier, policy checker |
| Covenant ProofPlan trigger/scope/read coverage, risk diagnostics, macro provenance | `proof_plan/` | `cellc explain-proof`, auditors |
| Target-profile policy violations | `lib.rs` | `cellc check`, CI gates |

`cellc constraints` produces a human-readable subset focused on production
readiness: ABI slot usage, register/stack-spill placement, witness byte bounds,
CKB cycle/capacity estimates.

### Runtime & Stdlib

| Module | What it does |
|---|---|
| **Stdlib** (`stdlib/`) | Built-in functions and compiler-recognized patterns that lower to explicit verifier effects: lifecycle helpers such as `std::lifecycle::transfer`, `std::receipt::claim`, and `std::lifecycle::settle`; cell metadata helpers such as `std::cell::preserve_type`, `std::cell::preserve_lock`, and `std::cell::preserve_capacity`; plus ckb-vm syscall/runtime helpers. Module-injected, not linked separately. |
| **Collections** (`stdlib/collections.rs`) | Bounded stack-backed `Vec<T: FixedWidth>` helpers for verifier-local values, including `new`, `with_capacity`, `capacity`, `push`, `extend_from_slice`, `len`, `is_empty`, indexing, `first`, `last`, `contains`, `set`, `remove`, `pop`, `insert`, `reverse`, `truncate`, `swap`, and `clear`. Cell-backed collection ownership remains unsupported. |

### Tooling Surface

| Tool | Module | How it works |
|---|---|---|
| **CLI** | `cli/` + `main.rs` | `cellc` binary with all subcommands |
| **LSP** | `lsp/` + `lsp/server.rs` | In-process `LspServer` + `tower-lsp` JSON-RPC over stdio (`cellc --lsp`) |
| **VS Code** | `editors/vscode-cellscript/` | Shells out to `cellc` for highlighting, diagnostics, reports |
| **Formatter** | `fmt/` | Idempotent formatter for `cellc fmt` and LSP |
| **Doc generator** | `docgen/` | HTML/Markdown/JSON docs from AST + metadata |
| **Simulator** | `simulate.rs` | Simulated evaluator — emits `TraceEvent` logs without ckb-vm |
| **REPL** | `repl.rs` | Interactive read-eval-print loop |

### Package & Build System

| Module | What it does |
|---|---|
| **Package workflow** (`package/`) | `Cell.toml` parsing, local path dependency resolution, transitive `Cell.lock` reproducibility, `cellc init`/`add`/`remove`/`install --path`/`update`/`info`. Registry publishing and registry dependency resolution are shaped but fail-closed. |
| **Incremental compiler** (`incremental/`) | Dependency-graph-aware build cache — skips recompilation when inputs are unchanged. |
| **Build integration** (`lib.rs`) | Resolves `Cell.toml` → `CellBuildConfig`, merges CLI + manifest options, selects entry scope, runs policy gates, writes artifacts + metadata. |

### CKB Target Profile

The CKB profile is not a final packaging switch. It is a policy layer that runs
from semantic analysis through code generation, metadata emission, and release
evidence. The goal is to make CKB assumptions visible before an artifact is
treated as deployable.

```mermaid
flowchart TB
    Source[".cell source + Cell.toml\n--target-profile ckb"] --> Frontend["Lexer + parser\nstable source spans"]
    Frontend --> Semantics["Type + state checks\nlinear resources, verifier require,\ninput/output/protected/witness/lock_args classification"]
    Semantics --> Policy["CKB policy gate\nfail closed on unsupported runtime or state shapes"]

    subgraph Rules["CKB profile rules"]
        R1["CKB syscall ABI\nsource flags + syscall numbers"]
        R2["Molecule-facing schema\nentry witness + lock args ABI"]
        R3["CKB Blake2b\nartifact + deployment hashes"]
        R4["hash_type / CellDep / DepGroup policy"]
        R5["capacity policy\nwith_capacity_floor, occupied_capacity,\ntx-size and cycle evidence"]
    end

    Rules --> Policy
    Policy --> IR["IR lowering + optimizer\nCell effects, entry ABI,\nverifier obligations"]
    IR --> Metadata["metadata sidecar\nschema, ABI, runtime errors,\nconstraints, CKB policy"]
    IR --> Codegen["RISC-V codegen\nCKB syscalls, raw ELF,\nper-entry trampolines"]
    Codegen --> Artifact["CKB artifact\n.s / .elf"]

    Artifact --> Verify["cellc verify-artifact\nprofile, source hash,\nartifact hash, policy flags"]
    Metadata --> Verify

    Artifact --> Builder["builder workflow\ninputs, outputs, outputs_data,\nwitness, cell_deps, capacity floors"]
    Metadata --> Builder
    Builder --> Acceptance["CKB acceptance gate\ndry-run, commit, cycles,\ntx size, occupied capacity,\nvalid/invalid lock matrix"]
```

This separates three boundaries:

- **compiler boundary** — parse, type/state checks, CKB policy rejection, IR,
  codegen, and metadata;
- **artifact boundary** — `cellc verify-artifact` proves the artifact, sidecar,
  source hash, target profile, and selected policy flags agree;
- **chain-evidence boundary** — builders and acceptance scripts prove concrete
  CKB transaction shape, capacity, cycles, tx size, and lock/action behavior.

Capacity in this profile has two layers. `with_capacity_floor(shannons)`
declares a type-level output floor that is visible in metadata and constraints.
`occupied_capacity("TypeName")` keeps runtime-visible capacity checks available.
Neither replaces builder evidence: the final transaction still has to measure
occupied capacity, provide enough output capacity, and record tx-size evidence.

### Wasm Gate

`wasm/` is a **fail-closed** audit scaffold: it compiles and is tested, but
explicitly rejects executable CellScript entries because CellScript has no
production Wasm backend. Type-only IR modules emit an audit report; all other
entries return `WasmSupportStatus::UnsupportedProgram`. The module exists to
prevent a hidden, stale backend from drifting away from the current IR.

---

## Reference

### Manifest

`Cell.toml` sets the package entry point, source roots, target profile, and
policy defaults:

```toml
[package]
name = "token"
version = "0.15.0"
entry = "src/main.cell"
source_roots = ["src"]

[build]
target = "riscv64-elf"
target_profile = "ckb"

[policy]
production = true
deny_fail_closed = true
deny_ckb_runtime = false
deny_runtime_obligations = false
```

Command-line flags can tighten policy checks for a build or CI job.

### Package Workflow

CellScript ships a local-first package workflow in `cellc`. Local packages,
source roots, path dependencies, lockfile refresh, and package
build/check/doc/fmt flows are production-style. Registry publishing and
registry dependency resolution remain experimental and fail-closed.

**Supported today:**

- `cellc init` — create an application or library package with `Cell.toml`
- `cellc build` / `check` / `doc` / `fmt` — operate on the current package
- top-level `cellc <input>` and report commands accept `.cell` files, package
  directories, or `Cell.toml` manifests where the command supports an input
- `cellc add --path` — records local path dependencies in `Cell.toml`
- `cellc install --path` and `cellc update` — resolve local path dependency
  graphs and refresh `Cell.lock`
- Local path dependencies are resolved recursively and included in module
  loading, source hashing, and metadata
- `Cell.lock` — captures direct and transitive resolved dependency identity
  for reproducible checks
- `cellc info --json` — exposes package metadata for CI and tooling

**Experimental / fail-closed:**

- Registry `publish`, registry package installation/resolution, and `login`
  are command-shaped but fail-closed until the registry backend and trust model
  are finalized
- Git dependencies are explicit remote source fetches; treat them as
  review-required inputs, not the registry production path

### CLI Commands

| Command | Purpose |
|---|---|
| `cellc <input>` | Compile a `.cell` file, package directory, or `Cell.toml` |
| `cellc build` | Compile the package, write artifacts + metadata |
| `cellc check` | Type-check and lower without writing artifacts |
| `cellc metadata` | Emit lowering, runtime, scheduler, source, and schema metadata |
| `cellc constraints` | Emit profile-aware production constraints |
| `cellc abi` | Explain `_cellscript_entry` witness ABI layout for an action or lock |
| `cellc entry-witness` | Encode `_cellscript_entry` witness bytes |
| `cellc scheduler-plan` | Consume scheduler hints and report serial/conflict policy |
| `cellc ckb-hash` | Compute CKB default Blake2b-256 hashes for builders and release evidence |
| `cellc opt-report` | Compare O0..O3 artifact size and constraints status |
| `cellc verify-artifact` | Verify an artifact against its metadata sidecar |
| `cellc test` | Run compiler and policy tests (no trusted runtime execution) |
| `cellc doc` | Generate API and audit documentation |
| `cellc fmt` | Format `.cell` sources or check formatting |
| `cellc init` | Create a package skeleton |
| `cellc add` / `remove` | Mutate local package dependencies |
| `cellc install --path` / `update` | Resolve local path dependencies and refresh `Cell.lock` |
| `cellc info` | Print manifest and package information |
| `cellc repl` | Start the interactive REPL |
| `cellc run` | Run ELF entrypoints via VM runner or simulator |
| `cellc publish` / registry `install` / registry-backed `update` / `login` | Experimental registry flows, fail-closed |

### CLI Options

| Option | Purpose |
|---|---|
| `--target riscv64-asm` | Emit RISC-V assembly |
| `--target riscv64-elf` | Emit a RISC-V ELF artifact |
| `--target-profile ckb` | Use the CKB profile |
| `--entry-action <ACTION>` | Compile a single action as the artifact entrypoint |
| `--entry-lock <LOCK>` | Compile a single lock as the artifact entrypoint |
| `--json` | Emit machine-readable summaries where supported |
| `--production` | Apply production-oriented metadata policy checks |
| `--deny-fail-closed` | Reject fail-closed runtime features or obligations |
| `--deny-ckb-runtime` | Reject CKB transaction/syscall runtime requirements |
| `--deny-runtime-obligations` | Reject runtime-required verifier obligations |

---

## Project Layout

```text
cellscript/
├── src/                 # compiler, parser, type checker, lowering, codegen, CLI
├── examples/            # example contracts and protocol patterns
├── tests/               # compiler and CLI tests
└── editors/
    └── vscode-cellscript/
```

Development style and backend/codegen rules are tracked in
[`CODING_STYLE.md`](CODING_STYLE.md).

## License

License metadata is declared in [`Cargo.toml`](Cargo.toml). The repository
includes [`LICENSE-MIT`](LICENSE-MIT).

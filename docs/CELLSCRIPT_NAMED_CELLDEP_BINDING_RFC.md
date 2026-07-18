# RFC: Named CellDep Binding And Deployment-Resolved Dependency Identity

## Status

Draft for community review. Targets the 0.22 release line as the earliest
realistic landing; not part of 0.21.1 production boundary. Tracking issue:
none yet (required before implementation adoption).

This RFC sits next to the v0.22 typed transaction-view handles proposal
(`roadmap/CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md` §"Typed
Transaction-View Handles Before Tx Builder Surface") and the
`docs/CELLSCRIPT_CELL_MODEL_SYNTAX_AUDIT_2026_07_05.md` audit. It does **not**
replace the typed-view work; it is the policy + identity layer that needs to
land together with the view layer.

## TL;DR

CellScript scripts can already read a CellDep by index and verify its data
hash at runtime, but they cannot say **which** CellDep is the oracle,
verifier, or config cell. This RFC proposes a thin **named CellDep binding**
surface:

1. a module-level `cell_deps { dep name: Policy }` declaration block;
2. a `source::cell_dep(name)` access expression that lowers to the existing
   `source::cell_dep(index)` machinery;
3. an **intent vs. fact split** in `Cell.toml` and `Deployed.toml` so a named
   binding carries an identity policy but never a literal `out_point`;
4. an explicit **evidence tier** aligned with the expanded 0.22 taxonomy so
   adapter-side checks and on-chain hash checks do not silently merge.

The proposal rejects bare `cell_dep("0x...:0")` and
`cell_dep("registry:ns/name/version")` in source. Names are policy handles,
not authority. Authority is the cryptographic identity the manifest requires
and the on-chain verifier confirms.

## The Story

You are writing a settlement contract. The verifier reads a price from a
CellDep-backed oracle and compares it to a fair-price witness. Today:

```cellscript
action settle(amount: u64, fair_price: u64) {
    verification
        let oracle = source::cell_dep(0)               // which cell is this?
        let size   = ckb::cell_data_size(oracle)      // 4 bytes
        require size == 32, "oracle schema drift"
        let hash   = ckb::cell_data_hash(oracle)
        require hash == 0xab12...34, "oracle rotated" // compares to a literal
        // ...read price bytes, compare to fair_price, settle...
}
```

This compiles and runs in CKB VM today (`ckb::cell_data_size` /
`ckb::cell_data_hash` are real runtime builtins, see
`tests/support/ckb_script_runner.rs:217-228` and
`tests/ickb_diff.rs:3369-3396`). But four things are wrong:

1. **`source::cell_dep(0)` is a bare index.** A reader of the source cannot
   tell whether `0` is the oracle, the verifier script, or some unrelated
   dep added by the builder. The author has to leave a comment, and the
   comment can rot.
2. **`require hash == 0xab12...34` puts a deployment fact in source.** The
   hash is the oracle's current `data_hash` on a specific network, baked
   into the script. Re-deploying against a rotated oracle means a
   recompile.
3. **The Cell.toml entry sits ambiguously between intent and fact.**
   `[deploy.ckb.cell_deps]` currently lets you write both the policy (what
   identity the dep must carry) and the fact (the literal `out_point`),
   side by side. The two roles are not enforced.
4. **There is no link between the source index, the manifest entry, and
   the deployment record.** Three places name the same thing, but the
   compiler does not require them to agree.

This RFC fixes all four without changing the CKB VM ABI, the runtime
helper surface, or the wasm bundle size.

## Current State In 0.21.1

A short tour of what is already in the compiler, to anchor the proposal in
the existing surface.

### Source-level CellDep access

| Surface | Status | Reference |
|---|---|---|
| `source::cell_dep(i)` (typed view) | implemented, executable | `roadmap/CELLSCRIPT_ROADMAP_OVERVIEW.md:375`; `src/parser/...`, `src/ir/mod.rs:4459` |
| `source::header_dep(i)` | implemented, executable | roadmap §6.3 |
| `ckb::cell_data_hash(view)` | implemented, returns `Hash`, lowers to `LOAD_CELL_DATA` | `src/ir/mod.rs:4459-4470`; `src/codegen/mod.rs:16086` |
| `ckb::cell_data_hash_at(view, offset)` | implemented | `src/ir/mod.rs:4468-4470` |
| `ckb::cell_data_size(view)` | implemented, returns `u64` | `src/ir/mod.rs:4658-4660`; `src/codegen/mod.rs:16037` |
| `ckb::cell_capacity(view)` | implemented | `src/types/mod.rs:4904` |
| `read_ref<T>()` (type-bound data dep) | implemented | `src/lib.rs:20800-20810` |
| `read param: T` (action/lock entry) | implemented | `src/codegen/mod.rs:1849-1921` |

The above is **not metadata-only**. `tests/ickb_diff.rs:3369-3396` compiles
`ckb::cell_data_size(source::cell_dep(0))` to an ELF, places a 4-byte
CellDep in the fixture transaction, and asserts `exit_code == 0` from
CKB VM. RISC-V execution is real; cycles are charged.

### Manifest-level CellDep declaration

```toml
# Cell.toml — see src/package/mod.rs:215-236
[deploy.ckb]
hash_type = "data1"
dep_type  = "code"

[[deploy.ckb.cell_deps]]
name      = "price_oracle"
out_point = "0xabcd...:0"
dep_type  = "dep_group"
hash_type = "type"
```

`CkbCellDepConfig` carries `name` / `out_point` / `tx_hash` / `index` /
`dep_type` / `data_hash` / `hash_type` / `type_id`. All fields are
`#[serde(default)]`, so any of them may be absent. The compiler does not
currently enforce that at least one identity field is present. Parser
location: `src/lib.rs:1665-1715` (`parse_ckb_cell_dep_location`).

### Deployment record

```toml
# Deployed.toml — see docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md
[[deployments]]
network     = "mainnet"
record      = "0x...:0"
out_point   = "0x...:0"
code_hash   = "0x..."
data_hash   = "0x..."
hash_type   = "type"
dep_type    = "code"
type_id     = "0x..."

[[deployments.cell_deps]]
name     = "price_oracle"
out_point = "0xabcd...:0"
dep_type = "dep_group"
```

### Adapter and verification

`docs/CELLSCRIPT_CKB_ADAPTER.md` documents the manifest-backed CellDep
completion path. The 0.21 RC added adapter-side
`args_parts` / manifest CellDep / scan-selector evidence
(`docs/releases/CELLSCRIPT_0_21_RELEASE_NOTES.md` §"Builder And Adapter
Resolution"). What is **not** present:

- a link from a source-level `source::cell_dep(0)` call to a specific
  `[deploy.ckb.cell_deps]` entry;
- an enforced identity policy (e.g. "this dep must have `data_hash` X");
- an evidence tier that distinguishes adapter-side checks from on-chain
  hash comparisons.

## The Gap

Three concrete failure modes the current surface does not handle.

### Gap 1 — "Index 0 is the oracle" is invisible

`source::cell_dep(0)` carries no information about the cell's role. A
reader must rely on a comment, a position convention, or a separate
document. When the builder adds, removes, or re-orders CellDeps, the
index silently changes and the script reads the wrong cell.

### Gap 2 — Two same-type CellDeps are indistinguishable to the type system

`read_ref<OracleConfig>()` binds to "the first CellDep with type_hash
matching `OracleConfig`'s type script". If the same transaction references
two CellDeps with identical type scripts but different deployment
identities (different `data_hash`, different `args`), the type-based
binding cannot tell them apart. CellScript type equality collapses
distinctions that matter on chain.

### Gap 3 — Intent and fact share one struct

`CkbCellDepConfig` accepts both identity policy fields (`data_hash`,
`type_id`, `hash_type`, `dep_type`) **and** the deployment fact
(`out_point` or `tx_hash` + `index`). The current `Cell.toml` is meant
to express intent, but a script author can write the literal `out_point`
in `Cell.toml` and the compiler will not object. A redeploy to a
different network then requires editing `Cell.toml`, which the docs
already say should not contain deployment facts.

## Proposal

### Source syntax — minimal expression form

A module-level declaration block introduces named cell deps. The block
appears once per module, near the top, after `use` imports:

```cellscript
module amm::settle

use cellscript::fungible_token::Token

// New: cell_deps declaration block.
cell_deps {
    dep price_oracle: OracleConfig      // policy: data cell, data_hash pinned
    dep risk_oracle:  RiskConfig        // policy: data cell, type_id-anchored
    dep verifier:     Secp256k1Script   // policy: code cell, code_hash pinned
}
```

In an action body, the binding is referenced by name:

```cellscript
action settle(amount: u64, fair_price: u64) {
    verification
        let oracle_view = source::cell_dep(price_oracle)
        require ckb::cell_data_size(oracle_view) == 32, "oracle schema drift"
        require ckb::cell_data_hash(oracle_view) == price_oracle.data_hash,
            "oracle rotated"
        let price = oracle_view.price   // layout-derived field access
        require price matches fair_price within slippage, "stale price"
        // ... settle ...
}
```

The expression `source::cell_dep(price_oracle)` is **syntactic sugar** for
`source::cell_dep(<const-folded index>)`. After type checking, the
identifier `price_oracle` resolves to a `u64` literal derived from the
manifest binding, and the existing codegen path is unchanged. No new
runtime helper is introduced. The wasm bundle does not grow.

### Why not `cell_dep::price_oracle` or `dep::price_oracle`

Two reasons:

- `dep::` is already a **reserved module namespace for package
  dependencies** (`use dep::token::Token`, see
  `tests/cli.rs:2392-2425` and the package manager module-mapping code).
  Reusing it would force every consumer of this RFC to disambiguate
  between "the `token` package" and "a `token` CellDep binding".
- A second `::` namespace would invite the question of which namespace
  owns lifecycle, which owns access, and which owns policy. A
  declaration block keeps the policy in one place; access is just the
  existing `source::cell_dep(...)` form.

The expression form is the **first cut**. Source-qualifier sugar such as
`read config: OracleConfig from price_oracle` is a later stage that
should ship with the 0.22 action source-qualifier work
(`roadmap/CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md` §"Discussion
Candidates, Not Baseline Commitments") — not bolted onto this RFC.

### Manifest — intent vs. fact split

`Cell.toml` carries the **intent**: which named bindings exist and what
identity policy each one requires. `Deployed.toml` carries the **fact**:
which on-chain cell satisfies each named binding on which network.

```toml
# Cell.toml — intent only. Out_points do not belong here.
[deploy.ckb]
hash_type = "data1"

[[deploy.ckb.cell_deps]]
name              = "price_oracle"
kind              = "data"            # data | code | dep_group
require_data_hash = "0xab12...34"     # policy: must match at resolve time
# Optional, mutually exclusive with require_data_hash:
# require_type_id  = "0x..."

[[deploy.ckb.cell_deps]]
name               = "verifier"
kind               = "code"
require_code_hash  = "0xc0de...ef"
require_hash_type  = "type"
require_dep_type   = "code"
```

```toml
# Deployed.toml — facts, per network.
[[deployments]]
network   = "mainnet"
entry     = "0x...:0"

[[deployments.cell_deps]]
name      = "price_oracle"
out_point = "0xabcd...:0"
data_hash = "0xab12...34"      # must match the require_data_hash above

[[deployments.cell_deps]]
name      = "verifier"
out_point = "0x9876...:0"
code_hash = "0xc0de...ef"
hash_type = "type"
dep_type  = "code"
```

The split has three consequences:

- `Cell.toml` is now genuinely network-neutral. The same source ships to
  mainnet, testnet, and devnet unchanged.
- `Deployed.toml` is network-scoped. The adapter picks the matching
  `[[deployments]]` block per request.
- The "identity" of a named binding is **the policy in `Cell.toml`**,
  not the `out_point` in `Deployed.toml`. The `out_point` is the fact
  that satisfies the policy on a specific chain.

### Adapter — name resolution

The adapter receives:

1. `CompileMetadata.named_cell_deps` (from the compiler, with the
   declared names, kinds, and policies);
2. `Deployed.toml` for the target network;
3. a candidate transaction under construction.

For each `source::cell_dep(name)` call, the adapter:

1. Looks up `name` in `CompileMetadata.named_cell_deps` to obtain the
   required policy.
2. Looks up `name` in the per-network `Deployed.toml` block to obtain
   the candidate `out_point`.
3. Resolves the live cell (via `get_live_cell` RPC) and checks that its
   fields satisfy the policy:
   - `kind = "data"`: `data_hash` matches `require_data_hash` (or
     recomputed from cell data via `ckb::hash_data_packed`-equivalent);
   - `kind = "code"`: `code_hash`, `hash_type`, `dep_type` all match;
   - `kind = "dep_group"`: the dep-group cell's data hashes resolve to
     the expected sub-deps.
4. Places the resolved CellDep at the **next free CellDep index** in the
   transaction under construction, and rewrites the `source::cell_dep(i)`
   call site to use that index.
5. Emits a `builder_required_assumption` for each named binding
   recording the resolved `out_point`, the network, and the
   `policy_satisfied = true` result.

### Evidence tiers

The 0.21 ProofPlan already uses a three-state model
(`docs/releases/CELLSCRIPT_0_21_RELEASE_NOTES.md` §"Aggregate Invariant
Coverage"):

| Tier | Meaning |
|---|---|
| `metadata-only` | compiler records the obligation; no executable surface |
| `runtime-helper-required` | helper exists but no generated access covers it |
| `checked-runtime` | generated runtime access covers the obligation |

Named CellDep bindings use the expanded 0.22 evidence taxonomy. Their relevant
tiers and discharge conditions are:

| Tier | When |
|---|---|
| `metadata-only` | source declares the binding but emits no `require` and no `source::cell_dep(name)` access |
| `builder-evidence-required` | adapter must verify the policy against the live cell at build time, but the script emits no on-chain check |
| `checked-runtime` | the script's IR lowers to a `require ckb::cell_data_hash(view) == bound_policy_hash` (or equivalent) that runs in CKB VM |

Strict metadata validation must reject any claim labelled `checked-runtime`
that lacks a generated runtime access. A `builder-evidence-required` claim may
remain builder-only, but production evidence must not relabel it as an on-chain
check. The two checks are **independent**: a builder-side verification does not
become on-chain-checked just because the adapter passed.

### Identity policy grammar

A named binding's policy is one of:

```
data cell:
    require_data_hash  = "<blake2b-256 hex>"
    | require_type_id   = "<type-id hex>"

code cell:
    require_code_hash  = "<blake2b-256 hex>"
    require_hash_type  = "type" | "data" | "data1" | "data2"
    require_dep_type   = "code" | "dep_group"

dep_group:
    require_data_hash  = "<blake2b-256 hex of the dep-group cell data>"
    require_sub_dep    = "<name reference>"   # transitive
```

Mutual exclusion rules:

- `require_data_hash` and `require_type_id` are mutually exclusive for
  `data` bindings; pick one.
- A `kind = "code"` binding with `require_dep_type = "dep_group"` is
  rejected; dep_group is a data kind.
- An empty policy (no `require_*` set) is rejected for any production
  mode; dev mode may accept it with a `dev_unguarded_dep` warning.

## Why Not Bare OutPoint Or Registry Locator

Three forms are explicitly **rejected** in `.cell` source:

```cellscript
// Rejected: pins a literal OutPoint into the script source.
let v = cell_dep("0xabcd...:0")

// Rejected: conflates source-package identity with on-chain deployment.
let v = cell_dep("registry:nervos/cellscript-oracle/1.4.2")

// Rejected: a bare name with no identity commitment.
let v = cell_dep("price_oracle")
```

Why:

- A literal `OutPoint` makes the source chain-bound. The same source
  cannot run on testnet and mainnet without recompiling, and the
  compiler has no way to verify the literal is still a live cell at
  deploy time.
- A registry locator is a **package** identity, not a **deployment**
  identity. A published package may have many deployments; binding
  source to a registry coordinate picks a source version, not a cell.
- A bare name with no policy is unverifiable. The compiler has no
  reason to know which cell `price_oracle` refers to, and the
  adapter has no reason to trust the author.

The "real" named binding — `dep price_oracle: OracleConfig` plus a
`require_data_hash` policy — is what the CKB Cell model gives us:
identity is hash-based, not locator-based. This proposal aligns the
language with that model.

A legitimate special case — a script whose correctness is genuinely
bound to a specific OutPoint because the protocol defines that exact
cell — is handled by **chain-bound deployment profiles** in the
adapter, not by source syntax. The profile declares the OutPoint;
the source is silent on it.

## Layering With 0.22 Typed View

The 0.22 roadmap proposes `ckb::cell_dep(index) -> CellDepView` as a
typed read-only transaction view
(`roadmap/CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md:374`). The two
proposals are **complementary layers**, not competitors:

```
.declared name        (this RFC)        source layer
    │
    ▼  const-fold
index                (compiler)        binding layer
    │
    ▼  typed view
SourceView<CellDep>  (0.22 roadmap)    type layer
    │
    ▼  accessor
ckb::cell_data_hash  (existing)        runtime layer
```

What each layer adds:

- **this RFC**: a logical name in source, a policy in the manifest,
  an intent/fact split.
- **0.22 typed view**: stronger return type (`SourceView<CellDep>`
  rather than `u64`-view-by-convention), first-class handle in the
  type system, better LSP hover.
- **existing runtime helpers**: the actual `LOAD_CELL_DATA` syscalls
  and the helper-emission machinery in `src/codegen/mod.rs:16037-16086`.

A single `source::cell_dep(name)` call exercises all four layers.
No new codegen, no new helper, no new syscall.

## Open Questions For Community Review

These are the points where feedback is most wanted. The proposal is
explicitly **not** locked on any of them.

### OQ1 — Where does the `cell_deps { ... }` block live?

Two options under active discussion:

- **Module top level** (this RFC's primary proposal). Pro: one place
  per module, easy to audit. Con: an action cannot introduce its own
  binding that the rest of the module ignores.
- **Action- or lock-level** (`action settle(...) { deps { ... }; ... }`).
  Pro: tight scope. Con: a shared config dep is declared in every
  action that needs it, with risk of drift.

Module top is the working assumption. Speak up if action-level scope
is preferred.

### OQ2 — Should `require_data_hash` be the default policy?

For `kind = "data"`, a binding without a hash policy is unverifiable.
For `kind = "code"`, a binding without a `require_code_hash` is the
common case for in-house verifiers where the code hash is allowed to
roam. The question is whether the policy grammar should require
explicit `require_*` for every binding, or whether `code` bindings
get a default of "no hash commitment, builder resolves by name".

The working assumption is **explicit required for production modes**,
per the policy grammar in this RFC. Dev mode accepts empty policies
with a warning.

### OQ3 — How do `dep` names interact with `use`?

`use dep::token::Token` already refers to a **package**. The new
`cell_deps { dep name: T }` declares a **binding**. Both are in scope
in the action body. They share no syntax but sit in the same namespace
if `T` is a type from an imported package. Concretely:

```cellscript
use dep::oracle_pkg::OracleConfig   // a type

cell_deps {
    dep price: OracleConfig         // a binding of that type
}
```

Should the type and binding names collide? The working assumption is
**no collision by design** — the binding is a name in the `cell_deps`
block, the type is a name in the `use` scope. They occupy different
identifier tables. But the parser needs to be explicit about which
table an identifier in `source::cell_dep(name)` consults first.

### OQ4 — Cross-package named bindings

If a shared `oracle` package declares `cell_deps { dep main: ... }`,
can a downstream consumer reference `oracle::main` directly, or must
it re-declare the binding? The working assumption is **re-declare**,
because the consumer's manifest is the source of its own policy, and
re-declaring makes the policy explicit at the use site.

### OQ5 — TYPE_ID-based policy

`require_type_id = "0x..."` lets a binding anchor on a TYPE_ID cell
that may rotate. The CKB TYPE_ID pattern is well-defined; the question
is whether the compiler should resolve the TYPE_ID at build time
(fixing the OutPoint) or carry the TYPE_ID forward to the on-chain
verifier and let it resolve. The working assumption is **build-time
resolve, with a build receipt recording the resolved OutPoint**; the
on-chain check still uses the recorded `data_hash`, so a rotation
between build and submit is detectable.

## Validation Criteria

Success looks like all of the following, all enforceable in CI.

| Validation | Tooling | Mode |
|---|---|---|
| `cell_deps { dep name: T }` parses; `name` is a string-literal-typed identifier | parser, formatter, syntax-combo | dev |
| `source::cell_dep(name)` resolves to a `u64` constant; missing name is a compile error | type checker | dev |
| `Cell.toml` with a `[[deploy.ckb.cell_deps]]` entry without any `require_*` field is rejected | package loader | `ci` |
| `Cell.toml` with a `[[deploy.ckb.cell_deps]]` entry containing an `out_point` is rejected (or warned in dev) | package loader | `ci` |
| `Deployed.toml` `[[deployments.cell_deps]]` whose `data_hash` does not match the policy `require_data_hash` is rejected | adapter | `ci` |
| `Deployed.toml` resolves a `name` that the source does not declare → warning | adapter | `dev` |
| `source::cell_dep(name)` lowers to the existing `__ckb_source_cell_dep` constructor; a hash accessor continues to use `__ckb_cell_data_hash`; no new helper emitted | codegen, opt-report | `backend` |
| `builder_required_assumption` is emitted in metadata for each named binding | metadata audit | `ci` |
| An `evidence_tier` of `checked-runtime` is only emitted when the IR contains a `require ckb::cell_data_hash(view) == bound_hash` access | strict metadata validation | `ci` |
| `checked-runtime` is rejected when the script does not generate a matching on-chain check; `builder-evidence-required` remains explicitly builder-only | strict metadata validation | `ci` |
| Bundle size budget unchanged (no new wasm surface) | `website/scripts/build-wasm.sh` | `release` |
| New syntax is rejected by 0.21.x gate as "future-version syntax" | syntax-combo matrix | `dev` |

## Rollout

### Phase 1 — parser, formatter, type-check (this RFC)

- Add `cell_deps { ... }` block to parser, formatter, LSP hover, docgen.
- Add `source::cell_dep(name)` access form.
- Reject policies with no `require_*` in non-dev modes.
- Reject `out_point` in `[[deploy.ckb.cell_deps]]` in non-dev modes.
- Extend syntax-combo matrix with `SCA-BUG-NAMED-CELLDEP-POLICY-MISSING`
  and `SCA-BUG-NAMED-CELLDEP-OUTPOINT-IN-INTENT` bug classes.

### Phase 2 — adapter resolution

- Adapter consumes `CompileMetadata.named_cell_deps` and resolves
  per-network `Deployed.toml` entries.
- Emits `builder_required_assumption` records.
- Fails closed on policy mismatches.

### Phase 3 — runtime evidence tier (1st RFC ↔ 0.22 typed view coupling)

- Evidence tier classification in `CompileMetadata` and strict 0.17
  validation, following the 0.21 aggregate-invariant pattern.
- `require ckb::cell_data_hash(view) == bound_hash` promotion path
  to `checked-runtime` when the IR contains a matching access.

### Phase 4 — source-qualifier sugar (follow-up RFC, 0.22+)

- `read config: OracleConfig from price_oracle` action/lock parameter
  sugar.
- Lands together with the 0.22 action source-qualifier work
  (`roadmap/CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md`).

### Phase 5 — TYPE_ID policy (deferred)

- TYPE_ID-based identity in policy.
- Requires a runtime helper to recompute TYPE_ID; out of scope for
  0.22 unless the helper already exists.

## Out Of Scope

- `cell_dep("0x...:0")` source syntax. Permanently rejected.
- `cell_dep("registry:ns/name/version")` source syntax. Permanently
  rejected; the registry is for package discovery, not deployment
  identity.
- A bare `cell_dep("name")` with no policy. Permanently rejected.
- Large `TxView` objects, `env::tx()`, and full transaction-builder
  DSL. Belongs in a separate RFC, per the 0.22 roadmap.
- `TemplateLayout`-based identity commitment. Belongs in the
  TemplateLayout RFC, not here.

## References

- `docs/CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md` —
  intent / build / fact split
- `docs/CELLSCRIPT_REGISTRY_PHASE1.md` — registry as package discovery
- `docs/CELLSCRIPT_CKB_ADAPTER.md` — adapter resolution path
- `roadmap/CELLSCRIPT_0_22_TYPE_AND_SET_THEORY_ROADMAP.md` — typed
  transaction-view handles
- `docs/CELLSCRIPT_CELL_MODEL_SYNTAX_AUDIT_2026_07_05.md` — source
  view audit
- `docs/CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` — grammar governance
  contract
- `src/package/mod.rs:215-236` — `CkbCellDepConfig` definition
- `src/lib.rs:1665-1715` — `parse_ckb_cell_dep_location`
- `src/lib.rs:729-739` — `CkbCellDepMetadata`
- `src/ir/mod.rs:4459-4470`, `4658-4660` — runtime helper lowering
- `src/codegen/mod.rs:16037-16086` — `__ckb_cell_data_size` /
  `__ckb_cell_data_hash` emission
- `tests/support/ckb_script_runner.rs:217-228` — CellDep VM fixture
- `tests/ickb_diff.rs:3369-3396` — VM test that runs the fixture
- `tests/cli.rs:2392-2425` — `dep::` namespace usage as package
  reference (reserved)
- `roadmap/CELLSCRIPT_ROADMAP_OVERVIEW.md:375` — `source::cell_dep`
  status
- `docs/releases/CELLSCRIPT_0_21_RELEASE_NOTES.md` §"Aggregate
  Invariant Coverage" — three-state evidence tier precedent

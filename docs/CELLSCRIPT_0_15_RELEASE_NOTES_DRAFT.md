# CellScript 0.15 Release Notes Draft

**Status**: Release-gate draft for the `cellscript-0.15` implementation branch.

**Updated**: 2026-04-28.

CellScript 0.15 is the scoped-invariant and Covenant ProofPlan milestone. It
makes verifier triggers, scope, coverage, builder assumptions, and enforcement
gaps explicit in source and metadata, and promotes cell identity into a
first-class primitive while resetting the capability vocabulary from protocol
verbs to kernel effects.

The short version: 0.15 adds scoped invariant declarations, aggregate
assertion primitives, Covenant ProofPlan metadata and `cellc explain-proof`,
first-class cell identity policies, explicit destruction policies, a
kernel/protocol primitive split, a compat/strict migration path, and renames
internal `type_hash` metadata fields.

## Highlights

### Scoped Invariant Syntax

0.15 adds first-class invariant declarations with explicit trigger, scope,
and reads:

```cellscript
invariant udt_amount_non_increase {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount

    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}
```

Supported triggers: `explicit_entry`, `lock_group`, `type_group`.
Supported scopes: `selected_cells`, `group`, `transaction`.

Invariants are preserved through AST, type checking, IR, module metadata,
formatting, LSP symbols, hover/completions, docs, and scoped CKB entry
compilation.

### Aggregate Invariant Primitives

0.15 adds scoped aggregate assertion primitives for common covenant-style
relations:

```cellscript
assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
assert_conserved(Token.amount, scope = group)
assert_delta(Token.amount, delta, scope = selected_cells)
assert_distinct(outputs<NFT>.token_id, scope = transaction)
assert_singleton(Config.config_id, scope = group)
```

Aggregate fields must resolve to fixed-width integer or fixed-byte schema
fields. Dynamic tables, generic collections, and bool fields are rejected.

**Boundary**: Aggregate primitives are currently metadata-only. They emit
`codegen_coverage_status: "gap:metadata-only"` and
`status: "runtime-required"` until a later lowering pass proves them on
chain.

### Covenant ProofPlan Metadata

0.15 adds a `ProofPlan` stage and `cellc explain-proof` audit surface.
Runtime, action, function, and lock metadata expose ProofPlan records with:

- invariant name and source span
- trigger, scope, reads, coverage
- input/output relation checks
- group cardinality
- identity/lifecycle policy
- builder assumptions
- diagnostics and codegen coverage status

`cellc explain-proof` prints trigger/scope/reads/coverage/on-chain status in
human-readable and JSON output.

`ScriptArgs` and `lock_args` provenance is reported under `reads.lock_args`,
not `reads.witness`; witness remains reserved for transaction witness data.

`cellc check --deny-runtime-obligations` rejects runtime-required ProofPlan
gaps, including declared invariants whose coverage is still metadata-only.

Lock-group transaction risk diagnostics warn when a `lock_group` verifier
scans transaction-wide views, because only inputs sharing that lock trigger
the verifier.

### Cell Identity and TYPE_ID Lifecycle

0.15 promotes cell identity from a metadata annotation into a first-class
primitive policy:

```cellscript
resource Token has store {
    identity(ckb_type_id)
    amount: u64
}
```

Supported identity policies:

| Policy | Meaning | 0.15 executable boundary |
|--------|---------|--------------------------|
| `identity none` | No identity tracking (default, backward compatible) | No identity verifier is emitted |
| `identity ckb_type_id` | CKB TYPE_ID: derived from first input + output index | `create_unique` requires a TYPE_ID output plan and reports global creation uniqueness as runtime-required; `replace_unique` preserves TypeHash |
| `identity field(path)` | Fixed-width field identity within the data payload | `create_unique` anchors the output field bytes and reports global uniqueness as runtime-required; `replace_unique` compares input/output field bytes |
| `identity script_args` | Identity derived from the executing script args | `create_unique` anchors the output LockHash and reports global uniqueness as runtime-required; `replace_unique` preserves LockHash |
| `identity singleton_type` | Singleton type identity | `create_unique` anchors the output TypeHash and reports singleton creation exclusivity as runtime-required; `replace_unique` preserves TypeHash |

Identity-aware lifecycle forms:

```cellscript
// Identity-aware creation
let minted = create_unique<Token>(identity = ckb_type_id) {
    amount: 100
} with_lock(recipient)

// Identity-aware replacement (consumes input, preserves identity)
let updated = replace_unique<Token>(identity = ckb_type_id) old {
    amount: old.amount - 50
}
```

`IrInstruction::CreateUnique` and `IrInstruction::ReplaceUnique` carry
identity metadata through the full compile pipeline. `TypeMetadata.identity_policy`
exposes the policy in compiled JSON metadata (hidden when `none`).

`replace_unique` has the syntax
`replace_unique<T>(identity = policy) input_cell { ... }`; the input operand is
required because the verifier compares the consumed Cell with the replacement
output. It does not take a `with_lock(...)` clause.

For `create_unique` policies, 0.15 emits local runtime anchors for the created
output and records the full global uniqueness proof as `runtime-required`.
For `ckb_type_id`, the remaining boundary is the TYPE_ID builder plan. For
field-, script-args-, and singleton-type creation, global uniqueness remains a
builder/indexer responsibility outside the CKB-VM execution scope.

### Explicit Destruction Policies

0.15 adds policy-specific destruction forms so the compiler and verifier know
*what is being proved*:

| Form | What it proves |
|------|---------------|
| `destroy_singleton_type(cell)` | No output with the same TypeHash exists |
| `destroy_unique(cell, identity = type_id)` | TYPE_ID continuation absence, lowered through the same output TypeHash scan |
| `destroy_instance(cell, identity_field = id)` | A field-identified instance destruction intent; full same-field output scan is runtime-required |
| `burn_amount(cell, field = amount)` | Quantity-delta burn intent; executable delta proof is runtime-required |

Bare `destroy cell` still compiles as `DestructionPolicy::Default`. In strict
mode it must be authorized by the 0.15 kernel effects `consume + burn` instead
of the legacy `has destroy` capability. Use a policy-specific form when the
audit needs to distinguish singleton absence, TYPE_ID consumption,
field-identified instance consumption, or amount burn.

`IrInstruction::Destroy` now carries `policy: IrDestructionPolicy` through
IR and codegen. Codegen only emits the legacy same-TypeHash absence scan for
singleton/type-id destruction policies; instance and amount policies are
reported as runtime-required instead of being over-constrained as singleton
absence.

### Kernel/Protocol Primitive Split

0.15 splits resource capabilities into kernel effects and protocol verbs.

New kernel-effect capabilities in `has ...` clauses:

```cellscript
resource Token has store, create, consume, replace, burn, relock, retarget_type, read_ref
```

These are context-sensitive identifiers: they are only treated as capability
keywords inside `has ...` clauses and remain ordinary identifiers elsewhere
(e.g., `action burn(token: Token)` compiles normally).

`Capability::is_protocol_verb()` and `Capability::kernel_effects()` classify
capabilities for migration tooling. `transfer` and `destroy` are protocol
verbs in 0.15; their effects decompose as:

```text
transfer  -> consume + create + relock (+ replace if lock changes)
destroy   -> consume + burn (or consume + assert_absence)
```

### Capability Vocabulary Reset and Compat/Strict Modes

0.15 provides a migration path from the v0.14 capability vocabulary:

**Compatibility mode** (`--primitive-compat=0.14`):
- accepts `has transfer` and `has destroy` without errors
- existing examples compile unchanged

**Strict mode** (`--primitive-strict=0.15`):
- rejects `has transfer` with diagnostic CS0150
- rejects `has destroy` with diagnostic CS0151
- requires kernel-effect capabilities and explicit destruction policies

Top-level single-file compilation accepts the same primitive migration flags as
package subcommands, so this works for direct example inspection:

```bash
cellc examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.15
```

Direct lifecycle checks accept the 0.15 kernel-effect equivalents:
`transfer` accepts `replace + relock`; `destroy` accepts `consume + burn`.

Migration diagnostics (CS0150–CS0160) provide old syntax, new syntax, and
affected proof obligations.

### Internal Metadata Renaming

Public metadata fields that previously used `type_hash` ambiguously are now
explicit about which CKB hash domain they refer to:

| Old name | New name |
|----------|----------|
| `type_hash-absence` | `ckb_type_script_hash-absence` |
| `type_hash-preservation` | `ckb_type_script_hash-preservation` |
| `lock_hash-preservation` | `ckb_lock_script_hash-preservation` |

### Protocol Macro Provenance

ProofPlan coverage records include macro provenance for selected
compiler-recognized flows such as `transfer`, `create`, `claim`, `settle`,
`consume`, `destroy`, and pool protocol metadata. This is audit metadata;
it is not a replacement for builder-backed CKB transaction evidence.

### Runtime-Obligation Policy Gate

`cellc check --deny-runtime-obligations` rejects runtime-required ProofPlan
gaps, including declared invariants whose coverage is still metadata-only.

## New Syntax Reference

### Type Declaration Identity

```cellscript
resource Token has store {
    identity(ckb_type_id)      // CKB TYPE_ID
    amount: u64
}

shared OracleData {
    identity(script_args)       // Script.args identity
    value: u64
}

resource NFT has store {
    identity(field(token_id))   // Field-based identity
    token_id: [u8; 32]
    owner: Address
}
```

Default is `identity none` (no tracking); backward compatible.

### Identity-Aware Lifecycle Forms

```cellscript
// create_unique — identity-aware cell creation
let token = create_unique<Token>(identity = ckb_type_id) {
    amount: 100
} with_lock(recipient)

// create_unique with a field identity
let nft = create_unique<NFT>(identity = field(token_id)) {
    token_id,
    owner
} with_lock(owner)

// replace_unique - identity-aware replacement (consumes input)
let updated = replace_unique<Token>(identity = ckb_type_id) token {
    amount: token.amount - 10
}

let moved = replace_unique<NFT>(identity = field(token_id)) nft {
    token_id: nft.token_id,
    owner: new_owner
}
```

### Destruction Policy Forms

```cellscript
// Prove no same-TypeHash output exists
destroy_singleton_type(token)

// Prove TYPE_ID identity is consumed (not replaced)
destroy_unique(token, identity = type_id)

// Prove a specific instance is consumed (allow other same-type outputs)
destroy_instance(token, identity_field = id)

// Prove quantity delta (burn)
burn_amount(token, field = amount)
```

### Capability Vocabulary

```cellscript
// v0.14 (compat mode: --primitive-compat=0.14)
resource Token has store, transfer, destroy { ... }

// v0.15 (strict mode: --primitive-strict=0.15)
resource Token has store, create, consume, replace, burn, relock { ... }
```

### Aggregate Invariant Syntax

```cellscript
invariant conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount

    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}

invariant no_duplicate_nft {
    trigger: type_group
    scope: transaction
    reads: outputs<NFT>.token_id

    assert_distinct(outputs<NFT>.token_id, scope = transaction)
}
```

## Intentional Boundaries

0.15 does not include:

- executable verifier lowering for aggregate invariants (metadata-only);
- automatic constraint placement between lock and type;
- covenant helper stdlib;
- Address/LockScript/LockHash type split;
- explicit CKB script role declarations;
- versioned cell data layout policies;
- removal of claim/receipt name heuristics;
- explicit mutation cardinality forms;
- `shared` as a scheduler policy library;
- global uniqueness proof for `create_unique(...)`; 0.15 emits local runtime
  anchors and reports the full uniqueness proof as runtime-required;
- full ProofPlan soundness checks (v0.16 scope).

## Verification

Targeted 0.15 gate:

```bash
cargo fmt --all
cargo check --locked -p cellscript
cargo test --locked -p cellscript proof_plan --lib -- --test-threads=1
cargo test --locked -p cellscript aggregate_invariant --lib -- --test-threads=1
cargo test --locked -p cellscript identity --lib -- --test-threads=1
cargo test --locked -p cellscript --test cli cellc_explain_proof -- --test-threads=1
cargo test --locked -p cellscript --test examples -- --test-threads=1
cargo test --locked -p cellscript --test v0_14 -- --test-threads=1
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cd editors/vscode-cellscript && npm run validate
git diff --check
```

Full release gate:

```bash
bash scripts/cellscript_ckb_release_gate.sh
bash scripts/ckb_cellscript_acceptance.sh
```

## Summary

CellScript 0.15 makes CKB safety boundaries explicit instead of hiding
lock/type differences. Scoped invariants declare when the verifier runs, what
it reads, and which cells it protects. Cell identity is now a first-class
primitive with `create_unique`/`replace_unique` lifecycle forms and runtime
identity anchors/preservation checks. Destruction policies make it explicit
whether you are proving output absence, identity continuation, or quantity
delta. The capability vocabulary has been reset from protocol verbs to kernel
effects, with a compat/strict migration path. Covenant ProofPlan metadata and
`cellc explain-proof` give auditors a complete
trigger/scope/reads/coverage/on-chain view.

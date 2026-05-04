# CellScript 0.13 Release Scope

**Updated**: 2026-05-03

0.13 is a closed implementation-scope release track. The stable release is
`v0.13.2`; this document explains what 0.13 includes, what it intentionally
leaves out, and where each subtopic is tracked in more detail.

For the broader plan, see [CellScript Roadmap](../../roadmap/CELLSCRIPT_ROADMAP.md).

## 0.13 Goals

0.13 has five concrete goals:

1. add executable stack-backed `Vec<T>` helper support for fixed-width values;
2. improve the source surface without changing core CKB semantics;
3. make verifier-boundary data sources visible with `protected`, `witness`,
   `lock_args`, and pure `require` constraints;
4. keep CKB production evidence strict enough to support release claims for the
   bundled suite;
5. keep stdlib and syntax sugar audit-visible by lowering them to canonical
   verifier effects and checking the parser/type/lowering/codegen combinations
   automatically.

## Status Summary

| Track | Status | Notes |
|---|---|---|
| Stack-backed `Vec<T>` helpers | Done | Covers fixed-width local vectors and helper matrix. |
| Contextual `Vec<T>` literals | Done | `[]` and `[x, y]` work only when the expected type is `Vec<T>`; empty `[]` lowers through the existing `Vec::new()` path. |
| Field shorthand | Done | `field` lowers as `field: field` for create and struct literals. |
| Example canonicalization | Done | Top-level `examples/*.cell` is the single checked-in bundled business source; language examples stay under `examples/language`. |
| Lock classification syntax | Done | `protected`, `witness`, fixed-width `lock_args`, and pure verifier-boundary `require` constraints are implemented and documented. |
| `lock_args` | Done | Fixed-width lock parameters are decoded from the executing script's `Script.args`; explicit signature verification is still deferred. |
| Stdlib lifecycle and Cell metadata patterns | Done | `std::lifecycle::transfer`, `std::receipt::claim`, `std::lifecycle::settle`, `std::cell::same_lock`, `std::cell::preserve_lock`, and `std::cell::preserve_capacity` lower to explicit verifier obligations. |
| Syntax-combination audit | Done | Quick and CI matrices exercise parser, formatter, type, lowering, metadata, codegen, and negative obsolete-syntax oracles. |
| Stateful business-flow acceptance | Done | The production CKB gate can run stateful scenarios that commit live outputs from earlier actions into later actions, then fill remaining action branches so all production acceptance actions appear in the stateful report. |
| Release gate wrapper | Done | `./scripts/cellscript_ckb_release_gate.sh full` is the release-facing gate and includes the syntax-combination CI matrix, builder-backed CKB acceptance, and stateful action coverage. |
| Explicit sighash verification | Deferred | Requires digest mode, script group scope, witness layout, and replay assumptions. |
| First-class signer values | Deferred | Must wait for explicit verification primitives. |
| Generic maps / cell-backed collections | Out of scope | Remain fail-closed until ownership semantics are executable. |

## Stack-Backed Collections

0.13 adds executable helper support for fixed-width stack-backed local vectors.

Implemented helper coverage:

- `Vec::new`
- `Vec::with_capacity`
- `Vec::capacity`
- `Vec::push`
- `Vec::extend_from_slice`
- `Vec::len`
- `Vec::is_empty`
- indexing
- `Vec::first`
- `Vec::last`
- `Vec::contains`
- `Vec::set`
- `Vec::remove`
- `Vec::pop`
- `Vec::insert`
- `Vec::reverse`
- `Vec::truncate`
- `Vec::swap`
- `Vec::clear`

Supported element categories:

- `u64`;
- fixed-byte values such as `Address` and `Hash`;
- fixed-width schema values covered by the fixed-width layout machinery.

Important boundaries:

- this is not full generic collection support;
- cell-backed linear collections remain fail-closed;
- generic maps and sets remain out of scope;
- `Option<T>` remains reserved for a later explicit optional/error model.

Detailed tracker:

- [0.13 release tracker](../../roadmap/CELLSCRIPT_0_13_TODOLIST.md)
- [Collections support matrix](../CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)

## Surface Syntax And Examples

0.13 completes the low-risk syntax pass from the surface elegance RFC.

Implemented:

- namespace-style bundled example modules;
- DSL-native `has` capability declarations;
- create and struct field shorthand;
- contextual `Vec<T>` literals;
- top-level `examples/*.cell` as the single checked-in bundled business source;
- production acceptance that compiles those canonical examples directly;
- `examples/language/registry.cell` for collection helper coverage;
- LSP and VS Code grammar/snippet updates.

Design boundary:

- the syntax pass must not hide Cell movement;
- examples must not imply signer authority from `Address` values;
- acceptance/profiled examples keep production metadata where evidence needs it.

Detailed design:

- [Surface elegance RFC](../CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
- [Wiki cookbook](../wiki/Cookbook-Recipes.md)

## Syntax Governance And Stdlib Patterns

0.13.2 closes the syntax-governance pass for lifecycle and local verifier
sugar. The stable rule is that sugar may shorten source, but it must not hide a
Cell effect or protocol-specific authorization rule.

Implemented:

- `std::lifecycle::transfer(input, output, to) { fields }` consumes `input`,
  creates the named output with `with_lock(to)`, preserves the listed data
  fields, and checks type continuity;
- `std::receipt::claim(receipt, output, lock) { fields }` consumes the receipt,
  creates the receipt-declared output type with the supplied lock, and
  preserves only the listed fields;
- `std::lifecycle::settle(input, output, lock) { fields }` uses the same
  consume-plus-named-output pattern for settlement-shaped protocols;
- `std::cell::same_lock`, `std::cell::preserve_lock`, and
  `std::cell::preserve_capacity` lower to canonical Cell metadata verifier
  checks;
- `preserve` sugar checks that preserved fields exist on both sides with
  matching field types, making it type-equivalent to its canonical `require`
  expansion;
- anonymous `require` blocks remain pure boolean proof syntax and reject
  lifecycle stdlib calls or other Cell effects.

Removed boundary:

- protocol-specific claim/signature behavior is not keyed off action names or
  compiler-internal string hooks.

Automated audit:

- [Syntax-combination audit methodology](../CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md)
- `./scripts/cellscript_syntax_combo_audit.sh quick`
- `./scripts/cellscript_syntax_combo_audit.sh ci`

## Lock Boundary Surface

0.13 adds classification syntax for locks:

```cellscript
lock owner_only(protected wallet: Wallet, witness claimed_owner: Address) -> bool {
    require wallet.owner == claimed_owner
}
```

Meaning:

- `protected T` is a typed view of one selected input Cell in the current script
  group whose spend is guarded by the lock invocation;
- `witness T` is decoded transaction witness data;
- `lock_args T` is typed data decoded from the executing lock script's
  `Script.args`;
- `require` fails current script validation when false and is allowed only as
  pure verifier-boundary constraint syntax, not as a lifecycle/effect block.

Important boundary:

- `witness Address` is not a signer;
- `Address` is not an authorization proof by name;
- `lock_args` binds fixed-width lock parameters to the executing script's args;
- hidden sighash defaults are rejected.

Deferred authorization roadmap:

1. explicit sighash verification primitive;
2. metadata/report obligations for signature verification;
3. first-class verified signer values;
4. optional `protects T { self ... }` sugar only after binding semantics are exact.

Detailed design:

- [Surface elegance RFC](../CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
- [CKB language audit](../CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)
- [CKB glossary](../wiki/CKB-Glossary.md)

## CKB Production Evidence

0.13 keeps the release boundary tied to builder-backed CKB evidence.

Required evidence for the bundled suite:

- syntax-combination CI matrix pass before CKB acceptance;
- strict CKB profile admission;
- scoped action compile and builder-backed action runs;
- scoped lock compile and builder-backed valid-spend / invalid-spend matrices;
- stateful local CKB scenario coverage for every production acceptance action;
- live-output handoff checks for the main bundled business flows;
- stable invalid-spend script failure evidence;
- valid transaction dry-runs and committed valid transactions;
- malformed rejection;
- measured cycles;
- consensus-serialized transaction size;
- occupied-capacity evidence;
- no under-capacity outputs;
- final production hardening gate pass.

Detailed evidence docs:

- [Metadata verification and production gates wiki](../wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md)
- [Capacity and builder contract](../CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [CKB language audit](../CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)

## Documentation And Tooling

0.13 documentation and tooling work includes:

- version-neutral GitHub Wiki tutorials;
- cookbook recipes and CKB glossary;
- standard-library tutorial covering stable lifecycle, Cell metadata,
  accounting, runtime, and collection helpers;
- rendered GitHub Wiki links instead of raw markdown links;
- LSP completions and VS Code grammar/snippets for new lock-boundary and
  stdlib syntax;
- release notes that separate 0.12 schema/ABI foundation from 0.13 executable
  collection helper work and record the 0.13.2 syntax-governance boundary.

Detailed docs:

- [GitHub Wiki](https://github.com/tsukifune-kosei/CellScript/wiki)
- [0.13.2 release notes](CELLSCRIPT_0_13_2_RELEASE_NOTES.md)

## Explicit Non-Goals

0.13 does not include:

- first-class signer or witness-sighash authorization syntax;
- hidden signer derivation from `Address`, witness data, or parameter names;
- hidden sighash defaults;
- `protects T { self ... }` sugar;
- full generic `HashMap<K, V>` or `HashSet<T>`;
- `Vec<Cell<T>>` or other cell-backed generic ownership collections;
- source-level `Option<T>` lowering;
- fully declarative capacity and since/header policy.

These are not accidental omissions. Each item either needs stronger CKB binding
semantics, stronger ownership semantics, or more release evidence before it
should be exposed as stable source syntax.

## Verification Commands

For normal pre-push checks:

```bash
./scripts/cellscript_ckb_release_gate.sh quick
```

For release-facing evidence:

```bash
./scripts/cellscript_ckb_release_gate.sh full
```

The full gate includes the compiler/tooling checks, syntax-combination CI
matrix, VS Code validation, docs boundary checks, builder-backed local CKB
acceptance, and stateful scenario/action coverage. The component commands
remain useful for focused debugging:

```bash
./scripts/cellscript_syntax_combo_audit.sh ci
cargo fmt --all --check
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo test --locked -p cellscript -- --test-threads=1
git diff --check
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
./scripts/cellscript_ckb_stateful_scenarios.sh
python3 scripts/validate_ckb_cellscript_production_evidence.py \
  target/ckb-cellscript-acceptance/<run>/ckb-cellscript-acceptance-report.json
```

The stateful section is intentionally stricter than a few happy-path flows:
the current production scope requires 7 end-to-end business scenarios, 20
stateful action-branch scenarios, 46 committed stateful steps, and 43/43
production acceptance actions covered with no missing action IDs.

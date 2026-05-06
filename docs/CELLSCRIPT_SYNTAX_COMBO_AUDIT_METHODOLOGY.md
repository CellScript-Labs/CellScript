# CellScript Syntax Combination Audit Methodology

This document defines a token-light, automatically executable audit method for
finding bugs caused by syntax combinations across parsing, formatting,
type/source checks, lowering, IR, metadata, codegen, and CKB-facing evidence.

The goal is aggressive coverage without producing huge logs. The audit runner
must be deterministic, self-shrinking, and summarized by compact machine-readable
reports.

## Principle

Audit syntax as a compiler pipeline contract:

```text
source
  -> lexer/parser
  -> formatter roundtrip
  -> resolver/type/source/effect checks
  -> lowering + optimizer
  -> IR invariants
  -> metadata/constraints
  -> codegen/assembler/ELF
  -> selected CKB acceptance
```

A syntax feature is safe only if every layer either:

1. accepts it and records the expected obligations; or
2. rejects it with a stable, precise diagnostic.

Silent acceptance with missing obligations is the highest-risk failure mode.

## Bug Classes To Hunt

| Layer | High-value bug patterns |
|---|---|
| Lexer/parser | Keyword/identifier ambiguity, newline-sensitive parse drift, precedence bugs, block boundary bugs, accepted obsolete syntax. |
| Formatter | Non-idempotent output, AST-changing formatting, dropped source qualifiers, block flattening that changes proof scope. |
| Resolver/type checker | Source qualifier escape, lifecycle op in pure context, linear value leak, branch merge unsoundness, field type mismatch, stale capability gates. |
| Stdlib patterns | Unknown `std::...` accepted, lifecycle pattern hidden in `require`, missing consume, duplicate consume, missing locked output, missing preserved field coverage. |
| Lowering/IR | Missing verifier instruction, duplicate runtime access, use-before-def, bad block terminator, stale protocol-name recognizer, wrong source span. |
| Metadata | Overclaiming production support, missing runtime blocker, missing source/ABI/schema obligation, drift from IR effects. |
| Codegen | Unsupported mnemonic, register clobber, large immediate bug, branch relaxation bug, stack offset bug, syscall arg order bug, unregistered fail code. |
| CKB evidence | Compiler success without builder evidence, invalid lock spend accepted, valid spend rejected, capacity/cycles/tx-size evidence missing. |

## Combination Axes

The audit generator should compose features from these axes. Use pairwise
coverage for quick mode, triplewise for CI/deep mode, and curated adversarial
seeds for known dangerous interactions.

| Axis | Values |
|---|---|
| Entry kind | `fn`, `action`, `lock` |
| Binding source | ordinary param, Cell input, named output, `read`, `protected`, `witness`, `lock_args`, `read_ref<T>()` |
| Cell kind | `resource`, `receipt`, `receipt -> Output`, `shared`, plain `struct` |
| Lifecycle | none, `consume`, `destroy`, stdlib transfer, stdlib claim, stdlib settle |
| Proof syntax | atomic `require`, anonymous `require {}`, `assert`, branch-local require, helper `fn` call |
| Continuity | explicit field `require`, `preserve`, `std::cell::*`, `std::accounting::conserved` |
| Control flow | straight-line, `if`, nested `if`, `match`, loop, early return, block tail expression |
| Data shape | scalar, bool/u32/u64/u128, fixed bytes, tuple, enum, fixed struct, dynamic Molecule field |
| Collection | none, `Vec<T>` literal, `Vec` helper, unsupported generic map, cell-backed collection attempt |
| Profile | default, `--target-profile ckb`, `--production`, fail-closed unsupported profile |
| Backend | check only, assembly, ELF, metadata/constraints, internal assembler, CKB acceptance subset |

## Automation Contract

The audit must run without an LLM in the loop.

Recommended command surface:

```bash
scripts/cellscript_syntax_combo_audit.sh quick
scripts/cellscript_syntax_combo_audit.sh ci
scripts/cellscript_syntax_combo_audit.sh deep --seed 20260503 --budget 5000
```

The repository includes the first executable runner:

```text
scripts/cellscript_syntax_combo_audit.sh
scripts/cellscript_syntax_combo_audit.py
tests/syntax_combo/matrix.toml
tests/syntax_combo/seeds/*.cell
```

## Reuse Contract

The audit is reusable by design. It is not tied to one release candidate or one
bug class.

To reuse it for a new syntax feature:

1. add or extend one matrix origin in `tests/syntax_combo/matrix.toml`;
2. add at least one minimized seed under `tests/syntax_combo/seeds/` for the
   riskiest accepted or rejected shape;
3. update the mode contract if the new origin should be required in `quick`,
   `ci`, or `deep`;
4. run `scripts/cellscript_syntax_combo_audit.sh quick` while developing;
5. run `scripts/cellscript_syntax_combo_audit.sh ci` before merge or release;
6. keep reports under `target/syntax-combo-audit/` instead of pasting artifacts
   into review threads.

The reusable unit is a compact case plus its expected pipeline result, not a
large log. A failing case should become either a seed or a matrix origin so the
same class stays covered without an LLM in the loop.

`quick` mode runs the minimal deterministic corpus plus regression seeds.
`ci` adds matrix-generated stdlib lifecycle, proof-syntax, metadata-helper, and
lock-source qualifier combinations. `deep` adds higher-risk reject mutations for
release-local replay before expensive CKB-node execution.
Each mode has a contract in `tests/syntax_combo/matrix.toml`: minimum generated,
accepted, and rejected case counts plus required origin families. A mode fails
closed if a budget/configuration change drops coverage below that contract.
The report also emits:

- `governance_release_matrix`: the grammar-governance tracks covered by this
  run;
- `governance_oracles`: the enabled parser, formatter, type/effect,
  metadata, codegen, and compact-report oracles;
- `known_bug_classes`: high-risk historical bug classes and the concrete cases
  or origins that keep each class covered.

`quick`, `ci`, and `deep` have escalating required bug classes. Losing a
required case such as stdlib locked-output lowering, preserve type equivalence,
require-block purity, stdlib argument validation, or
deep hidden-lifecycle rejection fails the audit before release wording can be
updated.

## Acceptance Integration

The syntax-combination audit is a release acceptance preflight. It runs before
builder-backed CKB acceptance in `scripts/cellscript_ckb_release_gate.sh full`.

Keep this layering strict:

- `scripts/cellscript_syntax_combo_audit.sh quick` is the local smoke gate;
- `scripts/cellscript_syntax_combo_audit.sh ci` is the merge/release syntax
  gate;
- `scripts/cellscript_syntax_combo_audit.sh deep` is the production-release
  replay for higher-risk mutations;
- `scripts/ckb_cellscript_acceptance.sh --production` remains the chain-evidence
  component;
- `scripts/cellscript_ckb_release_gate.sh full` is the acceptance-standard
  wrapper that requires syntax-combination CI, syntax-combination deep replay,
  and builder-backed CKB evidence.

Do not treat a passing CKB acceptance run as a substitute for a failed
syntax-combination audit. CKB evidence proves selected concrete transactions;
the syntax-combination audit proves the compiler pipeline does not silently
accept dangerous source combinations with missing verifier obligations.

Recommended repository layout:

```text
tests/syntax_combo/
  matrix.toml              # feature axes, legal/illegal combinations
  seeds/*.cell             # hand-written adversarial seeds
  expected_failures.toml   # stable diagnostics for intentionally bad cases
  generated/               # ignored generated cases
  reports/                 # ignored compact JSON/JSONL reports
```

The runner should:

1. generate `.cell` cases from the matrix;
2. run formatter roundtrip;
3. run `cellc check` for accepted and rejected profiles;
4. run metadata/constraints checks for accepted cases;
5. compile selected accepted cases to assembly and ELF;
6. run internal assembler surface checks;
7. run selected CKB acceptance only for high-risk minimized cases;
8. shrink failing cases before reporting;
9. write one compact report.

## Modes And Budgets

| Mode | Purpose | Budget |
|---|---|---|
| `quick` | local pre-commit smoke | <= 100 generated cases, <= 60s, pairwise axes, no CKB node |
| `ci` | PR gate | <= 1,000 generated cases, pairwise + curated triples, assembly/ELF for accepted cases |
| `deep` | release audit | 5,000+ generated cases, triplewise, mutation fuzz, selected CKB acceptance |
| `repro` | one failure replay | one seed/case id, full artifacts, no generation |

Deep mode may run for a long time, but report output must stay small.

## Token-Light Output Rule

Never dump full source, IR, assembly, or metadata to stdout by default.

Stdout should be at most:

```text
syntax-combo-audit: failed
seed=20260503 mode=ci generated=1000 accepted=612 rejected=388 failures=2
report=target/syntax-combo-audit/20260503/report.json
top:
  SCA-IR-001 duplicate consume after std::lifecycle::transfer case=7fa2b1
  SCA-TY-004 require block accepted lifecycle stdlib call case=81c03e
```

Full artifacts stay on disk:

```text
target/syntax-combo-audit/<run>/
  cases/<case-id>.cell
  shrink/<case-id>.cell
  ir/<case-id>.json
  meta/<case-id>.json
  asm/<case-id>.s
  report.json
  report.jsonl
```

Report entries should be JSONL with stable keys:

```json
{"case":"7fa2b1","phase":"ir","status":"fail","code":"SCA-IR-001","summary":"duplicate consume for coin","shrunk":"shrink/7fa2b1.cell"}
```

## Oracles

Use multiple cheap oracles before expensive execution.

### Parser Oracle

- Accepted syntax must parse to AST.
- Rejected syntax must fail with a stable diagnostic substring.
- Obsolete core forms, such as expression-level `transfer`, `claim`, and
  `settle`, must not reappear as accepted AST nodes.

### Formatter Oracle

For every accepted source:

```text
parse(source) == parse(fmt(source))
fmt(fmt(source)) == fmt(source)
```

Reject any formatting pass that changes source qualifiers, `verification`
section boundaries, `require` block boundaries, `preserve` fields, or stdlib
field whitelists.

### Type/Effect Oracle

Every case gets an expected classification:

```text
accept
reject_parse
reject_type
reject_policy
reject_codegen
```

For accepted cases:

- no linear input may silently disappear;
- lifecycle operations are forbidden in `fn`, pure contexts, and `require {}`;
- `read`, `protected`, `witness`, and `lock_args` cannot be consumed/destroyed;
- `preserve` compares existing fields with matching types;
- stdlib lifecycle patterns consume exactly one intended input.

### Lowering/IR Oracle

For accepted cases, validate structural invariants:

- all variables are defined before use;
- every block has one terminator;
- every lifecycle input appears exactly once in `consume_set`;
- named output creates match declared output bindings;
- stdlib transfer/claim/settle emits locked output constraints;
- Cell metadata helpers emit metadata equality instructions or canonical
  verifier obligations;
- no obsolete protocol-name IR instructions exist.

### Metadata Oracle

Metadata must be a faithful summary, not a marketing surface:

- source hash and compiler version match;
- effects in metadata match IR effects;
- runtime blockers are present for unsupported shapes;
- production flags do not appear for compile-only evidence;
- constraints expose named outputs, preserved fields, cell metadata checks, and
  CKB profile obligations.

### Codegen Oracle

For assembly/ELF cases:

- generated mnemonics are declared in the internal assembler surface;
- unregistered numeric fail codes are forbidden;
- branch targets resolve;
- large immediates and stack offsets go through known-safe helpers;
- generated ELF is non-empty;
- metadata artifact hash/size matches the output.

### CKB Acceptance Oracle

Do not run full CKB for every generated case. Select minimized failures or
release-critical combinations:

- lock source qualifiers + witness/lock_args;
- lifecycle stdlib pattern + locked output;
- capacity/lock/type metadata helpers;
- dynamic schema field + output verification;
- valid/invalid lock spend matrix deltas.

## Generator Strategy

Use a weighted grammar, not unconstrained random strings.

1. Start from minimal templates that compile.
2. Apply one feature from each chosen axis.
3. Apply one adversarial mutation:
   - transition syntax into wrong scope;
   - swap source qualifier;
   - omit one preserved field;
   - change one field type;
   - wrap lifecycle in `require {}`;
   - put Cell op behind helper `fn`;
   - add branch asymmetry;
   - use unsupported collection shape.
4. Compute expected outcome from matrix rules.
5. Run pipeline.
6. Shrink on failure.

Prefer deterministic seeds:

```text
case_id = blake2b(seed, axis_values, mutation, template_version)[0..12]
```

## Shrinking Rules

A failing case is useful only if it is small.

Shrink in this order:

1. remove unused types;
2. remove unused fields;
3. remove unrelated actions/locks;
4. simplify expressions to identifiers/literals;
5. reduce branches to one decisive branch;
6. reduce field whitelist to the minimal failing fields;
7. reduce profile/backend to the earliest failing phase.

The report should show only the shrunk case path. Full original cases stay on
disk for replay.

## Differential Checks

Run cheap differential checks to catch semantic drift:

| Check | Catches |
|---|---|
| sugar vs canonical source | `preserve` or `require {}` lowering drift |
| stdlib pattern vs explicit expansion | missing consume/create/lock/preserve obligations |
| fmt source vs original source | formatter changes semantics |
| check vs build metadata | metadata missing after codegen path |
| assembly vs ELF metadata | artifact hash/size mismatch |
| old accepted syntax corpus vs current compiler | accidental compatibility with removed syntax |

Example stdlib differential pair:

```cellscript
std::lifecycle::transfer(coin, next_coin, to) {
  amount
  nonce
}
```

must match the obligations of:

```cellscript
consume coin
create next_coin = Coin { amount: coin.amount, nonce: coin.nonce } with_lock(to)
std::cell::preserve_type(next_coin, coin)
```

## Regression Corpus

Every real bug becomes a permanent seed:

```text
tests/syntax_combo/seeds/<bug-code>-<short-name>.cell
```

Each seed must record:

```text
// audit: phase=reject_compile
// audit: contains=require block
// audit: contains=verifier-boundary syntax
```

The runner should fail if a seed changes behavior without an explicit update to
`expected_failures.toml`.

## Release Gate Policy

Before a stable release:

```bash
scripts/cellscript_syntax_combo_audit.sh ci
scripts/cellscript_syntax_combo_audit.sh deep
./scripts/cellscript_ckb_release_gate.sh full
```

The release is blocked if any of these are true:

- generated accepted syntax has missing IR/metadata obligations;
- rejected syntax is accepted without a stable diagnostic;
- formatter changes AST;
- codegen emits unsupported assembly;
- metadata overclaims production readiness;
- a known regression seed changes behavior unintentionally;
- a high-risk minimized case fails CKB acceptance.

## Minimal Report Schema

The final JSON report should be small enough for review tools:

```json
{
  "status": "passed",
  "mode": "ci",
  "seed": 20260503,
  "generated": 1000,
  "accepted": 612,
  "rejected": 388,
  "failures_count": 0,
  "governance_release_matrix": [
    {
      "track": "stdlib_lifecycle_patterns",
      "status": "covered_by_gate",
      "gate": "syntax-combo stdlib lifecycle metadata oracles"
    }
  ],
  "known_bug_classes": [
    {
      "id": "SCA-BUG-STD-LIFECYCLE-LOCKED-OUTPUT",
      "status": "covered",
      "required": true
    }
  ],
  "coverage": {
    "pairwise": 1.0,
    "triplewise_sampled": 0.42
  },
  "phases": {
    "parse": {"passed": 1000, "failed": 0},
    "fmt": {"passed": 612, "failed": 0},
    "type": {"passed": 1000, "failed": 0},
    "ir": {"passed": 612, "failed": 0},
    "metadata": {"passed": 612, "failed": 0},
    "codegen": {"passed": 120, "failed": 0}
  },
  "failures": []
}
```

If there are failures, include only top N by default:

```json
{"failures":[{"case":"7fa2b1","phase":"ir","code":"SCA-IR-001","summary":"duplicate consume","shrunk":"shrink/7fa2b1.cell"}]}
```

## Implementation Checklist

The first implementation should be intentionally narrow:

1. build `matrix.toml` with the axes in this document;
2. generate pairwise cases for `action`/`lock`/`fn`, source qualifiers,
   lifecycle, `require`, `preserve`, and stdlib patterns;
3. implement parser/type/fmt/IR/metadata oracles;
4. add codegen oracle for a sampled accepted subset;
5. add shrinker for line/block/type removal;
6. store full artifacts under `target/syntax-combo-audit`;
7. print only compact summaries;
8. add CI mode after quick mode is stable.

Do not start with random fuzz alone. Start with matrix-driven generation, then
add random mutation after the deterministic corpus is reliable.

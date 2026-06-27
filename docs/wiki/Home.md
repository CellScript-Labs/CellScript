CellScript is a small language for writing Cell-based contracts on CKB. You
describe the Cells your protocol cares about, the actions that move those Cells,
and the locks that decide whether a Cell may be spent. The compiler then turns
that `.cell` source into ckb-vm compatible RISC-V assembly or ELF artifacts, and
writes metadata that explains what was built.

Last updated: 2026-06-22.

This wiki is a guided path. It starts with one compiled example, then slowly
builds the mental model: source files, Cell effects, packages, the CKB profile,
metadata, tooling, and finally the bundled examples. You do not need to
understand every production gate on the first read. The important thing is to
learn what each layer proves, and what it does not prove yet.

## How to Read This Wiki

If CellScript is new to you, read the tutorials in order. The first four
language chapters explain how a `.cell` file is shaped, how resources move, why
effects such as `consume` and `create` are explicit, and how the canonical
action model expresses input-to-output verification with `transition` and
`verification`.
The v0.15 material then extends that model with identity policies, scoped
invariants, ProofPlan metadata, and primitive capability boundaries.

After that, the wiki continues outward:

- packages make builds repeatable;
- the CKB profile chooses the chain-facing runtime rules;
- metadata explains the artifact;
- v0.16 assurance commands explain ProofPlan soundness and builder assumptions;
- production evidence proves more than compiler success;
- editor tooling shortens the local loop;
- bundled examples show the style in real contracts.

If you already know what you need, jump directly:

- writing source: start with [Language Basics](Tutorial-02-Language-Basics.md);
- understanding Cell movement: read [Resources and Cell Effects](Tutorial-03-Resources-and-Cell-Effects.md);
- understanding actions: read [Action Model and Canonical Syntax](Tutorial-09-Action-Model-and-0-13-Syntax.md);
- using stdlib patterns: read [Standard Library](Tutorial-10-Standard-Library.md);
- copying a known pattern: use [Cookbook Recipes](Cookbook-Recipes.md);
- checking CKB terms: keep [CKB Glossary](CKB-Glossary.md) nearby;
- building a package: use [Packages and CLI Workflow](Tutorial-04-Packages-and-CLI-Workflow.md);
- compiling for CKB: read [CKB Target Profiles](Tutorial-05-CKB-Target-Profiles.md);
- preparing evidence: use [Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md);
- working in an editor: read [LSP and Tooling](Tutorial-07-LSP-and-Tooling.md);
- learning by example: finish with [Bundled Example Contracts](Tutorial-08-Bundled-Example-Contracts.md);
- driving `cellc` from an agent: read [Agentic Loops and cellc-mcp](Tutorial-13-Agentic-Loops-and-cellc-mcp.md).

## Tutorial Path

1. [Getting Started](Tutorial-01-Getting-Started.md): compile one example and
   verify its artifact.
2. [Language Basics](Tutorial-02-Language-Basics.md): learn the shape of a
   `.cell` file.
3. [Resources and Cell Effects](Tutorial-03-Resources-and-Cell-Effects.md):
   understand how values move through a Cell transaction.
4. [Action Model and Canonical Syntax](Tutorial-09-Action-Model-and-0-13-Syntax.md):
   learn the signature-direction action model, `verification`, `transition`,
   named outputs, and source qualifiers.
5. [Standard Library](Tutorial-10-Standard-Library.md):
   use stdlib lifecycle, Cell metadata, accounting, runtime, and collection
   helpers without hiding verifier obligations.
6. [Scoped Invariants and ProofPlan](Tutorial-11-Scoped-Invariants-and-ProofPlan.md):
   inspect 0.15 invariant trigger/scope/read metadata and understand
   metadata-only ProofPlan gaps.
7. [Cookbook Recipes](Cookbook-Recipes.md): copy small patterns once the basic
   vocabulary is familiar.
8. [Packages and CLI Workflow](Tutorial-04-Packages-and-CLI-Workflow.md):
   create a package, build it, check it, and inspect reports.
9. [CKB Target Profiles](Tutorial-05-CKB-Target-Profiles.md): choose the CKB
   runtime assumptions before compiling.
10. [Metadata, Verification, and Production Gates](Tutorial-06-Metadata-Verification-and-Production-Gates.md):
   learn what artifact verification proves, and what still needs chain
   evidence.
11. [LSP and Tooling](Tutorial-07-LSP-and-Tooling.md): use editor feedback and
   command-backed reports.
12. [Bundled Example Contracts](Tutorial-08-Bundled-Example-Contracts.md): study
   the examples in a useful order.
13. [Agentic Loops and cellc-mcp](Tutorial-13-Agentic-Loops-and-cellc-mcp.md):
   drive the read-only compiler surface from an automated writer in a
   write → check → explain → fix loop.

## The Core Idea

CellScript tries to keep the CKB model visible. A contract should not look like
an account database if it is really spending input Cells and creating output
Cells.

That is why the language has:

- `resource`, `shared`, and `receipt` for persistent Cell-backed values;
- explicit effects such as `consume`, `create`, action-boundary `read`
  parameters, expression-level `read_ref<T>()`, `destroy`, `claim`, and
  `settle`;
- compiler-recognized stdlib lifecycle patterns such as
  `std::lifecycle::transfer`, `std::receipt::claim`, and
  `std::lifecycle::settle`;
- identity-aware lifecycle forms such as `create_unique` and `replace_unique`;
- scoped `invariant` declarations with explicit trigger, scope, and reads;
- `action` entries for type-script style state transitions;
- `lock` entries for spend-boundary predicates;
- `protected`, `witness`, `lock_args`, and `require` so verifier-boundary source
  data and failure points are visible in source;
- metadata sidecars and ProofPlan records that describe schema, ABI,
  constraints, runtime requirements, and verifier obligations.
- builder assumption records and schema-bound transaction-shape validation for
  pre-signing review.

The wiki uses the same rule throughout: if something is only compiler evidence,
it is described as compiler evidence. If something needs a builder-backed CKB
transaction, the wiki says so.

## First Run

The fastest way to get oriented is to compile the token example:

```bash
git clone https://github.com/CellScript-Labs/CellScript.git
cd CellScript
./scripts/cellscript_gate.sh dev
cargo run --locked --bin cellc -- examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.16 -o /tmp/token.elf
cargo run --locked --bin cellc -- verify-artifact /tmp/token.elf --expect-target-profile ckb
```

The compile step writes two files:

```text
/tmp/token.elf
/tmp/token.elf.meta.json
```

The ELF is the executable artifact. The metadata sidecar is the explanation:
where the source came from, which profile was used, what schema was produced,
and which obligations still need review.

## Before You Call It Production

`cellc verify-artifact` is an important first check, but it is not the whole
story. It proves that an artifact and its metadata agree. It does not prove that
a concrete CKB transaction can spend the right inputs, serialize the right
witness, fit capacity rules, pass dry-run, and commit.

Keep two levels separate:

- compiler evidence: source, artifact, metadata, and selected policy flags
  agree;
- CKB chain evidence: builder-generated transactions were checked on a local CKB
  chain with cycles, transaction size, capacity, and positive/negative behavior
  evidence.

Release-facing CKB evidence comes from the repository root:

```bash
./scripts/cellscript_gate.sh release
```

The bundled examples are covered by the current local production evidence suite.
The NovaSeal core, Agreement, six planned NovaSeal profiles, and Evolving DOB
profile now have current local devnet/source-package readiness evidence. Public
or mainnet deployment claims still need their own CellDep, verifier TCB, BTC
SPV, RWA/legal, or other external attestations where a profile depends on those
facts.
The 0.16.1 patch line also closes the token/AMM/launch and NFT first-cell
bootstrap examples used by external builders.
New external contracts still need their own metadata review, builder evidence,
security review, and chain acceptance evidence before they should be called
production-ready.

## Reference Examples

- [CKB hashing workflow](https://github.com/CellScript-Labs/CellScript/blob/main/docs/examples/ckb_hashing.md)
- [Collections matrix](https://github.com/CellScript-Labs/CellScript/blob/main/docs/examples/collections_matrix.md)
- [Deployment manifest](https://github.com/CellScript-Labs/CellScript/blob/main/docs/examples/deployment_manifest.md)
- [Output append](https://github.com/CellScript-Labs/CellScript/blob/main/docs/examples/output_append.md)

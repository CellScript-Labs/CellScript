# CellScript Roadmap

**Updated**: 2026-05-03

This roadmap is the high-level planning map for CellScript. It links the
release-specific trackers and the deeper design notes so the project does not
split into unrelated TODO files.

The current project direction is simple:

1. keep the CKB Cell model visible in the language;
2. keep release claims tied to compiler evidence and builder-backed CKB
   evidence;
3. make the language surface easier to teach without hiding authorization,
   capacity, witness, or lock-group boundaries;
4. keep syntax sugar audit-visible by requiring parser, formatter, type,
   lowering, metadata, codegen, docs, and automated syntax-combination gates to
   agree before release.

## Current State

| Area | Current status | Detailed document |
|---|---|---|
| 0.13 release scope | Implementation scope is closed for the `v0.13.2` stable release; the full gate now includes stateful business-flow/action coverage. | [0.13 release scope](CELLSCRIPT_0_13_RELEASE_SCOPE.md), [0.13 release tracker](CELLSCRIPT_0_13_TODOLIST.md), [0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md) |
| CKB language fit | CKB-first design is confirmed; remaining gaps are signer binding, continuity policy, capacity policy, and declarative time policy. | [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md) |
| Surface syntax | Low-risk syntax pass and 0.13.2 syntax-governance hardening are implemented; authority-sensitive syntax remains staged. | [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md), [Syntax-combination audit](../docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md), [0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md) |
| Collections | Stack-backed fixed-width `Vec<T>` helper surface is implemented; cell-backed and generic map ownership remain fail-closed. | [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md), [0.13 release scope](CELLSCRIPT_0_13_RELEASE_SCOPE.md) |
| CKB production evidence | Bundled actions and locks have builder-backed local CKB evidence; full release claims also require stateful coverage for every production acceptance action. | [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md) |
| Documentation and wiki | Wiki is version-neutral, cookbook-oriented, includes a standard-library chapter, and is published separately to GitHub Wiki. | [GitHub Wiki](https://github.com/tsukifune-kosei/CellScript/wiki) |

## Release Tracks

### 0.13: Closed Implementation Scope

0.13 is the current release gate. Its closed implementation scope now covers:

- executable stack-backed `Vec<T>` helper support for fixed-width values;
- low-risk surface syntax improvements and cleaner example organization;
- CKB lock-boundary classification with `protected`, `witness`, and `require`;
- 0.13.2 stdlib lifecycle/cell metadata patterns that lower to explicit
  verifier effects instead of core protocol-name magic;
- automated syntax-combination audit coverage for parser, formatter, type,
  lowering, metadata, codegen, and release-gate contracts.
- full release-gate stateful evidence: 7 end-to-end business scenarios plus
  action-branch coverage for all 43 production acceptance actions.

0.13 deliberately does not introduce hidden signer authority, hidden sighash
defaults, full generic maps, or cell-backed collection ownership.

Detailed status:

- [0.13 release scope](CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [0.13 release tracker](CELLSCRIPT_0_13_TODOLIST.md)
- [0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md)
- [Syntax-combination audit methodology](../docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md)

### Next Authorization Hardening Track

The next security-sensitive track should make CKB authorization literal before
it becomes ergonomic.

Fixed-width `lock_args` binding to the executing script args landed in the
0.13 line. Remaining planned order:

1. explicit sighash verification primitive with digest mode, script group scope,
   witness layout, and replay assumptions;
2. stable metadata and report fields for signature verification obligations;
3. first-class verified signer values only after explicit primitives are proven;
4. optional `protects T { self ... }` sugar only after protected-input
   selection and lock-group aggregation semantics are exact.

Non-goals:

- no implicit signer derivation from `Address`;
- no hidden sighash defaults;
- no parameter-name-based authority.

Source documents:

- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)

### CKB Evidence Hardening Track

The CKB acceptance surface should continue moving from broad acceptance evidence
to predicate-specific evidence.

Priorities:

- keep action acceptance builder-backed and report-validated;
- keep lock valid-spend and invalid-spend matrices mandatory for bundled locks;
- require invalid-spend cases to match stable script failure paths, not generic
  transaction rejection;
- keep cycles, serialized transaction size, occupied capacity, and malformed
  rejection evidence in reports;
- keep stateful business-flow/action coverage mandatory for full releases;
- extend the matrix when new bundled locks enter production scope.

Source documents:

- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)
- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md)

### Collections And Ownership Track

The collections roadmap stays conservative because CKB Cell ownership is not a
generic heap model.

Completed:

- stack-backed fixed-width `Vec<T>` helper support;
- typed/contextual `Vec<T>` literals for local stack vectors;
- metadata and `cellc explain-generics` visibility for checked instantiations.

Deferred:

- full generic `HashMap<K, V>` and `HashSet<T>`;
- `Vec<Cell<T>>` and other cell-backed linear ownership collections;
- source-level `Option<T>` lowering;
- explicit `Vec<T, N>[...]` bounded-vector literal syntax.

Source documents:

- [0.13 release scope](CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)
- [Linear ownership](../docs/CELLSCRIPT_LINEAR_OWNERSHIP.md)

### Declarative CKB Policy Track

Some CKB facts are currently visible in metadata and builder evidence rather than
first-class source policy.

Future work:

- declarative capacity requirements where the compiler can check them;
- declarative since/header/timepoint assumptions for timelock-like protocols;
- explicit continuity policy for signature-directed input/output Cell updates, including type id,
  lock, data schema, and capacity continuity;
- clearer builder obligations in action builder plans.

Source documents:

- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Output bindings](../docs/CELLSCRIPT_OUTPUT_BINDINGS.md)
- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)

### Documentation And Developer Experience Track

The docs should stay useful to new readers and strict enough for reviewers.

Completed:

- GitHub Wiki is version-neutral and cookbook-oriented;
- `_Sidebar.md` gives a book-like navigation structure;
- cookbook recipes and CKB glossary exist;
- LSP and VS Code grammar/snippets cover the new lock-boundary syntax.

Future work:

- keep wiki links rendered through GitHub Wiki URLs;
- add recipes when new stable language patterns land;
- keep release notes in `docs/releases/` and roadmap files in `roadmap/`, separate from tutorial pages;
- keep examples split by audience: business, language, and acceptance.

Source documents:

- [GitHub Wiki](https://github.com/tsukifune-kosei/CellScript/wiki)
- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)

## Roadmap Discipline

Roadmap entries should follow these rules:

- completed work must point to tests, release notes, or evidence reports;
- deferred work must say why it is deferred;
- security-sensitive syntax must distinguish data source from authority;
- CKB production claims must distinguish compiler evidence from chain evidence;
- wiki pages should teach the current stable surface, not act as release notes.

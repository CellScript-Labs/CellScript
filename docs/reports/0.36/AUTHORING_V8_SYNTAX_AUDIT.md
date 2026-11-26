# CellScript authoring-v8 syntax audit

Audited on 2026-11-26 against [`e134d663` on `0.36`](https://github.com/CellScript-Labs/CellScript/commit/e134d663de9e7c226af5c4691648e86bc3a283ce), Edition 2027 authoring-v8. Native tests used a detached checkout and Rust 1.97.1. Browser and editor tests used the submodule pins in that commit. The finance comparison starts at [`e66d9cc5`](https://github.com/CellScript-Labs/CellScript/commit/e66d9cc55e98ca95b8946b8ec68df872c64fdd2d), after the Pool swap refactor and before the entry and lifecycle syntax changes.

## Findings at a glance

- **The pinned Playground admits source the native compiler rejects.** Both report authoring-v8. The browser accepts a `flow` edge without its required semicolon. The existing browser tests pass and miss this case.
- **The origin of a schema changes named-field admission.** Both `fresh` and `pool` require declaration order for local fields and accept a reversed list for the same schema imported from another module, through an alias or through a locked package dependency. Both still reject the tested missing, duplicate and unknown fields.
- **Some proposed deletions are well supported.** Three tested variations of explicit entry roles produce identical ELF bytes. A field-preservation shorthand also has a direct lowering to ordinary equality checks. Conditional lifecycle placement, Lock entry selection and identity-preserving replacement need separate decisions because they change the programs authors can express or the transactions those programs admit.
- **The tested DeFi composition path keeps its existing cost to authors and builders.** The ten checked package ELFs match the pre-convergence bytes. The Pool retains its nine-field entry contract, full ABI report and sample witness bytes for all nine modes. See [finance evidence](#defi-and-composition-evidence).

## Audit matrix

P0 denotes a reproduced disagreement between implementations or documented admission rules. P1 denotes a language design decision that affects executable composition or the strength of a guarantee. P2 denotes an ergonomic issue or a diagnostic gap. Evidence labels distinguish direct reproduction, regression suites and source review.

| ID | Area | Finding or retained distinction | Evidence | Proposed decision |
| --- | --- | --- | --- | --- |
| A01 · P0 | Native, LSP and Playground | The pinned browser bundle accepts a missing final `flow` semicolon. Native rejects it. Both advertise authoring-v8. The pinned bundle also gives the old generic error for a nested `retire`. | Direct native and shipped WASM probes | Pair the pinned bundle with the compiler. Run shared positive and negative fixtures against the distributed bytes, including fixes within the same source identity. |
| A02 · P0 | Named policy fields | Local `fresh` and `pool` require source order to match the schema. Imported and aliased schemas bypass the order check. Completeness and field identity still matter in every case. | 40 direct schema and field combinations | Validate names and completeness against the resolved schema in one phase. Decide field evaluation order separately from encoded layout. |
| A03 · P0 | `fresh` output coordinates | The grammar describes a Type group output. The ordinary and action-defined artifacts can bind transaction `output[0]`. A fixed group artifact binds `group-output[0]`. | Direct typed records and independent artifact checks | Document the resolved role. Show its physical coordinate and coverage in `expand` and tooltips. |
| A04 · P1 | Deep syntax-combination coverage | Three seeded cases still generate the retired `verification` marker. Parsing stops before transfer, Cell helper and intended rejection checks. | Deep runner and retained reproductions | Update the generators and their phase oracles. Keep coverage that distinguishes parser, compiler and runtime rejection. |
| A05 · P1 | Entry roles and output signatures | Prefix and postfix `witness` are equivalent in the tested action. A named arrow and inline `output` also agree in that action. The anonymous `-> Type` return has its own semantics. | Direct exact-byte comparison and ABI tests | Use role prefixes and inline Cell outputs in entry signatures. Keep helper and ordinary value returns. Migrate implicit sources only after resolving the current role. |
| A06 · P1 | `lock` and `lock_script` | Ordinary `lock` can be followed by a helper. `lock_script` must be the sole entry and the final declaration. The current artifact context enum admits Type groups. | Direct placement probe and source review | Add an explicit selectable Lock artifact contract before converging on `lock`. Preserve protected input, Lock args, Boolean result and witness placement. |
| A07 · P1 | Conditional lifecycle | `replace` composes in checked branches. `fresh`, `retire` and `pool` are entry policies. A nested `retire` is rejected before type checking. | Direct placement probe and branch VM regressions | Admit bounded branch-local creation and retirement first. Track each occurrence and prove complete, non-overlapping disposition on each accepting path. |
| A08 · P1 | Identity, replacement and retirement | `replace_unique` can express identity continuity with a different envelope from complete `replace`. A Type Script hash match, a field identity, Type ID and Lock-associated `script_args` carry different obligations. | Source review and identity regressions | Merge only after each old identity rule and its envelope have an exact destination. Keep the absence policy explicit on `retire`. |
| A09 · P1 | Creation, quantity burns and manual disposal | Named `fresh` and `retire` reuse lower-level kernels. Existing creation forms also include anonymous returned Cells. `consume` accounts for ownership. `burn_amount` governs quantity and retirement governs absence. | Source review and lifecycle ELF/VM regressions | Give each complete intent one primary public form. Preserve the expressible placement, output and accounting behavior when moving existing callers. |
| A10 · P2 | Local equality and state flow | `preserve` elaborates to field equalities. Bare `transition` can overlap an established successor. A field-qualified transition checks a declared state edge. | Source review and successor/flow regressions | Use `require` for local equality and `replace` for a complete successor. Remove duplicate bare lineage only when that same relation is already checked. Keep state edges explicit. |
| A11 · P2 | Statement and declaration boundaries | `validity` implicitly runs to the end of the type body. Some braced creation operations can be a final value. A grouped `require` is a pure list of assertions. | Parser and formatter review | Delimit the validity section. Explain expression versus statement positions. Keep purity and discard semantics visible. |
| A12 · P2 | Collections, budgets and view types | The same `Vec` spelling can describe a stored value, stack data or a member of an allocated collection. For a fixed-width local `Vec`, `capacity()` reports the fixed stack bound. | Support matrix, source review and typed runtime suites | Show actual storage, width and bounds in explain/hover. Prefer named limits. Keep Cell selection, witness plans, local allocation and data codecs distinct. |
| A13 · Preserve | Bindings, full coverage and dispatch | Fixed group coverage, transaction-level reads and shared transaction participants compose. The current artifact contract allows only specific combinations of coverage, layout and dispatch. | Direct artifact probes and real VM composition regressions | Keep the resolved axes explicit. Reduce repeated manifest input at tooling level and show all enforced choices before introducing new combinations. |
| A14 · Preserve | Assertions, capabilities and source domains | `audit` records metadata evidence. `require` checks an executable condition. Group and transaction invariants have different coverage. `Address`, `ScriptHash`, value vs Cell types, and read provenance vs borrowing have distinct contracts. | Grammar and source review, invariant and typed-view suites | Keep these distinctions and their diagnostics. Use one grammar authority across parser, formatter and tools. |

The earlier review records the remaining language inventory, including visibility, type validity, proof profiles and typed temporal domains. The resolved contract is authoritative for the executable syntax. [Earlier design review](../../CELLSCRIPT_SYNTAX_SURFACE_REVIEW.md) · [Authoring grammar](../../CELLSCRIPT_AUTHORING_GRAMMAR.md) · [Source contract](../../CELLSCRIPT_SOURCE_CONTRACT.md)

## Reproduced differences

### A01 · The version label does not establish parser parity

The pinned Playground at `1a6616f0c210df692f3cc0b67e1d524029ac1512` and native `cellc` both identify as Edition 2027 authoring-v8. The native compiler includes `5d542b76`, which enforces semicolons on all flow edges and improves nested policy diagnostics. The distributed WASM exposes the earlier behavior.

| Same source | Native metadata | Distributed WASM metadata |
| --- | --- | --- |
| `Start -> Done;` as the final `flow` edge | Accepts | Accepts |
| `Start -> Done` as the final `flow` edge | Rejects with the missing terminator diagnostic | Accepts and returns metadata |
| `retire` inside an `if` branch | Rejects and identifies the entry-level restriction | Rejects with a generic missing semicolon diagnostic |

The existing pinned browser compiler suite passed all 53 tests. Its published source-contract check and other fixtures therefore cover a narrower set of parser behavior. The report supports rebuilding and re-pinning the distributed bundle and feeding these native fixtures to its test suite. A per-build source-conformance identifier is worth considering if tools must match exact grammar behavior across corrections with the same edition and source identity. [Native terminator fixture](../../../tests/syntax_combo/seeds/flow-edge-terminator-reject.cell) · [Nested policy fixture](../../../tests/syntax_combo/seeds/branch-entry-policy-reject.cell) · [Pinned Playground tests](../../../website/scripts/test-playground-compiler.mjs)

### A02 · Field order depends on where the type was declared

Each probe used a schema with `a: u64, b: u64`. For creation it assigned two constants. For pooling it conserved both fields. The policy and the declared type were kept the same when moving the schema into another module, importing through an alias, or building through a locked local package dependency.

| Field list | Local schema | Imported module schema | Aliased import | Locked path dependency |
| --- | --- | --- | --- | --- |
| `a, b` | Accept | Accept | Accept | Accept |
| `b, a` | Reject | Accept | Accept | Accept |
| `a` | Reject | Reject | Reject | Reject |
| `a, b, a` | Reject | Reject | Reject | Reject |
| `a, c` | Reject | Reject | Reject | Reject |

The frontend obtains declaration order only from types in the current module. Lowered creation then uses resolved-field checks that reject missing, duplicate and unknown fields without imposing order. The same policy thus crosses different admission checks depending on its import route. The exact negative diagnostics also vary by phase. [Local checks](../../../src/parser/action_policies.rs#L500) · [Resolved initializer checks](../../../src/types/initializers.rs#L7)

All six accepted permutation pairs also changed ELF bytes. Each operation and field order produced the same bytes across the module, alias and package cases. For example, the `fresh` pair hashes to `ecc1f65ab9e5…` in schema order and `213680d57e2f…` in reversed order, using SHA-256 of the entire ELF. Reordering these clauses is therefore an observable build change today.

One option is to require schema order everywhere after resolution. That preserves the local rule and would reject currently accepted imported source. The recommended option is to accept complete named lists in either order and keep one schema-owned encoded layout. That removes the authoring inconsistency. It still needs a decision about the order in which expressions evaluate and fail. Treat field names, execution order and byte offsets as separate controls. Test constant assignments first, then expressions reading transaction data and each per-output pool assignment. Avoid moving a check across another check just to canonicalize presentation.

### A03 · Identical creation syntax has two valid physical coordinates

`fresh issued` in the probe resolves to `output[0]` under an ordinary build or a Type artifact with transaction-absolute bindings and action-defined coverage. A fixed-group artifact selecting the same action resolves it to `group-output[0]`. Both typed role bindings and disposition records report the selected coordinate. This is a prose correction in the grammar. The two coordinate systems remain supported. [Affected grammar paragraph](../../CELLSCRIPT_AUTHORING_GRAMMAR.md#other-fixed-role-relations) · [Binding regression](../../../tests/native_lifecycle_binding.rs#L244)

| Manifest combination for the same action | Observed result |
| --- | --- |
| Group-relative, fixed group, named `Token` layout, single entry | Accepts, records `group-output[0]` |
| Group-relative, fixed group, named `Token` layout, tagged dispatch | Accepts |
| Transaction-absolute, action-defined, single entry | Accepts, records `output[0]` |
| Transaction-absolute with fixed group, or fixed group without a named layout | Rejects |
| Action-defined with tagged dispatch | Rejects |

CKB exposes group-relative coordinates alongside transaction coordinates. The CellScript artifact decides which coordinates bind its named linear roles. The same action can read additional transaction-level Cells through typed views. The verifier still has to check the relevant Script identity when a specific foreign Cell is required. [CKB Script grouping](https://docs.nervos.org/docs/tech-explanation/script-group-exe) · [Manifest validation](../../../src/artifact.rs#L77) · [Funding and settlement regression](../../../tests/native_group_binding.rs#L73)

### A04 · The deep runner's failing cases never reach their oracles

| Default deep seed case | Intended validation | Observed phase |
| --- | --- | --- |
| `f87a06b41f47` | Successful stdlib lifecycle transfer and expected role obligations | Parse rejects `verification` |
| `e503dd96e132` | Successful stdlib Cell helper | Parse rejects `verification` |
| `87374bb796d9` | Selected negative reaches its compiler-phase diagnostic | Parse rejects `verification` |

The default seed is `20260503`. The generator also has old list and terminator spellings in its transfer strings. Update each source and verify that the cases exercise their original acceptance or rejection intent. Relabeling them all as expected parse failures would discard targeted coverage. [Deep case generator](../../../crates/cellscript-tools/src/syntax_combo.rs#L344) · [Matrix definitions](../../../tests/syntax_combo/matrix.toml)

### A05 · A safe signature simplification starts with explicit roles

The same transaction-absolute action declaring `fresh issued` was built with three signatures. Each used the same body, role order and selected artifact entry.

```cellscript
action mint(witness recipient: Address, output issued: Token) {
    // The same fresh disposition goes here.
}
action mint(witness recipient: Address) -> issued: Token {
    // The same fresh disposition goes here.
}
action mint(recipient: witness Address, output issued: Token) {
    // The same fresh disposition goes here.
}
```

All three files compiled, independently verified and produced the same SHA-256 of the ELF, beginning `53855bfb7a51`. Their output was `output[0]`. This supports one canonical spelling for explicit roles. A separate regression moves an anonymous return to a named output and compares witness bytes and CKB-VM outcomes. The existing Fiber migration shows why named output conversion needs its own ELF comparison. [Signature rules](https://github.com/CellScript-Labs/CellScript/blob/e134d663de9e7c226af5c4691648e86bc3a283ce/docs/CELLSCRIPT_AUTHORING_GRAMMAR.md#explicit-source-syntax-authoring-v8) · [Anonymous creation migration regression](../../../tests/native_lifecycle_binding.rs#L207)

## Convergence alternatives

| Decision | Candidate | Trade-off and boundary |
| --- | --- | --- |
| Entry signature | Keep all admitted spellings | Existing code stays readable to its authors. New users still learn two Cell-output forms and two marker positions. |
| Entry signature | Prefix roles and inline Cell outputs | Reduces authoring styles. Migrations preserve the resolved role source, side, ordinal and witness order. Anonymous returned Cells need separate review. |
| Lock entry | Use `lock` with a selectable Lock artifact | Removes module-final placement and supports multiple selectable declarations. A selected Lock still has its own invocation and placement contract. |
| Lock entry | Reuse a single `action` keyword for both Script kinds | Gives the smallest keyword count but shifts the source of authorization and result interpretation entirely to artifact configuration. `lock` communicates this responsibility where authors read the verifier. |
| Branch-local lifecycle | Retain top-level policies and use low-level operations on branches | Fits current compiler structure. The branch has a different authoring vocabulary and may produce weaker metadata. |
| Branch-local lifecycle | Share statement grammar and track path-local occurrences | Lets a branch express the same business guarantee. Requires source resolution, type/linear checks, CFG-aware lowering and independent coverage evidence. Admit one accepting-path shape at a time. |
| Unique replacement | Keep a focused separate operation | Keeps current transaction acceptance while identity and envelope policy are specified. |
| Unique replacement | Add an explicit identity treatment to `replace` | Works if every existing case can express its identity and envelope rules. A forced `capacity = same`, change in Lock treatment or new Type Script restriction can reject a previously accepted transaction. |
| Small shorthands | Keep intent-oriented forms with a common kernel | `assert_conserved` succinctly names an invariant when it shares the admitted arithmetic of the corresponding sum check. Full `replace` declares obligations that independent local equalities would leave to the author. |
| Small shorthands | Retire spellings with identical guarantees | `preserve` can generally migrate to the same ordered field equalities in `require`. Keep the distinct `flow` edge and the independent ability to write arbitrary local checks. |

Prefer validation fixes and truly equivalent surface reductions first. For fields, keep the compiler and metadata authoritative for the encoded layout. For typed roles, resolve the old source before rewriting it. For `fresh` and `retire`, cover early failure, two accepting branches, missing disposition and duplicate disposition before considering loops or helpers. For identity, test the full Script, field, Type ID and `script_args` cases with the existing policy choices before merging the syntax. [Successor lowering](../../../src/ir/relations.rs#L86) · [Unique replacement lowering](../../../src/ir/cell_operations.rs#L244) · [Runtime identity checks](../../../src/codegen/mod.rs#L4040)

The unique replacement backend currently checks the full Type Script hash for `ckb_type_id` and `singleton_type`, the full Lock hash for `script_args`, and the nominated data field for `field(name)`. Preserve the selected guarantee and the old admitted envelope together. In particular, an identity check that uses the Type Script hash leaves room for a different Lock where the operation permits it. The corresponding absence rule also needs its original search domain. [Identity check dispatcher](../../../src/codegen/mod.rs#L4040)

## DeFi and composition evidence

| Concern | What was checked | Consequence for a syntax change |
| --- | --- | --- |
| Deployed code identity | Rebuilt `asset`, `reserve`, `lp`, `minimum`, `pool`, `order`, `allocation`, `escrow`, `settlement` and `route` from both revisions using the tracked package inputs. All ten pairs are byte-identical and independently verify at their respective revisions. | Those exact package builds keep their ELF identity. Recheck any example whose source or selected output convention changes. |
| Transaction builder | Compared the entire Pool `verify` ABI report and a generated 144-byte payload in each of modes 0–8. | Preserve parameter types, order, byte encodings, and placement. Sample encoding and execution of an actual transaction are separate checks. |
| Composition and accounting | Native VM suites cover the Pool lifecycle, direction and quote modes, nonzero and permuted roles, two-pool routes, partial order fills, shared allocations and combined settlement. | Keep typed reads of transaction-level Cells and explicit identity checks usable alongside local obligations. |
| Same transaction, different Scripts | Fixed-group fixtures check foreign funding and settlement data. Action-defined fixtures retain absolute bindings across multiple layouts. | Let different Scripts check the same Cell. Scope exactly-once ownership and accounting rules to the verification that establishes them. |
| Package composition | The locked dependency probe uses the same schema across package boundaries. Typed read and bounded-value suites exercise retained declared shapes. | Use resolved schema identity and layout across imports and aliases. Keep bounds attached to the resolved types. |
| External ELF delegation | Reviewed the exact CellDep and its pinned data hash, ordered delegation steps and the verifier ABI. The dedicated external verifier and typed runtime suites exercise those contracts. | Preserve this separately from transaction composition and package resolution. |
| Witness interaction | ABI suites cover input-bearing and output-only paths and malformed envelopes. The tagged policy ABI aggregates records at the physical index and preserves other `WitnessArgs` fields. | Preserve single-entry and tagged layout choices. Include occupied-field rejection and the output-only fallback when auditing a migration. |

The comparison concerns the tracked finance packages at the two pinned revisions. Their dependency configuration must be materialized for a specific deployment. The VM suites separately compile transaction fixtures with concrete Script identities and execute success and failure paths. Source compactness alone is a poor measure here. Compare how many coordinates an author must maintain, which checks are visible, and whether the builder has to supply different data. [Finance package contract](../../../examples/finance/README.md) · [CKB Script testing](https://docs.nervos.org/docs/script/script-testing-guide) · [CKB witness conventions](https://docs.nervos.org/docs/tech-explanation/witness) · [CellScript witness ABI](../../CELLSCRIPT_POLICY_WITNESS_ABI.md) · [External verifier contract](../../CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md)

The language `pool` relation accounts for named roles and conserved quantities. The finance Pool also checks its reserve positions, peer identities, price formulas, slippage and share accounting. Its implementation shares one reserve predicate across directions and wallet or reserve-leg swaps. The syntax audit preserves the ability to write those distinct protocol checks. [Pool modes and formulas](../../../examples/finance/README.md#data-and-witness-contract)

## Verification for a subsequent language change

| Dimension | Positive and negative cases to compare |
| --- | --- |
| Source admission | Current and proposed spellings, explicit and inferred roles, local/imported/aliased/locked schemas, valid source with comments and trailing commas, removed spellings and the phase of each error. |
| Role and coverage | Group and transaction selectors, nonzero physical positions, empty input or output sides, extra members, wrong layout, foreign Cells before and after the group. |
| Lifecycle and identity | Fresh and retired instances, same and different business IDs, partial and full matches, amount burns, replaced Lock, Type Script and capacity, ordinary and conditional paths. |
| Bytes and placement | Identical and intentionally changed ELF bytes, source-map and lowering record consistency, code-hash-bound dependents, witness payload, selector and index, occupied fields and output-only groups. |
| Toolchain | Parser, formatter idempotence, native diagnostics, LSP workspaces and hover, distributed browser import/diagnostics, independent artifact validation and generated builder placement. |
| DeFi behavior | Same proposed transactions before and after, wallet and reserve-leg swaps, exact input and output, liquidity, two-pool routes, order fills, combined settlement, deterministic arithmetic and relevant rejections. |

Run the tools against the same source revision and pin both builds for differential checks. A result at one layer supports that layer. For example, an independently accepted artifact and an unchanged witness are two distinct results. A source migration that alters ELF bytes changes the exact artifact a code-hash-bound caller must use even when the transaction fixtures agree. [Repository gate scopes](../../CELLSCRIPT_GATE_POLICY.md#gate-modes)

## Observed checks

| Check | Result |
| --- | --- |
| Native syntax-combination runner, `quick` | 98 cases, 60 accepted and 38 rejected as expected |
| Native syntax-combination runner, `ci` | 120 cases, 74 accepted and 46 rejected as expected |
| Native syntax-combination runner, `deep` | 126 cases, 74 accepted, 49 rejected as expected and three unexpected parse-phase failures under A04 |
| Direct probes | 56 cases, 23 accepts and 33 rejects matching the recorded baseline expectations. Twenty-two emitted ELFs independently verified. The expectations document current behavior, including the A02 disagreement. |
| Native syntax, formatter, lifecycle, witness, checker, Fiber and finance suites | 185 tests passed in 13 integration test binaries |
| Typed views, bounded values and roles, external verifier contracts | 67 tests passed in five integration test binaries |
| Native LSP | 76 tests passed |
| Current WASM crate with `--features wasm` | 34 tests passed, including the native-equivalent flow and placement diagnostic cases |
| Distributed Playground and VS Code | 53 existing browser tests passed, and the VS Code validation script passed. The extra browser pairing probe found A01. |
| Before and after finance builds | Ten exact ELF matches and nine matching Pool witness samples. Both revisions independently verified all ten artifacts. |

The table describes targeted source, tool and transaction-fixture checks. It leaves deployed contracts, chain data and a future syntax migration outside the observed results.

## Reproduction

From a detached checkout of the audited commit, the repository runner reproduces the three syntax-combination modes. The two seed files linked under A01 can be passed separately to native `cellc metadata` from a directory containing only that case and to `compile_metadata_json_diagnostics` on the pinned browser bundle. Append the missing semicolon for the positive control.

```sh
./scripts/cellscript_syntax_combo_audit.sh quick
./scripts/cellscript_syntax_combo_audit.sh ci
./scripts/cellscript_syntax_combo_audit.sh deep
```

The following runs the negative fixture against the pinned browser bytes from the repository root. In the audited revision it reports zero errors and returns metadata.

```sh
node --input-type=module <<'JS'
import { readFileSync } from "node:fs";
import { initSync, compile_metadata_json_diagnostics } from "./website/public/wasm/cellscript_wasm.js";
initSync({ module: readFileSync("./website/public/wasm/cellscript_wasm_bg.wasm") });
const source = readFileSync("./tests/syntax_combo/seeds/flow-edge-terminator-reject.cell", "utf8");
console.log(compile_metadata_json_diagnostics(source, "2027", undefined));
JS
```

For the schema probe, compile the following as a standalone file and then move the identical `Token` declaration to a public schema module and import it from the action module. The two cases differ in field admission. Swap the field order for the positive control.

```cellscript
module audit::mint;

resource Token has store, create {
    a: u64,
    b: u64,
}

action mint(witness recipient: Address, output issued: Token) {
    fresh issued {
        data { b = 8, a = 7, };
        identity = none;
        lock = exact(recipient);
    }
}
```

For the finance comparison, build each of the two revisions with its own `cellc`. Run `cellc lock`, `cellc build --locked` and `cellc verify-artifact build/main.elf` inside each package, then compare the raw artifact bytes. For Pool witness samples use `cellc abi --lock verify` and `cellc entry-witness --lock verify` with the arguments in declaration order. [Pool source and mode contract](../../../examples/finance/README.md#data-and-witness-contract)

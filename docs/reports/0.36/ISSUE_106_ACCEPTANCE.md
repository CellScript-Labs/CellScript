# Issue 106: explicit source syntax and coordinated tooling

Status: implemented, focused-validated and pushed as signed commits on the 0.36
development line on 2026-11-09. This document is not a
release, deployment or merge-readiness receipt.

The implementation follows [issue #106](https://github.com/CellScript-Labs/CellScript/issues/106).
The reviewed starting revision was
ccc409cc88b06e6ec4084048f4e262800839a36d. The signed implementation revision is
[1ebd946c6b013ecdb9c39cafefb8b60131e0f09c](https://github.com/CellScript-Labs/CellScript/commit/1ebd946c6b013ecdb9c39cafefb8b60131e0f09c).
The maintainer explicitly prohibited gates
and CI; none were invoked. Ordinary focused builds, behavioral tests,
formatting, whitespace checks and the canonical WASM build were authorized.

| Repository | Signed implementation commit | Published branch |
| --- | --- | --- |
| CellScript | [1ebd946c6b013ecdb9c39cafefb8b60131e0f09c](https://github.com/CellScript-Labs/CellScript/commit/1ebd946c6b013ecdb9c39cafefb8b60131e0f09c) | 0.36 |
| Website and default WASM | [b1e9af1bccbb3499130858b82bf9e92a2300300d](https://github.com/CellScript-Labs/cellscript-website/commit/b1e9af1bccbb3499130858b82bf9e92a2300300d) | codex/0.36-source-contract |
| VS Code extension | [edda638e5f8e06455709be0a4793e6e351e42fd7](https://github.com/CellScript-Labs/cellscript-vscode/commit/edda638e5f8e06455709be0a4793e6e351e42fd7) | codex/0.36-source-contract |
| Active benchmark sources | [221f2638d6a23534be0f5c876b9f4b40500661e0](https://github.com/CellScript-Labs/cellscript-ickb-equivalence/commit/221f2638d6a23534be0f5c876b9f4b40500661e0) | codex/0.36-source-contract |

Each listed commit has a valid local GPG signature. Remote branch tips were
checked against those exact hashes, and the parent implementation records all
three submodule hashes. Commits carry `[skip ci]`. The maintainer separately
authorized benchmark branch publication and updating the parent reference.
This acceptance document is a subsequent documentation commit; the source
revision above identifies the implementation without a circular self-hash.

## The decision and its effect

The source contract is now Edition 2027 authoring-v3. There is one current
grammar, shared by native compilation, source formatting and browser
compilation. Existing source needs migration. The product contains no legacy
parser, automatic semicolon insertion or backward-compatibility switch.

The important improvement is predictable meaning. A newline can move without
changing a statement boundary. A constructor has an explicit keyword. Generic
call arguments have an unmistakable delimiter. Formatting checks the complete
program structure before returning edits. A selected editor/compiler pair can
detect a grammar mismatch even when both report the same alpha release number.

This does not require a Rust upgrade. Rust remains pinned to 1.97.1, the
workspace remains at Cargo version 0.33.0-alpha.2, and the source identity is
cellscript-source-semantics-2027-0.33-authoring-v3. The final cache generation is
project-source-set-v202-explicit-source-syntax. The earlier v201 generation was
used during development and was invalidated at the final cutover.

| Identity | Decision | Reason |
| --- | --- | --- |
| Development branch | 0.36 | The requested implementation line |
| Source edition | 2027 | One current edition; no parallel legacy grammar |
| Source semantics | authoring-v3 | A breaking grammar must reject old/mixed source products |
| Artifact cache | v202 | Invalidate old results and intermediate development output |
| Cargo release | 0.33.0-alpha.2, unchanged | A development change does not publish a new release |
| Rust toolchain | 1.97.1, unchanged | The grammar change needs no toolchain migration |
| Wire schema numbers | Unchanged | The existing DTO shapes carry the source profile |
| Existing V0 commitment domains | Unchanged | Source presentation must not redefine cryptographic identity |

## What changed at each layer

~~~mermaid
flowchart TD
    A["Source files and editor buffers"] --> B["Shared lexer: tokens, trivia, exact ranges"]
    B --> C["Shared semantic parser"]
    C --> D["AST: explicit discard and tail values"]
    C --> E["Lossless document: comments and syntax ranges"]
    D --> F["Resolution, generic binding, type and flow checks"]
    F --> G["IR: explicit reference conversion types"]
    G --> H["Metadata and backend lowering"]
    H --> I["Assembly and ELF"]
    I --> J["Independent artifact checker"]
    J --> K["Bounded CKB-VM execution controls"]
    E --> L["Source formatter"]
    D --> L
    L --> M["Reparse, structural comparison and idempotence"]
    M --> N["CLI writes or revision-checked LSP edits"]
~~~

| Layer | Concrete change | Practical effect |
| --- | --- | --- |
| Lexer | Whole u128 literals, pinned Unicode XID tables, separate text/byte escapes, lossless trivia | Consistent values and precise malformed-input diagnostics |
| Parser | Explicit delimiters, constructors, generic calls, attribute matrix and operator rules | Source interpretation no longer depends on capitalization or newline placement |
| AST and visitors | Discard is distinct from a final value expression | A discarded value cannot silently become a function return |
| Type/flow/generic resolution | Retain the distinction and reuse admitted imported template ownership | Aliased and qualified generic construction reaches the same declaration |
| IR | Reference takes one type layer; dereference removes one; borrowed iteration retains the item reference | Source types survive lowering instead of becoming misleading same-type unary operations |
| Metadata | Exact unary type propagation and retained payload widths | Evidence describes the actual reference/value operation |
| Backend | Preserve fixed aggregate backing and wide-byte storage | Borrowed projections and copies retain their real storage |
| Independent checker | Exact reference conversion and recursive unit ABI spelling normalization | Wrong-type and wrong-arity mutations still reject |
| Formatter | Token/trivia layout, exhaustive AST comparison and second-pass stability | Comments, scopes and runtime meaning survive source editing |
| CLI | Prepare all sources, stage replacements and check snapshots | A later syntax error cannot leave earlier files reformatted |
| Native LSP | Current templates, source type spelling, bounded completion recovery and existing revision guards | Suggestions and edits match the selected grammar |
| VS Code | Snippets, highlighting and CLI/LSP source-contract checks | A stale compiler pairing produces an actionable error |
| Website/WASM | Current templates, actual multi-file examples, asset hash and source handshake | The browser compiles the real example workspace |
| Current docs/fixtures/locks | Intentional migration and identity refresh | Examples, tool output and review evidence name the same source contract |

The checker remains independent of the compiler front end and code generator.
Sharing a closed source-profile constant does not share inference. No broad
instruction/profile allowlist was added to make output pass.

## Syntax: before and after

### Statements and values

A semicolon discards a value. A supported final expression without one supplies
a value. These programs intentionally differ:

~~~cellscript
fn value() -> u64 { 7 }
fn missing_value() -> u64 { 7; }
~~~

The second function does not return 7 and is rejected for its missing value.
Returns, lets, discarded calls, assignments, requirements, break/continue,
module/import/constant declarations and policy clauses use explicit
terminators. A newline is whitespace; a return may continue onto the next line.

~~~mermaid
flowchart LR
    A["Expression followed by semicolon"] --> B["Discard statement"]
    C["Final supported expression without semicolon"] --> D["Tail value"]
    B --> E["Evaluate effects and failures"]
    E --> F["Continue without supplying a return value"]
    D --> G["Check the declared result type"]
~~~

This is especially important for failures. Evaluating a discarded checked
operation must still retain its fatal error. Formatting the checked unsigned
negation regression keeps guest exit 49; zero controls still succeed.

Grouped requirements use comma-separated pure conditions. They do not grant
permission to hide lifecycle operations inside a constraint group.

### Construction, calls and tuples

~~~cellscript
let pair = new lower_pair<u64> {
    left: identity::<u64>(7),
    right: 8,
};
let one = (7,);
let grouped = (7);
~~~

Construction no longer relies on an uppercase-name heuristic. Lowercase,
underscore-containing, admitted qualified and imported alias names use the
same explicit construction syntax. Uppercase condition variables remain
ordinary values.

Ordinary value construction is separate from Cell lifecycle syntax:

~~~cellscript
create next = Token { amount: 7, } with_lock(owner)
~~~

A Cell create constrains a proposed transaction output. It does not allocate
chain state inside the VM, and it does not take an extra new keyword.

Calls use identity::<u64>(value). Dedicated read_ref, create_unique and
replace_unique operations use the same type-argument punctuation. Old
function<T>(...) calls reject rather than competing with comparisons.

Grouping and singleton tuples are distinct in both expressions and types.
The source printer emits (u64,) where that is a tuple. Established source_text
spelling used by V0 packed/commitment domains stays separate from the current
source_syntax printer. The checker recognizes () and unit recursively at the
ABI boundary while retaining tuple arity and every non-unit field.

### Operators, attributes and literals

Operator binding is coordinated between parser and renderer. From weaker to
stronger: assignment, range, logical OR/AND, equality, ordering, bitwise
OR/XOR/AND, shifts, addition, multiplication, cast, unary and postfix.
Unparenthesized comparison chains, including mixed equality/ordering chains,
reject with a diagnostic explaining grouping or explicit conjunction.
Multi-character operators must be contiguous.

The verification marker and capability attribute are retired. Lifecycle
permissions use has. Attributes precede visibility, apply only to their
declared owners and reject duplicates/conflicting scheduler modes. Accepted
attribute information remains represented.

Decimal and hexadecimal integers support the whole u128 range, including
between-digit underscores. A malformed suffix, separator or overflow rejects
the entire candidate. Tests exercise boundary tables and 20,000 deterministic
decimal/hex decoding inverses.

Text uses double quotes and explicit scalar escapes. Byte strings use ASCII
and byte escapes such as hexadecimal bytes. Raw controls/newlines, invalid
Unicode scalars and single-quoted string aliases reject. Identifier admission
uses unicode-ident 1.0.24 with Unicode 17.0.0 XID tables, without normalization.
Editor/website coloration is advisory; the compiler's pinned tables determine
semantic admission.

## Formatting is a checked source transformation

~~~mermaid
flowchart TD
    A["Original source snapshot"] --> B["Parse one lossless document"]
    B --> C["Layout tokens and retained comments"]
    C --> D["Parse candidate"]
    D --> E{"Same exhaustive semantic shape?"}
    E -- No --> X["Reject edit"]
    E -- Yes --> F["Format candidate again"]
    F --> G{"Identical second result?"}
    G -- No --> X
    G -- Yes --> H["Return validated source"]
~~~

The comparer explicitly covers current AST fields and variants. It excludes
spans and documentation presentation, not operators, bindings, attributes,
types, statement kinds or scopes. It does not compare Debug strings, internal
IR serialization, artifact hashes or effect summaries as a substitute for
program structure. New AST structure requires updating the exhaustive code.

Tokens and trivia partition the original source exactly. Ordinary/doc
comments, nested block comments, audit directives and comment order remain
source-owned. Parser-recorded type/generic/prefix ranges guide layout.
The formatter does not recover semantic structure from identifier case.

AST display rendering remains useful for synthetic source and diagnostics.
Editing real files and editor buffers uses the source formatter.

~~~mermaid
sequenceDiagram
    participant U as CLI or editor
    participant F as Source formatter
    participant S as Source snapshots
    participant D as Filesystem or LSP session
    U->>S: Capture every candidate
    U->>F: Prepare and validate every rewrite
    F-->>U: Validated replacements
    U->>D: Stage temporary files or capture query revision
    U->>S: Recheck complete snapshots
    alt Invalid source or stale revision
        U-->>U: Reject before applying stale edits
    else Current sources
        loop Each changed file
            U->>S: Recheck this source
            U->>D: Atomic rename for this file
        end
    end
~~~

CLI replacements preserve permissions, stage same-directory temporary files
and synchronize their contents. A rename-phase failure reports the number of
already replaced files and the failing path. This is per-file atomicity, not
an all-or-nothing filesystem transaction. Snapshot checks are optimistic
concurrency checks; they are not a universal filesystem lock.

LSP edits retain the existing document/dependency revision guards. Incomplete
completion recovery is temporary and bounded to the specific trailing-dot
query and a narrow missing-terminator suffix. It neither repairs saved source
nor reintroduces newline-based statement termination.

## References must survive lowering

~~~mermaid
flowchart LR
    A["Source value T"] -->|"reference"| B["IR value with type Ref(T)"]
    B -->|"dereference"| C["IR value with type T"]
    B --> D["Retained backing owner and projection"]
    C --> E["Typed operation evidence"]
    D --> F["Backend scalar or fixed-byte storage"]
    E --> G["Independent exact-layer checks"]
    F --> G
    G --> H["VM success and assertion-failure controls"]
~~~

The previous same-type treatment could lose the distinction between a value
and a reference before evidence construction. The repair changes exactly one
layer. Borrowed collection iteration also creates the reference explicitly;
otherwise a loop could advertise a borrowed element while indexing an
unwrapped value.

Fixed aggregate projections retain their actual backing origin and extent.
Wider byte values stay in fixed-byte storage rather than a 64-bit scalar slot.
Tests cover single Cell-field borrowing, reborrowing, tuple receivers,
borrowed collections and positive/negative execution.

The checker accepts the precise permitted conversion; it does not erase
arbitrary reference layers. Forged types, escaping views, use after owner
consumption and unrelated conversions retain their rejection boundaries.
This is bounded evidence for admitted profiles, not a general Rust-style
aliasing, memory-safety or arbitrary pointer proof.

A separate source-map defect found during this work was also repaired:
binary expressions now include their final operand in the source range, and
requirement text excludes the statement delimiter. Review tooling therefore
sees the actual complete condition rather than a truncated right-hand side.

## Toolchain, LSP, editor and website move together

~~~mermaid
flowchart TD
    A["One authoring-v3 source contract"] --> B["cellc language-contract JSON"]
    A --> C["LSP experimental capability"]
    A --> D["WASM contract export"]
    B --> E["VS Code checks selected CLI"]
    C --> F["VS Code checks initialized LSP"]
    D --> G["Browser worker checks source contract"]
    H["Committed WASM SHA-256"] --> I["Verify downloaded bytes"]
    I --> G
    G --> J["Compile actual multi-file workspace"]
~~~

The small handshake contains schema, compiler version, edition, source
semantics, identifier Unicode version and the maximum source bytes. A release
number alone does not identify an evolving alpha grammar.

Native LSP templates and typed/named-role insertion spellings were migrated.
VS Code snippets, TextMate grammar, extension checks and documentation were
updated. Its validation builds the extension and checks the current contract,
including mismatch controls. A real VS Code UI session and Marketplace
publication are not claimed.

The website scratch template and highlighter use current spelling. Examples
carry their actual dependencies, including Token and Pool definitions. The
old makeStandalone transformation and substitute schema inlining were removed.
Hero cards use explicitly labelled excerpts of real sources, not a second
handwritten grammar.

The worker validates the selected WASM SHA-256 before initialization and
checks its source contract before exposing compilation. A mismatched cached
asset produces an explicit load error.

The canonical default bundle enables wasm and remains metadata-only. Optional
browser semantic services use the separate language-service feature. Heavy
native ProofPlan, typed-semantic and verified-artifact records remain outside
the bounded browser summary.

The canonical Docker build retains Rust 1.97.1, wasm-pack 0.13.1,
wasm-bindgen 0.2.121 and Binaryen 131 with their archive checksums. Download
routing now supports configured proxies and two official release-asset
routes. Error/JSON bodies cannot pass the SHA-256 checks. Host-local proxies
use the existing explicit host-network setting on Linux.

| Browser artifact measurement | Result |
| --- | --- |
| SHA-256 | 6ba8fe3843a6a2a86bf0a5692ef0fefa2c46d92597c7b713617bdee37a93dc19 |
| Raw bytes | 1,960,591 |
| Gzip bytes | 742,388 |
| Existing budget | 1,500 KiB gzip, unchanged |
| Actual profile | Default metadata-only wasm build |

No extension/package release, website deployment or CKB deployment occurred.

For a local installation, build the native CLI from this source revision with
`cargo build --locked -p cellscript --bin cellc`, select that binary in the
editor, and build the matching extension. Inspect `cellc language-contract
--json` before opening current sources. The corresponding website checkout
already contains the rebuilt default WASM asset. A new Rust version, global
dependency update or Marketplace publication is not part of this change.

## Before and after for an author

| User task | Reviewed starting behavior | Implemented result | Layer that provides the guarantee |
| --- | --- | --- | --- |
| Format checked unsigned negation | A guest error 49 could become success | Grouping survives; nonzero and zero execution controls retain their results | Source layout, structural comparison, VM regression |
| Format a reference receiver | `(&pair).0` could become `&pair.0` and change type | Explicit receiver grouping and type survive | Source formatter and AST renderer |
| Keep an audit explanation | Ordinary comments disappeared | Exact comment text and relative order remain in source | Lossless lexer/document |
| Return or discard a helper result | Implicit statement boundaries obscured the distinction | `helper();` discards; a supported final `helper()` supplies a value | Parser, AST, checking, IR and simulator |
| Move a newline or `else` | Parser paths treated line boundaries differently | Delimiters determine structure; newlines are whitespace | Parser |
| Name a record `pair` or `Pair_Data` | Naming style selected constructor recognition | `new` selects construction for admitted names and imports | Parser and nominal/generic resolution |
| Call a generic function | `<` could compete with comparisons | `::<...>` explicitly introduces call type arguments | Parser and source printers |
| Write `(7,)` | Printing could erase its tuple identity | Singleton tuple punctuation survives | Parser, printers and semantic comparison |
| Type a malformed integer/string | Partial tokens or unsupported emitted escapes obscured the error | The complete candidate rejects, with a source range; admitted values round-trip | Lexer and literal renderer |
| Borrow and reborrow a Cell field | Metadata/ASM could pass while checked ELF rejected | Bounded positive and assertion-failure cases reach independent ELF admission and VM execution | IR, metadata, storage and independent checker |
| Format a package with a later invalid file | Earlier files could otherwise be rewritten before discovering failure | Prepare every candidate before staging writes | CLI orchestration |
| Edit while formatting | An earlier analysis could offer obsolete source changes | Revision/snapshot guards reject stale edits; a late write failure reports the replaced prefix | LSP session and CLI writer |
| Select an old compiler with new snippets | An unchanged alpha release number hid grammar drift | Source-contract mismatch produces an explicit error | CLI/LSP handshake and VS Code |
| Open a website example | Substitute standalone schemas could hide import identity | The browser compiles the real multi-file workspace | Website data and WASM consumer |

These changes improve predictability, diagnostics and source preservation.
They do not promise faster compilation or extend every frontend feature into
an executable backend profile.

## Migration and identity review

Migration used pre-cutover parser roles for closed source inputs, with file
snapshot hashes checked before applying byte-range edits. Dynamic fragments,
template inputs and JSON-embedded source programs needed separate review.
Six instance-policy templates and three active benchmark specs were included
after the first inventory missed their file forms.

Parser-derived edits alone are not a semantic proof. An early inventory was
invalidated by a later parser edit and misplaced changes; the affected source
was restored and snapshot checks added. Placeholder substitution also proved
insufficient for statement boundaries and nominal constructors. A pre-cutover
parser must not receive mixed/new grammar: it can interpret the new constructor
keyword as an ordinary identifier. Final validation caught and repaired these
invalid intermediate fragments. Repairs were
followed by actual owning tests, independent artifact admission and bounded
VM controls. No migration helper or frozen parser is part of the product. Frontend-only
examples are checked with DenyFailClosed when asserting executable rejection;
the default AllowFailClosed policy may instead emit a rejecting stub.

The broad first compiler unit run exposed both fixture drift and genuine
implementation defects. Examples include final Cell creation incorrectly
discarded as a return value, borrowed iteration losing its reference type,
qualified generic construction bypassing an admitted import binding, and
nested unit spelling disagreeing at the ABI boundary. These were repaired
structurally. They were not hidden behind relaxed checker rules.

Twenty-two example locks were intentionally regenerated after source
migration. Current characterization sources, sidecar identities and generated
website data were refreshed from actual output. The twelve closed
characterization programs retain their ELF identities. Their interface and
typed-semantic identities reflect the new source contract. Sorted collection
resource fixtures changed only their eight source hashes. Other reviewed
scenario updates retain machine/transaction identities and numeric resource
limits; metadata-bound aggregate identities change where appropriate.

One layout observation initially showed the wide-record case becoming
rejected. Investigation found a duplicated module semicolon in the test
builder. The builder was fixed instead of recording the rejection as a new
baseline.

Historical release reports, the benchmark transaction matrix, original
binaries and pinned proposal/certification resource mirrors remain historical.
Active benchmark source migration does not regenerate historical equivalence
or production evidence. The maintainer supplied the separate benchmark push
authorization required by AGENTS.md before its branch was published.

## Validation and resources

Final owning-suite results and signed publication anchors are recorded here.
Commands are direct behavioral/build checks; none invokes a gate or CI
workflow. Cargo work uses CARGO_PROFILE_DEV_DEBUG=0 to avoid large debug-symbol
artifacts, with RUST_BACKTRACE=0 for compact test logs.

| Scope | Observed result |
| --- | --- |
| Compiler library | 1,219 passed, 0 failed; 210.36 seconds, followed by the three writer controls below (two newly added) |
| CLI staged-write failure controls | 3 passed: initial stale snapshot, second-rename failure, edit arriving after the first replacement |
| CLI | 260 passed, 0 failed; serial execution, 33.44 seconds |
| Independent checker library | 111 passed |
| Artifact-checker integration | 270 passed, 0 failed; 1,551.63 seconds |
| Allocated runtime | 43 passed |
| Grammar contract | 14 passed, 0 failed; final expanded negation-control rerun 1.86 seconds |
| Real examples | 36 passed |
| Native iCKB differential suite | 222 passed, 1 ignored |
| Benchmark compile/model inventory | 5 passed; published source rerun 0.67 seconds |
| Optional WASM language service | 35 passed |
| Default browser compiler | 52 passed, including 18 real example workspaces |
| VS Code validation | Passed, including extension build and contract mismatch controls |
| Astro checking | 67 files; 0 errors, 0 warnings, 1 hint |
| Direct Astro build | 32 pages built |
| Near-limit source formatting probe | 1 test passed; 0.13 s wall time; peak host RSS 27,480 KiB |
| Formatting/whitespace | Workspace Rust formatting and root/website/editor/benchmark whitespace checks passed |
| Report diagrams | All six Mermaid diagrams parsed and rendered in local headless Chrome |

The resource probe formats a near-1-MiB source containing a large retained
comment and rejects over-limit input/configurations. It is a finite probe, not
a worst-case parser/formatter complexity or host-memory bound. The source
limit remains 1,048,576 bytes. Formatter configuration and output bounds remain
enforced; no existing budget was raised.

Native responsiveness tests cover revision ordering, asynchronous work,
dependency overlays and Unicode position conversion. Separately ignored
release-profile LSP scaling benchmarks were not run. Simulator results are
not consensus evidence; browser metadata is not executable-ELF evidence.

The first native inventory exercised 114 owning targets and exposed 185
failures. Repairs were rerun at the responsible targets; the final affected
batch covers formatting, frontend boundaries, frozen interfaces, grammar,
interface projection, IPC, LSP responsiveness, native groups, collections,
policy/checker boundaries, imports, schema upgrades, bounded selection,
nominal/vector types and verifier failures. Its last two import failures were
source-factory delimiter/constructor mistakes and were fixed; the complete
17-test import target then passed. The complete compiler-library and serial
CLI targets were also rerun after the repairs. This is an initial inventory
plus focused final validation, not a claim that one final CI/workspace gate
was run. Nineteen ignored native cases in the initial inventory were not run.

The final affected-target counts are: formatter 6; frontend 14; frozen
interface 47; grammar 14; interface projection 9; IPC 4; LSP responsiveness
4 (2 ignored); native group binding 8; owned collections 9; policy artifact
checker 77; policy imports 17; schema upgrade 26; selection bundle 18;
structured nominal types 16; structured vectors 14; verifier failure 11.
These overlap the inventory and must not be added to it as distinct tests.

Representative commands:

~~~bash
export CARGO_PROFILE_DEV_DEBUG=0
export RUST_BACKTRACE=0
cargo test --locked -p cellscript --lib -- --test-threads=4
cargo test --locked -p cellscript --lib cli::format::tests -- --test-threads=1
cargo test --locked -p cellscript-artifact-checker --lib
cargo test --locked -p cellscript --test grammar_contract
cargo test --locked -p cellscript --test artifact_checker -- --test-threads=4
cargo test --locked -p cellscript --test allocated_runtime
cargo test --locked -p cellscript --test cli -- --test-threads=1
cargo test --locked -p cellscript-wasm --features 'wasm language-service'
CELLSCRIPT_WASM_DOCKER_NETWORK=host website/scripts/build-wasm.sh
~~~

In the website checkout: test:playground-compiler, test:playground-session,
test:playground-presentation, test:playground-focus, direct astro check and
direct astro build were used. The website's build command chains build:ci and
was deliberately not used. No backend/release gate, real-node acceptance,
NovaSeal verifier rebuild or publication assurance is claimed.

Retained development failures include the initial 1,106/113 compiler unit
result, incomplete literal/template migration, stale source-replacement
anchors, the duplicated module delimiter, download timeouts/API 403 responses
and a GPG signing timeout. Successful zero-test filter probes were not counted
as evidence. Final acceptance depends on nonzero executed owning checks.

## Acceptance criteria traceability

The numbers below follow the nineteen checkboxes in issue #106, in order.
"Implemented" names a bounded contract and its actual owning evidence; it is
not a claim about every possible source program. Signed implementation and
submodule anchors are published in the table at the start of this report.

| AC | Requirement | Implementation and observed evidence | Status |
| --- | --- | --- | --- |
| 1 | Preserve F1 arithmetic failure | `grammar_contract` executes original/formatted/AST-rendered checked negation, casts, nested unary operations and variable/nonliteral operands; guest 49 and zero controls | Implemented |
| 2 | Preserve postfix receiver types | Source retains receiver tokens/grouping; AST rendering gives postfix operations precedence-aware receivers; reference/tuple controls compile and execute before/after | Implemented for admitted cases |
| 3 | Tuple/grouping and literal inverse | Singleton expression/type regressions, separate source/domain printers, 20,000 deterministic integer decoding inverses and closed escape tables | Implemented |
| 4 | Complete current grammar | Authoring grammar plus migrated source families, examples, frontend/parser library tests and 114-target native inventory | Implemented; existing profile limits retained |
| 5 | Explicit delimiters and operators | Positive/negative grammar cases, comparison-chain diagnostics, grouped guards, native bodies, comment/range checks and migrated syntax seeds | Implemented |
| 6 | Naming-independent construction | Lowercase/underscore cases, uppercase conditions and real package alias/qualified generic constructors reach checked ELF/VM before/after | Implemented |
| 7 | Closed literal admission | Lexer tests reject the entire malformed candidate and validate source spans, numeric overflow, byte escapes and Unicode scalars | Implemented |
| 8 | Attribute ownership | Parser placement/duplicate tests; retired marker/capability attribute diagnose; retained scheduler/type obligations reach normal consumers | Implemented |
| 9 | Safe public formatter | `src/syntax.rs`, `src/ast/semantic_shape.rs`, `src/fmt/source.rs`; exact token/trivia partition, comments, exhaustive structural comparison and idempotence | Implemented |
| 10 | Honest write/revision failures | CLI prepares all and stages same-directory files; stale-source, rename-failure and late-write controls; LSP revision/dependency guards | Implemented; per-file atomicity only |
| 11 | Field borrow and reborrow | Explicit one-layer IR conversion; original/formatted independently admitted ELF; amount 1 succeeds and amount 0 asserts with guest 5; hostile checker corpus retains rejection | Implemented for bounded profiles |
| 12 | Before/after pipeline corpus | `grammar_contract` covers native group, patterns, borrowing and lifecycle through metadata, assembly and checked ELF; frontend-only lifecycle iteration remains denied under `DenyFailClosed` | Implemented |
| 13 | Observable execution controls | F1 49/0, field/reborrow 0/5, reference receivers, construction and operator conditions; native runtime suites independently exercise admitted behavior | Implemented for named finite cases |
| 14 | Independent checker | Separate checker inference/decoding retained; 111 library and 270 integration tests, plus 77 policy-checker cases; no broad admission relaxation | Implemented |
| 15 | Coordinated consumers | CLI 260, LSP responsiveness 4, optional WASM service 35, browser compiler 52 (18 workspaces), extension validation, Astro check/build; matching submodule commits published | Implemented; package/Marketplace release not performed |
| 16 | Explicit identities | Source semantics v3, cache v202, old/mixed profile rejection tests, 22 intentional lock repins, current generated metadata; unchanged schema/V0 domains documented | Implemented |
| 17 | Existing resource boundaries | Canonical metadata-only WASM: 742,388 gzip bytes; unchanged 1,500-KiB limit; near-1-MiB source probe and bounded configuration | Observed within existing limits |
| 18 | Historical evidence | Retain frozen benchmark matrix, original binaries, proposal mirrors and release reports; migrate active sources and active governance only | Implemented |
| 19 | Exact final receipt | This report records exact signed implementation/submodule revisions, commands, nonzero counts, failures, size/resource observations and unsupported profiles; remote branch hashes verified | Recorded |

Implementation entry points are the shared [syntax document](../../../src/syntax.rs),
[semantic comparison](../../../src/ast/semantic_shape.rs),
[source formatter](../../../src/fmt/source.rs),
[CLI writer](../../../src/cli/format.rs) and
[grammar/runtime controls](../../../tests/grammar_contract.rs).

## Risk decisions

| Risk | Decision and control | Remaining limit |
| --- | --- | --- |
| Formatting changes behavior | Exhaustive AST comparison, idempotence and VM success/failure controls | Finite execution is not universal equivalence |
| Migrating generated source | Snapshot-checked source roles plus actual owning tests | Placeholder parsing alone supplies no semantic assurance |
| Mixed compiler/editor/browser | Source contract handshake and exact WASM asset hash | Local builds must be installed together; no release was published |
| File write failure | Prepare all, stage, recheck and report replaced prefix | No package-wide filesystem transaction or universal lock |
| Reference/storage mistakes | Explicit type layers, retained backing and independent rejection controls | No general mutable-alias or arbitrary-pointer proof |
| Cryptographic identity drift | Separate source presentation from established V0 domain spelling | A grammar change does not guarantee every future Script hash stays fixed |
| Browser growth | Preserve default metadata-only profile and 1,500-KiB budget | Optional semantic-service profile is separate |
| Historical evidence misuse | Keep old evidence pinned and label new source migration | New source syntax does not establish old deployment/production equivalence |
| Publication before review | Signed commits, explicit benchmark coordination and an English issue receipt | No package/Marketplace release or deployment; gates/CI are unrun by instruction |

## Glossary

| Term | Meaning in this change |
| --- | --- |
| AST | Structured program meaning consumed by semantic passes |
| Lossless syntax document | Tokens, exact trivia and parser-owned source ranges for one revision |
| Trivia | Comments/whitespace retained by source tools |
| Discard | Evaluate an operation while supplying no tail/return value |
| Tail value | A final supported expression that supplies the enclosing value |
| Semantic shape | Exhaustive program structure after excluding presentation-only fields |
| Idempotence | Formatting formatted source changes nothing |
| Lowering | Convert checked source operations into explicit lower-level operations |
| Backing owner | Actual storage retained for an aggregate or borrowed projection |
| Wire DTO | Versioned boundary record, separate from internal compiler IR |
| Source semantics | Grammar/meaning identity independent of release number and target |
| XID tables | Pinned Unicode identifier-start/continue character sets |
| Independent admission | Checker validation of supplied records/artifact without compiler inference |
| ELF | Executable artifact container; successful assembly text alone is insufficient evidence |
| ABI | The call/witness/storage convention at a named boundary |
| Fail closed | Reject when the required source, type, profile or evidence contract cannot be established |

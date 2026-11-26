# CellScript Authoring Grammar

## Status and scope

Edition 2027 uses one business-language vocabulary: `resource`, `action`,
`lock`, `require`, ordinary expressions and control flow, and structured
lifecycle relations. A Type group artifact binds one or more named `action`
declarations to that group. An explicit Lock-group artifact selects an ordinary
`lock` declaration.

The syntax remains an evolving alpha contract. This document describes its
bounded executable surface, not a CellScript 1.0 grammar freeze or production
acceptance. Issue [#98](https://github.com/CellScript-Labs/CellScript/issues/98)
tracks the authoring convergence. The [authoring target](CELLSCRIPT_AUTHORING_TARGET.md)
and [semantic foundation](CELLSCRIPT_SEMANTIC_FOUNDATION.md) define the broader
requirements and independent verification boundary.
The [syntax surface review](CELLSCRIPT_SYNTAX_SURFACE_REVIEW.md) tracks future
convergence proposals and completed contract fixes separately.

The earlier preview spelling is recorded in the immutable
[alpha.2 grammar snapshot](https://github.com/CellScript-Labs/CellScript/blob/f7a7a0f6458dec594f75bd0e2c8cd65fa914a1e2/docs/CELLSCRIPT_2027_PREVIEW_GRAMMAR.md).
It is historical source, not an alternate current grammar.

## Package selection

```toml
[package]
name = "example"
version = "0.1.0"
edition = "2027"
entry = "src/main.cell"
```

The source edition, compiler release, target, metadata schemas, and witness and
deployment ABIs have separate identities. A spelling change does not implicitly
change an entry payload or its `WitnessArgs.input_type` placement. See the
[current source contract](CELLSCRIPT_SOURCE_CONTRACT.md).

## Explicit source syntax: authoring-v9

The current source-semantics identity is
`cellscript-source-semantics-2027-0.33-authoring-v9`. There is one current
parser, with no legacy syntax switch. A source epoch is independent of the
Cargo release version and of deployed wire/hash domains.

The shared kernel below applies inside ordinary and group-bound entries.
Newlines and comments are whitespace; they never terminate an operation.
Braces delimit executable scopes and structured lifecycle bodies. A grouped
`require` is a pure conjunction list, not an executable block.

```ebnf
module-header = "module", path, ";" ;
module        = module-header, item*, EOF ;
item          = attribute*, visibility?, declaration ;
visibility    = "public" | "public(package)" | "private" ;
attribute     = "#[type_id(", string, ")]"
              | "#[scheduler_hint(", hint-list, ")]" ;
body          = "{", statement*, tail-expression?, "}" ;
statement     = "let", "mut"?, binding-pattern, (":", type)?, "=", expression, ";"
              | "return", expression?, ";"
              | ("break" | "continue"), identifier?, ";"
              | "require", (expression, (",", message)? | "{", boolean-list, "}"), ";"
              | assignment, ";" | expression, ";" | block-statement ;
block-statement = if-statement | for-statement | while-statement
                | labelled-loop | borrow-scope | lifecycle-block ;
constructor   = "new", type-path, "{", field-initializers?, "}" ;
generic-call  = callable, "::", "<", type-list, ">", "(", arguments?, ")" ;
type          = scalar-type | path | type-application | "()"
              | "(", type, ")" | "(", type, ",", type-list?, ")"
              | "[", type, ";", unsigned-bound, "]"
              | "&", "mut"?, type ;
list<T>       = (T, (",", T)*, ","?)? ;
unsigned-bound = unsigned-integer | constant-path ;
```

This kernel is read together with the domain productions and restrictions
below. In particular, not every syntactically valid expression has executable
support. Existing type, ownership, source-binding, bounded-layout and target
admission checks continue to apply.

`&mut T` is recognized syntax for a reserved type boundary, not an executable
general mutable-reference ABI. Ordinary read-only helper views and checked
borrow scopes retain their separate admitted contracts.

- `module`, `use` and `const` declarations end in `;`. Field, argument,
  generic-argument, tuple, array, import and named-limit lists use commas.
  Trailing commas and newlines are permitted. Header modifiers such as
  `identity(field(id))` remain explicitly delimited modifiers before a body.
- `value();` discards its result. An admitted final `value()` supplies a tail
  value. A tail is unique and final; callable return compatibility is checked.
  Loop bodies do not acquire an implicit callable return. `return;` returns no
  value. `return` followed across a newline by `value;` returns that value.
- A braced `if`/`else`, loop or borrow scope needs no extra statement
  semicolon. An `else` may begin on the following line. Existing final `if`
  branch-value rules remain checked in the enclosing value context.
- `(value)` and `(Type)` group; `(value,)` and `(Type,)` are singleton tuples;
  `()` is unit. Source type printing uses explicit singleton commas. Existing
  packed/commitment V0 type-spelling domains retain their established spelling;
  source rendering is a separate operation.
- `new pair<T> { value, }` constructs an ordinary admitted value. Lowercase,
  underscores, qualified names and resolved aliases do not select a different
  grammar. `new` grants neither Cell lifecycle permission nor allocation
  authority. Members such as `Vec::new()` remain ordinary callable paths.
- Generic calls use `::<...>`. The dedicated typed operations
  `read_ref::<Type>()`, `create_unique::<Type>(identity = ...) { ... }` and
  `replace_unique::<Type>(identity = ...) value { ... }` use that punctuation
  too, while retaining their existing authority/type restrictions. Type
  applications in type positions remain `Pair<Type>`.
  The unique operations also accept empty `()` identity arguments, deriving
  the requested policy from the resolved Cell declaration. A declaration with
  `identity(none)` cannot supply a unique policy; imports and aliases resolve
  their defining declaration before derivation. Explicit policies must match
  that declaration and retain the existing identity/capability checks.

### Structured forms and statement boundaries

Braces delimit a region; the surrounding production determines whether that
region is a statement or an expression. In an admitted expression position,
the final value and an explicitly discarded value remain different AST forms.

| Form in a callable body | Boundary and result |
| --- | --- |
| `let`, `return`, `break`, `continue`, assignment | Statement ending in `;`; a newline does not terminate it. |
| `consume cell;`, `destroy cell;` | Explicit Unit disposal statement, with its own ownership and policy checks. |
| `require condition;`, `require condition, "message";`, `require { first, second, };` | A body statement requires `;`, including grouped require. The grouped list remains pure conjunction. |
| `Pattern => require condition, "message",` | A direct match-arm check is a Unit expression; its message and arm separator have separate commas. Comments/newlines do not detach the message. A braced arm uses ordinary statement termination. |
| An admitted call, arithmetic expression, tuple, array or `new Type { ... }` | `;` discards the result. A final unterminated expression supplies the block's tail value, subject to return and ownership checks. |
| `create ... { ... }`, `create_unique::<T>(...) { ... }`, `replace_unique::<T>(...) cell { ... }` | These are structured expressions. Before another statement their result is discarded; a final unterminated occurrence can supply a tail value. An explicit `;` discards that value while retaining the operation's checks. |
| `replace before -> after { ... }`, `preserve after from before { ... }` | Unit checks with an implicit statement boundary at `}`. Neither supplies a returned Cell. Replacement requires all four envelope treatments; preservation only compares its selected fields. |
| `pool`, `retire`, `fresh` | Structured lifecycle statements, with no extra semicolon after `}`. Their internal named clauses keep their required semicolons and list commas. |
| `if`/`else`, `match`, ordinary block | The existing statement/expression context and branch type rules determine the result. A closing brace alone does not discard a value. |
| Loop, labelled loop, borrow scope | Statement scope, with no extra semicolon after `}` and no implicit callable tail return. |
| `audit`, type `validity` | Delimited declaration regions rather than returned values. Audit remains metadata-only; validity contains pure terminated predicates. |

For example, an anonymous `create` at the tail of `action mint(...) -> Token`
can provide its returned Cell. Adding `;` changes that body to discard the
expression result; the compiler must still check the callable's declared return.
A formatter never inserts that discard to normalize the appearance of braces.

### Declaration and domain coverage

| Family | Syntax boundary and admission |
| --- | --- |
| Imports and constants | `use path::{name, ...};`, optional aliases; `const NAME: Type = expression;` |
| Value records and enums | `struct`/`enum`, optional bounded type parameters and value abilities; comma-separated fields/variants; existing public fixed-layout admission |
| Cell-backed types | `resource`, `shared`, `receipt`; one `has ...` capability declaration; concrete schemas and existing identity/hash/capacity modifiers |
| Ordinary callables | `fn`, `action`, `lock`; comma-separated ports/parameters; retained source qualifiers, return/output signatures and action transition clauses |
| State transitions | Existing state-path/state-name forms; transition clauses and flow edges end in `;`; initial/terminal state lists retain their explicit terminators |
| Invariants and validity | An aggregate requires an enclosing `scope`. Bare targets inherit it and explicit views must agree with it. Predicate/count/assertion clauses end in `;`. `forall binding in view<T>` derives its role from the finite view |
| Collections | `collections`, named allocation limits and typed Vec/OrderedMap/OrderedSet members; comma-separated entries and unchanged finite bounds |
| Nested codecs | `data_shape`, typed element and distinct named limits; no implicit witness budget or source authority |
| Typed Cell data reads | `cell.data.field` and `cell.data.rows[index].field` for fixed schemas. A row array contains fixed, serializable value records with unsigned scalar, hash, address, or fixed-byte fields |
| Named Cell roles | `cell_roles`, named finite scan/selection/byte limits and explicit role policies; comma-separated policy entries; no inferred authority |
| Ordinary lifecycle | Existing `consume`, destruction policies, claim/settle, create/read operations and bounded `consume_each`/`create_each`; executable restrictions remain explicit |
| Successors and preservation | `replace` has all four mandatory treatments; `preserve after from before { fields, }` is an explicit field whitelist, without implicit disposition |
| Group-bound policies | Type group artifacts select `action` declarations; Lock group artifacts select `lock` declarations. Fixed roles and `pool`, `retire`, `fresh` use the selected context. Policy clauses end in `;` and lists use commas |
| Audit | `audit name { expected_evidence = external_policy(value); }`; metadata-only evidence, never an executable accepting condition |
| Patterns and control | Name/tuple/wildcard bindings, existing match patterns and expression/block arms, labelled break/continue, if/for/while and statement-scoped borrow/reborrow |

Each `flow` edge is a clause ending in `;`, including the final edge before
`}`. Commas separate names in initial/terminal state lists; a comma cannot
replace an edge's semicolon. Comments and newlines do not terminate an edge.

An optional `require condition, "message"` keeps its message across comments
and newlines, including a direct `match` arm. The arm's following separator is
separate from the message comma; formatting cannot detach the failure message.

Entry-source qualifiers use the canonical prefix form, such as
`witness value: T`, `protected cell: T` and `lock_args owner: Address`.
Every action and Lock parameter declares its source explicitly; `read name: T`
is the read-only CellDep source. A markerless entry parameter receives E1032
after resolved typing, ownership and ABI checks, with the corresponding prefix
spelling. Ordinary helper parameters remain unqualified arguments. A leading
`mut` on an admitted witness value makes its local binding mutable, as in
`mut witness count: u64`; it grants no write authority over transaction data.
Cell references, protected/Lock-args ports and bounded owner/view parameters
retain their independent immutability restrictions.
Postfix `value: witness T`, `cell: protected T` and `owner: lock_args Address`
receive E1032 with the prefix spelling. The source words remain ordinary nominal
type names in type positions, including qualified paths and type applications.
Actions, Locks and helpers share this parameter parser. Named Cell outputs use
inline `output name: Type` ports and the Unit entry convention. The removed
`-> name: Type` and `-> (first: Type, second: Type)` signatures receive E1032.
Ordinary `-> T` value returns, including anonymous returned Cells and tuple
values, remain distinct from named Cell roles. Declaring an output port binds
a proposed Cell; executable lifecycle policies establish its business guarantees.
The selected artifact supplies coordinates and coverage independently of this
spelling. See the [action tutorial](wiki/Tutorial-09-Action-Model-and-Canonical-Syntax.md).

### Delimited validity predicates

Type validity uses an explicit region inside the type body:

```cellscript
resource PositiveAmount has store {
    amount: u64,
    validity {
        require amount > 0;
    }
}
```

The region contains at least one pure `require` predicate, each ending in `;`.
Only one validity region is admitted per type; ordinary fields may appear
before or after it. A field named `validity: T` remains an ordinary field,
including with intervening whitespace. Predicate purity, approved environment
reads, helper dependencies and runtime evidence retain their existing checks.
The old unbraced section receives E1032 with the required braced spelling.
Braces do not turn metadata-only or deferred predicates into executable proof.

### Collection storage and bounds

`Vec<T>` does not select one backing model by itself. A supported fixed-width
local vector has 256 bytes of stack backing and capacity `256 / element_width`;
`with_capacity(n)` does not resize it. Encoded Cell fields are views, arena
members follow their named `collections` policy, and bounded Cell selections
and output plans retain separate source and codec contracts. Native
`cellc explain collections --json` report v2 and editor hover show the actual
resolved model, widths and bounds. Declaration reserve/growth scenarios remain
separate from actual storage and do not prove program paths or resource fit.
See the [collections support matrix](CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md#authoring-guidance).

### Named unsigned bounds

Supported bounds accept an unsigned integer literal or a constant path. A named
constant has declared type `u8`, `u16`, `u32`, `u64` or `u128`; its initializer is
an integer literal or another such constant reference. Resolution follows the
defining module, imported aliases and normal package visibility, with at most
32 reference levels. Every declared scalar width is checked before the bound's
own range, alignment and layout checks. Unknown/inaccessible names, cycles,
overflow, signed or non-scalar constants, calls and computed initializers reject.
Arithmetic in a bound is not admitted; this is not user-defined compile-time
execution or a constant-generic parameter system.

```cellscript
const ITEMS: u32 = 3;
const ARENA_BYTES: u32 = 256;
const EVENTS: u32 = 3;

collections Work {
    limits { bytes: ARENA_BYTES, allocations: EVENTS },
    rows: Vec<u64> { items: ITEMS }
}

data_shape Messages {
    element: Vec<u8>,
    limits { items: ITEMS, payload_bytes: 8, encoded_bytes: 128 }
}
```

The same rule covers array lengths such as `[u8; ITEMS]`, the numeric limits
and count-interval endpoints in `cell_roles`, and the existing numeric argument
positions of `NestedValues`, `NestedBytesView`, `NestedRecordView`,
`BoundedCellSet`, `BoundedList` and `WitnessBytesView`. Those constructors retain
their distinct admission, ownership, source and codec contracts. Ordinary
nominal/generic applications continue to take type arguments.

Named bounds normalize to the existing literal type/policy nodes before layout,
specialization and artifact admission. They introduce no new encoded format or
V0 hash spelling. Formatting preserves the authored constant names and comments;
native explanation reports resolved numeric bounds. A literal replacement with
the same value retains the established executable/layout contract.

### Scoped aggregates and executable evidence

`assert_conserved(Type.field)` is the intent form of a matching input/output
sum equality in the same resolved numeric domain. Bare targets inherit the
invariant scope; explicit views must agree with it. Keep declared `reads` and
scope: they identify the invariant's domain and coverage, rather than merely
listing calls found elsewhere in the module.

```cellscript
resource Coin has store { units: u128, }
invariant supply {
    trigger: type_group;
    scope: group;
    reads: group_inputs<Coin>.units, group_outputs<Coin>.units;
    assert_conserved(Coin.units);
}
action verify() {
    xudt::require_group_amount_conserved();
}
```

The established raw group helper checks complete Cell data encoded as exactly
one little-endian u128 at offset zero. Its API name does not require the source
field to be named `amount`. A u64 beside eight unrelated bytes, a shifted u128,
or a larger/dynamic record is a different field domain and cannot acquire
checked conservation evidence from that helper. Explicit `pool` relations keep
their separate resolved field-sum guards and admitted numeric domains.

Native ProofPlan evidence also requires helper coverage on every accepting path
of the selected entry, through actual calls or an emitted unconditional prelude.
An unused helper, another action, a conditional call or an early return cannot
supply missing coverage. Tagged artifacts require every selected variant to be
covered. The common-check callable retains its existing independent scalar/Unit
admission; this rule does not admit arbitrary group helpers into that contract.
An aggregate delta's rendered argument is not a typed binding to a helper
operand, so finding a minted/burned helper alone leaves that delta
runtime-required. Native ProofPlan analysis does not introduce an independent
checker proof for arbitrary source aggregates.

The explicitly selected `fungible-type-group-v1` compiler profile uses an
eligible invariant as a structural domain template and implements a different,
conditional contract: authorized owners may issue/destroy, while non-owners
must conserve checked u128 totals in nonempty input/output groups. Its
`selected-profile-invariant` and `selected-profile-aggregate` records state
owner-or-conservation explicitly; they do not label the source equality
unconditionally checked. Ordinary source names cannot select that profile.
See the [target-profile tutorial](wiki/Tutorial-05-CKB-Target-Profiles.md#fiber-is-an-interoperability-path-not-a-target-profile).

`require` remains an executable condition and grouped `require` a pure
conjunction. `audit` records metadata claims. Neither an audit name nor an
unlowered invariant is runtime authorization evidence.

### Operators and literals

From weakest to strongest: assignment (`=`, `+=`, right-associated), range
(`..`, non-associative), `||`, `&&`, equality (`==`, `!=`), ordering
(`<`, `<=`, `>`, `>=`), bitwise `|`, `^`, `&`, shifts, addition/subtraction,
multiplication/division/remainder, casts, prefix operators, and postfix
call/index/field access. Casts associate left and prefix operators associate
right. Arithmetic, overflow behavior, evaluation order and short circuiting
retain their existing contracts.

Bitwise operators bind more tightly than comparisons: `mask & 1 != 0` means
`(mask & 1) != 0`. Unparenthesized comparison chains, including mixed equality
and ordering, reject; write `a < b && b < c`, or explicitly group a boolean
comparison. `<<`, `>>` and other multi-character operators require contiguous
source characters; comments cannot join separate operator tokens. Nested
closing type arguments are parsed in type context.

Decimal integers and `0x` hexadecimal integers admit single underscores only
between digits and must fit unsigned 128-bit source values. Malformed suffixes,
separators, hex digits and overflow reject as a complete offending literal.
Family-specific bounds still apply to arrays, budgets and bounded codecs.
No integer suffixes or arbitrary constant-expression generics are introduced.

Text uses double quotes and UTF-8. Admitted ASCII escapes are `\n`, `\r`,
`\t`, `\0`, `\\`, `\"` and `\'`; `\u{...}` has one to six hexadecimal digits
and must denote a Unicode scalar. Raw control characters and line breaks,
surrogates, out-of-range scalars and malformed escapes reject. Byte strings
use `b"..."`, ASCII text/escapes and exact `\xNN`; Unicode escapes and non-byte
content reject. Single-quoted string aliases were removed. Identifiers use
Unicode 17.0.0 XID_Start or `_`, then XID_Continue, pinned by
`unicode-ident = "=1.0.24"`, without normalization or case folding. Editor
UTF-16 positions remain distinct from source byte offsets.

### Attribute and permission rules

Attributes precede visibility and the declaration. `type_id` is meaningful only
on admitted concrete resource/shared/receipt/value-record definitions; generic
value records cannot acquire Cell identity through that attribute.
`scheduler_hint` is action-only. Unknown attributes, repeated attributes,
repeated values and conflicting parallel/sequential hints reject. Every
accepted attribute has a consumer. The removed `#[capability(...)]` form has no
alias; lifecycle permissions use `has ...`. Cell capabilities and value
abilities retain their separate semantic domains.

### Source tooling

The public formatter layouts the lossless source document from the same parser.
It preserves exact comment text and token order, grouping, scopes and explicit
discard/tail distinctions. It reparses candidates, compares exhaustive semantic
AST structure and checks idempotence before allowing a write. AST display
rendering is separate from rewriting an existing file.

Line width is a soft boundary: comments and literal contents are not rewritten
to satisfy it. Layout accepts indent widths 0..16, line widths 1..4096 and
0..16 blank lines between items; formatted output has a four-source-budget
cap. Rejection leaves the source intact. CLI package formatting prepares all
candidates before per-file atomic replacements, checks for stale sources and
reports an already-written prefix on an I/O failure. A package replacement is
not a filesystem transaction. LSP formatting uses the captured document and
workspace revision; stale results are rejected by session ownership.

## Counter proof witnesses

In a package with a declared counter verifier policy, an action can take
`witness proof: ZkProof<Policy>` as its sole parameter and call
`zk::require_valid(proof);` as its first statement. The policy identifier
resolves from the package deployment declaration. See the
[proof profiles and entry witness ABI](CELLSCRIPT_ZK_PROFILE.md#package-bound-counter-witness).

## Ordinary successor relations

A relation states its complete data, Lock, Type Script, and capacity treatments:

```cellscript
replace before -> after {
    data = same except {
        owner = recipient,
    };
    lock = exact_hash(recipient_lock_hash);
    type_script = same;
    capacity = same;
}
```

`type_script = same` checks the complete Type Script hash. It does not establish
business-object identity, a unique instance, or preservation of an `id` field.
Those are separate field or declared identity obligations. `capacity = same`
checks exact capacity equality. All four treatments are required; omission
never means that the compiler supplies a policy default.

`lock = same` preserves the complete Lock Script hash. `lock = exact(address)`
accepts an `Address`; `lock = exact_hash(script_hash)` requires the dedicated
`ScriptHash` domain. Typed Cell views expose `cell.lock_hash` and
`cell.type_hash` as `ScriptHash`. The `ckb::cell_lock_hash` and
`ckb::cell_type_hash` functions accept raw selectors from `source::*`.
`ckb::script_hash(hash)` explicitly admits a trusted raw `Hash` into that domain;
this conversion does not establish deployment, authorization, or Script
existence.

Use `data = same;` to preserve every field. For an exhaustive policy, write
one treatment per field:

```cellscript
data {
    collection_id = same,
    token_id = same,
    amount = same,
    owner = recipient,
};
```

```cellscript
data = same except {
    owner = recipient,
};
```

Use `data = same except { ... };` for a partial update with at least one
assignment. Whole-data preservation and partial updates expand against the
concrete schema. Unknown,
duplicate, or incompatible assignments reject. A schema upgrade that changes
this expansion needs exact old/new review evidence; see
[schema acknowledgements](CELLSCRIPT_SCHEMA_ACKNOWLEDGEMENTS.md).

A `replace` relates one predecessor to one successor. That local cardinality
is inherent in the construct. It does not prove that the whole current Script
group has only one input or output.
The complete relation establishes its own lineage; authors need not repeat
`transition before -> after;` for the same pair. A field-qualified transition
such as `transition before.state: Live -> after.state: Filled;` remains a
separate declared-flow check. Likewise `preserve after from before { owner, }`
only compares the selected fields: it establishes neither ownership disposal
nor data completeness, Lock, Type Script or capacity preservation.
The declared flow graph still constrains state changes. A field-qualified
transition selects a particular edge; an edge bound with `by action_name`
requires that action to declare the exact transition. The remaining named-output
fields keep their independent verification requirements.

## Explicit Type Script group

```cellscript
module cellscript::examples::semantic_foundation_2027;

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

action transfer(
    input token: Token,
    witness recipient: Address,
    output next: Token,
) {
    require token.amount > 0;

    replace token -> next {
        data {
            owner = same,
            amount = same,
        };
        type_script = same;
        lock = exact(recipient);
        capacity = same;
    }
}
```

The runnable package is
[`examples/semantic-foundation-2027`](../examples/semantic-foundation-2027/README.md).
The package selects that action and its Type group context in `Cell.toml`:

```toml
[[artifacts]]
name = "token-transfer"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "single-entry"
entry = "transfer"
```

`single-entry` selects one action and uses the ordinary entry witness ABI.
A package with one matching single-entry declaration can use the default
build. `--artifact token-transfer` selects it explicitly. A package with
multiple matching declarations selects one by name. To share a deployed
Type Script across several operations, name the actions and their numeric
tags in a [`policy-witness-v1` artifact](CELLSCRIPT_POLICY_WITNESS_ABI.md).

The artifact states its execution context, role bindings, coverage, layout,
and dispatch separately. Transaction Cell input roles use parameter
declaration order. Witnesses and CellDeps use their own coordinate systems.
Inline output roles use declaration order. The selected artifact supplies
the action's physical coordinate system. Ordinary builds use
transaction-absolute binding.
`cellc expand --artifact <name>` shows the resolved roles and obligations.
VS Code hover uses the active artifact. Set `cellscript.entryArtifact` and
reload the window to select among multiple artifacts in one package.

`consume binding;` ends ownership of one named linear Cell binding. It
contributes Unit to the enclosing block and is admitted only as a statement.
Use `replace` for a checked successor, `pool` for pooled accounting,
`fresh` for a new output, and `retire` for an explicit absence policy. A plain
`consume` records linear disposal with an unspecified business disposition.
See the [action tutorial](wiki/Tutorial-09-Action-Model-and-Canonical-Syntax.md)
for worked examples.

| Context | Bindings | Coverage | Layout | Supported dispatch |
| --- | --- | --- | --- | --- |
| Type group | `group-relative` | `fixed-group` | One named Cell-backed schema | `single-entry`, `policy-witness-v1` |
| Type group | `transaction-absolute` | `action-defined` | Layouts used by the action | `single-entry` |
| Lock group | `group-relative` | `authorization-only` | No Type-group layout | `single-entry` |

Fixed coverage checks the complete current Type group. The compiler derives
the exact input and output counts from the resolved roles and rejects extra
group members. Structured lifecycle roles must be accounted for once on every
accepting path. Conditional relations account for their own execution paths.

A fixed-group action can also check foreign funding and settlement Cells
through typed transaction views. A `ckb::input::<Funding>(index)` binding
observes a transaction input. Use a Script hash check when the foreign Cell
must have a particular identity. Read-only CellDep references use their own
source. Another Script can verify the same funding Cell in the same
transaction. [CKB Script group execution](https://docs.nervos.org/docs/tech-explanation/script-group-exe)

An action-defined artifact keeps coverage in the body. For example, a
hand-written `action verify()` can scan the current group, read distinct
schemas, and use absolute transaction positions alongside those reads.

```toml
[[artifacts]]
name = "asset"
context = { kind = "type-group" }
bindings = "transaction-absolute"
coverage = "action-defined"
dispatch = "single-entry"
entry = "verify"
```

## Explicit Lock Script group

```cellscript
module cellscript::examples::lock_script_2027;

resource Vault has store {
    owner: Address,
}

lock unlock(
    protected vault: Vault,
    lock_args owner: Address,
    witness claimed_owner: Address,
) {
    require vault.owner == owner;
    require claimed_owner == owner;
    true
}
```

The runnable syntax fixture is
[`examples/lock-script-2027`](../examples/lock-script-2027/README.md).
Its manifest selects the Lock entry explicitly:

```toml
[[artifacts]]
name = "vault-lock"
context = { kind = "lock-group" }
bindings = "group-relative"
coverage = "authorization-only"
dispatch = "single-entry"
entry = "unlock"
```

Helpers and other selectable entries may follow the Lock declaration. The
bounded explicit Lock artifact admits zero or one protected Cell view and
retains read-only CellDep ports. It does not introduce tagged Lock dispatch
or claim fixed Type-group input/output coverage. The removed `lock_script`
declaration receives E1032; names such as `fn lock_script` and the separately
defined `ckb::lock_script` API remain contextual ordinary names.

These public-value equalities demonstrate source binding, not authenticated
ownership. A real ownership Lock needs a proof bound to the transaction and
owner credential. Its `AuthorizationOnly` semantic disposition records the
Lock responsibility separately from data, successor, identity, and capacity
responsibilities belonging to a Type Script or another explicit policy.

## Other fixed-role relations

Type Script entries support bounded pooling, retirement, fresh creation and
metadata-only audits directly in the action body. Named policy clauses may
appear in any order. Executable `pool`, `retire`, `fresh` and `replace` policies
use shared statement syntax and compose in checked action branches. Every
accepting path must account for each affected role exactly once. Missing or
overlapping dispositions and repeated lifecycle operations in loops reject.
These policies require declared action roles; they are not helper declarations.
When a structured policy and an ordinary `consume` or named `create` account
for the same role on different branches, each keeps its actual executable
instruction binding. A conditional lower-level output check by itself does
not require that output check to execute on every accepting path.
Existing field-constraint completeness checks still apply to the other paths.
An inferred fixed-group kernel successor with an explicit destination Lock
retains its actual consume/create sites and builder-set capacity policy. It
does not acquire the capacity equality required by complete `replace`.
`audit` remains an entry-level metadata declaration and never supplies an
executable branch condition.

Named data fields in `fresh` and `pool` may appear in any order, with every
schema field present exactly once. The resolved schema owns encoded field
offsets. Fresh initializers and each output's pool initializers retain authored
field order; pool sum guards precede output initialization. Local, imported and
aliased schemas use the same resolved initializer checks. Reordering a source
list is therefore not a formatter normalization or a promise of identical ELF
bytes.

```cellscript
pool value_flow {
    inputs { left, right, };
    outputs { merged, };
    data {
        owner { merged = recipient, },
        amount = conserve,
    };
    lock { merged = exact(recipient), };
}
```

A pool names non-empty, unique local input and output sets. At least one
unsigned numeric field uses `conserve`, which generates an overflow-checked
`u128` sum-equality over the named roles. Other fields and each output Lock
have exact assignments in declared output order. The operation checks Type
Script group membership only when the resolved roles belong to the current
Type group. Ordinary transaction-absolute roles do not acquire Type Script
equality from the nominal Cell schema. Pooling uses quantity accounting and
leaves capacity planning to the transaction builder.

```cellscript
retire expired {
    absence = field(note_id);
}

fresh next {
    data {
        note_id = next_id,
        owner = recipient,
        amount = 0,
    };
    identity = field(note_id);
    lock = exact(recipient);
}
```

Retirement requires an exact `field(...)`, `ckb_type_id`, or `singleton_type`
absence policy. Fresh creation initializes all fields, names its output role,
and constrains the destination Lock. Its identity clause is optional: omission
derives the resolved Cell declaration's policy, including the non-unique `none`
policy. An explicit clause must match that declaration; `identity = none` cannot
disable a declared unique identity. Fresh identity
policies are `none`, `ckb_type_id`, `script_args`, `singleton_type`, or
`field(name)`. Retirement has no successor data, Lock, Type Script or capacity
relation. Fresh creation uses the output role's resolved physical binding:
fixed group coverage selects a `group-output` coordinate, while ordinary or
action-defined transaction-absolute bindings select an `output` coordinate.
The nominal Cell schema alone does not select a Script group. Capacity planning
remains with the builder under the applicable declaration/profile checks.
`retire` and `fresh` each name one role. A `pool` derives its input and output
counts from its role lists.

Native `cellc expand` and Cell-role/entry hover share a resolved policy view.
For data, identity/accounting, Lock, Type Script, capacity and cardinality it
shows `explicit`, `derived-and-enforced`, `unconstrained-by-this-relation`, or
`not-applicable`. An explicit policy can originate in the Cell declaration or
the operation contract, including an omitted identity request resolved from
that declaration. The separate evidence label remains authoritative: an
explicit policy with `builder-evidence-required` is not a runtime proof.
The view also shows actual role selectors, selected entry, coverage and fixed
group counts. Local relation cardinality does not establish full group coverage.
Retirement identifies its actual group-output or transaction-output scan,
filtered by the input Type Script hash. A declared capacity floor is displayed
separately from predecessor/successor equality; the floor keeps its own verifier
obligations. Hover bounds long details and points to the complete expansion.
The browser summary omits these native policy records and directs users to
native expansion rather than reconstructing evidence from a type name.

For example, a type declaring `identity(field(id))` can use
`fresh next { data { id = new_id, }; lock = exact(owner); }` or
`create_unique::<Note>() { id: new_id, } with_lock(owner)`. Its identity policy
is resolved before kernel elaboration and appears as `field:id` in semantic
expansion. No Lock, capacity or Type ID builder contract is inferred merely
because the source omitted the identity request. In particular, CKB TYPE_ID
creation keeps its separate declared Type ID and builder-plan requirements.
The shared [declaration-derived identity fixture](../tests/fixtures/authoring/declared-identity.cell)
also exercises replacement and explicit conflict rejection has its own corpus
fixture. Retirement's absence policy remains mandatory and independent.

The advanced `replace_unique::<Note>(identity = field(id)) before { id: before.id, }`
form retains its declared identity-continuity policy and anonymous output
semantics. It has a different envelope from complete `replace`; Lock and capacity
equality are not supplied by that syntax. `claim voucher` and `settle before`
are prefix expressions that can provide admitted tail values. Receipt claims
retain their declared resource/shared result type; settlement retains its
applicable state/output obligations. Their actual input/result coordinates and
remaining evidence boundaries are shown by `cellc expand`. See the shared
[unique/retire](../tests/fixtures/authoring/branch-unique-retire.cell),
[receipt](../tests/fixtures/authoring/receipt-claim-kernel.cell) and
[settlement](../tests/fixtures/authoring/settle-kernel.cell) fixtures.
Omitting a receipt's Cell result keeps the existing scalar fallback
runtime-required; an explicit scalar claim-result declaration remains rejected.

The lower-level `burn_amount(cell, field = amount)` requests quantity
destruction, independently of object retirement. Its quantity-delta verifier
remains runtime-required. Metadata does not claim output absence for it;
strict executable admission rejects the unsupported path, and permissive
fail-closed artifacts abort with `quantity-burn-runtime-required` (88).
Use `retire` only when an exact object-absence policy is the intended rule.

```cellscript
audit settlement_policy {
    expected_evidence = external_policy(recipient);
}
```

An audit subject must be pure, type-correct, and cannot capture a Cell-backed
linear value directly. Audit evidence has no executable condition or accepting
branch. It never replaces a `require`, authorization proof, or migration.

## Group entry grammar

Expressions, types and ordinary statements use the shared parser.

```ebnf
group-action = "action", identifier, "(", list<type-port>, ")",
               "{", type-statement*, "}" ;
type-port    = ("input" | "output"), identifier, ":", cell-type | witness-port ;
lock-entry   = "lock", identifier, "(", list<lock-port>, ")", ("->", "bool")?,
               "{", lock-statement*, tail-expression?, "}" ;
lock-port    = "protected", identifier, ":", cell-type
             | "lock_args", identifier, ":", type
             | "read", identifier, ":", cell-type | witness-port ;
witness-port = "witness", identifier, ":", type ;
```

An artifact can select actions anywhere in its entry module. A fixed Type
group action needs at least one Cell role and uses the unit status convention.
A selected Lock returns a boolean and may be placed anywhere in its source
module. Its explicit artifact admits at most one protected Cell view.

| Role | Resolved source |
| --- | --- |
| `input`, `output` | Input or output in the selected role coordinate system |
| `protected` | Lock group input zero |
| `lock_args` | Current Lock Script arguments |
| `read` | Read-only CellDep view, without group membership authority |
| `witness` | `WitnessArgs.input_type` at the first group input, with the existing output fallback |

Expansion and entry metadata retain these physical bindings and declared
witness order. The [multi-action policy contract](CELLSCRIPT_POLICY_WITNESS_ABI.md)
selects its entry points separately.

### Edition 2027 source spelling changes

| Before | Current spelling |
| --- | --- |
| `type_script transfer(input token: Token, output next: Token)` | `action transfer(input token: Token, output next: Token)` |
| The declaration keyword selected the Type group and coverage | The artifact declares `context`, `bindings`, `coverage`, and `layout` |
| A Type Script declaration was the module's only entry | A module declares several actions and the artifact selects its exports |

The group entry keeps its callable name, role order, and witness encoding and
placement across these spelling changes.

## Structured lowering and evidence

The parser retains declaration spelling and spans for formatting and source
maps. It generates structured AST/IR and semantic records directly; printed
expansion is never generated and reparsed as a second source language.
`cellc expand` displays the resolved obligations, roles, fields, source bindings,
conditions, and metadata-only claims for inspection. Successor execution records
bind each field and destination Lock to actual typed Create operands. Human
output includes the referenced local-definition DAG; JSON keeps structured
operands and CFG locations. Display text is not a substitute for those checked
value bindings or proof of arbitrary business semantics.

The checker independently verifies the bounded typed, CFG, ABI, machine, and
semantic-foundation contracts. Source and source-map identities remain separate
from semantic identities. The execution context, physical Cell role contract,
and witness dispatch contribute to the entry-contract identity. For a fixed
group, the checker binds the declared layout and role count to the typed record
and checks the exact cardinality guards in the ELF.

Current closure needs agreement across native parsing and diagnostics,
formatting, type and flow checks, lowering, metadata and the independent
checker, CLI, manifest-aware LSP, browser metadata/language services, editor
snippets, runnable examples, syntax-combination seeds, and VM positive and
negative cases. Neither metadata compilation alone nor a formatter round-trip
establishes on-chain behavior or production acceptance.

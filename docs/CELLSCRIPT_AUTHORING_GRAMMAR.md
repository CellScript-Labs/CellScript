# CellScript Authoring Grammar

## Status and scope

Edition 2027 uses one business-language vocabulary: `resource`, `action`,
`lock`, `require`, ordinary expressions and control flow, and structured
lifecycle relations. Explicit `type_script` and `lock_script` containers add
Script-group and physical source bindings. Their bodies use the same ordinary
statements; they do not introduce another condition language.

The syntax remains an evolving alpha contract. This document describes its
bounded executable surface, not a CellScript 1.0 grammar freeze or production
acceptance. Issue [#98](https://github.com/CellScript-Labs/CellScript/issues/98)
tracks the authoring convergence. The [authoring target](CELLSCRIPT_AUTHORING_TARGET.md)
and [semantic foundation](CELLSCRIPT_SEMANTIC_FOUNDATION.md) define the broader
requirements and independent verification boundary.

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

## Explicit source syntax: authoring-v3

The current source-semantics identity is
`cellscript-source-semantics-2027-0.33-authoring-v3`. There is one current
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
              | "require", (expression | "{", boolean-list, "}"), message?, ";"
              | assignment, ";" | expression, ";" | block-statement ;
block-statement = if-statement | for-statement | while-statement
                | labelled-loop | borrow-scope | lifecycle-block ;
constructor   = "new", type-path, "{", field-initializers?, "}" ;
generic-call  = callable, "::", "<", type-list, ">", "(", arguments?, ")" ;
type          = scalar-type | path | type-application | "()"
              | "(", type, ")" | "(", type, ",", type-list?, ")"
              | "[", type, ";", unsigned-integer, "]"
              | "&", "mut"?, type ;
list<T>       = (T, (",", T)*, ","?)? ;
```

This kernel is read together with the domain productions and restrictions
below. In particular, not every syntactically valid expression has executable
support. Existing type, ownership, source-binding, bounded-layout and target
admission checks continue to apply.

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

### Declaration and domain coverage

| Family | Syntax boundary and admission |
| --- | --- |
| Imports and constants | `use path::{name, ...};`, optional aliases; `const NAME: Type = expression;` |
| Value records and enums | `struct`/`enum`, optional bounded type parameters and value abilities; comma-separated fields/variants; existing public fixed-layout admission |
| Cell-backed types | `resource`, `shared`, `receipt`; one `has ...` capability declaration; concrete schemas and existing identity/hash/capacity modifiers |
| Ordinary callables | `fn`, `action`, `lock`; comma-separated ports/parameters; retained source qualifiers, return/output signatures and action transition clauses |
| State transitions | Existing state-path/state-name forms; transition clauses and flow edges end in `;`; initial/terminal state lists retain their explicit terminators |
| Invariants and validity | Existing closed aggregate/quantifier forms and pure predicates; predicate/count/assertion clauses end in `;`; braced `forall` retains its finite source-view restriction |
| Collections | `collections`, named allocation limits and typed Vec/OrderedMap/OrderedSet members; comma-separated entries and unchanged finite bounds |
| Nested codecs | `data_shape`, typed element and distinct named limits; no implicit witness budget or source authority |
| Named Cell roles | `cell_roles`, named finite scan/selection/byte limits and explicit role policies; comma-separated policy entries; no inferred authority |
| Ordinary lifecycle | Existing `consume`, destruction policies, claim/settle, create/read operations and bounded `consume_each`/`create_each`; executable restrictions remain explicit |
| Successors and preservation | `replace` has all four mandatory treatments; `preserve after from before { fields, }` is an explicit field whitelist, without implicit disposition |
| Group-bound policies | `type_script`/`lock_script`, explicit physical ports and bounded `pool`, `retire`, `fresh`; policy assignments end in `;`, name lists use commas |
| Audit | `audit name { expected_evidence = external_policy(value); }`; metadata-only evidence, never an executable accepting condition |
| Patterns and control | Name/tuple/wildcard bindings, existing match patterns and expression/block arms, labelled break/continue, if/for/while and statement-scoped borrow/reborrow |

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
`ScriptHash` domain. Typed CKB Script-hash reads already produce `ScriptHash`.
`ckb::script_hash(hash)` explicitly admits a trusted raw `Hash` into that domain;
this conversion does not establish deployment, authorization, or Script
existence.

Data has two useful forms:

```cellscript
data {
    same { collection_id, token_id, amount, },
    owner = recipient,
};
```

```cellscript
data = same except {
    owner = recipient,
};
```

The first form explicitly covers every concrete schema field once. The second
expands unlisted fields as preserved after concrete schema resolution. Unknown,
duplicate, or incompatible assignments reject. A schema upgrade that changes
this expansion needs exact old/new review evidence; see
[schema acknowledgements](CELLSCRIPT_SCHEMA_ACKNOWLEDGEMENTS.md).

A `replace` relates one predecessor to one successor. That local cardinality
is inherent in the construct. It does not prove that the whole current Script
group has only one input or output.

## Explicit Type Script group

```cellscript
module cellscript::examples::semantic_foundation_2027;

resource Token has store, replace, relock {
    owner: Address,
    amount: u64,
}

type_script TokenTransfer on type_group<Token> {
    action transfer(
        input token: Token from group_input[0],
        witness recipient: ScriptHash from group_witness.input_type,
        output next: Token from group_output[0],
    ) {
        require token.amount > 0;
        replace token -> next {
            data { owner = same, amount = same, };
            lock = exact_hash(recipient);
            type_script = same;
            capacity = same;
        }
    }
}
```

The runnable package is
[`examples/semantic-foundation-2027`](../examples/semantic-foundation-2027/README.md).
The container declares the exact `type-group<Token>` trigger. Its ordinary
`action` body can use local bindings, conditionals, matches, and checked
relations. On every accepting path each declared Cell role must have exactly
one disposition; failing paths need not complete a transition. A relation in
one branch does not discharge a role in another branch. Lifecycle disposal in
loops remains rejected.

Explicit group ordinals bind to the running Script group, independently of
absolute transaction positions. Fixed relations also check group coverage:
extra current-group inputs or outputs cannot escape the declared dispositions.
Unrelated Script groups remain outside that local coverage obligation.

## Explicit Lock Script group

```cellscript
module cellscript::examples::lock_script_2027;
resource Vault has store { owner: Address, }

lock_script VaultOwner on lock_group {
    lock unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
    }
}
```

The runnable syntax fixture is
[`examples/lock-script-2027`](../examples/lock-script-2027/README.md).
These public-value equalities demonstrate source binding, not authenticated
ownership. A real ownership Lock needs a proof bound to the transaction and
owner credential. Its `AuthorizationOnly` semantic disposition records the
Lock responsibility separately from data, successor, identity, and capacity
responsibilities belonging to a Type Script or another explicit policy.

## Other fixed-role relations

The same Type Script body retains bounded pooling, retirement, fresh creation,
and metadata-only audit declarations. Their explicit policies have distinct
meaning and remain required.

```cellscript
pool value_flow {
    inputs { left, right }
    outputs { merged }
    data {
        owner { merged = recipient; }
        amount = conserve;
    }
    identity = pooled;
    type_script = same;
    lock { merged = exact(recipient); }
    capacity = builder_computed;
    cardinality = declared;
}
```

A pool names non-empty, unique local input and output sets. At least one
unsigned numeric field uses `conserve`, which generates an overflow-checked
`u128` sum-equality over the named roles. Other fields and each output Lock
have exact assignments in declared output order. Builder-computed capacity is
not an artifact-local capacity equality. A pooled identity policy does not
invent persistent individual identities for fungible Cells.

```cellscript
retire; expired; {
    absence = field(note_id);
    data = discarded;
    lock = none;
    type_script = absent;
    capacity = released;
    cardinality = one;
}

fresh; next; {
    data; { note_id = next_id; owner = recipient; amount = 0; }
    identity = field(note_id);
    type_script = declared;
    lock = exact(recipient);
    capacity = builder_computed;
    cardinality = one;
}
```

Retirement requires an exact `field(...)`, `ckb_type_id`, or `singleton_type`
absence policy. Fresh creation initializes all fields, declares its identity
policy and output role, and constrains the destination Lock. Fresh identity
policies are `none`, `ckb_type_id`, `script_args`, `singleton_type`, or
`field(name)`. Existing identity and lifecycle type checks remain authoritative.

```cellscript
audit; settlement_policy; {
    expected_evidence = external_policy(recipient);
}
```

An audit subject must be pure, type-correct, and cannot capture a Cell-backed
linear value directly. Audit evidence has no executable condition or accepting
branch. It never replaces a `require`, authorization proof, or migration.

## Bounded container grammar

Expressions, types, ordinary statements, `require` blocks, and successor data
plans use the shared parser. Container headers and source bindings add:

```ebnf
group-module       = module-header, ordinary-non-entry-declaration*, script-container ;
script-container   = type-container | lock-container ;
type-container     = "type_script", identifier, "on", "type_group",
                     "<", cell-type, ">", "{", type-action, "}" ;
type-action        = "action", identifier, "(", type-port-list?, ")",
                     "{", type-statement*, "}" ;
type-port-list     = type-port, (",", type-port)*, ","? ;
type-port          = input-port | output-port | witness-port ;
input-port         = "input", identifier, ":", cell-type,
                     "from", "group_input", "[", ordinal, "]" ;
output-port        = "output", identifier, ":", cell-type,
                     "from", "group_output", "[", ordinal, "]" ;
witness-port       = "witness", identifier, ":", type,
                     "from", "group_witness", ".", "input_type" ;
lock-container     = "lock_script", identifier, "on", "lock_group",
                     "{", lock-entry, "}" ;
lock-entry         = "lock", identifier, "(", lock-port-list, ")",
                     "{", lock-statement*, "}" ;
lock-port-list     = lock-port, (",", lock-port)*, ","? ;
lock-port          = protected-port | witness-port | lock-args-port ;
protected-port     = "protected", identifier, ":", cell-type,
                     "from", "group_input", "[", "0", "]" ;
lock-args-port     = "lock_args", identifier, ":", type,
                     "from", "current_script", ".", "args" ;
```

Port lists are comma-separated and may have a trailing comma. Exactly one
container is present, as the final top-level declaration, with one `action` or
`lock`; ordinary top-level entry declarations cannot be mixed into that module.
This is the bounded single-entry container contract. It does not replace the
separately selected [multi-action policy contract](CELLSCRIPT_POLICY_WITNESS_ABI.md).

Type ports have the declared trigger schema and explicit sequential zero-based
group ordinals. Output-producing relations follow the declared output order.
Lock containers have exactly one Cell-backed `protected` role at group input
zero and cannot dispose of Cells. Other ordinary source forms and bounded
collection facilities retain their own admission boundaries.

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
from semantic identities. Moving from an implicit group trigger to an exact
`type-group<T>` binding intentionally changes the entry-contract identity.

Current closure needs agreement across native parsing and diagnostics,
formatting, type and flow checks, lowering, metadata and the independent
checker, CLI, manifest-aware LSP, browser metadata/language services, editor
snippets, runnable examples, syntax-combination seeds, and VM positive and
negative cases. Neither metadata compilation alone nor a formatter round-trip
establishes on-chain behavior or production acceptance.

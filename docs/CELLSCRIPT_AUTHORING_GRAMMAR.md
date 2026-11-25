# CellScript Authoring Grammar

## Status and scope

Edition 2027 uses one business-language vocabulary: `resource`, `action`,
`lock`, `require`, ordinary expressions and control flow, and structured
lifecycle relations. A Type group artifact binds one or more named `action`
declarations to that group. `lock_script` declares a group-bound Lock entry.

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

## Explicit source syntax: authoring-v8

The current source-semantics identity is
`cellscript-source-semantics-2027-0.33-authoring-v8`. There is one current
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
| Invariants and validity | An aggregate requires an enclosing `scope`. Bare targets inherit it and explicit views must agree with it. Predicate/count/assertion clauses end in `;`. `forall binding in view<T>` derives its role from the finite view |
| Collections | `collections`, named allocation limits and typed Vec/OrderedMap/OrderedSet members; comma-separated entries and unchanged finite bounds |
| Nested codecs | `data_shape`, typed element and distinct named limits; no implicit witness budget or source authority |
| Typed Cell data reads | `cell.data.field` and `cell.data.rows[index].field` for fixed schemas. A row array contains fixed, serializable value records with unsigned scalar, hash, address, or fixed-byte fields |
| Named Cell roles | `cell_roles`, named finite scan/selection/byte limits and explicit role policies; comma-separated policy entries; no inferred authority |
| Ordinary lifecycle | Existing `consume`, destruction policies, claim/settle, create/read operations and bounded `consume_each`/`create_each`; executable restrictions remain explicit |
| Successors and preservation | `replace` has all four mandatory treatments; `preserve after from before { fields, }` is an explicit field whitelist, without implicit disposition |
| Group-bound policies | Type group artifacts select `action` declarations. Fixed roles and `pool`, `retire`, `fresh` use the selected context. `lock_script` binds a Lock group. Policy clauses end in `;` and lists use commas |
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
action = "transfer"
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

| Bindings | Coverage | Layout | Supported dispatch |
| --- | --- | --- | --- |
| `group-relative` | `fixed-group` | One named Cell-backed schema | `single-entry`, `policy-witness-v1` |
| `transaction-absolute` | `action-defined` | Layouts used by the action | `single-entry` |

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
action = "verify"
```

## Explicit Lock Script group

```cellscript
module cellscript::examples::lock_script_2027;

resource Vault has store {
    owner: Address,
}

lock_script unlock(
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
These public-value equalities demonstrate source binding, not authenticated
ownership. A real ownership Lock needs a proof bound to the transaction and
owner credential. Its `AuthorizationOnly` semantic disposition records the
Lock responsibility separately from data, successor, identity, and capacity
responsibilities belonging to a Type Script or another explicit policy.

## Other fixed-role relations

Type Script entries support bounded pooling, retirement, fresh creation and
metadata-only audits. Named policy clauses may appear in any order.

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
Script hash preservation, uses pooled identity and leaves capacity planning
to the transaction builder.

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
absence policy. Fresh creation initializes all fields, declares its identity
policy and output role, and constrains the destination Lock. Fresh identity
policies are `none`, `ckb_type_id`, `script_args`, `singleton_type`, or
`field(name)`. Retirement has no successor data, Lock, Type Script or capacity
relation. Fresh creation uses the output role’s Type Script group and leaves
capacity planning to the builder.
`retire` and `fresh` each name one role. A `pool` derives its input and output
counts from its role lists.

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
lock-entry   = "lock_script", identifier, "(", list<lock-port>, ")",
               "{", lock-statement*, tail-expression?, "}" ;
lock-port    = "protected", identifier, ":", cell-type
             | "lock_args", identifier, ":", type | witness-port ;
witness-port = "witness", identifier, ":", type ;
```

An artifact can select actions anywhere in its entry module. A fixed Type
group action needs at least one Cell role and uses the unit status convention.
A `lock_script` declares one protected Cell, returns a boolean, and is the
final entry in its source module.

| Role | Resolved source |
| --- | --- |
| `input`, `output` | Input or output in the selected role coordinate system |
| `protected` | Lock group input zero |
| `lock_args` | Current Lock Script arguments |
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

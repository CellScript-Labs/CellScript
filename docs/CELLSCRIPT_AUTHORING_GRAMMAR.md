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

## Ordinary successor relations

A relation states its complete data, Lock, Type Script, and capacity treatments:

```cellscript
replace before -> after {
    data = same except {
        owner = recipient
    }
    lock = exact_hash(recipient_lock_hash)
    type_script = same
    capacity = same
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
    same { collection_id, token_id, amount }
    owner = recipient
}
```

```cellscript
data = same except {
    owner = recipient
}
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
module cellscript::examples::semantic_foundation_2027

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
        require token.amount > 0
        replace token -> next {
            data { owner = same; amount = same }
            lock = exact_hash(recipient)
            type_script = same
            capacity = same
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
module cellscript::examples::lock_script_2027
resource Vault has store { owner: Address }

lock_script VaultOwner on lock_group {
    lock unlock(
        protected vault: Vault from group_input[0],
        lock_args owner: Address from current_script.args,
        witness claimed_owner: Address from group_witness.input_type,
    ) {
        require vault.owner == owner
        require claimed_owner == owner
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
        owner { merged = recipient }
        amount = conserve
    }
    identity = pooled
    type_script = same
    lock { merged = exact(recipient) }
    capacity = builder_computed
    cardinality = declared
}
```

A pool names non-empty, unique local input and output sets. At least one
unsigned numeric field uses `conserve`, which generates an overflow-checked
`u128` sum-equality over the named roles. Other fields and each output Lock
have exact assignments in declared output order. Builder-computed capacity is
not an artifact-local capacity equality. A pooled identity policy does not
invent persistent individual identities for fungible Cells.

```cellscript
retire expired {
    absence = field(note_id)
    data = discarded
    lock = none
    type_script = absent
    capacity = released
    cardinality = one
}

fresh next {
    data { note_id = next_id; owner = recipient; amount = 0 }
    identity = field(note_id)
    type_script = declared
    lock = exact(recipient)
    capacity = builder_computed
    cardinality = one
}
```

Retirement requires an exact `field(...)`, `ckb_type_id`, or `singleton_type`
absence policy. Fresh creation initializes all fields, declares its identity
policy and output role, and constrains the destination Lock. Fresh identity
policies are `none`, `ckb_type_id`, `script_args`, `singleton_type`, or
`field(name)`. Existing identity and lifecycle type checks remain authoritative.

```cellscript
audit settlement_policy {
    expected_evidence = external_policy(recipient)
}
```

An audit subject must be pure, type-correct, and cannot capture a Cell-backed
linear value directly. Audit evidence has no executable condition or accepting
branch. It never replaces a `require`, authorization proof, or migration.

## Bounded container grammar

Expressions, types, ordinary statements, `require` blocks, and successor data
plans use the shared parser. Container headers and source bindings add:

```ebnf
group-module       = ordinary-non-entry-declaration*, script-container ;
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

# Tutorial 09: Action Model and Canonical Syntax

An `action` verifies one proposed Cell transformation in a CKB transaction.
Its signature names the inputs, outputs and witness data the verifier needs.
The artifact selects the action, the coordinates of its Cell roles and the
coverage it must enforce.

| What the transaction does | Write | What it expresses |
| --- | --- | --- |
| Updates one Cell as the successor of another | `replace` | The predecessor, successor and treatments for data, Lock, Type Script and capacity |
| Accounts for a set of inputs and outputs together | `pool` | The two sets, the conserved fields and the checks on each output |
| Creates a new output | `fresh` | The output data, identity policy and Lock |
| Ends an object under an absence rule | `retire` | The input and the rule for checking that it has no prohibited successor |

Use `require` for additional conditions such as a price bound or a deadline.
A lifecycle relation records the roles and guarantees the compiler can carry
through to transaction construction and review.

## State continuation with `replace`

```cellscript
module tutorial::vault;

resource Vault has store, create, consume, replace, relock, burn {
    owner: Address,
    balance: u64,
}

action handover(
    input before: Vault,
    witness recipient: Address,
    output after: Vault,
) {
    require before.balance > 0;
    replace before -> after {
        data = same except {
            owner = recipient,
        };
        lock = exact(recipient);
        type_script = same;
        capacity = same;
    }
}
```

The relation ends ownership of `before` and binds its successor to `after`.
The balance stays the same, the data owner and Lock go to the recipient, and
the complete Type Script and capacity are preserved. This one relation
expresses the continuation and accounts for the input. Preserve any business
identity field that the protocol needs to retain. See the
[successor grammar](../CELLSCRIPT_AUTHORING_GRAMMAR.md#ordinary-successor-relations).

For this example, put the following declaration in the package's `Cell.toml`.
The fixed group strategy checks the exact counts derived from the roles in
the signature.

```toml
[[artifacts]]
name = "handover"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Vault"
dispatch = "single-entry"
entry = "handover"
```

Inputs bind in declaration order. Witness values use their own positions.
Run `cellc expand --artifact handover` to review the resolved bindings and
dispositions. See the [runnable Type group package](../../examples/semantic-foundation-2027/README.md)
for the complete package workflow.

## Creation, pooled accounting and retirement

The three actions below share a `Token` schema. Select each action in its own
artifact with `layout = "Token"` and the corresponding `action` name.

```cellscript
module tutorial::token_lifecycle;

resource Token has store, create, consume, burn {
    amount: u64,
}

action issue(witness recipient: Address, output issued: Token) {
    fresh issued {
        data { amount = 7, };
        identity = none;
        lock = exact(recipient);
    }
}

action merge(
    input left: Token,
    input right: Token,
    witness recipient: Address,
    output merged: Token,
) {
    pool value_flow {
        inputs { left, right, };
        outputs { merged, };
        data { amount = conserve, };
        lock { merged = exact(recipient), };
    }
}

action close(input token: Token) {
    retire token {
        absence = singleton_type;
    }
}
```

`fresh` supplies every field and chooses an identity policy. In this example
`none` means the creation has no additional uniqueness policy. Issue
authorization belongs to the protocol's transaction checks.

`pool` checks the sum of `amount` across the named inputs and outputs. A merge
expresses accounting for a set of Cells without picking one input to continue
as the identity of the merged output.

`retire` makes the absence rule explicit. Choose the policy to match what the
protocol treats as an instance, such as a Type ID or a business identity
field. The [fixed-role grammar](../CELLSCRIPT_AUTHORING_GRAMMAR.md#other-fixed-role-relations)
lists the supported policies and their bounds.

## Flow state edges

A field-level clause such as
`transition before.state: Live -> after.state: Filled;` checks an edge of
the declared `flow`. It answers which state change is allowed. The
successor relation answers how the Cell's data, scripts and capacity are
treated. Use `replace_unique` with the supported identity policy when the
protocol requires unique identity continuity. See the
[flow syntax](Tutorial-02-Language-Basics.md#actions).

## Manual checks and library operations

Use `consume binding;` when an action needs to account for a linear input
and defines the business rule through its own checks. The resulting record
has an unspecified business disposition. Field comparisons and
`preserve` can enforce additional equalities. The action still needs the
appropriate input and output checks for its protocol.

Names such as `claim`, `settle` and `transfer` describe business operations.
They fit naturally as action names or as standard library operations when
a library supplies the required checks.

## Lock entries

Locks check spending authorization. Protect a Cell through its Lock group
and explicitly verify the authority required by the protocol. Witness data
alone carries no authority. See the
[Lock group example](../CELLSCRIPT_AUTHORING_GRAMMAR.md#explicit-lock-script-group).

## Syntax conventions

Statements and policy clauses end with `;`. Lists use commas and may have
a trailing comma. Use `new Type { ... }` for ordinary value construction
and `function::<Type>(...)` for generic calls. A final expression without a
semicolon supplies the block's value. See the
[complete Edition 2027 grammar](../CELLSCRIPT_AUTHORING_GRAMMAR.md).

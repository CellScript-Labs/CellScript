# CellScript Linear Ownership

**Status**: production semantics for the current CellScript CKB profile.

CellScript treats cell-backed resources as linear values. A linear value cannot
be copied, silently dropped, or used after it has been consumed, destroyed,
moved into an input/output transition, or consumed by a stdlib lifecycle
pattern such as transfer, claim, or settle.

## Compile-Time Rules

The type checker enforces:

- values are unavailable after `consume`, `destroy`, or a stdlib lifecycle
  pattern that consumes the value
- both branches of `if` and `match` must leave linear values in compatible
  ownership states
- loops cannot hide linear state changes that would make ownership depend on
  runtime iteration count
- a linear value cannot be stored in an ordinary local aggregate and then escape
  the checked ownership path
- references rooted in linear values cannot outlive the root value

These are compile-time checks. Generated verifier code may also clear consumed
stack slots as a runtime defense, but stack clearing is not the primary
ownership model.

For state machines, 0.21 also validates flow-edge membership statically. An
action that claims `transition before.state: A -> after.state: B` must use an
edge declared by the corresponding `flow` block. This is a linearity rule for
state continuation: undeclared transitions fail before lowering instead of
leaving the verifier to infer protocol intent from action names.

## Required End States

Every acquired cell-backed value must reach an explicit terminal operation:

- `destroy`
- stdlib lifecycle transfer, claim, or settle pattern
- named output binding
- another verified operation documented in metadata

Silent end-of-scope loss is rejected.

## Cell-Backed Collections

Generic ownership of collections of linear cells is not a production feature.
0.13 stack-backed `Vec<T: FixedWidth>` helpers are verifier-local value
helpers, not cell ownership containers. A `Vec<Token>` or `HashMap<Hash, NFT>`
would require a verifier-backed membership and consumption model. Until that
model exists, such cases must remain compile-time rejected or represented as
structured runtime blockers.

Missing verifier pieces:

- `consume_each`
- typed collection destructuring
- verifier-backed membership proofs
- schema-level ownership witnesses

# Structured type and specialization identity migration

Status: implementation contract for [#96](https://github.com/CellScript-Labs/CellScript/issues/96).
Final integration of the representation migration is incomplete. This document
makes the internal choices explicit; it does not announce new generic syntax or
an artifact schema.
Owners are the frontend/type/IR maintainers, with independent-checker and
interface consumer review.

## Internal representation and phase ownership

Source AST applications use `Type::Applied(Box<TypeApplication<Type>>)`;
the application retains its constructor and arguments. `TypeArgument<Type>`
distinguishes `Type(Type)` and `UnsignedConstant(u128)`;
a numeric token is never a nominal declaration. The constant width follows the
existing lexer. Each family still enforces its existing value range. This does
not introduce constant parameters, constant expressions, inference of new
parameter kinds or acceptance of constants by value-generic declarations.
Plain source paths retain their source spelling until resolution.

Resolution supplies a declaration identity consisting of the defining module
identity and declared name. The module identity comes from the existing resolved
package/module catalog, not a caller-provided alias or layout hash. Executable
IR carries this identity, structured arguments and the admitted family variant.
Diagnostic spelling and machine labels are separate presentation data.

Monomorphization retains a specialization origin for each generated declaration:
its kind, defining template identity and ordered structured arguments. Imported
requests, external instantiation seeds and scoped IR merges must transport this
origin. Consumers may look up a generated label in that table, but must not
decode a label to discover whether it is a specialization or what its arguments
mean. Private visibility, transitive type dependencies, source projection,
interface emission and generic contract emission all use retained origins.
Missing or conflicting origins reject at the owning phase.

An encoder may keep the current symbol bytes by rendering those structured
arguments under the characterized symbol contract and encoding them once.
The encoder is an output boundary, not a source of semantic facts. Removing a
semantic decoder while retaining a generated-name prefix check would leave the
migration incomplete.

## Render and reader boundaries

Renderers select an explicit contract. Source spelling, typed wire spelling,
compact identity spelling and machine-symbol encoding must not be collapsed
into one implicit `Display` implementation. Shared traversal can remove duplicate
algorithms while keeping these choices visible. Hash/unit spelling is a format
choice, not a case-insensitive nominal comparison.

| Output | Identity contract |
| --- | --- |
| Source and diagnostics | Preserve source spelling and diagnostic aliases |
| Typed executable wire | Use `hash`, `address` and `unit`; resolve nominal owners through the admitted catalog |
| Specialization symbols | Render the deterministic encoding from retained structured arguments |
| Interfaces | The declared interface preserves declaration spelling; the checked interface uses resolved nominal identity |

Renaming a source alias must preserve the checked interface and executable
bytes. The declared interface retains its spelling-sensitive identity.
Different declarations with identical layouts remain distinct; stack vectors,
allocation-backed owners and borrowed views keep their separate ownership,
lifetime and ABI admission rules.

Source parsing and versioned metadata reading are the allowed text-to-structure
boundaries. A closed shared syntax description/parser may be owned by the
checker crate, as with WitnessBytesView. It may identify argument kinds and
syntax only. The checker independently resolves catalogs, reconstructs type and
effect constraints and validates machine evidence. Producer resolution results
must not become checker admission flags.

After boundary parsing, executable family consumers switch on structured
variants. They must not rebuild `CellDataView<...>`, selection names, witness
views or generic applications and parse them again. Existing structured nested
views remain structured; no family is converted back to a generic string as an
intermediate migration step.

## Wire, identity and cache decision

The first migration target preserves the current versioned public strings,
canonical identities and generated machine bytes. Internal AST/IR changes alone
do not justify changing the metadata envelope, package lock domain or interface
hash domain. Metadata renderers are the only writers of the retained text form;
readers parse it once and then perform their own admission.

This compatibility target is subject to characterization, not an assumption of
success. If a required identity correction changes a public hash or wire record,
that correction needs an explicit schema/domain decision, stale/mixed-record
rejection tests, regenerated current products and a cache generation change.
It must not be hidden as a harmless renderer refactor. Any artifact-affecting
semantic correction invalidates old persistent results even if its wire shape
stays the same.

## Acceptance and remaining work

The [alpha.2 backlog](releases/CELLSCRIPT_0_33_ALPHA_2_TODO.md) owns the remaining
representation work and acceptance criteria for #96. Completion requires the
compiler, native/WASM metadata, language services, interface consumers and
independent checker to agree on the admitted identities. Positive execution
and hash-rebound substitution tests establish their own bounded evidence;
checkpoint logs do not establish completion of this RFC.

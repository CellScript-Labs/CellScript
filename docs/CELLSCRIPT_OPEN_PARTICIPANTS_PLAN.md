# Compatible-open handles and transaction participants: 0.32 work plan

Status: implementation plan, not an implemented language/runtime contract.
Owners: #28 (compatible-open handles) and #29 (open roles). Implementer: Codex.
Independent security reviewer: unassigned; stable admission remains pending.
The consolidated design record and threat model for these contracts live in
[CELLSCRIPT_OPEN_HANDLE_DESIGN.md](CELLSCRIPT_OPEN_HANDLE_DESIGN.md).
This work was promoted into 0.32 on 2026-10-04. It does not change the
accepted exact-handle or closed-role contracts.

## Evidence baseline

The initial audit is against `35bf983db30aae80281f97e30bbce08a878d7c58`.

- `src/script_handle.rs` encodes exact 202-byte handles. Its constructor
  validates receipt shape and supplied identities; it is not itself an
  independent artifact-admission operation.
- `src/protocol_bundle.rs::admit_artifact` checks actual ELF/metadata/lowering/
  source-map bytes with the independent checker before deriving an exact
  receipt. Reuse this boundary and the existing materializer/transaction
  serializer.
- `src/deployment_line_handle.rs` supplies exact-version admission history.
  Its current compatibility comparison is compiler-side; calling it does not
  establish the independent compatible-open receipt required by #28.
- The independent checker's `validate_public_interface_metadata` checks
  canonical interface hashing, edition/module binding, item ordering and
  selected generic/ability constraints. A new compatibility receipt must also
  project and check every field it claims against the independently checked
  typed/layout/ABI records. Hash consistency alone is insufficient.
- The inspection prerequisite now binds concrete callable signatures to those
  checked entries: kind, ordered parameters and appended outputs, names, types,
  source domains, mutable/reference flags and return type. Rebound-hash
  substitutions reject. This does not supply missing execution evidence for
  pruned exports or uninstantiated templates, nor equate declared and inferred
  effects; complete H1 receipt projection remains pending.
- Locally instantiated public templates also project checked type arguments
  onto structural fields, enum payloads and generic callable signatures.
  Rebinding a template's layout and interface hashes cannot substitute its
  present concrete instance. Universal template constraints, absent instances
  and full directional compatibility still need their own receipt evidence.
- Typed semantics v9 retains explicit value-ability declarations for ordinary
  types, including private types. Independent field-derived validation and
  argument-constraint checks reject unsupported claims after hashes are rebound.
  Generic instantiation v2 retains ordered private/imported parameter contracts,
  declaration origins and lowered aliases. The checker requires coverage for
  every retained specialization, including when its public declaration is absent.
  Entry scopes retain identity-only dependencies through tuples, arrays, nested
  generic types and enum payloads; phantom Cell names do not create runtime roles.
  Missing nominal evidence cannot grant `non_linear`. Private template
  structural use and phantom-only identity dependencies now project through the
  checked module closure, and symbolic universal verification sweeps every
  retained template declaration including private and never-instantiated
  entries with resolvable function-template signatures. Imported template
  applications now serve as generic arguments: the producer derives their
  value-ability evidence from the owner's template definition (explicitly or by
  field derivation), and the checker registers scope-qualified alias evidence
  plus qualified concrete-type keys, so two owners exporting one template name
  keep distinct qualified identities while their raw ambiguous spelling stays
  fail-closed without the declaration catalogs. An imported template
  instantiated under another imported template's arguments — the doubly-external
  nested case — still fails closed at the owner seed boundary and needs its own
  orchestrator contract. Directional receipt comparison now implements the
  enumerated effect-weakening relation and binder-constraint relaxation with
  exact binder identities, abilities and codecs; the complete H1 open receipt
  still needs its general codec/builder obligations. This prerequisite does
  not complete H1 or runtime admission.
- Existing closed roles identify their exact providers at bundle construction.
  New participant selection must retain their lifecycle/observation distinction
  and preserve the transaction identity through signing.

### Pinned dependency-resolution facts

The dependency binding must account for the repository's CKB pin
`f7fa4436737756f97a24e254f22c13a36316ecea`:

- CellDep syscalls index the resolved dependency list; dep-group expansion
  means that this need not equal the raw transaction's CellDep index.
- Cell field syscalls expose data and Script identities, but no CellDep
  OutPoint field. Input OutPoint loading is a different operation.
- Data-hash lookup retains the last dependency with matching data. Type-hash
  lookup accepts same-data duplicates and rejects different-data multiple
  matches. Consensus does not reject every duplicate Type hash.

These facts are checked in upstream [dependency expansion](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/types/src/core/cell.rs#L754),
[CellDep indexing](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/load_cell.rs#L45),
[Cell field identifiers](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/mod.rs#L112)
and [Script lookup](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/types.rs#L828).
Matching code bytes and selecting an exact deployment OutPoint remain separate
claims. The runtime design must explicitly bind raw and resolved dependencies,
or admit a justified restricted dependency profile, before claiming both.
No such profile or new runtime helper is selected by this investigation.

## Required design decisions before executable admission

The [authorization-set wire contract](CELLSCRIPT_OPEN_HANDLE_POLICY.md) now
fixes the experimental bounded commitment/selection representation for the
implementation: 1–32 canonically ordered members, a five-level tree and a
656-byte selection. Host membership tests and independent CCC Molecule/hash
vectors cover that boundary. This supplies part of decisions 2–5 below; it
does not admit unchecked artifact receipts, resolve the nominal interface
parameter, implement CKB enforcement or complete H1/H2.

1. Define what the nominal interface parameter `I` denotes and how the resolver
   binds it to a checked package interface. Do not use a raw Script, arbitrary
   user struct, package name or producer-supplied boolean as its authority.
   Freeze this mapping before admitting `ScriptHandle<I>` or
   `VerifierHandle<I>` source syntax.
2. Define a bounded authorization-set policy. Every authorized member must
   identify an exact checked artifact and complete deployment, with a versioned
   commitment scheme and canonical ordering. Freeze member/count/byte budgets
   before implementing the codec. The selection may vary at construction time;
   the selected member is exact once the final transaction is frozen.
3. Locate the immutable authorization root in the current Script's committed
   code/args or in a state transition whose controlling Script enforces changes.
   Registry contents are discovery data. A receipt provided in a witness does
   not authorize its own root. A transaction builder cannot replace the root.
4. Define exact versus compatible selection, active/yanked members, minimum
   accepted sequence/version, stale snapshots and downgrade rejection. An
   immutable snapshot cannot learn about a later Registry yank by itself.
   Any live revocation mechanism needs an authenticated on-chain state model;
   do not promise global freshness from offline receipts.
5. Define the concrete Type-hash code-Cell/history checks. Matching the Type
   hash must not permit replacement code bytes or a different outpoint than
   the admitted selected member. Keep network identity and live-node
   observations separate from checks a running Script can actually perform.
6. Freeze compatibility axes: recursive serialized layouts, call signatures,
   witness/source codec, effects/capabilities, target/profile, runtime ABI,
   builder and deployment requirements. Compatibility is directional and
   explicitly versioned; it does not imply behavioral equivalence. Decide
   which extensions are compatible and reject unclassified changes.

These decisions are required implementation inputs, not discretionary tests
that can be omitted after a working example is available.

## Implementation sequence and acceptance

### H1: independent receipt projection and mutation prototype (#28)

The checked module projection prerequisite now derives nested nominal closure,
retained layouts, source callable signatures, actual inferred effects, fixed
Cell bindings and external dispatch tags from actual inspected bundles. A
private checked value supports conservative directional required-contract
matching with candidate additions. Different body predicates can preserve its
API identity while changing the exact artifact. Frozen package ownership,
complete codecs/builders/executable obligations, deployment and all-member
admission remain pending; this prerequisite does not satisfy H1 acceptance.


Use bounded actual artifact bundles as inputs. Recompute canonical interface,
artifact, ABI/profile, deployment and receipt identities inside the standalone
checker without depending on parser, resolver, type checker, IR or codegen.
Where the current record lacks evidence for a projected field, extend the
versioned record and its producer/checker agreement first; do not copy an
unchecked metadata assertion into a supposedly verified receipt.

Prototype two implementations of one admitted interface, one exact selection
and one compatible selection. Retain a known incompatible implementation.
Mutation tests must rebind outer hashes so that rejection demonstrates the
intended inner invariant: nested layout, callable/witness ABI, codec, effect,
profile, Script role/args, artifact/CellDep, network, status, sequence and
missing/duplicate/conflicting receipt members.

### H2: nominal source values and bounded runtime enforcement (#28)

After H1's decision record and prototype are complete, admit nominal handle
values with a fixed or explicitly bounded encoding. Define class/role
separation and prohibit coercion from raw bytes/Script values without the
required verification boundary. Runtime helpers must check authorized
membership and the selected actual Script/code dependency for each claim
declared on-chain. A compiler-only compatibility check is not runtime evidence.

Update parser/type/lowering/metadata/checker, formatter, LSP/editor, examples,
generated builders and WASM consumers together wherever the surface changes.
Run CKB-VM positives and the mutated deployment/membership cases. Record exact
artifacts, transactions, cycle/stack/witness budgets and deterministic receipts.
No existing exact-handle identity or deployment receipt is rewritten.

### R1: open participant ownership matrix (#29)

Each bounded selected participant must retain:

| Field | Required binding |
| --- | --- |
| Handle | Admitted H1/H2 policy root, selected member, exact receipt |
| Script | Complete code hash, hash type, args, Lock/Type/verifier role |
| Cell source | Transaction-wide or group-relative domain and selected indices |
| Data | Required schema, canonical codec and cardinality |
| Authority | Local lifecycle ownership versus foreign observation |
| Execution | Required peer group or explicit dependency-only role |
| Deployment | Exact selected code bytes/CellDep and network context |
| Correspondence | Required protected inputs and resulting outputs |

Every assertion is attributed to exactly the applicable boundary:
`checked_by_current_script`, `required_peer_script_group`,
`checked_by_bundle_dry_run`, or `guaranteed_only_by_ckb_consensus`.
Observing a foreign Cell never creates local consume/create authority.
Successful local execution does not establish that the selected peer accepted.

### R2: construction/signing and complete-transaction execution (#29)

Extend the existing ProtocolBundle input/report rather than creating a second
bundle system. Resolve each open role through the verified receipt policy,
materialize the exact participant before signing, and bind all role/source/
receipt/dependency/witness mappings to the serialized transaction. Generated
builders and the adapter must agree on identical transaction bytes. A changed
raw transaction requires reconstruction and fresh signing; a post-build role
or proof/receipt witness substitution must reject.

Execute the complete transaction with every required Script group. Cover:
same-interface/wrong-artifact, a selected peer which actually rejects, missing
peer group, wrong source/role, foreign ownership misuse, bad correspondence,
unbound Registry record, wrong network/deployment, incompatible/yanked/stale
receipt, downgrade, duplicate/conflicting participants, and post-signing
substitution. Distinguish intended rejection from unrelated malformed-signature
failure. Keep the existing closed-role positive/negative corpus unchanged.

## Completion record

For each issue, record final source identities, positive/negative execution,
checker mutations, frozen budgets, generated-builder byte parity, editor/WASM
scope, and matching dev/CI/backend gate results. The receipt prototype alone
cannot close #28; a bundle-only precheck cannot close #29. Independent security
review is a final stable-admission requirement for these newly promoted
contracts. Do not infer that it occurred from the earlier 0.32 review waiver.

The declaration prerequisite now retains optional bounded pre-optimization
source catalogs, qualified ordinary layout origins and complete public sets.
A separate symbolic checker rejects unsupported universal ability guarantees
without inferring them from a concrete instance. See the
[receipt contract](CELLSCRIPT_OPEN_INTERFACE_RECEIPT.md) for its exact limits.
Full H1 receipt projection/admission and H2 runtime enforcement remain pending.

No phase in this document is yet accepted. This plan identifies the work that
remains; it is not a replacement for either issue's acceptance evidence.

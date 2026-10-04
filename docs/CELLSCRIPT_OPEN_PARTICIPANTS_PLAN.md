# Compatible-open handles and transaction participants: 0.32 work plan

Status: implementation plan, not an implemented language/runtime contract.
Owners: #28 (compatible-open handles) and #29 (open roles). Implementer: Codex.
Independent security reviewer: unassigned; stable admission remains pending.
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
- Existing closed roles identify their exact providers at bundle construction.
  New participant selection must retain their lifecycle/observation distinction
  and preserve the transaction identity through signing.

## Required design decisions before executable admission

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

No phase in this document is yet accepted. This plan identifies the work that
remains; it is not a replacement for either issue's acceptance evidence.

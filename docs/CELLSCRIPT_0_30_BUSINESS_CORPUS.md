# CellScript 0.30 Business Corpus

**Status**: Candidate contract, frozen inventory; release evidence incomplete

This document defines the finite business portfolio behind the statement that
CellScript 0.30 aims to provide application-layer coverage comparable to
hand-written Rust CKB Scripts. The claim applies only to the bounded portfolio
below. It does not claim arbitrary Rust language, library, syscall, or contract
parity.

The canonical machine-readable inventory is
[`tests/fixtures/business_corpus.json`](../tests/fixtures/business_corpus.json).
`cellscript-tools check-business-corpus` validates its eight family IDs,
evidence-layer classifications, anchor requirements, referenced Git files, and
one SHA-256 digest over the complete frozen inventory. The `dev`, `ci`, and
`backend` gates run that validator. Stale, missing, untracked, duplicated, or
path-escaping evidence fails the gate.

The same gate validates the separate
[0.30 product capability ledger](CELLSCRIPT_0_30_CAPABILITY_LEDGER.md). The
ledger prevents an admitted compiler-surface result, an owner-only scenario
inventory, or a local CKB-VM result from being presented as complete product,
chain, or release evidence.

## Frozen portfolio

| Family | Required business boundary | Principal executable evidence |
|---|---|---|
| Fungible asset | Authorized lifecycle, bounded split/merge, conservation, identity and overflow failures | `bounded_group_input.rs`, `bounded_output_plan.rs`, `entry_witness_abi.rs`, and matched Rust references |
| NFT or DOB | Unique mint, metadata/owner/capacity transitions, burn, stale and unauthorized failures | Signed persistent-policy lifecycle plus an independently checked capacity typed-view artifact, exact fixtures, `nft.cell`, and the matched NFT Lock cost row |
| Order and AMM | Partial fill/cancel/settle, variable payments, price/reserve rules, ordered outputs, authenticated dependency | iCKB differential fixtures, AMM examples, cost corpus, and the composition anchor |
| Temporal | Absolute/relative locks, vesting, epochs, timestamps, blocks, headers, and `Since` | typed runtime-view and iCKB CKB-VM fixtures |
| Authorization | Standard signing, multisig, issuer and Script identity, post-signing mutation rejection | bundled multisig-v2, SDK signing, policy lifecycle, and sighash fixtures |
| Committed state | Authenticated opening, successor commitment, shared witness ownership, stale/root/index failures | typed `Commitment<T>`/`Opening<T>` CKB-VM and resource fixtures, bounded hash/Merkle cases, schema acknowledgements, and matched schema-roll reference |
| Multi-Script composition | At least four artifacts, interacting Type and Lock groups, persistent action dispatch, ProtocolBundle conflicts and exact identities | `business_corpus.rs`, ProtocolBundle CLI and adapter tests |
| External verifier | Exact-identity EXEC or SPAWN/WAIT with explicit trusted boundary and substitution failures | `trusted_external.rs` and exact-handle CKB-VM cases |

The companion transaction inventory
[`tests/fixtures/business_transaction_inventory.json`](../tests/fixtures/business_transaction_inventory.json)
names the required positive and adversarial scenarios and their current test
owners. It does not by itself prove that each name has a canonical Molecule
transaction. The separately hashed
[`business_scenario_evidence.json`](../tests/fixtures/business_scenario_evidence.json)
records the actual evidence grade and gaps for every family. The release gate
requires an exact artifact fixture for every named positive and adversarial row;
owner-only test lists and family-level CKB-VM coverage do not satisfy it.

The committed-state family now meets that per-row evidence boundary. Its two
positive rows and five adversarial rows execute one independently checked
artifact in CKB-VM, and the fixture pins the artifact/lowering/source-map/bundle
identities plus distinct raw and serialized transaction hashes. The stale and
wrong-root cases fail at authenticated opening error 73; malformed witness,
wrong-index successor, and wrong successor output fail at their exact bounded
ABI or output-correspondence errors.

The temporal family also has exact per-row evidence. A single independently
checked typed-Since/HeaderDep artifact executes four positive contexts and five
adversarial contexts with distinct Script args, input `since` values, HeaderDep
sets, and builder payloads. Cross-domain narrowing uses error 37, missing and
one-past HeaderDeps use error 45, early release uses the verification failure,
and epoch-duration overflow uses error 20; all raw and serialized transaction
hashes are fixture-bound.

The fungible family now has exact per-row evidence over a persistent, signed
policy lifecycle. Deterministic genesis OutPoints feed five accepted mint,
transfer, bounded split, bounded merge, and burn transactions. Six rejected
records bind replay at the local live-set boundary plus missing output, wrong
amount, checked-add overflow, wrong issuer identity, and incomplete multisig
authority at their actual enforcement layers. The independently checked
five-action artifact and every raw and serialized transaction hash are pinned
in `tests/fixtures/fungible_scenarios.json`.

The same deterministic lifecycle closes the authorization family without
claiming that Type Scripts replace Lock authentication. Four accepted rows
bind the bundled standard multisig-v2 Lock, its 2-of-2 threshold, issuer
authority, and the complete Data2 policy Script identity. Six rejected rows
exercise post-signing transaction mutation, a copied owner witness, a witness
from another signing domain, a wrong key, one-of-two partial signing, and
live-set replay. Exact Lock exit codes and transaction identities are pinned
in `tests/fixtures/authorization_scenarios.json`.

The NFT/DOB family now binds all ten inventory rows to exact CKB-VM
transactions. One signed persistent policy carries a uniquely identified
single-Cell object through mint, a verifier-derived state increment, ownership
transfer, and burn; adversarial transactions reject a duplicate output, stale
state, wrong Lock, missing Type Script, and absent owner multisig. Capacity
adjustment is deliberately a second independently checked typed-view artifact:
it requires the data, complete Lock, and complete Type identities to remain
unchanged while capacity increases, rather than claiming unsupported capacity
mutation inside the persistent policy. Both artifacts and all raw/serialized
transaction identities are pinned in `tests/fixtures/nft_scenarios.json`.

The order/AMM family now reuses exact anchor evidence where the frozen rows
are genuinely identical: partial fill, cancel, two-order settlement,
partial-settlement rejection, and authenticated dependency substitution.
Those five rows are hash-bound; pool merge, replay, output reordering, and
wrong-price remain explicitly incomplete, as does reproduction of the
unreachable pinned iCKB reference.

## Same-transaction anchor

The current executable anchor is an authenticated two-order settlement. One
CKB transaction runs four independently compiled CellScript ELFs and five
actual Script groups:

1. an authorization Lock validates the protected fungible input;
2. a fungible Type Script enforces amount conservation;
3. a persistent order-state Type Script dispatches the `partial_fill`, `settle`,
   and `cancel` actions through one full Script identity;
4. an order Type Script consumes two GroupInput orders and checks a two-element
   output Plan against GroupOutput order, data, Lock, Type, and capacity; and
5. that settlement Script binds an exact CellDep data hash.

The CKB-VM test rejects a wrong authorization credential, fungible inflation,
partial-settlement mismatch, persistent-state substitution, and dependency
substitution. The fixture pins a distinct raw and serialized transaction hash
for each rejected mutation, and the executable test binds the four inventory-
mapped authorization, Type-state, output, and CellDep substitutions back to the
same exact four artifact identities in the scenario-evidence manifest. Its
pinned resource
record is 41,822 cycles, 16,424 combined ELF bytes, a 5,376-byte largest checked
stack frame, 321 witness bytes, a 1,423-byte transaction, and 32.8 CKB occupied
capacity. Budgets in
[`tests/fixtures/capability_anchor_cases.json`](../tests/fixtures/capability_anchor_cases.json)
fail on regression.

The stateful companion test verifies `partial_fill`, registers its output under
the exact transaction OutPoint, then consumes that output with `settle`; it also
executes `cancel` and rejects an invalid full fill. The anchor therefore
establishes both same-transaction Script interaction and prior-output
continuity. Domain-separated fixed input OutPoints make the `partial_fill`,
successor `settle`, and independent `cancel` transaction hashes reproducible;
the fixture pins all six raw/serialized identities and maps the two terminal
inventory scenarios to the exact policy artifact. Every persistent-policy
action also reads the current GroupInput's
exact data size and capacity, so that typed runtime-view slice executes through
policy-witness dispatch in both the composition anchor and stateful chain. The
anchor now writes the four exact artifact bundles and generated
builders, admits their roles, witnesses, dependencies, and bounded output Plan
through ProtocolBundle, and requires adapter materialization to reproduce the
executed Molecule transaction byte for byte. The canonical fixture pins each
participating ELF hash plus its lowering-record, source-map, and verified-bundle
identity, together with the raw transaction, serialized transaction, and
canonical ProtocolBundle hashes. Any artifact, sidecar, fixture, or construction
drift therefore fails the test. Aggregate CKB-VM cycles are bound back to all
four direct CellScript Script-group records without inventing per-group cycle
attribution. A second bundle using the same exact transaction and four
artifacts adds a conflicting exclusive input role, must report `PB200`, and
must be rejected by the adapter before materialization.

## Evidence state

Every family records parser/type/IR, metadata/checker, simulator, CKB-VM,
stateful, node-admission, builder/signing, deployment, measurement, and review
layers separately. `passed`, `not-applicable`, `pending`, and
`release-candidate-required` retain their literal meanings. A lower layer is
never treated as evidence for a higher layer.

The inventory remains `candidate` because selected-network node admission,
deployment identities, and independent review are still pending.
The scenario-evidence manifest additionally records that the configured
NovaSeal and iCKB reference submodule commits are currently unreachable from
their remotes, so matched-reference reproducibility is blocked rather than
silently rebased to a different commit. The four-artifact anchor, its byte-
identical ProtocolBundle materialization, four inventory-mapped rejected
substitutions, and the maximum-width authenticated-opening row are exact-
artifact scenarios: each freezes its ELF identity or identities plus raw and
serialized transaction identities; their owning fixtures also bind lowering
records, source maps, verified bundles, and resource measurements. Within the
multi-Script family, all four positive and all five adversarial inventory rows
now have exact-artifact fixtures. The remaining non-exact scenarios are the
four explicitly listed order/AMM rows and the external-verifier family; the
unreachable matched references, selected-network node/deployment evidence, and
independent review remain separate blockers.
`check-business-corpus --release` rejects that state. Stable versioning, tags,
package publication, editor/browser
publication, and network deployment remain outside this candidate record.

## Updating the corpus

Change a family, fixture, reference, or evidence owner only as an explicit
portfolio change. Then run:

```bash
cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
  --root . check-business-corpus --write
```

Review the inventory diff, run the owning focused tests, and run the applicable
unified gates. Release validation additionally requires `status = "accepted"`,
all family layers complete or deliberately not applicable, and every release
requirement passed.

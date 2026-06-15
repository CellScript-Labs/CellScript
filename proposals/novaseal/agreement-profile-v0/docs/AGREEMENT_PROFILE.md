# Agreement Profile

NovaSeal Agreement Profile v0 is a profile package, not a change to NovaSeal
Canonical. It gives financial meaning to the shared NovaSeal transition
discipline.

In the staged NovaSeal roadmap, this work is **v0.2 Agreement Profile**. The
`v0` in this package name means "first version of the Agreement Profile schema
and package", not "the base NovaSeal roadmap stage".

## Design Motto

Canonical stays thin; profiles carry meaning.

`NovaSealCanonicalV0` remains focused on authority, typed envelopes, CKB Cell
transition commitments, nonce/expiry, policy hashes, and receipt commitments.
Agreement semantics belong here.

## Canonical Conformance

Agreement Profile now declares:

```toml
conforms_to = "NovaSealCanonicalV0"
canonical_schema_hash = "0xe9a157d0211d63586f2e9334878f8354f87d4786f94aa9e5ea163fed5360aec6"
conformance_gate = "cellc certify --plugin novaseal-profile-v0"
certification_plugin = "novaseal-profile-v0"
certification_report = "target/cellscript-certification/novaseal-profile-v0.json"
```

The contract does not call a Core runtime. Instead, `NovaAgreementSignedIntentV0`
contains:

```text
core: NovaAgreementIntentCoreV0
canonical_envelope_hash: Byte32
expected_receipt_hash: Byte32
```

Each transition recomputes `NovaSealCanonicalEnvelopeV0` from the Agreement
body:

| Canonical field | Agreement mapping |
| --- | --- |
| `profile_id` | `agreement_id` |
| `policy_hash` | `terms_hash` |
| `action` / `terminal_path` | originate, repay, or claim path id |
| `subject_id` | `agreement_id` |
| `old_state_commitment` | previous `latest_receipt_hash`, or zero at origination |
| `new_state_commitment` | new materialised receipt commitment |
| `old_nonce` / `new_nonce` | Agreement nonce transition |
| `expiry` | `expiry_timepoint` |
| `authority_hash` | borrower/lender authority identifier for signature and display; not a payout recipient lock hash |
| `profile_body_hash` | `hash_blake2b_packed(NovaAgreementIntentCoreV0)` |
| `payout_commitment_hash` | typed payout or payout-pair commitment |

The signed intent must contain the matching `canonical_envelope_hash`. This
makes Canonical influence the signed and verified Agreement transition without
forcing Agreement to import or call a separate Core contract. Rather civilised,
by protocol standards.

Payout routing remains Agreement profile and builder surface. It is committed by
`payout_commitment_hash` and materialised typed payout outputs, not inferred from
the BTC authority identifier.

The schema-security reference point is RGB Strict Types, but only at the design
level. Agreement does not use RGB's schema engine, Vesper/STL files, operation
commitments, or client-side validation flow. The borrowed discipline is narrower:

- a canonical schema hash is computed from normalised schema lines,
- the manifest pins that schema hash,
- the gate checks exact canonical field order,
- the wallet vector exposes the canonical envelope hash,
- the `.cell` runtime recomputes and checks the same hash before accepting the
  signed intent.

RGB++ was reviewed as a CKB/Bitcoin binding and lockscript reference, not as a
schema source. Its public design material describes isomorphic bindings and
CKB script validation; it does not provide the strict schema machinery used as
the reference point here.

## Public Certification Gate

The public compiler entry is:

```text
cellc certify --plugin novaseal-profile-v0
```

`cellc certify` is the stable compiler-hosted boundary. The NovaSeal-specific
rules live behind that boundary as the Rust built-in `novaseal-profile-v0`
certification module, which produces and verifies
`target/novaseal-production-gates.json`. The public
ecosystem gate inside that report is:

```text
agreement_profile_public_ecosystem_certification_v0
```

The gate is local and deterministic: it does not call an external service, does
not add a Core runtime dependency, and does not add new on-chain machinery.

The gate passes only when all local certification evidence is present:

| Requirement | Evidence |
| --- | --- |
| Canonical conformance | manifest `conforms_to`, normalised canonical schema hash, exact canonical field order, source-level canonical envelope checks |
| Profile schema set | exact checked-in Agreement schema files and SHA-256 hashes in the report |
| Fixture set | exact checked-in Agreement fixture files, including `principal + fixed_fee` and nonce max/max-1 arithmetic boundary fixtures |
| Signing boundary | originate, repay and claim wallet vectors with fixed-width signed intent bytes, signer sets, BIP340 message hashes and displayed `canonical_envelope_hash` |
| Runtime behaviour | fresh live devnet originate -> repay and originate -> claim evidence |
| Negative cases | live dry-run rejects for wrong signatures, wrong asset kind, wrong payout, early claim and payout binding failures; local arithmetic-boundary rejects for terminal amount and nonce overflow |
| Invariant matrix | `authority-binding` and `u64-overflow-prevention` obligations are recorded as runtime-checked |
| TCB status | local BIP340 verifier review bundle plus manifest verifier pinning |

This is enough for **public ecosystem profile certification of the local
package**. A production statement still requires the external gates in the same
report: public/shared CellDep pinning and external BIP340 verifier TCB
attestation. This distinction is deliberate; otherwise the certification would
be wearing borrowed robes.

## v0 Shape

The first slice models CKB-native agreements only:

- collateral asset: CKB
- principal asset: CKB
- fee: fixed fee
- terminal paths: originate, repay before expiry, claim after expiry
- no price feed
- no margin call
- no dynamic liquidation

Actor hashes are explicit fields and guards in this slice. Cryptographic locks
and BTC authority hooks are future profile slices.

The default claim path pays the locked collateral only. The fixed fee is a
repayment-path amount; adding it to the default claim would imply extra CKB
outside the locked agreement cell.

## Why This Is Not Ordinary Lending

Phroi's critique is respected: without oracle/margin-call machinery, this is not
ordinary overcollateralized DeFi lending. It is a priced terminal-rights
agreement. If the market makes one terminal path attractive, the party with that
right will exercise it.

That is the point: the agreement is digitally native because its terminal paths
are deterministic.

## Local Shape Harness

`scripts/nova_agreement_tx_shape_harness.py` checks the builder-visible output
shape for the CKB/CKB profile: occupied-capacity floors, principal payout,
repayment amount, collateral return, default collateral claim, time rejects,
party rejects, wrong-settlement rejects, `principal + fixed_fee` overflow and
max-boundary behaviour, and nonce max/max-1 increment behaviour.

`harness/ckb_vm` executes the compiled `originate_agreement`,
`repay_before_expiry`, and `claim_after_expiry` action ELFs in `ckb-vm`. It
covers the action/type-script layer for time guards, party guards, nonce
increments, latest-receipt-hash binding, receipt output fields, typed payout output
fields, terms-hash output binding, canonical-envelope-hash binding, and
preserved-field checks.

`novaseal_agreement_tx_harness` constructs deterministic resolved transactions
and runs them through `ckb-script` plus the CKB non-contextual/contextual
verification stack. It uses a local always-success lock so that terminal input
transactions can reach the Agreement Profile type/action script. All fixture
files are now covered by this layer.

These are still local evidence layers. They do not replace live-chain deployment
evidence, real CellDep liveness, production authority locks, concrete
payout wallet plumbing, public/shared CellDep attestation, or external BIP340
TCB review. Fixed-width wallet signing vectors are generated by the root
`scripts/novaseal_wallet_signing_vectors.py` gate and checked together with
the wallet/lock digest alignment gate.

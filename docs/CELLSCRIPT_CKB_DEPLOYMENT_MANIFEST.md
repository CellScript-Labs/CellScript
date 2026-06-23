# CellScript CKB Deployment Manifest

**Status**: production authoring surface for current CellScript metadata and
builder handoff.

CellScript tries to keep verifier logic and deployment evidence separate.

Contract authors should not have to encode CKB `CellDep`, `hash_type`, or
capacity evidence through ad hoc verifier scripts. Instead, a package can declare
those deployment-facing CKB facts in `Cell.toml`. The compiler then carries them
into `constraints.ckb` as structured obligations that builders, release gates,
and auditors can check.

This does not mean the compiler magically proves every final CKB transaction.
Capacity, cell layout, and concrete deps are still transaction-level facts.
The point is to make those assumptions explicit and machine-readable instead of
leaving them scattered across scripts, builders, and release notes.

## Manifest Shape

```toml
[deploy.ckb]
hash_type = "data1"
artifact_hash = "blake2b:..."
data_hash = "0x..."
out_point = "0x...:0"
dep_type = "code"
type_id = "0x..."

[[deploy.ckb.cell_deps]]
name = "secp256k1"
out_point = "0x...:0"
dep_type = "dep_group"
hash_type = "type"
```

`[[deploy.ckb.cell_deps]]` also accepts the split location form:
`tx_hash = "0x..."` plus `index = 0`. Use one form per dependency. A manifest
that specifies both forms for the same dependency is rejected.

Supported `hash_type` values are:

- `data`
- `type`
- `data1`
- `data2`

Supported `dep_type` values are:

- `code`
- `dep_group`

Unknown `hash_type` or `dep_type` values are compile errors. They are not
warnings, because a builder that uses the wrong script hash mode or cell dep
mode can deploy a transaction that differs from the audited artifact identity.

## CKB Default Hash Helper

Use:

```bash
cellc ckb-hash --hex 00
cellc ckb-hash --file build/contract
```

The command computes Blake2b-256 with the `ckb-default-hash` personalization.
Empty bytes must hash to:

```text
44f4c69744d5f8c55d642062949dcae49bc4e7ef43d388c5a12f42b5633d163e
```

The same algorithm is available to Rust tooling as
`cellscript::ckb_blake2b256`. This is the supported builder/release helper
surface. Under the v0.14 CKB profile, `hash_blake2b(input: Hash) -> Hash`
also lowers to an executable in-script RISC-V Blake2b-256 helper for 32-byte
digest inputs. It does not claim arbitrary byte-slice or resource serialization
hashing.

## Constraints Output

`cellc constraints --target-profile ckb` emits:

- `ckb.hash_type_policy`
- `ckb.dep_group_manifest`
- `ckb.timelock_policy`
- `ckb.capacity_evidence_contract`

The compiler does not claim to statically prove full CKB occupied
capacity. The production contract is explicit: builders must attach measured
occupied-capacity evidence and consensus transaction-size evidence for
state-changing transactions.

## Builder Requirements

A production CKB builder must verify:

- deployed script `hash_type` equals the manifest or compiler default
- declared `dep_group` entries are referenced or expanded intentionally
- code cell data hash matches the compiled artifact
- type-id lineage matches metadata when type-id is used
- tx-size and occupied-capacity measurements are retained as release evidence

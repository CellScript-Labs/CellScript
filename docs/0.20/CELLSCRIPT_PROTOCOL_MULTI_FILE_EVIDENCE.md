# CellScript 0.20 Protocol Multi-File Evidence

**Date**: 2026-06-25

This note records which protocol sources actually use the new multi-file
compiler surface and which evidence exists for each change. It is deliberately
stricter than homepage or release copy: a source refactor is not protocol
evidence unless the matching CKB VM or devnet gate is refreshed and passes.

## Summary

| Protocol source | Current status | Evidence status |
|---|---|---|
| NovaSeal fungible-xUDT | Refactored to use a shared `.cell` schema import. | Metadata, artifact preparation, and live local devnet stateful evidence pass. |
| iCKB benchmark specs | Unchanged. | No natural shared schema/type boundary was found in the current three benchmark files. |
| DobEvo / DOB-EVO | Unchanged. | The checked-out proposal directory currently contains no `.cell` source to refactor. |

## NovaSeal Fungible xUDT

The profile now uses a real cross-file type import:

- `proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_schema.cell`
  owns the shared witness and commitment schema structs.
- `nova_fungible_xudt_type.cell` imports those schema types for the profile
  actions.
- `nova_fungible_xudt_lifecycle_type.cell` imports the same schema types for
  the CKB-facing lifecycle type entry.

The imported types are schema-only. They avoid duplicated witness and
commitment layout declarations without introducing cross-file business logic,
cross-script runtime coupling, or an ELF-linker claim.

### Compiler Evidence

Command:

```bash
target/debug/cellc metadata proposals/novaseal/fungible-xudt-profile-v0
```

Observed source graph:

```text
entry:   nova_fungible_xudt_type.cell
package: nova_fungible_xudt_lifecycle_type.cell
package: nova_fungible_xudt_schema.cell
```

Command:

```bash
target/debug/cellc metadata \
  proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell
```

Observed source graph:

```text
entry:   nova_fungible_xudt_lifecycle_type.cell
package: nova_fungible_xudt_schema.cell
package: nova_fungible_xudt_type.cell
```

Artifact preparation also includes the shared schema source unit:

```bash
python3 scripts/novaseal_planned_profiles_devnet_stateful_live.py \
  --profile fungible-xudt \
  --prepare-artifacts \
  --pretty
```

Result:

```text
status=passed
artifact=target/novaseal-planned-profile-artifacts/fungible-xudt/nova_fungible_xudt_lifecycle.elf
size_bytes=125352
```

The artifact metadata records `nova_fungible_xudt_schema.cell` in
`source_units` alongside the entry file. The imported fixed schema layout is
also visible in metadata: `NovaFungibleXudtSignedIntentV0` field offsets are
`core=0`, `canonical_envelope_hash=243`, and `expected_receipt_hash=275`.

### Live Devnet Evidence

Command:

```bash
python3 scripts/novaseal_planned_profiles_devnet_stateful_live.py \
  --profile fungible-xudt \
  --ckb-repo ../ckb \
  --ckb-bin ../ckb-bin/ckb_v0.207.0_x86_64-unknown-linux-gnu-portable/ckb \
  --live \
  --pretty
```

Result:

```text
status=passed
scenario=fungible_xudt_issue_transfer_settle
lifecycle_data_hash=0x394da78133cb2f5a5d6cd911feceeab9e97e6ad5d36c0e50f18be56653af85e5
lifecycle_artifact_size_bytes=125352
```

Live checks:

```text
issue tx=0x42c15a65eb6e8e1ebb7520695966d8e0685ddd7b37757c4dc20fd599c118ee11 cycles=0x3df1ef
transfer tx=0xe27ca15ef73b3f701caeb78936b44d53571100dc7c9bb586a801e41c45cced5a cycles=0x3f1ae2
settle tx=0x54b1d5d7d981ec7c8e776c1a1f5c5e6bfd058844b80d1c3275fe89136681c2a3 cycles=0x3ddb4c
```

Negative checks:

```text
wrong holder signature transfer: rejected, matched expected error 56
transfer amount mismatch: rejected, matched expected error 5
wrong holder signature settle: rejected, matched expected error 56
```

Therefore the NovaSeal fungible-xUDT source refactor is accepted as a
protocol-source multi-file usage candidate with matching live local devnet
stateful evidence. The claim remains scoped to shared type/schema imports; it
does not introduce cross-file business logic, cross-script runtime coupling, or
an ELF-linker model.

## iCKB

The current iCKB benchmark sources are:

- `tests/benchmarks/ickb_specs/ickb_logic.cell`
- `tests/benchmarks/ickb_specs/limit_order.cell`
- `tests/benchmarks/ickb_specs/owned_owner.cell`

They currently have no `use` imports and model different verifier surfaces.
No duplicated schema module was refactored because doing so would be artificial
and would risk obscuring the existing iCKB equivalence evidence. If a future
iCKB refresh adds a real shared receipt/header/schema boundary, it should carry
an updated CKB VM differential matrix.

## DobEvo / DOB-EVO

The current checkout under `proposals/evolving-dob` contains no `.cell` source
files. There is nothing to refactor into a multi-file source graph in this
workspace. Any future DobEvo / DOB-EVO source adoption must start from actual
checked-in `.cell` contracts and rerun the relevant devnet or registry-pressure
evidence.

## Release Claim

The 0.20 release claim is limited to compiler/tooling support for validated
multi-file source graphs and a browser-local playground workspace. Protocol
source adoption remains evidence-gated. The current protocol source that uses
the new feature is NovaSeal fungible-xUDT, and its live local devnet stateful
evidence now passes for issue, transfer, settle, and required negative cases.

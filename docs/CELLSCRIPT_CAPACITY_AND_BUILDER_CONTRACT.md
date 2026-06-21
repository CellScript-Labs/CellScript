# CellScript Capacity And Builder Contract

**Status**: production builder contract for the current CellScript CKB profile.

CellScript exposes capacity requirements, but it does not claim to statically
prove every CKB transaction's occupied capacity. Capacity is a transaction-level
fact because it depends on concrete lock/type scripts, output data, fees, and
builder-selected cell layout.

## Source-Level Capacity Declarations

CellScript supports a conservative type-level floor:

```cell
resource TimedToken has store
with_capacity_floor(6100000000)
{
    amount: u64,
}
```

`with_capacity_floor(...)` records a minimum output capacity in shannons for
that schema-backed CKB Cell view. The compiler carries it into `TypeMetadata` and
`constraints.ckb.declared_capacity_floors`.

This is not full capacity proof. It gives builders and auditors a declared
floor to preserve while the builder still computes the real occupied capacity
for the final `CellOutput` and output data.

## Compiler Output

For CKB artifacts, `constraints.ckb.capacity_evidence_contract` includes:

- code cell lower-bound capacity
- recommended code cell capacity margin
- declared type-level capacity floors, when present
- whether occupied-capacity evidence is required
- whether consensus transaction-size evidence is required
- measured occupied capacity, when supplied by acceptance/builder tooling
- measured tx size, when supplied by acceptance/builder tooling

State-changing actions that create or update outputs require builder evidence.

## Measurement Helper

The release helper lives at `tools/ckb-tx-measure` and reads a CKB JSON
transaction from stdin. The checked-in manifest assumes the standalone
repository layout where `ckb/` and `CellScript/` are siblings:

```bash
cargo run --manifest-path tools/ckb-tx-measure/Cargo.toml --locked < tx.json
```

When CellScript is used from the nested checkout, the
CKB acceptance script builds the same source through a generated temporary
manifest pointing at its configured `CKB_REPO`.

This helper is a repository-local release evidence tool. It is intentionally
excluded from the crates.io package because it links against a local CKB checkout
to reuse CKB packed transaction and occupied-capacity implementations.

It emits:

- `consensus_serialized_tx_size_bytes`
- `occupied_capacity_shannons`
- `output_occupied_capacity_shannons`
- `output_capacity_shannons`
- `capacity_is_sufficient`
- `under_capacity_output_indexes`

Occupied capacity is derived with CKB's own `packed::CellOutput` capacity API:
`output.occupied_capacity(Capacity::bytes(output_data.len()))`. The helper does
not use a local approximation and rejects transactions whose `outputs` and
`outputs_data` lengths differ.

## Builder Requirements

A production builder must:

- preserve any `constraints.ckb.declared_capacity_floors` on matching outputs
- compute occupied capacity for every output
- reject under-capacity outputs before submission
- retain measured occupied-capacity evidence in release reports
- retain consensus-serialized transaction size
- retain dry-run or VM execution evidence for cycles
- preserve `hash_type`, CellDep, and type-id metadata declared by the compiler
  and deployment manifest

The compiler can give lower bounds and requirements. The builder supplies the
transaction-specific proof.

## Resource Identity Contract

For CKB targets, `constraints.ckb.resource_identities` records the compiler's
current resource identity contract for schema-backed CellScript resources and
receipts.

Statuses have builder meaning:

- `ckb-type-id-builder-managed` means the builder must install the declared CKB
  TYPE_ID script and preserve the compiler metadata for that identity.
- `compiler-passive-identity-available` means `cellc resource-identity` can
  emit a passive resource identity artifact and JSON plan for that resource.

Scoped action artifacts are active verifiers. They must not be used as passive
type-script identities for newly-created resource Cells. `cellc validate-tx`
rejects transactions whose output type script uses the current scoped action
artifact hash as a resource identity, because CKB would execute that artifact
during output creation and the entry wrapper may expect action witness bytes.

Use `cellc resource-identity` to generate the compiler-owned passive badge:

```bash
cellc resource-identity examples/amm_pool.cell \
  --target-profile ckb \
  --identity Token=token-a \
  --identity Token:token_b_out=token-b \
  --identity Pool=pool-main \
  --plan-output build/resource-identities.json
```

The plan contains the exact `{ code_hash, hash_type, args }` type script each
created resource Cell should wear. `--identity TYPE=INSTANCE` sets a type-level
default; `--identity TYPE:BINDING=INSTANCE` overrides one create binding when
the same resource type needs more than one passive identity. Production
builders should pass the plan back into validation:

```bash
cellc validate-tx --against build/action.elf.meta.json \
  --resource-identities build/resource-identities.json \
  build/tx.json --json
```

For builder-facing integrations, prefer the manifest/check layer over manually
stitching the lower-level reports together:

```bash
cellc builder-manifest examples/amm_pool.cell \
  --entry-action swap_a_for_b \
  --target-profile ckb \
  --resource-identities build/resource-identities.json \
  --output build/swap.builder.json \
  --json

cellc builder-check \
  --manifest build/swap.builder.json \
  --tx build/swap.tx.json \
  --production \
  --json
```

`builder-manifest` embeds
`transaction_template.transaction_plan.builder_assumption_evidence_template` as
a fillable JSON skeleton. Copy it into the candidate transaction as
`builder_assumption_evidence`, then replace the placeholders with concrete
builder facts such as selected live cells, output indexes, measured occupied
capacity, fee/change accounting, and dry-run evidence.

Fixture scripts such as `always_success` can still be useful in local shape
tests, but they are not production resource identities. Treat them as
`always_success_fixture_only`: acceptable in fixture, harness, or negative-test
contexts; forbidden as the type script for real `MintAuthority`, `Token`,
`Pool`, or `LPReceipt` resource outputs.

For production-facing validation, pass `--production`:

```bash
cellc validate-tx --against build/action.elf.meta.json \
  --resource-identities build/resource-identities.json \
  --production \
  build/tx.json --json
```

In production mode, `validate-tx` rejects known fixture-only resource type
identities such as the devnet `always_success` code hash and all-zero
placeholder hashes. This keeps fixture plumbing useful without allowing it to
become a resource identity scheme.

## Mass Constraints

For CKB, `constraints.ckb` exposes compiler-estimated compute, storage,
transient, code deployment, standard transaction mass, and block mass. The
devnet/acceptance path remains authoritative for real transaction mass because
the final mass depends on the full transaction and network policy.

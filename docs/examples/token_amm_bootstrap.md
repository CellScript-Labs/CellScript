# Token And AMM Bootstrap Builder Path

This note gives builders a concrete path from a fresh local CKB chain to a
token-backed AMM transaction using the bundled examples.

It is intentionally narrower than a release evidence report. Use it to build a
correct transaction fixture, then keep the normal production evidence: dry-run,
commit, cycle count, consensus transaction size, occupied capacity, sufficient
output capacity, CellDeps, script refs, and both valid and invalid lock spends.

## Boundary

`examples/token.cell` is the fungible-token state machine. It is not the genesis
authority contract. Its `mint_with_authority` action consumes an existing `MintAuthority` input
Cell and creates the successor authority plus the minted `Token` output.

The bundled bootstrap companion is `examples/launch.cell`:

- `bootstrap_token` creates the first `MintAuthority` and token distribution
  outputs.
- `launch_token` creates the first `MintAuthority`, distribution token outputs,
  a pool seed token, a `Pool` output, and an `LPReceipt` output.

`examples/amm_pool.cell` is the standalone AMM state machine. Use `seed_pool`
when you already have two real token Cells. Use `swap_a_for_b`,
`add_liquidity`, and `remove_liquidity` after a live `Pool` Cell exists.
The bundled AMM is a bounded constant-product example: it checks token
identity, nonzero reserves, a capped fee rate, slippage, LP ownership on
withdrawal, and arithmetic bounds around the reserve and LP-supply updates. It
is still intentionally narrower than a full exchange: it has one explicit swap
direction, no oracle/TWAP, no protocol-fee withdrawal, no routing, and no
concentrated-liquidity model.

The practical bootstrap chain is:

```text
launch.bootstrap_token or launch.launch_token
  -> MintAuthority output
  -> token.mint_with_authority
  -> Token outputs
  -> amm_pool.seed_pool, unless launch_token already materialised the Pool
  -> amm_pool.swap_a_for_b or another pool action
```

`launch_token` materialises the Pool and LP receipt topology directly. It does
not link or call the `amm_pool.seed_pool` entry at runtime.

## Resource Identity Boundary

There are three separate script roles in this flow:

- scoped action artifacts are active verifiers for one selected CellScript
  action;
- resource type scripts are passive or lifecycle-stable identity badges for
  `MintAuthority`, `Token`, `Pool`, and `LPReceipt` Cells;
- fixture badges such as `always_success_fixture_only` are local test
  scaffolding only.

Do not use an action artifact such as `token_mint_with_authority.elf` as the
passive type script for a newly-created resource Cell. CKB executes output type
scripts during creation, so the generated `_cellscript_entry` wrapper will look
for action witness bytes and can fail with `entry-witness-abi-invalid`.

Current compiler metadata exposes this boundary through
`constraints.ckb.resource_identities` and `cellc tx solve` under
`transaction_plan.resource_identities`. Entries marked
`compiler-passive-identity-available` should be materialized with
`cellc resource-identity`, which emits the passive artifact plus the exact
`{ code_hash, hash_type, args }` scripts to place on resource outputs. A
production builder must not replace that passive identity with the scoped
action artifact.

## Entry Action Selection

Do not rely on "the first action runs on creation" as a protocol rule. Cell
creation is just CKB transaction output creation plus script verification. The
CellScript entry wrapper runs the action selected for the compiled artifact.

For builder fixtures and public examples, select the entry explicitly:

```bash
cellc examples/launch.cell \
  --entry-action launch_token \
  --target riscv64-elf \
  --target-profile ckb \
  -o build/launch_token.elf

cellc examples/token.cell \
  --entry-action mint_with_authority \
  --target riscv64-elf \
  --target-profile ckb \
  -o build/token_mint_with_authority.elf

cellc examples/amm_pool.cell \
  --entry-action swap_a_for_b \
  --target riscv64-elf \
  --target-profile ckb \
  -o build/amm_swap_a_for_b.elf
```

The compiler still has a convenience default for examples and diagnostics, but
builder code should pass the action name. That makes the selected entry a
builder input instead of an accidental source-order dependency.

## Entry Witness Encoding

Do not hand-encode `CSARGv1\0` bytes in builder code. Ask the compiler for the
payload ABI and use `cellc entry-witness` or the same ABI rules in your SDK.
Cell-bound inputs and outputs are transaction Cells, not witness payload args.

Inspect the ABI first:

```bash
cellc abi examples/launch.cell --target-profile ckb --action launch_token
cellc abi examples/token.cell --target-profile ckb --action mint_with_authority
cellc abi examples/amm_pool.cell --target-profile ckb --action seed_pool
cellc abi examples/amm_pool.cell --target-profile ckb --action swap_a_for_b
```

The current payload parameters are:

| Action | Payload parameters | Runtime-bound Cells |
|---|---|---|
| `launch_token` | `symbol`, `max_supply`, `initial_mint`, `pool_seed_amount`, `fee_rate_bps`, `creator`, `distribution` | `pool_paired_token` |
| `mint_with_authority` | `to`, `amount` | `auth_before` |
| `seed_pool` | `fee_rate_bps`, `provider` | `token_a`, `token_b` |
| `swap_a_for_b` | `min_output`, `to` | `pool_before`, `input` |

Example witness commands:

```bash
cellc entry-witness examples/launch.cell \
  --target-profile ckb \
  --action launch_token \
  --arg 0x4c41554e43483031 \
  --arg 10000 \
  --arg 1000 \
  --arg 500 \
  --arg 30 \
  --arg 0x1111111111111111111111111111111111111111111111111111111111111111 \
  --arg 0x<160-byte-distribution>

cellc entry-witness examples/token.cell \
  --target-profile ckb \
  --action mint_with_authority \
  --arg 0x<32-byte-recipient-address> \
  --arg 25

cellc entry-witness examples/amm_pool.cell \
  --target-profile ckb \
  --action seed_pool \
  --arg 30 \
  --arg 0x<32-byte-provider-address>

cellc entry-witness examples/amm_pool.cell \
  --target-profile ckb \
  --action swap_a_for_b \
  --arg 2 \
  --arg 0x<32-byte-recipient-address>
```

For `launch_token`, the `distribution` payload is exactly four
`(Address, u64)` entries, so it is 160 bytes: `4 * (32 + 8)`.

## One-Line Builder Contract Bundle

After building the scoped artifact and producing a candidate transaction JSON,
run the compiler-facing contract checks as one shell bundle. The preferred
builder-facing path emits one scoped manifest, then validates the candidate
transaction against it:

```bash
INPUT=examples/amm_pool.cell ACTION=swap_a_for_b TX=build/swap.tx.json RID=build/resource-identities.json MANIFEST=build/swap.builder.json MIN_OUT=49000 TO=0x1111111111111111111111111111111111111111111111111111111111111111; cellc resource-identity "$INPUT" --target-profile ckb --identity Token=token-default --identity Token:token_out=token-b --identity Pool=pool-main --identity LPReceipt=pool-main --plan-output "$RID" && cellc builder manifest "$INPUT" --target-profile ckb --entry-action "$ACTION" --resource-identities "$RID" --output "$MANIFEST" --primitive-strict 0.16 && cellc entry-witness "$INPUT" --target-profile ckb --action "$ACTION" --arg "$MIN_OUT" --arg "$TO" && cellc builder check --manifest "$MANIFEST" --tx "$TX" --production --primitive-strict 0.16
```

Use `cellc abi`, `cellc constraints`, `cellc explain assumptions`, and
`cellc tx solve` directly when debugging one layer of the manifest.
Builder-facing contract commands emit JSON by default; add `--human` for a
short terminal summary.
The manifest also carries
`transaction_template.transaction_plan.builder_assumption_evidence_template`,
which is the fillable skeleton a Rust builder can attach to the candidate
transaction after replacing placeholders with concrete cell, capacity, and
dry-run facts.

`cellc entry-witness` emits the raw `_cellscript_entry` payload. Do not wrap it
in `WitnessArgs.input_type` unless the CellScript source explicitly reads that
separate CKB witness surface.

## ProofPlan And Builder Assumptions

Before signing, builders should inspect both the proof plan and the concrete
transaction requirements:

```bash
cellc explain assumptions examples/token.cell \
  --target-profile ckb \
  --primitive-strict 0.16 \
  --json

cellc explain assumptions examples/amm_pool.cell \
  --target-profile ckb \
  --primitive-strict 0.16 \
  --json

cellc explain assumptions examples/launch.cell \
  --target-profile ckb \
  --primitive-strict 0.16 \
  --json
```

Use the emitted builder assumptions as reject-before-signing checks. In
particular, verify:

- each cell-bound parameter resolves to the expected input or output Cell;
- output data matches the generated Molecule schema;
- resource output type scripts follow `constraints.ckb.resource_identities`;
- scoped action artifacts are not used as passive resource type identities;
- CellDeps or script refs resolve to the intended code cells;
- capacity floors and occupied capacity are both satisfied;
- the transaction size and cycle budget are measured from the final
  transaction;
- builder-owned change outputs cannot satisfy typed output obligations by
  accident;
- lock scripts have positive and negative spend evidence.

Strict v0.16 ProofPlan checks compile the bundled token, AMM, and launch
actions as original scoped entries. Keep the chain evidence alongside that
compile gate: valid and invalid builder-backed spends, measured cycles,
transaction size, occupied capacity, capacity floors, CellDeps, and script refs.

## Local Dev Fixture

For repository acceptance, the stateful local-chain path already lives in:

```bash
./scripts/ckb_cellscript_acceptance.sh --bounded --stateful-scenarios
```

The relevant flows are:

- launch-to-token-mint-with-authority: a `launch.cell` output `MintAuthority` feeds
  `token.cell::mint_with_authority`;
- AMM lifecycle: seed, add liquidity, swap, and remove liquidity operate on
  live `Pool`, `Token`, and `LPReceipt` Cells.

External builder repos should mirror those flows, but should treat
`always_success` resource type scripts as `always_success_fixture_only` badges.
They prove the transaction shape is plausible; they do not prove the production
resource identity story. The compiler now exposes passive resource identity
contracts in metadata and emits resource identity plans so builders can fail
early instead of discovering identity mistakes through an opaque action-witness
error. Use `cellc builder check --production` with the generated manifest to
reject known fixture identities before signing.

The production gate now exercises these flows with original scoped strict
artifacts for `token.cell`, `amm_pool.cell`, and `launch.cell`; generated
harnesses and passive fixture badges remain coverage scaffolding, not the
external-builder production identity contract.

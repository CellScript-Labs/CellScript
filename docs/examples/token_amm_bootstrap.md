# Token And AMM Bootstrap Builder Path

This note gives builders a concrete path from a fresh local CKB chain to a
token-backed AMM transaction using the bundled examples.

It is intentionally narrower than a release evidence report. Use it to build a
correct transaction fixture, then keep the normal production evidence: dry-run,
commit, cycle count, consensus transaction size, occupied capacity, sufficient
output capacity, CellDeps, script refs, and both valid and invalid lock spends.

## Boundary

`examples/token.cell` is the fungible-token state machine. It is not the genesis
authority contract. Its `mint` action consumes an existing `MintAuthority` input
Cell and creates the successor authority plus the minted `Token` output.

The bundled bootstrap companion is `examples/launch.cell`:

- `simple_launch` creates the first `MintAuthority` and token distribution
  outputs.
- `launch_token` creates the first `MintAuthority`, distribution token outputs,
  a pool seed token, a `Pool` output, and an `LPReceipt` output.

`examples/amm_pool.cell` is the standalone AMM state machine. Use `seed_pool`
when you already have two real token Cells. Use `swap_a_for_b`,
`swap_b_for_a`, `add_liquidity`, and `remove_liquidity` after a live `Pool`
Cell exists.

The practical bootstrap chain is:

```text
launch.simple_launch or launch.launch_token
  -> MintAuthority output
  -> token.mint
  -> Token outputs
  -> amm_pool.seed_pool, unless launch_token already materialised the Pool
  -> amm_pool.swap_a_for_b or another pool action
```

`launch_token` materialises the Pool and LP receipt topology directly. It does
not link or call the `amm_pool.seed_pool` entry at runtime.

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
  --entry-action mint \
  --target riscv64-elf \
  --target-profile ckb \
  -o build/token_mint.elf

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
cellc abi examples/token.cell --target-profile ckb --action mint
cellc abi examples/amm_pool.cell --target-profile ckb --action seed_pool
cellc abi examples/amm_pool.cell --target-profile ckb --action swap_a_for_b
```

The current payload parameters are:

| Action | Payload parameters | Runtime-bound Cells |
|---|---|---|
| `launch_token` | `symbol`, `max_supply`, `initial_mint`, `pool_seed_amount`, `fee_rate_bps`, `creator`, `distribution` | `pool_paired_token` |
| `mint` | `to`, `amount` | `auth_before` |
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
  --action mint \
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

## ProofPlan And Builder Assumptions

Before signing, builders should inspect both the proof plan and the concrete
transaction requirements:

```bash
cellc explain-assumptions examples/token.cell \
  --target-profile ckb \
  --primitive-compat 0.14 \
  --json

cellc explain-assumptions examples/amm_pool.cell \
  --target-profile ckb \
  --primitive-compat 0.14 \
  --json

cellc explain-assumptions examples/launch.cell \
  --target-profile ckb \
  --primitive-compat 0.14 \
  --json
```

Use the emitted builder assumptions as reject-before-signing checks. In
particular, verify:

- each cell-bound parameter resolves to the expected input or output Cell;
- output data matches the generated Molecule schema;
- output type scripts use the intended artifact and script args;
- CellDeps or script refs resolve to the intended code cells;
- capacity floors and occupied capacity are both satisfied;
- the transaction size and cycle budget are measured from the final
  transaction;
- builder-owned change outputs cannot satisfy typed output obligations by
  accident;
- lock scripts have positive and negative spend evidence.

Strict v0.16 ProofPlan checks intentionally reject some aggregate and Pool
obligations in the current examples. Use the explicit compatibility flag above
when reproducing the current bundled-example metadata, and do not turn those
reports into deployable production claims without the matching chain evidence.

## Local Dev Fixture

For repository acceptance, the stateful local-chain path already lives in:

```bash
./scripts/ckb_cellscript_acceptance.sh --bounded --stateful-scenarios
```

The relevant flows are:

- launch-to-token-mint: a `launch.cell` output `MintAuthority` feeds
  `token.cell::mint`;
- AMM lifecycle: seed, add liquidity, swap, and remove liquidity operate on
  live `Pool`, `Token`, and `LPReceipt` Cells.

External builder repos should mirror those flows instead of using
`always_success` token stand-ins for the token and pool state path. It is fine
to keep `always_success` locks for pure locking scaffolding, but token Cells,
Pool Cells, LP receipts, and the MintAuthority hand-off should be real
CellScript typed Cells when testing this bootstrap sequence.

The production gate is stricter than this local-dev fixture. It must remain
fail-closed while the current `token.cell`, `amm_pool.cell`, and `launch.cell`
strict v0.16 ProofPlan gaps and non-original action harnesses are still present.

# CellScript 0.16.1 Release Notes

**Status**: Released as `v0.16.1`.

**Release date**: 2026-06-15.

**Release tag**: `v0.16.1`.

**Updated**: 2026-06-15.

CellScript 0.16.1 is a patch release for bundled example lifecycle clarity and
builder handoff. It keeps the 0.16 assurance/tooling scope, while making the
token, launch, AMM, and NFT examples easier to build against from an external
transaction builder.

## Highlights

- The token authority mint action is now `mint_with_authority`, making the
  required `MintAuthority` input explicit.
- The launch bootstrap action is now `bootstrap_token`.
- `bootstrap_token` and `launch_token` expose the first-token-cell path
  directly.
- `launch_token` materialises the Pool and LP receipt topology directly; it
  does not rely on an implicit `amm_pool.seed_pool` call.
- `nft.cell` now exposes `create_collection` for the first `Collection` Cell.
- The token/AMM bootstrap guide documents the CLI-first builder path through
  scoped entry selection, ABI inspection, entry-witness generation,
  builder-assumption inspection, and transaction JSON validation.

## Validation

The release gate was run in production mode against local CKB/devnet
transactions:

```bash
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
python3 scripts/validate_ckb_cellscript_production_evidence.py <report.json>
```

The validated evidence covers all bundled strict original scoped actions, lock
spend checks, measured cycles, transaction sizes, occupied capacity, and
stateful lifecycle scenarios including launch-to-mint, AMM seed/add/swap/remove,
and NFT collection bootstrap.

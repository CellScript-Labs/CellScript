The repository includes seven bundled examples. Treat them as guided reading,
not just files to compile. Each one teaches a different part of the language:
linear resources, shared state, receipts, locks, proposal flows, time checks,
and CKB production evidence.

This chapter helps you choose what to read first and what to learn from each
example.

## The Examples

| Example | What it teaches |
|---|---|
| `examples/token.cell` | Minting, transfer, burn, and guarded token merge. |
| `examples/nft.cell` | Unique assets, metadata, ownership transitions, and owner locks. |
| `examples/timelock.cell` | Time-gated release checks, release requests, and approval flow. |
| `examples/multisig.cell` | Threshold policy, proposal records, signatures-as-data, and lock-boundary predicates. |
| `examples/vesting.cell` | Vesting grants, receipts, claim flow, and admin-boundary comments. |
| `examples/amm_pool.cell` | Shared pool state, swap logic, liquidity receipts, and settlement effects. |
| `examples/launch.cell` | Mint-authority bootstrap and launch/pool composition patterns. |

The top-level `examples/*.cell` files are the canonical bundled business
source. They are both the clean reading surface and the source compiled by the
CKB acceptance runner. There are no checked-in `examples/business` or
`examples/acceptance` mirrors; acceptance-only profile/effect/scheduler
metadata belongs in runner configuration or generated files under `target/`.

`examples/registry.cell` and every checked-in `examples/language/*.cell` file
are intentionally outside the bundled production matrix. They are language
examples for compiler/tooling surfaces such as local stack-backed `Vec<T>`,
stdlib patterns, CKB source/witness, TYPE_ID, Spawn/IPC, capacity/time, and
dynamic BLAKE2b. They are covered by compiler/tooling tests rather than CKB
production action acceptance.

For a visual business-flow map of every bundled example, see
[`CELLSCRIPT_EXAMPLE_BUSINESS_FLOWS.md`](https://github.com/a19q3/CellScript/blob/main/docs/CELLSCRIPT_EXAMPLE_BUSINESS_FLOWS.md).
For a concrete token-to-AMM builder path with entry witness commands, see
[`token_amm_bootstrap.md`](https://github.com/a19q3/CellScript/blob/main/docs/examples/token_amm_bootstrap.md).
For small reusable patterns drawn from the same ideas, see
[Cookbook Recipes](https://github.com/a19q3/CellScript/wiki/Cookbook-Recipes).

## A Good Reading Order

If you are learning the language, read them in this order:

1. `token.cell`: start here. It is the smallest example with a clear resource
   flow.
2. `nft.cell`: learn unique assets and ownership-style locks.
3. `timelock.cell`: learn time guards and release evidence.
4. `multisig.cell`: learn proposal records and threshold logic.
5. `vesting.cell`: learn receipt-style claim flows.
6. `amm_pool.cell`: learn shared pool state after you understand resources.
7. `launch.cell`: read this last because it composes multiple patterns.

Do not try to learn everything from the densest example first. The examples are
more useful when each one adds one new idea.

## Compile All Examples

From the repository root:

```bash
for f in examples/*.cell; do
  echo "==> $f"
  cellc "$f" --target riscv64-elf --target-profile ckb -o "/tmp/$(basename "$f" .cell).elf"
done
```

This is a compile pass, not a full CKB production claim. It is useful while
learning because it shows that the examples fit the compiler and CKB profile.
Use `--primitive-strict 0.16` for the pre-production ProofPlan gate. The token,
AMM, and launch examples now compile their bundled business actions as original
scoped entries under that strict gate; keep the matching chain evidence before
calling the artifacts production-ready.

## Token Walkthrough

Start with the token example. It is small enough to keep in your head.

The token example declares two resources:

```cellscript
resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}

resource MintAuthority has store, create, replace {
    token_symbol: [u8; 8]
    max_supply: u64
    minted: u64
}
```

`Token` is the asset. `MintAuthority` is the state that limits how much can be
minted. The checked-in `examples/token.cell` declares `MintAuthority` with
`store, create, replace`, because another action has to create the first
authority Cell before `mint_with_authority` can consume it.

`mint_with_authority` updates authority state and validates a proposed new token output:

```cellscript
action mint_with_authority(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token)
where
    assert(auth_before.minted + amount <= auth_before.max_supply, "exceeds max supply")

    require auth_after.token_symbol == auth_before.token_symbol
    require auth_after.max_supply == auth_before.max_supply
    require auth_after.minted == auth_before.minted + amount

    create token = Token {
        amount,
        symbol: auth_before.token_symbol
    } with_lock(to)
```

Read `auth_before` as the existing authority Cell and `auth_after` as the
proposed output. The action signature names the input/output topology; the
`require` guards are the field-level proof.

This is the key bootstrap boundary: `mint_with_authority` is not a genesis action. A builder
must first create a real `MintAuthority` Cell, normally with
`examples/launch.cell::bootstrap_token` or `examples/launch.cell::launch_token`,
then pass that Cell as the runtime-bound `auth_before` input to
`examples/token.cell::mint_with_authority`.

`transfer_token` consumes an input token and validates a proposed output
under a new lock:

```cellscript
action transfer_token(token: Token, to: Address) -> next_token: Token
where
    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
```

`burn` consumes the token and destroys it:

```cellscript
action burn(token: Token)
where
    assert(token.amount > 0, "cannot burn zero")
    destroy token
```

These three actions show the basic resource effect flow: propose an output,
update state, destroy state.

## Locks In The Examples

The bundled locks use `protected` to show the input Cell guarded by the current
lock invocation and `witness` to show decoded transaction witness data. Those
markers do not make an `Address` a signer proof.

When you see a lock like this:

```cellscript
lock owner_only(protected asset: NFT, witness claimed_owner: Address) -> bool {
    require asset.owner == claimed_owner
}
```

read it carefully:

- `asset` is the protected input Cell view;
- `claimed_owner` is decoded witness data;
- `require` fails the script if the comparison is false;
- the comparison does not prove that `claimed_owner` signed the transaction.

Real signature authorization still needs explicit sighash verification and its
own positive and negative CKB transaction matrix. `lock_args` can expose where
script-args data comes from, but it does not turn an `Address` into a signer.

## CKB Production Expectations

The CKB profile is strict, and the bundled suite has a defined production
boundary:

- bundled examples compile under the CKB profile;
- strict v0.16 ProofPlan gate checks pass for the original scoped token, AMM, and
  launch business actions;
- bundled business actions have scoped CKB production harnesses;
- bundled locks have builder-backed valid-spend and invalid-spend matrices;
- valid CKB transactions are builder-generated and dry-run;
- malformed transactions are rejected for non-policy/non-capacity reasons;
- transaction size, cycles, and occupied-capacity evidence are retained;
- bundled examples are deployed in the CKB production acceptance report;
- the final production hardening gate must pass.

This does not mean arbitrary new contracts are automatically production-ready.
Use the examples as patterns, then run your own constraints review, entry ABI
review, builder evidence, security review, and chain acceptance evidence.

## Production Checklist

Before treating an example-derived contract as deployable, run the compiler-side
checks:

```bash
cellc fmt --check
cellc check --target-profile ckb --production
cellc build --target riscv64-elf --target-profile ckb --production
cellc verify-artifact build/main.elf --verify-sources --expect-target-profile ckb --production
cellc examples/nft.cell --entry-action transfer --target riscv64-elf --target-profile ckb --primitive-strict 0.16 --production
# --entry-action selects a single action entry point for targeted inspection
```

For release-facing CKB evidence, run the CellScript acceptance gate:

```bash
./scripts/cellscript_gate.sh release
```

This wrapper runs compiler/backend evidence and the syntax-combination CI
preflight before the builder-backed CKB acceptance script, so bundled examples
cannot become release evidence if a
new syntax/lowering combination is failing.

Do not use compile-only or bounded diagnostic runs as production release
evidence. They are helpful during development, but they do not replace the chain
acceptance boundary.

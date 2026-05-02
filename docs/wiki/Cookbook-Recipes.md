This page is a practical companion to the tutorials. Each recipe gives you a
small goal, the code or command to start from, and the boundary you should keep
in mind.

Read the main tutorials first if the concepts are unfamiliar. Use this page when
you already know what you want to do.

## Recipe: Compile One File For CKB

Use this when you have a single `.cell` file and want a CKB-profile artifact.

```bash
cellc examples/token.cell --target riscv64-elf --target-profile ckb -o /tmp/token.elf
cellc verify-artifact /tmp/token.elf --expect-target-profile ckb
```

This proves that the artifact and metadata agree under the CKB profile. It does
not prove that a complete CKB transaction has been built or accepted.

## Recipe: Create A Linear Resource

Use a `resource` when a value should not be duplicated or silently dropped.

```cellscript
resource Token has store, transfer, destroy {
    amount: u64
    symbol: [u8; 8]
}
```

The compiler tracks `Token` as a linear value. An action that receives a token
must consume, return, transfer, claim, settle, or destroy it.

## Recipe: Mint A New Output Cell

Use `create` when an action materializes new Cell state.

```cellscript
action mint(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token)
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

The field shorthand `amount` means `amount: amount`. The `with_lock(to)` part is
the lock on the created output Cell.

## Recipe: Update State Without Updating In Place

Use an input-to-output action signature when the transaction updates state. The
input and output names are ordinary bindings; `require` clauses prove continuity
and the allowed field changes.

```cellscript
action bump_nonce(wallet_before: Wallet) -> wallet_after: Wallet
where
    require wallet_after.owner == wallet_before.owner
    require wallet_after.nonce == wallet_before.nonce + 1
```

When reviewing this pattern, inspect metadata and builder evidence for the input
and output binding. Do not treat it as account storage.

## Recipe: Write An Honest Lock Predicate

Use `protected`, `witness`, and `require` to make the CKB boundary readable.

```cellscript
lock owner_only(protected wallet: Wallet, witness claimed_owner: Address) -> bool {
    require wallet.owner == claimed_owner
}
```

Read this carefully:

- `wallet` is the protected input Cell view;
- `claimed_owner` is witness data;
- `require` fails validation if the comparison is false;
- the comparison does not prove that `claimed_owner` signed the transaction.

## Recipe: Avoid Fake Signer Semantics

Do not use names such as `signer` unless the value is actually produced by
signature verification.

```cellscript
// Misleading: this is still only witness data.
lock bad_owner_check(protected wallet: Wallet, witness signer: Address) -> bool {
    require wallet.owner == signer
}
```

Prefer names such as `claimed_owner` or `provided_owner` until the language has
explicit script-args and sighash verification primitives.

## Recipe: Reserve Script Args For Future Binding

The intended shape for real signature authorization is explicit:

```cellscript
lock signed_owner(
    protected wallet: Wallet,
    lock_args owner: Address,
    witness sig: Signature
) -> bool {
    require verify_sighash_all(sig, owner)
    require wallet.owner == owner
}
```

`lock_args Address` is decoded from the executing lock script's `Script.args`.
It is script-bound data, not a signature proof by itself; keep signature
verification explicit when that primitive is available.

## Recipe: Use Empty Vec Literals Safely

Use `[]` only where the expected `Vec<T>` type is known.

```cellscript
let mut keys: Vec<Hash> = []

create proposal = Proposal {
    proposal_id,
    proposer,
    data: [],
    signatures: []
}
```

`[]` is empty `Vec<T>` sugar in a typed context. It is not a generic collection
model, and it does not enable cell-backed collection ownership.

## Recipe: Inspect Entry ABI And Witness Layout

Use ABI and entry-witness reports before building transaction code.

```bash
cellc abi . --target-profile ckb --action transfer
cellc entry-witness . --target-profile ckb --action transfer --json
```

These reports tell builders and reviewers what data the entry expects. They do
not prove that the transaction has been assembled correctly.

## Recipe: Check A Package Before Building

Use this loop while developing a package:

```bash
cellc fmt --check
cellc check --target-profile ckb --all-targets --production
cellc build --target riscv64-elf --target-profile ckb --production
cellc verify-artifact build/main.elf --expect-target-profile ckb --verify-sources --production
```

This is a compiler/package gate. Use it before asking for deeper CKB evidence.

## Recipe: Run The CKB Production Gate

Use this only from the CellScript repository root:

```bash
./scripts/cellscript_ckb_release_gate.sh production
./scripts/ckb_cellscript_acceptance.sh --production
python3 scripts/validate_ckb_cellscript_production_evidence.py \
  target/ckb-cellscript-acceptance/<run>/ckb-cellscript-acceptance-report.json
```

This is the boundary where compiler evidence becomes builder-backed local CKB
evidence for the bundled suite.

## Recipe: Choose An Example To Read

Start with the smallest example that teaches the idea you need:

| Goal | Read |
|---|---|
| Linear resource effects | `examples/token.cell` |
| Unique assets and ownership | `examples/nft.cell` |
| Time-gated releases | `examples/timelock.cell` |
| Threshold proposals | `examples/multisig.cell` |
| Claim receipts | `examples/vesting.cell` |
| Shared liquidity state | `examples/amm_pool.cell` |
| Composition patterns | `examples/launch.cell` |
| Local bounded vectors | `examples/language/registry.cell` |
| Local order-vector helpers | `examples/language/order_book.cell` |

Read one example for one idea. The examples are easier to learn from when you do
not treat them as one large feature checklist.

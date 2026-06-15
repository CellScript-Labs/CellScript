This page is a practical companion to the tutorials. Each recipe gives you a
small goal, the code or command to start from, and the boundary you should keep
in mind.

Read the main tutorials first if the concepts are unfamiliar. Use this page when
you already know what you want to do.

## Recipe: Compile One File For CKB

Use this when you have a single `.cell` file and want a CKB-profile artifact.

```bash
cellc examples/token.cell --target riscv64-elf --target-profile ckb --primitive-strict 0.16 -o /tmp/token.elf
cellc verify-artifact /tmp/token.elf --expect-target-profile ckb
```

This proves that the artifact and metadata agree under the CKB profile. It does
not prove that a complete CKB transaction has been built or accepted.

## Recipe: Create A Linear Resource

Use a `resource` when a value should not be duplicated or silently dropped.

```cellscript
resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}
```

The compiler tracks `Token` as a linear value. An action that receives a token
must consume, return, destroy, validate a named successor output, or pass it
through an explicit stdlib lifecycle pattern such as
`std::lifecycle::transfer`, `std::receipt::claim`, or
`std::lifecycle::settle`.

## Recipe: Mint With Authority

Use `create` when an action materializes new Cell state.

```cellscript
action mint_with_authority(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token) {
    transition auth_before -> auth_after

    verification
        require auth_before.minted + amount <= auth_before.max_supply, "exceeds max supply"
        require auth_after.token_symbol == auth_before.token_symbol
        require auth_after.max_supply == auth_before.max_supply
        require auth_after.minted == auth_before.minted + amount

        create token = Token {
            amount,
            symbol: auth_before.token_symbol
        } with_lock(to)
}
```

The field shorthand `amount` means `amount: amount`. The `with_lock(to)` part is
the lock on the created output Cell.

## Recipe: Mint And Replace A Unique Cell

Use an identity policy plus `create_unique` and `replace_unique` when a Cell
lineage must be explicit in source and metadata.

```cellscript
resource Badge has store, create, replace
    identity(field(badge_id))
{
    badge_id: [u8; 32]
    owner: Address
}

action issue_badge(badge_id: [u8; 32], owner: Address) -> Badge {
    verification
        create_unique<Badge>(identity = field(badge_id)) {
            badge_id,
            owner
        } with_lock(owner)
}

action transfer_badge(badge: Badge, new_owner: Address) -> Badge {
    verification
        replace_unique<Badge>(identity = field(badge_id)) badge {
            badge_id: badge.badge_id,
            owner: new_owner
        }
}
```

`replace_unique` consumes the named input before the field initializer block.
For `field(...)`, the generated verifier compares the fixed-width identity field
between input and output. `create_unique` emits a local output anchor and
records full create-time uniqueness as runtime-required; field identity
uniqueness still needs builder or indexer evidence.

## Recipe: Update State Without Updating In Place

Use an input-to-output action signature when the transaction updates state. The
input and output names are ordinary bindings; `require` clauses prove continuity
and the allowed field changes.

```cellscript
action bump_nonce(wallet_before: Wallet) -> wallet_after: Wallet {
    transition wallet_before -> wallet_after

    verification
        require wallet_after.owner == wallet_before.owner
        require wallet_after.nonce == wallet_before.nonce + 1
}
```

When reviewing this pattern, inspect metadata and builder evidence for the input
and output binding. Do not treat it as account storage.

## Recipe: Choose A Destruction Policy

Use the destruction form that says what the verifier should prove:

```cellscript
destroy_singleton_type(config)
destroy_unique(asset, identity = type_id)
destroy_instance(badge, identity_field = badge_id)
burn_amount(token, field = amount)
```

In `--primitive-strict=0.15` mode, bare `destroy value` requires the `consume +
burn` kernel effects instead of the legacy `destroy` attribute. Keep the policy explicit
when reviewers must distinguish output absence, identity consumption, instance
consumption, and quantity burn.

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
explicit signer verification primitives.

## Recipe: Bind A Lock Predicate To Script Args

Use `lock_args` when a lock predicate depends on the executing script's args:

```cellscript
lock owner_boundary(
    wallet: protected Wallet,
    owner: lock_args Address,
    claimed_owner: witness Address
) -> bool {
    let input = source::group_input(0)
    let witness_lock = witness::lock(input)
    let digest = env::sighash_all(input)
    require wallet.owner == owner
    require claimed_owner == owner
    require witness_lock == digest
}
```

This makes the data source visible: `owner` comes from CKB `Script.args`, while
`claimed_owner` and `witness_lock` come from witness data. It still does not
turn either value into signer authority by name. Keep signature verification
explicit when that primitive lands; do not treat `Address` as a signature proof.

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
./scripts/cellscript_gate.sh release
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

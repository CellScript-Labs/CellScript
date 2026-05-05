# Action Model And Canonical Syntax

An `action` is a verifier case, not a method call and not runtime execution.
A user builds a CKB transaction, and the selected action verifier checks whether
the proposed Cell transformation is allowed.

The canonical form is:

```cellscript
action NAME(params...) -> outputs {
    transition old_state -> new_state
    transition another_old.state: A -> another_new.state: B

    verification
        require ...
        consume ...
        destroy ...
        preserve old_state -> new_state {
            field_a
            field_b
        }
        create ...
}
```

`transition` is optional. Actions that only consume and create resources, such
as token split/merge, often do not have an identity-bearing state continuation.

## Three Rules

1. `action` names a verifier branch. It is not a call target in the CKB
   transaction.
2. `transition old -> new` declares Cell lifecycle continuation. It does not
   prove field changes by itself.
3. `verification` contains proof obligations. `consume`, `create`, and
   `destroy` validate transaction shape; they are not VM-side allocation or
   mutation effects.

## State Continuation

Use `transition old -> new` for a same-type Cell continuation:

```cellscript
shared Pool has store {
    token_a_symbol: [u8; 8]
    token_b_symbol: [u8; 8]
    reserve_a: u64
    reserve_b: u64
    total_lp: u64
    fee_rate_bps: u16
}

action swap_a_for_b(pool_before: Pool, input: Token, min_output: u64, to: Address)
    -> (pool_after: Pool, token_out: Token)
{
    transition pool_before -> pool_after

    verification
        require input.symbol == pool_before.token_a_symbol
        let amount_out = quote_swap_out(
            input.amount,
            pool_before.reserve_a,
            pool_before.reserve_b,
            pool_before.fee_rate_bps
        )
        require amount_out >= min_output
        consume input
        require pool_after.reserve_a == pool_before.reserve_a + input.amount
        require pool_after.reserve_b == pool_before.reserve_b - amount_out
        preserve pool_before -> pool_after {
            token_a_symbol
            token_b_symbol
            total_lp
            fee_rate_bps
        }
        create token_out = Token {
            amount: amount_out,
            symbol: pool_before.token_b_symbol
        } with_lock(to)
}
```

The transition line says the Pool Cell continues. The `require` and `preserve`
statements prove the allowed delta.

## Flow State Edges

When a type has an explicit state graph, use field-level transition syntax:

```cellscript
flow Offer.state {
    Live -> Filled;
    Live -> Cancelled;
}

action fill_offer(input: Offer, buyer: Address) -> output: Offer {
    transition input.state: Live -> output.state: Filled

    verification
        require output.buyer == buyer
        preserve input -> output {
            seller
            price
            payment_symbol
        }
}
```

This form binds a declared `flow` edge. It is still only a lifecycle
declaration; authorization, payment checks, and field preservation remain in
`verification`.

## Terminal Inputs

Not every input is a transition. A receipt can be destroyed while another Cell
continues:

```cellscript
receipt Listing has destroy {
    nft_hash: Hash
    seller: Address
    price: u64
    payment_symbol: [u8; 8]
    expires_at: u64
}

action buy_listing(listing: Listing, nft_before: NFT, payment: Token, buyer: Address)
    -> (nft_after: NFT, seller_payment: Token)
{
    transition nft_before -> nft_after

    verification
        require env::current_timepoint() <= listing.expires_at
        require listing.nft_hash == hash_nft(nft_before)
        require payment.symbol == listing.payment_symbol
        require payment.amount >= listing.price
        consume payment
        destroy listing
        require nft_after.owner == buyer
        preserve nft_before -> nft_after {
            collection_id
            token_id
            metadata_hash
        }
        create seller_payment = Token {
            amount: listing.price,
            symbol: listing.payment_symbol
        } with_lock(listing.seller)
}
```

Read this as: NFT continues, Listing terminates, Payment is consumed, Seller
payment is created.

## Resource Accounting Without Transition

Split and merge are resource accounting actions, not identity-bearing state
continuations:

```cellscript
action split_token(token: Token, amount_a: u64, owner_a: Address, owner_b: Address)
    -> (part_a: Token, part_b: Token)
{
    verification
        require amount_a > 0
        require amount_a < token.amount
        consume token
        create part_a = Token {
            amount: amount_a,
            symbol: token.symbol
        } with_lock(owner_a)
        create part_b = Token {
            amount: token.amount - amount_a,
            symbol: token.symbol
        } with_lock(owner_b)
}

action merge_tokens(a: Token, b: Token, to: Address) -> merged: Token {
    verification
        require a.symbol == b.symbol
        consume a
        consume b
        create merged = Token {
            amount: a.amount + b.amount,
            symbol: a.symbol
        } with_lock(to)
}
```

No `transition` is needed here because there is no single logical Cell identity
that continues.

## Lock Entries

Locks are authorization predicates, not actions. They can still use
`verification`:

```cellscript
lock owner_only(protected nft: NFT, witness claimed_owner: Address) -> bool {
    verification
        require nft.owner == claimed_owner
}
```

Witness data is only data until a lock explicitly verifies signatures, script
args, digest scope, and witness layout. Parameter names such as `signer` or
`owner` do not create authority.

## One-Sentence Model

`action` says what transaction shape is being checked, `transition` says which
state Cell continues, and `verification` says why the proposed Cell
transformation is valid.

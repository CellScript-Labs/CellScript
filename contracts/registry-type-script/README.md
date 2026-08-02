# CellScript Registry Type Script

Canonical Type Script for mainnet Registry commitment Cells. It accepts only a
32-byte custody Lock Script hash in `args` and exact 39-byte Cell data:

```text
"CSREGv1" || ckb_blake2b_256(canonical commitment JSON)
```

The Script validates the data and custody Lock of every input and output in its
Type Script group. It also requires every creation, replacement, or destruction
transaction to consume at least one Cell whose Lock Script hash equals `args`.
Creating an output locked to the Registry therefore cannot impersonate an
official commitment: the transaction must exercise the Registry custody Lock.
The Script deliberately does not interpret off-chain JSON; the Registry API
binds the 32-byte hash to accepted release and deployment evidence and
revalidates live Cells independently.

Production uses the standard mainnet `secp256k1_blake160_sighash_all` genesis
Script for custody. Type Script args are the CKB Script hash of that complete
custody Script, including its 20-byte signer args. The Registry Type Script is
immutable at the data-hash layer unless a reviewed deployment explicitly
chooses a Type ID code Cell.

Build and test with the pinned repository toolchain:

```bash
contracts/registry-type-script/build_reproducible_release.sh
cargo test --locked --manifest-path contracts/registry-type-script/Cargo.toml
```

The test suite executes the stripped RISC-V binary in CKB-VM through
`ckb-testtool`, covering authorized creation, replacement, destruction,
unauthorized creation, incorrect custody Locks, malformed data, and
non-canonical Script args.

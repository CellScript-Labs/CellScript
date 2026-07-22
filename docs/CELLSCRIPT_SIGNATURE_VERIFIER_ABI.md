# CellScript BIP340 Verifier CellDep ABI

**Status**: executable 0.22 compiler/runtime boundary; deployment and message
policy remain package responsibilities.

CKB-VM does not provide a signature-verification syscall. CellScript therefore
spawns a separately deployed verifier binary from a transaction `CellDep` and
sends one fixed request over an inherited VM2 pipe.

## Source API

Use the explicit form for new code:

```cellscript
ckb::require_cell_data_hash(source::cell_dep(3), pinned_verifier_data_hash)
verifier::btc::bip340::require_signature_from_cell_dep(
    3,
    message_hash,
    xonly_pubkey,
    signature,
)
```

The dependency index must be an integer literal in `0..=63`. The
`require_cell_data_hash` preflight binds the selected resolved CellDep's data
hash before it is spawned. `require_signature(message_hash, xonly_pubkey,
signature)` remains a compatibility spelling that selects CellDep index `0`;
new packages should make the index explicit.

The expected verifier data hash must come from a reviewed manifest or other
trusted package configuration. Accepting it from witness data does not create
an identity guarantee. A builder must also pin the dependency out point and
`dep_type`; CKB syscalls expose the resolved CellDep sequence, not the original
DepGroup container identity.

## Frozen Request Envelope

The spawned verifier receives exactly 144 bytes (`18` little-endian `u64`
writes) on inherited file descriptor `0`:

| Offset | Size | Field | Required value |
|---:|---:|---|---|
| 0 | 8 | magic | ASCII `NSBV0IPC` |
| 8 | 2 | version | `0`, little-endian |
| 10 | 2 | scheme | `1`, BIP340 Schnorr/secp256k1 |
| 12 | 4 | flags | `0` |
| 16 | 32 | message | caller-provided prehash |
| 48 | 32 | public key | BIP340 x-only key |
| 80 | 64 | signature | BIP340 `r || s` |

Exit code `0` accepts. Any non-zero child exit, pipe/spawn/write/close/wait
failure, malformed fixed-width value, or dependency mismatch rejects the
parent script with a stable runtime error.

The compatible verifier package in `proposals/novaseal/v0-mvp-skeleton` pins
`verifier_id = "btc.bip340.v0"` and
`ipc_abi = "cellscript-btc-bip340-ipc-v0"`. That package is deployment evidence,
not an ambient standard-library implementation.

## Security Boundary

This ABI verifies only the supplied 32-byte prehash against the supplied key
and signature. The application profile must separately define and test:

- domain separation and canonical message construction;
- CKB ScriptGroup and `WitnessArgs` selection;
- lock placeholder and sighash rules;
- binding the verified key to on-chain authority;
- chain, script, action, nonce, and protocol replay policy;
- exact verifier artifact/out point and upgrade policy;
- positive and negative CKB-VM fixtures.

The compiler does not infer these rules from action names or field names. A
successful BIP340 call is not, by itself, proof that the correct transaction
message or authority was verified.

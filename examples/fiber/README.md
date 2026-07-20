# Fiber-Compatible CellScript Examples

These examples share the closed `fungible-type-group-v1` channel boundary:
the Fiber-managed Cell contains exactly one little-endian `u128`, ordinary
transactions conserve the complete Type Script group, and Script args contain
either a legacy 32-byte input Lock Script hash or `0x01` followed by a 32-byte
input Type Script hash that authorises issuance or destruction.

The examples differ only in their channel-external policy Cells. Fiber does
not read those Cells during payment, routing, cooperative shutdown, force
close, or watchtower settlement.

| Example | Demonstrated boundary |
| --- | --- |
| `ordinary_fungible.cell` | Ordinary owner-authorised fungible asset. |
| `fixed_supply.cell` | A one-shot authority Type Script creates the initial supply and is destroyed, leaving no live Cell whose Type Script hash can authorise later issuance. |
| `governed_supply_cap.cell` | A stateful policy Type Script enforces a cap. Its Cell Lock can independently be a single-owner, multisig, or governance Lock; tagged asset args identify the policy Type Script, not that Lock. |
| `reserve_compliance.cell` | Reserve and compliance epochs stay in a separate policy Cell and gate issuance outside Fiber. They are not ambient dependencies of channel settlement. |
| `wrapped_bridge.cell` | Deposit/redeem accounting stays in a bridge policy Cell while the wrapped payment asset remains a plain Fiber amount. External event proof verification is still an application-specific bridge obligation. |
| `multi_asset.cell` | One package contains two structurally compatible assets. Select one with `cellscript-fiber check ... --asset FiberUsd` or `--asset FiberEur`. |
| `type_id_upgradeable.cell` | The asset stays structurally stable while code deployment may be resolved through a Type ID CellDep. Upgrade governance and artifact review remain external obligations. |

The examples intentionally do not claim direct support for NFTs, per-unit
state, rebasing balances, payment-time dynamic oracles, per-payment CKB
witness policies, implicit interest fields, payloads longer than 16 bytes, or
Fiber callbacks. Those shapes require Fiber protocol or transaction-builder
support that the current integration does not provide.

Compile the examples through the compatibility boundary:

```bash
cargo run --locked -p cellscript-fiber-adapter --bin cellscript-fiber -- \
  check examples/fiber/ordinary_fungible.cell

cargo run --locked -p cellscript-fiber-adapter --bin cellscript-fiber -- \
  check examples/fiber/multi_asset.cell --asset FiberUsd
```

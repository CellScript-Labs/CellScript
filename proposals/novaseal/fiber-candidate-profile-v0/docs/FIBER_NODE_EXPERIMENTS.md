# NovaSeal Fiber Node Experiments

## External Repositories

| Repository | Branch | Commit | Purpose |
| --- | --- | --- | --- |
| `https://github.com/nervosnetwork/fiber.git` | `develop` | `27d458b8529e3b4ed76a3abd5f8babd2a0120f15` | Fiber Network Node workflow execution |
| `https://github.com/nervosnetwork/ckb-cli.git` | `develop` | `a3450f91aaebf97e98d517c8d9aad872dc21c9db` | Fiber dev-chain setup helper |
| `https://github.com/lightningnetwork/lnd.git` | `v0.20.1-beta` | `848b72ce9` | LND and lncli binaries for cross-chain hub execution |

## Live Execution Evidence

`scripts/novaseal_fiber_node_experiments.py` generated
`target/novaseal-fiber-node-experiments.json` with:

- status: `failed`
- required Fiber workflow suites present: `15/15`
- executed Fiber workflow suites: `15/15`
- passed Fiber workflow suites: `14/15`
- executed suites: `invoice-ops`, `open-use-close-a-channel`,
  `3-nodes-transfer`, `router-pay`, `shutdown-force`, `reestablish`,
  `external-funding-open`, `funding-tx-verification`, `udt`,
  `udt-router-pay`, `watchtower/force-close-after-open-channel`,
  `watchtower/force-close-with-pending-tlcs`,
  `watchtower/force-close-with-pending-tlcs-and-udt`,
  `watchtower/force-close-preimage-multiple`, `cross-chain-hub`

Commands:

```bash
PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite invoice-ops --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite open-use-close-a-channel --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite 3-nodes-transfer --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite router-pay --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite shutdown-force --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite reestablish --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite external-funding-open --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite funding-tx-verification --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite udt --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite udt-router-pay --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite watchtower/force-close-after-open-channel --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite watchtower/force-close-with-pending-tlcs --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite watchtower/force-close-with-pending-tlcs-and-udt --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite watchtower/force-close-preimage-multiple --timeout-seconds 1800

PATH="/Users/arthur/go/bin:/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite cross-chain-hub --timeout-seconds 2400
```

Each run started a local CKB dev chain, built or reused Fiber `fnn`, started
three Fiber nodes, waited for ports `8344`, `21714`, `8345`, `21715`, `8346`,
and `21716`, then ran the selected Bruno suite. The harness preserved previous
execution evidence in the aggregate report between runs.

The UDT watchtower suite was run from a temporary copied Bruno collection under
`target/novaseal-fiber-node-experiments/watchtower__force-close-with-pending-tlcs-and-udt/bruno-worktree`.
The harness converts four UDT balance variables from JavaScript `BigInt` values
to strings in that copied collection only, preserving the external Fiber checkout
while avoiding a Bruno QuickJS assertion-runtime incompatibility.

The cross-chain hub suite was executed twice after building `lnd` and `lncli`
from LND `v0.20.1-beta`. Both runs repeated the same receive-BTC failure after
the send-BTC half succeeded.

Observed Bruno result:

- `invoice-ops`: `5/5` requests passed, `10/10` assertions passed
- `open-use-close-a-channel`: `22/22` requests passed, `40/40` assertions passed
- `3-nodes-transfer`: `23/23` requests passed, `41/41` assertions passed
- `router-pay`: `39/39` requests passed, `50/50` assertions passed
- `shutdown-force`: `30/30` requests passed, `39/39` assertions passed
- `reestablish`: `9/9` requests passed, `15/15` assertions passed
- `external-funding-open`: `22/22` requests passed, `38/38` assertions passed
- `funding-tx-verification`: `3/3` requests passed, `7/7` assertions passed
- `udt`: `15/15` requests passed, `27/27` assertions passed
- `udt-router-pay`: `16/16` requests passed, `24/24` assertions passed
- `watchtower/force-close-after-open-channel`: `18/18` requests passed, `18/18` assertions passed
- `watchtower/force-close-with-pending-tlcs`: `24/24` requests passed, `27/27` assertions passed
- `watchtower/force-close-with-pending-tlcs-and-udt`: `28/28` requests passed, `32/32` assertions passed
- `watchtower/force-close-preimage-multiple`: `25/25` requests passed, `25/25` assertions passed
- `cross-chain-hub`: failed on repeated execution with `22/27` requests passed and `44/47` assertions passed

Covered live paths:

- invoice generation, duplicate rejection, decode, lookup, and cancellation
- single-channel connection/open flow
- three-node channel graph setup
- routed TLC transfer through the intermediate node
- router payment, graph listing, status lookup, duplicate/failure coverage, and custom-record payment flow
- force shutdown after peer disconnect, closed-channel state, and on-chain settlement trigger check
- channel reestablishment after disconnect, followed by TLC removal and shutdown
- external funding-script retrieval, externally funded channel open, funding transaction signing/submission, ready-state wait, balance checks, cooperative shutdown, and shutdown transaction inspection
- funding transaction verification rejection for an unaccepted auto-opened channel
- UDT channel open, invalid UDT channel rejection, UDT invoice/TLC flow, manual accept, two-channel listing, and shutdown
- routed UDT payment, UDT invoice send, UDT keysend, and insufficient-liquidity rejection
- watchtower force-close after open, commitment transaction progression, settlement generation, balance checks, and disconnected peer cleanup
- watchtower force-close with pending TLCs, on-chain timestamp updates, final settlement transactions, and balance transfer checks
- watchtower force-close with pending UDT TLCs, UDT settlement balance checks, and CKB balance drift bounds
- watchtower multiple-preimage settlement after force-close
- cross-chain hub send-BTC half: LND invoice creation, CKB-to-hub payment, wrapped-BTC receipt, and LND payee balance check
- TLC add/remove validation paths
- cooperative shutdown
- closed-channel state check after generated blocks

Repeated cross-chain failure:

- `cross-chain-hub/11-create-receive-btc-order` returned a JSON-RPC error in
  the receive-BTC half, so Bruno could not persist `BTC_PAY_REQ`.
- `cross-chain-hub/13-pay-btc-invoice` then attempted to pay the previous
  Lightning invoice and received HTTP 500 from LND router send.
- `cross-chain-hub/14-check-hub-received-btc-in-lnd` exhausted `10/10` polling
  attempts with `Hub has not received the payment`.
- `cross-chain-hub/15-check-payee-received-wrapped-btc` observed local balance
  `0x186a0` instead of the expected `0x249f0`.

## Boundary

This is real Fiber-node execution evidence for the invoice workflow, the basic
channel lifecycle workflow, the three-node transfer workflow, and the router
payment, force-shutdown, reestablishment, UDT channel, and UDT routed-payment
workflows, plus external funding, funding transaction verification, and all
watchtower workflows. It does not complete NovaSeal's external Fiber
requirement because `cross-chain-hub` executes but does not pass. Full coverage
still requires the receive-BTC half of the cross-chain hub workflow to pass.

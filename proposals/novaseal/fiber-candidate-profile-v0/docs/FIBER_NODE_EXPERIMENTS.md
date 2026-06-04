# NovaSeal Fiber Node Experiments

## External Repositories

| Repository | Branch | Commit | Purpose |
| --- | --- | --- | --- |
| `https://github.com/nervosnetwork/fiber.git` | `develop` | `27d458b8529e3b4ed76a3abd5f8babd2a0120f15` | Fiber Network Node workflow execution |
| `https://github.com/nervosnetwork/ckb-cli.git` | `develop` | `a3450f91aaebf97e98d517c8d9aad872dc21c9db` | Fiber dev-chain setup helper |

## Live Execution Evidence

`scripts/novaseal_fiber_node_experiments.py` generated
`target/novaseal-fiber-node-experiments.json` with:

- status: `partial_execution_passed`
- required Fiber workflow suites present: `15/15`
- executed Fiber workflow suites: `8/15`
- passed Fiber workflow suites: `8/8`
- executed suites: `invoice-ops`, `open-use-close-a-channel`,
  `3-nodes-transfer`, `router-pay`, `shutdown-force`, `reestablish`, `udt`,
  `udt-router-pay`

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
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite udt --timeout-seconds 1800

PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite udt-router-pay --timeout-seconds 1800
```

Each run started a local CKB dev chain, built or reused Fiber `fnn`, started
three Fiber nodes, waited for ports `8344`, `21714`, `8345`, `21715`, `8346`,
and `21716`, then ran the selected Bruno suite. The harness preserved previous
execution evidence in the aggregate report between runs.

Observed Bruno result:

- `invoice-ops`: `5/5` requests passed, `10/10` assertions passed
- `open-use-close-a-channel`: `22/22` requests passed, `40/40` assertions passed
- `3-nodes-transfer`: `23/23` requests passed, `41/41` assertions passed
- `router-pay`: `39/39` requests passed, `50/50` assertions passed
- `shutdown-force`: `30/30` requests passed, `39/39` assertions passed
- `reestablish`: `9/9` requests passed, `15/15` assertions passed
- `udt`: `15/15` requests passed, `27/27` assertions passed
- `udt-router-pay`: `16/16` requests passed, `24/24` assertions passed

Covered live paths:

- invoice generation, duplicate rejection, decode, lookup, and cancellation
- single-channel connection/open flow
- three-node channel graph setup
- routed TLC transfer through the intermediate node
- router payment, graph listing, status lookup, duplicate/failure coverage, and custom-record payment flow
- force shutdown after peer disconnect, closed-channel state, and on-chain settlement trigger check
- channel reestablishment after disconnect, followed by TLC removal and shutdown
- UDT channel open, invalid UDT channel rejection, UDT invoice/TLC flow, manual accept, two-channel listing, and shutdown
- routed UDT payment, UDT invoice send, UDT keysend, and insufficient-liquidity rejection
- TLC add/remove validation paths
- cooperative shutdown
- closed-channel state check after generated blocks

## Boundary

This is real Fiber-node execution evidence for the invoice workflow, the basic
channel lifecycle workflow, the three-node transfer workflow, and the router
payment, force-shutdown, reestablishment, UDT channel, and UDT routed-payment
workflows. It
does not complete NovaSeal's external Fiber requirement. Full coverage still
requires all mapped Fiber suites in `scripts/novaseal_fiber_node_experiments.py`
to execute and pass, including force-close/watchtower workflows, external
funding, and cross-chain hub paths.

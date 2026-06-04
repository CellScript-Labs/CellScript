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
- executed Fiber workflow suites: `2/15`
- passed Fiber workflow suites: `2/2`
- executed suites: `invoice-ops`, `open-use-close-a-channel`

Command:

```bash
PATH="/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite invoice-ops --timeout-seconds 1800
```

The run started a local CKB dev chain, built Fiber `fnn`, started three Fiber
nodes, waited for ports `8344`, `21714`, `8345`, `21715`, `8346`, and `21716`,
then ran Bruno `e2e/invoice-ops` and `e2e/open-use-close-a-channel`.

Observed Bruno result:

- `invoice-ops`: `5/5` requests passed, `10/10` assertions passed
- `open-use-close-a-channel`: `22/22` requests passed, `40/40` assertions passed

Covered live paths:

- invoice generation, duplicate rejection, decode, lookup, and cancellation
- single-channel connection/open flow
- TLC add/remove validation paths
- cooperative shutdown
- closed-channel state check after generated blocks

## Boundary

This is real Fiber-node execution evidence for the invoice workflow and the
basic channel lifecycle workflow. It
does not complete NovaSeal's external Fiber requirement. Full coverage still
requires all mapped Fiber suites in `scripts/novaseal_fiber_node_experiments.py`
to execute and pass, including multi-hop channel transfers, router payments,
UDT channel flows, force-close/watchtower workflows, external funding, and
cross-chain hub paths.

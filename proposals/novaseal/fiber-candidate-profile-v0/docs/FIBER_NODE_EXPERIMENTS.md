# NovaSeal Fiber Node Experiments

## External Repositories

| Repository | Branch | Commit | Purpose |
| --- | --- | --- | --- |
| `https://github.com/nervosnetwork/fiber.git` | `develop` | `27d458b8529e3b4ed76a3abd5f8babd2a0120f15` | Fiber Network Node workflow execution |
| `https://github.com/nervosnetwork/ckb-cli.git` | `develop` | `a3450f91aaebf97e98d517c8d9aad872dc21c9db` | Fiber dev-chain setup helper |
| `https://github.com/lightningnetwork/lnd.git` | `v0.20.1-beta` | `848b72ce9` | LND and lncli binaries for cross-chain hub execution, built with `invoicesrpc routerrpc` tags |

## Live Execution Evidence

`scripts/novaseal_fiber_node_experiments.py` generated
`target/novaseal-fiber-node-experiments.json` with:

- status: `passed`
- schema: `novaseal-fiber-node-execution-v0.3`
- latest clean-room rerun: 2026-06-05
- clean-room worktree:
  `/Users/arthur/RustroverProjects/CellScript-cleanrooms/novaseal-phase5-20260605-121420`
- required Fiber workflow suites present: `16/16`
- executed Fiber workflow suites: `16/16`
- passed Fiber workflow suites: `16/16`
- recorded suite duration: `2319.305s`
- aggregate Bruno result: `317/317` requests passed, `473/473` assertions
  passed
- runnable devnet contract present: `true`
- each suite recorded `execution.started_node: true`, the exact Bruno command
  `npm exec -- @usebruno/cli run e2e/<suite> -r --env test`,
  `execution.returncode: 0`, positive `execution.duration_seconds`, and
  persisted stdout/stderr log paths under
  `target/novaseal-fiber-node-experiments/`
- executed suites: `invoice-ops`, `open-use-close-a-channel`,
  `3-nodes-transfer`, `router-pay`, `shutdown-force`, `reestablish`,
  `external-funding-open`, `funding-tx-verification`, `udt`,
  `udt-router-pay`, `watchtower/force-close-after-open-channel`,
  `watchtower/force-close-with-pending-tlcs`,
  `watchtower/force-close-with-pending-tlcs-and-udt`,
  `watchtower/force-close-preimage-multiple`, `cross-chain-hub`,
  `cross-chain-hub-separate`

Current rerun command shape:

```bash
FIBER_REPO=/Users/arthur/RustroverProjects/fiber
export PATH="/Users/arthur/go/bin:/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH"
export REMOVE_OLD_STATE=y

for suite in invoice-ops open-use-close-a-channel 3-nodes-transfer \
             router-pay shutdown-force reestablish external-funding-open \
             funding-tx-verification udt udt-router-pay \
             watchtower/force-close-after-open-channel \
             watchtower/force-close-with-pending-tlcs \
             watchtower/force-close-with-pending-tlcs-and-udt \
             watchtower/force-close-preimage-multiple; do
  python3 scripts/novaseal_fiber_node_experiments.py \
    --fiber-repo "$FIBER_REPO" \
    --run-suite "$suite" \
    --timeout-seconds 1800 \
    --pretty
done

for suite in cross-chain-hub cross-chain-hub-separate; do
  python3 scripts/novaseal_fiber_node_experiments.py \
    --fiber-repo "$FIBER_REPO" \
    --run-suite "$suite" \
    --timeout-seconds 2400 \
    --pretty
done
```

The latest clean-room rerun used the loop above with explicit `--fiber-repo`.
The earlier individual command transcript is retained below for suite-name and
timeout traceability:

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

PATH="/Users/arthur/go/bin:/Users/arthur/RustroverProjects/ckb/target/debug:/Users/arthur/RustroverProjects/ckb-cli/target/debug:$PATH" \
REMOVE_OLD_STATE=y \
python3 scripts/novaseal_fiber_node_experiments.py --pretty --run-suite cross-chain-hub-separate --timeout-seconds 2400
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
Certification requires this copied Bruno worktree to be an ordinary directory
inside the generated CellScript-side experiment tree; symlinked, absolute, or
escaping worktree paths are rejected.

The cross-chain hub suites require LND's `invoicesrpc` service for
`AddHoldInvoice`. The local `lnd` and `lncli` binaries were rebuilt from LND
`v0.20.1-beta` with `invoicesrpc routerrpc` build tags after an initial
diagnostic run showed `unknown service invoicesrpc.Invoices`. The 2026-06-05
clean-room rerun used the rebuilt binaries from `/Users/arthur/go/bin` and did
not reproduce the service-missing failure.

The cross-chain suites were run from temporary copied Bruno collections under
`target/novaseal-fiber-node-experiments/cross-chain-hub/bruno-worktree` and
`target/novaseal-fiber-node-experiments/cross-chain-hub-separate/bruno-worktree`.
The harness logs the receive-BTC JSON-RPC body and guards
`resp.data.destroy()` in the copied collection only, preserving the external
Fiber checkout while avoiding a Bruno QuickJS stream-runtime incompatibility.
The recorded patch metadata must list non-empty patch files inside the copied
worktree; certification rejects empty metadata and worktree roots that resolve
outside the CellScript repository.

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
- `cross-chain-hub`: `19/19` requests passed, `40/40` assertions passed
- `cross-chain-hub-separate`: `19/19` requests passed, `40/40` assertions passed

Aggregate observed Bruno result:

- requests: `317/317`
- assertions: `473/473`

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
- cross-chain hub embedded mode: send-BTC half with LND invoice creation, CKB-to-hub payment, wrapped-BTC receipt, and LND payee balance check; receive-BTC half with hold-invoice creation, BTC payment into hub LND, wrapped-BTC delivery, and channel shutdown
- cross-chain hub separate-service mode: same send-BTC and receive-BTC workflow with CCH running as a standalone service connected to Fiber node 3 by RPC/WebSocket
- TLC add/remove validation paths
- cooperative shutdown
- closed-channel state check after generated blocks

Resolved cross-chain issue:

- Initial cross-chain runs failed at `receive_btc` because the local LND binary
  had been built without the `invoicesrpc` tag, so `AddHoldInvoice` returned
  `unknown service invoicesrpc.Invoices`.
- Rebuilding LND `v0.20.1-beta` with `invoicesrpc routerrpc` enabled the hold
  invoice service and both embedded and separate cross-chain suites passed.
- The remaining Bruno runner mismatch was limited to `resp.data.destroy()` on
  the LND streaming payment response; the harness guards that call in a copied
  worktree, and the underlying payment/balance assertions pass.

Expected clean-room log noise:

- Fiber network-secret permission warnings for generated local node keys.
- CCH WebSocket `connection refused` retries while the separate service waits
  for the Fiber WebSocket endpoint to become ready.
- Watchtower duplicate settlement-transaction RPC errors when the retry loop
  observes that the transaction is already in the local CKB transaction pool.

These are not pass conditions. The pass condition remains the JSON report plus
per-suite Bruno request/assertion success.

## Boundary

This is real Fiber-node execution evidence for the invoice workflow, the basic
channel lifecycle workflow, the three-node transfer workflow, and the router
payment, force-shutdown, reestablishment, UDT channel, and UDT routed-payment
workflows, plus external funding, funding transaction verification, all mapped
watchtower workflows, and both embedded and separate-service cross-chain hub
workflows. This completes the currently tracked NovaSeal external Fiber-node
execution requirement: all required mapped Fiber workflow suites execute and
pass through Fiber's devnet node runner and Bruno e2e harness.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.

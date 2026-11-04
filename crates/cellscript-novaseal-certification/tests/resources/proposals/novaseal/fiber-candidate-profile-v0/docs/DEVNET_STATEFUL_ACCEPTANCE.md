# NovaSeal Fiber Candidate Profile v0 Devnet Stateful Acceptance

The NovaSeal CKB stateful candidate path is present. The external
`fiber_node_execution_v0` report records all required Fiber workflow suites
executed and passed for the pinned Nervos Fiber devnet workflow evidence.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and Fiber candidate profile code as live
   CellDeps.
2. Submit a valid candidate settlement and prove the old Cell is dead plus the
   settled successor Cell and receipt are live.
3. Dry-run wrong-operator, no-op balance replay, zero channel, zero route,
   stale nonce, and expired settlement negatives and prove they do not consume
   state.
4. Keep the Fiber execution report attached to the pinned Nervos Fiber commit
   and rerun it after any profile, harness, or upstream workflow change.

`fiber_candidate_path` covers the NovaSeal profile on CKB devnet. The separate
`fiber_node_execution_v0` report covers Fiber node/channel workflow execution.
It is external evidence only: the CellScript profile still does not verify
Fiber HTLCs, routes, liquidity, fees, or revocations inside the NovaSeal
transition.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.

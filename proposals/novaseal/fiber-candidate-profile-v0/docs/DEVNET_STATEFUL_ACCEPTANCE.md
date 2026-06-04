# NovaSeal Fiber Candidate Profile v0 Devnet Stateful Acceptance

The NovaSeal CKB stateful candidate path is present. External Fiber-node
workflow execution remains pending.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and Fiber candidate profile code as live
   CellDeps.
2. Submit a valid candidate settlement and prove the old Cell is dead plus the
   settled successor Cell and receipt are live.
3. Dry-run wrong-operator, no-op balance replay, zero channel, zero route,
   stale nonce, and expired settlement negatives and prove they do not consume
   state.
4. Attach real Fiber execution evidence before any Fiber production execution
   claim.

`fiber_candidate_path` covers the NovaSeal profile on CKB devnet. The separate
`fiber_node_execution_v0` report must execute every required Fiber workflow
suite before claiming coverage of Fiber node/channel behaviour.

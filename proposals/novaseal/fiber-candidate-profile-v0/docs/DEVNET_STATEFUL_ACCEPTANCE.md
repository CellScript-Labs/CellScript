# NovaSeal Fiber Candidate Profile v0 Devnet Stateful Acceptance

Required V1 evidence is not present yet.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and Fiber candidate profile code as live
   CellDeps.
2. Submit a valid candidate settlement and prove the old Cell is dead plus the
   settled successor Cell and receipt are live.
3. Dry-run wrong-operator, no-op balance replay, zero channel, zero route,
   stale nonce, and expired settlement negatives and prove they do not consume
   state.
4. Attach real Fiber execution evidence before any production claim.

Until this evidence is generated, `fiber_candidate_path` must remain missing in
the V1 readiness matrix.

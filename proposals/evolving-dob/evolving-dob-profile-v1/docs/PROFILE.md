# DOB-EVO/1 Profile

DOB-EVO/1 defines an evolving state layer for Spore-style DOBs. It does not
mutate the base DOB Cell. The immutable DOB remains the public identity and
content anchor; `DobEvolutionStateV1` is a separate CKB state line that carries
the current DNA, trait, render, rule, and decoder commitments.

The renderer rule is:

```text
render_input = immutable_spore_dob(spore_id)
             + latest_live_DobEvolutionStateV1(spore_id)
             + decoder(decoder_hash)
```

## Production Boundary

- Only DOB-EVO/1 is accepted. There is no legacy DOB-EVO/0 parser, migration
  path, or optional fallback.
- State continuity uses `identity(ckb_type_id)` plus explicit `spore_id` and
  `cluster_id` preservation.
- The owner authority is the CKB lock that spends the live state line.
- `evolve_dob_state` is an `Active` same-phase replacement enforced by action
  guards. The current `flow` DSL records non-no-op phase edges only
  (`Unborn -> Active`, `Active -> Final`).
- Every accepted state edge emits a `DobEvolutionEventV1` receipt whose
  `new_event_hash` must equal the on-chain `hash_pair` event commitment tree.
  Scalar fields such as action, generation, and phase are enforced by action
  guards; the hash tree binds the action salt, Spore/Cluster identity,
  previous event hash, old/new DNA, old/new traits, old/new render commitment,
  rule hash, and decoder hash.
- Registry publication and package/build/deployment identity checks are part of
  the release gate, not documentation-only ceremony.

## Actions

| Action | Purpose |
| --- | --- |
| `initialise_dob_state` | Creates the first state Cell for an immutable DOB. |
| `evolve_dob_state` | Replaces an active state Cell with generation + 1. |
| `finalise_dob_state` | Produces the terminal state; no further evolution is accepted. |

## Non-Goals

- Rewriting existing Spore DOB content.
- Supporting mutable state through an off-chain database only.
- Supporting older evolving-DOB encodings.
- Claiming live-chain deployment without `Deployed.toml` and registry
  verification evidence.

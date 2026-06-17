# Security Boundary

DOB-EVO/1 treats the evolving state Cell as the aggregate root. The base DOB is
read by indexers and renderers but is not rewritten by this profile.

## Required Checks

- `version == 1` for every intent and state.
- `spore_id`, `cluster_id`, `rule_hash`, `decoder_hash`, owner lock, and
  expiry are preserved after initialisation.
- `generation` increments exactly once.
- `Final` is terminal.
- `new_event_hash` is computed on-chain from a `hash_pair` commitment tree and
  rejected if the witness supplies any other value.
- Authorisation is delegated to the CKB lock that spends the live state line;
  the type script preserves `owner_lock` and uses it for receipt outputs.
- Registry publication must bind source identity; build must bind artifact,
  metadata, schema, ABI, and constraints identity.

## Main Risks

Duplicate state lines for the same `spore_id` are prevented by the selected
state Cell TYPE_ID lineage, not by scanning the whole chain for all historical
DOB-EVO states. Builders and indexers must resolve one live TYPE_ID state line
per DOB and reject ambiguous application-level listings.

The profile verifies state transition intent, lineage, and output commitments.
It does not prove that a renderer is honest. Renderers must treat `rule_hash`
and `decoder_hash` as part of the content-addressed rendering contract.

The current CellScript `flow` DSL rejects no-op phase edges, so repeated
`Active` evolution is enforced in `evolve_dob_state` rather than represented as
`Active -> Active` in the flow block.

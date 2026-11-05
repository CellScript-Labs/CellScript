# Focused Schema Acknowledgements

## Status and boundary

Edition 2027 implements a versioned, off-chain review workflow
for Edition 2027 `data = same except { ... }` relations. It compares one exact
old/new package pair and one explicitly selected `action`/predecessor/successor
relation. It does not authorize a deployment, migrate existing Cell data, edit
`Cell.lock`, or edit `Deployed.toml`.

The workflow uses two canonical records. Upgrade-bound records also name their
exact subject, environment, and old/new graph identities:

- `cellscript-schema-change-plan-v2` binds the old/new module endpoints, action and roles, old and
  new interface hashes, field-layout identities, canonical relation identities,
  checked field-type/layout identities, canonical old/new source-closure identities,
  changed field treatments, blockers, and a plan hash;
- `cellscript-schema-acknowledgement-v2` binds an eligible plan to a named
  reviewer, rationale, the exact changed-field list, and an acknowledgement
  hash.

The reviewer field is an audit label, not a cryptographic signature or an
authorization credential. A later governance system may bind a verified
receipt hash to its own authenticated approval process.

A genuinely new resource schema needs no old-to-new acknowledgement.
Adding or renaming a relation over an existing resource does not create a new
schema baseline.
A missing old source tree is not a new baseline: transactional upgrade planning
requires the source bound by the authoritative old lock. The v2 records name
old/new module names and resource types explicitly, including an absent side for additions or
removals. A change to an existing schema sets
`state_migration_required = true`; the acknowledgement records review of the
field policy and does not satisfy that separate migration requirement.

## New-field rule

`same except` expands every unlisted field as preserved. That is safe for an
unchanged concrete schema, but it is not evidence that an author reviewed a
field added by an upgrade. A newly added field therefore causes blocker
`SACK1001` when it remains implicit.

For example, adding `approval_nonce` to `Token` while leaving the old relation
unchanged cannot produce a receipt:

```cellscript
data = same except {
    amount = token.amount
}
```

The candidate must state the new policy explicitly:

```cellscript
data = same except {
    amount = token.amount
    approval_nonce = 0
}
```

The compiler runtime already checks the created output field against the
constant. The ProofPlan classifies fixed constant assignments as checked
successor updates, so this reset can remain executable under the production
`DenyFailClosed` policy.

## CLI workflow

Generate the review plan first:

```bash
cellc schema-ack \
  --old ./token-v1 \
  --new ./token-v2 \
  --action transfer \
  --before token \
  --after next \
  --output schema-change-plan.json
```

After reviewing the field delta and fixing all blockers, create a receipt:

```bash
cellc schema-ack \
  --old ./token-v1 \
  --new ./token-v2 \
  --action transfer \
  --before token \
  --after next \
  --acknowledge-by "reviewer identity" \
  --rationale "approval_nonce resets on every transfer" \
  --output schema-acknowledgement.json
```

Verify a standalone receipt against its exact candidate pair:

```bash
cellc schema-ack \
  --old ./token-v1 \
  --new ./token-v2 \
  --action transfer \
  --before token \
  --after next \
  --verify schema-acknowledgement.json
```

The standalone `--old`/`--new` workflow requires the same module name on both
sides. A namespace change is reviewed through the upgrade plan as retirement of
the old module's relations and fresh endpoints in the new module. It does not
establish nominal continuity between their resource types. The retired
`same except` relation still requires its exact review receipt and state
migration decision; changing the namespace cannot remove that obligation.

Changing a bound schema, assignment expression, lock treatment, role, action,
module, or helper implementation changes the bound review identity and makes the
old receipt stale. Canonical source-closure identities include imported helpers;
formatting changes do not change those identities or the canonical relation.
Nested field changes retain their checked type identities and field paths, so
unchanged top-level type spelling cannot hide an imported layout change.

## Transactional upgrade integration

`cellscript-upgrade-plan-v2` enumerates the actual old/new successor relations
for every affected root and changed dependency source module, including
library-only packages and dependency actions unused by a root, over the union
of old and new manifest environments. Multiple modules in a dependency can have
distinct impacts and subjects in the same consuming lock graph.
Introduced and retired environments retain explicit optional-side evidence;
an unavailable old baseline remains a hard blocker. An environment selector
limits repinning and does not narrow this schema-review coverage. It retains source baseline
identities and exact schema-change plans. Unavailable old source, implicit
preservation of a newly added field, missing required receipts, or a receipt
bound to another schema/relation blocks apply. A generic `--acknowledge` code
cannot discharge these blockers.

Preserve the old package tree and its dependency layout before changing path
sources. Its sources and `Cell.lock` must match the authoritative old graph:

```bash
cellc update-plan . --offline \
  --schema-baseline app=../old-tree/app \
  --output target/upgrade-plan.json
```

Create a receipt directly from the selected upgrade review, using its exact
compilation profile and relation identity:

```bash
cellc schema-ack \
  --upgrade-plan target/upgrade-plan.json \
  --subject app \
  --action transfer --before token --after next \
  --acknowledge-by "reviewer identity" \
  --rationale "approval_nonce resets on every transfer" \
  --output target/schema-acknowledgement.json

cellc update-plan . --offline \
  --schema-baseline app=../old-tree/app \
  --schema-ack app=target/schema-acknowledgement.json \
  --output target/upgrade-plan.json

cellc update --apply-plan target/upgrade-plan.json
```

The review subject is a root member name or the exact dependency subject ID
from `dependency_schema_impacts`; its `owners` identify the affected roots.
Use `--environment NAME` when selecting an environment-specific review, and
`--schema-ack app@NAME=FILE` when attaching its receipt. Repeat `--schema-ack`
for distinct subject/environment/relation reviews. Duplicate, unused, stale, or
wrongly scoped receipts reject. Upgrade receipts bind the exact graph context;
a standalone path-pair receipt does not substitute for that binding.

When old and new namespaces contain the same action and role names, use
`--module` to select the exact endpoint. For example, review the retired
`token_v1` relation from a namespace change:

```bash
cellc schema-ack \
  --upgrade-plan target/upgrade-plan.json \
  --subject app --module token_v1 \
  --action transfer --before token --after next \
  --acknowledge-by "reviewer identity" \
  --rationale "reviewed retirement and the separate state migration" \
  --output target/token-v1-retirement-acknowledgement.json
```

Both retired and fresh endpoint records retain the actual old/new source
identities and the same graph context, with `old_module`/`new_module` naming
their optional module sides. They do not pair renamed nominal types implicitly.
`--schema-baseline` and `--schema-ack` are
planning inputs; apply revalidates the saved plan against source and lock state.

The receipt remains separate from interface compatibility, builder migration,
deployment authorization and state migration. It cannot turn an incompatible
interface into a compatible one or authorize a Type ID/Type-hash upgrade. See
[transactional upgrades](CELLSCRIPT_TRANSACTIONAL_UPGRADES.md).
